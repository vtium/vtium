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
//! reference's behaviour exactly, including where that departs from the
//! vt100.net diagram: DEL prints in ground state, CAN and SUB end an OSC
//! string by dispatching it, and an ESC ends one whatever follows. Each such
//! rule is cited to its line in `tsm-vte.c` where it is implemented.
//!
//! Charset mapping, screen effects and replies are the consumer's.
//!
//! # Bounds
//!
//! The byte stream comes from an untrusted program, so nothing it sends
//! grows memory: the parser holds every buffer inline and allocates
//! nothing. Each bound is the C reference's.
//!
//! | What | Bound | Past it |
//! |---|---|---|
//! | Parameters | [`PARAMS_MAX`], 16 | dropped; the sequence dispatches |
//! | Sub-parameters of one parameter | [`SUBPARAMS_MAX`], 8 | dropped, digits too, until the next `;` |
//! | A parameter's value | takes no digit once past 65535 | the digits are dropped |
//! | Intermediates and markers | a set of flags | repeats collapse; unknown ones set [`Intermediates::OTHER`] |
//! | An OSC string | [`OSC_MAX`], 127 bytes | a character that does not fit is dropped |
//! | An XTGETTCAP payload | [`XTGETTCAP_MAX`], 1039 bytes | the rest is dropped and the request flagged |
//! | DCS and APC data | not kept: streamed to the consumer | the consumer's bound |

mod consumer;
mod params;
mod parser;
mod utf8;

pub use consumer::{Chunk, Consumer, InputMode};
pub use params::{Intermediates, PARAMS_MAX, Params, SUBPARAMS_MAX};
pub use parser::{OSC_MAX, Parser, XTGETTCAP_MAX};
