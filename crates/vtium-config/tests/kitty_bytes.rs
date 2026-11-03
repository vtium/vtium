//! The TUI's size pictures read by the console's own terminal built with
//! `images` (ADR-1201 D4, D8): its graphics query is answered, a picture
//! sent in one command or in chunks makes one placement, and the delete
//! removes it. The workspace's `--all-features` run turns on the
//! `kitty-check` feature that builds vtium-term so.

#![cfg(feature = "kitty-check")]

use vtium_config::kitty;
use vtium_config::tui::model;
use vtium_term::images::{Kitty, Switches};
use vtium_term::{ImageString, Terminal};

/// vtium's terminal with kitty graphics on, as a console built with
/// `images` sets it up.
fn term() -> Terminal {
    let mut t = Terminal::new(80, 25, 0);
    t.screen_mut().set_image_limit(1 << 26);
    t.screen_mut().set_image_cell(8, 16).expect("a cell");
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
    t
}

fn feed(t: &mut Terminal, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = t.input(bytes);
        bytes = &bytes[n..];
    }
}

#[test]
fn the_graphics_query_is_answered_before_da1() {
    let mut t = term();
    feed(&mut t, &model::probe());
    let replies = String::from_utf8_lossy(&t.take_replies()).into_owned();
    let ok = replies
        .find(&format!("\x1b_Gi={};OK\x1b\\", model::QUERY_ID))
        .expect("the query answered");
    let da1 = replies.find("\x1b[?").expect("DA1 answered");
    assert!(ok < da1, "{replies:?}");
    assert_eq!(t.screen().placements().len(), 0, "the query keeps nothing");
}

#[test]
fn a_picture_makes_one_placement_and_the_delete_removes_it() {
    for (w, h) in [(4u32, 2u32), (40, 40)] {
        // 40x40 is 6400 bytes, 8536 of base64, three chunks
        let rgba = vec![200u8; (w * h * 4) as usize];
        let show = kitty::show(kitty::IMAGE_ID, w, h, &rgba);
        let chunks = String::from_utf8_lossy(&show).matches("\x1b_G").count();
        assert_eq!(chunks, if w == 4 { 1 } else { 3 });
        let mut t = term();
        feed(&mut t, b"\x1b[4;26H");
        feed(&mut t, &show);
        assert_eq!(t.screen().placements().len(), 1, "{w}x{h}");
        assert!(t.take_replies().is_empty(), "q=2 asks for no reply");
        assert_eq!(t.screen().cursor(), (25, 3), "C=1 leaves the cursor");
        feed(&mut t, &kitty::delete(kitty::IMAGE_ID));
        assert_eq!(t.screen().placements().len(), 0, "{w}x{h}");
    }
}
