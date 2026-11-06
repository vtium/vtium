//! vtium-config's own BDF reader (ADR-1200 D1, D14): a `bdf:` source's
//! glyphs, plain or compressed, read as Adobe's Glyph Bitmap Distribution
//! Format 2.1 lays them out, and never through swash. A BDF font is read
//! for a character cell: one or two cells wide, its cell as wide as most
//! of its glyphs advance and its ascent and descent high.
//!
//! The file is someone else's bytes: every line, count and side is bounded
//! before anything is allocated for it, and a malformed line refuses the
//! font, naming the line.

use std::collections::BTreeMap;

use vtium_font::{Cell, PSF_BYTES_MAX, PSF_GLYPHS_MAX, Width};

use crate::font::Font;
use crate::frit::FRIT_CELL_MAX;

/// The most bytes one line holds.
pub const BDF_LINE_MAX: usize = 4096;
/// The most glyphs a BDF font names: a bitmap font's 16-bit glyph count,
/// as a PSF's.
pub const BDF_GLYPHS_MAX: u32 = PSF_GLYPHS_MAX;
/// The largest side of a glyph's bitmap, in pixels.
pub const BDF_BITMAP_MAX: u32 = 2 * FRIT_CELL_MAX;

/// A glyph as the file gives it, before the font's cell is known.
struct Char {
    line: usize,
    encoding: u32,
    advance: u32,
    /// The bitmap's width, height, and its left and bottom offsets from
    /// the origin, as `BBX` gives them.
    bbx: (u32, u32, i32, i32),
    /// The bitmap's rows, `bbx.0.div_ceil(8)` bytes each.
    rows: Vec<u8>,
}

/// A BDF font's bytes as its glyphs at its own cell: decompressed, every
/// glyph whose advance is the cell's width or twice it, and whose
/// encoding is a Unicode scalar value, placed on the baseline at its own
/// offsets and cut at its cells' edges. A font whose character set is not
/// ISO 10646-1 is refused, since its encodings are not codepoints. The
/// set is read from the `CHARSET_REGISTRY` and `CHARSET_ENCODING`
/// properties, and where a property is missing, from the last two fields
/// of the XLFD name on the `FONT` line. A glyph of `ENCODING -1`, which
/// has no place in the font's character set, is left out and is not
/// counted against `CHARS`.
pub fn font(bytes: &[u8]) -> Result<Font, String> {
    let (plain, compression) = crate::decode::decompress(bytes)?;
    let mut lines = plain
        .split(|&b| b == b'\n')
        .zip(1usize..)
        .map(|(line, n)| {
            if line.len() > BDF_LINE_MAX {
                return Err(format!("BDF line {n}: the line is too long"));
            }
            let line = std::str::from_utf8(line)
                .map_err(|_| format!("BDF line {n}: not UTF-8 text"))?
                .trim();
            Ok((line, n))
        })
        .filter(|l| !matches!(l, Ok((line, _)) if line.is_empty() || line.starts_with("COMMENT")));
    let mut next = || lines.next().transpose();
    let mut header = Header::default();
    let Some((first, _)) = next()? else {
        return Err("the BDF font is empty".to_owned());
    };
    if word(first, 0) != Some("STARTFONT") {
        return Err("not a BDF font (no STARTFONT line)".to_owned());
    }
    // the header, up to CHARS
    let count = loop {
        let (line, n) = next()?.ok_or("a BDF font cut short before CHARS")?;
        let err = |why: &str| format!("BDF line {n}: {why}");
        match word(line, 0) {
            Some("FONT") => header.xlfd = xlfd(line),
            Some("FONTBOUNDINGBOX") => {
                header.bbx =
                    Some(numbers(line).ok_or_else(|| err("FONTBOUNDINGBOX must be four numbers"))?);
            }
            Some("DWIDTH") => {
                header.advance =
                    Some(advance(line).ok_or_else(|| err("DWIDTH must be two numbers"))?)
            }
            Some("STARTPROPERTIES") => loop {
                let (line, n) = next()?.ok_or("the BDF font ends inside its properties")?;
                let err = |why: &str| format!("BDF line {n}: {why}");
                match word(line, 0) {
                    Some("ENDPROPERTIES") => break,
                    Some("FONT_ASCENT") => {
                        header.ascent =
                            Some(number(line).ok_or_else(|| err("FONT_ASCENT must be a number"))?);
                    }
                    Some("FONT_DESCENT") => {
                        header.descent =
                            Some(number(line).ok_or_else(|| err("FONT_DESCENT must be a number"))?);
                    }
                    Some(key @ ("CHARSET_REGISTRY" | "CHARSET_ENCODING")) => {
                        let value = line
                            .get(key.len()..)
                            .unwrap_or_default()
                            .trim()
                            .trim_matches('"')
                            .to_owned();
                        if key == "CHARSET_REGISTRY" {
                            header.registry = Some(value);
                        } else {
                            header.encoding = Some(value);
                        }
                    }
                    _ => {}
                }
            },
            Some("CHARS") => {
                let count: u32 = word(line, 1)
                    .and_then(|w| w.parse().ok())
                    .ok_or_else(|| err("CHARS must be a number"))?;
                if count > BDF_GLYPHS_MAX {
                    return Err(err("the BDF font has too many glyphs"));
                }
                break count;
            }
            Some("ENDFONT") => return Err(err("ENDFONT comes before CHARS")),
            _ => {}
        }
    };
    header.charset()?;
    // the glyphs
    let mut chars = Vec::new();
    let mut seen = 0u32;
    loop {
        let (line, n) = next()?.ok_or("the BDF font ends before ENDFONT")?;
        match word(line, 0) {
            Some("ENDFONT") => break,
            Some("STARTCHAR") => {
                // the rows read are bounded by the file's own bytes; what
                // they are placed into is bounded in place()
                let parsed = glyph(&mut next, n, header.advance)?;
                if matches!(parsed, Parsed::Unencoded) {
                    continue;
                }
                if seen >= count {
                    return Err(format!("BDF line {n}: more glyphs than CHARS says"));
                }
                seen += 1;
                if let Parsed::Kept(c) = parsed {
                    chars.push(c);
                }
            }
            _ => return Err(format!("BDF line {n}: expected STARTCHAR or ENDFONT")),
        }
    }
    place(&header, chars, compression)
}

