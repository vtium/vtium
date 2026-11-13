#![forbid(unsafe_code)]
//! `make tier1-shots`: every tier 1 golden drawn as a picture through
//! vtium's own renderer, for the maintainer's review (ADR-1505, V3).
//!
//! Tier 1's goldens are text: the screen state and replies vtium produces
//! after a stream of input bytes. The tool replays each case of the corpus
//! and of vtium's own cases through vtium-replay's code, with its test
//! commands and the fixture host, and checks the replay's output against
//! the case's goldens. It then draws the terminal the replay leaves, text
//! and pictures, in unifont at 8×16. For each behaviour scenario it draws
//! each screen the scenario dumps, in order, and checks the scenario's
//! transcript. A picture whose output differs from its golden is still
//! drawn, and the manifest and the report list the lines that differ.
//!
//! [`select`] says what is drawn, [`judge`] compares an output with its
//! golden, [`manifest`] names each picture and writes `manifest.json`, and
//! [`shoot`] replays and draws.

pub mod judge;
pub mod manifest;
// the display view and the image commands are vtium-replay's with the
// bidi and images features, which the draw feature turns on
#[cfg(feature = "draw")]
pub mod select;
#[cfg(feature = "draw")]
pub mod shoot;

use std::path::PathBuf;

/// Tier 1's directory, from the repository's root: the corpus, vtium's
/// own cases and the scenarios. The manifest names each golden by its
/// path from the root.
pub const CONFORMANCE: &str = "tools/conformance";

/// The repository's root, from this crate's manifest.
pub fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}
