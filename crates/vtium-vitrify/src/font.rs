//! A bitmap source's glyphs at its own cell (ADR-1200 D1), as the helper's
//! engine draws them: a parsed PSF, CPI or raw font, or a built-in font.

use std::collections::BTreeMap;

use vtium_font::set::builtin;
use vtium_font::{Cell, Face, Sha256, Style, Width};

use crate::glyphs::Glyphs;

/// One source's glyphs at its own cell: 1-bit, each row padded to a byte,
/// one or two cells wide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub cell: Cell,
    /// The baseline's row from the cell's top, where the font gives one:
    /// a built-in set's strike does, a PSF font does not.
    pub baseline: Option<u32>,
    pub glyphs: Vec<(Width, Vec<u8>)>,
    /// Each codepoint and its glyph.
    pub map: BTreeMap<u32, u32>,
    /// What was decoded, as `psf2+gzip+table` or `builtin:unifont`.
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
            baseline: None,
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
        let baseline = set
            .strikes()
            .find(|s| s.cell == cell && s.style == Style::default())
            .map(|s| s.baseline);
        let face = vtium_font::set::SetFace::new(set, cell.height)
            .ok_or_else(|| format!("the built-in {name} has no strike"))?;
        let mut font = Font {
            cell,
            baseline,
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

impl Font {
    /// The SHA-256 of the glyphs as read: the cell, then each glyph's
    /// width and rows, then each codepoint and its glyph, which a built-in
    /// source's read carries as a font file's carries the file's.
    pub fn digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(&self.cell.width.to_le_bytes());
        h.update(&self.cell.height.to_le_bytes());
        h.update(&self.baseline.unwrap_or(u32::MAX).to_le_bytes());
        h.update(&(self.glyphs.len() as u64).to_le_bytes());
        for (width, bits) in &self.glyphs {
            h.update(&[u8::from(*width == Width::Wide)]);
            h.update(&(bits.len() as u64).to_le_bytes());
            h.update(bits);
        }
        for (c, g) in &self.map {
            h.update(&c.to_le_bytes());
            h.update(&g.to_le_bytes());
        }
        h.finish()
    }
}

fn set_bit(bits: &mut [u8], row: usize, x: u32, y: u32) {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    if let Some(b) = bits.get_mut(at) {
        *b |= 0x80 >> (x % 8);
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
