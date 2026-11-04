//! `vtium-config report` (ADR-1200 D24): gathers what a bug report needs
//! into one plain-text document and changes nothing on the machine.
//!
//! The document has eight parts, each under a plain heading, and
//! [`parts`] gathers each one, with [`binary`] for the vtium binaries,
//! [`units`] for the vtium units and their logs, and [`edid`] for the
//! monitors. [`read`] is the only way the report reaches the machine: it
//! reads files under a root and runs the programs the report names, so a
//! test points the root at a stand-in tree. [`scrub`] replaces the
//! hostname and the monitors' serials in what was gathered. Run as root,
//! the report writes no file: `--save` is refused, and the user's own
//! shell saves what it prints.

mod binary;
mod edid;
mod parts;
mod read;
mod scrub;
mod units;

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub use binary::{FEATURES, MARKERS, marker};

/// The log lines of each console the document holds when `--lines` does
/// not say.
pub const LINES: u32 = 300;

/// The most log lines `--lines` takes.
const LINES_MAX: u32 = 100_000;

/// The machine a report is made of, and how.
#[derive(Clone, Debug)]
pub struct Machine {
    /// The root every path is read under, `/` for this machine.
    pub root: PathBuf,
    /// The effective uid the report runs with: 0 reads the root-only
    /// parts.
    pub euid: u32,
    /// The log lines of each console the document holds.
    pub lines: u32,
    /// Whether the logs come from the boot before this one, `-b -1`, as
    /// after a defect that forced a reboot.
    pub previous_boot: bool,
    /// The time the report is made, in its header.
    pub now: SystemTime,
}

impl Machine {
    /// The arguments that name the boot whose journal the report reads.
    fn boot(&self) -> Vec<&'static str> {
        if self.previous_boot {
            vec!["-b", "-1"]
        } else {
            vec!["-b"]
        }
    }

    /// That boot, in words.
    fn boot_words(&self) -> &'static str {
        if self.previous_boot {
            "the previous boot"
        } else {
            "this boot"
        }
    }
}

/// A function that gathers one part of the document.
type Gather = fn(&Machine, &units::Units) -> String;

/// The document's parts, in order: each heading and the function that
/// gathers what is under it.
const PARTS: [(&str, Gather); 7] = [
    ("vtium", binary::vtium),
    ("System", parts::system),
    ("Consoles", parts::consoles),
    ("Displays", parts::displays),
    ("Configuration", parts::configuration),
    ("Logs", parts::logs),
    ("Kernel display state", parts::kernel_state),
];

/// The report on `m`: a title, the part that says what the report holds,
/// then each part [`PARTS`] gathers, with the hostname and the monitors'
/// serials replaced in what was gathered.
pub fn report(m: &Machine) -> String {
    let secrets = scrub::Secrets::of(m);
    let units = units::Units::of(m);
    let mut doc = String::new();
    heading(&mut doc, "vtium report", '=');
    doc.push_str(&format!(
        "Made by vtium-config {} at {} UTC, run as {}, with the logs of {}.\n",
        env!("CARGO_PKG_VERSION"),
        utc(m.now).text(),
        if m.euid == 0 { "root" } else { "a user" },
        m.boot_words()
    ));
    doc.push('\n');
    heading(&mut doc, "What this report holds", '-');
    doc.push_str(parts::HOLDS);
    for (name, gather) in PARTS {
        doc.push('\n');
        heading(&mut doc, name, '-');
        doc.push_str(&secrets.scrub(&gather(m, &units)));
    }
    doc
}

/// `name` underlined with `rule`, and a blank line after.
fn heading(doc: &mut String, name: &str, rule: char) {
    doc.push_str(name);
    doc.push('\n');
    doc.extend(std::iter::repeat_n(rule, name.chars().count()));
    doc.push_str("\n\n");
}

/// A time of day in UTC.
struct Utc {
    year: i64,
    month: u32,
    day: u32,
    seconds: u64,
}

impl Utc {
    fn hms(&self) -> (u64, u64, u64) {
        (
            self.seconds / 3600,
            self.seconds / 60 % 60,
            self.seconds % 60,
        )
    }

    /// `2026-11-03 20:30:44`.
    fn text(&self) -> String {
        let (h, m, s) = self.hms();
        format!(
            "{:04}-{:02}-{:02} {h:02}:{m:02}:{s:02}",
            self.year, self.month, self.day
        )
    }
}

/// `now` in UTC, by the civil calendar's days since 1970 (Howard
/// Hinnant's `civil_from_days`); a time before 1970 reads as 1970.
fn utc(now: SystemTime) -> Utc {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Utc {
        year,
        month: u32::try_from(month).unwrap_or(1),
        day: u32::try_from(day).unwrap_or(1),
        seconds: secs % 86_400,
    }
}

/// The name `--save` gives the report made at `now`:
/// `vtium-report-YYYYMMDD-HHMMSS.txt`, in UTC.
pub fn file_name(now: SystemTime) -> String {
    let t = utc(now);
    let (h, m, s) = t.hms();
    format!(
        "vtium-report-{:04}{:02}{:02}-{h:02}{m:02}{s:02}.txt",
        t.year, t.month, t.day
    )
}

/// Where `--save` writes.
#[derive(Debug, PartialEq, Eq)]
enum Save {
    Out,
    Here,
    At(PathBuf),
}

