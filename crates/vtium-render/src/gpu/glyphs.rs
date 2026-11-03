//! The GPU path's glyphs: ADR-1300's four pools as textures, and the
//! decoration sprites (ADR-1300 D6).
//!
//! The pools are the CPU path's: masks and color glyphs, one or two cells
//! wide, under the same budgets and the same LRU, through the same
//! [`GlyphCache`], which holds each glyph's slot where the CPU path holds
//! its pixels. A slot is the size of the glyph's cells, and a pool lays its
//! slots out as a grid in each of its textures, masks in
//! [`Format::R8`] and color glyphs in [`Format::Rgba8`]. A glyph is drawn
//! from the font once, as the CPU path stores it, and its slot is filled
//! from the stored glyph's lines at the size it is drawn, so its pixels are
//! the ones the CPU path reads.
//!
//! A glyph the pool's budget has no room for, one that would evict a glyph
//! this frame draws, is drawn from a slot held for this frame alone and
//! given back at its end (ADR-1300 D3). The next frame asks the budget
//! again, so the pool takes the glyph once the frame no longer draws every
//! glyph it holds. A pool grows a texture at a time, each holding twice the
//! slots of the one before up to [`GLYPH_TEXTURE_SIDE`] pixels a side, and
//! keeps its textures while the glyphs' renderer draws.

use vtium_font::{Face, Font, Style, Width};

use super::gl::{Error, ErrorKind, Format, Gl, Size, Texture};
use crate::bounds::GLYPH_TEXTURE_SIDE;
use crate::cache::{Budgets, COLOR, GlyphCache, Held, Storage, WIDE, pool_of};
use crate::deco::{Kind, Sprites};
use crate::mono::Fallback;
use crate::stored::Stored;
use crate::target::{Orientation, Rect};

/// The slots a pool's first texture holds; each later one holds twice the
/// one before.
const FIRST_SLOTS: u32 = 64;

/// A glyph's slot in a pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pool: usize,
    index: u32,
    bytes: usize,
}

impl Held for Slot {
    fn bytes(&self) -> usize {
        self.bytes
    }

    const RECLAIM: bool = true;
}

/// Where a cell's glyph lies: a texture of the glyphs and the slot's top
/// left in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    /// The texture, by its index in [`Glyphs::texture`].
    pub(crate) texture: usize,
    pub(crate) x: u32,
    pub(crate) y: u32,
    /// A color glyph, in an [`Format::Rgba8`] texture.
    pub(crate) color: bool,
}

/// One texture of a pool: its slots, laid out `cols` across.
#[derive(Debug)]
struct Grid {
    texture: usize,
    cols: u32,
    slots: u32,
}

/// One pool's textures and slots.
#[derive(Debug)]
struct Pool {
    format: Format,
    /// A slot's size: the glyph's cells.
    slot: Size,
    grids: Vec<Grid>,
    /// Slots handed out so far, each below this index.
    next: u32,
    /// Slots no glyph holds.
    free: Vec<u32>,
    /// Slots held for this frame's uncached glyphs.
    held: Vec<u32>,
}

impl Pool {
    fn new(format: Format, slot: Size) -> Pool {
        Pool {
            format,
            slot,
            grids: Vec::new(),
            next: 0,
            free: Vec::new(),
            held: Vec::new(),
        }
    }

    fn bytes(&self) -> usize {
        let px = u64::from(self.slot.width).saturating_mul(u64::from(self.slot.height));
        usize::try_from(px)
            .unwrap_or(usize::MAX)
            .saturating_mul(self.format.bytes())
    }

    /// The texture and top left of slot `index`.
    fn place(&self, index: u32) -> Option<(usize, u32, u32)> {
        let mut first = 0u32;
        for g in &self.grids {
            let end = first.saturating_add(g.slots);
            if index < end {
                let i = index.saturating_sub(first);
                let (col, row) = (i.checked_rem(g.cols)?, i.checked_div(g.cols)?);
                return Some((
                    g.texture,
                    col.saturating_mul(self.slot.width),
                    row.saturating_mul(self.slot.height),
                ));
            }
            first = end;
        }
        None
    }

    fn capacity(&self) -> u32 {
        self.grids.iter().fold(0, |n, g| n.saturating_add(g.slots))
    }
}

/// The GPU path's glyphs for one renderer.
#[derive(Debug)]
pub(crate) struct Glyphs {
    /// The renderer whose font drew them.
    owner: u64,
    cache: GlyphCache<Slot>,
    pools: [Pool; 4],
    /// Every texture of the pools, and the sprites' last.
    textures: Vec<Texture>,
    /// The sprites' texture: each [`Kind`] a cell side by side.
    sprites: usize,
    /// The largest side of a texture.
    side: u32,
    cell: Size,
    /// Textures made and filled since the count was last taken.
    uploads: usize,
}

