#!/usr/bin/env python3
"""Authoring only: cache the individual supplier pages used by the installation audit."""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor,as_completed
from html.parser import HTMLParser
import json,urllib.request,hashlib,re,time
root=Path(__file__).resolve().parents[2];cache=root/'target/connector-audit-sources';cache.mkdir(parents=True,exist_ok=True)
models=json.loads((root/'enhancements/connector-catalog.json').read_text());urls=list(dict.fromkeys(m['product_url'] for m in models))
class Text(HTMLParser):
 def __init__(self):super().__init__();self.out=[];self.skip=0
 def handle_starttag(self,t,a):
  if t in ['style','script','noscript']:self.skip+=1
  elif not self.skip and t in ['p','br','li','h1','h2','h3','div','tr']:self.out.append('\n')
 def handle_endtag(self,t):
  if t in ['style','script','noscript']:self.skip=max(0,self.skip-1)
 def handle_data(self,d):
  if not self.skip:self.out.append(d)
def get(url):
 key=hashlib.sha256(url.encode()).hexdigest()[:20];html=cache/(key+'.html');txt=cache/(key+'.txt')
 if not html.exists():html.write_bytes(urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'Mozilla/5.0'}),timeout=30).read())
 text=Text();text.feed(html.read_text());s='\n'.join(' '.join(line.split()) for line in ''.join(text.out).splitlines() if line.strip());txt.write_text(s);return {'url':url,'text_file':str(txt),'html_sha256':hashlib.sha256(html.read_bytes()).hexdigest()}
out=[]
with ThreadPoolExecutor(max_workers=6) as pool:
 jobs={pool.submit(get,u):u for u in urls}
 for n,f in enumerate(as_completed(jobs),1):
  try:out.append(f.result())
  except Exception as e:out.append({'url':jobs[f],'error':str(e)})
  if n%40==0:print(n,'/',len(urls),'product pages fetched',flush=True)
(cache/'index.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n');print('complete',len(out),'errors',sum('error'in x for x in out),flush=True)
