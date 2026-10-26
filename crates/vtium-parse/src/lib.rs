#![forbid(unsafe_code)]
//! The escape-sequence parser.
//!
//! Turns the byte stream a program writes into actions: printable
//! characters, C0/C1 controls, and complete CSI, ESC, OSC, DCS and APC
//! sequences with their parameters, sub-parameters and intermediates. A
//! [`Parser`] drives a [`Consumer`], one method per kind of action.
//!
//! ```
//! use vtium_parse::{Chunk, Consumer, Intermediates, Params, Parser};
//!
//! #[derive(Default)]
//! struct Text(String);
//!
//! impl Consumer for Text {
//!     fn print(&mut self, c: u32) {
//!         self.0.extend(char::from_u32(c));
//!     }
//!     fn execute(&mut self, _: u8) {}
//!     fn esc_dispatch(&mut self, _: Intermediates, _: u8) {}
//!     fn csi_dispatch(&mut self, _: &Params, _: Intermediates, _: u8) {}
//!     fn osc_dispatch(&mut self, _: &[u8], _: u8) {}
//!     fn dcs_hook(&mut self, _: &Params, _: Intermediates, _: u32) {}
//!     fn dcs_put(&mut self, _: Chunk<'_>) {}
//!     fn dcs_unhook(&mut self, _: u8) {}
//!     fn apc_start(&mut self) {}
//!     fn apc_put(&mut self, _: Chunk<'_>) {}
//!     fn apc_end(&mut self, _: u8) {}
//!     fn xtgettcap(&mut self, _: &[u8], _: bool) {}
//! }
//!
//! let mut parser = Parser::new();
//! let mut text = Text::default();
//! parser.advance(&mut text, b"\x1b[1mbold\x1b[m \xc3\xa9");
//! assert_eq!(text.0, "bold é");
//! ```
//!
//! The state machine is the C reference's, libtsm's `tsm-vte.c`, which
//! follows the DEC/ANSI parser described at vt100.net. It reproduces the C
//! reference's behaviour, including where that departs from the
//! vt100.net diagram: DEL prints in ground state, CAN and SUB end an OSC
//! string by dispatching it, and an ESC ends one whatever follows. Each such
//! rule is cited to its line in `tsm-vte.c` where it is implemented.
//! One rule departs from the C reference on purpose: after a reset between
//! the ESC that ended an APC string and the `\` of its ST, that `\` is
//! dropped (aaronsb/libtsm#53; see [`Parser::reset`]).
//!
//! Charset mapping, screen effects and replies are the consumer's.
//!
//! # Bounds
//!
//! The byte stream comes from an untrusted program, so nothing it sends
//! grows memory: the parser holds every buffer inline and allocates
//! nothing. Each bound is the C reference's, defined once in `src/bounds.rs`
//! with its citation and tabulated in the crate's README: [`PARAMS_MAX`],
//! [`SUBPARAMS_MAX`], [`PARAM_VALUE_CAP`], [`OSC_MAX`] and
//! [`XTGETTCAP_MAX`]. Intermediates are a set of flags, so their count takes
//! no room, and DCS and APC data are streamed to the consumer, whose bound
//! applies.

mod bounds;
mod consumer;
mod params;
mod parser;
mod utf8;

pub use bounds::{OSC_MAX, PARAM_VALUE_CAP, PARAMS_MAX, SUBPARAMS_MAX, XTGETTCAP_MAX};
pub use consumer::{Chunk, Consumer, InputMode};
pub use params::{Intermediates, Params};
pub use parser::Parser;
