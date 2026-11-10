#!/usr/bin/env python3
"""Authoring only. Audit pinned CAD face measurements and supplier source receipts.

Run after collecting product pages and native model.faces/inspect.measure receipts.
No inferred bounding-box centre is accepted as an installation datum. The output
separates a reviewed datum from a physically verified fit to an actual profile.
"""
from pathlib import Path
import argparse, collections, csv, hashlib, json, math, re

parser=argparse.ArgumentParser()
parser.add_argument('--geometry',type=Path,default=Path('target/connector-audit-geometry'))
parser.add_argument('--baseline',type=Path,default=Path('target/connector-catalog-before-audit.json'))
parser.add_argument('--fits',type=Path,default=Path('target/connector-audit-fit-results.json'))
parser.add_argument('--sources',type=Path,default=Path('target/connector-audit-sources'))
args=parser.parse_args();root=Path(__file__).resolve().parents[2]
path=root/'enhancements/connector-catalog.json'
if not args.baseline.exists():args.baseline.write_text(path.read_text())
models=json.loads(args.baseline.read_text());receipts={r['url']:r for r in json.loads((args.sources/'index.json').read_text())}
fits=json.loads(args.fits.read_text())['results'] if args.fits.exists() else []
rows=[]
def dot(a,b):return sum(x*y for x,y in zip(a,b))
def unit(a):
 d=math.sqrt(dot(a,a));return [x/d for x in a]
def clean(a):return [round(v,8) if abs(v)>1e-8 else 0.0 for v in a]
def planes(data):return [f for b in data['bodies'] for f in b['faces'] if f.get('plane_normal')]
def description(m):
 s=Path(receipts[m['product_url']]['text_file']).read_text();i=s.lower().find('description')
 return s[i+11:].split('\nFiles\n')[0].split('\nReviews\n')[0].strip() if i>=0 else ''
def frame(m,anchor,normal,along):
 n=unit(normal);u=unit(along);assert abs(dot(n,u))<1e-5,(m['id'],n,u)
 m.update(anchor=clean(anchor),normal=clean(n),along=clean(u),placement='interface')
def ready(m,interfaces,method,**extra):
 a=m['installation'];a.update(state='datum_checked',interfaces=interfaces,method=method,**extra)
def flat(m,data):
 # Plate faces must both be actual opposite parallel planes, separated by CAD thickness.
 assert len(data['bodies'])==1
 b=data['bodies'][0];box=b['measure']['bbox'];dims=[b-a for a,b in zip(box['min'],box['max'])];axis=min(range(3),key=dims.__getitem__);thickness=dims[axis]
 assert thickness<7 and sorted(dims)[1]>thickness*2.5
 ps=planes(data);top=max(ps,key=lambda f:f['area']);assert abs(top['plane_normal'][axis])>.99999
 low=min(f['centroid'][axis] for f in ps if abs(f['plane_normal'][axis])>.99999 and f['area']>top['area']*.5)
 contact=next(f for f in ps if abs(f['centroid'][axis]-low)<1e-4 and f['plane_normal'][axis]<-.99999 and f['area']>top['area']*.5)
 # Group split cylindrical faces; exclude corner fillets by the total angular coverage.
 holes={}
 for f in b['faces']:
  c=f.get('cylinder')
  if not c or abs(c['axis'][axis])<.99999 or not 1.5<=c['radius']<=7:continue
  point=c['origin'].copy();point[axis]=low;k=tuple(round(v,4) for v in point)+(round(c['radius'],4),)
  h=holes.setdefault(k,{'point':point,'radius':c['radius'],'area':0});h['area']+=f['area']
 holes=[h for h in holes.values() if h['area']>=1.5*math.pi*h['radius']*thickness]
 assert holes,'no proven through-hole'
 transverse=[j for j in range(3) if j!=axis];long=max(transverse,key=dims.__getitem__)
 middle=[(a+b)/2 for a,b in zip(box['min'],box['max'])]
 chosen=min(holes,key=lambda h:(round(sum((h['point'][j]-middle[j])**2 for j in transverse),3),tuple(h['point'])))
 n=[0.,0.,0.];n[axis]=1.;u=[0.,0.,0.];u[long]=1.
 frame(m,chosen['point'],n,u);ready(m,['surface','slot'],'cad_mounting_plane_and_through_hole',hole_radius=round(chosen['radius'],6),hole_count=len(holes),contact_plane=clean(contact['centroid']))
 return holes
