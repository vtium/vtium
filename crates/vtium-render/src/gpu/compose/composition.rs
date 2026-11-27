//! The upright frame and what composes into it: [`Composition`] holds the
//! frame's tiles, the programs it draws with, the arrow's texture and the
//! copy of the pixels beneath a draw, and draws each frame's work into
//! every tile it meets.

use crate::arrow::Arrow;
use crate::bounds::GPU_IDLE_FRAMES;
use crate::compose::Area;
use crate::gpu::gl::{
    Batch, Blend, Draw, DrawTarget, Error, ErrorKind, Filter, Format, Gl, Program, Quad, Size,
    Source, Texture, Value,
};
use crate::gpu::glyphs::{Glyphs, Spec, rgba};
use crate::gpu::tile::{Tile, meet, tiles};
use crate::notice::Placed;
use crate::reach::Bound;
use crate::target::Rect;

use super::{
    ARROW_FRAG, ARROW_INPUTS, ArrowAt, CELL_INPUTS, CELL_VERT, Cells, INK_INPUTS, QUAD_VERT,
    UNDER_FRAG, UNDER_INPUTS, Work, cell_frag,
};
#[cfg(doc)]
use super::{BENEATH, INK};
#[cfg(feature = "images")]
use super::{ES3, LAYER_INPUTS, OVER, PICTURE_FRAG, PICTURE_INPUTS, PICTURE_OVER_INPUTS};

/// The upright frame and what composes into it.
#[derive(Debug)]
pub(crate) struct Composition {
    size: Size,
    /// How many pixels neighbouring tiles share.
    overlap: u32,
    tiles: Vec<(Tile, DrawTarget)>,
    programs: Programs,
    /// The bound on a glyph's reach the cell shaders were built with.
    bound: Bound,
    /// The arrow last uploaded and its texture.
    arrow_image: Option<(Arrow, Texture)>,
    glyphs: Option<Glyphs>,
    /// The border's color it last cleared to.
    border: Option<u32>,
    /// Whether the next frame draws the whole upright frame.
    whole: bool,
    /// The notices the last frame drew.
    overlays: Vec<Placed>,
    /// Where a draw over the pixels beneath it copies them: a target kept
    /// from frame to frame, as large in each side as the most pixels a
    /// frame has drawn over in one tile since it was made.
    copy: Option<DrawTarget>,
    /// For each tile, the rectangle of the upright frame the copy's first
    /// texel holds this frame: the smallest one holding every pixel the
    /// frame draws over in that tile, or `None` where it draws over none.
    copy_at: Vec<Option<Rect>>,
    /// Frames in a row that read no pixel from the copy, which give it back
    /// at `GPU_IDLE_FRAMES`.
    copy_idle: u64,
}

/// What a draw over the pixels beneath it needs: the under shader, which
/// copies them out of a tile, the copy, and for each tile the rectangle of
/// the upright frame the copy's first texel holds.
pub(crate) struct Beneath<'a> {
    program: &'a Program,
    copy: &'a DrawTarget,
    at: &'a [Option<Rect>],
}

impl Beneath<'_> {
    /// Copies the pixels under `quads`, tile-local quads of the `k`-th
    /// tile, out of the tile's `target` into the copy, and gives the copy
    /// for a shader that goes over them to read them from. The quads of one
    /// draw over the pixels beneath must not overlap, since each reads what
    /// lay beneath before the draw.
    pub(crate) fn copy<G: Gl>(
        &self,
        gl: &mut G,
        k: usize,
        tile: &Tile,
        target: &DrawTarget,
        quads: &[Quad],
    ) -> Result<Copied<'_>, Error> {
        let b = self
            .at
            .get(k)
            .copied()
            .flatten()
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "a tile has no copy made ready"))?;
        let frame = target
            .texture()
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "a tile has no texture"))?;
        let copied = self
            .copy
            .texture()
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "the copy has no texture"))?;
        // the tile-local pixel the copy's first texel holds
        let at = local(b, tile);
        let whole = Size::new(tile.held.width, tile.held.height).rect();
        let out: Vec<Quad> = quads
            .iter()
            .filter_map(|q| dst_rect(q.dst).and_then(|r| meet(r, whole)))
            .map(|r| Quad {
                dst: [
                    side(r.x.saturating_sub(at.x)),
                    side(r.y.saturating_sub(at.y)),
                    side(r.width),
                    side(r.height),
                ],
                src: [r.x, r.y, 0, 0],
                params: [0; 4],
            })
            .collect();
        gl.draw(&Draw {
            target: self.copy,
            program: self.program,
            inputs: &[
                ("target_size", target_size(self.copy)),
                ("under", Value::Texture(frame, Filter::Nearest)),
            ],
            quads: &out,
            blend: Blend::Replace,
            scissor: None,
        })?;
        Ok(Copied {
            texture: copied,
            at: (at.x, at.y),
        })
    }
}

/// The pixels beneath a draw, copied: the copy's texture, and the
/// tile-local pixel its first texel holds.
pub(crate) struct Copied<'a> {
    texture: &'a Texture,
    at: (u32, u32),
}

