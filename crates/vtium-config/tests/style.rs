//! Style asks (ADR-1200 D21, ADR-1302 D4): the helper draws bold, italic
//! and bold italic glyphs from the frit's own faces for them where it
//! names them, and synthesises them from the regular glyph where it does
//! not: an outline slanted by 0.2 about the baseline and emboldened before
//! it is rasterised, a bitmap glyph widened and sheared a pixel row at a
//! time at its font's own size, as vtium-font's own bitmap faces shear it.
//! Every glyph of an answer comes in a strike of the style asked for.

#[path = "common/ttf.rs"]
mod ttf;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use vtium_config::serve::{
    ASK, Engine, FRAME_DONE, FRAME_GLYPHS, FritText, OwnCell, Paths, ServeRequest, serve,
};
use vtium_font::set::{Set, SetFace};
use vtium_font::{Cell, Face, Glyph, Psf, Style};

const REGULAR: Style = Style {
    bold: false,
    italic: false,
};
const BOLD: Style = Style {
    bold: true,
    italic: false,
};
const ITALIC: Style = Style {
    bold: false,
    italic: true,
};
const BOLD_ITALIC: Style = Style {
    bold: true,
    italic: true,
};

/// A directory of a test's own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("vtium-style-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test's directory");
        Scratch(dir)
    }

    /// Writes `bytes` to `name` in the directory: its path.
    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).expect("a file");
        path.display().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn frit(text: &str) -> FritText {
    FritText {
        name: "t".to_owned(),
        text: text.to_owned(),
    }
}

fn engine(text: &str) -> Engine {
    Engine::load(&frit(text), &Paths::default(), None).expect("the frit loads")
}

/// The one strike's style and `c`'s glyph in the set `bytes`, drawn in that
/// style, at `cell`.
fn glyph(bytes: Vec<u8>, cell: Cell, c: char) -> (Style, Glyph) {
    let set = Arc::new(Set::from_bytes(bytes).expect("a set"));
    assert_eq!(set.cells(), vec![cell]);
    let styles: Vec<Style> = set.strikes().map(|s| s.style).collect();
    let style = *styles.first().expect("a strike");
    assert!(styles.iter().all(|&s| s == style), "{styles:?}");
    let face = SetFace::new(set, cell.height).expect("a face");
    (style, face.draw(&[c], style).expect("the glyph"))
}

/// `c`'s glyph of `style` that `engine` draws at `cell`, checked to come in
/// a strike of that style.
fn drawn(engine: &Engine, cell: Cell, style: Style, c: char) -> Glyph {
    let set = engine
        .draw_style(cell, style, &[u32::from(c)], &[])
        .expect("draws")
        .expect("a glyph");
    let (got, g) = glyph(set, cell, c);
    assert_eq!(got, style, "the strike's style is the one asked for");
    g
}

/// The glyph's coverage over its ink box where it has one, else over its
/// cells: each row, and the box's left from the cells' left edge.
fn ink(g: &Glyph) -> (i32, Vec<Vec<u8>>) {
    match g.ink() {
        Some(ink) => (
            ink.left(),
            ink.pixels()
                .chunks(ink.width() as usize)
                .map(<[u8]>::to_vec)
                .collect(),
        ),
        None => (0, g.rows().map(<[u8]>::to_vec).collect()),
    }
}

/// The left edge of a row's ink, in pixels from the cells' left edge, to a
/// fraction of a pixel: the first inked column, less the part of it the
/// ink covers.
fn left_edge(left: i32, row: &[u8]) -> Option<f32> {
    let x = row.iter().position(|&p| p != 0)?;
    Some(left as f32 + x as f32 + 1.0 - f32::from(row[x]) / 255.0)
}

/// The right edge of a row's ink, as [`left_edge`] finds the left.
fn right_edge(left: i32, row: &[u8]) -> Option<f32> {
    let x = row.iter().rposition(|&p| p != 0)?;
    Some(left as f32 + x as f32 + f32::from(row[x]) / 255.0)
}

/// `Test Mono` with its `A` and its `W` replaced: a face of the frit
/// another source names.
fn face(family: &str, a: ttf::Glyph, w: ttf::Glyph) -> Vec<u8> {
    let mut f = ttf::test_mono();
    f.family = family.to_owned();
    f.glyphs[2] = a;
    f.glyphs[3] = w;
    f.build()
}

const CELL: Cell = Cell {
    width: 12,
    height: 16,
};

