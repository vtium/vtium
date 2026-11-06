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
//! boxes have no room for then is drawn cut at its cells.
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

use vtium_font::{Face, Font, Style, Width};

use std::collections::BTreeSet;

use super::gl::{Error, ErrorKind, Format, Gl, Size, Texture};
use super::ink::InkGrid;
use crate::bounds::{GLYPH_TEXTURE_CEILING, GLYPH_TEXTURE_SIDE, GPU_IDLE_FRAMES};
use crate::cache::{Budgets, COLOR, GlyphCache, Held, Storage, WIDE, pool_of};
use crate::deco::{Kind, Sprites};
use crate::mono::Fallback;
use crate::reach::{Bound, Reach};
use crate::stored::Stored;
use crate::target::{Orientation, Rect};

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
    /// For a glyph whose ink passes its cells, its slot among the boxes and
    /// how far its ink reaches.
    pub(crate) boxed: Option<(u32, Reach)>,
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

/// The boxes: the slots of the glyphs whose ink passes their cells, each
/// as large as the largest box a glyph may have, a wide glyph's two cells
/// and the bound's cells to each side, by its row and the bound's rows
/// above and below. A glyph's first cell lies `margin` from its slot's top
/// left. The slots lie `cols` across in one texture, whose width stays and
/// whose rows grow, so a slot keeps its place when the texture grows.
#[derive(Debug)]
struct Boxes {
    slot: Size,
    margin: (u32, u32),
    cols: u32,
    /// Rows of slots the texture holds.
    rows: u32,
    /// The texture, by its index in [`Glyphs::texture`].
    texture: Option<usize>,
    /// Slots handed out so far, each below this index.
    next: u32,
    /// Slots no glyph holds, the lowest handed out first.
    free: BTreeSet<u32>,
    /// Slots held for this frame's uncached glyphs.
    held: Vec<u32>,
    /// Each slot's glyph, which fills a larger texture when the boxes grow.
    kept: Vec<Option<Stored>>,
    /// The frame since which no glyph holds a slot.
    empty_since: Option<u64>,
}

impl Boxes {
    /// The boxes for cells of `cell` at `bound`, in textures at most `side`
    /// pixels a side.
    fn new(cell: Size, bound: Bound, side: u32) -> Boxes {
        let (side_cells, rows) = (u32::from(bound.side), u32::from(bound.rows));
        let slot = Size::new(
            cell.width
                .saturating_mul(side_cells.saturating_mul(2).saturating_add(2)),
            cell.height
                .saturating_mul(rows.saturating_mul(2).saturating_add(1)),
        );
        Boxes {
            slot,
            margin: (
                cell.width.saturating_mul(side_cells),
                cell.height.saturating_mul(rows),
            ),
            cols: side.checked_div(slot.width).unwrap_or(0),
            rows: 0,
            texture: None,
            next: 0,
            free: BTreeSet::new(),
            held: Vec::new(),
            kept: Vec::new(),
            empty_since: None,
        }
    }

    fn bytes(&self) -> usize {
        usize::try_from(self.slot.rect().area()).unwrap_or(usize::MAX)
    }

    /// The top left of slot `index` in the texture.
    fn at(&self, index: u32) -> (u32, u32) {
        (
            index
                .checked_rem(self.cols)
                .unwrap_or(0)
                .saturating_mul(self.slot.width),
            index
                .checked_div(self.cols)
                .unwrap_or(0)
                .saturating_mul(self.slot.height),
        )
    }

    /// The texture's size at `rows` rows of slots.
    fn size(&self, rows: u32) -> Size {
        Size::new(
            self.cols.saturating_mul(self.slot.width),
            rows.saturating_mul(self.slot.height),
        )
    }

    /// `stored`'s coverage across a slot, its first cell `margin` from the
    /// slot's top left, into `out`, lines `stride` apart from `at`.
    fn draw(&self, stored: &Stored, out: &mut [u8], (x, y): (u32, u32), stride: usize) {
        let w = usize::try_from(self.slot.width).unwrap_or(0);
        let (mx, my) = (i64::from(self.margin.0), i64::from(self.margin.1));
        let x = usize::try_from(x).unwrap_or(0);
        for line in 0..self.slot.height {
            let at = usize::try_from(y.saturating_add(line))
                .unwrap_or(0)
                .saturating_mul(stride)
                .saturating_add(x);
            if let Some(row) = out.get_mut(at..at.saturating_add(w)) {
                stored.mask_line(mx.saturating_neg(), i64::from(line).saturating_sub(my), row);
            }
        }
    }

