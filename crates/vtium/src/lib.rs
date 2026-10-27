#![forbid(unsafe_code)]
// Determinism under hostile input (docs/architecture.md, "Determinism"):
// a configuration file is untrusted input, and nothing in one can panic
// the console.
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
//! The `vtium` binary's logic, apart from the wiring: the configuration
//! and command line, the terminfo entry and the XTGETTCAP table generated
//! from it, the login child's command and environment, the respawn policy,
//! and the rules the terminal wiring follows (the blink timer, the font
//! fallback and the pointer guard).
//!
//! Each is a function of what it is given, time included, so a unit test
//! drives it without a VT, a display or a pty. The crate's README lists
//! where vtium departs from the C reference and the bounds it sets.

pub mod app;
pub mod blink;
pub mod check;
pub mod child;
pub mod config;
pub mod font;
pub mod grab;
pub mod hook;
pub mod layout;
pub mod logger;
pub mod pointer;
pub mod respawn;
pub mod start;
pub mod sync;
pub mod term;
pub mod termcap;
pub mod video;
pub mod warn;
