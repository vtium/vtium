//! A set from a frit's parsed sources (ADR-1200 D1, D2, D4): the first
//! source sets the cell, each later one fills only the codepoints the
//! earlier ones lack, scaled by the largest whole factor at which its cell
//! fits the strike's, placed on the strike's baseline and centred across
//! the cell; a source whose cell does not fit at factor 1 fills nothing.
//! Bitmap sources give one 1-bit strike.

use std::collections::BTreeMap;

use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder, VERSION, builtin};
use vtium_font::sprite::Range;
use vtium_font::{
    Cell, Coverage, Face, RangeCoverage, Sha256, SpritePolicy, Style, Width, sha256_hex,
};

use crate::frit::{Frit, Source};
use crate::glyphs::Glyphs;
use crate::sources::Resolved;

/// One source's glyphs at its own cell: 1-bit, each row padded to a byte,
/// one or two cells wide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub cell: Cell,
    pub glyphs: Vec<(Width, Vec<u8>)>,
    /// Each codepoint and its glyph.
    pub map: BTreeMap<u32, u32>,
    /// What was decoded, for the provenance.
    pub format: String,
}

impl Font {
    /// A parsed bitmap font: every glyph one cell wide.
    pub fn from_glyphs(g: Glyphs) -> Font {
        let size = g.glyph_bytes().max(1);
        Font {
            cell: Cell {
                width: g.width,
                height: g.height,
            },
            glyphs: g
                .bits
                .chunks(size)
                .map(|rows| (Width::Narrow, rows.to_vec()))
                .collect(),
            map: g.map.into_iter().collect(),
            format: g.format,
        }
    }

    /// A built-in set's glyphs at its smallest cell, read through the
    /// set's own face.
    pub fn from_builtin(name: &str) -> Result<Font, String> {
        let set = builtin(name).ok_or_else(|| format!("this build holds no built-in {name}"))?;
        let cell = *set
            .cells()
            .first()
            .ok_or_else(|| format!("the built-in {name} has no strike"))?;
        let face = vtium_font::set::SetFace::new(set, cell.height)
            .ok_or_else(|| format!("the built-in {name} has no strike"))?;
        let mut font = Font {
            cell,
            glyphs: Vec::new(),
            map: BTreeMap::new(),
            format: format!("builtin:{name}"),
        };
        for c in (0..=0x10_ffff_u32).filter_map(char::from_u32) {
            if !face.has_glyph(c) {
                continue;
            }
            let Some(glyph) = face.draw_unscaled(&[c], Style::default()) else {
                continue;
            };
            let row = glyph.width().div_ceil(8) as usize;
            let mut bits = vec![0u8; row.saturating_mul(glyph.height() as usize)];
            for y in 0..glyph.height() {
                for x in 0..glyph.width() {
                    if glyph
                        .pixel(x, y)
                        .and_then(|p| p.first())
                        .is_some_and(|&a| a >= 0x80)
                    {
                        set_bit(&mut bits, row, x, y);
                    }
                }
            }
            let g = u32::try_from(font.glyphs.len()).map_err(|_| "too many glyphs".to_owned())?;
            font.glyphs.push((glyph.cells(), bits));
            font.map.insert(u32::from(c), g);
        }
        Ok(font)
    }
}

fn bit(bits: &[u8], row: usize, x: u32, y: u32) -> bool {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    bits.get(at).is_some_and(|b| b & (0x80 >> (x % 8)) != 0)
}

fn set_bit(bits: &mut [u8], row: usize, x: u32, y: u32) {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    if let Some(b) = bits.get_mut(at) {
        *b |= 0x80 >> (x % 8);
    }
}

/// `glyph`, `width` cells wide in a font of `from`'s cell, scaled by
/// `scale` into a glyph of the same width at `cell`: on the bottom row,
/// centred across.
fn place(glyph: &[u8], width: Width, from: Cell, cell: Cell, scale: u32) -> Vec<u8> {
    let cells = width.cells();
    let (src_w, dst_w) = (from.width * cells, cell.width * cells);
    let (src_row, dst_row) = (src_w.div_ceil(8) as usize, dst_w.div_ceil(8) as usize);
    let x0 = (dst_w - src_w * scale) / 2;
    let y0 = cell.height - from.height * scale;
    let mut out = vec![0u8; dst_row * cell.height as usize];
    for y in 0..from.height * scale {
        for x in 0..src_w * scale {
            if bit(glyph, src_row, x / scale, y / scale) {
                set_bit(&mut out, dst_row, x0 + x, y0 + y);
            }
        }
    }
    out
}

/// The build key (ADR-1200 D4): the SHA-256 of the format version, the
/// frit's bytes and the SHA-256 of everything read for each source, in
/// order, or that the source was missing, so a set rebuilds when any of
/// them changes, a missing font that appears included.
pub fn build_key(frit: &Frit, sources: &[Option<Resolved>]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"vtium-vitrify build key\0");
    h.update(&VERSION.0.to_le_bytes());
    h.update(&VERSION.1.to_le_bytes());
    h.update(&(frit.text.len() as u64).to_le_bytes());
    h.update(frit.text.as_bytes());
    for s in sources {
        let Some(s) = s else {
            h.update(b"missing\0");
            continue;
        };
        h.update(&s.sha256);
        for extra in [s.unimap_sha256, s.def_uni_sha256] {
            match extra {
                Some(sha) => {
                    h.update(&[1]);
                    h.update(&sha);
                }
                None => h.update(&[0]),
            }
        }
    }
    h.finish()
}

