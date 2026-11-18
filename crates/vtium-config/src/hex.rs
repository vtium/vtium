//! vtium-config's reader of the `.hex` font format that GNU Unifont's
//! tools write, for a `hex:` source. Each line holds one glyph: a code
//! point in hexadecimal, a colon, and the glyph's rows in hexadecimal, 32
//! digits for an 8×16 glyph or 64 for a 16×16 one. Every glyph is 16 rows
//! high, so the font's cell is 8×16, and a 16×16 glyph is two cells wide.
//!
//! The file is someone else's bytes, so the reader refuses the whole font
//! for a line it cannot read, a code point that is not a Unicode
//! character, a glyph of another size, or a second glyph for one code
//! point. Each refusal names the line. The file is read uncompressed,
//! within the bound the helper reads every bitmap file with.
//!
//! References:
//! - ADR-1204 D5: the `hex:` source kind, the format, and one glyph at
//!   most for each code point.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use vtium_font::{Cell, Width};

use crate::font::Font;

/// The cell of every glyph a `.hex` font holds: 8 pixels wide and 16
/// high, a 16×16 glyph filling two cells.
pub const HEX_CELL: Cell = Cell {
    width: 8,
    height: 16,
};
/// The row that a `.hex` font's glyphs stand on, counted from the top of
/// the 16-row cell: Unifont's glyphs have 14 rows above the baseline and 2
/// below it.
pub const HEX_BASELINE: u32 = 14;
/// The hexadecimal digits of a glyph one cell wide: 16 rows of one byte.
pub const NARROW_DIGITS: usize = 32;
/// The hexadecimal digits of a glyph two cells wide: 16 rows of two bytes.
pub const WIDE_DIGITS: usize = 64;
/// The most hexadecimal digits a code point is written with, enough for
/// U+10FFFF.
pub const CODE_POINT_DIGITS_MAX: usize = 6;
/// The longest text before a line's colon that a message quotes when the
/// text is not a code point. A longer text is given by its length.
const SHOWN_MAX: usize = 16;

/// A `.hex` font's bytes as its glyphs at its 8×16 cell, each glyph one or
/// two cells wide. The file ends with a newline or without one; an empty
/// line elsewhere is not a glyph and refuses the font.
pub fn font(bytes: &[u8]) -> Result<Font, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        "The .hex font is not text: a .hex font holds only code points, colons and hexadecimal digits.".to_owned()
    })?;
    let body = text.strip_suffix('\n').unwrap_or(text);
    if body.is_empty() {
        return Err("The .hex font is empty: it holds no glyph.".to_owned());
    }
    let mut font = Font {
        cell: HEX_CELL,
        baseline: Some(HEX_BASELINE),
        glyphs: Vec::new(),
        map: BTreeMap::new(),
        format: "hex".to_owned(),
    };
    for (line, n) in body.split('\n').zip(1usize..) {
        let (c, glyph) = glyph(line).map_err(|why| format!("Line {n} of the .hex font {why}"))?;
        let g = u32::try_from(font.glyphs.len())
            .map_err(|_| "The .hex font holds too many glyphs.".to_owned())?;
        match font.map.entry(c) {
            // every line before this one became one glyph, in order, so
            // glyph number g came from line g + 1
            Entry::Occupied(first) => {
                return Err(format!(
                    "Line {n} of the .hex font gives a second glyph for U+{c:04X}, after line {}. A .hex font holds one glyph for each character.",
                    u64::from(*first.get()) + 1
                ));
            }
            Entry::Vacant(at) => {
                at.insert(g);
            }
        }
        font.glyphs.push(glyph);
    }
    Ok(font)
}

