//! The test fonts tier 2 and tier 3 draw the small displays with (vtium/vtium#75):
//! PC Screen Fonts in both versions, written here so no test depends on a
//! console font a system happens to ship.
//!
//! | Font | Format | Cell | Draws |
//! |---|---|---|---|
//! | [`small8x8`] | PSF2 | 8x8 | a 320x240 display as 40 by 30 cells |
//! | [`small8x16`] | PSF1 | 8x16 | a 640x480 display as 80 by 30 cells |
//!
//! Each holds 256 glyphs and a unicode table: the [`REPERTOIRE`] in order,
//! then blank glyphs no codepoint maps to. The glyphs are vtium's unifont
//! (GNU Unifont, under the SIL Open Font License 1.1 in vtium-font's
//! `data/OFL-1.1.txt`): as they are for 8x16, and folded to half their
//! height for 8x8 by [`fold`]. Box drawing and block elements are left out,
//! so vtium's sprites draw them at either cell.
//!
//! The format is kbd's, as vtium-font's PSF loader reads it: a header, the
//! glyph bitmaps a row of bytes at a time, high bit leftmost, and the table,
//! each glyph's codepoints followed by the terminator (PSF1: little-endian
//! u16 and 0xFFFF; PSF2: UTF-8 and the byte 0xFF).

use vtium_font::{Face, Style, Unifont};

/// The characters both fonts draw, in glyph order: printable ASCII, the
/// Latin-1 supplement from U+00A0 but the soft hyphen, which a terminal
/// gives no cell, typographic quotes and dashes, the
/// bullet and ellipsis, the euro sign, the four arrows, and U+FFFD.
pub const REPERTOIRE: &[(char, char)] = &[
    (' ', '~'),
    ('\u{a0}', '\u{ac}'),
    ('\u{ae}', '\u{ff}'),
    ('\u{2013}', '\u{2014}'),
    ('\u{2018}', '\u{2019}'),
    ('\u{201c}', '\u{201d}'),
    ('\u{2022}', '\u{2022}'),
    ('\u{2026}', '\u{2026}'),
    ('\u{20ac}', '\u{20ac}'),
    ('\u{2190}', '\u{2193}'),
    ('\u{fffd}', '\u{fffd}'),
];

/// Glyphs in each font: PSF1 holds 256 or 512.
pub const GLYPHS: usize = 256;

/// The unifont rows each row of an 8x8 glyph takes, ORed together. Unifont
/// sets capitals in rows 4 to 13, lower case from row 6, and descenders in
/// 14 and 15; the 8x8 glyph keeps capitals in rows 0 to 6 and descenders in
/// row 7, and merges the pairs of rows where a letter's strokes are
/// furthest apart, so `a`, `e` and `s` keep their three bars. Accents above
/// a capital merge into its row 0.
const FOLD: [(usize, usize); 8] = [
    (0, 4),
    (5, 5),
    (6, 7),
    (8, 8),
    (9, 9),
    (10, 11),
    (12, 13),
    (14, 15),
];

/// A bitmap font: one byte a row, since both fonts are 8 pixels wide, and
/// the codepoints each glyph draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub height: usize,
    /// `GLYPHS` glyphs of `height` rows each, high bit leftmost.
    pub glyphs: Vec<Vec<u8>>,
    /// The codepoints of each glyph, in glyph order.
    pub codes: Vec<Vec<char>>,
}

/// The repertoire's characters in glyph order.
pub fn repertoire() -> impl Iterator<Item = char> {
    REPERTOIRE.iter().flat_map(|&(a, b)| a..=b)
}

/// Unifont's 8x16 glyph for `c` as 16 row bytes, or `None` where unifont
/// draws it wide or not at all.
fn unifont_rows(face: &Unifont, c: char) -> Option<Vec<u8>> {
    let glyph = face.draw(&[c], Style::default())?;
    if glyph.width() != 8 || glyph.height() != 16 {
        return None;
    }
    Some(
        glyph
            .rows()
            .map(|row| {
                row.iter()
                    .fold(0u8, |byte, &px| (byte << 1) | u8::from(px != 0))
            })
            .collect(),
    )
}

/// An 8x16 glyph folded to 8x8 by [`FOLD`].
pub fn fold(rows: &[u8]) -> Vec<u8> {
    FOLD.iter()
        .map(|&(a, b)| {
            rows.get(a..=b)
                .unwrap_or_default()
                .iter()
                .fold(0, |acc, r| acc | r)
        })
        .collect()
}

