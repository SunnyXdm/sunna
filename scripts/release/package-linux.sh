#!/usr/bin/env bash
# Usage: package-linux.sh VERSION INPUT_DIR OUTPUT_DIR --repo-url URL --repo-key PUBLIC_KEY_FILE
set -euo pipefail
here=$(cd "$(dirname "$0")/packages" && pwd)
version=${1:?version}; input=$(realpath "${2:?tarball directory}"); out=$(realpath -m "${3:?output}")
[[ $# == 7 && $4 == --repo-url && $6 == --repo-key ]]
url=${5%/}; key=$(realpath "$7")
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && $url =~ ^https?://[a-zA-Z0-9./:_-]+$ ]]
# Reject secret key packets, including those mislabeled as public armor.
packets=$(gpg --batch --list-packets "$key")
[[ $packets == *':public key packet:'* && $packets != *':secret '* ]]
head -1 "$key" | grep -Fxq -- '-----BEGIN PGP PUBLIC KEY BLOCK-----'
mkdir -p "$out"
for fmt in deb rpm; do
  docker build -q -t "sunna-pkg-$fmt" -f "$here/Dockerfile.$fmt" "$here" >/dev/null
  docker run --rm --name "sunna-pkg-$fmt-$$" -v "$here:/tools:ro" -v "$key:/public.asc:ro" \
    -v "$input:/input:ro" -v "$out:/out" "sunna-pkg-$fmt" bash /tools/build.sh "$fmt" "$version" "$url"
done
