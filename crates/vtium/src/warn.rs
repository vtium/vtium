//! Where the operator's warnings go: what the configuration could not
//! take, and what a font lacks. The log, on stderr, for now: how the
//! operator sees them is undecided (vtium/vtium#21, #85), and this module
//! is the one place a decision changes.

use std::fmt::Display;

use crate::config::Report;

/// One warning for the operator.
pub fn warn(text: impl Display) {
    log::warn!(target: "config", "{text}");
}

/// Each of the configuration's reports.
pub fn reports(reports: &[Report]) {
    for r in reports {
        warn(r);
    }
}
