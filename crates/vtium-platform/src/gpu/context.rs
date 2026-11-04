//! [`Context`], the safe GL type (ADR-1303 D7).
//!
//! Each method makes its GL calls sound by itself. A name GL never made,
//! or one another context made, only makes GL raise an error, which
//! [`Context::check`] reads later. What would be undefined behaviour is
//! checked before the call: a slice shorter than the pixels an upload or a
//! read names, an instance buffer shorter than the draw, a buffer bound for
//! pixel transfers that would turn a pointer into an offset, and a call
//! made while another context is current on the thread. Every method makes
//! its own context current first, and sets the pixel-store state its call
//! reads.
//!
//! This file sets the context up and reads its errors; `objects.rs` makes,
//! fills and frees its textures, frames, imported buffers and programs, and
//! `draw.rs` draws, clears, submits and reads back.
//!
//! The C reference read GL's error flag once after a batch of uploads and
//! once after a frame's draws (`kmscon/src/render/gltex.c`, `upload` and
//! `gltex_render`).

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::{CStr, c_char, c_void};
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::os::fd::OwnedFd;
use std::path::Path;

use glow::HasContext;
use khronos_egl as egl;
use rustix::fs::{Mode, OFlags};

mod draw;
mod objects;

pub use draw::{Blend, DrawCall, Filter, Instance, Location, Uniform};

use super::egl::Egl;
use super::gbm::Gbm;
use super::{Error, ErrorKind, SetupError, Step};
use crate::display::Rect;

/// `GL_CONTEXT_LOST`, which OpenGL ES 3.2 and `GL_KHR_robustness` report.
const CONTEXT_LOST: u32 = 0x0507;
/// Most errors one check reads: GL keeps one flag for each kind of error,
/// and a lost context may report one at every read.
const ERRORS_MAX: usize = 16;
/// Bytes of one instance: three vectors of four 32-bit integers.
const INSTANCE_BYTES: usize = 48;
/// The same, as GL's attribute stride takes it.
const INSTANCE_STRIDE: i32 = 48;

type GetString = unsafe extern "system" fn(u32) -> *const c_char;
type ImageTargetRenderbuffer = unsafe extern "system" fn(u32, *const c_void);
type ResetStatus = unsafe extern "system" fn() -> u32;

/// The GPU's limits, read once when the context is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// `GL_MAX_TEXTURE_SIZE`.
    pub texture: u32,
    /// `GL_MAX_RENDERBUFFER_SIZE`.
    pub renderbuffer: u32,
    /// `GL_MAX_VIEWPORT_DIMS`.
    pub viewport: (u32, u32),
    /// Whether fragment shaders have `highp` floats and integers, through
    /// `glGetShaderPrecisionFormat`. A context is not made without them.
    pub fragment_highp: bool,
    /// `GL_MAX_TEXTURE_IMAGE_UNITS`: the most textures one draw reads.
    pub texture_units: u32,
}

/// A texture's format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    /// One 8-bit channel, `GL_R8`.
    R8,
    /// Red, green, blue and alpha, 8 bits each, `GL_RGBA8`.
    Rgba8,
}

impl TextureFormat {
    fn bytes(self) -> usize {
        match self {
            TextureFormat::R8 => 1,
            TextureFormat::Rgba8 => 4,
        }
    }

    fn internal(self) -> u32 {
        match self {
            TextureFormat::R8 => glow::R8,
            TextureFormat::Rgba8 => glow::RGBA8,
        }
    }

    fn pixels(self) -> u32 {
        match self {
            TextureFormat::R8 => glow::RED,
            TextureFormat::Rgba8 => glow::RGBA,
        }
    }
}

/// A texture the context made, as the context keeps it, so an upload is
/// checked against the texture itself.
#[derive(Clone, Copy, Debug)]
struct TextureInfo {
    format: TextureFormat,
    width: u32,
    height: u32,
}

/// An imported buffer's renderbuffer and image, which its framebuffer
/// draws into.
struct Import {
    renderbuffer: glow::NativeRenderbuffer,
    image: egl::Image,
}