impl Copied<'_> {
    /// The inputs a shader that goes over the pixels beneath reads the copy
    /// through: [`BENEATH`].
    pub(crate) fn inputs(&self) -> [(&'static str, Value<'_>); 2] {
        [
            ("under", Value::Texture(self.texture, Filter::Nearest)),
            (
                "under_at",
                Value::IVec4([side(self.at.0), side(self.at.1), 0, 0]),
            ),
        ]
    }
}

/// The tiles of an upright frame of `size`, overlapping by `overlap`, each
/// with its target. A call that fails frees what it made.
fn make_tiles<G: Gl>(
    gl: &mut G,
    size: Size,
    overlap: u32,
) -> Result<Vec<(Tile, DrawTarget)>, Error> {
    let limits = gl.limits();
    let largest = Size::new(
        limits.texture.min(limits.viewport.width),
        limits.texture.min(limits.viewport.height),
    );
    let layout = tiles(size, largest, overlap).ok_or_else(|| {
        Error::new(
            ErrorKind::Limit,
            format!(
                "a {}x{} frame cannot be tiled within {}x{}",
                size.width, size.height, largest.width, largest.height
            ),
        )
    })?;
    let mut made: Vec<(Tile, DrawTarget)> = Vec::with_capacity(layout.len());
    for tile in layout {
        let held = tile.held;
        match gl.create_frame(Size::new(held.width, held.height)) {
            Ok(t) => made.push((tile, t)),
            Err(e) => {
                free_tiles(gl, made);
                return Err(e);
            }
        }
    }
    Ok(made)
}

/// Frees `tiles`, after an error that matters more than theirs.
fn free_tiles<G: Gl>(gl: &mut G, tiles: Vec<(Tile, DrawTarget)>) {
    for (_, t) in tiles {
        let _ = gl.free_target(t);
    }
}

/// Every program the composition draws with, compiled when the frame is
/// set up, so that no frame waits for a compile: not the frame in which the
/// first glyph whose ink passes its cells takes its box, nor the first
/// frame with the pointer or with a picture.
#[derive(Debug)]
struct Programs {
    /// The cell shader, for the cells that receive no ink.
    cell: Program,
    /// The cell shader built with [`INK`], for the cells that receive ink.
    ink: Program,
    arrow: Program,
    /// The under shader, which copies the pixels beneath a draw out of a
    /// tile into the copy, and tints the selection.
    under: Program,
    /// The programs of the cells that pictures reach: the picture shader
    /// for an opaque picture and, built with [`OVER`], for one that is
    /// not; and the cell shader built with [`OVER`] and [`INK`], for the
    /// text layers and marks.
    #[cfg(feature = "images")]
    picture: Program,
    #[cfg(feature = "images")]
    picture_over: Program,
    #[cfg(feature = "images")]
    layer: Program,
}

impl Programs {
    /// The programs, the cell shaders' loops bounded by `bound`. A call
    /// that fails frees what it made.
    fn new<G: Gl>(gl: &mut G, bound: Bound) -> Result<Programs, Error> {
        let (cell, ink) = (
            cell_frag(false, false, bound),
            cell_frag(false, true, bound),
        );
        #[cfg(feature = "images")]
        let layer = cell_frag(true, true, bound);
        let sources = [
            (&[CELL_VERT], &cell[..], &CELL_INPUTS[..]),
            (&[CELL_VERT], &ink[..], &INK_INPUTS[..]),
            (&[QUAD_VERT], &[ARROW_FRAG][..], &ARROW_INPUTS[..]),
            (&[CELL_VERT], &[UNDER_FRAG][..], &UNDER_INPUTS[..]),
            #[cfg(feature = "images")]
            (&[CELL_VERT], &[ES3, PICTURE_FRAG][..], &PICTURE_INPUTS[..]),
            #[cfg(feature = "images")]
            (
                &[CELL_VERT],
                &[ES3, OVER, PICTURE_FRAG][..],
                &PICTURE_OVER_INPUTS[..],
            ),
            #[cfg(feature = "images")]
            (&[CELL_VERT], &layer[..], &LAYER_INPUTS[..]),
        ];
        let mut made = Vec::with_capacity(sources.len());
        for (vertex, fragment, inputs) in sources {
            match gl.compile(&Source {
                vertex,
                fragment,
                inputs,
            }) {
                Ok(p) => made.push(p),
                Err(e) => {
                    for p in made {
                        let _ = gl.free_program(p);
                    }
                    return Err(e);
                }
            }
        }
        let mut made = made.into_iter();
        let mut next = || {
            made.next()
                .ok_or_else(|| Error::new(ErrorKind::Misuse, "a program was not made"))
        };
        Ok(Programs {
            cell: next()?,
            ink: next()?,
            arrow: next()?,
            under: next()?,
            #[cfg(feature = "images")]
            picture: next()?,
            #[cfg(feature = "images")]
            picture_over: next()?,
            #[cfg(feature = "images")]
            layer: next()?,
        })
    }

