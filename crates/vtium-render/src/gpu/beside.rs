//! The ink beside the pieces of magnified characters, as the cell shader
//! reads it: for each piece whose underline the frame draws, the rows of
//! its [`Beside`], in a texture of their own.
//!
//! A piece of a character on a line of double size, or of a sized run,
//! carries the pixels near the ink in the character's pieces beside it,
//! which its underline skips as it skips the piece's own ink
//! ([`crate::beside`]). The GPU path keeps each slot's rows when the slot
//! is filled, because the slot keeps no pixels the CPU could read later.
//! Each frame then gives every slot whose piece draws an underline that
//! skips ink an entry, the first such slot entry 0, and the quad of each
//! such cell names its slot's entry plus one in the bits of `src[2]` from
//! [`SHIFT`] up. A quad with no entry names 0 there.
//!
//! The texture is [`Format::Rgba8`]: each entry takes a cell's height of
//! texels, one for each row of the cell, upright, from the top, and
//! [`Besides::across`] entries lie side by side in each row of texels.
//! Entry `n` starts at column `(n % across) · h` of texel row
//! `n / across`, for a cell `h` rows tall. A row's texel holds the pixels
//! kept clear from the cell's left edge in its red byte and from its right
//! edge in its green byte. A frame whose entries and rows are the last
//! frame's uploads nothing.

use std::collections::BTreeMap;

use super::gl::{Error, ErrorKind, Format, Gl, Size, Texture};
use crate::beside::Beside;
use crate::target::Rect;

/// The bit of a quad's `src[2]` that its entry here, plus one, starts at.
/// The bits below hold the column of the quad's first cell.
pub(crate) const SHIFT: u32 = 16;

/// The most a quad's `src[2]` can hold from [`SHIFT`] up: the last entry
/// a quad can name, plus one.
const NAMED_MAX: u32 = u32::MAX >> SHIFT;

/// The most entries that lie side by side in a row of texels.
const ACROSS_MAX: u32 = 64;

/// A slot of the glyphs: its pool and its index there.
type Key = (usize, u32);

/// Each slot's rows of the ink beside its piece, the frame's entries, and
/// their texture.
#[derive(Debug)]
pub(crate) struct Besides {
    /// A cell's height: the texels of an entry.
    height: u32,
    /// Entries side by side in a row of texels.
    across: u32,
    /// The rows of each slot whose piece has ink beside it.
    rows: BTreeMap<Key, Box<[[u8; 2]]>>,
    /// The frame's entries, in order, and each one's place among them.
    entries: Vec<Key>,
    index: BTreeMap<Key, u32>,
    /// The entries the texture holds, and whether a slot's rows changed
    /// since it was filled.
    uploaded: Vec<Key>,
    changed: bool,
    texture: Option<(Texture, Size)>,
}

impl Besides {
    /// None yet, for cells `height` rows tall, in textures at most `side`
    /// texels a side.
    pub(crate) fn new(height: u32, side: u32) -> Besides {
        Besides {
            height,
            across: (side.checked_div(height).unwrap_or(1)).clamp(1, ACROSS_MAX),
            rows: BTreeMap::new(),
            entries: Vec::new(),
            index: BTreeMap::new(),
            uploaded: Vec::new(),
            changed: false,
            texture: None,
        }
    }

    /// Entries side by side in a row of texels, as the cell shader's
    /// `cell.w` takes them.
    pub(crate) fn across(&self) -> u32 {
        self.across
    }

    /// Notes what the piece just filled into slot `key` has beside it, or
    /// that it has nothing.
    pub(crate) fn set(&mut self, key: Key, beside: Option<&Beside>) {
        let old = match beside {
            Some(b) => self.rows.insert(key, b.rows().into()),
            None => self.rows.remove(&key),
        };
        if old.as_deref() != beside.map(Beside::rows) {
            self.changed = true;
        }
    }

    /// Starts a frame with no entries.
    pub(crate) fn begin(&mut self) {
        self.entries.clear();
        self.index.clear();
    }

    /// The entry of slot `key` plus one, given now if the frame has none
    /// for it yet, for a quad's `src[2]`; 0 when its piece has nothing
    /// beside it on a row of its cell where `underlines` says the quad's
    /// underline draws, since the shader then reads nothing here.
    ///
    /// A quad can name entries up to [`NAMED_MAX`] less one. Once a frame
    /// has given that many, a slot with no entry yet gets 0, so its piece's
    /// underline skips only the piece's own ink. A value clamped to the
    /// field's width would name the last entry, whose rows belong to
    /// another piece.
    pub(crate) fn entry(&mut self, key: Key, mut underlines: impl FnMut(usize) -> bool) -> u32 {
        let read = self.rows.get(&key).is_some_and(|rows| {
            rows.iter()
                .enumerate()
                .any(|(v, &row)| row != [0, 0] && underlines(v))
        });
        if !read {
            return 0;
        }
        if let Some(&n) = self.index.get(&key) {
            return n.saturating_add(1);
        }
        let next = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        if next >= NAMED_MAX {
            return 0;
        }
        self.entries.push(key);
        self.index.insert(key, next);
        next.saturating_add(1)
    }

