//! The kitty graphics protocol through the terminal (ADR-1100, D5, D6):
//! the work bound stops an input call inside one APC run of payload, and
//! the stream cut anywhere ends the same.

#![cfg(feature = "images")]

use vtium_term::images::{Kitty, Switches};
use vtium_term::{ImageString, Terminal};

/// A terminal of `cols` by `rows` cells of 8 by 16 pixels that offers
/// kitty graphics: the handler installed, kitty on, a budget of 1 GiB.
fn term(cols: usize, rows: usize) -> Terminal {
    let mut t = Terminal::new(cols, rows, 100);
    t.screen_mut().set_image_limit(1 << 30);
    t.screen_mut().set_image_cell(8, 16).expect("a cell");
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
    t
}

/// Feeds `s` as a host does, a call at a time, and returns the image work
/// each call did.
fn feed_calls(t: &mut Terminal, s: &[u8]) -> Vec<u64> {
    let mut work = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let n = t.input(rest);
        work.push(t.screen().images().work());
        rest = rest.get(n..).expect("in the stream");
    }
    work
}

/// A transmission and placement of `w` by `h` RGBA pixels in one APC
/// string, its payload `AAAA` repeated: transparent black.
fn frame(w: usize, h: usize) -> Vec<u8> {
    let mut s = format!("\x1b_Ga=T,f=32,s={w},v={h},i=1,C=1;").into_bytes();
    s.extend(b"AAAA".repeat(w * h * 4 / 3));
    s.extend(b"\x1b\\");
    s
}

// ADR-1100 D5: one APC run of 22 MB of base64 is more work than one input
// call may do, 2^24, so the bound stops the call inside the run, within a
// byte of it, and the next call takes the rest; whole or cut anywhere, the
// picture lands the same.
// covers: image.kitty
#[test]
fn the_work_bound_stops_an_input_call_inside_an_apc_run() {
    const BOUND: u64 = 1 << 24; // vtium-term's INPUT_WORK_MAX
    let s = frame(2048, 2049);
    let mut t = term(12, 4);
    let work = feed_calls(&mut t, &s);
    assert!(work.len() >= 2, "{} calls", work.len());
    // the pixels counted at the string's end are the only work past it
    let pixels = 2048 * 2049;
    for (call, &w) in work.iter().enumerate() {
        assert!(w <= BOUND + pixels, "call {call} did {w}");
    }
    assert!(work.first().is_some_and(|&w| w <= BOUND + 1));
    assert_eq!(t.screen().placements().len(), 1);
    let mut pieces = term(12, 4);
    for piece in s.chunks(1_000_003) {
        feed_calls(&mut pieces, piece);
    }
    assert!(pieces.screen() == t.screen());
}
