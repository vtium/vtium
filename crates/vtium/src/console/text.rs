//! What `vtium console` says, in one place, so it can be read and reworded
//! together.

use std::fmt::Debug;

pub const USAGE: &str = "Usage: vtium console enable N | enable --all | disable N | disable --all | status\n\
Gives VT N, or every VT, to vtium@.service at its next start, and takes it back. N is 1 to 63, or ttyN.\n\
enable and disable need root; they start and stop nothing. status shows what each VT runs.\n";

pub const NO_VERB: &str = "name a verb: enable, disable or status";

pub fn not_utf8(word: impl Debug) -> String {
    format!("{word:?} is not UTF-8")
}

pub fn extra_word(word: &str) -> String {
    format!("{word} is one word too many")
}

pub fn needs_which(verb: &str) -> String {
    format!("{verb} needs a VT's number or --all")
}

pub fn unknown_verb(word: &str) -> String {
    format!("{word} is no verb of vtium console: use enable, disable or status")
}

pub fn unknown_option(word: &str) -> String {
    format!("unknown option {word}")
}

pub fn not_a_vt(word: &str) -> String {
    format!("{word} is no VT 1 to {}", super::LAST_VT)
}

pub const NO_SYSTEMD: &str = "this host was not booted with systemd (it has no /run/systemd/system), so there are no units to change. Its init runs vtium on a VT where it ran a getty, with vtium --vt=ttyN --no-switchvt; see docs/systemd.md.";

pub fn needs_root(words: &str) -> String {
    format!(
        "this changes systemd's units, which needs root. Run it as root, for example: sudo vtium console {words}"
    )
}

pub const NOT_INSTALLED: &str = "vtium@.service is in none of systemd's unit directories. The vtium package installs it in /usr/lib/systemd/system.";

pub fn cannot_run(line: &str, why: &str) -> String {
    format!("cannot run systemctl {line}: {why}")
}