for index,m in enumerate(models):
 old={k:m[k] for k in ['category','interface','placement','anchor','normal','along','compatible_series','compatible_slots','profile_type']};desc=description(m);name=m['label'].lower();sku=m['part_number'].lower();data=None
 if m['filename']:
  p=args.geometry/(m['step_sha256']+'.json');data=json.loads(p.read_text());assert data['sha256']==m['step_sha256'] and data['bodies']
 role={'angle':'rigid_bridge','nut':'slot_insert','plate':'face_plate','cap':'end_cap','cube':'multiport_node','joint':'moving_joint','internal':'internal_bridge','quick':'hidden_connector','fastener':'axial_fastener','cover':'slot_cover'}.get(m['category'],'accessory')
 target='profile';reason='datum_required';interfaces=[];requirements=['fasteners']
 if not m['filename']:role='reference';target='reference';reason='no_geometry';requirements=[]
 elif index in [24,25]:role='motor';target='motor_bracket';reason='custom_mount'
 elif 26<=index<=34:role='shaft';target='bearing';reason='shaft_support'
 elif index==35:role='fitting_link';target='connector';reason='fitting_only'
 elif index==44:role='belt';target='pulley';reason='belt_system'
 elif index in [36,37]:role='cable_mount';interfaces=['surface','slot']
 elif index in [38,39,40,41,42]:role='cable_slot_mount';interfaces=['slot'];requirements=['quarter_turn']
 elif index in [46,47]:role='joint_adapter';interfaces=['slot'];requirements=['matching_joint']
 elif index in [48,49,50,189]:role='hidden_connector';m['category']='quick';m['interface']='end';requirements=['matching_bore','second_member']
 elif 'bracket' in name and 'cap' in name or index in [76,77,78,79,80,81,82,91,92,93]:role='bracket_cap';target='connector';reason='bracket_only';m['interface']='';requirements=['matching_bracket']
 elif index in [113,114,115]:role='corner_node';interfaces=['slot'];m['interface']='slot';requirements=['three_slots','set_screws'];reason='corner_node'
 elif index in [120,121,122,128,129,130,131,248,249,250,251,252,253,254,255,256,257,258,259,260,261,262,263]:role='hidden_connector' if index>=248 else 'axial_fastener';target='machined_profile';reason='machining';m['interface']='bore';requirements=['matching_bore','machining']
 elif index in [123,124,125,126]:role='hammer_bolt';interfaces=['slot'];m['interface']='slot';requirements=['flange_nut']
 elif index in [127,164,165]:role='bolt_fitting';target='bolt';reason='bolt_only';m['interface']='';requirements=['matching_bolt']
 elif 134<=index<=158 and index not in [145,146,147,148]:role='joint_kit';target='connector';reason='joint_kit';requirements=['matching_joint']
 elif index in [145,146]:role='shaft_bearing';target='hex_profile';reason='hex_core';requirements=['hex_core','shaft']
 elif index==148:role='pivot_plate';m['interface']='surface';requirements=['fasteners','angle_lock']
 elif index==163:role='rigid_bridge';m['category']='angle';m['interface']='end';requirements=['fasteners','angle_45']
 elif index in [215,216]:role='slot_splice';m['interface']='slot';requirements=['two_slots']
 elif index in [243,244]:role='internal_bridge';m['interface']='slot';requirements=['second_member','set_screws']
 elif m['category']=='angle':requirements=['fasteners','second_member']+(['angle_45'] if '45' in name and ('°' in name or 'connector 45' in name) else [])
 elif m['category']=='joint':requirements=['fasteners','moving_joint']
 elif m['category']=='cap':requirements=['matching_section']
 elif m['category']=='cover':m['interface']='slot';requirements=['matching_slot','fixed_length']
 m['installation']={'revision':1,'state':'reference_only' if not m['filename'] else 'manual_only','role':role,'target':target,'interfaces':interfaces,'reason':reason,'requirements':requirements,'source_sha256':receipts[m['product_url']]['html_sha256'],'cad_sha256':m['step_sha256'],'excluded_stock':[]}
 a=m['installation']
 # Repair metadata only from the individual supplier description, never category pages.
 text=m['label']+' '+desc
 types=sorted({s.upper() for s in re.findall(r'\b([BIO])[ -]*(?:type|typ)\b',m['label'],re.I)})
 if index==12:types=['B','I']
 if not types:types=m['profile_type'].split('/') if m['profile_type'] else sorted({s.upper() for s in re.findall(r'\b([BIO])[ -]*(?:type|typ)\b',desc,re.I)})
 if role=='face_plate':types=sorted({s.upper() for s in re.findall(r'\b([BIO])[ -]*(?:type|typ)\b',text,re.I)}) or types
 slots=sorted({int(s) for s in re.findall(r'\b(?:slot|groove|nut)\s*(5|6|8|10)\b',m['label'],re.I)})
 if not slots:slots=sorted({int(s) for s in re.findall(r'\b(?:slot|groove|nut)\s*(5|6|8|10)\b',desc,re.I)})
 if index in [72,76]:slots=[8,10]
 if types:m['profile_type']='/'.join(types)
 if slots:m['compatible_slots']=slots;m['slot']=slots[0]
 if role=='face_plate' and index not in [215,216,243,244]:
  try:
   holes=flat(m,data)
   series_match=re.match(r'LP(20|30|40|45)',m['part_number'],re.I) or re.match(r'(20|30|40|45)-V',m['part_number'],re.I)
   series=int(series_match.group(1)) if series_match else ({235:45,237:30,246:20,247:40}.get(index))
   if not series:raise ValueError('supplier profile pitch not known')
   m['compatible_series']=[series];m['series']=series
   if not m['compatible_slots']:m['compatible_slots']={20:[5,6],30:[6,8],40:[8],45:[10]}[series]
   if not m['profile_type']:m['profile_type']='B/I' if series in [20,30] else 'I' if series==40 else 'B'
   m['interface']='surface';a['reason']='plane_hole'
  except (AssertionError,ValueError) as e:a.update(state='manual_only',reason='hole_group_required',interfaces=[])
 elif index in [0,1,2]:
  # Existing bracket local corner and two orthogonal mounting planes have been measured.
  ready(m,['surface','slot','end'],'cad_bracket_corner');m['placement']='angle';a['reason']='corner_planes'
 elif index in [61,62,64]:
  # These wider brackets are in a different CAD orientation from the original 20/30 corner.
  frame(m,[0,0,0],[0,0,1],[0,1,0]);ready(m,['surface','slot'],'cad_bracket_corner');a['reason']='corner_planes'
 elif index in [60,63,65,66,67,68,69,70,71,72,73]:
  # Require actual Y=0 mounting plane; do not assume every bracket shares this orientation.
  ps=planes(data);contact=[f for f in ps if abs(f['centroid'][1])<1e-5 and f['plane_normal'][1]<-.99999 and f['area']>30]
  if contact:frame(m,[0,0,0],[0,1,0],[-1,0,0]);ready(m,['surface','slot'],'cad_bracket_corner');a['reason']='corner_planes'
 elif index in [12,13,14]:ready(m,['end'],'cad_joint_base_plane');a['reason']='joint_base'
 elif index in [15,16,17,18,116,117,118,119]:
  sizes={15:30,16:30,17:40,18:40,116:20,117:20,118:45,119:45};size=sizes[index];m['series']=size;m['compatible_series']=[size]
  ready(m,['end','bore'],'named_cube_ports',section=[size,size]);a['reason']='cube_planes';a['requirements']=['threading','countersunk_screws','second_member']
 elif index==113:
  # This molded corner node has three measured orthogonal slot tongues, not cube faces.
  # Interactive ports are usable after free placement; automatic parent mounting stays disabled.
  m['series']=20;m['compatible_series']=[20]
  a.update(reason='three_axis_slot_tongues',method='named_three_axis_ports',section=[20,20])
 elif index in [19,22,23]:ready(m,['end'],'cad_cap_seating_plane',section=[m['series'],m['series']]);a['reason']='cap_plane'
 elif role=='end_cap':
  # Review every rectangular cap; none can fit a square stock just because the first dimension matches.
  shape=re.search(r'(\d+)\s*[x×]\s*(\d+)',m['label'],re.I)
  if shape:a['section']=[int(shape.group(1)),int(shape.group(2))];a['reason']='matching_section'
 elif index in [3,4,5,6,7,8,197,198,199]:
  # Drawing callouts: S6B 3.20+1.00; S8B 4.50+1.50; S8I 4.50+3.00.
  shoulder=3.2 if 's6b' in m['id'] else 4.5
  frame(m,[0,0,shoulder],[0,0,1],[0,1,0]);ready(m,['slot'],'drawing_shoulder_and_thread_axis',seat='under_slot_lip');a['reason']='slot_shoulder';a['requirements']=['quarter_turn','supplier_section']
 elif index in [20,21]:ready(m,['slot'],'cad_inner_bracket_leg');a['reason']='internal_leg';a['requirements']=['set_screws','second_member','supplier_section']
 elif index in [171,172,173,174]:
  frame(m,[0,4.1,0],[0,1,0],[0,0,1]);ready(m,['slot'],'drawing_shoulder_and_thread_axis',seat='under_slot_lip');a['reason']='slot_shoulder';a['requirements']=['supplier_section']
 elif index in [179,180,181,182,183]:
  frame(m,[0,-1,0],[0,1,0],[0,0,1]);ready(m,['slot'],'cad_support_planes_and_thread_axis',seat='under_slot_lip');a['reason']='slot_shoulder';a['requirements']=['supplier_section']
 elif index in [200,201,202,203]:
  frame(m,[0,0,-0.8],[0,0,1],[1,0,0]);ready(m,['slot'],'drawing_shoulder_and_thread_axis',seat='under_slot_lip');a['reason']='slot_shoulder';a['requirements']=['supplier_section']
 elif index in [204,205,206]:
  frame(m,[0,0,-0.9],[0,0,1],[1,0,0]);ready(m,['slot'],'drawing_shoulder_and_thread_axis',seat='under_slot_lip');a['reason']='slot_shoulder';a['requirements']=['supplier_section']
 elif index==214:
  frame(m,[4.5,7,2.5],[0,0,1],[0,1,0]);ready(m,['slot'],'cad_support_planes_and_thread_axis',seat='under_slot_lip');a['reason']='slot_shoulder';a['requirements']=['supplier_section']
 # Keep failed and incomplete physical checks out of automatic mounting. This is
 # section-specific; the checked CAD datum remains usable with a future matching stock.
 fit_rows=[r for r in fits if r['id']==m['id']]
 a['fit_results']=[{k:r[k] for k in ['profile','state','pairs','unchecked'] if k in r} for r in fit_rows]
 a['cleared_stock']=sorted({r['profile'] for r in fit_rows if r['state']=='clear'})
 a['excluded_stock']=sorted({r['profile'] for r in fit_rows if r['state']!='clear'})
 # A supplier file with a different named thickness cannot stand in for that product.
 if role=='face_plate' and a['state']=='datum_checked':
  dims=re.search(r'(\d+)\s*[x×]\s*(\d+)\s*[x×]\s*(\d+)',m['label'],re.I)
  if dims:
   declared=min(map(float,dims.groups()));box=data['bodies'][0]['measure']['bbox'];actual=min(b-a for a,b in zip(box['min'],box['max']))
   if abs(actual-declared)>.1:a.update(state='source_conflict',reason='source_dimensions',thickness_mismatch=[declared,round(actual,6)])
 # Never leave an uncalibrated frame looking usable to the runtime or a later generator.
 if a['state']!='datum_checked':m['placement']='manual'
 # Output is a per-model review, with proof source hashes and the precise change.
 changes={k:{'before':old[k],'after':m[k]} for k in old if old[k]!=m[k]}
 rows.append({'id':m['id'],'part_number':m['part_number'],'name':m['label'],'source_url':m['product_url'],'source_sha256':a['source_sha256'],'cad_sha256':m['step_sha256'],'geometry_examined':bool(data),'state':a['state'],'role':role,'target':target,'reason':a['reason'],'requirements':a['requirements'],'changes':changes,'installation':a.copy()})
