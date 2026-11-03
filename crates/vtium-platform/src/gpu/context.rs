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

use std::collections::BTreeMap;
use std::ffi::c_void;
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
    imports: BTreeMap<u32, Import>,
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
        let (renderer, version, limits) = unsafe {
            let renderer = gl.get_parameter_string(glow::RENDERER);
            let version = gl.get_parameter_string(glow::VERSION);
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
            (renderer, version, limits)
        };
        let v = gl.version();
        if !v.is_embedded || v.major < 3 {
            return Err(fail(
                Step::Version,
                format!("OpenGL ES 3.0 or later is needed, and the context is {version}"),
            ));
        }
        if !limits.fragment_highp {
            return Err(fail(
                Step::Highp,
                format!("{renderer} lacks highp floats or integers in fragment shaders"),
            ));
        }
        let extensions = gl.supported_extensions();
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
            let reset = if v.major > 3 || v.minor >= 2 {
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
            imports: BTreeMap::new(),
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

    /// Makes the context current where another is.
    fn current(&self) -> Result<(), Error> {
        self.egl
            .current()
            .map_err(|e| Error::new(ErrorKind::ContextLost, e))
    }

    /// Reads GL's errors since the last check, every flag GL holds, and the
    /// context's reset status where the driver reports one, sorted as
    /// [`sort`] sorts them.
    pub fn check(&mut self) -> Result<(), Error> {
        self.current()?;
        // SAFETY: GL's error and reset queries on the current context; the
        // reset query takes no argument.
        #[expect(unsafe_code, reason = "glGetError and glGetGraphicsResetStatus")]
        let (errors, reset) = unsafe {
            let mut errors = Vec::new();
            for _ in 0..ERRORS_MAX {
                match self.gl.get_error() {
                    glow::NO_ERROR => break,
                    e => errors.push(e),
                }
            }
            let reset = self.reset_status.map_or(glow::NO_ERROR, |f| f());
            (errors, reset)
        };
        sort(&errors, reset)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
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