/// What the header gives the glyphs.
#[derive(Default)]
struct Header {
    bbx: Option<(u32, u32, i32, i32)>,
    advance: Option<u32>,
    ascent: Option<i32>,
    descent: Option<i32>,
    registry: Option<String>,
    encoding: Option<String>,
    /// The registry and encoding the XLFD name on the `FONT` line ends
    /// with, where that name is an XLFD name.
    xlfd: Option<(String, String)>,
}

impl Header {
    /// Refuses a font whose character set is not ISO 10646-1: each of the
    /// registry and the encoding is the property's, else the XLFD name's.
    /// A font that gives an ISO10646 registry and no encoding is read, as
    /// the registry alone says its encodings are Unicode's.
    fn charset(&self) -> Result<(), String> {
        let (xlfd_registry, xlfd_encoding) = match &self.xlfd {
            Some((r, e)) => (Some(r.as_str()), Some(e.as_str())),
            None => (None, None),
        };
        let registry = self.registry.as_deref().or(xlfd_registry);
        let encoding = self.encoding.as_deref().or(xlfd_encoding);
        match (registry, encoding) {
            (Some(r), None) if r.eq_ignore_ascii_case("ISO10646") => Ok(()),
            (Some(r), Some("1")) if r.eq_ignore_ascii_case("ISO10646") => Ok(()),
            (Some(r), Some(e)) if r.eq_ignore_ascii_case("ISO10646") => Err(format!(
                "the BDF font uses CHARSET_ENCODING {e}, which is not Unicode: vtium-config reads only ISO10646-1"
            )),
            (Some(r), _) => Err(format!(
                "the BDF font uses CHARSET_REGISTRY {r}, which is not Unicode: vtium-config reads only ISO10646"
            )),
            (None, _) => Err(
                "a BDF font with no CHARSET_REGISTRY, in its properties or its XLFD name, so its encodings are not known to be Unicode's"
                    .to_owned(),
            ),
        }
    }
}

