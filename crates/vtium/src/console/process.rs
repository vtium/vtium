//! The programs whose controlling terminal is a VT now, such as a display
//! manager's X server, which holds its VT with no logind session
//! (ADR-1500 D7).
//!
//! The kernel ties each session of processes to at most one terminal, its
//! controlling terminal, and `/proc/PID/stat` gives that terminal's device
//! number as its seventh field, `tty_nr` (proc(5)). The file is readable
//! by every user, so `status` sees it without root. An X server that
//! starts on a VT makes the VT its controlling terminal, as padnoir's
//! `/usr/lib/Xorg ... vt2` did (measured: `ps` showed it on `tty2`), and so
//! does a getty, a login shell and a debug shell. `/proc/PID/cgroup` names
//! the systemd unit the process runs in, so vtium can tell a program that
//! another rule already accounts for from one that holds its VT on its own
//! ([`super::Run::judge_processes`]).
//!
//! A process the kernel hides from the caller, under a `/proc` mounted
//! with `hidepid`, is not seen, and neither is a program that has the VT
//! open without making it its controlling terminal; such a program takes
//! the VT through logind, whose session vtium reads instead.

use std::collections::BTreeMap;
use std::path::Path;

/// `TTY_MAJOR`, the device major number of the VTs `/dev/tty1` to
/// `/dev/tty63`; `/dev/ttyS0` and up share it from minor 64.
const TTY_MAJOR: u32 = 4;

/// The longest program name vtium shows.
const PROGRAM_MAX: usize = 80;

/// A process whose controlling terminal is a VT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    /// The VT that is its controlling terminal.
    pub vt: u16,
    /// Whether it leads its session, as the process that made the VT its
    /// controlling terminal does.
    pub leader: bool,
    /// Its command's first word, or its short name when it has no command
    /// line.
    pub program: String,
    /// The systemd unit it runs in, such as `sddm.service` or
    /// `session-3.scope`, when its control group names one.
    pub unit: Option<String>,
}

/// The VT a `tty_nr` names, from its major and minor numbers as the
/// kernel's `new_encode_dev` packs them.
fn vt(tty_nr: u64) -> Option<u16> {
    let major = (tty_nr >> 8) & 0xfff;
    let minor = (tty_nr & 0xff) | ((tty_nr >> 12) & 0xfff00);
    (major == u64::from(TTY_MAJOR))
        .then(|| u16::try_from(minor).ok())
        .flatten()
        .filter(|n| (1..=super::LAST_VT).contains(n))
}

/// The session id and `tty_nr` in a `/proc/PID/stat` line. The short name
/// in parentheses may hold spaces and parentheses, so the fields are read
/// after its last `)`.
fn stat(line: &str) -> Option<(u32, u64)> {
    let (_, rest) = line.rsplit_once(')')?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // state, ppid, pgrp, session, tty_nr.
    let session = fields.get(3)?.parse().ok()?;
    let tty_nr = fields.get(4)?.parse().ok()?;
    Some((session, tty_nr))
}

/// The short name in a `/proc/PID/stat` line.
fn comm(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once('(')?;
    rest.rsplit_once(')').map(|(name, _)| name)
}

/// The systemd unit a `/proc/PID/cgroup` file names: the last part of the
/// unified hierarchy's path (`0::/system.slice/sddm.service`), or of
/// systemd's own hierarchy on a hybrid host (`1:name=systemd:...`), that
/// ends in `.service` or `.scope`.
pub fn unit(cgroup: &str) -> Option<String> {
    let path = cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .or_else(|| {
            cgroup
                .lines()
                .find_map(|l| l.split_once(":name=systemd:").map(|(_, p)| p))
        })?;
    path.rsplit('/')
        .find(|part| part.ends_with(".service") || part.ends_with(".scope"))
        .map(str::to_owned)
}

/// `text` with each control character shown as `?`, cut to
/// [`PROGRAM_MAX`] characters, since a process sets its own command line
/// and vtium prints it to a terminal.
fn shown(text: &str) -> String {
    text.chars()
        .take(PROGRAM_MAX)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Every process under `root`'s `/proc` whose controlling terminal is a
/// VT, read with the root's [`super::names`].
pub fn on_vts(root: &Path) -> Vec<Process> {
    let proc = root.join("proc");
    let mut out = Vec::new();
    for name in super::names(&proc) {
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let dir = proc.join(&name);
        let Ok(line) = std::fs::read_to_string(dir.join("stat")) else {
            continue;
        };
        let Some((session, Some(vt))) = stat(&line).map(|(s, t)| (s, vt(t))) else {
            continue;
        };
        let command = std::fs::read(dir.join("cmdline")).unwrap_or_default();
        let first = command.split(|&b| b == 0).next().unwrap_or_default();
        let program = match String::from_utf8_lossy(first) {
            p if !p.is_empty() => shown(&p),
            _ => shown(comm(&line).unwrap_or("?")),
        };
        let unit = std::fs::read_to_string(dir.join("cgroup"))
            .ok()
            .and_then(|c| unit(&c));
        out.push(Process {
            pid,
            vt,
            leader: session == pid,
            program,
            unit,
        });
    }
    out
}

/// One process for each VT and unit, so that a program and its children
/// are named once: the session leader, else the lowest process id.
pub fn one_each(processes: Vec<Process>) -> Vec<Process> {
    let mut each: BTreeMap<(u16, Option<String>), Process> = BTreeMap::new();
    for p in processes {
        let key = (p.vt, p.unit.clone());
        let better = |old: &Process| (!p.leader, p.pid) < (!old.leader, old.pid);
        if each.get(&key).is_none_or(better) {
            each.insert(key, p);
        }
    }
    each.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_controlling_terminal_names_a_vt_only_for_tty1_to_tty63() {
        assert_eq!(vt(1026), Some(2));
        assert_eq!(vt(1024 + 63), Some(63));
        // tty0, ttyS0 (minor 64), a pty (major 136) and no terminal.
        assert_eq!(vt(1024), None);
        assert_eq!(vt(1024 + 64), None);
        assert_eq!(vt((136 << 8) | 1), None);
        assert_eq!(vt(0), None);
    }

    #[test]
    fn a_stat_line_is_read_past_a_short_name_with_parentheses() {
        let line = "1207 (Xorg (x) 2) S 1186 1207 1207 1026 1207 4194560";
        assert_eq!(stat(line), Some((1207, 1026)));
        assert_eq!(comm(line), Some("Xorg (x) 2"));
        assert_eq!(stat("1 (init)"), None);
    }

    #[test]
    fn a_control_group_names_its_unit() {
        assert_eq!(
            unit("0::/system.slice/sddm.service\n").as_deref(),
            Some("sddm.service")
        );
        assert_eq!(
            unit("0::/user.slice/user-1000.slice/session-8.scope\n").as_deref(),
            Some("session-8.scope")
        );
        // A unit that delegates its control group keeps its name above.
        assert_eq!(
            unit("0::/system.slice/foo.service/payload\n").as_deref(),
            Some("foo.service")
        );
        assert_eq!(
            unit("12:pids:/\n1:name=systemd:/system.slice/getty@tty3.service\n").as_deref(),
            Some("getty@tty3.service")
        );
        assert_eq!(unit("0::/\n"), None);
    }

    #[test]
    fn a_program_name_is_shown_without_control_characters() {
        assert_eq!(shown("evil\x1b[2J"), "evil?[2J");
        assert_eq!(shown(&"x".repeat(200)).len(), PROGRAM_MAX);
    }
}