    /// Frees the programs, giving the first error.
    fn free<G: Gl>(self, gl: &mut G) -> Result<(), Error> {
        let all = [
            self.cell,
            self.ink,
            self.arrow,
            self.under,
            #[cfg(feature = "images")]
            self.picture,
            #[cfg(feature = "images")]
            self.picture_over,
            #[cfg(feature = "images")]
            self.layer,
        ];
        let mut out = Ok(());
        for p in all {
            out = out.and(gl.free_program(p));
        }
        out
    }
}

impl Composition {
    /// The upright frame of `size`, in tiles overlapping by `overlap`, and
    /// the composition's programs. A call that fails frees what it made.
    pub(crate) fn new<G: Gl>(gl: &mut G, size: Size, overlap: u32) -> Result<Composition, Error> {
        let made = make_tiles(gl, size, overlap)?;
        let bound = Bound::DRAWN;
        let programs = match Programs::new(gl, bound) {
            Ok(p) => p,
            Err(e) => {
                free_tiles(gl, made);
                return Err(e);
            }
        };
        Ok(Composition {
            size,
            overlap,
            tiles: made,
            programs,
            bound,
            arrow_image: None,
            glyphs: None,
            border: None,
            whole: true,
            overlays: Vec::new(),
            copy: None,
            copy_at: Vec::new(),
            copy_idle: 0,
        })
    }

    /// The upright frame's size.
    pub(crate) fn size(&self) -> Size {
        self.size
    }

    /// Makes the upright frame anew at `size`, which a quarter turn of a
    /// display that is not square needs, and draws it whole next. Only the
    /// frame's tiles change: the glyphs, the arrow and the programs stay.
    /// The new tiles are one batch of allocations, checked before the old
    /// ones go: a call whose allocation or check fails frees what it made
    /// and leaves the frame as it was. Once the new tiles stand, an error
    /// freeing the old ones is given back with the frame at `size`.
    pub(crate) fn reshape<G: Gl>(&mut self, gl: &mut G, size: Size) -> Result<(), Error> {
        let made = make_tiles(gl, size, self.overlap)?;
        if let Err(e) = gl.check(Batch::Uploads) {
            free_tiles(gl, made);
            return Err(e);
        }
        let old = core::mem::replace(&mut self.tiles, made);
        self.size = size;
        self.redraw_whole();
        let mut out = Ok(());
        for (_, t) in old {
            out = out.and(gl.free_target(t));
        }
        out
    }

    /// The tiles of the upright frame, each with its texture.
    pub(crate) fn tiles(&self) -> impl Iterator<Item = (&Tile, Option<&Texture>)> {
        self.tiles.iter().map(|(tile, t)| (tile, t.texture()))
    }

    /// Whether the next frame must draw the whole upright frame: it is new,
    /// the last frame failed, or the border's color changes to `border`.
    pub(crate) fn needs_whole(&self, border: u32) -> bool {
        self.whole || self.border != Some(border)
    }

    /// Makes the next frame draw the whole upright frame.
    pub(crate) fn redraw_whole(&mut self) {
        self.whole = true;
        self.overlays.clear();
    }

    /// The areas of the last frame's notices that `overlays` no longer
    /// draws as they were: the cells beneath them draw again.
    pub(crate) fn overlays_gone(&self, overlays: &[Placed]) -> Vec<Rect> {
        self.overlays
            .iter()
            .filter(|o| !overlays.contains(o))
            .map(Placed::rect)
            .collect()
    }

    /// Makes the glyphs of the renderer `spec` names ready, made anew, the
    /// old ones freed, when they belong to another renderer: a new font or
    /// cell needs a new renderer, and the frame draws whole. Glyphs kept
    /// from the last frame give back the textures they no longer fill.
    pub(crate) fn glyphs<G: Gl>(&mut self, gl: &mut G, spec: &Spec<'_>) -> Result<(), Error> {
        if spec.bound != self.bound {
            // a renderer at another bound, as a test makes: the cell
            // shaders' loops follow it
            let programs = Programs::new(gl, spec.bound)?;
            core::mem::replace(&mut self.programs, programs).free(gl)?;
            self.bound = spec.bound;
        }
        if self.glyphs.as_ref().map(Glyphs::owner) != Some(spec.owner) {
            if let Some(old) = self.glyphs.take() {
                old.free(gl)?;
            }
            self.whole = true;
        }
        match &mut self.glyphs {
            Some(g) => g.give_back(gl)?,
            None => self.glyphs = Some(Glyphs::new(gl, spec)?),
        }
        Ok(())
    }

