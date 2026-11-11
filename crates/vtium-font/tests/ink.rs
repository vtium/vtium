//! Ink boxes (ADR-1200 D2, ADR-1302 D6, ADR-1306 D3): a glyph whose ink
//! passes its cells carries its box in `INKX`, read back beside its pixels
//! cut to its cells, for a codepoint and for a cluster; the writer and the
//! reader each refuse a box past the glyph's reach, a cell past its cells
//! on each side and a row above and below them, or not past its cells; a
//! record written by format 2.0, without the box's top and height, reads
//! as a box of the cell's height; and a set with no box has no `INKX`.

mod common;

use std::sync::Arc;

use common::{sections, with_section};
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetError, SetFace, StrikeBuilder};
use vtium_font::{Cell, Face, Glyph, REACH_CELLS, REACH_ROWS, Style, Width};

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
    s.ink(a, (left, 0), (width, CELL.height), columns(width));
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

// a set whose glyphs fit their cells writes no INKX, so a reader that
// does not know INKX reads it as before
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

// a box whose record names a color strike is refused, as a color glyph fits
// its cells: a set of an R8 strike holding A and its box and an RGBA strike
// holding B at one cell, the box's record then pointed at the RGBA strike,
// number 1 of the two
#[test]
fn the_reader_refuses_a_box_in_a_color_strike() {
    let mut r8 = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default()).compress(false);
    let a = r8.glyph(Width::Narrow, vec![9; 8]);
    r8.map('A', a);
    r8.ink(a, (-1, 0), (5, CELL.height), columns(5));
    let mut rgba = StrikeBuilder::new(PixelFormat::Rgba, CELL, Style::default()).compress(false);
    let b = rgba.glyph(Width::Narrow, vec![0; 32]);
    rgba.map('B', b);
    let bytes = SetBuilder::new("two")
        .strike(r8)
        .strike(rgba)
        .write()
        .expect("written");
    assert!(Set::from_bytes(bytes.clone()).is_ok(), "the untampered set");
    let tampered = with_section(&bytes, b"INKX", |inkx| {
        inkx[16..18].copy_from_slice(&1u16.to_le_bytes());
    });
    match Set::from_bytes(tampered) {
        Err(SetError::Malformed(why)) => assert!(why.contains("color"), "{why}"),
        other => panic!("{other:?}"),
    }
}

// ADR-1306 D1, D3: a glyph's box may reach REACH_ROWS rows above and below
// its row as well as REACH_CELLS cells beside it, and no further; a box
// that only covers the glyph's own cells is no box
// covers: text.overhang
#[test]
fn a_glyphs_box_reaches_a_row_up_and_down_and_no_further() {
    let (w, h) = (CELL.width, CELL.height);
    let (across, down) = ((w * REACH_CELLS) as i32, (h * REACH_ROWS) as i32);
    let glyph = || Glyph::mask(w, h).expect("a glyph");
    let boxed = |left: i32, top: i32, width: u32, height: u32| {
        let n = (width * height) as usize;
        glyph().with_ink((left, top), (width, height), vec![7; n])
    };
    // the whole reach: the 3 by 3 block of cells around the glyph's own
    let full = (w + 2 * across as u32, h + 2 * down as u32);
    let g = boxed(-across, -down, full.0, full.1).expect("the whole reach");
    let ink = g.ink().expect("its box");
    assert_eq!(
        (ink.left(), ink.top(), ink.width(), ink.height()),
        (-across, -down, full.0, full.1)
    );
    // a row up only, and a row down only
    assert!(boxed(0, -down, w, h + down as u32).is_some(), "up");
    assert!(boxed(0, 0, w, h + down as u32).is_some(), "down");
    // a pixel past the reach above, below, left or right
    assert!(
        boxed(0, -down - 1, w, h + down as u32 + 1).is_none(),
        "above"
    );
    assert!(boxed(0, 0, w, h + down as u32 + 1).is_none(), "below");
    assert!(
        boxed(-across - 1, 0, w + across as u32 + 1, h).is_none(),
        "left"
    );
    assert!(boxed(0, 0, w + across as u32 + 1, h).is_none(), "right");
    // a box that leaves out a row of the glyph's own, or only covers them
    assert!(
        boxed(0, -1, w, h).is_none(),
        "the cell's bottom row left out"
    );
    assert!(boxed(0, 0, w, h).is_none(), "not past the cells");
    // the pixels must be the box's
    assert!(glyph().with_ink((0, -1), (w, h + 1), vec![7; 3]).is_none());
}

