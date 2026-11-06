//! Slot 2, composition into the upright frame (ADR-1303 D3).
//!
//! The upright frame is the whole display, held in tiles that each fit the
//! GPU's largest texture and viewport, one in the common case. Each frame
//! the composition draws what changed into every tile it meets, with the
//! tile's offset taken off: the border's parts the frame redraws, cleared
//! to the border's color; each cell the frame redraws, as one quad through
//! the cell shader, which does ADR-1300 D6's arithmetic; the pointer's
//! arrow over every pixel the frame drew; and last the console's notices,
//! uploaded over what the frame drew beneath them.
//!
//! A cell that pictures reach is drawn in the layers of ADR-1100 D4, as the
//! CPU path composes it: its background as a cell with nothing on it; then
//! each picture over the cells it reaches, through slot 1's textures, in
//! the frame's order, which is the layers' order; the cells' text layers,
//! premultiplied over the pictures beneath, before the first picture above
//! the text; the selection's tint; and last vtium's mark, the bar of a bar
//! cursor or an underline cursor's line.
//!
//! No draw reads the target it draws into, and the GPU's own blending
//! turns its float result into a byte as the driver chooses. So each draw
//! that goes over pixels already drawn, the arrow and, in a cell pictures
//! reach, a picture that is not opaque, a text layer, vtium's mark and the
//! selection's tint, first copies the pixels beneath it out of the tile
//! into the copy, a target that lasts from frame to frame, and then reads
//! them from there and blends with the CPU path's arithmetic. The copy
//! grows when a frame draws over more than it holds, and is given back
//! once no frame has read it for `GPU_IDLE_FRAMES` frames.
//!
//! The renderer decides what changed, as it does on the CPU path: the
//! cells, the cursor, the border and the pointer, through the same front
//! end, and hands each row's cell jobs to [`GpuBack`], which turns them
//! into quads and the border into clears.

use vtium_font::{Face, Font};
use vtium_term::Rgb;

use super::gl::{
    Batch, Blend, Draw, DrawTarget, Error, ErrorKind, Filter, Format, Gl, Input, InputKind,
    Program, Quad, Size, Source, Texture, Value,
};
use super::glyphs::{Glyphs, Place, Spec, rgba};
#[cfg(feature = "images")]
use super::pictures::Shown;
use super::tile::{Tile, meet, tiles};
use super::turn::Turn;
use crate::arrow::Arrow;
use crate::blend::pixel;
use crate::bounds::GPU_IDLE_FRAMES;
use crate::compose::{Area, Composer, Job};
use crate::deco::Kind;
use crate::notice::Placed;
use crate::renderer::Back;
use crate::target::Rect;

const QUAD_VERT: &str = include_str!("glsl/quad.vert");
pub(crate) const CELL_VERT: &str = include_str!("glsl/cell.vert");
/// The first piece of the cell and picture shaders, which leave it out so
/// that [`OVER`] can follow it: GLSL takes nothing before `#version`.
const ES3: &str = "#version 300 es\n";
/// The piece that makes the cell or picture shader draw over the pixels
/// beneath it, which it reads from a copy of them, with the CPU path's
/// over_px in whole numbers: a cell's text layer or vtium's mark, or a
/// picture that is not opaque.
#[cfg(feature = "images")]
const OVER: &str = "#define OVER\n";
const CELL_FRAG: &str = include_str!("glsl/cell.frag");
const ARROW_FRAG: &str = include_str!("glsl/arrow.frag");
#[cfg(feature = "images")]
const PICTURE_FRAG: &str = include_str!("glsl/picture.frag");
const UNDER_FRAG: &str = include_str!("glsl/under.frag");

pub(crate) const TARGET_SIZE: Input = Input {
    name: "target_size",
    kind: InputKind::Vec2,
};

const CELL_INPUTS: [Input; 5] = [
    TARGET_SIZE,
    Input {
        name: "masks",
        kind: InputKind::Texture,
    },
    Input {
        name: "colors",
        kind: InputKind::Texture,
    },
    Input {
        name: "sprites",
        kind: InputKind::Texture,
    },
    Input {
        name: "cell",
        kind: InputKind::IVec4,
    },
];

