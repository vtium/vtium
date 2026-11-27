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
//! slots of the one before up to [`GLYPH_TEXTURE_SIDE`] pixels a side.
//!
//! A glyph whose ink passes its cells goes to a fifth pool, the boxes
//! (ADR-1306 D9), whose slots each hold the largest box a glyph's ink may
//! have, so that a cell's shader can read the ink a neighbour's glyph sends
//! it. The boxes count against the masks' budgets, as the CPU path counts
//! the same glyph against them, and lie in one texture, so that the shader
//! samples every neighbour's glyph from it: when the texture is full, a
//! texture with twice its rows takes its place, filled again from the
//! glyphs the boxes keep, until it reaches the largest side. A glyph the
//! boxes have no room for then is drawn cut at its cells. A color emoji
//! drawn in one cell, whose 1-pixel edge reaches each neighbour (ADR-1302
//! D5), takes a box too: its alpha in the boxes' texture, which says where
//! it holds ink, and its premultiplied RGBA in the color boxes, a texture
//! of the same slots that the boxes take only once such a glyph comes.
//!
//! A pool hands out its lowest free slot, so its glyphs gather in its first
//! textures, and gives back its last texture once no glyph has held a slot
//! there for [`GPU_IDLE_FRAMES`] frames. The pools and the sprites together
//! never pass [`GLYPH_TEXTURE_CEILING`] bytes of textures: past it a pool
//! takes no new texture, and a glyph it has no free slot for is drawn blank
//! (the issue on GPU texture garbage collection, ADR-1303 D9). That differs
//! from the CPU path, which draws every glyph, so the glyphs give a note
//! for the log when it happens, at most once in [`GPU_IDLE_FRAMES`]
//! frames.

use vtium_font::{Face, Font, Variant, Width};

use std::collections::BTreeSet;

use super::beside::Besides;
use super::gl::{Error, ErrorKind, Format, Gl, Size, Texture};
use super::ink::{InkGrid, ReceivingJob};
use crate::bounds::{GLYPH_TEXTURE_CEILING, GLYPH_TEXTURE_SIDE, GPU_IDLE_FRAMES};
use crate::cache::{Budgets, COLOR, GlyphCache, Held, Storage, WIDE, pool_of};
use crate::deco::{LINES, Line, Sprites};
use crate::mono::Fallback;
use crate::reach::{Bound, Reach};
use crate::renderer::double::Piece;
use crate::stored::Stored;
use crate::target::{Orientation, Rect};

mod boxes;

use boxes::{Boxes, sprite_grid, sprite_texture};

/// The slots a pool's first texture holds; each later one holds twice the
/// one before.
const FIRST_SLOTS: u32 = 64;

/// The pool a [`Slot`] names for a slot among the boxes.
const BOXES: usize = 4;

/// A glyph's slot in a pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pool: usize,
    index: u32,
    bytes: usize,
    /// How far the glyph's ink reaches past its cells: none outside the
    /// boxes.
    reach: Reach,
    /// A color glyph, which among the boxes lies in the color boxes too.
    color: bool,
    /// A piece of a sized run that mixes text with color glyphs: a slot of
    /// the wide color pool whose left cell holds the color layer and whose
    /// right cell holds the text's mask in its alpha ([`Glyphs::fill`]).
    mixed: bool,
}

impl Held for Slot {
    fn bytes(&self) -> usize {
        self.bytes
    }

    fn reaches(&self) -> bool {
        !self.reach.is_none()
    }

    const RECLAIM: bool = true;
}