    /// The glyphs' notes for the log since the last call.
    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        self.glyphs
            .as_mut()
            .map(Glyphs::take_notes)
            .unwrap_or_default()
    }

    /// Bytes the glyphs' textures hold, while the frame has glyphs.
    pub(crate) fn glyph_bytes(&self) -> Option<u64> {
        self.glyphs.as_ref().map(Glyphs::bytes)
    }

    /// The glyphs [`Composition::glyphs`] made ready.
    pub(crate) fn held_glyphs(&mut self) -> Result<&mut Glyphs, Error> {
        self.glyphs
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "no glyphs made ready"))
    }

    /// The arrow's texture, uploaded when its image changed. Gives how many
    /// uploads that took.
    fn prepare_arrow<G: Gl>(
        &mut self,
        gl: &mut G,
        arrow: Option<ArrowAt<'_>>,
    ) -> Result<usize, Error> {
        let Some(a) = arrow else { return Ok(0) };
        if self
            .arrow_image
            .as_ref()
            .is_some_and(|(img, _)| img == a.image)
        {
            return Ok(0);
        }
        if let Some((_, t)) = self.arrow_image.take() {
            gl.free_texture(t)?;
        }
        let size = Size::new(a.image.width(), a.image.height());
        let texture = gl.create_texture(Format::Rgba8, size)?;
        let bytes: Vec<u8> = a.image.pixels().iter().flat_map(|&p| rgba(p)).collect();
        let stride = usize::try_from(size.width).unwrap_or(0).saturating_mul(4);
        if let Err(e) = gl.fill_texture(&texture, size.rect(), &bytes, stride) {
            let _ = gl.free_texture(texture);
            return Err(e);
        }
        self.arrow_image = Some((a.image.clone(), texture));
        Ok(3)
    }

    /// Makes what the frame reads ready: the arrow's texture, and for a
    /// frame that draws over pixels already drawn a copy large enough.
    /// Gives how many uploads it made.
    pub(crate) fn prepare<G: Gl>(&mut self, gl: &mut G, work: &Work<'_>) -> Result<usize, Error> {
        let calls = self.prepare_arrow(gl, work.arrow)?;
        // every pixel the frame draws over: the arrow's, and those of the
        // cells pictures reach
        let arrow = match work.arrow {
            Some(a) => arrow_quads(a, &self.drawn(work), self.size),
            None => Vec::new(),
        };
        let over: Vec<Rect> = arrow
            .iter()
            .filter_map(|q| dst_rect(q.dst))
            .chain(pictured_over(work))
            .collect();
        Ok(calls.saturating_add(self.prepare_copy(gl, &over)?))
    }

    /// Makes the copy ready for a frame that draws over the pixels of
    /// `over`. The copy holds, for each tile, the
    /// smallest rectangle holding every pixel of `over` there, so it is as
    /// large as the largest of those; a copy that is smaller in either side
    /// is made anew at the larger of each. Gives how many uploads it made.
    fn prepare_copy<G: Gl>(&mut self, gl: &mut G, over: &[Rect]) -> Result<usize, Error> {
        let mut calls = 0usize;
        self.copy_at = self
            .tiles
            .iter()
            .map(|(tile, _)| {
                over.iter()
                    .filter_map(|&r| meet(r, tile.held))
                    .reduce(bounds)
            })
            .collect();
        let mut need = Size::new(0, 0);
        for b in self.copy_at.iter().flatten() {
            need = Size::new(need.width.max(b.width), need.height.max(b.height));
        }
        if need.width == 0 || need.height == 0 {
            return Ok(calls);
        }
        let held = self.copy.as_ref().map_or(Size::new(0, 0), DrawTarget::size);
        if need.width > held.width || need.height > held.height {
            let size = Size::new(need.width.max(held.width), need.height.max(held.height));
            if let Some(old) = self.copy.take() {
                gl.free_target(old)?;
            }
            self.copy = Some(gl.create_frame(size)?);
            calls = calls.saturating_add(2);
        }
        Ok(calls)
    }

    /// What a draw over the pixels beneath it needs this frame.
    fn beneath(&self) -> Result<Beneath<'_>, Error> {
        match &self.copy {
            Some(copy) => Ok(Beneath {
                program: &self.programs.under,
                copy,
                at: &self.copy_at,
            }),
            None => Err(Error::new(
                ErrorKind::Misuse,
                "the copy of the pixels beneath is not made ready",
            )),
        }
    }

    /// The rectangles of the upright frame `work` draws: the whole frame
    /// when the composition draws it whole, or the border's parts it clears
    /// and the cells it draws.
    fn drawn(&self, work: &Work<'_>) -> Vec<Rect> {
        let frame = self.size.rect();
        if self.needs_whole(work.border) {
            return vec![frame];
        }
        let cells = work
            .cells
            .iter()
            .flat_map(|c| &c.quads)
            .map(|q| dst_rect(q.dst));
        work.clear
            .iter()
            .copied()
            .chain(cells.flatten())
            .filter_map(|r| meet(r, frame))
            .collect()
    }

    /// Draws `work` into the upright frame, and gives the rectangles that
    /// changed: the whole frame when it is drawn whole.
    pub(crate) fn draw<G: Gl>(
        &mut self,
        gl: &mut G,
        work: &Work<'_>,
        #[cfg(feature = "images")] pictures: &super::pictures::Pictures,
    ) -> Result<Vec<Rect>, Error> {
        let frame = self.size.rect();
        let whole = self.needs_whole(work.border);
        let drawn = self.drawn(work);
        self.border = Some(work.border);
        let whole_rect = [frame];
        let clear = if whole { &whole_rect[..] } else { work.clear };
        for (tile, target) in &self.tiles {
            for &r in clear {
                if let Some(m) = meet(r, tile.held) {
                    gl.clear(target, local(m, tile), work.border)?;
                }
            }
        }
        self.draw_cells(gl, work.cells)?;
        #[cfg(feature = "images")]
        self.draw_pictured(gl, work, pictures)?;
        // a bar cursor's bar lies over its cell's letter and over every
        // picture, text layer and mark in the cell
        self.draw_cells(gl, work.bars)?;
        if let Some(a) = work.arrow {
            self.draw_arrow(gl, a, &drawn)?;
        }
        let uploaded = self.upload_overlays(gl, work.overlays, &drawn)?;
        self.overlays = work.overlays.to_vec();
        self.whole = false;
        if whole {
            return Ok(vec![frame]);
        }
        // a notice uploaded where the frame drew nothing changed too
        let mut changed: Vec<Rect> = work
            .changed
            .iter()
            .filter_map(|&r| meet(r, frame))
            .collect();
        for u in uploaded {
            if !changed.iter().any(|&c| meet(c, u) == Some(u)) {
                changed.push(u);
            }
        }
        Ok(changed)
    }

    /// Over the cells pictures reach, in ADR-1100 D4's layers: each
    /// picture in the frame's order, the text layers before the first
    /// picture above the text, the selection's tint, and vtium's marks.
    /// Each but an opaque picture goes over the pixels beneath it, which it
    /// reads from a copy of them.
    #[cfg(feature = "images")]
    fn draw_pictured<G: Gl>(
        &self,
        gl: &mut G,
        work: &Work<'_>,
        pictures: &super::pictures::Pictures,
    ) -> Result<(), Error> {
        let mut text = false;
        for (i, s) in work.pictures.iter().enumerate() {
            if s.rects.is_empty() {
                continue;
            }
            if s.above && !text {
                self.draw_layers(gl, work.text)?;
                text = true;
            }
            let p = &self.programs;
            if s.opaque {
                pictures.draw(gl, i, s, &self.tiles, &p.picture, None)?;
            } else {
                let beneath = self.beneath()?;
                pictures.draw(gl, i, s, &self.tiles, &p.picture_over, Some(&beneath))?;
            }
        }
        if !text {
            self.draw_layers(gl, work.text)?;
        }
        self.draw_tints(gl, work.tints)?;
        self.draw_layers(gl, work.marks)
    }

    /// The selection's tint over `tints`, in each tile: the cells copied
    /// out, then drawn back tinted from the copy.
    #[cfg(feature = "images")]
    fn draw_tints<G: Gl>(&self, gl: &mut G, tints: &[(Rect, u32)]) -> Result<(), Error> {
        if tints.is_empty() {
            return Ok(());
        }
        let beneath = self.beneath()?;
        let strength = u32::from(crate::picture::SELECTION_TINT);
        for (k, (tile, target)) in self.tiles.iter().enumerate() {
            let quads: Vec<Quad> = tints
                .iter()
                .filter_map(|&(r, c)| meet(r, tile.held).map(|m| (local(m, tile), c)))
                .map(|(r, c)| Quad {
                    dst: [side(r.x), side(r.y), side(r.width), side(r.height)],
                    src: [0; 4],
                    params: [c & 0x00ff_ffff | 1 << 24, strength, 0, 0],
                })
                .collect();
            if quads.is_empty() {
                continue;
            }
            let copied = beneath.copy(gl, k, tile, target, &quads)?;
            // each quad reads the copy from its own texel of it
            let (ax, ay) = copied.at;
            let back: Vec<Quad> = quads
                .iter()
                .map(|q| Quad {
                    src: [
                        q.dst[0].unsigned_abs().saturating_sub(ax),
                        q.dst[1].unsigned_abs().saturating_sub(ay),
                        0,
                        0,
                    ],
                    ..*q
                })
                .collect();
            gl.draw(&Draw {
                target,
                program: beneath.program,
                inputs: &[
                    ("target_size", target_size(target)),
                    ("under", Value::Texture(copied.texture, Filter::Nearest)),
                ],
                quads: &back,
                blend: Blend::Replace,
                scissor: None,
            })?;
        }
        Ok(())
    }

    /// Ends the frame: the copy counts one more frame in a row that read
    /// nothing from it, or starts counting again after one that did, and is
    /// given back at `GPU_IDLE_FRAMES` such frames, counted in frames and
    /// never wall time.
    pub(crate) fn end_frame<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        let read = self.copy_at.iter().any(Option::is_some);
        self.copy_at.clear();
        self.copy_idle = if read {
            0
        } else {
            self.copy_idle.saturating_add(1)
        };
        if self.copy_idle < GPU_IDLE_FRAMES {
            return Ok(());
        }
        match self.copy.take() {
            Some(t) => gl.free_target(t),
            None => Ok(()),
        }
    }

    /// Gives back the glyphs' textures, the arrow's, the copy and the
    /// upright frame's tiles, keeping the programs, and draws the whole
    /// frame next, its tiles made again first (ADR-1303 D9).
    pub(crate) fn release<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        let mut out = Ok(());
        for (_, t) in self.tiles.drain(..) {
            out = out.and(gl.free_target(t));
        }
        if let Some(g) = self.glyphs.take() {
            out = out.and(g.free(gl));
        }
        if let Some((_, t)) = self.arrow_image.take() {
            out = out.and(gl.free_texture(t));
        }
        if let Some(t) = self.copy.take() {
            out = out.and(gl.free_target(t));
        }
        self.copy_at.clear();
        // belt and braces: the next frame makes the tiles and the glyphs
        // anew, either of which already draws the frame whole
        self.redraw_whole();
        out
    }

    /// Makes the upright frame's tiles again at its size when a release
    /// gave them back, and draws it whole. Gives how many allocations that
    /// took, a call each of the frame's batch of uploads.
    ///
    /// Unlike [`Composition::reshape`], it does not check its own new tiles
    /// before it goes on: the frame's check of its uploads reads their
    /// errors. ADR-1303 D9 bounds what that costs: an out-of-memory error
    /// takes the head off the GPU path, any other error costs the frame,
    /// and errors in two frames running move the head to the CPU path.
    pub(crate) fn restore<G: Gl>(&mut self, gl: &mut G) -> Result<usize, Error> {
        if !self.tiles.is_empty() {
            return Ok(0);
        }
        self.tiles = make_tiles(gl, self.size, self.overlap)?;
        self.redraw_whole();
        Ok(self.tiles.len())
    }

    /// The textures a batch of cells samples, its masks, its colors, the
    /// sprites and the ink beside the pieces of magnified characters, and
    /// the cell's size with the sprites and those entries across their
    /// textures, as the cell shader's inputs; and for the cell shader
    /// built with [`INK`], as `ink` asks, the boxes, a box slot's size and the ink
    /// grid. A draw before any glyph has taken a box, or any cell has
    /// received ink, binds the sprites in place of the boxes and the ink
    /// grid, and never samples them.
    fn cell_inputs(
        &self,
        batch: &Cells,
        ink: bool,
    ) -> Result<Vec<(&'static str, Value<'_>)>, Error> {
        let glyphs = self
            .glyphs
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "cells drawn with no glyphs"))?;
        let texture = |i: usize| {
            glyphs
                .texture(i)
                .ok_or_else(|| Error::new(ErrorKind::Misuse, "no such texture of glyphs"))
        };
        let sprites = texture(glyphs.sprites())?;
        let (masks, colors) = match batch.glyphs {
            Some((t, false)) => (texture(t)?, sprites),
            Some((t, true)) => (sprites, texture(t)?),
            None => (sprites, sprites),
        };
        let cell = glyphs.cell();
        // no quad names an entry of the ink beside pieces before a piece
        // with ink beside it draws an underline
        let besides = glyphs.besides.texture().unwrap_or(sprites);
        let mut inputs = vec![
            ("masks", Value::Texture(masks, Filter::Nearest)),
            ("colors", Value::Texture(colors, Filter::Nearest)),
            ("sprites", Value::Texture(sprites, Filter::Nearest)),
            (
                "cell",
                Value::IVec4([
                    side(cell.width),
                    side(cell.height),
                    side(glyphs.sprite_columns()),
                    side(glyphs.besides.across()),
                ]),
            ),
            ("besides", Value::Texture(besides, Filter::Nearest)),
        ];
        if ink {
            let (boxes, color_boxes, [w, h, across]) = glyphs.boxes();
            let boxes = match boxes {
                Some(t) => texture(t)?,
                None => sprites,
            };
            // no cell reads the color boxes before a color glyph takes a box
            let color_boxes = match color_boxes {
                Some(t) => texture(t)?,
                None => sprites,
            };
            let grid = glyphs.ink.texture().unwrap_or(sprites);
            inputs.extend([
                ("boxes", Value::Texture(boxes, Filter::Nearest)),
                (
                    "box_slot",
                    Value::IVec4([side(w), side(h), side(across), 0]),
                ),
                ("ink", Value::Texture(grid, Filter::Nearest)),
                ("color_boxes", Value::Texture(color_boxes, Filter::Nearest)),
            ]);
        }
        Ok(inputs)
    }

    /// The cells' quads, a draw for each texture of glyphs, whether its
    /// cells receive ink, and tile.
    fn draw_cells<G: Gl>(&self, gl: &mut G, cells: &[Cells]) -> Result<(), Error> {
        for batch in cells {
            let p = &self.programs;
            let program = if batch.ink { &p.ink } else { &p.cell };
            let inputs = self.cell_inputs(batch, batch.ink)?;
            for (tile, target) in &self.tiles {
                let quads = in_tile(&batch.quads, tile);
                if quads.is_empty() {
                    continue;
                }
                let mut bound = vec![("target_size", target_size(target))];
                bound.extend(inputs.iter().copied());
                gl.draw(&Draw {
                    target,
                    program,
                    inputs: &bound,
                    quads: &quads,
                    blend: Blend::Replace,
                    scissor: None,
                })?;
            }
        }
        Ok(())
    }

    /// The text layers or marks of `cells`, a draw for each texture of
    /// glyphs and tile, over the pixels beneath them, which each draw reads
    /// from a copy of them.
    #[cfg(feature = "images")]
    fn draw_layers<G: Gl>(&self, gl: &mut G, cells: &[Cells]) -> Result<(), Error> {
        if cells.is_empty() {
            return Ok(());
        }
        let program = &self.programs.layer;
        let beneath = self.beneath()?;
        for batch in cells {
            let inputs = self.cell_inputs(batch, true)?;
            for (k, (tile, target)) in self.tiles.iter().enumerate() {
                let quads = in_tile(&batch.quads, tile);
                if quads.is_empty() {
                    continue;
                }
                let copied = beneath.copy(gl, k, tile, target, &quads)?;
                let mut bound = vec![("target_size", target_size(target))];
                bound.extend(inputs.iter().copied());
                bound.extend(copied.inputs());
                gl.draw(&Draw {
                    target,
                    program,
                    inputs: &bound,
                    quads: &quads,
                    blend: Blend::Replace,
                    scissor: None,
                })?;
            }
        }
        Ok(())
    }

    /// The arrow over the pixels of `drawn` it covers, as quads cut to
    /// them, so no pixel takes it twice in a frame: the upright frame keeps
    /// what earlier frames drew, which already holds the arrow. In each
    /// tile the pixels beneath the quads are copied out first, and the
    /// arrow's shader goes over them with the CPU path's `over_bg`.
    fn draw_arrow<G: Gl>(&self, gl: &mut G, a: ArrowAt<'_>, drawn: &[Rect]) -> Result<(), Error> {
        let Some((_, texture)) = &self.arrow_image else {
            return Err(Error::new(ErrorKind::Misuse, "the arrow is not uploaded"));
        };
        let quads = arrow_quads(a, drawn, self.size);
        if quads.is_empty() {
            return Ok(());
        }
        let beneath = self.beneath()?;
        for (k, (tile, target)) in self.tiles.iter().enumerate() {
            let quads = in_tile(&quads, tile);
            if quads.is_empty() {
                continue;
            }
            let copied = beneath.copy(gl, k, tile, target, &quads)?;
            let [under, under_at] = copied.inputs();
            gl.draw(&Draw {
                target,
                program: &self.programs.arrow,
                inputs: &[
                    ("target_size", target_size(target)),
                    ("arrow", Value::Texture(texture, Filter::Nearest)),
                    under,
                    under_at,
                ],
                quads: &quads,
                blend: Blend::Replace,
                scissor: None,
            })?;
        }
        Ok(())
    }

    /// Each notice uploaded over what the frame drew beneath it, or whole
    /// when it is new or changed since the last frame. Gives the parts it
    /// uploaded.
    fn upload_overlays<G: Gl>(
        &self,
        gl: &mut G,
        overlays: &[Placed],
        drawn: &[Rect],
    ) -> Result<Vec<Rect>, Error> {
        let mut uploaded = Vec::new();
        let under = drawn.iter().copied().reduce(bounds);
        for o in overlays {
            let area = o.rect();
            let part = if self.overlays.contains(o) {
                under.and_then(|u| meet(u, area))
            } else {
                Some(area)
            };
            let Some(part) = part.and_then(|p| meet(p, self.size.rect())) else {
                continue;
            };
            uploaded.push(part);
            for (tile, target) in &self.tiles {
                let Some(m) = meet(part, tile.held) else {
                    continue;
                };
                let Some(texture) = target.texture() else {
                    continue;
                };
                let (bytes, stride) = o.rgba(Rect {
                    x: m.x.saturating_sub(o.x),
                    y: m.y.saturating_sub(o.y),
                    width: m.width,
                    height: m.height,
                });
                gl.fill_texture(texture, local(m, tile), &bytes, stride)?;
            }
        }
        Ok(uploaded)
    }

    /// Frees what it holds in GL.
    pub(crate) fn free<G: Gl>(self, gl: &mut G) -> Result<(), Error> {
        let mut out = self.programs.free(gl);
        if let Some((_, t)) = self.arrow_image {
            out = out.and(gl.free_texture(t));
        }
        if let Some(g) = self.glyphs {
            out = out.and(g.free(gl));
        }
        if let Some(t) = self.copy {
            out = out.and(gl.free_target(t));
        }
        for (_, t) in self.tiles {
            out = out.and(gl.free_target(t));
        }
        out
    }
}