fn misuse(detail: impl Into<String>) -> Error {
    Error::new(ErrorKind::Misuse, detail)
}

/// `v` as a GL integer.
fn int(v: u32) -> Result<i32, Error> {
    i32::try_from(v).map_err(|_| misuse(format!("{v} passes GL's integers")))
}

/// A rectangle as GL takes it.
fn ints(r: Rect) -> Result<[i32; 4], Error> {
    Ok([int(r.x)?, int(r.y)?, int(r.width)?, int(r.height)?])
}

/// The major and minor version of an OpenGL ES `GL_VERSION`, which reads
/// "OpenGL ES 3.2 Mesa 26.2.6"; `None` for any other string, desktop
/// OpenGL's included.
pub(super) fn es_version(version: &str) -> Option<(u32, u32)> {
    let number = version.strip_prefix("OpenGL ES ")?.split(' ').next()?;
    let (major, minor) = number.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// A GL enum as `glTexParameteri` and `glSamplerParameteri` take it.
fn param(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// The errors one check read, `errors` from `glGetError` in the order GL
/// gave them and `reset` from the reset status, as one error: a lost
/// context first, then out of memory, the two that leave the GPU path
/// (ADR-1303 D9), then the first other.
pub(super) fn sort(errors: &[u32], reset: u32) -> Result<(), Error> {
    if reset != glow::NO_ERROR || errors.contains(&CONTEXT_LOST) {
        return Err(Error::new(
            ErrorKind::ContextLost,
            format!("the context was lost (reset status {reset:#06x})"),
        ));
    }
    if errors.contains(&glow::OUT_OF_MEMORY) {
        return Err(Error::new(ErrorKind::OutOfMemory, "GL_OUT_OF_MEMORY"));
    }
    match errors.first() {
        None => Ok(()),
        Some(&e) => Err(Error::new(
            ErrorKind::Gl(e),
            format!(
                "GL error {e:#06x}{}",
                if errors.len() > 1 { " and others" } else { "" }
            ),
        )),
    }
}

fn name(v: u32) -> Result<NonZeroU32, Error> {
    NonZeroU32::new(v).ok_or_else(|| misuse("a GL object named 0"))
}

/// What a test makes the next wait find.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitFault {
    /// The fence not passed within the wait's bound.
    TimedOut,
    /// The GPU reset while the frame ran.
    Reset,
}

/// A GL context: OpenGL ES 3.0 or later on an EGL display, current on the
/// thread that made it, which it never leaves.
pub struct Context {
    gl: glow::Context,
    vao: glow::NativeVertexArray,
    instances: glow::NativeBuffer,
    /// The samplers for [`Filter::Nearest`] and [`Filter::Linear`].
    samplers: [glow::NativeSampler; 2],
    image_target: Option<ImageTargetRenderbuffer>,
    reset_status: Option<ResetStatus>,
    textures: BTreeMap<u32, TextureInfo>,
    imports: BTreeMap<u32, Import>,
    /// Whether a frame's end makes an EGL fence sync, or waits in place
    /// with `glFinish` where the display or GL cannot make one.
    fence_sync: bool,
    /// Each fence a submit gave and no wait has taken yet, by its name: an
    /// EGL fence sync, or `None` for a frame `glFinish` already finished.
    fences: BTreeMap<u32, Option<egl::Sync>>,
    /// The next wait times out, or finds the GPU reset: for the tests of
    /// what a wait gives then.
    wait_fault: Option<WaitFault>,
    /// The last fence's name.
    last_fence: u32,
    /// GL's error flags a call read and set aside for the next check, so
    /// that it read only its own; at most `ERRORS_MAX`.
    pending: Vec<u32>,
    /// An error the next call reports, which a test raised.
    raised: Cell<Option<ErrorKind>>,
    limits: Limits,
    renderer: String,
    version: String,
    /// Last, so GL's objects go before the context.
    egl: Egl,
    /// Not [`Send`]: the context is current on the thread that made it.
    _thread: PhantomData<*const ()>,
}

impl Context {
    /// A head's context on `node`, its card's render node: GBM over the
    /// node, EGL's GBM platform over that, and the context.
    pub fn on_render_node(node: &Path) -> Result<Context, SetupError> {
        let fd: OwnedFd = rustix::fs::open(node, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
            .map_err(|e| SetupError {
                step: Step::RenderNode,
                detail: format!("{}: {e}", node.display()),
            })?;
        let gbm = Gbm::new(fd).map_err(|detail| SetupError {
            step: Step::GbmDevice,
            detail,
        })?;
        Context::new(Egl::on_gbm(gbm)?)
    }

    /// A context on Mesa's llvmpipe, with no device: the tests' and tier
    /// 2's. It imports no buffer.
    pub fn software() -> Result<Context, SetupError> {
        Context::new(Egl::software()?)
    }

    fn new(egl: Egl) -> Result<Context, SetupError> {
        let fail = |step, detail: String| SetupError { step, detail };
        // glow panics on a version it cannot read, so the version is read
        // and judged first, through a glGetString of its own
        let get_string = egl
            .proc_address("glGetString")
            .ok_or_else(|| fail(Step::Version, "GL has no glGetString".to_owned()))?;
        // SAFETY: the pointer is glGetString, cast to its signature; the
        // context is current on this thread, and the string it returns, or
        // null, is GL's own.
        #[expect(unsafe_code, reason = "GL_VERSION through glGetString, before glow")]
        let version = unsafe {
            let get_string = std::mem::transmute::<extern "system" fn(), GetString>(get_string);
            let raw = get_string(glow::VERSION);
            (!raw.is_null()).then(|| CStr::from_ptr(raw).to_str().map(str::to_owned))
        };
        let version = match version {
            Some(Ok(v)) => v,
            _ => return Err(fail(Step::Version, "GL_VERSION cannot be read".to_owned())),
        };
        let (major, minor) = es_version(&version)
            .filter(|&(major, _)| major >= 3)
            .ok_or_else(|| {
                fail(
                    Step::Version,
                    format!("OpenGL ES 3.0 or later is needed, and the context is {version}"),
                )
            })?;
        // SAFETY: the context is current on this thread, and each name
        // eglGetProcAddress resolves is the GL function of that name.
        #[expect(unsafe_code, reason = "loading GL's function table")]
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                egl.proc_address(name)
                    .map_or(std::ptr::null(), |f| f as *const c_void)
            })
        };
        // SAFETY: glGetString, glGetIntegerv into a list of the length the
        // query writes, and glGetShaderPrecisionFormat, on the current
        // context.
        #[expect(unsafe_code, reason = "GL's queries of the new context")]
        let (renderer, limits) = unsafe {
            let renderer = gl.get_parameter_string(glow::RENDERER);
            let mut viewport = [0; 2];
            gl.get_parameter_i32_slice(glow::MAX_VIEWPORT_DIMS, &mut viewport);
            let [viewport_width, viewport_height] = viewport;
            let side = |v: i32| u32::try_from(v).unwrap_or(0);
            let highp_float = gl
                .get_shader_precision_format(glow::FRAGMENT_SHADER, glow::HIGH_FLOAT)
                .is_some_and(|p| p.precision > 0);
            let highp_int = gl
                .get_shader_precision_format(glow::FRAGMENT_SHADER, glow::HIGH_INT)
                .is_some_and(|p| p.range_max > 0);
            let limits = Limits {
                texture: side(gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE)),
                renderbuffer: side(gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE)),
                viewport: (side(viewport_width), side(viewport_height)),
                fragment_highp: highp_float && highp_int,
                texture_units: side(gl.get_parameter_i32(glow::MAX_TEXTURE_IMAGE_UNITS)),
            };
            (renderer, limits)
        };
        if !limits.fragment_highp {
            return Err(fail(
                Step::Highp,
                format!("{renderer} lacks highp floats or integers in fragment shaders"),
            ));
        }
        let extensions = gl.supported_extensions();
        let fence_sync = egl.fence_syncs() && extensions.contains("GL_OES_EGL_sync");
        let load = |name: &str| egl.proc_address(name);
        // SAFETY: each pointer is the function of its name, which its
        // extension declares with the signature it is cast to.
        #[expect(unsafe_code, reason = "GL extension functions loaded by name")]
        let (image_target, reset_status) = unsafe {
            let image_target = extensions
                .contains("GL_OES_EGL_image")
                .then(|| load("glEGLImageTargetRenderbufferStorageOES"))
                .flatten()
                .map(|f| std::mem::transmute::<extern "system" fn(), ImageTargetRenderbuffer>(f));
            let reset = if major > 3 || minor >= 2 {
                Some("glGetGraphicsResetStatus")
            } else if extensions.contains("GL_KHR_robustness") {
                Some("glGetGraphicsResetStatusKHR")
            } else if extensions.contains("GL_EXT_robustness") {
                Some("glGetGraphicsResetStatusEXT")
            } else {
                None
            };
            let reset_status = reset
                .and_then(load)
                .map(|f| std::mem::transmute::<extern "system" fn(), ResetStatus>(f));
            (image_target, reset_status)
        };
        // SAFETY: GL calls on the current context. The vertex array's
        // attributes read the instance buffer at offsets inside one
        // instance, and every draw fills the buffer with exactly its
        // instances first.
        #[expect(unsafe_code, reason = "GL's objects for every draw")]
        let objects = unsafe {
            (|| -> Result<_, String> {
                let vao = gl.create_vertex_array()?;
                let instances = gl.create_buffer()?;
                gl.bind_vertex_array(Some(vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(instances));
                for (location, ty, offset) in [
                    (0, glow::INT, 0),
                    (1, glow::UNSIGNED_INT, 16),
                    (2, glow::UNSIGNED_INT, 32),
                ] {
                    gl.enable_vertex_attrib_array(location);
                    gl.vertex_attrib_pointer_i32(location, 4, ty, INSTANCE_STRIDE, offset);
                    gl.vertex_attrib_divisor(location, 1);
                }
                gl.bind_vertex_array(None);
                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                // dithering may move a channel by a level where a format has
                // fewer bits; vtium's formats have 8 a channel, and no draw
                // asks for it
                gl.disable(glow::DITHER);
                let mut samplers = Vec::new();
                for filter in [glow::NEAREST, glow::LINEAR] {
                    let s = gl.create_sampler()?;
                    gl.sampler_parameter_i32(s, glow::TEXTURE_MIN_FILTER, param(filter));
                    gl.sampler_parameter_i32(s, glow::TEXTURE_MAG_FILTER, param(filter));
                    gl.sampler_parameter_i32(s, glow::TEXTURE_WRAP_S, param(glow::CLAMP_TO_EDGE));
                    gl.sampler_parameter_i32(s, glow::TEXTURE_WRAP_T, param(glow::CLAMP_TO_EDGE));
                    samplers.push(s);
                }
                let error = gl.get_error();
                if error != glow::NO_ERROR {
                    return Err(format!("GL error {error:#06x} making the draw's objects"));
                }
                Ok((vao, instances, samplers))
            })()
        };
        let (vao, instances, samplers) = objects.map_err(|e| fail(Step::Context, e))?;
        let [nearest, linear] = <[_; 2]>::try_from(samplers)
            .map_err(|_| fail(Step::Context, "two samplers".to_owned()))?;
        Ok(Context {
            gl,
            vao,
            instances,
            samplers: [nearest, linear],
            image_target,
            reset_status,
            textures: BTreeMap::new(),
            imports: BTreeMap::new(),
            fence_sync,
            fences: BTreeMap::new(),
            wait_fault: None,
            last_fence: 0,
            pending: Vec::new(),
            raised: Cell::new(None),
            limits,
            renderer,
            version,
            egl,
            _thread: PhantomData,
        })
    }

    /// The driver's name for the renderer, `GL_RENDERER`, such as
    /// "llvmpipe (LLVM 19.1.7, 256 bits)".
    pub fn renderer(&self) -> &str {
        &self.renderer
    }

    /// `GL_VERSION`.
    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Whether a frame's end makes an EGL fence sync that the render thread
    /// waits on; without one, it waits in place with `glFinish`.
    pub fn fence_syncs(&self) -> bool {
        self.fence_sync
    }

    /// Makes the context current where another is.
    fn current(&self) -> Result<(), Error> {
        if let Some(kind) = self.raised.take() {
            return Err(Error::new(kind, "raised by a test"));
        }
        self.egl
            .current()
            .map_err(|e| Error::new(ErrorKind::ContextLost, e))
    }

    // Makes the next call that reaches GL, `check` included, fail with an
    // error of `kind`, and touches no GL: for the tests of what a caller
    // does with each kind, which no real driver raises on demand.
    // vtium-render's stand-in GL raises errors the same way.
    #[doc(hidden)]
    pub fn raise(&mut self, kind: ErrorKind) {
        self.raised.set(Some(kind));
    }

    /// Reads the error flags GL holds, at most `ERRORS_MAX`. Its callers make
    /// the context current first.
    fn flags(&self) -> Vec<u32> {
        let mut errors = Vec::new();
        // SAFETY: glGetError, which takes no argument.
        #[expect(unsafe_code, reason = "glGetError")]
        unsafe {
            for _ in 0..ERRORS_MAX {
                match self.gl.get_error() {
                    glow::NO_ERROR => break,
                    e => errors.push(e),
                }
            }
        }
        errors
    }

    /// The context's reset status where the driver reports one, else
    /// `GL_NO_ERROR`.
    fn reset(&self) -> u32 {
        // SAFETY: the function of the reset status's name, which takes no
        // argument.
        #[expect(unsafe_code, reason = "glGetGraphicsResetStatus")]
        unsafe {
            self.reset_status.map_or(glow::NO_ERROR, |f| f())
        }
    }

    /// Sets aside the error flags GL holds now, which an earlier call raised,
    /// for the next check, so the call about to read its own reads no other.
    fn set_aside(&mut self) {
        let flags = self.flags();
        self.pending.extend(flags);
        self.pending.truncate(ERRORS_MAX);
    }

    /// Reads GL's errors since the last check, those a call set aside first,
    /// and the context's reset status where the driver reports one, sorted
    /// as [`sort`] sorts them.
    pub fn check(&mut self) -> Result<(), Error> {
        self.current()?;
        let mut errors = std::mem::take(&mut self.pending);
        errors.extend(self.flags());
        sort(&errors, self.reset())
    }
}

