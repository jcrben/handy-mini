#!/usr/bin/env bash
# build-jailed.bash
#
# Run a Handy build with all egress forced through the local mitmproxy jail.
# Observation-grade: any tool that respects HTTPS_PROXY / SSL_CERT_FILE will be
# routed through mitmproxy with the build allowlist enforced. A tool that
# ignores those env vars and dials raw sockets would escape unobserved -
# escalate to strict-mode (podman + shared netns) if that becomes a concern.
#
# Usage:
#   ./build-jailed.bash                  # full build (bun install + bun tauri build)
#   ./build-jailed.bash install          # bun install only
#   ./build-jailed.bash dev              # bun tauri dev (long-running, ctrl+c to stop)
#   ./build-jailed.bash -- <any cmd>     # run arbitrary cmd inside jailed env

set -euo pipefail

jail_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
handy_src="${HANDY_SRC:-$jail_dir/../handy}"
ca_file="$jail_dir/runtime/mitmproxy/mitmproxy-ca-cert.pem"

if [[ ! -d "$handy_src/src-tauri" ]]; then
  echo "error: handy source tree not found at $handy_src" >&2
  echo "set HANDY_SRC=... or clone into ~/handy/handy/" >&2
  exit 1
fi

echo "==> bringing up mitmproxy + coredns in BUILD phase"
(
  cd "$jail_dir"
  NETWORK_JAIL_PHASE=build \
  NETWORK_JAIL_ALLOWLIST_FILE=/addons/allowlist-build.txt \
    docker compose up -d --force-recreate mitmproxy coredns
)

echo "==> waiting for mitmproxy CA bundle to appear ($ca_file)"
for _ in $(seq 1 30); do
  [[ -s "$ca_file" ]] && break
  sleep 1
done
if [[ ! -s "$ca_file" ]]; then
  echo "error: mitmproxy CA never materialised" >&2
  echo "       check 'docker compose -f $jail_dir/docker-compose.yml logs mitmproxy'" >&2
  exit 1
fi

echo "==> exporting proxy + CA env"
export HTTPS_PROXY="http://127.0.0.1:18080"
export HTTP_PROXY="http://127.0.0.1:18080"
export ALL_PROXY="http://127.0.0.1:18080"
export NO_PROXY="localhost,127.0.0.1,::1"
export SSL_CERT_FILE="$ca_file"
export NODE_EXTRA_CA_CERTS="$ca_file"
export CARGO_HTTP_CAINFO="$ca_file"
export GIT_SSL_CAINFO="$ca_file"
export REQUESTS_CA_BUNDLE="$ca_file"
# Some Rust crates fetch via curl-sys; point them at the CA too.
export CURL_CA_BUNDLE="$ca_file"

cd "$handy_src"

case "${1:-build}" in
  install)  set -- bun install ;;
  dev)      set -- bun tauri dev ;;
  build)    set -- bash -c 'bun install && bun run tauri build' ;;
  --)       shift ;;
  *)        ;;
esac

echo "==> running: $* (cwd=$PWD)"
echo "    proxy: $HTTPS_PROXY  CA: $SSL_CERT_FILE"
exec "$@"
