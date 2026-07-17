// paste-probe — own the clipboard via zwlr_data_control_v1 and log every read.
//
// Prototype for Handy lost-paste detection (todo 0bd694): Handy pastes via
// clipboard + synthetic Ctrl+V and cannot tell whether the target ever read
// the selection. A data source CAN tell: the compositor delivers a `send`
// event per reader request. Plasma's clipboard manager reads eagerly within
// ~10ms of the offer; any read after that burst is the paste target.
//
// Usage: paste-probe <text> [timeout_secs]
// Output: one "READ <ms>ms mime=<mime>" line per send event, then
//         "SUMMARY reads=<n>" on timeout (exit 0) or "CANCELLED" if another
//         client took the selection (exit 2).

use std::io::Write;
use std::os::fd::AsFd;
use std::time::Instant;

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

struct State {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<ExtDataControlManagerV1>,
    text: String,
    start: Instant,
    reads: u32,
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
                state.reads += 1;
                println!("READ {}ms mime={}", ms, mime_type);
                // Write and drop the fd; small payloads won't block.
                let mut f = std::fs::File::from(fd);
                let _ = f.write_all(state.text.as_bytes());
            }
            ext_data_control_source_v1::Event::Cancelled => {
                state.cancelled = true;
                println!("CANCELLED (another client took the selection)");
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
        // data_offer/selection events describe OTHER clients' clipboards; ignore.
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

fn main() {
    let mut args = std::env::args().skip(1);
    let text = args.next().unwrap_or_else(|| {
        eprintln!("usage: paste-probe <text> [timeout_secs]");
        std::process::exit(1);
    });
    let timeout_secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);

    let conn = Connection::connect_to_env().expect("no Wayland display");
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    display.get_registry(&qh, ());

    let mut state = State {
        seat: None,
        manager: None,
        text,
        start: Instant::now(),
        reads: 0,
        cancelled: false,
    };

    // Two roundtrips: one to hear globals, one to finish the binds.
    event_queue.roundtrip(&mut state).unwrap();
    event_queue.roundtrip(&mut state).unwrap();

    let seat = state.seat.clone().expect("no wl_seat advertised");
    let manager = state
        .manager
        .clone()
        .expect("compositor lacks ext/zwlr data_control_manager_v1");

    let source = manager.create_data_source(&qh, ());
    for mime in MIMES {
        source.offer(mime.to_string());
    }
    let device = manager.get_data_device(&seat, &qh, ());
    device.set_selection(Some(&source));
    state.start = Instant::now(); // clock starts at selection ownership
    event_queue.flush().unwrap();

    let deadline = Instant::now() + std::time::Duration::from_secs(timeout_secs);
    while Instant::now() < deadline && !state.cancelled {
        // blocking_dispatch has no timeout; poll the fd with a short slice instead.
        use std::os::unix::io::AsRawFd;
        let guard = match conn.prepare_read() {
            Some(g) => g,
            None => {
                event_queue.dispatch_pending(&mut state).unwrap();
                continue;
            }
        };
        let mut pfd = libc_poll_fd(conn.as_fd().as_raw_fd());
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout_ms = remaining.as_millis().min(200) as i32;
        let ready = unsafe { libc_poll(&mut pfd, 1, timeout_ms) };
        if ready > 0 {
            let _ = guard.read();
            event_queue.dispatch_pending(&mut state).unwrap();
        } else {
            drop(guard);
        }
        event_queue.flush().unwrap();
    }

    println!("SUMMARY reads={}", state.reads);
    std::process::exit(if state.cancelled { 2 } else { 0 });
}

// Minimal poll(2) shim to avoid a libc crate dependency.
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

fn libc_poll_fd(fd: i32) -> PollFd {
    PollFd {
        fd,
        events: 1, // POLLIN
        revents: 0,
    }
}

extern "C" {
    #[link_name = "poll"]
    fn libc_poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
}
