#!/usr/bin/env python3
"""Authoring only: download manufacturer CAD into a local cache; never part of app runtime."""
from pathlib import Path
from html.parser import HTMLParser
from urllib.parse import urlparse,quote
from concurrent.futures import ThreadPoolExecutor,as_completed
import argparse,json,hashlib,subprocess,zipfile,time,re,urllib.request

parser=argparse.ArgumentParser(description="Collect the official Motedis slot-accessory CAD index and inspect its archives (authoring tool).")
parser.add_argument("--index-dir",type=Path,default=Path("target/motedis-index"))
parser.add_argument("--library",type=Path,required=True)
parser.add_argument("--output",type=Path,default=Path("target/motedis-inventory.json"))
parser.add_argument("--refresh",action="store_true")
args=parser.parse_args()
args.index_dir.mkdir(parents=True,exist_ok=True)
def fetch_index(page):
 p=args.index_dir/f"page-{page}.html"
 if args.refresh or not p.exists():
  u=f"https://www.motedis.com/en/cad-database?category_id=16&page={page}"
  p.write_bytes(urllib.request.urlopen(u,timeout=40).read())
 return p.read_text()
first=fetch_index(1)
match=re.search(r"Page\s+1\s+of\s+(\d+)",first)
if not match:raise RuntimeError("Manufacturer pagination is missing")
pages=int(match.group(1))
if not 1<=pages<=100:raise RuntimeError("Unexpected number of manufacturer pages")
with ThreadPoolExecutor(max_workers=4) as pool:list(pool.map(fetch_index,range(2,pages+1)))
class Rows(HTMLParser):
 def __init__(self):super().__init__();self.rows=[];self.row=None;self.title=False;self.a=None
 def handle_starttag(self,tag,attrs):
  a=dict(attrs)
  if tag=='article' and 'cd-row' in a.get('class','').split():self.row={'name':'','product':'','download':'','image':''}
  if self.row is None:return
  if tag=='h2':self.title=True
  if tag=='img':self.row['image']=a.get('src','')
  if tag=='a':self.a=[a.get('href',''),'', 'download' in a]
 def handle_data(self,data):
  if self.row is None:return
  if self.title:self.row['name']+=data
  if self.a is not None:self.a[1]+=data
 def handle_endtag(self,tag):
  if self.row is None:return
  if tag=='h2':self.title=False
  if tag=='a' and self.a:
   url,text,download=self.a
   if download or text.strip()=='Download':self.row['download']=url
   elif 'View Product' in text:self.row['product']=url
   self.a=None
  if tag=='article':
   if self.row['product'] and self.row['download']:self.row['name']=self.row['name'].strip();self.rows.append(self.row)
   self.row=None
rows=[]
for p in [args.index_dir/f'page-{n}.html' for n in range(1,pages+1)]:
 s=Rows();s.feed(p.read_text());rows.extend(s.rows)
products={}
for r in rows:
 item=products.setdefault(r['product'],{'name':r['name'],'product_url':r['product'],'image_url':r['image'],'resources':[]})
 if r['download'] not in item['resources']:item['resources'].append(r['download'])
urls=sorted({r['download'] for r in rows if urlparse(r['download']).path.lower().endswith(('.zip','.stp','.step'))})
cache=args.library/'archives';cache.mkdir(parents=True,exist_ok=True)
def get(url):
 p=cache/(hashlib.sha256(url.encode()).hexdigest()[:24]+Path(urlparse(url).path).suffix.lower())
 if not p.exists():
  n=p.with_suffix('.next');subprocess.run(['curl','--fail','--location','--retry','1','--max-time','60','--max-filesize','50000000','--silent','--show-error','--output',str(n),quote(url,safe=':/?&=%')],check=True);n.replace(p)
 data=p.read_bytes();out={'url':url,'archive_sha256':hashlib.sha256(data).hexdigest(),'cache_path':str(p),'bytes':len(data),'members':[]}
 if p.suffix=='.zip':
  with zipfile.ZipFile(p) as z:
   for i in z.infolist():
    if i.is_dir():continue
    ext=Path(i.filename).suffix.lower();entry={'name':i.filename,'size':i.file_size,'extension':ext}
    if ext in ['.step','.stp','.igs','.iges','.stl']:
     if i.file_size>20000000:entry['error']='model exceeds 20MB limit'
     else:entry['sha256']=hashlib.sha256(z.read(i)).hexdigest()
    out['members'].append(entry)
 else:out['members']=[{'name':'','size':len(data),'extension':p.suffix,'sha256':out['archive_sha256']}]
 return out
inventory=[];errors=[];start=time.time()
with ThreadPoolExecutor(max_workers=6) as pool:
 jobs={pool.submit(get,u):u for u in urls}
 for n,future in enumerate(as_completed(jobs),1):
  try:inventory.append(future.result())
  except Exception as e:errors.append({'url':jobs[future],'error':str(e)});print('FAILED',jobs[future],str(e),flush=True)
  if n%10==0 or n==len(urls):print(f'{n}/{len(urls)} archives, {sum(i["bytes"] for i in inventory)//1024} KiB, {time.time()-start:.0f}s',flush=True)
report={'source_url':'https://www.motedis.com/en/cad-database?category_id=16','products':list(products.values()),'archives':inventory,'errors':errors}
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
from collections import Counter
print(json.dumps({'products':len(products),'archives':len(inventory),'errors':errors,'extensions':dict(Counter(e['extension'] for a in inventory for e in a['members']))},ensure_ascii=False),flush=True)
