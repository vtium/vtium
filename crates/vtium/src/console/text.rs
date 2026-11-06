//! What `vtium console` says, in one place, so it can be read and reworded
//! together.

use std::fmt::Debug;

use super::held::{Holder, Reason};

pub const USAGE: &str = "Usage: vtium console enable N | enable --all | disable N | disable --all | status | fallback N\n\
enable makes vtium run on VT N, or on every VT, from its next start, and disable undoes that. N is 1 to 63, or ttyN.\n\
enable and disable need root, and they start or stop nothing now. status shows what runs on each VT.\n\
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

pub const NOT_INSTALLED: &str = "vtium@.service is not installed: no systemd unit directory has it. The vtium package installs it in /usr/lib/systemd/system.";

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

pub fn masked(n: u16) -> String {
    format!(
        "vtium@tty{n}.service is masked, so vtium leaves it alone. To unmask it, run systemctl unmask vtium@tty{n}.service."
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

pub fn already_given(n: u16) -> String {
    format!("vtium is already enabled on VT {n}. Nothing changed.")
}

/// What `enable N` says once VT `n` is vtium's; `running` when vtium runs
/// on VT `n` already, so there is nothing to start and no getty to stop.
pub fn given(n: u16, running: bool) -> String {
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
            " To start it now, run systemctl start vtium@tty{n}.service; that ends every session on VT {n}."
        )
    };
    format!("vtium is enabled on VT {n}. systemd starts it there at the next boot{switch}.{now}")
}

pub fn not_given(n: u16) -> String {
    format!("vtium is not enabled on VT {n}. Nothing changed.")
}

pub fn taken_back(n: u16, running: bool) -> String {
    let stop = if running {
        format!(
            " It keeps running until it stops. To stop it now, run systemctl stop vtium@tty{n}.service; that ends every session on VT {n}."
        )
    } else {
        String::new()
    };
    format!("vtium is disabled on VT {n} from its next start.{stop}")
}

pub fn template_still_vtium(n: u16) -> String {
    format!(
        "logind still starts vtium on VT {n}, because /etc/systemd/system/autovt@.service links to vtium@.service. Run vtium console disable --all to undo that."
    )
}

pub const ALREADY_ALL: &str = "vtium is already enabled on every VT. Nothing changed.";

pub fn given_all(vt1: bool, vt6: bool) -> String {
    let boot = if vt1 {
        "systemd starts it on VT 1 at the next boot, and logind starts it on each other VT"
    } else {
        "logind starts it on every VT except VT 1"
    };
    let now = if vt1 {
        " To start vtium on VT 1 now, run systemctl start vtium@tty1.service; that ends every session there."
    } else {
        ""
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
        "vtium is enabled on every VT from its next start: {boot} when you switch to it while nothing runs there. A login prompt running now on VTs 2 to 5 keeps its VT until it stops.{six}{now}"
    )
}

pub const TEMPLATE_MASKED: &str = "vtium@.service is masked, so vtium leaves it alone. To unmask it, run systemctl unmask vtium@.service.";

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
        "vtium keeps running on VT {list} until it stops. To stop it now, run systemctl stop vtium@ttyN.service; that ends every session on that VT."
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
        "vtium" => "vtium",
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