/// Where a cell's glyph lies: a texture of the glyphs and the top left of
/// the glyph's cells in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    /// The texture, by its index in [`Glyphs::texture`].
    pub(crate) texture: usize,
    pub(crate) x: u32,
    pub(crate) y: u32,
    /// A color glyph, in an [`Format::Rgba8`] texture.
    pub(crate) color: bool,
    /// A sized run's piece of text and color glyphs, in an
    /// [`Format::Rgba8`] texture: the color layer in the slot's left cell,
    /// and the text's mask in the alpha of its right cell.
    pub(crate) mixed: bool,
    /// For a glyph whose ink passes its cells, its slot among the boxes and
    /// how far its ink reaches.
    pub(crate) boxed: Option<(u32, Reach)>,
    /// Its slot: the pool, the boxes among them, and its index there, by
    /// which [`Besides`] keeps the ink beside a piece.
    pub(crate) slot: (usize, u32),
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
    /// Slots no glyph holds, the lowest handed out first.
    free: BTreeSet<u32>,
    /// Slots held for this frame's uncached glyphs.
    held: Vec<u32>,
    /// The frame since which no glyph holds a slot of the last texture.
    empty_since: Option<u64>,
}

impl Pool {
    fn new(format: Format, slot: Size) -> Pool {
        Pool {
            format,
            slot,
            grids: Vec::new(),
            next: 0,
            free: BTreeSet::new(),
            held: Vec::new(),
            empty_since: None,
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

    /// Whether no glyph holds a slot of the last texture.
    fn last_empty(&self) -> bool {
        let Some(last) = self.grids.last() else {
            return false;
        };
        let start = self.capacity().saturating_sub(last.slots);
        let used = self.next.saturating_sub(start);
        let free = self.free.range(start..).count();
        usize::try_from(used).is_ok_and(|u| free >= u)
    }
}

/// The GPU path's glyphs for one renderer.
#[derive(Debug)]
pub(crate) struct Glyphs {
    /// The renderer whose font drew them.
    owner: u64,
    cache: GlyphCache<Slot>,
    pools: [Pool; 4],
    boxes: Boxes,
    /// Whether a glyph whose ink passes its cells has taken a box: until
    /// one has, no cell's ink reaches another.
    reaching: bool,
    /// What the cell shader reads of the neighbours whose ink a cell
    /// receives.
    pub(crate) ink: InkGrid,
    /// What the cell shader reads of the ink beside the pieces of
    /// magnified characters, whose underlines skip it.
    pub(crate) besides: Besides,
    /// Every texture of the pools, and the sprites', each by its place; a
    /// place a pool gave its texture back from is empty until a new one
    /// takes it.
    textures: Vec<Option<Texture>>,
    /// Bytes the textures hold together, at most `ceiling`.
    bytes: u64,
    ceiling: u64,
    /// Frames begun, which the pools' idle textures are counted in.
    frame: u64,
    /// The frame the last note of a glyph drawn blank past the ceiling was
    /// given in.
    noted: Option<u64>,
    /// The note for the log given since the notes were last taken, at
    /// most one, so a head that never takes them holds no more.
    note: Option<String>,
    /// The sprites' texture: each [`Line`] a cell, laid out in a grid of
    /// `sprite_columns` cells across, the first row left to right, then
    /// the next.
    sprites: usize,
    sprite_columns: u32,
    /// Every sprite, the doubled ones after the normal ones, until a line
    /// of double size first draws: the sprites' texture then holds them
    /// all. A screen with no such line uploads only the normal sprites.
    pending: Option<Vec<Stored>>,
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
    /// How far a glyph's ink may reach past its cells.
    pub(crate) bound: Bound,
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
        let normal = Line::all().filter_map(|line| spec.sprites.get(line));
        let (texture, sprite_columns, grid) = sprite_texture(gl, cell, normal.enumerate(), LINES)?;
        Ok(Glyphs {
            owner: spec.owner,
            cache: GlyphCache::new(spec.budgets, spec.storage, Orientation::Normal)
                .with_fallback(spec.fallback),
            pools,
            boxes: Boxes::new(cell, spec.bound, side),
            reaching: false,
            ink: InkGrid::default(),
            besides: Besides::new(cell.height, side),
            textures: vec![Some(texture)],
            bytes: texture_bytes(Format::R8, grid),
            ceiling: GLYPH_TEXTURE_CEILING,
            frame: 0,
            noted: None,
            note: None,
            sprites: 0,
            sprite_columns,
            pending: Some(spec.sprites.all().cloned().collect()),
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
        self.textures.get(i).and_then(Option::as_ref)
    }

    /// Bytes the textures hold together.
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Holds the textures to `ceiling` bytes in place of
    /// [`GLYPH_TEXTURE_CEILING`], for a test that drives a pool to it.
    #[cfg(test)]
    pub(crate) fn set_ceiling(&mut self, ceiling: u64) {
        self.ceiling = ceiling;
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

    /// Makes the sprites' texture hold every set of sprites, the doubled
    /// ones too, the first time a line of double size draws. The texture
    /// keeps its place among the textures, and the cell shader reads the
    /// new grid from [`Glyphs::sprite_columns`].
    pub(crate) fn double_sprites<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        let Some(all) = self.pending.take() else {
            return Ok(());
        };
        let count = all.len();
        let made = sprite_texture(gl, self.cell, all.iter().enumerate(), count);
        let (texture, columns, grid) = match made {
            Ok(made) => made,
            Err(e) => {
                self.pending = Some(all);
                return Err(e);
            }
        };
        let old = self
            .textures
            .get_mut(self.sprites)
            .and_then(|t| t.replace(texture));
        if let Some(old) = old {
            // the texture held the normal sprites alone
            let (_, size) = sprite_grid(self.cell, LINES);
            gl.free_texture(old)?;
            self.bytes = self.bytes.saturating_sub(texture_bytes(Format::R8, size));
        }
        self.bytes = self.bytes.saturating_add(texture_bytes(Format::R8, grid));
        self.sprite_columns = columns;
        self.uploads = self.uploads.saturating_add(2);
        Ok(())
    }

    /// The sprites' texture, by its index.
    pub(crate) fn sprites(&self) -> usize {
        self.sprites
    }

    /// The sprites across the sprites' texture, as the cell shader's
    /// `cell.z` takes them.
    pub(crate) fn sprite_columns(&self) -> u32 {
        self.sprite_columns
    }

    /// The boxes' texture, by its index, once a glyph has taken a box; the
    /// color boxes' texture, once a color glyph has; and a box slot's width
    /// and height and the slots across the texture, as the cell shader's
    /// `box_slot` takes them.
    pub(crate) fn boxes(&self) -> (Option<usize>, Option<usize>, [u32; 3]) {
        let b = &self.boxes;
        (b.texture, b.color, [b.slot.width, b.slot.height, b.cols])
    }

    /// Fills the ink grid's texture where its entries changed, and the
    /// texture of the ink beside the pieces whose underlines the frame
    /// draws, a call or more of the frame's batch of uploads each.
    pub(crate) fn ink_upload<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        let calls = self.ink.upload(gl)?;
        self.uploads = self.uploads.saturating_add(calls);
        let calls = self.besides.upload(gl)?;
        self.uploads = self.uploads.saturating_add(calls);
        Ok(())
    }

    /// Starts the ink grid's frame on a grid of `cols` by `rows` cells
    /// ([`InkGrid::begin`]).
    pub(crate) fn begin_ink(&mut self, cols: usize, rows: usize) {
        self.ink.begin(cols, rows, self.cell);
    }

    /// Settles the ink grid once the frame is scanned. Each glyph the frame
    /// named as a source of ink takes the reach and span that `reach` gives
    /// its cell ([`InkGrid::settle`]). Then the grid sets the line bits of
    /// each of `jobs`, the frame's jobs that receive ink. It learns which
    /// lines a neighbour's ink reaches from the notes kept on that
    /// neighbour's box when its glyph took the box ([`InkGrid::mark_jobs`]).
    pub(crate) fn settle_ink(
        &mut self,
        reach: impl FnMut(usize, usize) -> (Reach, u8),
        jobs: impl Iterator<Item = ReceivingJob>,
    ) {
        self.ink.settle(reach);
        let kept = &self.boxes.kept;
        let lines = |slot: u32| {
            let (_, lines) = kept.get(usize::try_from(slot).ok()?)?.as_ref()?;
            Some(lines)
        };
        self.ink
            .mark_jobs(jobs, (self.boxes.bound, self.cell), lines);
    }

    /// Whether a glyph whose ink passes its cells has taken a box: until
    /// one has, no cell's ink reaches another.
    pub(crate) fn reaching(&self) -> bool {
        self.reaching
    }

    /// How far the ink of the piece `piece` of `text`'s glyph reaches past
    /// its cells, the piece drawn and uploaded when it is not cached: none
    /// for a glyph the font draws nothing for, or one the boxes had no room
    /// for.
    pub(crate) fn reach<G: Gl, F: Face>(
        &mut self,
        gl: &mut G,
        font: &Font<F>,
        key: (&[char], Width, Variant),
        piece: Piece,
    ) -> Result<Reach, Error> {
        let place = self.glyph(gl, font, key, piece)?;
        Ok(place.and_then(|p| p.boxed).map_or(Reach::NONE, |(_, r)| r))
    }

    /// Starts a frame: glyphs used from now on are this frame's, and so
    /// are the entries of the ink beside pieces.
    pub(crate) fn begin_frame(&mut self) {
        self.cache.begin_frame();
        self.besides.begin();
    }

    /// Counts a frame, and gives back each pool's last texture that no
    /// glyph has held a slot of for [`GPU_IDLE_FRAMES`] frames, counted
    /// from the end of the frame that emptied it. Each texture freed is a
    /// call of the frame's batch of uploads.
    pub(crate) fn give_back<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        self.frame = self.frame.saturating_add(1);
        let frame = self.frame;
        if let Some(since) = self.boxes.empty_since
            && frame.saturating_sub(since) >= GPU_IDLE_FRAMES
        {
            let b = &mut self.boxes;
            let size = b.size(b.rows);
            (b.next, b.rows, b.empty_since) = (0, 0, None);
            b.free.clear();
            b.kept.clear();
            for (texture, format) in [
                (b.texture.take(), Format::R8),
                (b.color.take(), Format::Rgba8),
            ] {
                if let Some(t) = texture
                    .and_then(|i| self.textures.get_mut(i))
                    .and_then(Option::take)
                {
                    self.bytes = self.bytes.saturating_sub(texture_bytes(format, size));
                    self.uploads = self.uploads.saturating_add(1);
                    gl.free_texture(t)?;
                }
            }
        }
        for p in &mut self.pools {
            let Some(since) = p.empty_since else {
                continue;
            };
            if frame.saturating_sub(since) < GPU_IDLE_FRAMES {
                continue;
            }
            let Some(grid) = p.grids.pop() else { continue };
            let start = p.capacity();
            p.free.retain(|&i| i < start);
            p.next = p.next.min(start);
            // the texture before it waits its own idle frames
            p.empty_since = None;
            if let Some(t) = self.textures.get_mut(grid.texture).and_then(Option::take) {
                let size = Size::new(
                    grid.cols.saturating_mul(p.slot.width),
                    grid.slots
                        .div_ceil(grid.cols.max(1))
                        .saturating_mul(p.slot.height),
                );
                self.bytes = self.bytes.saturating_sub(texture_bytes(p.format, size));
                self.uploads = self.uploads.saturating_add(1);
                gl.free_texture(t)?;
            }
        }
        Ok(())
    }

    /// Ends a frame: the slots held for its uncached glyphs are free
    /// again, and a pool whose last texture this frame left with no glyph
    /// notes the frame.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        let b = &mut self.boxes;
        for i in b.held.drain(..) {
            if let Some(k) = usize::try_from(i).ok().and_then(|i| b.kept.get_mut(i)) {
                *k = None;
            }
            b.free.insert(i);
        }
        if b.texture.is_some() && b.empty() {
            b.empty_since.get_or_insert(frame);
        } else {
            b.empty_since = None;
        }
        for p in &mut self.pools {
            p.free.extend(p.held.drain(..));
            if p.last_empty() {
                p.empty_since.get_or_insert(frame);
            } else {
                p.empty_since = None;
            }
        }
    }

