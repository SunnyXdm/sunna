#!/usr/bin/env python3
"""Host-side helpers for the actual X sessions (no screenshot compositing)."""
import subprocess, shlex, sys
C='sunna-frames-desktops'
def run(command):
    return subprocess.run(['docker','exec',C,'bash','-lc',command],check=True,text=True,capture_output=True).stdout
def ui(user,command):
    # Read session bus address without exposing environment in screenshots.
    env=run('cat /tmp/env-'+user)
    variables=dict(line.split('=',1) for line in env.splitlines() if '=' in line)
    args=['docker','exec','-u',user]
    for key in ('DISPLAY','DBUS_SESSION_BUS_ADDRESS','XDG_RUNTIME_DIR'):
        args+=['-e',key+'='+variables[key]]
    args += [C,'bash','-lc',command]
    return subprocess.run(args,check=True,text=True,capture_output=True).stdout
if __name__=='__main__':
    print(ui(sys.argv[1],sys.argv[2]))
