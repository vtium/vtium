//! The real sd-bus half of the follow, on a peer-to-peer sd-bus connection
//! over a socketpair, which needs no bus daemon. One end stands in for
//! localed and sends signals it builds with sd-bus. The other end is
//! vtium's [`SystemSignals`], with its real match, its real handler and its
//! real `take`. A peer-to-peer connection applies the match in sd-bus
//! itself, as sd-bus does for a bus connection after the bus has routed a
//! message.

#![expect(unsafe_code, reason = "sd-bus's C interface, to build the peer")]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::os::fd::IntoRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::ptr;

use super::*;
use crate::logind::sd_bus_flush_close_unref;

/// sd-bus's `sd_id128_t`, which `sd_bus_set_server` takes by value.
#[repr(C)]
#[derive(Clone, Copy)]
struct Id128 {
    qwords: [u64; 2],
}

unsafe extern "C" {
    fn sd_bus_new(bus: *mut *mut c_void) -> c_int;
    fn sd_bus_set_fd(bus: *mut c_void, input: c_int, output: c_int) -> c_int;
    fn sd_bus_set_server(bus: *mut c_void, b: c_int, id: Id128) -> c_int;
    fn sd_bus_set_anonymous(bus: *mut c_void, b: c_int) -> c_int;
    fn sd_bus_start(bus: *mut c_void) -> c_int;
    fn sd_bus_is_ready(bus: *mut c_void) -> c_int;
    fn sd_bus_process(bus: *mut c_void, r: *mut *mut c_void) -> c_int;
    fn sd_bus_message_new_signal(
        bus: *mut c_void,
        m: *mut *mut c_void,
        path: *const c_char,
        interface: *const c_char,
        member: *const c_char,
    ) -> c_int;
    fn sd_bus_message_append_basic(m: *mut c_void, kind: c_char, p: *const c_void) -> c_int;
    fn sd_bus_message_open_container(
        m: *mut c_void,
        kind: c_char,
        contents: *const c_char,
    ) -> c_int;
    fn sd_bus_message_close_container(m: *mut c_void) -> c_int;
    fn sd_bus_message_set_sender(m: *mut c_void, sender: *const c_char) -> c_int;
    fn sd_bus_message_set_destination(m: *mut c_void, destination: *const c_char) -> c_int;
    fn sd_bus_send(bus: *mut c_void, m: *mut c_void, cookie: *mut u64) -> c_int;
    fn sd_bus_flush(bus: *mut c_void) -> c_int;
    fn sd_bus_message_unref(m: *mut c_void) -> *mut c_void;
}

fn check(what: &str, r: c_int) {
    assert!(
        r >= 0,
        "{what}: {}",
        io::Error::from_raw_os_error(r.saturating_neg())
    );
}

/// A new sd-bus connection on `fd`, not started.
fn connection(fd: UnixStream) -> *mut c_void {
    let mut bus = ptr::null_mut();
    // SAFETY: `bus` is a valid out pointer.
    check("sd_bus_new", unsafe { sd_bus_new(&mut bus) });
    let fd = fd.into_raw_fd();
    // SAFETY: `bus` is the new connection; it owns `fd` from here on.
    check("sd_bus_set_fd", unsafe { sd_bus_set_fd(bus, fd, fd) });
    bus
}

/// The end that stands in for localed.
struct Localed(*mut c_void);

impl Drop for Localed {
    fn drop(&mut self) {
        // SAFETY: the connection `pair` made, freed once.
        unsafe {
            sd_bus_flush_close_unref(self.0);
        }
    }
}

/// localed's end and vtium's end of one peer-to-peer connection, both
/// through their authentication.
fn pair() -> (Localed, SystemSignals) {
    let (a, b) = UnixStream::pair().unwrap_or_else(|e| panic!("a socketpair: {e}"));
    let server = connection(a);
    let client = connection(b);
    let id = Id128 {
        qwords: [0x7674_6975_6d00_0001, 0x6c61_796f_7574_0080],
    };
    // SAFETY: both are new connections, configured before they start.
    unsafe {
        check("sd_bus_set_server", sd_bus_set_server(server, 1, id));
        check("sd_bus_set_anonymous", sd_bus_set_anonymous(server, 1));
        check("sd_bus_set_anonymous", sd_bus_set_anonymous(client, 1));
        check("sd_bus_start", sd_bus_start(server));
        check("sd_bus_start", sd_bus_start(client));
    }
    let localed = Localed(server);
    let vtium = SystemSignals::on(client);
    // the two ends authenticate in turns, each turn moving one step
    for _ in 0..100 {
        // SAFETY: both connections are open.
        let ready = unsafe {
            check("sd_bus_process", sd_bus_process(server, ptr::null_mut()));
            check("sd_bus_process", sd_bus_process(client, ptr::null_mut()));
            sd_bus_is_ready(server) > 0 && sd_bus_is_ready(client) > 0
        };
        if ready {
            return (localed, vtium);
        }
    }
    panic!("the two ends of the socketpair never finished authenticating");
}