/// The pixels a frame draws over in the cells pictures reach: the pictures
/// that are not opaque, the text layers, the selection's tint and the
/// marks.
#[cfg(feature = "images")]
fn pictured_over(work: &Work<'_>) -> Vec<Rect> {
    let pictures = work
        .pictures
        .iter()
        .filter(|s| !s.opaque)
        .flat_map(|s| s.rects.iter().copied());
    let quads = work.text.iter().chain(work.marks).flat_map(|c| &c.quads);
    pictures
        .chain(quads.filter_map(|q| dst_rect(q.dst)))
        .chain(work.tints.iter().map(|&(r, _)| r))
        .collect()
}

/// A build without `images` draws no picture, so nothing over one.
#[cfg(not(feature = "images"))]
fn pictured_over(_: &Work<'_>) -> Vec<Rect> {
    Vec::new()
}

fn side(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

pub(crate) fn target_size(target: &DrawTarget) -> Value<'static> {
    let s = target.size();
    #[expect(
        clippy::cast_precision_loss,
        reason = "a target's side is at most the largest viewport, which a float holds exactly"
    )]
    Value::Vec2([s.width as f32, s.height as f32])
}

/// `r`, in the upright frame, in `tile`'s own pixels.
fn local(r: Rect, tile: &Tile) -> Rect {
    Rect {
        x: r.x.saturating_sub(tile.held.x),
        y: r.y.saturating_sub(tile.held.y),
        ..r
    }
}

