#!/usr/bin/env python3
"""Authoring only: preserve existing IDs/datums and publish pinned catalogue metadata."""
from pathlib import Path
from urllib.parse import quote,urlparse
import argparse,hashlib,json,re,zipfile,collections

parser=argparse.ArgumentParser(description="Build pinned catalogue metadata from a collected Motedis inventory (authoring tool).")
parser.add_argument("--inventory",type=Path,default=Path("target/motedis-inventory.json"))
parser.add_argument("--library",type=Path,required=True)
args=parser.parse_args()
root=Path(__file__).resolve().parents[2];library=args.library;library.mkdir(parents=True,exist_ok=True)
report=json.loads(args.inventory.read_text());old=json.loads((root/'enhancements/connector-catalog.json').read_text());archives={a['url']:a for a in report['archives']};out=[];used=set();products=[]
def category(name):
 n=name.lower()
 if 'cover' in n:return 'cover' if any(x in n for x in ['bracket','profile','slot']) and 'cap' not in n else 'cap'
 if any(x in n for x in ['nut','slotted block','sliding block','threaded plate']):return 'nut'
 if 'inner bracket' in n:return 'internal'
 if 'hinge' in n:return 'hinge'
 if any(x in n for x in ['plate','connection strip','connecting strip']):return 'plate'
 if any(x in n for x in ['cube connector','corner connector']):return 'cube'
 if any(x in n for x in ['automatic connector','quick connector','t-matik','standard connector','central connector']):return 'quick'
 if any(x in n for x in ['joint','swivel','swivelling','swiveling','pivot']):return 'joint'
 if any(x in n for x in ['bracket','angle']):return 'angle'
 if any(x in n for x in ['screw','bolt','sleeve','spacer']):return 'fastener'
 return 'accessory'
def metadata(product):
 text=product['name']+' '+product['product_url'].split('/')[-1].replace('-',' ')
 slots=sorted({int(x) for x in re.findall(r'\bslot\s*(5|6|8|10)\b',text,re.I)})
 dimension=re.search(r'\b(\d+)\s*[x×]\s*(\d+)',text,re.I)
 series=[int(dimension.group(1))] if dimension and int(dimension.group(1)) in [20,30,40,45,60,80,90] else []
 if not series:
  clean=re.sub(r'\b45\s*[°º]|\b45\s*(?:degrees?|G)\b','',text,flags=re.I)
  series=sorted({int(n) for n in re.findall(r'\b(20|30|40|45|60|80|90)\s+(?:[BIO][ -]*(?:type)?|slot|I-Type|B-Type)',clean,re.I)})
 types=sorted(set(re.findall(r'\b([BIO])[ -]*type\b|\btype\s+([BIO])\b',text,re.I)))
 types=sorted({v.upper() for pair in types for v in pair if v})
 return series,slots,'/'.join(types)
