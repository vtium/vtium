//! XTWINOPS 8 (ctlseqs): a program asks for a text area in characters,
//! which the host takes with `Terminal::take_grid_request` while it turns
//! window operations on, and XTWINOPS 18, the report of the text area's
//! size, which answers the size the host made.

use vtium_term::{Extent, GridRequest, Terminal};

/// A terminal of 10 columns and 5 rows whose host takes XTWINOPS 8.
fn on() -> Terminal {
    let mut t = Terminal::new(10, 5, 10);
    t.set_window_ops(true);
    t
}

/// A request for `rows` and `cols`.
fn asked(rows: Extent, cols: Extent) -> Option<GridRequest> {
    Some(GridRequest { rows, cols })
}

/// Feeds `bytes` whole, as the host does after each return: before each
/// call it acts on a request as a host does, resizing to the cells asked
/// for and keeping a side asked to keep, and it returns the replies.
fn host(t: &mut Terminal, mut bytes: &[u8]) -> Vec<u8> {
    while !bytes.is_empty() {
        if let Some(r) = t.take_grid_request() {
            let side = |e: Extent, now: usize| match e {
                Extent::Cells(n) => n,
                Extent::Keep | Extent::Display => now,
            };
            let (cols, rows) = (t.screen().cols(), t.screen().rows());
            t.resize(side(r.cols, cols), side(r.rows, rows));
        }
        bytes = &bytes[t.input(bytes)..];
    }
    t.take_replies()
}

// covers: window.resize
#[test]
fn a_request_for_a_text_area_waits_for_the_host() {
    let mut t = on();
    let bytes = b"\x1b[8;25;80tA";
    let taken = t.input(bytes);
    assert_eq!(taken, 10, "the input call returns after the request");
    assert!(t.grid_requested());
    assert_eq!(
        t.take_grid_request(),
        asked(Extent::Cells(25), Extent::Cells(80))
    );
    assert_eq!(t.take_grid_request(), None, "taken once");
    assert_eq!(t.take_replies(), b"", "a request gets no reply");
    assert_eq!(
        (t.screen().cols(), t.screen().rows()),
        (10, 5),
        "the terminal keeps its size until the host resizes it"
    );
}

#[test]
fn an_omitted_side_keeps_its_length_and_zero_takes_the_displays() {
    let mut t = on();
    for (bytes, want) in [
        (
            b"\x1b[8;;80t".as_slice(),
            asked(Extent::Keep, Extent::Cells(80)),
        ),
        (b"\x1b[8;25t", asked(Extent::Cells(25), Extent::Keep)),
        (b"\x1b[8t", asked(Extent::Keep, Extent::Keep)),
        (b"\x1b[8;0;80t", asked(Extent::Display, Extent::Cells(80))),
        (b"\x1b[8;0;0t", asked(Extent::Display, Extent::Display)),
    ] {
        t.input(bytes);
        assert_eq!(t.take_grid_request(), want, "{bytes:?}");
    }
}

#[test]
fn a_later_request_replaces_an_earlier_one() {
    let mut t = on();
    let mut bytes = b"\x1b[8;25;80t\x1b[8;24;100t".as_slice();
    while !bytes.is_empty() {
        bytes = &bytes[t.input(bytes)..];
    }
    assert_eq!(
        t.take_grid_request(),
        asked(Extent::Cells(24), Extent::Cells(100))
    );
}

// ctlseqs: "These controls may be disabled using the allowWindowOps
// resource."
// covers: window.resize
#[test]
fn a_request_is_ignored_until_the_host_turns_window_operations_on() {
    let mut t = Terminal::new(10, 5, 10);
    let bytes = b"\x1b[8;25;80tA";
    assert_eq!(t.input(bytes), bytes.len(), "nothing stops the input");
    assert!(!t.grid_requested());
    assert_eq!(t.take_grid_request(), None);
    assert_eq!(t.take_replies(), b"");
    t.set_window_ops(true);
    t.input(b"\x1b[8;25;80t");
    t.set_window_ops(false);
    assert_eq!(t.take_grid_request(), None, "turning off drops a request");
    t.input(b"\x1b[8;25;80t");
    assert_eq!(t.take_grid_request(), None, "off, a request is ignored");
}

// ctlseqs: CSI 18 t reports the size of the text area in characters, as
// CSI 8 ; height ; width t. After a request in the same write, it answers
// the size the host made.
// covers: window.size-report
#[test]
fn the_size_report_after_a_request_answers_the_size_the_host_made() {
    let mut t = on();
    assert_eq!(host(&mut t, b"\x1b[18t"), b"\x1b[8;5;10t");
    assert_eq!(host(&mut t, b"\x1b[8;3;7t\x1b[18t"), b"\x1b[8;3;7t");
    assert_eq!(host(&mut t, b"\x1b[8;;9t\x1b[18t"), b"\x1b[8;3;9t");
    // Off, the request changes nothing, and the report answers as before.
    t.set_window_ops(false);
    assert_eq!(host(&mut t, b"\x1b[8;6;12t\x1b[18t"), b"\x1b[8;3;9t");
}
