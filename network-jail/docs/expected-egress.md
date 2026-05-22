---
title: Expected Egress
status: draft
updated: 2026-05-16
---

# Expected Network Egress for Handy

What we expect Handy to call out to, separated into **build-time** and **runtime**.

This is a hypothesis from reading the upstream `Cargo.toml`, `package.json`, and
`tauri.conf.json`. The point of the jail is to **verify** it against reality, not
to trust it. Anything observed but not listed below is a finding.

## Tools and version pinning

- bun (TypeScript / Vite frontend) — pinned via `package.json#packageManager` if present, otherwise system bun
- cargo (Rust backend) — currently sourced from nix-portable on this host; CA trust env vars may behave differently than rustup-installed cargo
- git (used by cargo for git-source dependencies)

## Build-time egress

The Tauri build pulls from two ecosystems in parallel: Rust (cargo) and
JavaScript (bun). All deps come over HTTPS — no proprietary registries.

### Rust / cargo

| Host | Why | Notes |
|---|---|---|
| `index.crates.io` | crate index (sparse HTTP) | Default since cargo 1.70 |
| `static.crates.io` | crate tarball downloads | Bulk of bytes |
| `crates.io` | metadata / fallback | |
| `github.com` | git-source dependencies | See git deps below |
| `codeload.github.com` | git tarball fetches | git-over-https mechanism |
| `objects.githubusercontent.com` | LFS objects, release assets | |
| `raw.githubusercontent.com` | unlikely but possible (build.rs fetches) | flag if seen |
| `cdn.pyke.io` | ort-rs prebuilt ONNX Runtime tarball | Added after first jailed build flagged it. Path: `/0/pyke:ort-rs/ms@1.24.2/x86_64-unknown-linux-gnu.tar.lzma2`. Transitively required by transcribe-rs (Parakeet / Moonshine). |

**Cargo git dependencies (all GitHub):**

- `rustdesk-org/rdev` — patched keyboard/mouse hook fork
- `cjpais/vad-rs` — author's VAD fork
- `cjpais/rodio.git` — author's audio fork
- `ahkohd/tauri-nspanel` (branch `v2.1`) — macOS only, won't fetch on Linux
- `cjpais/tauri.git` (branch `handy-2.10.2`) — patched tauri-runtime / tauri-runtime-wry / tauri-utils

Each of these will produce a TLS connection to `github.com` and an
`objects.githubusercontent.com` fetch.

### JavaScript / bun

| Host | Why |
|---|---|
| `registry.npmjs.org` | npm registry |
| `bun.sh` | if bun is not preinstalled, the install script pulls from here |
| `github.com` | bun's binary release source (`oven-sh/bun`) |

Possible secondaries (some Vite plugins / Tailwind 4 pull from CDNs at build):

- `esm.sh`, `cdn.jsdelivr.net`, `unpkg.com` — flag if observed
- `fonts.googleapis.com` / `fonts.gstatic.com` — flag if observed (would
  indicate a runtime font fetch baked into bundle)

### Tauri build CLI

