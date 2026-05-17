---
title: Runtime Egress Findings
status: active
updated: 2026-05-17
---

# Runtime Egress Findings

What the jailed Handy v0.8.3 build (bundled Parakeet v3) actually
phones home to on a Linux launch with isolated XDG dirs + private DBus
session.

## Test setup

- Built binary: `~/handy/handy/src-tauri/target/release/handy`
- Built with: `~/handy/network-jail/build-in-distrobox.bash` (Ubuntu 24.04
  distrobox `handy-build`)
- Bundled model: `parakeet-tdt-0.6b-v3-int8` (verified SHA256
  `43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77`)
- Run env (to bypass tauri-plugin-single-instance which uses session DBus
  name registration and was colliding with the user's running AppImage):
  - private DBus session via `dbus-launch --sh-syntax`
  - isolated `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
    `XDG_CACHE_HOME`, `XDG_STATE_HOME` under `/tmp/handy-jail-test/`
  - host-side mitmproxy with `allowlist-run.txt`

## Captured egress (30 s, app launch only)

After three independent clean runs with User-Agent capture enabled, the
deterministic egress is just the Tauri updater:

| Host | Hits | UA | Path | Verdict |
|---|---:|---|---|---|
| `github.com:443` | 2 | `tauri-plugin-updater/2.10.0` | `/cjpais/Handy/releases/latest/download/latest.json` and the redirect to `/v0.8.3/latest.json` | Tauri updater — expected, fires on startup |
| `release-assets.githubusercontent.com:443` | 1 | `tauri-plugin-updater/2.10.0` | (long Azure Blob signed URL for `latest.json`) | GitHub redirect target for the updater. Blocked because not in run allowlist. Updater logs `update endpoint did not respond with a successful status code` and gives up cleanly. |

**Zero hits to `blob.handy.computer`** — the bundling patch eliminated
all model-download egress, as designed.

**Zero hits to Mixpanel / PostHog / any analytics vendor.** (See
retraction below.)

## Key findings

### 1. ~~Mixpanel analytics fires on launch — undocumented~~ (RETRACTED)

**Initial finding retracted.** The first capture pass showed three
blocked requests to `http://cdn.mxpnl.com/libs/mixpanel-2-latest.min.js`,
which I attributed to Handy. Three subsequent reproductions of the same
exact setup (clean `/tmp/handy-jail-test`, private DBus, isolated XDG,
host-side mitmproxy) produced **zero** Mixpanel hits.

Source-level grep for `mixpanel|mxpnl|posthog|analytics|telemetry` in
`src/`, `src-tauri/src/`, the built `dist/assets/*.js`, and the
installed `node_modules/` returns zero matches.

Most likely explanation: another process on the host briefly had
`HTTPS_PROXY=http://127.0.0.1:18080` exported (probably one of my own
debug subshells) and its requests landed in the same observed-hosts log.
Mitmproxy doesn't tag flows by source PID, so cross-talk between shells
that share the same `HTTPS_PROXY` env value is indistinguishable in the
log.

Lesson for the audit method: when running multiple jailed tests
concurrently, set `HTTPS_PROXY` only inside the launch process, never
into a parent shell. The current `build-in-distrobox.bash` and
`run-in-distrobox.bash` wrappers already follow this pattern; the leak
was from my interactive shell experimentation, not the wrappers.

### 2. Tauri updater fires unconditionally

The updater hits `https://github.com/cjpais/Handy/releases/latest/download/latest.json`
on launch, even with `update_checks_enabled: true` (default) but no user
prompt. GitHub returns a redirect to the new
`release-assets.githubusercontent.com` URL pattern (which serves files
from Azure Blob via signed JWT).

For the work install (shadow IT) we want **zero outbound on launch**.
Recommend:

- Either disable `tauri-plugin-updater` in `Cargo.toml`, or
- Unset its `endpoints` array in `tauri.conf.json`, or
- Set `update_checks_enabled: false` as the default value in the source.

Removing the plugin entirely is cleanest — reduces binary size and code
surface.

### 3. Bundled model migration works end-to-end

Logged at startup:

```
[INFO] Migrating bundled model parakeet-tdt-0.6b-v3-int8 to user directory
[INFO] Successfully migrated parakeet-tdt-0.6b-v3-int8
[INFO] Auto-selecting model: parakeet-tdt-0.6b-v3 (Parakeet V3)
```

Our `migrate_bundled_models` patch (see
`patches/01-bundle-parakeet-v3-and-dir-copy.patch`) successfully copied
the directory-shaped Parakeet from `resources/models/` to
`$XDG_DATA_HOME/com.pais.handy/`, and the auto-select picked Parakeet v3
as the default model.

### 4. Other notable runtime behavior (not egress)

- `arboard` clipboard fell back from Wayland → X11 inside distrobox.
- `GTK layer shell not available, falling back to regular window` — the
  overlay window can't use wlr-layer-shell from inside distrobox.
- ALSA can't find pipewire/pulse/jack/oss inside distrobox by default —
  audio would not actually work without bind-mounting the PipeWire
  socket. Doesn't affect the egress audit.
- `tauri-plugin-single-instance` uses session DBus name registration; on
  Linux, two installations colliding causes the second to silently
  `exit(0)`. Required a private dbus-daemon for testing.

### 5. Settings expose a long list of LLM post-processing providers

Default settings include base URLs for:

- `api.openai.com`, `api.anthropic.com`, `openrouter.ai`, `api.groq.com`,
  `api.cerebras.ai`, `api.z.ai`, `bedrock-mantle.us-east-1.api.aws`,
  `localhost:11434` (Ollama).

None will fire unless `post_process_enabled: true` and an API key is
filled in, but they are present in the bundled default settings.

For a paranoid build, recommend either pruning these defaults to only
`localhost` (Ollama) or removing post-processing entirely.

## Next steps

1. ~~Track down the Mixpanel reference.~~ Retracted - it was operator error.
2. Strip `tauri-plugin-updater` from `Cargo.toml`. Rebuild. Expected
   result: zero outbound on launch.
3. Optionally prune post-process providers list.
4. Re-run the runtime test. Expect **zero** captured egress (all hosts
   blocked or never reached).
5. Then promote to "ready to ship to work" status.

## Conclusion

Handy v0.8.3 with the bundled Parakeet v3 model and the directory-copy
patch is a clean offline app *except for the unconditional Tauri updater
hit on launch*. Strip the updater and the build is silent on the
network. Compared to the upstream Handy install (which downloads the
model from blob.handy.computer on first run *and* hits the updater),
this version is materially safer for an air-gapped or
allowlist-restricted environment.

## Update 2026-05-17: stripped build verified silent

Applied `patches/02-strip-tauri-plugin-updater.patch`:

- Removed `tauri-plugin-updater = "2.10.0"` from `src-tauri/Cargo.toml`
- Removed `.plugin(tauri_plugin_updater::Builder::new().build())` from
  `src-tauri/src/lib.rs`
- Removed `"updater:default"` from both
  `src-tauri/capabilities/default.json` and
  `src-tauri/capabilities/desktop.json`
- Kept the `plugins.updater` block in `src-tauri/tauri.conf.json` with
  `endpoints: []` (Tauri's bundler step still reads the block to decide
  whether to produce updater artifacts; the runtime plugin code is no
  longer compiled in so the empty endpoints don't matter at runtime)

Re-ran the jailed runtime test (clean `/tmp/handy-jail-test`, private
DBus, isolated XDG, mitmproxy on host with run allowlist + User-Agent
capture).

Result: **zero outbound HTTP/S calls** during the full 25 s launch
window. The `observed-hosts.jsonl` file does not even get created
because the mitmproxy addon only writes on the first observed request.

App functionality preserved:

```
[INFO] Migrating bundled model parakeet-tdt-0.6b-v3-int8 ...
[INFO] Successfully migrated parakeet-tdt-0.6b-v3-int8
[INFO] Auto-selecting model: parakeet-tdt-0.6b-v3
```

Stripped binary size: 68 MB (down from 70 MB).