/// What [`Glyphs::new`] needs of the renderer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Spec<'a> {
    pub(crate) owner: u64,
    pub(crate) budgets: Budgets,
    pub(crate) storage: Storage,
    pub(crate) fallback: Option<Fallback>,
    pub(crate) cell: Size,
    pub(crate) sprites: &'a Sprites,
}

impl Glyphs {
    /// Empty pools for `spec`'s renderer, and its sprites uploaded: one
    /// batch of uploads, which the caller checks.
    pub(crate) fn new<G: Gl>(gl: &mut G, spec: &Spec<'_>) -> Result<Glyphs, Error> {
        let side = gl.limits().texture.min(GLYPH_TEXTURE_SIDE);
        let cell = spec.cell;
        let wide = Size::new(cell.width.saturating_mul(2), cell.height);
        let pools = [0, WIDE, COLOR, COLOR | WIDE].map(|p| {
            let format = if p & COLOR != 0 {
                Format::Rgba8
            } else {
                Format::R8
            };
            Pool::new(format, if p & WIDE != 0 { wide } else { cell })
        });
        let kinds = u32::try_from(Kind::ALL.len()).unwrap_or(u32::MAX);
        let strip = Size::new(cell.width.saturating_mul(kinds), cell.height);
        let texture = gl.create_texture(Format::R8, strip)?;
        let mut pixels = vec![0u8; usize::try_from(strip.rect().area()).unwrap_or(0)];
        let stride = usize::try_from(strip.width).unwrap_or(0);
        let cw = usize::try_from(cell.width).unwrap_or(0);
        for (k, kind) in Kind::ALL.iter().enumerate() {
            let Some(s) = spec.sprites.get(*kind) else {
                continue;
            };
            for (y, line) in (0..).zip(pixels.chunks_mut(stride.max(1))) {
                let at = k.saturating_mul(cw);
                if let Some(out) = line.get_mut(at..at.saturating_add(cw)) {
                    s.mask_line(0, y, out);
                }
            }
        }
        if let Err(e) = gl.fill_texture(&texture, strip.rect(), &pixels, stride) {
            // the error that matters is the upload's
            let _ = gl.free_texture(texture);
            return Err(e);
        }
        Ok(Glyphs {
            owner: spec.owner,
            cache: GlyphCache::new(spec.budgets, spec.storage, Orientation::Normal)
                .with_fallback(spec.fallback),
            pools,
            textures: vec![texture],
            sprites: 0,
            side,
            cell,
            uploads: 2,
        })
    }

    /// The renderer whose glyphs these are.
    pub(crate) fn owner(&self) -> u64 {
        self.owner
    }

    /// Texture `i`.
    pub(crate) fn texture(&self, i: usize) -> Option<&Texture> {
        self.textures.get(i)
    }

    /// The cell's size.
    pub(crate) fn cell(&self) -> Size {
        self.cell
    }

    /// Textures made and filled since the last call: the calls of the
    /// frame's batch of uploads.
    pub(crate) fn take_uploads(&mut self) -> usize {
        core::mem::take(&mut self.uploads)
    }

    /// The sprites' texture, by its index.
    pub(crate) fn sprites(&self) -> usize {
        self.sprites
    }

    /// Starts a frame: glyphs used from now on are this frame's.
    pub(crate) fn begin_frame(&mut self) {
        self.cache.begin_frame();
    }

    /// Ends a frame: the slots held for its uncached glyphs are free again.
    pub(crate) fn end_frame(&mut self) {
        for p in &mut self.pools {
            p.free.append(&mut p.held);
        }
    }

    /// The glyph lookups since the counts were last taken.
    pub(crate) fn take_counts(&mut self) -> crate::cache::CacheCounts {
        self.cache.take_counts()
    }

    /// Forgets each glyph whose text holds one of `chars`, which are
    /// sorted.
    pub(crate) fn forget(&mut self, chars: &[char]) {
        self.cache.forget(chars);
        self.reclaim();
    }

    /// Where `text`'s glyph lies, drawn and uploaded when it is not
    /// cached; `None` when the font draws nothing. Each upload is a call of
    /// the frame's batch of uploads.
    pub(crate) fn glyph<G: Gl, F: Face>(
        &mut self,
        gl: &mut G,
        font: &Font<F>,
        text: &[char],
        width: Width,
        style: Style,
    ) -> Result<Option<Place>, Error> {
        if let Some(&s) = self.cache.lookup(text, width, style) {
            return Ok(self.place(s));
        }
        let Some(stored) = self.cache.draw(font, text, width, style) else {
            return Ok(None);
        };
        let pool = pool_of(stored.is_rgba(), width == Width::Wide);
        let bytes = self.pools.get(pool).map_or(0, Pool::bytes);
        let kept = self.cache.room(pool, bytes);
        self.reclaim();
        let index = self.take(gl, pool)?;
        let slot = Slot { pool, index, bytes };
        if let Err(e) = self.fill(gl, slot, &stored) {
            if let Some(p) = self.pools.get_mut(pool) {
                p.free.push(index);
            }
            return Err(e);
        }
        if kept {
            self.cache.keep(text, width, style, pool, slot);
        } else {
            if let Some(p) = self.pools.get_mut(pool) {
                p.held.push(index);
            }
            self.cache.count_uncached();
        }
        Ok(self.place(slot))
    }