/// A source found and parsed: what it read, and its glyphs.
pub struct Part<'a> {
    pub source: &'a Source,
    pub resolved: &'a Resolved,
    pub font: Font,
}

/// The set `frit` describes, from the fonts of its sources found, in
/// order, the first setting the cell: its bytes. The sources left out are
/// named in the provenance.
pub fn assemble(
    frit: &Frit,
    parts: &[Part<'_>],
    missing: &[&Source],
    key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let main = parts
        .first()
        .ok_or("no source of the frit is on this machine")?;
    let cell = main.font.cell;
    let mut strike = StrikeBuilder::new(PixelFormat::Bits, cell, Style::default());
    if !frit.compress {
        strike = strike.compress(false);
    }
    let mut mapped: BTreeMap<u32, ()> = BTreeMap::new();
    for part in parts {
        fill(&mut strike, &mut mapped, &part.font, part.source, cell);
    }
    if mapped.is_empty() {
        return Err("the sources give the set no glyph".to_owned());
    }
    let coverage = Coverage::new(|range: Range| {
        RangeCoverage::count(range, |c| mapped.contains_key(&u32::from(c)))
    });
    let mut set = SetBuilder::new(&frit.name)
        .policy(SpritePolicy::default_for(&coverage))
        .meta("build-key", &sha256_hex(key))
        .meta("tool", concat!("vtium-vitrify ", env!("CARGO_PKG_VERSION")));
    for line in frit.text.lines() {
        set = set.meta("frit", line);
    }
    for part in parts {
        set = set
            .meta("source", &part.source.spelling)
            .meta("source-sha256", &sha256_hex(&part.resolved.sha256))
            .meta("source-format", &part.font.format);
        if let Some(sha) = part.resolved.unimap_sha256 {
            set = set.meta("source-unimap-sha256", &sha256_hex(&sha));
        }
    }
    for source in missing {
        set = set.meta("source-missing", &source.spelling);
    }
    if let Some(license) = &frit.license {
        set = set.meta("license", license);
    }
    set.strike(strike).write().map_err(|e| e.to_string())
}

/// Adds `font`'s glyphs for the codepoints `source` covers and the strike
/// lacks, placed at the strike's `cell`.
fn fill(
    strike: &mut StrikeBuilder,
    mapped: &mut BTreeMap<u32, ()>,
    font: &Font,
    source: &Source,
    cell: Cell,
) {
    let scale = (cell.width / font.cell.width.max(1)).min(cell.height / font.cell.height.max(1));
    if scale == 0 {
        return;
    }
    let mut placed: BTreeMap<u32, u32> = BTreeMap::new();
    for (&c, &g) in &font.map {
        let Some(ch) = char::from_u32(c) else {
            continue;
        };
        if mapped.contains_key(&c) || !source.covers(c) {
            continue;
        }
        let index = match placed.get(&g) {
            Some(&i) => i,
            None => {
                let Some((width, bits)) = font.glyphs.get(g as usize) else {
                    continue;
                };
                let i = strike.glyph(*width, place(bits, *width, font.cell, cell, scale));
                placed.insert(g, i);
                i
            }
        };
        strike.map(ch, index);
        mapped.insert(c, ());
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    #[test]
    fn a_smaller_glyph_is_scaled_onto_the_baseline_and_centred() {
        // a 2x2 glyph, its top-left pixel set, into a 6x6 cell: scale 2,
        // 1 column from the left, 2 rows from the top
        let from = Cell {
            width: 2,
            height: 2,
        };
        let cell = Cell {
            width: 6,
            height: 6,
        };
        let out = place(&[0x80, 0x00], Width::Narrow, from, cell, 2);
        assert_eq!(out, [0, 0, 0b0110_0000, 0b0110_0000, 0, 0]);
        // two cells wide: 8 pixels into 12, 2 columns from the left
        let wide = place(&[0x80, 0x00], Width::Wide, from, cell, 2);
        assert_eq!(wide.len(), 2 * 6);
        assert_eq!(&wide[4..8], &[0b0011_0000, 0, 0b0011_0000, 0]);
    }

    #[test]
    fn a_builtin_reads_as_a_font() {
        let f = Font::from_builtin("unscii-8").expect("unscii-8");
        assert_eq!((f.cell.width, f.cell.height), (8, 8));
        assert!(f.map.contains_key(&0x41));
        let u = Font::from_builtin("unifont").expect("unifont");
        assert_eq!((u.cell.width, u.cell.height), (8, 16));
        let wide = u.map[&0x4e00];
        assert_eq!(u.glyphs[wide as usize].0, Width::Wide);
        assert_eq!(u.glyphs[wide as usize].1.len(), 2 * 16);
    }
}
