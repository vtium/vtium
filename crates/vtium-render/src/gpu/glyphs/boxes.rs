//! The boxes and the sprites' texture: [`Boxes`] holds the glyphs whose ink
//! passes their cells, each in a slot as large as the largest box a glyph
//! may have, and the sprites' texture holds the decoration sprites.

use std::collections::BTreeSet;

#[cfg(doc)]
use crate::deco::Line;
use crate::gpu::box_lines::BoxLines;
use crate::gpu::gl::{Error, ErrorKind, Format, Gl, Size, Texture};
use crate::reach::Bound;
use crate::stored::Stored;
use crate::target::Rect;

use super::{BOXES, FIRST_SLOTS, Glyphs, rgba, texture_bytes};

/// The boxes: the slots of the glyphs whose ink passes their cells, each
/// as large as the largest box a glyph may have, a wide glyph's two cells
/// and the bound's cells to each side, by its row and the bound's rows
/// above and below. A glyph's first cell lies `margin` from its slot's top
/// left. The slots lie `cols` across in one texture, whose width stays and
/// whose rows grow, so a slot keeps its place when the texture grows.
#[derive(Debug)]
pub(super) struct Boxes {
    /// How far a glyph's ink may reach, which sets the slot's size.
    pub(super) bound: Bound,
    pub(super) slot: Size,
    pub(super) margin: (u32, u32),
    pub(super) cols: u32,
    /// Rows of slots the texture holds.
    pub(super) rows: u32,
    /// The texture, by its index in [`Glyphs::texture`].
    pub(super) texture: Option<usize>,
    /// The color boxes' texture, [`Format::Rgba8`], of the same slots, by
    /// its index in [`Glyphs::texture`], once a color glyph has taken a
    /// box.
    pub(super) color: Option<usize>,
    /// Slots handed out so far, each below this index.
    pub(super) next: u32,
    /// Slots no glyph holds, the lowest handed out first.
    pub(super) free: BTreeSet<u32>,
    /// Slots held for this frame's uncached glyphs.
    pub(super) held: Vec<u32>,
    /// Each slot's glyph, which fills a larger texture when the boxes grow,
    /// and where its box holds ink, which sets the line bits of the cells
    /// its ink reaches.
    pub(super) kept: Vec<Option<(Stored, BoxLines)>>,
    /// The frame since which no glyph holds a slot.
    pub(super) empty_since: Option<u64>,
}

impl Boxes {
    /// The boxes for cells of `cell` at `bound`, in textures at most `side`
    /// pixels a side.
    pub(super) fn new(cell: Size, bound: Bound, side: u32) -> Boxes {
        let (side_cells, rows) = (u32::from(bound.side), u32::from(bound.rows));
        let slot = Size::new(
            cell.width
                .saturating_mul(side_cells.saturating_mul(2).saturating_add(2)),
            cell.height
                .saturating_mul(rows.saturating_mul(2).saturating_add(1)),
        );
        Boxes {
            bound,
            slot,
            margin: (
                cell.width.saturating_mul(side_cells),
                cell.height.saturating_mul(rows),
            ),
            cols: side.checked_div(slot.width).unwrap_or(0),
            rows: 0,
            texture: None,
            color: None,
            next: 0,
            free: BTreeSet::new(),
            held: Vec::new(),
            kept: Vec::new(),
            empty_since: None,
        }
    }

    pub(super) fn bytes(&self) -> usize {
        usize::try_from(self.slot.rect().area()).unwrap_or(usize::MAX)
    }

