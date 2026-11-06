//! Accented capitals keep their accents (ADR-1306 D1, D2, D8): in DejaVu
//! Sans Mono, hinted lightly, the helper draws each glyph's ink whole into
//! its ink box, up to a row above and below its own row, with the accent
//! where the font puts it, at 16- and 32-pixel cells, with and without
//! added line spacing.
//!
//! The ink each glyph should have is drawn once more by the rasteriser in
//! a cell three rows high, on a baseline a whole row lower and at the same
//! size, so nothing above or below the glyph's own row is cut there. What
//! the console reads from the helper's answer, the glyph's own pixels and
//! its ink box, must hold exactly that ink.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "a test fails by panicking"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use vtium_config::frit::Hinting;
use vtium_config::outline::{OutlineRequest, baseline, cell_width, rasterise, size};
use vtium_config::serve::{ASK, FRAME_GLYPHS, FritText, Paths, ServeRequest, serve};
use vtium_font::set::{Set, SetFace};
use vtium_font::spacing::Band;
use vtium_font::{Cell, Face, Glyph, REACH_ROWS, Style};

/// A subset of DejaVu Sans Mono 2.37: printable ASCII, the capitals below
/// and the combining marks they decompose to (`tests/fixtures/README.md`).
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DejaVuSansMono-accents.ttf")
}

/// À É Î Õ Ü Ő Ű and Ộ, each of whose accents passes the top of a
/// 16-pixel cell in DejaVu Sans Mono or comes close to it.
const CAPITALS: [char; 8] = ['À', 'É', 'Î', 'Õ', 'Ü', 'Ő', 'Ű', 'Ộ'];

/// The glyph frames the helper sends for an ask, each a face at the whole
/// cell: the codepoints come in one frame and the clusters in another.
struct Answer(Vec<SetFace>);

impl Answer {
    fn draw(&self, c: char) -> Glyph {
        self.0
            .iter()
            .find_map(|f| f.draw(&[c], Style::default()))
            .unwrap()
    }

    fn cluster(&self, key: &[char]) -> Option<Glyph> {
        self.0.iter().find_map(|f| f.cluster(key, Style::default()))
    }
}

/// The glyphs the helper sends for `chars` and `clusters` in a cell of
/// text `text` pixels high with `band` added, and the cell's width.
fn helper(text: u32, band: Band, chars: &[char], clusters: &[&[char]]) -> (Answer, u32) {
    let font = std::fs::read(fixture()).unwrap();
    let width = cell_width(&[(&font, "")], text).unwrap();
    let cell = Cell {
        width,
        height: text + band.added(),
    };
    let request = ServeRequest {
        kind: ASK,
        frit: FritText {
            name: "accents".to_owned(),
            text: format!("source=outline:{}\n", fixture().display()),
        },
        cell,
        band,
        style: Style::default(),
        codepoints: chars.iter().map(|&c| u32::from(c)).collect(),
        clusters: clusters
            .iter()
            .map(|k| k.iter().map(|&c| u32::from(c)).collect())
            .collect(),
        held: Vec::new(),
        console: None,
    };
    let mut out = Vec::new();
    let input = std::io::Cursor::new(request.encode());
    assert_eq!(
        serve(input, &mut out, &Paths::default(), Duration::from_secs(5)),
        0
    );
    let mut bytes = out.as_slice();
    let mut faces = Vec::new();
    while bytes.len() >= 8 {
        let kind = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let payload = &bytes[8..8 + len];
        if kind == FRAME_GLYPHS {
            // the band the frame names, then the set
            let set = Set::from_bytes(payload[8..].to_vec()).unwrap();
            faces.push(SetFace::new(Arc::new(set), cell.height).unwrap());
        }
        bytes = &bytes[8 + len..];
    }
    assert!(!faces.is_empty(), "no glyph frame");
    (Answer(faces), width)
}

/// `c` drawn uncut: in a cell three times `text` high, on a baseline a
/// whole row lower, at the size the helper draws a `text`-pixel cell at.
/// Each row of the result is a row of the text band from `-text` to
/// `2 * text`.
fn uncut(text: u32, width: u32, c: char) -> Vec<u8> {
    let font = std::fs::read(fixture()).unwrap();
    let ppem = size(&font, "", text).unwrap();
    let base = baseline(&font, "", text, None, Hinting::Light).unwrap();
    let o = rasterise(&OutlineRequest {
        font: font.into(),
        family: String::new(),
        cells: vec![(width, 3 * text)],
        cover: vec![(u32::from(c), u32::from(c))],
        clusters: Vec::new(),
        baselines: vec![base + text],
        sizes: vec![ppem.to_bits()],
        reaches: Vec::new(),
        hinting: Hinting::Light,
        fit: false,
        synth: Default::default(),
    })
    .unwrap();
    let s = &o.strikes[0];
    let (_, g) = s.map.iter().find(|(cp, _)| *cp == u32::from(c)).unwrap();
    let g = &s.glyphs[*g as usize];
    assert!(g.ink.is_none(), "{c}: no ink past the tall cell");
    // nothing reaches the tall cell's top or bottom row, so nothing is cut
    let rows: Vec<&[u8]> = g.pixels.chunks(width as usize).collect();
    assert!(rows[0].iter().all(|&p| p == 0), "{c}: the tall cell's top");
    assert!(
        rows[rows.len() - 1].iter().all(|&p| p == 0),
        "{c}: the tall cell's bottom"
    );
    g.pixels.clone()
}

