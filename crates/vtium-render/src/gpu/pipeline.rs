//! The GPU path's frame: five slots in a fixed order (ADR-1303 D1).
//!
//! 1. Pictures at another size: the textures the composition samples for
//!    each picture, by GPU scaling or the CPU copy
//!    (`super::pictures::Pictures`, in a build with `images`).
//! 2. [`Composition`] draws what changed into the upright frame, a texture
//!    the size of the whole display that outlives a frame.
//! 3. [`Color`] maps each pixel's color to a new one.
//! 4. [`Spatial`] reads the upright frame around a pixel.
//! 5. [`Present`] turns the upright frame into the display's buffer, with
//!    the color and spatial steps that are switched on folded into its one
//!    pass.
//!
//! The slots never change and a configuration changes only what a slot
//! holds. A slot with nothing to do holds a no-op that makes no GL call,
//! joins no shader source and changes no pixel. The color and spatial slots
//! hold only their no-ops until the frame shader's record defines their
//! steps.
//!
//! GL's errors are read at the batch boundaries D7 names: after the set-up's
//! allocations, after the composition's draws and at the end of the frame,
//! and only when the batch made a call. A frame that drew ends with
//! [`Gl::submit`], so the display can show the buffer.
//!
//! A turn of the display changes present's quads alone, and keeps every
//! texture and program: the upright frame is made anew only where a quarter
//! turn swaps its sides.
//!
//! [`Pipeline::release`] gives back the glyph and picture textures and the
//! upright frame, as a head does when its VT is released, keeping the
//! programs and the context, and the next frame makes the frame again and
//! draws whole (ADR-1303 D9).

use std::collections::VecDeque;

use super::compose::{CELL_VERT, Composition, TARGET_SIZE, Work};
use super::gl::{
    Batch, Blend, Draw, DrawTarget, Error, ErrorKind, Filter, Gl, Input, InputKind, Program, Quad,
    Size, Source, Value,
};
use super::glyphs::{Glyphs, Spec};
use super::tile::{Tile, meet};
use super::turn::{Turn, upright};
use crate::bounds::PRESENT_HISTORY;
use crate::notice::Placed;
use crate::target::{Orientation, Rect};

const PRESENT_HEAD: &str = include_str!("glsl/present_head.frag");
const PRESENT_MAIN: &str = include_str!("glsl/present_main.frag");

/// Present's own inputs, before those of the steps folded into it.
const PRESENT_INPUTS: [Input; 2] = [
    TARGET_SIZE,
    Input {
        name: "frame",
        kind: InputKind::Texture,
    },
];

/// Slot 3: a step that maps each pixel's color to a new one. Its no-op is
/// the identity, which present leaves out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Color {
    /// No color step.
    #[default]
    None,
}

impl Color {
    /// The GLSL it joins into present's fragment shader: the macro
    /// `COLOR_STEP` and `vec4 color_step(vec4)`.
    fn source(self) -> &'static [&'static str] {
        match self {
            Color::None => &[],
        }
    }

    /// The inputs it adds to present's.
    fn inputs(self) -> &'static [Input] {
        match self {
            Color::None => &[],
        }
    }
}

/// Slot 4: a step that reads the upright frame around a pixel, such as a
/// CRT look. Its no-op is the identity, which present leaves out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Spatial {
    /// No spatial step.
    #[default]
    None,
}

impl Spatial {
    /// The GLSL it joins into present's fragment shader: the macro
    /// `SPATIAL_STEP` and `vec4 spatial_step(ivec2)`.
    fn source(self) -> &'static [&'static str] {
        match self {
            Spatial::None => &[],
        }
    }

    /// The inputs it adds to present's.
    fn inputs(self) -> &'static [Input] {
        match self {
            Spatial::None => &[],
        }
    }

    /// How many pixels from the pixel it writes the step reads, on each
    /// side. The upright frame's tiles overlap by twice it, so that every
    /// pixel a tile owns has its neighbours in that tile (ADR-1303 D3).
    pub fn reach(self) -> u32 {
        match self {
            Spatial::None => 0,
        }
    }

    /// Whether every present draws the whole buffer: a step that reads other
    /// pixels than the one it writes, or the time (ADR-1303 D5).
    pub fn presents_whole(self) -> bool {
        match self {
            Spatial::None => false,
        }
    }
}

/// What the configuration puts in each slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub color: Color,
    pub spatial: Spatial,
}

/// Slot 1, pictures at another size, in a build with `images`.
#[cfg(feature = "images")]
use super::pictures::Pictures;

/// Slot 1 in a build without `images`, which draws no picture.
#[cfg(not(feature = "images"))]
#[derive(Debug)]
struct Pictures;

#[cfg(not(feature = "images"))]
impl Pictures {
    fn new() -> Pictures {
        Pictures
    }