    /// The glyph lookups since the counts were last taken.
    pub(crate) fn take_counts(&mut self) -> crate::cache::CacheCounts {
        self.cache.take_counts()
    }

    /// The characters of each text the font drew as its missing glyph
    /// since this was last asked ([`GlyphCache::take_missing`]).
    pub(crate) fn take_missing(&mut self) -> Vec<char> {
        self.cache.take_missing()
    }

    /// Forgets each glyph whose text holds one of `chars`, which are
    /// sorted, and each glyph the font drew as its missing glyph
    /// ([`GlyphCache::forget`]).
    pub(crate) fn forget(&mut self, chars: &[char]) {
        self.cache.forget(chars);
        self.reclaim();
    }

    /// Where the piece `piece` of `text`'s glyph lies, drawn and uploaded
    /// when it is not cached; `None` when the font draws nothing. Each
    /// upload is a call of the frame's batch of uploads. A doubled piece is
    /// one cell, so it takes a slot of a one-cell pool whatever the glyph's
    /// width, or a box when its ink passes its cell.
    pub(crate) fn glyph<G: Gl, F: Face>(
        &mut self,
        gl: &mut G,
        font: &Font<F>,
        (text, width, style): (&[char], Width, Variant),
        piece: Piece,
    ) -> Result<Option<Place>, Error> {
        if let Some(&s) = self.cache.lookup((text, width, style), piece) {
            return Ok(self.place(s));
        }
        let Some((stored, missing)) = self.cache.draw(font, (text, width, style), piece) else {
            return Ok(None);
        };
        // a piece of text and color glyphs takes two cells of the color
        // pool, one for each of its layers
        let mixed = stored.layer().is_some();
        let pool = match mixed {
            true => pool_of(true, true),
            false => pool_of(stored.is_rgba(), width == Width::Wide && piece.is_whole()),
        };
        let key = ((text, width, style), piece);
        let stored = match self.boxed(gl, key, pool, (stored, missing))? {
            Ok(place) => return Ok(Some(place)),
            Err(stored) => stored,
        };
        let bytes = self.pools.get(pool).map_or(0, Pool::bytes);
        let kept = self.cache.room(pool, bytes);
        self.reclaim();
        let Some(index) = self.take(gl, pool)? else {
            // past the ceiling: drawn blank, and asked for again next frame
            self.cache.count_uncached();
            self.note(format!(
                "GPU glyph textures at their ceiling of {} MiB: glyphs past it are drawn blank",
                self.ceiling >> 20
            ));
            return Ok(None);
        };
        let slot = Slot {
            pool,
            index,
            bytes,
            reach: Reach::NONE,
            color: false,
            mixed,
        };
        if let Err(e) = self.fill(gl, slot, &stored) {
            if let Some(p) = self.pools.get_mut(pool) {
                p.free.insert(index);
            }
            return Err(e);
        }
        if kept {
            let key = ((text, width, style), piece);
            self.cache.keep_piece(key, pool, slot, missing);
        } else {
            if let Some(p) = self.pools.get_mut(pool) {
                p.held.push(index);
            }
            self.cache.count_uncached();
        }
        Ok(self.place(slot))
    }

