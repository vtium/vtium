//! `vtium check-config`: the configuration read as vtium would read it at
//! start, with every warning, and nothing applied. It opens nothing
//! under `/dev` and writes nothing, so it runs unprivileged: the devices
//! present it reads from sysfs and udev's database (ADR-1401 D7).
//!
//! The head file goes through vtium-config's reader, parser and
//! evaluator, the ones the start runs (ADR-1401 D7): the file's own
//! errors, each with what it drops, and its warnings go to standard error
//! and make the status 1. What depends on the machine it is checked on,
//! or is a choice rather than a fault, is noted on standard output outside
//! the status, so a file can be checked away from its machine, and the
//! notes end on what a start does with the file and the policy it states.

use std::path::{Path, PathBuf};

use vtium_config::{
    Against, AtStart, Config as Heads, Identity, Lockout, Match, Note, Parsed, Problem, Read,
    Rejected, Remark, Runs, Stating, Token, Trust, Untrusted, at,
};

use vtium_platform::listing::{self, Listing, Roots};

use crate::config::{Config, Loaded, keys};
use crate::font;
use crate::head::identity;
use crate::notice::check_config;

/// The subcommand's name, which stands among the options.
pub const SUBCOMMAND: &str = "check-config";

/// Whether a rule sending input to a head by `id` or `name` alone counts
/// against the file. ADR-1401's probe 4 is the operator's, and its
/// recommendation, a note outside the status, stands until the operator
/// answers otherwise; another answer changes this line.
const WEAK_MATCH_COUNTS: bool = false;

/// Whether a candidate that fails the ownership check where it sits counts
/// against it. A candidate is checked before it is installed, and
/// installing it makes it root's (ADR-1401 D12), so it is a note outside
/// the status, the team lead's answer of 2026-10-29; the installed head
/// file always counts it, as the start does.
const CANDIDATE_OWNERSHIP_COUNTS: bool = false;

/// The subcommand's help.
pub const HELP: &str = check_config::HELP;

/// What a value means beyond its own key, which the start reports too.
pub fn semantics(config: &Config) -> Vec<String> {
    // Any user may read PID 1's status; `SigCgt` says what it catches.
    let status = std::fs::read_to_string("/proc/1/status").map_err(|e| e.to_string());
    let mut out = semantics_under(config, &status, vtium_platform::power::power_off_signal());
    out.extend(frit(config, &font::default_frits()));
    out
}

/// What the frit `font-name` names in `frits` says of its cell, as
/// [`font::frit_warnings`] reads it; nothing when the name is no frit's.
fn frit(config: &Config, frits: &Path) -> Vec<String> {
    let Some(wanted) = font::wanted(config, "", "").filter(|w| w.came == font::Came::Config) else {
        return Vec::new();
    };
    let Some(path) = font::frit(frits, &wanted.name) else {
        return Vec::new();
    };
    match font::frit_text(&path) {
        Ok(text) => font::frit_warnings(&wanted.name, &text),
        Err(why) => vec![check_config::frit_unreadable(&wanted.name, &why)],
    }
}

/// Whether PID 1 catches `signal`, from the text of `/proc/1/status`:
/// the `SigCgt` line is a hex mask with bit `signal - 1` set for a caught
/// signal. `Err` is the report of why the text does not answer.
fn init_catches(status: &str, signal: i32) -> Result<bool, String> {
    let mask = status
        .lines()
        .find_map(|line| line.strip_prefix("SigCgt:"))
        .ok_or_else(|| check_config::power_off_no_sigcgt(signal))?;
    let mask = u64::from_str_radix(mask.trim(), 16)
        .map_err(|_| check_config::power_off_bad_mask(signal, mask.trim()))?;
    let bit = u32::try_from(signal).ok().and_then(|n| n.checked_sub(1));
    Ok(bit
        .and_then(|b| mask.checked_shr(b))
        .is_some_and(|m| m & 1 == 1))
}

/// [`semantics`] where `status` is what reading `/proc/1/status` gave (the
/// text, or the error's message) and `signal` is the power-off signal.
fn semantics_under(config: &Config, status: &Result<String, String>, signal: i32) -> Vec<String> {
    let mut out = Vec::new();
    if config.switch(&keys::HWACCEL) {
        out.push(check_config::HWACCEL_NOT_BUILT.to_owned());
    }
    if config.switch(&keys::USE_ORIGINAL_MODE) && !config.modes(&keys::MODE).is_empty() {
        out.push(check_config::MODE_OVER_ORIGINAL.to_owned());
    }
    if !config.chords(&keys::GRAB_POWER_OFF).is_empty() {
        let caught = match status {
            Ok(text) => init_catches(text, signal),
            Err(system_error) => Err(check_config::power_off_status_unreadable(
                signal,
                system_error,
            )),
        };
        match caught {
            Ok(true) => {}
            Ok(false) => out.push(check_config::power_off_uncaught(signal)),
            Err(report) => out.push(report),
        }
    }
    out
}

/// Every warning the start would give for `loaded`: the configuration's
/// reports, then [`semantics`].
pub fn warnings(loaded: &Loaded) -> Vec<String> {
    let mut out: Vec<String> = loaded.reports.iter().map(ToString::to_string).collect();
    out.extend(semantics(&loaded.config));
    out.extend(crate::config::budget::memory_warning());
    out
}

/// What the subcommand reports beyond the warnings, on standard output
/// and outside the exit status: the density points convert at,
/// `display-dpi`'s, else each display's, which only the start reads, since
/// the subcommand opens no display (the operator's decision on
/// vtium/vtium#145).
pub fn notes(config: &Config) -> Vec<String> {
    let density = match config.number(&keys::DISPLAY_DPI) {
        Some(dpi) => check_config::display_dpi(dpi),
        None => check_config::MEASURED_DENSITY.to_owned(),
    };
    vec![density]
}

/// The subcommand on the configuration `loaded`, which the command line
/// named it in: the lines to print and the exit status.
pub fn run(loaded: &Loaded) -> (Vec<String>, i32) {
    if loaded.config.switch(&keys::HELP) {
        return (vec![HELP.trim_end().to_owned()], 0);
    }
    let lines = warnings(loaded);
    let status = i32::from(!lines.is_empty());
    (lines, status)
}

/// Where the subcommand finds the machine's files: the installed head
/// file, the udev daemon's control socket, sysfs and udev's database,
/// which list the devices present, and the frits a profile names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub head_file: PathBuf,
    pub udev_control: PathBuf,
    pub roots: Roots,
    pub frits: PathBuf,
}

impl Host {
    /// The machine's own, which the binary takes and nothing overrides.
    pub fn system() -> Host {
        Host {
            head_file: PathBuf::from(vtium_config::HEAD_FILE),
            udev_control: PathBuf::from(UDEV_CONTROL),
            roots: Roots::system(),
            frits: font::default_frits(),
        }
    }
}

/// The seat whose devices are listed: the kernel VTs', which vtium runs on.
const SEAT: &str = "seat0";

/// The devices `listing` holds as the rules see them, through the mapping
/// the start walks them through ([`crate::head::identity`]): each card's
/// displays with the card and their place among its displays, each
/// framebuffer's display as the start names it, `fb`, then each input.
///
/// A display's place is the lister's connector-id order, which can differ
/// from the order the start registers a card's displays in, and no finding
/// depends on it: a rule matching no device and the layer 1 warnings read
/// only which rule each device walks to, and the place feeds only where the
/// display guard puts a display, which the check does not run.
pub fn devices(listing: &Listing) -> Vec<Identity> {
    let displays = listing.cards.iter().flat_map(|c| {
        let card = identity::Card::at(&c.node, c.id_path.as_deref(), c.primary);
        (0u32..)
            .zip(&c.displays)
            .map(move |(order, d)| identity::display(d, &card, order))
    });
    let framebuffers = listing
        .framebuffers
        .iter()
        .map(|f| identity::unnamed_display("fb", &identity::Card::at(&f.node, None, f.primary), 0));
    displays
        .chain(framebuffers)
        .chain(listing.inputs.iter().map(identity::input))
        .collect()
}

/// What the head file is checked against of the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Machine<'a> {
    /// A udev daemon runs ([`udev_daemon_runs`]).
    pub udev_daemon: bool,
    /// The devices present, as [`devices`] gives them; `None` checks the
    /// file alone, as away from its machine.
    pub devices: Option<&'a [Identity]>,
}

/// The socket a udev daemon listens on, which `udevadm control` talks to.
pub const UDEV_CONTROL: &str = "/run/udev/control";

/// Whether a udev daemon runs: its control socket `control` is present.
/// Without one, the keys that read udev's database match nothing
/// (ADR-1401 D4).
pub fn udev_daemon_runs(control: &Path) -> bool {
    control.exists()
}

/// What the subcommand prints, and its exit status.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The configuration's warnings and the head file's errors and
    /// warnings.
    pub stderr: Vec<String>,
    /// What the status does not count.
    pub stdout: Vec<String>,
    /// 1 when standard error holds any line.
    pub status: u8,
}

