"""Native keyboard removal proof; uses an isolated fixture."""
import os,subprocess,time,json
from pathlib import Path
from tempfile import TemporaryDirectory
from native_fixture import seed_machines
workspace=TemporaryDirectory(prefix="sunna-remove-")
profile=Path(workspace.name)
seed_machines(profile)
root=Path(__file__).resolve().parents[3];out=root/'apps/sunna/dev/design/shots-app';env={**os.environ,'DISPLAY':':67','GDK_BACKEND':'x11','HOME':str(profile),'XDG_CONFIG_HOME':str(profile/'.config'),'XDG_DATA_HOME':str(profile/'.local/share'),'WEBKIT_DISABLE_DMABUF_RENDERER':'1','LIBGL_ALWAYS_SOFTWARE':'1'}
store=profile/'.sunna/machines.json';picture=profile/'.sunna/pictures/proof0.jpg'
picture.parent.mkdir(parents=True,exist_ok=True)
picture.write_bytes((root/'apps/sunna/dev/design/frames/01-plasma-rust.jpg').read_bytes());picture.chmod(0o600)
def run(*args):return subprocess.run(args,env=env,capture_output=True,text=True,timeout=20,check=True).stdout.strip()
def key(k):run('xdotool','key','--clearmodifiers',k);time.sleep(.7)
log=open(profile/'native.log','w');wm=subprocess.Popen(['openbox'],env=env,stdout=log,stderr=log);app=subprocess.Popen([str(root/'target/debug/sunna')],env=env,stdout=log,stderr=log)
try:
 time.sleep(7);w=run('xdotool','search','--onlyvisible','--name','Sunna').splitlines()[0];run('xdotool','windowfocus',w);key('ctrl+1');time.sleep(1)
 for _ in range(6):key('Left')
 key('Delete');run('import','-window',w,str(out/'webkit-remove-picture.png'));key('Tab');key('Return');time.sleep(2)
 saved=json.loads(store.read_text())['machines'];assert not any(m['id']=='proof0' for m in saved),'Computer was not removed';assert not picture.exists(),'Picture remained'
 result=json.loads((out/'native-results.json').read_text());result['removal_deletes_picture']=True;(out/'native-results.json').write_text(json.dumps(result,indent=2));print('Native keyboard removal deleted the saved picture.')
finally:
 app.terminate();app.wait(timeout=10);wm.terminate();wm.wait(timeout=10);log.close();workspace.cleanup()
