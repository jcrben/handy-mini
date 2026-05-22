---
title: Handy Network Jail
status: draft
updated: 2026-05-16
---

# Handy Network Jail

Local interception setup for [Handy](https://github.com/cjpais/Handy/) on this
machine. Purpose: catalog and constrain everything the Tauri build and the
running app reach out to over the network, so an audited build can be carried
to a locked-down Windows machine at work.

## Layout

```
handy/                       # upstream clone (cjpais/Handy)
network-jail/
  docker-compose.yml          # mitmproxy + coredns
  coredns/Corefile
  mitmproxy/
    addons/host_inventory.py  # deny-by-default + JSONL logger + phase tag
    allowlist-build.txt
    allowlist-run.txt
  build-jailed.bash           # wrapper for `bun install && bun tauri build`
  run-jailed.bash             # wrapper for the built binary
  docs/
    expected-egress.md        # hypothesis of what we expect to see
    usage.md
  runtime/                    # gitignored: mitm CA, observed-hosts log, flows
```

## Mode

**Observation-grade** for both build and run: egress is routed via
`HTTPS_PROXY` / `SSL_CERT_FILE` env vars, mitmproxy enforces the allowlist on
anything that respects those vars. A binary that ignores them and dials raw
sockets would escape unobserved.

Upgrade path to **strict containment** (deferred):

- Rootless podman with `--network=container:mitmproxy` (build and/or run
  share the proxy container's netns; only egress is the proxy).
- Or `bwrap --unshare-net` + `slirp4netns` for a tighter sandbox without
  needing podman.

## Quick start

```bash
# 1. cold start: bring jail up once so the CA is materialised
docker compose -f network-jail/docker-compose.yml up -d

# 2. build (build allowlist enforced)
network-jail/build-jailed.bash

# 3. run (run allowlist enforced)
network-jail/run-jailed.bash --release

# 4. inspect what was observed
cat network-jail/runtime/data/observed-hosts.jsonl | jq -c '{phase,kind,host,port,method,path}'

# 5. live web UI
xdg-open http://127.0.0.1:18081
```

## Allowlists

Two files, switched by env var at compose-up:

- `mitmproxy/allowlist-build.txt` - crates.io, github.com, npm registry, bun.sh
- `mitmproxy/allowlist-run.txt` - huggingface.co (model fetch), github.com
  (Tauri updater)

See [docs/expected-egress.md](docs/expected-egress.md) for the rationale.

Wildcard subdomains supported: `*.huggingface.co` matches `cdn-lfs.huggingface.co`.

## Related

- [docs/expected-egress.md](docs/expected-egress.md)
- [docs/usage.md](docs/usage.md)