/// The subcommand on `loaded` and the machine `host`.
pub fn outcome(loaded: &Loaded, host: &Host) -> Outcome {
    let (stderr, status) = run(loaded);
    if loaded.config.switch(&keys::HELP) {
        return Outcome {
            stderr,
            stdout: Vec::new(),
            status: 0,
        };
    }
    let devices = devices(&listing::list(&host.roots, SEAT));
    let machine = Machine {
        udev_daemon: udev_daemon_runs(&host.udev_control),
        devices: Some(&devices),
    };
    let kmscon = Kmscon::of(&loaded.config, &host.frits);
    // a candidate that is the installed file is checked as the start
    // reads it, ownership and all
    let heads = match loaded.candidate.as_deref() {
        Some(path) if !same_file(path, &host.head_file) => {
            head_file(path, true, &Trust::ROOT, &machine, &kmscon)
        }
        Some(path) => head_file(path, false, &Trust::ROOT, &machine, &kmscon),
        None => head_file(&host.head_file, false, &Trust::ROOT, &machine, &kmscon),
    };
    let status = u8::from(status != 0 || !heads.counted.is_empty());
    Outcome {
        stderr: stderr.into_iter().chain(heads.counted).collect(),
        stdout: notes(&loaded.config)
            .into_iter()
            .chain(heads.notes)
            .collect(),
        status,
    }
}

/// The subcommand as the binary runs it on the machine's own files: it
/// prints [`outcome`] and returns the exit status.
pub fn main(loaded: &Loaded) -> u8 {
    let out = outcome(loaded, &Host::system());
    for line in &out.stderr {
        eprintln!("{line}");
    }
    for line in &out.stdout {
        println!("{line}");
    }
    out.status
}

/// What the head file's check found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HeadCheck {
    /// The file's own errors and warnings, which make the status 1.
    pub counted: Vec<String>,
    /// What the status does not count.
    pub notes: Vec<String>,
}

/// Whether `a` and `b` name one file, by its device and inode once
/// symlinks are followed.
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// What the head file is checked against of kmscon.conf and the machine's
/// frits: the console caps (ADR-1401 D10) and the frits directory a
/// profile's frit is looked up in (D7).
#[derive(Clone, Copy, Debug)]
pub struct Kmscon<'a> {
    /// `head-consoles-max`.
    pub head_cap: u32,
    /// `consoles-max`.
    pub total: u32,
    pub frits: &'a Path,
}

impl Kmscon<'_> {
    /// The caps `config` sets, and the frits in `frits`.
    pub fn of<'a>(config: &Config, frits: &'a Path) -> Kmscon<'a> {
        let number = |key, unset: u32| {
            config
                .number(key)
                .map_or(unset, |n| u32::try_from(n).unwrap_or(u32::MAX))
        };
        Kmscon {
            head_cap: number(
                &keys::HEAD_CONSOLES_MAX,
                vtium_config::HEAD_CONSOLES_DEFAULT,
            ),
            total: number(&keys::CONSOLES_MAX, vtium_config::CONSOLES_TOTAL_DEFAULT),
            frits,
        }
    }
}

/// Checks the head file at `path`, the installed one or, with
/// `candidate`, one given to check before it is installed, its bytes read
/// under `trust`, against `machine` and `kmscon`. It reads the file as
/// the start does (ADR-1401 D7): each error, counted, with what it drops,
/// then what survives, and last, as notes, what a start does with the
/// file and the policy it states. A candidate the ownership check fails
/// is read as the start would read it once installed.
pub fn head_file(
    path: &Path,
    candidate: bool,
    trust: &Trust,
    machine: &Machine,
    kmscon: &Kmscon,
) -> HeadCheck {
    let mut out = HeadCheck::default();
    let inspected = match vtium_config::inspect(path, trust) {
        Ok(Some(inspected)) => inspected,
        Ok(None) if candidate => {
            out.counted
                .push(at(path, None, check_config::NO_SUCH_FILE).to_string());
            return out;
        }
        Ok(None) => {
            out.notes
                .push(at(path, None, AtStart(Read::Missing)).to_string());
            return out;
        }
        Err(rejected) if denied(&rejected) => {
            out.counted
                .push(at(path, None, check_config::ROOT_ONLY).to_string());
            return out;
        }
        Err(rejected) => {
            errors(&mut out, path, &rejected);
            out.notes
                .push(at(path, None, AtStart(Read::Inaccessible)).to_string());
            return out;
        }
    };
    if let Err(why) = inspected.trusted {
        if candidate && !CANDIDATE_OWNERSHIP_COUNTS {
            let note = match &why {
                Untrusted::Owner { path, uid } => check_config::candidate_owner(path, *uid),
                Untrusted::Writable { path, mode } => check_config::candidate_writable(path, *mode),
            };
            out.notes.push(at(path, None, note).to_string());
        } else {
            let problem = Problem::Untrusted(why);
            out.counted.push(at(path, None, problem).to_string());
            out.notes
                .push(at(path, None, AtStart(Read::Untrusted)).to_string());
            return out;
        }
    }
    let reading = vtium_config::reading(&inspected.bytes);
    errors(&mut out, path, &reading.errors);
    let trusted = reading.trusted();
    let runs = vtium_config::resolve(&Read::Trusted(trusted)).runs;
    findings(&mut out, path, &reading.parsed, machine, kmscon, runs);
    out.notes
        .push(at(path, None, AtStart(Read::Trusted(trusted))).to_string());
    out.notes.push(at(path, None, Stating(trusted)).to_string());
    out
}

/// Whether `rejected` is the open's refusal for want of permission: a
/// head file root keeps from other users, which only root can check.
fn denied(rejected: &Rejected) -> bool {
    matches!(
        rejected.errors.as_slice(),
        [vtium_config::Error {
            problem: Problem::Unreadable(std::io::ErrorKind::PermissionDenied),
            ..
        }]
    )
}

/// Each error at its line, counted, with what it drops of a file that is
/// read (ADR-1401 D7, "The rest").
fn errors(out: &mut HeadCheck, path: &Path, rejected: &Rejected) {
    for e in &rejected.errors {
        out.counted.push(at(path, e.line(), e).to_string());
        if let Some(drops) = e.drops {
            out.counted.push(at(path, e.line(), drops).to_string());
        }
    }
    if rejected.more > 0 {
        out.counted
            .push(at(path, None, crate::notice::more_errors(rejected.more)).to_string());
    }
}

/// What survives of the file, which a start `runs`: its warnings, counted,
/// with those measured against `kmscon`, a head's `consoles` past a cap
/// and a profile naming a missing frit (ADR-1401 D7, D10); and its notes:
/// each rule that needs a udev daemon where none runs, each
/// `connector-path:` rule, and the remarks on what runs, with what the
/// devices present find (D3, D4, D5, D9). A start that runs the shipped
/// default or nothing runs none of the file, so no remark is made on it.
fn findings(
    out: &mut HeadCheck,
    path: &Path,
    parsed: &Parsed,
    machine: &Machine,
    kmscon: &Kmscon,
    runs: Runs,
) {
    let has_frit = |name: &str| font::frit(kmscon.frits, name).is_some();
    let against = Against {
        head_cap: kmscon.head_cap,
        total: kmscon.total,
        frits: kmscon.frits,
        has_frit: &has_frit,
    };
    for w in &vtium_config::warnings(parsed, &against) {
        out.counted.push(at(path, Some(w.line()), w).to_string());
    }
    if parsed.more_warnings > 0 {
        out.counted.push(
            at(
                path,
                None,
                check_config::more_warnings(parsed.more_warnings),
            )
            .to_string(),
        );
    }
    for n in &parsed.notes {
        let line = at(path, Some(n.line()), n).to_string();
        if WEAK_MATCH_COUNTS && matches!(n, Note::WeakMatch { .. }) {
            out.counted.push(line);
        } else {
            out.notes.push(line);
        }
    }
    if parsed.more_notes > 0 {
        out.notes
            .push(at(path, None, check_config::more_notes(parsed.more_notes)).to_string());
    }
    let runs_file = match runs {
        Runs::File | Runs::NoHead | Runs::Survivors => true,
        Runs::Builtin | Runs::Off => false,
    };
    let remarks = if runs_file {
        vtium_config::remarks(&parsed.config, machine.devices)
    } else {
        Vec::new()
    };
    // a lockout is the first thing said of the file
    let (lockout, remarks): (Vec<_>, Vec<_>) = remarks
        .into_iter()
        .partition(|(_, r)| *r == Remark::ClosedLockout);
    let lockout: Vec<String> = lockout
        .iter()
        .map(|(line, r)| at(path, *line, r).to_string())
        .collect();
    out.notes.splice(0..0, lockout);
    // each rule noted as matching nothing here takes no second note
    let mut noted = Vec::new();
    if !machine.udev_daemon {
        for (line, key) in parsed.config.needs_udev() {
            noted.push(line);
            out.notes
                .push(at(path, Some(line), check_config::no_udev(key)).to_string());
        }
    }
    for line in connector_path_rules(&parsed.config) {
        noted.push(line);
        out.notes
            .push(at(path, Some(line), check_config::no_connector_path()).to_string());
    }
    if let Some(devices) = machine.devices.filter(|_| runs_file) {
        out.notes
            .extend(device_findings(path, &parsed.config, devices, &noted));
    }
    out.notes.extend(
        remarks
            .iter()
            .map(|(line, r)| at(path, *line, r).to_string()),
    );
}

