//! Any bytes as a vitrified set: no panic. A set the reader accepts has
//! every chunk checked, and at each of its cell sizes, at 1 and 2 times its
//! height, the first 0x300 codepoints and the last drawn in each style,
//! a 1-bit glyph sheared for italic where its strike is not italic,
//! each glyph exactly its cells' size, and each ink box inside its reach:
//! its glyph's cells, `REACH_CELLS` cells past them on each side and
//! `REACH_ROWS` rows above and below them (ADR-1302 D1, D6, ADR-1306 D1,
//! D3).
#![no_main]

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use vtium_font::set::{Set, SetFace};
use vtium_font::{Face, REACH_CELLS, REACH_ROWS, Style};

fuzz_target!(|data: &[u8]| {
    let Ok(set) = Set::from_bytes(data.to_vec()) else {
        return;
    };
    let _ = set.check_chunks();
    let set = Arc::new(set);
    for cell in set.cells() {
        for scale in 1..=2 {
            let face = SetFace::new(set.clone(), cell.height * scale).expect("a face");
            let c = face.cell();
            for ch in (0..0x300).filter_map(char::from_u32).chain(['\u{10ffff}']) {
                for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                    let style = Style { bold, italic };
                    if let Some(g) = face.draw(&[ch], style) {
                        assert_eq!(g.width(), c.width * g.cells().cells());
                        assert_eq!(g.height(), c.height);
                        if let Some(ink) = g.ink() {
                            let across = i64::from(c.width) * i64::from(REACH_CELLS);
                            let down = i64::from(g.height()) * i64::from(REACH_ROWS);
                            let (span, height) = (i64::from(g.width()), i64::from(g.height()));
                            let left = i64::from(ink.left());
                            let right = left + i64::from(ink.width());
                            assert!(left <= 0 && left >= -across, "{left}");
                            assert!(right >= span && right <= span + across, "{right}");
                            let top = i64::from(ink.top());
                            let bottom = top + i64::from(ink.height());
                            assert!(top <= 0 && top >= -down, "{top}");
                            assert!(bottom >= height && bottom <= height + down, "{bottom}");
                            assert_eq!(ink.pixels().len(), (ink.width() * ink.height()) as usize);
                        }
                    }
                }
            }
        }
    }
});
