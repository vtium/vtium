//! Any input, as the data of a DCS `q` string the sixel handler takes,
//! after the sixel modes and the string's parameters the input's second
//! byte chooses: no panic, and the string cut where the input's first byte
//! says gives the screen, its pictures and the replies it gives whole.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_term::images::{Sixel, Switches};
use vtium_term::{ImageString, Terminal};

fn fed(pieces: &[&[u8]]) -> Terminal {
    let mut t = Terminal::new(20, 6, 16);
    t.screen_mut().set_image_limit(1 << 24);
    t.set_image_switches(Switches {
        sixel: true,
        kitty: false,
        iterm2: false,
    });
    t.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
    for piece in pieces {
        let mut rest = *piece;
        while !rest.is_empty() {
            let taken = t.input(rest);
            if taken == 0 {
                break;
            }
            rest = rest.get(taken..).unwrap_or_default();
        }
    }
    t
}

/// The modes before the string, and its parameters, from the bits of
/// `choice`: DECSDM, mode 1070 reset, mode 8452, a scroll region, the
/// cursor near the bottom, P2 of 1, and a second picture after the first.
fn stream(choice: u8, payload: &[u8]) -> Vec<u8> {
    let mut s = Vec::new();
    let on = |bit: u8| choice & (1 << bit) != 0;
    if on(0) {
        s.extend_from_slice(b"\x1b[?80h");
    }
    if on(1) {
        s.extend_from_slice(b"\x1b[?1070l");
    }
    if on(2) {
        s.extend_from_slice(b"\x1b[?8452h");
    }
    if on(3) {
        s.extend_from_slice(b"\x1b[2;5r");
    }
    if on(4) {
        s.extend_from_slice(b"\x1b[5;19H");
    }
    let params: &[u8] = if on(5) { b"0;1" } else { b"" };
    let picture = |s: &mut Vec<u8>| {
        s.extend_from_slice(b"\x1bP");
        s.extend_from_slice(params);
        s.push(b'q');
        s.extend_from_slice(payload);
        s.extend_from_slice(b"\x1b\\");
    };
    picture(&mut s);
    if on(6) {
        picture(&mut s);
    }
    s.extend_from_slice(b"\x1b[6n");
    s
}

fuzz_target!(|data: &[u8]| {
    let [cut, choice, payload @ ..] = data else {
        return;
    };
    let s = stream(*choice, payload);
    let at = (usize::from(*cut) * s.len() / 256).min(s.len());
    let whole = fed(&[&s]);
    let (a, b) = s.split_at(at);
    let cut = fed(&[a, b]);
    assert!(whole.screen() == cut.screen());
    assert_eq!(whole.replies(), cut.replies());
    let pixels = |t: &Terminal| -> Vec<Vec<u32>> {
        let p = t.screen().placements();
        p.iter().map(|p| p.pixels.as_slice().to_vec()).collect()
    };
    assert_eq!(pixels(&whole), pixels(&cut));
});
