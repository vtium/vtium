//! The kitty graphics protocol through the terminal (ADR-1100, D5, D6):
//! the work bound stops an input call inside one APC run of payload, and
//! the stream cut anywhere ends the same.

#![cfg(feature = "images")]

use vtium_term::images::decode::{Payload, Reply};
use vtium_term::images::{Cause, Kitty, Limits, Switches};
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

// A program can send data past s and v with every frame, so vtium's
// handler notes it for the log once for each upload, and answers as if it
// had not come.
#[test]
fn data_past_the_pixels_is_noted_for_the_log_and_answered_ok() {
    let mut t = term(12, 4);
    let past = "a kitty upload's data past the pixels s and v call for, ignored";
    // one RGB pixel is 3 bytes; AAAAAAAA decodes to 6
    t.input(b"\x1b_Ga=t,f=24,s=1,v=1,i=1;AAAAAAAA\x1b\\");
    assert_eq!(t.take_replies(), b"\x1b_Gi=1;OK\x1b\\");
    let notes = t.take_image_notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(
        (
            notes[0].protocol,
            notes[0].cause,
            notes[0].why.as_str(),
            notes[0].count
        ),
        (ImageString::Kitty, Cause::Program, past, 1)
    );
    // a second is counted, and reaches no log
    t.input(b"\x1b_Ga=t,f=24,s=1,v=1,i=1;AAAAAAAA\x1b\\");
    assert_eq!(t.take_image_notes(), []);
    // the query counts the bytes and notes the same
    let mut t = term(12, 4);
    t.input(b"\x1b_Ga=q,f=24,s=1,v=1,i=2;AAAAAAAA\x1b\\");
    assert_eq!(t.take_replies(), b"\x1b_Gi=2;OK\x1b\\");
    assert_eq!(t.take_image_notes().len(), 1);
    // in chunks, the bytes past arriving in the last
    let mut t = term(12, 4);
    t.input(b"\x1b_Ga=t,f=24,s=1,v=1,i=3,m=1;AAAA\x1b\\\x1b_Gm=0;AAAA\x1b\\");
    assert_eq!(t.take_replies(), b"\x1b_Gi=3;OK\x1b\\");
    assert_eq!(t.take_image_notes().len(), 1);
    // exactly the pixels: no note
    let mut t = term(12, 4);
    t.input(b"\x1b_Ga=t,f=24,s=1,v=1,i=1;AAAA\x1b\\");
    t.input(b"\x1b_Ga=q,f=24,s=1,v=1,i=2;AAAA\x1b\\");
    assert_eq!(t.take_image_notes(), []);
}

// A program may ask for no answer, with `q=2` or no id, so a picture the
// budget refuses is noted for the log whether or not the program hears
// ENOSPC: the picture never draws, and the operator can raise the budget.
#[test]
fn a_picture_the_budget_refuses_is_noted_even_unanswered() {
    let mut t = term(12, 4);
    t.screen_mut().set_image_limit(4096);
    let mut s = b"\x1b_Ga=t,f=32,s=64,v=64,q=2;".to_vec();
    s.extend(b"AAAA".repeat(64 * 64 * 4 / 3));
    s.extend(b"\x1b\\");
    t.input(&s);
    assert_eq!(t.take_replies(), b"");
    let notes: Vec<_> = t
        .take_image_notes()
        .into_iter()
        .map(|n| (n.cause, n.why))
        .collect();
    assert_eq!(
        notes,
        [(
            Cause::Console,
            "the budget refuses a kitty picture".to_owned()
        )]
    );
}

/// The notes the kitty handler wrote, as (cause, reason).
fn kitty_notes(t: &mut Terminal) -> Vec<(Cause, String)> {
    t.take_image_notes()
        .into_iter()
        .map(|n| {
            assert_eq!(n.protocol, ImageString::Kitty);
            (n.cause, n.why)
        })
        .collect()
}

/// A terminal with an image helper, which has taken one PNG upload's
/// request, `q=2` so the program hears nothing.
fn requested() -> Terminal {
    let mut t = term(12, 4);
    t.set_image_helper(true);
    t.input(b"\x1b_Ga=t,f=100,i=1,q=2;iVBORw0KGgo=\x1b\\");
    assert!(t.take_image_request().is_some());
    t
}

