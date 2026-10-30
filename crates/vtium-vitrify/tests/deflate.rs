//! vtium-vitrify links vtium-font's set writer with its deflate, the
//! `deflate` feature, which the console leaves out (ADR-1200 D19).

use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
use vtium_font::{Cell, Style, Width};

// A strike asking compress=yes is stored smaller than the same strike raw
#[test]
fn the_set_writer_deflates_a_chunk_that_gains() {
    let cell = Cell {
        width: 16,
        height: 16,
    };
    let set = |compress| {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(compress);
        let g = s.glyph(Width::Narrow, vec![0; 256]);
        s.map('x', g);
        SetBuilder::new("deflate")
            .strike(s)
            .write()
            .expect("the set writes")
    };
    let (packed, raw) = (set(true), set(false));
    assert!(
        packed.len() < raw.len(),
        "{} B deflated against {} B raw",
        packed.len(),
        raw.len()
    );
}