/// The errors binding an EGL image to a renderbuffer raised, as the
/// import's error: a lost context and out of memory keep their kinds, which
/// leave the GPU path, and any other error makes the import fail as
/// [`ErrorKind::Import`], which a staging copy answers (ADR-1303 D9).
pub(super) fn bound(errors: &[u32], reset: u32) -> Result<(), Error> {
    sort(errors, reset).map_err(|e| match e.kind {
        ErrorKind::Gl(_) => Error::new(
            ErrorKind::Import,
            format!("binding the buffer's image raised {}", e.detail),
        ),
        _ => e,
    })
}

impl Drop for Context {
    fn drop(&mut self) {
        for sync in std::mem::take(&mut self.fences).into_values().flatten() {
            self.egl.destroy_sync(sync);
        }
        let fbs: Vec<u32> = self.imports.keys().copied().collect();
        for fb in fbs {
            let _ = self.free_import(fb);
        }
        if self.current().is_ok() {
            // SAFETY: GL calls on the current context with no pointer.
            #[expect(unsafe_code, reason = "deleting the draw's objects")]
            unsafe {
                self.gl.delete_vertex_array(self.vao);
                self.gl.delete_buffer(self.instances);
                for s in self.samplers {
                    self.gl.delete_sampler(s);
                }
            }
        }
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("renderer", &self.renderer)
            .field("version", &self.version)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