    fn release<G: Gl>(&mut self, _gl: &mut G) -> Result<(), Error> {
        Ok(())
    }
}

/// Slot 5, present.
#[derive(Debug)]
struct Present {
    program: Program,
    /// The rectangles each recent frame composed, newest first.
    history: VecDeque<Vec<Rect>>,
}

impl Present {
    /// Compiles present's program, with the steps of `color` and `spatial`
    /// joined in.
    fn new<G: Gl>(gl: &mut G, color: Color, spatial: Spatial) -> Result<Present, Error> {
        let fragment: Vec<&'static str> = [PRESENT_HEAD]
            .into_iter()
            .chain(spatial.source().iter().copied())
            .chain(color.source().iter().copied())
            .chain([PRESENT_MAIN])
            .collect();
        let inputs: Vec<Input> = PRESENT_INPUTS
            .iter()
            .chain(spatial.inputs())
            .chain(color.inputs())
            .copied()
            .collect();
        let program = gl.compile(&Source {
            vertex: &[CELL_VERT],
            fragment: &fragment,
            inputs: &inputs,
        })?;
        Ok(Present {
            program,
            history: VecDeque::with_capacity(PRESENT_HISTORY),
        })
    }

    /// Turns into `target`, a buffer `age` frames old, what it lacks: the
    /// upright frame's rectangles composed over its age, `composed` the
    /// newest, or the whole buffer at age 0 or past the history; the whole
    /// buffer too when `whole` and it lacks anything, or when what it lacks
    /// includes the whole. Each tile of the upright frame draws the part it
    /// owns. Gives the rectangles of the buffer it drew, the frame's damage.
    /// A buffer that lacks nothing is not drawn, and the frame does not
    /// count for the ages that follow, as on the CPU path.
    #[expect(
        clippy::too_many_arguments,
        reason = "the frame, the buffer and its age, and what the frame composed"
    )]
    fn draw<G: Gl>(
        &mut self,
        gl: &mut G,
        composition: &Composition,
        turn: Turn,
        target: &DrawTarget,
        age: u32,
        composed: Vec<Rect>,
        whole: bool,
    ) -> Result<Vec<Rect>, Error> {
        self.history.push_front(composed);
        let age = usize::try_from(age).unwrap_or(usize::MAX);
        // The history holds every frame composed since the pipeline was
        // made or last forgot its buffers, newest first, this frame's
        // included, and the first of those frames composed the whole
        // upright frame. A buffer `age` frames old lacks the newest `age`
        // of them; at an age equal to the history's length, that includes
        // the whole first one, as the CPU path's held serial does.
        let unknown = age == 0 || age > self.history.len().min(PRESENT_HISTORY);
        let lacking: Vec<Rect> = if unknown {
            Vec::new()
        } else {
            self.history.iter().take(age).flatten().copied().collect()
        };
        if !unknown && lacking.is_empty() {
            self.history.pop_front();
            return Ok(lacking);
        }
        self.history.truncate(PRESENT_HISTORY);
        let size = target.size();
        let all = composition.size().rect();
        let rects = if unknown || whole || lacking.iter().any(|&r| meet(r, all) == Some(all)) {
            vec![all]
        } else {
            lacking
        };
        for (tile, frame) in composition.tiles() {
            let frame =
                frame.ok_or_else(|| Error::new(ErrorKind::Misuse, "present reads no frame"))?;
            let quads: Vec<Quad> = rects
                .iter()
                .filter_map(|&r| meet(r, tile.owned))
                .map(|r| quad(r, tile, turn))
                .collect();
            if quads.is_empty() {
                continue;
            }
            gl.draw(&Draw {
                target,
                program: &self.program,
                inputs: &[
                    (
                        "target_size",
                        Value::Vec2([size.width as f32, size.height as f32]),
                    ),
                    ("frame", Value::Texture(frame, Filter::Nearest)),
                ],
                quads: &quads,
                blend: Blend::Replace,
                scissor: None,
            })?;
        }
        Ok(rects.into_iter().map(|r| turn.down(r)).collect())
    }

    /// Forgets what the buffers hold, so the next present draws whole.
    fn forget(&mut self) {
        self.history.clear();
    }
}

/// A quad that turns `r` of the upright frame, which `tile` holds, onto
/// the display: `dst` the display's rectangle it draws, `src` the tile's
/// texels under `r`, and the turn's code in its first parameter, which
/// present's shader reads as `params.x`.
fn quad(r: Rect, tile: &Tile, turn: Turn) -> Quad {
    let i = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    let d = turn.down(r);
    Quad {
        dst: [i(d.x), i(d.y), i(d.width), i(d.height)],
        src: [
            r.x.saturating_sub(tile.held.x),
            r.y.saturating_sub(tile.held.y),
            r.width,
            r.height,
        ],
        params: [turn.code(), 0, 0, 0],
    }
}

