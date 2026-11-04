//! The catalog of what vtium shows a person on its own screen: every
//! notice (ADR-1401 D7, D8, D9) and the on-screen display's full line
//! (D5), in one place, so they can be reviewed and reworded together, and
//! tier 2 and the specimens draw the same words the console shows. The head
//! file's own errors come from vtium-settings's catalog, `messages`.
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
        vtium_settings::HEAD_FILE
    )
}

/// The rescue key moved the keyboard at `device` to the active head `to`
/// (ADR-1401 D9).
pub fn rescue(device: impl Display, to: &str) -> String {
    format!(
        "The rescue key moved the keyboard at {device}. It types into the active screen, {to}, until the next boot. To keep it there after that, give it a rule in {}.",
        vtium_settings::HEAD_FILE
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

/// What a console whose session ended under `oneshot` shows while no
/// other console of its head runs: the roaming chord's keys, which switch
/// to another screen (vtium/vtium#156, ADR-1401 D1).
pub fn session_ended() -> String {
    format!(
        "This session has ended and will not start again. To keep working, press {} to switch to another screen.",
        chord::keys(HeadChord::NextHead)
    )
}

/// What a console whose session ended under `oneshot` shows while another
/// console of its head still runs: the console chord's keys, which show
/// that console (ADR-1401 D1, D5).
pub fn session_ended_console() -> String {
    format!(
        "This session has ended and will not start again. To keep working, press {} to show another console on this screen.",
        chord::keys(HeadChord::NextConsole)
    )
}

/// What the boot display shows once a crash loop stopped every head under
/// the closed policy, for as long as vtium runs (ADR-1401 D1).
pub fn stopped() -> String {
    format!(
        "A session kept ending as soon as it started, so vtium stopped every screen, and nothing here takes input. Fix the login command in {} or vtium.conf, then restart vtium or reboot.",
        vtium_settings::HEAD_FILE
    )
}

/// The panic screen's second line (ADR-1401 D7): what the errors of the
/// broken closed head file at `path` left.
pub fn no_head_left(path: &std::path::Path) -> String {
    format!(
        "The errors in {}, listed below, leave no head that can run.",
        vtium_settings::Shown(path)
    )
}

/// The panic screen (ADR-1401 D7), which the broken closed head file at
/// `path` shows on the boot display for as long as vtium runs: its console
/// line, that no head is left, the file's first `errors`, each with its
/// file and line, the count of the `more` past them and of any the display
/// has no room for, and what to do next, each naming that file.
pub fn panic(
    path: &std::path::Path,
    errors: Vec<String>,
    more: usize,
) -> vtium_render::NoticeReport {
    let line = vtium_settings::Line::ClosedOff;
    vtium_render::NoticeReport {
        happened: vec![line.text().to_owned(), no_head_left(path)],
        items: errors,
        more,
        label: MORE_ERRORS.to_owned(),
        next: line.next(path).to_string(),
    }
}

/// The command that checks the configuration, as a notice names it: a
/// placeholder's value, never words to translate.
pub const CHECK_CONFIG: &str = "vtium check-config";

/// The label of a count of a head file's errors past those listed
/// (ADR-1401 D7), which the number follows.
pub const MORE_ERRORS: &str = "More errors:";

/// The line that counts the `n` errors of a head file past those listed
/// (ADR-1401 D7).
pub fn more_errors(n: usize) -> String {
    format!("{MORE_ERRORS} {n}")
}

/// The on-screen display's brief line when the session of a console the
/// open chord opened cannot start, and the console leaves (ADR-1401 D5).
pub const NO_CONSOLE_OPENED: &str = "No console opened: its login could not start.";

/// The on-screen display's brief line on head `head` when the open chord
/// finds it at `head-consoles-max`, its `n` consoles (ADR-1401 D5).
pub fn no_more_consoles(head: &str, n: usize) -> String {
    format!("No more consoles: {head} has {n}, its most.")
}

/// The on-screen display's brief line when the open chord finds every head
/// together at `consoles-max`, their `n` consoles (ADR-1401 D5).
pub fn no_more_consoles_together(n: usize) -> String {
    format!("No more consoles: every head together has {n}, the most.")
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
/// from vtium-settings's catalog, and the configuration's reports from the
/// configuration's reader.
pub mod check_config {
    use std::fmt::Display;

    /// The subcommand's help.
    pub const HELP: &str = "Usage: vtium check-config [OPTION]... [FILE]\n\n\
Reads the configuration as vtium would start with these options:\n\
vtium.conf and the seat's file in the configuration directory, then the\n\
options, and the head file, /etc/vtium/heads.conf, or FILE in its place\n\
to check it before it is installed. It prints each warning, and each\n\
of the head file's errors, each followed by what it drops, and its\n\
warnings: a shadowed rule, a head's consoles past its cap or past the\n\
total, and a profile naming a missing frit. It exits 1 when there is\n\
any, 0 when there is none. It notes on standard output what it does not\n\
count: the density font sizes in points convert at, which the start\n\
reads from each display, and of the head file a weak match, a rule that\n\
needs a udev daemon where none runs, a rule using connector-path:, which\n\
it cannot check without opening the card, a head no rule names, every\n\
head isolated, and, of the devices present, each rule matching none, each\n\
device no rule places and what the policy does with it, no head that\n\
can be the active head, under closed no keyboard reaching any head, a\n\
head claiming pointers while every keyboard is claimed elsewhere, every\n\
keyboard ignored, and a head whose only keyboard is one device. Its\n\
notes end with what a start does with the file, and the file's policy.\n\
It also notes when vtium would refuse the glyph helper font-helper\n\
names. It only checks: it opens nothing under /dev, writes nothing and\n\
applies nothing. It reads the configuration files, the frit font-name\n\
names, the helper's owner and mode, and the devices present from /sys\n\
and udev's database, /run/udev/data.\n";

    /// The frit `font-name` names could not be read.
    pub fn frit_unreadable(name: &str, why: &str) -> String {
        format!("the frit {name} cannot be read ({why}); vtium draws its built-in font")
    }

    /// `hwaccel` on in a build without the `gpu` feature (ADR-1303 D9).
    pub const HWACCEL_NOT_IN_BUILD: &str =
        "hwaccel needs a build with the gpu feature; vtium draws in software";

    /// A key vtium takes and does not act on yet, set by a source.
    pub fn not_built(key: &str) -> String {
        format!("{key} is not built yet; vtium ignores it")
    }

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

    /// The helper `font-helper` names is one vtium refuses, `why`, for the
    /// account the check runs as; the start, which may run as another,
    /// checks it again.
    pub fn helper_refused(why: &str) -> String {
        format!(
            "font-helper: {why}; vtium would draw its built-in font, and the start checks again for the account it runs as"
        )
    }

    /// The helper `font-helper` names is one a start as root refuses,
    /// `why`, though the account the check runs as takes it.
    pub fn helper_refused_as_root(why: &str) -> String {
        format!(
            "font-helper: started as root, vtium refuses it: {why}; the root start draws its built-in font"
        )
    }

    /// The helper `image-helper` names is one vtium refuses, `why`, for
    /// the account the check runs as; the start, which may run as
    /// another, checks it again.
    pub fn image_helper_refused(why: &str) -> String {
        format!(
            "image-helper: {why}; every console would answer files, shared memory, PNG and zlib itself, as with no helper, and the start checks again for the account it runs as"
        )
    }

    /// The helper `image-helper` names is one a start as root refuses,
    /// `why`, though the account the check runs as takes it.
    pub fn image_helper_refused_as_root(why: &str) -> String {
        format!(
            "image-helper: started as root, vtium refuses it: {why}; under the root start every console answers files, shared memory, PNG and zlib itself, as with no helper"
        )
    }

    /// A candidate head file that is not there.
    pub const NO_SUCH_FILE: &str = "does not exist";

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

    /// A rule using `connector-path:`, which vtium matches when it finds
    /// the display and check-config cannot check against the displays
    /// present.
    pub fn unchecked_connector_path() -> String {
        format!(
            "check-config opens no card, so it cannot read a connector's PATH and does not check `{token}` against the displays present; vtium matches it when it finds the display",
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

/// What `vtium config` prints when it cannot hand its words to
/// vtium-config (ADR-1201 D12).
pub mod config_tool {
    use std::fmt::Display;

    /// vtium refused the tool at the glyph helper's path for `why`, the
    /// helper's own reason, which names the path.
    pub fn refused(why: &str) -> String {
        format!(
            "vtium config: {why}. vtium config runs vtium-config, the font and settings tool, from the glyph helper's path, which the font-helper key sets."
        )
    }

    /// The tool at `program`, opened as descriptor `fd`, could not be
    /// started, for `why`.
    pub fn cannot_run(program: impl Display, fd: i32, why: impl Display) -> String {
        format!(
            "vtium config: cannot start {program} through /proc/self/fd/{fd}: {why}; vtium config starts the tool from the file it opened, through /proc/self/fd, so /proc must be mounted."
        )
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

    // While another console of the head runs, the notice names the console
    // chord's keys as vtium binds them.
    #[test]
    fn the_ended_session_names_the_keys_that_show_another_console() {
        assert_eq!(
            session_ended_console(),
            "This session has ended and will not start again. To keep working, press Ctrl+Super+→ to show another console on this screen."
        );
    }

    #[test]
    fn the_full_line_names_the_head_and_the_shown_console() {
        assert_eq!(osd_line(true, "desk", 2, 3), "● desk · console 2 of 3");
        assert_eq!(osd_line(false, "panel", 1, 1), "○ panel · console 1 of 1");
    }
}
