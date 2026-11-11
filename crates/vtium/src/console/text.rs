//! What `vtium console` says, in one place, so it can be read and reworded
//! together.

use std::fmt::Debug;

use super::Mode;
use super::advice::{self, Facts, SeatBlock};
use super::held::{Holder, Reason};

pub const USAGE: &str = "Usage: vtium console enable --direct|--seat N|--all | enable [N|--all] | disable N|--all | status | fallback N\n\
enable makes vtium run on VT N, or on every VT, from its next start, and disable undoes that. N is 1 to 63, or ttyN.\n\
--direct runs vtium in direct mode, which opens the screen and the keyboards itself. --seat runs it in seat mode, which takes them from logind.\n\
enable with no mode changes nothing: it says which mode fits this machine and the command to run.\n\
enable and disable need root, and they start or stop nothing now. disable turns off either mode. status shows what runs on each VT.\n\
fallback N is what vtium-getty@.service runs when vtium on VT N fails: it starts a login prompt (getty) on VT N, unless that would stop another program.\n";

pub const NO_VERB: &str = "give a command: enable, disable, status or fallback";

pub fn not_utf8(word: impl Debug) -> String {
    format!("{word:?} is not UTF-8 text")
}

pub fn extra_word(word: &str) -> String {
    format!("unexpected argument {word}")
}

pub fn needs_which(verb: &str) -> String {
    format!("{verb} needs a VT number or --all")
}

pub const TWO_MODES: &str = "enable takes one mode: --direct or --seat, not both";

pub fn disable_no_mode(flag: &str) -> String {
    format!(
        "disable takes no mode, so {flag} does not fit here. disable turns off vtium in whichever mode it has on each VT: give a VT number or --all"
    )
}

/// The unit of the mode whose prefix is `prefix`, in words.
fn mode_of(prefix: &str) -> &'static str {
    if prefix == super::SEAT {
        "seat mode"
    } else {
        "direct mode"
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Direct => "direct mode",
        Mode::Seat => "seat mode",
    }
}

/// Why `enable --seat` refuses, and the same command in direct mode.
pub fn seat_cannot_run(block: SeatBlock, which: &str) -> String {
    let why = match block {
        SeatBlock::Build => {
            "Seat mode cannot run with this vtium: it is built without libseat and logind, and seat mode needs both."
        }
        SeatBlock::Unit => {
            "Seat mode cannot run here: no systemd unit directory has vtium-seat@.service, or it is masked. An install of a vtium built with libseat and logind ships it."
        }
        SeatBlock::Logind => {
            "Seat mode cannot run here: logind manages no seat0 on this machine (there is no /run/systemd/seats/seat0)."
        }
    };
    format!(
        "{why} Nothing changed. To turn vtium on in direct mode instead, run vtium console enable --direct {which}."
    )
}

pub fn unknown_verb(word: &str) -> String {
    format!("vtium console has no command {word}: use enable, disable, status or fallback")
}

pub fn unknown_option(word: &str) -> String {
    format!("unknown option {word}")
}

pub fn not_a_vt(word: &str) -> String {
    format!("{word} is not a VT number from 1 to {}", super::LAST_VT)
}

pub const NO_SYSTEMD: &str = "nothing to change: this machine was not started with systemd (there is no /run/systemd/system). To run vtium on a VT, have your init system start vtium --vt=ttyN --no-switchvt where it started a getty; see docs/systemd.md.";

pub fn needs_root(words: &str) -> String {
    format!(
        "this needs root, because it changes systemd units. For example: sudo vtium console {words}"
    )
}

pub fn not_installed(prefix: &str) -> String {
    format!(
        "{prefix}@.service, the unit of {}, is not installed: no systemd unit directory has it. The vtium package installs it in /usr/lib/systemd/system.",
        mode_of(prefix)
    )
}

pub fn cannot_run(line: &str, why: &str) -> String {
    format!("cannot run systemctl {line}: {why}")
}

pub fn failed(line: &str, stderr: &str) -> String {
    let said = stderr.trim_end();
    format!(
        "systemctl {line} failed{}{said}\nThe steps after it were not done. Run the same command again to finish the change, or the opposite command (enable or disable) to undo it.",
        if said.is_empty() { "" } else { ":\n" }
    )
}

pub fn running(line: &str) -> String {
    format!("running systemctl {line}")
}

pub fn linking(path: &str, to: &str) -> String {
    format!("linking {path} to {to}")
}

pub fn removing(path: &str) -> String {
    format!("removing {path}")
}

