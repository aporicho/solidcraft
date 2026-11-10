#!/usr/bin/env python3
"""Authoring only: import each pinned CAD hash in an empty isolated app and record its faces."""
from pathlib import Path
import argparse,socket,json,time
parser=argparse.ArgumentParser(description="Collect native face evidence from an empty isolated SolidCraft control instance (authoring only).")
parser.add_argument('--port',required=True,type=int)
parser.add_argument('--token-file',required=True,type=Path)
args=parser.parse_args()
if args.port==27879:parser.error('the installed user session must not be used for catalogue checks')
class Client:
 def __init__(self):
  self.sock=socket.create_connection(('127.0.0.1',args.port),timeout=90);self.file=self.sock.makefile('rwb');self.n=0
 def call(self,method,params=None):
  self.n+=1;query={'id':self.n,'method':method,'params':params or {}}
  if self.n==1:query['token']=args.token_file.read_text().strip()
  self.file.write((json.dumps(query)+'\n').encode());self.file.flush();reply=json.loads(self.file.readline())
  if reply.get('ok') is False or 'error' in reply:raise RuntimeError(str(reply))
  return reply.get('result',reply)
 def execute(self,command,params=None):return self.call('engine.execute',{'command':command,'params':params or {}})
root=Path(__file__).resolve().parents[2];out=root/'target/connector-audit-geometry';out.mkdir(parents=True,exist_ok=True);models=json.loads((root/'enhancements/connector-catalog.json').read_text());c=Client();design=c.execute('document.inspect',{'measure':False})
if any(design.get(key) for key in ['dirty','path','bodies','sketches','timeline','params','metadata']):raise RuntimeError('start an empty isolated instance before collecting geometry')
start=time.time();done=0
for m in models:
 if not m['filename']:continue
 p=out/(m['step_sha256']+'.json')
 if not p.exists():
  try:
   c.execute('file.new');c.execute('aluminium.connector',{'model':m['id']});bodies=c.execute('inspect.measure')['bodies'];rows=[]
   for body in bodies:
    faces=c.execute('model.faces',{'body':body['name']})['faces'];rows.append({'measure':body,'faces':faces})
   p.write_text(json.dumps({'id':m['id'],'sha256':m['step_sha256'],'bodies':rows},ensure_ascii=False,indent=2)+'\n')
  except Exception as e:print('FAILED',m['id'],str(e)[:180],flush=True);continue
 done+=1
 if done%15==0:print(done,'/252 geometry examined',round(time.time()-start),'s',flush=True)
print('DONE',done,round(time.time()-start),'s',flush=True);c.execute('file.new')
