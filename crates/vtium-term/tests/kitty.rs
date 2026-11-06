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

/// The first chunk of an upload of kitty image 5, 2 by 2 pixels of blue
/// RGB, placed with `a=T` at the cursor, which stays where it is.
const FIRST: &[u8] = b"\x1b_Ga=T,f=24,s=2,v=2,i=5,C=1,q=2,m=1;AAD/AAD/\x1b\\";
/// Its last chunk.
const LAST: &[u8] = b"\x1b_Gm=0;AAD/AAD/\x1b\\";

/// The upload's first chunk, then `between`, then its last chunk: whether
/// the picture was placed, and whether the store holds any bytes.
fn upload_around(t: &mut Terminal, between: &mut dyn FnMut(&mut Terminal)) -> (usize, u64) {
    t.input(FIRST);
    between(t);
    t.input(LAST);
    (t.screen().placements().len(), t.screen().images().bytes())
}

// Two chunks back to back draw, as kitty-doom sends them; anything else
// that arrives between two chunks ends the upload, whose last chunk is then
// a command of its own, a transmission without s and v, refused unanswered.
// covers: image.kitty
#[test]
fn output_between_chunks_ends_an_upload_and_the_next_command_starts_fresh() {
    let mut t = term(12, 4);
    assert_eq!(upload_around(&mut t, &mut |_| {}).0, 1, "no gap: drawn");
    let between: [&[u8]; 11] = [
        b"x",
        b"\r\n",
        b"\x07",
        b"\x1b[H",
        b"\x1b7",
        b"\x1b]0;title\x1b\\",
        b"\x1bP$qm\x1b\\",
        b"\x1bP+q544e\x1b\\",
        b"\x1b_Xother\x1b\\",
        // an APC string whose first character is past ASCII, which the
        // parser hands over a character at a time, and an empty one
        "\x1b_\u{e9}t\x1b\\".as_bytes(),
        b"\x1b_\x1b\\",
    ];
    for bytes in between {
        let mut t = term(12, 4);
        let got = upload_around(&mut t, &mut |t| {
            t.input(bytes);
        });
        assert_eq!(got, (0, 0), "{bytes:?} between the chunks");
        t.input(b"\x1b_Ga=T,f=24,s=2,v=2,i=6,C=1;AAD/AAD/AAD/AAD/\x1b\\");
        assert_eq!(
            t.screen().placements().len(),
            1,
            "{bytes:?}: the next upload is fresh"
        );
    }
}

// covers: image.kitty
#[test]
fn ris_ends_an_upload() {
    let mut t = term(12, 4);
    let got = upload_around(&mut t, &mut |t| {
        t.input(b"\x1bc");
    });
    assert_eq!(got, (0, 0));
}

// covers: image.kitty
#[test]
fn the_session_ending_ends_an_upload() {
    let mut t = term(12, 4);
    let got = upload_around(&mut t, &mut Terminal::session_ended);
    assert_eq!(got, (0, 0));
}

// With 8-bit controls, a program may end each chunk with the 8-bit ST,
// 0x9C, which the terminal also runs as a control after the string: it is
// no other output, and the upload goes on.
// covers: image.kitty
#[test]
fn an_8_bit_st_ending_each_chunk_leaves_the_upload_open() {
    let mut t = term(12, 4);
    // DECSCL: level 2, with 8-bit controls
    t.input(b"\x1b[62;0\"p");
    t.input(b"\x1b_Ga=T,f=24,s=2,v=2,i=5,C=1,q=2,m=1;AAD/AAD/\x9c");
    t.input(b"\x1b_Gm=0;AAD/AAD/\x9c");
    assert_eq!(t.screen().placements().len(), 1);
}