pub fn recorded(what: &str, path: &str) -> String {
    format!("recording {what} in {path}")
}

pub fn cannot_write(path: &str, why: &str) -> String {
    format!("cannot write {path}: {why}")
}

pub fn cannot_remove(path: &str, why: &str) -> String {
    format!("cannot remove {path}: {why}")
}

pub fn getty_unit(n: u16, to: &str) -> String {
    format!("that getty@tty{n}.service was enabled (linked to {to})")
}

pub fn no_getty(n: u16) -> String {
    format!("that no getty was enabled on VT {n}")
}

pub const TEMPLATE_WHAT: &str = "where autovt@.service pointed";

pub fn kmscon_holds(n: u16, own_link: bool) -> String {
    let link = if own_link {
        format!(", and rm /etc/systemd/system/autovt@tty{n}.service, which disable does not remove")
    } else {
        String::new()
    };
    format!(
        "kmscon (kmsconvt@tty{n}.service) runs on VT {n}. Free the VT first with systemctl disable --now kmsconvt@tty{n}.service, which ends every session on VT {n}{link}."
    )
}

pub const KMSCON_TEMPLATE: &str = "kmscon runs on every VT: autovt@.service links to kmsconvt@.service. vtium can take every VT only on a machine without kmscon; systemctl disable kmsconvt@.service removes its links. To run vtium beside kmscon, give it one VT with vtium console enable N.";

pub fn kept_kmscon(n: u16) -> String {
    format!(
        "VT {n} stays with kmscon (kmsconvt@tty{n}), which is enabled, linked or running there. To free it, run systemctl disable --now kmsconvt@tty{n}.service; that ends every session on VT {n}."
    )
}

pub const TEMPLATE_NOT_LINK: &str = "vtium does not replace /etc/systemd/system/autovt@.service: it is a file, not a link, so someone wrote it on purpose.";

pub fn dev_holds(n: u16) -> String {
    format!(
        "a development build of vtium (vtium-dev@tty{n}.service) runs on VT {n}. Run make vt-uninstall VT={n} to free it."
    )
}

pub fn masked(n: u16, prefix: &str) -> String {
    format!(
        "{prefix}@tty{n}.service is masked, so vtium leaves it alone. To unmask it, run systemctl unmask {prefix}@tty{n}.service."
    )
}

pub fn own_link_elsewhere(n: u16, to: &str) -> String {
    format!(
        "/etc/systemd/system/autovt@tty{n}.service links to {to}, so vtium cannot be enabled on VT {n}. To give VT {n} to vtium, remove that link first."
    )
}

pub fn own_link_not_link(n: u16) -> String {
    format!(
        "vtium does not replace /etc/systemd/system/autovt@tty{n}.service: it is a file, not a link, so someone wrote it on purpose."
    )
}

pub fn already_given(n: u16, mode: Mode) -> String {
    format!(
        "vtium is already enabled in {} on VT {n}. Nothing changed.",
        mode_name(mode)
    )
}

/// What `enable` says for a VT it moved from the other mode.
pub fn moved(n: u16, mode: Mode) -> String {
    let (from, to) = match mode {
        Mode::Direct => ("seat mode", "direct mode"),
        Mode::Seat => ("direct mode", "seat mode"),
    };
    format!("VT {n} moves from {from} to {to}: the unit of {from} is disabled there.")
}

/// What `enable N` says once VT `n` is vtium's in `mode`; `running` when
/// vtium runs on VT `n` in that mode already, so there is nothing to start
/// and no getty to stop.
pub fn given(n: u16, mode: Mode, running: bool) -> String {
    let unit = mode.prefix();
    let switch = match n {
        // logind never starts a console on VT 1.
        1 => String::new(),
        super::RESERVED_VT if running => String::new(),
        super::RESERVED_VT => format!(
            ", and logind starts it when you switch to VT {n} (logind's reserved VT; a login prompt waiting there is stopped, but no session ends)"
        ),
        _ => format!(", and logind starts it when you switch to VT {n} while nothing runs there"),
    };
    let now = if running {
        " vtium already runs there.".to_owned()
    } else {
        format!(
            " To start it now, run systemctl start {unit}@tty{n}.service; that ends every session on VT {n}."
        )
    };
    format!(
        "vtium is enabled in {} on VT {n}. systemd starts it there at the next boot{switch}.{now}",
        mode_name(mode)
    )
}

pub fn not_given(n: u16) -> String {
    format!("vtium is not enabled on VT {n}. Nothing changed.")
}

