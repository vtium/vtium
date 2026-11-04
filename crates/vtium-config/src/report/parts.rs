//! The report's parts, each gathered from the machine through
//! [`super::read`] and written as plain lines.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use super::Machine;
use super::read::{BINARY_MAX, FILE_MAX, Ran, cannot, indented};

/// What the opening part says: what the report holds and what to check
/// before sharing it.
pub const HOLDS: &str = "This report holds what a vtium bug report needs: the vtium builds installed and the features each was built with, the kernel and the distribution, the state of each vtium console, the displays the kernel sees, vtium's configuration, and this boot's log of each vtium console. It leaves out the machine's hostname and each monitor's serial number. The logs can still name user accounts, because a login writes the user's name to them, and they hold what vtium logged about the programs it ran. Read the report through before you share it, and remove anything you would rather keep to yourself. Attach it to an issue at https://github.com/vtium/vtium/issues with what you saw and what you expected.\n";

/// The vtium console binaries the report looks for, and whether one that
/// is missing is named.
const BINARIES: [(&str, bool); 2] = [
    ("/usr/bin/vtium", true),
    ("/usr/local/bin/vtium-dev", false),
];

/// The packaged console, whose `console status` and `check-config` the
/// report runs.
const VTIUM: &str = "/usr/bin/vtium";

/// The features a vtium build adds on top of minspec, as
/// crates/vtium/Cargo.toml names them (a test holds them equal).
pub const FEATURES: [&str; 3] = ["gpu", "images", "logind"];

/// For each feature that has one, a line of text that only a build with
/// the feature carries, with a `|` cut into it: what vtium says when it
/// cannot open its image helper, from code built only with `images`, and
/// what it says when logind is not on the bus, from code built only with
/// `logind`. [`marker`] takes the `|` out only when the report searches a
/// binary, so neither this source nor vtium-config's binary holds the line
/// itself, which must stay in the one place a build with the feature
/// carries it (the development install's script and its tests rely on
/// that for `images`). The tests hold each marker to the file it comes
/// from.
pub const MARKERS: [(&str, &str); 2] = [
    ("images", "cannot open the image| helper "),
    ("logind", "logind is not on| the bus ("),
];

/// The marker `cut` with its `|` taken out, as a build with its feature
/// carries it.
pub fn marker(cut: &str) -> String {
    cut.replace('|', "")
}

/// The units whose state and logs the report gathers, by the start of
/// their names.
const UNIT_PREFIXES: [&str; 3] = ["vtium@", "vtium-dev@", "vtium-getty@"];

/// The units the report found, each with its log, and whether the
/// journal could be read.
pub struct Units {
    units: Vec<Unit>,
    journal: Journal,
    /// Why no unit list was read, where none was.
    listed: Option<String>,
}

struct Unit {
    name: String,
    /// Its latest log lines this boot, where the journal was read.
    log: Option<String>,
}

enum Journal {
    Readable,
    /// journalctl is not on the machine.
    Missing,
    /// journalctl said the user cannot read the system's messages; what
    /// it said.
    Unreadable(String),
}

/// What journalctl says when the user cannot read the system's journal.
const UNREADABLE: [&str; 2] = [
    "not seeing messages from other users",
    "insufficient permissions",
];

/// The arguments that print a unit's last `lines` log lines this boot,
/// without the hostname.
fn log_args(unit: &str, lines: &str) -> Vec<String> {
    [
        "-b",
        "-u",
        unit,
        "-n",
        lines,
        "-o",
        "short-iso",
        "--no-hostname",
        "--no-pager",
    ]
    .map(str::to_owned)
    .to_vec()
}