/// A set of one strike of `format` at CELL, compressed when `compress`:
/// `A` a narrow glyph of 9s with the box `(left, top)`, `(width, height)`
/// whose pixels count up from 1 row by row, or the writer's refusal.
fn set_with_rows(
    (left, top): (i32, i32),
    (width, height): (u32, u32),
    format: PixelFormat,
    compress: bool,
) -> Result<Vec<u8>, String> {
    let mut s = StrikeBuilder::new(format, CELL, Style::default()).compress(compress);
    let (cells, boxed) = match format {
        PixelFormat::Bits => (
            vec![0b1010_0000; CELL.height as usize],
            (0..height)
                .flat_map(|y| {
                    vec![if y % 2 == 0 { 0xff } else { 0x81 }; width.div_ceil(8) as usize]
                })
                .collect(),
        ),
        _ => (
            vec![9; (CELL.width * CELL.height) as usize],
            (1..=width * height).map(|v| v as u8).collect(),
        ),
    };
    let a = s.glyph(Width::Narrow, cells);
    s.map('A', a);
    s.ink(a, (left, top), (width, height), boxed);
    SetBuilder::new("ink")
        .strike(s)
        .write()
        .map_err(|e| e.to_string())
}

// ADR-1306 D3: the box's top and height round trip through INKX, for R8
// and 1-bit strikes, stored raw and deflated, and scale with their glyph
// covers: text.overhang
#[test]
fn a_box_above_and_below_its_row_round_trips() {
    let (w, h) = (CELL.width, CELL.height);
    // a cell to the left, a row above and a row below
    let (left, top, width, height) = (-(w as i32), -(h as i32), 2 * w, 3 * h);
    for compress in [false, true] {
        let bytes = set_with_rows((left, top), (width, height), PixelFormat::R8, compress)
            .expect("written");
        let (major, minor, _) = sections(&bytes);
        assert_eq!((major, minor), vtium_font::set::VERSION);
        assert_eq!(vtium_font::set::VERSION, (2, 3));
        let a = face(bytes.clone())
            .draw(&['A'], Style::default())
            .expect("A");
        assert_eq!(a.pixels(), &[9u8; 8], "the glyph's pixels stay its cell's");
        let ink = a.ink().expect("A's box");
        assert_eq!(
            (ink.left(), ink.top(), ink.width(), ink.height()),
            (left, top, width, height),
            "compress {compress}"
        );
        let want: Vec<u8> = (1..=width * height).map(|v| v as u8).collect();
        assert_eq!(ink.pixels(), want.as_slice());
        // drawn at twice the height, every side of the box doubles
        let set = Arc::new(Set::from_bytes(bytes).expect("a set"));
        let big = SetFace::new(set, h * 2).expect("a face at twice the height");
        let ink = big
            .draw(&['A'], Style::default())
            .expect("A")
            .ink()
            .cloned();
        let ink = ink.expect("the scaled box");
        assert_eq!(
            (ink.left(), ink.top(), ink.width(), ink.height()),
            (2 * left, 2 * top, 2 * width, 2 * height)
        );
    }
    // a 1-bit box a row below the cell: each row a byte, 0xff then 0x81
    let bytes = set_with_rows((0, 0), (8, 2 * h), PixelFormat::Bits, false).expect("written");
    let a = face(bytes).draw(&['A'], Style::default()).expect("A");
    let ink = a.ink().expect("A's box");
    assert_eq!(
        (ink.left(), ink.top(), ink.width(), ink.height()),
        (0, 0, 8, 4)
    );
    let rows: Vec<&[u8]> = ink.pixels().chunks(8).collect();
    assert_eq!(rows[0], &[255; 8]);
    assert_eq!(rows[1], &[255, 0, 0, 0, 0, 0, 0, 255]);
    assert_eq!(rows[3], &[255, 0, 0, 0, 0, 0, 0, 255]);
}

/// `set` with each `INKX` record cut to format 2.0's 24 bytes, without
/// its top and height, and the chunks' offsets from the section's start
/// moved back by the bytes cut, as a 2.0 writer laid them out.
fn as_format_2_0(set: &[u8]) -> Vec<u8> {
    let out = with_section(set, b"INKX", |inkx| {
        let word = |b: &[u8], i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let (count, size, chunks) = (
            word(inkx, 0) as usize,
            word(inkx, 4) as usize,
            word(inkx, 8) as usize,
        );
        assert_eq!(size, 32, "a 2.1 record");
        let mut cut = inkx[..16].to_vec();
        cut[4..8].copy_from_slice(&24u32.to_le_bytes());
        for r in 0..count {
            cut.extend_from_slice(&inkx[16 + r * 32..16 + r * 32 + 24]);
        }
        let shrink = (count * 8) as u64;
        let table = 16 + count * 32;
        for c in 0..chunks {
            let mut record = inkx[table + c * 20..table + c * 20 + 20].to_vec();
            let at = u64::from_le_bytes(record[..8].try_into().unwrap()) - shrink;
            record[..8].copy_from_slice(&at.to_le_bytes());
            cut.extend_from_slice(&record);
        }
        cut.extend_from_slice(&inkx[table + chunks * 20..]);
        *inkx = cut;
    });
    let (major, _, secs) = sections(&out);
    common::assemble(major, 0, &secs)
}

