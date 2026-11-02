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

pub fn kmscon_holds_vts(vts: &[u16]) -> String {
    let list = vts
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "the C reference's console, kmsconvt@, holds VT {list}. vtium on every VT is for a machine without it; free each VT with systemctl disable --now kmsconvt@ttyN.service, which ends every session there."
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
    format!(
        "VT {n} is vtium's from its next start: systemd starts vtium@tty{n} there at the next boot, and logind starts it there when you switch to VT {n} while nothing runs on it. To start it now, which ends every session on VT {n}: systemctl start vtium@tty{n}.service"
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

pub fn given_all(vt1: bool) -> String {
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
    format!(
        "Every VT is vtium's from its next start: {boot} when you switch to it while nothing runs there. A getty running now keeps its VT until it stops.{now}"
    )
}

pub const TEMPLATE_MASKED: &str = "vtium@.service is masked. A mask is the administrator's choice; systemctl unmask vtium@.service lifts it.";

pub fn display_manager_holds(dm: &str) -> String {
    format!(
        "the display manager, {dm}, starts on VT 1 at boot (/etc/systemd/system/display-manager.service links to it), and a vtium started there at boot would race it for the display. vtium console leaves VT 1 to it; give vtium another VT."
    )
}

pub fn kept_display_manager(dm: &str) -> String {
    format!(
        "VT 1 keeps the display manager, {dm}, and its getty as they are: /etc/systemd/system/display-manager.service links to it, and it starts on VT 1 at boot."
    )
}

pub fn status_display_manager(dm: &str) -> String {
    format!(
        "The display manager, {dm}, starts on VT 1 at boot: /etc/systemd/system/display-manager.service links to it."
    )
}

pub fn display_manager_cell(dm: &str) -> String {
    format!("{dm} (display manager)")
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
