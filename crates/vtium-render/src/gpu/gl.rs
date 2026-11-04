//! The `Gl` trait: what the GPU path asks of a GL context, in the
//! renderer's terms (ADR-1303 D7).
//!
//! vtium-render forbids unsafe code, so it never calls GL itself. It draws
//! through this trait. The binary implements it with an adapter over
//! vtium-platform's safe GL type, and the renderer's tests implement it with
//! [`StandIn`](super::StandIn), which records the calls and draws nothing.
//! Both check every call through one [`Ledger`](super::Ledger). The crate's
//! README lists what the ledger refuses, and what only a real driver can
//! refuse, which the stand-in cannot see.
//!
//! Every handle the trait gives out, a [`Texture`], a [`DrawTarget`], a
//! [`Program`] or a [`Fence`], is an owned value that names the context that made it. It
//! cannot be copied, and freeing it consumes it. A call with a handle that
//! was freed, or that another context made, returns an error of kind
//! [`ErrorKind::Handle`], so nothing the renderer does through the trait is
//! undefined behaviour.
//!
//! GL reports most of its own errors late, through an error flag. The
//! renderer reads that flag only at the batch boundaries D7 names, through
//! [`Gl::check`]: once after a batch of allocations and uploads, once after
//! a batch of draws and once at the end of a frame. A check after every call
//! would make a threaded driver wait for the GPU each time. The arguments of
//! each call are checked at once, before the call reaches GL.

use std::fmt;

use crate::target::Rect;

/// A width and a height in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    /// The whole of it as a rectangle at the origin.
    pub fn rect(self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        }
    }

    /// Whether `rect` lies wholly inside it.
    pub fn holds(self, rect: Rect) -> bool {
        rect.x
            .checked_add(rect.width)
            .is_some_and(|r| r <= self.width)
            && rect
                .y
                .checked_add(rect.height)
                .is_some_and(|b| b <= self.height)
    }
}

/// The pixel format of a texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// One 8-bit channel a pixel: a glyph's coverage mask (ADR-1300 D6).
    R8,
    /// Four 8-bit channels a pixel, red, green, blue and alpha in that
    /// order in memory: a picture, and the upright frame.
    Rgba8,
}

impl Format {
    /// Bytes each pixel takes.
    pub const fn bytes(self) -> usize {
        match self {
            Format::R8 => 1,
            Format::Rgba8 => 4,
        }
    }
}

/// The GPU's limits, which a context reports once when it is made.
///
/// Each is a fixed width or height that more memory does not lift
/// (ADR-1303 D3). OpenGL ES 3.0 guarantees a largest texture of at least
/// 2048 pixels a side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The largest side of a texture, `GL_MAX_TEXTURE_SIZE`.
    pub texture: u32,
    /// The largest side of a renderbuffer, `GL_MAX_RENDERBUFFER_SIZE`,
    /// which bounds an imported display buffer.
    pub renderbuffer: u32,
    /// The largest viewport, `GL_MAX_VIEWPORT_DIMS`, which bounds every
    /// target the renderer draws into.
    pub viewport: Size,
    /// Whether fragment shaders have `highp` floats and integers. OpenGL ES
    /// 3.0 requires it, so a context of that version always reports true;
    /// the renderer compiles no program without it (ADR-1300 D6).
    pub fragment_highp: bool,
}

impl Limits {
    /// Whether a texture of `size` fits the largest texture.
    pub fn fits_texture(&self, size: Size) -> bool {
        (1..=self.texture).contains(&size.width) && (1..=self.texture).contains(&size.height)
    }

    /// Whether a target of `size` fits the largest viewport.
    pub fn fits_viewport(&self, size: Size) -> bool {
        (1..=self.viewport.width).contains(&size.width)
            && (1..=self.viewport.height).contains(&size.height)
    }

    /// Whether the GPU can draw into a display buffer of `size`: within the
    /// largest renderbuffer and the largest viewport. A display whose
    /// buffer does not fit draws on the CPU path while its mode lasts
    /// (ADR-1303 D9).
    pub fn fits_buffer(&self, size: Size) -> bool {
        (1..=self.renderbuffer).contains(&size.width)
            && (1..=self.renderbuffer).contains(&size.height)
            && self.fits_viewport(size)
    }
}

/// Which context made a handle. Each [`Ledger`](super::Ledger) takes a new
/// one, so two contexts never share an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextId(pub(super) u64);

/// One handle's identity: its context, and a serial the context never gives
/// out twice, so a freed handle's serial never names a newer object.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Handle {
    pub(super) context: ContextId,
    pub(super) serial: u64,
}