// synthetic italic of an outline glyph moves each point right by 0.2 of
// its height above the baseline before it is rasterised: each row of a
// bar's ink moves right by 0.2 of the row's middle's height, to the
// pixel, ink above the baseline right and none at it; the glyph keeps its
// advance and its place, so its ink at the baseline does not move
// covers: sgr.italic
// covers: text.outline
#[test]
fn synthetic_italic_slants_an_outline_by_a_fifth_about_the_baseline() {
    let dir = Scratch::new("slant");
    let mono = dir.file("mono.ttf", &ttf::test_mono().build());
    let e = engine(&format!("source=outline:{mono}\n"));
    // in a cell 8 wide the bar's slanted top passes the cell's right edge
    let narrow = Cell {
        width: 8,
        height: 16,
    };
    let regular = drawn(&e, narrow, REGULAR, 'A');
    let italic = drawn(&e, narrow, ITALIC, 'A');
    let (rl, rrows) = ink(&regular);
    let (il, irows) = ink(&italic);
    // the bar stands on the baseline, so its lowest inked row is the
    // baseline's row above it
    let bottom = rrows
        .iter()
        .rposition(|r| r.iter().any(|&p| p != 0))
        .expect("ink");
    let baseline = bottom as f32 + 1.0;
    let mut rows = 0;
    for (y, (r, i)) in rrows.iter().zip(&irows).enumerate() {
        let (Some(a), Some(b)) = (left_edge(rl, r), left_edge(il, i)) else {
            continue;
        };
        let want = 0.2 * (baseline - y as f32 - 0.5);
        assert!(
            (b - a - want).abs() <= 0.5,
            "row {y}: moved {} where 0.2 of its height is {want}",
            b - a
        );
        rows += 1;
    }
    assert!(rows >= 10, "{rows} rows of ink");
    assert!(italic.ink().is_some(), "the slanted top passes the cell");
    assert_eq!(regular.cells(), italic.cells(), "the advance is unchanged");
}

// synthetic bold grows the outline from side to side by a 24th of the
// size in all, half on each side, and by half a pixel a side at least
// (the maintainer, 2026-11-04), and adds no row above or below: at 16
// pixels the floor's half pixel, at 64 a 48th of the size
// covers: sgr.bold
// covers: text.outline
#[test]
fn synthetic_bold_emboldens_an_outline_from_side_to_side() {
    let dir = Scratch::new("bold");
    let mono = dir.file("mono.ttf", &ttf::test_mono().build());
    let e = engine(&format!("source=outline:{mono}\n"));
    for (cell, grow) in [
        (CELL, 0.5),
        (
            Cell {
                width: 40,
                height: 64,
            },
            // the size the cell draws Test Mono at: its ascent and descent
            // span the height less half a pixel
            (64.0 - 0.5) / 48.0,
        ),
    ] {
        let (rl, rrows) = ink(&drawn(&e, cell, REGULAR, 'A'));
        let (bl, brows) = ink(&drawn(&e, cell, BOLD, 'A'));
        let inked = |rows: &[Vec<u8>]| -> Vec<bool> {
            rows.iter().map(|r| r.iter().any(|&p| p != 0)).collect()
        };
        assert_eq!(inked(&rrows), inked(&brows), "the same rows hold ink");
        let y = rrows
            .iter()
            .position(|r| r.iter().any(|&p| p != 0))
            .expect("ink")
            + 3;
        let (l, r) = (
            left_edge(rl, &rrows[y]).expect("ink"),
            right_edge(rl, &rrows[y]).expect("ink"),
        );
        let (bl_, br) = (
            left_edge(bl, &brows[y]).expect("ink"),
            right_edge(bl, &brows[y]).expect("ink"),
        );
        let h = cell.height;
        assert!(((l - bl_) - grow).abs() < 0.2, "{h}: left by {}", l - bl_);
        assert!(((br - r) - grow).abs() < 0.2, "{h}: right by {}", br - r);
    }
}

