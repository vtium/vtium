#![forbid(unsafe_code)]
// Fonts and sets are untrusted input, and the parser's answer is checked
// before it is trusted: nothing either holds can panic the tool.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-vitrify: generates vtium's glyphs from frits at run time
//! (ADR-1200, ADR-1301 D2).
//!
//! A frit ([`frit`]) names its sources; [`sources`] finds and reads them;
//! [`decode`]'s decoders parse a bitmap font into the bounded [`glyphs`]
//! form, which [`font`] holds at its own cell, and [`outline`] rasterises
//! an outline font. [`serve`] is the helper the console starts, which
//! draws a frit's glyphs at a cell and sends them in the glyph chunk
//! format; [`child`] parses in an unprivileged child process. [`switch`]
//! switches the running console among the frits. [`cli`] is the command
//! line, and [`banner`] the art it opens with at a terminal.

pub mod banner;
pub mod child;
pub mod cli;
pub mod decode;
pub mod font;
pub mod frit;
pub mod glyphs;
pub mod outline;
pub mod resample;
pub mod serve;
pub mod sources;
pub mod switch;
