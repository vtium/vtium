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
//! vtium-vitrify: builds vitrified font sets for vtium (ADR-1200).
//!
//! A recipe ([`recipe`]) names a set's sources; [`sources`] finds and
//! reads them as the invoking user; [`child`] parses each in an
//! unprivileged child process with [`decode`]'s decoders and hands back
//! the bounded [`glyphs`] form; [`build`] lays the glyphs into a set; and
//! [`store`] writes it into the root-owned store under its lock, or
//! `create --out` into a portable file. [`cli`] is the command line.

pub mod build;
pub mod child;
pub mod cli;
pub mod decode;
pub mod glyphs;
pub mod recipe;
pub mod sources;
pub mod store;