// synthetic bold italic of an outline glyph is emboldened first and the
// widened outline slanted after (ADR-1302 D4), so the bar's bottom row,
// half a pixel above the baseline, sits 0.1 of a pixel right of the bold
// glyph's on both sides. Slanted first and emboldened after, the bar's
// slanted sides grow along their slant, down and to the left at its
// foot, and the foot lands between a third and a half of a pixel left of
// the bold foot at this size, where each side grows by 2.7 pixels.
// covers: text.outline
#[test]
fn bold_italic_emboldens_an_outline_before_it_slants_it() {
    let dir = Scratch::new("bold-italic");
    let mono = dir.file("mono.ttf", &ttf::test_mono().build());
    let e = engine(&format!("source=outline:{mono}\n"));
    let cell = Cell {
        width: 80,
        height: 128,
    };
    let (bl, brows) = ink(&drawn(&e, cell, BOLD, 'A'));
    let (il, irows) = ink(&drawn(&e, cell, BOLD_ITALIC, 'A'));
    let bottom = |rows: &[Vec<u8>]| {
        rows.iter()
            .rposition(|r| r.iter().any(|&p| p != 0))
            .expect("ink")
    };
    let y = bottom(&brows);
    assert_eq!(bottom(&irows), y, "the foot stays on the baseline's row");
    let moved = (
        left_edge(il, &irows[y]).expect("ink") - left_edge(bl, &brows[y]).expect("ink"),
        right_edge(il, &irows[y]).expect("ink") - right_edge(bl, &brows[y]).expect("ink"),
    );
    assert!(
        (moved.0 - 0.1).abs() <= 0.05 && (moved.1 - 0.1).abs() <= 0.05,
        "the foot's left and right edges moved {moved:?}, where the slant moves them 0.1"
    );
}

// a frit that names a bold and an italic face draws each style from its
// own face, and bold italic, which it names no face for, from the regular
// glyph emboldened and then slanted (the maintainer, 2026-11-04)
// covers: text.outline
#[test]
fn a_frits_own_faces_draw_their_styles() {
    let dir = Scratch::new("faces");
    let regular = dir.file("r.ttf", &ttf::test_mono().build());
    let bold = dir.file(
        "b.ttf",
        &face(
            "Test Mono Bold",
            ttf::Glyph::rect(600, 50, 0, 550, 700),
            ttf::Glyph::rect(600, 0, 0, 600, 700),
        ),
    );
    let italic = dir.file(
        "i.ttf",
        &face(
            "Test Mono Italic",
            ttf::Glyph::rect(600, 250, 0, 350, 700),
            ttf::Glyph::rect(600, 0, 0, 600, 700),
        ),
    );
    let e = engine(&format!(
        "source=outline:{regular}\n\
         source=outline:{bold}\n\
         style=bold\n\
         source=outline:{italic}\n\
         style=italic\n"
    ));
    let alone = |path: &str, style: Style| {
        drawn(
            &engine(&format!("source=outline:{path}\n")),
            CELL,
            style,
            'A',
        )
    };
    // each face's own glyph, drawn as the font draws it
    assert_eq!(
        drawn(&e, CELL, BOLD, 'A').pixels(),
        alone(&bold, REGULAR).pixels()
    );
    assert_eq!(
        drawn(&e, CELL, ITALIC, 'A').pixels(),
        alone(&italic, REGULAR).pixels()
    );
    assert_eq!(
        drawn(&e, CELL, BOLD_ITALIC, 'A').pixels(),
        alone(&regular, BOLD_ITALIC).pixels(),
        "the regular glyph emboldened and slanted"
    );
    assert_ne!(
        drawn(&e, CELL, BOLD_ITALIC, 'A').pixels(),
        alone(&bold, ITALIC).pixels(),
        "not the bold face slanted"
    );
    // a frit that names neither synthesises both from its regular face
    assert_eq!(
        drawn(&e, CELL, REGULAR, 'A').pixels(),
        alone(&regular, REGULAR).pixels()
    );
    assert_ne!(
        alone(&regular, BOLD).pixels(),
        alone(&bold, REGULAR).pixels()
    );
}

// a frit that names a bold face and no bold-italic face draws bold italic
// from its regular source, emboldened and then slanted, not from the bold
// face (the maintainer, 2026-11-04; ADR-1200 D21)
// covers: text.outline
#[test]
fn bold_italic_with_no_face_of_its_own_comes_from_the_regular_glyph() {
    let dir = Scratch::new("bold-only");
    let regular = dir.file("r.ttf", &ttf::test_mono().build());
    let bold = dir.file(
        "b.ttf",
        &face(
            "Test Mono Bold",
            ttf::Glyph::rect(600, 250, 0, 350, 700),
            ttf::Glyph::rect(600, 0, 0, 600, 700),
        ),
    );
    let e = engine(&format!(
        "source=outline:{regular}\nsource=outline:{bold}\nstyle=bold\n"
    ));
    let from = |path: &str, style: Style| {
        drawn(
            &engine(&format!("source=outline:{path}\n")),
            CELL,
            style,
            'A',
        )
    };
    let both = drawn(&e, CELL, BOLD_ITALIC, 'A');
    assert_eq!(both.pixels(), from(&regular, BOLD_ITALIC).pixels());
    assert_ne!(both.pixels(), from(&bold, ITALIC).pixels());
    // bold alone still comes from the bold face
    assert_eq!(
        drawn(&e, CELL, BOLD, 'A').pixels(),
        from(&bold, REGULAR).pixels()
    );
}

