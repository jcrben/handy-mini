---
title: Usage
status: draft
updated: 2026-05-16
---

# Usage

## Bring the jail up

```bash
cd ~/handy/network-jail
docker compose up -d
```

Wait a few seconds; the mitmproxy CA will appear at
`runtime/mitmproxy/mitmproxy-ca-cert.pem`. The wrappers below wait for this
automatically.

## Build

```bash
~/handy/network-jail/build-jailed.bash
```

This:

1. Forces compose to recreate `mitmproxy` with `NETWORK_JAIL_PHASE=build` and
   the build allowlist.
2. Exports `HTTPS_PROXY`, `HTTP_PROXY`, `SSL_CERT_FILE`, `CARGO_HTTP_CAINFO`,
   `NODE_EXTRA_CA_CERTS`, `GIT_SSL_CAINFO`, `REQUESTS_CA_BUNDLE`,
   `CURL_CA_BUNDLE` all pointing at the mitmproxy CA.
3. Runs `bun install && bun run tauri build` in `~/handy/handy/`.

Variants:

- `build-jailed.bash install` - just `bun install`
- `build-jailed.bash dev` - `bun tauri dev`
- `build-jailed.bash -- <cmd ...>` - run an arbitrary command in the jailed env

### Prerequisites on the host

- `bun` on `PATH` (install once: `curl -fsSL https://bun.sh/install | bash` -
  ideally do this *outside* the jail since the jail will block bun.sh except
  during build, and bun's installer doesn't honour `CARGO_HTTP_CAINFO`).
- `cargo` on `PATH` (any source: rustup, nix, system pkg).
- Linux system deps per upstream `BUILD.md`:
  - Fedora: `sudo dnf install alsa-lib-devel pkgconf openssl-devel
    vulkan-devel gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel
    librsvg2-devel gtk-layer-shell gtk-layer-shell-devel cmake`

## Run

```bash
~/handy/network-jail/run-jailed.bash --release
```

Switches the jail into RUN phase (different allowlist), then exec's the built
binary with proxy + CA env vars.

Driving the app for a complete trace:

1. First launch: triggers the model download. Expect
   `huggingface.co` + `cdn-lfs.huggingface.co` in the log.
2. Idle for 30s: catches the Tauri updater hit to
   `github.com/cjpais/Handy/releases/latest/download/latest.json`.
3. Do a transcription: should be **silent on the network**. Any egress here
   is a finding.
4. Switch models in settings (if applicable): another HF download.

## Inspect

- Live web UI: <http://127.0.0.1:18081>
- JSONL log: `runtime/data/observed-hosts.jsonl`
- Flow capture: `runtime/data/flows.mitm` (open with `mitmweb`)

Useful jq one-liners:

```bash
# unique hosts seen so far
jq -r '.host' runtime/data/observed-hosts.jsonl | sort -u

# only blocked attempts
jq -c 'select(.kind == "http_request_blocked")' runtime/data/observed-hosts.jsonl

# split by phase
jq -c 'select(.phase == "run")'   runtime/data/observed-hosts.jsonl
jq -c 'select(.phase == "build")' runtime/data/observed-hosts.jsonl
```

## Tear down

```bash
docker compose -f ~/handy/network-jail/docker-compose.yml down
```

The CA file and observed-hosts log persist under `runtime/` until you
`rm -rf runtime/`. Keep them - they are the evidence trail.
