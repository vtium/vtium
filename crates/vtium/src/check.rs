//! `vtium check-config`: the configuration read as vtium would read it at
//! start, with every warning, and nothing applied. It opens no VT,
//! display, input device or tty, and reads only the configuration files,
//! so it runs unprivileged (the operator's decision on vtium/vtium#21).
//!
//! The head file goes through vtium-config's reader, parser and
//! evaluator, the ones the start runs (ADR-1401 D7): the file's own
//! errors and warnings go to standard error and make the status 1, and
//! what depends on the machine it is checked on, or is a choice rather
//! than a fault, is noted on standard output outside the status, so a
//! file can be checked away from its machine.

use std::path::{Path, PathBuf};

use vtium_config::{
    Config as Heads, Identity, Lockout, Note, Parsed, Problem, Rejected, Trust, at,
};

use crate::config::{Config, Loaded, keys};
use crate::font;

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
pub const HELP: &str = "Usage: vtium check-config [OPTION]... [FILE]\n\n\
Reads the configuration as vtium would start with these options:\n\
kmscon.conf and the seat's file in the configuration directory, then the\n\
options, and the head file, /etc/vtium/heads.conf, or FILE in its place\n\
to check it before it is installed. It prints each warning, and each\n\
of the head file's errors and warnings, and exits 1 when there is any,\n\
0 when there is none. It notes on standard output what it does not\n\
count: the density font sizes in points convert at, which the start\n\
reads from each display, and of the head file a guard turned off, a\n\
weak match, and a rule that needs a udev daemon where none runs.\n\
It only checks: it opens no VT, display or input device, applies\n\
nothing, and reads nothing but the configuration files and the frit\n\
font-name names.\n";

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
        Err(why) => vec![format!(
            "the frit {} cannot be read ({why}); vtium draws its built-in font",
            wanted.name
        )],
    }
}

