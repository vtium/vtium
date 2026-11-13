//! The on-screen display, the OSD: vtium's status at the top of each head,
//! outside the session's output. It says which head is active, and under
//! `full` which console the head shows, for `osd-timeout` after a change.
//! vtium.conf's `osd` picks one of four levels, `full`, `minimal`, `off`
//! and `silent`, and each level hides everything the one before it hides.
//! `silent` also hides vtium's ordinary notices, which every other level
//! shows. vtium.conf's `focus-mark` is read as the older name of `osd` and
//! `osd-timeout`.
//!
//! References:
//! - ADR-1401 D5: the levels, when the OSD shows, and the notices that
//!   only `silent` hides.

use std::time::Duration;

use crate::config::{Config, Origin, keys};

/// What the OSD draws: vtium.conf's `osd`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// No status.
    Off,
    /// A dot in each head's top-right cell, filled on the active head.
    Minimal,
    /// A line across the top of each head ([`crate::notice::osd_line`]).
    Full,
    /// No status, and none of vtium's ordinary notices either. The panic
    /// screen, the notice of a crash loop that stopped every head, and the
    /// kernel's messages still show.
    Silent,
}

/// The OSD as vtium.conf sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Osd {
    pub mode: Mode,
    /// How long it stays after a change, from the first frame that draws
    /// it; `None` keeps it up from the start.
    pub timeout: Option<Duration>,
}

impl Osd {
    /// The OSD `config` sets: `osd` and `osd-timeout`, and where either is
    /// unset, what `focus-mark` says in its place, with a line in the log
    /// naming the new keys.
    pub fn configured(config: &Config) -> Osd {
        let set = |key| !matches!(config.origin(key), None | Some(Origin::Default));
        let old = config.text(&keys::FOCUS_MARK);
        if let Some(old) = old {
            log::warn!(
                "focus-mark is an old key: use osd and osd-timeout instead (focus-mark = {old} is read as their equivalent)"
            );
        }
        let mode = match config.text(&keys::OSD) {
            _ if old == Some("off") && !set(&keys::OSD) => Mode::Off,
            Some("off") => Mode::Off,
            Some("full") => Mode::Full,
            Some("silent") => Mode::Silent,
            _ => Mode::Minimal,
        };
        let timeout = if old == Some("always") && !set(&keys::OSD_TIMEOUT) {
            None
        } else {
            config.number(&keys::OSD_TIMEOUT).map(Duration::from_secs)
        };
        Osd { mode, timeout }
    }

    /// Whether the OSD draws status at all: the dot of `minimal` or the
    /// line of `full`, and the brief line that answers a press.
    pub fn draws_status(self) -> bool {
        matches!(self.mode, Mode::Minimal | Mode::Full)
    }

    /// Whether vtium hides its ordinary notices and the render timer's
    /// figures, under `osd = silent`.
    pub fn silent(self) -> bool {
        self.mode == Mode::Silent
    }

    /// Whether a change shows the OSD for a time: not under `off` or
    /// `silent`, which draw none, nor under `never`, which keeps it up.
    pub fn shows_on_change(self) -> bool {
        self.draws_status() && self.timeout.is_some()
    }
}

/// Whether `config` sets `osd = silent`, which a head reads to hide its
/// notices. It reads `osd` alone, since `focus-mark` never means `silent`.
pub fn silent(config: &Config) -> bool {
    config.text(&keys::OSD) == Some("silent")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::*;

    fn osd(args: &[&str]) -> Osd {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "{args:?}");
        Osd::configured(&loaded.config)
    }

    const THREE: Option<Duration> = Some(Duration::from_secs(3));

    // ADR-1401 D5: unset, the OSD is the dot for 3 s; `osd` and
    // `osd-timeout` set it.
    #[test]
    fn osd_and_osd_timeout_set_the_osd() {
        let at = |mode, timeout| Osd { mode, timeout };
        assert_eq!(osd(&[]), at(Mode::Minimal, THREE));
        assert_eq!(osd(&["--osd=full"]), at(Mode::Full, THREE));
        assert_eq!(osd(&["--osd=off"]), at(Mode::Off, THREE));
        assert_eq!(
            osd(&["--osd-timeout=10"]),
            at(Mode::Minimal, Some(Duration::from_secs(10)))
        );
        assert_eq!(osd(&["--osd-timeout=never"]), at(Mode::Minimal, None));
    }

    /// `osd = silent` is a level of its own, below `off`. `focus-mark`,
    /// the older name, never stands in for it, since `osd` is set.
    ///
    /// References:
    /// - ADR-1401 D5: the four levels, `full`, `minimal`, `off` and
    ///   `silent`.
    #[test]
    fn osd_silent_is_a_level_of_its_own() {
        let at = |mode, timeout| Osd { mode, timeout };
        assert_eq!(osd(&["--osd=silent"]), at(Mode::Silent, THREE));
        assert_eq!(
            osd(&["--focus-mark=always", "--osd=silent"]),
            at(Mode::Silent, None)
        );
    }

    // ADR-1401 D5: `focus-mark` is the older name: `auto` the defaults,
    // `always` the dot kept up and `off` nothing; `osd` and `osd-timeout`
    // win where they are set.
    #[test]
    fn focus_mark_reads_as_osd_where_osd_is_unset() {
        let at = |mode, timeout| Osd { mode, timeout };
        assert_eq!(osd(&["--focus-mark=auto"]), at(Mode::Minimal, THREE));
        assert_eq!(osd(&["--focus-mark=always"]), at(Mode::Minimal, None));
        assert_eq!(osd(&["--focus-mark=off"]), at(Mode::Off, THREE));
        assert_eq!(
            osd(&["--focus-mark=off", "--osd=full"]),
            at(Mode::Full, THREE)
        );
        assert_eq!(
            osd(&["--focus-mark=always", "--osd-timeout=5", "--osd=full"]),
            at(Mode::Full, Some(Duration::from_secs(5)))
        );
        assert_eq!(
            osd(&["--osd=minimal", "--focus-mark=off"]),
            at(Mode::Minimal, THREE),
            "a default written out is set"
        );
    }
}
