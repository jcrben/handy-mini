# paste-probe — lost-paste detection prototype

Prototype for detecting whether a clipboard paste was actually consumed by the
target application (dotfiles todo `0bd694`: Handy dictation clobbered by the
Claude Code TUI reflash).

## The problem

Handy pastes via clipboard + synthetic Ctrl+V, then restores the previous
clipboard after a hardcoded 50ms (`clipboard.rs` `paste_via_clipboard`). A busy
target (Claude Code mid-redraw) dequeues the Ctrl+V late, reads the
already-restored clipboard, and the dictation lands nowhere. The keystroke is
fire-and-forget on Wayland — Handy cannot see whether it worked.

## The detection

A Wayland client that **owns** the selection receives a `send` event from the
compositor for every reader request. So instead of shelling out to `wl-copy`,
own the selection via `ext_data_control_v1` (KWin on Plasma 6 advertises only
`ext`, not the older `zwlr` variant) and watch who reads it.

Plasma's built-in clipboard manager reads every new offer eagerly — and re-reads
even identical content, so offer-twice/dedup tricks don't work. But its reads
are a tight burst right after `set_selection`; the paste target's read comes
later, when the app processes the Ctrl+V.

## Validated on 2026-07-16 (KWin, Plasma 6, Aurora)

```
negative (nothing pastes):        READ 0ms  READ 2ms              SUMMARY reads=2
positive (target reads at 1.5s):  READ 6ms  READ 7ms  READ 1502ms SUMMARY reads=3
```

**Rule: ignore reads within the initial burst window (~50ms of owning the
selection). Any read after the burst ⇒ paste consumed. No read within the
timeout ⇒ paste lost.** Manager burst is 2 reads of
`text/plain;charset=utf-8` on this system.

## Usage

```
cargo build --release
target/release/paste-probe <text> [timeout_secs]   # exit 0 = timed out, 2 = selection stolen
```

Each selection read prints `READ <ms>ms mime=<mime>`.

## Handy patch sketch (next step)

In `paste_via_clipboard` (Rust — this prototype's code carries over directly):

1. Replace the `wl-copy` shell-out with an owned `ext_data_control_v1` source
   (fall back to `zwlr_data_control_v1`, then `wl-copy`, for other compositors).
2. Send the paste key combo as today.
3. Wait for a post-burst `send` on the source (e.g. burst window 50ms, verdict
   timeout ~1000ms) instead of the fixed 50ms sleep.
4. On consumed → restore the old clipboard as today. On lost → keep serving the
   transcript (or notify) so the user can paste manually — never silently drop.

Caveats for the patch: the burst window is compositor/manager-specific
(measure at startup or make configurable); a second clipboard manager would add
burst reads; X11 sessions need the old path.
