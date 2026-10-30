#![forbid(unsafe_code)]
// The head file is untrusted input as far as its parser goes (ADR-1401
// D10): nothing in it can panic the console that reads it.
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
//! The head file of ADR-1401: `/etc/vtium/heads.conf`, its heads, its
//! first-match ruleset and its guards.
//!
//! [`read`] takes the file from disk with its checks (a regular file owned
//! by root and writable by no one else, in directories that pass the same
//! check, within [`FILE_MAX`] bytes, UTF-8, with no control character but
//! tab and newline). [`parse`] turns its text into a [`Config`] or rejects
//! it whole, naming every error with its line. [`Config::walk`] places one
//! device by the first rule that matches it, reading nothing but the
//! device's [`Identity`] and the rules, and [`Config::place`] places a set
//! of devices and runs the guards over the result. [`Config::builtin`] is
//! the compiled-in configuration, the parse of [`TEMPLATE`].
//!
//! ```
//! use vtium_config::{Class, Identity, Place, parse};
//!
//! let text = "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> focus\n";
//! let config = parse(text).map(|p| p.config).unwrap_or_default();
//! let keyboard = Identity::input().with_class(Class::Keyboard);
//! assert_eq!(config.walk(&keyboard).place, Place::Focus);
//! ```

mod bounds;
mod cmdline;
mod config;
mod error;
mod file;
mod identity;
mod matching;
mod parse;
mod place;

pub use bounds::{
    CONSOLES_MAX, ERRORS_MAX, FILE_MAX, FONT_SIZE_MAX, HEADS_MAX, LINE_MAX, NAME_MAX, QUOTE_MAX,
    RULES_MAX, TOKENS_MAX,
};
pub use cmdline::{HeadsWord, heads_word};
pub use config::{Config, Font, Guards, Head, OnLoss, Rule, TEMPLATE, Target};
pub use error::{At, Error, Note, Problem, Quoted, Rejected, Warning, at};
pub use file::{HEAD_FILE, Inspected, Trust, Untrusted, inspect, read, text};
pub use identity::{Class, Classes, Edid, Identity, Kind, UsbSerial};
pub use matching::{Match, Token, glob};
pub use parse::{Parsed, parse};
pub use place::{By, Disposition, Lockout, Place, Placed, Placement, rule_for};