/// What a shader that goes over the pixels beneath it takes besides its
/// own inputs: the copy of those pixels, and the pixel of the target its
/// first texel holds. The arrow's shader always takes them, the cell and
/// picture shaders when built with [`OVER`].
const BENEATH: [Input; 2] = [
    Input {
        name: "under",
        kind: InputKind::Texture,
    },
    Input {
        name: "under_at",
        kind: InputKind::IVec4,
    },
];

#[cfg(feature = "images")]
const LAYER_INPUTS: [Input; 7] = [
    CELL_INPUTS[0],
    CELL_INPUTS[1],
    CELL_INPUTS[2],
    CELL_INPUTS[3],
    CELL_INPUTS[4],
    BENEATH[0],
    BENEATH[1],
];

const ARROW_INPUTS: [Input; 4] = [
    TARGET_SIZE,
    Input {
        name: "arrow",
        kind: InputKind::Texture,
    },
    BENEATH[0],
    BENEATH[1],
];

#[cfg(feature = "images")]
const PICTURE_INPUTS: [Input; 4] = [
    TARGET_SIZE,
    Input {
        name: "picture",
        kind: InputKind::Texture,
    },
    Input {
        name: "scale",
        kind: InputKind::UVec4,
    },
    Input {
        name: "origin",
        kind: InputKind::IVec4,
    },
];

#[cfg(feature = "images")]
const PICTURE_OVER_INPUTS: [Input; 6] = [
    PICTURE_INPUTS[0],
    PICTURE_INPUTS[1],
    PICTURE_INPUTS[2],
    PICTURE_INPUTS[3],
    BENEATH[0],
    BENEATH[1],
];

const UNDER_INPUTS: [Input; 2] = [
    TARGET_SIZE,
    Input {
        name: "under",
        kind: InputKind::Texture,
    },
];

/// The cells of a frame that sample one texture of glyphs: its index among
/// the glyphs' textures and whether it holds color glyphs, or `None` for
/// cells with no glyph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cells {
    pub(crate) glyphs: Option<(usize, bool)>,
    pub(crate) quads: Vec<Quad>,
}

/// The pointer's arrow for a frame: its upright image and where its top
/// left lies in the upright frame.
#[derive(Clone, Copy, Debug)]
pub struct ArrowAt<'a> {
    pub image: &'a Arrow,
    pub x: i64,
    pub y: i64,
}

/// One frame's work for the composition, in the upright frame's pixels.
#[derive(Clone, Copy, Debug)]
pub struct Work<'a> {
    /// The border's color, `0x00RRGGBB`. A new one composes the whole frame.
    pub border: u32,
    /// The rectangles cleared to the border's color.
    pub clear: &'a [Rect],
    /// The rectangles that changed, which present draws into each buffer
    /// that lacks them.
    pub changed: &'a [Rect],
    /// The pointer's arrow, drawn over every pixel this frame draws.
    pub arrow: Option<ArrowAt<'a>>,
    /// The console's notices and focus mark, in the order they are drawn.
    pub overlays: &'a [Placed],
    pub(crate) cells: &'a [Cells],
    /// Uploads the frame made before the composition, which its check of
    /// uploads covers.
    pub(crate) uploads: usize,
    /// The frame's pictures, in draw order, and where each draws.
    #[cfg(feature = "images")]
    pub(crate) pictures: &'a [Shown],
    /// The text layers of the cells pictures reach.
    #[cfg(feature = "images")]
    pub(crate) text: &'a [Cells],
    /// The selection's tint over the cells pictures reach that are
    /// selected: each cell and the selection's color, `0x00RRGGBB`.
    #[cfg(feature = "images")]
    pub(crate) tints: &'a [(Rect, u32)],
    /// vtium's marks over the cells pictures reach.
    #[cfg(feature = "images")]
    pub(crate) marks: &'a [Cells],
}

impl<'a> Work<'a> {
    /// A frame that clears `clear` to `border` and changes `changed`, with
    /// no cell, arrow or notice.
    pub fn new(border: u32, clear: &'a [Rect], changed: &'a [Rect]) -> Work<'a> {
        Work {
            border,
            clear,
            changed,
            arrow: None,
            overlays: &[],
            cells: &[],
            uploads: 0,
            #[cfg(feature = "images")]
            pictures: &[],
            #[cfg(feature = "images")]
            text: &[],
            #[cfg(feature = "images")]
            tints: &[],
            #[cfg(feature = "images")]
            marks: &[],
        }
    }
}

