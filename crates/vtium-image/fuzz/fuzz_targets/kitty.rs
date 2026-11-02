//! Any input, as the data of an APC `G` string the kitty graphics handler
//! takes: no panic, and the string cut where the input's first byte says
//! gives the screen and the replies it gives whole.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_term::images::{Kitty, Switches};
use vtium_term::{ImageString, Terminal};

fn fed(pieces: &[&[u8]]) -> Terminal {
    let mut t = Terminal::new(20, 6, 16);
    t.screen_mut().set_image_limit(1 << 24);
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
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

fuzz_target!(|data: &[u8]| {
    let [cut, payload @ ..] = data else {
        return;
    };
    let mut string = b"\x1b_G".to_vec();
    string.extend_from_slice(payload);
    string.extend_from_slice(b"\x1b\\");
    let at = usize::from(*cut).min(string.len());
    let whole = fed(&[&string]);
    let (a, b) = string.split_at(at);
    let cut = fed(&[a, b]);
    assert!(whole.screen() == cut.screen());
    assert_eq!(whole.replies(), cut.replies());
});