impl Units {
    /// The vtium units systemd has loaded and those this boot's journal
    /// names, with each one's log. A unit only the journal names is kept
    /// when it logged this boot.
    pub fn of(m: &Machine) -> Units {
        let mut names: Vec<String> = Vec::new();
        let listed = match m.program("systemctl") {
            Some(systemctl) => {
                let mut args = vec![
                    "list-units",
                    "--all",
                    "--plain",
                    "--no-legend",
                    "--no-pager",
                    "--full",
                ];
                let patterns: Vec<String> = UNIT_PREFIXES.iter().map(|p| format!("{p}*")).collect();
                args.extend(patterns.iter().map(String::as_str));
                match m.run(&systemctl, &args) {
                    Ok(ran) => {
                        names.extend(
                            ran.out
                                .lines()
                                .filter_map(|l| l.split_whitespace().next())
                                .map(str::to_owned),
                        );
                        None
                    }
                    Err(e) => Some(format!("cannot run systemctl: {e}")),
                }
            }
            None => Some("systemctl is not on this machine, so no unit list is read.".to_owned()),
        };
        names.retain(|n| ours(n));
        let loaded = names.clone();
        let Some(journalctl) = m.program("journalctl") else {
            return Units::named(names, Journal::Missing, listed);
        };
        if let Ok(ran) = m.run(&journalctl, &["-b", "-F", "_SYSTEMD_UNIT", "--no-pager"]) {
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
        let mut journal = Journal::Readable;
        let mut units = Vec::new();
        for name in names {
            let args = log_args(&name, &lines);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let ran = match m.run(&journalctl, &args) {
                Ok(ran) => ran,
                Err(e) => Ran {
                    code: None,
                    out: format!("cannot run journalctl: {e}\n"),
                    err: String::new(),
                },
            };
            if m.euid != 0 && UNREADABLE.iter().any(|u| ran.err.contains(u)) {
                journal = Journal::Unreadable(
                    ran.err.lines().next().unwrap_or_default().trim().to_owned(),
                );
            }
            let empty = ran
                .out
                .lines()
                .all(|l| l.trim().is_empty() || l.trim() == "-- No entries --");
            if empty && !loaded.contains(&name) {
                continue;
            }
            units.push(Unit {
                name,
                log: Some(ran.out),
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

    fn named(names: Vec<String>, journal: Journal, listed: Option<String>) -> Units {
        Units {
            units: names
                .into_iter()
                .map(|name| Unit { name, log: None })
                .collect(),
            journal,
            listed,
        }
    }
}

/// Whether `name` is one of the units the report gathers.
fn ours(name: &str) -> bool {
    UNIT_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// What `--version` says of a binary: its version line, the revision and
/// the features where it lists them.
struct Version {
    line: String,
    revision: Option<String>,
    features: Option<Vec<String>>,
}

fn version(ran: &Ran) -> Version {
    let mut lines = ran.out.lines();
    let line = lines.next().unwrap_or_default().trim().to_owned();
    let mut v = Version {
        line,
        revision: None,
        features: None,
    };
    for l in lines {
        if let Some(r) = l.strip_prefix("revision ") {
            v.revision = Some(r.trim().to_owned());
        } else if let Some(f) = l.strip_prefix("features") {
            v.features = Some(
                f.split_whitespace()
                    .filter(|w| *w != "none")
                    .map(str::to_owned)
                    .collect(),
            );
        }
    }
    v
}

/// Which of [`MARKERS`] the file at `path` carries, reading at most
/// [`BINARY_MAX`] bytes a block at a time.
fn carries(path: &Path) -> io::Result<Vec<bool>> {
    let markers: Vec<String> = MARKERS.iter().map(|(_, m)| marker(m)).collect();
    let longest = markers.iter().map(String::len).max().unwrap_or(0);
    let mut found = vec![false; markers.len()];
    let mut file = File::open(path)?.take(BINARY_MAX);
    let mut window: Vec<u8> = Vec::new();
    let mut block = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        window.extend_from_slice(block.get(..n).unwrap_or_default());
        for (f, marker) in found.iter_mut().zip(&markers) {
            *f = *f || window.windows(marker.len()).any(|w| w == marker.as_bytes());
        }
        // keep what a marker could start in and end in the next block
        let keep = window.len().saturating_sub(longest.saturating_sub(1));
        window.drain(..keep);
    }
    Ok(found)
}

/// What the features line says of a binary that `--version` printed
/// `v` for.
fn features(path: &Path, v: &Version) -> String {
    if let Some(listed) = &v.features {
        let listed = if listed.is_empty() {
            "none".to_owned()
        } else {
            listed.join(" ")
        };
        return format!("{listed}, as its --version lists them");
    }
    let found = match carries(path) {
        Ok(found) => found,
        Err(e) => return format!("unknown: {e}"),
    };
    let mut said: Vec<String> = MARKERS
        .iter()
        .zip(found)
        .map(|((name, _), on)| format!("{name} {}", if on { "yes" } else { "no" }))
        .collect();
    said.extend(
        FEATURES
            .iter()
            .filter(|f| !MARKERS.iter().any(|(m, _)| m == *f))
            .map(|f| format!("{f} unknown")),
    );
    format!(
        "{}, read from text in the binary, since its --version lists none",
        said.join(", ")
    )
}

/// The vtium part: each console binary, its version and features, and
/// vtium-config's own version.
pub fn vtium(m: &Machine, _: &Units) -> String {
    let mut s = String::new();
    for (abs, named) in BINARIES {
        let path = m.path(abs);
        if !path.is_file() {
            if named {
                s.push_str(&format!("{abs}: not installed\n"));
            }
            continue;
        }
        s.push_str(&format!("{abs}\n"));
        match m.run(&path, &["--version"]) {
            Ok(ran) => {
                let v = version(&ran);
                s.push_str(&format!("  version: {}\n", v.line));
                if let Some(r) = &v.revision {
                    s.push_str(&format!("  revision: {r}\n"));
                }
                s.push_str(&format!("  features: {}\n", features(&path, &v)));
            }
            Err(e) => s.push_str(&format!("  cannot run it: {e}\n")),
        }
    }
    s.push_str(&format!("vtium-config: {}\n", env!("CARGO_PKG_VERSION")));
    s
}

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
            cannot("/proc/sys/kernel/osrelease", &e)
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

/// The revision a console logged when it last started this boot.
fn revision(m: &Machine, unit: &str) -> Option<String> {
    let journalctl = m.program("journalctl")?;
    let ran = m
        .run(
            &journalctl,
            &[
                "-b",
                "-u",
                unit,
                "-o",
                "cat",
                "--no-pager",
                "-g",
                "^log: vtium Revision ",
            ],
        )
        .ok()?;
    let line = ran
        .out
        .lines()
        .rev()
        .find(|l| l.contains("vtium Revision "))?;
    let (_, after) = line.split_once("vtium Revision ")?;
    after.split_whitespace().next().map(str::to_owned)
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
        s.push_str("No vtium@, vtium-dev@ or vtium-getty@ unit is loaded, and this boot's journal names none.\n");
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

/// A monitor's EDID: its base block, checked as the console checks it.
pub struct Edid {
    base: Vec<u8>,
}

/// The offsets of the base block's four descriptors.
const DESCRIPTORS: [usize; 4] = [54, 72, 90, 108];

impl Edid {
    /// The EDID in the connector directory `dir`, where it holds one whose
    /// header and checksum are good.
    pub fn read(m: &Machine, dir: &str) -> Option<Edid> {
        let bytes = m.bytes(&format!("{dir}/edid"), 128).ok()?;
        vtium_platform::display::Edid::parse(&bytes)?;
        Some(Edid { base: bytes })
    }

    /// The text of the descriptor tagged `tag`, up to its line feed,
    /// without the spaces that pad it.
    fn descriptor(&self, tag: u8) -> Option<String> {
        let d = DESCRIPTORS
            .iter()
            .filter_map(|&at| self.base.get(at..at + 18))
            .find(|d| d.get(..4) == Some(&[0, 0, 0, tag]))?;
        let text = d.get(5..)?;
        let end = text.iter().position(|&b| b == b'\n').unwrap_or(text.len());
        let text = String::from_utf8_lossy(text.get(..end)?).trim().to_owned();
        (!text.is_empty()).then_some(text)
    }

    /// The monitor's name, its printable characters only.
    fn name(&self) -> Option<String> {
        self.descriptor(0xfc).map(|n| {
            n.chars()
                .map(|c| {
                    if c.is_ascii_graphic() || c == ' ' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        })
    }

    /// The serial string, which the report only ever replaces.
    pub fn serial_text(&self) -> Option<String> {
        self.descriptor(0xff)
    }

    /// The serial number, bytes 12 to 15, where it is not 0; the report
    /// only ever replaces it.
    pub fn serial_number(&self) -> Option<u32> {
        let b = self.base.get(12..16)?;
        let n = u32::from_le_bytes([*b.first()?, *b.get(1)?, *b.get(2)?, *b.get(3)?]);
        (n != 0).then_some(n)
    }

    /// The size it states, in millimetres, as the console reads it.
    fn mm(&self) -> Option<(u32, u32)> {
        vtium_platform::display::Edid::parse(&self.base)?.mm()
    }
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
        Err(e) => return format!("{}\n", cannot("/sys/class/drm", &e)),
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
        Err(e) => s.push_str(&format!("{}\n", cannot(conf, &e))),
    }
    for dir in ["/etc/vtium", "/etc/vtium/fonts"] {
        match m.names(dir) {
            Ok(names) if names.is_empty() => s.push_str(&format!("{dir} is empty.\n")),
            Ok(names) => s.push_str(&format!("Files in {dir}: {}\n", names.join(", "))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                s.push_str(&format!("There is no {dir}.\n"));
            }
            Err(e) => s.push_str(&format!("{}\n", cannot(dir, &e))),
        }
    }
    s.push_str(&vtium_subcommand(
        m,
        &["check-config"],
        "vtium check-config",
    ));
    s
}

/// The logs part: each unit's latest log lines this boot.
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
                "The journal is not readable to you. Run the report as root, with sudo vtium-config report, or have root add your user to the systemd-journal group, with usermod -aG systemd-journal followed by your user name, and log in again. journalctl said: {said}\n"
            ));
            return s;
        }
        Journal::Readable => {}
    }
    let logged: Vec<&Unit> = units.units.iter().filter(|u| u.log.is_some()).collect();
    if logged.is_empty() {
        s.push_str("No vtium@, vtium-dev@ or vtium-getty@ unit has logged this boot.\n");
    }
    for u in logged {
        s.push_str(&format!(
            "\n{}, its last {} lines this boot:\n{}",
            u.name,
            m.lines,
            indented(u.log.as_deref().unwrap_or_default())
        ));
    }
    s
}

/// The kernel display state part: debugfs's atomic state of each card, as
/// root.
pub fn kernel_state(m: &Machine, _: &Units) -> String {
    if m.euid != 0 {
        return "needs root: sudo vtium-config report\n".to_owned();
    }
    let cards = match cards(m) {
        Ok(c) if c.is_empty() => return NO_CARD.to_owned(),
        Ok(c) => c,
        Err(e) => return format!("{}\n", cannot("/sys/class/drm", &e)),
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
                cannot(tried.first().map_or("", String::as_str), &e)
            )),
            (None, None) => {}
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`FEATURES`] is crates/vtium's own list of features but `default`,
    /// so a feature added there fails here until the report knows it.
    #[test]
    fn the_features_are_vtium_s() {
        let manifest = include_str!("../../../vtium/Cargo.toml");
        let section = manifest
            .split("\n[features]\n")
            .nth(1)
            .and_then(|s| s.split("\n[").next())
            .unwrap_or_default();
        let mut named: Vec<&str> = section
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim()))
            .filter(|k| !k.is_empty() && *k != "default")
            .collect();
        named.sort_unstable();
        assert_eq!(named, FEATURES);
    }

    #[test]
    fn version_reads_the_revision_and_the_features_where_listed() {
        let ran = |out: &str| Ran {
            code: Some(0),
            out: out.to_owned(),
            err: String::new(),
        };
        let v = version(&ran(
            "vtium 1.0\nrevision v1-2-gabc\nfeatures images logind\n",
        ));
        assert_eq!(v.line, "vtium 1.0");
        assert_eq!(v.revision.as_deref(), Some("v1-2-gabc"));
        assert_eq!(
            v.features,
            Some(vec!["images".to_owned(), "logind".to_owned()])
        );
        let v = version(&ran("vtium 1.0\nfeatures none\n"));
        assert_eq!(v.features, Some(vec![]));
        let v = version(&ran("vtium 1.0\n"));
        assert_eq!((v.revision, v.features), (None, None));
    }
}