/// The registry and the encoding an XLFD name gives, its last two of 14
/// fields: `FONT -foundry-family-...-iso10646-1` gives `iso10646` and
/// `1`. A field may hold blanks, as a family's name does, but not a `-`.
/// `None` for a name that is not 14 fields after a leading `-`.
fn xlfd(line: &str) -> Option<(String, String)> {
    let name = line.get("FONT".len()..)?.trim();
    let fields: Vec<&str> = name.strip_prefix('-')?.split('-').collect();
    match fields.as_slice() {
        [_, _, _, _, _, _, _, _, _, _, _, _, registry, encoding] => {
            Some(((*registry).to_owned(), (*encoding).to_owned()))
        }
        _ => None,
    }
}

/// The `i`th blank-separated word of `line`.
fn word(line: &str, i: usize) -> Option<&str> {
    line.split_ascii_whitespace().nth(i)
}

/// The one number after the keyword.
fn number(line: &str) -> Option<i32> {
    let mut words = line.split_ascii_whitespace().skip(1);
    let n = words.next()?.parse().ok()?;
    words.next().is_none().then_some(n)
}

/// `BBX` and `FONTBOUNDINGBOX`: a width and height from 0 to
/// [`BDF_BITMAP_MAX`] and two offsets of at most that size.
fn numbers(line: &str) -> Option<(u32, u32, i32, i32)> {
    let mut words = line.split_ascii_whitespace().skip(1);
    let mut side = || {
        words
            .next()?
            .parse::<u32>()
            .ok()
            .filter(|&v| v <= BDF_BITMAP_MAX)
    };
    let (w, h) = (side()?, side()?);
    let mut offset = || {
        words
            .next()?
            .parse::<i32>()
            .ok()
            .filter(|v| v.unsigned_abs() <= BDF_BITMAP_MAX)
    };
    let (x, y) = (offset()?, offset()?);
    words.next().is_none().then_some((w, h, x, y))
}

/// `DWIDTH`'s horizontal advance, at most [`BDF_BITMAP_MAX`]; a vertical
/// one is not read.
fn advance(line: &str) -> Option<u32> {
    let mut words = line.split_ascii_whitespace().skip(1);
    let dx = words.next()?.parse::<i32>().ok()?;
    words.next()?.parse::<i32>().ok()?;
    if words.next().is_some() {
        return None;
    }
    u32::try_from(dx).ok().filter(|&v| v <= BDF_BITMAP_MAX)
}

/// What a glyph read gives the font.
enum Parsed {
    /// A glyph of a Unicode scalar value with an advance.
    Kept(Char),
    /// A glyph of `ENCODING -1`, outside the font's character set, which
    /// `CHARS` need not count.
    Unencoded,
    /// A glyph of another encoding that is no Unicode scalar value, or
    /// of no advance.
    Dropped,
}

