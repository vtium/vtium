#![forbid(unsafe_code)]
// Fonts, sets and configuration files are untrusted input: nothing they
// hold can panic the tool.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-core: the configuration that vtium-config's command line
//! and TUI share, and the commands they call (ADR-1201, ADR-1403).
//!
//! [`drafts`] keeps the user's draft of vtium.conf, which [`ops`] edits
//! and [`store`] holds with the library ([`library`]) and the history;
//! [`batch`] keeps the active resources beside a draft; an install
//! becomes an install bundle ([`draft`], [`apply`]), and [`vts`] is the
//! step after it. [`flowmodel`] builds the [`flow`] from a draft and the
//! devices present, and [`flowcli`] holds the flow's commands. [`commands`]
//! holds the other commands, [`discover`] finds the fonts on this machine,
//! [`specimen`] starts the font helper for a frit's specimen, [`switch`]
//! switches the running console among the frits, [`report`] writes
//! `report`'s document, [`help`] holds the `--help` text, and [`banner`]
//! draws the art that `--help about` opens with.
//!
//! [`document`] is the configuration as the TUI's one open document, with
//! its undo history and the desk its draft is written through, and [`ui`]
//! holds the shared state, the action and the commands that
//! vtium-config-widgets' `Tab` trait names. [`found`], [`preview`],
//! [`fontuse`] and [`versions`] are the resources found on the machine,
//! the pictures, the fonts a configuration uses, and the document's
//! versions.
//!
//! vtium-config re-exports each module that its own code used before the
//! split at its root, so `vtium_config::drafts` and the rest keep their
//! paths. The modules for the rebuilt TUI are this crate's alone.

pub mod apply;
pub mod banner;
pub mod batch;
pub mod commands;
pub mod discover;
pub mod document;
pub mod draft;
pub mod drafts;
pub mod flow;
pub mod flowcli;
pub mod flowmodel;
pub mod fontlist;
pub mod fontuse;
pub mod found;
pub mod help;
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
pub mod ui;
pub mod versions;
pub mod vts;
pub mod words;

// The font helper's modules that this crate's code names, at the paths
// they had when the code sat in vtium-config.
pub(crate) use vtium_font_helper::{
    describe, frit, made, needs, onscreen, outline, serve, sources,
};