summary={'catalogue_entries':len(models),'source_products_examined':len(receipts),'cad_models_examined':sum(r['geometry_examined'] for r in rows),'states':dict(collections.Counter(r['state'] for r in rows)),'roles':dict(collections.Counter(r['role'] for r in rows)),'changed_entries':sum(bool(r['changes']) for r in rows),'physical_fit_cases':len(fits),'physical_fit_results':dict(collections.Counter(r['state'] for r in fits)),'note':'A reviewed installation datum is not a certification of fit to a supplier section or a complete fastener kit. Manual-only parts remain available for free placement.'}
path.write_text(json.dumps(models,ensure_ascii=False,indent=2)+'\n')
report=root/'enhancements/connector-installation-audit.json';report.write_text(json.dumps({'summary':summary,'models':rows},ensure_ascii=False,indent=2)+'\n')
with (root/'enhancements/connector-installation-audit.csv').open('w',newline='') as f:
 w=csv.writer(f,lineterminator="\n");w.writerow(['型号ID','料号','名称','安装基准审查','安装类型','配合对象','检查结果','受限截面','厂商资料'])
 for r in rows:
  states={'datum_checked':'已核对安装基准','manual_only':'需手动定位或配套组件','reference_only':'仅参考资料','source_conflict':'厂商CAD厚度与名称不符'}
  roles={'rigid_bridge':'外角码/刚性连接','slot_insert':'槽内螺母','face_plate':'面贴连接板','moving_joint':'活动接头','multiport_node':'实心方块','end_cap':'型材端盖','internal_bridge':'内置连接','motor':'电机','shaft':'六角轴','fitting_link':'配件连接块','cable_mount':'扎带座','cable_slot_mount':'槽装扎带座','reference':'参考资料','belt':'皮带','joint_adapter':'接头转接件','hidden_connector':'隐藏式连接','bracket_cap':'角码盖','slot_cover':'槽盖条','corner_node':'三维角件','axial_fastener':'孔轴紧固件','hammer_bolt':'锤头螺栓','bolt_fitting':'螺栓配套件','accessory':'其他附件','joint_kit':'接头配套组件','shaft_bearing':'六角轴支承','pivot_plate':'转角连接板','slot_splice':'槽内拼接条'}
  targets={'profile':'型材','reference':'参考资料','motor_bracket':'电机座','bearing':'轴承/轴支承','connector':'配套连接件','pulley':'带轮','machined_profile':'需匹配孔/螺纹的型材','bolt':'螺栓','hex_profile':'六角芯孔型材'}
  results={'clear':'未检出干涉','overlap':'检测到重叠','unchecked':'未完成干涉检查','error':'未完成检查'}
  fit='；'.join(x['profile']+'：'+results[x['state']] for x in r['installation'].get('fit_results',[])) or ('未提供自动安装' if r['state']!='datum_checked' else '待匹配实际型材截面')
  if r['state']=='source_conflict':fit='商品名厚度 '+str(r['installation']['thickness_mismatch'][0])+' mm；CAD厚度 '+str(r['installation']['thickness_mismatch'][1])+' mm'
  w.writerow([r['id'],r['part_number'],r['name'],states[r['state']],roles[r['role']],targets.get(r['target'],r['target']),fit,' / '.join(r['installation']['excluded_stock']),r['source_url']])
print(json.dumps(summary,ensure_ascii=False,indent=2))
