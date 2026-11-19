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
//! joins no shader source and changes no pixel. In a build with `filters`
//! the color slot holds the head's color table ([`Pipeline::set_color_table`]);
//! the spatial slot holds only its no-op until the spatial filters are
//! built.
//!
//! GL's errors are read at the batch boundaries D7 names: after the set-up's
//! allocations, after the composition's draws and at the end of the frame,
//! and only when the batch made a call. A frame that drew ends with
//! [`Gl::submit`] and waits on the fence it gives ([`Gl::wait`]), so the
//! buffer the display shows next holds a finished frame.
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
    Size, Source, Texture, Value,
};
use super::glyphs::{Glyphs, Spec};
use super::tile::{Tile, meet};
use super::turn::{Turn, upright};
use crate::bounds::PRESENT_HISTORY;
use crate::notice::Placed;
use crate::target::{Orientation, Rect};

const PRESENT_HEAD: &str = include_str!("glsl/present_head.frag");
const PRESENT_MAIN: &str = include_str!("glsl/present_main.frag");
#[cfg(feature = "filters")]
const PRESENT_COLOR: &str = include_str!("glsl/present_color.frag");

/// The color step's input: the color table as a 3D texture.
#[cfg(feature = "filters")]
const COLOR_TABLE: Input = Input {
    name: "color_table",
    kind: InputKind::Volume,
};

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
    /// The head's color table, which present samples with linear
    /// filtering. A pipeline set up with it and no table yet samples the
    /// identity table, which changes no color.
    ///
    /// References:
    /// - ADR-1304 D2: the GPU path's color step.
    #[cfg(feature = "filters")]
    Table,
}

impl Color {
    /// The GLSL it joins into present's fragment shader: the macro
    /// `COLOR_STEP` and `vec4 color_step(vec4)`.
    fn source(self) -> &'static [&'static str] {
        match self {
            Color::None => &[],
            #[cfg(feature = "filters")]
            Color::Table => &[PRESENT_COLOR],
        }
    }

    /// The inputs it adds to present's.
    fn inputs(self) -> &'static [Input] {
        match self {
            Color::None => &[],
            #[cfg(feature = "filters")]
            Color::Table => &[COLOR_TABLE],
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
        table: Option<&Texture>,
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
            let target_size = (
                "target_size",
                Value::Vec2([size.width as f32, size.height as f32]),
            );
            let frame = ("frame", Value::Texture(frame, Filter::Nearest));
            let with_table;
            let inputs: &[(&'static str, Value<'_>)] = match table {
                Some(t) => {
                    with_table = [
                        target_size,
                        frame,
                        ("color_table", Value::Volume(t, Filter::Linear)),
                    ];
                    &with_table
                }
                None => &[target_size, frame],
            };
            gl.draw(&Draw {
                target,
                program: &self.program,
                inputs,
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
    color: Color,
    /// The color table present samples while `color` is [`Color::Table`],
    /// and the id of the table it holds, `None` for the identity a
    /// pipeline set up with the color step starts from.
    table: Option<(Texture, Option<u64>)>,
    spatial: Spatial,
    present: Present,
    /// The pipeline's own notes for the log, beside the composition's.
    notes: Vec<String>,
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
        let table = match identity(gl, config.color) {
            Ok(t) => t,
            Err(e) => {
                // the error that matters is the upload's
                let _ = composition.free(gl);
                let _ = gl.free_program(present.program);
                return Err(e);
            }
        };
        let pipeline = Pipeline {
            turn,
            pictures: Pictures::new(),
            composition,
            color: config.color,
            table: table.map(|t| (t, None)),
            spatial: config.spatial,
            present,
            notes: Vec::new(),
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
            self.table.as_ref().map(|(t, _)| t),
        )?;
        if damage.is_empty() {
            return Ok(damage);
        }
        gl.check(Batch::Frame)?;
        let fence = gl.submit(target)?;
        gl.wait(fence)?;
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
        let mut notes = core::mem::take(&mut self.notes);
        notes.extend(self.composition.take_notes());
        notes
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
        let table = self.table.map_or(Ok(()), |(t, _)| gl.free_texture(t));
        self.composition
            .free(gl)
            .and(program)
            .and(pictures)
            .and(table)
    }

    /// What the color slot holds.
    pub fn color(&self) -> Color {
        self.color
    }

    /// Puts `table`, a head's color table with the id its renderer gave
    /// it, in the color slot, or empties the slot with `None`. A table of
    /// the id the slot holds already changes nothing, so a renderer can
    /// hand its table over before every frame. Anything else uploads the
    /// table as a 3D texture, in a batch of its own that is checked at
    /// once, compiles present again when the step comes or goes, and makes
    /// the next frame compose and present whole.
    ///
    /// A failure that the look causes, an upload GL refuses or a present
    /// the driver will not compile with the step joined, leaves the slot
    /// empty, so the display draws without the look's colors, and gives a
    /// note for the log. Only GL running out of memory, a lost context and
    /// a present that does not compile even without the step fail the
    /// call, as failures of the GPU path itself.
    ///
    /// References:
    /// - ADR-1304 D2: the GPU path uploads the table as an RGBA16F 3D texture and presents whole after a change.
    /// - ADR-1304 D6: a filter's failure is never the GPU path's failure.
    #[cfg(feature = "filters")]
    pub fn set_color_table<G: Gl>(
        &mut self,
        gl: &mut G,
        table: Option<(u64, &crate::filter::Table)>,
    ) -> Result<(), Error> {
        let held = self.table.as_ref().and_then(|(_, id)| *id);
        if held == table.map(|(id, _)| id) && (table.is_some() || self.color == Color::None) {
            return Ok(());
        }
        let Some((id, table)) = table else {
            return self.empty_color(gl);
        };
        let texels = match table.rgba16f() {
            Ok(t) => t,
            Err(e) => return self.drop_colors(gl, format!("building the color table: {e}")),
        };
        let texture = match upload(gl, &texels) {
            Ok(t) => t,
            Err(e) if serious(&e) => return Err(e),
            Err(e) => return self.drop_colors(gl, format!("uploading the color table: {e}")),
        };
        if self.color != Color::Table {
            match Present::new(gl, Color::Table, self.spatial) {
                Ok(present) => {
                    let old = core::mem::replace(&mut self.present, present);
                    self.present.history = old.history;
                    gl.free_program(old.program)?;
                }
                Err(e) => {
                    let _ = gl.free_texture(texture);
                    if e.kind == ErrorKind::Compile {
                        return self.drop_colors(gl, format!("compiling the color step: {e}"));
                    }
                    return Err(e);
                }
            }
        }
        self.color = Color::Table;
        if let Some((old, _)) = self.table.replace((texture, Some(id))) {
            gl.free_texture(old)?;
        }
        // every pixel changes: the history's rectangles say nothing now
        self.redraw_whole();
        Ok(())
    }

    /// Empties the color slot after a failure the look caused, with a
    /// note for the log that says what failed.
    #[cfg(feature = "filters")]
    fn drop_colors<G: Gl>(&mut self, gl: &mut G, why: String) -> Result<(), Error> {
        self.notes.push(format!(
            "the display draws without the vtlook's color filters: {why}"
        ));
        self.empty_color(gl)
    }

    /// Empties the color slot: present is compiled again without the color
    /// step, the table's texture is freed, and the next frame presents
    /// whole.
    #[cfg(feature = "filters")]
    fn empty_color<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        if self.color != Color::None {
            let present = Present::new(gl, Color::None, self.spatial)?;
            let old = core::mem::replace(&mut self.present, present);
            self.present.history = old.history;
            gl.free_program(old.program)?;
            self.color = Color::None;
        }
        if let Some((old, _)) = self.table.take() {
            gl.free_texture(old)?;
        }
        self.redraw_whole();
        Ok(())
    }
}

