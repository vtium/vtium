//! Ink boxes (ADR-1200 D2, ADR-1302 D6): a glyph whose ink passes its
//! cells carries its box in `INKX`, read back beside its pixels cut to its
//! cells, for a codepoint and for a cluster; the writer and the reader each
//! refuse a box past the glyph's reach, one cell past its cells on each
//! side, or not past its cells; and a set with no box has no `INKX`.

mod common;

use std::sync::Arc;

use common::{sections, with_section};
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetError, SetFace, StrikeBuilder};
use vtium_font::{Cell, Face, Style, Width};

const CELL: Cell = Cell {
    width: 4,
    height: 2,
};

/// A box `width` columns of R8 over CELL's height, each pixel its column's
/// number from 1.
fn columns(width: u32) -> Vec<u8> {
    (0..CELL.height)
        .flat_map(|_| (1..=width).map(|x| x as u8))
        .collect()
}

/// A set of one R8 strike at CELL: `A` a narrow glyph with the box
/// `(left, width)`, `B` a narrow glyph with none, and the cluster `e`
/// U+0301 drawn by `A`'s glyph; or the writer's refusal.
fn set_with(left: i32, width: u32, format: PixelFormat) -> Result<Vec<u8>, String> {
    let mut s = StrikeBuilder::new(format, CELL, Style::default()).compress(false);
    let bpp = if format == PixelFormat::Rgba { 4 } else { 1 };
    let cells = vec![9u8; (CELL.width * CELL.height * bpp) as usize];
    let a = s.glyph(Width::Narrow, cells.clone());
    let b = s.glyph(Width::Narrow, cells);
    s.map('A', a);
    s.map('B', b);
    s.cluster(&['e', '\u{301}'], a);
    s.ink(a, left, width, columns(width));
    SetBuilder::new("ink")
        .strike(s)
        .write()
        .map_err(|e| e.to_string())
}

fn face(bytes: Vec<u8>) -> SetFace {
    SetFace::new(
        Arc::new(Set::from_bytes(bytes).expect("a set")),
        CELL.height,
    )
    .expect("a face")
}

// covers: text.overhang
#[test]
fn a_glyph_and_a_cluster_carry_their_ink_box() {
    let bytes = set_with(-2, 7, PixelFormat::R8).expect("written");
    let tags: Vec<[u8; 4]> = sections(&bytes).2.iter().map(|s| s.0).collect();
    assert!(tags.contains(b"INKX"), "{tags:?}");
    let face = face(bytes);
    let a = face.draw(&['A'], Style::default()).expect("A");
    assert_eq!(
        a.pixels(),
        &[9u8; 8],
        "the glyph's pixels stay cut to its cell"
    );
    let ink = a.ink().expect("A's box");
    assert_eq!((ink.left(), ink.width()), (-2, 7));
    assert_eq!(ink.pixels(), columns(7).as_slice());
    let e = face
        .cluster(&['e', '\u{301}'], Style::default())
        .expect("the cluster");
    assert_eq!(e.ink().map(|i| (i.left(), i.width())), Some((-2, 7)));
    let b = face.draw(&['B'], Style::default()).expect("B");
    assert!(b.ink().is_none());
}

// a box drawn at a whole factor scales as its glyph does, and bold widens
// it inside the box
#[test]
fn a_scaled_or_bold_glyph_scales_and_widens_its_box() {
    let set = Arc::new(
        Set::from_bytes(set_with(-1, 6, PixelFormat::R8).expect("written")).expect("a set"),
    );
    let face = SetFace::new(set, CELL.height * 2).expect("a face at twice the height");
    let a = face.draw(&['A'], Style::default()).expect("A");
    let ink = a.ink().expect("A's box");
    assert_eq!((ink.left(), ink.width()), (-2, 12));
    let row: Vec<u8> = ink.pixels()[..12].to_vec();
    assert_eq!(row, [1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6]);
    let bold = SetFace::new(
        Arc::new(
            Set::from_bytes(set_with(-1, 6, PixelFormat::R8).expect("written")).expect("a set"),
        ),
        CELL.height,
    )
    .expect("a face")
    .draw(
        &['A'],
        Style {
            bold: true,
            italic: false,
        },
    )
    .expect("bold A");
    // each pixel the larger of itself and its left neighbour in the box
    assert_eq!(
        &bold.ink().expect("a box").pixels()[..6],
        &[1, 2, 3, 4, 5, 6]
    );
}

// a set whose glyphs fit their cells writes no INKX, so a reader of
// format 2.0 reads it as before
#[test]
fn a_set_with_no_box_has_no_inkx() {
    let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
    let a = s.glyph(Width::Narrow, vec![1; 8]);
    s.map('A', a);
    let bytes = SetBuilder::new("plain").strike(s).write().expect("written");
    let tags: Vec<[u8; 4]> = sections(&bytes).2.iter().map(|s| s.0).collect();
    assert!(!tags.contains(b"INKX"), "{tags:?}");
}

// ADR-1302 D1, D6: the reach is one cell past the glyph's cells on each
// side, and a box holds only ink that passes its cells; a color glyph fits
// its cells
#[test]
fn the_writer_refuses_a_box_past_the_reach_or_not_past_the_cells() {
    for (left, width, why) in [
        (-5, 9, "a cell past the left"),
        (0, 9, "a cell past the right"),
        (1, 4, "right of the cell's edge"),
        (0, 4, "the cell itself"),
        (-1, 3, "short of the cell's right edge"),
    ] {
        let e = set_with(left, width, PixelFormat::R8).expect_err(why);
        assert!(e.contains("reach"), "{why}: {e}");
    }
    assert!(set_with(-4, 12, PixelFormat::R8).is_ok(), "the whole reach");
    let e = set_with(-1, 5, PixelFormat::Rgba).expect_err("a color box");
    assert!(e.contains("color"), "{e}");
}

/// `INKX`'s first record's `left` and `width`, set to `left` and `width`.
fn tampered(left: i32, width: u32) -> Vec<u8> {
    let bytes = set_with(-1, 5, PixelFormat::R8).expect("written");
    with_section(&bytes, b"INKX", |inkx| {
        // the head is 16 bytes; a record's left is at 8 and its width at 12
        inkx[16 + 8..16 + 12].copy_from_slice(&left.to_le_bytes());
        inkx[16 + 12..16 + 16].copy_from_slice(&width.to_le_bytes());
    })
}

// the console reads the helper's glyphs through this reader, so a box past
// the reach is refused before any glyph is drawn (ADR-1200 D15)
#[test]
fn the_reader_refuses_a_box_past_the_reach() {
    assert!(
        Set::from_bytes(tampered(-1, 5)).is_ok(),
        "the untampered box"
    );
    for (left, width) in [(-5, 9), (0, 9), (0, 4), (1, 4)] {
        match Set::from_bytes(tampered(left, width)) {
            Err(SetError::Malformed(why)) => {
                assert!(why.contains("reach"), "{left} {width}: {why}")
            }
            other => panic!("{left} {width}: {other:?}"),
        }
    }
}
