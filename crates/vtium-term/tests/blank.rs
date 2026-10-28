//! The Linux console's blank sequences (console_codes(4)), which the
//! terminal hands the host and nothing else: no reply, and nothing on the
//! screen. `CSI 9 ; n ]` sets the blank timeout and wakes the screen,
//! `CSI 13 ]` wakes it, and `CSI 14 ; n ]` is ignored. RIS returns the
//! timeout to the host's own; DECSTR keeps it.

mod common;

use common::{rows, term};
use vtium_term::{BlankRequest, BlankTimeout, Terminal};

/// What a terminal fed `bytes` hands the host, once.
fn request(bytes: &[u8]) -> Option<BlankRequest> {
    let mut t = term(8, 2, bytes);
    let taken = t.take_blank_request();
    assert_eq!(t.take_blank_request(), None, "{bytes:?} taken twice");
    assert_eq!(t.take_replies(), b"", "{bytes:?} replied");
    taken
}

fn timeout(minutes: u8) -> Option<BlankRequest> {
    Some(BlankRequest {
        timeout: Some(BlankTimeout::Minutes(minutes)),
        wake: true,
    })
}

// console_codes(4): `CSI 9 ; n ]` sets the blank timeout to n minutes.
// The Linux console woke the screen as it set it, and capped n at 60; n
// of 0, or none, turns blanking off.
// covers: ctl.linux-blank
#[test]
fn csi_9_sets_the_timeout_in_minutes_and_wakes_the_screen() {
    assert_eq!(request(b"\x1b[9;5]"), timeout(5));
    assert_eq!(request(b"\x1b[9;60]"), timeout(60));
    assert_eq!(request(b"\x1b[9;61]"), timeout(60));
    assert_eq!(request(b"\x1b[9;4294967295]"), timeout(60));
    assert_eq!(request(b"\x1b[9;0]"), timeout(0));
    assert_eq!(request(b"\x1b[9]"), timeout(0));
    assert_eq!(request(b"\x1b[9;]"), timeout(0));
    assert_eq!(request(b"\x1b[9;5;7]"), timeout(5));
}

#[test]
fn the_last_timeout_set_is_the_one_taken() {
    assert_eq!(request(b"\x1b[9;5]\x1b[13]\x1b[9;7]"), timeout(7));
    let mut t = term(8, 2, b"\x1b[9;5]");
    assert_eq!(t.take_blank_request(), timeout(5));
    t.input(b"\x1b[13]");
    assert_eq!(
        t.take_blank_request(),
        Some(BlankRequest {
            timeout: None,
            wake: true
        }),
        "a timeout taken is not handed again"
    );
}

// console_codes(4): `CSI 13 ]` unblanks the screen.
// covers: ctl.linux-unblank
#[test]
fn csi_13_wakes_the_screen() {
    assert_eq!(
        request(b"\x1b[13]"),
        Some(BlankRequest {
            timeout: None,
            wake: true
        })
    );
    assert_eq!(
        request(b"\x1b[13;5]"),
        Some(BlankRequest {
            timeout: None,
            wake: true
        })
    );
}

// console_codes(4): `CSI 14 ; n ]` sets the VESA powerdown interval.
// vtium has one DPMS step, which `CSI 9 ; n ]` times, so it is accepted and
// ignored (the operator's decision on vtium/vtium#79; the crate's README).
// covers: ctl.linux-powerdown
#[test]
fn csi_14_is_ignored() {
    assert_eq!(request(b"\x1b[14;5]"), None);
    assert_eq!(request(b"\x1b[14]"), None);
}

#[test]
fn other_forms_are_not_the_blank_sequences() {
    for seq in [
        &b"\x1b[]"[..],
        b"\x1b[10;5]",
        b"\x1b[?9;5]",
        b"\x1b[>13]",
        b"\x1b[9;5$]",
        b"\x1b[13 ]",
        b"\x1b[16;250]",
    ] {
        assert_eq!(request(seq), None, "{seq:?}");
    }
}

// The operator's decision on vtium/vtium#79: a program's timeout lasts
// until changed, and RIS returns it to the host's configured one. RIS does
// not wake the screen; DECSTR leaves the timeout alone.
#[test]
fn ris_returns_the_timeout_to_the_hosts() {
    assert_eq!(
        request(b"\x1bc"),
        Some(BlankRequest {
            timeout: Some(BlankTimeout::Configured),
            wake: false
        })
    );
    assert_eq!(
        request(b"\x1b[9;5]\x1bc"),
        Some(BlankRequest {
            timeout: Some(BlankTimeout::Configured),
            wake: true
        })
    );
    assert_eq!(request(b"\x1bc\x1b[9;5]"), timeout(5));
    assert_eq!(request(b"\x1b[!p"), None);
}

#[test]
fn the_blank_sequences_leave_the_screen_alone() {
    let screen = |t: &Terminal| (rows(t.screen()), t.screen().cursor());
    let plain = term(8, 2, b"ab\x1b[1;31mcd");
    for seq in [&b"\x1b[9;5]"[..], b"\x1b[13]", b"\x1b[14;5]"] {
        let mut t = term(8, 2, b"ab\x1b[1;31m");
        t.input(seq);
        t.input(b"cd");
        assert_eq!(screen(&t), screen(&plain), "{seq:?}");
        assert_eq!(
            t.screen().pen(),
            plain.screen().pen(),
            "{seq:?} moved the pen"
        );
    }
}