    /// The top left of slot `index` in the texture.
    pub(super) fn at(&self, index: u32) -> (u32, u32) {
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
    pub(super) fn size(&self, rows: u32) -> Size {
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

    /// A color `stored`'s premultiplied RGBA across a slot, as
    /// [`Boxes::draw`] lays its coverage, into `out`, four bytes a pixel,
    /// lines `stride` bytes apart from `at`.
    fn draw_rgba(&self, stored: &Stored, out: &mut [u8], (x, y): (u32, u32), stride: usize) {
        let w = usize::try_from(self.slot.width).unwrap_or(0);
        let (mx, my) = (i64::from(self.margin.0), i64::from(self.margin.1));
        let x = usize::try_from(x).unwrap_or(0).saturating_mul(4);
        let mut line = vec![0u32; w];
        for row in 0..self.slot.height {
            let at = usize::try_from(y.saturating_add(row))
                .unwrap_or(0)
                .saturating_mul(stride)
                .saturating_add(x);
            stored.argb_line(
                mx.saturating_neg(),
                i64::from(row).saturating_sub(my),
                &mut line,
            );
            if let Some(out) = out.get_mut(at..at.saturating_add(w.saturating_mul(4))) {
                for (px, &argb) in out.as_chunks_mut::<4>().0.iter_mut().zip(&line) {
                    px.copy_from_slice(&rgba(argb));
                }
            }
        }
    }

    /// The color boxes' pixels at `size`, every color glyph the boxes keep
    /// drawn in its slot.
    fn color_pixels(&self, size: Size) -> Vec<u8> {
        let stride = usize::try_from(size.width).unwrap_or(0).saturating_mul(4);
        let mut pixels =
            vec![0u8; stride.saturating_mul(usize::try_from(size.height).unwrap_or(0))];
        for (i, kept) in (0u32..).zip(&self.kept) {
            if let Some((stored, _)) = kept.as_ref().filter(|(s, _)| s.is_rgba()) {
                self.draw_rgba(stored, &mut pixels, self.at(i), stride);
            }
        }
        pixels
    }

    /// Whether no glyph holds a slot.
    pub(super) fn empty(&self) -> bool {
        usize::try_from(self.next).is_ok_and(|n| self.free.len() >= n)
    }
}

/// The sprites across the sprites' texture for a `cell`, and the
/// texture's size: of the grids that hold every [`Line`], each kind of
/// line in each script's set (ADR-1202 D8), the one whose
/// larger side is the smallest, and of those the one with the fewest empty
/// places. A row of the sprites would grow with their number until a
/// wide cell passed the largest texture, so the grid keeps the texture
/// near square: an 8x16 cell's 24 sprites lie six across and four down,
/// 48x64, where a row of them is 192x16.
///
/// A line of double size draws the doubled sprites as well, so once one
/// draws, the texture holds `count` sprites, every set of them.
pub(super) fn sprite_grid(cell: Size, count: usize) -> (u32, Size) {
    let kinds = u32::try_from(count).unwrap_or(u32::MAX).max(1);
    (1..=kinds)
        .map(|across| {
            let down = kinds.div_ceil(across);
            let size = Size::new(
                cell.width.saturating_mul(across),
                cell.height.saturating_mul(down),
            );
            (across, size)
        })
        .min_by_key(|(_, s)| (s.width.max(s.height), s.rect().area()))
        .unwrap_or((1, Size::new(cell.width, cell.height.saturating_mul(kinds))))
}

/// The sprites' texture for a `cell`, holding `count` sprites: each of
/// `sprites` at its index, laid out as [`sprite_grid`] lays them out. Gives
/// the texture, the sprites across it and its size: one creation and one
/// upload of the frame's batch, or an error that names the sprites when
/// the grid passes the largest texture.
pub(super) fn sprite_texture<'s, G: Gl>(
    gl: &mut G,
    cell: Size,
    sprites: impl Iterator<Item = (usize, &'s Stored)>,
    count: usize,
) -> Result<(Texture, u32, Size), Error> {
    let (sprite_columns, grid) = sprite_grid(cell, count);
    let largest = gl.limits().texture;
    if !gl.limits().fits_texture(grid) {
        return Err(Error::new(
            ErrorKind::Limit,
            format!(
                "the decoration sprites of a {}x{} cell take a {}x{} texture, \
                 which passes the largest texture, {largest}",
                cell.width, cell.height, grid.width, grid.height
            ),
        ));
    }
    let texture = gl.create_texture(Format::R8, grid)?;
    let mut pixels = vec![0u8; usize::try_from(grid.rect().area()).unwrap_or(0)];
    let stride = usize::try_from(grid.width).unwrap_or(0);
    let (cw, ch) = (
        usize::try_from(cell.width).unwrap_or(0),
        usize::try_from(cell.height).unwrap_or(0),
    );
    let across = usize::try_from(sprite_columns).unwrap_or(1);
    for (k, s) in sprites {
        let x = k.checked_rem(across).unwrap_or(0).saturating_mul(cw);
        let top = k.checked_div(across).unwrap_or(0).saturating_mul(ch);
        let lines = pixels.chunks_mut(stride.max(1)).skip(top).take(ch);
        for (y, line) in (0..).zip(lines) {
            if let Some(out) = line.get_mut(x..x.saturating_add(cw)) {
                s.mask_line(0, y, out);
            }
        }
    }
    if let Err(e) = gl.fill_texture(&texture, grid.rect(), &pixels, stride) {
        // the error that matters is the upload's
        let _ = gl.free_texture(texture);
        return Err(e);
    }
    Ok((texture, sprite_columns, grid))
}

impl Glyphs {
    /// A slot among the boxes no glyph holds: the lowest free one, the
    /// next one, or the next one of a texture with twice the rows, which
    /// takes the old texture's place filled with every glyph the boxes
    /// keep. For a `color` glyph the color boxes must hold the slot too:
    /// they are made at the boxes' size first, and they grow with the
    /// boxes. `None` when the texture is at the largest side, or the
    /// textures a slot needs would take the textures past their ceiling.
    /// A texture that cannot be made leaves the boxes as they were.
    pub(super) fn take_box<G: Gl>(
        &mut self,
        gl: &mut G,
        color: bool,
    ) -> Result<Option<u32>, Error> {
        if color && self.boxes.color.is_none() && self.boxes.rows > 0 {
            let size = self.boxes.size(self.boxes.rows);
            if !self.make_color_boxes(gl, size)? {
                return Ok(None);
            }
        }
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
        // the color boxes grow with the boxes, slot for slot, or are made
        // with them for a color glyph's first box
        let with_color = color || b.color.is_some();
        let grow =
            |format: Format| texture_bytes(format, new).saturating_sub(texture_bytes(format, old));
        let grown = grow(Format::R8).saturating_add(if with_color {
            match b.color {
                Some(_) => grow(Format::Rgba8),
                None => texture_bytes(Format::Rgba8, new),
            }
        } else {
            0
        });
        if self.bytes.saturating_add(grown) > self.ceiling {
            return Ok(None);
        }
        // both textures are made and filled before either takes its place
        let texture = gl.create_texture(Format::R8, new)?;
        let colors = match with_color {
            true => match gl.create_texture(Format::Rgba8, new) {
                Ok(t) => Some(t),
                Err(e) => {
                    let _ = gl.free_texture(texture);
                    return Err(e);
                }
            },
            false => None,
        };
        self.uploads = self
            .uploads
            .saturating_add(1)
            .saturating_add(usize::from(colors.is_some()));
        let filled = (|| -> Result<(), Error> {
            if b.kept.iter().any(Option::is_some) {
                let stride = usize::try_from(new.width).unwrap_or(0);
                let mut pixels = vec![0u8; usize::try_from(new.rect().area()).unwrap_or(0)];
                for (i, kept) in (0u32..).zip(&b.kept) {
                    if let Some((stored, _)) = kept {
                        b.draw(stored, &mut pixels, b.at(i), stride);
                    }
                }
                gl.fill_texture(&texture, new.rect(), &pixels, stride)?;
            }
            if let Some(c) = &colors {
                let stride = usize::try_from(new.width).unwrap_or(0).saturating_mul(4);
                gl.fill_texture(c, new.rect(), &b.color_pixels(new), stride)?;
            }
            Ok(())
        })();
        if let Err(e) = filled {
            let _ = gl.free_texture(texture);
            if let Some(c) = colors {
                let _ = gl.free_texture(c);
            }
            return Err(e);
        }
        self.uploads = self.uploads.saturating_add(2);
        self.bytes = self.bytes.saturating_add(grown);
        let mut put = |at: Option<usize>, t: Texture| -> (usize, Option<Texture>) {
            match at {
                Some(at) => (at, self.textures.get_mut(at).and_then(|s| s.replace(t))),
                None => {
                    let at = match self.textures.iter().position(Option::is_none) {
                        Some(at) => at,
                        None => {
                            self.textures.push(None);
                            self.textures.len().saturating_sub(1)
                        }
                    };
                    if let Some(s) = self.textures.get_mut(at) {
                        *s = Some(t);
                    }
                    (at, None)
                }
            }
        };
        let (at, old_r8) = put(b.texture, texture);
        b.texture = Some(at);
        let old_color = match colors {
            Some(c) => {
                let (at, old) = put(b.color, c);
                b.color = Some(at);
                old
            }
            None => None,
        };
        for old in [old_r8, old_color].into_iter().flatten() {
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

    /// Makes the color boxes' texture at `size`, the boxes' own, filled
    /// with the color glyphs the boxes keep; false, making nothing, when it
    /// would take the textures past their ceiling.
    fn make_color_boxes<G: Gl>(&mut self, gl: &mut G, size: Size) -> Result<bool, Error> {
        let bytes = texture_bytes(Format::Rgba8, size);
        if self.bytes.saturating_add(bytes) > self.ceiling {
            return Ok(false);
        }
        let texture = gl.create_texture(Format::Rgba8, size)?;
        let stride = usize::try_from(size.width).unwrap_or(0).saturating_mul(4);
        if let Err(e) = gl.fill_texture(
            &texture,
            size.rect(),
            &self.boxes.color_pixels(size),
            stride,
        ) {
            let _ = gl.free_texture(texture);
            return Err(e);
        }
        self.uploads = self.uploads.saturating_add(2);
        self.bytes = self.bytes.saturating_add(bytes);
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
        self.boxes.color = Some(at);
        Ok(true)
    }

    /// Fills box `index` from `stored`'s coverage across the whole slot,
    /// and gives where the box holds ink.
    pub(super) fn fill_box<G: Gl>(
        &mut self,
        gl: &mut G,
        index: u32,
        stored: &Stored,
    ) -> Result<BoxLines, Error> {
        self.uploads = self.uploads.saturating_add(1);
        self.besides.set((BOXES, index), stored.beside());
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
        gl.fill_texture(texture, rect, &pixels, stride)?;
        let lines = BoxLines::new(&pixels, b.slot, self.cell);
        if stored.is_rgba() {
            self.fill_color_box(gl, rect, stored)?;
        }
        Ok(lines)
    }

    /// Fills the color boxes' slot at `rect` from the color glyph
    /// `stored`; [`Glyphs::take_box`] has made the color boxes.
    fn fill_color_box<G: Gl>(
        &mut self,
        gl: &mut G,
        rect: Rect,
        stored: &Stored,
    ) -> Result<(), Error> {
        let b = &self.boxes;
        let texture = b
            .color
            .and_then(|i| self.textures.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no texture of color boxes"))?;
        let stride = usize::try_from(b.slot.width).unwrap_or(0).saturating_mul(4);
        let mut pixels =
            vec![0u8; stride.saturating_mul(usize::try_from(b.slot.height).unwrap_or(0))];
        b.draw_rgba(stored, &mut pixels, (0, 0), stride);
        self.uploads = self.uploads.saturating_add(1);
        gl.fill_texture(texture, rect, &pixels, stride)
    }
}
