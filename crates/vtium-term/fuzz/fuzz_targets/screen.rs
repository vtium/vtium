//! Any input, parsed by vtium-parse into the screen: no panic, and the input
//! cut where its third byte says gives the screen and the replies it gives
//! whole. Its first two bytes pick the screen's size.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_term::Terminal;

fn fed(cols: usize, rows: usize, pieces: &[&[u8]]) -> Terminal {
    let mut t = Terminal::new(cols, rows, 64);
    t.screen_mut().set_scrollback_max(64);
    for piece in pieces {
        t.input(piece);
    }
    t
}

fuzz_target!(|data: &[u8]| {
    let [c, r, cut, input @ ..] = data else {
        return;
    };
    let (cols, rows) = (1 + usize::from(c % 40), 1 + usize::from(r % 12));
    let at = usize::from(*cut).min(input.len());
    let whole = fed(cols, rows, &[input]);
    let (a, b) = input.split_at(at);
    let cut = fed(cols, rows, &[a, b]);
    assert!(whole.screen() == cut.screen());
    assert_eq!(whole.replies(), cut.replies());
});
