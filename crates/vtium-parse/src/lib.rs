#![forbid(unsafe_code)]
// Determinism under hostile input (docs/architecture.md, "Determinism"):
// nothing a program sends can panic the parser.
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
//! The escape-sequence parser.
//!
//! Turns the byte stream a program writes into actions: printable
//! characters, C0/C1 controls, and complete CSI, ESC, OSC, DCS and APC
//! sequences with their parameters, sub-parameters and intermediates. A
//! [`Parser`] drives a [`Consumer`], one method per kind of action.
//!
//! ```
//! use vtium_parse::{Chunk, Consumer, Intermediates, Params, Parser, StringEnd};
//!
//! #[derive(Default)]
//! struct Text(String);
//!
//! impl Consumer for Text {
//!     fn print(&mut self, c: char) {
//!         self.0.push(c);
//!     }
//!     fn execute(&mut self, _: u8) {}
//!     fn esc_dispatch(&mut self, _: Intermediates, _: u8) {}
//!     fn csi_dispatch(&mut self, _: &Params, _: Intermediates, _: u8) {}
//!     fn osc_end(&mut self, _: &[u8], _: StringEnd) {}
//!     fn dcs_hook(&mut self, _: &Params, _: Intermediates, _: u8) {}
//!     fn dcs_put(&mut self, _: Chunk<'_>) {}
//!     fn dcs_unhook(&mut self, _: StringEnd) {}
//!     fn apc_start(&mut self) {}
//!     fn apc_put(&mut self, _: Chunk<'_>) {}
//!     fn apc_end(&mut self, _: StringEnd) {}
//!     fn xtgettcap(&mut self, _: Option<&[u8]>) {}
//! }
//!
//! let mut parser = Parser::new();
//! let mut text = Text::default();
//! parser.advance(&mut text, b"\x1b[1mbold\x1b[m \xc3\xa9");
//! assert_eq!(text.0, "bold é");
//! ```
//!
//! The state machine is the C reference's, libtsm's `tsm-vte.c`, which
//! follows the DEC/ANSI parser described at vt100.net. The C reference is
//! the floor: the parser does everything it does, each rule cited to its
//! line in `tsm-vte.c`, and where the C reference departs from a protocol
//! specification, the parser follows the specification. The crate's README
//! lists each departure with its reason and the specification it follows:
//! among them, UTF-8 decoding to the Unicode Standard, strings that act only
//! when ST or BEL completes them, and every empty parameter omitted.
//!
//! Charset mapping, screen effects and replies are the consumer's.
//!
//! # Bounds
//!
//! The byte stream comes from an untrusted program, so nothing it sends
//! grows memory: the parser holds every buffer inline and allocates
//! nothing. Each bound is defined once in `src/bounds.rs` with its
//! citation and tabulated in the crate's README: [`PARAMS_MAX`],
//! [`SUBPARAMS_MAX`], [`PARAM_VALUE_CAP`], [`OSC_MAX`], [`XTGETTCAP_MAX`]
//! and [`INTERMEDIATES_MAX`]. DCS and APC data are streamed to the
//! consumer, whose bound applies.

mod bounds;
mod consumer;
mod params;
mod parser;
mod utf8;

pub use bounds::{
    INTERMEDIATES_MAX, OSC_MAX, PARAM_VALUE_CAP, PARAMS_MAX, SUBPARAMS_MAX, XTGETTCAP_MAX,
};
pub use consumer::{Chunk, Consumer, InputMode, StringEnd};
pub use params::{Intermediates, Params};
pub use parser::Parser;
