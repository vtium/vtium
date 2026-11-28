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
//! vtium-font-helper: reads vtium's frits and their fonts, and generates
//! vtium's glyphs from them at run time (ADR-1200, ADR-1301 D2).
//!
//! A frit ([`frit`]) names its sources, each limited by `cover=` to a
//! range or a named set of [`cover`]; [`sources`] finds and reads them;
//! [`decode`]'s decoders parse a bitmap font into the bounded [`glyphs`]
//! form, which [`font`] holds at its own cell, as [`bdf`] reads a BDF font
//! and [`hex`] a `.hex` font into it, and [`outline`] rasterises an outline font. [`serve`] is the helper the console starts, which
//! draws a frit's glyphs at a cell and sends them in the glyph chunk
//! format, under the seccomp filters of [`seccomp`]; [`child`] is the parser's child process.
//!
//! vtium-config re-exports each module at its root, and its binary runs
//! the helper as `vtium-config serve`.

/// A message made once from `format!`'s arguments and kept for the rest of
/// the run, for a reader whose errors are `&'static str` and whose message
/// names a limit's number.
macro_rules! static_message {
    ($($arg:tt)*) => {{
        static MESSAGE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        MESSAGE.get_or_init(|| format!($($arg)*)).as_str()
    }};
}

pub mod bdf;
pub mod child;
pub mod cover;
pub mod decode;
pub mod describe;
pub mod font;
pub mod frit;
pub mod glyphs;
pub mod hex;
pub mod made;
pub mod names;
pub mod needs;
pub mod onscreen;
pub mod outline;
pub mod pointer_svg;
pub mod resample;
#[cfg(test)]
mod scratch;
pub mod seccomp;
pub mod serve;
pub mod shaping;
pub mod sources;
pub mod xcursor;