/// The glyph after `STARTCHAR` on line `start`, through `ENDCHAR`.
fn glyph<'a>(
    next: &mut impl FnMut() -> Result<Option<(&'a str, usize)>, String>,
    start: usize,
    font_advance: Option<u32>,
) -> Result<Parsed, String> {
    let (mut encoding, mut advance_x, mut bbx) = (None, font_advance, None);
    loop {
        let (line, n) = next()?.ok_or_else(|| format!("BDF line {start}: a glyph is cut short"))?;
        let err = |why: &str| format!("BDF line {n}: {why}");
        match word(line, 0) {
            Some("ENCODING") => {
                let e: i64 = word(line, 1)
                    .and_then(|w| w.parse().ok())
                    .ok_or_else(|| err("ENCODING must be a number"))?;
                encoding = Some(e);
            }
            Some("DWIDTH") => {
                advance_x = Some(advance(line).ok_or_else(|| err("DWIDTH must be two numbers"))?)
            }
            Some("BBX") => {
                bbx = Some(numbers(line).ok_or_else(|| err("BBX must be four numbers"))?)
            }
            Some("BITMAP") => {
                let (w, h, x, y) = bbx.ok_or_else(|| err("BITMAP comes before BBX"))?;
                let row = w.div_ceil(8) as usize;
                let mut rows = Vec::with_capacity(row * h as usize);
                // a bitmap no pixel wide has empty rows, which are skipped
                // as every blank line is
                for _ in 0..if row == 0 { 0 } else { h } {
                    let (line, n) = next()?
                        .ok_or_else(|| format!("BDF line {start}: a bitmap is cut short"))?;
                    let hex = line.as_bytes();
                    if hex.len() < row * 2 || hex.len() % 2 != 0 {
                        return Err(format!(
                            "BDF line {n}: a bitmap row does not have {} hex digits",
                            row * 2
                        ));
                    }
                    for pair in hex.chunks(2).take(row) {
                        let s = std::str::from_utf8(pair)
                            .map_err(|_| format!("BDF line {n}: not a hex number"))?;
                        rows.push(
                            u8::from_str_radix(s, 16)
                                .map_err(|_| format!("BDF line {n}: not a hex number"))?,
                        );
                    }
                    if !hex.iter().all(u8::is_ascii_hexdigit) {
                        return Err(format!("BDF line {n}: not a hex number"));
                    }
                }
                let (end, n) =
                    next()?.ok_or_else(|| format!("BDF line {start}: a glyph is cut short"))?;
                if end != "ENDCHAR" {
                    return Err(format!(
                        "BDF line {n}: a bitmap is longer than its BBX, or has no ENDCHAR"
                    ));
                }
                let encoding =
                    encoding.ok_or_else(|| format!("BDF line {start}: a glyph has no ENCODING"))?;
                if encoding == -1 {
                    return Ok(Parsed::Unencoded);
                }
                let scalar = u32::try_from(encoding)
                    .ok()
                    .filter(|&c| char::from_u32(c).is_some());
                return Ok(match (scalar, advance_x) {
                    (Some(encoding), Some(advance)) if advance > 0 => Parsed::Kept(Char {
                        line: start,
                        encoding,
                        advance,
                        bbx: (w, h, x, y),
                        rows,
                    }),
                    _ => Parsed::Dropped,
                });
            }
            Some("ENDCHAR") => return Err(err("ENDCHAR comes before BITMAP")),
            Some("STARTCHAR" | "ENDFONT") => return Err(err("a glyph has no ENDCHAR")),
            _ => {}
        }
    }
}