    /// Whether no glyph holds a slot.
    fn empty(&self) -> bool {
        usize::try_from(self.next).is_ok_and(|n| self.free.len() >= n)
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
            boxes: Boxes::new(cell, spec.bound, side),
            reaching: false,
            ink: InkGrid::default(),
            textures: vec![Some(texture)],
            bytes: texture_bytes(Format::R8, strip),
            ceiling: GLYPH_TEXTURE_CEILING,
            frame: 0,
            noted: None,
            note: None,
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

    /// The sprites' texture, by its index.
    pub(crate) fn sprites(&self) -> usize {
        self.sprites
    }

    /// The boxes' texture, by its index, once a glyph has taken a box, and
    /// a box slot's width and height and the slots across the texture, as
    /// the cell shader's `box_slot` takes them.
    pub(crate) fn boxes(&self) -> (Option<usize>, [u32; 3]) {
        let b = &self.boxes;
        (b.texture, [b.slot.width, b.slot.height, b.cols])
    }

    /// Fills the ink grid's texture where its entries changed, a call or
    /// more of the frame's batch of uploads.
    pub(crate) fn ink_upload<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        let calls = self.ink.upload(gl)?;
        self.uploads = self.uploads.saturating_add(calls);
        Ok(())
    }

    /// Whether a glyph whose ink passes its cells has taken a box: until
    /// one has, no cell's ink reaches another.
    pub(crate) fn reaching(&self) -> bool {
        self.reaching
    }

    /// How far the ink of `text`'s glyph reaches past its cells, the glyph
    /// drawn and uploaded when it is not cached: none for a glyph the font
    /// draws nothing for, or one the boxes had no room for.
    pub(crate) fn reach<G: Gl, F: Face>(
        &mut self,
        gl: &mut G,
        font: &Font<F>,
        text: &[char],
        width: Width,
        style: Style,
    ) -> Result<Reach, Error> {
        let place = self.glyph(gl, font, text, width, style)?;
        Ok(place.and_then(|p| p.boxed).map_or(Reach::NONE, |(_, r)| r))
    }