/// What [`Pipeline::begin`] decided for a frame.
#[derive(Debug)]
pub(crate) struct Begun {
    /// The composition draws the whole upright frame.
    pub(crate) whole: bool,
    /// Where the last frame's notices were that this one no longer draws.
    pub(crate) gone: Vec<Rect>,
}

/// The GPU path's frame for one display, through its five slots.
#[derive(Debug)]
pub struct Pipeline {
    /// The display's turn and size, which every buffer it lends has.
    turn: Turn,
    pictures: Pictures,
    composition: Composition,
    spatial: Spatial,
    present: Present,
}

impl Pipeline {
    /// Sets up the frame for an upright display of `size`:
    /// [`Pipeline::turned`] with [`Orientation::Normal`].
    pub fn new<G: Gl>(gl: &mut G, size: Size, config: Config) -> Result<Pipeline, Error> {
        Pipeline::turned(gl, size, Orientation::Normal, config)
    }

    /// Sets up the frame for a display of `size` turned as `orientation`
    /// says: the upright frame in tiles, the composition's programs, and
    /// present's program with the configured steps joined in. Its
    /// allocations are one batch, checked once.
    pub fn turned<G: Gl>(
        gl: &mut G,
        size: Size,
        orientation: Orientation,
        config: Config,
    ) -> Result<Pipeline, Error> {
        let overlap = config.spatial.reach().saturating_mul(2);
        let turn = Turn::new(orientation, size);
        let composition = Composition::new(gl, turn.upright(), overlap)?;
        let present = match Present::new(gl, config.color, config.spatial) {
            Ok(p) => p,
            Err(e) => {
                // the error that matters is the compile's
                let _ = composition.free(gl);
                return Err(e);
            }
        };
        let pipeline = Pipeline {
            turn,
            pictures: Pictures::new(),
            composition,
            spatial: config.spatial,
            present,
        };
        if let Err(e) = gl.check(Batch::Uploads) {
            // the error that matters is the check's
            let _ = pipeline.free(gl);
            return Err(e);
        }
        Ok(pipeline)
    }

    /// Draws one frame into `target`, the display's buffer `age` frames
    /// old (0 when its contents are unknown), and gives the rectangles of
    /// the buffer it drew: the frame's damage. A frame that composes nothing
    /// into a buffer that lacks nothing makes no call and gives no damage,
    /// and the display does not swap it. A buffer of another size than the
    /// display's is refused.
    ///
    /// A frame that fails leaves the upright frame and the buffers unknown,
    /// so the next frame composes and presents whole (ADR-1303 D3, D9).
    pub fn frame<G: Gl>(
        &mut self,
        gl: &mut G,
        work: &Work<'_>,
        target: &DrawTarget,
        age: u32,
    ) -> Result<Vec<Rect>, Error> {
        if target.size() != self.turn.display {
            return Err(Error::new(
                ErrorKind::Misuse,
                "the buffer's size is not the display's",
            ));
        }
        let drawn = self.slots(gl, work, target, age);
        if drawn.is_err() {
            self.redraw_whole();
        }
        drawn
    }

    fn slots<G: Gl>(
        &mut self,
        gl: &mut G,
        work: &Work<'_>,
        target: &DrawTarget,
        age: u32,
    ) -> Result<Vec<Rect>, Error> {
        // a frame after a release makes the upright frame again first
        let restored = self.composition.restore(gl)?;
        #[cfg(feature = "images")]
        let pictures = self
            .pictures
            .prepare(gl, work.pictures, self.composition.size())?;
        #[cfg(not(feature = "images"))]
        let pictures = 0usize;
        let uploads = pictures
            .saturating_add(restored)
            .saturating_add(work.uploads)
            .saturating_add(self.composition.prepare(gl, work)?);
        if uploads > 0 {
            gl.check(Batch::Uploads)?;
        }
        let composed = self.composition.draw(
            gl,
            work,
            #[cfg(feature = "images")]
            &self.pictures,
        )?;
        self.composition.end_frame(gl)?;
        if !composed.is_empty() {
            gl.check(Batch::Draws)?;
        }
        // slots 3 and 4 run inside present's one pass
        let damage = self.present.draw(
            gl,
            &self.composition,
            self.turn,
            target,
            age,
            composed,
            self.spatial.presents_whole(),
        )?;
        if damage.is_empty() {
            return Ok(damage);
        }
        gl.check(Batch::Frame)?;
        gl.submit(target)?;
        Ok(damage)
    }

