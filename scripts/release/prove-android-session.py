"""Run inside the dedicated offline emulator container with the two signed fixture feeds."""
from pathlib import Path
import os
import re
import shutil
import subprocess
import time
import xml.etree.ElementTree as ET

assert Path('/.dockerenv').exists() and os.environ.get('SUNNA_FIX_CONTAINER') == '1'
adb = ['/sdk/platform-tools/adb', '-s', 'emulator-5554']
package = 'dev.sunna.app'


def command(*args, timeout=50):
    return subprocess.check_output([*adb, *args], stderr=subprocess.DEVNULL, timeout=timeout).decode().strip()


def shell(*args):
    return command('shell', *args)


def wait(fn, label, seconds=100):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = fn()
            if value:
                return value
        except (subprocess.SubprocessError, ET.ParseError, ValueError):
            pass
        time.sleep(1)
    raise RuntimeError('timeout waiting for '+label)


def button(label, system=False):
    shell('uiautomator', 'dump', '/data/local/tmp/sunna-fix.xml')
    root = ET.fromstring(shell('cat','/data/local/tmp/sunna-fix.xml'))
    for n in root.iter('node'):
        if system and n.get('package') not in {'com.android.packageinstaller','com.google.android.packageinstaller','com.android.permissioncontroller'}:
            continue
        if n.get('text','').lower() == label.lower():
            x,y,xx,yy = map(int,re.findall(r'\d+',n.get('bounds')))
            return str((x+xx)//2),str((y+yy)//2)


def tap(label, system=False):
    shell('input','tap',*wait(lambda:button(label,system),label))


def select(folder):
    source=Path('/proof')/folder
    dest=Path('/proof/feed/B')
    for f in source.iterdir(): shutil.copyfile(f,dest/f.name)


def open_app():
    shell('am','start','-n',package+'/.MainActivity')


def booted():
    if shell('getprop','sys.boot_completed') != '1': return False
    shell('input','keyevent','KEYCODE_WAKEUP')
    return shell('pm','path','android').startswith('package:')
wait(booted,'responsive Android framework',600)
print('PASS Android framework booted',flush=True)
shell('wm','dismiss-keyguard')
assert command('install','-r','/proof/A.apk',timeout=180).endswith('Success')
command('reverse','tcp:48999','tcp:48999')
select('approved')
open_app()
wait(lambda:button('Download'),'signed offer')
select('changed')
tap('Download')
wait(lambda:button('Release changed. Check for updates again.'),'changed release rejection')
print('PASS signed feed changed after prompt; download refused',flush=True)
select('approved')
shell('pm','clear',package)
open_app()
tap('Download')
tap('Cancel')
prefs=shell('run-as',package,'cat','shared_prefs/updates.xml')
assert 'name="digest"' not in prefs and 'permissionPending' not in prefs
print('PASS cancelled permission prompt retained no install consent',flush=True)
shell('pm','clear',package)
shell('appops','set',package,'REQUEST_INSTALL_PACKAGES','allow')
open_app()
tap('Download')
tap('Install',system=True)
wait(lambda:'versionName=0.2.0' in shell('dumpsys','package',package),'confirmed Android upgrade',180)
print('PASS system installer confirmation upgraded fixture A to B',flush=True)