    /// Puts `stored`, the piece `piece` of the glyph for `text` that counts
    /// against the cache's pool `pool`, in a box when its ink passes its
    /// cells, and gives its place; `missing` when the font drew it as its
    /// missing glyph. Gives
    /// `stored` back when its ink stays in its cells, or when the boxes
    /// have no room for it, which notes it for the log: it is then drawn
    /// cut at its cells, and none of its ink reaches another cell.
    fn boxed<G: Gl>(
        &mut self,
        gl: &mut G,
        key: ((&[char], Width, Variant), Piece),
        pool: usize,
        (stored, missing): (Stored, bool),
    ) -> Result<Result<Place, Stored>, Error> {
        let reach = stored.reach();
        if reach.is_none() {
            return Ok(Err(stored));
        }
        let color = stored.is_rgba();
        let bytes = self.boxes.bytes();
        let kept = self.cache.room(pool, bytes);
        self.reclaim();
        let Some(index) = self.take_box(gl, color)? else {
            self.note(format!(
                "GPU glyph textures hold no more glyphs whose ink passes their cells: \
                 glyphs past the {} held are drawn cut at their cells",
                self.boxes.next
            ));
            return Ok(Err(stored));
        };
        let lines = match self.fill_box(gl, index, &stored) {
            Ok(lines) => lines,
            Err(e) => {
                self.boxes.free.insert(index);
                return Err(e);
            }
        };
        if let Some(k) = usize::try_from(index)
            .ok()
            .and_then(|i| self.boxes.kept.get_mut(i))
        {
            *k = Some((stored, lines));
        }
        self.reaching = true;
        let slot = Slot {
            pool: BOXES,
            index,
            bytes,
            reach,
            color,
            mixed: false,
        };
        if kept {
            self.cache.keep_piece(key, pool, slot, missing);
        } else {
            self.boxes.held.push(index);
            self.cache.count_uncached();
        }
        self.place(slot)
            .map(Ok)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such box"))
    }

