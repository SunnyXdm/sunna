"""Source-checkout update Settings in real WebKitGTK, on an owned display/profile."""
import os,subprocess,time,tempfile,shutil,json
from pathlib import Path
root=Path(__file__).resolve().parents[3]
out=root/'apps/sunna/dev/design/shots-app'
home=Path(tempfile.mkdtemp(prefix='sunna-updui-native-'))
(home/'.sunna').mkdir();(home/'.sunna/config.json').write_text(json.dumps({'view':'devices','pictures':'off'}))
env={**os.environ,'HOME':str(home),'XDG_CONFIG_HOME':str(home/'.config'),'XDG_DATA_HOME':str(home/'.local/share'),'DISPLAY':':86','GDK_BACKEND':'x11','WEBKIT_DISABLE_DMABUF_RENDERER':'1','LIBGL_ALWAYS_SOFTWARE':'1','SUNNA_LOG_URL':'','SUNNA_LOG_TOKEN':''}
assert not Path('/tmp/.X11-unix/X86').exists(), 'Display :86 in use'
log=open(home/'native.log','w');processes=[]
def run(*args):return subprocess.run(args,env=env,capture_output=True,text=True,check=True,timeout=20).stdout.strip()
def key(k):run('xdotool','key','--clearmodifiers',k);time.sleep(.6)
try:
 processes.append(subprocess.Popen(['Xvfb',':86','-screen','0','1280x900x24','-nolisten','tcp'],env=env,stdout=log,stderr=log));time.sleep(.6)
 processes.append(subprocess.Popen(['openbox'],env=env,stdout=log,stderr=log));time.sleep(.5)
 processes.append(subprocess.Popen([str(root/'target/debug/sunna')],env=env,stdout=log,stderr=log));time.sleep(7)
 w=run('xdotool','search','--onlyvisible','--name','^Sunna$').splitlines()[0];run('xdotool','windowfocus',w)
 for size,width,height in [('default',980,660),('minimum',680,480)]:
  run('xdotool','windowsize',w,str(width),str(height));time.sleep(.5)
  for view,shortcut in [('devices','ctrl+1'),('space','ctrl+3')]:
   key(shortcut);key('ctrl+comma');time.sleep(.7)
   run('xdotool','mousemove','--window',w,str(width//2),str(height//2),'click','--repeat','24','--delay','25','5');time.sleep(.7)
   run('import','-window',w,str(out/f'updates-webkit-{view}-{size}.png'));key('Escape')
 (out/'updates-native-results.json').write_text(json.dumps({'engine':'Tauri / WebKitGTK','source_binary':'target/debug/sunna','display':':86','captures':4,'message':'This copy cannot update itself. Install a signed download first. For a system package, use apt, dnf or pacman.'},indent=2))
 print('Captured native update Settings in dark and Space at both sizes.')
finally:
 for p in reversed(processes):
  p.terminate()
  try:p.wait(timeout=10)
  except subprocess.TimeoutExpired:p.kill();p.wait()
 log.close();shutil.rmtree(home)
