#![forbid(unsafe_code)]
// What a frame draws comes from a program (docs/architecture.md,
// "Determinism"): nothing it writes can panic the renderer.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! Rendering.

mod blend;
mod bounds;
mod cache;
mod deco;
mod renderer;
mod stored;
mod target;
mod trig;

pub use bounds::{POOL_BYTES_PERFORMANCE, ROW_SCRATCH_MAX};
pub use cache::{Budgets, CacheCounts, Profile, Storage};
pub use renderer::{Config, DecoColor, DecorationColors, DrawState, Drawn, Renderer, Stats};
pub use target::{Canvas, Mapped, Orientation, Rect, Target};
