#!/usr/bin/env bash
# run-jailed.bash
#
# Launch the built Handy binary with all egress forced through mitmproxy
# (RUN phase allowlist). Observation-grade: relies on reqwest/Tauri respecting
# HTTPS_PROXY env vars. Any rogue raw-socket egress would not be caught here
# - upgrade to strict-mode (rootless podman + --network=container:mitmproxy
# or bwrap + slirp4netns) if that becomes a concern.
#
# Usage:
#   ./run-jailed.bash                       # launch debug build
#   ./run-jailed.bash --release             # launch release build
#   ./run-jailed.bash --bin /path/to/handy  # explicit binary path
#
# Audio (pulse/pipewire) and X11/Wayland sockets are NOT sandboxed here -
# Handy needs them. Only network is constrained.

set -euo pipefail

jail_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
handy_src="${HANDY_SRC:-$jail_dir/../handy}"
ca_file="$jail_dir/runtime/mitmproxy/mitmproxy-ca-cert.pem"

mode="debug"
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
  echo "       build first with: $jail_dir/build-jailed.bash" >&2
  exit 1
fi

echo "==> bringing up mitmproxy + coredns in RUN phase"
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

echo "==> exporting proxy + CA env"
export HTTPS_PROXY="http://127.0.0.1:18080"
export HTTP_PROXY="http://127.0.0.1:18080"
export ALL_PROXY="http://127.0.0.1:18080"
export NO_PROXY="localhost,127.0.0.1,::1"
export SSL_CERT_FILE="$ca_file"
export CURL_CA_BUNDLE="$ca_file"
export REQUESTS_CA_BUNDLE="$ca_file"
# rustls picks up SSL_CERT_FILE via webpki-roots fallback only when the crate
# is built with the rustls-tls-native-roots feature. reqwest with default-tls
# uses native-tls (OpenSSL on Linux) which honours SSL_CERT_FILE.

echo "==> launching $bin"
echo "    proxy: $HTTPS_PROXY  CA: $SSL_CERT_FILE  observed-hosts: $jail_dir/runtime/data/observed-hosts.jsonl"
exec "$bin" "$@"
