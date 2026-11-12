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

/// The same subset without the precomposed capitals, so HarfRust cannot
/// compose a capital and its marks into one glyph and places each mark on
/// its letter instead (`tests/fixtures/README.md`).
fn marks_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DejaVuSansMono-marks.ttf")
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

/// The glyphs the helper sends from the font at `path` for `chars` and
/// `clusters` in a cell of text `text` pixels high with `band` added, and
/// the cell's width.
fn helper(
    path: &Path,
    (text, band): (u32, Band),
    chars: &[char],
    clusters: &[&[char]],
) -> (Answer, u32) {
    let font = std::fs::read(path).unwrap();
    let width = cell_width(&[(&font, "")], text).unwrap();
    let cell = Cell {
        width,
        height: text + band.added(),
    };
    let request = ServeRequest {
        kind: ASK,
        frit: FritText {
            name: "accents".to_owned(),
            text: format!("source=outline:{}\n", path.display()),
        },
        cell,
        band,
        style: Style::default(),
        script: vtium_font::Script::Baseline,
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

/// `key`, a character or a cluster, drawn uncut from the font at `path`:
/// in a cell three times `text` high, on a baseline a whole row lower, at
/// the size the helper draws a `text`-pixel cell at. Each row of the
/// result is a row of the text band from `-text` to `2 * text`.
fn uncut(path: &Path, text: u32, width: u32, key: &[char]) -> Vec<u8> {
    let font = std::fs::read(path).unwrap();
    let ppem = size(&font, "", text).unwrap();
    let base = baseline(&font, "", text, None, Hinting::Light).unwrap();
    let scalars: Vec<u32> = key.iter().map(|&c| u32::from(c)).collect();
    let o = rasterise(&OutlineRequest {
        font: font.into(),
        family: String::new(),
        cells: vec![(width, 3 * text)],
        cover: scalars.iter().map(|&c| (c, c)).collect(),
        clusters: if key.len() > 1 {
            vec![vtium_config::shaping::ask(&scalars)]
        } else {
            Vec::new()
        },
        baselines: vec![base + text],
        sizes: vec![ppem.to_bits()],
        reaches: Vec::new(),
        hinting: Hinting::Light,
        fit: false,
        synth: Default::default(),
        script: Default::default(),
    })
    .unwrap();
    let s = &o.strikes[0];
    let g = if key.len() > 1 {
        s.clusters.iter().find(|(k, _)| *k == scalars).unwrap().1
    } else {
        s.map.iter().find(|(cp, _)| *cp == scalars[0]).unwrap().1
    };
    let g = &s.glyphs[g as usize];
    assert!(g.ink.is_none(), "{key:?}: no ink past the tall cell");
    // nothing reaches the tall cell's top or bottom row, so nothing is cut
    let rows: Vec<&[u8]> = g.pixels.chunks(width as usize).collect();
    assert!(
        rows[0].iter().all(|&p| p == 0),
        "{key:?}: the tall cell's top"
    );
    assert!(
        rows[rows.len() - 1].iter().all(|&p| p == 0),
        "{key:?}: the tall cell's bottom"
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
        let (face, width) = helper(&fixture(), (text, band), &CAPITALS, &[]);
        for c in CAPITALS {
            let what = format!("{c} at {text} with {band:?}");
            let glyph = face.draw(c);
            let want = uncut(&fixture(), text, width, &[c]);
            holds(&glyph, &want, (text, width, band), &what);
        }
    }
    // in a 16-pixel cell with no spacing, the acute on É passes the cell's
    // top, so É carries a box that starts above its row
    let (face, _) = helper(&fixture(), (16, Band::NONE), &['É'], &[]);
    let e = face.draw('É');
    let ink = e.ink().expect("É's box");
    assert!(
        ink.top() < 0,
        "the acute reaches the row above: {}",
        ink.top()
    );
}

// ADR-1306 D2: inside the reach a mark draws where HarfRust places it. In
// the subset that holds the precomposed capitals, HarfRust composes E and
// U+0301 into É's own glyph, so the helper draws the sequence as É; the
// subset without them makes HarfRust place each mark on its letter by the
// font's mark attachment, and the helper's mark rule judges it. Each
// decomposed capital keeps its marks where HarfRust put them, neither moved
// nor cut, at 16 and 32 pixels, with and without line spacing: its own
// pixels and its box hold the cluster drawn uncut. The font places a
// composed accent and an attached one a pixel or two apart in places, as
// É's acute at 32 pixels, so the two need not draw alike
// covers: text.overhang
#[test]
fn a_decomposed_capital_keeps_its_marks_where_the_font_puts_them() {
    let keys: [&[char]; 4] = [
        &['E', '\u{301}'],
        &['A', '\u{300}'],
        &['O', '\u{30b}'],
        &['O', '\u{302}', '\u{323}'],
    ];
    for (text, band) in sizes() {
        let (marked, width) = helper(&marks_fixture(), (text, band), &[], &keys);
        for key in keys {
            let what = format!("{key:?} at {text} with {band:?}");
            let glyph = marked.cluster(key).expect("the helper draws the cluster");
            let want = uncut(&marks_fixture(), text, width, key);
            holds(&glyph, &want, (text, width, band), &what);
        }
    }
    // with the precomposed capital in the font, HarfRust composes the
    // sequence into its glyph, so the two draw alike
    let (whole, _) = helper(&fixture(), (16, Band::NONE), &['É'], &[&['E', '\u{301}']]);
    let composed = whole.cluster(&['E', '\u{301}']).expect("the cluster");
    let precomposed = whole.draw('É');
    assert_eq!(
        (composed.pixels(), composed.ink()),
        (precomposed.pixels(), precomposed.ink())
    );
}

// ADR-1306 D2: the baseline rises so the underscore and ASCII's
// descenders end inside their own cell, so printable ASCII sends no ink
// into another row
#[test]
fn printable_ascii_keeps_its_ink_in_its_row() {
    let ascii: Vec<char> = (0x21u8..0x7f).map(char::from).collect();
    for text in [16, 32] {
        let (face, _) = helper(&fixture(), (text, Band::NONE), &ascii, &[]);
        for &c in &ascii {
            let g = face.draw(c);
            assert!(g.ink().is_none(), "{c} at {text}");
        }
    }
}
