//! Cuts the strikes of vtium's built-in font, "vtium built-in", from their
//! sources with the list of characters in `data/builtin-chars.txt`.
//!
//! `build.rs` includes this module to pack the strikes into the binary,
//! and the tests include it to read the same sources again and check what
//! the build packed. It uses nothing else of the crate, so the build
//! script can include it on its own. The sources are trusted files in the
//! tree, so a malformed line is an error that stops the build.
//!
//! References:
//! - ADR-1204 D1: the built-in font, its list, its three strikes and the
//!   sources each is cut from.

// The readers here read only the trusted files in the tree, at build time
// and in the tests, where a malformed file should stop the build. The
// console reads no font file, so the crate's rules for untrusted input,
// which forbid a panic, do not apply to them.
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use std::collections::BTreeMap;

/// The font's name, as its sets' provenance and vtium's documents give
/// it. A subset of Terminus is a modified version of it, and "Terminus
/// Font" is a reserved name, so the strikes carry vtium's own name.
pub const NAME: &str = "vtium built-in";

/// The font's name in its sets' provenance, where a set's name holds no
/// space.
pub const SET_NAME: &str = "vtium-builtin";

/// What a strike is cut from.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Format {
    /// Unifont's `codepoint:bits` hex form, each glyph a cell of the
    /// strike's size, stood on `baseline` rows.
    Hex { baseline: u16 },
    /// A BDF file, whose own bounding box gives the cell and baseline.
    Bdf,
}

/// One of the built-in font's strikes and its source.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Strike {
    /// The name its packed set has in the build's output folder.
    pub file: &'static str,
    pub width: u16,
    pub height: u16,
    /// The source, relative to the crate's folder.
    pub path: &'static str,
    pub format: Format,
    /// The source's name and version, for the set's provenance.
    pub source: &'static str,
    pub license: &'static str,
}

/// The built-in font's strikes, smallest first: 8×8 for cells under 16
/// pixels high, 8×16 for cells from 16 to 31, and 16×32 from 32.
pub(crate) const STRIKES: [Strike; 3] = [
    Strike {
        file: "builtin-8x8.vtglass",
        width: 8,
        height: 8,
        path: "data/unscii-8.hex",
        // Unscii-8's glyphs stand on row 7 of 8, one row of descent
        format: Format::Hex { baseline: 7 },
        source: "Unscii 2.1, unscii-8.hex",
        license: "public domain",
    },
    Strike {
        file: "builtin-8x16.vtglass",
        width: 8,
        height: 16,
        path: "data/ter-u16n.bdf",
        format: Format::Bdf,
        source: "Terminus Font 4.49.1, ter-u16n.bdf",
        license: "OFL-1.1",
    },
    Strike {
        file: "builtin-16x32.vtglass",
        width: 16,
        height: 32,
        path: "data/ter-u32n.bdf",
        format: Format::Bdf,
        source: "Terminus Font 4.49.1, ter-u32n.bdf",
        license: "OFL-1.1",
    },
];

/// A strike's glyphs as a source holds them: its cell, its baseline, and
/// each glyph's rows by codepoint, each row `width.div_ceil(8)` bytes with
/// the leftmost pixel in the highest bit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Glyphs {
    pub width: u16,
    pub height: u16,
    pub baseline: u16,
    pub glyphs: BTreeMap<u32, Vec<u8>>,
}

/// The ranges of codepoints the list `text` names, each with both ends
/// included, in the list's order: one codepoint `U+XXXX` or a range
/// `U+XXXX..U+YYYY` a line, with `#` starting a comment.
pub(crate) fn list(text: &str) -> Result<Vec<(u32, u32)>, String> {
    let point = |s: &str, n: usize| {
        s.strip_prefix("U+")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .filter(|&cp| char::from_u32(cp).is_some())
            .ok_or_else(|| format!("line {n}: {s:?} is not a character's U+XXXX"))
    };
    let mut ranges = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (first, last) = match line.split_once("..") {
            Some((a, b)) => (point(a.trim(), n)?, point(b.trim(), n)?),
            None => (point(line, n)?, point(line, n)?),
        };
        if first > last {
            return Err(format!("line {n}: the range ends before it starts"));
        }
        ranges.push((first, last));
    }
    Ok(ranges)
}

/// Whether `ranges` holds `cp`.
pub(crate) fn listed(ranges: &[(u32, u32)], cp: u32) -> bool {
    ranges.iter().any(|&(a, b)| (a..=b).contains(&cp))
}

/// The glyphs of a hex file `text` whose cells are `width` by `height`,
/// stood on `baseline`. A glyph of another width, such as Unscii-8's
/// 16×8 U+0000, is left out, since no strike draws it.
pub(crate) fn hex(text: &str, width: u16, height: u16, baseline: u16) -> Result<Glyphs, String> {
    let digits = usize::from(width.div_ceil(8)) * usize::from(height) * 2;
    let mut glyphs = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let (cp, bits) = line
            .split_once(':')
            .ok_or_else(|| format!("line {}: no ':'", i + 1))?;
        let cp = u32::from_str_radix(cp, 16).map_err(|e| format!("line {}: {e}", i + 1))?;
        if bits.len() != digits {
            continue;
        }
        glyphs.insert(cp, bytes(bits).ok_or(format!("line {}: not hex", i + 1))?);
    }
    Ok(Glyphs {
        width,
        height,
        baseline,
        glyphs,
    })
}

