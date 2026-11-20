"""Record two isolated X11 displays. Run from the repository root."""
import os, secrets, subprocess as sp, sys, time
from pathlib import Path

root = Path.cwd()
work = Path('~/ext/work/site/r4').expanduser()
home = work/'home'
bin_dir = Path(sys.argv[1]).resolve() if len(sys.argv)>1 else work/'target/debug'
for directory in [home,work/'tmp',work/'run']:
    directory.mkdir(mode=0o700,parents=True,exist_ok=True)
env = dict(PATH='/usr/local/bin:/usr/bin:/bin', HOME=str(home), TMPDIR=str(work/'tmp'),
           XDG_CACHE_HOME=str(work/'cache'), XDG_CURRENT_DESKTOP='XFCE', NO_AT_BRIDGE='1', XDG_RUNTIME_DIR=str(work/'run'), XDG_CONFIG_HOME=str(home/'.config'),
           LANG='C.UTF-8', SDL_AUDIODRIVER='dummy', SUNNA_TOKEN=secrets.token_hex(16),
           SUNNA_STATS='0', SUNNA_SUPERVISED='1', XDG_SESSION_TYPE='x11')
procs = []
def start(args, display=60, **kw):
    p=sp.Popen(args,env=dict(env,DISPLAY=f':{display}'),cwd=home,**kw)
    procs.append(p)
    return p
def run(args,display=61):
    return sp.check_output(args,env=dict(env,DISPLAY=f':{display}'),cwd=home)
def snap(name):
    run(['import','-window','root',str(work/(name+'.png'))])