/// The coverage `glyph` draws at column `x` and row `y` of its cell, its
/// box's where it has one and the box holds the pixel, else its own.
fn ink_at(glyph: &Glyph, x: i32, y: i32) -> u8 {
    if let Some(ink) = glyph.ink() {
        let (bx, by) = (x - ink.left(), y - ink.top());
        if (0..ink.width() as i32).contains(&bx) && (0..ink.height() as i32).contains(&by) {
            return ink.pixels()[(by as u32 * ink.width() + bx as u32) as usize];
        }
    }
    if (0..glyph.width() as i32).contains(&x) && (0..glyph.height() as i32).contains(&y) {
        return glyph.pixels()[(y as u32 * glyph.width() + x as u32) as usize];
    }
    0
}

/// Checks that `glyph`, drawn by the helper in a cell of text `text`
/// pixels high with `band` added, holds exactly the ink `want` ([`uncut`])
/// holds, its text band `band.above` rows down its cell, and nothing else.
fn holds(glyph: &Glyph, want: &[u8], (text, width, band): (u32, u32, Band), what: &str) {
    let h = text as i32;
    let reach = (glyph.height() * REACH_ROWS) as i32;
    let above = band.above as i32;
    // the reference covers the text band's rows from -h to 2h, inside the
    // whole cell's reach
    assert!(above - h >= -reach && above + 2 * h <= glyph.height() as i32 + reach);
    for r in -h..2 * h {
        for x in 0..width as i32 {
            let want = want[((r + h) as u32 * width + x as u32) as usize];
            assert_eq!(
                ink_at(glyph, x, r + above),
                want,
                "{what}: text band row {r}, column {x}"
            );
        }
    }
    if let Some(ink) = glyph.ink() {
        assert_eq!(
            (ink.left(), ink.width()),
            (0, width),
            "{what}: no ink beside the cell"
        );
    }
}

/// The text heights and bands the capitals are drawn at: 16 and 32 pixels,
/// with no spacing, with 10 per cent and with 25 per cent.
fn sizes() -> Vec<(u32, Band)> {
    [16, 32]
        .into_iter()
        .flat_map(|text| [0, 10, 25].map(|percent| (text, Band::of(text, percent))))
        .collect()
}

// ADR-1306 D1, D2 and D8, the work about accents cut off capital letters:
// each capital's ink lies wholly in its own pixels and its ink box, its
// accent where the font puts it, neither moved nor cut, at 16 and 32 pixels,
// with and without line spacing
// covers: text.overhang
#[test]
fn accented_capitals_keep_their_whole_accents() {
    for (text, band) in sizes() {
        let (face, width) = helper(text, band, &CAPITALS, &[]);
        for c in CAPITALS {
            let what = format!("{c} at {text} with {band:?}");
            let glyph = face.draw(c);
            holds(&glyph, &uncut(text, width, c), (text, width, band), &what);
        }
    }
    // in a 16-pixel cell with no spacing, the acute on É passes the cell's
    // top, so É carries a box that starts above its row
    let (face, _) = helper(16, Band::NONE, &['É'], &[]);
    let e = face.draw('É');
    let ink = e.ink().expect("É's box");
    assert!(
        ink.top() < 0,
        "the acute reaches the row above: {}",
        ink.top()
    );
}

// ADR-1306 D2: a precomposed É and E followed by U+0301 draw the same ink,
// since neither is moved or cut while its ink stays within a row of its own
// covers: text.overhang
#[test]
fn a_precomposed_and_a_decomposed_capital_draw_alike() {
    let pairs: [(char, &[char]); 3] = [
        ('É', &['E', '\u{301}']),
        ('À', &['A', '\u{300}']),
        ('Ő', &['O', '\u{30b}']),
    ];
    for (text, band) in sizes() {
        let keys: Vec<&[char]> = pairs.iter().map(|p| p.1).collect();
        let chars: Vec<char> = pairs.iter().map(|p| p.0).collect();
        let (face, width) = helper(text, band, &chars, &keys);
        for (one, two) in pairs {
            let what = format!("{one} at {text} with {band:?}");
            let precomposed = face.draw(one);
            let decomposed = face.cluster(two).expect("the helper draws the cluster");
            let h = (text + band.added()) as i32;
            for y in -h..2 * h {
                for x in 0..width as i32 {
                    assert_eq!(
                        ink_at(&decomposed, x, y),
                        ink_at(&precomposed, x, y),
                        "{what}: row {y}, column {x}"
                    );
                }
            }
        }
    }
}

// ADR-1306 D2: the baseline rises so the underscore and ASCII's
// descenders end inside their own cell, so printable ASCII sends no ink
// into another row
#[test]
fn printable_ascii_keeps_its_ink_in_its_row() {
    let ascii: Vec<char> = (0x21u8..0x7f).map(char::from).collect();
    for text in [16, 32] {
        let (face, _) = helper(text, Band::NONE, &ascii, &[]);
        for &c in &ascii {
            let g = face.draw(c);
            assert!(g.ink().is_none(), "{c} at {text}");
        }
    }
}
