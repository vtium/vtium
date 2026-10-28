//! Any bytes as a vitrified set: no panic. A set the reader accepts has
//! every chunk checked, and at each of its cell sizes, at 1 and 2 times its
//! height, the first 0x300 codepoints and the last drawn plain and bold,
//! each glyph exactly its cells' size.
#![no_main]

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use vtium_font::set::{Set, SetFace};
use vtium_font::{Face, Style};

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
                for bold in [false, true] {
                    let style = Style {
                        bold,
                        italic: false,
                    };
                    if let Some(g) = face.draw(&[ch], style) {
                        assert_eq!(g.width(), c.width * g.cells().cells());
                        assert_eq!(g.height(), c.height);
                    }
                }
            }
        }
    }
});
