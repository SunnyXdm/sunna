#!/usr/bin/env bash
# Container entry point. Each component has a private ELF search directory.
set -euo pipefail
fmt=$1; version=$2; url=$3
for component in app host; do
  pkg=sunna
  [[ $component == app ]] || pkg=sunna-host
  mkdir -p "/build/$pkg/input"
  cd "/build/$pkg"
  tar xf "/input/sunna-$component-linux-x86_64.tar.gz" -C input
  root=$PWD/root
  tree=$root/usr/lib/$pkg
  doc=$root/usr/share/doc/$pkg
  mkdir -p "$tree/bin" "$doc" "$root/usr/bin"
  cp -r input/share/sunna/ffmpeg "$doc/"
  install -m644 /public.asc "$tree/repo-key.asc"
  cat > "$doc/copyright" <<COPY
Sunna
Copyright (c) 2026 Sunna contributors. All rights reserved.
Homepage: https://sunna.sunnydx.dev

FFmpeg is copyright the FFmpeg developers and distributed under the GNU
Lesser General Public License, version 2.1 or later. The matching source
and full license are in /usr/share/doc/$pkg/ffmpeg/.
COPY
  mkdir "$tree/lib"
  # The release includes two identical copies of each DSO. Keep its SONAME.
  cp input/lib/libav*.so.[0-9][0-9] "$tree/lib/"
  if [[ $fmt == deb ]]; then chmod 644 "$tree"/lib/*; fi
  if [[ $component == app ]]; then
    summary='reaches your other computers'
    description='Connect to a computer running the Sunna host to see and control its desktop.'
    weak='Recommends: sunna-host'
    cp input/bin/sunna-real "$tree/bin/sunna"
    ln -s ../lib/sunna/bin/sunna "$root/usr/bin/sunna"
    desktop=$root/usr/share/applications/dev.sunna.app.desktop
    mkdir -p "$(dirname "$desktop")"
    cat > "$desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Sunna
GenericName=Remote Desktop
Comment=Use your other computers from this one
Exec=sunna
Icon=dev.sunna.app
Terminal=false
Categories=Network;RemoteAccess;
Keywords=remote;desktop;vnc;screen;
DESKTOP
    icons=$root/usr/share/icons/hicolor
    for pair in scalable:icon.svg 128x128:128x128.png 256x256:128x128@2x.png; do
      file=${pair#*:}
      install -Dm644 "input/share/sunna/icons/$file" "$icons/${pair%:*}/apps/dev.sunna.app.${file##*.}"
    done
  else
    summary='shares this computer with the Sunna app'
    description='Share a desktop and accept keyboard and mouse input from a connected Sunna app.'
    cp input/bin/{sunnad,sunna-host} "$tree/bin/"
    mkdir -p "$tree/share/sunna"
    cp input/share/sunna/*.sh "$tree/share/sunna/"
    sed -i '1s|^#!/usr/bin/env bash$|#!/bin/bash|' "$tree/bin/sunna-host" "$tree"/share/sunna/*.sh
    cp "$tree/bin/sunna-host" "$root/usr/bin/sunna-host"
    ln -s ../lib/sunna-host/bin/sunnad "$root/usr/bin/sunnad"
    ln -s ../lib/sunna-host/share/sunna "$root/usr/share/sunna"
    weak='Suggests: xvfb, x11-utils, dbus-x11, xfce4, pulseaudio-utils'
  fi
  if [[ $fmt == deb ]]; then
    source=/etc/apt/sources.list.d/sunna.sources; key=/etc/apt/keyrings/sunna.asc
    # dpkg retains the last configured version after remove (until purge).
    # This receipt tracks a continuous installation across configure retries.
    first="[ ! -f /var/lib/sunna-packages/$pkg ]"; guard='[ "$1" = configure ] || exit 0'
    remove='case "$1" in remove|purge) ;; *) exit 0;; esac'
    config="Types: deb
URIs: $url/apt
Suites: stable
Components: main
Architectures: amd64
Signed-By: $key"
  else
    source=/etc/yum.repos.d/sunna.repo; key=/etc/pki/rpm-gpg/sunna.asc
    first='[ "$1" = 1 ]'; guard=:; remove='[ "$1" = 0 ] || exit 0'
    config="[sunna]
name=Sunna
baseurl=$url/rpm
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=file://$key"
  fi
  notice=:
  [[ $component != host ]] || notice='echo "The new host is used after sunna-host restart."'
  echo "$config" > config
  sed -e "s|@KEY@|$key|g;s|@SOURCE@|$source|g;s|@PACKAGE@|$pkg|g;s|@FIRST@|$first|;s#@GUARD@#$guard#;s|@NOTICE@|$notice|" \
    -e '/@CONFIG@/r config' -e '/@CONFIG@/d' /tools/install-source > post
  sed -e "s#@GUARD@#$remove#;s|@KEY@|$key|g;s|@SOURCE@|$source|g" /tools/remove-source > postrm
  if [[ $fmt == deb ]]; then
    cat > changelog <<CHANGELOG
$pkg ($version) stable; urgency=medium

  * Package the upstream Linux release.

 -- Sunna <sunna@sunnydx.dev>  $(date -R)
CHANGELOG
    gzip -9nc changelog > "$doc/changelog.gz"
    # Only these upstream artifacts are intentionally shipped verbatim.
    overrides=$root/usr/share/lintian/overrides
    mkdir -p "$overrides"
    cat > "$overrides/$pkg" <<OVERRIDES
$pkg: embedded-library libavutil usr/lib/$pkg/lib/libavutil.so.*
$pkg: unstripped-binary-or-object usr/lib/$pkg/bin/*
OVERRIDES
    mkdir -p debian "$root/DEBIAN"
    touch debian/control
    for lib in "$tree"/lib/*; do
      soname=${lib##*/}
      echo "${soname%.so.*} ${soname##*.} $pkg"
    done > debian/shlibs.local
    deps=$(dpkg-shlibdeps -O -x"$pkg" -l"$tree/lib" "$tree/bin/$( [[ $component == app ]] && echo sunna || echo sunnad )" "$tree"/lib/*)
    # apt needs the system CA bundle to fetch the HTTPS update source.
    deps="${deps#shlibs:Depends=}, ca-certificates"
    [[ $component != host ]] || deps+=', python3, systemd, iproute2'
    cat > "$root/DEBIAN/control" <<CONTROL
Package: $pkg
Version: $version
Architecture: amd64
Maintainer: Sunna <sunna@sunnydx.dev>
Homepage: https://sunna.sunnydx.dev
Section: net
Priority: optional
Installed-Size: $(du -sk "$root/usr" | cut -f1)
Depends: $deps
$weak
Description: $summary
 $description
CONTROL
    echo "install -d /var/lib/sunna-packages && touch /var/lib/sunna-packages/$pkg" >> post
    cat >> postrm <<REMOVE
rm -f /var/lib/sunna-packages/$pkg
if [ -d /var/lib/sunna-packages ] && [ -z "\$(ls -A /var/lib/sunna-packages)" ]; then
  rmdir /var/lib/sunna-packages
fi
REMOVE
    install -m755 post "$root/DEBIAN/postinst"
    install -m755 postrm "$root/DEBIAN/postrm"
    dpkg-deb -Zxz -z3 --build --root-owner-group "$root" "/out/${pkg}_${version}_amd64.deb"
  else
    if [[ $component == host ]]; then
      weak=$'Requires: python3, systemd, iproute\nSuggests: xorg-x11-server-Xvfb, xprop, dbus-x11, xfce4-session, pulseaudio-utils'
    fi
    # The upstream source archive retains its original license notice. Ship
    # the canonical LGPL text with current FSF contact details beside it.
    cp /usr/local/share/LGPL-2.1 "$doc/ffmpeg/COPYING.LGPLv2.1"
    cat > package.spec <<SPEC
%global _build_id_links none
%global debug_package %{nil}
%global __os_install_post %{nil}
%global __requires_exclude ^libav(codec|util)\\\\.so
%global __provides_exclude ^libav(codec|util)\\\\.so
Name: $pkg
Version: $version
Release: 1
Summary: ${summary^}
License: LicenseRef-Sunna AND LGPL-2.1-or-later
URL: https://sunna.sunnydx.dev
Packager: Sunna <sunna@sunnydx.dev>
$weak
%description
$description
%install
mkdir -p %{buildroot}
cp -a $root/. %{buildroot}/
%post -f $PWD/post
%postun -f $PWD/postrm
%files
/usr/lib/$pkg
/usr/bin/*
/usr/share/doc/$pkg
SPEC
    if [[ $component == host ]]; then echo /usr/share/sunna; else
      echo /usr/share/applications/dev.sunna.app.desktop
      echo '/usr/share/icons/hicolor/*/apps/dev.sunna.app.*'
    fi >> package.spec
    cat >> package.spec <<CHANGELOG
%changelog
* $(LC_ALL=C date '+%a %b %d %Y') Sunna <sunna@sunnydx.dev> - $version-1
- Package the upstream Linux release.
CHANGELOG
    rpmbuild -bb package.spec
    cp "/root/rpmbuild/RPMS/x86_64/$pkg-$version-1.x86_64.rpm" /out/
  fi
done