/// One line's code point and glyph, or why the line is not one, worded to
/// follow "Line N of the .hex font".
fn glyph(line: &str) -> Result<(u32, (Width, Vec<u8>)), String> {
    let Some((code, bits)) = line.split_once(':') else {
        return Err(
            "is not a glyph. A glyph's line reads CODEPOINT:BITS, such as 0041:0000000018242442427E424242420000."
                .to_owned(),
        );
    };
    let hex = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex(code) || code.len() > CODE_POINT_DIGITS_MAX {
        // the text is someone else's and may be megabytes long or hold
        // control characters, so the message quotes only a short,
        // printable one and gives the length of any other
        let found = if code.is_empty() {
            "has nothing before the colon".to_owned()
        } else if code.len() <= SHOWN_MAX && code.bytes().all(|b| b.is_ascii_graphic()) {
            format!("starts with {code}")
        } else {
            format!("has {} characters before the colon", code.chars().count())
        };
        return Err(format!(
            "does not start with a code point: it {found}. A code point is 1 to {CODE_POINT_DIGITS_MAX} hexadecimal digits, such as 0041."
        ));
    }
    let c = u32::from_str_radix(code, 16)
        .ok()
        .filter(|&c| char::from_u32(c).is_some())
        .ok_or_else(|| format!("names {code}, which is not a Unicode character."))?;
    // a file saved with Windows line endings keeps a carriage return at
    // the end of each line, which is the most common way a .hex file is
    // damaged, so the message names it
    if bits.ends_with('\r') {
        return Err(
            "ends with a carriage return. A .hex font's lines end with a newline alone, so convert the file's line endings from Windows to Unix."
                .to_owned(),
        );
    }
    let width = match bits.len() {
        NARROW_DIGITS => Width::Narrow,
        WIDE_DIGITS => Width::Wide,
        _ => {
            let n = bits.chars().count();
            return Err(format!(
                "has {n} characters after the colon. A glyph is {NARROW_DIGITS} hexadecimal digits for an 8×16 character, or {WIDE_DIGITS} for a 16×16 one."
            ));
        }
    };
    if !hex(bits) {
        return Err("has a character after the colon that is not a hexadecimal digit.".to_owned());
    }
    let rows = bits
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|p| u8::from_str_radix(p, 16).ok())
                .unwrap_or(0)
        })
        .collect();
    Ok((c, (width, rows)))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// A narrow glyph's 32 digits and a wide one's 64, each of one byte
    /// repeated.
    fn narrow(byte: &str) -> String {
        byte.repeat(16)
    }
    fn wide(byte: &str) -> String {
        byte.repeat(32)
    }

    // A .hex font reads as an 8×16 font standing on row 14: a 32-digit
    // glyph is one cell wide, a 64-digit glyph two, each row's digits
    // read as bytes from the left, and each code point maps to its glyph.
    //
    // References:
    // - ADR-1204 D5: the codepoint:bits lines, 8×16 and 16×16.
    #[test]
    fn a_hex_font_reads_narrow_and_wide_glyphs_at_8x16() {
        let text = format!(
            "0041:{}\n4E00:{}\n1F600:{}\n",
            "00000000182424427E42424242000000",
            wide("FF"),
            narrow("0f")
        );
        let f = font(text.as_bytes()).unwrap();
        assert_eq!((f.cell, f.baseline), (HEX_CELL, Some(14)));
        assert_eq!(f.format, "hex");
        assert_eq!(f.glyphs.len(), 3);
        let a = &f.glyphs[f.map[&0x41] as usize];
        assert_eq!(a.0, Width::Narrow);
        assert_eq!(
            a.1,
            vec![
                0, 0, 0, 0, 0x18, 0x24, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0x42, 0, 0, 0
            ]
        );
        let han = &f.glyphs[f.map[&0x4e00] as usize];
        assert_eq!((han.0, han.1.len()), (Width::Wide, 32));
        assert!(han.1.iter().all(|&b| b == 0xff));
        let beyond = &f.glyphs[f.map[&0x1f600] as usize];
        assert_eq!((beyond.0, beyond.1.len()), (Width::Narrow, 16));
        assert!(beyond.1.iter().all(|&b| b == 0x0f));
        // the last line needs no newline
        let last = format!("0041:{}", narrow("00"));
        assert_eq!(font(last.as_bytes()).unwrap().map.len(), 1);
    }

    // Each fault refuses the whole font and names its line: a line with
    // no colon, an empty line, a code point that is not hexadecimal, too
    // long or not a character, a glyph of a digit count that is neither
    // 32 nor 64, a glyph digit that is not hexadecimal, and a second glyph
    // for one code point, which also names the first glyph's line.
    //
    // References:
    // - ADR-1204 D5: one glyph at most for each code point.
    #[test]
    fn a_malformed_line_refuses_the_font_naming_its_line() {
        let good = format!("0041:{}", narrow("00"));
        for (bad, line, says) in [
            ("0042".to_owned(), 2, "is not a glyph"),
            (String::new(), 2, "is not a glyph"),
            (
                format!("00G2:{}", narrow("00")),
                2,
                "does not start with a code point: it starts with 00G2.",
            ),
            (
                format!("0000042:{}", narrow("00")),
                2,
                "does not start with a code point: it starts with 0000042.",
            ),
            (
                format!(":{}", narrow("00")),
                2,
                "does not start with a code point: it has nothing before the colon.",
            ),
            (
                format!("\u{1}41:{}", narrow("00")),
                2,
                "does not start with a code point: it has 3 characters before the colon.",
            ),
            (
                format!("D800:{}", narrow("00")),
                2,
                "is not a Unicode character",
            ),
            (
                format!("110000:{}", narrow("00")),
                2,
                "is not a Unicode character",
            ),
            (
                "0042:0011".to_owned(),
                2,
                "has 4 characters after the colon",
            ),
            (format!("0042:{}0", narrow("00")), 2, "has 33 characters"),
            (
                format!("0042:{}\r", narrow("00")),
                2,
                "ends with a carriage return",
            ),
            (
                format!("0042:{}", narrow("0g")),
                2,
                "not a hexadecimal digit",
            ),
            (
                format!("0042:{}", narrow("0 ")),
                2,
                "not a hexadecimal digit",
            ),
            (
                format!("0041:{}", wide("00")),
                2,
                "a second glyph for U+0041, after line 1",
            ),
        ] {
            let text = format!("{good}\n{bad}\n");
            let e = font(text.as_bytes()).expect_err(&bad);
            assert!(
                e.starts_with(&format!("Line {line} of the .hex font ")),
                "{bad:?}: {e}"
            );
            assert!(e.contains(says), "{bad:?}: {e}");
        }
        assert!(font(b"").unwrap_err().contains("empty"));
        assert!(font(b"\n").unwrap_err().contains("empty"));
        assert!(font(b"\xff\n").unwrap_err().contains("not text"));
        // a gzipped file is not text either
        let gz = crate::decode::tests::gzip(good.as_bytes());
        assert!(font(&gz).is_err());
        // a file saved with Windows line endings is refused at its first
        // line, and the message names the carriage return
        let crlf = format!("0041:{}\r\n0042:{}\r\n", narrow("00"), narrow("00"));
        let e = font(crlf.as_bytes()).unwrap_err();
        assert!(
            e.starts_with("Line 1 of the .hex font ends with a carriage return"),
            "{e}"
        );
    }

    // A line whose text before the colon is megabytes long, or holds
    // control characters, is refused with a message that gives the
    // text's length and does not repeat the text, so the message stays
    // short whatever the file holds.
    #[test]
    fn a_long_code_field_is_refused_in_a_short_message() {
        for byte in ["\u{1}", "g", "0"] {
            let text = format!("{}:{}\n", byte.repeat(1 << 20), narrow("00"));
            let e = font(text.as_bytes()).unwrap_err();
            assert!(e.len() < 200, "{} bytes: {}", e.len(), &e[..200]);
            assert!(
                e.contains("it has 1048576 characters before the colon"),
                "{e}"
            );
            assert!(!e.contains('\u{1}') && !e.contains("\\u"), "{e}");
        }
    }

    // The all-planes Unifont file that every build installs reads whole:
    // 126,641 glyphs in planes 0 to 3, 14 and 15, 20,869 of them one cell
    // wide and 105,772 two. The tree holds the same file as vtium-font's
    // data/unifont.hex.
    //
    // References:
    // - ADR-1204 D5: the unifont layer's file and its glyph count.
    #[test]
    fn the_all_planes_unifont_file_reads_whole() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../vtium-font/data/unifont.hex");
        let bytes = std::fs::read(&path).expect("the tree's unifont.hex");
        assert_eq!(bytes.len(), 8_329_308);
        let f = font(&bytes).unwrap();
        assert_eq!(f.map.len(), 126_641);
        let planes: std::collections::BTreeSet<u32> = f.map.keys().map(|c| c >> 16).collect();
        assert_eq!(planes.into_iter().collect::<Vec<_>>(), [0, 1, 2, 3, 14, 15]);
        let wide = f.glyphs.iter().filter(|(w, _)| *w == Width::Wide).count();
        assert_eq!((f.glyphs.len() - wide, wide), (20_869, 105_772));
        assert!(f.map.contains_key(&0x2_0000), "CJK Extension B");
        assert!(f.map.contains_key(&0x1_d400), "Mathematical Alphanumerics");
    }
}
