//! The catalog of what vtium shows a person on its own screen: every
//! notice (ADR-1401 D7, D8, D9) and the on-screen display's full line
//! (D5), in one place, so they can be reviewed and reworded together, and
//! tier 2 and the specimens draw the same words the console shows. The head
//! file's own errors come from vtium-config's catalog, `messages`.
//!
//! Each notice says what happened, then what the person reading it can do
//! next, in plain words. A key it names is built from the binding vtium
//! acts on ([`chord::keys`]), never typed in.

use std::fmt::Display;

use crate::head::chord::{self, HeadChord};

/// The start found no keyboard on the seat (ADR-1401 D8).
pub const NO_KEYBOARD: &str = "No keyboard was found. Plug one in to type.";

/// The anti-lockout guard sent every keyboard to the head named `to`
/// (ADR-1401 D9).
pub fn anti_lockout(to: &str) -> String {
    format!(
        "No keyboard was reaching a screen that is on, so every keyboard now types into {to}. To send a keyboard somewhere else, change its rule in {}, then restart vtium or reboot.",
        vtium_config::HEAD_FILE
    )
}

/// The rescue key moved the keyboard at `device` to the active head `to`
/// (ADR-1401 D9).
pub fn rescue(device: impl Display, to: &str) -> String {
    format!(
        "The rescue key moved the keyboard at {device}. It types into the active screen, {to}, until the next boot. To keep it there after that, give it a rule in {}.",
        vtium_config::HEAD_FILE
    )
}

/// The rescue key moved the keyboard at `device` while no head is active
/// (ADR-1401 D9).
pub fn rescue_no_head(device: impl Display) -> String {
    format!(
        "The rescue key moved the keyboard at {device}. It types into the active screen until the next boot, and no screen is active now. Press {} to choose one.",
        chord::keys(HeadChord::NextHead)
    )
}

/// What a head whose session ended under `oneshot` shows in place of its
/// session, which vtium does not start again (vtium/vtium#156).
pub fn session_ended() -> String {
    format!(
        "This session has ended and will not start again. To keep working, press {} to switch to another screen.",
        chord::keys(HeadChord::NextHead)
    )
}

/// The last line of the notice of a head file with errors (ADR-1401 D7).
pub fn head_file_ignored() -> String {
    format!(
        "This file has errors, so vtium ignored it and uses its built-in rules. Fix the errors above and check the file with `{command}`, then restart vtium or reboot to use it.",
        command = CHECK_CONFIG,
    )
}

/// The command that checks the configuration, as a notice names it: a
/// placeholder's value, never words to translate.
pub const CHECK_CONFIG: &str = "vtium check-config";

/// The line of a head file's notice that counts the `n` errors past those
/// it lists (ADR-1401 D7).
pub fn more_errors(n: usize) -> String {
    format!("More errors: {n}")
}

/// The on-screen display's full line on a head (ADR-1401 D5), such as
/// `● desk · console 2 of 3`: a filled dot on the `active` head and a
/// hollow one on the others, the head's name, and the shown console's
/// place among the head's `of` consoles, counted from 1.
pub fn osd_line(active: bool, head: &str, console: usize, of: usize) -> String {
    let dot = if active { '\u{25cf}' } else { '\u{25cb}' };
    format!("{dot} {head} \u{b7} console {console} of {of}")
}

/// What `vtium check-config` prints (ADR-1401 D7, D12): its help, and the
/// lines of its report that it writes itself. The head file's errors come
/// from vtium-config's catalog, and the configuration's reports from the
/// configuration's reader.
pub mod check_config {
    use std::fmt::Display;

