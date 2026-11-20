"""Real Tauri/WebKitGTK proof on an owned Xvfb display and isolated HOME.
Point SUNNA_NATIVE_ADDRESS and SUNNA_TOKEN at an owned test listener, or use the sunna-shot fixture.
"""
import ctypes,struct,os,subprocess,time,json,signal,sys
from pathlib import Path
from tempfile import TemporaryDirectory
from native_fixture import seed_machines
ROOT=Path(__file__).resolve().parents[3]
OUT=Path(os.environ.get('SUNNA_NATIVE_OUTPUT',ROOT/'apps/sunna/dev/design/shots-app'))
OUT.mkdir(parents=True,exist_ok=True)
workspace=TemporaryDirectory(prefix='sunna-native-')
PROFILE=Path(workspace.name)
if '--views-only' in sys.argv:
 seed_machines(PROFILE)
elif address := os.environ.get('SUNNA_NATIVE_ADDRESS'):
 seed_machines(PROFILE,os.environ['SUNNA_TOKEN'],address)
else:
 variables=json.loads(subprocess.check_output(['docker','inspect','--format','{{json .Config.Env}}','sunna-shot']))
 seed_machines(PROFILE,next(v.partition('=')[2] for v in variables if v.startswith('SUNNA_TOKEN=')))
