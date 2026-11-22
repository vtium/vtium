//! The host's hold of the replies (`Terminal::hold_replies`): while the
//! host holds them, the terminal goes on parsing and drawing the program's
//! output, and the replies to its queries wait. At the release they go out
//! in the order the program asked, the size reports of XTWINOPS 14, 16, 18
//! and 19 answered from the sizes the terminal has then. The held replies
//! count toward `REPLIES_PENDING_MAX`, so a program that queries in a loop
//! during the hold cannot grow them past it.

mod common;

use common::{rows, term};
use vtium_term::{REPLIES_PENDING_MAX, Terminal};

/// Feeds `bytes` until the terminal takes no more of them, and returns how
/// many it took.
fn fed(t: &mut Terminal, bytes: &[u8]) -> usize {
    let mut taken = 0;
    loop {
        let n = t.input(bytes.get(taken..).unwrap_or_default());
        if n == 0 {
            return taken;
        }
        taken += n;
    }
}

// The host holds the replies while the cell changes from 8×8 to 10×8 and
// the grid from 10 to 8 columns. The text the program writes during the
// hold is drawn at once. At the release the size reports give the new
// cell, text area and grid, and the cursor report the place the cursor
// had when the program asked, all in the order the program asked.
// covers: window.size-report
#[test]
fn held_replies_go_out_in_order_with_the_sizes_at_the_release() {
    let mut t = term(10, 4, b"");
    t.set_cell_size(8, 8);
    t.hold_replies();
    let program = b"ab\x1b[16t\x1b[6n\x1b[14t\x1b[18t\x1b[19tcd";
    assert_eq!(
        fed(&mut t, program),
        program.len(),
        "the output is all read"
    );
    assert_eq!(rows(t.screen()).first().map(String::as_str), Some("abcd"));
    assert_eq!(t.replies(), b"", "the replies wait");
    t.set_cell_size(10, 8);
    t.resize(8, 4);
    assert_eq!(t.replies(), b"", "the replies wait past the resize");
    assert!(t.replies_held());
    t.release_replies();
    assert!(!t.replies_held());
    assert_eq!(
        t.take_replies(),
        b"\x1b[6;8;10t\x1b[1;3R\x1b[4;32;80t\x1b[8;4;8t\x1b[9;4;8t"
    );
    assert_eq!(fed(&mut t, b"\x1b[16t"), 5);
    assert_eq!(
        t.take_replies(),
        b"\x1b[6;8;10t",
        "the replies go out at once"
    );
}

// A cell size the host had not set when the program asked is answered
// once the host has set it by the release.
#[test]
fn a_held_cell_report_answers_a_cell_set_during_the_hold() {
    let mut t = term(10, 4, b"");
    t.hold_replies();
    t.input(b"\x1b[16t\x1b[14t");
    t.set_cell_size(9, 18);
    t.release_replies();
    assert_eq!(t.take_replies(), b"\x1b[6;18;9t\x1b[4;72;90t");
}

// What a paste sends reaches the program during the hold, ahead of the
// replies the program asked for before it.
#[test]
fn a_paste_does_not_wait_for_the_release() {
    let mut t = term(10, 4, b"");
    t.hold_replies();
    t.input(b"\x1b[5n");
    t.paste(b"x");
    assert_eq!(t.take_replies(), b"x");
    t.release_replies();
    assert_eq!(t.take_replies(), b"\x1b[0n");
}

// A program that asks for the cell in a loop during the hold stops the
// input call once the held reports weigh REPLIES_PENDING_MAX, 48 bytes
// each: 1,366 reports. The rest of its output waits until the host has
// released and taken them. The same bound holds for replies the terminal
// writes at once, such as DSR 5's four bytes: 16,384 of them.
#[test]
fn a_flood_of_queries_during_the_hold_stops_at_the_bound() {
    assert_eq!(REPLIES_PENDING_MAX, 65_536);
    let mut t = term(10, 4, b"");
    t.set_cell_size(8, 8);
    t.hold_replies();
    let flood = b"\x1b[16t".repeat(2000);
    assert_eq!(fed(&mut t, &flood), 1366 * 5);
    assert_eq!(t.replies(), b"");
    t.release_replies();
    assert_eq!(t.take_replies(), b"\x1b[6;8;8t".repeat(1366));
    assert_eq!(fed(&mut t, &flood), flood.len(), "read again once taken");

    let mut t = term(10, 4, b"");
    t.hold_replies();
    let flood = b"\x1b[5n".repeat(20_000);
    assert_eq!(fed(&mut t, &flood), 16_384 * 4);
    t.release_replies();
    assert_eq!(t.take_replies(), b"\x1b[0n".repeat(16_384));
}

// The host's reset drops the held replies with the program that asked
// for them, and the host goes on holding.
#[test]
fn a_reset_drops_the_held_replies_and_keeps_the_hold() {
    let mut t = term(10, 4, b"");
    t.set_cell_size(8, 8);
    t.hold_replies();
    t.input(b"\x1b[5n\x1b[16t");
    t.reset();
    assert!(t.replies_held());
    t.input(b"\x1b[18t");
    t.release_replies();
    assert_eq!(t.take_replies(), b"\x1b[8;4;10t");
}
