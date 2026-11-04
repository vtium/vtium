//! The report's parts, each gathered from the machine through
//! [`super::read`] and written as plain lines.

use std::io;

use super::Machine;
use super::edid::Edid;
use super::read::{FILE_MAX, indented};
use super::units::{Journal, Unit, Units, revision};

/// What the opening part says: what the report holds and what to check
/// before sharing it.
pub const HOLDS: &str = "This report holds what a vtium bug report needs: the vtium builds installed and the features each was built with, the kernel and the distribution, the state of each vtium console, the displays the kernel sees, vtium's configuration, and the log of each vtium console. It leaves out the machine's hostname and each monitor's serial number. It can still name user accounts: a login writes the user's name to the log, vtium console status names the user of each graphical session, and a line of vtium.conf can hold a user's name or home directory. The logs' times show your time zone, and they hold what vtium logged about the programs it ran. Read the report through before you share it, and remove anything you would rather keep to yourself. Attach it to an issue at https://github.com/vtium/vtium/issues with what you saw and what you expected.\n";

/// The packaged console, whose `console status` and `check-config` the
/// report runs.
const VTIUM: &str = "/usr/bin/vtium";

/// The value of `key` in an os-release file's `text`, its quotes taken
/// off.
fn os_release<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().trim_matches('"').trim_matches('\''))
}

/// The system part: the kernel, the architecture, the distribution, and
/// whether systemd and logind run.
pub fn system(m: &Machine, _: &Units) -> String {
    let mut s = String::new();
    match m.text("/proc/sys/kernel/osrelease", 256) {
        Ok(r) => s.push_str(&format!("kernel: {}\n", r.trim())),
        Err(e) => s.push_str(&format!(
            "kernel: {}\n",
            m.cannot("/proc/sys/kernel/osrelease", &e)
        )),
    }
    match m.text("/proc/sys/kernel/arch", 256) {
        Ok(a) => s.push_str(&format!("architecture: {}\n", a.trim())),
        Err(_) => s.push_str(&format!(
            "architecture: {}, as vtium-config was built, since the kernel does not say\n",
            std::env::consts::ARCH
        )),
    }
    let release = ["/etc/os-release", "/usr/lib/os-release"]
        .iter()
        .find_map(|f| m.text(f, 64 * 1024).ok());
    match release {
        Some(text) => {
            let name = os_release(&text, "NAME").unwrap_or("no NAME");
            match os_release(&text, "VERSION_ID") {
                Some(v) => s.push_str(&format!("distribution: {name}, version {v}\n")),
                None => s.push_str(&format!("distribution: {name}\n")),
            }
        }
        None => s.push_str("distribution: unknown, since there is no /etc/os-release\n"),
    }
    let systemd = m.path("/run/systemd/system").is_dir();
    s.push_str(if systemd {
        "systemd: runs as the init system\n"
    } else {
        "systemd: does not run as the init system\n"
    });
    s.push_str(if m.path("/run/systemd/seats").is_dir() {
        "logind: runs\n"
    } else {
        "logind: does not run\n"
    });
    s
}

/// Runs the packaged vtium's subcommand `words` when its `--help` names
/// it, and says what it printed under `label`; or says why it did not run.
fn vtium_subcommand(m: &Machine, words: &[&str], label: &str) -> String {
    let path = m.path(VTIUM);
    if !path.is_file() {
        return format!("{VTIUM} is not installed, so {label} cannot run.\n");
    }
    let named = words.join(" ");
    let help = m.run(&path, &["--help"]).map(|r| r.out).unwrap_or_default();
    if !help.contains(&format!("vtium {named}")) {
        return format!("{label}: this vtium has no {named}\n");
    }
    match m.run(&path, words) {
        Ok(ran) => {
            let code = ran
                .code
                .map_or_else(|| "ends on a signal".to_owned(), |c| format!("exits {c}"));
            let both = format!("{}{}", ran.out, ran.err);
            let text = if both.trim().is_empty() {
                "    (it prints nothing)\n".to_owned()
            } else {
                indented(&both)
            };
            format!("{label} {code}:\n{text}")
        }
        Err(e) => format!("{label}: cannot run it: {e}\n"),
    }
}

/// The value of `key` in `systemctl show`'s output.
fn property<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
        .unwrap_or_default()
        .trim()
}