    /// The subcommand's help.
    pub const HELP: &str = "Usage: vtium check-config [OPTION]... [FILE]\n\n\
Reads the configuration as vtium would start with these options:\n\
kmscon.conf and the seat's file in the configuration directory, then the\n\
options, and the head file, /etc/vtium/heads.conf, or FILE in its place\n\
to check it before it is installed. It prints each warning, and each\n\
of the head file's errors and warnings, and exits 1 when there is any,\n\
0 when there is none. It notes on standard output what it does not\n\
count: the density font sizes in points convert at, which the start\n\
reads from each display, and of the head file a guard turned off, a\n\
weak match, a rule that needs a udev daemon where none runs, a rule\n\
using connector-path:, which this build matches to nothing, and, of\n\
the devices present, each rule matching none, a head claiming pointers\n\
while every keyboard is claimed elsewhere, every keyboard ignored, and\n\
a head whose only keyboard is one device.\n\
It only checks: it opens nothing under /dev, writes nothing and applies\n\
nothing. It reads the configuration files, the frit font-name names,\n\
and the devices present from /sys and udev's database, /run/udev/data.\n";

    /// The frit `font-name` names could not be read.
    pub fn frit_unreadable(name: &str, why: &str) -> String {
        format!("the frit {name} cannot be read ({why}); vtium draws its built-in font")
    }

    pub const HWACCEL_NOT_BUILT: &str = "hwaccel is not built yet; vtium draws in software";

    pub const MODE_OVER_ORIGINAL: &str = "mode is set, so use-original-mode is off";

    /// `grab-power-off` is bound and PID 1 does not catch the power-off
    /// `signal`.
    pub fn power_off_uncaught(signal: i32) -> String {
        format!(
            "grab-power-off is bound, and PID 1 does not catch SIGRTMIN+4 (signal {signal}, from SigCgt in /proc/1/status): it powers nothing off"
        )
    }

    /// `grab-power-off` is bound and `/proc/1/status` could not be read,
    /// the system saying `system_error`.
    pub fn power_off_status_unreadable(signal: i32, system_error: impl Display) -> String {
        format!(
            "grab-power-off is bound, and whether PID 1 catches SIGRTMIN+4 (signal {signal}) is unknown, since /proc/1/status could not be read: {system_error}"
        )
    }

    /// `grab-power-off` is bound and `/proc/1/status` has no `SigCgt` line.
    pub fn power_off_no_sigcgt(signal: i32) -> String {
        format!(
            "grab-power-off is bound, and whether PID 1 catches SIGRTMIN+4 (signal {signal}) is unknown, since /proc/1/status has no SigCgt line"
        )
    }

    /// `grab-power-off` is bound and `/proc/1/status`'s `SigCgt` line,
    /// `mask`, is not a hex mask.
    pub fn power_off_bad_mask(signal: i32, mask: &str) -> String {
        format!(
            "grab-power-off is bound, and whether PID 1 catches SIGRTMIN+4 (signal {signal}) is unknown, since the SigCgt line of /proc/1/status is not a hex mask: {mask:?}"
        )
    }

    pub fn display_dpi(dpi: u64) -> String {
        format!(
            "display-dpi={dpi}: every display converts points at {dpi} DPI, its measured density aside"
        )
    }

    pub const MEASURED_DENSITY: &str = "each display converts points at the density it measures from its physical size, else at 96 DPI with a warning naming it; the start logs which";

    /// A candidate head file that is not there.
    pub const NO_SUCH_FILE: &str = "does not exist";

    /// The installed head file is not there.
    pub const ABSENT: &str = "absent, so the start runs the shipped rules";

    pub const ROOT_ONLY: &str = "permission denied: the head file can be checked only as root";

    /// A candidate head file owned by `uid`, not root: the start trusts it
    /// once it is installed.
    pub fn candidate_owner(path: impl Display, uid: u32) -> String {
        format!(
            "{path} belongs to user {uid}, not to root. The start reads this file once it is installed, owned by root and writable only by root."
        )
    }

    /// A candidate head file its group or others may change, in `mode`:
    /// the start trusts it once it is installed.
    pub fn candidate_writable(path: impl Display, mode: u32) -> String {
        format!(
            "{path} can be changed by its group or by other users (mode {mode:o}). The start reads this file once it is installed, owned by root and writable only by root."
        )
    }

    pub const REJECTED: &str = "rejected whole, so a start reading it runs the shipped rules";

