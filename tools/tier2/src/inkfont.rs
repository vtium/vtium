//! The ink test font: an outline font built in memory, whose glyphs send
//! their ink past their cells by known amounts (ADR-1302, ADR-1306).
//!
//! Tier 2 reads no font of the system's, so the screens that judge where
//! ink past a cell draws use this font, written as vtium-config's tests
//! write theirs (`crates/vtium-config/tests/common/ttf.rs`, which this
//! module includes rather than copies). The font has 1000 units to the
//! em, an ascent of 800 and a descent of 200. The glyph helper fits the
//! ascent and descent to the cell less half a pixel, so the font draws at
//! a little under one pixel to the em for each pixel of the cell's height,
//! and every distance below is close to the fraction of the cell it gives:
//! 100 units are about a tenth of the cell's height, and 150 units about a
//! quarter of its width, since every narrow glyph advances 600.
//!
//! Each glyph is a few straight-edged shapes. The capitals, the lower case
//! letters with descenders and the underscore keep their ink inside the
//! cell, as printable ASCII must (ADR-1302 D3). Every other glyph passes
//! its cell by the amount its comment gives:
//!
//! - the accents of À (0.1 of the cell above the row), É (0.2), Î (0.3),
//!   Ấ (0.5) and Ộ (0.28 above and 0.13 below), and the same accents as
//!   combining marks, U+0300, U+0301, U+0302 and U+0323;
//! - the cedilla of ç, 0.25 below the row;
//! - ƒ, whose hooks reach a quarter of a cell to the left and the right;
//! - the ideograph 一, two cells wide, whose bar reaches a quarter of a
//!   cell past each side;
//! - Arabic: alef, beh and noon, which stay in the row, and lam, whose
//!   stem passes the row's top by 0.08; the marks shadda (0.13 above), fatha (0.26), damma (0.32) and
//!   sukun (0.34) above, kasra (0.12) and kasratan (0.22) below; and the
//!   superscript alef, U+0670, which stands between 0.75 and 1.1 of the
//!   cell above the row, so it passes the one row that ink may reach and
//!   the helper moves or scales it (ADR-1306 D2).
//!
//! The font has an empty `GPOS` table, so HarfRust places no mark itself:
//! each mark draws where its outline stands, from the pen HarfRust leaves
//! it at. A Latin mark follows its base from left to right, so its pen is
//! after the base and its outline stands over the 600 units before the
//! pen. An Arabic mark is shaped from right to left, so its pen is the
//! base's own and its outline stands over the base directly.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[path = "../../../crates/vtium-config/tests/common/ttf.rs"]
mod ttf;

use ttf::{Font, Glyph};

/// A contour of straight edges, clockwise, as TrueType fills.
type Contour = Vec<(i16, i16)>;