pub fn failed(line: &str, stderr: &str) -> String {
    let said = stderr.trim_end();
    format!(
        "systemctl {line} failed{}{said}\nNothing after that step was done. Running the same command again finishes the change, and the opposite verb undoes what was done.",
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
    format!("that getty@tty{n}.service was enabled, linked to {to},")
}

pub fn no_getty(n: u16) -> String {
    format!("that no getty was enabled on VT {n}")
}

pub const TEMPLATE_WHAT: &str = "where autovt@.service linked";

pub fn kmscon_holds(n: u16, own_link: bool) -> String {
    let link = if own_link {
        format!(", and rm /etc/systemd/system/autovt@tty{n}.service, which disable leaves")
    } else {
        String::new()
    };
    format!(
        "the C reference's console, kmsconvt@tty{n}.service, holds VT {n}. Free it first with systemctl disable --now kmsconvt@tty{n}.service, which ends every session on VT {n}{link}."
    )
}

pub const KMSCON_TEMPLATE: &str = "the C reference's console holds every VT: autovt@.service links to kmsconvt@.service. vtium on every VT is for a machine without it; systemctl disable kmsconvt@.service removes its links. vtium console enable N gives one VT to vtium beside it.";

pub fn kept_kmscon(n: u16) -> String {
    format!(
        "VT {n} keeps the C reference's console, kmsconvt@tty{n}: it is enabled, linked or running there. systemctl disable --now kmsconvt@tty{n}.service frees it, ending every session there."
    )
}

pub const TEMPLATE_NOT_LINK: &str = "/etc/systemd/system/autovt@.service is a file, not a link, so it is someone's own unit, and vtium does not replace it.";

pub fn dev_holds(n: u16) -> String {
    format!(
        "the development install, vtium-dev@tty{n}.service, holds VT {n}. make vt-uninstall VT={n} frees it."
    )
}

pub fn masked(n: u16) -> String {
    format!(
        "vtium@tty{n}.service is masked. A mask is the administrator's choice; systemctl unmask vtium@tty{n}.service lifts it."
    )
}

pub fn own_link_elsewhere(n: u16, to: &str) -> String {
    format!(
        "/etc/systemd/system/autovt@tty{n}.service links to {to}, so systemctl enable vtium@tty{n}.service would fail. Remove the link first if VT {n} is to be vtium's."
    )
}

pub fn own_link_not_link(n: u16) -> String {
    format!(
        "/etc/systemd/system/autovt@tty{n}.service is a file, not a link, so it is someone's own unit, and vtium does not replace it."
    )
}

pub fn already_given(n: u16) -> String {
    format!("VT {n} is vtium's already: nothing changed.")
}

pub fn given(n: u16) -> String {
    let switch = match n {
        // logind never starts a console on VT 1.
        1 => String::new(),
        super::RESERVED_VT => format!(
            ", and logind starts it there at the next switch to VT {n}, logind's reserved VT, which stops a getty waiting at its login prompt there; that ends no session, since no one is logged in there"
        ),
        _ => format!(
            ", and logind starts it there when you switch to VT {n} while nothing runs on it"
        ),
    };
    format!(
        "VT {n} is vtium's from its next start: systemd starts vtium@tty{n} there at the next boot{switch}. To start it now, which ends every session on VT {n}: systemctl start vtium@tty{n}.service"
    )
}

pub fn not_given(n: u16) -> String {
    format!("vtium is not enabled on VT {n}: nothing changed.")
}

pub fn taken_back(n: u16, running: bool) -> String {
    let stop = if running {
        format!(
            " vtium@tty{n} keeps running until it stops. To stop it now, which ends every session on VT {n}: systemctl stop vtium@tty{n}.service"
        )
    } else {
        String::new()
    };
    format!("VT {n} is no longer vtium's from its next start.{stop}")
}

pub fn template_still_vtium(n: u16) -> String {
    format!(
        "logind still starts vtium on VT {n}, through /etc/systemd/system/autovt@.service, which links to vtium@.service. vtium console disable --all takes that back."
    )
}

pub const ALREADY_ALL: &str = "Every VT is vtium's already: nothing changed.";

pub fn given_all(vt1: bool, vt6: bool) -> String {
    let boot = if vt1 {
        "systemd starts vtium@tty1 at the next boot, and logind starts vtium on each other VT"
    } else {
        "logind starts vtium on each VT but VT 1"
    };
    let now = if vt1 {
        " To start vtium on VT 1 now, which ends every session there: systemctl start vtium@tty1.service"
    } else {
        ""
    };
    let six = if vt6 {
        format!(
            " On VT {0}, logind's reserved VT, logind starts vtium at the next switch to it, which stops a getty waiting at its login prompt there; that ends no session, since no one is logged in there.",
            super::RESERVED_VT
        )
    } else {
        String::new()
    };
    format!(
        "Every VT is vtium's from its next start: {boot} when you switch to it while nothing runs there. A getty running now on VTs 2 to 5 keeps its VT until it stops.{six}{now}"
    )
}

pub const TEMPLATE_MASKED: &str = "vtium@.service is masked. A mask is the administrator's choice; systemctl unmask vtium@.service lifts it.";

/// Who holds a VT for graphics, in words: "the display manager
/// sddm.service" or "the graphical session of aaron (sddm, wayland)".
fn holder(holder: &super::Holder) -> String {
    match holder {
        super::Holder::DisplayManager(dm) => format!("the display manager {dm}"),
        super::Holder::Session {
            user,
            service,
            kind,
        } if kind == "greeter" => {
            format!("the greeter of {user} ({service})")
        }
        super::Holder::Session {
            user,
            service,
            kind,
        } => {
            format!("the graphical session of {user} ({service}, {kind})")
        }
    }
}

fn holders(list: &[super::Holder]) -> String {
    list.iter().map(holder).collect::<Vec<_>>().join(" and ")
}

pub fn graphics_hold(n: u16, list: &[super::Holder]) -> String {
    format!(
        "{} holds VT {n}, and a vtium there would race it for the display. vtium console leaves VT {n} to it; give vtium another VT.",
        holders(list)
    )
}

pub fn graphics_on_reserved_vt(n: u16, list: &[super::Holder]) -> String {
    format!(
        "{} holds VT {n}, logind's reserved VT, where logind starts autovt@tty{n}.service on every switch even while the VT is in use. With autovt@.service linked to vtium, a vtium would start beside it there. Move it to another VT, or give vtium VTs one at a time with vtium console enable N.",
        holders(list)
    )
}

/// Who is logged in on a VT, in words: "the login session of carol
/// (login, tty)".
fn logins(list: &[super::Holder]) -> String {
    list.iter()
        .map(|h| match h {
            super::Holder::Session {
                user,
                service,
                kind,
            } => format!("the login session of {user} ({service}, {kind})"),
            super::Holder::DisplayManager(dm) => format!("the display manager {dm}"),
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Why a VT cannot be given to vtium while `what` is on VT `n`, logind's
/// reserved VT: `by` says what makes logind start vtium there, `then` what
/// that start does, and `fix` how to free the VT.
fn reserved_vt(n: u16, what: &str, by: &str, then: &str, fix: &str) -> String {
    format!(
        "{what} on VT {n}, logind's reserved VT, where logind starts autovt@tty{n}.service at every switch to it, even while the VT is in use. {by}, the next switch to VT {n} would start vtium there, and that {then}. {fix}"
    )
}

/// What makes logind start vtium on the reserved VT: VT `n`'s own link
/// for `enable N`, or the template link for `enable --all`.
fn reserved_by(n: u16, all: bool) -> String {
    if all {
        "With autovt@.service linked to vtium".to_owned()
    } else {
        format!("With autovt@tty{n}.service linked to vtium")
    }
}

pub fn login_on_reserved_vt(n: u16, list: &[super::Holder], all: bool) -> String {
    let is = if list.len() > 1 { "are" } else { "is" };
    reserved_vt(
        n,
        &format!("{} {is}", logins(list)),
        &reserved_by(n, all),
        "stops the getty and ends every session on the VT",
        &format!(
            "Log out of VT {n} first: the getty there then waits at its login prompt, and nothing ends when it stops."
        ),
    )
}

pub fn kmscon_on_reserved_vt(n: u16) -> String {
    reserved_vt(
        n,
        &format!("the C reference's console, kmsconvt@tty{n}.service, is enabled or running"),
        &reserved_by(n, true),
        "stops kmscon through vtium's Conflicts=",
        &format!(
            "Free VT {n} with systemctl disable --now kmsconvt@tty{n}.service, which ends every session on it, or keep it for kmscon with a link of its own: ln -s /usr/lib/systemd/system/kmsconvt@.service /etc/systemd/system/autovt@tty{n}.service."
        ),
    )
}

pub fn dev_on_reserved_vt(n: u16) -> String {
    reserved_vt(
        n,
        &format!("the development install, vtium-dev@tty{n}.service, is enabled or running"),
        &reserved_by(n, true),
        "stops the development install through vtium's Conflicts=",
        &format!("make vt-uninstall VT={n} frees it."),
    )
}

pub fn masked_under_template(n: u16, reserved: bool) -> String {
    let when = if reserved {
        format!(
            "at every switch to VT {n}, logind's reserved VT, where logind starts autovt@tty{n}.service even while the VT is in use"
        )
    } else {
        format!(
            "when you switch to VT {n} while nothing runs there, since no getty is enabled there to hold the VT"
        )
    };
    format!(
        "vtium@tty{n}.service is masked, but with autovt@.service linked to vtium, logind would still load vtium through that link {when}: the mask does not reach a unit loaded through a link (measured). A mask is the administrator's choice, so vtium changes nothing: lift it with systemctl unmask vtium@tty{n}.service, or give VT {n} a link of its own, /etc/systemd/system/autovt@tty{n}.service, to the console it is to have."
    )
}

pub fn kept_masked(n: u16) -> String {
    format!(
        "VT {n} is left as it is, its getty included: vtium@tty{n}.service is masked, and systemctl unmask vtium@tty{n}.service lifts the mask."
    )
}

pub fn kept_graphics(n: u16, list: &[super::Holder]) -> String {
    format!("VT {n} is left as it is: {} holds it.", holders(list))
}

pub fn holder_cell(h: &super::Holder) -> String {
    match h {
        super::Holder::DisplayManager(dm) => {
            format!("display manager ({dm})")
        }
        super::Holder::Session {
            user,
            service,
            kind,
        } if kind == "greeter" => {
            format!("greeter of {user} ({service})")
        }
        super::Holder::Session {
            user,
            service,
            kind,
        } => {
            format!("graphical session of {user} ({service}, {kind})")
        }
    }
}

pub fn cannot_ask_logind(line: &str, why: &str) -> String {
    format!(
        "cannot ask logind which graphical sessions run (loginctl {line}: {why}), so vtium cannot tell which VTs they hold, and gives vtium no VT."
    )
}

pub fn status_display_manager(dm: &str) -> String {
    format!("The display manager is {dm}: /etc/systemd/system/display-manager.service links to it.")
}

pub fn kept_dev(n: u16) -> String {
    format!(
        "VT {n} keeps the development install, vtium-dev@tty{n}; make vt-uninstall VT={n} frees it."
    )
}

pub fn kept_own_link(n: u16, unit: &str) -> String {
    format!(
        "VT {n} keeps {unit}@tty{n}: its own link, /etc/systemd/system/autovt@tty{n}.service, names {unit}@.service."
    )
}

pub fn kept_high_getty(n: u16) -> String {
    format!(
        "VT {n} keeps getty@tty{n}: logind starts consoles by itself only on VTs 1 to 6, so vtium console enable {n} gives VT {n} to vtium."
    )
}

pub const NOT_GIVEN_ANY: &str = "vtium is enabled on no VT: nothing changed.";

pub const TAKEN_BACK_ALL: &str = "No VT is vtium's from its next start: the gettys vtium console took are enabled again, and autovt@.service links where it linked before.";

pub fn still_running(vts: &[u16]) -> String {
    let list = vts
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "vtium keeps running on VT {list} until it stops. To stop it now, which ends every session on that VT: systemctl stop vtium@ttyN.service"
    )
}

pub fn template_none() -> String {
    "On a VT with no link of its own, logind starts nothing: no unit directory has autovt@.service."
        .to_owned()
}

pub fn template_not_link() -> String {
    "On a VT with no link of its own, logind starts autovt@.service, a unit file of its own."
        .to_owned()
}

pub fn template_names(unit: &str) -> String {
    let who = match unit {
        "vtium" => "vtium",
        "getty" => "a getty",
        "kmsconvt" => "the C reference's console",
        "vtium-dev" => "the development install",
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
        super::Record::NoGetty => "no getty recorded",
        _ => "getty recorded",
    }
}
