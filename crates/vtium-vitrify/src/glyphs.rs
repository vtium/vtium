//! The bounded form the parser hands back (ADR-1200 D14): one bitmap
//! font's glyphs and Unicode map. The parser writes it and the parent
//! checks every field before it trusts any, as it would a file.
//!
//! ```text
//! magic "VTVGLYF1", width u32, height u32, glyph count u32,
//! map length u32, format length u32, the format's bytes,
//! the glyphs: count of height rows of width.div_ceil(8) bytes,
//! the map: each (codepoint u32, glyph u32), sorted, no codepoint twice
//! ```
//!
//! Little-endian throughout.

use vtium_font::{PSF_ENTRIES_MAX, PSF_GLYPHS_MAX, PSF_HEIGHT_MAX, PSF_WIDTH_MAX};

const MAGIC: &[u8; 8] = b"VTVGLYF1";
/// The most bytes the format's name takes.
const FORMAT_MAX: usize = 64;

/// The most bytes a glyph form takes: the largest PSF's glyphs and a map
/// of every codepoint.
pub const GLYPHS_BYTES_MAX: usize = 28
    + FORMAT_MAX
    + (PSF_GLYPHS_MAX as usize) * (PSF_HEIGHT_MAX as usize) * (PSF_WIDTH_MAX as usize / 8)
    + (PSF_ENTRIES_MAX as usize) * 8;

/// A bitmap font's glyphs at one size, and the codepoints that draw them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Glyphs {
    pub width: u32,
    pub height: u32,
    pub count: u32,
    /// `count` glyphs of `height` rows of `width.div_ceil(8)` bytes, the
    /// first pixel the high bit.
    pub bits: Vec<u8>,
    /// Each codepoint and its glyph, sorted by codepoint.
    pub map: Vec<(u32, u32)>,
    /// What was decoded, such as `psf2+gzip`.
    pub format: String,
}

impl Glyphs {
    /// The bytes one glyph takes.
    pub fn glyph_bytes(&self) -> usize {
        (self.width.div_ceil(8) as usize).saturating_mul(self.height as usize)
    }

    /// Glyph `g`'s rows.
    pub fn glyph(&self, g: u32) -> Option<&[u8]> {
        let size = self.glyph_bytes();
        let start = (g as usize).checked_mul(size)?;
        self.bits.get(start..start.checked_add(size)?)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            28 + self.format.len() + self.bits.len() + self.map.len().saturating_mul(8),
        );
        out.extend_from_slice(MAGIC);
        let len = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        for v in [
            self.width,
            self.height,
            self.count,
            len(self.map.len()),
            len(self.format.len()),
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(self.format.as_bytes());
        out.extend_from_slice(&self.bits);
        for &(c, g) in &self.map {
            out.extend_from_slice(&c.to_le_bytes());
            out.extend_from_slice(&g.to_le_bytes());
        }
        out
    }

    /// Reads and checks a glyph form: every count within the PSF bounds,
    /// the glyphs exactly as long as the counts say, every codepoint a
    /// scalar value in order, every glyph one the form holds, and nothing
    /// after the map.
    pub fn decode(bytes: &[u8]) -> Result<Glyphs, &'static str> {
        if bytes.len() > GLYPHS_BYTES_MAX {
            return Err("larger than GLYPHS_BYTES_MAX");
        }
        let mut r = Reader(bytes);
        if r.take(8)? != MAGIC {
            return Err("not a glyph form");
        }
        let (width, height, count) = (r.u32()?, r.u32()?, r.u32()?);
        let (map_len, format_len) = (r.u32()?, r.u32()?);
        if !(1..=PSF_WIDTH_MAX).contains(&width)
            || !(1..=PSF_HEIGHT_MAX).contains(&height)
            || !(1..=PSF_GLYPHS_MAX).contains(&count)
            || map_len > PSF_ENTRIES_MAX
            || format_len as usize > FORMAT_MAX
        {
            return Err("a size past the PSF bounds");
        }
        let format = std::str::from_utf8(r.take(format_len as usize)?)
            .ok()
            .filter(|f| f.bytes().all(|b| b.is_ascii_graphic()))
            .ok_or("a format name that is not printable ASCII")?
            .to_owned();
        let size = (width.div_ceil(8) as usize)
            .checked_mul(height as usize)
            .and_then(|s| s.checked_mul(count as usize))
            .ok_or("glyphs past usize")?;
        let bits = r.take(size)?.to_vec();
        let mut map = Vec::with_capacity(map_len as usize);
        for _ in 0..map_len {
            let (c, g) = (r.u32()?, r.u32()?);
            if char::from_u32(c).is_none() {
                return Err("a codepoint that is not a Unicode scalar value");
            }
            if g >= count {
                return Err("a glyph past the count");
            }
            if map.last().is_some_and(|&(p, _)| p >= c) {
                return Err("the map is not sorted, or names a codepoint twice");
            }
            map.push((c, g));
        }
        if !r.0.is_empty() {
            return Err("bytes after the map");
        }
        Ok(Glyphs {
            width,
            height,
            count,
            bits,
            map,
            format,
        })
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        if n > self.0.len() {
            return Err("cut short");
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn u32(&mut self) -> Result<u32, &'static str> {
        let b = self.take(4)?;
        let mut a = [0u8; 4];
        a.copy_from_slice(b);
        Ok(u32::from_le_bytes(a))
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

    fn sample() -> Glyphs {
        Glyphs {
            width: 9,
            height: 2,
            count: 2,
            bits: vec![0x80, 0x80, 0xff, 0x80, 1, 2, 3, 4],
            map: vec![(0x41, 0), (0x42, 1), (0x2500, 1)],
            format: "psf2+gzip".to_owned(),
        }
    }

    #[test]
    fn a_form_reads_back() {
        let g = sample();
        assert_eq!(Glyphs::decode(&g.encode()), Ok(g.clone()));
        assert_eq!(g.glyph(1), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(g.glyph(2), None);
    }

    #[test]
    fn each_malformed_form_is_refused() {
        let good = sample().encode();
        let mut cases: Vec<(Vec<u8>, &str)> = Vec::new();
        cases.push((good[..good.len() - 1].to_vec(), "cut short"));
        let mut extra = good.clone();
        extra.push(0);
        cases.push((extra, "after the map"));
        let mut magic = good.clone();
        magic[0] = b'X';
        cases.push((magic, "not a glyph form"));
        let mut wide = good.clone();
        wide[8..12].copy_from_slice(&33u32.to_le_bytes());
        cases.push((wide, "PSF bounds"));
        let mut glyph = good.clone();
        let at = good.len() - 4;
        glyph[at..].copy_from_slice(&2u32.to_le_bytes());
        cases.push((glyph, "past the count"));
        let mut order = good.clone();
        let at = good.len() - 8;
        order[at..at + 4].copy_from_slice(&0x41u32.to_le_bytes());
        cases.push((order, "not sorted"));
        let mut surrogate = good.clone();
        surrogate[at..at + 4].copy_from_slice(&0xd800u32.to_le_bytes());
        cases.push((surrogate, "scalar value"));
        for (bytes, why) in cases {
            let e = Glyphs::decode(&bytes).expect_err(why);
            assert!(e.contains(why), "{why}: {e}");
        }
    }
}
