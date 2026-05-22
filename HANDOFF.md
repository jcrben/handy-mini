---
title: Handy-mini handoff
status: linux-evidence-shipped
updated: 2026-05-18
---

## Status milestone 2026-05-17: Linux build verified offline-clean

The Linux build of Handy with the two `patches/` applied is silent on
the network. This repo is the audit evidence trail.

| Artifact | Path | Status |
|---|---|---|
| `target/release/handy` (Linux x86_64) | `~/handy/handy/src-tauri/target/release/handy` (68 MB) | Built and verified |
| Parakeet v3 int8 model | `model-source/parakeet-v3-int8.tar.gz` (457 MB, gitignored) | SHA256 matches upstream-declared hash |
| Bundling patch | `patches/01-bundle-parakeet-v3-and-dir-copy.patch` | Applied, builds, migrates model on first run |
| Updater-strip patch | `patches/02-strip-tauri-plugin-updater.patch` | Applied, builds, zero outbound on launch |
| Jailed build wrapper | `network-jail/build-in-distrobox.bash` | Ubuntu 24.04 distrobox `handy-build` |
| Jailed run wrapper | `network-jail/run-in-distrobox.bash` | (Note: tauri-plugin-single-instance uses session DBus; run-tests need a private dbus-launch'd bus to bypass collisions with any host-side running Handy) |
| Runtime egress findings | `docs/runtime-egress-findings.md` | Final result: **zero outbound** |

The Windows cross-compile path via `cargo-xwin` was attempted and is
**deferred** - see open task / "Open issues" below.



# Handy-mini — handoff state

This repo wraps [cjpais/Handy](https://github.com/cjpais/Handy/) with a network
jail and a plan to produce an audited, offline-capable build for use on a
a locked-down Windows machine without admin privileges.

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

- `network-jail/` uses mitmproxy + coredns + JSONL logger to observe and restrict egress.
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

## What's done since first commit

- **bun 1.3.14** installed globally via `mise use -g bun@latest`.
- **Parakeet v3 int8 model downloaded and SHA256-verified.**
  - File: `model-source/parakeet-v3-int8.tar.gz` (457 MB, gitignored)
  - Hash: `43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77`
  - **Matches** the hash in Handy's source at
    `handy/src-tauri/src/managers/model.rs:309-310`. End-to-end trust chain
    is verified: source → git → GitHub release → downloaded bytes.
- Tarball extracted to `model-source/parakeet-tdt-0.6b-v3-int8/`. macOS
  `._*` AppleDouble files stripped. Contents:
  - `encoder-model.int8.onnx` (622 MB)
  - `decoder_joint-model.int8.onnx` (18 MB)
  - `nemo128.onnx` (137 KB)
  - `vocab.txt` (92 KB)
  - `config.json` (97 B, says `model_type: nemo-conformer-tdt`)
- **Bundling patch written** at
  `patches/01-bundle-parakeet-v3-and-dir-copy.patch`. Two changes:
  - Adds `parakeet-tdt-0.6b-v3-int8` to the bundled-models list in
    `migrate_bundled_models`.
  - Adds a `copy_dir_recursive` helper so directory-shaped models work
    (existing code was single-file-only via `fs::copy`).
  - Patch is also applied to the working upstream clone at
    `handy/src-tauri/src/managers/model.rs` for immediate building.
- Model staged at
  `handy/src-tauri/resources/models/parakeet-tdt-0.6b-v3-int8/` so the
  `resources/**/*` glob in `tauri.conf.json` will bundle it.
- **Build environment is the current blocker.** Aurora is immutable, so
  `dnf install` of webkit2gtk-4.1 etc. requires admin + reboot. Tried two
  approaches:
  1. `nix develop` against Handy's flake — fails at the final
     `nix-shell-env.drv` build step with
     `error: setting up a private mount namespace: Operation not permitted`
     even with `--option sandbox false`. Substituters did succeed at copying
     webkitgtk, rustc 1.94, cargo, clang, gtk3, pipewire into `/nix/store/`.
  2. `nix print-dev-env` + source in a clean shell — bypasses the sandbox
     but Handy's flake `shellHook` runs `bun install` automatically (outside
     the jail) and the resulting env does not have webkit2gtk on
     `PKG_CONFIG_PATH`. Half-broken.
  Bwrap works on this kernel (we use it elsewhere), so user namespaces are
  *enabled*; the nix failure is specific to its sandbox setup. Suspect
  `unprivileged_userns_clone` or a Toolbox-style mount restriction.

## Build environment: distrobox handy-build (Ubuntu 24.04)

Resolved. The box exists and the Tauri Linux deps are installed.

```bash
distrobox create --name handy-build --image docker.io/library/ubuntu:24.04 --yes
distrobox enter handy-build -- bash -c '
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends \
    build-essential libasound2-dev pkg-config libssl-dev \
    libvulkan-dev vulkan-tools glslc \
    libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
    librsvg2-dev libgtk-layer-shell0 libgtk-layer-shell-dev \
    patchelf cmake curl ca-certificates git unzip
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --default-toolchain stable --profile minimal
'
```

Versions in the box:

- `libwebkit2gtk-4.1-dev` 2.52.3-0ubuntu0.24.04.1
- `libgtk-3-dev` 3.24.41
- `libasound2-dev` 1.2.11
- rustup-installed `cargo 1.95.0` + `rustc 1.95.0` (April 2026 stable)

Host `bun 1.3.14` (installed via `mise use -g bun@latest`) is visible
inside the box because `$HOME` is shared by distrobox.

Mitmproxy on `127.0.0.1:18080` is reachable from inside distrobox because
the box shares the host's network namespace.

The wrapper that ties this together is `network-jail/build-in-distrobox.bash`.
It runs `docker compose up` on the host (BUILD phase + build allowlist),
waits for the CA, then `distrobox enter handy-build -- bash -lc "..."` to
run the build with the right `PATH` (`~/.cargo/bin` + bun first, ahead of
the nix-portable cargo wrapper that's on the shared `$HOME` PATH) and the
proxy + CA env vars.

## What's next (do this in order)

1. **Set up the build environment** — see "Path forward for the build
   environment" section above. Top recommendation: `distrobox create
   --name handy-build --image fedora:41` and dnf-install Tauri's deps.

2. **(Optional but recommended) Disable the Tauri updater** so the runtime
   never phones home for updates. Either remove `tauri-plugin-updater`
   from `handy/src-tauri/Cargo.toml` or unset its endpoint in
   `handy/src-tauri/tauri.conf.json`. Verify by re-running the run-jail
   and confirming no `github.com` hit at startup.

3. **First jailed build**: `~/handy/network-jail/build-jailed.bash`.
   Capture `~/handy/network-jail/runtime/data/observed-hosts.jsonl`. Diff
   observed hosts against `allowlist-build.txt`. Iterate until clean (no
   `http_request_blocked` records). Hosts not in the allowlist but
   legitimate → add to allowlist; hosts unexpected → investigate.

4. **First jailed run**: `~/handy/network-jail/run-jailed.bash --release`.
   Drive: first-launch, two transcriptions, idle 60s. Confirm zero
   `blob.handy.computer` hits (because model is bundled), and updater hit
   to `github.com` if not patched out in step 2.

5. **Smoke-eval Parakeet** on ~100 LibriSpeech-test-clean utterances.
   Compare WER to NVIDIA's published number (~5%). Catches gross
   replacement of weights. Doesn't catch a targeted backdoor.

6. **Publish**:
   - Push this repo to `github.com/jcrben/handy-mini` (public).
   - Build Windows binary on a remote Windows machine (see "Windows binary
     not yet built" section for the step-by-step).
   - Upload `.msi` / `.exe` + `sha256.txt` as a GitHub Release attachment.
   - Document work-side install in repo `INSTALL-AT-WORK.md`:
     - `git clone`
     - `curl -LO` release artifact
     - `Get-FileHash` to verify
     - Double-click installer (NSIS supports per-user install, no admin).

## Open issues for next session

### Windows binary not yet built

Cross-compile via `cargo-xwin` on the Ubuntu 24.04 distrobox got close
but hit two practical blockers:

1. `whisper-rs-sys` (a transitive dep of `transcribe-rs`) is set to
   compile whisper.cpp with `GGML_VULKAN=ON` because Handy's
   `Cargo.toml` enables the `whisper-vulkan` feature for Windows.
   Cross-compiling `whisper.cpp` for Windows needs the Windows Vulkan
   SDK, which `cargo-xwin` doesn't provide.
2. `cargo-xwin` itself bypasses our `HTTPS_PROXY` (downloaded ~1.2 GB
   of MSVC components into `~/.cache/cargo-xwin/` without showing up
   in `observed-hosts.jsonl`). Likely uses a TLS stack that doesn't
   respect proxy env vars. Means our build-time jail integrity is
   weaker than we thought for tools other than cargo / bun / git.

Path forward: **qemu-kvm Win11 VM on the remote Aurora box (chosen).**

The Aurora box (`aurora` / `192.168.8.128`, `ssh -p 8022 ben@192.168.8.128`) has
more CPU/RAM than this machine, making it a better host for the Win11 VM.
Spin up a qemu-kvm Win11 guest there, install MSVC build tools + Rust + bun +
Vulkan SDK, apply the two patches, stage the Parakeet model, and build natively.

Steps on the Aurora box:
1. Spin up qemu-kvm Win11 VM (see `~/code/cloud-init-jsonnet-play/local-network/`)
2. Inside the VM: install Rust (MSVC toolchain), Bun, MSVC Build Tools, Vulkan SDK for Windows
3. Clone `gitlab.com:jcrben/handy`
4. Apply `patches/01-bundle-parakeet-v3-and-dir-copy.patch` and `patches/02-strip-tauri-plugin-updater.patch`
5. Transfer `model-source/parakeet-tdt-0.6b-v3-int8/` into `src-tauri/resources/models/`
6. `bun install && bun run tauri build`
7. Collect `.exe` + `.msi` from `src-tauri/target/release/bundle/`
8. SHA256-hash the artifacts and upload as a GitLab Release under `v0.1-windows`

Alternative fallback paths (not chosen):
- **Strip whisper-vulkan + retry xwin.** Drop the `whisper-vulkan` feature
  on Windows in `src-tauri/Cargo.toml` and retry `cargo-xwin` on Linux.
  May surface more cross-compile issues; audit integrity still weaker.

### Smoke-eval Parakeet not yet done

Task #13. The Parakeet v3 int8 model passed SHA256 verification against
Handy's own source-declared hash, but we have not done a WER test
against a known set of utterances (e.g. LibriSpeech-test-clean ~100
samples). Cheap belt-and-suspenders against a wholesale-replaced model;
will not catch targeted backdoors.

### Allowlist gap: cargo-xwin

Build allowlist now lists `aka.ms`, `dl.microsoft.com`,
`visualstudio.microsoft.com`, `download.visualstudio.microsoft.com`,
`*.azureedge.net`, `*.cdn.visualstudio.com` for cargo-xwin's MSVC
downloads, but in practice cargo-xwin doesn't honour `HTTPS_PROXY` so
those records never showed up in observed-hosts. The entries are still
useful documentation of intended hosts.

## Open questions / risks

- **Windows build machine.** Decided: use a remote Windows machine (native
  MSVC build). See "Windows binary not yet built" section.
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
- Handy bundling code: `handy/src-tauri/src/managers/model.rs:651`
- Tauri config: `handy/src-tauri/tauri.conf.json`
- Model catalog (URLs + sizes): `handy/src-tauri/src/managers/model.rs:130-600` and `docs/expected-egress.md`