// the W rule takes the bold face's W too, the wider of the two, so a bold
// face wider than the regular one still fits the cell (ADR-1200 D17)
#[test]
fn the_w_rule_takes_the_bold_faces_w() {
    let dir = Scratch::new("w");
    let regular = dir.file("r.ttf", &ttf::test_mono().build());
    let bold = dir.file(
        "b.ttf",
        &face(
            "Test Mono Bold",
            ttf::Glyph::rect(600, 50, 0, 550, 700),
            ttf::Glyph::rect(800, 0, 0, 800, 700),
        ),
    );
    let alone = engine(&format!("source=outline:{regular}\n"))
        .own_cell(16)
        .expect("a cell");
    let both = engine(&format!(
        "source=outline:{regular}\nsource=outline:{bold}\nstyle=bold\n"
    ))
    .own_cell(16)
    .expect("a cell");
    // W's ink spans 600 and 800 units of 1000 at 15.5 pixels to the em,
    // 9.3 and 12.4 pixels, over 10 and 13 columns
    assert_eq!(
        alone,
        OwnCell::Outline(Cell {
            width: 10,
            height: 16
        })
    );
    assert_eq!(
        both,
        OwnCell::Outline(Cell {
            width: 13,
            height: 16
        })
    );
}

/// A 256-glyph 8x16 PSF1 font whose table maps each glyph to the code
/// point of its index, so loading it reads no unimap from the host: `H`
/// inked on rows 2 to 11, so its baseline lies under row 11, `|` a column
/// at the cell's left edge down every row, `\\` a column at x 6, next to
/// its right edge, and `/` a column at x 7, the cell's last.
fn psf() -> Vec<u8> {
    let mut b = vec![0x36, 0x04, 0x02, 16];
    b.resize(4 + 256 * 16, 0);
    for row in 2..12 {
        b[4 + 0x48 * 16 + row] = 0b1100_0110;
    }
    for row in 0..16 {
        b[4 + 0x7c * 16 + row] = 0b1000_0000;
        b[4 + 0x5c * 16 + row] = 0b0000_0010;
        b[4 + 0x2f * 16 + row] = 0b0000_0001;
    }
    for glyph in 0..256u16 {
        b.extend(glyph.to_le_bytes());
        b.extend([0xff, 0xff]);
    }
    b
}

/// The first inked column of each row, from the ink box's left.
fn first_ink(g: &Glyph) -> Vec<Option<usize>> {
    ink(g)
        .1
        .iter()
        .map(|r| r.iter().position(|&p| p != 0))
        .collect()
}

// an 8x16 bitmap glyph sheared for italic in the helper moves its rows by
// 2, 1, 0 and -1 pixels, as vtium-font shears the faces the console
// draws itself, and at a scale of 2 by twice as many; bold italic widens
// first and shears the widened rows, in both alike
// covers: sgr.italic
#[test]
fn the_helper_shears_a_bitmap_as_vtium_font_does() {
    let dir = Scratch::new("shear");
    let path = dir.file("f.psf", &psf());
    let e = engine(&format!("source=file:{path}\n"));
    let at8 = Cell {
        width: 8,
        height: 16,
    };
    let bar = drawn(&e, at8, ITALIC, '|');
    assert_eq!(bar.ink().map(|i| (i.left(), i.width())), Some((-1, 9)));
    let mut want = vec![Some(3); 5];
    want.extend([Some(2); 5]);
    want.extend([Some(1); 5]);
    want.push(Some(0));
    assert_eq!(first_ink(&bar), want);
    let mut own = Psf::parse(&psf()).expect("the font");
    // `/` in the cell's last column has no room to widen in the cell, so
    // its bold italic glyph is its italic glyph in both
    for c in ['|', '/'] {
        for style in [ITALIC, BOLD_ITALIC] {
            let theirs = own.draw(&[c], style).expect("vtium-font's");
            let ours = drawn(&e, at8, style, c);
            assert_eq!(ours.pixels(), theirs.pixels(), "{c} {style:?}");
            assert_eq!(ours.ink(), theirs.ink(), "{c} {style:?}");
        }
    }
    let at16 = Cell {
        width: 16,
        height: 32,
    };
    own.set_height(32);
    let ours = drawn(&e, at16, ITALIC, '|');
    let theirs = own.draw(&['|'], ITALIC).expect("vtium-font's");
    assert_eq!(ours.ink().map(|i| (i.left(), i.width())), Some((-2, 18)));
    assert_eq!(ours.pixels(), theirs.pixels());
    assert_eq!(ours.ink(), theirs.ink());
    // the regular glyph stays upright
    assert!(drawn(&e, at8, REGULAR, '|').ink().is_none());
    // the regular strike states the baseline the helper shears about, row
    // 12 at the font's own cell, so vtium-font's set reader leans the
    // regular glyph for italic as the helper does
    let regular = e
        .draw_style(at8, REGULAR, &[u32::from('|')], &[])
        .expect("draws")
        .expect("a glyph");
    let set = Arc::new(Set::from_bytes(regular).expect("a set"));
    assert_eq!(
        set.strikes().map(|s| s.baseline).collect::<Vec<u32>>(),
        [12]
    );
    let reader = SetFace::new(set, 16).expect("a face");
    let theirs = reader.draw(&['|'], ITALIC).expect("the reader's");
    let ours = drawn(&e, at8, ITALIC, '|');
    assert_eq!((ours.pixels(), ours.ink()), (theirs.pixels(), theirs.ink()));
}

