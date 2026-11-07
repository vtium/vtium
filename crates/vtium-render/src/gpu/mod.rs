//! The GPU path, behind the `gpu` feature (ADR-1303).
//!
//! The renderer draws through the safe [`Gl`] trait and never calls GL
//! itself: vtium-render forbids unsafe code, and every unsafe call to GL,
//! EGL and GBM lives in vtium-platform. The binary implements [`Gl`] with an
//! adapter over vtium-platform's GL type, and checks each call through a
//! [`Ledger`], as [`StandIn`], the tests' GL, does.
//!
//! A display's frame passes through the [`Pipeline`]'s five slots: pictures
//! at another size, composition into the upright frame, color, spatial
//! effects, and present into the display's buffer. The renderer decides
//! what each frame draws, as on the CPU path, and draws it through the
//! pipeline with `Renderer::draw_gpu`.

pub(crate) mod compose;
mod gl;
pub(crate) mod glyphs;
mod ink;
mod ledger;
mod model;
#[cfg(feature = "images")]
pub(crate) mod pictures;
mod pipeline;
mod stand_in;
mod tile;
pub(crate) mod turn;

pub use compose::{ArrowAt, Work};
pub use gl::{
    Batch, Blend, ContextId, Draw, DrawTarget, Error, ErrorKind, Fence, Filter, Format, Gl, Input,
    InputKind, Limits, Program, Quad, Size, Source, Texture, Value,
};
pub use ledger::{Checked, Ledger, Resolved, TargetInfo, TextureInfo};
#[cfg(feature = "images")]
pub use pictures::PictureStep;
pub use pipeline::{Color, Config, Pipeline, Spatial};
pub use stand_in::{Call, Op, Seen, StandIn};
pub use tile::{Tile, tiles};
