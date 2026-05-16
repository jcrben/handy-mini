---
title: Handy-mini handoff
status: in-progress
updated: 2026-05-16
---

# Handy-mini — handoff state

This repo wraps [cjpais/Handy](https://github.com/cjpais/Handy/) with a network
jail and a plan to produce an audited, offline-capable build for use on a
locked-down Windows machine at work (no admin, shadow-IT scenario).

The upstream clone at `./handy/` is intentionally gitignored. It is a
shallow `git clone --depth=1 git@github.com:cjpais/Handy.git`. Re-clone if
missing.

## Goal

Produce one `.exe` that:

- bundles `parakeet-tdt-0.6b-v3-int8` (best English ASR currently, ~456 MB)
- never calls home at runtime (no `blob.handy.computer`, no Tauri updater)
- is signed/hashed by a build the user did personally on this machine
- can be carried into work via `git clone` (allowed) + GitHub Release
  attachment download (allowed)
- runs on Windows without admin privileges

## Decisions already made

| Choice | Picked |
|---|---|
| English model | **Parakeet TDT 0.6B v3 int8** (`parakeet-v3-int8.tar.gz`, 456 MB) |
| Build location | Home only, in the network jail |
| Build strategy | Strip eventually to a "handy-mini" CLI fork; for now build full Tauri build first to capture egress baseline |
| Repo visibility | Public |
| Build-time confinement mode | Observation-grade (HTTPS_PROXY + SSL_CERT_FILE env vars). Strict containment via rootless podman or `bwrap --unshare-net` + slirp4netns deferred. |
| Sandbox tool | bwrap (firejail not installed) |
| Transport to work | Plain GitHub repo (source) + GitHub Release attachments (binary). No Git LFS. |
| GitHub Actions for CI | **Not yet** — adds supply-chain risk (e.g. tj-actions/changed-files Mar-2025). If added later, pin every action to a full commit SHA, no third-party actions, no secrets. |

## What's done

- `network-jail/` directory mirrors the `~/thegoodapp/thegoodapp/network-jail/`
  pattern (mitmproxy + coredns + JSONL logger).
- Two allowlists: `mitmproxy/allowlist-build.txt` and
  `mitmproxy/allowlist-run.txt`. Wildcard subdomain support
  (`*.huggingface.co` etc.) added.
- `host_inventory.py` mitmproxy addon stamps every record with a `phase`
  field so build-time and run-time observations are distinguishable in the
  same JSONL log.
- `build-jailed.bash` — forces compose to re-up with `NETWORK_JAIL_PHASE=build`
  + build allowlist, exports all the right CA env vars
  (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `SSL_CERT_FILE`,
  `NODE_EXTRA_CA_CERTS`, `CARGO_HTTP_CAINFO`, `GIT_SSL_CAINFO`,
  `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`), then runs
  `bun install && bun run tauri build` in `~/handy/handy/`.
- `run-jailed.bash` — same idea but RUN phase + run allowlist, executes the
  built binary.
- `docs/expected-egress.md` — hypothesis catalog of what we expect to see
  (build-time: crates.io / npm / github.com; run-time: blob.handy.computer +
  github.com for updater).
- `docs/usage.md` — operator runbook.
- Upstream Handy cloned to `./handy/` (shallow).
- Source survey done:
  - Models hosted on `blob.handy.computer` (NOT HuggingFace). Full model
    catalog with sizes catalogued in `docs/expected-egress.md`.
  - Bundling mechanism at `src-tauri/src/managers/model.rs:651`
    (`migrate_bundled_models`). Currently hardcoded for single-file
    `["ggml-small.bin"]`. Needs a directory-aware variant for Parakeet
    (which is a tarball that extracts to a directory of `*.onnx` + vocab).
  - `tauri.conf.json` already has `"resources": ["resources/**/*"]` glob.

## What's next (do this in order)

1. **Install bun.** Aurora is rpm-ostree (immutable). Options:
   - `mise use -g bun@latest` (mise is the user's package manager; `bun`
     exists in mise registry as `core:bun`)
   - or `curl -fsSL https://bun.sh/install | bash` (installs to `~/.bun/`)
   - Do this **outside** the jail so build allowlist isn't polluted by bun
     installer noise. The build allowlist tolerates `bun.sh` and
     `github.com` anyway.

2. **Pull Parakeet v3 int8** from `https://blob.handy.computer/parakeet-v3-int8.tar.gz`.
   - Record SHA256 to `~/handy/model-source/parakeet-v3-int8.sha256`.
   - Extract under `~/handy/model-source/` (NOT yet into the Handy src tree).
   - Open question: do we ALSO want to cross-check against an NVIDIA
     official source? Their `nvidia/parakeet-tdt-0.6b-v3` HF repo ships
     `.nemo` (NeMo format), not int8 ONNX. Converting to int8 ONNX is the
     work blob.handy.computer is doing on our behalf. Two trust-axis
     options:
       - Accept the int8 ONNX from blob.handy.computer + smoke-eval to
         catch tampering (pragmatic).
       - Reproduce the int8 quantization locally from NVIDIA's `.nemo`
         (high-effort, gives full provenance).
     Pick before step 5.

3. **Patch `migrate_bundled_models`** at
   `~/handy/handy/src-tauri/src/managers/model.rs:651`. Two changes:
   - Add `parakeet-tdt-0.6b-v3-int8` to the bundled list.
   - Teach it to copy a directory recursively when the bundled path is a
     directory rather than a file.
   - Optional: disable the Tauri updater outright (we want zero
     post-install egress). Either remove `tauri-plugin-updater` from
     `Cargo.toml` or unset its endpoint in `tauri.conf.json`.

4. **Drop the model in** at
   `~/handy/handy/src-tauri/resources/models/parakeet-tdt-0.6b-v3-int8/`.

5. **First jailed build**: `~/handy/network-jail/build-jailed.bash`.
   Capture `~/handy/network-jail/runtime/data/observed-hosts.jsonl`. Diff
   observed hosts against `allowlist-build.txt`. Iterate until clean (no
   `http_request_blocked` records). Hosts not in the allowlist but
   legitimate → add to allowlist; hosts unexpected → investigate.

6. **First jailed run**: `~/handy/network-jail/run-jailed.bash --release`.
   Drive: first-launch, two transcriptions, idle 60s. Confirm zero
   `blob.handy.computer` hits (because model is bundled), and updater hit
   to `github.com` if not patched out in step 3.

7. **Smoke-eval Parakeet** on ~100 LibriSpeech-test-clean utterances.
   Compare WER to NVIDIA's published number (~5%). Catches gross
   replacement of weights. Doesn't catch a targeted backdoor.

8. **Publish**:
   - Push this repo to `github.com/jcrben/handy-mini` (public).
   - Cross-compile Windows build (Tauri cross-compile from Linux is
     possible but cargo-tauri prefers native MSVC build; alternative:
     spin up a Windows VM or use a GH Actions Windows runner one-shot).
   - Upload `.msi` / `.exe` + `sha256.txt` as a GitHub Release attachment.
   - Document work-side install in repo `INSTALL-AT-WORK.md`:
     - `git clone`
     - `curl -LO` release artifact
     - `Get-FileHash` to verify
     - Double-click installer (NSIS supports per-user install, no admin).

## Open questions / risks

- **Tauri cross-compilation to Windows from Linux.** Possible via `cargo
  xwin` or `mingw-w64` but not first-class supported by `cargo-tauri`.
  Easier route: provision a Windows builder once (personal VM, or a single
  GH Actions Windows job — note CI risks above). Decide before step 8.
- **Bundling balloons installer size.** Tauri NSIS installer with a 456 MB
  resource will be ~500 MB. Within GitHub Release 2 GB limit, but expect
  slow uploads.
- **The Tauri updater fires on Linux/macOS launch.** For our use case we
  want it off. Recommend stripping in step 3.
- **Model integrity at the publisher level.** Pulling from
  `blob.handy.computer` delegates trust to cjpais. Smoke eval (step 7) is
  the cheap defense. Full provenance requires reproducing int8 quant from
  NVIDIA `.nemo` ourselves — defer unless required.
- **bwrap-only sandbox doesn't actually enforce network confinement** in
  observation mode. Rust crates that ignore `HTTPS_PROXY` could escape.
  If we want strict mode, switch to rootless podman with
  `--network=container:mitmproxy` (build) and a podman runtime container
  with audio/X11 socket passthrough (run).

## Reference paths

- Jail config: `network-jail/`
- Upstream clone: `handy/` (gitignored)
- Sibling project this pattern came from: `~/thegoodapp/thegoodapp/network-jail/`
- Handy bundling code: `handy/src-tauri/src/managers/model.rs:651`
- Tauri config: `handy/src-tauri/tauri.conf.json`
- Model catalog (URLs + sizes): `handy/src-tauri/src/managers/model.rs:130-600` and `docs/expected-egress.md`
