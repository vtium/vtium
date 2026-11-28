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
//! vtium-config re-exports each of those modules in its `tui` module, so
//! `vtium_config::tui::canvas` and the rest keep their paths.
//!
//! The rebuilt TUI joins its shell and its six tabs through this crate's
//! seams. [`tab`] holds the `Tab` trait every tab implements and the small
//! traits a tab implements to gain a shared behaviour, [`contract`] the
//! interface tree and the key table compiled in, and [`roles`] the named
//! roles every colored element is drawn through. [`keys`], [`search`],
//! [`list_box`], [`frame`] and [`menu`] hold the shared components built
//! on those seams.

pub mod backend;
pub mod canvas;
pub mod color;
pub mod contract;
pub mod frame;
pub mod input;
pub mod keys;
pub mod list_box;
pub mod listbox;
pub mod menu;
pub mod roles;
pub mod search;
pub mod tab;
