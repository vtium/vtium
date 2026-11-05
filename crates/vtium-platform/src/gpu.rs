//! The GPU path's platform half, behind the `gpu` feature (ADR-1303 D7,
//! D8, D9).
//!
//! vtium-platform holds every unsafe call the GPU path makes, to GL, EGL and
//! GBM. It exposes [`Context`], a safe GL type: a GL context and the calls
//! vtium's renderer needs of it, each of which checks what its unsafe call
//! needs to be sound by itself, with no help from vtium-render's checks
//! (vtium-platform builds without vtium-render). The binary implements
//! vtium-render's `Gl` trait over it.
//!
//! - [`Context::on_render_node`] sets a head's context up on its card's
//!   render node, in the steps ADR-1303 D9 orders: the render node, the GBM
//!   device, the EGL display and its extensions, the context and its
//!   version, and `highp` in fragment shaders. The first step that fails
//!   ends the attempt, and [`SetupError`] names it.
//! - [`Context::software`] sets a context up on Mesa's llvmpipe, with no
//!   device and no GBM, for the tests and tier 2.
//! - [`Gbm`] is a GBM device, from which a display allocates the buffers it
//!   shows ([`Bo`]) and exports each as a [`DmaBuf`](crate::display::DmaBuf),
//!   which the context imports ([`Context::import`]).
//! - [`render_node`] finds a card's render node.
//!
//! libEGL and libgbm are opened when a context or a GBM device is made,
//! never linked, so a build with `gpu` starts on a machine without them.

mod context;
mod egl;
mod gbm;

use std::fmt;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub use context::{
    Blend, Context, DrawCall, Filter, Instance, Limits, Location, TextureFormat, Uniform,
};
pub use gbm::{Bo, Gbm, Usage};

/// Ends the process at once with `status`, as `_exit(2)` does: no exit
/// handler and no library destructor runs, and the kernel closes what the
/// process holds. vtium ends this way while a render thread it went on
/// without may still be inside the GPU driver (ADR-1303 D9), since libEGL's
/// and Mesa's exit handlers would tear EGL down under that thread. The
/// caller flushes its output first.
pub fn exit_now(status: u8) -> ! {
    // SAFETY: _exit ends the process with any status, and reads no memory
    // of the program.
    #[expect(unsafe_code, reason = "_exit(2), which std does not offer")]
    unsafe {
        libc::_exit(i32::from(status))
    }
}

/// A step of the GPU path's set-up, in the order ADR-1303 D9 runs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Opening the card's render node.
    RenderNode,
    /// Opening libgbm and making the GBM device.
    GbmDevice,
    /// Opening libEGL, and getting and initializing the EGL display.
    EglDisplay,
    /// The extensions the display must have.
    EglExtensions,
    /// Making the context and making it current.
    Context,
    /// OpenGL ES 3.0 or later.
    Version,
    /// `highp` floats and integers in fragment shaders.
    Highp,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Step::RenderNode => "the render node",
            Step::GbmDevice => "the GBM device",
            Step::EglDisplay => "the EGL display",
            Step::EglExtensions => "the EGL display's extensions",
            Step::Context => "the GL context",
            Step::Version => "the OpenGL ES version",
            Step::Highp => "highp in fragment shaders",
        })
    }
}

/// The step at which setting a context up failed, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupError {
    pub step: Step,
    pub detail: String,
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the GPU path's set-up failed at {}: {}",
            self.step, self.detail
        )
    }
}

impl std::error::Error for SetupError {}

/// What went wrong in a [`Context`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// `GL_OUT_OF_MEMORY`. GL's state is undefined afterwards.
    OutOfMemory,
    /// The context was lost, through `GL_CONTEXT_LOST` or a GPU reset, or
    /// cannot be made current any more.
    ContextLost,
    /// Another error GL reported, with its code, or a framebuffer GL calls
    /// incomplete, with its status.
    Gl(u32),
    /// A shader that did not compile or a program that did not link.
    Compile,
    /// A buffer the context could not import.
    Import,
    /// A call whose arguments fail the type's own checks, which reaches no
    /// GL: a slice shorter than the pixels the call names, a size GL's
    /// integers do not hold, a name of 0.
    Misuse,
}

/// An error from a [`Context`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    /// What happened, for the log.
    pub detail: String,
}

impl Error {
    fn new(kind: ErrorKind, detail: impl Into<String>) -> Error {
        Error {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for Error {}

/// The render node of the card at `card`, such as `/dev/dri/card0`: the
/// `renderD*` node sysfs lists beside the card's device. The card is not
/// opened, so finding its node takes no DRM master.
pub fn render_node(card: &Path) -> io::Result<PathBuf> {
    let rdev = std::fs::metadata(card)?.rdev();
    let (major, minor) = (rustix::fs::major(rdev), rustix::fs::minor(rdev));
    let drm = PathBuf::from(format!("/sys/dev/char/{major}:{minor}/device/drm"));
    for entry in std::fs::read_dir(&drm)? {
        let name = entry?.file_name();
        if name.to_string_lossy().starts_with("renderD") {
            return Ok(Path::new("/dev/dri").join(name));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} has no render node", card.display()),
    ))
}

#[cfg(test)]
mod tests;