/// A texture, owned by the renderer until it is freed.
#[derive(Debug, PartialEq, Eq)]
pub struct Texture(pub(super) Handle);

/// Something the renderer draws into: the upright frame, which holds its
/// own texture, or a display buffer the context imported.
#[derive(Debug, PartialEq, Eq)]
pub struct DrawTarget {
    pub(super) handle: Handle,
    pub(super) size: Size,
    pub(super) texture: Option<Texture>,
}

impl DrawTarget {
    /// Its size in pixels.
    pub fn size(&self) -> Size {
        self.size
    }

    /// The texture it draws into, for the upright frame, which a later pass
    /// reads; `None` for a display buffer.
    pub fn texture(&self) -> Option<&Texture> {
        self.texture.as_ref()
    }
}

/// A program compiled from the renderer's own shader source.
#[derive(Debug, PartialEq, Eq)]
pub struct Program(pub(super) Handle);

/// A point in a context's commands that the GPU passes once it has done
/// every command before it: the end of a frame, which [`Gl::submit`] gives
/// and [`Gl::wait`] waits on and frees.
#[derive(Debug, PartialEq, Eq)]
pub struct Fence(pub(super) Handle);

/// The kind of a program's named input, a uniform of its source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// A sampler: a texture and how it is filtered.
    Texture,
    Float,
    Vec2,
    Vec4,
    IVec4,
    UVec4,
}

/// A named input of a program, which its source declares as a uniform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Input {
    pub name: &'static str,
    pub kind: InputKind,
}

/// A program's source: the vertex and fragment shaders, each the renderer's
/// own pieces of GLSL joined in order, and the inputs the program takes.
///
/// The pieces are `'static`, so every program is built from source compiled
/// into vtium-render. Joining pieces is how the present pass folds in the
/// color and spatial steps that are switched on (ADR-1303 D6).
#[derive(Clone, Copy, Debug)]
pub struct Source<'a> {
    pub vertex: &'a [&'static str],
    pub fragment: &'a [&'static str],
    pub inputs: &'a [Input],
}

impl Source<'_> {
    /// The vertex shader's text.
    pub fn vertex_text(&self) -> String {
        self.vertex.concat()
    }

    /// The fragment shader's text.
    pub fn fragment_text(&self) -> String {
        self.fragment.concat()
    }
}

/// How a texture input is sampled where a texel does not meet a pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Linear,
}

/// A value bound to a program's input for one draw.
#[derive(Clone, Copy, Debug)]
pub enum Value<'a> {
    Texture(&'a Texture, Filter),
    Float(f32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
    IVec4([i32; 4]),
    UVec4([u32; 4]),
}

impl Value<'_> {
    /// The kind of input it fills.
    pub fn kind(&self) -> InputKind {
        match self {
            Value::Texture(..) => InputKind::Texture,
            Value::Float(_) => InputKind::Float,
            Value::Vec2(_) => InputKind::Vec2,
            Value::Vec4(_) => InputKind::Vec4,
            Value::IVec4(_) => InputKind::IVec4,
            Value::UVec4(_) => InputKind::UVec4,
        }
    }
}

/// One quad of a batch.
///
/// The adapter hands a batch to the vertex shader as instances of a strip
/// of four vertices, each quad's fields at fixed attribute locations: `dst`
/// at 0 as an `ivec4`, `src` at 1 and `params` at 2 as `uvec4`s. The vertex
/// shader takes the corner from `gl_VertexID` (`quad.vert`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quad {
    /// Where it draws in the target: x, y, width and height in the
    /// target's pixels, row 0 at the top. It may reach past the target's
    /// edges, and GL clips it.
    pub dst: [i32; 4],
    /// What it samples: x, y, width and height in texels.
    pub src: [u32; 4],
    /// Four values the program reads as it likes, such as a cell's colors.
    pub params: [u32; 4],
}

/// How a draw's pixels meet what the target holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    /// The draw's pixels replace the target's.
    Replace,
    /// Premultiplied over: `src + dst * (1 - src.a)`, GL's `GL_ONE`,
    /// `GL_ONE_MINUS_SRC_ALPHA`.
    Over,
}

/// One draw: a batch of quads through one program into one target.
#[derive(Clone, Copy, Debug)]
pub struct Draw<'a> {
    pub target: &'a DrawTarget,
    pub program: &'a Program,
    /// A value for each of the program's inputs, each named once.
    pub inputs: &'a [(&'static str, Value<'a>)],
    pub quads: &'a [Quad],
    pub blend: Blend,
    /// The rectangle the draw is cut to, in the target's pixels.
    pub scissor: Option<Rect>,
}

