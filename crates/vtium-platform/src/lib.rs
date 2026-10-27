// The only crate that may use `unsafe` (docs/architecture.md): each site
// carries an `#[expect(unsafe_code, reason = "...")]` and a SAFETY comment.
#![deny(unsafe_code)]
// Determinism under hostile input (docs/architecture.md, "Determinism"):
// the pty's output comes from an untrusted program, and nothing it sends
// can panic the loop that reads it.
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
//! The platform layer.
//!
//! Everything that touches the kernel and the session: DRM/KMS and fbdev
//! displays, the virtual terminal (VT_PROCESS switching, keyboard mode),
//! seats and sessions (logind, libseat), keyboard input through
//! xkbcommon, the pty and the child process, and the event loop that
//! serves them without ever blocking.
//!
//! Syscalls go through rustix, so descriptors are owned values and errors
//! are `Result`s, and DRM's ioctls through `drm-ffi`. The pty, the event
//! loop and the displays are here:
//!
//! - [`event_loop::Loop`]: calloop, with one signalfd for every signal the
//!   console handles. The display, VT, udev and input sources go in
//!   through [`event_loop::Loop::handle`].
//! - [`pty::Pty`]: the pty, its child, the window size and the queue of
//!   what the child has not read, which is bounded for replies and
//!   coalesces pointer motion. The child's output reaches a
//!   [`pty::Consumer`] in parts it chooses, and what the consumer sees and
//!   replies depends only on the bytes, never on how reads split them.
//! - [`display`]: a DRM card drawn into dumb buffers ([`display::Drm2d`])
//!   and fbdev ([`display::Fbdev`]), each lending its back buffer to the
//!   compositor and presenting it with the frame's damage, paced by page
//!   flips, and the DPMS blank ([`display::Blank`]).
//!
//! Each bound is defined once in `src/bounds.rs` and tabulated in the
//! crate's README.

mod bounds;
mod channel;
pub mod display;
pub mod event_loop;
mod ioctl;
mod mmap;
pub mod pty;
mod queue;

pub use bounds::{
    DAMAGE_CLIPS_MAX, EXIT_FEED_MAX, FLIP_WAIT_MS, MOTION_COALESCE_AT, READ_MAX, READS_PER_TURN,
    REPLY_HIGH, REPLY_LOW, REPLY_MAX, WAKE_RETRIES, WAKE_RETRY_MS,
};
pub use calloop;
