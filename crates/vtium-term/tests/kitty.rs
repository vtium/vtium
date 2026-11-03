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

/// Transmits kitty image `id`, 8 by 16 opaque white RGB pixels, one cell,
/// with replies off.
fn transmit(t: &mut Terminal, id: u32) {
    let mut s = format!("\x1b_Ga=t,f=24,s=8,v=16,i={id},q=2;").into_bytes();
    s.extend(b"////".repeat(128));
    s.extend(b"\x1b\\");
    t.input(&s);
}

/// The frame's boxes as (view row, column), sorted.
fn boxes(t: &Terminal) -> Vec<(i64, u32)> {
    let mut b: Vec<(i64, u32)> = t
        .screen()
        .placements()
        .iter()
        .map(|p| (p.row, p.col))
        .collect();
    b.sort_unstable();
    b
}

// ADR-1100 D13: a relative placement's box is its parent's origin plus its
// offset, so the cells vtium-term marks for it are its own rows, `row`
// rows below its root's line, when it is placed and when it goes.
// covers: image.kitty-relative
#[test]
fn a_relative_placement_marks_its_own_rows_when_it_comes_and_goes() {
    let mut t = term(12, 6);
    transmit(&mut t, 1);
    transmit(&mut t, 2);
    t.input(b"\x1b[2;2H\x1b_Ga=p,i=1,p=1,C=1,q=2\x1b\\");
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b_Ga=p,i=2,p=1,P=1,Q=1,H=2,V=3,q=2\x1b\\");
    assert_eq!(boxes(&t), vec![(1, 1), (4, 3)]);
    let damage = t.screen().damage();
    assert!(damage.changed(4, 3, seen), "the child's own cell is marked");
    assert!(!damage.changed(1, 3, seen), "its root's row is not");
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b_Ga=d,d=i,i=2,p=1,q=2\x1b\\");
    assert_eq!(boxes(&t), vec![(1, 1)]);
    let damage = t.screen().damage();
    assert!(
        damage.changed(4, 3, seen),
        "the cell the child left is marked"
    );
    assert!(!damage.changed(1, 3, seen));
}

// ADR-1100 D13 and D8: a child that starts above its parent still draws
// when the parent has left the bottom of a view moved back into the
// scrollback, since the frame is given the lines below the view.
// covers: image.kitty-relative
#[test]
fn a_child_above_a_parent_below_a_scrolled_back_view_is_drawn() {
    let mut t = term(12, 6);
    t.screen_mut().set_scrollback_max(100);
    transmit(&mut t, 1);
    transmit(&mut t, 2);
    t.input(&b"\r\n".repeat(10));
    t.input(b"\x1b[6;1H\x1b_Ga=p,i=1,p=1,C=1,q=2\x1b\\");
    t.input(b"\x1b_Ga=p,i=2,p=1,P=1,Q=1,H=4,V=-3,q=2\x1b\\");
    assert_eq!(boxes(&t), vec![(2, 4), (5, 0)]);
    t.screen_mut().scroll_view_up(2);
    assert_eq!(
        boxes(&t),
        vec![(4, 4)],
        "the parent is below the view and its child is in it"
    );
}