/// The batch whose GL errors a [`Gl::check`] reads (ADR-1303 D7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Batch {
    /// Allocations and uploads: textures made, filled or freed, targets
    /// made and programs compiled.
    Uploads,
    /// A slot's draws within a frame.
    Draws,
    /// The end of a frame.
    Frame,
}

/// What went wrong, sorted so that the head can decide what to do about it
/// (ADR-1303 D9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The driver ran out of memory, `GL_OUT_OF_MEMORY`. GL's state is
    /// undefined afterwards.
    OutOfMemory,
    /// The context was lost. Every later call fails the same way.
    ContextLost,
    /// Any other error GL reported.
    Gl,
    /// A handle that was freed, or that another context made.
    Handle,
    /// A size past the GPU's limits.
    Limit,
    /// A call the trait's contract refuses: an upload outside its texture,
    /// an input the program lacks, a draw that reads its own target.
    Misuse,
    /// A shader that did not compile or a program that did not link.
    Compile,
    /// A display buffer the context could not import (ADR-1303 D9's first
    /// kind of failure, which a staging copy answers).
    Import,
}

/// An error from a [`Gl`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    /// The batch whose check found it, for an error GL reported late.
    pub batch: Option<Batch>,
    /// What happened, for the log.
    pub detail: String,
}

impl Error {
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Error {
        Error {
            kind,
            batch: None,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.batch {
            Some(b) => write!(f, "{:?} after {:?}: {}", self.kind, b, self.detail),
            None => write!(f, "{:?}: {}", self.kind, self.detail),
        }
    }
}

impl std::error::Error for Error {}

/// A GL context, as the renderer sees it.
///
/// Each call's arguments are checked when it is made, and a call that
/// fails the check reaches no GL. GL's own errors wait for [`Gl::check`].
pub trait Gl {
    /// A display buffer the display lent, as the implementation names it.
    type Buffer;

    /// The GPU's limits.
    fn limits(&self) -> Limits;

    /// Makes a texture of `format` and `size`, its contents undefined.
    fn create_texture(&mut self, format: Format, size: Size) -> Result<Texture, Error>;

    /// Fills `rect` of `texture` from `pixels`, rows `stride` bytes apart,
    /// each pixel in the texture's format.
    fn fill_texture(
        &mut self,
        texture: &Texture,
        rect: Rect,
        pixels: &[u8],
        stride: usize,
    ) -> Result<(), Error>;

    /// Frees `texture`.
    fn free_texture(&mut self, texture: Texture) -> Result<(), Error>;

    /// Makes an upright frame of `size`: a target that draws into a
    /// [`Format::Rgba8`] texture of its own, within the largest texture and
    /// viewport. A call that fails leaves nothing made.
    fn create_frame(&mut self, size: Size) -> Result<DrawTarget, Error>;

    /// A target that draws into the display buffer `buffer`, which must fit
    /// the largest renderbuffer and viewport.
    fn import_buffer(&mut self, buffer: &Self::Buffer) -> Result<DrawTarget, Error>;

    /// Frees `target`, and the upright frame's texture with it.
    fn free_target(&mut self, target: DrawTarget) -> Result<(), Error>;

    /// Compiles and links `source`. Each of its inputs must be declared as
    /// a uniform in one of its shaders.
    fn compile(&mut self, source: &Source<'_>) -> Result<Program, Error>;

    /// Frees `program`.
    fn free_program(&mut self, program: Program) -> Result<(), Error>;

    /// Fills `rect` of `target` with `color`, `0x00RRGGBB`, opaque.
    fn clear(&mut self, target: &DrawTarget, rect: Rect, color: u32) -> Result<(), Error>;

    /// Draws a batch of quads.
    fn draw(&mut self, draw: &Draw<'_>) -> Result<(), Error>;

    /// Ends the frame drawn into `target`: sends its queued commands to the
    /// GPU and gives a fence after them, which [`Gl::wait`] waits on before
    /// the display shows the buffer.
    fn submit(&mut self, target: &DrawTarget) -> Result<Fence, Error>;

    /// Waits until the GPU has passed `fence`, so that every command before
    /// it is done, and frees it. The render thread waits before it gives
    /// the buffer back to the display, which then flips to a finished
    /// frame.
    fn wait(&mut self, fence: Fence) -> Result<(), Error>;

    /// Reads GL's errors since the last check, for the batch that just
    /// ended. The error names the batch.
    fn check(&mut self, batch: Batch) -> Result<(), Error>;

    /// Reads `rect` of `target` into `out`, RGBA8 rows with no gap between
    /// them. For tests: a read makes the CPU wait for the GPU.
    fn read(&mut self, target: &DrawTarget, rect: Rect, out: &mut [u8]) -> Result<(), Error>;
}