/// One `PropertiesChanged` that the localed end sends.
struct Signal<'a> {
    sender: &'a CStr,
    destination: Option<&'a CStr>,
    path: &'a CStr,
    interface: &'a CStr,
    /// The changed properties, each with a string value.
    changed: &'a [(&'a CStr, &'a CStr)],
    /// The names of the invalidated properties.
    invalidated: &'a [&'a CStr],
}

/// localed's broadcast that its X11 layout and model changed, with the
/// console keymap invalidated.
fn localeds() -> Signal<'static> {
    Signal {
        sender: c"org.freedesktop.locale1",
        destination: None,
        path: c"/org/freedesktop/locale1",
        interface: c"org.freedesktop.locale1",
        changed: &[(c"X11Layout", c"de"), (c"X11Model", c"pc105")],
        invalidated: &[c"VConsoleKeymap"],
    }
}

fn byte(c: u8) -> c_char {
    c_char::from_ne_bytes([c])
}

impl Localed {
    /// Sends `signal`, built with sd-bus as localed builds its own.
    fn send(&self, signal: &Signal<'_>) {
        let mut m = ptr::null_mut();
        // SAFETY: `self.0` is open; every string is NUL-ended; each
        // container opened is closed in turn, as "sa{sv}as" lays them out;
        // the message is sent once and then freed.
        unsafe {
            check(
                "sd_bus_message_new_signal",
                sd_bus_message_new_signal(
                    self.0,
                    &mut m,
                    signal.path.as_ptr(),
                    c"org.freedesktop.DBus.Properties".as_ptr(),
                    c"PropertiesChanged".as_ptr(),
                ),
            );
            check(
                "sender",
                sd_bus_message_set_sender(m, signal.sender.as_ptr()),
            );
            if let Some(destination) = signal.destination {
                check(
                    "destination",
                    sd_bus_message_set_destination(m, destination.as_ptr()),
                );
            }
            let string = |m, s: &CStr| {
                check(
                    "append",
                    sd_bus_message_append_basic(m, byte(b's'), s.as_ptr().cast()),
                );
            };
            string(m, signal.interface);
            check(
                "a{sv}",
                sd_bus_message_open_container(m, byte(b'a'), c"{sv}".as_ptr()),
            );
            for (name, value) in signal.changed {
                check(
                    "{sv}",
                    sd_bus_message_open_container(m, byte(b'e'), c"sv".as_ptr()),
                );
                string(m, name);
                check(
                    "v",
                    sd_bus_message_open_container(m, byte(b'v'), c"s".as_ptr()),
                );
                string(m, value);
                check("close", sd_bus_message_close_container(m));
                check("close", sd_bus_message_close_container(m));
            }
            check("close", sd_bus_message_close_container(m));
            check(
                "as",
                sd_bus_message_open_container(m, byte(b'a'), c"s".as_ptr()),
            );
            for name in signal.invalidated {
                string(m, name);
            }
            check("close", sd_bus_message_close_container(m));
            check("send", sd_bus_send(self.0, m, ptr::null_mut()));
            check("flush", sd_bus_flush(self.0));
            sd_bus_message_unref(m);
        }
    }

    /// Sends a `PropertiesChanged` whose arguments are one string, which
    /// is not the signal's signature.
    fn send_malformed(&self) {
        let mut m = ptr::null_mut();
        // SAFETY: as in `send`.
        unsafe {
            check(
                "sd_bus_message_new_signal",
                sd_bus_message_new_signal(
                    self.0,
                    &mut m,
                    c"/org/freedesktop/locale1".as_ptr(),
                    c"org.freedesktop.DBus.Properties".as_ptr(),
                    c"PropertiesChanged".as_ptr(),
                ),
            );
            check(
                "sender",
                sd_bus_message_set_sender(m, c"org.freedesktop.locale1".as_ptr()),
            );
            check(
                "append",
                sd_bus_message_append_basic(
                    m,
                    byte(b's'),
                    c"org.freedesktop.locale1".as_ptr().cast(),
                ),
            );
            check("send", sd_bus_send(self.0, m, ptr::null_mut()));
            check("flush", sd_bus_flush(self.0));
            sd_bus_message_unref(m);
        }
    }
}