    /// Takes `stored` as the glyph for `text`, as a cache miss takes the
    /// font's: a test puts glyphs whose ink reaches further than the font
    /// crate's bound lets a font draw, here and through
    /// `Renderer::put_glyph_for_test`.
    pub(crate) fn put<G: Gl>(
        &mut self,
        gl: &mut G,
        (text, width, style): (&[char], Width, Variant),
        stored: Stored,
    ) -> Result<Option<Place>, Error> {
        let pool = pool_of(stored.is_rgba(), width == Width::Wide);
        let key = ((text, width, style), Piece::WHOLE);
        Ok(self.boxed(gl, key, pool, (stored, false))?.ok())
    }

    /// Gives `text` as a note for the log, unless a note was given in the
    /// last [`GPU_IDLE_FRAMES`] frames.
    fn note(&mut self, text: String) {
        let frame = self.frame;
        if self
            .noted
            .is_some_and(|n| frame.saturating_sub(n) < GPU_IDLE_FRAMES)
        {
            return;
        }
        self.noted = Some(frame);
        self.note = Some(text);
    }

    /// The note for the log given since the last call, if any.
    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        self.note.take().into_iter().collect()
    }

    /// Gives each slot eviction took back to its pool.
    fn reclaim(&mut self) {
        for s in self.cache.take_evicted() {
            if s.pool == BOXES {
                let b = &mut self.boxes;
                if let Some(k) = usize::try_from(s.index)
                    .ok()
                    .and_then(|i| b.kept.get_mut(i))
                {
                    *k = None;
                }
                b.free.insert(s.index);
            } else if let Some(p) = self.pools.get_mut(s.pool) {
                p.free.insert(s.index);
            }
        }
    }

    fn place(&self, s: Slot) -> Option<Place> {
        if s.pool == BOXES {
            let b = &self.boxes;
            let (x, y) = b.at(s.index);
            return Some(Place {
                texture: if s.color { b.color? } else { b.texture? },
                x: x.saturating_add(b.margin.0),
                y: y.saturating_add(b.margin.1),
                color: s.color,
                mixed: false,
                boxed: Some((s.index, s.reach)),
                slot: (BOXES, s.index),
            });
        }
        let (texture, x, y) = self.pools.get(s.pool)?.place(s.index)?;
        Some(Place {
            texture,
            x,
            y,
            color: s.pool & COLOR != 0,
            mixed: s.mixed,
            boxed: None,
            slot: (s.pool, s.index),
        })
    }

    /// A slot of pool `pool` no glyph holds: the lowest free one, the next
    /// of its textures, or the first of a new texture; `None` when a new
    /// texture would take the textures past their ceiling.
    fn take<G: Gl>(&mut self, gl: &mut G, pool: usize) -> Result<Option<u32>, Error> {
        let side = self.side;
        let p = self
            .pools
            .get_mut(pool)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such pool"))?;
        if let Some(i) = p.free.pop_first() {
            return Ok(Some(i));
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
            let bytes = texture_bytes(p.format, size);
            if self.bytes.saturating_add(bytes) > self.ceiling {
                return Ok(None);
            }
            let texture = gl.create_texture(p.format, size)?;
            self.uploads = self.uploads.saturating_add(1);
            self.bytes = self.bytes.saturating_add(bytes);
            // a place a texture was given back from takes the new one
            let at = match self.textures.iter().position(Option::is_none) {
                Some(at) => at,
                None => {
                    self.textures.push(None);
                    self.textures.len().saturating_sub(1)
                }
            };
            if let Some(t) = self.textures.get_mut(at) {
                *t = Some(texture);
            }
            p.grids.push(Grid {
                texture: at,
                cols,
                slots: cols.saturating_mul(rows),
            });
        }
        let i = p.next;
        p.next = p.next.saturating_add(1);
        Ok(Some(i))
    }

    /// Fills `slot` from `stored`'s lines, the size of its cells, as the CPU
    /// path reads them.
    fn fill<G: Gl>(&mut self, gl: &mut G, slot: Slot, stored: &Stored) -> Result<(), Error> {
        self.uploads = self.uploads.saturating_add(1);
        self.besides.set((slot.pool, slot.index), stored.beside());
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
            .and_then(Option::as_ref)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such texture"))?;
        let w = usize::try_from(p.slot.width).unwrap_or(0);
        let stride = w.saturating_mul(p.format.bytes());
        let mut pixels =
            vec![0u8; stride.saturating_mul(usize::try_from(p.slot.height).unwrap_or(0))];
        let mut line = vec![0u32; w];
        // a piece with a color layer: the layer in the slot's left cell,
        // the mask's coverage in the alpha of its right cell
        let cell = usize::try_from(self.cell.width).unwrap_or(0);
        let mut cov = vec![0u8; cell];
        for (row, out) in (0..).zip(pixels.chunks_mut(stride.max(1))) {
            if let (Some(layer), Format::Rgba8) = (stored.layer(), p.format) {
                layer.argb_line(0, row, &mut line);
                stored.mask_line(0, row, &mut cov);
                let (left, right) = out.split_at_mut(cell.saturating_mul(4).min(out.len()));
                for (px, &argb) in left.as_chunks_mut::<4>().0.iter_mut().zip(&line) {
                    px.copy_from_slice(&rgba(argb));
                }
                for (px, &a) in right.as_chunks_mut::<4>().0.iter_mut().zip(&cov) {
                    *px = [0, 0, 0, a];
                }
                continue;
            }
            match p.format {
                Format::R8 => stored.mask_line(0, row, out),
                Format::Rgba8 => {
                    stored.argb_line(0, row, &mut line);
                    for (px, &argb) in out.as_chunks_mut::<4>().0.iter_mut().zip(&line) {
                        px.copy_from_slice(&rgba(argb));
                    }
                }
                // a glyph pool is never the color table's 3D texture
                Format::Rgba16f => {}
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
        let mut out = self.ink.free(gl).and(self.besides.free(gl));
        for t in self.textures.into_iter().flatten() {
            out = out.and(gl.free_texture(t));
        }
        out
    }
}

/// Bytes a texture of `format` and `size` holds.
fn texture_bytes(format: Format, size: Size) -> u64 {
    size.rect()
        .area()
        .saturating_mul(u64::try_from(format.bytes()).unwrap_or(u64::MAX))
}

/// A premultiplied `0xAARRGGBB` pixel as the bytes of an
/// [`Format::Rgba8`] texel.
pub(crate) fn rgba(argb: u32) -> [u8; 4] {
    let [a, r, g, b] = argb.to_be_bytes();
    [r, g, b, a]
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests;