/// The lines of the rules using `connector-path:`, which no display's
/// identity carries on this build (vtium/vtium#157).
fn connector_path_rules(config: &Heads) -> Vec<usize> {
    config
        .rules
        .iter()
        .filter(|r| match &r.matcher {
            Match::Any => false,
            Match::All(tokens) => tokens.iter().any(|t| matches!(t, Token::ConnectorPath(_))),
        })
        .map(|r| r.line)
        .collect()
}

/// A device as it reports itself, escaped: its name, else its path, else
/// its connector.
fn reported(id: &Identity) -> String {
    let shown = id
        .name
        .as_deref()
        .or(id.path.as_deref())
        .or(id.connector.as_deref())
        .unwrap_or(check_config::UNNAMED_DEVICE);
    format!("\"{}\"", shown.escape_debug())
}

/// What depends on the devices present (ADR-1401 D7, D9): each rule no
/// device matches, but those on the lines `noted`, already noted as
/// matching nothing, and the layer 1 warnings, each naming its device as
/// reported. Each is a note outside the status, so a file can be checked
/// away from its machine.
pub fn device_findings(
    path: &Path,
    config: &Heads,
    devices: &[Identity],
    noted: &[usize],
) -> Vec<String> {
    let head = |i: usize| config.heads.get(i).map_or("?", |h| h.name.as_str());
    let mut out: Vec<String> = config
        .idle_rules(devices)
        .into_iter()
        .filter(|line| !noted.contains(line))
        .map(|line| at(path, Some(line), check_config::MATCHES_NOTHING).to_string())
        .collect();
    for l in config.lockout(devices) {
        let what = match l {
            Lockout::PointersOnly { head: h } => check_config::pointers_only(head(h)),
            Lockout::EveryKeyboardIgnored => check_config::EVERY_KEYBOARD_IGNORED.to_owned(),
            Lockout::OneKeyboard { head: h, device } => check_config::one_keyboard(
                head(h),
                &devices.get(device).map_or_else(String::new, reported),
            ),
        };
        out.push(at(path, None, what).to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use std::path::PathBuf;

    use super::*;
    use crate::config;

    fn run_with(args: &[&str], conf: &str) -> (Vec<String>, i32) {
        let args: Vec<OsString> = [SUBCOMMAND]
            .iter()
            .chain(args)
            .map(OsString::from)
            .collect();
        let conf = conf.as_bytes().to_vec();
        let loaded = config::load(&args, Some("seat0"), &mut |p: &Path| {
            Ok((p == Path::new("/etc/kmscon/kmscon.conf")).then(|| conf.clone()))
        });
        assert!(loaded.check);
        run(&loaded)
    }

    #[test]
    fn a_clean_configuration_exits_zero() {
        let (lines, status) = run_with(&[], "font-size=16\nno-blink\n");
        assert_eq!((lines, status), (Vec::<String>::new(), 0));
    }

    #[test]
    fn each_warning_is_printed_and_the_status_is_one() {
        let (lines, status) = run_with(&["--bogus"], "sb-size=200000\nfrob=1\n");
        assert_eq!(status, 1);
        assert_eq!(
            lines,
            [
                "argument 2: unknown key \"--bogus\", ignored",
                "/etc/kmscon/kmscon.conf:1: sb-size=\"200000\" is out of bounds, 100000 used",
                "/etc/kmscon/kmscon.conf:2: unknown key \"frob\", ignored",
            ]
        );
    }

    #[test]
    fn what_the_values_mean_together_is_checked_too() {
        let (lines, status) = run_with(&[], "mode=1024x768\nuse-original-mode\n");
        assert_eq!(status, 1);
        assert_eq!(lines, ["mode is set, so use-original-mode is off"]);
    }

    #[test]
    fn a_palette_vtium_does_not_hold_is_reported() {
        let (lines, status) = run_with(&["--palette=frob"], "palette=nord\n");
        assert_eq!(status, 1);
        assert_eq!(
            lines,
            [
                "argument 2: palette=\"frob\" is none of legacy, custom, nord, solarized, \
                 solarized-black, solarized-white, soft-black, base16-dark, base16-light, vga, \
                 ignored"
            ]
        );
        for name in vtium_term::PALETTE_NAMES.iter().chain(&["custom"]) {
            let conf = format!("palette={name}\n");
            assert_eq!(run_with(&[], &conf), (Vec::new(), 0), "{name}");
        }
    }

    #[test]
    fn a_key_not_built_yet_warns() {
        let (lines, status) = run_with(&[], "hwaccel\n");
        assert_eq!(status, 1);
        assert_eq!(lines, ["hwaccel is not built yet; vtium draws in software"]);
        assert_eq!(run_with(&[], "no-hwaccel\n"), (Vec::new(), 0));
    }

    fn status(cgt: &str) -> Result<String, String> {
        Ok(format!(
            "Name:\tinit\nSigIgn:\t0000000000001000\nSigCgt:\t{cgt}\nCapInh:\t0\n"
        ))
    }

    fn power_off_warnings(conf: &str, status: &Result<String, String>) -> Vec<String> {
        let args = [OsString::from(SUBCOMMAND)];
        let conf = conf.as_bytes().to_vec();
        let loaded = config::load(&args, None, &mut |_: &Path| Ok(Some(conf.clone())));
        semantics_under(&loaded.config, status, 38)
    }

    const BOUND: &str = "grab-power-off=<Ctrl><Alt>End\n";

    #[test]
    fn grab_power_off_is_silent_where_init_catches_the_signal() {
        // Both masks have bit 37 (signal 38) set.
        for cgt in ["00000021fdc1fefb", "0000002000014a03"] {
            assert_eq!(
                power_off_warnings(BOUND, &status(cgt)),
                [] as [&str; 0],
                "{cgt}"
            );
        }
    }

    #[test]
    fn grab_power_off_warns_where_init_catches_nothing_of_it() {
        assert_eq!(
            power_off_warnings(BOUND, &status("0000000000000000")),
            [
                "grab-power-off is bound, and PID 1 does not catch SIGRTMIN+4 (signal 38, \
                 from SigCgt in /proc/1/status): it powers nothing off"
            ]
        );
        // Bit 38 (signal 39) is a neighbour, not the signal.
        assert_eq!(
            power_off_warnings(BOUND, &status("0000004000000000")).len(),
            1
        );
    }

    #[test]
    fn grab_power_off_names_why_an_unreadable_status_does_not_answer() {
        let unreadable = Err("Permission denied (os error 13)".to_owned());
        assert_eq!(
            power_off_warnings(BOUND, &unreadable),
            [
                "grab-power-off is bound, and whether PID 1 catches SIGRTMIN+4 (signal 38) is \
                 unknown, since /proc/1/status could not be read: Permission denied (os error 13)"
            ]
        );
        let no_line = Ok("Name:\tinit\n".to_owned());
        let says = |lines: Vec<String>, what: &str| lines.iter().all(|l| l.contains(what));
        assert!(says(
            power_off_warnings(BOUND, &no_line),
            "since /proc/1/status has no SigCgt line"
        ));
        assert!(says(
            power_off_warnings(BOUND, &status("zz")),
            "not a hex mask"
        ));
        assert_eq!(power_off_warnings(BOUND, &no_line).len(), 1);
        assert_eq!(power_off_warnings(BOUND, &status("zz")).len(), 1);
    }

    #[test]
    fn an_unbound_grab_power_off_checks_nothing() {
        let unbound = "grab-reboot=<Ctrl><Alt>Delete\n";
        assert_eq!(power_off_warnings(unbound, &status("0")), [] as [&str; 0]);
        assert_eq!(
            power_off_warnings(unbound, &Err("x".to_owned())),
            [] as [&str; 0]
        );
    }

    // #145: a frit's cell that does not suit it, where no font need be
    // parsed to tell
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_frit_font_name_names_is_checked() {
        let dir = std::env::temp_dir().join(format!("vtium-check-frit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("small.vtfrit"),
            "source=builtin:unifont\ncell=8x12\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("fine.vtfrit"),
            "source=outline:/f.ttf\ncell=10x20\n",
        )
        .unwrap();
        let config = |name: &str| {
            let args: Vec<OsString> = [SUBCOMMAND, "--font-name", name]
                .iter()
                .map(OsString::from)
                .collect();
            config::load(&args, Some("seat0"), &mut |_: &Path| Ok(None)).config
        };
        assert_eq!(
            frit(&config("small"), &dir),
            [
                "the frit small: line 1: source=builtin:unifont's 8x16 cell does not fit its 8x12 cell, so it gives nothing"
            ]
        );
        assert!(frit(&config("fine"), &dir).is_empty());
        assert!(frit(&config("ter-v16n"), &dir).is_empty(), "no frit");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // the operator on #145, 2026-10-28: the density points convert at,
    // display-dpi's, else each display's measured or assumed, which only
    // the start can read, since check-config opens no display
    #[test]
    fn the_density_source_is_noted_outside_the_status() {
        let note = |args: &[&str]| {
            let args: Vec<OsString> = [SUBCOMMAND]
                .iter()
                .chain(args)
                .map(OsString::from)
                .collect();
            notes(&config::load(&args, None, &mut |_: &Path| Ok(None)).config)
        };
        assert_eq!(
            note(&["--display-dpi=109"]),
            [
                "display-dpi=109: every display converts points at 109 DPI, its measured density aside"
            ]
        );
        assert_eq!(
            note(&[]),
            [
                "each display converts points at the density it measures from its physical size, else at 96 DPI with a warning naming it; the start logs which"
            ]
        );
        assert_eq!(run_with(&["--display-dpi=109"], ""), (Vec::new(), 0));
    }

    /// A head file in a fixture directory of its own, the trust that
    /// reads it as the start reads root's files, and the directory.
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn heads_fixture(name: &str, text: &str, mode: u32) -> (std::path::PathBuf, Trust) {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("vtium-check-heads-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = dir.join("heads.conf");
        std::fs::write(&file, text).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
        let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
        (file, trust)
    }

    /// kmscon.conf's caps with `head_cap` and the default total, and frits
    /// nowhere.
    fn caps(head_cap: u32) -> Kmscon<'static> {
        Kmscon {
            head_cap,
            total: vtium_config::CONSOLES_TOTAL_DEFAULT,
            frits: Path::new("/nonexistent/vtium/frits"),
        }
    }

    /// A machine where a udev daemon runs or not, whose devices are not
    /// read.
    fn machine(udev_daemon: bool) -> Machine<'static> {
        Machine {
            udev_daemon,
            devices: None,
        }
    }

    fn checked(name: &str, text: &str, udev: bool) -> (HeadCheck, String) {
        let (file, trust) = heads_fixture(name, text, 0o644);
        let out = head_file(&file, false, &trust, &machine(udev), &caps(4));
        std::fs::remove_dir_all(file.parent().unwrap_or(&file)).ok();
        (out, file.display().to_string())
    }

    // ADR-1401 D7: a warning about the file itself counts against it
    #[test]
    fn a_head_file_warning_counts_and_its_notes_do_not() {
        let (out, f) = checked(
            "warn",
            "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> roaming\ninput class:keyboard -> desk\n",
            true,
        );
        assert_eq!(
            out.counted,
            [format!(
                "{f}:6: the rule on line 5 takes every device this one matches"
            )]
        );
        assert_eq!(verdict(&out, &f), [AS_WRITTEN, UNSET]);
        assert_eq!(out.notes.len(), 2, "{:?}", out.notes);
    }

    /// The last two notes of `out`, the outcome and the policy, without
    /// the path `f` before them.
    fn verdict<'a>(out: &'a HeadCheck, f: &str) -> Vec<&'a str> {
        let prefix = format!("{f}: ");
        let at = out.notes.len().saturating_sub(2);
        out.notes
            .get(at..)
            .unwrap_or_default()
            .iter()
            .map(|n| n.strip_prefix(&prefix).unwrap_or(n))
            .collect()
    }

    const OPEN: &str = "The policy is open: an input no rule places roams, a head is isolated only where it says `isolated = yes`, and the anti-lockout guard and the rescue key are on.";
    const UNSET: &str = "The file sets no `policy`, so its policy is open: an input no rule places roams, a head is isolated only where it says `isolated = yes`, and the anti-lockout guard and the rescue key are on.";
    const CLOSED: &str = "The policy is closed: an input no rule places is ignored, a head that leaves `isolated` unset is isolated, and the anti-lockout guard and the rescue key are off.";
    const AS_WRITTEN: &str = "A start runs this file as written and shows nothing.";
    const PARTLY_OPEN: &str = "A start runs the heads and rules that survive, then the shipped rules' tail, with every head acting as not isolated whatever its `isolated` says, and shows `config partly invalid, running what's valid`.";
    const PARTLY_UNSET_POLICY: &str = "The file sets no `policy`, so its policy is open, and since the file is partly broken every head acts as not isolated whatever its `isolated` says: an input no surviving rule places walks the shipped rules' tail, and the anti-lockout guard and the rescue key are on.";
    const PARTLY_OPEN_POLICY: &str = "The policy is open, and since the file is partly broken every head acts as not isolated whatever its `isolated` says: an input no surviving rule places walks the shipped rules' tail, and the anti-lockout guard and the rescue key are on.";

    // ADR-1401 D7, level 3: each error names its line and what it drops,
    // and the file is read on past it, so what survives is checked too
    // covers: platform.heads.rules
    #[test]
    fn each_error_names_its_line_and_what_it_drops() {
        let (out, f) = checked(
            "drops",
            "version = 1\n\
             [head desk]\n\
             frob = 1\n\
             [head kiosk]\n\
             [head kiosk]\n\
             consoles = 2\n\
             [rules]\n\
             display * -> kiosk\n\
             input path:pci-0000:15:00.0-usb-0:3.3 -> desc\n\
             input * -> roaming\n\
             input * -> desk\n\
             input class:keyboard -> roaming\n",
            true,
        );
        let at = |line: usize, what: &str| format!("{f}:{line}: {what}");
        assert_eq!(
            out.counted,
            [
                at(
                    3,
                    "This section takes no key named \"frob\". Check the key's spelling, or remove the line."
                ),
                at(
                    3,
                    "For this error vtium drops this head, with every rule that names it."
                ),
                at(
                    5,
                    "The file already declares a head named \"kiosk\". Give one of the two another name."
                ),
                at(
                    5,
                    "For this error vtium drops this section, with every line in it."
                ),
                at(
                    9,
                    "This rule sends devices to the head \"desc\", which the file does not declare. Correct the name, or declare the head with its own `[head NAME]` section."
                ),
                at(
                    9,
                    "For this error vtium drops this rule alone, and a device walks on to the next rule."
                ),
                // the survivors' own warning: line 11 went with head desk
                at(
                    12,
                    "the rule on line 10 takes every device this one matches"
                ),
            ]
        );
        assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_UNSET_POLICY]);
    }

    // ADR-1401 D7: after its errors the report gives what a start does
    // with the file, by D7's outcomes table, and the policy it states
    // covers: platform.heads.policy
    #[test]
    fn the_outcome_and_the_policy_close_the_report() {
        let head = "[head a]\n[rules]\ndisplay * -> a\n";
        let rows = [
            (
                format!("version = 1\npolicy = open\n{head}"),
                "A start runs this file as written and shows nothing.",
                OPEN,
            ),
            (
                format!("policy = closed\nversion = 1\n{head}"),
                "A start runs this file as written and shows nothing.",
                CLOSED,
            ),
            (
                "version = 1\n".to_owned(),
                "This file declares nothing, so a start runs the shipped default and shows nothing.",
                UNSET,
            ),
            (
                "version = 1\n[rules]\ninput * -> roaming\n".to_owned(),
                "This file has rules and declares no head, so a start runs its rules, and the display guard lights the boot display for the default head.",
                UNSET,
            ),
            (
                "version = 1\npolicy = closed\n".to_owned(),
                "This file declares no head under the closed policy, so a start lights no display, delivers no input and shows nothing: loaded but off.",
                CLOSED,
            ),
            (
                format!("version = 1\n{head}display * -> b\n"),
                PARTLY_OPEN,
                PARTLY_UNSET_POLICY,
            ),
            (
                format!("version = 1\npolicy = closed\n{head}display * -> b\n"),
                "A start runs the heads and rules that survive, with the rescue key off; each head keeps the isolation it states, and a head that states none is isolated. It shows `config partly invalid, running what's valid`.",
                CLOSED,
            ),
            (
                format!("version = 2\n{head}"),
                "No head survives this file's errors, so a start runs the shipped default and shows `invalid config skipped, using builtin`.",
                UNSET,
            ),
            (
                format!("version = 2\npolicy = closed\n{head}"),
                "No head survives this file's errors under the closed policy, so a start delivers no input and shows the panic screen on the boot display, starting `vtium: config invalid, policy closed: input off`.",
                CLOSED,
            ),
            (
                format!("version = 1\npolicy = maybe\n{head}"),
                PARTLY_OPEN,
                "vtium cannot read the file's `policy` line, so the file counts as open and as partly broken, and every head acts as not isolated whatever its `isolated` says. Write one `policy = open` or `policy = closed` line above the first section.",
            ),
        ];
        for (n, (text, outcome, policy)) in rows.iter().enumerate() {
            let (out, f) = checked(&format!("row-{n}"), text, true);
            assert_eq!(verdict(&out, &f), [*outcome, *policy], "{text}");
            // an error, and only an error, counts against the file
            let broken = text.contains("-> b") || text.contains("= 2") || text.contains("maybe");
            assert_eq!(out.counted.is_empty(), !broken, "{text}: {:?}", out.counted);
        }
    }

    // ADR-1401 D7: a line that is not UTF-8 drops only its rule, and the
    // file runs what survives
    // covers: platform.heads.rules
    #[test]
    fn a_line_that_is_not_utf8_drops_only_its_rule() {
        let (file, trust) = heads_fixture("bytes", "", 0o644);
        std::fs::write(
            &file,
            b"version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput name:\xff -> a\n",
        )
        .ok();
        let f = file.display().to_string();
        let out = head_file(&file, false, &trust, &machine(true), &caps(4));
        std::fs::remove_dir_all(file.parent().unwrap_or(&file)).ok();
        assert_eq!(
            out.counted,
            [
                format!("{f}:5: This line is not UTF-8 text. Save the file as UTF-8."),
                format!(
                    "{f}:5: For this error vtium drops this rule alone, and a device walks on to the next rule."
                ),
            ]
        );
        assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_UNSET_POLICY]);
    }

    // ADR-1401 D10: a head's `consoles` past kmscon.conf's
    // `head-consoles-max` gets the cap, and counts against the file, once,
    // at its `consoles` line, past the hard cap of 50 too
    // covers: platform.heads.rules
    #[test]
    fn consoles_past_the_head_cap_count_against_the_file() {
        let past = |n: u32| {
            format!(
                "The head \"a\" asks for more consoles than `head-consoles-max` allows, {n}, so it gets {n}. Lower its `consoles` to {n} or fewer, or raise `head-consoles-max` in kmscon.conf."
            )
        };
        for (consoles, cap, want) in [
            (5, 4, Some(past(4))),
            (60, 4, Some(past(4))),
            (5, 6, None),
            (
                60,
                50,
                Some(
                    "`consoles` is past 50, the most consoles a head may have, so the head gets 50 at most. Lower it to 50 or fewer."
                        .to_owned(),
                ),
            ),
        ] {
            let text =
                format!("version = 1\n[head a]\nconsoles = {consoles}\n[rules]\ndisplay * -> a\n");
            let (file, trust) = heads_fixture(&format!("cap-{consoles}-{cap}"), &text, 0o644);
            let f = file.display().to_string();
            let kmscon = Kmscon {
                total: 64,
                ..caps(cap)
            };
            let out = head_file(&file, false, &trust, &machine(true), &kmscon);
            std::fs::remove_dir_all(file.parent().unwrap_or(&file)).ok();
            let want: Vec<String> = want.iter().map(|w| format!("{f}:3: {w}")).collect();
            assert_eq!(out.counted, want, "consoles {consoles}, cap {cap}");
        }
    }

    // ADR-1401 D10: with kmscon.conf's caps unset, check-config caps as the
    // seat does, 4 consoles a head and 8 in all
    #[test]
    fn the_caps_unset_are_the_seats() {
        let k = Kmscon::of(&config::Config::default(), Path::new("/f"));
        assert_eq!((k.head_cap, k.total), (4, 8));
        assert_eq!(
            (k.head_cap, k.total),
            (
                vtium_config::HEAD_CONSOLES_DEFAULT,
                vtium_config::CONSOLES_TOTAL_DEFAULT
            )
        );
    }

    // ADR-1401 D10: past `consoles-max`, each head keeps its first console
    // and the others go in the file's order while the total lasts; a head
    // left short counts against the file
    // covers: platform.heads.rules
    #[test]
    fn consoles_past_the_total_count_against_the_file() {
        let (out, f) = checked(
            "total",
            "version = 1\n[head a]\nconsoles = 4\n[head b]\nconsoles = 4\n[head c]\nconsoles = 4\n\
             [rules]\ndisplay * -> a\ninput * -> roaming\n",
            true,
        );
        let short = |line: usize, head: &str, gets: u32| {
            format!(
                "{f}:{line}: The heads together ask for more consoles than `consoles-max` allows, 8, so the head \"{head}\" gets {gets}. Lower the heads' `consoles`, or raise `consoles-max` in kmscon.conf."
            )
        };
        assert_eq!(out.counted, [short(5, "b", 3), short(7, "c", 1)]);
    }

    // ADR-1401 D7: a profile naming a frit not installed is a warning about
    // the file, and an installed one is none
    // covers: platform.heads.rules
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_profile_naming_a_missing_frit_counts_against_the_file() {
        let text = "version = 1\n[head a]\nprofile = desk\n[rules]\ndisplay * -> a\n";
        let (file, trust) = heads_fixture("frit", text, 0o644);
        let f = file.display().to_string();
        let frits = file.parent().unwrap().join("frits");
        std::fs::create_dir_all(&frits).unwrap();
        let kmscon = Kmscon {
            frits: &frits,
            ..caps(4)
        };
        let missing = head_file(&file, false, &trust, &machine(true), &kmscon);
        std::fs::write(frits.join("desk.vtfrit"), "source=builtin:unifont\n").unwrap();
        let installed = head_file(&file, false, &trust, &machine(true), &kmscon);
        std::fs::remove_dir_all(file.parent().unwrap()).ok();
        let dir = frits.display();
        assert_eq!(
            missing.counted,
            [format!(
                "{f}:3: The head \"a\"'s profile names the frit \"desk\", which is not in \"{dir}\", so the head draws the default frit. Put that frit in \"{dir}\", or change the head's `profile`."
            )]
        );
        assert_eq!(installed.counted, [] as [String; 0]);
    }

    // ADR-1401 D7: the two drops the rest of the tests leave unsaid: an
    // unknown version drops the whole file but its policy, and a `policy`
    // line inside a section drops that line alone
    // covers: platform.heads.rules
    #[test]
    fn a_whole_file_error_and_a_line_error_say_what_they_drop() {
        let (out, f) = checked("whole", "version = 2\n[head a]\n", true);
        assert_eq!(
            out.counted,
            [
                format!(
                    "{f}:1: vtium knows only `version = 1`, not version \"2\". Change the version line to `version = 1`."
                ),
                format!("{f}:1: For this error vtium drops the whole file but its `policy`."),
            ]
        );
        let (out, f) = checked(
            "line",
            "version = 1\n[head a]\npolicy = closed\n[rules]\ndisplay * -> a\n",
            true,
        );
        assert_eq!(
            out.counted,
            [
                format!(
                    "{f}:3: vtium reads `policy` only above the first section, so it ignores this line, and the file's policy comes from the lines above the first section. Move it there, beside `version`."
                ),
                format!("{f}:3: For this error vtium drops this line alone."),
            ]
        );
    }

    // ADR-1401 D7: a `policy` line inside a section states nothing, so the
    // top-level line's policy stands
    // covers: platform.heads.policy
    #[test]
    fn a_policy_line_inside_a_section_leaves_the_top_levels_standing() {
        let (out, f) = checked(
            "in-section",
            "version = 1\npolicy = open\n[head a]\npolicy = closed\n[rules]\ndisplay * -> a\n",
            true,
        );
        assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_OPEN_POLICY]);
    }

    // ADR-1401 D7: under a partly broken open file `isolated` is ignored,
    // and the report says so where a head writes `isolated = yes`
    // covers: platform.heads.isolated
    #[test]
    fn a_partly_broken_open_file_says_isolated_is_ignored() {
        let (out, f) = checked(
            "isolated",
            "version = 1\n[head a]\nisolated = yes\n[rules]\ndisplay * -> a\ndisplay * -> b\n",
            true,
        );
        assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_UNSET_POLICY]);
    }

    // ADR-1401 D3: a head no rule names is valid, and noted
    // covers: platform.heads.rules
    #[test]
    fn a_head_no_rule_names_is_a_note() {
        let (out, f) = checked(
            "unnamed",
            "version = 1\n[head a]\n[head b]\n[rules]\ndisplay * -> a\n",
            true,
        );
        assert_eq!(out.counted, [] as [String; 0]);
        assert_eq!(
            out.notes,
            [
                format!(
                    "{f}:3: No rule names the head \"b\", so no display or input reaches it. Add a rule that sends a display to it, or remove the head."
                ),
                format!("{f}: {AS_WRITTEN}"),
                format!("{f}: {UNSET}"),
            ]
        );
    }

    /// `text` checked against the devices `devices`, its notes but the
    /// outcome and the policy, without the path before them.
    fn remarked(name: &str, text: &str, devices: &[Identity]) -> (Vec<String>, Vec<String>) {
        let (file, trust) = heads_fixture(name, text, 0o644);
        let f = format!("{}: ", file.display());
        let m = Machine {
            udev_daemon: true,
            devices: Some(devices),
        };
        let out = head_file(&file, false, &trust, &m, &caps(4));
        std::fs::remove_dir_all(file.parent().unwrap_or(&file)).ok();
        let notes = out.notes.len().saturating_sub(2);
        let notes = out
            .notes
            .get(..notes)
            .unwrap_or_default()
            .iter()
            .map(|n| n.strip_prefix(&f).unwrap_or(n).to_owned())
            .collect();
        (out.counted, notes)
    }

    /// A display, a keyboard and a joystick, each named.
    fn present() -> Vec<Identity> {
        use vtium_config::Class;
        let named = |mut id: Identity, name: &str| {
            id.name = Some(name.to_owned());
            id
        };
        vec![
            named(Identity::display(), "DP-1"),
            named(Identity::input().with_class(Class::Keyboard), "Keyboard A"),
            named(Identity::input().with_class(Class::Joystick), "Pad"),
        ]
    }

    // ADR-1401 D9, "What fell to the default": each device present no rule
    // places is noted with what the policy gives it; under open the display
    // guard lights the display no rule takes, so it is not dark
    // covers: platform.heads.policy
    #[test]
    fn what_falls_to_the_open_policy_is_noted() {
        let (counted, notes) = remarked(
            "fell-open",
            "version = 1\n[head a]\n[rules]\ninput class:joystick -> a\n",
            &present(),
        );
        assert_eq!(counted, [] as [String; 0]);
        assert_eq!(
            notes,
            [
                "No rule matches the input \"Keyboard A\", so the open policy lets it roam into the active head. Add an `input` rule for it to place it anywhere else.",
            ]
        );
    }

    // ADR-1401 D5, D9: under closed no head is given a display the file does
    // not give it, so a head with no display rule leaves no head that can be
    // the active head, and the display stays dark
    // covers: platform.heads.focus
    #[test]
    fn no_head_that_can_be_active_is_noted() {
        let (counted, notes) = remarked(
            "no-active",
            "version = 1\npolicy = closed\n[head a]\nisolated = no\n[rules]\n\
             input class:keyboard -> a\ninput * -> ignore\n",
            &present(),
        );
        assert_eq!(counted, [] as [String; 0]);
        assert_eq!(
            notes,
            [
                "head a's only keyboard is one device, \"Keyboard A\"",
                "No head that can be the active head has a display present, so a keyboard that roams reaches no head until one lights. Connect a display for a head that is not isolated.",
            ]
        );
    }

    // ADR-1401 D9: under closed, a keyboard no rule places is ignored, and
    // the lockout that leaves is the first thing said
    // covers: platform.heads.policy
    #[test]
    fn a_closed_lockout_is_noted_first() {
        let (counted, notes) = remarked(
            "fell-closed",
            "version = 1\npolicy = closed\n[head a]\nisolated = no\n[rules]\ndisplay * -> a\n",
            &present(),
        );
        assert_eq!(counted, [] as [String; 0]);
        assert_eq!(
            notes,
            [
                "No keyboard present reaches any head, so under the closed policy nobody can type at this machine. Send a keyboard to a head, or to `roaming` with a head that is not isolated, before you install the file.",
                "every keyboard is ignored",
                "No rule matches the input \"Keyboard A\", so the closed policy ignores it. Add an `input` rule for it if it should reach a head.",
                "No rule matches the input \"Pad\", so the closed policy ignores it. Add an `input` rule for it if it should reach a head.",
            ]
        );
    }

    // ADR-1401 D5, D9: a closed file in which every head is isolated draws
    // the note, and a keyboard a head claims is no lockout
    // covers: platform.heads.isolated
    #[test]
    fn every_head_isolated_is_noted() {
        let (counted, notes) = remarked(
            "isolated-all",
            "version = 1\npolicy = closed\n[head a]\n[rules]\ndisplay * -> a\n\
             input class:keyboard -> a\ninput * -> ignore\n",
            &present(),
        );
        assert_eq!(counted, [] as [String; 0]);
        assert_eq!(
            notes,
            [
                "head a's only keyboard is one device, \"Keyboard A\"",
                "Every head is isolated, so no keyboard can switch the VT and no head can be the active head: only a keyboard a head's own rule claims reaches it. Write `isolated = no` on a head if a keyboard should roam to it."
            ]
        );
    }

    // ADR-1401 D7: the device notes of a partly broken open file walk the
    // shipped rules' tail a start runs after the surviving rules
    // covers: platform.heads.rules
    #[test]
    fn a_partly_broken_open_files_devices_walk_the_shipped_tail() {
        let (counted, notes) = remarked(
            "fell-tail",
            "version = 1\n[head a]\n[rules]\ndisplay * -> a\ndisplay * -> b\n",
            &present(),
        );
        assert_eq!(counted.len(), 2, "{counted:?}");
        assert_eq!(
            notes,
            [
                "No surviving rule matches the input \"Keyboard A\", so it walks the shipped rules' tail and roams into the active head.",
                "No surviving rule matches the input \"Pad\", so it walks the shipped rules' tail and is ignored.",
            ]
        );
    }

    // ADR-1401 D3, D9: the head file has no `[guards]` and no `rescue`, so
    // a file in the first cut's names counts against itself, each error
    // naming the word that replaced the old one
    #[test]
    fn the_first_cuts_guards_and_rescue_count_against_the_file() {
        let (out, f) = checked(
            "guards",
            "version = 1\n[head kiosk]\nrescue = off\n[rules]\ndisplay * -> kiosk\n\
             [guards]\nanti-lockout = off\n",
            true,
        );
        let counted: Vec<&str> = out.counted.iter().map(String::as_str).collect();
        let at = |line: usize, word: &str| {
            counted
                .iter()
                .any(|c| c.starts_with(&format!("{f}:{line}: ")) && c.contains(word))
        };
        assert!(at(3, "`isolated = yes`"), "{counted:?}");
        assert!(at(6, "`policy`"), "{counted:?}");
        assert_eq!(
            verdict(&out, &f),
            [
                "No head survives this file's errors, so a start runs the shipped default and shows `invalid config skipped, using builtin`.",
                UNSET
            ]
        );
        assert_eq!(out.notes.len(), 2, "{:?}", out.notes);
    }

    // ADR-1401 D4, probe 4 (the recommendation, WEAK_MATCH_COUNTS): input
    // sent to a head by `id` or `name` alone is a note
    #[test]
    fn input_reaching_a_head_by_id_or_name_alone_is_a_note() {
        let (out, f) = checked(
            "weak",
            "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\n\
             input id:046d:c52b -> desk\ninput name:Foo* -> desk\n\
             input id:046d:c52c path:pci-0000:00:14.0-usb-0:1 -> desk\n",
            true,
        );
        assert_eq!(out.counted, [] as [String; 0]);
        let weak = |line: usize| {
            format!(
                "{f}:{line}: input reaches a head by `id` or `name` alone, which any device can claim"
            )
        };
        let shown = |what: &str| format!("{f}: {what}");
        assert_eq!(
            out.notes,
            [weak(5), weak(6), shown(AS_WRITTEN), shown(UNSET)]
        );
    }

    // ADR-1401 D4: with no udev daemon, each rule using a key that reads
    // udev's database is noted, naming the key
    #[test]
    fn without_a_udev_daemon_each_rule_needing_one_is_noted_with_its_key() {
        let text = "version = 1\n[head desk]\n[rules]\n\
                    display card:pci-0000:03:00.0 -> desk\n\
                    display connector:pci-0000:03:00.0/DP-* -> desk\n\
                    display edid:AAA:0x0001:LARGE00001 -> desk\n\
                    input path:pci-0000:15:00.0-usb-0:3.3 serial:aaaa:0001:X1 -> desk\n\
                    input udev:VTIUM_HEAD=desk -> desk\n\
                    input class:keyboard -> roaming\n";
        let (out, f) = checked("udev", text, false);
        assert_eq!(out.counted, [] as [String; 0]);
        let no = |line: usize, key: &str| {
            format!("{f}:{line}: no udev daemon runs, so `{key}:` matches nothing")
        };
        let udev: Vec<&String> = out
            .notes
            .iter()
            .filter(|n| n.contains("udev daemon"))
            .collect();
        assert_eq!(
            udev,
            [
                &no(4, "card"),
                &no(5, "connector"),
                &no(7, "path"),
                &no(7, "serial"),
                &no(8, "udev")
            ]
        );
        let (out, _) = checked("udevd", text, true);
        assert!(
            !out.notes.iter().any(|n| n.contains("udev daemon")),
            "{:?}",
            out.notes
        );
    }

    // ADR-1401 D4, vtium/vtium#157: no display carries a KMS connector's
    // PATH on this build, so each `connector-path:` rule is noted, with or
    // without a udev daemon
    #[test]
    fn each_connector_path_rule_is_noted_as_matching_nothing_on_this_build() {
        let text = "version = 1\n[head desk]\n[head hub]\n[rules]\n\
                    display connector-path:mst:63-1 -> hub\n\
                    display edid:AAA:0x0001:LARGE00001 -> desk\n\
                    display connector-path:mst:63-2 edid:BBB:0x0002:X -> hub\n\
                    display * -> desk\n";
        for udev in [true, false] {
            let (out, f) = checked(&format!("mst-{udev}"), text, udev);
            assert_eq!(out.counted, [] as [String; 0]);
            let mst: Vec<&String> = out
                .notes
                .iter()
                .filter(|n| n.contains("connector-path"))
                .collect();
            let none = |line: usize| {
                format!(
                    "{f}:{line}: this build reads no connector's PATH, so `connector-path:` \
                     matches nothing (vtium/vtium#157)"
                )
            };
            assert_eq!(mst, [&none(5), &none(7)], "udev {udev}");
        }
    }

    // ADR-1401 D6, D7: the installed head file absent is the shipped
    // rules' case, a candidate absent is an error
    #[test]
    fn an_absent_head_file_is_a_note_and_an_absent_candidate_an_error() {
        let path =
            std::env::temp_dir().join(format!("vtium-check-heads-none-{}", std::process::id()));
        let trust = Trust::ROOT;
        let shown = path.display();
        assert_eq!(
            head_file(&path, false, &trust, &machine(true), &caps(4)),
            HeadCheck {
                counted: Vec::new(),
                notes: vec![format!(
                    "{shown}: There is no head file here, so a start runs the compiled-in configuration and shows nothing."
                )],
            }
        );
        assert_eq!(
            head_file(&path, true, &trust, &machine(true), &caps(4)),
            HeadCheck {
                counted: vec![format!("{shown}: does not exist")],
                notes: Vec::new(),
            }
        );
    }

    // ADR-1401 D10: the installed file failing the ownership check is
    // invalid; a candidate failing it where it sits is noted and checked
    // (the recommendation, CANDIDATE_OWNERSHIP_COUNTS)
    #[test]
    fn ownership_counts_for_the_installed_file_and_is_a_note_for_a_candidate() {
        let text = "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> desc\n";
        let (file, trust) = heads_fixture("owner", text, 0o666);
        let f = file.display().to_string();
        let untrusted = format!(
            "{f}: vtium does not trust this file, because \"{f}\" can be changed by its group or by other users (mode 666). Make root the owner of that path, and let only root write to it."
        );
        let installed = head_file(&file, false, &trust, &machine(true), &caps(4));
        assert_eq!(installed.counted, [untrusted]);
        assert_eq!(
            installed.notes,
            [format!(
                "{f}: A start never reads a file it does not trust, so it runs the shipped default and shows `invalid config skipped, using builtin`."
            )]
        );
        let candidate = head_file(&file, true, &trust, &machine(true), &caps(4));
        assert_eq!(
            candidate.notes,
            [
                format!(
                    "{f}: \"{f}\" can be changed by its group or by other users (mode 666). The start reads this file once it is installed, owned by root and writable only by root."
                ),
                format!("{f}: {PARTLY_OPEN}"),
                format!("{f}: {PARTLY_UNSET_POLICY}"),
            ]
        );
        assert_eq!(
            candidate.counted,
            [
                format!(
                    "{f}:5: This rule sends devices to the head \"desc\", which the file does not declare. Correct the name, or declare the head with its own `[head NAME]` section."
                ),
                format!(
                    "{f}:5: For this error vtium drops this rule alone, and a device walks on to the next rule."
                ),
            ]
        );
        std::fs::remove_dir_all(file.parent().unwrap_or(&file)).ok();
    }

    // ADR-1401 D7, D9: what depends on the devices present names each
    // rule no device matches and each layer 1 warning, with the device as
    // it reports itself, escaped
    #[test]
    fn device_findings_name_idle_rules_and_lockouts_with_the_device_escaped() {
        use vtium_config::{Class, parse};
        let text = "version = 1\n[head desk]\n[head panel]\n[rules]\n\
                    display * -> desk\n\
                    input name:Keyb* -> desk\n\
                    input class:mouse -> panel\n\
                    input name:Gone -> panel\n";
        let config = parse(text).map(|p| p.config).unwrap_or_default();
        let mut keyboard = Identity::input().with_class(Class::Keyboard);
        keyboard.name = Some("Keyb\u{1b}[2J".to_owned());
        let mouse = Identity::input().with_class(Class::Mouse);
        let path = Path::new("/etc/vtium/heads.conf");
        assert_eq!(
            device_findings(path, &config, &[Identity::display(), keyboard, mouse], &[]),
            [
                "/etc/vtium/heads.conf:8: the rule matches no device present",
                "/etc/vtium/heads.conf: head desk's only keyboard is one device, \"Keyb\\u{1b}[2J\"",
                "/etc/vtium/heads.conf: head panel claims pointers and no keyboard, and every keyboard is claimed by another head",
            ]
        );
    }

    // ADR-1401 D7: the word after the subcommand is the candidate, and a
    // second is no option
    #[test]
    fn the_word_after_the_subcommand_is_the_candidate() {
        let load = |args: &[&str]| {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            config::load(&args, None, &mut |_: &Path| Ok(None))
        };
        let l = load(&[SUBCOMMAND, "/tmp/heads.conf"]);
        assert_eq!(l.candidate.as_deref(), Some(Path::new("/tmp/heads.conf")));
        assert!(l.reports.is_empty(), "{:?}", l.reports);
        let l = load(&["-c", "/etc/kmscon", SUBCOMMAND, "a.conf", "b.conf"]);
        assert_eq!(l.candidate.as_deref(), Some(Path::new("a.conf")));
        assert_eq!(l.reports.len(), 1, "{:?}", l.reports);
        let l = load(&["a.conf", SUBCOMMAND]);
        assert_eq!(l.candidate, None);
        assert_eq!(l.reports.len(), 1, "{:?}", l.reports);
        let l = load(&[SUBCOMMAND]);
        assert_eq!((l.candidate, l.check), (None, true));
    }

    // a head file root keeps from other users, such as root's 0600 file in
    // a 0700 /etc/vtium: only root can check it, and the start, root,
    // reads it
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_head_file_the_user_may_not_read_can_be_checked_only_as_root() {
        use std::os::unix::fs::PermissionsExt;
        if rustix::process::geteuid().is_root() {
            eprintln!("run as root, every file is readable: the check is skipped");
            return;
        }
        let mode = |p: &Path, m: u32| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
        };
        let (file, trust) = heads_fixture("denied", "version = 1\n", 0o000);
        let dir = file.parent().unwrap().to_path_buf();
        let denied = HeadCheck {
            counted: vec![format!(
                "{}: permission denied: the head file can be checked only as root",
                file.display()
            )],
            notes: Vec::new(),
        };
        assert_eq!(
            head_file(&file, false, &trust, &machine(true), &caps(4)),
            denied,
            "the file"
        );
        assert_eq!(
            head_file(&file, true, &trust, &machine(true), &caps(4)),
            denied,
            "a candidate"
        );
        mode(&file, 0o644);
        mode(&dir, 0o000);
        assert_eq!(
            head_file(&file, false, &trust, &machine(true), &caps(4)),
            denied,
            "its directory"
        );
        mode(&dir, 0o700);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_udev_daemon_runs_where_its_control_socket_is_present() {
        let (file, _) = heads_fixture("udev-control", "", 0o644);
        assert!(udev_daemon_runs(&file));
        assert!(!udev_daemon_runs(&file.with_file_name("absent")));
        std::fs::remove_dir_all(file.parent().unwrap()).ok();
    }

    /// The subcommand on the command line `args` and a host of fixtures.
    fn outcome_on(args: &[&str], host: &Host) -> Outcome {
        let args: Vec<OsString> = [SUBCOMMAND]
            .iter()
            .chain(args)
            .map(OsString::from)
            .collect();
        outcome(&config::load(&args, None, &mut |_: &Path| Ok(None)), host)
    }

    // the installed head file is the host's, read as the start reads it,
    // and the udev daemon's socket is the host's too
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_installed_head_file_and_the_udev_socket_are_the_hosts() {
        let (file, _) = heads_fixture(
            "host",
            "version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput path:pci-0000:00:14.0-usb-0:1 -> a\n",
            0o644,
        );
        let f = file.display().to_string();
        let host = Host {
            head_file: file.clone(),
            udev_control: file.with_file_name("no-udev"),
            roots: nowhere(),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
        };
        let out = outcome_on(&[], &host);
        assert_eq!(out.status, 1);
        assert_eq!(
            out.stderr,
            [format!(
                "{f}: vtium does not trust this file, because \"{f}\" belongs to user {} and not to root. Make root the owner of that path, and let only root write to it.",
                rustix::process::geteuid().as_raw()
            ),]
        );
        assert!(
            out.stdout.contains(&format!(
                "{f}: A start never reads a file it does not trust, so it runs the shipped default and shows `invalid config skipped, using builtin`."
            )),
            "{:?}",
            out.stdout
        );
        // a copy is a candidate, which the ownership check notes alone
        let copy = file.with_file_name("candidate.conf");
        std::fs::copy(&file, &copy).unwrap();
        let c = copy.display().to_string();
        let out = outcome_on(&[copy.to_str().unwrap()], &host);
        assert_eq!((out.status, out.stderr), (0, Vec::new()));
        assert!(
            out.stdout.contains(&format!(
                "{c}:5: no udev daemon runs, so `path:` matches nothing"
            )),
            "{:?}",
            out.stdout
        );
        let host = Host {
            udev_control: file.clone(),
            ..host
        };
        let out = outcome_on(&[copy.to_str().unwrap()], &host);
        assert!(
            !out.stdout.iter().any(|n| n.contains("udev daemon")),
            "{:?}",
            out.stdout
        );
        let gone = Host {
            head_file: file.with_file_name("absent.conf"),
            ..host
        };
        let out = outcome_on(&[], &gone);
        assert_eq!((out.status, out.stderr), (0, Vec::new()));
        assert!(
            out.stdout
                .iter()
                .any(|n| n.ends_with(
                    "absent.conf: There is no head file here, so a start runs the compiled-in configuration and shows nothing."
                ))
        );
        std::fs::remove_dir_all(file.parent().unwrap()).ok();
    }

    // ADR-1401 D7: the installed file named on the command line, by its
    // own path or another, is checked as the start reads it, so the next
    // step a notice names never turns its skipped file into a candidate
    // covers: platform.heads.rules
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_installed_file_named_as_the_candidate_is_checked_as_installed() {
        let (file, _) = heads_fixture(
            "same",
            "version = 1\n[head a]\n[rules]\ndisplay * -> a\n",
            0o644,
        );
        let link = file.with_file_name("link.conf");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let host = Host {
            head_file: file.clone(),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
        };
        for named in [&file, &link] {
            let out = outcome_on(&[named.to_str().unwrap()], &host);
            assert_eq!(out.status, 1, "{named:?}");
            let n = named.display();
            assert_eq!(
                out.stderr,
                [format!(
                    "{n}: vtium does not trust this file, because \"{n}\" belongs to user {} and not to root. Make root the owner of that path, and let only root write to it.",
                    rustix::process::geteuid().as_raw()
                )],
                "{named:?}"
            );
        }
        std::fs::remove_dir_all(file.parent().unwrap()).ok();
    }

    // heads.md: each file docs/heads holds checks with exit status 0, its
    // profiles' frits installed, away from any machine
    // covers: platform.heads.rules
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn each_documented_head_file_checks_with_exit_0() {
        let docs = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/heads"));
        let (anchor, _) = heads_fixture("docs", "", 0o644);
        let frits = anchor.with_file_name("frits");
        std::fs::create_dir_all(&frits).unwrap();
        let host = Host {
            head_file: anchor.with_file_name("absent.conf"),
            udev_control: anchor.clone(),
            roots: nowhere(),
            frits: frits.clone(),
        };
        let mut files: Vec<PathBuf> = std::fs::read_dir(docs)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "conf"))
            .collect();
        files.sort();
        // the profiles the files name have their frits installed
        for f in &files {
            let text = std::fs::read_to_string(f).unwrap();
            let profiles = text.lines().filter_map(|l| {
                let (key, value) = l.split('#').next()?.split_once('=')?;
                (key.trim() == "profile").then(|| value.trim().to_owned())
            });
            for p in profiles {
                std::fs::write(
                    frits.join(format!("{p}.vtfrit")),
                    "source=builtin:unifont\n",
                )
                .unwrap();
            }
        }
        assert!(files.len() >= 7, "{files:?}");
        for f in &files {
            let out = outcome_on(&[f.to_str().unwrap()], &host);
            assert_eq!((out.status, &out.stderr), (0, &Vec::new()), "{f:?}");
        }
        std::fs::remove_dir_all(anchor.parent().unwrap()).ok();
    }

    /// Roots under which nothing is listed.
    fn nowhere() -> Roots {
        Roots {
            sys: PathBuf::from("/nonexistent/vtium/sys"),
            udev_data: PathBuf::from("/nonexistent/vtium/udev"),
        }
    }

    // the devices vtium-platform lists, through the mapping the start uses:
    // each card's displays, each framebuffer's as `fb`, then each input
    #[test]
    fn listed_devices_map_as_the_start_maps_them() {
        use vtium_platform::display::Identity as Display;
        use vtium_platform::input::{Class as Udev, Classes as UdevClasses};
        use vtium_platform::listing::{Card, Framebuffer};
        use vtium_platform::udev::InputIdentity;

        let input = |class: Udev, name: &str| InputIdentity {
            classes: UdevClasses::empty().with(class),
            tagged: true,
            name: Some(name.to_owned()),
            ..InputIdentity::default()
        };
        let display = |connector: &str| Display {
            connector: connector.to_owned(),
            card: Some("pci-0000:03:00.0".to_owned()),
            edid: None,
            lit_at_start: false,
        };
        let listing = Listing {
            inputs: vec![
                input(Udev::Keyboard, "Keyboard A"),
                input(Udev::Mouse, "A mouse"),
            ],
            cards: vec![Card {
                node: PathBuf::from("/dev/dri/card1"),
                id_path: Some("pci-0000:03:00.0".to_owned()),
                primary: true,
                displays: vec![display("DP-2"), display("DP-1")],
            }],
            framebuffers: vec![Framebuffer {
                node: PathBuf::from("/dev/fb2"),
                primary: false,
            }],
        };
        // a device's connector else name, card, boot card, card order, order
        type Shown<'a> = (Option<&'a str>, Option<&'a str>, bool, u32, u32);
        let devices = devices(&listing);
        let shown: Vec<Shown> = devices
            .iter()
            .map(|d| {
                (
                    d.connector.as_deref().or(d.name.as_deref()),
                    d.card.as_deref(),
                    d.on_boot_card,
                    d.card_order,
                    d.order,
                )
            })
            .collect();
        let gpu = Some("pci-0000:03:00.0");
        assert_eq!(
            shown,
            [
                (Some("DP-2"), gpu, true, 1, 0),
                (Some("DP-1"), gpu, true, 1, 1),
                (Some("fb"), None, false, 2, 0),
                (Some("Keyboard A"), None, false, 0, 0),
                (Some("A mouse"), None, false, 0, 0),
            ]
        );
    }

    /// A stand-in machine under `dir`: a card with one lit connector, a
    /// keyboard udev has not tagged, and a mouse it has.
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn stand_in_machine(dir: &Path) -> Roots {
        let sys = dir.join("sys");
        let write = |path: &str, text: &str| {
            let path = sys.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        let link = |path: &str, target: &str| {
            let path = sys.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(target, path).unwrap();
        };
        let card = "devices/platform/gpu/drm/card0";
        write(&format!("{card}/dev"), "226:0\n");
        write(&format!("{card}/card0-HDMI-A-1/status"), "connected\n");
        write(&format!("{card}/card0-HDMI-A-1/modes"), "1920x1080\n");
        link("class/drm/card0", &format!("../../{card}"));
        for (input, event, name, caps) in [
            ("input0", "event0", "Keyboard A", "fffffffe"),
            ("input1", "event1", "A mouse", "0"),
        ] {
            let dir = format!("devices/platform/{input}/input/{input}");
            link(&format!("{dir}/subsystem"), "../../../../class/input");
            write(&format!("{dir}/name"), &format!("{name}\n"));
            write(&format!("{dir}/capabilities/ev"), "3\n");
            write(&format!("{dir}/capabilities/key"), &format!("{caps}\n"));
            let minor = if event == "event0" { 64 } else { 65 };
            write(&format!("{dir}/{event}/dev"), &format!("13:{minor}\n"));
            link(
                &format!("class/input/{event}"),
                &format!("../../{dir}/{event}"),
            );
        }
        let udev = dir.join("udev");
        std::fs::create_dir_all(&udev).unwrap();
        std::fs::write(udev.join("c13:65"), "E:ID_INPUT=1\nE:ID_INPUT_MOUSE=1\n").unwrap();
        Roots {
            sys,
            udev_data: udev,
        }
    }

    // ADR-1401 D7, D9: the devices listed from sysfs and udev's database
    // give the hardware findings on standard output, outside the status
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_devices_present_give_hardware_findings_outside_the_status() {
        let (file, _) = heads_fixture(
            "devices",
            "version = 1\n[head desk]\n[head panel]\n[rules]\n\
             display * -> desk\n\
             input class:keyboard -> desk\n\
             input class:mouse -> panel\n\
             input class:joystick -> panel\n",
            0o644,
        );
        let f = file.display().to_string();
        let host = Host {
            head_file: file.with_file_name("absent.conf"),
            udev_control: file.clone(),
            roots: stand_in_machine(file.parent().unwrap()),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
        };
        let out = outcome_on(&[file.to_str().unwrap()], &host);
        assert_eq!((out.status, out.stderr), (0, Vec::new()));
        let heads: Vec<&String> = out.stdout.iter().filter(|n| n.starts_with(&f)).collect();
        assert_eq!(
            heads,
            [
                &format!(
                    "{f}: \"{f}\" belongs to user {}, not to root. The start reads this file once it is installed, owned by root and writable only by root.",
                    rustix::process::geteuid().as_raw()
                ),
                &format!("{f}:8: the rule matches no device present"),
                &format!("{f}: head desk's only keyboard is one device, \"Keyboard A\""),
                &format!(
                    "{f}: head panel claims pointers and no keyboard, and every keyboard is claimed by another head"
                ),
                &format!("{f}: {AS_WRITTEN}"),
                &format!("{f}: {UNSET}"),
            ]
        );
        std::fs::remove_dir_all(file.parent().unwrap()).ok();
    }

    // a rule noted as matching nothing here, for want of a udev daemon or
    // of a connector's PATH, takes no second note as idle
    #[test]
    fn a_rule_noted_as_matching_nothing_is_noted_once() {
        let text = "version = 1\n[head desk]\n[rules]\n\
                    display connector-path:mst:63-1 -> desk\n\
                    input path:pci-0000:15:00.0-usb-0:3.3 -> desk\n\
                    input class:joystick -> desk\n";
        let (file, trust) = heads_fixture("once", text, 0o644);
        let f = file.display().to_string();
        let machine = Machine {
            udev_daemon: false,
            devices: Some(&[]),
        };
        let out = head_file(&file, false, &trust, &machine, &caps(4));
        std::fs::remove_dir_all(file.parent().unwrap_or(&file)).ok();
        assert_eq!(
            out.notes,
            [
                format!("{f}:5: no udev daemon runs, so `path:` matches nothing"),
                format!(
                    "{f}:4: this build reads no connector's PATH, so `connector-path:` matches nothing (vtium/vtium#157)"
                ),
                format!("{f}:6: the rule matches no device present"),
                format!(
                    "{f}: No head that can be the active head has a display present, so a keyboard that roams reaches no head until one lights. Connect a display for a head that is not isolated."
                ),
                format!("{f}: {AS_WRITTEN}"),
                format!("{f}: {UNSET}"),
            ]
        );
    }

    // a newline or an escape in FILE never splits a line or reaches the
    // terminal as itself
    #[test]
    fn a_control_character_in_the_path_is_shown_escaped() {
        let path = Path::new("/tmp/vtium-check-heads-a\nb\u{1b}[2J.conf");
        assert_eq!(
            head_file(path, true, &Trust::ROOT, &machine(true), &caps(4)).counted,
            ["/tmp/vtium-check-heads-a\\nb\\u{1b}[2J.conf: does not exist"]
        );
    }

    #[test]
    fn help_says_it_only_checks() {
        let (lines, status) = run_with(&["--help"], "");
        assert_eq!(status, 0);
        assert_eq!(lines, [HELP.trim_end()]);
        assert!(HELP.contains("It only checks"));
    }
}