/// The upright frame and what composes into it.
#[derive(Debug)]
pub(crate) struct Composition {
    size: Size,
    /// How many pixels neighbouring tiles share.
    overlap: u32,
    tiles: Vec<(Tile, DrawTarget)>,
    cell: Program,
    arrow: Program,
    /// The arrow last uploaded and its texture.
    arrow_image: Option<(Arrow, Texture)>,
    glyphs: Option<Glyphs>,
    /// The border's color it last cleared to.
    border: Option<u32>,
    /// Whether the next frame draws the whole upright frame.
    whole: bool,
    /// The notices the last frame drew.
    overlays: Vec<Placed>,
    /// The programs of the cells that pictures reach, each compiled the
    /// first time a frame needs it: the picture shader for an opaque
    /// picture and, built with [`OVER`], for one that is not; and the cell
    /// shader built with [`OVER`], for the text layers and marks.
    #[cfg(feature = "images")]
    picture: Option<Program>,
    #[cfg(feature = "images")]
    picture_over: Option<Program>,
    #[cfg(feature = "images")]
    layer: Option<Program>,
    /// The under shader, compiled the first time a frame needs it, which
    /// copies the pixels beneath a draw out of a tile into the copy, and
    /// tints the selection.
    under: Option<Program>,
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

impl Composition {
    /// The upright frame of `size`, in tiles overlapping by `overlap`, and
    /// the composition's programs. A call that fails frees what it made.
    pub(crate) fn new<G: Gl>(gl: &mut G, size: Size, overlap: u32) -> Result<Composition, Error> {
        let made = make_tiles(gl, size, overlap)?;
        let cell = match gl.compile(&Source {
            vertex: &[CELL_VERT],
            fragment: &[ES3, CELL_FRAG],
            inputs: &CELL_INPUTS,
        }) {
            Ok(p) => p,
            Err(e) => {
                free_tiles(gl, made);
                return Err(e);
            }
        };
        let arrow = match gl.compile(&Source {
            vertex: &[QUAD_VERT],
            fragment: &[ARROW_FRAG],
            inputs: &ARROW_INPUTS,
        }) {
            Ok(p) => p,
            Err(e) => {
                let _ = gl.free_program(cell);
                free_tiles(gl, made);
                return Err(e);
            }
        };
        Ok(Composition {
            size,
            overlap,
            tiles: made,
            cell,
            arrow,
            arrow_image: None,
            glyphs: None,
            border: None,
            whole: true,
            overlays: Vec::new(),
            #[cfg(feature = "images")]
            picture: None,
            #[cfg(feature = "images")]
            picture_over: None,
            #[cfg(feature = "images")]
            layer: None,
            under: None,
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

    /// Makes what the frame reads ready: the arrow's texture, for a frame
    /// that draws pictures their programs, and for a frame that draws over
    /// pixels already drawn the under shader and a copy large enough.
    /// Gives how many uploads it made.
    pub(crate) fn prepare<G: Gl>(&mut self, gl: &mut G, work: &Work<'_>) -> Result<usize, Error> {
        let calls = self.prepare_arrow(gl, work.arrow)?;
        #[cfg(feature = "images")]
        let calls = calls.saturating_add(self.prepare_pictured(gl, work)?);
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

    /// The programs of the cells pictures reach that the frame needs.
    #[cfg(feature = "images")]
    fn prepare_pictured<G: Gl>(&mut self, gl: &mut G, work: &Work<'_>) -> Result<usize, Error> {
        let mut calls = 0usize;
        let drawn = |s: &&Shown| !s.rects.is_empty();
        let opaque = work.pictures.iter().filter(drawn).any(|s| s.opaque);
        let clear = work.pictures.iter().filter(drawn).any(|s| !s.opaque);
        let layers = !work.text.is_empty() || !work.marks.is_empty();
        let needs = [opaque, clear, layers];
        let programs = [&mut self.picture, &mut self.picture_over, &mut self.layer];
        let sources: [(&[&'static str], &[Input]); 3] = [
            (&[ES3, PICTURE_FRAG], &PICTURE_INPUTS),
            (&[ES3, OVER, PICTURE_FRAG], &PICTURE_OVER_INPUTS),
            (&[ES3, OVER, CELL_FRAG], &LAYER_INPUTS),
        ];
        for ((needed, program), (fragment, inputs)) in needs.into_iter().zip(programs).zip(sources)
        {
            if needed && program.is_none() {
                *program = Some(gl.compile(&Source {
                    vertex: &[CELL_VERT],
                    fragment,
                    inputs,
                })?);
                calls = calls.saturating_add(1);
            }
        }
        Ok(calls)
    }

    /// Makes the under shader and the copy ready for a frame that draws
    /// over the pixels of `over`. The copy holds, for each tile, the
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
        if self.under.is_none() {
            self.under = Some(gl.compile(&Source {
                vertex: &[CELL_VERT],
                fragment: &[UNDER_FRAG],
                inputs: &UNDER_INPUTS,
            })?);
            calls = calls.saturating_add(1);
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
        match (&self.under, &self.copy) {
            (Some(program), Some(copy)) => Ok(Beneath {
                program,
                copy,
                at: &self.copy_at,
            }),
            _ => Err(Error::new(
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
            let missing = || Error::new(ErrorKind::Misuse, "the picture shader is not made");
            if s.opaque {
                let program = self.picture.as_ref().ok_or_else(missing)?;
                pictures.draw(gl, i, s, &self.tiles, program, None)?;
            } else {
                let program = self.picture_over.as_ref().ok_or_else(missing)?;
                let beneath = self.beneath()?;
                pictures.draw(gl, i, s, &self.tiles, program, Some(&beneath))?;
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

    /// The textures a batch of cells samples, its masks, its colors and the
    /// sprites, and the cell's size, as the cell shader's inputs.
    fn cell_inputs(&self, batch: &Cells) -> Result<[(&'static str, Value<'_>); 4], Error> {
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
        Ok([
            ("masks", Value::Texture(masks, Filter::Nearest)),
            ("colors", Value::Texture(colors, Filter::Nearest)),
            ("sprites", Value::Texture(sprites, Filter::Nearest)),
            (
                "cell",
                Value::IVec4([side(cell.width), side(cell.height), 0, 0]),
            ),
        ])
    }

    /// The cells' quads, a draw for each texture of glyphs and tile.
    fn draw_cells<G: Gl>(&self, gl: &mut G, cells: &[Cells]) -> Result<(), Error> {
        for batch in cells {
            let [masks, colors, sprites, cell] = self.cell_inputs(batch)?;
            for (tile, target) in &self.tiles {
                let quads = in_tile(&batch.quads, tile);
                if quads.is_empty() {
                    continue;
                }
                gl.draw(&Draw {
                    target,
                    program: &self.cell,
                    inputs: &[
                        ("target_size", target_size(target)),
                        masks,
                        colors,
                        sprites,
                        cell,
                    ],
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
        let program = self
            .layer
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::Misuse, "the layer shader is not made"))?;
        let beneath = self.beneath()?;
        for batch in cells {
            let [masks, colors, sprites, cell] = self.cell_inputs(batch)?;
            for (k, (tile, target)) in self.tiles.iter().enumerate() {
                let quads = in_tile(&batch.quads, tile);
                if quads.is_empty() {
                    continue;
                }
                let copied = beneath.copy(gl, k, tile, target, &quads)?;
                let [under, under_at] = copied.inputs();
                gl.draw(&Draw {
                    target,
                    program,
                    inputs: &[
                        ("target_size", target_size(target)),
                        masks,
                        colors,
                        sprites,
                        cell,
                        under,
                        under_at,
                    ],
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
                program: &self.arrow,
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
        let mut out = gl.free_program(self.cell);
        out = out.and(gl.free_program(self.arrow));
        if let Some((_, t)) = self.arrow_image {
            out = out.and(gl.free_texture(t));
        }
        if let Some(g) = self.glyphs {
            out = out.and(g.free(gl));
        }
        #[cfg(feature = "images")]
        for p in [self.picture, self.picture_over, self.layer]
            .into_iter()
            .flatten()
        {
            out = out.and(gl.free_program(p));
        }
        if let Some(p) = self.under {
            out = out.and(gl.free_program(p));
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
fn dst_rect([x, y, w, h]: [i32; 4]) -> Option<Rect> {
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

/// The renderer's [`Back`] on the GPU path: the border's parts as clears,
/// and each cell job as one quad through the cell shader, its glyph from
/// the pools, uploaded when the pools lack it. A cell that pictures reach
/// is taken apart into ADR-1100 D4's layers: its background as a cell, the
/// parts of each picture over it, its text layer, the selection's tint and
/// vtium's mark. The front end gives each in the display's pixels, and
/// `turn` maps it into the upright frame's.
pub(crate) struct GpuBack<'a, G: Gl> {
    gl: &'a mut G,
    glyphs: &'a mut Glyphs,
    turn: Turn,
    pub(crate) clear: Vec<Rect>,
    pub(crate) cells: Vec<Cells>,
    pub(crate) error: Option<Error>,
    /// The pictures meeting the cell being drawn.
    #[cfg(feature = "images")]
    meeting: Vec<usize>,
    /// Where each of the frame's pictures draws, by its place in the
    /// frame's order, in the upright frame.
    #[cfg(feature = "images")]
    pub(crate) picture_rects: Vec<Vec<Rect>>,
    #[cfg(feature = "images")]
    pub(crate) text: Vec<Cells>,
    #[cfg(feature = "images")]
    pub(crate) tints: Vec<(Rect, u32)>,
    #[cfg(feature = "images")]
    pub(crate) marks: Vec<Cells>,
}

impl<'a, G: Gl> GpuBack<'a, G> {
    pub(crate) fn new(gl: &'a mut G, glyphs: &'a mut Glyphs, turn: Turn) -> GpuBack<'a, G> {
        glyphs.begin_frame();
        GpuBack {
            gl,
            glyphs,
            turn,
            clear: Vec::new(),
            cells: Vec::new(),
            error: None,
            #[cfg(feature = "images")]
            meeting: Vec::new(),
            #[cfg(feature = "images")]
            picture_rects: Vec::new(),
            #[cfg(feature = "images")]
            text: Vec::new(),
            #[cfg(feature = "images")]
            tints: Vec::new(),
            #[cfg(feature = "images")]
            marks: Vec::new(),
        }
    }

    /// Takes the cell `job`, whose glyph lies at `place`, apart into its
    /// layers when pictures reach it, as the CPU path's `pictured` composes
    /// it: no picture under a block cursor, and none hidden beneath the
    /// last opaque picture over the whole cell. Says whether it did.
    #[cfg(feature = "images")]
    fn pictured(&mut self, compose: &Composer, job: &Job, place: Option<Place>) -> bool {
        use crate::picture::Mark;
        use vtium_image::Layer;
        if job.under.mark == Mark::Opaque {
            return false;
        }
        let pictures = &compose.pictures;
        pictures.meeting(&job.r, &mut self.meeting);
        if self.meeting.is_empty() {
            return false;
        }
        let shows = |l: Layer| l != Layer::BelowBackground || job.under.default_bg;
        let top = self.meeting.iter().rposition(|&i| {
            pictures
                .pic(i)
                .is_some_and(|p| p.opaque() && p.covers(&job.r) && shows(p.layer()))
        });
        let (r, g) = (self.turn.up_area(job.r), self.turn.up_area(job.g));
        let bare = Job {
            underline: None,
            strike: None,
            ..*job
        };
        push(&mut self.cells, None, cell_quad(&bare, r, g, None));
        self.picture_rects
            .resize_with(pictures.pics().len(), Vec::new);
        for &i in self.meeting.get(top.unwrap_or(0)..).unwrap_or_default() {
            let Some(p) = pictures.pic(i).filter(|p| shows(p.layer())) else {
                continue;
            };
            let part = area_rect(self.turn.up_area(meet_area(job.r, p.clip())));
            if let (Some(part), Some(rects)) = (part, self.picture_rects.get_mut(i)) {
                rects.push(part);
            }
        }
        let mark = job.under.mark;
        let text = Job {
            underline: job.underline.filter(|_| mark != Mark::Underline),
            ..*job
        };
        let glyph = place.filter(|_| mark != Mark::Bar);
        if glyph.is_some() || text.layered() {
            let quad = cell_quad(&text, r, g, glyph);
            push(&mut self.text, glyph.map(|p| (p.texture, p.color)), quad);
        }
        if job.under.selected
            && let Some(cell) = area_rect(r)
        {
            self.tints.push((cell, pixel(job.bg)));
        }
        match mark {
            Mark::Bar => {
                if let Some(p) = place {
                    let quad = cell_quad(&bare, r, g, Some(p));
                    push(&mut self.marks, Some((p.texture, p.color)), quad);
                }
            }
            Mark::Underline if job.underline.is_some() => {
                let line = Job {
                    strike: None,
                    ..*job
                };
                push(&mut self.marks, None, cell_quad(&line, r, g, None));
            }
            _ => {}
        }
        true
    }
}

/// Adds `quad` to the batch of `cells` that samples the texture of glyphs
/// `glyphs`.
fn push(cells: &mut Vec<Cells>, glyphs: Option<(usize, bool)>, quad: Quad) {
    match cells.iter_mut().find(|c| c.glyphs == glyphs) {
        Some(c) => c.quads.push(quad),
        None => cells.push(Cells {
            glyphs,
            quads: vec![quad],
        }),
    }
}

/// Where `a` and `b` meet, empty when they do not.
#[cfg(feature = "images")]
fn meet_area(a: Area, b: Area) -> Area {
    let (x, y) = (a.x.max(b.x), a.y.max(b.y));
    Area {
        x,
        y,
        w: a.right().min(b.right()).saturating_sub(x).max(0),
        h: a.bottom().min(b.bottom()).saturating_sub(y).max(0),
    }
}

/// `a`'s part at or right of and below the origin, as a rectangle.
#[cfg(feature = "images")]
fn area_rect(a: Area) -> Option<Rect> {
    let clamp = |v: i64| i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
    dst_rect([clamp(a.x), clamp(a.y), clamp(a.w), clamp(a.h)])
}

impl<G: Gl> Back for GpuBack<'_, G> {
    fn border(&mut self, _: &mut Composer, (display, grid): (Rect, Option<Area>), _: Rgb, r: Rect) {
        let Some(r) = meet(r, display) else { return };
        let turn = self.turn;
        let Some(g) = grid else {
            self.clear.push(turn.up(r));
            return;
        };
        let to_u32 = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
        let (gx0, gy0, gx1, gy1) = (
            to_u32(g.x),
            to_u32(g.y),
            to_u32(g.right()),
            to_u32(g.bottom()),
        );
        let (rx1, ry1) = (r.x.saturating_add(r.width), r.y.saturating_add(r.height));
        let mut piece = |x0: u32, y0: u32, x1: u32, y1: u32| {
            if x0 < x1 && y0 < y1 {
                self.clear.push(turn.up(Rect {
                    x: x0,
                    y: y0,
                    width: x1.saturating_sub(x0),
                    height: y1.saturating_sub(y0),
                }));
            }
        };
        // above the grid, below it, and beside it on its lines
        piece(r.x, r.y, rx1, ry1.min(gy0));
        piece(r.x, r.y.max(gy1), rx1, ry1);
        let (my0, my1) = (r.y.max(gy0), ry1.min(gy1));
        piece(r.x, my0, rx1.min(gx0), my1);
        piece(r.x.max(gx1), my0, rx1, my1);
    }

    fn row<F: Face>(&mut self, compose: &mut Composer, font: &Font<F>, _: (usize, usize)) {
        if self.error.is_some() {
            return;
        }
        for job in &compose.jobs {
            let place = match job.text {
                Some((start, len, width, style)) => {
                    let text = compose
                        .text
                        .get(start..start.saturating_add(len))
                        .unwrap_or_default();
                    match self.glyphs.glyph(self.gl, font, text, width, style) {
                        Ok(p) => p,
                        Err(e) => {
                            self.error = Some(e);
                            return;
                        }
                    }
                }
                None => None,
            };
            #[cfg(feature = "images")]
            if self.pictured(compose, job, place) {
                continue;
            }
            let quad = cell_quad(
                job,
                self.turn.up_area(job.r),
                self.turn.up_area(job.g),
                place,
            );
            push(&mut self.cells, place.map(|p| (p.texture, p.color)), quad);
        }
    }

    // the GPU path draws no ink past a glyph's cells yet (ADR-1306 D9)
    const RECEIVES: bool = false;
}

/// A cell job as the cell shader's quad: where it draws, `r` in the
/// upright frame; the glyph's texel under its top left, the glyph lying at
/// `g`; and its colors, each with a kind in its top byte: the glyph's
/// (none, a mask, a color glyph), the underline's sprite and the strike's,
/// each its place among the sprites plus one, or 0 for none.
pub(crate) fn cell_quad(job: &Job, r: Area, g: Area, place: Option<Place>) -> Quad {
    let i = |v: i64| i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
    let u = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
    let (src, kind) = match place {
        Some(p) => (
            [
                p.x.saturating_add(u(r.x.saturating_sub(g.x))),
                p.y.saturating_add(u(r.y.saturating_sub(g.y))),
                0,
                0,
            ],
            if p.color { 2 } else { 1 },
        ),
        None => ([0; 4], 0),
    };
    let sprite = |k: Kind| {
        Kind::ALL
            .iter()
            .position(|&x| x == k)
            .and_then(|n| u32::try_from(n).ok())
            .map_or(0, |n| n.saturating_add(1))
    };
    let with = |c: Rgb, k: u32| pixel(c) | k << 24;
    Quad {
        dst: [i(r.x), i(r.y), i(r.w), i(r.h)],
        src,
        params: [
            with(job.fg, kind),
            pixel(job.bg),
            job.underline.map_or(0, |(k, c)| with(c, sprite(k))),
            job.strike.map_or(0, |c| with(c, sprite(Kind::Strike))),
        ],
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// How many quads cover each pixel of a `w` by `h` frame.
    fn cover(quads: &[Quad], w: u32, h: u32) -> Vec<u32> {
        let mut n = vec![0; (w * h) as usize];
        for q in quads {
            for y in q.dst[1]..q.dst[1] + q.dst[3] {
                for x in q.dst[0]..q.dst[0] + q.dst[2] {
                    n[(y as u32 * w + x as u32) as usize] += 1;
                }
            }
        }
        n
    }

    #[test]
    fn the_arrow_covers_each_drawn_pixel_under_it_once() {
        let image = Arrow::new(16, crate::target::Orientation::Normal);
        let a = ArrowAt {
            image: &image,
            x: 3,
            y: 2,
        };
        // two cells drawn and a border piece overlapping one of them, and
        // a cell under the arrow the frame did not draw
        let drawn = [rect(0, 0, 8, 16), rect(8, 0, 8, 16), rect(4, 4, 10, 2)];
        let size = Size::new(40, 30);
        let quads = arrow_quads(a, &drawn, size);
        let n = cover(&quads, 40, 30);
        for y in 0..30u32 {
            for x in 0..40u32 {
                let under = x >= 3 && x < 3 + image.width() && y >= 2 && y < 2 + image.height();
                let in_drawn = drawn
                    .iter()
                    .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height);
                let want = u32::from(under && in_drawn);
                assert_eq!(n[(y * 40 + x) as usize], want, "{x},{y}");
            }
        }
        // each quad samples the arrow where it lies
        for q in &quads {
            assert_eq!(q.src[0] as i32, q.dst[0] - 3);
            assert_eq!(q.src[1] as i32, q.dst[1] - 2);
            assert_eq!((q.src[2] as i32, q.src[3] as i32), (q.dst[2], q.dst[3]));
        }
        // the lines alike are joined: the two cells give one quad down
        // their rows above the border piece
        assert!(quads.len() < 8, "{quads:?}");
    }

    #[test]
    fn the_arrow_past_the_frames_edge_is_cut_to_it() {
        let image = Arrow::new(16, crate::target::Orientation::Normal);
        let a = ArrowAt {
            image: &image,
            x: -4,
            y: 26,
        };
        let size = Size::new(40, 30);
        let quads = arrow_quads(a, &[size.rect()], size);
        let n = cover(&quads, 40, 30);
        assert_eq!(n.iter().sum::<u32>(), (image.width() - 4) * 4);
        assert!(quads.iter().all(|q| q.src[0] >= 4));
    }
}
