#!/usr/bin/env python3
"""Set all release version fields in a staging checkout or the working tree."""
import argparse, json, pathlib, re
p = argparse.ArgumentParser(); p.add_argument('version'); p.add_argument('--check', action='store_true'); p.add_argument('--root', default='.'); a = p.parse_args()
assert re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)', a.version), 'expected X.Y.Z'
r = pathlib.Path(a.root)
major, minor, patch = map(int,a.version.split('.'))
assert minor < 1000 and patch < 1000
code = major*1000000 + minor*1000 + patch
assert 0 < code < 2100000000
if a.check:
    gradle = (r/'apps/android/app/build.gradle.kts').read_text()
    cargo = (r/'Cargo.toml').read_text().split('[workspace.package]',1)[1].split('[',1)[0]
    if (not re.search(r'^version = "' + re.escape(a.version) + '"$', cargo, re.M)
        or json.loads((r/'apps/sunna/tauri.conf.json').read_text())['version'] != a.version
        or f'versionName = "{a.version}"' not in gradle
        or not re.search(r'^ *versionCode = ' + str(code) + '$', gradle, re.M)):
        raise SystemExit('version mismatch between tag and checkout')
    raise SystemExit(0)
f = r/'Cargo.toml'; f.write_text(re.sub(r'(\[workspace.package\]\nversion = ")[^"]+', r'\g<1>'+a.version, f.read_text()))
f = r/'apps/sunna/tauri.conf.json'; d = json.loads(f.read_text()); d['version'] = a.version; f.write_text(json.dumps(d, indent=2)+'\n')
major, minor, patch = map(int,a.version.split('.')); assert minor < 1000 and patch < 1000
code = major*1000000 + minor*1000 + patch; assert 0 < code < 2100000000
f = r/'apps/android/app/build.gradle.kts'; s = re.sub(r'versionName = "[^"]+"', f'versionName = "{a.version}"', f.read_text()); s = re.sub(r'versionCode = \d+', f'versionCode = {code}', s); f.write_text(s)