/// Whether `e` is a failure of the GPU path itself, which ADR-1303 D9
/// sorts, whatever the look: GL out of memory, or a lost context. After
/// either, nothing the context holds can be trusted.
#[cfg(feature = "filters")]
fn serious(e: &Error) -> bool {
    matches!(e.kind, ErrorKind::OutOfMemory | ErrorKind::ContextLost)
}

/// Uploads `texels`, a color table's RGBA16F texels, as a 3D texture, and
/// checks the upload's batch at once, so that an error GL raises is traced
/// to the table. A texture GL refused is freed.
#[cfg(feature = "filters")]
fn upload<G: Gl>(gl: &mut G, texels: &[u8]) -> Result<Texture, Error> {
    let side = u32::try_from(crate::filter::Table::side()).unwrap_or(u32::MAX);
    let texture = gl.create_volume(side, texels)?;
    if let Err(e) = gl.check(Batch::Uploads) {
        // the error that matters is the check's
        let _ = gl.free_texture(texture);
        return Err(e);
    }
    Ok(texture)
}

/// The identity table's texture, for a pipeline set up with the color
/// step and no table yet; `None` with no color step.
#[cfg_attr(
    not(feature = "filters"),
    expect(unused_variables, reason = "no color step")
)]
fn identity<G: Gl>(gl: &mut G, color: Color) -> Result<Option<Texture>, Error> {
    match color {
        Color::None => Ok(None),
        #[cfg(feature = "filters")]
        Color::Table => {
            let table = crate::filter::Table::build(&[])
                .map_err(|e| Error::new(ErrorKind::OutOfMemory, e.to_string()))?;
            let texels = table
                .rgba16f()
                .map_err(|e| Error::new(ErrorKind::OutOfMemory, e.to_string()))?;
            gl.create_volume(
                u32::try_from(crate::filter::Table::side()).unwrap_or(u32::MAX),
                &texels,
            )
            .map(Some)
        }
    }
}
