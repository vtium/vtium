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
//!
//! Draws `vtium-term`'s frames into a display buffer on the CPU: each cell
//! through the drawn-glyph cache, its decorations as tinted sprites, the
//! cursor and the pointer, in any of four orientations, redrawing only the
//! cells a buffer lacks and returning them as damage. ADR-1300 records the
//! design: the C reference's flow, with glyphs cached under their text in
//! four pools, stored at 1 bit and at the font's own size, drawn uncached
//! when the cache is full, and cells of several layers composed a row at a
//! time.
//!
//! ```
//! use vtium_font::{Font, Unifont};
//! use vtium_render::{Canvas, Config, DrawState, Renderer};
//! use vtium_term::{Age, Cursor, Damage, Frame, Glyph, Line, LineId, Palette, Rgb, Style, Width};
//!
//! let mut line = Line::new(LineId::new(1), 4);
//! line.write(0, Glyph::Char('h'), Width::Narrow, &Style::DEFAULT, None);
//! let damage = Damage::new(4, 1);
//! let palette = Palette::new([Rgb::default(); 256], Rgb::new(200, 200, 200), Rgb::new(0, 0, 0));
//! let frame = Frame::new(vec![&line], &damage, &palette, Age::NEVER, Cursor::default(), false);
//!
//! let mut r = Renderer::new(Font::new(Unifont::new(16)), 32, 16, Config::default());
//! let mut canvas = Canvas::new(32, 16, 0);
//! let drawn = r.draw(&frame, &mut canvas, &DrawState::default());
//! assert_eq!(drawn.damage.len(), 1);
//! assert_eq!(drawn.stats.cells, 4);
//! ```
//!
//! The display buffer is a [`Target`], which only takes runs of pixels:
//! device memory is never read. A [`Canvas`] keeps them in memory, for
//! tests and tier 2; a [`Mapped`] writes them into a mapped dumb buffer.
//!
//! # Bounds
//!
//! Each bound is defined once in `src/bounds.rs` and tabulated in the
//! crate's README: [`POOL_BYTES_PERFORMANCE`], [`ROW_SCRATCH_MAX`] and
//! [`RENDER_TIMER_SAMPLES`].

mod blend;
mod bounds;
mod cache;
mod compose;
mod damage;
mod deco;
mod renderer;
mod stored;
mod target;
mod timer;
mod trig;

pub use bounds::{POOL_BYTES_PERFORMANCE, RENDER_TIMER_SAMPLES, ROW_SCRATCH_MAX};
pub use cache::{Budgets, CacheCounts, Profile, Storage};
pub use renderer::{Config, DecoColor, DecorationColors, DrawState, Drawn, Renderer, Stats};
pub use target::{Canvas, Mapped, Orientation, Rect, Target};
pub use timer::{RenderTimer, Report};
