//! Any bytes as a vitrified set: no panic. A set the reader accepts has
//! every chunk checked, and at each of its cell sizes, at 1 and 2 times its
//! height, the first 0x300 codepoints and the last drawn in each style,
//! a 1-bit glyph sheared for italic where its strike is not italic,
//! each glyph exactly its cells' size, and each ink box inside its reach:
//! its glyph's cells, `REACH_CELLS` cells past them on each side and
//! `REACH_ROWS` rows above and below them (ADR-1302 D1, D6, ADR-1306 D1,
//! D3). Each glyph is drawn as superscript and subscript too, from the
//! face's origin for it (ADR-1202): each the glyph's own size and cells,
//! a 1-bit strike's only fully on or off, any ink box beside the cells
//! inside the reach and no taller than the row (D8), and a color glyph
//! still premultiplied. Each codepoint is drawn from the set's superscript
//! and subscript strikes too (ADR-1202 D11), each glyph its cells' size.
#![no_main]

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use vtium_font::set::{Set, SetFace};
use vtium_font::{Face, Format, Glyph, Kind, REACH_CELLS, REACH_ROWS, Script, Style, Variant};

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
            // every line of every script's set lies inside the cell, a row
            // thick at least, the overline as thick as the underline
            // (ADR-1202 D8)
            for script in [Script::Baseline, Script::Super, Script::Sub] {
                let d = face.script_decorations(script);
                assert!(d.underline_thickness >= 1 && d.strike_thickness >= 1);
                assert!(d.underline + d.underline_thickness <= c.height);
                assert!(d.strike + d.strike_thickness <= c.height);
                assert!(d.overline + d.underline_thickness <= c.height);
            }
            for ch in (0..0x300).filter_map(char::from_u32).chain(['\u{10ffff}']) {
                for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                    let style = Style { bold, italic };
                    for script in [Script::Super, Script::Sub] {
                        if let Some(g) = face.draw_variant(&[ch], Variant { style, script }) {
                            assert_eq!(g.width(), c.width * g.cells().cells());
                            assert_eq!(g.height(), c.height);
                        }
                    }
                    if let Some(g) = face.draw(&[ch], style) {
                        assert_eq!(g.width(), c.width * g.cells().cells());
                        assert_eq!(g.height(), c.height);
                        let origin = face.origin(&[ch], style);
                        let upright = face.draw(&[ch], style.without(origin.synth)).expect("the glyph upright");
                        for script in [Script::Super, Script::Sub] {
                            let s = upright.scripted(script, &origin, face.band()).expect("a scaled glyph");
                            assert_eq!((s.width(), s.height()), (g.width(), g.height()));
                            assert_eq!((s.cells(), s.format()), (g.cells(), g.format()));
                            if origin.kind == Kind::Bitmap {
                                assert!(s.pixels().iter().all(|&p| p == 0 || p == 0xff));
                            }
                            if let Some(ink) = s.ink() {
                                let across = i64::from(c.width) * i64::from(REACH_CELLS);
                                assert_eq!((ink.top(), ink.height()), (0, s.height()));
                                assert!(i64::from(ink.left()) >= -across);
                                assert!(i64::from(ink.left()) + i64::from(ink.width()) <= i64::from(s.width()) + across);
                            }
                            if s.format() == Format::Rgba {
                                let px = s.pixels().to_vec();
                                assert!(Glyph::from_pixels(s.width(), s.height(), Format::Rgba, px).is_some());
                            }
                        }
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