ENV={**os.environ,'DISPLAY':os.environ.get('SUNNA_NATIVE_DISPLAY',':67'),'GDK_BACKEND':'x11','WEBKIT_DISABLE_DMABUF_RENDERER':'1','LIBGL_ALWAYS_SOFTWARE':'1','HOME':str(PROFILE),'XDG_CONFIG_HOME':str(PROFILE/'.config'),'XDG_DATA_HOME':str(PROFILE/'.local/share')}
def run(*args,check=True):return subprocess.run(args,env=ENV,capture_output=True,text=True,check=check,timeout=20).stdout.strip()
def xd(*args):return run('xdotool',*map(str,args))
def ids():return run('xdotool','search','--onlyvisible','--name','Sunna',check=False).splitlines()
def geometry(w):return dict(s.split('=',1) for s in xd('getwindowgeometry','--shell',w).splitlines())
def shot(name,w=None):run('import','-window',w or 'root',str(OUT/f'webkit-{name}.png'))
def focus(w):xd('windowfocus',w)
def press(w,key):focus(w);xd('key',key);time.sleep(1.2)
prefs=PROFILE/'.sunna/config.json'
log=open(PROFILE/'native.log','w')
wm=subprocess.Popen(['openbox'],env=ENV,stdout=log,stderr=log,start_new_session=True)
app=subprocess.Popen([str(ROOT/'target/debug/sunna')],env=ENV,stdout=log,stderr=log,start_new_session=True)
results={}
video=None
try:
 time.sleep(8)
 windows=ids()
 for _ in range(20):
  if windows:break
  time.sleep(.5);windows=ids()
 assert windows,'No visible Tauri window';w=windows[0]
 focus(w);xd('mousemove','--window',w,500,630,'click',1)
 for _ in range(35):
  press(w,'ctrl+5')
  if json.loads(prefs.read_text()).get('view')=='gallery':break
 else:raise AssertionError('Native page did not become interactive')
 time.sleep(3)
 for i,view in enumerate(['devices','wall','space','type','gallery'],1):
  press(w,f'ctrl+{i}');time.sleep(.8)
  chosen=json.loads((PROFILE/'.sunna/config.json').read_text()).get('view')
  assert chosen==view,f'View shortcut did not persist {view}: {chosen}'
  press(w,'Right');press(w,'Left');shot(view,w);results[view]='rendered and persisted'
 print('Captured five native views; keyboard switching persisted.',flush=True)
 press(w,'ctrl+3');press(w,'ctrl+comma');shot('settings-space',w);press(w,'Escape')
 before=geometry(w)
 xd('mousemove','--window',w,400,25,'mousedown',1);time.sleep(.4)
 xd('mousemove_relative','--sync',40,30);time.sleep(.4);xd('mouseup',1)
 time.sleep(.8);after=geometry(w)
 results['toolbar_drag']=(before['X'],before['Y'])!=(after['X'],after['Y'])
 shot('after-drag',w)
 xd('windowmove',w,150,140)
 xd('windowsize',w,680,480);time.sleep(1)
 for i,view in enumerate(['devices','wall','space','type','gallery'],1):
  press(w,f'ctrl+{i}');shot(f'{view}-minimum',w)
 xd('windowsize',w,980,660);time.sleep(1)
 if '--views-only' in sys.argv:
  previous=json.loads((OUT/'native-results.json').read_text());previous.update(results)
  (OUT/'native-results.json').write_text(json.dumps(previous,indent=2))
  print('Final native views captured.',flush=True);sys.exit(0)
 picture=PROFILE/'.sunna/pictures/proof0.jpg'
 picture.parent.mkdir(exist_ok=True)
 libc=ctypes.CDLL(None,use_errno=True)
 fd=libc.inotify_init1(os.O_NONBLOCK)
 def events():
  try:data=os.read(fd,65536)
  except BlockingIOError:return []
  writes=[]
  while data:
   _,mask,_,length=struct.unpack_from('iIII',data)
   name=data[16:16+length].rstrip(b'\0').decode()
   writes.append((name,mask));data=data[16+length:]
  return writes
 for mode,x,limit in [('sharp',569,1280),('blurred',631,32),('off',685,0)]:
  if mode!='sharp':
   press(w,'ctrl+comma');xd('mousemove','--window',w,x,262,'click',1);time.sleep(1)
   assert json.loads(prefs.read_text())['pictures']==mode,mode
   assert not picture.exists(),f'{mode} retained earlier picture'
   press(w,'Escape')
  picture.parent.mkdir(exist_ok=True)
  assert libc.inotify_add_watch(fd,os.fsencode(picture.parent),0x80|0x8)>=0
  events()
  if mode=='sharp':
   video=subprocess.Popen(['ffmpeg','-loglevel','error','-y','-f','x11grab','-framerate','12','-video_size','1280x900','-i',ENV['DISPLAY'],'-c:v','libx264','-preset','ultrafast','-crf','25','-pix_fmt','yuv420p',str(OUT/'webkit-connect-leave.mp4')],stdout=log,stderr=log)
  press(w,'ctrl+alt+1')
  for _ in range(40):
   viewer=next((win for win in ids() if win!=w),None)
   if viewer:break
   time.sleep(.5)
  assert viewer,f'{mode}: native viewer did not open'
  time.sleep(32)
  assert not events(),f'{mode}: picture written during session'
  if mode=='sharp':shot('viewer')
  press(viewer,'ctrl+alt+q');time.sleep(3)
  assert viewer not in ids(),'Viewer did not leave'
  writes=events()
  if limit:
   assert writes.count((picture.name,0x80))==1,writes
   width,height=map(int,run('identify','-format','%w %h',str(picture)).split())
   assert 0<width<=limit and picture.stat().st_mode & 0o777==0o600
   results[mode]=dict(width=width,height=height,final_writes=1,during_session=0)
   if mode=='sharp':
    shot('returned',w);video.send_signal(signal.SIGINT);video.wait(timeout=15);video=None
  else:
   assert not writes and not picture.exists()
   results[mode]=dict(final_writes=0,during_session=0)
  print(f'{mode}: passed',flush=True)
 os.close(fd)
 results['native_session']='connected and left with Ctrl+Alt+Q'
 (OUT/'native-results.json').write_text(json.dumps(results,indent=2))
 print(json.dumps(results),flush=True)
finally:
 if video and video.poll() is None:video.send_signal(signal.SIGINT);video.wait(timeout=15)
 if app.poll() is None:app.terminate();app.wait(timeout=10)
 if wm.poll() is None:wm.terminate();wm.wait(timeout=10)
 log.close();workspace.cleanup()
