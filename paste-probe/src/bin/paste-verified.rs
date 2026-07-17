// paste-verified — Handy external_script paste with lost-paste detection.
//
// Wired via Handy settings: paste_method=external_script, external_script_path
// pointing at the handy-paste-verified.bash wrapper. Handy invokes this with
// the transcript as argv[1] and blocks until exit; non-zero exit makes Handy
// log a paste error and emit its paste-error event.
//
// Flow (validated by ../paste-probe experiments, 2026-07-16):
//   1. save the current clipboard (wl-paste)
//   2. OWN the selection with the transcript (ext_data_control_v1)
//   3. after Handy's usual 60ms settle, send Ctrl+V exactly as Handy does
//      (ydotool key 29:1 47:1 47:0 29:0)
//   4. every reader of the selection hits our `send` handler. Reads BEFORE the
//      keystroke finished = Plasma's clipboard-manager burst (measured 0-7ms).
//      Any read AFTER it = the paste target consuming the paste.
//   5. landed  -> restore the old clipboard, exit 0
//      lost    -> restore the old clipboard, notify-send pointing at the
//                 Ctrl+Alt+Space re-type recovery, exit 1 (never silent)
//
// --simulate skips the Ctrl+V (standalone testing: trigger a read manually
// with `wl-paste`).

use std::io::Write;
use std::os::fd::AsFd;
use std::process::Command;
use std::time::{Duration, Instant};

use wayland_client::{
    event_created_child,
    protocol::{wl_registry, wl_seat},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::ExtDataControlOfferV1,
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};

const MIMES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];
const SETTLE_MS: u64 = 60; // mirrors Handy's paste_delay_ms
const VERDICT_TIMEOUT_MS: u64 = 1500;