/// The consoles part: `vtium console status`, and each unit's state.
pub fn consoles(m: &Machine, units: &Units) -> String {
    let mut s = vtium_subcommand(m, &["console", "status"], "vtium console status");
    s.push('\n');
    if let Some(why) = &units.listed {
        s.push_str(why);
        s.push('\n');
    }
    if units.units.is_empty() {
        s.push_str(&format!("No vtium@, vtium-dev@ or vtium-getty@ unit is loaded, and the journal names none in {}.\n", m.boot_words()));
        return s;
    }
    let systemctl = m.program("systemctl");
    let journal = matches!(units.journal, Journal::Readable);
    for u in &units.units {
        let shown = systemctl.as_ref().map(|p| {
            m.run(
                p,
                &[
                    "show",
                    "--no-pager",
                    "-p",
                    "ActiveState",
                    "-p",
                    "SubState",
                    "-p",
                    "ActiveEnterTimestamp",
                    "-p",
                    "NRestarts",
                    &u.name,
                ],
            )
        });
        let mut line = format!("{}:", u.name);
        match shown {
            Some(Ok(ran)) => {
                let o = &ran.out;
                line.push_str(&format!(
                    " {} ({})",
                    property(o, "ActiveState"),
                    property(o, "SubState")
                ));
                let since = property(o, "ActiveEnterTimestamp");
                if !since.is_empty() {
                    line.push_str(&format!(" since {since}"));
                }
                let n = property(o, "NRestarts");
                if !n.is_empty() {
                    let times = if n == "1" { "time" } else { "times" };
                    line.push_str(&format!(", restarted {n} {times}"));
                }
            }
            Some(Err(e)) => line.push_str(&format!(" cannot run systemctl: {e}")),
            None => line.push_str(" its state is not known without systemctl"),
        }
        if journal
            && !u.name.starts_with("vtium-getty@")
            && let Some(r) = revision(m, &u.name)
        {
            line.push_str(&format!(", revision {r}"));
        }
        s.push_str(&line);
        s.push('\n');
    }
    s
}

/// The DRM cards in sysfs, by number.
fn cards(m: &Machine) -> io::Result<Vec<u32>> {
    let mut cards: Vec<u32> = m
        .names("/sys/class/drm")?
        .iter()
        .filter_map(|n| n.trim_end_matches('/').strip_prefix("card")?.parse().ok())
        .collect();
    cards.sort_unstable();
    Ok(cards)
}

/// The connectors in sysfs, of card `card` or of every card: each
/// connector's name, such as `DP-1`, and its directory's absolute path.
pub fn connectors(m: &Machine, card: Option<u32>) -> Vec<(String, String)> {
    let names = m.names("/sys/class/drm").unwrap_or_default();
    names
        .iter()
        .map(|n| n.trim_end_matches('/'))
        .filter_map(|n| {
            let rest = n.strip_prefix("card")?;
            let (number, name) = rest.split_once('-')?;
            let number: u32 = number.parse().ok()?;
            card.is_none_or(|c| c == number)
                .then(|| (name.to_owned(), format!("/sys/class/drm/{n}")))
        })
        .collect()
}

/// One line of a sysfs file of the connector or card directory `dir`.
fn line(m: &Machine, dir: &str, file: &str) -> Option<String> {
    let text = m.text(&format!("{dir}/{file}"), 4096).ok()?;
    let line = text.lines().next()?.trim().to_owned();
    (!line.is_empty()).then_some(line)
}

const NO_CARD: &str = "No DRM card is in /sys/class/drm.\n";

/// The displays part: each card's driver and render node, and each of its
/// connectors.
pub fn displays(m: &Machine, _: &Units) -> String {
    let cards = match cards(m) {
        Ok(c) if c.is_empty() => return NO_CARD.to_owned(),
        Ok(c) => c,
        Err(e) => return format!("{}\n", m.cannot("/sys/class/drm", &e)),
    };
    let mut s = String::new();
    for n in cards {
        let dir = format!("/sys/class/drm/card{n}");
        let driver = std::fs::read_link(m.path(&format!("{dir}/device/driver")))
            .ok()
            .and_then(|l| l.file_name().map(|f| f.to_string_lossy().into_owned()));
        let render = m
            .names(&format!("{dir}/device/drm"))
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.trim_end_matches('/').to_owned())
            .find(|r| r.starts_with("renderD"));
        s.push_str(&format!(
            "card{n}: {}, {}\n",
            driver.map_or_else(|| "no driver".to_owned(), |d| format!("driver {d}")),
            render.map_or_else(
                || "no render node".to_owned(),
                |r| format!("render node {r}")
            ),
        ));
        for (name, dir) in connectors(m, Some(n)) {
            let mut said = vec![
                line(m, &dir, "status").unwrap_or_else(|| "status unknown".to_owned()),
                line(m, &dir, "enabled").unwrap_or_else(|| "enabled state unknown".to_owned()),
            ];
            if let Some(mode) = line(m, &dir, "modes") {
                said.push(format!("preferred mode {mode}"));
            }
            if let Some(edid) = Edid::read(m, &dir) {
                said.push(edid.name().map_or_else(
                    || "monitor not named".to_owned(),
                    |n| format!("monitor {n}"),
                ));
                if let Some((w, h)) = edid.mm() {
                    said.push(format!("{w}x{h} mm"));
                }
            }
            s.push_str(&format!("  {name}: {}\n", said.join(", ")));
        }
    }
    s
}