    /// The texture, once it holds an entry.
    pub(crate) fn texture(&self) -> Option<&Texture> {
        self.texture.as_ref().map(|(t, _)| t)
    }

    /// Fills the texture with the frame's entries, making it first, or
    /// again with more rows, when it is too small. Gives how many calls of
    /// the frame's batch of uploads it made: none when the frame has no
    /// entry, or the entries and their rows are the texture's already.
    pub(crate) fn upload<G: Gl>(&mut self, gl: &mut G) -> Result<usize, Error> {
        if self.entries.is_empty() || (!self.changed && self.entries == self.uploaded) {
            return Ok(0);
        }
        let n = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        let lines = n.div_ceil(self.across);
        let width = self.across.saturating_mul(self.height);
        let mut calls = 0usize;
        let held = self.texture.as_ref().map_or(0, |&(_, s)| s.height);
        if held < lines {
            let size = Size::new(width, lines.next_power_of_two());
            if !gl.limits().fits_texture(size) {
                return Err(Error::new(
                    ErrorKind::Limit,
                    format!(
                        "the rows beside {n} pieces of magnified characters pass the largest texture"
                    ),
                ));
            }
            if let Some((old, _)) = self.texture.take() {
                gl.free_texture(old)?;
                calls = calls.saturating_add(1);
            }
            self.texture = Some((gl.create_texture(Format::Rgba8, size)?, size));
            calls = calls.saturating_add(1);
        }
        let Some((texture, _)) = &self.texture else {
            return Ok(calls);
        };
        let stride = usize::try_from(width).unwrap_or(0).saturating_mul(4);
        let mut pixels = vec![0u8; stride.saturating_mul(usize::try_from(lines).unwrap_or(0))];
        let h = usize::try_from(self.height).unwrap_or(0);
        let across = usize::try_from(self.across).unwrap_or(1).max(1);
        for (n, key) in self.entries.iter().enumerate() {
            let (line, column) = (
                n.checked_div(across).unwrap_or(0),
                n.checked_rem(across).unwrap_or(0),
            );
            let at = line
                .saturating_mul(stride)
                .saturating_add(column.saturating_mul(h).saturating_mul(4));
            let rows = self.rows.get(key).map(AsRef::as_ref).unwrap_or_default();
            let out = pixels.get_mut(at..at.saturating_add(h.saturating_mul(4)));
            for (texel, &[left, right]) in out.into_iter().flat_map(|o| o.chunks_mut(4)).zip(rows) {
                texel.copy_from_slice(&[left, right, 0, 0]);
            }
        }
        let rect = Rect {
            x: 0,
            y: 0,
            width,
            height: lines,
        };
        gl.fill_texture(texture, rect, &pixels, stride)?;
        self.uploaded.clone_from(&self.entries);
        self.changed = false;
        Ok(calls.saturating_add(1))
    }

    /// Frees the texture.
    pub(crate) fn free<G: Gl>(self, gl: &mut G) -> Result<(), Error> {
        match self.texture {
            Some((t, _)) => gl.free_texture(t),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "a test fails by panicking, on small known sizes"
)]
mod tests {
    use vtium_font::{Format as GlyphFormat, Glyph};

    use super::*;
    use crate::beside::Ink;

    // A quad holds its entry plus one in the 16 bits above the column, so
    // the most it can name is 0xffff, the entry 0xfffe. A slot past that
    // gets no entry, and its piece's underline skips only the piece's own
    // ink, rather than reading the rows of a slot another piece owns. A
    // slot that has an entry keeps it for the rest of the frame.
    #[test]
    fn a_slot_past_the_entries_a_quad_can_name_gets_none() {
        let left = Glyph::from_pixels(1, 1, GlyphFormat::Mask, vec![255]).unwrap();
        let ink = Some(Ink {
            glyph: &left,
            layer: None,
            scale: 1,
        });
        let beside = Beside::of((1, 1), 1, &[ink], &[None]).unwrap();
        let mut besides = Besides::new(1, 64);
        let last = NAMED_MAX;
        for i in 0..=last {
            besides.set((0, i), Some(&beside));
        }
        besides.begin();
        for i in 0..last {
            assert_eq!(besides.entry((0, i), |_| true), i + 1);
        }
        assert_eq!(besides.entry((0, last), |_| true), 0);
        assert_eq!(besides.entry((0, 7), |_| true), 8);
        assert_eq!(besides.entries.len(), usize::try_from(last).unwrap());
    }
}
