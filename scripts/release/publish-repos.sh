#!/usr/bin/env bash
# Usage: SUNNA_REPO_GNUPGHOME=... SUNNA_REPO_KEY_ID=... publish-repos.sh PACKAGES_DIR WEB_ROOT
set -euo pipefail
umask 022
packages=$(realpath "${1:?package directory}"); web=$(realpath -m "${2:?web root}")
export GNUPGHOME=${SUNNA_REPO_GNUPGHOME:?signing key home}
kid=${SUNNA_REPO_KEY_ID:?signing fingerprint}
here=$(cd "$(dirname "$0")/packages" && pwd)
mkdir -p "$web/apt/pool/main/s/sunna" "$web/apt/dists/stable" "$web/rpm/repodata"
exec 9<"$web"
flock -x 9
stage=$(mktemp -d "$web/.sunna-publish.XXXXXX")
trap 'find "$stage" -depth -delete' EXIT
rpm=$stage/rpm; apt=$stage/apt
mkdir -p "$rpm" "$apt/dists/stable/main/binary-amd64"
ln -s "$web/apt/pool" "$apt/pool"
# Signing agent stays on the host. Only its restricted socket and public certificate
# enter the signing container. Mount the stage at the same path for apt's pool link.
gpg --batch --armor --export "$kid" > "$stage/public.gpg"
test -s "$stage/public.gpg"
gpgconf --launch gpg-agent
socket=$(gpgconf --list-dirs agent-extra-socket)
for fmt in deb rpm; do
  docker build -q -t "sunna-pkg-$fmt" -f "$here/Dockerfile.$fmt" "$here" >/dev/null
done
run() {
  local fmt=$1; shift
  docker run --rm --name "sunna-pkg-publish-$fmt-$$" --user "$(id -u):$(id -g)" \
    -e HOME=/work/home -e GNUPGHOME=/work/home/gpg -e kid="$kid" \
    -v "$socket:/agent:ro" -v "$web:$web:ro" -v "$stage:/work" \
    -v "$here:/tools:ro" "sunna-pkg-$fmt" "$@"
}
for deb in "$packages"/sunna*.deb; do
  [[ -f $deb ]] || { echo "No Sunna .deb packages in $packages" >&2; exit 1; }
  dest=$web/apt/pool/main/s/sunna/${deb##*/}
  if [[ -e $dest ]]; then cmp "$deb" "$dest"; else
    cp "$deb" "$stage/package.deb"; mv "$stage/package.deb" "$dest"
  fi
done
cp "$packages"/sunna-*.rpm "$rpm/"
mkdir -p -m700 "$stage/home/gpg"
ln -s /agent "$stage/home/gpg/S.gpg-agent"
run rpm bash -ec '
  gpg --batch --import /work/public.gpg
  rpmkeys --import /work/public.gpg --dbpath /work/home/rpmdb
  for f in /work/rpm/*.rpm; do
    rpmsign --define "_gpg_name $kid" --addsign "$f"
    rpmkeys --checksig --verbose --dbpath /work/home/rpmdb "$f" | tee /work/checksig
    grep -Ei "signature.*: OK" /work/checksig
  done'
for file in "$rpm"/*.rpm; do
  dest=$web/rpm/${file##*/}
  if [[ -e $dest ]]; then
    # An existing version is immutable. Verify both its signature and content.
    run rpm bash -ec '
      rpmkeys --dbpath /work/home/rpmdb --checksig --verbose "$2" > /work/existing-signature
      grep -Ei "signature.*: OK" /work/existing-signature
      header() { rpm --dbpath /work/home/rpmdb -qp --qf "%{SHA256HEADER}" "$1"; }
      digest=$(header "$1")
      [[ $digest != "(none)" && $digest == "$(header "$2")" ]]
    ' sh "/work/rpm/${file##*/}" "$dest"
  else
    mv "$file" "$dest"
  fi
done
run rpm createrepo_c --checksum sha256 --outputdir /work "$web/rpm"
stable=$apt/dists/stable
index=$stable/main/binary-amd64
run deb bash -ec 'cd /work/apt; apt-ftparchive packages pool' > "$index/Packages"
gzip -nc "$index/Packages" > "$index/Packages.gz"
xz -c "$index/Packages" > "$index/Packages.xz"
run deb apt-ftparchive -o APT::FTPArchive::Release::Suite=stable -o APT::FTPArchive::Release::Codename=stable \
  -o APT::FTPArchive::Release::Architectures=amd64 -o APT::FTPArchive::Release::Components=main \
  -o APT::FTPArchive::Release::Acquire-By-Hash=yes release /work/apt/dists/stable > "$stage/Release"
mv "$stage/Release" "$stable/"
sign=(gpg --batch --local-user "$kid")
"${sign[@]}" --clearsign -o "$stable/InRelease" "$stable/Release"
"${sign[@]}" --armor --detach-sign -o "$stable/Release.gpg" "$stable/Release"
"${sign[@]}" --armor --detach-sign -o "$stage/repodata/repomd.xml.asc" "$stage/repodata/repomd.xml"
# Keep immutable indexes for clients still using the previous signed metadata.
dest=$web/apt/dists/stable/main/binary-amd64
for algo in SHA256 SHA512; do
  mkdir -p "$dest/by-hash/$algo"
  for file in "$index"/Packages*; do
    digest=$("${algo,,}sum" "$file" | cut -d' ' -f1)
    ln -f "$file" "$dest/by-hash/$algo/$digest"
  done
done
mv -f "$index"/Packages* "$dest/"
mv -f "$stable"/{Release.gpg,Release,InRelease} "$web/apt/dists/stable/"
for file in "$stage/repodata"/*; do
  [[ ${file##*/} == repomd.xml* ]] || mv -f "$file" "$web/rpm/repodata/"
done
mv -f "$stage/repodata"/{repomd.xml.asc,repomd.xml} "$web/rpm/repodata/"