// A helper that fails leaves a picture that never draws, which the operator
// can act on; a request the console drops when its program has gone is no
// failure, and notes nothing.
#[test]
fn a_failing_helper_is_noted_and_an_abandoned_request_is_not() {
    let mut t = requested();
    t.image_reply(Reply::Failed);
    assert_eq!(
        kitty_notes(&mut t),
        [(
            Cause::Console,
            "the image helper fails on a kitty picture".to_owned()
        )]
    );
    let mut t = requested();
    t.image_reply(Reply::TooLarge);
    assert_eq!(
        kitty_notes(&mut t),
        [(
            Cause::Console,
            "the image helper finds a kitty picture past its bounds".to_owned()
        )]
    );
    let mut t = requested();
    t.abandon_image_request();
    assert!(!t.image_request_pending());
    assert_eq!(t.take_replies(), b"");
    assert_eq!(kitty_notes(&mut t), []);
    assert_eq!(
        t.screen().images().bytes(),
        0,
        "the upload's bytes are given back"
    );
}

// ADR-1005, D10: the host's reset abandons a request out to the image
// helper, as the host's abandon does, so the terminal takes the program's
// output again at once; nothing is answered or noted.
#[test]
fn a_host_reset_abandons_the_request_out() {
    let mut t = requested();
    assert_eq!(t.input(b"x"), 0, "the request holds the input");
    t.reset();
    assert!(!t.image_request_pending());
    assert_eq!(t.take_replies(), b"");
    assert_eq!(kitty_notes(&mut t), []);
    assert_eq!(t.screen().images().bytes(), 0);
    assert_eq!(t.input(b"x"), 1, "the terminal takes input again");
}

// A picture past `max-graphic-size`, or a box past `BOX_CELLS_MAX`, is
// refused with EINVAL, which a program with `q=2` never hears, so the
// handler notes the bound the operator can raise.
#[test]
fn a_picture_past_vtiums_bounds_is_noted() {
    let mut t = term(12, 4);
    t.set_image_limits(Limits {
        graphic: (8, 8),
        ..Limits::default()
    });
    t.input(b"\x1b_Ga=t,f=24,s=16,v=1,q=2;AAAA\x1b\\");
    assert_eq!(
        kitty_notes(&mut t),
        [(
            Cause::Console,
            "a kitty picture past max-graphic-size".to_owned()
        )]
    );
    // a key the program got wrong is its own, and notes nothing
    t.input(b"\x1b_Ga=t,f=24,i=1,I=2,s=1,v=1,q=2;AAAA\x1b\\");
    assert_eq!(kitty_notes(&mut t), []);
    t.input(b"\x1b_Ga=t,f=24,s=1,v=1,i=5,q=2;AAAA\x1b\\");
    t.input(b"\x1b_Ga=p,i=5,c=10001,r=1,q=2\x1b\\");
    assert_eq!(
        kitty_notes(&mut t),
        [(
            Cause::Console,
            "a kitty picture's box passes BOX_CELLS_MAX".to_owned()
        )]
    );
    // a decoded picture past a `max-graphic-size` lowered since its request
    let mut t = requested();
    t.set_image_limits(Limits {
        graphic: (8, 8),
        ..Limits::default()
    });
    t.image_reply(Reply::Ok(Payload::Rgba {
        width: 16,
        height: 16,
        data: vec![0; 16 * 16 * 4],
    }));
    assert_eq!(
        kitty_notes(&mut t),
        [(
            Cause::Console,
            "a kitty picture past max-graphic-size".to_owned()
        )]
    );
}

/// Opens an upload of image 1, a 2×1 RGB picture, in its first chunk.
const OPEN1: &[u8] = b"\x1b_Ga=t,f=24,s=2,v=1,i=1,m=1;/wAA\x1b\\";
/// The upload's last chunk.
const LAST1: &[u8] = b"\x1b_Gm=0;AP8A\x1b\\";
/// A put of image 1 that leaves the cursor where it is.
const PUT1: &[u8] = b"\x1b_Ga=p,i=1,C=1\x1b\\";
const OK1: &[u8] = b"\x1b_Gi=1;OK\x1b\\";
const NOENT1: &[u8] =
    b"\x1b_Gi=1;ENOENT:Put command refers to non-existent image with id: 1 and number: 0\x1b\\";

/// The replies after `OPEN1`, then `between`, then `LAST1` and `PUT1`.
fn around(t: &mut Terminal, between: &[u8]) -> Vec<u8> {
    t.input(OPEN1);
    t.input(between);
    t.input(LAST1);
    t.input(PUT1);
    t.take_replies()
}