    /// Gives each slot eviction took back to its pool.
    fn reclaim(&mut self) {
        for s in self.cache.take_evicted() {
            if let Some(p) = self.pools.get_mut(s.pool) {
                p.free.push(s.index);
            }
        }
    }

    fn place(&self, s: Slot) -> Option<Place> {
        let (texture, x, y) = self.pools.get(s.pool)?.place(s.index)?;
        Some(Place {
            texture,
            x,
            y,
            color: s.pool & COLOR != 0,
        })
    }

    /// A slot of pool `pool` no glyph holds: a free one, the next of its
    /// textures, or the first of a new texture.
    fn take<G: Gl>(&mut self, gl: &mut G, pool: usize) -> Result<u32, Error> {
        let side = self.side;
        let p = self
            .pools
            .get_mut(pool)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such pool"))?;
        if let Some(i) = p.free.pop() {
            return Ok(i);
        }
        if p.next >= p.capacity() {
            let cols = side.checked_div(p.slot.width).unwrap_or(0);
            let rows = side.checked_div(p.slot.height).unwrap_or(0);
            if cols == 0 || rows == 0 {
                return Err(Error::new(
                    ErrorKind::Limit,
                    format!(
                        "a {}x{} glyph passes the largest texture, {side}",
                        p.slot.width, p.slot.height
                    ),
                ));
            }
            let shift = u32::try_from(p.grids.len()).unwrap_or(u32::MAX).min(16);
            let slots = (FIRST_SLOTS << shift).min(cols.saturating_mul(rows));
            let cols = cols.min(slots);
            let rows = slots.div_ceil(cols);
            let size = Size::new(
                cols.saturating_mul(p.slot.width),
                rows.saturating_mul(p.slot.height),
            );
            let texture = gl.create_texture(p.format, size)?;
            self.uploads = self.uploads.saturating_add(1);
            p.grids.push(Grid {
                texture: self.textures.len(),
                cols,
                slots: cols.saturating_mul(rows),
            });
            self.textures.push(texture);
        }
        let i = p.next;
        p.next = p.next.saturating_add(1);
        Ok(i)
    }

    /// Fills `slot` from `stored`'s lines, the size of its cells, as the CPU
    /// path reads them.
    fn fill<G: Gl>(&mut self, gl: &mut G, slot: Slot, stored: &Stored) -> Result<(), Error> {
        self.uploads = self.uploads.saturating_add(1);
        let p = self
            .pools
            .get(slot.pool)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such pool"))?;
        let (texture, x, y) = p
            .place(slot.index)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such slot"))?;
        let texture = self
            .textures
            .get(texture)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such texture"))?;
        let w = usize::try_from(p.slot.width).unwrap_or(0);
        let stride = w.saturating_mul(p.format.bytes());
        let mut pixels =
            vec![0u8; stride.saturating_mul(usize::try_from(p.slot.height).unwrap_or(0))];
        let mut line = vec![0u32; w];
        for (row, out) in (0..).zip(pixels.chunks_mut(stride.max(1))) {
            match p.format {
                Format::R8 => stored.mask_line(0, row, out),
                Format::Rgba8 => {
                    stored.argb_line(0, row, &mut line);
                    for (px, &argb) in out.as_chunks_mut::<4>().0.iter_mut().zip(&line) {
                        px.copy_from_slice(&rgba(argb));
                    }
                }
            }
        }
        let rect = Rect {
            x,
            y,
            width: p.slot.width,
            height: p.slot.height,
        };
        gl.fill_texture(texture, rect, &pixels, stride)
    }

    /// Frees every texture.
    pub(crate) fn free<G: Gl>(self, gl: &mut G) -> Result<(), Error> {
        let mut out = Ok(());
        for t in self.textures {
            out = out.and(gl.free_texture(t));
        }
        out
    }
}

/// A premultiplied `0xAARRGGBB` pixel as the bytes of an
/// [`Format::Rgba8`] texel.
pub(crate) fn rgba(argb: u32) -> [u8; 4] {
    let [a, r, g, b] = argb.to_be_bytes();
    [r, g, b, a]
}