def slug(text):return re.sub('[^a-z0-9]+','-',text.lower()).strip('-')[:90] or 'part'
for product in report['products']:
 name=product['name'];url=product['product_url'];resources=[{'url':quote(u,safe=':/?&=%'),'kind':Path(urlparse(u).path).suffix.lower().lstrip('.')} for u in product['resources']];series,slots,types=metadata(product);cat=category(name);model_ids=[];seen=set()
 for source in product['resources']:
  a=archives.get(source)
  if not a:continue
  for member in a['members']:
   if member['extension'] not in ['.stp','.step'] or 'sha256' not in member:continue
   sha=member['sha256']
   if sha in seen:continue
   seen.add(sha)
   prior=next((m for m in old if m['product_url']==url and m['step_sha256']==sha),None)
   if prior:m=dict(prior)
   else:
    sku=Path(member['name'] or urlparse(source).path).stem;sku=re.sub(r'^motedis[_ -]*','',sku,flags=re.I);id='motedis-'+slug(sku)+'-'+hashlib.sha256((url+member['name']).encode()).hexdigest()[:8]
    m=dict(id=id,series=series[0] if series else 0,slot=slots[0] if slots else 0,supplier='Motedis',part_number=sku,label=name,product_url=url,archive_url=quote(source,safe=':/?&=%'),archive_sha256=a['archive_sha256'],step_member=member['name'],filename='motedis-cad-'+sha[:24]+'.step',step_sha256=sha,category=cat,interface='slot' if cat in ['nut','internal','quick'] else 'end' if cat in ['cap','joint','cube'] else 'surface',placement='manual',anchor=[0,0,0],normal=[0,0,1],along=[1,0,0],compatible_series=series)
   if prior and m['placement']=='manual':m.update(compatible_series=series,series=series[0] if series else 0,slot=slots[0] if slots else 0)
   m.update(resources=resources,profile_type=types,compatible_slots=slots or ([m['slot']] if m['slot'] else []),availability=prior.get('availability','ready') if prior else 'ready',import_error=prior.get('import_error','') if prior else '')
   if prior and m['placement']!='manual' and not m['profile_type']:m['profile_type']='B' if m['series'] in [20,30] else 'I'
   if m['id'] not in used:
    if member['name']:
     with zipfile.ZipFile(a['cache_path']) as z:data=z.read(member['name'])
    else:data=Path(a['cache_path']).read_bytes()
    target=library/m['filename']
    if not target.exists() or hashlib.sha256(target.read_bytes()).hexdigest()!=sha:target.write_bytes(data)
    out.append(m);used.add(m['id'])
   model_ids.append(m['id'])
 if not model_ids:
  id='motedis-source-'+hashlib.sha256(url.encode()).hexdigest()[:12];availability='unsupported' if any(u in archives for u in product['resources']) else 'source_only'
  m=dict(id=id,series=series[0] if series else 0,slot=slots[0] if slots else 0,supplier='Motedis',part_number=url.split('/')[-1],label=name,product_url=url,archive_url='',archive_sha256='',step_member='',filename='',step_sha256='',category=cat,interface='',placement='manual',anchor=[0,0,0],normal=[0,0,1],along=[1,0,0],compatible_series=series,compatible_slots=slots,profile_type=types,availability=availability,import_error='',resources=resources)
  out.append(m);used.add(id);model_ids.append(id)
 products.append(dict(name=name,product_url=url,models=model_ids,resources=resources))
# Preserve every previously published model ID, even if its vendor archive has since changed.
for m in old:
 if m['id'] not in used:
  m=dict(m);m.update(resources=[],profile_type='B' if m['series'] in [20,30] else 'I',compatible_slots=[m['slot']],availability=prior.get('availability','ready') if prior else 'ready',import_error=prior.get('import_error','') if prior else '');out.insert(0,m);used.add(m['id']);print('Retained legacy model',m['id'])
# Original IDs remain first in the chooser for existing projects and familiar defaults.
order={m['id']:i for i,m in enumerate(old)};out.sort(key=lambda m:(0,order[m['id']]) if m['id'] in order else (1,m['category'],m['label'],m['part_number']))
(root/'enhancements/connector-catalog.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
summary={'source_url':report['source_url'],'product_count':len(products),'catalogue_entries':len(out),'step_models':sum(bool(m['filename']) for m in out),'status_counts':dict(collections.Counter(m['availability'] for m in out)),'archive_count':len(archives),'archive_errors':report['errors'],'unique_cad_files':len({m['step_sha256'] for m in out if m['filename']}),'verified_models':sum(bool(m.get('geometry')) and m['availability']=='ready' for m in out),'geometry_counts':dict(collections.Counter(m.get('geometry','reference') for m in out))}
(root/'enhancements/connector-products.json').write_text(json.dumps({'summary':summary,'products':products},ensure_ascii=False,indent=2)+'\n');(library/'catalogue.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(summary));print('categories',dict(collections.Counter(m['category'] for m in out)))
print('without STEP',json.dumps([(m['label'],m['availability']) for m in out if not m['filename']],ensure_ascii=False))