/// Whether PID 1 catches `signal`, from the text of `/proc/1/status`:
/// the `SigCgt` line is a hex mask with bit `signal - 1` set for a caught
/// signal. `Err` names why the text does not answer.
fn init_catches(status: &str, signal: i32) -> Result<bool, String> {
    let mask = status
        .lines()
        .find_map(|line| line.strip_prefix("SigCgt:"))
        .ok_or("it has no SigCgt line")?;
    let mask = u64::from_str_radix(mask.trim(), 16)
        .map_err(|_| format!("its SigCgt line is not a hex mask: {:?}", mask.trim()))?;
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
        out.push("hwaccel is not built yet; vtium draws in software".to_owned());
    }
    if config.switch(&keys::USE_ORIGINAL_MODE) && !config.modes(&keys::MODE).is_empty() {
        out.push("mode is set, so use-original-mode is off".to_owned());
    }
    if !config.chords(&keys::GRAB_POWER_OFF).is_empty() {
        let caught = match status {
            Ok(text) => init_catches(text, signal).map_err(|why| format!("/proc/1/status: {why}")),
            Err(why) => Err(format!("/proc/1/status could not be read: {why}")),
        };
        match caught {
            Ok(true) => {}
            Ok(false) => out.push(format!(
                "grab-power-off is bound, and PID 1 does not catch SIGRTMIN+4 (signal {signal}, \
                 from SigCgt in /proc/1/status): it powers nothing off"
            )),
            Err(why) => out.push(format!(
                "grab-power-off is bound, and whether PID 1 catches SIGRTMIN+4 (signal {signal}) \
                 is unknown, since {why}"
            )),
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
        Some(dpi) => format!(
            "display-dpi={dpi}: every display converts points at {dpi} DPI, its measured density aside"
        ),
        None => "each display converts points at the density it measures from its physical size, else at 96 DPI with a warning naming it; the start logs which".to_owned(),
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
/// file, and the udev daemon's control socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub head_file: PathBuf,
    pub udev_control: PathBuf,
}

impl Host {
    /// The machine's own, which the binary takes and nothing overrides.
    pub fn system() -> Host {
        Host {
            head_file: PathBuf::from(vtium_config::HEAD_FILE),
            udev_control: PathBuf::from(UDEV_CONTROL),
        }
    }
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
    let udev = udev_daemon_runs(&host.udev_control);
    let heads = match loaded.candidate.as_deref() {
        Some(path) => head_file(path, true, &Trust::ROOT, udev),
        None => head_file(&host.head_file, false, &Trust::ROOT, udev),
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

/// Checks the head file at `path`, the installed one or, with
/// `candidate`, one given to check before it is installed, its bytes read
/// under `trust`, on a machine where a udev daemon runs or not.
pub fn head_file(path: &Path, candidate: bool, trust: &Trust, udev_daemon: bool) -> HeadCheck {
    let mut out = HeadCheck::default();
    let inspected = match vtium_config::inspect(path, trust) {
        Ok(Some(inspected)) => inspected,
        Ok(None) if candidate => {
            out.counted
                .push(at(path, None, "does not exist").to_string());
            return out;
        }
        Ok(None) => {
            out.notes
                .push(at(path, None, "absent, so the start runs the shipped rules").to_string());
            return out;
        }
        Err(rejected) if denied(&rejected) => {
            out.counted.push(
                at(
                    path,
                    None,
                    "permission denied: the head file can be checked only as root",
                )
                .to_string(),
            );
            return out;
        }
        Err(rejected) => {
            reject(&mut out, path, &rejected);
            return out;
        }
    };
    if let Err(why) = inspected.trusted {
        let problem = Problem::Untrusted(why);
        if candidate && !CANDIDATE_OWNERSHIP_COUNTS {
            out.notes.push(
                at(
                    path,
                    None,
                    format_args!(
                        "{problem}; the start reads it only once it is installed as root's"
                    ),
                )
                .to_string(),
            );
        } else {
            let mut rejected = Rejected::default();
            rejected.errors.push(vtium_config::Error {
                line: None,
                problem,
            });
            reject(&mut out, path, &rejected);
            return out;
        }
    }
    let parsed = inspected.text.and_then(|text| vtium_config::parse(&text));
    match parsed {
        Ok(parsed) => findings(&mut out, path, &parsed, udev_daemon),
        Err(rejected) => reject(&mut out, path, &rejected),
    }
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

/// A file rejected whole: each error at its line, counted.
fn reject(out: &mut HeadCheck, path: &Path, rejected: &Rejected) {
    for e in &rejected.errors {
        out.counted.push(at(path, e.line(), e).to_string());
    }
    if rejected.more > 0 {
        out.counted
            .push(at(path, None, format_args!("{} more errors", rejected.more)).to_string());
    }
    out.counted.push(
        at(
            path,
            None,
            "rejected whole, so a start reading it runs the shipped rules",
        )
        .to_string(),
    );
}

/// A valid file's warnings, counted, and its notes, with a note for each
/// rule that needs a udev daemon where none runs (ADR-1401 D4, D9).
fn findings(out: &mut HeadCheck, path: &Path, parsed: &Parsed, udev: bool) {
    for w in &parsed.warnings {
        out.counted.push(at(path, Some(w.line()), w).to_string());
    }
    if parsed.more_warnings > 0 {
        out.counted.push(
            at(
                path,
                None,
                format_args!("{} more warnings", parsed.more_warnings),
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
            .push(at(path, None, format_args!("{} more notes", parsed.more_notes)).to_string());
    }
    if !udev {
        for (line, key) in parsed.config.needs_udev() {
            out.notes.push(
                at(
                    path,
                    Some(line),
                    format_args!("no udev daemon runs, so `{key}:` matches nothing"),
                )
                .to_string(),
            );
        }
    }
    out.notes.push(
        at(
            path,
            None,
            "no device is read, so no rule is checked against the devices present",
        )
        .to_string(),
    );
}

/// A device as it reports itself, escaped: its name, else its path, else
/// its connector.
fn reported(id: &Identity) -> String {
    let shown = id
        .name
        .as_deref()
        .or(id.path.as_deref())
        .or(id.connector.as_deref())
        .unwrap_or("a device with no name");
    format!("\"{}\"", shown.escape_debug())
}

/// What depends on the devices present (ADR-1401 D7, D9): each rule no
/// device matches, and the layer 1 warnings, each naming its device as
/// reported. Each is a note outside the status, so a file can be checked
/// away from its machine.
pub fn device_findings(path: &Path, config: &Heads, devices: &[Identity]) -> Vec<String> {
    let head = |i: usize| config.heads.get(i).map_or("?", |h| h.name.as_str());
    let mut out: Vec<String> = config
        .idle_rules(devices)
        .into_iter()
        .map(|line| at(path, Some(line), "the rule matches no device present").to_string())
        .collect();
    for l in config.lockout(devices) {
        let what = match l {
            Lockout::PointersOnly { head: h } => format!(
                "head {} claims pointers and no keyboard, and every keyboard is claimed by another head",
                head(h)
            ),
            Lockout::EveryKeyboardIgnored => "every keyboard is ignored".to_owned(),
            Lockout::OneKeyboard { head: h, device } => format!(
                "head {}'s only keyboard is one device, {}",
                head(h),
                devices.get(device).map_or_else(String::new, reported)
            ),
        };
        out.push(at(path, None, what).to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

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
            "it has no SigCgt line"
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

    fn checked(name: &str, text: &str, udev: bool) -> (HeadCheck, String) {
        let (file, trust) = heads_fixture(name, text, 0o644);
        let out = head_file(&file, false, &trust, udev);
        std::fs::remove_dir_all(file.parent().unwrap_or(&file)).ok();
        (out, file.display().to_string())
    }

    // ADR-1401 D7: a warning about the file itself counts against it
    #[test]
    fn a_head_file_warning_counts_and_its_notes_do_not() {
        let (out, f) = checked(
            "warn",
            "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> focus\ninput class:keyboard -> desk\n",
            true,
        );
        assert_eq!(
            out.counted,
            [format!(
                "{f}:6: the rule on line 5 takes every device this one matches"
            )]
        );
        assert_eq!(
            out.notes,
            [format!(
                "{f}: no device is read, so no rule is checked against the devices present"
            )]
        );
    }

    // ADR-1401 D9: each guard turned off, and a head's rescue = off, is a
    // note, never an error
    #[test]
    fn each_guard_turned_off_is_a_note_outside_the_status() {
        let (out, f) = checked(
            "guards",
            "version = 1\n[head kiosk]\nrescue = off\n[rules]\ndisplay * -> kiosk\n\
             [guards]\nanti-lockout = off\nrescue-key = off\nnever-dark = off\n",
            true,
        );
        assert_eq!(out.counted, [] as [String; 0]);
        assert_eq!(
            out.notes,
            [
                format!("{f}:3: the rescue key leaves this head's keyboards"),
                format!("{f}:7: the anti-lockout guard is off"),
                format!("{f}:8: the rescue-key guard is off"),
                format!("{f}:9: the never-dark guard is off"),
                format!(
                    "{f}: no device is read, so no rule is checked against the devices present"
                ),
            ]
        );
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
        assert_eq!(
            out.notes,
            [
                weak(5),
                weak(6),
                format!(
                    "{f}: no device is read, so no rule is checked against the devices present"
                ),
            ]
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
                    input class:keyboard -> focus\n";
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

    // ADR-1401 D6, D7: the installed head file absent is the shipped
    // rules' case, a candidate absent is an error
    #[test]
    fn an_absent_head_file_is_a_note_and_an_absent_candidate_an_error() {
        let path =
            std::env::temp_dir().join(format!("vtium-check-heads-none-{}", std::process::id()));
        let trust = Trust::ROOT;
        let shown = path.display();
        assert_eq!(
            head_file(&path, false, &trust, true),
            HeadCheck {
                counted: Vec::new(),
                notes: vec![format!(
                    "{shown}: absent, so the start runs the shipped rules"
                )],
            }
        );
        assert_eq!(
            head_file(&path, true, &trust, true),
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
        let untrusted =
            format!("{f}: not trusted: \"{f}\" is writable by its group or others (mode 666)");
        let rejected = format!("{f}: rejected whole, so a start reading it runs the shipped rules");
        let installed = head_file(&file, false, &trust, true);
        assert_eq!(installed.counted, [untrusted.clone(), rejected.clone()]);
        let candidate = head_file(&file, true, &trust, true);
        assert_eq!(
            candidate.notes,
            [format!(
                "{untrusted}; the start reads it only once it is installed as root's"
            )]
        );
        assert_eq!(
            candidate.counted,
            [
                format!("{f}:5: rule names undefined head \"desc\""),
                rejected,
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
            device_findings(path, &config, &[Identity::display(), keyboard, mouse]),
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
        assert_eq!(head_file(&file, false, &trust, true), denied, "the file");
        assert_eq!(head_file(&file, true, &trust, true), denied, "a candidate");
        mode(&file, 0o644);
        mode(&dir, 0o000);
        assert_eq!(
            head_file(&file, false, &trust, true),
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
        };
        let out = outcome_on(&[], &host);
        assert_eq!(out.status, 1);
        assert_eq!(
            out.stderr,
            [
                format!(
                    "{f}: not trusted: \"{f}\" is owned by uid {}, not root",
                    rustix::process::geteuid().as_raw()
                ),
                format!("{f}: rejected whole, so a start reading it runs the shipped rules"),
            ]
        );
        let out = outcome_on(&[file.to_str().unwrap()], &host);
        assert_eq!((out.status, out.stderr), (0, Vec::new()));
        assert!(
            out.stdout.contains(&format!(
                "{f}:5: no udev daemon runs, so `path:` matches nothing"
            )),
            "{:?}",
            out.stdout
        );
        let host = Host {
            udev_control: file.clone(),
            ..host
        };
        let out = outcome_on(&[file.to_str().unwrap()], &host);
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
                .any(|n| n.ends_with("absent.conf: absent, so the start runs the shipped rules"))
        );
        std::fs::remove_dir_all(file.parent().unwrap()).ok();
    }

    // a newline or an escape in FILE never splits a line or reaches the
    // terminal as itself
    #[test]
    fn a_control_character_in_the_path_is_shown_escaped() {
        let path = Path::new("/tmp/vtium-check-heads-a\nb\u{1b}[2J.conf");
        assert_eq!(
            head_file(path, true, &Trust::ROOT, true).counted,
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