try:
    for d in [60,61]:
        if Path(f'/tmp/.X11-unix/X{d}').exists(): raise RuntimeError('display occupied')
        start(['Xvfb',f':{d}','-screen','0','1280x862x24','-nolisten','tcp','-noreset'],d)
    time.sleep(.55)
    start(['openbox','--sm-disable'],61)
    bus=start(['dbus-daemon','--session','--address=unix:path='+str(work/'run/bus'),'--nofork','--print-address=1'],stdout=sp.PIPE)
    env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().decode().strip()
    # Start the stock XFCE session using capture-specific configuration.
    defaults=work/'defaults/xfce4/xfconf/xfce-perchannel-xml'
    defaults.mkdir(parents=True,exist_ok=True)
    import shutil
    shutil.copy('/etc/xdg/xfce4/xfconf/xfce-perchannel-xml/xfce4-session.xml',defaults)
    env['XDG_CONFIG_DIRS']=str(work/'defaults')
    settings=home/'.config/xfce4/xfconf/xfce-perchannel-xml'
    settings.mkdir(parents=True,exist_ok=True)
    panel=Path('/etc/xdg/xfce4/panel/default.xml').read_text().replace('<value type="int" value="10"/>','')
    (settings/'xfce4-panel.xml').write_text(panel)
    for agent in ['ssh-agent','gpg-agent']:
        run(['xfconf-query','-c','xfce4-session','-p',f'/startup/{agent}/enabled',
             '--create','-t','bool','-s','false'],60)
    start(['xfce4-session','--disable-tcp'])
    time.sleep(4)
    def prop(channel,key,kind,value):
        run(['xfconf-query','-c',channel,'-p',key,'--create','-t',kind,'-s',value],60)
    for ws in range(4):
        prop('xfce4-desktop',f'/backdrop/screen0/monitorscreen/workspace{ws}/last-image','string','/usr/share/backgrounds/xfce/xfce-teal.svg')
    prop('xfce4-desktop','/desktop-icons/style','int','0')
    prop('thunar','/last-location-bar','string','ThunarLocationButtons')
    prop('thunar','/last-side-pane','string','void')
    project=work.parent/'demo/Project'
    for folder in ['Notes','Sources']:
        (project/folder).mkdir(parents=True,exist_ok=True)
    for name in ['Readme.md','Checklist.txt']:
        (project/name).touch()
    alias=home/'Project'
    if not alias.exists(): alias.symlink_to(project)
    start(['thunar',str(alias)])
    time.sleep(1)
    run(['wmctrl','-r','Project','-e','0,590,82,590,396'],60)
    dest=home/'sunna'
    dest.mkdir(exist_ok=True)
    # Use the real crate source and lockfile with a target local to the demo.
    manifest=(root/'crates/proto/Cargo.toml').read_text()
    workspace=(root/'Cargo.toml').read_text().splitlines()
    for line in manifest.splitlines():
        if '.workspace = true' in line:
            key=line.split('.')[0]
            manifest=manifest.replace(line,next(v for v in workspace if v.startswith(key+' = ')))
    (dest/'Cargo.toml').write_text(manifest)
    for name in ['src','tests']:
        if not (dest/name).exists(): (dest/name).symlink_to(root/'crates/proto'/name)
    shutil.copy(root/'Cargo.lock',dest/'Cargo.lock')
    toolchain=Path(sp.check_output(['rustc','--print','sysroot'],text=True).strip())/'bin'
    env.update(PATH=str(toolchain)+':'+env['PATH'],CARGO_HOME=str(Path.home()/'.cargo'),CARGO_NET_OFFLINE='true')
    sp.run(['cargo','test','--lib','roundtrip','--no-run'],env=env,cwd=dest,check=True)
    rc=work/'shell.rc'
    rc.write_text("PS1='dev@ubuntu:~/sunna$ '\nunset PROMPT_COMMAND\n")
    start(['xfce4-terminal','--disable-server','--hide-menubar','--hide-toolbar','--hide-scrollbar','--font=DejaVu Sans Mono 13','--geometry=82x19+74+287','--color-bg=#20242b','--color-text=#e5e9ed','--title=Terminal','--working-directory='+str(dest),'-x','bash','--rcfile',str(rc),'-i'])
    time.sleep(1)
    start([str(bin_dir/'sunnad'),'--listen','127.0.0.1:48860','--name','ubuntu','--source','screen','--fps','24','--no-clipboard','--no-audio'],stdout=sp.DEVNULL,stderr=sp.DEVNULL)
    time.sleep(.9)
    viewer=start([str(bin_dir/'sunna-cli'),'view','127.0.0.1:48860','--name','ubuntu','--no-audio'],61,stdout=sp.DEVNULL,stderr=sp.DEVNULL)
    time.sleep(7)
    if viewer.poll() is not None: raise RuntimeError('viewer exited')
    run(['xdotool','mousemove','1279','861']); time.sleep(.5)
    snap('before')
    rec=start(['ffmpeg','-nostdin','-y','-loglevel','error','-f','x11grab','-draw_mouse','1','-framerate','24','-video_size','1280x862','-i',':61','-c:v','libx264rgb','-crf','0','-preset','ultrafast',str(work/'session.mkv')],61)
    time.sleep(.45)
    run(['xdotool','mousemove','985','586'])
    time.sleep(.25)
    run(['xdotool','type','--delay','65','cargo test --lib roundtrip'])
    run(['xdotool','key','Return']);time.sleep(2)
    snap('result')
    run(['xdotool','type','--delay','70','clear']);run(['xdotool','key','Return']);time.sleep(.65)
    run(['xdotool','mousemove','1279','861']);time.sleep(.35)
    snap('after')
    run(['xdotool','mousemove','32','25','click','1']);time.sleep(.65)
    snap('menu')
    run(['xdotool','mousemove','70','257'])
    time.sleep(.6);run(['xdotool','click','1']);viewer.wait(timeout=4)
finally:
    for p in reversed(procs):
        if p.poll() is None:
            p.terminate()
            try: p.wait(timeout=4)
            except sp.TimeoutExpired: p.kill();p.wait()

# Feed the mock actual decoded frames from the finished viewer recording.
for name,second in [('before','0.08'),('after','5.00')]:
    sp.run(['ffmpeg','-y','-loglevel','error','-ss',second,'-i',str(work/'session.mkv'),
            '-frames:v','1',str(work/(name+'.png'))],env=env,check=True)
