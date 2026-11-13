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
//! vtium-config: generates vtium's glyphs from frits at run time
//! (ADR-1200, ADR-1301 D2).
//!
//! A frit ([`frit`]) names its sources, each limited by `cover=` to a
//! range or a named set of [`cover`]; [`sources`] finds and reads them;
//! [`decode`]'s decoders parse a bitmap font into the bounded [`glyphs`]
//! form, which [`font`] holds at its own cell, as [`bdf`] reads a BDF font
//! into it, and [`outline`] rasterises an outline font. [`serve`] is the helper the console starts, which
//! draws a frit's glyphs at a cell and sends them in the glyph chunk
//! format; [`child`] is the parser's child process. [`switch`]
//! switches the running console among the frits. [`cli`] is the command
//! line, and [`banner`] the art it opens with at a terminal.

/// A message made once from `format!`'s arguments and kept for the rest of
/// the run, for a reader whose errors are `&'static str` and whose message
/// names a limit's number.
macro_rules! static_message {
    ($($arg:tt)*) => {{
        static MESSAGE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        MESSAGE.get_or_init(|| format!($($arg)*)).as_str()
    }};
}

pub mod apply;
pub mod banner;
pub mod bdf;
pub mod child;
pub mod cli;
pub mod commands;
pub mod cover;
pub mod decode;
pub mod display;
pub mod draft;
pub mod drafts;
pub mod flowcli;
pub mod flowmodel;
pub mod font;
pub mod frit;
pub mod glyphs;
pub mod help;
pub mod kitty;
pub mod library;
pub mod made;
pub mod names;
pub mod ops;
pub mod outcome;
pub mod outline;
pub mod preview;
pub mod report;
pub mod resample;
#[cfg(test)]
mod scratch;
pub mod serve;
pub mod shaping;
pub mod sources;
pub mod specimen;
pub mod store;
pub mod switch;
pub mod tui;
pub mod vts;
