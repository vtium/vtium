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
//! The head file of ADR-1401: `/etc/vtium/heads.conf`, its policy, its
//! heads and its first-match ruleset.
//!
//! [`inspect`] and [`read`] take the file from disk with the ownership
//! check (a regular file owned by root and writable by no one else, in
//! directories that pass the same check, read to at most [`FILE_MAX`]
//! bytes). [`reading`] reads its bytes line by line, each error dropping
//! what [`scope`] says it takes, and gives what survives, every `policy`
//! line and the scopes, which [`Reading::trusted`] classifies for
//! [`resolve`]. [`parse`] is the strict reading, a [`Config`] only for a
//! file with no error, naming every error with its line. [`Config::walk`]
//! places one device by the first rule that matches it, reading nothing
//! but the device's [`Identity`] and the rules, and [`Config::place`]
//! places a set of devices and runs the guards over the result.
//! [`Config::builtin`] is the compiled-in configuration, the parse of
//! [`TEMPLATE`]. [`resolve`] decides what runs for each state the file can
//! be in: missing, untrusted, valid, partly broken or broken.
//!
//! ```
//! use vtium_settings::{Class, Identity, Place, parse};
//!
//! let text = "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";
//! let config = parse(text).map(|p| p.config).unwrap_or_default();
//! let keyboard = Identity::input().with_class(Class::Keyboard);
//! assert_eq!(config.walk(&keyboard).place, Place::Roaming);
//! ```

mod bounds;
mod config;
mod error;
mod file;
mod findings;
mod identity;
mod matching;
mod messages;
mod outcome;
mod parse;
mod place;

pub use bounds::{
    CONSOLES_MAX, CONSOLES_TOTAL_DEFAULT, ERRORS_MAX, FILE_MAX, HEAD_CONSOLES_DEFAULT, HEADS_MAX,
    LINE_MAX, NAME_MAX, QUOTE_MAX, RULES_MAX, TOKENS_MAX,
};
pub use config::{Config, Font, Head, Rule, TEMPLATE, Target, WhenDisplayLost};
pub use error::{
    At, AtStart, Drops, Error, Fate, Note, PolicyFault, Problem, Quoted, Rejected, Remark, Renamed,
    Shown, Stating, Syntax, Warning, at,
};
pub use file::{HEAD_FILE, Inspected, Trust, Untrusted, inspect, read, text};
pub use findings::{Against, remarks, warnings};
pub use identity::{Class, Classes, Edid, Identity, Kind, UsbSerial};
pub use matching::{Match, Token, glob};
pub use outcome::{
    DisplayWait, Fallback, Kept, Line, Lit, Outcome, Policy, PolicyLine, Read, Rescue, Rest, Runs,
    Scope, Stated, Switch, SwitchKey, Trusted, Unplaced, Within, resolve, scope,
};
pub use parse::{Parsed, Reading, parse, reading};
pub use place::{By, Disposition, Lockout, Place, Placed, Placement, rule_for};