/// The glyphs placed in the font's cell: its width the advance most of
/// its glyphs take, the narrower on a tie, its height the ascent and
/// descent, from the properties, else the bounding box.
fn place(
    header: &Header,
    chars: Vec<Char>,
    compression: crate::decode::Compression,
) -> Result<Font, String> {
    let (ascent, descent) = match (header.ascent, header.descent, header.bbx) {
        (Some(a), Some(d), _) => (a, d),
        (_, _, Some((_, h, _, y))) => (i32::try_from(h).unwrap_or(0) + y, -y),
        _ => {
            return Err(
                "a BDF font with no FONT_ASCENT and FONT_DESCENT, nor FONTBOUNDINGBOX".to_owned(),
            );
        }
    };
    let height = u32::try_from(ascent.saturating_add(descent))
        .ok()
        .filter(|h| (1..=FRIT_CELL_MAX).contains(h))
        .ok_or_else(|| format!("the BDF font is {ascent} pixels above its baseline and {descent} below; the total must be 1 to {FRIT_CELL_MAX}"))?;
    // the advance most of its glyphs take, a tie to the narrower
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for c in &chars {
        *counts.entry(c.advance).or_default() += 1;
    }
    let width = counts
        .iter()
        .max_by(|(wa, na), (wb, nb)| na.cmp(nb).then(wb.cmp(wa)))
        .map(|(&w, _)| w)
        .ok_or("the BDF font has no glyph for any Unicode character")?;
    if width > FRIT_CELL_MAX {
        return Err(format!(
            "the BDF font's characters are mostly {width} pixels wide, more than {FRIT_CELL_MAX}"
        ));
    }
    let cell = Cell { width, height };
    let mut font = Font {
        cell,
        baseline: u32::try_from(ascent).ok(),
        glyphs: Vec::new(),
        map: BTreeMap::new(),
        format: match compression {
            crate::decode::Compression::Plain => "bdf".to_owned(),
            c => format!("bdf+{}", c.name()),
        },
    };
    let mut placed = 0usize;
    for c in chars {
        let cells = match c.advance {
            a if a == width => Width::Narrow,
            a if a == 2 * width => Width::Wide,
            _ => continue,
        };
        if font.map.contains_key(&c.encoding) {
            continue;
        }
        let across = width * cells.cells();
        let row = across.div_ceil(8) as usize;
        placed = placed.saturating_add(row * height as usize);
        if placed > PSF_BYTES_MAX as usize {
            return Err(format!(
                "BDF line {}: the glyphs would take more than {PSF_BYTES_MAX} bytes in their {width}x{height} character cell",
                c.line
            ));
        }
        let mut bits = vec![0u8; row * height as usize];
        let (w, h, x0, y0) = c.bbx;
        let src_row = w.div_ceil(8) as usize;
        // the bitmap's top row sits h + y0 above the baseline
        let top = i64::from(ascent) - i64::from(y0) - i64::from(h);
        for by in 0..h {
            for bx in 0..w {
                let on = c
                    .rows
                    .get(by as usize * src_row + (bx / 8) as usize)
                    .is_some_and(|b| b & (0x80 >> (bx % 8)) != 0);
                let (x, y) = (i64::from(x0) + i64::from(bx), top + i64::from(by));
                if !on || x < 0 || y < 0 || x >= i64::from(across) || y >= i64::from(height) {
                    continue;
                }
                if let Some(b) = bits.get_mut(y as usize * row + x as usize / 8) {
                    *b |= 0x80 >> (x % 8);
                }
            }
        }
        let g = u32::try_from(font.glyphs.len())
            .map_err(|_| format!("BDF line {}: too many glyphs", c.line))?;
        font.glyphs.push((cells, bits));
        font.map.insert(c.encoding, g);
    }
    Ok(font)
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test fails by panicking"
)]
pub(crate) mod tests {
    use super::*;

    /// A BDF font of 4x6 cells, ascent 5 and descent 1, holding `A` as a
    /// 3x5 bitmap on the baseline and one column in, `中` two cells wide,
    /// and `glyphs` after them.
    pub(crate) fn bdf(glyphs: &str) -> String {
        format!(
            "STARTFONT 2.1\n\
             COMMENT a test font\n\
             FONT -test-\n\
             SIZE 6 75 75\n\
             FONTBOUNDINGBOX 8 6 0 -1\n\
             STARTPROPERTIES 3\n\
             FONT_ASCENT 5\n\
             FONT_DESCENT 1\n\
             CHARSET_REGISTRY \"ISO10646\"\n\
             ENDPROPERTIES\n\
             CHARS 99\n\
             STARTCHAR A\n\
             ENCODING 65\n\
             SWIDTH 666 0\n\
             DWIDTH 4 0\n\
             BBX 3 5 1 0\n\
             BITMAP\n\
             40\nA0\nE0\nA0\nA0\n\
             ENDCHAR\n\
             STARTCHAR uni4E2D\n\
             ENCODING 20013\n\
             DWIDTH 8 0\n\
             BBX 8 6 0 -1\n\
             BITMAP\n\
             10\n10\nFF\n91\nFF\n10\n\
             ENDCHAR\n\
             {glyphs}\
             ENDFONT\n"
        )
    }