    /// Gives back the glyph and picture textures and the upright frame,
    /// keeping the compiled programs and the context, as a head does when
    /// its VT is released (ADR-1303 D9). The next frame makes the upright
    /// frame again at the display's size, makes the glyphs again, uploads
    /// the pictures it draws, and composes and presents whole.
    pub fn release<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        let pictures = self.pictures.release(gl);
        let composition = self.composition.release(gl);
        self.present.forget();
        pictures.and(composition)
    }

    /// Chooses the step that draws pictures at another size: GPU scaling,
    /// the default, or the CPU copy, which tests choose to check a
    /// picture's pixels exactly (ADR-1303 D2). No key of vtium.conf sets
    /// it. A picture already uploaded keeps its textures while they last.
    #[cfg(feature = "images")]
    pub fn set_picture_step(&mut self, step: super::PictureStep) {
        self.pictures.set_step(step);
    }

    /// Holds the picture textures to `side` pixels a side, or the GPU's
    /// largest texture with `None`, so a test can tile a picture with no
    /// GPU of a small limit.
    #[cfg(feature = "images")]
    pub fn limit_picture_textures(&mut self, side: Option<u32>) {
        self.pictures.set_largest(side);
    }

    /// What the picture textures did and hold, counted as the CPU path
    /// counts its copies: textures made and kept, pictures drawn from the
    /// scratch texture, evictions, and the bytes held.
    #[cfg(feature = "images")]
    pub fn picture_counts(&self) -> crate::picture::PictureCounts {
        self.pictures.counts()
    }

    /// The notes for the log since the last call, which the head writes
    /// to its log: today only that glyphs were drawn blank past the glyph
    /// textures' ceiling, given at most once in `GPU_IDLE_FRAMES` frames.
    pub fn take_notes(&mut self) -> Vec<String> {
        self.composition.take_notes()
    }

    /// Bytes the GPU glyph pools' and sprites' textures hold, while the
    /// pipeline holds glyphs, at most the bounds table's
    /// `GLYPH_TEXTURE_CEILING`.
    pub fn glyph_texture_bytes(&self) -> Option<u64> {
        self.composition.glyph_bytes()
    }

    /// Makes the next frame compose the whole upright frame and present the
    /// whole buffer.
    pub fn redraw_whole(&mut self) {
        self.composition.redraw_whole();
        self.present.forget();
    }

    /// The display's size.
    pub fn size(&self) -> Size {
        self.turn.display
    }

    /// How the display is turned.
    pub fn orientation(&self) -> Orientation {
        self.turn.orientation
    }

    /// Turns the display as `orientation` says (ADR-1303 D3, D9). The next
    /// frame composes the upright frame whole, since the grid's place in it
    /// moves, and presents the whole buffer turned. A turn compiles nothing
    /// and keeps the glyphs and the arrow. Only a quarter turn of a display
    /// that is not square makes the upright frame anew, its sides swapped,
    /// a batch of allocations checked before the old frame goes. A turn
    /// whose allocation or check fails leaves the pipeline as it was, the
    /// orientation and the frame's shape together, so a later turn to
    /// either orientation finds them in step.
    pub fn turn<G: Gl>(&mut self, gl: &mut G, orientation: Orientation) -> Result<(), Error> {
        let size = upright(orientation, self.turn.display);
        if orientation == self.turn.orientation && size == self.composition.size() {
            return Ok(());
        }
        let reshaped = if size == self.composition.size() {
            Ok(())
        } else {
            self.composition.reshape(gl, size)
        };
        // the orientation follows the frame's shape: a reshape that failed
        // before its new tiles stood left both as they were
        if self.composition.size() == size {
            self.turn.orientation = orientation;
            self.redraw_whole();
        }
        reshaped
    }

    /// Starts a frame of the renderer `spec` names: its glyphs, made anew
    /// when the pipeline holds another renderer's, which composes the frame
    /// whole. Gives whether the composition draws the whole upright frame,
    /// so the renderer draws the whole picture, and the areas of the last
    /// frame's notices and focus mark that `overlays` no longer draws as
    /// they were, whose cells the renderer draws again. A start that fails
    /// leaves the pipeline with no glyphs, so the next start makes them and
    /// composes whole.
    pub(crate) fn begin<G: Gl>(
        &mut self,
        gl: &mut G,
        spec: &Spec<'_>,
        border: u32,
        overlays: &[Placed],
    ) -> Result<Begun, Error> {
        self.composition.glyphs(gl, spec)?;
        Ok(Begun {
            whole: self.composition.needs_whole(border),
            gone: self.composition.overlays_gone(overlays),
        })
    }

    /// The glyphs [`Pipeline::begin`] made ready, for the renderer's back end.
    pub(crate) fn glyphs(&mut self) -> Result<&mut Glyphs, Error> {
        self.composition.held_glyphs()
    }

    /// Frees what the pipeline holds in GL.
    pub fn free<G: Gl>(mut self, gl: &mut G) -> Result<(), Error> {
        let pictures = self.pictures.release(gl);
        let program = gl.free_program(self.present.program);
        self.composition.free(gl).and(program).and(pictures)
    }
}