/// The bytes the hex digits `bits` spell, two digits a byte.
fn bytes(bits: &str) -> Option<Vec<u8>> {
    if !bits.len().is_multiple_of(2) {
        return None;
    }
    (0..bits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(bits.get(i..i + 2)?, 16).ok())
        .collect()
}

/// The glyphs of a BDF file `text`: its cell from `FONTBOUNDINGBOX`, its
/// baseline where that box's descent ends, and each encoded glyph placed
/// in the cell by its own `BBX`. A glyph that would pass the cell is an
/// error.
pub(crate) fn bdf(text: &str) -> Result<Glyphs, String> {
    let numbers = |rest: &str| -> Result<Vec<i32>, String> {
        rest.split_whitespace()
            .map(|n| n.parse::<i32>().map_err(|e| format!("{rest:?}: {e}")))
            .collect()
    };
    let mut lines = text.lines();
    let mut out = Glyphs::default();
    let mut descent = 0;
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("FONTBOUNDINGBOX ") {
            let n = numbers(rest)?;
            let [w, h, _, y] = n[..] else {
                return Err(format!("FONTBOUNDINGBOX {rest}"));
            };
            out.width = u16::try_from(w).map_err(|e| e.to_string())?;
            out.height = u16::try_from(h).map_err(|e| e.to_string())?;
            descent = -y;
            out.baseline = u16::try_from(h - descent).map_err(|e| e.to_string())?;
        }
        if !line.starts_with("STARTCHAR") {
            continue;
        }
        let (mut cp, mut bbx) = (None, None);
        for line in lines.by_ref() {
            if let Some(rest) = line.strip_prefix("ENCODING ") {
                cp = u32::try_from(numbers(rest)?[0]).ok();
            } else if let Some(rest) = line.strip_prefix("BBX ") {
                bbx = Some(numbers(rest)?);
            } else if line == "BITMAP" {
                break;
            }
        }
        let Some([bw, bh, bx, by]) = bbx.and_then(|b| <[i32; 4]>::try_from(b).ok()) else {
            return Err(format!("a glyph with no BBX, U+{:04X}", cp.unwrap_or(0)));
        };
        let row_bytes = usize::from(out.width.div_ceil(8));
        let mut glyph = vec![0u8; row_bytes * usize::from(out.height)];
        // the glyph's top row in the cell, counted down from the cell's top
        let top = i32::from(out.height) - descent - by - bh;
        for (r, line) in lines.by_ref().take_while(|l| *l != "ENDCHAR").enumerate() {
            let row = bytes(line.trim()).ok_or_else(|| format!("BITMAP row {line:?}"))?;
            for x in 0..bw {
                let byte = row.get(usize::try_from(x / 8).unwrap_or(0)).copied();
                if byte.unwrap_or(0) & (0x80 >> (x % 8)) == 0 {
                    continue;
                }
                let (cx, cy) = (bx + x, top + i32::try_from(r).unwrap_or(i32::MAX));
                let inside = (0..i32::from(out.width)).contains(&cx)
                    && (0..i32::from(out.height)).contains(&cy);
                if !inside {
                    return Err(format!("U+{:04X} passes its cell", cp.unwrap_or(0)));
                }
                let (cx, cy) = (cx.unsigned_abs() as usize, cy.unsigned_abs() as usize);
                glyph[cy * row_bytes + cx / 8] |= 0x80 >> (cx % 8);
            }
        }
        if let Some(cp) = cp {
            out.glyphs.insert(cp, glyph);
        }
    }
    if out.width == 0 || out.height == 0 {
        return Err("no FONTBOUNDINGBOX".to_owned());
    }
    Ok(out)
}

/// The glyphs of `strike`'s source `text`.
pub(crate) fn read(strike: &Strike, text: &str) -> Result<Glyphs, String> {
    let glyphs = match strike.format {
        Format::Hex { baseline } => hex(text, strike.width, strike.height, baseline)?,
        Format::Bdf => bdf(text)?,
    };
    if (glyphs.width, glyphs.height) != (strike.width, strike.height) {
        return Err(format!(
            "{} has {}×{} cells, not {}×{}",
            strike.path, glyphs.width, glyphs.height, strike.width, strike.height
        ));
    }
    Ok(glyphs)
}

/// `source`'s glyphs for the characters `ranges` holds, and no others.
pub(crate) fn cut(source: &Glyphs, ranges: &[(u32, u32)]) -> Glyphs {
    Glyphs {
        glyphs: source
            .glyphs
            .iter()
            .filter(|(cp, _)| listed(ranges, **cp))
            .map(|(cp, g)| (*cp, g.clone()))
            .collect(),
        ..*source
    }
}
