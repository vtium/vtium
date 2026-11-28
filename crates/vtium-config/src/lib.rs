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
//! and [`hex`] a `.hex` font into it, and [`outline`] rasterises an outline font. [`serve`] is the helper the console starts, which
//! draws a frit's glyphs at a cell and sends them in the glyph chunk
//! format, under the seccomp filters of [`seccomp`]; [`child`] is the parser's child process. [`switch`]
//! switches the running console among the frits. [`cli`] is the command
//! line, and [`banner`] the art it opens with at a terminal.

pub mod apply;
pub mod banner;
pub mod batch;
pub mod cli;
pub mod commands;
pub mod discover;
pub mod display;
pub mod draft;
pub mod drafts;
pub mod flow;
pub mod flowcli;
pub mod flowmodel;
pub mod fontlist;
pub mod help;
pub mod kitty;
pub mod library;
pub mod ops;
pub mod options;
pub mod outcome;
pub mod places;
pub mod preview;
pub mod report;
#[cfg(test)]
mod scratch;
pub mod shell;
pub mod specimen;
pub mod store;
pub mod switch;
pub mod tui;
pub mod vts;
pub mod words;

// The font helper's modules, which vtium-font-helper holds, at the paths
// they had in this crate.
pub use vtium_font_helper::{
    bdf, child, cover, decode, describe, font, frit, glyphs, hex, made, names, needs, onscreen,
    outline, pointer_svg, resample, seccomp, serve, shaping, sources, xcursor,
};