/// The repertoire from unifont, each glyph through `shape`, padded with
/// blank glyphs to [`GLYPHS`].
fn build(height: usize, shape: impl Fn(Vec<u8>) -> Vec<u8>) -> Result<Bitmap, String> {
    let face = Unifont::new(16);
    let mut glyphs = Vec::with_capacity(GLYPHS);
    let mut codes = Vec::with_capacity(GLYPHS);
    for c in repertoire() {
        let rows = unifont_rows(&face, c)
            .ok_or_else(|| format!("unifont has no narrow glyph for U+{:04X}", u32::from(c)))?;
        glyphs.push(shape(rows));
        codes.push(vec![c]);
    }
    if glyphs.len() > GLYPHS {
        return Err(format!(
            "the repertoire holds {} characters, past {GLYPHS}",
            glyphs.len()
        ));
    }
    glyphs.resize(GLYPHS, vec![0; height]);
    codes.resize(GLYPHS, Vec::new());
    Ok(Bitmap {
        height,
        glyphs,
        codes,
    })
}

/// The 8x16 font: unifont's glyphs as they are.
pub fn small8x16() -> Result<Bitmap, String> {
    build(16, |rows| rows)
}

/// The 8x8 font: unifont's glyphs folded by [`fold`].
pub fn small8x8() -> Result<Bitmap, String> {
    build(8, |rows| fold(&rows))
}

impl Bitmap {
    /// The font as PSF1: 256 glyphs of `height` bytes, with a table of
    /// little-endian u16 codepoints.
    pub fn psf1(&self) -> Result<Vec<u8>, String> {
        const MODE_HAS_TAB: u8 = 0x02;
        let height = u8::try_from(self.height).map_err(|_| "a PSF1 glyph is at most 255 rows")?;
        if self.glyphs.len() != 256 {
            return Err("this writer's PSF1 fonts hold 256 glyphs".to_owned());
        }
        let mut out = vec![0x36, 0x04, MODE_HAS_TAB, height];
        self.glyphs.iter().for_each(|g| out.extend_from_slice(g));
        for codes in &self.codes {
            for &c in codes {
                let unit = u16::try_from(u32::from(c))
                    .map_err(|_| format!("PSF1 maps the BMP only, not U+{:X}", u32::from(c)))?;
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out.extend_from_slice(&0xffffu16.to_le_bytes());
        }
        Ok(out)
    }

    /// The font as PSF2: a 32-byte header, the glyphs, and a UTF-8 table.
    pub fn psf2(&self) -> Result<Vec<u8>, String> {
        const HAS_TABLE: u32 = 0x01;
        let count = u32::try_from(self.glyphs.len()).map_err(|_| "too many glyphs")?;
        let height = u32::try_from(self.height).map_err(|_| "too tall")?;
        let mut out = vec![0x72, 0xb5, 0x4a, 0x86];
        for field in [0, 32, HAS_TABLE, count, height, height, 8] {
            out.extend_from_slice(&u32::to_le_bytes(field));
        }
        self.glyphs.iter().for_each(|g| out.extend_from_slice(g));
        for codes in &self.codes {
            let mut buf = [0u8; 4];
            for &c in codes {
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            out.push(0xff);
        }
        Ok(out)
    }
}

/// The SIL Open Font License 1.1, under which unifont, and so these fonts,
/// are distributed.
pub const OFL: &str = include_str!("../../../crates/vtium-font/data/OFL-1.1.txt");

/// The notice the OFL asks a Modified Version to carry: what the fonts are
/// and whose work they derive from.
pub const NOTICE: &str = "\
vtium-test-8x8.psf and vtium-test-8x16.psf are Modified Versions of GNU
Unifont 17.0.03, written by vtium's tools/testfont for its tests: printable
ASCII, Latin-1 and a few signs, the 8x8 font folded to half height.

GNU Unifont is the work of Roman Czyborra, Paul Hardy and the Unifont
contributors (unifoundry.com), and is licensed under the SIL Open Font
License 1.1, whose text is in OFL-1.1.txt beside this file.
";

/// The fonts by file name, with the licence and the notice, as `testfont`
/// writes them and tier 3's guest reads them.
pub fn files() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    Ok(vec![
        ("vtium-test-8x8.psf", small8x8()?.psf2()?),
        ("vtium-test-8x16.psf", small8x16()?.psf1()?),
        ("OFL-1.1.txt", OFL.as_bytes().to_vec()),
        ("NOTICE", NOTICE.as_bytes().to_vec()),
    ])
}