/// What `disable N` says; `running` names the units of vtium that run on
/// VT `n`.
pub fn taken_back(n: u16, running: &[String]) -> String {
    let stop = match running.first() {
        Some(unit) => format!(
            " It keeps running until it stops. To stop it now, run systemctl stop {unit}; that ends every session on VT {n}."
        ),
        None => String::new(),
    };
    format!("vtium is disabled on VT {n} from its next start.{stop}")
}

pub fn template_still_vtium(n: u16, unit: &str) -> String {
    format!(
        "logind still starts vtium on VT {n}, because /etc/systemd/system/autovt@.service links to {unit}@.service. Run vtium console disable --all to undo that."
    )
}

pub fn already_all(mode: Mode) -> String {
    format!(
        "vtium is already enabled in {} on every VT. Nothing changed.",
        mode_name(mode)
    )
}

pub fn given_all(mode: Mode, vt1: bool, vt6: bool) -> String {
    let unit = mode.prefix();
    let boot = if vt1 {
        "systemd starts it on VT 1 at the next boot, and logind starts it on each other VT"
    } else {
        "logind starts it on every VT except VT 1"
    };
    let now = if vt1 {
        format!(
            " To start vtium on VT 1 now, run systemctl start {unit}@tty1.service; that ends every session there."
        )
    } else {
        String::new()
    };
    let six = if vt6 {
        format!(
            " On VT {0}, logind's reserved VT, logind starts vtium the next time you switch to it; a login prompt waiting there is stopped, but no session ends.",
            super::RESERVED_VT
        )
    } else {
        String::new()
    };
    format!(
        "vtium is enabled in {} on every VT from its next start: {boot} when you switch to it while nothing runs there. A login prompt running now on VTs 2 to 5 keeps its VT until it stops.{six}{now}",
        mode_name(mode)
    )
}

pub fn template_masked(prefix: &str) -> String {
    format!(
        "{prefix}@.service is masked, so vtium leaves it alone. To unmask it, run systemctl unmask {prefix}@.service."
    )
}

/// Who holds a VT, in words: "the display manager sddm.service", "the
/// graphical session of aaron (sddm, wayland)".
fn who(holder: &Holder) -> String {
    match holder {
        Holder::Unit {
            unit,
            display_manager: true,
            ..
        } => format!("the display manager {unit}"),
        Holder::Unit { unit, .. } => unit.clone(),
        Holder::Console { unit } => format!("another console, {unit},"),
        Holder::Session {
            user,
            service,
            kind,
        } if kind == "greeter" => format!("the greeter of {user} ({service})"),
        Holder::Session {
            user,
            service,
            kind,
        } => format!("the graphical session of {user} ({service}, {kind})"),
    }
}

/// What holds VT `n` and why, a clause for each holder: "the greeter of
/// sddm (sddm-greeter) uses VT 1 now".
fn holders(n: u16, list: &[Holder]) -> String {
    list.iter()
        .map(|h| match h {
            Holder::Unit { reason, .. } => {
                format!("{} uses VT {n} at boot{}", who(h), reason_tail(n, reason))
            }
            _ => format!("{} uses VT {n} now", who(h)),
        })
        .collect::<Vec<_>>()
        .join("; and ")
}

/// Why a unit uses VT `n` at boot, as the end of a sentence.
fn reason_tail(n: u16, reason: &Reason) -> String {
    match reason {
        Reason::Convention => ": a display manager is enabled, and by convention VT 1 belongs to the display manager and its login prompt, whatever VT it uses now".to_owned(),
        Reason::Conflicts => format!(
            ": its unit lists getty@tty{n}.service in Conflicts=, so it takes the VT from the login prompt"
        ),
        Reason::TtyPath => format!(": its unit runs on /dev/tty{n} (TTYPath=)"),
        Reason::Instance => ": it is enabled at boot on that VT".to_owned(),
        Reason::Configured(s) => format!(", because of {}: {}", s.source, s.effect),
    }
}

pub fn graphics_hold(n: u16, list: &[Holder]) -> String {
    format!(
        "{}. vtium is not enabled on VT {n}, so that it never takes the VT or the display away. Give vtium another VT.",
        holders(n, list)
    )
}

/// What SDDM's settings say, in words.
pub mod sddm {
    pub fn minimum_vt(vt: u16) -> String {
        format!("SDDM before 0.20 starts its login screen on VT {vt} or the first free VT after it")
    }
}

