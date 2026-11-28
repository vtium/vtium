#![forbid(unsafe_code)]
// The terminal's replies are untrusted input, and nothing the decoder or
// the helpers are given can panic the tool.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-widgets: the toolkit that vtium-config's TUI draws with.
//!
//! [`backend`] is the Backend through which ratatui draws, which turns
//! ratatui's cells into the bytes the TUI's loop writes. [`canvas`] holds
//! the rows of styled text a screen is written in and the helpers that fit
//! text to them, [`color`] their colors, [`listbox`] the shared list, and
//! [`input`] the decoder of what the terminal sends.
//!
//! vtium-config re-exports each module in its `tui` module, so
//! `vtium_config::tui::canvas` and the rest keep their paths.

pub mod backend;
pub mod canvas;
pub mod color;
pub mod input;
pub mod listbox;