// ink sheared past the cell's right edge widens the box right, bold
// italic widened first, in the helper as in vtium-font; and a box past a
// set's chunk, at a 256x512 cell, is left out with the glyph cut to its
// cells (ADR-1302 D6)
// covers: sgr.italic
#[test]
fn a_bitmap_box_passes_the_right_edge_and_stays_inside_a_chunk() {
    let dir = Scratch::new("right");
    let path = dir.file("f.psf", &psf());
    let e = engine(&format!("source=file:{path}\n"));
    let at8 = Cell {
        width: 8,
        height: 16,
    };
    let own = Psf::parse(&psf()).expect("the font");
    for (style, width) in [(ITALIC, 9), (BOLD_ITALIC, 10)] {
        let ours = drawn(&e, at8, style, '\\');
        assert_eq!(ours.ink().map(|i| (i.left(), i.width())), Some((0, width)));
        let theirs = own.draw(&['\\'], style).expect("vtium-font's");
        assert_eq!((ours.pixels(), ours.ink()), (theirs.pixels(), theirs.ink()));
    }
    let big = Cell {
        width: 256,
        height: 512,
    };
    let ours = drawn(&e, big, ITALIC, '|');
    assert!(ours.ink().is_none(), "no box past a chunk");
    let mut own = own;
    own.set_height(512);
    let theirs = own.draw(&['|'], ITALIC).expect("vtium-font's");
    assert_eq!((ours.pixels(), ours.ink()), (theirs.pixels(), theirs.ink()));
}

// italic and bold asked of the helper as the console asks: each answer's
// strikes in the style asked for, for a frit that names faces and one
// that names none
// covers: sgr.italic
// covers: sgr.bold
#[test]
fn style_asks_are_answered_in_their_style() {
    let dir = Scratch::new("asks");
    let regular = dir.file("r.ttf", &ttf::test_mono().build());
    let bold = dir.file(
        "b.ttf",
        &face(
            "Test Mono Bold",
            ttf::Glyph::rect(600, 50, 0, 550, 700),
            ttf::Glyph::rect(600, 0, 0, 600, 700),
        ),
    );
    for text in [
        format!("source=outline:{regular}\n"),
        format!("source=outline:{regular}\nsource=outline:{bold}\nstyle=bold\n"),
    ] {
        let mut input = Vec::new();
        for style in [BOLD, ITALIC, BOLD_ITALIC] {
            input.extend(
                ServeRequest {
                    kind: ASK,
                    frit: frit(&text),
                    cell: CELL,
                    band: vtium_font::spacing::Band::NONE,
                    style,
                    codepoints: vec![0x41],
                    clusters: Vec::new(),
                    held: Vec::new(),
                    console: None,
                }
                .encode(),
            );
        }
        let mut out = Vec::new();
        let status = serve(
            std::io::Cursor::new(input),
            &mut out,
            &Paths::default(),
            Duration::from_secs(5),
        );
        assert_eq!(status, 0);
        let mut rest = &out[..];
        let mut answers = Vec::new();
        while rest.len() >= 8 {
            let kind = u32::from_le_bytes(rest[..4].try_into().expect("4 bytes"));
            let len = u32::from_le_bytes(rest[4..8].try_into().expect("4 bytes")) as usize;
            let payload = rest[8..8 + len].to_vec();
            rest = &rest[8 + len..];
            match kind {
                // the band, none here, then the set
                FRAME_GLYPHS => answers.push(glyph(payload[8..].to_vec(), CELL, 'A').0),
                FRAME_DONE => {}
                other => panic!("a frame of kind {other}"),
            }
        }
        assert_eq!(answers, [BOLD, ITALIC, BOLD_ITALIC], "{text}");
    }
}