struct State {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<ExtDataControlManagerV1>,
    text: String,
    start: Instant,
    // set once the Ctrl+V has been dispatched; reads after this = the target
    threshold: Option<Instant>,
    burst_reads: u32,
    landed: bool,
    cancelled: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" => {
                    state.seat =
                        Some(registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(2), qh, ()));
                }
                "ext_data_control_manager_v1" => {
                    state.manager = Some(registry.bind::<ExtDataControlManagerV1, _, _>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    ));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtDataControlManagerV1,
        _: <ExtDataControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlSourceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtDataControlSourceV1,
        event: <ExtDataControlSourceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { mime_type, fd } => {
                let ms = state.start.elapsed().as_millis();
                let is_target = state
                    .threshold
                    .map(|t| Instant::now() >= t)
                    .unwrap_or(false);
                if is_target {
                    state.landed = true;
                    eprintln!("paste-verified: target read at {}ms ({})", ms, mime_type);
                } else {
                    state.burst_reads += 1;
                    eprintln!(
                        "paste-verified: manager burst read at {}ms ({})",
                        ms, mime_type
                    );
                }
                let mut f = std::fs::File::from(fd);
                let _ = f.write_all(state.text.as_bytes());
            }
            ext_data_control_source_v1::Event::Cancelled => {
                state.cancelled = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtDataControlDeviceV1,
        _: <ExtDataControlDeviceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(State, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ExtDataControlOfferV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtDataControlOfferV1,
        _: <ExtDataControlOfferV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

fn read_current_clipboard() -> Option<String> {
    let out = Command::new("wl-paste").arg("-n").output().ok()?;
    if out.status.success() {
        String::from_utf8(out.stdout).ok()
    } else {
        None // empty clipboard or unreadable
    }
}

fn restore_clipboard(old: &Option<String>) {
    // wl-copy takes selection ownership; our source gets Cancelled, which is fine.
    let result = match old {
        Some(s) if !s.is_empty() => Command::new("wl-copy").arg("--").arg(s).status(),
        _ => Command::new("wl-copy").arg("--clear").status(),
    };
    if result.is_err() {
        eprintln!("paste-verified: WARNING failed to restore clipboard");
    }
}

fn notify_lost(text: &str) {
    let preview: String = text.chars().take(80).collect();
    let _ = Command::new("notify-send")
        .args([
            "-u",
            "critical",
            "-a",
            "handy-paste-verified",
            "Dictation did NOT land",
            &format!("Ctrl+Alt+Space re-types it.\n\u{201c}{}\u{2026}\u{201d}", preview),
        ])
        .status();
}

fn main() {
    let mut args = std::env::args().skip(1);
    let text = args.next().unwrap_or_else(|| {
        eprintln!("usage: paste-verified <text> [--simulate]");
        std::process::exit(2);
    });
    let simulate = args.next().as_deref() == Some("--simulate");

    let old_clipboard = read_current_clipboard();

    let conn = Connection::connect_to_env().expect("no Wayland display");
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    display.get_registry(&qh, ());

    let mut state = State {
        seat: None,
        manager: None,
        text: text.clone(),
        start: Instant::now(),
        threshold: None,
        burst_reads: 0,
        landed: false,
        cancelled: false,
    };

    event_queue.roundtrip(&mut state).unwrap();
    event_queue.roundtrip(&mut state).unwrap();

    let seat = state.seat.clone().expect("no wl_seat advertised");
    let manager = state
        .manager
        .clone()
        .expect("compositor lacks ext_data_control_manager_v1");

    let source = manager.create_data_source(&qh, ());
    for mime in MIMES {
        source.offer(mime.to_string());
    }
    let device = manager.get_data_device(&seat, &qh, ());
    device.set_selection(Some(&source));
    state.start = Instant::now();
    event_queue.flush().unwrap();

    // Serve the manager burst during the settle window.
    let settle_deadline = Instant::now() + Duration::from_millis(SETTLE_MS);
    pump(&conn, &mut event_queue, &mut state, settle_deadline);

    // Dispatch Ctrl+V exactly as Handy's ydotool path does, then start the clock.
    if !simulate {
        let status = Command::new("ydotool")
            .args(["key", "29:1", "47:1", "47:0", "29:0"])
            .status();
        match status {
            Ok(s) if s.success() => {}
            _ => {
                restore_clipboard(&old_clipboard);
                eprintln!("paste-verified: ydotool failed (is ydotoold running?)");
                std::process::exit(2);
            }
        }
    }
    state.threshold = Some(Instant::now());

    let verdict_deadline = Instant::now() + Duration::from_millis(VERDICT_TIMEOUT_MS);
    while Instant::now() < verdict_deadline && !state.landed && !state.cancelled {
        pump(
            &conn,
            &mut event_queue,
            &mut state,
            Instant::now() + Duration::from_millis(50),
        );
    }

    if state.landed {
        restore_clipboard(&old_clipboard);
        eprintln!("paste-verified: OK (burst_reads={})", state.burst_reads);
        std::process::exit(0);
    }
    if state.cancelled {
        // Another client took the selection mid-verdict (e.g. the user copied
        // something). Verdict unknown — don't clobber their new clipboard,
        // don't claim failure.
        eprintln!("paste-verified: selection taken by another client; verdict unknown");
        std::process::exit(0);
    }
    restore_clipboard(&old_clipboard);
    notify_lost(&text);
    eprintln!(
        "paste-verified: LOST — no read within {}ms (burst_reads={})",
        VERDICT_TIMEOUT_MS, state.burst_reads
    );
    std::process::exit(1);
}

fn pump(conn: &Connection, queue: &mut wayland_client::EventQueue<State>, state: &mut State, until: Instant) {
    use std::os::unix::io::AsRawFd;
    while Instant::now() < until {
        let guard = match conn.prepare_read() {
            Some(g) => g,
            None => {
                queue.dispatch_pending(state).unwrap();
                continue;
            }
        };
        let mut pfd = PollFd {
            fd: conn.as_fd().as_raw_fd(),
            events: 1, // POLLIN
            revents: 0,
        };
        let remaining = until.saturating_duration_since(Instant::now());
        let timeout_ms = remaining.as_millis().min(50) as i32;
        let ready = unsafe { libc_poll(&mut pfd, 1, timeout_ms) };
        if ready > 0 {
            let _ = guard.read();
            queue.dispatch_pending(state).unwrap();
        } else {
            drop(guard);
        }
        queue.flush().unwrap();
    }
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    #[link_name = "poll"]
    fn libc_poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
}