// a box written by format 2.0 holds no top or height and reads as a box
// of the cell's height (ADR-1306 D3); and a 2.1 record's first six fields
// name the box's rows of the glyph's own row, so for a box that passes its
// cells to the side, a reader that knows only those reads the glyph's ink
// cut at its row. A box that passes its cells only above or below, such
// as (0, -h), (w, 2h), reads by those fields as a box of the cells alone,
// which a 2.0 reader refused, so it refused the whole set; this build has
// no 2.0 reader to show it with, and the console and the helper always
// come from one build (ADR-1200 D15)
#[test]
fn a_format_2_0_box_reads_as_the_cells_height() {
    let (w, h) = (CELL.width, CELL.height);
    let flat = set_with(-2, 7, PixelFormat::R8).expect("written");
    let old = as_format_2_0(&flat);
    assert_eq!((sections(&old).0, sections(&old).1), (2, 0));
    let a = face(old).draw(&['A'], Style::default()).expect("A");
    let ink = a.ink().expect("A's box");
    assert_eq!(
        (ink.left(), ink.top(), ink.width(), ink.height()),
        (-2, 0, 7, h)
    );
    assert_eq!(ink.pixels(), columns(7).as_slice());
    // a box a cell to the left and a row above and below, read by its first
    // six fields: its middle rows, the glyph's own row
    let (width, height) = (w + 1, 3 * h);
    let tall =
        set_with_rows((-1, -(h as i32)), (width, height), PixelFormat::R8, false).expect("written");
    let a = face(as_format_2_0(&tall))
        .draw(&['A'], Style::default())
        .expect("A");
    let ink = a.ink().expect("A's box");
    assert_eq!(
        (ink.left(), ink.top(), ink.width(), ink.height()),
        (-1, 0, width, h)
    );
    let middle: Vec<u8> = (width * h + 1..=width * 2 * h).map(|v| v as u8).collect();
    assert_eq!(ink.pixels(), middle.as_slice());
}

/// `INKX`'s first record's `top` and `height`, set to `top` and `height`,
/// in a set whose box is a cell to the left over the cell's height.
fn tampered_rows(top: i32, height: u32) -> Vec<u8> {
    let bytes = set_with_rows((-1, 0), (5, CELL.height), PixelFormat::R8, false).expect("written");
    with_section(&bytes, b"INKX", |inkx| {
        // a record's top is at 24 and its height at 28
        inkx[16 + 24..16 + 28].copy_from_slice(&top.to_le_bytes());
        inkx[16 + 28..16 + 32].copy_from_slice(&height.to_le_bytes());
    })
}

// ADR-1306 D3, ADR-1200 D15: the console reads the helper's glyphs through
// this reader, so a box past the reach above or below is refused, as one
// past it to the side is
#[test]
fn the_reader_refuses_a_box_past_the_reach_up_or_down() {
    let h = CELL.height * REACH_ROWS;
    let reach = h as i32;
    // the box's bytes start `-top` rows before its offset, so a taller box
    // than was written reads past its chunk; these are refused for their
    // place before their bytes are looked at
    for (top, height, why) in [
        (-reach - 1, CELL.height + h + 1, "a pixel above the reach"),
        (0, CELL.height + h + 1, "a pixel below the reach"),
        (1, CELL.height, "below the row's top"),
        (0, CELL.height - 1, "short of the row's bottom"),
    ] {
        match Set::from_bytes(tampered_rows(top, height)) {
            Err(SetError::Malformed(why_not)) => {
                assert!(why_not.contains("reach"), "{why}: {why_not}")
            }
            other => panic!("{why}: {other:?}"),
        }
    }
    assert!(
        Set::from_bytes(tampered_rows(0, CELL.height)).is_ok(),
        "untampered"
    );
    // a box inside the reach whose bytes would start before its chunk
    match Set::from_bytes(tampered_rows(-reach, CELL.height + h)) {
        Err(SetError::Malformed(why)) => assert!(why.contains("chunk"), "{why}"),
        other => panic!("{other:?}"),
    }
}

// ADR-1306 D1, D3: the writer refuses a box past the reach above or below,
// and takes the whole reach
#[test]
fn the_writer_refuses_a_box_past_the_reach_up_or_down() {
    let h = CELL.height * REACH_ROWS;
    let reach = h as i32;
    let r8 = PixelFormat::R8;
    for (top, height, why) in [
        (-reach - 1, CELL.height + reach as u32 + 1, "above"),
        (0, CELL.height + h + 1, "below"),
        (-1, CELL.height, "the row's bottom left out"),
    ] {
        let e = set_with_rows((-1, top), (5, height), r8, false).expect_err(why);
        assert!(e.contains("reach"), "{why}: {e}");
    }
    let whole = (CELL.width * (1 + 2 * REACH_CELLS), CELL.height + 2 * h);
    let across = (CELL.width * REACH_CELLS) as i32;
    assert!(
        set_with_rows((-across, -reach), whole, r8, false).is_ok(),
        "the whole reach"
    );
}
