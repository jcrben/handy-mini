#!/usr/bin/env bash
# run-in-distrobox.bash
#
# Run the built Handy binary inside the same distrobox we built it in, with
# the host-side mitmproxy network-jail in front (RUN phase + run allowlist).
# Audio (pulse/pipewire), X11/Wayland and the host network are shared by
# distrobox; only HTTPS_PROXY routing is constrained here.
#
# For first-pass verification we only care about the egress catalog. If
# /dev/uinput or hotkey grab fail, that's a separate workstream - the app
# still boots and we can observe what it tries to dial out to.
#
# Usage:
#   ./run-in-distrobox.bash                       # --release
#   ./run-in-distrobox.bash --debug
#   ./run-in-distrobox.bash --bin /path/to/handy

set -euo pipefail

jail_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
handy_src="${HANDY_SRC:-$jail_dir/../handy}"
ca_file="$jail_dir/runtime/mitmproxy/mitmproxy-ca-cert.pem"
box="${HANDY_BUILD_BOX:-handy-build}"

mode="release"
bin=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) mode="release"; shift ;;
    --debug)   mode="debug"; shift ;;
    --bin)     bin="$2"; shift 2 ;;
    *)         break ;;
  esac
done

if [[ -z "$bin" ]]; then
  bin="$handy_src/src-tauri/target/$mode/handy"
fi

if [[ ! -x "$bin" ]]; then
  echo "error: handy binary not found or not executable: $bin" >&2
  echo "       build first with: $jail_dir/build-in-distrobox.bash" >&2
  exit 1
fi

echo "==> bringing up mitmproxy + coredns in RUN phase (host side)"
(
  cd "$jail_dir"
  NETWORK_JAIL_PHASE=run \
  NETWORK_JAIL_ALLOWLIST_FILE=/addons/allowlist-run.txt \
    docker compose up -d --force-recreate mitmproxy coredns
)

echo "==> waiting for mitmproxy CA bundle"
for _ in $(seq 1 30); do
  [[ -s "$ca_file" ]] && break
  sleep 1
done
if [[ ! -s "$ca_file" ]]; then
  echo "error: mitmproxy CA never materialised" >&2
  exit 1
fi

echo "==> entering '$box' to launch: $bin $*"
exec distrobox enter "$box" -- bash -lc "
set -euo pipefail

export HTTPS_PROXY='http://127.0.0.1:18080'
export HTTP_PROXY='http://127.0.0.1:18080'
export ALL_PROXY='http://127.0.0.1:18080'
export NO_PROXY='localhost,127.0.0.1,::1'
export SSL_CERT_FILE='$ca_file'
export CURL_CA_BUNDLE='$ca_file'
export REQUESTS_CA_BUNDLE='$ca_file'

echo '==> in-box: proxy='\$HTTPS_PROXY' ca='\$SSL_CERT_FILE
echo '==> in-box: launching:' '$bin' $*
exec '$bin' $*
"
