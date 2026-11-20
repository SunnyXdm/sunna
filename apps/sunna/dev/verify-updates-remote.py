"""Real Tauri host_update proof. Takes an OWNED sunna-updui- work directory
with locally built test-key releases A/B; never uses the release private key.
All host/app homes, listeners and displays are isolated; no services/containers.
"""
from pathlib import Path
import os,sys,subprocess,shutil,tarfile,json,time,threading,http.server,secrets,signal
root=Path(__file__).resolve().parents[3]
out=root/'apps/sunna/dev/design/shots-app'
w=Path(sys.argv[1]).resolve();assert w.name.startswith('sunna-updui-')
feed=w/'releases';base='http://127.0.0.1:48989'
# A was packaged before building B, using the layout in release/linux-inner.sh.
stage=w/'stage-B/bin';stage.mkdir(parents=True,exist_ok=True)
b=feed/'B';b.mkdir(exist_ok=True)
for name in ['sunnad','sunna-update']:shutil.copy(w/'source/target/debug'/name,stage/name)
shutil.copy(root/'scripts/sunna-host',stage/'sunna-host')
# Match the release packager. PAX timestamps create special entries the verifier rejects.
with tarfile.open(b/'sunna-host-linux-x86_64.tar.gz','w:gz',format=tarfile.USTAR_FORMAT) as archive:archive.add(stage.parent,arcname='.')
shutil.copy(stage/'sunna-update',b/'sunna-update-linux-x86_64')
def run(cmd,env=None):
 r=subprocess.run(list(map(str,cmd)),cwd=root,env=env,capture_output=True,text=True,timeout=90)
 if r.returncode:raise RuntimeError(f'{cmd[0]} failed ({r.returncode})')
 return r.stdout.strip()
for release,version in [('A','0.1.0'),('B','0.2.0')]:
 run(['python3','scripts/release/manifest.py',version,feed/release,base+'/'+release],{**os.environ,'SUNNA_RELEASE_KEY':str(w/'test.key')})
def select(name):
 for source in (feed/name).iterdir():
  dest=feed/source.name
  if dest.is_symlink() or dest.is_file():dest.unlink()
  dest.symlink_to(Path(name)/source.name)
class Handler(http.server.SimpleHTTPRequestHandler):
 def __init__(self,*args,**kwargs):super().__init__(*args,directory=str(feed),**kwargs)
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',48989),Handler)
threading.Thread(target=server.serve_forever,daemon=True).start()
hosthome=w/'host-home';apphome=w/'app-home'
for home in [hosthome,apphome]:home.mkdir(exist_ok=True)
def isolated(home):
 env={**os.environ,'HOME':str(home),'XDG_CONFIG_HOME':str(home/'.config'),'XDG_DATA_HOME':str(home/'.local/share'),'SUNNA_UPDATE_URL':base,'SUNNA_LOG_URL':'','SUNNA_LOG_TOKEN':''}
 env.pop('SUNNA_UPDATE_ROOT',None);return env
hostenv=isolated(hosthome);hostenv['SUNNA_TOKEN']=secrets.token_hex(16);hostenv['SUNNA_UPDATE']='notify'
appenv=isolated(apphome);appenv.update(DISPLAY=':87',GDK_BACKEND='x11',WEBKIT_DISABLE_DMABUF_RENDERER='1',LIBGL_ALWAYS_SOFTWARE='1')
assert not Path('/tmp/.X11-unix/X87').exists(),'Display :87 in use'
processes=[];logs=[]
def start(args,env,logname):
 log=open(w/logname,'w');logs.append(log)
 p=subprocess.Popen(list(map(str,args)),env=env,stdout=log,stderr=log,start_new_session=True);processes.append(p);return p
statefile=hosthome/'.local/share/sunna/host/state.json'
def state():return json.loads(statefile.read_text())
def waitversion(version):
 for _ in range(160):
  s=state()
  if s['current']==version and s['healthy']:return s
  time.sleep(.5)
 raise AssertionError('Host did not become healthy on '+version)
def key(k):run(['xdotool','key','--clearmodifiers',k],appenv);time.sleep(.5)
def shot(name):run(['import','-window',window,str(out/(name+'.png'))],appenv)
try:
 select('A');run(['sh',feed/'A/install.sh','host'],hostenv)
 start([hosthome/'.local/bin/sunna-update','supervise','host','--listen','127.0.0.1:48990','--source','synthetic','--codec','raw','--no-audio','--no-clipboard'],hostenv,'host.log')
 waitversion('0.1.0');select('B')
 s=state();s['last_check']=0;statefile.write_text(json.dumps(s))
 for _ in range(30):
  if state().get('available')=='0.2.0':break
  time.sleep(.5)
 assert state().get('available')=='0.2.0','Host did not discover B'
 prefs=apphome/'.sunna';prefs.mkdir(exist_ok=True)
 (prefs/'config.json').write_text(json.dumps({'view':'devices','pictures':'off'}))
 (prefs/'machines.json').write_text(json.dumps({'machines':[{'id':'updui-proof','name':'Update proof','address':'127.0.0.1:48990','key':hostenv['SUNNA_TOKEN'],'os':'Linux','device':'server','width':1280,'height':720}]}));(prefs/'machines.json').chmod(0o600)
 start(['Xvfb',':87','-screen','0','1280x900x24','-nolisten','tcp'],appenv,'display.log');time.sleep(.6)
 start(['openbox'],appenv,'wm.log');time.sleep(.5)
 start([root/'target/debug/sunna'],appenv,'app.log');time.sleep(8)
 window=run(['xdotool','search','--onlyvisible','--name','^Sunna$'],appenv).splitlines()[0]
 run(['xdotool','windowfocus',window],appenv);time.sleep(1);shot('updates-real-host-before')
 # Focus the computer and open its menu. The third item is Update this computer.
 key('Left');key('shift+F10');key('Down');key('Down');key('Return');time.sleep(1);shot('updates-real-host-confirm')
 # Confirmation starts on Cancel. Move to Update computer and submit.
 key('Tab');key('Return');time.sleep(1);shot('updates-real-host-updating')
 final=waitversion('0.2.0');time.sleep(5);shot('updates-real-host-done')
 (out/'updates-remote-results.json').write_text(json.dumps({'command':'Tauri host_update, invoked by the Update computer confirmation','release_a':'0.1.0','release_b':'0.2.0','current':final['current'],'previous':final['previous'],'healthy':final['healthy'],'signature':'throwaway test key, verified by host','source':'synthetic / raw','feed':'loopback only','services_touched':False},indent=2))
 print('PASS: real Tauri host_update changed signed host A 0.1.0 to B 0.2.0; B healthy.')
finally:
 server.shutdown();server.server_close()
 for p in reversed(processes):
  try:os.killpg(p.pid,signal.SIGTERM)
  except ProcessLookupError:pass
  try:p.wait(timeout=10)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
 for log in logs:log.close()
