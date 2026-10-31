//! The on-screen display, the OSD (ADR-1401 D5): vtium's status at the top
//! of each head, outside the session's output. It says which head is
//! active, and in full which console the head shows, for `osd-timeout`
//! after a change. kmscon.conf's `focus-mark` is read as the older name of
//! `osd` and `osd-timeout`.

use std::time::Duration;

use crate::config::{Config, Origin, keys};

/// What the OSD draws: kmscon.conf's `osd`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// No status.
    Off,
    /// A dot in each head's top-right cell, filled on the active head.
    Minimal,
    /// A line across the top of each head ([`crate::notice::osd_line`]).
    Full,
}

/// The OSD as kmscon.conf sets it.
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
                "focus-mark = {old} is the older name of osd and osd-timeout, which replace it"
            );
        }
        let mode = match config.text(&keys::OSD) {
            _ if old == Some("off") && !set(&keys::OSD) => Mode::Off,
            Some("off") => Mode::Off,
            Some("full") => Mode::Full,
            _ => Mode::Minimal,
        };
        let timeout = if old == Some("always") && !set(&keys::OSD_TIMEOUT) {
            None
        } else {
            config.number(&keys::OSD_TIMEOUT).map(Duration::from_secs)
        };
        Osd { mode, timeout }
    }

    /// Whether a change shows the OSD for a time: not under `off`, which
    /// draws none, nor under `never`, which keeps it up.
    pub fn shows_on_change(self) -> bool {
        self.mode != Mode::Off && self.timeout.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::*;

    fn osd(args: &[&str]) -> Osd {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, None, &mut |_: &Path| Ok(None));
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
