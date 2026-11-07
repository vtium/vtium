//! The catalog of what vtium shows a person on its own screen: every
//! notice (ADR-1401 D7, D8, D9) and the on-screen display's full line
//! (D5), in one place, so they can be reviewed and reworded together, and
//! tier 2 and the specimens draw the same words the console shows. The head
//! file's own errors come from vtium-settings's catalog, `messages`.
//!
//! Each notice says what happened, then what the person reading it can do
//! next, in plain words. A key it names is built from the binding vtium
//! acts on, the one vtium.conf binds ([`Bindings::keys`]), never typed in.
//! A notice that would name a key vtium.conf leaves empty leaves the key
//! out.

use std::fmt::Display;

use crate::grab::{Action, Bindings};
use crate::head::chord::HeadChord;

/// The start found no keyboard on the seat (ADR-1401 D8).
pub const NO_KEYBOARD: &str = "No keyboard found. Connect one to type.";

/// The anti-lockout guard sent every keyboard to the head named `to`
/// (ADR-1401 D9).
pub fn anti_lockout(to: &str) -> String {
    format!(
        "Every keyboard now types into {to}, because no keyboard reached a screen that is on. To send a keyboard elsewhere, change its rule in {}, then restart vtium or reboot.",
        vtium_settings::CONF_FILE
    )
}

/// The rescue key moved the keyboard at `device` to the active head `to`
/// (ADR-1401 D9).
pub fn rescue(device: impl Display, to: &str) -> String {
    format!(
        "Keyboard {device} now types into {to}, the active screen, until the next boot (the rescue key moved it). To keep it there, give it a rule in {}.",
        vtium_settings::CONF_FILE
    )
}

/// `sentence` with the keys `bindings` binds to `chord` in place of its
/// `{}`, after a space; nothing when vtium.conf leaves the chord empty.
fn press(bindings: &Bindings, chord: HeadChord, sentence: &str) -> String {
    bindings
        .keys(Action::Head(chord))
        .map(|keys| format!(" {}", sentence.replace("{}", &keys)))
        .unwrap_or_default()
}

/// The rescue key moved the keyboard at `device` while no head is active
/// (ADR-1401 D9), naming the focus chord's keys `bindings` binds.
pub fn rescue_no_head(device: impl Display, bindings: &Bindings) -> String {
    format!(
        "Keyboard {device} now types into the active screen until the next boot (the rescue key moved it), but no screen is active.{}",
        press(bindings, HeadChord::NextHead, "Press {} to choose one.")
    )
}

/// What a console whose session ended under `oneshot` shows while no
/// other console of its head runs: the roaming chord's keys `bindings`
/// binds, which switch to another screen (vtium/vtium#156, ADR-1401 D1).
pub fn session_ended(bindings: &Bindings) -> String {
    format!(
        "Session ended. It will not start again.{}",
        press(
            bindings,
            HeadChord::NextHead,
            "Press {} to switch to another screen."
        )
    )
}

/// What a console whose session ended under `oneshot` shows while another
/// console of its head still runs: the console chord's keys `bindings`
/// binds, which show that console (ADR-1401 D1, D5).
pub fn session_ended_console(bindings: &Bindings) -> String {
    format!(
        "Session ended. It will not start again.{}",
        press(
            bindings,
            HeadChord::NextConsole,
            "Press {} to show another console on this screen."
        )
    )
}

/// What the boot display shows once a crash loop stopped every head under
/// the closed policy, for as long as vtium runs (ADR-1401 D1).
pub fn stopped() -> String {
    format!(
        "All screens stopped: a session kept ending as soon as it started. Nothing here takes input. Fix the login command in {}, then restart vtium or reboot.",
        vtium_settings::CONF_FILE
    )
}

