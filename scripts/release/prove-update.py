#!/usr/bin/env python3
"""Local end-to-end proof. Never uses the real release key or other machines.
Run after building A=0.1.0 and B=0.2.0; uses the local server on 48993.
"""
import argparse, contextlib, hashlib, http.server, json, os, pathlib, shutil, socketserver, subprocess, tarfile, tempfile, threading, time
p=argparse.ArgumentParser();p.add_argument('work');p.add_argument('client');a=p.parse_args()
w=pathlib.Path(a.work).resolve();releases=w/'releases'; client=str(pathlib.Path(a.client).resolve())
key=w/'test.key'; assert key.is_file()
base='http://127.0.0.1:48993'
env=os.environ.copy(); env.update(DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent-sunna-fix',XDG_RUNTIME_DIR=str(w/'runtime'),SUNNA_UPDATE_URL=base,SUNNA_LOG_URL='',SUNNA_LOG_TOKEN='',SUNNA_TOKEN=os.urandom(16).hex())
# The proof clients run outside their installed package. Use the same FFmpeg
# libraries as the packaged host, including for the previous revision's viewer.
libs = w / 'proof-libraries'
libs.mkdir(exist_ok=True)
with tarfile.open(releases / 'A' / 'sunna-host-linux-x86_64.tar.gz') as archive:
    for member in archive:
        name = pathlib.PurePosixPath(member.name)
        if member.isfile() and name.parent == pathlib.PurePosixPath('lib') and name.name.startswith(('libavutil.so.', 'libavcodec.so.')):
            with archive.extractfile(member) as source, (libs / name.name).open('wb') as dest:
                shutil.copyfileobj(source, dest)
env['LD_LIBRARY_PATH'] = str(libs) + (os.pathsep + env['LD_LIBRARY_PATH'] if env.get('LD_LIBRARY_PATH') else '')
results=[]
def run(cmd,env=env,ok=True):
    r=subprocess.run(list(map(str,cmd)),env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=180)
    if ok and r.returncode: raise RuntimeError(f'{cmd[0]} failed ({r.returncode}): {r.stderr[-2000:]}')
    if not ok: assert r.returncode != 0, f'{cmd[0]} unexpectedly succeeded'
    return r

def sign(name,version,minimum=8):
    e=env.copy();e['SUNNA_RELEASE_KEY']=str(key);e['SUNNA_TEST_PUBLIC_KEY']=str(w/'test.pub')
    run(['python3','scripts/release/manifest.py',version,releases/name,base+'/'+name,'--minimum-protocol',str(minimum)],env=e)
def select(name):
    for f in (releases/name).iterdir():
        link=releases/f.name
        if link.is_symlink() or link.is_file(): link.unlink()
        link.symlink_to(pathlib.Path(name)/f.name)
def state(e): return json.loads((pathlib.Path(e['HOME'])/'.local/share/sunna/host/state.json').read_text())
def wait(e,version,timeout=50):
    stop=time.monotonic()+timeout
    while time.monotonic()<stop:
        s=state(e)
        if s['current']==version and s['healthy']: return s
        time.sleep(.25)
    raise AssertionError(f'timed out waiting for healthy {version}: {state(e)}')
def record(label): results.append(label); print('PASS:',label,flush=True)
def install(name):
    select(name)
    h=pathlib.Path(tempfile.mkdtemp(prefix='sunna-upd-home-',dir=w));e=env.copy();e['HOME']=str(h);e.pop('XDG_DATA_HOME',None);e.pop('SUNNA_UPDATE_ROOT',None)
    run(['sh',releases/name/'install.sh','host'],env=e)
    return e
@contextlib.contextmanager
def host(e):
    with socketserver.TCPServer(('127.0.0.1',0),None) as s: port=s.server_address[1]
    address=f'127.0.0.1:{port}'
    log=open(pathlib.Path(e['HOME'])/'host.log','w')
    process=subprocess.Popen([str(pathlib.Path(e['HOME'])/'.local/bin/sunna-update'),'supervise','host','--listen',address,'--source','synthetic','--codec','raw','--no-audio','--no-clipboard'],env=e,stdout=log,stderr=log,start_new_session=True)
    try:
        wait(e,'0.1.0'); yield address
    finally:
        import signal
        try: os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError: pass
        process.wait(timeout=10);log.close()
pause_artifact=threading.Event()
downloading=threading.Event()
continue_download=threading.Event()
class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if pause_artifact.is_set() and self.path.endswith('/sunna-host-linux-x86_64.tar.gz'):
            downloading.set()
            continue_download.wait(20)
        try: super().do_GET()
        except (BrokenPipeError,ConnectionResetError): pass
    def __init__(self,*args,**kwargs): super().__init__(*args,directory=str(releases),**kwargs)
    def log_message(self,*args): pass
sign('A','0.1.0');sign('B','0.2.0')
server=http.server.ThreadingHTTPServer(('127.0.0.1',48993),Handler)
threading.Thread(target=server.serve_forever,daemon=True).start()
try:
    e=install('A')
    with host(e) as addr:
        select('B');run([pathlib.Path(e['HOME'])/'.local/bin/sunna-host','update','--yes'],env=e)
        wait(e,'0.2.0');record('command A -> B, supervised restart and health confirmation')
    e=install('A')
    with host(e) as addr:
        select('B');run([client,'auto',addr],env=e)
        # A was checked at startup; force the next daily interval without waiting a day.
        f=pathlib.Path(e['HOME'])/'.local/share/sunna/host/state.json';s=state(e);s['last_check']=0;f.write_text(json.dumps(s))
        wait(e,'0.2.0');record('automatic policy A -> B when the daily check is due')
    e=install('A')
    with host(e) as addr:
        hold=subprocess.Popen([client,'hold',addr,'22'],env=e,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        assert hold.stdout.readline().strip()=='connected'
        select('B');run([client,'update',addr],env=e)
        time.sleep(8);assert state(e)['current']=='0.1.0';assert state(e)['pending']=='0.2.0'
        assert hold.wait(timeout=30)==0;wait(e,'0.2.0');record('authenticated wire request waits through active session, then A -> B')
    e=install('A')
    with host(e) as addr:
        select('B');pause_artifact.set()
        run([client,'update',addr],env=e)
        assert downloading.wait(12) and state(e)['request']=='update'
        # Exiting this fixture terminates the supervisor and child together, mid-request.
    pause_artifact.clear();continue_download.set()
    with host(e) as addr:
        wait(e,'0.2.0')
        assert state(e)['request'] is None
        run([client,'probe',addr],env=e)
        record('interrupted download resumed its durable request and committed once')
    for kind in ['signature','hash','truncated']:
        bad=releases/kind;shutil.copytree(releases/'B',bad,dirs_exist_ok=True);sign(kind,'0.2.0')
        if kind=='signature': (bad/'manifest.json.sig').write_bytes(bytes(64))
        else:
            f=bad/'sunna-host-linux-x86_64.tar.gz'
            with f.open('r+b') as out:
                if kind=='truncated':out.truncate(f.stat().st_size//2)
                else:out.write(b'corrupted')
        e=install('A')
        with host(e) as addr:
            select(kind);run([client,'update',addr],env=e);time.sleep(9)
            assert state(e)['current']=='0.1.0';run([client,'probe',addr],env=e)
            status=json.loads(run([client,'status',addr],env=e).stdout)
            assert status['state']['error'], 'background rejection missing from authenticated status'
            record(f'{kind} refused; A remains reachable')
    broken=releases/'broken';shutil.copytree(releases/'B',broken,dirs_exist_ok=True)
    stage=w/'broken-stage';stage.mkdir(exist_ok=True)
    run(['tar','-xzf',broken/'sunna-host-linux-x86_64.tar.gz','-C',stage])
    exe=stage/'bin/sunnad';exe.write_text('#!/bin/sh\nif [ "${1:-}" = --version ]; then echo "sunnad 0.2.0"; exit 0; fi\nexit 42\n');exe.chmod(0o755)
    run(['tar','-czf',broken/'sunna-host-linux-x86_64.tar.gz','-C',stage,'.']);sign('broken','0.2.0')
    e=install('A')
    with host(e) as addr:
        select('broken');run([client,'update',addr],env=e)
        time.sleep(20);s=wait(e,'0.1.0');assert '0.2.0' in s['failed'];run([client,'probe',addr],env=e)
        run([client,'update',addr],env=e);time.sleep(6);assert state(e)['current']=='0.1.0'
        record('B passes preflight but crashes at startup: automatic rollback and failed-version suppression')
    select('A')
    e=install('A')
    helper=pathlib.Path(e['HOME'])/'.local/bin/sunna-update'
    select('B')
    run([helper,'update','host','--yes'],env=e)
    run([helper,'update','host','--rollback'],env=e)
    mirror=releases/'stale';shutil.copytree(releases/'B',mirror,dirs_exist_ok=True)
    sign('stale','0.1.5');select('stale')
    failure=run([helper,'update','host','--yes'],env=e,ok=False)
    assert 'version floor' in failure.stderr and state(e)['current']=='0.1.0'
    record('signed intermediate release rejected after explicit rollback')
    e=install('A')
    with host(e) as addr:
        future=releases/'future';shutil.copytree(releases/'B',future,dirs_exist_ok=True)
        sign('future','0.2.0',9);select('future')
        run([pathlib.Path(e['HOME'])/'.local/bin/sunna-update','check','host'],env=e)
        refused=run([client,'update',addr],env=e,ok=False)
        assert 'update the app first' in refused.stderr and state(e)['request_id']==0
        record('app refuses protocol-9 release before sending update from protocol 8')
    # Use A's installer while latest points to B. Exercise the actual wrapped app launcher.
    select('B')
    appenv=env.copy()
    appenv['HOME']=tempfile.mkdtemp(dir=w)
    appenv.pop('XDG_DATA_HOME',None)
    run(['sh',releases/'A/install.sh','app'],env=appenv)
    approot=pathlib.Path(appenv['HOME'])/'.local/share/sunna/app'
    assert json.loads((approot/'state.json').read_text())['current']=='0.1.0'
    record('bootstrap installs its pinned A while latest serves B')
    for missing in ['sunna-update','sunna']:
        label='app-missing-'+missing
        pkg=releases/label
        shutil.copytree(releases/'B',pkg,dirs_exist_ok=True)
        with tempfile.TemporaryDirectory(dir=w) as stage:
            archive=pkg/'sunna-app-linux-x86_64.tar.gz'
            run(['tar','-xzf',archive,'-C',stage])
            (pathlib.Path(stage)/'bin'/missing).unlink()
            run(['tar','-czf',archive,'-C',stage,'.'])
        sign(label,'0.2.0')
        select(label)
        # Each failed release is quarantined, so use a fresh A for each negative case.
        appenv['HOME']=tempfile.mkdtemp(dir=w)
        run(['sh',releases/'A/install.sh','app'],env=appenv)
        approot=pathlib.Path(appenv['HOME'])/'.local/share/sunna/app'
        run([approot/'current/bin/sunna-update','update','app'],env=appenv,ok=False)
        assert (approot/'current').resolve().name=='0.1.0'
        record('missing app launch component rejected during prepare: '+missing)
    # Original installer output (built and installed in an isolated container).
    legacy=w/'legacy'
    assert (legacy/'sunnad').is_file(), 'run the legacy install proof first'
    select('A')
    home=pathlib.Path(tempfile.mkdtemp(prefix='sunna-upd-migration-',dir=w));e=env.copy();e['HOME']=str(home);e.pop('XDG_DATA_HOME',None);e.pop('SUNNA_UPDATE_ROOT',None)
    (home/'.local/bin').mkdir(parents=True)
    for name in ['sunnad','sunna-host']: shutil.copy2(legacy/name,home/'.local/bin'/name)
    run(['sh',releases/'A/install.sh','host'],env=e)
    assert (home/'.local/bin/sunnad').is_symlink()
    assert state(e)['previous'].endswith('+legacy')
    with host(e) as addr:
        select('B');run([home/'.local/bin/sunna-host','update','--yes'],env=e);wait(e,'0.2.0')
        record('plain files produced by original source installer migrate and update to B')
    e=install('A')
    with host(e) as addr:
        run([w/'old-client','probe',addr],env=e)
        run([w/'old-client','hold',addr,'2'],env=e)
        record('current-main client probes and opens a session on new host')
    oldhome=pathlib.Path(tempfile.mkdtemp(prefix='sunna-upd-old-home-',dir=w));e=env.copy();e['HOME']=str(oldhome)
    with socketserver.TCPServer(('127.0.0.1',0),None) as sock: addr=f'127.0.0.1:{sock.server_address[1]}'
    log=open(oldhome/'host.log','w')
    old=subprocess.Popen([w/'old-sunnad','--listen',addr,'--source','synthetic','--codec','raw','--no-audio','--no-clipboard'],env=e,stdout=log,stderr=log,start_new_session=True)
    try:
        time.sleep(2)
        assert 'Needs an update' in run([client,'probe',addr],env=e,ok=False).stderr
        e['SUNNA_TEST_ALLOW_OLDER']='1'
        assert 'not fully verified' in run([client,'probe',addr],env=e).stdout
        run([client,'hold',addr,'2'],env=e)
        refusal=run([client,'update',addr],env=e,ok=False)
        assert 'predates remote updates' in refusal.stderr
        record('new client probes and opens a session on current-main host; update reports unsupported')
    finally:
        import signal
        try: os.killpg(old.pid,signal.SIGTERM)
        except ProcessLookupError: pass
        old.wait(timeout=10);log.close()
    (w/'results.json').write_text(json.dumps(results,indent=2)+'\n')
finally: server.shutdown()
