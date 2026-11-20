#!/usr/bin/env python3
"""Create a manifest and sign its exact bytes. Private key is only opened by openssl."""
from inventory import ALLOWED, JOBS, files as job_files
import re
import argparse, datetime, hashlib, json, os, pathlib, subprocess
p=argparse.ArgumentParser();p.add_argument('version');p.add_argument('directory');p.add_argument('base_url');p.add_argument('--notes',default='');p.add_argument('--minimum-protocol',type=int,default=8)
# A release may be made for some of the platforms. Which ones is said out loud,
# and the directory must hold exactly their files.
p.add_argument('--targets',default=','.join(JOBS.values()));a=p.parse_args()
targets=a.targets.split(',')
if len(set(targets)) != len(targets) or not set(targets) <= set(JOBS.values()): raise ValueError("unknown or repeated release target")
if not 1 <= a.minimum_protocol <= 65535: raise ValueError("invalid minimum protocol")
r=pathlib.Path(a.directory);files=[]
# Both templates interpolate the feed into quoted shell source.
test_feed = bool(re.fullmatch(r"http://127\.0\.0\.1:[0-9]+(?:/[a-zA-Z0-9._/-]*)?", a.base_url))
if not (re.fullmatch(r"https://[a-zA-Z0-9.:-]+(?:/[a-zA-Z0-9._/-]*)?", a.base_url)
        or test_feed and os.environ.get("SUNNA_TEST_PUBLIC_KEY")):
    raise ValueError("invalid release base URL")
if not os.environ.get("SUNNA_TEST_PUBLIC_KEY"):
    expected_names = set.union(*(job_files(target) for target in targets))
    if {p.name for p in r.iterdir()} != expected_names:
        raise ValueError("incomplete or unexpected production inventory")
for f in sorted(r.iterdir()):
    name=f.name
    if name not in ALLOWED:
        if name.startswith(("sunna-", "Sunna-")): raise ValueError("unlisted artifact target")
        continue
    if f.is_symlink() or not f.is_file(): raise ValueError("expected regular artifact")
    if name.startswith('sunna-host-') or name.startswith('sunna-app-'):
        component=name.split('-')[1]; target=name.removeprefix('sunna-'+component+'-').removesuffix('.tar.gz').removesuffix('.zip')
    elif name.startswith('Sunna-macos-'): component='app';target=name.removeprefix('Sunna-').removesuffix('.zip')
    elif name=='sunna-android.apk':component='android';target='android'
    elif name.startswith('sunna-update-'):component='updater';target=name.removeprefix('sunna-update-').removesuffix('.exe')
    else: continue
    files.append(dict(component=component,target=target,url=a.base_url.rstrip('/')+'/'+name,size=f.stat().st_size,sha256=hashlib.file_digest(f.open('rb'),'sha256').hexdigest()))
assert files, 'no release artifacts'
manifest=dict(version=a.version,date=datetime.datetime.now(datetime.timezone.utc).isoformat(),notes=a.notes,minimum_protocol=a.minimum_protocol,files=files)
f=r/'manifest.json'; f.write_text(json.dumps(manifest,indent=2)+'\n')
key=os.environ['SUNNA_RELEASE_KEY']
expected = pathlib.Path(os.environ.get("SUNNA_TEST_PUBLIC_KEY", "crates/update/release.pub"))
pub = subprocess.check_output(["openssl", "pkey", "-in", key, "-pubout", "-outform", "DER"])
if pub != bytes.fromhex("302a300506032b6570032100") + expected.read_bytes():
    raise ValueError("signing key differs from compiled public key")
subprocess.run(['openssl','pkeyutl','-sign','-rawin','-inkey',key,'-in',str(f),'-out',str(r/'manifest.json.sig')],check=True)
# The bootstrap helper's hash is pinned in the installer delivered over HTTPS.
template=pathlib.Path('scripts/release/install.sh.in').read_text()
cases='\n'.join(f'  {x["target"]}) hash={x["sha256"]} ;;' for x in files if x['component']=='updater')
(r/'install.sh').write_text(template.replace('@CURL_PROTO@','=http' if test_feed else '=https').replace('@HASH_CASES@',cases).replace('@BASE_URL@',a.base_url.rstrip('/')))
windows = next((x for x in files if x['component']=='updater' and x['target']=='windows-x86_64'), None)
if windows:
    template=pathlib.Path('scripts/release/install.ps1.in').read_text()
    (r/'install.ps1').write_text(template.replace('@WINDOWS_HASH@',windows['sha256']).replace('@BASE_URL@',a.base_url.rstrip('/')))