/// vtium's end, watching for localed's signal with its real match.
fn watching() -> (Localed, SystemSignals) {
    let (localed, mut vtium) = pair();
    watch_localed(&mut vtium).unwrap_or_else(|e| panic!("the match: {e}"));
    (localed, vtium)
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

/// The handler reads localed's signal whole: the changed properties'
/// names from the dictionary, which is all localed fills, and the
/// invalidated names after them, with no destination.
#[test]
fn the_handler_reads_localeds_signal() {
    let (localed, mut vtium) = watching();
    localed.send(&localeds());
    let taken = vtium.take().unwrap_or_else(|e| panic!("take: {e}"));
    assert_eq!(
        taken,
        [Changed {
            destination: None,
            interface: "org.freedesktop.locale1".to_owned(),
            properties: names(&["X11Layout", "X11Model", "VConsoleKeymap"]),
        }]
    );
    assert!(taken.iter().all(Changed::moves_layout));
    assert_eq!(vtium.take().map(|t| t.len()).ok(), Some(0), "taken once");
}

/// The match passes only localed's object and the PropertiesChanged
/// member, and the handler drops a signal it cannot read.
#[test]
fn the_match_and_the_handler_drop_what_is_not_localeds_signal() {
    let (localed, mut vtium) = watching();
    localed.send(&Signal {
        path: c"/org/freedesktop/locale1/other",
        ..localeds()
    });
    localed.send_malformed();
    assert_eq!(vtium.take().map(|t| t.len()).ok(), Some(0));
}

/// A signal addressed to vtium arrives past the match, and the handler
/// records its destination, so it moves nothing.
#[test]
fn a_signal_addressed_to_vtium_is_recorded_as_unicast() {
    let (localed, mut vtium) = watching();
    localed.send(&Signal {
        sender: c":1.99",
        destination: Some(c":1.100"),
        ..localeds()
    });
    let taken = vtium.take().unwrap_or_else(|e| panic!("take: {e}"));
    assert_eq!(
        taken
            .iter()
            .map(|c| c.destination.clone())
            .collect::<Vec<_>>(),
        [Some(":1.100".to_owned())]
    );
    assert!(!taken.iter().any(Changed::moves_layout));
}

/// When the other end closes, `take` says the connection failed.
#[test]
fn a_closed_connection_is_an_error() {
    let (localed, mut vtium) = watching();
    drop(localed);
    assert!(vtium.take().is_err(), "the connection is gone");
}

/// The whole follow over the real connection: localed's broadcast gives
/// every keyboard the German layout, and a signal addressed to vtium does
/// not.
// covers: platform.system-layout-follow
#[test]
fn the_follow_takes_localeds_layout_over_a_real_connection() {
    let (localed, vtium) = pair();
    let us = RuleNames {
        layout: "us".to_owned(),
        ..RuleNames::default()
    };
    let (mut input, nodes) = input(&KeymapConfig::default(), &us);
    let applied = input.keymap().compiled().clone();
    let mut follow = LayoutFollow::with(vtium, KeymapConfig::default(), applied)
        .unwrap_or_else(|e| panic!("the match: {e}"));
    let root = root("peer", None);
    let german = || {
        Ok(RuleNames {
            layout: "de".to_owned(),
            ..RuleNames::default()
        })
    };
    localed.send(&Signal {
        sender: c":1.99",
        destination: Some(c":1.100"),
        ..localeds()
    });
    let moved = follow.dispatch_in(Path::new(&root), german, Some(&mut input));
    assert!(matches!(moved, Ok(false)), "{moved:?}");
    localed.send(&localeds());
    let moved = follow.dispatch_in(Path::new(&root), german, Some(&mut input));
    assert!(matches!(moved, Ok(true)), "{moved:?}");
    assert_eq!(
        key_y(&mut input, &nodes),
        [[keysyms::KEY_z], [keysyms::KEY_z]]
    );
}