    fn rows(font: &Font, c: char) -> Vec<String> {
        let g = font.map[&u32::from(c)];
        let (width, bits) = &font.glyphs[g as usize];
        let across = font.cell.width * width.cells();
        let row = across.div_ceil(8) as usize;
        bits.chunks(row)
            .map(|r| {
                (0..across)
                    .map(|x| {
                        if r[(x / 8) as usize] & (0x80 >> (x % 8)) != 0 {
                            '#'
                        } else {
                            '.'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    // ADR-1200 D1, D14: a bdf: source's glyphs, read by vtium-config's own reader
    #[test]
    fn a_bdf_font_reads_at_its_own_cell_one_or_two_cells_wide() {
        let f = font(bdf("").as_bytes()).expect("reads");
        assert_eq!((f.cell.width, f.cell.height, f.baseline), (4, 6, Some(5)));
        assert_eq!(f.format, "bdf");
        assert_eq!(
            rows(&f, 'A'),
            ["..#.", ".#.#", ".###", ".#.#", ".#.#", "...."]
        );
        assert_eq!(f.glyphs[f.map[&0x4e2d] as usize].0, Width::Wide);
        assert_eq!(
            rows(&f, '中'),
            [
                "...#....", "...#....", "########", "#..#...#", "########", "...#...."
            ]
        );
        let gz = font(&crate::decode::tests::gzip(bdf("").as_bytes())).expect("gzip");
        assert_eq!((gz.format.as_str(), gz.map.len()), ("bdf+gzip", 2));
    }

    #[test]
    fn a_glyph_past_its_cells_is_cut_and_one_of_another_advance_left_out() {
        let f = font(
            bdf(
                "STARTCHAR B\nENCODING 66\nDWIDTH 4 0\nBBX 4 2 2 4\nBITMAP\nF0\nF0\nENDCHAR\n\
                 STARTCHAR wide\nENCODING 67\nDWIDTH 6 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\n\
                 STARTCHAR none\nENCODING -1\nDWIDTH 4 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\n\
                 STARTCHAR again\nENCODING 65\nDWIDTH 4 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\n\
                 STARTCHAR space\nENCODING 32\nDWIDTH 4 0\nBBX 0 0 0 0\nBITMAP\nENDCHAR\n",
            )
            .as_bytes(),
        )
        .expect("reads");
        // B's two rows start a pixel above the cell and two columns in
        assert_eq!(rows(&f, 'B')[0], "..##");
        assert_eq!(rows(&f, 'B')[1], "....");
        assert!(!f.map.contains_key(&0x43), "six wide is neither 4 nor 8");
        // the first glyph of a codepoint is the one drawn
        assert_eq!(rows(&f, 'A')[0], "..#.");
        assert_eq!(rows(&f, ' '), ["...."; 6]);
        let keys: Vec<u32> = f.map.keys().copied().collect();
        assert_eq!(keys, [0x20, 0x41, 0x42, 0x4e2d]);
    }

    /// A BDF font of `CHARS 65535` with no ink, FONT_ASCENT 96: `narrow`
    /// glyphs advancing 96 and the rest 192, each `BBX 0 0`, so a few
    /// bytes of the file place a glyph of up to 2,304 bytes.
    fn inkless(narrow: u32) -> String {
        let mut f = String::from(
            "STARTFONT 2.1\nSTARTPROPERTIES 3\nFONT_ASCENT 96\nFONT_DESCENT 0\n\
             CHARSET_REGISTRY \"ISO10646\"\nENDPROPERTIES\nCHARS 65535\n",
        );
        for i in 0..65535u32 {
            let advance = if i < narrow { 96 } else { 192 };
            f.push_str(&format!(
                "STARTCHAR g\nENCODING {}\nDWIDTH {advance} 0\nBBX 0 0 0 0\nBITMAP\nENDCHAR\n",
                0x20000 + i
            ));
        }
        f.push_str("ENDFONT\n");
        f
    }

    /// `text` cut to its first `glyphs` glyphs.
    fn first(text: &str, glyphs: usize) -> String {
        let cut = text
            .match_indices("STARTCHAR")
            .nth(glyphs)
            .map(|(i, _)| i)
            .expect("a glyph");
        format!("{}ENDFONT\n", &text[..cut])
    }

    // review-s5's file: 4.4 MB of BDF that placed 146 MB of glyphs, under
    // 1 MiB gzipped. Its glyphs mostly advance 192, so its cell is past
    // FRIT_CELL_MAX; half and one narrow keep the cell at 96, and place
    // over 100 MB.
    #[test]
    fn glyphs_placed_past_psf_bytes_max_refuse_the_font() {
        let reviewers = inkless(1);
        assert!(reviewers.len() < 5 << 20, "{}", reviewers.len());
        let gz = crate::decode::tests::gzip(reviewers.as_bytes());
        assert!(
            gz.len() < crate::decode::COMPRESSED_BYTES_MAX,
            "{}",
            gz.len()
        );
        let Err(e) = font(&gz) else {
            panic!("accepted")
        };
        assert!(e.contains("mostly 192 pixels wide"), "{e}");
        let Err(e) = font(&crate::decode::tests::gzip(inkless(32_768).as_bytes())) else {
            panic!("accepted")
        };
        assert!(e.contains("would take more than"), "{e}");
        assert!(!e.contains("_MAX"), "{e}");
        // 14,563 glyphs of 1,152 bytes stay under 16 MiB, and one more
        // passes it
        let narrow = inkless(65_535);
        let held = font(first(&narrow, 14_563).as_bytes()).expect("reads");
        assert_eq!(held.glyphs.len(), 14_563);
        assert!(
            font(first(&narrow, 14_564).as_bytes()).is_err(),
            "14,564 glyphs"
        );
    }

    // the cell is the advance most of the glyphs take, so one stray glyph
    // narrower than the rest leaves only itself out
    #[test]
    fn the_cell_is_the_most_frequent_advance() {
        let f = font(
            bdf(
                "STARTCHAR B\nENCODING 66\nDWIDTH 4 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\n\
                 STARTCHAR stray\nENCODING 105\nDWIDTH 2 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\n",
            )
            .as_bytes(),
        )
        .expect("reads");
        assert_eq!((f.cell.width, f.cell.height), (4, 6));
        let keys: Vec<u32> = f.map.keys().copied().collect();
        assert_eq!(keys, [0x41, 0x42, 0x4e2d], "the stray i is left out");
        assert_eq!(f.glyphs[f.map[&0x41] as usize].0, Width::Narrow);
        // a tie goes to the narrower advance: one 4 and one 8 here
        let tie = font(bdf("").as_bytes()).expect("reads");
        assert_eq!(tie.cell.width, 4);
    }

    // the published Tiny5 Mono gives its character set only in its XLFD
    // name: the registry and the encoding are read from there when the
    // properties leave them out, and a property says it over the name
    #[test]
    fn the_character_set_comes_from_the_xlfd_name_where_no_property_gives_it() {
        let a = bdf("");
        let named = |xlfd: &str, properties: &str| {
            a.replace("FONT -test-", &format!("FONT {xlfd}"))
                .replace("CHARSET_REGISTRY \"ISO10646\"\n", properties)
        };
        let tiny5 = "-Stefan Schmidt Art-Tiny5 Mono-Regular-R-Regular--9-9-75-75-c-80-iso10646-1";
        let f = font(named(tiny5, "").as_bytes()).expect("the XLFD name's iso10646-1");
        assert_eq!(f.map.len(), 2);
        // a property wins over the name
        let e = font(named(tiny5, "CHARSET_REGISTRY \"JISX0208.1983\"\n").as_bytes())
            .expect_err("the property's registry");
        assert!(e.contains("CHARSET_REGISTRY JISX0208.1983"), "{e}");
        let e = font(named(tiny5, "CHARSET_ENCODING \"0\"\n").as_bytes())
            .expect_err("the property's encoding");
        assert!(e.contains("CHARSET_ENCODING 0"), "{e}");
        let good = font(
            named(
                "-x-y-medium-r-normal--9-90-75-75-c-60-jisx0208.1983-0",
                "CHARSET_REGISTRY \"ISO10646\"\nCHARSET_ENCODING \"1\"\n",
            )
            .as_bytes(),
        );
        assert!(good.is_ok(), "the properties over the name");
        // the name's own registry and encoding are checked as a property's
        let e = font(named("-x-y-medium-r-normal--9-90-75-75-c-60-iso8859-1", "").as_bytes())
            .expect_err("the name's iso8859");
        assert!(e.contains("CHARSET_REGISTRY iso8859"), "{e}");
        let e = font(named("-x-y-medium-r-normal--9-90-75-75-c-60-iso10646-2", "").as_bytes())
            .expect_err("the name's encoding 2");
        assert!(e.contains("CHARSET_ENCODING 2"), "{e}");
        // a name that is not 14 fields gives nothing
        for not_xlfd in [
            "fixed",
            "-iso10646-1",
            "-a-b-c-d-e-f-g-h-i-j-k-l-m-iso10646-1",
        ] {
            let e = font(named(not_xlfd, "").as_bytes()).expect_err(not_xlfd);
            assert!(e.contains("no CHARSET_REGISTRY"), "{not_xlfd}: {e}");
        }
    }

    // the published Tiny5 Mono's CHARS leaves out its 73 glyphs of
    // ENCODING -1: such a glyph is not counted, and not drawn
    #[test]
    fn a_glyph_of_encoding_minus_one_is_left_out_and_not_counted() {
        let unencoded = "STARTCHAR a.sc\nENCODING -1\nDWIDTH 4 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\n\
                         STARTCHAR b.sc\nENCODING -1 7\nDWIDTH 4 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\n";
        let text = bdf(&format!(
            "{unencoded}STARTCHAR B\nENCODING 66\nDWIDTH 4 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\n{unencoded}"
        ));
        let f = font(text.replace("CHARS 99", "CHARS 3").as_bytes()).expect("CHARS counts 3");
        let keys: Vec<u32> = f.map.keys().copied().collect();
        assert_eq!(keys, [0x41, 0x42, 0x4e2d]);
        assert_eq!(f.glyphs.len(), 3);
        // the glyphs CHARS counts are still held to it
        let e = font(text.replace("CHARS 99", "CHARS 2").as_bytes()).expect_err("CHARS 2");
        assert!(e.contains("more glyphs than CHARS"), "{e}");
        // an unencoded glyph is read as any other, so a malformed one refuses
        let e = font(bdf(&unencoded.replace("BBX 1 1 0 0", "BBX 1 1 0")).as_bytes())
            .expect_err("a malformed BBX");
        assert!(e.contains("BBX must be four numbers"), "{e}");
    }

    #[test]
    fn each_malformed_bdf_is_refused_naming_its_line() {
        let a = bdf("");
        for (text, why) in [
            (String::new(), "the BDF font is empty"),
            ("FONT x\n".to_owned(), "no STARTFONT"),
            (
                a.replace("BBX 3 5 1 0", "BBX 3 5 1"),
                "BDF line 16: BBX must be four numbers",
            ),
            (
                a.replace("BBX 3 5 1 0", "BBX 300 5 1 0"),
                "BDF line 16: BBX",
            ),
            (a.replace("E0\n", "G0\n"), "BDF line 20: not a hex number"),
            (a.replace("E0\n", "E\n"), "BDF line 20: a bitmap row"),
            (
                a.replace("A0\nENDCHAR", "A0\nA0\nENDCHAR"),
                "BDF line 23: a bitmap is longer",
            ),
            (
                a.replace("ENCODING 65\n", ""),
                "BDF line 12: a glyph has no ENCODING",
            ),
            (
                a.replace("CHARS 99", "CHARS 1"),
                "BDF line 24: more glyphs than CHARS",
            ),
            (a.replace("CHARS 99", "CHARS 65536"), "too many glyphs"),
            (
                a.replace("\"ISO10646\"", "\"JISX0208.1983\""),
                "CHARSET_REGISTRY JISX0208.1983",
            ),
            (
                a.replace("CHARSET_REGISTRY \"ISO10646\"\n", ""),
                "no CHARSET_REGISTRY, in its properties or its XLFD name",
            ),
            (
                a.replace("FONT_ASCENT 5", "FONT_ASCENT 500"),
                "pixels above its baseline",
            ),
            (a.replace("ENDFONT\n", ""), "ends before ENDFONT"),
            (
                a.replace("SIZE 6 75 75", &"x".repeat(BDF_LINE_MAX + 1)),
                "BDF line 4: the line is too long",
            ),
        ] {
            let e = font(text.as_bytes()).expect_err(why);
            assert!(e.contains(why), "{why}: {e}");
        }
    }
}