/// What `report`'s own arguments ask for.
#[derive(Debug, PartialEq, Eq)]
struct Args {
    save: Save,
    lines: u32,
    previous_boot: bool,
}

/// What vtium-config says when root asks `report` to save: root writes no
/// file, so that no rule that lets a user run the report as root lets that
/// user create a file wherever root can.
pub const ROOT_SAVES_NOTHING: &str = "run as root, vtium-config report writes no file, so it takes no --save. Run sudo vtium-config report > vtium-report.txt, and your own shell writes the file, as you";

/// Whether `rest`, `report`'s own arguments, ask it to save, in any form.
pub fn saves(rest: &[impl AsRef<str>]) -> bool {
    rest.iter()
        .any(|a| a.as_ref() == "--save" || a.as_ref().starts_with("--save="))
}

/// `report`'s own arguments: `--save`, `--save=PATH`, `--lines N` and
/// `--previous-boot`.
fn parse(rest: &[String]) -> Result<Args, String> {
    let mut args = Args {
        save: Save::Out,
        lines: LINES,
        previous_boot: false,
    };
    let mut it = rest.iter();
    let number = |v: &str| {
        v.parse::<u32>()
            .ok()
            .filter(|n| (1..=LINES_MAX).contains(n))
            .ok_or_else(|| format!("--lines takes a number from 1 to {LINES_MAX}, not {v}"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--save" => args.save = Save::Here,
            "--previous-boot" => args.previous_boot = true,
            "--lines" => {
                let v = it.next().ok_or("--lines takes a number")?;
                args.lines = number(v)?;
            }
            a => {
                if let Some(path) = a.strip_prefix("--save=") {
                    if path.is_empty() {
                        return Err("--save= takes a path".to_owned());
                    }
                    args.save = Save::At(path.into());
                } else if let Some(v) = a.strip_prefix("--lines=") {
                    args.lines = number(v)?;
                } else {
                    return Err(format!("report takes no {a}"));
                }
            }
        }
    }
    Ok(args)
}

/// `vtium-config report`: prints the report on the machine under `root`,
/// `/` when none is named, run with the effective uid `euid`, or saves it
/// as `rest` asks. Exits 0 when the report is printed or saved, 1 when it
/// cannot be saved, and 2 on a usage error, root's `--save` among them.
pub fn command(
    rest: &[String],
    root: Option<&Path>,
    euid: u32,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let args = match parse(rest) {
        Ok(a) => a,
        Err(why) => {
            let _ = writeln!(err, "vtium-config: {why}\nTry vtium-config --help report.");
            return 2;
        }
    };
    // `main`'s refusal says this first; the command holds to it too
    if euid == 0 && args.save != Save::Out {
        let _ = writeln!(err, "vtium-config: {ROOT_SAVES_NOTHING}");
        return 2;
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "a report is dated, and a saved one named, for the time it was made (ADR-1200 D24)"
    )]
    let now = SystemTime::now();
    let m = Machine {
        root: root.map_or_else(|| PathBuf::from("/"), Path::to_path_buf),
        euid,
        lines: args.lines,
        previous_boot: args.previous_boot,
        now,
    };
    let doc = report(&m);
    let path = match args.save {
        Save::Out => {
            let _ = out.write_all(doc.as_bytes());
            return 0;
        }
        Save::Here => PathBuf::from(file_name(m.now)),
        Save::At(path) => path,
    };
    let path = std::env::current_dir().map_or(path.clone(), |here| here.join(&path));
    match write_new(&path, doc.as_bytes()) {
        Ok(()) => {
            let _ = writeln!(out, "{}", path.display());
            0
        }
        Err(e) => {
            let _ = writeln!(err, "vtium-config: cannot write {}: {e}", path.display());
            1
        }
    }
}

/// Writes `bytes` as the new file `path`, readable by its owner only, and
/// never over a file that exists. A file whose write fails is removed, so
/// no part of a report is left behind. No test reaches that removal: a
/// write fails after `create_new` has made the file only when the disk
/// fills or the file system fails, which a test cannot bring about on a
/// file it has just created without a fault-injecting file system.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes).inspect_err(|_| {
        let _ = std::fs::remove_file(path);
    })
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn report_s_arguments() {
        let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let args = |save, lines, previous_boot| Args {
            save,
            lines,
            previous_boot,
        };
        assert_eq!(parse(&[]).unwrap(), args(Save::Out, LINES, false));
        assert_eq!(
            parse(&words(&["--save"])).unwrap(),
            args(Save::Here, LINES, false)
        );
        assert_eq!(
            parse(&words(&[
                "--save=a/b.txt",
                "--lines",
                "9",
                "--previous-boot"
            ]))
            .unwrap(),
            args(Save::At("a/b.txt".into()), 9, true)
        );
        assert_eq!(
            parse(&words(&["--lines=12"])).unwrap(),
            args(Save::Out, 12, false)
        );
        assert!(saves(&["--lines", "3", "--save=/etc/x"]));
        assert!(saves(&["--save"]));
        assert!(!saves(&["--lines", "3", "--previous-boot"]));
        for bad in [
            &["--save="][..],
            &["--lines"],
            &["--lines", "0"],
            &["--lines", "100001"],
            &["--lines", "x"],
            &["--save", "x"],
            &["--frob"],
        ] {
            assert!(parse(&words(bad)).is_err(), "{bad:?}");
        }
    }
}