// ADR-1100 D16: text, controls, CSI, OSC, DCS and an APC string that is
// not a kitty command leave an open upload as it was; DECRQSS is answered
// in its place among the upload's replies.
// covers: image.kitty
#[test]
fn output_between_chunks_leaves_the_upload_open() {
    let mut t = term(12, 4);
    let between = b"hello\x1b[3;3H\x1b[31mX\x1b[m\x1b]2;title\x07\x1b_Xjunk\x1b\\\x1b[!p";
    assert_eq!(around(&mut t, between), [OK1, OK1].concat());
    let mut t = term(12, 4);
    let replies = around(&mut t, b"\x1bP$qm\x1b\\");
    assert!(replies.ends_with(&[OK1, OK1].concat()), "{replies:?}");
    assert!(replies.starts_with(b"\x1bP1$r"), "DECRQSS answers first");
}

// ADR-1100 D16: a sixel between kitty chunks draws as always and leaves
// the kitty upload open.
// covers: image.kitty
#[test]
fn a_sixel_between_chunks_leaves_the_upload_open() {
    let mut t = term(12, 4);
    t.set_image_switches(Switches {
        sixel: true,
        kitty: true,
        iterm2: false,
    });
    t.set_image_handler(
        ImageString::Sixel,
        Some(Box::new(vtium_term::images::Sixel::default())),
    );
    let sixel = b"\x1bP0;0;0q\"1;1;1;1#0;2;100;0;0#0~\x1b\\";
    assert_eq!(around(&mut t, sixel), [OK1, OK1].concat());
    assert_eq!(t.screen().placements().len(), 2, "the sixel and image 1");
}

// ADR-1100 D16: RIS ends the open upload with no answer, and the session's
// end drops it silently; either way its reservation goes back.
// covers: image.kitty
#[test]
fn ris_and_the_sessions_end_drop_the_upload() {
    let mut t = term(12, 4);
    assert_eq!(around(&mut t, b"\x1bc"), NOENT1);
    let mut t = term(12, 4);
    t.input(OPEN1);
    assert_eq!(t.screen().images().bytes(), 8 + 512);
    t.session_ended();
    assert_eq!(t.screen().images().bytes(), 0);
    t.input(LAST1);
    t.input(PUT1);
    assert_eq!(t.take_replies(), NOENT1);
}

// ADR-1100 D16: ED 2 and the clear of entering the alternate screen leave
// an open upload alone, and an a=T upload places after the clear.
// covers: image.kitty
#[test]
fn a_clear_leaves_the_upload_open() {
    let mut t = term(12, 4);
    assert_eq!(around(&mut t, b"\x1b[2J"), [OK1, OK1].concat());
    let mut t = term(12, 4);
    t.input(b"\x1b_Ga=T,f=24,s=2,v=1,i=1,c=4,r=2,C=1,m=1;/wAA\x1b\\");
    t.input(b"\x1b[2J");
    t.input(LAST1);
    assert_eq!(t.take_replies(), OK1);
    assert_eq!(boxes(&t), [(0, 0)]);
}

// ADR-1100 D16: each screen holds its own upload; a trip to the alternate
// screen and back leaves the main screen's open, and its last chunk sent
// on the alternate screen is a command of its own there.
// covers: image.kitty
#[test]
fn each_screen_holds_its_own_upload() {
    let mut t = term(12, 4);
    assert_eq!(
        around(&mut t, b"\x1b[?1049h\x1b[?1049l"),
        [OK1, OK1].concat()
    );
    let mut t = term(12, 4);
    t.input(OPEN1);
    t.input(b"\x1b[?1049h");
    t.input(LAST1);
    t.input(PUT1);
    t.input(b"\x1b[?1049l");
    t.input(PUT1);
    assert_eq!(t.take_replies(), [NOENT1, NOENT1].concat());
}

// ADR-1100 D6: while kitty graphics is not offered, the start of an APC
// string drops an upload left open, so it holds no budget.
// covers: image.kitty
#[test]
fn kitty_switched_off_drops_the_upload_at_the_next_apc() {
    let mut t = term(12, 4);
    t.input(OPEN1);
    t.set_image_switches(Switches {
        sixel: false,
        kitty: false,
        iterm2: false,
    });
    t.input(b"\x1b_Xjunk\x1b\\");
    assert_eq!(t.screen().images().bytes(), 0);
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.input(LAST1);
    t.input(PUT1);
    assert_eq!(t.take_replies(), NOENT1);
}
