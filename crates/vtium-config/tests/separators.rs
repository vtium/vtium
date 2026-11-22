//! Powerline separators from a merged symbol font, as the font helper
//! draws them for a frit: DejaVu Sans Mono sets the cell, and a symbol font
//! shaped like Monoid Nerd Font Mono gives the Private Use Area through
//! `cover=`. A separator's flat side meets the segment beside it, so its
//! column at that cell edge holds ink on every row the separator spans:
//! the whole cell under the default fit, the text's rows under
//! `fit=natural`. Only the corner rows, where the slanted sides meet the
//! flat side, may hold no ink there. The test draws the separators at every text height from
//! 12 to 48 pixels, at normal size and on lines of double width and double
//! height, with and without line spacing. An icon from the same font keeps
//! the text's size.

use std::path::PathBuf;
use std::sync::Arc;

use vtium_config::serve::{Engine, FritText, OwnCell, Paths};
use vtium_font::set::{Set, SetFace};
use vtium_font::spacing::{Band, SpacedCell};
use vtium_font::{Cell, Doubled, Face, Glyph, Style, Variant};
use vtium_testfont::ttf;

const DEJAVU: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/DejaVuSansMono-ascii.ttf"
);

/// U+E0B0, the right-pointing solid triangle: its flat side is on the left.
const RIGHT: char = '\u{e0b0}';
/// U+E0B2, the left-pointing solid triangle: its flat side is on the right.
const LEFT: char = '\u{e0b2}';
/// U+F015, an icon.
const HOUSE: char = '\u{f015}';

/// A symbol font laid out as Monoid Nerd Font Mono lays out its Powerline
/// separators: 1536 units to the em, an ascent of 1664 and a descent of
/// 384, every glyph advancing 1024 units. That advance is narrower than
/// DejaVu Sans Mono's cell at the size where the font's ascent and descent
/// span the cell. Each solid triangle runs 62 units past its flat side and
/// 10 units past the ascent and the descent, as the Nerd Fonts patcher
/// makes separators overlap their neighbours. The house sits inside the
/// advance, in the middle of the span.
fn symbols() -> Vec<u8> {
    let glyph = |contours| ttf::Glyph {
        advance: 1024,
        contours,
    };
    let glyphs = vec![
        ttf::Glyph::empty(1024),
        glyph(vec![vec![(-62, -394), (-62, 1674), (1023, 640)]]),
        glyph(vec![vec![(1086, -394), (0, 640), (1086, 1674)]]),
        glyph(vec![vec![(200, 200), (200, 1000), (824, 1000), (824, 200)]]),
    ];
    ttf::Font {
        family: "Separator Test".to_owned(),
        upem: 1536,
        ascent: 1664,
        descent: -384,
        glyphs,
        cmap: vec![(0xe0b0, 1), (0xe0b2, 2), (0xf015, 3)],
        colr: Vec::new(),
        palette: Vec::new(),
        cbdt: None,
        gsub: Vec::new(),
        gpos: Vec::new(),
        underline: None,
        os2: None,
        empty_gpos: false,
    }
    .build()
}

/// The engine for DejaVu Sans Mono with the symbol font merged behind it,
/// and `extra` lines after the symbol font's.
fn engine(test: &str, extra: &str) -> Engine {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("vtium-separators-{test}"));
    std::fs::create_dir_all(&dir).expect("the test's directory");
    let path = dir.join("SeparatorTest.ttf");
    std::fs::write(&path, symbols()).expect("the symbol font");
    let frit = FritText {
        name: test.to_owned(),
        text: format!(
            "source=outline:{DEJAVU}\nsource=outline:{}\ncover=U+E000-U+F8FF\n{extra}",
            path.display()
        ),
    };
    Engine::load(&frit, &Paths::default(), None).expect("the frit loads")
}

/// The size a line draws at: normal, double width or double height.
#[derive(Clone, Copy, Debug)]
enum Size {
    Normal,
    Doubled(Doubled),
}

/// `c` as the console reads it from `engine` at the text height `height`,
/// with `band` added by line spacing, at `size`.
fn drawn(engine: &Engine, height: u32, band: Band, size: Size, c: char) -> Glyph {
    let OwnCell::Outline(own) = engine.own_cell(height).expect("a cell") else {
        panic!("DejaVu Sans Mono gives an outline cell");
    };
    let at = SpacedCell {
        cell: Cell {
            width: own.width,
            height: height + band.added(),
        },
        band,
    };
    let want = [u32::from(c)];
    let (bytes, strike) = match size {
        Size::Normal => (
            engine.draw_variant(at, Variant::default(), &want, (&[], false)),
            at.cell.height,
        ),
        Size::Doubled(d) => (
            engine.draw_doubled(at, (Variant::default(), d), &want),
            at.cell.height * d.rows(),
        ),
    };
    let bytes = bytes.expect("draws").expect("a glyph");
    let face = SetFace::new(
        Arc::new(Set::from_bytes(bytes).expect("the set reads")),
        strike,
    )
    .expect("a strike");
    face.draw_unscaled(&[c], Style::default())
        .unwrap_or_else(|| panic!("{c:?} is drawn"))
}

