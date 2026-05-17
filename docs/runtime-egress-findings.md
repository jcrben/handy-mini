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

| Host | Hits | Path | Verdict |
|---|---:|---|---|
| `cdn.mxpnl.com:80` | 3 | `/libs/mixpanel-2-latest.min.js` | **🚨 Mixpanel analytics — undocumented, fires unconditionally on launch** |
| `github.com:443` | 2 | `/cjpais/Handy/releases/latest/download/latest.json` + redirected to `/v0.8.3/latest.json` | Tauri updater — expected, fires on startup |
| `release-assets.githubusercontent.com:443` | 1 | (long Azure Blob signed URL) | GitHub redirect target for updater's `latest.json`. Blocked because not in run allowlist. The updater logs `update endpoint did not respond with a successful status code` and gives up cleanly. |

**Zero hits to `blob.handy.computer`** — the bundling patch eliminated
all model-download egress, as designed.

## Key findings

### 1. Mixpanel analytics fires on launch — undocumented

Three blocked requests on every launch:

```
http://cdn.mxpnl.com/libs/mixpanel-2-latest.min.js
```

Upstream Handy's README claims: *"Opt-in Analytics: Privacy-first
approach with clear opt-in"* and *"Your voice stays on your computer."*

But the v0.8.3 build (commit `e3206aa` on `main`) silently loads the
Mixpanel SDK JS on every launch. The actual payload calls (tracking
events) would follow once the JS loads — we couldn't observe them
because the jail blocks the SDK script first.

**This is the most important finding of the audit.** For a shadow-IT
install at work, this is unacceptable — Mixpanel JS execution from the
webview would phone home with event data and the user's IP each launch.

Source-level grep for `mixpanel` / `mxpnl` / `posthog` / `analytics` /
`telemetry` in `src/`, `src-tauri/src/`, `index.html` returns **zero
matches**. The Mixpanel call is therefore likely:

- bundled into the React dist via a transitive npm dependency, or
- a runtime injection by a Tauri plugin, or
- compiled-in via Vite's bundling (Mixpanel SDK loaded by URL even though
  the project doesn't reference it directly).

Need to track down which. Candidates to inspect:

- `dist/assets/*.js` (the built bundle) — grep there
- Tauri plugin dependencies that may inject analytics scripts
- Vite/React plugins that add tracking by default

**Mitigations:**

- Block at the jail level (Linux-only solution, won't survive at work).
- Strip the offending dependency or build flag and rebuild.
- Compile with a Vite plugin that intercepts and removes Mixpanel.
- (Most robust) Patch the dist bundle post-build to remove the Mixpanel
  fetch.

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

1. Track down the Mixpanel reference. Grep `dist/`. If it's in a transitive
   dep, identify which one and strip it.
2. Strip `tauri-plugin-updater` from `Cargo.toml`. Rebuild.
3. Optionally prune post-process providers list.
4. Re-run the runtime test. Expect **zero** captured egress (all hosts
   blocked or never reached).
5. Then promote to "ready to ship to work" status.
