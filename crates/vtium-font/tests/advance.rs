//! Each face's [`Face::advance`]: the cells a character's glyph spans, as
//! the glyph [`Face::draw`] gives marks them, found without drawing the
//! glyph. A set and the built-in unifont decode a chunk only when a glyph
//! is drawn from it, so a chunk still undecoded after measuring shows that
//! measuring drew nothing. The unifont test is the only one here that
//! touches the built-in unifont, so no other test in this process decodes
//! its chunks first.

mod common;

use std::sync::Arc;

use common::psf2;
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetFace, StrikeBuilder};
use vtium_font::{Cell, Face, Psf, Style, Unifont, Width};

/// What `face` draws for `c`: the cells of its glyph, or `None`.
fn drawn<F: Face>(face: &F, c: char) -> Option<Width> {
    face.draw(&[c], Style::default()).map(|g| g.cells())
}

// every glyph of a PSF font is one cell wide (the PSF format has one glyph
// size), and a character the font lacks has no advance
#[test]
fn a_psf_fonts_glyphs_are_one_cell_wide() {
    let mut bytes = psf2(0, 256, 16, 16, 8);
    bytes.resize(bytes.len() + 256 * 16, 0xaa);
    let psf = Psf::parse(&bytes).expect("the font loads");
    for c in ['\0', 'A', '\u{ff}'] {
        assert_eq!(psf.advance(c), Some(Width::Narrow), "{c:?}");
        assert_eq!(psf.advance(c), drawn(&psf, c), "{c:?}");
    }
    for c in ['\u{100}', '\u{4e00}', '\u{10ffff}'] {
        assert_eq!(psf.advance(c), None, "{c:?}");
        assert_eq!(drawn(&psf, c), None, "{c:?}");
    }
}

/// A set at an 8×16 cell with a narrow `A` and a wide U+4E00, its chunk
/// deflated so that drawing a glyph decodes it.
fn set() -> Arc<Set> {
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(true);
    let a = s.glyph(Width::Narrow, vec![255; 8 * 16]);
    let wide = s.glyph(Width::Wide, vec![255; 16 * 16]);
    s.map('A', a);
    s.map('\u{4e00}', wide);
    let bytes = SetBuilder::new("t")
        .strike(s)
        .write()
        .expect("the set writes");
    Arc::new(Set::from_bytes(bytes).expect("the set reads"))
}

// a set's advance comes from its strike's map, where each glyph is marked
// one cell or two, so measuring decodes no chunk; the
// advance is the cells the drawn glyph spans, at every scale
#[test]
fn a_sets_advance_comes_from_its_map_without_drawing() {
    let set = set();
    for height in [16, 32] {
        let face = SetFace::new(Arc::clone(&set), height).expect("a face");
        assert_eq!(face.advance('A'), Some(Width::Narrow));
        assert_eq!(face.advance('\u{4e00}'), Some(Width::Wide));
        assert_eq!(face.advance('B'), None);
    }
    assert_eq!(set.loaded_chunks(), 0, "measuring drew a glyph");
    let face = SetFace::new(Arc::clone(&set), 16).expect("a face");
    for c in ['A', '\u{4e00}', 'B'] {
        assert_eq!(face.advance(c), drawn(&face, c), "{c:?}");
    }
    assert!(set.loaded_chunks() > 0, "drawing decodes the chunk");
}

// the built-in unifont's advance comes from its set's map: GNU Unifont
// draws a CJK ideograph two cells wide and a Latin letter one, and
// measuring them decodes none of its chunks
#[test]
fn unifont_measures_without_drawing() {
    let set = vtium_font::set::builtin("unifont").expect("the built-in unifont");
    assert_eq!(set.loaded_chunks(), 0);
    let unifont = Unifont::new(32);
    let chars = ['A', '\u{e9}', '\u{4e00}', '\u{ac00}', '\u{10ffff}'];
    let want = [
        Some(Width::Narrow),
        Some(Width::Narrow),
        Some(Width::Wide),
        Some(Width::Wide),
        None,
    ];
    for (c, want) in chars.into_iter().zip(want) {
        assert_eq!(unifont.advance(c), want, "{c:?}");
    }
    assert_eq!(set.loaded_chunks(), 0, "measuring drew a glyph");
    for c in chars {
        assert_eq!(unifont.advance(c), drawn(&unifont, c), "{c:?}");
    }
}

// a boxed face gives its face's advance, not the default that draws
#[test]
fn a_boxed_face_gives_its_faces_advance() {
    let set = set();
    let face: Box<dyn Face> = Box::new(SetFace::new(Arc::clone(&set), 16).expect("a face"));
    assert_eq!(face.advance('\u{4e00}'), Some(Width::Wide));
    assert_eq!(set.loaded_chunks(), 0, "measuring drew a glyph");
}