- `tauri-apps/cli` is fetched via crates.io as `tauri-cli` binary, *or* via npm
  as `@tauri-apps/cli` (we'll see which path Handy uses).
- On Linux, `cargo-tauri` may invoke `linuxdeploy` / `appimagetool`. These are
  downloaded as Github release assets by `cargo-tauri` on first build. Flag if
  observed:
  - `github.com/linuxdeploy/linuxdeploy/releases/...`
  - `github.com/AppImage/appimagetool/releases/...`

### Build-time hosts NOT expected

- HuggingFace — should not be hit at build. Model files are downloaded at
  **runtime**, not bundled. If we see HF traffic during build, that's a
  surprise.
- Telemetry endpoints (Sentry, PostHog, Plausible, Google Analytics) — none
  declared in deps.

## Runtime egress

Once installed and running, Handy is supposed to be local-first. Per the README:
"Your voice stays on your computer." So the runtime egress should be small
and bounded.

| Host | Why | When |
|---|---|---|
| `blob.handy.computer` | Whisper / Parakeet / Moonshine / etc. model downloads (author's CDN, not HuggingFace) | First-run + when switching models |
| `github.com` | Updater check — hits `cjpais/Handy/releases/latest/download/latest.json` | On startup (configurable) |
| `objects.githubusercontent.com` | Updater download of the installer if an update is found | Only when user accepts update |

### Model catalog (from `src-tauri/src/managers/model.rs`)

All URLs are `https://blob.handy.computer/<filename>`.

| Filename | Size | Notes |
|---|---:|---|
| `ggml-small.bin` | 465 MB | Whisper-small, **bundled by default** (`bundled_models` includes this) |
| `whisper-medium-q4_1.bin` | 469 MB | quantized medium |
| `ggml-large-v3-q5_0.bin` | 1031 MB | quantized large-v3 |
| `ggml-large-v3-turbo.bin` | 1549 MB | large-v3-turbo |
| `breeze-asr-q5_k.bin` | 1030 MB | Mandarin/Taiwanese |
| `parakeet-v2-int8.tar.gz` | 451 MB | Parakeet TDT 0.6B v2 |
| `parakeet-v3-int8.tar.gz` | 456 MB | Parakeet TDT 0.6B v3 |
| `moonshine-base.tar.gz` | 55 MB | |
| `moonshine-tiny-streaming-en.tar.gz` | 31 MB | smallest English option |
| `moonshine-small-streaming-en.tar.gz` | 99 MB | |
| `moonshine-medium-streaming-en.tar.gz` | 192 MB | |
| `sense-voice-int8.tar.gz` | 152 MB | |
| `giga-am-v3-int8.tar.gz` | 151 MB | |
| `canary-180m-flash.tar.gz` | 146 MB | |
| `canary-1b-v2.tar.gz` | 691 MB | |
| `cohere-int8.tar.gz` | 1708 MB | largest |

### Updater details

From `tauri.conf.json`:

- Endpoint: `https://github.com/cjpais/Handy/releases/latest/download/latest.json`
- Signature verified with the embedded minisign public key.
- README says updater is **disabled on Windows** — Linux/macOS only. So on
  Linux this hits GitHub on every launch.

### Runtime hosts NOT expected

- Any analytics — README says "Opt-in Analytics" is in progress and not
  default-on in current versions. Flag any analytics traffic as a finding.
- Audio CDNs, font CDNs, etc. — the UI is bundled, fonts should be embedded.
- DNS to anything outside the above hosts.

## What the jail enforces

- Outbound HTTP/HTTPS must traverse `mitmproxy` (deny-by-default).
- Allowlist is two files:
  - `mitmproxy/allowlist-build.txt` — hosts permitted while building
  - `mitmproxy/allowlist-run.txt` — hosts permitted while running
- Build and run use different wrappers so the allowlist scope matches the
  phase. Build hosts (crates.io, npm) MUST NOT be reachable at runtime.

## What the jail does not prove

- A native binary that bypasses HTTPS\_PROXY env vars and opens raw sockets
  would still escape the build-time wrapper unless we additionally confine
  the build to a netns. Runtime is confined by sandbox; build is currently
  observation-grade.
- Non-HTTP protocols (raw TCP, UDP, custom) — would appear in DNS log via
  CoreDNS but otherwise pass through.
- TLS-pinned clients — mitmproxy can't decrypt, but it can still see the SNI
  hostname and deny based on it.

## Next-step verification plan

1. Bring up the jail (`docker compose up -d`).
2. Run `build-jailed.bash` against a fresh Handy clone. Capture
   `runtime/data/observed-hosts.jsonl`.
3. Compare observed hosts against `allowlist-build.txt`. Update either the
   allowlist (if a host is legitimate) or this doc (if a host was unexpected).
4. Run the built binary via `run-jailed.bash`. Drive it: first-run model
   download, then a few transcriptions, then idle for a minute (to catch
   periodic phone-home).
5. Compare runtime observations to `allowlist-run.txt`. Same loop.
6. After the catalog is stable, freeze the allowlists.
