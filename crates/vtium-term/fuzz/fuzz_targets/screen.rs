//! Any input, parsed by vtium-parse into the screen: no panic, and the input
//! cut where its third byte says gives the screen, the replies and the frame
//! it gives whole. The input cut the same way while the host holds the
//! replies, released at the end, gives the same screen and the same
//! replies, since nothing changes the sizes in between. Its first two bytes
//! pick the screen's size. The frame is
//! drawn in the order the bidirectional algorithm gives (ADR-1004 D8), so
//! the target drives the guarded call into unicode-bidi (D13), and the
//! pointer is mapped back through it on every row.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_term::{Age, Glyph, Palette, Terminal};

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
    assert_eq!(drawn(&whole), drawn(&cut));
    let mut held = Terminal::new(cols, rows, 64);
    held.screen_mut().set_scrollback_max(64);
    held.hold_replies();
    // A held size report weighs more than its reply, so a flood of them
    // can stop the held input before the whole one stops.
    let took = held.input(a) == a.len() && held.input(b) == b.len();
    assert!(held.replies().is_empty(), "every reply waits in the hold");
    held.release_replies();
    if took {
        assert!(whole.screen() == held.screen());
        assert_eq!(whole.replies(), held.replies());
    }
    for row in 0..rows {
        let (col, _) = whole.pointer_cell(usize::from(*c) % cols, row, 0);
        assert!(col < cols);
    }
});

/// Each row as the frame draws it, a character a cell, and the cursor.
fn drawn(t: &Terminal) -> (Vec<String>, (usize, usize)) {
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, Age::NEVER);
    let rows = (0..frame.rows())
        .map(|r| {
            frame
                .cells(r)
                .map(|c| match c.glyph {
                    Glyph::Empty => ' ',
                    Glyph::Char(ch) => ch,
                    Glyph::Cluster(s) => s.chars().next().unwrap_or(' '),
                })
                .collect()
        })
        .collect();
    let cursor = frame.cursor();
    (rows, (cursor.row, cursor.col))
}
