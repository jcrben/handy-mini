#!/usr/bin/env bash
# build-in-distrobox.bash
#
# Run the Handy build inside a distrobox (where webkit2gtk + rustup + the
# rest of the Tauri Linux build deps are installed), with the host-side
# mitmproxy network-jail in front.
#
# Aurora is rpm-ostree (immutable): we can't apt/dnf install Tauri's deps on
# the host. The box has them. Mitmproxy on host's 127.0.0.1:18080 is
# reachable from inside the box because distrobox shares the host's network
# namespace by default.
#
# Usage:
#   ./build-in-distrobox.bash                # full bun install + bun tauri build
#   ./build-in-distrobox.bash install        # bun install only
#   ./build-in-distrobox.bash dev            # bun tauri dev
#   ./build-in-distrobox.bash -- <cmd ...>   # arbitrary command in jailed env
#
# Env:
#   HANDY_BUILD_BOX  override distrobox name (default: handy-build)
#   HANDY_SRC        override Handy source dir (default: ../handy)
#   BOX_BUN          path to bun binary visible inside box (default: host mise install)
#   BOX_CARGO_BIN    cargo bin dir inside box (default: ~/.cargo/bin)

set -euo pipefail

jail_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
handy_src="${HANDY_SRC:-$jail_dir/../handy}"
ca_file="$jail_dir/runtime/mitmproxy/mitmproxy-ca-cert.pem"
box="${HANDY_BUILD_BOX:-handy-build}"
box_bun="${BOX_BUN:-/home/ben/dotfiles/local/xdgdata/mise/installs/bun/1.3.14/bin/bun}"
box_cargo_bin="${BOX_CARGO_BIN:-$HOME/.cargo/bin}"

if [[ ! -d "$handy_src/src-tauri" ]]; then
  echo "error: handy source tree not found at $handy_src" >&2
  exit 1
fi

if ! distrobox list 2>/dev/null | grep -qE "\\b$box\\b"; then
  echo "error: distrobox '$box' does not exist" >&2
  echo "       create with: distrobox create --name $box --image docker.io/library/ubuntu:24.04 --yes" >&2
  exit 1
fi

echo "==> bringing up mitmproxy + coredns in BUILD phase (host side)"
(
  cd "$jail_dir"
  NETWORK_JAIL_PHASE=build \
  NETWORK_JAIL_ALLOWLIST_FILE=/addons/allowlist-build.txt \
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

case "${1:-build}" in
  install)  inner_cmd='bun install' ;;
  dev)      inner_cmd='bun tauri dev' ;;
  build)    inner_cmd='bun install && bun run tauri build' ;;
  --)       shift; inner_cmd="$*" ;;
  *)        inner_cmd="$*" ;;
esac

echo "==> entering '$box' to run: $inner_cmd"
exec distrobox enter "$box" -- bash -lc "
set -euo pipefail

# Prepend real toolchain dirs so we don't pick up nix-portable cargo wrapper.
export PATH='$box_cargo_bin':\"\$(dirname '$box_bun')\":\$PATH

# Proxy + CA env: everything that respects HTTPS_PROXY / SSL_CERT_FILE.
export HTTPS_PROXY='http://127.0.0.1:18080'
export HTTP_PROXY='http://127.0.0.1:18080'
export ALL_PROXY='http://127.0.0.1:18080'
export NO_PROXY='localhost,127.0.0.1,::1'
export SSL_CERT_FILE='$ca_file'
export NODE_EXTRA_CA_CERTS='$ca_file'
export CARGO_HTTP_CAINFO='$ca_file'
export GIT_SSL_CAINFO='$ca_file'
export REQUESTS_CA_BUNDLE='$ca_file'
export CURL_CA_BUNDLE='$ca_file'

cd '$handy_src'
echo '==> in-box: proxy='\$HTTPS_PROXY' ca='\$SSL_CERT_FILE
echo '==> in-box: bun='\$(command -v bun)' cargo='\$(command -v cargo)
echo '==> in-box: running:' '$inner_cmd'
$inner_cmd
"