/// The quads of `quads` that meet `tile`, moved into its own pixels.
fn in_tile(quads: &[Quad], tile: &Tile) -> Vec<Quad> {
    let (tx, ty) = (side(tile.held.x), side(tile.held.y));
    quads
        .iter()
        .filter(|q| dst_rect(q.dst).and_then(|r| meet(r, tile.held)).is_some())
        .map(|q| Quad {
            dst: [
                q.dst[0].saturating_sub(tx),
                q.dst[1].saturating_sub(ty),
                q.dst[2],
                q.dst[3],
            ],
            ..*q
        })
        .collect()
}

/// A quad's place, the part of it at or right of and below the origin.
pub(super) fn dst_rect([x, y, w, h]: [i32; 4]) -> Option<Rect> {
    let x1 = x.saturating_add(w);
    let y1 = y.saturating_add(h);
    let (x0, y0) = (x.max(0), y.max(0));
    (x1 > x0 && y1 > y0).then(|| Rect {
        x: x0.unsigned_abs(),
        y: y0.unsigned_abs(),
        width: x1.saturating_sub(x0).unsigned_abs(),
        height: y1.saturating_sub(y0).unsigned_abs(),
    })
}

/// The smallest rectangle holding both.
fn bounds(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = a.x.saturating_add(a.width).max(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .max(b.y.saturating_add(b.height));
    Rect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

/// The arrow's quads over the pixels of `drawn` it covers within a frame
/// of `size`: each line's runs, a line tall, the runs of lines alike
/// joined into one quad. The rectangles of `drawn` may overlap; each pixel
/// is covered once.
pub(crate) fn arrow_quads(a: ArrowAt<'_>, drawn: &[Rect], size: Size) -> Vec<Quad> {
    let (w, h) = (a.image.width(), a.image.height());
    let area = Area {
        x: a.x,
        y: a.y,
        w: i64::from(w),
        h: i64::from(h),
    };
    let to_u32 = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
    let x0 = to_u32(area.x);
    let y0 = to_u32(area.y);
    let x1 = to_u32(area.right()).min(size.width);
    let y1 = to_u32(area.bottom()).min(size.height);
    let mut quads: Vec<Quad> = Vec::new();
    let mut last: Vec<(u32, u32)> = Vec::new();
    // the quads that began on the line before, which a line alike extends
    let mut open: Vec<usize> = Vec::new();
    let mut covered = vec![false; usize::try_from(x1.saturating_sub(x0)).unwrap_or(0)];
    for y in y0..y1 {
        covered.fill(false);
        for r in drawn {
            if y < r.y || y >= r.y.saturating_add(r.height) {
                continue;
            }
            let from = r.x.max(x0);
            let to = r.x.saturating_add(r.width).min(x1);
            for x in from..to {
                if let Some(c) =
                    covered.get_mut(usize::try_from(x.saturating_sub(x0)).unwrap_or(usize::MAX))
                {
                    *c = true;
                }
            }
        }
        let mut runs = Vec::new();
        let mut start = None;
        for (x, &c) in (x0..).zip(covered.iter().chain([&false])) {
            match (c, start) {
                (true, None) => start = Some(x),
                (false, Some(s)) => {
                    runs.push((s, x));
                    start = None;
                }
                _ => {}
            }
        }
        if !runs.is_empty() && runs == last {
            for &q in &open {
                if let Some(q) = quads.get_mut(q) {
                    q.dst[3] = q.dst[3].saturating_add(1);
                    q.src[3] = q.src[3].saturating_add(1);
                }
            }
            continue;
        }
        open.clear();
        for &(s, e) in &runs {
            open.push(quads.len());
            let sx = i64::from(s).saturating_sub(area.x);
            let sy = i64::from(y).saturating_sub(area.y);
            quads.push(Quad {
                dst: [side(s), side(y), side(e.saturating_sub(s)), 1],
                src: [to_u32(sx), to_u32(sy), e.saturating_sub(s), 1],
                params: [0; 4],
            });
        }
        last = runs;
    }
    quads
}