    pub fn more_warnings(n: usize) -> String {
        format!("More warnings: {n}")
    }

    pub fn more_notes(n: usize) -> String {
        format!("More notes: {n}")
    }

    /// A rule matching on `key`, which only a udev daemon gives.
    pub fn no_udev(key: impl Display) -> String {
        format!("no udev daemon runs, so `{key}:` matches nothing")
    }

    pub fn no_connector_path() -> String {
        format!(
            "this build reads no connector's PATH, so `{token}` matches nothing (vtium/vtium#157)",
            token = "connector-path:",
        )
    }

    /// A device that reports no name, path or connector.
    pub const UNNAMED_DEVICE: &str = "a device with no name";

    pub const MATCHES_NOTHING: &str = "the rule matches no device present";

    pub fn pointers_only(head: &str) -> String {
        format!(
            "head {head} claims pointers and no keyboard, and every keyboard is claimed by another head"
        )
    }

    pub const EVERY_KEYBOARD_IGNORED: &str = "every keyboard is ignored";

    /// The head `head`'s only keyboard is the one `device`, as it reports
    /// itself.
    pub fn one_keyboard(head: &str, device: &str) -> String {
        format!("head {head}'s only keyboard is one device, {device}")
    }
}

/// What `vtium blank` and `vtium wake` print: their usage, the errors in
/// their words, and what became of the request.
pub mod blank {
    use std::fmt::{Debug, Display};

    /// `vtium blank`'s usage.
    pub const USAGE_BLANK: &str = "Usage: vtium blank --vt N\nFinds the vtium on VT N and blanks its displays now (SIGRTMIN+1). Root or the user vtium runs as may send it.\n";

    /// `vtium wake`'s usage.
    pub const USAGE_WAKE: &str = "Usage: vtium wake --vt N\nFinds the vtium on VT N and wakes its displays (SIGRTMIN+2). Root or the user vtium runs as may send it.\n";

    /// The subcommand `word` refused its words for `why`, then its
    /// `usage`.
    pub fn refused(word: &str, why: &str, usage: &str) -> String {
        format!("vtium {word}: {why}\n{usage}")
    }

    pub fn not_utf8(word: impl Debug) -> String {
        format!("{word:?} is not UTF-8")
    }

    pub const VT_NEEDS_NUMBER: &str = "--vt needs a VT's number";

    pub fn unknown_option(word: &str) -> String {
        format!("unknown option {word}")
    }

    pub const VT_TWICE: &str = "--vt is given twice";

    pub fn not_a_vt(value: &str, last: u16) -> String {
        format!("{value} is no VT 1 to {last}")
    }

    pub const VT_MISSING: &str = "--vt N names the VT";

    /// No vtium found on VT `vt`, and another user's could not be read.
    pub fn none_readable(word: &str, vt: u16) -> String {
        format!(
            "vtium {word}: no vtium found on VT {vt}; another user's vtium cannot be read: run as root or as vtium's user"
        )
    }

    pub fn none_on(word: &str, vt: u16) -> String {
        format!("vtium {word}: no vtium runs on VT {vt}")
    }

    pub fn left_before(word: &str, pid: u32, vt: u16) -> String {
        format!("vtium {word}: vtium {pid} left VT {vt} before the signal")
    }

    pub fn cannot_signal(word: &str, pid: u32, vt: u16, why: impl Display) -> String {
        format!("vtium {word}: cannot signal vtium {pid} on VT {vt}: {why}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The maintainer's wording: the ended session's notice names the focus
    // chord's keys as vtium binds them.
    #[test]
    fn the_ended_session_names_the_keys_that_switch_screens() {
        assert_eq!(
            session_ended(),
            "This session has ended and will not start again. To keep working, press Ctrl+Super+↑ to switch to another screen."
        );
    }

    #[test]
    fn the_full_line_names_the_head_and_the_shown_console() {
        assert_eq!(osd_line(true, "desk", 2, 3), "● desk · console 2 of 3");
        assert_eq!(osd_line(false, "panel", 1, 1), "○ panel · console 1 of 1");
    }
}