/// What LightDM's settings say, in words.
pub mod lightdm {
    pub const DEFAULT: &str = "LightDM's built-in minimum-vt=7, which no file changes";

    pub fn effect(vt: u16) -> String {
        format!("LightDM starts its first login screen on VT {vt}")
    }

    pub fn not_a_number(value: &str, file: &str) -> String {
        format!("minimum-vt={value} in {file}, which LightDM reads as 1")
    }
}

/// What greetd's settings say, in words.
pub mod greetd {
    pub fn effect(vt: u16) -> String {
        format!("greetd runs its login screen on VT {vt}")
    }

    pub fn current() -> String {
        "greetd runs its login screen on the VT shown when it starts, which is VT 1 at boot"
            .to_owned()
    }

    pub fn command_line(value: &str, unit: &str) -> String {
        format!("--vt {value} on the ExecStart= line of {unit}")
    }
}

pub fn graphics_on_reserved_vt(n: u16, list: &[Holder]) -> String {
    format!(
        "{}. VT {n} is logind's reserved VT: logind starts autovt@tty{n}.service every time you switch to it, even while the VT is in use. If autovt@.service linked to vtium, vtium would start there beside it. Move that program to another VT, or give vtium VTs one at a time with vtium console enable N.",
        holders(n, list)
    )
}