/// The panic screen's second line (ADR-1401 D7): what the errors of the
/// broken closed head file at `path` left.
pub fn no_head_left(path: &std::path::Path) -> String {
    format!(
        "No head can run because of the errors in {}, listed below.",
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

/// vtium's notice when the session of a console the open chord opened
/// cannot start, and the console leaves: a notice of a problem, which
/// shows whatever `osd` says (ADR-1401 D5).
pub const NO_CONSOLE_OPENED: &str = "Console not opened: login could not start.";

/// The on-screen display's brief line on head `head` when the open chord
/// finds it at `head-consoles-max`, its `n` consoles (ADR-1401 D5).
pub fn no_more_consoles(head: &str, n: usize) -> String {
    format!("No more consoles: {head} has its limit of {n}.")
}

/// The on-screen display's brief line when the open chord finds every head
/// together at `consoles-max`, their `n` consoles (ADR-1401 D5).
pub fn no_more_consoles_together(n: usize) -> String {
    format!("No more consoles: all heads together have the limit of {n}.")
}

/// The on-screen display's full line on a head (ADR-1401 D5), such as
/// `● desk · console 2 of 3`: a filled dot on the `active` head and a
/// hollow one on the others, the head's name, and the shown console's
/// place among the head's `of` consoles, counted from 1.
pub fn osd_line(active: bool, head: &str, console: usize, of: usize) -> String {
    let dot = if active { '\u{25cf}' } else { '\u{25cb}' };
    format!("{dot} {head} \u{b7} console {console} of {of}")
}

/// What `vtium check-config` prints (ADR-1401 D7, D12; ADR-1403 D3): its
/// help, and the lines of its report that it writes itself. The errors of
/// vtium.conf's sections come from vtium-settings's catalog, and the
/// reports of its top level from the configuration's reader.
pub mod check_config {
    use std::fmt::Display;

    /// The subcommand's help.
    pub const HELP: &str = "Usage: vtium check-config [OPTION]... [--no-devices] [FILE]\n\n\
Checks vtium's configuration without applying it. It reads\n\
/etc/vtium/vtium.conf (or the vtium.conf in --configdir, or FILE, to\n\
check a file before you install it) and the other options, as vtium\n\
would at start.\n\n\
It checks the whole file: the settings at the top, and the [look],\n\
[head] and [rules] sections. It prints each error with what vtium would\n\
skip because of it, and each warning, such as a rule that never matches,\n\
a head with too many consoles, or a look whose font set is missing.\n\n\
It also checks that at least one display that is connected reaches a\n\
head that a keyboard reaches, and says what is missing if not. With no\n\
display or keyboard connected, or with --no-devices, it checks the rules\n\
alone and says so.\n\n\
It exits 1 when it finds an error or warning, and 0 when it finds none.\n\
Notes, printed on standard output, do not count: for example a weak\n\
match, a rule that needs udev, a head no rule uses, a device no rule\n\
places, or a font helper vtium would refuse. The notes end with what\n\
vtium would do with the file at start, and the file's policy.\n\n\
It only reads: vtium.conf, the font set font-name names, the font\n\
helper's owner and mode, and the devices from /sys and /run/udev/data.\n\
It opens nothing under /dev and changes nothing.\n";

    /// The frit `font-name` names could not be read.
    pub fn frit_unreadable(name: &str, why: &str) -> String {
        format!("font set {name} cannot be read ({why}), so vtium uses its built-in font")
    }

    /// `hwaccel` on in a build without the `gpu` feature (ADR-1303 D9).
    pub const HWACCEL_NOT_IN_BUILD: &str =
        "hwaccel needs a build with GPU support, so vtium draws in software";

    /// A key vtium takes and does not act on yet, set by a source.
    pub fn not_built(key: &str) -> String {
        format!("{key} has no effect yet, so vtium ignores it")
    }

    pub const MODE_OVER_ORIGINAL: &str = "mode is set, so use-original-mode is off";

    /// `grab-power-off` is bound and PID 1 neither catches nor blocks the
    /// power-off `signal`.
    pub fn power_off_not_taken(signal: i32) -> String {
        format!(
            "the grab-power-off key will not work: the init system (PID 1) neither handles nor blocks SIGRTMIN+4 (signal {signal}, from SigCgt and SigBlk in /proc/1/status). Unbind the key, or use an init system that powers off on that signal, such as systemd or sysvinit."
        )
    }

    /// `grab-power-off` is bound and `/proc/1/status` could not be read,
    /// the system saying `system_error`.
    pub fn power_off_status_unreadable(signal: i32, system_error: impl Display) -> String {
        format!(
            "the grab-power-off key may not work: vtium cannot tell whether the init system (PID 1) handles SIGRTMIN+4 (signal {signal}), because /proc/1/status could not be read: {system_error}"
        )
    }

    /// `grab-power-off` is bound and `/proc/1/status` has no `line` line,
    /// `SigCgt` or `SigBlk`.
    pub fn power_off_no_mask(signal: i32, line: &str) -> String {
        format!(
            "the grab-power-off key may not work: vtium cannot tell whether the init system (PID 1) handles SIGRTMIN+4 (signal {signal}), because /proc/1/status has no {line} line"
        )
    }

    /// `grab-power-off` is bound and the `line` line of `/proc/1/status`,
    /// `SigCgt` or `SigBlk`, holds `mask`, which is not a hex mask.
    pub fn power_off_bad_mask(signal: i32, line: &str, mask: &str) -> String {
        format!(
            "the grab-power-off key may not work: vtium cannot tell whether the init system (PID 1) handles SIGRTMIN+4 (signal {signal}), because the {line} line of /proc/1/status is not a hex mask: {mask:?}"
        )
    }

    pub fn display_dpi(dpi: u64) -> String {
        format!(
            "display-dpi={dpi}: every display uses {dpi} DPI for font sizes, whatever density it reports"
        )
    }

    pub const MEASURED_DENSITY: &str = "each display uses the density it reports from its physical size for font sizes, or 96 DPI with a warning if it reports none; vtium logs which at start";

    /// The helper `font-helper` names is one vtium refuses, `why`, for the
    /// account the check runs as; the start, which may run as another,
    /// checks it again.
    pub fn helper_refused(why: &str) -> String {
        format!(
            "font-helper: {why}. vtium would use its built-in font. vtium checks again at start, for the account it runs as."
        )
    }

    /// The helper `font-helper` names is one a start as root refuses,
    /// `why`, though the account the check runs as takes it.
    pub fn helper_refused_as_root(why: &str) -> String {
        format!(
            "font-helper: when vtium runs as root, it refuses this helper: {why}. vtium would then use its built-in font."
        )
    }

    /// The helper `image-helper` names is one vtium refuses, `why`, for
    /// the account the check runs as; the start, which may run as
    /// another, checks it again.
    pub fn image_helper_refused(why: &str) -> String {
        format!(
            "image-helper: {why}. vtium would refuse images sent as files or shared memory, and PNG and zlib images. vtium checks again at start, for the account it runs as."
        )
    }

    /// The helper `image-helper` names is one a start as root refuses,
    /// `why`, though the account the check runs as takes it.
    pub fn image_helper_refused_as_root(why: &str) -> String {
        format!(
            "image-helper: when vtium runs as root, it refuses this helper: {why}. vtium would then refuse images sent as files or shared memory, and PNG and zlib images."
        )
    }

    /// A folder of `font-paths` that does not exist (ADR-1200 D25).
    pub fn font_path_missing(dir: &str) -> String {
        format!("font-paths: {dir:?} does not exist, so no fonts can be found there")
    }

    /// A folder of `font-paths` that other accounts cannot read and
    /// search (ADR-1200 D25).
    pub fn font_path_private(dir: &str) -> String {
        format!(
            "font-paths: other users cannot read {dir:?}. Before someone logs in, the font helper runs as nobody and cannot use the fonts there; they appear only after login."
        )
    }

    /// A candidate vtium.conf that is not there.
    pub const NO_SUCH_FILE: &str = "does not exist";

    pub const ROOT_ONLY: &str = "permission denied: only root can check this file";

    /// A vtium.conf with no section and no policy line: a kmscon.conf,
    /// whose keys apply and for which vtium runs its compiled-in flow
    /// (ADR-1403 D3).
    pub const NO_FLOW: &str = "This file has no sections and no policy line, so vtium uses its settings with the built-in looks, heads and rules, and shows no notice.";

    /// A candidate vtium.conf owned by `uid`, not root: the start trusts it
    /// once it is installed.
    pub fn candidate_owner(path: impl Display, uid: u32) -> String {
        format!(
            "{path} belongs to user {uid}, not to root. That is fine for a check: vtium reads the file once it is installed, owned by root and writable only by root."
        )
    }

    /// A candidate vtium.conf its group or others may change, in `mode`:
    /// the start trusts it once it is installed.
    pub fn candidate_writable(path: impl Display, mode: u32) -> String {
        format!(
            "Other users can change {path} (mode {mode:o}). That is fine for a check: vtium reads the file once it is installed, owned by root and writable only by root."
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
        format!("`{key}:` matches nothing, because udev is not running")
    }

    /// A rule using `connector-path:`, which vtium matches when it finds
    /// the display and check-config cannot check against the displays
    /// present.
    pub fn unchecked_connector_path() -> String {
        format!(
            "`{token}` is not checked against the connected displays, because check-config does not open the graphics card; vtium matches it when it finds the display",
            token = "connector-path:",
        )
    }

    /// A device that reports no name, path or connector.
    pub const UNNAMED_DEVICE: &str = "a device with no name";

    pub const MATCHES_NOTHING: &str = "the rule matches no connected device";

    pub fn pointers_only(head: &str) -> String {
        format!("head {head} has mice but no keyboard: every keyboard goes to another head")
    }

    pub const EVERY_KEYBOARD_IGNORED: &str = "every keyboard is ignored";

    /// The head `head`'s only keyboard is the one `device`, as it reports
    /// itself.
    pub fn one_keyboard(head: &str, device: &str) -> String {
        format!("head {head} has only one keyboard, {device}")
    }
}

/// What `vtium blank` and `vtium wake` print: their usage, the errors in
/// their words, and what became of the request.
pub mod blank {
    use std::fmt::{Debug, Display};

    /// `vtium blank`'s usage.
    pub const USAGE_BLANK: &str = "Usage: vtium blank --vt N\nTurns off the displays of the vtium on VT N now (it sends SIGRTMIN+1). Run it as root or as the user vtium runs as.\n";

    /// `vtium wake`'s usage.
    pub const USAGE_WAKE: &str = "Usage: vtium wake --vt N\nTurns on the displays of the vtium on VT N (it sends SIGRTMIN+2). Run it as root or as the user vtium runs as.\n";

    /// The subcommand `word` refused its words for `why`, then its
    /// `usage`.
    pub fn refused(word: &str, why: &str, usage: &str) -> String {
        format!("vtium {word}: {why}\n{usage}")
    }

    pub fn not_utf8(word: impl Debug) -> String {
        format!("{word:?} is not UTF-8 text")
    }

    pub const VT_NEEDS_NUMBER: &str = "--vt needs a VT number";

    pub fn unknown_option(word: &str) -> String {
        format!("unknown option {word}")
    }

    pub const VT_TWICE: &str = "--vt is given twice";

    pub fn not_a_vt(value: &str, last: u16) -> String {
        format!("{value} is not a VT number from 1 to {last}")
    }

    pub const VT_MISSING: &str = "give the VT with --vt N";

    /// No vtium found on VT `vt`, and another user's could not be read.
    pub fn none_readable(word: &str, vt: u16) -> String {
        format!(
            "vtium {word}: no vtium found on VT {vt}. A vtium run by another user cannot be seen: run this as root or as that user."
        )
    }

    pub fn none_on(word: &str, vt: u16) -> String {
        format!("vtium {word}: no vtium runs on VT {vt}")
    }

    pub fn left_before(word: &str, pid: u32, vt: u16) -> String {
        format!("vtium {word}: vtium (process {pid}) left VT {vt} before the signal reached it")
    }

    pub fn cannot_signal(word: &str, pid: u32, vt: u16, why: impl Display) -> String {
        format!("vtium {word}: cannot signal vtium (process {pid}) on VT {vt}: {why}")
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
            "vtium config: {why}. vtium config runs vtium-config from the path in the font-helper setting."
        )
    }

    /// The tool at `program`, which vtium config starts through
    /// `/proc/self/fd`, could not be started, for `why`.
    pub fn cannot_run(program: impl Display, why: impl Display) -> String {
        format!(
            "vtium config: cannot start {program}: {why}. vtium config starts it through /proc/self/fd, so /proc must be mounted."
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
            session_ended(&Bindings::default()),
            "Session ended. It will not start again. Press Ctrl+Super+↑ to switch to another screen."
        );
    }

    // While another console of the head runs, the notice names the console
    // chord's keys as vtium binds them.
    #[test]
    fn the_ended_session_names_the_keys_that_show_another_console() {
        assert_eq!(
            session_ended_console(&Bindings::default()),
            "Session ended. It will not start again. Press Ctrl+Super+→ to show another console on this screen."
        );
    }

    // ADR-1005 D10: a notice names the keys vtium.conf binds, and leaves
    // out the sentence that would name a key vtium.conf empties.
    #[test]
    fn a_notice_names_the_keys_vtium_conf_binds() {
        let mut config = crate::config::Config::default();
        config.set_chords(
            &crate::config::keys::GRAB_FOCUS_NEXT,
            crate::grab::parse("<Alt>n").unwrap_or_default(),
        );
        config.set_chords(&crate::config::keys::GRAB_CONSOLE_NEXT, Vec::new());
        let bound = Bindings::of(&config);
        assert_eq!(
            session_ended(&bound),
            "Session ended. It will not start again. Press Alt+n to switch to another screen."
        );
        assert_eq!(
            rescue_no_head("/dev/input/event3", &bound),
            "Keyboard /dev/input/event3 now types into the active screen until the next boot (the rescue key moved it), but no screen is active. Press Alt+n to choose one."
        );
        assert_eq!(
            session_ended_console(&bound),
            "Session ended. It will not start again."
        );
    }

    #[test]
    fn the_full_line_names_the_head_and_the_shown_console() {
        assert_eq!(osd_line(true, "desk", 2, 3), "● desk · console 2 of 3");
        assert_eq!(osd_line(false, "panel", 1, 1), "○ panel · console 1 of 1");
    }
}
