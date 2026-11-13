//! setBackground and setForeground (ADR-1000, "Replies and policy"):
//! kmscon's private OSC strings, which the host takes with
//! `Terminal::take_display_request`. Both are ignored until the host turns
//! display operations on.

use vtium_term::{DisplayRequest, Terminal};

/// A terminal whose host takes setBackground and setForeground.
fn on() -> Terminal {
    let mut t = Terminal::new(10, 5, 10);
    t.set_display_ops(true);
    t
}

/// Feeds `bytes` whole, taking each request as the host does after each
/// return, and returns the requests in the order they were taken.
fn feed(t: &mut Terminal, mut bytes: &[u8]) -> Vec<DisplayRequest> {
    let mut taken = Vec::new();
    while !bytes.is_empty() {
        bytes = &bytes[t.input(bytes)..];
        taken.extend(t.take_display_request());
    }
    taken
}

// covers: osc.kmscon-display
#[test]
fn each_string_waits_for_the_host_ended_by_st_or_bel() {
    let mut t = on();
    t.input(b"\x1b]setBackground\x1b\\");
    assert!(t.display_requested());
    assert_eq!(t.take_display_request(), Some(DisplayRequest::Background));
    assert_eq!(t.take_display_request(), None, "taken once");
    assert!(!t.display_requested());
    t.input(b"\x1b]setForeground\x07");
    assert_eq!(t.take_display_request(), Some(DisplayRequest::Foreground));
    assert_eq!(t.take_replies(), b"", "neither string gets a reply");
}

// covers: osc.kmscon-display
#[test]
fn the_input_call_returns_after_each_request_so_the_host_takes_both_in_order() {
    let mut t = on();
    let both = b"\x1b]setBackground\x07\x1b]setForeground\x07after";
    let first = t.input(both);
    assert_eq!(
        first,
        b"\x1b]setBackground\x07".len(),
        "the call returns at the end of the first string"
    );
    assert_eq!(t.take_display_request(), Some(DisplayRequest::Background));
    let rest = feed(&mut t, &both[first..]);
    assert_eq!(rest, [DisplayRequest::Foreground]);
    assert_eq!(
        t.screen().cursor(),
        (5, 0),
        "the text after both is written"
    );
}

// covers: osc.kmscon-display
#[test]
fn with_display_operations_off_both_strings_are_ignored() {
    let mut t = Terminal::new(10, 5, 10);
    let both = b"\x1b]setBackground\x1b\\\x1b]setForeground\x07x";
    assert_eq!(t.input(both), both.len(), "nothing pauses the input");
    assert_eq!(t.take_display_request(), None);
    assert_eq!(
        t.screen().cursor(),
        (1, 0),
        "the text after both is written"
    );
    // Turning them off drops a request the host has not taken.
    let mut t = on();
    t.input(b"\x1b]setBackground\x07");
    t.set_display_ops(false);
    assert_eq!(t.take_display_request(), None);
}

// covers: osc.kmscon-display
#[test]
fn only_the_two_exact_strings_are_requests() {
    let mut t = on();
    for text in [
        &b"setbackground"[..],
        b"setBackground;1",
        b"0;setBackground",
        b"setBackgroundx",
        b" setForeground",
        b"setForegroun",
    ] {
        let mut bytes = b"\x1b]".to_vec();
        bytes.extend_from_slice(text);
        bytes.push(0x07);
        assert_eq!(
            feed(&mut t, &bytes),
            [],
            "{}",
            String::from_utf8_lossy(text)
        );
    }
    // An OSC string cut short is dropped whole, as every OSC is.
    assert_eq!(feed(&mut t, b"\x1b]setBackground\x18"), []);
}
