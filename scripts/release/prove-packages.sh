#!/usr/bin/env bash
# WEB_ROOT contains signed initial packages and a higher version (or use the hook
# below to publish it after all four initial installs). INPUT contains the tarballs.
# Usage: prove-packages.sh WEB_ROOT INPUT VERSION HIGHER_VERSION PUBLIC_KEY
set -euo pipefail
web=$(realpath "$1"); input=$(realpath "$2"); version=$3; higher=$4; key=$(realpath "$5")
port=${SUNNA_PROOF_PORT:-48992}
here=$(cd "$(dirname "$0")/packages" && pwd)
names=(); pids=()
cleanup() {
  for name in "${names[@]}"; do docker rm -f "$name" >/dev/null; done
}
trap cleanup EXIT
wait_all() {
  local failed=0
  for pid in "${pids[@]}"; do wait "$pid" || failed=1; done
  pids=()
  return "$failed"
}
for base in ubuntu:22.04 ubuntu:24.04 debian:13 fedora:42; do
  name=sunna-pkg-proof-${base/:/-}-$$
  docker run -d --name "$name" -e version="$version" -e higher="$higher" \
    -e proofurl="http://127.0.0.1:$port" -v "$web:/web:ro" -v "$key:/public.asc:ro" \
    -v "$input:/input:ro" -v "$here:/tools:ro" "$base" sleep infinity >/dev/null
  names+=("$name")
  docker exec "$name" bash /tools/proof.sh install &
  pids+=("$!")
done
wait_all
for fmt in deb rpm; do
  docker build -q -t "sunna-pkg-$fmt" -f "$here/Dockerfile.$fmt" "$here" >/dev/null
done
docker run --rm --name "sunna-pkg-proof-lint-deb-$$" -v "$web:/web:ro" -e version="$version" \
  sunna-pkg-deb sh -ec 'lintian --allow-root /web/apt/pool/main/s/sunna/*_"$version"_amd64.deb'
docker run --rm --name "sunna-pkg-proof-lint-rpm-$$" -v "$web:/web:ro" -v "$key:/public.asc:ro" -e version="$version" \
  sunna-pkg-rpm sh -ec 'rpmkeys --import /public.asc; rpmlint /web/rpm/*-"$version"-1.x86_64.rpm'
if [[ -n ${SUNNA_PROOF_PUBLISH:-} ]]; then "$SUNNA_PROOF_PUBLISH"; fi
for name in "${names[@]}"; do
  docker exec "$name" bash /tools/proof.sh upgrade &
  pids+=("$!")
done
wait_all