/// The configuration part: vtium.conf without its comments, the files in
/// `/etc/vtium` and its fonts directory, and `vtium check-config`.
pub fn configuration(m: &Machine, _: &Units) -> String {
    let conf = "/etc/vtium/vtium.conf";
    let mut s = String::new();
    match m.text(conf, FILE_MAX) {
        Ok(text) => {
            let kept: String = text
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
                .map(|l| format!("    {l}\n"))
                .collect();
            s.push_str(&format!("{conf}, without its comments and blank lines:\n"));
            s.push_str(if kept.is_empty() {
                "    (it sets nothing)\n"
            } else {
                &kept
            });
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            s.push_str(&format!(
                "There is no {conf}, so vtium runs with its defaults.\n"
            ));
        }
        Err(e) => s.push_str(&format!("{}\n", m.cannot(conf, &e))),
    }
    for dir in ["/etc/vtium", "/etc/vtium/fonts"] {
        match m.names(dir) {
            Ok(names) if names.is_empty() => s.push_str(&format!("{dir} is empty.\n")),
            Ok(names) => s.push_str(&format!("Files in {dir}: {}\n", names.join(", "))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                s.push_str(&format!("There is no {dir}.\n"));
            }
            Err(e) => s.push_str(&format!("{}\n", m.cannot(dir, &e))),
        }
    }
    s.push_str(&vtium_subcommand(
        m,
        &["check-config"],
        "vtium check-config",
    ));
    s
}

/// The logs part: each unit's latest log lines in the boot the report
/// reads.
pub fn logs(m: &Machine, units: &Units) -> String {
    let mut s = String::from(
        "The font helper and the image helper run inside their console's unit, so their lines are in its log.\n",
    );
    match &units.journal {
        Journal::Missing => {
            s.push_str("journalctl is not on this machine, so the report holds no log.\n");
            return s;
        }
        Journal::Unreadable(said) => {
            s.push_str(&format!(
                "The journal is not readable to you. Run the report as root, with sudo vtium-config report > vtium-report.txt, or have root add your user to the systemd-journal group, whose members read the whole journal, with usermod -aG systemd-journal followed by your user name, and log in again. journalctl said: {said}\n"
            ));
            return s;
        }
        Journal::Readable => {}
    }
    let logged: Vec<&Unit> = units.units.iter().filter(|u| u.log.is_some()).collect();
    if logged.is_empty() {
        s.push_str(&format!(
            "No vtium@, vtium-dev@ or vtium-getty@ unit logged in {}.\n",
            m.boot_words()
        ));
    }
    for u in logged {
        s.push_str(&format!(
            "\n{}, its last {} lines in {}:\n{}",
            u.name,
            m.lines,
            m.boot_words(),
            indented(u.log.as_deref().unwrap_or_default())
        ));
    }
    s
}

/// The kernel display state part: debugfs's atomic state of each card, as
/// root.
pub fn kernel_state(m: &Machine, _: &Units) -> String {
    if m.euid != 0 {
        return "needs root: sudo vtium-config report > vtium-report.txt\n".to_owned();
    }
    let cards = match cards(m) {
        Ok(c) if c.is_empty() => return NO_CARD.to_owned(),
        Ok(c) => c,
        Err(e) => return format!("{}\n", m.cannot("/sys/class/drm", &e)),
    };
    let mut s = String::new();
    for n in cards {
        // debugfs names a card's directory by its minor number, and newer
        // kernels by its device's PCI address too
        let mut tried = vec![format!("/sys/kernel/debug/dri/{n}/state")];
        if let Some(pci) = std::fs::read_link(m.path(&format!("/sys/class/drm/card{n}/device")))
            .ok()
            .and_then(|l| l.file_name().map(|f| f.to_string_lossy().into_owned()))
        {
            tried.push(format!("/sys/kernel/debug/dri/{pci}/state"));
        }
        let mut first_error = None;
        let mut read = None;
        for path in &tried {
            match m.text(path, FILE_MAX) {
                Ok(text) => {
                    read = Some((path, text));
                    break;
                }
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        match (read, first_error) {
            (Some((path, text)), _) => {
                s.push_str(&format!("card{n}, from {path}:\n{}", indented(&text)));
            }
            (None, Some(e)) => s.push_str(&format!(
                "card{n}: {}\n",
                m.cannot(tried.first().map_or("", String::as_str), &e)
            )),
            (None, None) => {}
        }
    }
    s
}