/// The rows at each end of a separator `rows` rows tall where its flat
/// side may hold no ink at its cell's edge. Each triangle's flat side lies
/// 62 units past the edge, and its slanted sides fall 1034 units over the
/// 1085 units across, so at the edge they lie 59 units under its top and
/// over its bottom. The triangle passes the ascent and the descent by 10
/// units, so its sides cross the edge 49 units inside the 2048 units from
/// the descent to the ascent, which the font helper maps onto the rows.
fn corner_rows(rows: usize) -> u32 {
    (49.0 * rows as f32 / 2048.0).ceil() as u32
}

/// The coverage of column `x` of `glyph`, row by row.
fn column(glyph: &Glyph, x: u32) -> Vec<u8> {
    (0..glyph.height())
        .map(|y| glyph.pixels()[(y * glyph.width() + x) as usize])
        .collect()
}

/// Checks that each separator's flat side reaches its cell edge on `rows`, at every text height and every size, and that the house
/// touches neither side of its cell.
fn separators_meet_their_segments(
    engine: &Engine,
    spacing: u32,
    rows: impl Fn(u32, Band, u32) -> std::ops::Range<u32>,
) {
    let mut wrong = Vec::new();
    for height in 12..=48u32 {
        // the band as vtium-font splits a whole percentage of the height
        let added = (height * spacing + 50) / 100;
        let band = Band {
            above: added.div_ceil(2),
            below: added / 2,
        };
        for size in [
            Size::Normal,
            Size::Doubled(Doubled::Width),
            Size::Doubled(Doubled::Height),
        ] {
            let scale = match size {
                Size::Doubled(Doubled::Height) => 2,
                _ => 1,
            };
            for (c, side) in [(RIGHT, "left"), (LEFT, "right")] {
                let g = drawn(engine, height, band, size, c);
                let (edge, inner) = match side {
                    "left" => (0, 1),
                    _ => (g.width() - 1, g.width() - 2),
                };
                let (edge, inner) = (column(&g, edge), column(&g, inner));
                let want = rows(g.height(), band, scale);
                // A triangle's ink grows toward its flat side, so on each
                // row the edge column holds at least as much ink as the
                // column beside it, and full ink where that column has it.
                // The edge column holds ink on every row but the corners,
                // where the slanted sides meet the flat side.
                let corner = corner_rows(want.len());
                let inside = want.start + corner..want.end - corner;
                let short: Vec<u32> = want
                    .clone()
                    .filter(|&y| {
                        let (e, i) = (edge[y as usize], inner[y as usize]);
                        (e == 0 && inside.contains(&y)) || e < i || (i == 255 && e != 255)
                    })
                    .collect();
                let middle = edge[((want.start + want.end) / 2) as usize];
                if !short.is_empty() || middle != 255 {
                    wrong.push(format!(
                        "height {height} {size:?}: {c:?} falls short of its {side} edge in rows {short:?} of {want:?}, {middle} in the middle row: {edge:?}"
                    ));
                }
            }
            let house = drawn(engine, height, band, size, HOUSE);
            let sides = [0, house.width() - 1];
            if sides
                .iter()
                .any(|&x| column(&house, x).iter().any(|&p| p > 0))
            {
                wrong.push(format!(
                    "height {height} {size:?}: the house reaches a side of its cell"
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// Under the default fit, a Powerline separator is fill: it spans the whole
// cell, so the segment beside it joins it on every row. With no line
// spacing the whole cell is the text's rows.
#[test]
fn a_merged_separator_reaches_its_flat_edge_on_every_row() {
    let engine = engine("plain", "");
    separators_meet_their_segments(&engine, 0, |h, _, _| 0..h);
}

// Line spacing adds rows above and below the text, and the default fit
// stretches a separator over them too.
#[test]
fn a_merged_separator_fills_a_spaced_row_edge_to_edge() {
    let engine = engine("spaced", "line-spacing=30\n");
    separators_meet_their_segments(&engine, 30, |h, _, _| 0..h);
}

// Under fit=natural a separator keeps the text's rows, with the added rows
// blank above and below it, but it still reaches its cell's side edge on
// each of the text's rows.
#[test]
fn a_natural_separator_reaches_its_flat_edge_on_the_texts_rows() {
    let engine = engine("natural", "fit=natural\nline-spacing=30\n");
    separators_meet_their_segments(&engine, 30, |h, band, scale| {
        band.above * scale..h - band.below * scale
    });
}