/// The rectangle `x0, y0` to `x1, y1`, clockwise.
fn rect(x0: i16, y0: i16, x1: i16, y1: i16) -> Contour {
    vec![(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
}

/// The same rectangle counter-clockwise: a hole in a shape around it.
fn hole(x0: i16, y0: i16, x1: i16, y1: i16) -> Contour {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// A stroke from `x0..x1` at `y0` up to `y1`, its top moved across by
/// `dx`: an accent leaning right for a positive `dx`, left for a negative.
fn slant(x0: i16, y0: i16, x1: i16, y1: i16, dx: i16) -> Contour {
    vec![(x0, y0), (x0 + dx, y1), (x1 + dx, y1), (x1, y0)]
}

/// A circumflex from `y0` up to `y1`, centred on `x`.
fn chevron(x: i16, y0: i16, y1: i16) -> Contour {
    let mid = y1 - (y1 - y0) / 2;
    vec![
        (x - 160, y0),
        (x, y1),
        (x + 160, y0),
        (x + 80, y0),
        (x, mid),
        (x - 80, y0),
    ]
}

/// `contours` moved across by `dx`: a base letter's accent drawn as a
/// combining mark, which stands over the 600 units before its pen.
fn moved(contours: &[Contour], dx: i16) -> Vec<Contour> {
    contours
        .iter()
        .map(|c| c.iter().map(|&(x, y)| (x + dx, y)).collect())
        .collect()
}

fn glyph(advance: u16, contours: Vec<Contour>) -> Glyph {
    Glyph { advance, contours }
}

/// The capitals' shapes, inside the cell from the baseline to 700.
fn capital(c: char) -> Vec<Contour> {
    match c {
        'A' => vec![vec![(60, 0), (300, 700), (540, 0)]],
        'E' => vec![
            rect(100, 0, 190, 700),
            rect(100, 0, 500, 90),
            rect(100, 310, 440, 390),
            rect(100, 610, 500, 700),
        ],
        'I' => vec![
            rect(250, 0, 350, 700),
            rect(150, 0, 450, 80),
            rect(150, 620, 450, 700),
        ],
        'O' => vec![rect(100, 0, 500, 700), hole(190, 90, 410, 610)],
        'H' => vec![
            rect(100, 0, 190, 700),
            rect(410, 0, 500, 700),
            rect(100, 310, 500, 390),
        ],
        // W fills its advance, so its ink sets the cell's width
        _ => vec![
            rect(0, 0, 90, 700),
            rect(510, 0, 600, 700),
            rect(0, 0, 600, 90),
            rect(255, 0, 345, 500),
        ],
    }
}

/// A lower case bowl, from the baseline to 500.
fn bowl() -> Vec<Contour> {
    vec![rect(100, 0, 500, 500), hole(180, 80, 420, 420)]
}

/// The accents, over a glyph whose pen is at 0.
fn grave() -> Vec<Contour> {
    // 900: 0.1 of the cell above the row
    vec![slant(320, 760, 400, 900, -120)]
}

fn acute() -> Vec<Contour> {
    // 1000: 0.2 above
    vec![slant(220, 780, 300, 1000, 120)]
}

fn circumflex() -> Vec<Contour> {
    // 1100: 0.3 above
    vec![chevron(300, 800, 1100)]
}

fn dot_below() -> Vec<Contour> {
    // -330: 0.13 below
    vec![rect(260, -330, 340, -250)]
}

/// The font's characters and their glyphs, a space first.
fn glyphs() -> Vec<(char, Glyph)> {
    let mut all = vec![(' ', glyph(600, Vec::new()))];
    for c in ['A', 'E', 'I', 'O', 'H', 'W'] {
        all.push((c, glyph(600, capital(c))));
    }
    let with = |base: Vec<Contour>, more: Vec<Contour>| glyph(600, [base, more].concat());
    all.extend([
        ('À', with(capital('A'), grave())),
        ('É', with(capital('E'), acute())),
        ('Î', with(capital('I'), circumflex())),
        // a circumflex to 1000 and an acute over it to 1300: 0.5 above
        (
            'Ấ',
            with(
                capital('A'),
                vec![chevron(300, 760, 1000), slant(330, 1040, 410, 1300, 100)],
            ),
        ),
        // a circumflex to 1080, 0.28 above, and a dot 0.13 below
        (
            'Ộ',
            with(
                capital('O'),
                [vec![chevron(300, 820, 1080)], dot_below()].concat(),
            ),
        ),
    ]);
    all.extend([
        (
            'g',
            with(
                bowl(),
                vec![rect(420, -200, 500, 0), rect(100, -200, 500, -120)],
            ),
        ),
        (
            'j',
            glyph(
                600,
                vec![
                    rect(300, -200, 380, 500),
                    rect(120, -200, 380, -120),
                    rect(300, 600, 380, 700),
                ],
            ),
        ),
        ('p', with(bowl(), vec![rect(100, -200, 180, 0)])),
        ('q', with(bowl(), vec![rect(420, -200, 500, 0)])),
        (
            'y',
            glyph(
                600,
                vec![
                    rect(100, 100, 180, 500),
                    rect(420, -200, 500, 500),
                    rect(100, 100, 500, 180),
                    rect(100, -200, 500, -120),
                ],
            ),
        ),
        (
            'm',
            glyph(
                600,
                vec![
                    rect(60, 0, 140, 500),
                    rect(260, 0, 340, 500),
                    rect(460, 0, 540, 500),
                    rect(60, 420, 540, 500),
                ],
            ),
        ),
        // the underscore, across the whole advance at the cell's bottom
        ('_', glyph(600, vec![rect(0, -200, 600, -120)])),
        // the cedilla to -450: 0.25 below the row
        (
            'ç',
            glyph(
                600,
                vec![
                    rect(100, 0, 180, 500),
                    rect(100, 0, 450, 80),
                    rect(100, 420, 450, 500),
                    rect(260, -450, 340, 0),
                    rect(160, -450, 340, -380),
                ],
            ),
        ),
        // hooks from -150 to 750: a quarter of a cell past each side
        (
            'ƒ',
            glyph(
                600,
                vec![
                    rect(250, -150, 330, 700),
                    rect(250, 620, 750, 700),
                    rect(-150, -150, 330, -70),
                ],
            ),
        ),
        // two cells wide, its bar a quarter of a cell past each side
        (
            '一',
            glyph(
                1200,
                vec![rect(-150, 300, 1350, 420), rect(100, 100, 1100, 160)],
            ),
        ),
    ]);
    // the combining accents, each the precomposed capitals' accent
    for (c, accent) in [
        ('\u{300}', grave()),
        ('\u{301}', acute()),
        ('\u{302}', circumflex()),
        ('\u{323}', dot_below()),
    ] {
        all.push((c, glyph(0, moved(&accent, -600))));
    }
    // Arabic letters
    all.extend([
        ('\u{627}', glyph(600, vec![rect(260, 0, 340, 750)])),
        (
            '\u{628}',
            glyph(
                600,
                vec![
                    rect(60, 0, 540, 120),
                    rect(60, 0, 140, 250),
                    rect(460, 0, 540, 250),
                    rect(260, -200, 340, -120),
                ],
            ),
        ),
        (
            '\u{646}',
            glyph(
                600,
                vec![
                    rect(80, -180, 520, -100),
                    rect(80, -180, 160, 250),
                    rect(440, -180, 520, 250),
                    rect(260, 400, 340, 480),
                ],
            ),
        ),
        // lam's stem to 880: 0.08 above
        (
            '\u{644}',
            glyph(
                600,
                vec![
                    rect(400, -180, 480, 880),
                    rect(80, -180, 480, -100),
                    rect(80, -180, 160, 150),
                ],
            ),
        ),
    ]);
    // Arabic marks, each over its base, whose pen it shares
    let mark = |contours: Vec<Contour>| glyph(0, contours);
    all.extend([
        // shadda, its teeth to 930: 0.13 above
        (
            '\u{651}',
            mark(vec![
                rect(180, 830, 420, 870),
                rect(180, 830, 220, 930),
                rect(280, 830, 320, 930),
                rect(380, 830, 420, 930),
            ]),
        ),
        // fatha to 1060: 0.26 above
        ('\u{64e}', mark(vec![slant(200, 980, 380, 1060, 40)])),
        // damma to 1120: 0.32 above
        (
            '\u{64f}',
            mark(vec![rect(240, 980, 360, 1120), hole(280, 1040, 320, 1090)]),
        ),
        // sukun to 1140: 0.34 above
        (
            '\u{652}',
            mark(vec![rect(230, 1000, 370, 1140), hole(270, 1040, 330, 1100)]),
        ),
        // kasra to -320: 0.12 below
        ('\u{650}', mark(vec![slant(200, -320, 380, -260, 40)])),
        // kasratan to -420: 0.22 below
        (
            '\u{64d}',
            mark(vec![
                slant(200, -320, 380, -260, 40),
                slant(200, -420, 380, -360, 40),
            ]),
        ),
        // the superscript alef from 1550 to 1900, past the reach of one
        // row above, 1800
        ('\u{670}', mark(vec![rect(270, 1550, 330, 1900)])),
    ]);
    all
}

/// The font's bytes.
pub fn bytes() -> Vec<u8> {
    let all = glyphs();
    let mut glyphs = vec![Glyph::empty(600)];
    let mut cmap = Vec::new();
    for (c, g) in all {
        let id = u16::try_from(glyphs.len()).expect("fewer than 65536 glyphs");
        glyphs.push(g);
        cmap.push((u32::from(c), id));
    }
    Font {
        family: "vtium tier2 ink".to_owned(),
        upem: 1000,
        ascent: 800,
        descent: -200,
        glyphs,
        cmap,
        colr: Vec::new(),
        palette: Vec::new(),
        cbdt: None,
        gsub: Vec::new(),
        gpos: Vec::new(),
        underline: None,
        os2: None,
        empty_gpos: true,
    }
    .build()
}

/// The directory the font is kept in, which the frit's engine reads as
/// its only font directory: `ink` beside the outline fonts' cache
/// ([`crate::fonts::cache_dir`]), where each run finds the font's file
/// or writes it once. The file's name holds the start of its SHA-256, so
/// a run never reads a font built from other code, and the file is
/// written under a name of the process's own and renamed into place, so
/// two runs at once never read a half-written font.
pub fn dir() -> Result<&'static Path, String> {
    static DIR: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = crate::fonts::cache_dir().with_file_name("ink");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let font = bytes();
        let path = dir.join(file_name(&font));
        if std::fs::read(&path).ok().as_deref() != Some(font.as_slice()) {
            let part = dir.join(format!("{}.{}", file_name(&font), std::process::id()));
            std::fs::write(&part, &font).map_err(|e| format!("{}: {e}", part.display()))?;
            std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(dir)
    })
    .as_ref()
    .map(PathBuf::as_path)
    .map_err(Clone::clone)
}

/// The name of the file that holds `font`: the start of its SHA-256 in
/// it.
fn file_name(font: &[u8]) -> String {
    let sha = vtium_font::sha256_hex(&vtium_font::Sha256::digest(font));
    format!("vtium-tier2-ink-{}.ttf", sha.get(..16).unwrap_or(&sha))
}

/// The frit of the ink font in `dir`, its rows made taller by `spacing`
/// per cent of line spacing when it is not 0 (ADR-1302 D13).
pub fn frit_text(dir: &Path, spacing: u32) -> String {
    let mut text = format!(
        "source=outline:{}\n",
        dir.join(file_name(&bytes())).display()
    );
    if spacing > 0 {
        text.push_str(&format!("line-spacing={spacing}\n"));
    }
    text
}
