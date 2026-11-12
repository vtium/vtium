#![forbid(unsafe_code)]
// vtium.conf is untrusted input as far as its parser goes (ADR-1401
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
//! vtium.conf as one file holding the whole flow of ADR-1403, its top
//! level and its sections: the heads of ADR-1401, their looks, the policy
//! and the first-match ruleset.
//!
//! [`split`] divides vtium.conf at its first section heading into the top
//! level, vtium's own keys, and the sections, which [`sections`] reads the
//! strict way of ADR-1401 D7, `[look NAME]` among them, with the
//! whole file's line numbers. [`complete_route`] says whether a
//! configuration wires a display present to a head a keyboard present
//! reaches. [`set_key`], [`add_section`], [`remove_section`],
//! [`rename_section`], [`add_rule`], [`set_rule`], [`move_rule`] and
//! [`remove_rule`] are the section writer, and
//! [`placement`] says where a new rule goes.
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
//! [`Config::walk_on`] and [`Config::place_on`] do the same for the vtium
//! on one VT, where a head runs only on the VTs its `vt` names.
//! [`Config::builtin`] is the compiled-in configuration, the parse of
//! [`TEMPLATE`]. [`resolve`] decides what runs for each state the file can
//! be in: missing, untrusted, valid, partly broken or broken, and
//! [`install_file`] replaces a configuration file in one rename. [`edit_conf`] edits vtium.conf's top level line by
//! line, keeping every byte a change does not need (ADR-1201 D5), and
//! [`key_line`] reads one top-level line by vtium.conf's one comment rule. [`keys`] is the table of every key vtium.conf and the
//! console's command line take, with its type, default, bound and
//! description, which the console reads its configuration by and
//! vtium-config shows (ADR-1201 D1).
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
mod conf;
mod config;
mod error;
mod file;
mod findings;
mod identity;
pub mod keys;
mod matching;
mod messages;
mod outcome;
mod parse;
mod place;
mod route;
mod split;
mod write;

pub use bounds::{
    CONSOLES_MAX, CONSOLES_TOTAL_DEFAULT, ERRORS_MAX, FILE_MAX, HEAD_CONSOLES_DEFAULT, HEADS_MAX,
    LINE_MAX, LOOKS_MAX, NAME_MAX, QUOTE_MAX, RULES_MAX, TOKENS_MAX, VT_MAX,
};
pub use conf::{ConfEdit, Edit, KeyLine, edit_conf, key_line, setting, switch};
pub use config::{
    Config, Head, LABEL_MAX, Label, Look, Rule, Setting, TEMPLATE, Target, WhenDisplayLost,
    value_for,
};
pub use error::{
    At, AtStart, Drops, Error, Fate, Mend, Note, PolicyFault, Problem, Quoted, Reason, Refusal,
    Rejected, Remark, Renamed, Shown, Stating, Syntax, Warning, at,
};
pub use file::{CONF_FILE, Inspected, Trust, Untrusted, inspect, read, text};
pub use findings::{Against, remarks, warnings};
pub use identity::{Class, Classes, Edid, Identity, Kind, UsbSerial};
pub use matching::{Match, Token, glob};
pub use outcome::{
    DisplayWait, Fallback, Kept, Line, Lit, Outcome, Policy, PolicyLine, Read, Rescue, Rest, Runs,
    Scope, Stated, Switch, SwitchKey, Trusted, Unplaced, Within, resolve, scope,
};
pub use parse::{Parsed, Reading, escape_label, parse, parse_sections, reading, sections};
pub use place::{By, Disposition, Lockout, Place, Placed, Placement, rule_for};
pub use route::{Judged, Missing, Route, complete_route};
pub use split::{Split, split};
pub use write::{
    Of, SectionEdit, add_rule, add_section, install_file, move_rule, placement, remove_rule,
    remove_section, rename_section, rule_lines, set_key, set_rule,
};