/// Who is logged in on a VT, in words: "the login session of carol
/// (login, tty)".
fn logins(list: &[Holder]) -> String {
    list.iter()
        .map(|h| match h {
            Holder::Session {
                user,
                service,
                kind,
            } => format!("the login session of {user} ({service}, {kind})"),
            other => who(other),
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Why a VT cannot be given to vtium while `what` is on VT `n`, logind's
/// reserved VT, and what to do: the `cause`, which says what the next
/// switch to VT `n` would do, then the `fix`, each a whole message.
fn reserved_vt(cause: String, fix: String) -> String {
    format!("{cause} {fix}")
}

pub fn login_on_reserved_vt(n: u16, list: &[Holder], all: bool) -> String {
    let who = logins(list);
    let run = if list.len() > 1 { "run" } else { "runs" };
    let cause = if all {
        format!(
            "{who} {run} on VT {n}, logind's reserved VT: logind starts autovt@tty{n}.service every time you switch to it, even while the VT is in use. With autovt@.service linked to vtium, the next switch to VT {n} would start vtium there, and that would stop the login prompt and end every session on the VT."
        )
    } else {
        format!(
            "{who} {run} on VT {n}, logind's reserved VT: logind starts autovt@tty{n}.service every time you switch to it, even while the VT is in use. With autovt@tty{n}.service linked to vtium, the next switch to VT {n} would start vtium there, and that would stop the login prompt and end every session on the VT."
        )
    };
    let fix = format!(
        "Log out of VT {n} first; the login prompt then just waits, and nothing ends when it stops."
    );
    reserved_vt(cause, fix)
}

pub fn kmscon_on_reserved_vt(n: u16) -> String {
    let cause = format!(
        "kmscon (kmsconvt@tty{n}.service) is enabled or running on VT {n}, logind's reserved VT: logind starts autovt@tty{n}.service every time you switch to it, even while the VT is in use. With autovt@.service linked to vtium, the next switch to VT {n} would start vtium there, and that would stop kmscon."
    );
    let fix = format!(
        "Free VT {n} with systemctl disable --now kmsconvt@tty{n}.service, which ends every session on it, or keep it for kmscon with a link of its own: ln -s /usr/lib/systemd/system/kmsconvt@.service /etc/systemd/system/autovt@tty{n}.service."
    );
    reserved_vt(cause, fix)
}

pub fn dev_on_reserved_vt(n: u16) -> String {
    let cause = format!(
        "a development build of vtium (vtium-dev@tty{n}.service) is enabled or running on VT {n}, logind's reserved VT: logind starts autovt@tty{n}.service every time you switch to it, even while the VT is in use. With autovt@.service linked to vtium, the next switch to VT {n} would start vtium there, and that would stop the development build."
    );
    let fix = format!("Run make vt-uninstall VT={n} to free it.");
    reserved_vt(cause, fix)
}

pub fn masked_under_template(n: u16, reserved: bool) -> String {
    let when = if reserved {
        format!(
            "every time you switch to VT {n}, logind's reserved VT, where logind starts autovt@tty{n}.service even while the VT is in use"
        )
    } else {
        format!(
            "when you switch to VT {n} while nothing runs there, since no getty is enabled there to hold the VT"
        )
    };
    format!(
        "Nothing changed: vtium@tty{n}.service is masked, but a mask does not apply to a unit loaded through a link, so with autovt@.service linked to vtium, logind would still start vtium {when}. Unmask it with systemctl unmask vtium@tty{n}.service, or give VT {n} its own link, /etc/systemd/system/autovt@tty{n}.service, to the console it should have."
    )
}

pub fn kept_masked(n: u16, getty: bool) -> String {
    let getty = if getty {
        ", and so is its login prompt"
    } else {
        ""
    };
    format!(
        "VT {n} is unchanged{getty}: vtium@tty{n}.service is masked. To unmask it, run systemctl unmask vtium@tty{n}.service."
    )
}

pub fn kept_masked_own_link(n: u16) -> String {
    format!(
        "VT {n} is unchanged, and still runs vtium: vtium@tty{n}.service is masked, but VT {n}'s own link, /etc/systemd/system/autovt@tty{n}.service, points to vtium@.service, and a mask does not apply to a unit loaded through a link. Run vtium console disable {n} to undo that."
    )
}

pub fn kept_graphics(n: u16, list: &[Holder]) -> String {
    format!("VT {n} is unchanged: {}.", holders(n, list))
}

/// What holds a VT, in a cell of `status`'s table: "display manager
/// (sddm.service)", "cage@tty1", "login screen of sddm (sddm-greeter)".
pub fn holder_cell(h: &Holder) -> String {
    match h {
        Holder::Unit {
            unit,
            display_manager: true,
            ..
        } => format!("display manager ({unit})"),
        Holder::Unit { unit, .. } | Holder::Console { unit } => {
            unit.strip_suffix(".service").unwrap_or(unit).to_owned()
        }
        Holder::Session {
            user,
            service,
            kind,
        } if kind == "greeter" => format!("login screen of {user} ({service})"),
        Holder::Session {
            user,
            service,
            kind,
        } => format!("graphical session of {user} ({service}, {kind})"),
    }
}

/// Why VT `n` is held, a line under `status`'s table.
pub fn status_held(n: u16, list: &[Holder]) -> String {
    format!("VT {n}: {}.", holders(n, list))
}

pub fn cannot_ask_logind(line: &str, why: &str) -> String {
    format!(
        "vtium is not enabled on any VT: it cannot ask logind which graphical sessions run (loginctl {line}: {why}), so it cannot tell which VTs they use."
    )
}

pub fn status_display_manager(dm: &str) -> String {
    format!(
        "The display manager is {dm} (/etc/systemd/system/display-manager.service links to it)."
    )
}

pub fn kept_dev(n: u16) -> String {
    format!(
        "VT {n} stays with the development build of vtium (vtium-dev@tty{n}). Run make vt-uninstall VT={n} to free it."
    )
}

pub fn kept_own_link(n: u16, unit: &str) -> String {
    format!(
        "VT {n} stays with {unit}@tty{n}: its own link, /etc/systemd/system/autovt@tty{n}.service, points to {unit}@.service."
    )
}

pub fn kept_high_getty(n: u16) -> String {
    format!(
        "VT {n} keeps its login prompt (getty@tty{n}): logind starts consoles by itself only on VTs 1 to 6. Run vtium console enable {n} to give VT {n} to vtium."
    )
}

pub const NOT_GIVEN_ANY: &str = "vtium is not enabled on any VT. Nothing changed.";

pub const TAKEN_BACK_ALL: &str = "vtium is disabled on every VT from its next start. The login prompts vtium replaced are enabled again, and autovt@.service points where it pointed before.";

pub fn still_running(vts: &[u16]) -> String {
    let list = vts
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "vtium keeps running on VT {list} until it stops. To stop it now, run systemctl stop vtium@ttyN.service, or vtium-seat@ttyN.service in seat mode; that ends every session on that VT."
    )
}

pub fn template_none() -> String {
    "On a VT with no link of its own, logind starts nothing: no unit directory has autovt@.service."
        .to_owned()
}

pub fn template_not_link() -> String {
    "On a VT with no link of its own, logind starts autovt@.service, which is a unit file, not a link."
        .to_owned()
}

pub fn template_names(unit: &str) -> String {
    let who = match unit {
        "vtium" => "vtium in direct mode",
        "vtium-seat" => "vtium in seat mode",
        "getty" => "a login prompt (getty)",
        "kmsconvt" => "kmscon",
        "vtium-dev" => "the development build of vtium",
        _ => "another console",
    };
    format!(
        "On a VT with no link of its own, logind starts {who}: autovt@.service links to {unit}@.service."
    )
}

pub const STATUS_HEADER: [&str; 4] = ["VT", "at boot", "when switched to", "running"];

pub fn masked_cell(n: u16) -> String {
    format!("vtium@tty{n} masked")
}

pub fn recorded_cell(record: &super::Record) -> &'static str {
    match record {
        super::Record::NoGetty => "no getty to restore",
        _ => "getty to restore",
    }
}

pub fn fallback_one_vt() -> String {
    "fallback needs one VT number: the VT where vtium failed".to_owned()
}

pub fn fallback_cannot_tell(n: u16, why: &str) -> String {
    format!(
        "vtium on VT {n} failed, and VT {n} has no console now. No login prompt was started, because vtium cannot tell whether starting getty@tty{n}.service would stop another program. {why}"
    )
}

pub fn fallback_short_answer(units: &[String]) -> String {
    format!(
        "systemctl show did not report the state of {}.",
        units.join(", ")
    )
}

pub fn fallback_held_back(n: u16, units: &[super::fallback::UnitState]) -> String {
    let list = units
        .iter()
        .map(|u| {
            let mut state = vec![u.active.clone()];
            if !u.file.is_empty() {
                state.push(u.file.clone());
            }
            if !u.job.is_empty() {
                state.push(format!("job {}", u.job));
            }
            format!("{} ({})", u.id, state.join(", "))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "vtium on VT {n} failed, and VT {n} has no console now. No login prompt was started, because getty@tty{n}.service would stop {list}. Log in on another VT or over ssh, and restart vtium with systemctl restart vtium@tty{n}.service."
    )
}

pub fn fallback_starting(n: u16) -> String {
    format!(
        "vtium on VT {n} failed. Starting a login prompt (getty@tty{n}.service) in its place; this stops no other program."
    )
}

/// What plain `enable` says: what vtium finds on this machine, the mode it
/// recommends and why, whether seat mode can run, and the command that
/// turns the recommended mode on for `which`, the VT named or `--all`,
/// with `sudo` before it when the user is not root.
pub fn advice(facts: &Facts, which: &str, sudo: bool) -> Vec<String> {
    let sudo = if sudo { "sudo " } else { "" };
    let mut lines = vec![
        "vtium console enable turns nothing on without a mode. This is what it finds on this machine:".to_owned(),
        "- systemd runs as PID 1, so vtium console can turn vtium on.".to_owned(),
        if facts.seat0 {
            "- logind runs and manages seat0.".to_owned()
        } else {
            "- logind manages no seat0 here (there is no /run/systemd/seats/seat0).".to_owned()
        },
        if facts.seat_build {
            "- This vtium is built with libseat and logind, which seat mode needs.".to_owned()
        } else {
            "- This vtium is built without libseat and logind, which seat mode needs.".to_owned()
        },
    ];
    if let Some(dm) = &facts.display_manager {
        lines.push(format!(
            "- The display manager {dm} is enabled, so VT 1 and the other VTs it holds stay its own."
        ));
    }
    let mode = advice::recommend(facts);
    lines.push(String::new());
    lines.push(match mode {
        Mode::Direct => "vtium recommends direct mode for this machine. In direct mode, vtium opens the screen and the keyboards itself, and each login at vtium's prompt gets an active logind session on seat0, as a login at a getty does. So sound, removable disks and the actions that polkit allows the active session work as usual.".to_owned(),
        Mode::Seat => "vtium recommends seat mode for this machine.".to_owned(),
    });
    lines.push(match advice::seat_block(facts) {
        None => format!(
            "Seat mode can also run here. In seat mode, vtium takes the screen, the keyboards and VT switching from logind, and each login gets a logind session with no seat. polkit treats that session as inactive, and udev gives it no devices, such as sound cards. To use seat mode, run {sudo}vtium console enable --seat {which}."
        ),
        Some(SeatBlock::Build) => "Seat mode cannot run with this vtium, because it is built without libseat and logind.".to_owned(),
        Some(SeatBlock::Unit) => "Seat mode cannot run here, because no systemd unit directory has vtium-seat@.service, or it is masked.".to_owned(),
        Some(SeatBlock::Logind) => "Seat mode cannot run here, because logind manages no seat0.".to_owned(),
    });
    lines.push(String::new());
    lines.push(format!("To turn vtium on in {}, run:", mode_name(mode)));
    lines.push(format!(
        "  {sudo}vtium console enable {} {which}",
        mode.flag()
    ));
    lines
}
