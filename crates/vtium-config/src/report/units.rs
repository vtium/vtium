//! The vtium units the report gathers: those systemd has loaded and those
//! the journal names for the boot the report reads, each with its latest
//! log lines, and whether the user can read the journal at all.

use super::Machine;
use super::read::Ran;

/// The units whose state and logs the report gathers, by the start of
/// their names.
const UNIT_PREFIXES: [&str; 4] = ["vtium@", "vtium-dev@", "vtium-getty@", "vtium-dev-getty@"];

/// What journalctl says when the user cannot read the system's journal.
const UNREADABLE: [&str; 2] = [
    "not seeing messages from other users",
    "insufficient permissions",
];

/// The units the report found, each with its log, and whether the
/// journal could be read.
pub struct Units {
    pub units: Vec<Unit>,
    pub journal: Journal,
    /// Why no unit list was read, where none was.
    pub listed: Option<String>,
}

pub struct Unit {
    pub name: String,
    /// Its latest log lines in the boot the report reads, where the
    /// journal was read.
    pub log: Option<String>,
}

pub enum Journal {
    Readable,
    /// journalctl is not on the machine.
    Missing,
    /// journalctl said the user cannot read the system's messages; what
    /// it said.
    Unreadable(String),
    /// `--previous-boot` was asked for, and the journal holds no boot
    /// before this one; what journalctl said.
    NoPreviousBoot(String),
}

impl Journal {
    /// The journal after journalctl wrote `err` to a user (`euid` not 0):
    /// unreadable when `err` says the user cannot see the system's
    /// messages, else as it was.
    fn after(self, euid: u32, err: &str) -> Journal {
        if euid != 0 && UNREADABLE.iter().any(|u| err.contains(u)) {
            Journal::Unreadable(err.lines().next().unwrap_or_default().trim().to_owned())
        } else {
            self
        }
    }
}

impl Units {
    /// The vtium units systemd has loaded and those the journal names for
    /// the boot `m` reads, with each one's log. A unit only the journal
    /// names is kept when it logged in that boot.
    pub fn of(m: &Machine) -> Units {
        let (loaded, listed) = loaded(m);
        let Some(journalctl) = m.program("journalctl") else {
            return Units {
                units: loaded
                    .into_iter()
                    .map(|name| Unit { name, log: None })
                    .collect(),
                journal: Journal::Missing,
                listed,
            };
        };
        if m.previous_boot
            && let Some(said) = no_previous_boot(m, &journalctl)
        {
            return Units {
                units: loaded
                    .into_iter()
                    .map(|name| Unit { name, log: None })
                    .collect(),
                journal: Journal::NoPreviousBoot(said),
                listed,
            };
        }
        let mut journal = Journal::Readable;
        let mut names = loaded.clone();
        // The units the journal names in that boot: each entry's unit, for
        // the units the patterns match. journalctl refuses -F with -b, as
        // with any option that limits the journal.
        let patterns: Vec<String> = UNIT_PREFIXES.iter().map(|p| format!("{p}*")).collect();
        let mut args = m.boot().to_vec();
        args.extend(["-o", "cat", "--output-fields=_SYSTEMD_UNIT"]);
        for p in &patterns {
            args.extend(["-u", p.as_str()]);
        }
        args.push("--no-pager");
        if let Ok(ran) = m.run(&journalctl, &args) {
            journal = journal.after(m.euid, &ran.err);
            names.extend(
                ran.out
                    .lines()
                    .map(str::trim)
                    .filter(|n| ours(n))
                    .map(str::to_owned),
            );
        }
        names.sort();
        names.dedup();
        let lines = m.lines.to_string();
        let mut units = Vec::new();
        for name in names {
            let mut args = m.boot().to_vec();
            args.extend([
                "-u",
                &name,
                "-n",
                &lines,
                "-o",
                "short-iso",
                "--no-hostname",
                "--no-pager",
            ]);
            let ran = m.run(&journalctl, &args).unwrap_or_else(|e| Ran {
                code: None,
                out: format!("cannot run journalctl: {e}\n"),
                err: String::new(),
            });
            journal = journal.after(m.euid, &ran.err);
            let empty = ran
                .out
                .lines()
                .all(|l| l.trim().is_empty() || l.trim() == "-- No entries --");
            if empty && !loaded.contains(&name) {
                continue;
            }
            // a run that fails and prints nothing says why on stderr
            let log = if ran.code != Some(0) && ran.out.trim().is_empty() {
                let code = ran
                    .code
                    .map_or_else(|| "ends on a signal".to_owned(), |c| format!("exits {c}"));
                format!("journalctl {code}: {}\n", ran.err.trim())
            } else {
                ran.out
            };
            units.push(Unit {
                name,
                log: Some(log),
            });
        }
        if let Journal::Unreadable(_) = journal {
            for u in &mut units {
                u.log = None;
            }
            units.retain(|u| loaded.contains(&u.name));
        }
        Units {
            units,
            journal,
            listed,
        }
    }
}

/// What journalctl says when it holds no boot before this one, asked for
/// one entry of it; `None` when it holds one, or when journalctl says
/// something else, which the unit runs then show.
fn no_previous_boot(m: &Machine, journalctl: &std::path::Path) -> Option<String> {
    let ran = m
        .run(
            journalctl,
            &["-b", "-1", "-n", "1", "-o", "cat", "--no-pager"],
        )
        .ok()?;
    (ran.code != Some(0) && ran.err.contains("No journal boot entry"))
        .then(|| ran.err.lines().next().unwrap_or_default().trim().to_owned())
}

/// The vtium units systemd has loaded, and why there is no list where
/// there is none.
fn loaded(m: &Machine) -> (Vec<String>, Option<String>) {
    let Some(systemctl) = m.program("systemctl") else {
        return (
            Vec::new(),
            Some("systemctl is not on this machine, so no unit list is read.".to_owned()),
        );
    };
    let patterns: Vec<String> = UNIT_PREFIXES.iter().map(|p| format!("{p}*")).collect();
    let mut args = vec![
        "list-units",
        "--all",
        "--plain",
        "--no-legend",
        "--no-pager",
        "--full",
    ];
    args.extend(patterns.iter().map(String::as_str));
    match m.run(&systemctl, &args) {
        Ok(ran) => (
            ran.out
                .lines()
                .filter_map(|l| l.split_whitespace().next())
                .filter(|n| ours(n))
                .map(str::to_owned)
                .collect(),
            None,
        ),
        Err(e) => (Vec::new(), Some(format!("cannot run systemctl: {e}"))),
    }
}

/// Whether `name` is one of the units the report gathers.
fn ours(name: &str) -> bool {
    UNIT_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// The revision a console logged when it last started in the boot `m`
/// reads.
pub fn revision(m: &Machine, unit: &str) -> Option<String> {
    let journalctl = m.program("journalctl")?;
    let mut args = m.boot().to_vec();
    args.extend([
        "-u",
        unit,
        "-o",
        "cat",
        "--no-pager",
        "-g",
        "^log: vtium Revision ",
    ]);
    let ran = m.run(&journalctl, &args).ok()?;
    let line = ran
        .out
        .lines()
        .rev()
        .find(|l| l.contains("vtium Revision "))?;
    let (_, after) = line.split_once("vtium Revision ")?;
    after.split_whitespace().next().map(str::to_owned)
}
