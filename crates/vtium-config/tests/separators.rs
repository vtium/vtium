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
    monoid_shaped(glyphs, vec![(0xe0b0, 1), (0xe0b2, 2), (0xf015, 3)])
}

/// A symbol font with `glyphs`, mapped by `cmap`, laid out as Monoid Nerd
/// Font Mono is: 1536 units to the em, an ascent of 1664 and a descent of
/// 384.
fn monoid_shaped(glyphs: Vec<ttf::Glyph>, cmap: Vec<(u32, u16)>) -> Vec<u8> {
    ttf::Font {
        family: "Separator Test".to_owned(),
        upem: 1536,
        ascent: 1664,
        descent: -384,
        glyphs,
        cmap,
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

/// The engine for DejaVu Sans Mono with the symbol font `font` merged
/// behind it, and `extra` lines after the symbol font's.
fn engine_with(test: &str, font: &[u8], extra: &str) -> Engine {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("vtium-separators-{test}"));
    std::fs::create_dir_all(&dir).expect("the test's directory");
    let path = dir.join("SeparatorTest.ttf");
    std::fs::write(&path, font).expect("the symbol font");
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
    drawn_among(engine, height, band, size, &[c], c)
}

/// `c` as the console reads it from a set that `engine` draws with every
/// character of `among` in one request, at the text height `height`, with
/// `band` added by line spacing, at `size`.
fn drawn_among(
    engine: &Engine,
    height: u32,
    band: Band,
    size: Size,
    among: &[char],
    c: char,
) -> Glyph {
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
    let want: Vec<u32> = among.iter().map(|&c| u32::from(c)).collect();
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

/// A symbol font's separators, as the checks need them: the font, and how
/// far inside the 2048 units from its descent to its ascent the slanted
/// sides of each solid triangle cross the cell's edge.
struct Shape {
    font: Vec<u8>,
    corner: u32,
}

/// The Monoid-shaped font. Each triangle's flat side lies 62 units past
/// the edge, and its slanted sides fall 1034 units over the 1085 units
/// across, so at the edge they lie 59 units under its top and over its
/// bottom. The triangle passes the ascent and the descent by 10 units, so
/// its sides cross the edge 49 units inside the span.
fn monoid() -> Shape {
    Shape {
        font: symbols(),
        corner: 49,
    }
}

/// A symbol font laid out as Symbols Nerd Font Mono lays out its Powerline
/// separators: 2048 units to the em, an ascent of 1638 and a descent of
/// 410, every glyph advancing a whole em, 2048 units. That advance is far
/// wider than DejaVu Sans Mono's cell, and each solid triangle is drawn
/// only about 0.65 em wide: U+E0B0 from 123 units left of the pen to 1325,
/// and U+E0B2 from 723 to 2171, 123 units past the advance. Each passes
/// the ascent and the descent by 11 units. The flat side lies 123 units
/// past the edge, and the slanted sides fall 1035 units over the 1448
/// units across, so at the edge they lie 88 units under the top, which is
/// 77 units inside the span. The house sits in the middle of the advance, narrower than the cell.
fn symbols_nerd_shaped() -> Shape {
    let glyph = |contours| ttf::Glyph {
        advance: 2048,
        contours,
    };
    let glyphs = vec![
        ttf::Glyph::empty(2048),
        glyph(vec![vec![(-123, -421), (-123, 1649), (1325, 614)]]),
        glyph(vec![vec![(2171, -421), (723, 614), (2171, 1649)]]),
        glyph(vec![vec![
            (850, 300),
            (850, 1100),
            (1198, 1100),
            (1198, 300),
        ]]),
    ];
    let font = ttf::Font {
        family: "Separator Test".to_owned(),
        upem: 2048,
        ascent: 1638,
        descent: -410,
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
    .build();
    Shape { font, corner: 77 }
}

/// The rows at each end of a separator `rows` rows tall where its flat
/// side may hold no ink at its cell's edge: where its slanted sides cross
/// the edge `corner` units inside the 2048 units from the descent to the
/// ascent, which the font helper maps onto the rows.
fn corner_rows(rows: usize, corner: u32) -> u32 {
    (corner as f32 * rows as f32 / 2048.0).ceil() as u32
}

/// The coverage of column `x` of `glyph`, row by row.
fn column(glyph: &Glyph, x: u32) -> Vec<u8> {
    (0..glyph.height())
        .map(|y| glyph.pixels()[(y * glyph.width() + x) as usize])
        .collect()
}

/// Checks that each separator's flat side reaches its cell edge on `rows`,
/// at every text height and every size, that the ink its font draws past
/// that edge lies in its ink box, that its point reaches the opposite
/// edge, and that the house touches neither side of its cell.
/// `corner` is the shape's [`Shape::corner`].
fn separators_meet_their_segments(
    engine: &Engine,
    spacing: u32,
    corner: u32,
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
                let (edge, inner, tip) = match side {
                    "left" => (0, 1, g.width() - 1),
                    _ => (g.width() - 1, g.width() - 2, 0),
                };
                let (edge, inner, tip) = (column(&g, edge), column(&g, inner), column(&g, tip));
                let want = rows(g.height(), band, scale);
                // A triangle's ink grows toward its flat side, so on each
                // row the edge column holds at least as much ink as the
                // column beside it, and full ink where that column has it.
                // The edge column holds ink on every row but the corners,
                // where the slanted sides meet the flat side.
                let corner = corner_rows(want.len(), corner);
                let inside = want.start + corner..want.end - corner;
                let short: Vec<u32> = want
                    .clone()
                    .filter(|&y| {
                        let (e, i) = (edge[y as usize], inner[y as usize]);
                        (e == 0 && inside.contains(&y)) || e < i || (i == 255 && e != 255)
                    })
                    .collect();
                let mid = (want.start + want.end) / 2;
                let middle = edge[mid as usize];
                if !short.is_empty() || middle != 255 {
                    wrong.push(format!(
                        "height {height} {size:?}: {c:?} falls short of its {side} edge in rows {short:?} of {want:?}, {middle} in the middle row: {edge:?}"
                    ));
                }
                // The point lies on the row's middle line, so it holds ink
                // in the opposite edge column on one of the two rows that
                // meet there, or on the row the line runs through.
                if tip[mid as usize - 1].max(tip[mid as usize]) == 0 {
                    wrong.push(format!(
                        "height {height} {size:?}: the point of {c:?} stops short of the edge opposite its {side} edge: {tip:?}"
                    ));
                }
                // The flat side lies past the font's pen or advance, and
                // that overlap stays outside the cell, in the ink box.
                let past = g.ink().is_some_and(|ink| match side {
                    "left" => ink.left() < 0,
                    _ => ink.left() + ink.width() as i32 > g.width() as i32,
                });
                if !past {
                    wrong.push(format!(
                        "height {height} {size:?}: the overlap of {c:?} past its {side} edge is not in its ink box"
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
    let shape = monoid();
    let engine = engine_with("plain", &shape.font, "");
    separators_meet_their_segments(&engine, 0, shape.corner, |h, _, _| 0..h);
}

// Line spacing adds rows above and below the text, and the default fit
// stretches a separator over them too.
#[test]
fn a_merged_separator_fills_a_spaced_row_edge_to_edge() {
    let shape = monoid();
    let engine = engine_with("spaced", &shape.font, "line-spacing=30\n");
    separators_meet_their_segments(&engine, 30, shape.corner, |h, _, _| 0..h);
}

// Under fit=natural a separator keeps the text's rows, with the added rows
// blank above and below it, but it still reaches its cell's side edge on
// each of the text's rows.
#[test]
fn a_natural_separator_reaches_its_flat_edge_on_the_texts_rows() {
    let shape = monoid();
    let engine = engine_with("natural", &shape.font, "fit=natural\nline-spacing=30\n");
    separators_meet_their_segments(&engine, 30, shape.corner, |h, band, scale| {
        band.above * scale..h - band.below * scale
    });
}

// Symbols Nerd Font Mono advances a whole em and draws each solid triangle
// about 0.65 em wide inside that advance. Its separators still fill their
// cells from the flat side to the point, at normal size and on lines of
// double width and double height.
#[test]
fn a_symbols_nerd_shaped_separator_fills_its_cell_to_its_point() {
    let shape = symbols_nerd_shaped();
    let engine = engine_with("symbols-nerd", &shape.font, "");
    separators_meet_their_segments(&engine, 0, shape.corner, |h, _, _| 0..h);
}

// The same, with line spacing: the default fit stretches the separator
// over the added rows too.
#[test]
fn a_symbols_nerd_shaped_separator_fills_a_spaced_cell_to_its_point() {
    let shape = symbols_nerd_shaped();
    let engine = engine_with("symbols-nerd-spaced", &shape.font, "line-spacing=30\n");
    separators_meet_their_segments(&engine, 30, shape.corner, |h, _, _| 0..h);
}

// One glyph that a font maps from an icon before the separators' range,
// U+E0A0, from a separator, U+E0B0, and from an icon after the range,
// U+F015, is placed for each character as it is placed when that character
// is drawn alone: the separator fills its cell, and the icons keep the
// text's size. Drawn in one request, the characters share the glyph but
// not its placement.
#[test]
fn a_glyph_shared_by_a_separator_and_icons_is_placed_for_each() {
    let triangle = ttf::Glyph {
        advance: 1024,
        contours: vec![vec![(0, -384), (0, 1664), (1024, 640)]],
    };
    let font = monoid_shaped(
        vec![ttf::Glyph::empty(1024), triangle],
        vec![(0xe0a0, 1), (0xe0b0, 1), (0xf015, 1)],
    );
    let engine = engine_with("shared", &font, "");
    let all = ['\u{e0a0}', RIGHT, HOUSE];
    let mut wrong = Vec::new();
    for height in [16, 24] {
        for size in [
            Size::Normal,
            Size::Doubled(Doubled::Width),
            Size::Doubled(Doubled::Height),
        ] {
            for c in all {
                let alone = drawn(&engine, height, Band::NONE, size, c);
                let together = drawn_among(&engine, height, Band::NONE, size, &all, c);
                if alone != together {
                    wrong.push(format!(
                        "height {height} {size:?}: {c:?} drawn with the others differs from {c:?} drawn alone"
                    ));
                }
            }
            // the separator and the icon are placed differently, or the
            // comparison above could not tell their placements apart
            let separator = drawn(&engine, height, Band::NONE, size, RIGHT);
            let icon = drawn(&engine, height, Band::NONE, size, HOUSE);
            assert_ne!(separator, icon, "height {height} {size:?}");
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The characters the non-separator test draws: letters from DejaVu Sans
/// Mono, and from the symbol font the Powerline icons U+E0A0 and U+E0A1,
/// the characters on each side of the separators' range, U+E0AF and
/// U+E0D8, and two icons, U+F015 and U+F016.
const NOT_SEPARATORS: [char; 14] = [
    'A', 'g', '|', 'M', '_', 'W', '0', '@', '\u{e0a0}', '\u{e0a1}', '\u{e0af}', '\u{e0d8}',
    '\u{f015}', '\u{f016}',
];

/// The text heights the non-separator test draws at.
const NOT_SEPARATOR_HEIGHTS: [u32; 6] = [12, 16, 17, 24, 31, 48];

/// The 64-bit FNV-1a hash of `bytes`, which stays the same from one build
/// and one Rust release to the next.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Each glyph the non-separator test draws, one line per text height and
/// size, each line a hash of every character's glyph as the console reads
/// it: its size, coverage and ink box. The placement that draws no
/// separator differently, the candidate's at commit cd0bc28fa, gave these.
const NOT_SEPARATOR_DIGESTS: &str = "
12 Normal cb0b4002c95279cd 501fd92c2015f8c9 7ddd5ae55fdb952d 19c7e90b035b5659 520b0a4c13905b9a a5ed7c0a5425302f b6e0a3c92be122a3 a63f8664af84e304 fe313a084557d4a8 9ade9af973faddfc fe313a084557d4a8 fe313a084557d4a8 fe313a084557d4a8 9ade9af973faddfc
12 Doubled(Width) e87ab330a3cb9274 33bfab0173034d85 fe02480cf91c5923 b782b360c386cffa 56d877d00e683c02 a2224426362c0ac6 229be9e050679de0 56a3114c5235e26f 8e218a1c3348b55f 86fe3f4b56da8b62 8e218a1c3348b55f 8e218a1c3348b55f 8e218a1c3348b55f 86fe3f4b56da8b62
12 Doubled(Height) 9256a2fedbae4890 a61ac4f010483232 171307f305b85378 40b13c17a4e74649 d503a5189044f927 efab040a7f45cb59 0e8520813c0450ee c8516e3f54ce1cac c31afe5db4536bf1 cf007a7087ccfa5b c31afe5db4536bf1 c31afe5db4536bf1 c31afe5db4536bf1 cf007a7087ccfa5b
16 Normal 611a8ca441591103 d4589cf56985ad45 5307d9a7d6dbe222 c4fa9ecb1f26cac0 d40f6dfc3e628254 c7cb0709dd01b6e0 7cfb259833783e0f 489ab828f3bb0f99 fb0580cf8cf3189f 921db77607c4991a fb0580cf8cf3189f fb0580cf8cf3189f fb0580cf8cf3189f 921db77607c4991a
16 Doubled(Width) 2bb2ddb7b8d889d3 b6a24df60278f2c2 2660b58c3101a6ae e0dcc8350c04da31 3cd32a3ddcabe854 596f9567b51fa080 07774d2b35760756 678e0d3a92be8ee4 f06296b80358cc4b 035df1b9b67f56a5 f06296b80358cc4b f06296b80358cc4b f06296b80358cc4b 035df1b9b67f56a5
16 Doubled(Height) a4f2647e632b3d9e c913eaa8f6cfa09e 4e2930c841c7e6ef 0b70f0c27c6ee81e 36046e414bb1b3e3 74befeea835e6060 cc7774ab2c26a294 4fbbc1eb56e54551 9d0f4b4b8f9b95a3 8fa92f16b823f21f 9d0f4b4b8f9b95a3 9d0f4b4b8f9b95a3 9d0f4b4b8f9b95a3 8fa92f16b823f21f
17 Normal 108c9584916d52e8 3abd5f13c2a2db09 51e3cdc4a571d969 2c802ec9a43d865a 03a8b0f2aca12168 4d310f2906f98f65 d182178fe23ebf5e 8e175ea26238dbe1 c48d2ec1e834317a ea5109be3a8765a0 c48d2ec1e834317a c48d2ec1e834317a c48d2ec1e834317a ea5109be3a8765a0
17 Doubled(Width) e7deaffa26551145 b8d36c1f932ecb45 822ae0cbc20bac95 4ea9f8faf00895ea b87a03decc3f2bc1 414d247eedb7f277 52d1c4ae84f5ad68 df27eb0e6892ac0b 237a396859e5ad39 dc04066d78af0255 237a396859e5ad39 237a396859e5ad39 237a396859e5ad39 dc04066d78af0255
17 Doubled(Height) 04f7c56b99ea0b4f b740fa6a7632743d 56e41d731a85bdf0 5f9b6565c2cc2156 3e03ab7e62cad4fd 09916983c825b8ab c9af0b8204daa321 6339f29902467200 19033c0f8f60bb7f 11e4f0c9f86ccea1 19033c0f8f60bb7f 19033c0f8f60bb7f 19033c0f8f60bb7f 11e4f0c9f86ccea1
24 Normal 339cd74817e0328d 60fd7eac8d0c2e43 58a5b6cc0164e1be 08cb480b0143d531 046251c40945b58f 8003e03c849a94bf cb0962cbb2b764b5 e544f34763477d59 06baf62344ee809f d6dcee5ee064a666 06baf62344ee809f 06baf62344ee809f 06baf62344ee809f d6dcee5ee064a666
24 Doubled(Width) ad70757b188377aa a970ddc178c3e499 988799474a913f92 2f784898bdda19c8 2ea1076a55774c59 6aae547b58897d55 12d6d20845c129ad 88409d0f575cf555 754b88192599f2f5 a6160f0d89a55c16 754b88192599f2f5 754b88192599f2f5 754b88192599f2f5 a6160f0d89a55c16
24 Doubled(Height) 08dc2a128feca0e0 0b630666ed608f02 eec8e71f4c90c92a 85df8448f50c2228 2f755b38ae6d39ae fa1a4779b83af072 28783011cedbbe6a eac0d3d4cafa8349 8695d950873a8eee 0b1334c68c67d90f 8695d950873a8eee 8695d950873a8eee 8695d950873a8eee 0b1334c68c67d90f
31 Normal 29ff993deb5848d8 ddaf9d71a7db8298 f880499b7b142182 7a1acced0c7aa7f7 f8541d60a8c054e2 0f29d2888d6b096e ca23dcc972e0fa4c b0c5e6568f731838 83ccbc0d6e85aa0a 538db6bcc58ac493 83ccbc0d6e85aa0a 83ccbc0d6e85aa0a 83ccbc0d6e85aa0a 538db6bcc58ac493
31 Doubled(Width) eec02e0a00cbf757 39e0583f730ed6de a76178e6db3772f1 44a422bda38bdcd5 977e9e835df92356 5d3a70d5a05d6958 3b6353c982373eb3 79d2d0d22e34bb0e 040b63c01e9c54d5 b1f6bdc3209f8b58 040b63c01e9c54d5 040b63c01e9c54d5 040b63c01e9c54d5 b1f6bdc3209f8b58
31 Doubled(Height) 476e568ce1f487bd 5289edf1d4ff3e67 cf5bfe577af41cef 57529939dd2ac630 2a750b77a0935d57 052ff340d3bbb327 a9380b4550bfbde4 161e11d92f5209be 76e17e0018b646c4 f6c7e2b8a5cf609c 76e17e0018b646c4 76e17e0018b646c4 76e17e0018b646c4 f6c7e2b8a5cf609c
48 Normal 285429706fe34db2 6e3693b08de8cbe2 eb61cee5509a1f60 d70c83502fb18b80 81c74d8e5db3cfdd 251ac548ace8f78b f9ceb9414790f83d 78a128a4de687595 1e7d443ae944c511 1ce3142f0afa02b1 1e7d443ae944c511 1e7d443ae944c511 1e7d443ae944c511 1ce3142f0afa02b1
48 Doubled(Width) fc20dd96972d3b07 682a44b13428de48 685aa78eb861afe1 fba03c07d36873db f656b7261b665f5c 8ce9a688f3345656 23648f8e60323b60 757ed0a8a86ba5b7 49f01322261ea185 938129d2dc3a2420 49f01322261ea185 49f01322261ea185 49f01322261ea185 938129d2dc3a2420
48 Doubled(Height) ffd2663199f23fe3 de620aa128f59dfe 701bb77377ef0ab8 6a9d93dbd743624f 0dbb56f060dcff53 e517f3f8f885dadb de961c004e1531bf 9802a4ce17651583 f47d2c94d57b457f 810d300b14986334 f47d2c94d57b457f f47d2c94d57b457f f47d2c94d57b457f 810d300b14986334
";

// Only U+E0B0 to U+E0D7 are placed as separators. Letters, the Powerline
// icons before the range, the characters on each side of it, and other
// icons draw exactly as the placement that draws no separator differently
// draws them. U+E0A0, U+E0AF, U+E0D8 and U+F015 use a glyph too big for its
// cell, which the default fit draws smaller, and U+E0A1 and U+F016 one
// that fits.
#[test]
fn only_the_separators_range_is_placed_as_separators() {
    let big = ttf::Glyph {
        advance: 1024,
        contours: vec![vec![(-62, -394), (-62, 1674), (1086, 640)]],
    };
    let small = ttf::Glyph::rect(1024, 200, 200, 824, 1000);
    let font = monoid_shaped(
        vec![ttf::Glyph::empty(1024), big, small],
        vec![
            (0xe0a0, 1),
            (0xe0a1, 2),
            (0xe0af, 1),
            (0xe0d8, 1),
            (0xf015, 1),
            (0xf016, 2),
        ],
    );
    let engine = engine_with("not-separators", &font, "");
    let mut lines = Vec::new();
    for height in NOT_SEPARATOR_HEIGHTS {
        for size in [
            Size::Normal,
            Size::Doubled(Doubled::Width),
            Size::Doubled(Doubled::Height),
        ] {
            let hashes: Vec<String> = NOT_SEPARATORS
                .iter()
                .map(|&c| {
                    let g = drawn(&engine, height, Band::NONE, size, c);
                    format!("{:016x}", fnv1a(format!("{g:?}").as_bytes()))
                })
                .collect();
            lines.push(format!("{height} {size:?} {}", hashes.join(" ")));
        }
    }
    let got = lines.join("\n");
    assert!(
        got == NOT_SEPARATOR_DIGESTS.trim(),
        "the glyphs drew differently:\n{got}"
    );
}