    /// Starts a frame: glyphs used from now on are this frame's.
    pub(crate) fn begin_frame(&mut self) {
        self.cache.begin_frame();
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
            if let Some(t) = b
                .texture
                .take()
                .and_then(|i| self.textures.get_mut(i))
                .and_then(Option::take)
            {
                self.bytes = self.bytes.saturating_sub(texture_bytes(Format::R8, size));
                self.uploads = self.uploads.saturating_add(1);
                gl.free_texture(t)?;
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

    /// Whether the font drew a glyph as its missing glyph since this was
    /// last asked ([`GlyphCache::take_drew_missing`]).
    pub(crate) fn take_drew_missing(&mut self) -> bool {
        self.cache.take_drew_missing()
    }

    /// Forgets each glyph whose text holds one of `chars`, which are
    /// sorted, and each glyph `font` draws as its missing glyph
    /// ([`GlyphCache::forget`]).
    pub(crate) fn forget<F: Face>(&mut self, chars: &[char], font: &Font<F>) {
        self.cache.forget(chars, font);
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
        let stored = match self.boxed(gl, (text, width, style), pool, stored)? {
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
        };
        if let Err(e) = self.fill(gl, slot, &stored) {
            if let Some(p) = self.pools.get_mut(pool) {
                p.free.insert(index);
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

    /// Puts `stored`, the glyph for `text` that counts against the cache's
    /// pool `pool`, in a box when its ink passes its cells, and gives its
    /// place. Gives `stored` back when its ink stays in its cells, or when
    /// the boxes have no room for it, which notes it for the log: it is
    /// then drawn cut at its cells, and none of its ink reaches another
    /// cell.
    fn boxed<G: Gl>(
        &mut self,
        gl: &mut G,
        (text, width, style): (&[char], Width, Style),
        pool: usize,
        stored: Stored,
    ) -> Result<Result<Place, Stored>, Error> {
        let reach = stored.reach();
        if reach.is_none() || stored.is_rgba() {
            return Ok(Err(stored));
        }
        let bytes = self.boxes.bytes();
        let kept = self.cache.room(pool, bytes);
        self.reclaim();
        let Some(index) = self.take_box(gl)? else {
            self.note(format!(
                "GPU glyph textures hold no more glyphs whose ink passes their cells: \
                 glyphs past the {} held are drawn cut at their cells",
                self.boxes.next
            ));
            return Ok(Err(stored));
        };
        if let Err(e) = self.fill_box(gl, index, &stored) {
            self.boxes.free.insert(index);
            return Err(e);
        }
        if let Some(k) = usize::try_from(index)
            .ok()
            .and_then(|i| self.boxes.kept.get_mut(i))
        {
            *k = Some(stored);
        }
        self.reaching = true;
        let slot = Slot {
            pool: BOXES,
            index,
            bytes,
            reach,
        };
        if kept {
            self.cache.keep(text, width, style, pool, slot);
        } else {
            self.boxes.held.push(index);
            self.cache.count_uncached();
        }
        self.place(slot)
            .map(Ok)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such box"))
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
                texture: b.texture?,
                x: x.saturating_add(b.margin.0),
                y: y.saturating_add(b.margin.1),
                color: false,
                boxed: Some((s.index, s.reach)),
            });
        }
        let (texture, x, y) = self.pools.get(s.pool)?.place(s.index)?;
        Some(Place {
            texture,
            x,
            y,
            color: s.pool & COLOR != 0,
            boxed: None,
        })
    }

    /// A slot among the boxes no glyph holds: the lowest free one, the
    /// next one, or the next one of a texture with twice the rows, which
    /// takes the old texture's place filled with every glyph the boxes
    /// keep. `None` when the texture is at the largest side, or a larger
    /// one would take the textures past their ceiling.
    fn take_box<G: Gl>(&mut self, gl: &mut G) -> Result<Option<u32>, Error> {
        let b = &mut self.boxes;
        if let Some(i) = b.free.pop_first() {
            return Ok(Some(i));
        }
        if b.next < b.cols.saturating_mul(b.rows) {
            let i = b.next;
            b.next = b.next.saturating_add(1);
            return Ok(Some(i));
        }
        let most = self.side.checked_div(b.slot.height).unwrap_or(0);
        if b.cols == 0 || most == 0 {
            return Err(Error::new(
                ErrorKind::Limit,
                format!(
                    "a {}x{} box of a glyph's ink passes the largest texture, {}",
                    b.slot.width, b.slot.height, self.side
                ),
            ));
        }
        let rows = match b.texture {
            None => FIRST_SLOTS.div_ceil(b.cols),
            Some(_) => b.rows.saturating_mul(2),
        }
        .min(most);
        if rows <= b.rows {
            return Ok(None);
        }
        let (old, new) = (b.size(b.rows), b.size(rows));
        let grown = texture_bytes(Format::R8, new).saturating_sub(texture_bytes(Format::R8, old));
        if self.bytes.saturating_add(grown) > self.ceiling {
            return Ok(None);
        }
        let texture = gl.create_texture(Format::R8, new)?;
        self.uploads = self.uploads.saturating_add(1);
        if b.kept.iter().any(Option::is_some) {
            let stride = usize::try_from(new.width).unwrap_or(0);
            let mut pixels = vec![0u8; usize::try_from(new.rect().area()).unwrap_or(0)];
            for (i, stored) in (0u32..).zip(&b.kept) {
                if let Some(stored) = stored {
                    b.draw(stored, &mut pixels, b.at(i), stride);
                }
            }
            if let Err(e) = gl.fill_texture(&texture, new.rect(), &pixels, stride) {
                let _ = gl.free_texture(texture);
                return Err(e);
            }
            self.uploads = self.uploads.saturating_add(1);
        }
        self.bytes = self.bytes.saturating_add(grown);
        let old = match b.texture {
            Some(at) => self.textures.get_mut(at).and_then(|t| t.replace(texture)),
            None => {
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
                b.texture = Some(at);
                None
            }
        };
        if let Some(old) = old {
            self.uploads = self.uploads.saturating_add(1);
            gl.free_texture(old)?;
        }
        b.rows = rows;
        let i = b.next;
        b.next = b.next.saturating_add(1);
        b.kept.resize_with(
            usize::try_from(b.cols.saturating_mul(rows)).unwrap_or(0),
            || None,
        );
        Ok(Some(i))
    }

    /// Fills box `index` from `stored`'s coverage across the whole slot.
    fn fill_box<G: Gl>(&mut self, gl: &mut G, index: u32, stored: &Stored) -> Result<(), Error> {
        self.uploads = self.uploads.saturating_add(1);
        let b = &self.boxes;
        let texture = b
            .texture
            .and_then(|i| self.textures.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no texture of boxes"))?;
        let stride = usize::try_from(b.slot.width).unwrap_or(0);
        let mut pixels = vec![0u8; usize::try_from(b.slot.rect().area()).unwrap_or(0)];
        b.draw(stored, &mut pixels, (0, 0), stride);
        let (x, y) = b.at(index);
        let rect = Rect {
            x,
            y,
            width: b.slot.width,
            height: b.slot.height,
        };
        gl.fill_texture(texture, rect, &pixels, stride)
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
        let mut out = self.ink.free(gl);
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
mod tests {
    use vtium_font::Unifont;

    use super::*;
    use crate::cache::Profile;
    use crate::gpu::{Call, StandIn};

    fn glyphs(gl: &mut StandIn, sprites: &Sprites) -> Glyphs {
        let spec = Spec {
            owner: 1,
            budgets: Budgets::for_profile(Profile::Performance, 1000, 800, 600),
            storage: Storage::default(),
            fallback: None,
            cell: Size::new(8, 16),
            sprites,
            bound: Bound::DRAWN,
        };
        Glyphs::new(gl, &spec).unwrap()
    }

    // the issue on GPU texture garbage collection: past the ceiling a pool
    // takes no new texture, a glyph it has no slot for is drawn blank, and
    // a slot given back is taken again
    #[test]
    fn past_the_ceiling_a_pool_takes_no_texture_and_reuses_a_slot_given_back() {
        let font = Font::new(Unifont::new(16));
        let sprites = Sprites::new(8, 16, &font.decorations(), Orientation::Normal, true, false);
        let mut gl = StandIn::new();
        let mut g = glyphs(&mut gl, &sprites);
        // the sprites and one texture of 64 slots, 512x16 at a byte a pixel
        let ceiling = g.bytes() + 512 * 16;
        g.set_ceiling(ceiling);
        g.begin_frame();
        let style = Style::default();
        let chars: Vec<char> = ('A'..).take(65).collect();
        for &c in &chars[..64] {
            assert!(
                g.glyph(&mut gl, &font, &[c], Width::Narrow, style)
                    .unwrap()
                    .is_some()
            );
        }
        let textures = |gl: &StandIn| {
            gl.calls()
                .iter()
                .filter(|c| matches!(c, Call::CreateTexture(..)))
                .count()
        };
        let made = textures(&gl);
        // the 65th has no slot and no room for a texture: drawn blank
        let last = [chars[64]];
        assert_eq!(
            g.glyph(&mut gl, &font, &last, Width::Narrow, style)
                .unwrap(),
            None
        );
        assert_eq!(textures(&gl), made);
        assert_eq!(g.bytes(), ceiling);
        // the first blank glyph gives one note for the log; there is none
        // in the same frame, a frame later, or one frame short of
        // GPU_IDLE_FRAMES, and there is one again at GPU_IDLE_FRAMES
        assert_eq!(g.take_notes().len(), 1);
        let blank_at = |g: &mut Glyphs, gl: &mut StandIn, frames: u64| {
            for _ in 0..frames {
                g.give_back(gl).unwrap();
            }
            let drawn = g.glyph(gl, &font, &last, Width::Narrow, style);
            assert_eq!(drawn.unwrap(), None);
            g.take_notes().len()
        };
        assert_eq!(blank_at(&mut g, &mut gl, 0), 0);
        assert_eq!(blank_at(&mut g, &mut gl, 1), 0);
        assert_eq!(blank_at(&mut g, &mut gl, GPU_IDLE_FRAMES - 2), 0);
        assert_eq!(blank_at(&mut g, &mut gl, 1), 1);
        // a head that never takes the notes holds at most one
        for _ in 0..3 {
            for _ in 0..GPU_IDLE_FRAMES {
                g.give_back(&mut gl).unwrap();
            }
            let drawn = g.glyph(&mut gl, &font, &last, Width::Narrow, style);
            assert_eq!(drawn.unwrap(), None);
        }
        assert_eq!(g.take_notes().len(), 1);
        g.end_frame();
        // a glyph forgotten gives its slot back, which the 65th then takes
        g.forget(&['A'], &font);
        g.begin_frame();
        assert!(
            g.glyph(&mut gl, &font, &last, Width::Narrow, style)
                .unwrap()
                .is_some()
        );
        assert_eq!(textures(&gl), made);
    }
}
