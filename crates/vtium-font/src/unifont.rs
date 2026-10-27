//! [`Unifont`]: the built-in GNU Unifont, 8x16 and 16x16 glyphs for every
//! codepoint it covers, the font vtium draws with when no font file is set.
//!
//! The glyphs come from `data/unifont.hex` (GNU Unifont 17.0.03, the
//! unifont-all build, under the SIL Open Font License 1.1 in
//! `data/OFL-1.1.txt`). `build.rs` packs them in codepoint order into
//! chunks of at most 16 KiB, each deflated on its own, and indexes them by
//! runs of codepoints of one width. A chunk is inflated the first time a
//! glyph is drawn from it and kept for the life of the process, so the
//! binary carries the compressed font and memory holds only the chunks in
//! use. The C reference inflated the whole font, 3.7 MB, when it loaded
//! (font_unifont.c:203-245).
//!
//! The drawing follows the C reference's `font_unifont.c`, whose notice,
//! under the MIT licence whose text is in COPYING, is:
//!
//! Copyright (c) 2012 Ted Kotz <ted@kotz.us>
//! Copyright (c) 2012-2013 David Herrmann <dh.herrmann@googlemail.com>

use std::sync::OnceLock;

use crate::bounds::CELL_SIDE_MAX;
use crate::face::{Cell, Face, Style, Width};
use crate::glyph::Glyph;

/// A run of codepoints of one width whose glyphs lie together.
struct Block {
    first: u32,
    len: u32,
    wide: bool,
    /// Where the run's first glyph starts, in the unpacked bytes.
    offset: u32,
}

impl Block {
    const fn new(first: u32, len: u32, wide: bool, offset: u32) -> Block {
        Block {
            first,
            len,
            wide,
            offset,
        }
    }
}

/// Where a chunk starts, in the unpacked bytes and in the packed ones. The
/// last is the end of both.
struct Chunk {
    raw: u32,
    packed: u32,
}

impl Chunk {
    const fn new(raw: u32, packed: u32) -> Chunk {
        Chunk { raw, packed }
    }
}

include!(concat!(env!("OUT_DIR"), "/unifont.rs"));

static PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/unifont.z"));

/// Each chunk once inflated; an empty one if it would not inflate.
static INFLATED: [OnceLock<Box<[u8]>>; CHUNKS.len()] = [const { OnceLock::new() }; CHUNKS.len()];

/// The bytes of the glyph whose unpacked bytes start at `offset`, `len`
/// long. Chunks break only between glyphs.
fn glyph_bytes(offset: u32, len: u32) -> Option<&'static [u8]> {
    let i = CHUNKS.partition_point(|c| c.raw <= offset).checked_sub(1)?;
    let (start, end) = (CHUNKS.get(i)?, CHUNKS.get(i.checked_add(1)?)?);
    let chunk = INFLATED.get(i)?.get_or_init(|| {
        let packed = usize::try_from(start.packed)
            .ok()
            .zip(usize::try_from(end.packed).ok())
            .and_then(|(a, b)| PACKED.get(a..b));
        let raw = usize::try_from(end.raw.saturating_sub(start.raw)).unwrap_or(0);
        packed
            .and_then(|p| miniz_oxide::inflate::decompress_to_vec_with_limit(p, raw).ok())
            .unwrap_or_default()
            .into_boxed_slice()
    });
    let at = usize::try_from(offset.checked_sub(start.raw)?).ok()?;
    chunk.get(at..at.checked_add(usize::try_from(len).ok()?)?)
}

/// The block holding `code`.
fn block(code: u32) -> Option<&'static Block> {
    let i = BLOCKS.partition_point(|b| b.first <= code).checked_sub(1)?;
    BLOCKS.get(i).filter(|b| code.wrapping_sub(b.first) < b.len)
}

/// The built-in unifont, drawn at a whole multiple of 8x16.
#[derive(Clone, Copy, Debug)]
pub struct Unifont {
    scale: u32,
}

impl Default for Unifont {
    fn default() -> Unifont {
        Unifont { scale: 1 }
    }
}

impl Unifont {
    /// The font scaled by the largest whole factor of 16 that fits
    /// `height`, at least 1, as far as keeps a wide glyph's cell inside
    /// [`CELL_SIDE_MAX`].
    pub fn new(height: u32) -> Unifont {
        let mut font = Unifont::default();
        font.set_height(height);
        font
    }

    /// Rescales the font as [`Unifont::new`] does.
    pub fn set_height(&mut self, height: u32) {
        self.scale = (height / 16).clamp(1, CELL_SIDE_MAX / 16);
    }

    /// The factor glyphs are drawn at.
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// The glyph for `c` at its own width, 1 cell or 2, bold widening each
    /// byte of a row by a pixel to the right; `None` if unifont lacks it.
    pub fn render(&self, c: char, style: Style) -> Option<Glyph> {
        let code = u32::from(c);
        let b = block(code)?;
        let cells: u32 = if b.wide { 2 } else { 1 };
        let size = cells.saturating_mul(16);
        let offset = code
            .checked_sub(b.first)?
            .checked_mul(size)?
            .checked_add(b.offset)?;
        let data = glyph_bytes(offset, size)?;
        let scale = self.scale.max(1);
        let cell_w = scale.saturating_mul(8);
        let mut out = Glyph::mask(cell_w.saturating_mul(cells), scale.saturating_mul(16))?;
        let cell_w = usize::try_from(cell_w).ok()?;
        for (i, row) in (0u32..).zip(out.rows_mut()) {
            let src = i.checked_div(scale)?.saturating_mul(cells);
            let src = usize::try_from(src).ok()?;
            let src = data.get(src..)?;
            for (half, byte) in row.chunks_exact_mut(cell_w).zip(src) {
                let mut bits = *byte;
                if style.bold {
                    bits |= bits >> 1;
                }
                for (j, px) in (0u32..).zip(half.iter_mut()) {
                    let bit = 7u32.saturating_sub(j.checked_div(scale).unwrap_or(0));
                    *px = if bits >> bit & 1 != 0 { 0xff } else { 0 };
                }
            }
        }
        Some(out.with_cells(if b.wide { Width::Wide } else { Width::Narrow }))
    }
}

impl Face for Unifont {
    fn cell(&self) -> Cell {
        Cell {
            width: self.scale.saturating_mul(8),
            height: self.scale.saturating_mul(16),
        }
    }

    fn size_step(&self) -> u32 {
        16
    }

    fn has_glyph(&self, c: char) -> bool {
        block(u32::from(c)).is_some()
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.render(*cluster.first()?, style)
    }
}
