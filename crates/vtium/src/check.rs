//! `vtium check-config`: the configuration read as vtium would read it at
//! start, with every warning, and nothing applied. It opens nothing
//! under `/dev` and writes nothing, so it runs unprivileged: the devices
//! present it reads from sysfs and udev's database (ADR-1401 D7).
//!
//! It checks one vtium.conf, the installed one or a candidate given as
//! FILE, both its parts in one run with one exit status (ADR-1403 D3).
//! The top level goes through the console's own reader, and its reports go
//! to standard error. The sections go through vtium-settings's reader,
//! parser and evaluator, the ones the start runs (ADR-1401 D7): the
//! file's own errors, each with what it drops, its warnings and a
//! configuration with no complete route go to standard error, and each
//! makes the status 1. What depends on the machine it is checked on, or
//! is a choice rather than a fault, is noted on standard output outside
//! the status, so a file can be checked away from its machine, and the
//! notes end on what a start does with the file and the policy it states.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use vtium_settings::{
    Against, AtStart, Config as Heads, Identity, Lockout, Match, Note, Parsed, Problem, Read,
    Rejected, Remark, Runs, Stating, Token, Trust, Untrusted, at,
};

use vtium_devices::{SEAT, UDEV_CONTROL, devices, udev_daemon_runs};
use vtium_platform::listing::{self, Roots};

use crate::config::{Config, Loaded, Origin, keys};
use crate::font;
use crate::helper;
use crate::notice::check_config;

/// The subcommand's name, which stands among the options.
pub const SUBCOMMAND: &str = "check-config";

/// The subcommand's option that judges the rules alone, without the
/// devices present (ADR-1403 D3).
pub const NO_DEVICES: &str = "--no-devices";

/// Whether a rule sending input to a head by `id` or `name` alone counts
/// against the file. ADR-1401's probe 4 is the operator's, and its
/// recommendation, a note outside the status, stands until the operator
/// answers otherwise; another answer changes this line.
const WEAK_MATCH_COUNTS: bool = false;

/// Whether a candidate that fails the ownership check where it sits counts
/// against it. A candidate is checked before it is installed, and
/// installing it makes it root's (ADR-1401 D12), so it is a note outside
/// the status, the team lead's answer of 2026-10-29; the installed
/// vtium.conf always counts it, as the start does.
const CANDIDATE_OWNERSHIP_COUNTS: bool = false;

/// The subcommand's help.
pub const HELP: &str = check_config::HELP;

/// What a value means beyond its own key, which the start reports too.
pub fn semantics(config: &Config) -> Vec<String> {
    // Any user may read PID 1's status; `SigCgt` says what it catches.
    let status = std::fs::read_to_string("/proc/1/status").map_err(|e| e.to_string());
    let mut out = semantics_under(config, &status, vtium_platform::power::power_off_signal());
    // read as the start reads it, for the account vtium runs as
    out.extend(frit(config, &font::frits(config), &font::trust()));
    out
}

/// What the frit `font-name` names in `frits` says of its cell, as
/// [`font::frit_warnings`] reads it, once the ownership check `trust`
/// makes passes; nothing when the name is no frit's. A frit that cannot be
/// read says nothing here: at the start the font's own read logs why, for
/// the font-name frit and every look's alike, and check-config reports it
/// with [`font_name_frit`] and [`look_frits`].
fn frit(config: &Config, frits: &Path, trust: &Trust) -> Vec<String> {
    let Some((name, path)) = named_frit(config, frits) else {
        return Vec::new();
    };
    font::frit_text(&path, trust)
        .map_or_else(|_| Vec::new(), |text| font::frit_warnings(&name, &text))
}

/// The name of the frit `font-name` names in `frits`, and its file; none
/// when the name is no frit's there.
fn named_frit(config: &Config, frits: &Path) -> Option<(String, PathBuf)> {
    let wanted = font::wanted(config, "", "").filter(|w| w.came == font::Came::Config)?;
    let path = font::frit(frits, &wanted.name)?;
    Some((wanted.name, path))
}

/// What check-config finds of a frit's file read under `trust`, the
/// ownership check of the account the check runs as (ADR-1401 D10).
#[derive(Debug, PartialEq, Eq)]
enum Judged {
    /// The account reads it, and so would a start as root.
    Read,
    /// The account's own check refuses it, or it cannot be read: why.
    Refused(String),
    /// The account reads it, but a start as root, which reads only root's
    /// frits, would refuse it: why. Only a check run as another account
    /// than root finds this.
    RootRefuses(String),
}

/// How the frit at `file` reads under `trust`, and, where `trust` is not
/// root's, under root's too, as `helper_note` judges the font helper.
fn judge(file: &Path, trust: &Trust) -> Judged {
    match font::frit_text(file, trust) {
        Err(why) => Judged::Refused(why),
        Ok(_) if *trust == Trust::ROOT => Judged::Read,
        Ok(_) => font::frit_text(file, &Trust::ROOT)
            .err()
            .map_or(Judged::Read, Judged::RootRefuses),
    }
}

/// What check-config says of the layers the frit `name` at `file` adds,
/// read from the frits' directory `frits` under `trust` as the start reads
/// them: a sentence for each
/// `layer=` line that adds nothing, such as a missing layer, a layer the
/// ownership check refuses or a layer whose `role=` is wrong. Each counts
/// against the configuration, as a look's missing frit does, since the
/// start draws the set without that layer. A frit the account cannot read
/// says nothing here: [`judge`] reports it.
///
/// References:
/// - ADR-1204 D5: `vtium check-config` reads a set's layers too.
fn frit_layers(name: &str, file: &Path, frits: &Path, trust: &Trust) -> Vec<String> {
    font::frit_text(file, trust).map_or_else(
        |_| Vec::new(),
        |text| font::with_layers(name, frits, &text, trust).1,
    )
}

/// What check-config says of the frit `font-name` names in `frits`, read
/// under `trust`: the refusal, which counts against the configuration, or
/// the refusal of a start as root, which is a note.
fn font_name_frit(
    config: &Config,
    frits: &Path,
    trust: &Trust,
) -> (Option<String>, Option<String>) {
    let Some((name, file)) = named_frit(config, frits) else {
        return (None, None);
    };
    match judge(&file, trust) {
        Judged::Read => (None, None),
        Judged::Refused(why) => (
            Some(check_config::frit_unreadable(&name, &file, &why)),
            None,
        ),
        Judged::RootRefuses(why) => (
            None,
            Some(check_config::frit_refused_as_root(&name, &file, &why)),
        ),
    }
}

/// Whether PID 1 takes `signal`, from the text of `/proc/1/status`. The
/// kernel drops a signal sent to PID 1 that PID 1 neither catches with a
/// handler nor blocks, and keeps one it blocks pending until PID 1 reads it
/// (kernel/signal.c, `sig_task_ignored` and `sig_ignored`). sysvinit catches
/// SIGRTMIN+4 with a handler; systemd blocks it and reads it from a
/// signalfd. The `SigCgt` and `SigBlk` lines are hex masks with bit
/// `signal - 1` set for a caught and for a blocked signal. `Err` is the
/// report of why the text does not answer.
fn init_takes(status: &str, signal: i32) -> Result<bool, String> {
    let bit = u32::try_from(signal).ok().and_then(|n| n.checked_sub(1));
    let has_bit = |line: &str| -> Result<bool, String> {
        let mask = status
            .lines()
            .find_map(|l| l.strip_prefix(line)?.strip_prefix(':'))
            .ok_or_else(|| check_config::power_off_no_mask(signal, line))?;
        let mask = u64::from_str_radix(mask.trim(), 16)
            .map_err(|_| check_config::power_off_bad_mask(signal, line, mask.trim()))?;
        Ok(bit
            .and_then(|b| mask.checked_shr(b))
            .is_some_and(|m| m & 1 == 1))
    };
    Ok(has_bit("SigCgt")? || has_bit("SigBlk")?)
}

/// [`semantics`] where `status` is what reading `/proc/1/status` gave (the
/// text, or the error's message) and `signal` is the power-off signal.
fn semantics_under(config: &Config, status: &Result<String, String>, signal: i32) -> Vec<String> {
    let mut out = Vec::new();
    // `hwaccel` on in a build without the GPU path draws in software
    // (ADR-1303 D9); off is what such a build does.
    if !cfg!(feature = "gpu") && config.switch(&keys::HWACCEL) {
        out.push(check_config::HWACCEL_NOT_IN_BUILD.to_owned());
    }
    // A key not built yet warns once a source sets it.
    for key in keys::NOT_BUILT {
        if config.origin(key).is_some_and(|o| *o != Origin::Default) {
            out.push(check_config::not_built(key.name));
        }
    }
    if config.switch(&keys::USE_ORIGINAL_MODE) && !config.modes(&keys::MODE).is_empty() {
        out.push(check_config::MODE_OVER_ORIGINAL.to_owned());
    }
    if !config.chords(&keys::KEY_POWER_OFF).is_empty() {
        let taken = match status {
            Ok(text) => init_takes(text, signal),
            Err(system_error) => Err(check_config::power_off_status_unreadable(
                signal,
                system_error,
            )),
        };
        match taken {
            Ok(true) => {}
            Ok(false) => out.push(check_config::power_off_not_taken(signal)),
            Err(report) => out.push(report),
        }
    }
    out
}

/// Every warning the start would give for `loaded`: the configuration's
/// reports, then [`semantics`].
pub fn warnings(loaded: &Loaded) -> Vec<String> {
    warnings_of(loaded, |_| true)
}

/// Whether the report `r` counts against the configuration in
/// `check-config`. A key binding set by its old name does not: vtium reads
/// it as its new name, and check-config notes it on standard output, so a
/// vtium.conf from an earlier release still passes (ADR-1005 D10).
fn counts(r: &crate::config::Report) -> bool {
    !matches!(r.problem, crate::config::Problem::Renamed { .. })
}

/// [`warnings`], with only the reports `keep` keeps.
fn warnings_of(loaded: &Loaded, keep: impl Fn(&crate::config::Report) -> bool) -> Vec<String> {
    let mut out: Vec<String> = loaded
        .reports
        .iter()
        .filter(|r| keep(r))
        .map(ToString::to_string)
        .collect();
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
    let me = rustix::process::geteuid().as_raw();
    let notes = std::iter::once(density)
        .chain(helper_note(&helper::program(config), me))
        .chain(font_paths_notes(config));
    #[cfg(feature = "images")]
    let notes = notes
        .chain(image_helper_note(&crate::head::images::program(config), me))
        .chain(
            config
                .text(&keys::LOGIN_IMAGE)
                .and_then(|path| login_image_note(None, Path::new(path), me)),
        );
    notes.collect()
}

/// What check-config notes of the login picture at `path`, which the top
/// level names, or the look `look` names: why vtium could not use the
/// file, in the words of the start's log line. check-config looks at the
/// file as the account it runs as, `me`, and vtium reads it as root, so a
/// file that account may not read is noted as one check-config cannot
/// check. The note stands outside the exit status, because the file
/// belongs to the machine and not to vtium.conf.
///
/// References:
/// - ADR-1100 D17, "A file vtium cannot use": check-config reports a
///   login picture's file it cannot open.
#[cfg(feature = "images")]
fn login_image_note(look: Option<&str>, path: &Path, me: u32) -> Option<String> {
    let why = crate::head::images::login_image_unusable(path)?;
    let path = crate::font::quoted(path);
    let (what, its) = match look {
        None => ("login-image".to_owned(), "the"),
        Some(look) => (format!("Look {look:?}"), "its"),
    };
    Some(
        if me != 0 && why == crate::head::images::LOGIN_IMAGE_DENIED {
            format!(
                "{what}: this account may not read {its} login picture {path}, so check-config cannot check it. vtium reads it as root."
            )
        } else {
            format!(
                "{what}: vtium cannot show {its} login picture {path}, because {why}. vtium would start the login prompt without it."
            )
        },
    )
}

/// What check-config notes of each look's login picture, on the look's
/// `login-image` line ([`login_image_note`]).
#[cfg(feature = "images")]
fn look_login_images(out: &mut HeadCheck, path: &Path, parsed: &Parsed, me: u32) {
    for look in &parsed.config.looks {
        let named = look
            .settings
            .iter()
            .find(|s| s.key.name == keys::LOGIN_IMAGE.name);
        if let Some(named) = named {
            let said = login_image_note(Some(&look.name), Path::new(&named.value), me);
            out.notes
                .extend(said.map(|said| at(path, Some(named.line), said).to_string()));
        }
    }
}

/// What check-config notes of `font-paths` (ADR-1200 D25): each folder
/// that does not exist, and each that an account other than its owner,
/// such as `nobody`, which the helper runs as before anyone logs in,
/// cannot read and search.
fn font_paths_notes(config: &Config) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    let Some(paths) = config.text(&keys::FONT_PATHS) else {
        return Vec::new();
    };
    paths
        .split(':')
        .filter_map(|dir| match std::fs::metadata(dir) {
            Err(_) => Some(check_config::font_path_missing(dir)),
            Ok(meta) if meta.permissions().mode() & 0o005 != 0o005 => {
                Some(check_config::font_path_private(dir))
            }
            Ok(_) => None,
        })
        .collect()
}

/// What check-config notes of the font helper at `program` when it runs
/// as `me` ([`refusal_note`]).
fn helper_note(program: &Path, me: u32) -> Option<String> {
    refusal_note(
        |me| helper::open_program(program, me).map(drop),
        me,
        check_config::helper_refused,
        check_config::helper_refused_as_root,
    )
}

/// What check-config notes of the image helper at `program` when it runs
/// as `me` ([`refusal_note`]), in a build with images, the only build that
/// starts it (ADR-1101 D1).
#[cfg(feature = "images")]
fn image_helper_note(program: &Path, me: u32) -> Option<String> {
    refusal_note(
        |me| vtium_platform::image_helper::open_program(program, me).map(drop),
        me,
        check_config::image_helper_refused,
        check_config::image_helper_refused_as_root,
    )
}

/// What check-config notes of a helper that `open` opens for an account:
/// `refused` of its refusal for `me`, the account the check runs as,
/// else, when `me` is not root, `as_root` of the refusal of the start as
/// root, which takes only root's file.
fn refusal_note(
    open: impl Fn(u32) -> Result<(), String>,
    me: u32,
    refused: fn(&str) -> String,
    as_root: fn(&str) -> String,
) -> Option<String> {
    match open(me) {
        Err(why) => Some(refused(&why)),
        Ok(()) if me != 0 => open(0).err().map(|why| as_root(&why)),
        Ok(()) => None,
    }
}

/// The subcommand on the configuration `loaded`, which the command line
/// named it in: the lines to print and the exit status.
pub fn run(loaded: &Loaded) -> (Vec<String>, i32) {
    if loaded.config.switch(&keys::HELP) {
        return (vec![HELP.trim_end().to_owned()], 0);
    }
    let lines = warnings_of(loaded, counts);
    let status = i32::from(!lines.is_empty());
    (lines, status)
}

/// Where the subcommand finds the machine's files: the installed
/// vtium.conf, the udev daemon's control socket, sysfs and udev's
/// database, which list the devices present, and the frits a look names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub conf_file: PathBuf,
    pub udev_control: PathBuf,
    pub roots: Roots,
    pub frits: PathBuf,
    /// The ownership check a frit there is read under: [`font::trust`],
    /// or in a test whose frits sit under a directory others can write,
    /// one that stops below it.
    pub frit_trust: Trust,
    /// The root under which systemd's unit links say which VTs vtium is
    /// enabled on: `/`, or a scratch directory in a test.
    pub systemd_root: PathBuf,
}

impl Host {
    /// The machine's own, with the vtium.conf and the frits `config`
    /// names, which the binary takes and nothing overrides.
    pub fn system(config: &Config) -> Host {
        Host {
            conf_file: crate::config::path(config),
            udev_control: PathBuf::from(UDEV_CONTROL),
            roots: Roots::system(),
            frits: font::frits(config),
            frit_trust: font::trust(),
            systemd_root: PathBuf::from("/"),
        }
    }
}

/// What vtium.conf's sections are checked against of the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Machine<'a> {
    /// A udev daemon runs ([`udev_daemon_runs`]).
    pub udev_daemon: bool,
    /// The devices present, as [`devices`] gives them; `None` checks the
    /// file alone, as away from its machine and as `--no-devices` asks.
    pub devices: Option<&'a [Identity]>,
    /// The VTs vtium is enabled on, which a head's `vt` is checked against
    /// (ADR-1401 D3, "Enter on a head's own keyboard").
    pub vts: VtsEnabled<'a>,
}

/// What the check knows of the VTs vtium is enabled on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtsEnabled<'a> {
    /// The VTs systemd's links enable vtium on, at boot or as logind's
    /// autovt, and the mode they use ([`crate::console::enabled_vts`]).
    On(&'a crate::console::Enabled),
    /// Not read: `--no-devices` checks the file alone.
    FileAlone,
    /// Not read: the machine was not started with systemd, whose links
    /// say where vtium is enabled.
    NoSystemd,
}

/// What the subcommand prints, and its exit status.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The top level's reports, and the sections' errors and warnings.
    pub stderr: Vec<String>,
    /// What the status does not count.
    pub stdout: Vec<String>,
    /// 0 when standard error holds no line,
    /// [`vtium_settings::CHECK_MISSING_FONTS`] when every line it holds
    /// is a warning on a font that a font set needs and this machine
    /// lacks, and 1 otherwise.
    pub status: u8,
}

/// The subcommand on `loaded` and the machine `host`.
pub fn outcome(loaded: &Loaded, host: &Host) -> Outcome {
    outcome_with(loaded, host, None, None)
}

/// [`outcome`], asking the glyph helper through `squeeze` which letters of
/// the locale's everyday letters each look's font set covers, and through
/// `sources` which fonts each font set lacks. Each distinct font set is
/// asked each question once, however many looks name it, and `font-name`
/// as well, since each question starts or waits on a helper.
///
/// References:
/// - ADR-1401 D7: the note on the letters a look's font set leaves to the
///   squeeze.
/// - ADR-1204 D7: check-config names each font a set lacks.
fn outcome_with(
    loaded: &Loaded,
    host: &Host,
    squeeze: Option<Squeeze<'_>>,
    sources: Option<FontSources<'_>>,
) -> Outcome {
    type Covered = Result<(Vec<u32>, Option<String>), String>;
    type Asked = Vec<((helper::FritText, Vec<u32>), Covered)>;
    let covered: RefCell<Asked> = RefCell::default();
    let cover_once = |frit: &helper::FritText, letters: &[u32]| -> Covered {
        let key = (frit.clone(), letters.to_vec());
        if let Some((_, answer)) = covered.borrow().iter().find(|(k, _)| *k == key) {
            return answer.clone();
        }
        let answer = squeeze.map_or_else(|| Err(String::new()), |sq| (sq.ask)(frit, letters));
        covered.borrow_mut().push((key, answer.clone()));
        answer
    };
    let lacked: RefCell<Vec<(helper::FritText, Result<String, String>)>> = RefCell::default();
    let sources_once = |frit: &helper::FritText| -> Result<String, String> {
        if let Some((_, answer)) = lacked.borrow().iter().find(|(k, _)| k == frit) {
            return answer.clone();
        }
        let answer = sources.map_or_else(|| Err(String::new()), |s| (s.ask)(frit));
        lacked.borrow_mut().push((frit.clone(), answer.clone()));
        answer
    };
    let squeeze = squeeze.map(|sq| Squeeze {
        locale: sq.locale,
        ask: &cover_once,
    });
    let sources = sources.map(|_| FontSources { ask: &sources_once });
    let (stderr, _) = run(loaded);
    if loaded.config.switch(&keys::HELP) {
        return Outcome {
            stderr,
            stdout: Vec::new(),
            status: 0,
        };
    }
    let devices = devices(&listing::list(&host.roots, SEAT));
    let enabled = (!loaded.no_devices)
        .then(|| crate::console::enabled_vts(&host.systemd_root))
        .flatten();
    let vts = match &enabled {
        _ if loaded.no_devices => VtsEnabled::FileAlone,
        Some(enabled) => VtsEnabled::On(enabled),
        None => VtsEnabled::NoSystemd,
    };
    let machine = Machine {
        udev_daemon: udev_daemon_runs(&host.udev_control),
        devices: (!loaded.no_devices).then_some(devices.as_slice()),
        vts,
    };
    let settings = Settings {
        squeeze,
        sources,
        ..Settings::of(&loaded.config, &host.frits, host.frit_trust.clone())
    };
    // a candidate that is the installed file is checked as the start
    // reads it, ownership and all
    let path = loaded.candidate.as_deref().unwrap_or(&host.conf_file);
    let candidate = loaded.candidate.is_some() && !same_file(path, &host.conf_file);
    let heads = conf_file(path, candidate, &Trust::ROOT, &machine, &settings);
    // a start reads none of an installed file it does not trust, so the
    // file's own top-level reports are no start's
    let from_file = |r: &crate::config::Report| matches!(r.origin, Origin::File { .. });
    let stderr: Vec<String> = if heads.refused {
        warnings_of(loaded, |r| counts(r) && !from_file(r))
    } else {
        stderr
    };
    // the old names a start reads, each noted where it does not count
    let renamed = loaded
        .reports
        .iter()
        .filter(|r| !counts(r) && !(heads.refused && from_file(r)))
        .map(ToString::to_string);
    // the font-name frit, which at the start the font's own read reports
    let (refused, root_refuses) = font_name_frit(&loaded.config, settings.frits, &settings.trust);
    let layers = named_frit(&loaded.config, settings.frits)
        .map(|(name, file)| frit_layers(&name, &file, settings.frits, &settings.trust))
        .unwrap_or_default();
    // the fonts the font-name frit needs and this machine lacks: a missing
    // font is a warning, and an older one a note
    let needs = named_frit(&loaded.config, settings.frits)
        .map(|(name, file)| font_needs(&name, &file, &settings))
        .unwrap_or_default();
    let missing_fonts = needs.warnings.len().saturating_add(heads.missing_fonts);
    // each vtlook a start would read, read as the start reads it, with
    // where vtium.conf or the command line names the top level's; a start
    // reads none of a file it does not trust, but takes the command line
    let top_vtlook = vtium_settings::vtlook::chosen(loaded.config.text(&keys::VTLOOK))
        .zip(loaded.config.origin(&keys::VTLOOK))
        .filter(|(_, origin)| !(heads.refused && matches!(origin, Origin::File { .. })))
        .map(|(name, origin)| (name, origin.to_string()));
    let vtlooks = vtlook_findings(
        top_vtlook,
        path,
        &heads.vtlooks,
        Path::new(vtium_settings::vtlook::LOOKS),
        &settings.trust,
    );
    let stderr: Vec<String> = stderr
        .into_iter()
        .chain(refused)
        .chain(layers)
        .chain(needs.warnings)
        .chain(vtlooks)
        .collect();
    let problems = stderr.len().saturating_add(heads.counted.len());
    let status = match problems {
        0 => 0,
        _ if problems == missing_fonts => vtium_settings::CHECK_MISSING_FONTS,
        _ => 1,
    };
    Outcome {
        stderr: stderr.into_iter().chain(heads.counted).collect(),
        stdout: renamed
            .chain(notes(&loaded.config))
            .chain(root_refuses)
            .chain(needs.notes)
            .chain(heads.notes)
            .collect(),
        status,
    }
}

/// The subcommand as the binary runs it on the machine's own files: it
/// prints [`outcome`] and returns the exit status. It asks the glyph
/// helper which of the locale's everyday letters each look's font set
/// covers, and which fonts each font set lacks, waiting by `clock`.
pub fn main(loaded: &Loaded, clock: &dyn Fn() -> std::time::Instant) -> u8 {
    let env: Vec<_> = std::env::vars_os().collect();
    let locale = crate::app::locale(&env);
    let helpers = Helpers {
        launcher: helper::Launcher::configured(&loaded.config).map(|l| l.with_locale(&locale)),
        clock,
        started: RefCell::default(),
    };
    let ask = |frit: &helper::FritText, letters: &[u32]| {
        helpers.ask(frit, |helper, wait| helper.coverage(letters, wait, clock))
    };
    let squeeze = Squeeze {
        locale: &locale,
        ask: &ask,
    };
    let ask_sources =
        |frit: &helper::FritText| helpers.ask(frit, |helper, wait| helper.sources(wait, clock));
    let sources = FontSources { ask: &ask_sources };
    let out = outcome_with(
        loaded,
        &Host::system(&loaded.config),
        Some(squeeze),
        Some(sources),
    );
    for line in &out.stderr {
        eprintln!("{line}");
    }
    for line in &out.stdout {
        println!("{line}");
    }
    out.status
}

/// The glyph helpers `vtium check-config` starts: one for each distinct
/// font set, started the first time a question names the set and asked
/// every later question on it, so each start, which loads every font of
/// the set, happens once. Each helper logs none of the frit's warnings,
/// since check-config names each missing font itself.
struct Helpers<'a> {
    /// What starts each helper, or why none can start.
    launcher: Result<helper::Launcher, String>,
    clock: &'a dyn Fn() -> std::time::Instant,
    started: RefCell<Vec<(helper::FritText, helper::Helper)>>,
}

impl Helpers<'_> {
    /// Asks the helper of `frit`, started if it is not yet, the
    /// `question`, which waits at most `helper::REQUEST_MS` for its answer.
    fn ask<T>(
        &self,
        frit: &helper::FritText,
        question: impl FnOnce(&mut helper::Helper, std::time::Duration) -> Result<T, String>,
    ) -> Result<T, String> {
        let launcher = self.launcher.as_ref().map_err(Clone::clone)?;
        let mut started = self.started.borrow_mut();
        if !started.iter().any(|(f, _)| f == frit) {
            let mut helper = helper::Helper::new(
                launcher.clone(),
                std::sync::Arc::new(|| {}),
                (self.clock)(),
                frit.clone(),
                None,
                crate::deferred::Shared::default(),
                0,
            );
            helper.quiet();
            started.push((frit.clone(), helper));
        }
        let (_, helper) = started
            .iter_mut()
            .find(|(f, _)| f == frit)
            .ok_or_else(|| "the font helper did not start".to_owned())?;
        question(helper, std::time::Duration::from_millis(helper::REQUEST_MS))
    }
}

/// What the check of vtium.conf's sections found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HeadCheck {
    /// The file's own errors and warnings, which make the status 1.
    pub counted: Vec<String>,
    /// How many of `counted` are warnings on a font that a look's font
    /// set needs and this machine lacks. When they are all of `counted`
    /// and the top level has no problem, the status is
    /// [`vtium_settings::CHECK_MISSING_FONTS`].
    pub missing_fonts: usize,
    /// What the status does not count.
    pub notes: Vec<String>,
    /// The installed file fails the ownership check, so a start reads
    /// none of it, its top level included.
    pub refused: bool,
    /// The vtlook each look of the file that a start runs names, other
    /// than the built-in one: the look's name, the vtlook's and the line
    /// of its `vtlook`, for check-config to read.
    pub vtlooks: Vec<(String, String, usize)>,
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

/// What vtium.conf's sections are checked against of its top level and
/// the machine's frits: the console caps (ADR-1401 D10) and the frits
/// directory a look's frit is looked up in (D7).
#[derive(Clone, Debug)]
pub struct Settings<'a> {
    /// `head-consoles-max`.
    pub head_cap: u32,
    /// `consoles-max`.
    pub total: u32,
    pub frits: &'a Path,
    /// The ownership check a frit is read under: the one the start makes
    /// for the account the check runs as ([`font::trust`]).
    pub trust: Trust,
    /// How the check asks the glyph helper which letters of the locale's
    /// script a look's font set covers, for the warning on a font set
    /// that leaves them to the squeeze (ADR-1401 D7); `None` asks nothing.
    pub squeeze: Option<Squeeze<'a>>,
    /// How the check asks the glyph helper which fonts a font set lacks,
    /// for the notes on each missing font; `None` asks nothing.
    pub sources: Option<FontSources<'a>>,
}

/// How the check asks the glyph helper of a font set for the lines of its
/// answer to a sources query ([`helper::Helper::sources`]).
#[derive(Clone, Copy)]
pub struct FontSources<'a> {
    pub ask: &'a dyn Fn(&helper::FritText) -> Result<String, String>,
}

impl std::fmt::Debug for FontSources<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontSources").finish_non_exhaustive()
    }
}

/// The question the check asks the glyph helper of a font set: which of
/// these letters it covers, and which installed font covers the rest
/// ([`helper::Helper::coverage`]).
pub type AskCoverage<'a> =
    &'a dyn Fn(&helper::FritText, &[u32]) -> Result<(Vec<u32>, Option<String>), String>;

/// The system's locale, and how the check asks the glyph helper about a
/// font set's letters.
#[derive(Clone, Copy)]
pub struct Squeeze<'a> {
    pub locale: &'a str,
    pub ask: AskCoverage<'a>,
}

impl std::fmt::Debug for Squeeze<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Squeeze")
            .field("locale", &self.locale)
            .finish_non_exhaustive()
    }
}

impl Settings<'_> {
    /// The caps `config` sets, and the frits in `frits`, read under
    /// `trust`.
    pub fn of<'a>(config: &Config, frits: &'a Path, trust: Trust) -> Settings<'a> {
        let number = |key, unset: u32| {
            config
                .number(key)
                .map_or(unset, |n| u32::try_from(n).unwrap_or(u32::MAX))
        };
        Settings {
            head_cap: number(
                &keys::HEAD_CONSOLES_MAX,
                vtium_settings::HEAD_CONSOLES_DEFAULT,
            ),
            total: number(&keys::CONSOLES_MAX, vtium_settings::CONSOLES_TOTAL_DEFAULT),
            frits,
            trust,
            squeeze: None,
            sources: None,
        }
    }
}

/// Checks the sections and the policy of the vtium.conf at `path`, the
/// installed one or, with `candidate`, one given to check before it is
/// installed, its bytes read under `trust`, against `machine` and
/// `settings`. It reads the file as the start does (ADR-1401 D7, ADR-1403
/// D3): each error, counted, with what it drops, then what survives, then
/// whether what runs wires a complete route, and last, as notes, what a
/// start does with the file and the policy it states. A candidate the
/// ownership check fails is read as the start would read it once
/// installed.
pub fn conf_file(
    path: &Path,
    candidate: bool,
    trust: &Trust,
    machine: &Machine,
    settings: &Settings,
) -> HeadCheck {
    let mut out = HeadCheck::default();
    let inspected = match vtium_settings::inspect(path, trust) {
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
            out.counted.push(at(path, None, why.refusal()).to_string());
            out.notes
                .push(at(path, None, AtStart(Read::Untrusted)).to_string());
            out.refused = true;
            return out;
        }
    }
    let Some(reading) = vtium_settings::sections(&inspected.bytes) else {
        // a kmscon.conf: its keys apply, under the compiled-in flow's
        // open policy, which the file does not state
        let unstated = vtium_settings::Trusted {
            stated: vtium_settings::Stated::Unset,
            rest: vtium_settings::Rest::Valid,
        };
        out.notes
            .push(at(path, None, check_config::NO_FLOW).to_string());
        out.notes
            .push(at(path, None, Stating(unstated)).to_string());
        return out;
    };
    errors(&mut out, path, &reading.errors);
    let trusted = reading.trusted();
    let runs = vtium_settings::resolve(&Read::Trusted(trusted)).runs;
    findings(&mut out, path, &reading.parsed, machine, settings, runs);
    if runs_file(runs) {
        let route = vtium_settings::complete_route(&reading.parsed.config, machine.devices);
        if let Some(missing) = &route.missing {
            out.counted.push(at(path, None, missing).to_string());
        }
        if route.judged != vtium_settings::Judged::Devices {
            out.notes.push(at(path, None, route.judged).to_string());
        }
    }
    out.notes
        .push(at(path, None, AtStart(Read::Trusted(trusted))).to_string());
    out.notes.push(at(path, None, Stating(trusted)).to_string());
    out
}

/// Whether a start that `runs` runs the file's own flow, or what survives
/// of it, rather than the shipped default or nothing.
fn runs_file(runs: Runs) -> bool {
    match runs {
        Runs::File | Runs::NoHead | Runs::Survivors => true,
        Runs::Builtin | Runs::Off => false,
    }
}

/// Whether `rejected` is the open's refusal for want of permission: a
/// head file root keeps from other users, which only root can check.
fn denied(rejected: &Rejected) -> bool {
    matches!(
        rejected.errors.as_slice(),
        [vtium_settings::Error {
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

/// Each look's frit in `settings`' directory as check-config judges it
/// (ADR-1401 D10, ADR-1403 D5), on the look's `font-name` line: one the
/// ownership check of the account the check runs as refuses, or that
/// cannot be read, counts against the file, since the start draws that
/// look in its built-in font; one a start as root would refuse though the
/// account reads it is a note. A name no frit bears there is
/// `vtium_settings::warnings`' to report.
fn look_frits(out: &mut HeadCheck, path: &Path, parsed: &Parsed, settings: &Settings) {
    for look in &parsed.config.looks {
        let Some(named) = look
            .settings
            .iter()
            .find(|s| s.key.name == keys::FONT_NAME.name)
        else {
            continue;
        };
        let Some(file) = font::frit(settings.frits, &named.value) else {
            continue;
        };
        let line = Some(named.line);
        let judged = judge(&file, &settings.trust);
        // a font set the account reads is checked for its layers and for
        // the locale's letters
        if matches!(judged, Judged::Read | Judged::RootRefuses(_)) {
            for said in frit_layers(&named.value, &file, settings.frits, &settings.trust) {
                out.counted.push(at(path, line, said).to_string());
            }
            let said = settings
                .squeeze
                .and_then(|sq| squeeze_note(&look.name, &named.value, &file, settings, sq));
            out.notes
                .extend(said.map(|said| at(path, line, said).to_string()));
            let needs = font_needs(&named.value, &file, settings);
            out.missing_fonts = out.missing_fonts.saturating_add(needs.warnings.len());
            for said in needs.warnings {
                out.counted.push(at(path, line, said).to_string());
            }
            for said in needs.notes {
                out.notes.push(at(path, line, said).to_string());
            }
        }
        match judged {
            Judged::Read => {}
            Judged::Refused(why) => {
                let said =
                    check_config::look_frit_unreadable(&look.name, &named.value, &file, &why);
                out.counted.push(at(path, line, said).to_string());
            }
            Judged::RootRefuses(why) => {
                let said =
                    check_config::look_frit_refused_as_root(&look.name, &named.value, &file, &why);
                out.notes.push(at(path, line, said).to_string());
            }
        }
    }
}

/// The heads that a vtlook named at the top level are for.
const TOP_HEADS: &str = "the heads whose look names no vtlook of its own";

/// What check-config says of the vtlooks a start would read: `top`, the
/// one vtium.conf's top level or the command line names, with where it is
/// named, and those its looks name in `looks`, each with its look's name
/// and its line in the vtium.conf at `conf`. Each line counts against the
/// file, since the heads that take a vtlook with an error draw the
/// built-in vtlook.
///
/// `top` is always a vtlook's name: vtium refuses a top-level `vtlook`
/// that is not one, such as a path, when it reads vtium.conf or the
/// command line, and check-config reports that refusal at its line with
/// vtium.conf's other errors. A look's name passes the same rule when
/// vtium.conf's sections are read.
///
/// A build with frame filters reads each distinct vtlook once from `dir`
/// under `trust`, as the start reads it. For each vtlook that fails, it
/// lists the vtlook's errors, each starting with the vtlook's file and
/// line, and then each place in vtium.conf that names the vtlook. It also
/// reports a file named `default.vtlook` in `dir`, which vtium never
/// reads.
///
/// References:
/// - ADR-1304 D4: vtium refuses a file named default.vtlook.
/// - ADR-1304 D5: a vtlook's name, and the top level's `vtlook`.
/// - ADR-1304 D6: check-config lists every error of the reading and checking stages, each report whole.
#[cfg(feature = "filters")]
fn vtlook_findings(
    top: Option<(&str, String)>,
    conf: &Path,
    looks: &[(String, String, usize)],
    dir: &Path,
    trust: &Trust,
) -> Vec<String> {
    use vtium_settings::vtlook::{self, DEFAULT, SUFFIX};
    let mut names: Vec<&str> = top.iter().map(|(name, _)| *name).collect();
    for (_, name, _) in looks {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    let mut out = Vec::new();
    for name in names {
        let Err(failure) = vtlook::read(name, dir, trust) else {
            continue;
        };
        out.extend(failure.reports.iter().map(|r| format!("{}: {r}", r.at())));
        if failure.more > 0 {
            let file = dir.join(format!("{name}{SUFFIX}"));
            out.push(format!(
                "{}: {}",
                file.display(),
                crate::notice::more_errors(failure.more)
            ));
        }
        let fails = |heads: &str| {
            format!("The vtlook {name} has errors, so {heads} draw the built-in vtlook instead.")
        };
        out.extend(
            top.iter()
                .filter(|(top, _)| *top == name)
                .map(|(_, at)| format!("{at}: {}", fails(TOP_HEADS))),
        );
        out.extend(
            looks
                .iter()
                .filter(|(_, named, _)| named == name)
                .map(|(look, _, line)| {
                    let heads = format!("the heads that use look {look}");
                    at(conf, Some(*line), fails(&heads)).to_string()
                }),
        );
    }
    // a file that takes the built-in vtlook's name is never read, so only
    // the check can say so
    if dir
        .join(format!("{DEFAULT}{SUFFIX}"))
        .symlink_metadata()
        .is_ok()
    {
        let refused = vtlook::read(DEFAULT, dir, trust).err();
        out.extend(
            refused
                .iter()
                .flat_map(|f| &f.reports)
                .map(|r| format!("{}: {r}", r.at())),
        );
    }
    out
}

/// [`vtlook_findings`] in a build without frame filters, which reads no
/// vtlook: for each place that names one, it says that the heads that
/// place chooses for draw the built-in vtlook.
///
/// References:
/// - ADR-1304 D10: check-config warns of a vtlook in a build without `filters`, as it warns of `hwaccel` in a build without `gpu`.
#[cfg(not(feature = "filters"))]
fn vtlook_findings(
    top: Option<(&str, String)>,
    conf: &Path,
    looks: &[(String, String, usize)],
    _dir: &Path,
    _trust: &Trust,
) -> Vec<String> {
    let none = |heads: &str, name: &str| {
        format!(
            "This build of vtium has no frame filters, so {heads} draw the built-in vtlook instead of vtlook {name}."
        )
    };
    let top = top.map(|(name, at)| format!("{at}: {}", none(TOP_HEADS, name)));
    let looks = looks.iter().map(|(look, name, line)| {
        let heads = format!("the heads that use look {look}");
        at(conf, Some(*line), none(&heads, name)).to_string()
    });
    top.into_iter().chain(looks).collect()
}

/// What [`font_needs`] says of a font set's fonts: the warnings on fonts
/// this machine lacks, which count against the file with a status of
/// their own, and the notes, which do not count.
#[derive(Debug, Default)]
struct Needs {
    warnings: Vec<String>,
    notes: Vec<String>,
}

/// What check-config says of the fonts that the font set `name` at `file`
/// needs and that this machine lacks or has in another form, as the glyph
/// helper answers a sources query. A font that is not installed, and an
/// installed font of that name that lacks a character its `check=` names,
/// is a warning, because vtium draws the set without it. An older version
/// than `version=` asks for, and a file whose SHA-256 is not its
/// `sha256=`, are notes, because vtium still uses the font. Each line
/// names the font set, the font and the version the set asks for, then the
/// font's page where the set gives `url=`, then the help page on
/// installing fonts. A font this machine has and the helper refused to
/// read is a note that says why. A source that names the system's font
/// rather than a font of its own gets no line. When the
/// helper gives no answer, as when it cannot start, stalls or cannot read
/// the font set, one note says so and why, so the check never passes in
/// silence. Nothing when the check asks the helper nothing, or when the
/// font set's file is refused, which the check reports on its own.
///
/// References:
/// - ADR-1204 D7: `vtium check-config` names each missing font with its
///   version and page, as `vtium config fonts` does.
fn font_needs(name: &str, file: &Path, settings: &Settings) -> Needs {
    use check_config::font_needs;
    let Some(ask) = settings.sources else {
        return Needs::default();
    };
    // the layers' sentences are frit_layers' to report, so none is logged
    let Ok((frit, _)) = helper::FritText::read_noting(file, settings.frits, &settings.trust) else {
        return Needs::default();
    };
    let answer = match (ask.ask)(&frit) {
        Ok(answer) => answer,
        Err(why) => {
            return Needs {
                warnings: Vec::new(),
                notes: vec![font_needs::not_checked(name, &why)],
            };
        }
    };
    let lines: Vec<[&str; 4]> = answer
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(4, '\t');
            Some([f.next()?, f.next()?, f.next()?, f.next()?])
        })
        .collect();
    let mut out = Needs::default();
    for &[kind, line, spelling, value] in &lines {
        let Ok(line) = line.parse::<usize>() else {
            continue;
        };
        let Some((font, named)) = font_of(spelling) else {
            continue;
        };
        // a font this machine has and cannot read is no font to install:
        // a note that says why, with no page
        if kind == "refused" {
            out.notes.push(font_needs::refused(name, font, value));
            continue;
        }
        let (version, url) = needs_of(&frit.text, line, spelling);
        let version = version.as_deref();
        let (said, missing) = match kind {
            "missing" => {
                let lacks = lines.iter().find_map(|&[k, l, s, v]| {
                    (k == "lacks" && l.parse() == Ok(line) && s == spelling).then_some(v)
                });
                let said = match (version, lacks) {
                    (None, None) => font_needs::missing(name, font, named),
                    (Some(v), None) => font_needs::missing_version(name, font, v, named),
                    (None, Some(c)) => font_needs::lacks(name, font, c, named),
                    (Some(v), Some(c)) => font_needs::lacks_version(name, font, v, c, named),
                };
                (said, true)
            }
            "older" => match version {
                Some(v) => (font_needs::older(name, font, v, value), false),
                None => continue,
            },
            "sha256" => (font_needs::other_file(name, font), false),
            _ => continue,
        };
        let page = url.as_deref().map(font_needs::page);
        let mut note = said;
        for more in page.into_iter().chain([font_needs::how_to_install()]) {
            note.push(' ');
            note.push_str(&more);
        }
        if missing {
            out.warnings.push(note);
        } else {
            out.notes.push(note);
        }
    }
    out
}

/// The font a source's or a strike's `spelling` names, as a person knows
/// it: a family's or a kbd font's name, or a file's path, and which of the
/// two it is. `None` for `vconsole`, `console:` and `builtin:`, which name
/// no font a person installs.
fn font_of(spelling: &str) -> Option<(&str, check_config::font_needs::Named)> {
    let (kind, rest) = spelling.split_once(':')?;
    match kind {
        "font" | "kbd" => Some((rest, check_config::font_needs::Named::ByName)),
        "file" | "bdf" | "hex" | "outline" => Some((rest, check_config::font_needs::Named::ByPath)),
        _ => None,
    }
}

/// The `version=` and `url=` lines of the font that `spelling` names on
/// the font set's own line `line`, in `text`, the font set with its layers
/// in place. A layer's source names the font set's `layer=` line, as the
/// helper's answer does, so its spelling tells it from the layer's other
/// sources.
///
/// `spelling` is a `source=` line's or a `strike=` line's. A strike is a
/// file of its source's font at one more height, so a strike takes the
/// `version=` and `url=` of the source it follows. Those lines describe
/// the source wherever they stand among its lines, before a `strike=` line
/// or after one, and only a `sha256=` line belongs to the strike above it.
///
/// References:
/// - ADR-1204 D2: the `strike=` key, and which lines after it belong to the strike.
/// - ADR-1204 D7: check-config names a missing font's version and page.
fn needs_of(text: &str, line: usize, spelling: &str) -> (Option<String>, Option<String>) {
    let own = vtium_font::set::layer::lines(text);
    // the version= and url= of the source being read, and whether that
    // source or one of its strikes is the font asked about
    let mut found = (None, None);
    let mut named = false;
    for (body, at) in text.split('\n').zip(own) {
        let at = match at {
            vtium_font::set::layer::Line::Set(n) => n,
            vtium_font::set::layer::Line::Layer { at, .. } => at,
        };
        let body = body.split('#').next().unwrap_or_default();
        let Some((key, value)) = body.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let this = at == line && value == spelling;
        match key {
            "source" | "layer" if named => break,
            "source" | "layer" => {
                found = (None, None);
                named = key == "source" && this;
            }
            "strike" => named |= this,
            "version" => found.0 = Some(value.to_owned()),
            "url" => found.1 = Some(value.to_owned()),
            _ => {}
        }
    }
    if named { found } else { (None, None) }
}

/// The note on a look, `look`, whose font set `name` at `file` leaves
/// everyday letters of the system locale's language, CLDR's main exemplar
/// characters, to the squeeze (ADR-1401 D7, ADR-1302 D14): the letters the
/// built-in fonts draw squeezed in the font set's cell, of those the font
/// set does not cover, as the glyph helper answers. The note names the
/// script they are written in, and an installed font that covers them
/// where the helper finds one. `None` when the locale's language is not in
/// the table, when the built-in fonts squeeze none of its letters, when
/// the font set covers every such letter, or when the helper cannot
/// answer.
fn squeeze_note(
    look: &str,
    name: &str,
    file: &Path,
    settings: &Settings,
    sq: Squeeze<'_>,
) -> Option<String> {
    let script = vtium_font::letters::script_of(sq.locale)?;
    // the layers' sentences are frit_layers' to report, so none is logged
    let (frit, _) = helper::FritText::read_noting(file, settings.frits, &settings.trust).ok()?;
    let cell = font::frit_cell(&frit.text)
        .ok()
        .flatten()
        .unwrap_or(vtium_font::Cell {
            width: 8,
            height: 16,
        });
    let squeezed: Vec<u32> = vtium_font::letters::of_locale(sq.locale)
        .into_iter()
        .filter(|&c| font::builtin_squeezes(c, cell, sq.locale))
        .map(u32::from)
        .collect();
    if squeezed.is_empty() {
        return None;
    }
    let (covered, installed) = (sq.ask)(&frit, &squeezed).ok()?;
    let left = squeezed.iter().filter(|c| !covered.contains(c)).count();
    (left > 0)
        .then(|| check_config::squeezed_script(look, name, (script, left), installed.as_deref()))
}

/// What survives of the file, which a start `runs`: its warnings, counted,
/// with those measured against `settings`, a head's `consoles` past a cap
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
    settings: &Settings,
    runs: Runs,
) {
    let has_frit = |name: &str| font::frit(settings.frits, name).is_some();
    let against = Against {
        head_cap: settings.head_cap,
        total: settings.total,
        frits: settings.frits,
        has_frit: &has_frit,
    };
    for w in &vtium_settings::warnings(parsed, &against) {
        out.counted.push(at(path, Some(w.line()), w).to_string());
    }
    look_frits(out, path, parsed, settings);
    if runs_file(runs) {
        out.vtlooks = parsed
            .config
            .looks
            .iter()
            .filter_map(|l| {
                let name = vtium_settings::vtlook::chosen(l.vtlook.as_deref())?;
                Some((l.name.clone(), name.to_owned(), l.vtlook_line))
            })
            .collect();
    }
    #[cfg(feature = "images")]
    look_login_images(out, path, parsed, rustix::process::geteuid().as_raw());
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
    let runs_file = runs_file(runs);
    let remarks = if runs_file {
        vtium_settings::remarks(&parsed.config, machine.devices)
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
    // each rule noted here as matching nothing, or as unchecked, takes no
    // second note
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
            .push(at(path, Some(line), check_config::unchecked_connector_path()).to_string());
    }
    if let Some(devices) = machine.devices.filter(|_| runs_file) {
        out.notes
            .extend(device_findings(path, &parsed.config, devices, &noted));
    }
    if runs_file {
        out.notes
            .extend(vt_findings(path, &parsed.config, machine.vts));
    }
    out.notes.extend(
        remarks
            .iter()
            .map(|(line, r)| at(path, *line, r).to_string()),
    );
}

/// Each head whose `vt` names only VTs on which vtium is not enabled, as
/// `vts` says, noted at its line with the command that enables one: no
/// vtium runs that head (ADR-1401 D3, "Enter on a head's own keyboard").
/// Where `vts` was not read, one note says the check did not run, when a
/// head names a VT.
fn vt_findings(path: &Path, config: &Heads, vts: VtsEnabled) -> Vec<String> {
    let named: Vec<&vtium_settings::Head> =
        config.heads.iter().filter(|h| !h.vts.is_empty()).collect();
    let enabled = match vts {
        VtsEnabled::On(enabled) => enabled,
        VtsEnabled::FileAlone if !named.is_empty() => {
            return vec![at(path, None, check_config::VTS_UNCHECKED_FILE_ALONE).to_string()];
        }
        VtsEnabled::NoSystemd if !named.is_empty() => {
            return vec![at(path, None, check_config::VTS_UNCHECKED_NO_SYSTEMD).to_string()];
        }
        VtsEnabled::FileAlone | VtsEnabled::NoSystemd => return Vec::new(),
    };
    named
        .into_iter()
        .filter(|h| !h.vts.iter().any(|n| enabled.vts.contains(n)))
        .map(|h| {
            let line = (h.line > 0).then_some(h.line);
            let note = check_config::vtium_not_enabled(&h.name, &h.vts, enabled.mode);
            at(path, line, note).to_string()
        })
        .collect()
}

/// The lines of the rules using `connector-path:`. vtium reads each
/// display's connector `PATH` from its card when it finds the display, at
/// start and on a hotplug, but the listing opens no card and sysfs shows
/// no `PATH`, so no listed display carries one and check-config cannot
/// check these rules against the displays present.
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
    use crate::scratch::Scratch;

    /// What [`notes`] says for `args`, less the notes that begin with
    /// `left_out`, which other tests judge.
    fn notes_but(left_out: &[&str], args: &[&str]) -> Vec<String> {
        let args: Vec<OsString> = [SUBCOMMAND]
            .iter()
            .chain(args)
            .map(OsString::from)
            .collect();
        notes(&config::load(&args, &mut |_: &Path| Ok(None)).config)
            .into_iter()
            .filter(|n| !left_out.iter().any(|l| n.starts_with(l)))
            .collect()
    }

    /// `/bin/sh`, a program root owns, for the assertions that root's own
    /// helper draws no note. `make arm-test` runs the tests in a user
    /// namespace that does not map root (tools/arm/README.md), where every
    /// file of root's belongs to the overflow uid, 65534. No file there is
    /// root's, so those assertions cannot mean anything, and they skip and
    /// say so. Anywhere else, a `/bin/sh` that root does not own fails.
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn roots_program() -> Option<&'static Path> {
        let sh = Path::new("/bin/sh");
        let owner = std::fs::metadata(sh)
            .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
            .unwrap();
        if owner == 0 {
            return Some(sh);
        }
        let qemu = std::env::var_os("VTIUM_QEMU_USER");
        assert!(
            qemu.is_some(),
            "/bin/sh belongs to uid {owner}, and these tests need a program root owns"
        );
        eprintln!(
            "under {}, /bin/sh belongs to uid {owner}, so no file is root's: the assertions about root's own helper are skipped",
            qemu.unwrap_or_default().display()
        );
        None
    }

    fn run_with(args: &[&str], conf: &str) -> (Vec<String>, i32) {
        let args: Vec<OsString> = [SUBCOMMAND]
            .iter()
            .chain(args)
            .map(OsString::from)
            .collect();
        let conf = conf.as_bytes().to_vec();
        let loaded = config::load(&args, &mut |p: &Path| {
            Ok((p == Path::new("/etc/vtium/vtium.conf")).then(|| conf.clone()))
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
                "/etc/vtium/vtium.conf:1: sb-size=\"200000\" is out of range, so 100000 is used",
                "/etc/vtium/vtium.conf:2: unknown key \"frob\", ignored",
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
                "argument 2: palette=\"frob\" is not a value this setting takes. It takes legacy, \
                 custom, nord, solarized, solarized-black, solarized-white, soft-black, \
                 base16-dark, base16-light, or vga. Because of this error, vtium ignores this \
                 setting."
            ]
        );
        for name in vtium_term::PALETTE_NAMES.iter().chain(&["custom"]) {
            let conf = format!("palette={name}\n");
            assert_eq!(run_with(&[], &conf), (Vec::new(), 0), "{name}");
        }
    }

    #[test]
    fn hwaccel_warns_only_in_a_build_without_the_gpu_path() {
        let (lines, status) = run_with(&[], "hwaccel\n");
        if cfg!(feature = "gpu") {
            assert_eq!((lines, status), (Vec::<String>::new(), 0));
        } else {
            assert_eq!(status, 1);
            assert_eq!(
                lines,
                ["hwaccel needs a build with GPU support, so vtium draws in software"]
            );
        }
        assert_eq!(run_with(&[], "no-hwaccel\n"), (Vec::new(), 0));
    }

    #[test]
    fn a_key_not_built_yet_warns() {
        // Every other key not built yet warns once a source sets it, to
        // any value it takes, in the file or on the command line.
        // `font-line-spacing` is built (ADR-1302 D13), and says nothing.
        let (lines, status) = run_with(
            &["--font-line-spacing=20"],
            "display-buffers=3\nfont-hinting=light\n",
        );
        assert_eq!(status, 1);
        assert_eq!(
            lines,
            ["font-hinting has no effect yet, so vtium ignores it"]
        );
        assert_eq!(run_with(&["--font-line-spacing=20"], ""), (Vec::new(), 0));
        assert_eq!(run_with(&[], "font-line-spacing=100\n"), (Vec::new(), 0));
        assert_eq!(
            run_with(&["--font-hinting=full"], ""),
            (
                vec!["font-hinting has no effect yet, so vtium ignores it".to_owned()],
                1
            )
        );
        // A value vtium cannot take leaves the key unset: its report
        // alone.
        let (lines, _) = run_with(&[], "display-buffers=4\n");
        assert_eq!(
            lines,
            [
                "/etc/vtium/vtium.conf:1: display-buffers=\"4\" is not a value this setting takes. It takes 1, 2, or 3. Because of this error, vtium ignores this setting."
            ]
        );
        // ADR-1401 D15: kernel-messages takes its two values alone.
        assert_eq!(run_with(&[], "kernel-messages=always\n"), (Vec::new(), 0));
        assert_eq!(
            run_with(&[], "kernel-messages=sometimes\n"),
            (
                vec![
                    "/etc/vtium/vtium.conf:1: kernel-messages=\"sometimes\" is not a value this setting takes. It takes after-plymouth or always. Because of this error, vtium ignores this setting.".to_owned()
                ],
                1
            )
        );
    }

    /// The text of `/proc/1/status` for a PID 1 whose caught signals are
    /// `cgt` and whose blocked signals are `blk`.
    fn status(cgt: &str, blk: &str) -> Result<String, String> {
        Ok(format!(
            "Name:\tinit\nSigBlk:\t{blk}\nSigIgn:\t0000000000001000\nSigCgt:\t{cgt}\nCapInh:\t0\n"
        ))
    }

    fn power_off_warnings(conf: &str, status: &Result<String, String>) -> Vec<String> {
        let args = [OsString::from(SUBCOMMAND)];
        let conf = conf.as_bytes().to_vec();
        let loaded = config::load(&args, &mut |_: &Path| Ok(Some(conf.clone())));
        semantics_under(&loaded.config, status, 38)
    }

    const BOUND: &str = "key-power-off=<Ctrl><Alt>End\n";

    #[test]
    fn key_power_off_is_silent_where_init_takes_the_signal() {
        // systemd blocks SIGRTMIN+4 and reads it from a signalfd, so bit 37
        // (signal 38) is set in its SigBlk and not in its SigCgt; these are
        // the masks of systemd 262 as PID 1. sysvinit catches the signal
        // with a handler, so the bit is set in its SigCgt.
        for (cgt, blk) in [
            ("00000000000004ec", "7fefc1fe28014a03"),
            ("00000021fdc1fefb", "0000000000000000"),
        ] {
            assert_eq!(
                power_off_warnings(BOUND, &status(cgt, blk)),
                [] as [&str; 0],
                "SigCgt {cgt}, SigBlk {blk}"
            );
        }
    }

    #[test]
    fn key_power_off_warns_where_init_neither_catches_nor_blocks_it() {
        assert_eq!(
            power_off_warnings(BOUND, &status("0000000000000000", "0000000000000000")),
            [
                "the key-power-off key will not work: the init system (PID 1) neither \
                 handles nor blocks SIGRTMIN+4 (signal 38, from SigCgt and SigBlk in \
                 /proc/1/status). Unbind the key, or use an init system that powers off on \
                 that signal, such as systemd or sysvinit."
            ]
        );
        // Bit 38 (signal 39) is a neighbour, not the signal.
        let neighbour = "0000004000000000";
        assert_eq!(
            power_off_warnings(BOUND, &status(neighbour, neighbour)).len(),
            1
        );
    }

    #[test]
    fn key_power_off_names_why_an_unreadable_status_does_not_answer() {
        let unreadable = Err("Permission denied (os error 13)".to_owned());
        assert_eq!(
            power_off_warnings(BOUND, &unreadable),
            [
                "the key-power-off key may not work: vtium cannot tell whether the init \
                 system (PID 1) handles SIGRTMIN+4 (signal 38), because /proc/1/status could \
                 not be read: Permission denied (os error 13)"
            ]
        );
        let says = |status: Result<String, String>, what: &str| {
            let lines = power_off_warnings(BOUND, &status);
            matches!(lines.as_slice(), [line] if line.contains(what))
        };
        let no_line = Ok("Name:\tinit\n".to_owned());
        assert!(says(no_line, "because /proc/1/status has no SigCgt line"));
        // A SigCgt without the bit sends the check on to SigBlk.
        let no_blk = Ok("Name:\tinit\nSigCgt:\t0\n".to_owned());
        assert!(says(no_blk, "because /proc/1/status has no SigBlk line"));
        assert!(says(
            status("zz", "0"),
            "because the SigCgt line of /proc/1/status is not a hex mask: \"zz\""
        ));
        assert!(says(
            status("0", "zz"),
            "because the SigBlk line of /proc/1/status is not a hex mask: \"zz\""
        ));
    }

    #[test]
    fn an_unbound_key_power_off_checks_nothing() {
        let unbound = "key-reboot=<Ctrl><Alt>Delete\n";
        assert_eq!(
            power_off_warnings(unbound, &status("0", "0")),
            [] as [&str; 0]
        );
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
        let dir = Scratch::new("check-frit");
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
            config::load(&args, &mut |_: &Path| Ok(None)).config
        };
        let trust = font::trust().below(&dir);
        assert_eq!(
            frit(&config("small"), &dir, &trust),
            [
                "font set small: line 1: the characters of source=builtin:unifont (8x16) do not fit the 8x12 character cell, so vtium does not use that source"
            ]
        );
        assert!(frit(&config("fine"), &dir, &trust).is_empty());
        assert!(
            frit(&config("ter-v16n"), &dir, &trust).is_empty(),
            "no frit"
        );
    }

    // check-config reads a set's layers as the start reads them, and
    // reports a missing layer, a layer the ownership check refuses and a
    // layer whose role= is wrong.
    //
    // References:
    // - ADR-1204 D5: vtium check-config reads a set's layers too.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn check_config_reports_a_sets_layers() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("check-layers");
        let layers = dir.join("layers");
        std::fs::create_dir_all(&layers).unwrap();
        let write = |path: &Path, text: &str, mode: u32| {
            std::fs::write(path, text).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        for d in [dir.as_ref(), layers.as_path()] {
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let set = dir.join("set.vtfrit");
        write(
            &set,
            "source=builtin:unifont\nlayer=fine\nlayer=gone\nlayer=theirs\nlayer=icons\n",
            0o644,
        );
        write(
            &layers.join("fine.vtlayer"),
            "role=emoji\nsource=font:E\n",
            0o644,
        );
        write(
            &layers.join("theirs.vtlayer"),
            "role=script\nsource=font:T\n",
            0o664,
        );
        write(
            &layers.join("icons.vtlayer"),
            "role=icons\nsource=font:I\n",
            0o644,
        );
        let trust = font::trust().below(&dir);
        let said = frit_layers("set", &set, &dir, &trust);
        let file = |name: &str| font::quoted(&layers.join(format!("{name}.vtlayer")));
        assert_eq!(
            said,
            [
                format!(
                    "font set set: line 3: the layer gone is missing, because there is no file {}. vtium draws the font set without that layer.",
                    file("gone")
                ),
                format!(
                    "font set set: line 4: vtium does not read the layer theirs from {}, because other users can change {} (mode 664). Run `chmod go-w {}` so that only its owner can change it. vtium draws the font set without that layer.",
                    file("theirs"),
                    file("theirs"),
                    layers.join("theirs.vtlayer").display()
                ),
                format!(
                    "font set set: line 5: the layer icons in {} has an error: line 1: role=icons is not a role: use script, symbols, emoji or fallback. vtium draws the font set without that layer.",
                    file("icons")
                ),
            ]
        );
        // a look naming the set counts each sentence against the file
        let text =
            "version = 1\n[look l]\nfont-name = set\n[head a]\nlook = l\n[rules]\ndisplay * -> a\n";
        let (_heads, conf, conf_trust) = heads_fixture("look-layers", text, 0o644);
        let settings = Settings {
            frits: &dir,
            trust,
            ..caps(4)
        };
        let out = conf_file(&conf, false, &conf_trust, &machine(true), &settings);
        let counted: Vec<&String> = out.counted.iter().filter(|l| l.contains("layer")).collect();
        let at = |line: &String| format!("{}:3: {line}", conf.display());
        assert_eq!(
            counted,
            said.iter()
                .map(at)
                .collect::<Vec<_>>()
                .iter()
                .collect::<Vec<_>>()
        );
    }

    // ADR-1401 D10, ADR-1403 D3: check-config reads the frit font-name
    // names under the ownership check the start makes for the account it
    // runs as, and counts a frit the check refuses, with the path that
    // fails and the command that mends it. Run as another account than
    // root, it notes where a start as root refuses a frit the account
    // reads, as it notes for the font helper. The start's warnings say
    // nothing of a refusal, which the font's own read logs at the start
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_frit_the_ownership_check_refuses_is_reported() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = Scratch::new("check-trust");
        let open = dir.join("open");
        std::fs::create_dir_all(&open).unwrap();
        let mode = |path: &Path, mode: u32| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        for path in [
            dir.join("t.vtfrit"),
            dir.join("g.vtfrit"),
            open.join("t.vtfrit"),
        ] {
            std::fs::write(&path, "source=builtin:unifont\n").unwrap();
            mode(&path, 0o644);
        }
        mode(&dir, 0o755);
        mode(&dir.join("g.vtfrit"), 0o664);
        mode(&open, 0o775);
        let config = |name: &str| {
            let args: Vec<OsString> = [SUBCOMMAND, "--font-name", name]
                .iter()
                .map(OsString::from)
                .collect();
            config::load(&args, &mut |_: &Path| Ok(None)).config
        };
        let uid = std::fs::metadata(&dir).unwrap().uid();
        let mine = font::trust().below(&dir);
        let results = [
            font_name_frit(&config("t"), &dir, &mine),
            font_name_frit(&config("g"), &dir, &mine),
            font_name_frit(&config("t"), &open, &mine),
            font_name_frit(&config("ter-v16n"), &dir, &mine),
        ];
        let at_start = [
            frit(&config("g"), &dir, &mine),
            frit(&config("t"), &open, &mine),
        ];
        let refused = |name: &str, file: &Path, failed: &Path, mode: &str| {
            let (file, failed) = (file.display(), failed.display());
            format!(
                "vtium does not read the font set {name} from \"{file}\", because other users can change \"{failed}\" (mode {mode}). Run `chmod go-w {failed}` so that only its owner can change it. vtium uses its built-in font instead."
            )
        };
        let (g, t) = (dir.join("g.vtfrit"), dir.join("t.vtfrit"));
        let as_root = format!(
            "When vtium runs as root, it does not read the font set t from \"{t}\", because \"{t}\" belongs to user {uid}, not to root. Run `chown root:root {t}` to make root the owner. vtium would then use its built-in font.",
            t = t.display()
        );
        assert_eq!(
            results,
            [
                (None, Some(as_root)),
                (Some(refused("g", &g, &g, "664")), None),
                (
                    Some(refused("t", &open.join("t.vtfrit"), &open, "775")),
                    None
                ),
                (None, None),
            ]
        );
        assert_eq!(at_start, [[] as [String; 0], []]);
    }

    // the operator on #145, 2026-10-28: the density points convert at,
    // display-dpi's, else each display's measured or assumed, which only
    // the start can read, since check-config opens no display
    #[test]
    fn the_density_source_is_noted_outside_the_status() {
        let note = |args: &[&str]| notes_but(&["font-helper:", "image-helper:"], args);
        assert_eq!(
            note(&["--display-dpi=109"]),
            [
                "display-dpi=109: every display uses 109 DPI for font sizes, whatever density it reports"
            ]
        );
        assert_eq!(
            note(&[]),
            [
                "each display uses the density it reports from its physical size for font sizes, or 96 DPI with a warning if it reports none; vtium logs which at start"
            ]
        );
        assert_eq!(run_with(&["--display-dpi=109"], ""), (Vec::new(), 0));
    }

    // a helper vtium would refuse is noted outside the status, since the
    // start may run as another account and checks it again
    #[test]
    fn a_helper_vtium_would_refuse_is_noted() {
        let note = |args: &[&str]| notes_but(&["image-helper:"], args);
        let density = check_config::MEASURED_DENSITY;
        if let Some(sh) = roots_program() {
            let helper = format!("--font-helper={}", sh.display());
            assert_eq!(note(&[&helper]), [density]);
        }
        assert_eq!(
            note(&["--font-helper=bin/vtium-config"]),
            [
                density,
                "font-helper: vtium does not start the font helper bin/vtium-config: the path is not absolute. vtium would use its built-in font. vtium checks again at start, for the account it runs as."
            ]
        );
        assert_eq!(
            note(&["--font-helper=/"]),
            [
                density,
                "font-helper: vtium does not start the font helper /: it is not a regular file. vtium would use its built-in font. vtium checks again at start, for the account it runs as."
            ]
        );
        assert_eq!(
            run_with(&["--font-helper=bin/vtium-config"], ""),
            (Vec::new(), 0)
        );
    }

    // ADR-1101 D1: image-helper defaults to the path vtium was built
    // with, the one vtium-platform starts by default too
    #[cfg(feature = "images")]
    // covers: image.helper
    #[test]
    fn image_helper_defaults_to_the_built_path() {
        let loaded = config::load(&[OsString::from(SUBCOMMAND)], &mut |_: &Path| Ok(None));
        assert_eq!(
            crate::head::images::program(&loaded.config),
            Path::new(crate::head::images::PROGRAM)
        );
        assert_eq!(
            crate::head::images::PROGRAM,
            vtium_platform::image_helper::PROGRAM
        );
        let args: Vec<OsString> = [SUBCOMMAND, "--image-helper=/opt/vtium-image"]
            .iter()
            .map(OsString::from)
            .collect();
        let loaded = config::load(&args, &mut |_: &Path| Ok(None));
        assert_eq!(
            crate::head::images::program(&loaded.config),
            Path::new("/opt/vtium-image")
        );
    }

    // ADR-1101 D1: an image helper vtium would refuse is noted outside the
    // status, as the font helper is, since the start may run as another
    // account and checks it again
    #[cfg(feature = "images")]
    // covers: image.helper
    #[test]
    fn an_image_helper_vtium_would_refuse_is_noted() {
        let note = |args: &[&str]| notes_but(&["font-helper:"], args);
        let density = check_config::MEASURED_DENSITY;
        if let Some(sh) = roots_program() {
            let helper = format!("--image-helper={}", sh.display());
            assert_eq!(note(&[&helper]), [density]);
        }
        assert_eq!(
            note(&["--image-helper=bin/vtium-image"]),
            [
                density,
                "image-helper: the image helper's path bin/vtium-image is not absolute, so vtium does not start it. vtium would refuse images sent as files or shared memory, and PNG and zlib images. vtium checks again at start, for the account it runs as."
            ]
        );
        assert_eq!(
            note(&["--image-helper=/"]),
            [
                density,
                "image-helper: the image helper / is not a regular file, so vtium does not start it. vtium would refuse images sent as files or shared memory, and PNG and zlib images. vtium checks again at start, for the account it runs as."
            ]
        );
        assert_eq!(
            run_with(&["--image-helper=bin/vtium-image"], ""),
            (Vec::new(), 0)
        );
    }

    // A login picture that vtium could not use, at the top level or in a
    // look, is noted on standard output with the reason the start would
    // log, outside the exit status; a PNG file draws no note.
    #[cfg(feature = "images")]
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_login_picture_vtium_cannot_use_is_noted() {
        let dir = Scratch::new("check-login-image");
        let png = dir.join("login.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\nrest").unwrap();
        let text = dir.join("login.txt");
        std::fs::write(&text, b"no picture").unwrap();
        let missing = dir.join("missing.png");
        let note = |path: &Path| {
            notes_but(
                &["font-helper:", "image-helper:"],
                &[&format!("--login-image={}", path.display())],
            )
        };
        let density = check_config::MEASURED_DENSITY;
        assert_eq!(note(&png), [density]);
        assert_eq!(
            note(&missing),
            [
                density.to_owned(),
                format!(
                    "login-image: vtium cannot show the login picture \"{}\", because the file does not exist. vtium would start the login prompt without it.",
                    missing.display()
                )
            ]
        );
        assert_eq!(
            note(&dir),
            [
                density.to_owned(),
                format!(
                    "login-image: vtium cannot show the login picture \"{}\", because the file is not a regular file. vtium would start the login prompt without it.",
                    dir.display()
                )
            ]
        );
        let look = format!(
            "version = 1\n[look big]\nlogin-image = {}\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n",
            text.display()
        );
        let (_heads, file, trust) = heads_fixture("login-image", &look, 0o644);
        let out = conf_file(&file, false, &trust, &machine(true), &caps(4));
        assert_eq!(out.counted, [] as [String; 0]);
        let said: Vec<&String> = out
            .notes
            .iter()
            .filter(|n| n.contains("login picture"))
            .collect();
        assert_eq!(
            said,
            [&format!(
                "{}:3: Look \"big\": vtium cannot show its login picture \"{}\", because the file is not a PNG or JPEG picture. vtium would start the login prompt without it.",
                file.display(),
                text.display()
            )]
        );
    }

    // ADR-1101 D1: check-config run as another account notes an image
    // helper the root start refuses
    #[cfg(feature = "images")]
    // covers: image.helper
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn an_image_helper_the_root_start_refuses_is_noted() {
        let dir = Scratch::new("check-image");
        let program = dir.join("vtium-image");
        std::fs::write(&program, "").unwrap();
        let owner = std::fs::metadata(&program)
            .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
            .unwrap();
        if let Some(sh) = roots_program() {
            assert_eq!(image_helper_note(sh, 1000), None);
        }
        let note = image_helper_note(&program, owner);
        if owner == 0 {
            assert_eq!(note, None, "root's file");
        } else {
            assert_eq!(
                note.unwrap(),
                format!(
                    "image-helper: when vtium runs as root, it refuses this helper: the image helper {} is owned by uid {owner}, which is neither root nor the account vtium runs as, so vtium does not start it. vtium would then refuse images sent as files or shared memory, and PNG and zlib images.",
                    program.display()
                )
            );
        }
    }

    // check-config run as another account notes a helper the root start
    // refuses
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_helper_the_root_start_refuses_is_noted() {
        let dir = Scratch::new("check-helper");
        let program = dir.join("vtium-config");
        std::fs::write(&program, "").unwrap();
        // owned by the test's account, as a build is
        let owner = std::fs::metadata(&program)
            .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
            .unwrap();
        if let Some(sh) = roots_program() {
            assert_eq!(helper_note(sh, 1000), None);
            assert_eq!(helper_note(sh, 0), None);
        }
        let note = helper_note(&program, owner);
        if owner == 0 {
            assert_eq!(note, None, "root's file");
        } else {
            assert_eq!(
                note.unwrap(),
                format!(
                    "font-helper: when vtium runs as root, it refuses this helper: vtium does not start the font helper {}: it belongs to uid {owner}, which is neither root nor vtium's own account. vtium would then use its built-in font.",
                    program.display()
                )
            );
            let other = helper_note(&program, owner.wrapping_add(1)).unwrap();
            assert!(
                other.starts_with("font-helper: vtium does not start the font helper"),
                "{other}"
            );
        }
    }

    // font-frits names the directory check-config finds a profile's frit in
    #[test]
    fn check_config_finds_the_frits_in_font_frits() {
        let args: Vec<OsString> = [SUBCOMMAND, "--font-frits=/srv/frits"]
            .iter()
            .map(OsString::from)
            .collect();
        let loaded = config::load(&args, &mut |_: &Path| Ok(None));
        assert_eq!(Host::system(&loaded.config).frits, Path::new("/srv/frits"));
        let loaded = config::load(&[OsString::from(SUBCOMMAND)], &mut |_: &Path| Ok(None));
        assert_eq!(Host::system(&loaded.config).frits, Path::new(font::FRITS));
    }

    /// A fixture directory of its own, which the test removes when it
    /// ends, a head file in it, and the trust that reads the file as the
    /// start reads root's files.
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn heads_fixture(name: &str, text: &str, mode: u32) -> (Scratch, PathBuf, Trust) {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new(&format!("check-heads-{name}"));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = dir.join("heads.conf");
        std::fs::write(&file, text).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
        let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
        (dir, file, trust)
    }

    /// vtium.conf's caps with `head_cap` and the default total, and frits
    /// nowhere.
    fn caps(head_cap: u32) -> Settings<'static> {
        Settings {
            head_cap,
            total: vtium_settings::CONSOLES_TOTAL_DEFAULT,
            frits: Path::new("/nonexistent/vtium/frits"),
            trust: font::trust(),
            squeeze: None,
            sources: None,
        }
    }

    /// A machine where a udev daemon runs or not, whose devices are not
    /// read.
    fn machine(udev_daemon: bool) -> Machine<'static> {
        Machine {
            udev_daemon,
            devices: None,
            vts: VtsEnabled::FileAlone,
        }
    }

    fn checked(name: &str, text: &str, udev: bool) -> (HeadCheck, String) {
        let (_dir, file, trust) = heads_fixture(name, text, 0o644);
        let out = conf_file(&file, false, &trust, &machine(udev), &caps(4));
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
                "{f}:6: this rule never matches: the rule on line 5 matches every device it would"
            )]
        );
        assert_eq!(verdict(&out, &f), [AS_WRITTEN, UNSET]);
        assert_eq!(
            out.notes,
            [RULES_ALONE, AS_WRITTEN, UNSET].map(|n| format!("{f}: {n}"))
        );
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

    const OPEN: &str = "The policy is open: an input device no rule places roams, a head is isolated only where it says `isolated = yes`, and the keyboard safeguards (anti-lockout and the rescue key) are on.";
    const UNSET: &str = "The file sets no `policy`, so its policy is open: an input device no rule places roams, a head is isolated only where it says `isolated = yes`, and the keyboard safeguards (anti-lockout and the rescue key) are on.";
    const CLOSED: &str = "The policy is closed: an input device no rule places is ignored, a head without an `isolated` line is isolated, and the keyboard safeguards (anti-lockout and the rescue key) are off.";
    const AS_WRITTEN: &str = "vtium starts with this file as written and shows no notice.";
    /// What a check with no devices says of how it judged the route
    /// (ADR-1403 D3).
    const RULES_ALONE: &str =
        "vtium checked the routes by the rules alone, without the connected devices.";
    const NO_DISPLAY: &str =
        "No display is connected, so vtium checked the routes by the rules alone.";
    const PARTLY_OPEN: &str = "vtium starts with the heads and rules that have no errors, then the built-in rules. Every head acts as not isolated, whatever its `isolated` says. It shows `vtium.conf has errors: running the parts without errors.`.";
    const PARTLY_UNSET_POLICY: &str = "The file sets no `policy`, so its policy is open. Because parts of the file have errors, every head acts as not isolated, whatever its `isolated` says; an input device that no working rule places goes on to the built-in rules; and the keyboard safeguards (anti-lockout and the rescue key) are on.";
    const PARTLY_OPEN_POLICY: &str = "The policy is open. Because parts of the file have errors, every head acts as not isolated, whatever its `isolated` says; an input device that no working rule places goes on to the built-in rules; and the keyboard safeguards (anti-lockout and the rescue key) are on.";

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
                    "This section has no key named \"frob\". Check the spelling, or remove the line."
                ),
                at(
                    3,
                    "Because of this error, vtium ignores this head and every rule that names it."
                ),
                at(
                    5,
                    "The file already has a head named \"kiosk\". Rename one of the two."
                ),
                at(
                    5,
                    "Because of this error, vtium ignores this whole section."
                ),
                at(
                    9,
                    "This rule sends devices to head \"desc\", but the file has no head of that name. Correct the name, or add a `[head NAME]` section for it."
                ),
                at(
                    9,
                    "Because of this error, vtium ignores this rule, and each device goes on to the next rule."
                ),
                // the survivors' own warning: line 11 went with head desk
                at(
                    12,
                    "this rule never matches: the rule on line 10 matches every device it would"
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
                "vtium starts with this file as written and shows no notice.",
                OPEN,
            ),
            (
                format!("policy = closed\nversion = 1\n{head}"),
                "vtium starts with this file as written and shows no notice.",
                CLOSED,
            ),
            ("version = 1\n".to_owned(), check_config::NO_FLOW, UNSET),
            (
                "version = 1\n[rules]\ninput * -> roaming\n".to_owned(),
                "This file has rules but no head, so vtium starts with its rules. Each display that no rule turns off shows a notice that it is not configured for any terminal.",
                UNSET,
            ),
            (
                "version = 1\npolicy = closed\n".to_owned(),
                "This file has no head and its policy is closed, so vtium starts with every display off, takes no input and shows nothing.",
                CLOSED,
            ),
            (
                format!("version = 1\n{head}display * -> b\n"),
                PARTLY_OPEN,
                PARTLY_UNSET_POLICY,
            ),
            (
                format!("version = 1\npolicy = closed\n{head}display * -> b\n"),
                "vtium starts with the heads and rules that have no errors, and the rescue key off. Each head keeps its `isolated` setting, and a head without one is isolated. It shows `vtium.conf has errors: running the parts without errors.`.",
                CLOSED,
            ),
            (
                format!("version = 2\n{head}"),
                "Every head in this file has errors, so vtium starts with its built-in configuration and shows `vtium.conf not used: running the built-in configuration.`.",
                UNSET,
            ),
            (
                format!("version = 2\npolicy = closed\n{head}"),
                "Every head in this file has errors and the policy is closed, so vtium takes no input and shows an error screen on the boot display, starting `vtium.conf has errors and counts as closed: all input is off.`.",
                CLOSED,
            ),
            (
                format!("version = 1\npolicy = maybe\n{head}"),
                "vtium cannot read a `policy` line of this file, so at start it treats the whole file as broken with the closed policy: it takes no input and shows an error screen on the boot display, starting `vtium.conf has errors and counts as closed: all input is off.`.",
                "vtium cannot read the `policy` line above the first section, so the file's policy is closed. Write that line as `policy = open` or `policy = closed`.",
            ),
        ];
        for (n, (text, outcome, policy)) in rows.iter().enumerate() {
            let (out, f) = checked(&format!("row-{n}"), text, true);
            assert_eq!(verdict(&out, &f), [*outcome, *policy], "{text}");
            // an error counts against the file, and so does a flow that
            // wires no complete route, judged here by its rules alone
            // (ADR-1403 D3): only the open file whose head has a display
            // rule, and the file with no flow, count nothing
            let clean =
                *text == format!("version = 1\npolicy = open\n{head}") || text == "version = 1\n";
            assert_eq!(out.counted.is_empty(), clean, "{text}: {:?}", out.counted);
        }
    }

    // ADR-1401 D7: a line that is not UTF-8 drops only its rule, and the
    // file runs what survives
    // covers: platform.heads.rules
    #[test]
    fn a_line_that_is_not_utf8_drops_only_its_rule() {
        let (_dir, file, trust) = heads_fixture("bytes", "", 0o644);
        std::fs::write(
            &file,
            b"version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput name:\xff -> a\n",
        )
        .ok();
        let f = file.display().to_string();
        let out = conf_file(&file, false, &trust, &machine(true), &caps(4));
        assert_eq!(
            out.counted,
            [
                format!("{f}:5: This line is not UTF-8 text. Save the file as UTF-8."),
                format!(
                    "{f}:5: Because of this error, vtium ignores this rule, and each device goes on to the next rule."
                ),
            ]
        );
        assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_UNSET_POLICY]);
    }

    // ADR-1401 D10: a head's `consoles` past vtium.conf's
    // `head-consoles-max` gets the cap, and counts against the file, once,
    // at its `consoles` line, past the hard cap of 50 too
    // covers: platform.heads.rules
    #[test]
    fn consoles_past_the_head_cap_count_against_the_file() {
        let past = |n: u32| {
            format!(
                "Head \"a\" asks for more consoles than `head-consoles-max` allows, so it gets {n}. Lower its `consoles` to {n} or fewer, or raise `head-consoles-max` in vtium.conf."
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
                    "`consoles` is too high: a head can have at most 50 consoles, so this head gets 50. Lower it to 50 or fewer."
                        .to_owned(),
                ),
            ),
        ] {
            let text =
                format!("version = 1\n[head a]\nconsoles = {consoles}\n[rules]\ndisplay * -> a\n");
            let (_dir, file, trust) = heads_fixture(&format!("cap-{consoles}-{cap}"), &text, 0o644);
            let f = file.display().to_string();
            let settings = Settings {
                total: 64,
                ..caps(cap)
            };
            let out = conf_file(&file, false, &trust, &machine(true), &settings);
            let want: Vec<String> = want.iter().map(|w| format!("{f}:3: {w}")).collect();
            assert_eq!(out.counted, want, "consoles {consoles}, cap {cap}");
        }
    }

    // ADR-1401 D10: with vtium.conf's caps unset, check-config caps as the
    // seat does, 4 consoles a head and 8 in all
    #[test]
    fn the_caps_unset_are_the_seats() {
        let k = Settings::of(&config::Config::default(), Path::new("/f"), Trust::ROOT);
        assert_eq!((k.head_cap, k.total), (4, 8));
        assert_eq!(
            (k.head_cap, k.total),
            (
                vtium_settings::HEAD_CONSOLES_DEFAULT,
                vtium_settings::CONSOLES_TOTAL_DEFAULT
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
                "{f}:{line}: The heads together ask for more consoles than `consoles-max` (8) allows, so head \"{head}\" gets {gets}. Lower the heads' `consoles`, or raise `consoles-max` in vtium.conf."
            )
        };
        assert_eq!(out.counted, [short(5, "b", 3), short(7, "c", 1)]);
    }

    // ADR-1401 D7, ADR-1302 D14: a look whose font set leaves everyday
    // letters of the locale's language to the squeeze is noted, outside the exit
    // status, naming the look, the font set, the script, how many letters
    // and the installed font the glyph helper names. The stand-in answers
    // as the helper does: it refuses a frit it cannot read, covers what
    // the frit's sources hold, and names the font that covers the rest.
    // GNU Unifont, the built-in font at 8x16, draws Devanagari 16 pixels
    // wide, so it squeezes them. A font set that covers the locale's
    // script takes no note, and neither does the C locale
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_font_set_that_leaves_the_locales_script_to_the_squeeze_is_noted() {
        let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
        let (_dir, file, trust) = heads_fixture("squeeze", text, 0o644);
        let f = file.display().to_string();
        let frits = file.parent().unwrap().join("frits");
        std::fs::create_dir_all(&frits).unwrap();
        std::fs::write(frits.join("desk.vtfrit"), "source=font:Latin Only\n").unwrap();
        let ask = |frit: &helper::FritText, letters: &[u32]| {
            if frit.name != "desk" {
                return Err("no such font set".to_owned());
            }
            // a font set covers every everyday letter of the language its
            // one source is named for: Latin English's, Devanagari Hindi's
            let covers = |(script, locale): (&str, &str)| {
                let all = vtium_font::letters::of_locale(locale);
                frit.text.contains(script).then_some(all)
            };
            let held: Vec<char> = [("Latin", "en"), ("Devanagari", "hi")]
                .into_iter()
                .filter_map(covers)
                .flatten()
                .collect();
            let covered = letters
                .iter()
                .copied()
                .filter(|&c| char::from_u32(c).is_some_and(|c| held.contains(&c)))
                .collect();
            Ok((covered, Some("Noto Sans Devanagari".to_owned())))
        };
        let check = |locale: &str| {
            let settings = Settings {
                frits: &frits,
                trust: font::trust().below(file.parent().unwrap()),
                squeeze: Some(Squeeze { locale, ask: &ask }),
                ..caps(4)
            };
            conf_file(&file, false, &trust, &machine(true), &settings)
        };
        let left = vtium_font::letters::of_locale("hi_IN")
            .into_iter()
            .filter(|&c| {
                font::builtin_squeezes(
                    c,
                    vtium_font::Cell {
                        width: 8,
                        height: 16,
                    },
                    "hi_IN",
                )
            })
            .count();
        // 67 of Hindi's exemplar characters are two cells wide in GNU Unifont
        assert_eq!(left, 67);
        let out = check("hi_IN.UTF-8");
        assert!(out.counted.is_empty(), "{:?}", out.counted);
        let want = format!(
            "{f}:3: Look \"big\": the font set desk has no glyph for {left} letters that the system's language writes in the Devanagari script. vtium draws those letters from its built-in font squeezed into their character cells, which keeps them whole but narrow. The installed font Noto Sans Devanagari covers them; add it to the font set with a source=font:Noto Sans Devanagari line."
        );
        assert!(out.notes.contains(&want), "{:#?}", out.notes);
        let squeezed = |out: &HeadCheck| out.notes.iter().any(|n| n.contains("squeezed"));
        assert!(!squeezed(&check("en_US.UTF-8")));
        assert!(!squeezed(&check("C")));
        std::fs::write(
            frits.join("desk.vtfrit"),
            "source=font:Noto Sans Devanagari\n",
        )
        .unwrap();
        assert!(!squeezed(&check("hi_IN.UTF-8")));
    }

    // A look's font set that needs a font this machine lacks, or has in an
    // older version, is reported with the font, the version the set asks
    // for, the font's page and the help page on installing fonts, as the
    // glyph helper answers. A missing font, and one that fails its check=,
    // is a warning, which counts against the file. An older version and a
    // file with another hash are notes, because vtium still uses the font.
    // A source that names the system's font gets no line, and a helper
    // that cannot answer gives one note that says so.
    //
    // References:
    // - ADR-1204 D7: check-config names each missing font as vtium config
    //   fonts does.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_font_a_looks_font_set_lacks_is_noted_with_its_page() {
        let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
        let (_dir, file, trust) = heads_fixture("font-needs", text, 0o644);
        let f = file.display().to_string();
        let frits = file.parent().unwrap().join("frits");
        std::fs::create_dir_all(frits.join("layers")).unwrap();
        std::fs::write(
            frits.join("desk.vtfrit"),
            "source=font:Gone Mono\nversion=2.304\nurl=https://example.org/gone\nsource=font:Old Mono\nversion=3.1\nlayer=nerd\nsource=vconsole\n",
        )
        .unwrap();
        std::fs::write(
            frits.join("layers/nerd.vtlayer"),
            "role=symbols\nsource=font:Plain\nsource=font:Nerd\ncheck=U+E0B0\n",
        )
        .unwrap();
        let answer = "missing\t1\tfont:Gone Mono\tno font of family Gone Mono\n\
                      missing\t6\tfont:Nerd\tit lacks U+E0B0\n\
                      missing\t7\tvconsole\tno FONT=\n\
                      older\t4\tfont:Old Mono\t3.000\n\
                      lacks\t6\tfont:Nerd\tU+E0B0";
        let ask = |frit: &helper::FritText| {
            if frit.name == "desk" && frit.text.contains("source=font:Nerd") {
                Ok(answer.to_owned())
            } else {
                Err("the layer is not in place".to_owned())
            }
        };
        let check = |sources: Option<FontSources<'_>>| {
            let settings = Settings {
                frits: &frits,
                trust: font::trust().below(file.parent().unwrap()),
                sources,
                ..caps(4)
            };
            conf_file(&file, false, &trust, &machine(true), &settings)
        };
        let out = check(Some(FontSources { ask: &ask }));
        let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
        let warned = [
            format!(
                "{f}:3: Font set desk needs the font Gone Mono, version 2.304 or later, and no font of that name is installed. You can get the font from https://example.org/gone. {page}"
            ),
            format!(
                "{f}:3: Font set desk needs the font Nerd. The installed font of that name lacks the character U+E0B0, so it is a different font, and vtium does not use it. {page}"
            ),
        ];
        assert_eq!(out.counted, warned, "{out:#?}");
        let noted: Vec<&String> = out
            .notes
            .iter()
            .filter(|n| n.contains("Font set desk"))
            .collect();
        assert_eq!(
            noted,
            [&format!(
                "{f}:3: Font set desk asks for the font Old Mono, version 3.1 or later, and the installed font is version 3.000. vtium still uses it. {page}"
            )],
            "{:#?}",
            out.notes
        );
        let none = check(None);
        assert!(!none.notes.iter().any(|n| n.contains("Font set desk")));
        // a helper that gives no answer is said, so the check never passes
        // in silence on fonts it could not ask about
        let failing = |_: &helper::FritText| Err("the helper did not start".to_owned());
        let failed = check(Some(FontSources { ask: &failing }));
        assert!(failed.counted.is_empty(), "{:?}", failed.counted);
        let named: Vec<&String> = failed
            .notes
            .iter()
            .filter(|n| n.contains("Font set desk"))
            .collect();
        assert_eq!(
            named,
            [&format!(
                "{f}:3: Font set desk: vtium cannot check whether the fonts it needs are installed: the helper did not start."
            )]
        );
    }

    /// The glyph helper's answer to a sources query for `frit`:
    /// vtium-config's serve mode, run in this process on the request the
    /// console's helper sends, so the lines are the ones the real helper
    /// sends. A frit the helper draws nothing from, and that does not
    /// read, is its reason.
    #[expect(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "a test fails by panicking"
    )]
    fn served_sources(frit: &helper::FritText) -> Result<String, String> {
        use vtium_config::serve;
        let request = serve::ServeRequest {
            kind: serve::SOURCES,
            frit: serve::FritText {
                name: frit.name.clone(),
                text: frit.text.clone(),
            },
            cell: vtium_font::Cell {
                width: 0,
                height: 0,
            },
            band: vtium_font::spacing::Band::NONE,
            style: vtium_font::Style::default(),
            script: vtium_font::Script::Baseline,
            doubled: None,
            cluster_width: Default::default(),
            codepoints: Vec::new(),
            clusters: Vec::new(),
            held: Vec::new(),
            console: None,
        };
        let mut out = Vec::new();
        let status = serve::serve(
            std::io::Cursor::new(request.encode()),
            &mut out,
            &serve::Paths::default(),
            std::time::Duration::from_secs(5),
        );
        assert_eq!(status, 0);
        let mut rest = out.as_slice();
        while let [k0, k1, k2, k3, l0, l1, l2, l3, tail @ ..] = rest {
            let kind = u32::from_le_bytes([*k0, *k1, *k2, *k3]);
            let len = u32::from_le_bytes([*l0, *l1, *l2, *l3]) as usize;
            let text = String::from_utf8(tail[..len].to_vec()).unwrap();
            rest = &tail[len..];
            match kind {
                serve::FRAME_SOURCES => return Ok(text),
                serve::FRAME_DRAWS_NOTHING => return Err(text),
                _ => {}
            }
        }
        Err("the helper sent no answer".to_owned())
    }

    // A look's font set none of whose fonts this machine has draws nothing,
    // and check-config still names each font it needs, with the version
    // and the page the set gives, a layer's font among them. The lines come
    // from the glyph helper's own answer, so a layer's font is found by the
    // line the helper names for it, the set's layer= line.
    //
    // References:
    // - ADR-1204 D7: check-config names each missing font as vtium config
    //   fonts does, whether or not the set has a font left.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_set_with_no_font_left_names_each_font_from_the_helpers_answer() {
        let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
        let (_dir, file, trust) = heads_fixture("font-needs-none-left", text, 0o644);
        let f = file.display().to_string();
        let frits = file.parent().unwrap().join("frits");
        std::fs::create_dir_all(frits.join("layers")).unwrap();
        std::fs::write(
            frits.join("desk.vtfrit"),
            "source=font:Gone Mono Nowhere\nversion=2.304\nurl=https://example.org/gone\nlayer=nerd\n",
        )
        .unwrap();
        std::fs::write(
            frits.join("layers/nerd.vtlayer"),
            "role=symbols\nsource=font:Nerd Gone Nowhere\nversion=1.2\nurl=https://example.org/nerd\n",
        )
        .unwrap();
        let settings = Settings {
            frits: &frits,
            trust: font::trust().below(file.parent().unwrap()),
            sources: Some(FontSources {
                ask: &served_sources,
            }),
            ..caps(4)
        };
        let out = conf_file(&file, false, &trust, &machine(true), &settings);
        let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
        let want = [
            format!(
                "{f}:3: Font set desk needs the font Gone Mono Nowhere, version 2.304 or later, and no font of that name is installed. You can get the font from https://example.org/gone. {page}"
            ),
            format!(
                "{f}:3: Font set desk needs the font Nerd Gone Nowhere, version 1.2 or later, and no font of that name is installed. You can get the font from https://example.org/nerd. {page}"
            ),
        ];
        assert_eq!(out.counted, want, "{out:#?}");
        assert!(!out.notes.iter().any(|n| n.contains("Font set desk")));
    }

    /// What check-config says of the look `big`'s font set `desk`, given
    /// as `frit`, with each `layers` entry, a name and its text, written to
    /// the font sets' `layers` folder. The fonts this machine lacks come
    /// from the glyph helper's own answer. Returns the vtium.conf's path
    /// and the result, with the folder kept alive.
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn desk_needs(
        fixture: &str,
        frit: &str,
        layers: &[(&str, &str)],
    ) -> (Scratch, String, HeadCheck) {
        let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
        let (dir, file, trust) = heads_fixture(fixture, text, 0o644);
        let frits = file.parent().unwrap().join("frits");
        std::fs::create_dir_all(frits.join("layers")).unwrap();
        std::fs::write(frits.join("desk.vtfrit"), frit).unwrap();
        for (name, body) in layers {
            std::fs::write(frits.join(format!("layers/{name}.vtlayer")), body).unwrap();
        }
        let settings = Settings {
            frits: &frits,
            trust: font::trust().below(file.parent().unwrap()),
            sources: Some(FontSources {
                ask: &served_sources,
            }),
            ..caps(4)
        };
        let out = conf_file(&file, false, &trust, &machine(true), &settings);
        (dir, file.display().to_string(), out)
    }

    /// A BDF font of one glyph, `A`, in an 8×16 cell.
    const ONE_GLYPH_BDF: &str = "STARTFONT 2.1\nFONT -test-\nFONTBOUNDINGBOX 8 16 0 -4\nSTARTPROPERTIES 3\nFONT_ASCENT 12\nFONT_DESCENT 4\nCHARSET_REGISTRY \"ISO10646\"\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 8 0\nBBX 8 16 0 -4\nBITMAP\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\nENDCHAR\nENDFONT\n";

    // A layer's font named by the path of a .hex file, such as the unifont
    // layer's, is a font this machine can lack like any other file, so
    // check-config names it with its version and its page.
    //
    // References:
    // - ADR-1204 D5: the unifont layer's hex: source.
    // - ADR-1204 D7: check-config names each missing font as vtium config
    //   fonts does.
    #[test]
    fn a_missing_hex_font_is_named_with_its_page() {
        let (_dir, f, out) = desk_needs(
            "font-needs-hex",
            "source=builtin:unifont\nlayer=uni\n",
            &[(
                "uni",
                "role=fallback\nsource=hex:/nonesuch/unifont_all-17.0.03.hex\nversion=17.0.03\nurl=https://unifoundry.com/pub/unifont/unifont-17.0.03/\n",
            )],
        );
        let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
        assert_eq!(
            out.counted,
            [format!(
                "{f}:3: Font set desk needs the font /nonesuch/unifont_all-17.0.03.hex, version 17.0.03 or later, and there is no file at that path. You can get the font from https://unifoundry.com/pub/unifont/unifont-17.0.03/. {page}"
            )],
            "{out:#?}"
        );
    }

    // A strike is a file of its source's font at one more height, so a
    // missing strike is named with its source's version= and url=, whether
    // those lines come before the strike= line or after it. A sha256= line
    // after a strike= line belongs to the strike, and ends nothing.
    //
    // References:
    // - ADR-1204 D2: the strike= key, and the lines that belong to a strike.
    // - ADR-1204 D7: check-config names each missing font as vtium config
    //   fonts does.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_missing_strike_is_named_with_its_sources_version_and_page() {
        let fonts = Scratch::new("font-needs-strike-files");
        let (regular, bold) = (fonts.join("ter-u16n.bdf"), fonts.join("ter-u16b.bdf"));
        std::fs::write(&regular, ONE_GLYPH_BDF).unwrap();
        std::fs::write(&bold, ONE_GLYPH_BDF).unwrap();
        let hash = "0".repeat(64);
        let frit = format!(
            "source=bdf:{}\nversion=4.49.1\nurl=https://example.org/terminus\nstrike=bdf:/nonesuch/ter-u12n.bdf.gz\nsha256={hash}\nstrike=bdf:/nonesuch/ter-u14n.bdf.gz\nsource=bdf:{}\nstyle=bold\nstrike=bdf:/nonesuch/ter-u12b.bdf.gz\nsha256={hash}\nversion=2.0\nurl=https://example.org/bold\n",
            regular.display(),
            bold.display()
        );
        let (_dir, f, out) = desk_needs("font-needs-strike", &frit, &[]);
        let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
        let missing = |font: &str, version: &str, url: &str| {
            format!(
                "{f}:3: Font set desk needs the font {font}, version {version} or later, and there is no file at that path. You can get the font from {url}. {page}"
            )
        };
        assert_eq!(
            out.counted,
            [
                missing(
                    "/nonesuch/ter-u12n.bdf.gz",
                    "4.49.1",
                    "https://example.org/terminus"
                ),
                missing(
                    "/nonesuch/ter-u14n.bdf.gz",
                    "4.49.1",
                    "https://example.org/terminus"
                ),
                missing(
                    "/nonesuch/ter-u12b.bdf.gz",
                    "2.0",
                    "https://example.org/bold"
                ),
            ],
            "{out:#?}"
        );
    }

    // ADR-1401 D7, ADR-1403 D5: a look whose font-name names a frit not
    // installed is a warning about the file, and an installed one is none
    // covers: platform.heads.rules
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_look_naming_a_missing_frit_counts_against_the_file() {
        let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
        let (_dir, file, trust) = heads_fixture("frit", text, 0o644);
        let f = file.display().to_string();
        let frits = file.parent().unwrap().join("frits");
        std::fs::create_dir_all(&frits).unwrap();
        let settings = Settings {
            frits: &frits,
            trust: font::trust().below(file.parent().unwrap()),
            ..caps(4)
        };
        let missing = conf_file(&file, false, &trust, &machine(true), &settings);
        std::fs::write(frits.join("desk.vtfrit"), "source=builtin:unifont\n").unwrap();
        let installed = conf_file(&file, false, &trust, &machine(true), &settings);
        let dir = frits.display();
        assert_eq!(
            missing.counted,
            [format!(
                "{f}:3: Look \"big\" uses the font set \"desk\", but \"{dir}\" has no \"desk\".vtfrit. vtium then looks for a kbd font named \"desk\", and uses its built-in font if there is none. Put the font set in \"{dir}\", or change the look's `font-name`."
            )]
        );
        assert_eq!(installed.counted, [] as [String; 0]);
    }

    // ADR-1401 D10, ADR-1403 D5: a look's frit the ownership check refuses
    // counts against the file on its `font-name` line, as the font-name
    // frit's refusal counts, with the path that fails and the command that
    // mends it; a frit the account reads but a start as root would refuse
    // is noted on that line
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_looks_frit_the_ownership_check_refuses_is_reported() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let text = "version = 1\n[look big]\nfont-name = g\n[look small]\nfont-name = t\n[head a]\nlook = big\n[head b]\nlook = small\n[rules]\ndisplay * -> a\n";
        let (_dir, file, trust) = heads_fixture("look-trust", text, 0o644);
        let f = file.display().to_string();
        let frits = file.parent().unwrap().join("frits");
        std::fs::create_dir_all(&frits).unwrap();
        std::fs::set_permissions(&frits, std::fs::Permissions::from_mode(0o755)).unwrap();
        for (name, mode) in [("g", 0o664), ("t", 0o644)] {
            let path = frits.join(format!("{name}.vtfrit"));
            std::fs::write(&path, "source=builtin:unifont\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let uid = std::fs::metadata(&frits).unwrap().uid();
        let settings = Settings {
            frits: &frits,
            trust: font::trust().below(file.parent().unwrap()),
            ..caps(4)
        };
        let out = conf_file(&file, false, &trust, &machine(true), &settings);
        let (g, t) = (frits.join("g.vtfrit"), frits.join("t.vtfrit"));
        let refused: Vec<&String> = out
            .counted
            .iter()
            .filter(|l| l.contains("vtfrit"))
            .collect();
        let (g, t) = (g.display(), t.display());
        assert_eq!(
            refused,
            [&format!(
                "{f}:3: Look \"big\": vtium does not read its font set g from \"{g}\", because other users can change \"{g}\" (mode 664). Run `chmod go-w {g}` so that only its owner can change it. vtium draws that look in its built-in font."
            )]
        );
        let noted: Vec<&String> = out.notes.iter().filter(|l| l.contains("vtfrit")).collect();
        assert_eq!(
            noted,
            [&format!(
                "{f}:5: Look \"small\": when vtium runs as root, it does not read its font set t from \"{t}\", because \"{t}\" belongs to user {uid}, not to root. Run `chown root:root {t}` to make root the owner. vtium would then draw that look in its built-in font."
            )]
        );
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
                    "{f}:1: vtium does not know version \"2\"; it reads only `version = 1`. Change the version line to `version = 1`."
                ),
                format!(
                    "{f}:1: Because of this error, vtium ignores the whole file except its `policy` line."
                ),
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
                    "{f}:3: `policy` must be at the top of vtium.conf, above the first section, so vtium ignores this line. Move it there, beside `version`."
                ),
                format!("{f}:3: Because of this error, vtium ignores this line."),
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
                    "{f}:3: Head \"b\" is not used: no rule sends a display or input device to it. Add a rule that sends a display to it, or remove the head."
                ),
                format!("{f}: {RULES_ALONE}"),
                format!("{f}: {AS_WRITTEN}"),
                format!("{f}: {UNSET}"),
            ]
        );
    }

    /// `text` checked against the devices `devices`, its notes but the
    /// outcome and the policy, without the path before them: a note on a
    /// line starts with the line.
    fn remarked(name: &str, text: &str, devices: &[Identity]) -> (Vec<String>, Vec<String>) {
        let (_dir, file, trust) = heads_fixture(name, text, 0o644);
        let f = format!("{}:", file.display());
        let m = Machine {
            udev_daemon: true,
            devices: Some(devices),
            vts: VtsEnabled::FileAlone,
        };
        let out = conf_file(&file, false, &trust, &m, &caps(4));
        let bare = |n: &String| {
            let n = n.strip_prefix(&f).unwrap_or(n);
            n.strip_prefix(' ').unwrap_or(n).to_owned()
        };
        let notes = out.notes.len().saturating_sub(2);
        let notes = out
            .notes
            .get(..notes)
            .unwrap_or_default()
            .iter()
            .map(bare)
            .collect();
        (out.counted.iter().map(bare).collect(), notes)
    }

    /// What a check against the devices present counts of a file whose
    /// display rules send no display present to a head (ADR-1403 D3).
    const NO_DISPLAY_SENT: &str = "No connected display reaches a head: no rule matches display \"display 1\", so it stays off. Add a display rule that sends it to a head.";

    /// What it counts of a file whose only head is isolated, with no
    /// input rule that sends a keyboard to it.
    const ISOLATED: &str = "Head \"a\" is isolated, so roaming keyboards do not reach it. Add an input rule that sends a keyboard to \"a\", or write `isolated = no` in its section.";

    /// A display, a keyboard and a joystick, each named.
    fn present() -> Vec<Identity> {
        use vtium_settings::Class;
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
    // places is noted with what the policy gives it
    // covers: platform.heads.policy
    #[test]
    fn what_falls_to_the_open_policy_is_noted() {
        let (counted, notes) = remarked(
            "fell-open",
            "version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput class:joystick -> a\n",
            &present(),
        );
        assert_eq!(counted, [] as [String; 0]);
        assert_eq!(
            notes,
            [
                "No rule matches input device \"Keyboard A\", so with the open policy it types into the active head. Add an `input` rule to send it somewhere else.",
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
        assert_eq!(counted, [NO_DISPLAY_SENT]);
        assert_eq!(
            notes,
            [
                "head a has only one keyboard, \"Keyboard A\"",
                "No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated.",
                "No rule matches display \"DP-1\", so it stays off. Add a `display` rule for it if a head should use it.",
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
        assert_eq!(
            counted,
            [
                "No keyboard reaches head \"a\", which shows on \"display 1\". Add an input rule that sends a keyboard to \"a\" or to `roaming`."
            ]
        );
        assert_eq!(
            notes,
            [
                "No keyboard reaches any head, so with the closed policy nobody can type at this machine. Before you install the file, send a keyboard to a head, or to `roaming` with a head that is not isolated.",
                "every keyboard is ignored",
                "No rule matches input device \"Keyboard A\", so with the closed policy vtium ignores it. Add an `input` rule for it if it should reach a head.",
                "No rule matches input device \"Pad\", so with the closed policy vtium ignores it. Add an `input` rule for it if it should reach a head.",
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
                "head a has only one keyboard, \"Keyboard A\"",
                "Every head is isolated, so no keyboard can switch the VT and no head can be the active head. A head gets only the keyboards its own rules send to it. Write `isolated = no` in a head's section if a keyboard should roam to it."
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
                "No working rule matches input device \"Keyboard A\", so it goes on to the built-in rules and types into the active head. Fix the file's errors, or add an `input` rule to send it somewhere else.",
                "No working rule matches input device \"Pad\", so it goes on to the built-in rules, which ignore it. Fix the file's errors, or add an `input` rule for it if it should reach a head.",
            ]
        );
    }

    // ADR-1401 D3, D9: for a valid file the table decides, with or without
    // a VT number, so the first head no rule names gets no display and the
    // display no rule matches stays off, with the devices present or away
    // from them. Only what survives of an open file with errors runs the
    // display guard, which gives the first head the boot display while no
    // head has one.
    // covers: platform.heads.guards
    #[test]
    fn the_first_head_no_rule_names_is_lit_by_the_display_guard_only_for_a_file_with_errors() {
        let file = "version = 1\n[head a]\n[rules]\ninput * -> roaming\n";
        let unused = "Head \"a\" is not used: no rule sends a display or input device to it. Add a rule that sends a display to it, or remove the head.";
        let (counted, notes) = remarked("unnamed", file, &present());
        assert_eq!(counted, [NO_DISPLAY_SENT]);
        assert_eq!(
            notes,
            [
                "No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated.".to_owned(),
                "No rule matches display \"DP-1\", so it stays off. Add a `display` rule for it if a head should use it.".to_owned(),
                format!("2: {unused}"),
            ]
        );
        let lit = "No rule names head \"a\". Because the file has errors, vtium gives it the boot display under the open policy when no other head has a display. Add a rule that sends a display to it, or remove the head.";
        let broken = format!("{file}input path:usb-0:9 -> nowhere\n");
        let (_, notes) = remarked("unnamed-guard", &broken, &present());
        assert!(notes.contains(&format!("2: {lit}")), "{notes:?}");
        let (out, f) = checked("unnamed-away", file, true);
        assert_eq!(
            out.notes,
            [
                format!("{f}:2: {unused}"),
                format!("{f}: {RULES_ALONE}"),
                format!("{f}: {AS_WRITTEN}"),
                format!("{f}: {UNSET}")
            ]
        );
    }

    /// A keyboard and a joystick, each named, and no display.
    fn dark() -> Vec<Identity> {
        present().into_iter().skip(1).collect()
    }

    // ADR-1401 D5, D9: an input that roams while no head can be the active
    // head reaches no head, under the file's rules and under the shipped
    // rules' tail alike
    // covers: platform.heads.focus
    #[test]
    fn an_input_that_roams_with_no_active_head_is_noted_as_reaching_none() {
        let (counted, notes) = remarked(
            "roams-nowhere",
            "version = 1\n[head a]\nisolated = yes\n[head b]\n[rules]\ndisplay * -> a\n\
             input class:joystick -> a\n",
            &present(),
        );
        assert_eq!(counted, [ISOLATED]);
        assert_eq!(
            notes,
            [
                "No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated.",
                "Input device \"Keyboard A\" reaches no head: no rule matches it, so with the open policy it roams, but no head that can be active has a connected display. Add an `input` rule that sends it to a head.",
                "4: Head \"b\" is not used: no rule sends a display or input device to it. Add a rule that sends a display to it, or remove the head.",
            ]
        );
        let (counted, notes) = remarked(
            "roams-nowhere-tail",
            "version = 1\n[head a]\n[rules]\ndisplay * -> a\ndisplay * -> b\n",
            &dark(),
        );
        assert_eq!(counted.len(), 2, "{counted:?}");
        assert!(
            notes.contains(&"Input device \"Keyboard A\" reaches no head: no working rule matches it, so it goes on to the built-in rules and roams, but no head that can be active has a connected display. Fix the file's errors, or add an `input` rule that sends it to a head.".to_owned()),
            "{notes:?}"
        );
    }

    // ADR-1401 D5, D9: under closed a keyboard sent to `roaming` while no
    // head can be the active head reaches none, which is a lockout
    // covers: platform.heads.policy
    #[test]
    fn a_closed_keyboard_roaming_with_no_active_head_is_a_lockout() {
        let (counted, notes) = remarked(
            "roams-closed",
            "version = 1\npolicy = closed\n[head a]\n[rules]\ndisplay * -> a\ninput * -> roaming\n",
            &present(),
        );
        assert_eq!(counted, [ISOLATED]);
        assert_eq!(
            notes.first().map(String::as_str),
            Some(
                "No keyboard reaches any head, so with the closed policy nobody can type at this machine. Before you install the file, send a keyboard to a head, or to `roaming` with a head that is not isolated."
            ),
            "{notes:?}"
        );
    }

    // ADR-1401 D7: a start that runs the shipped default runs none of the
    // file, so a broken file checked on its machine draws no remark and no
    // device note
    #[test]
    fn a_file_the_start_drops_draws_no_device_notes() {
        for (name, file) in [
            (
                "dropped-version",
                "version = 2\n[head a]\n[rules]\ninput * -> a\n",
            ),
            (
                "dropped-heads",
                "version = 1\n[head kiosk]\nrescue = off\n[rules]\ninput * -> kiosk\n",
            ),
        ] {
            let (counted, notes) = remarked(name, file, &present());
            assert!(!counted.is_empty(), "{name}");
            assert_eq!(notes, [] as [String; 0], "{name}");
        }
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
                "Every head in this file has errors, so vtium starts with its built-in configuration and shows `vtium.conf not used: running the built-in configuration.`.",
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
                "{f}:{line}: weak match: this rule matches by `id` or `name` alone, which any device can claim"
            )
        };
        let shown = |what: &str| format!("{f}: {what}");
        assert_eq!(
            out.notes,
            [
                weak(5),
                weak(6),
                shown(RULES_ALONE),
                shown(AS_WRITTEN),
                shown(UNSET)
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
                    input class:keyboard -> roaming\n";
        let (out, f) = checked("udev", text, false);
        assert_eq!(out.counted, [] as [String; 0]);
        let no = |line: usize, key: &str| {
            format!("{f}:{line}: `{key}:` matches nothing, because udev is not running")
        };
        let udev: Vec<&String> = out
            .notes
            .iter()
            .filter(|n| n.contains("udev is not running"))
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

    // ADR-1401 D4, vtium/vtium#157: check-config opens no card, so no
    // listed display carries a KMS connector's PATH; each `connector-path:`
    // rule is noted as unchecked, with or without a udev daemon
    #[test]
    fn each_connector_path_rule_is_noted_as_unchecked_against_the_displays_present() {
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
                    "{f}:{line}: `connector-path:` is not checked against the connected \
                     displays, because check-config does not open the graphics card; vtium \
                     matches it when it finds the display"
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
            conf_file(&path, false, &trust, &machine(true), &caps(4)),
            HeadCheck {
                counted: Vec::new(),
                notes: vec![format!(
                    "{shown}: There is no vtium.conf here, so vtium starts with its built-in configuration and shows no notice."
                )],
                refused: false,
                missing_fonts: 0,
                vtlooks: Vec::new(),
            }
        );
        assert_eq!(
            conf_file(&path, true, &trust, &machine(true), &caps(4)),
            HeadCheck {
                counted: vec![format!("{shown}: does not exist")],
                notes: Vec::new(),
                refused: false,
                missing_fonts: 0,
                vtlooks: Vec::new(),
            }
        );
    }

    // ADR-1401 D10: the installed file failing the ownership check is
    // invalid; a candidate failing it where it sits is noted and checked
    // (the recommendation, CANDIDATE_OWNERSHIP_COUNTS)
    #[test]
    fn ownership_counts_for_the_installed_file_and_is_a_note_for_a_candidate() {
        let text = "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> desc\n";
        let (_dir, file, trust) = heads_fixture("owner", text, 0o666);
        let f = file.display().to_string();
        // ADR-1403 D3: the refusal names the path, its mode and the
        // command that mends it
        let untrusted = format!(
            "{f}: vtium ignores this file and uses its built-in configuration, because other users can change \"{f}\" (mode 666). Run `chmod go-w {f}` so that only its owner can change it."
        );
        let installed = conf_file(&file, false, &trust, &machine(true), &caps(4));
        assert_eq!(installed.counted, [untrusted]);
        assert_eq!(
            installed.notes,
            [format!(
                "{f}: vtium does not read a file it does not trust, so it starts with its built-in configuration and shows `vtium.conf not used: running the built-in configuration.`."
            )]
        );
        let candidate = conf_file(&file, true, &trust, &machine(true), &caps(4));
        assert_eq!(
            candidate.notes,
            [
                format!(
                    "{f}: Other users can change \"{f}\" (mode 666). That is fine for a check: vtium reads the file once it is installed, owned by root and writable only by root."
                ),
                format!("{f}: {RULES_ALONE}"),
                format!("{f}: {PARTLY_OPEN}"),
                format!("{f}: {PARTLY_UNSET_POLICY}"),
            ]
        );
        assert_eq!(
            candidate.counted,
            [
                format!(
                    "{f}:5: This rule sends devices to head \"desc\", but the file has no head of that name. Correct the name, or add a `[head NAME]` section for it."
                ),
                format!(
                    "{f}:5: Because of this error, vtium ignores this rule, and each device goes on to the next rule."
                ),
            ]
        );
    }

    // ADR-1401 D7, D9: what depends on the devices present names each
    // rule no device matches and each layer 1 warning, with the device as
    // it reports itself, escaped
    #[test]
    fn device_findings_name_idle_rules_and_lockouts_with_the_device_escaped() {
        use vtium_settings::{Class, parse};
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
                "/etc/vtium/heads.conf:8: the rule matches no connected device",
                "/etc/vtium/heads.conf: head desk has only one keyboard, \"Keyb\\u{1b}[2J\"",
                "/etc/vtium/heads.conf: head panel has mice but no keyboard: every keyboard goes to another head",
            ]
        );
    }

    // ADR-1401 D7: the word after the subcommand is the candidate, and a
    // second is no option
    #[test]
    fn the_word_after_the_subcommand_is_the_candidate() {
        let load = |args: &[&str]| {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            config::load(&args, &mut |_: &Path| Ok(None))
        };
        let l = load(&[SUBCOMMAND, "/tmp/heads.conf"]);
        assert_eq!(l.candidate.as_deref(), Some(Path::new("/tmp/heads.conf")));
        assert!(l.reports.is_empty(), "{:?}", l.reports);
        let l = load(&["-c", "/etc/vtium", SUBCOMMAND, "a.conf", "b.conf"]);
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
            eprintln!(
                "running as root, which can read every file, so the readability check is skipped"
            );
            return;
        }
        let mode = |p: &Path, m: u32| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
        };
        let (dir, file, trust) = heads_fixture("denied", "version = 1\n", 0o000);
        let denied = HeadCheck {
            counted: vec![format!(
                "{}: permission denied: only root can check this file",
                file.display()
            )],
            notes: Vec::new(),
            refused: false,
            missing_fonts: 0,
            vtlooks: Vec::new(),
        };
        assert_eq!(
            conf_file(&file, false, &trust, &machine(true), &caps(4)),
            denied,
            "the file"
        );
        assert_eq!(
            conf_file(&file, true, &trust, &machine(true), &caps(4)),
            denied,
            "a candidate"
        );
        mode(&file, 0o644);
        mode(&dir, 0o000);
        assert_eq!(
            conf_file(&file, false, &trust, &machine(true), &caps(4)),
            denied,
            "its directory"
        );
        mode(&dir, 0o700);
    }

    #[test]
    fn a_udev_daemon_runs_where_its_control_socket_is_present() {
        let (_dir, file, _) = heads_fixture("udev-control", "", 0o644);
        assert!(udev_daemon_runs(&file));
        assert!(!udev_daemon_runs(&file.with_file_name("absent")));
    }

    /// The subcommand on the command line `args` and a host of fixtures.
    fn outcome_on(args: &[&str], host: &Host) -> Outcome {
        let args: Vec<OsString> = [SUBCOMMAND]
            .iter()
            .chain(args)
            .map(OsString::from)
            .collect();
        outcome(&config::load(&args, &mut |_: &Path| Ok(None)), host)
    }

    use std::collections::BTreeSet;

    use crate::console::{Enabled, Mode};

    /// A laptop's file: a head with no `vt` and a head on `vts`.
    fn two_heads_on(vts: &str) -> String {
        format!(
            "version = 1\n[head console]\n[head terminal-1]\nvt = {vts}\n[rules]\ndisplay card:card1 -> terminal-1\ndisplay * -> console\n"
        )
    }

    /// `text`'s notes against a machine whose VTs are `vts`.
    fn vt_notes(name: &str, text: &str, vts: VtsEnabled) -> (HeadCheck, String) {
        let (_dir, file, trust) = heads_fixture(name, text, 0o644);
        let machine = Machine {
            udev_daemon: true,
            devices: None,
            vts,
        };
        let out = conf_file(&file, false, &trust, &machine, &caps(4));
        (out, file.display().to_string())
    }

    // ADR-1401 D3, "Enter on a head's own keyboard": a head whose VTs have
    // no vtium enabled is noted at its line, with the command that enables
    // one, outside the exit status; a head with one such VT, or with no
    // `vt`, is not
    // covers: platform.heads.rules
    #[test]
    fn a_head_on_vts_where_vtium_is_not_enabled_is_noted_with_the_command() {
        let boot = Enabled {
            vts: BTreeSet::from([2, 3, 4, 5, 6]),
            mode: Mode::Direct,
        };
        let (out, f) = vt_notes("vt-off", &two_heads_on("12"), VtsEnabled::On(&boot));
        assert_eq!(out.counted, Vec::<String>::new());
        assert!(
            out.notes.contains(&format!(
                "{f}:3: Head terminal-1 runs only on VT 12, and vtium is not enabled on VT 12, so nothing shows this head. To enable vtium there, run sudo vtium console enable --direct 12."
            )),
            "{:#?}",
            out.notes
        );
        let seat = Enabled {
            vts: BTreeSet::from([2]),
            mode: Mode::Seat,
        };
        let (out, f) = vt_notes("vt-two", &two_heads_on("7 12"), VtsEnabled::On(&seat));
        assert!(
            out.notes.contains(&format!(
                "{f}:3: Head terminal-1 runs only on VTs 7 and 12, and vtium is enabled on none of them, so nothing shows this head. To enable vtium on VT 7, run sudo vtium console enable --seat 7."
            )),
            "{:#?}",
            out.notes
        );
        let with_12 = Enabled {
            vts: BTreeSet::from([2, 12]),
            mode: Mode::Direct,
        };
        let (out, _) = vt_notes("vt-on", &two_heads_on("7 12"), VtsEnabled::On(&with_12));
        assert!(
            !out.notes
                .iter()
                .any(|n| n.contains("sudo vtium console enable")),
            "{:#?}",
            out.notes
        );
    }

    // where the VTs vtium is enabled on were not read, a file whose heads
    // name VTs says so once, and a file whose heads name none says nothing
    // covers: platform.heads.rules
    #[test]
    fn where_the_enabled_vts_were_not_read_the_check_says_so() {
        for (vts, line) in [
            (
                VtsEnabled::FileAlone,
                check_config::VTS_UNCHECKED_FILE_ALONE,
            ),
            (
                VtsEnabled::NoSystemd,
                check_config::VTS_UNCHECKED_NO_SYSTEMD,
            ),
        ] {
            let (out, f) = vt_notes("vt-unread", &two_heads_on("12"), vts);
            assert!(
                out.notes.contains(&format!("{f}: {line}")),
                "{:#?}",
                out.notes
            );
            let text = "version = 1\n[head console]\n[rules]\ndisplay * -> console\n";
            let (out, _) = vt_notes("vt-none", text, vts);
            assert!(
                !out.notes.iter().any(|n| n.contains(line)),
                "{:#?}",
                out.notes
            );
        }
    }

    // the subcommand reads the machine's systemd links unless --no-devices
    // asks it to check the file alone, and the note leaves the status 0
    // covers: platform.heads.rules
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn check_config_reads_the_systemd_links_of_its_host() {
        let (_dir, file, _) = heads_fixture("vt-host", &two_heads_on("12"), 0o644);
        let f = file.display().to_string();
        let root = file.with_file_name("root");
        std::fs::create_dir_all(root.join("run/systemd/system")).unwrap();
        let wants = root.join("etc/systemd/system/getty.target.wants");
        std::fs::create_dir_all(&wants).unwrap();
        let host = Host {
            conf_file: file.with_file_name("installed.conf"),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
            frit_trust: font::trust(),
            systemd_root: root.clone(),
        };
        let off = "Head terminal-1 runs only on VT 12, and vtium is not enabled on VT 12";
        let out = outcome_on(&[f.as_str()], &host);
        assert_eq!(out.status, 0, "{:#?}", out.stderr);
        assert!(
            out.stdout.iter().any(|n| n.contains(off)),
            "{:#?}",
            out.stdout
        );
        let out = outcome_on(&["--no-devices", f.as_str()], &host);
        assert!(
            out.stdout
                .contains(&format!("{f}: {}", check_config::VTS_UNCHECKED_FILE_ALONE)),
            "{:#?}",
            out.stdout
        );
        std::os::unix::fs::symlink(
            "/usr/lib/systemd/system/vtium@.service",
            wants.join("vtium@tty12.service"),
        )
        .unwrap();
        let out = outcome_on(&[f.as_str()], &host);
        assert!(
            !out.stdout.iter().any(|n| n.contains(off)),
            "{:#?}",
            out.stdout
        );
    }

    // ADR-1005 D10: a vtium.conf written for an earlier release binds its
    // keys by their old names. check-config notes each line on standard
    // output, where it does not count, so the file passes, and so does
    // vtium-config's install of it.
    #[test]
    fn an_old_key_binding_name_is_a_note_and_the_check_passes() {
        let (_dir, file, _) = heads_fixture(
            "renamed",
            "grab-paste=<Alt>p\ngrab-reboot=<Ctrl><Alt>Delete\nversion = 1\n[head a]\n[rules]\ndisplay * -> a\ninput path:pci-0000:00:14.0-usb-0:1 -> a\n",
            0o644,
        );
        let f = file.display().to_string();
        let host = Host {
            conf_file: file.with_file_name("installed.conf"),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
            frit_trust: font::trust(),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", f.as_str()]
            .iter()
            .map(OsString::from)
            .collect();
        let out = outcome(&config::load(&args, &mut config::read_bytes), &host);
        assert_eq!((out.status, out.stderr), (0, Vec::<String>::new()));
        for (n, old, new) in [
            (1, "grab-paste", "key-paste"),
            (2, "grab-reboot", "key-reboot"),
        ] {
            let note = format!(
                "{f}:{n}: {old} is now called {new}. vtium reads {old} as {new} for now; rename it"
            );
            assert!(out.stdout.contains(&note), "{note}: {:?}", out.stdout);
        }
    }

    // the installed head file is the host's, read as the start reads it,
    // and the udev daemon's socket is the host's too
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_installed_head_file_and_the_udev_socket_are_the_hosts() {
        let (_dir, file, _) = heads_fixture(
            "host",
            "version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput path:pci-0000:00:14.0-usb-0:1 -> a\n",
            0o644,
        );
        let f = file.display().to_string();
        let host = Host {
            conf_file: file.clone(),
            udev_control: file.with_file_name("no-udev"),
            roots: nowhere(),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
            frit_trust: font::trust(),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        let out = outcome_on(&[], &host);
        assert_eq!(out.status, 1);
        assert_eq!(
            out.stderr,
            [format!(
                "{f}: vtium ignores this file and uses its built-in configuration, because \"{f}\" belongs to user {}, not to root. Run `chown root:root {f}` to make root the owner.",
                rustix::process::geteuid().as_raw()
            ),]
        );
        assert!(
            out.stdout.contains(&format!(
                "{f}: vtium does not read a file it does not trust, so it starts with its built-in configuration and shows `vtium.conf not used: running the built-in configuration.`."
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
                "{c}:5: `path:` matches nothing, because udev is not running"
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
            conf_file: file.with_file_name("absent.conf"),
            ..host
        };
        let out = outcome_on(&[], &gone);
        assert_eq!((out.status, out.stderr), (0, Vec::new()));
        assert!(
            out.stdout
                .iter()
                .any(|n| n.ends_with(
                    "absent.conf: There is no vtium.conf here, so vtium starts with its built-in configuration and shows no notice."
                ))
        );
    }

    // ADR-1401 D7: the installed file named on the command line, by its
    // own path or another, is checked as the start reads it, so the next
    // step a notice names never turns its skipped file into a candidate
    // covers: platform.heads.rules
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_installed_file_named_as_the_candidate_is_checked_as_installed() {
        let (_dir, file, _) = heads_fixture(
            "same",
            "version = 1\n[head a]\n[rules]\ndisplay * -> a\n",
            0o644,
        );
        let link = file.with_file_name("link.conf");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let host = Host {
            conf_file: file.clone(),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
            frit_trust: font::trust(),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        for named in [&file, &link] {
            let out = outcome_on(&[named.to_str().unwrap()], &host);
            assert_eq!(out.status, 1, "{named:?}");
            let n = named.display();
            assert_eq!(
                out.stderr,
                [format!(
                    "{n}: vtium ignores this file and uses its built-in configuration, because \"{n}\" belongs to user {}, not to root. Run `chown root:root {n}` to make root the owner.",
                    rustix::process::geteuid().as_raw()
                )],
                "{named:?}"
            );
        }
    }

    // Each question to the glyph helper starts or waits on a helper, so
    // check-config asks it once for each distinct font set, though
    // font-name and two looks name the same set.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn each_font_set_is_asked_each_question_once() {
        let text = "font-name=desk\nversion = 1\n[look big]\nfont-name = desk\n[look small]\nfont-name = desk\n[head a]\nlook = big\n[head b]\nlook = small\n[rules]\ndisplay * -> a\ndisplay * -> b\n";
        let (_dir, file, _) = heads_fixture("asked-once", text, 0o644);
        let frits = file.with_file_name("frits");
        std::fs::create_dir_all(&frits).unwrap();
        std::fs::write(frits.join("desk.vtfrit"), "source=builtin:unscii-8\n").unwrap();
        let host = Host {
            conf_file: file.with_file_name("absent.conf"),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: frits.clone(),
            frit_trust: font::trust().below(file.parent().unwrap()),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        let (covered, lacked) = (std::cell::Cell::new(0), std::cell::Cell::new(0));
        let cover = |_: &helper::FritText, _: &[u32]| {
            covered.set(covered.get() + 1);
            Ok((Vec::new(), None))
        };
        let sources = |_: &helper::FritText| {
            lacked.set(lacked.get() + 1);
            Ok(String::new())
        };
        let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", file.to_str().unwrap()]
            .iter()
            .map(OsString::from)
            .collect();
        let out = outcome_with(
            &config::load(&args, &mut config::read_bytes),
            &host,
            Some(Squeeze {
                locale: "hi_IN.UTF-8",
                ask: &cover,
            }),
            Some(FontSources { ask: &sources }),
        );
        let squeezed = out.stdout.iter().filter(|l| l.contains("squeezed")).count();
        assert_eq!(squeezed, 2, "a note for each look: {:#?}", out.stdout);
        assert_eq!((covered.get(), lacked.get()), (1, 1));
    }

    // A font that font-name's set needs and this machine lacks is a
    // warning: check-config prints it on standard error and exits with
    // vtium_settings::CHECK_MISSING_FONTS. An
    // older version of a font is a note, which leaves the status alone.
    //
    // References:
    // - ADR-1204 D7: check-config names each missing font.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_missing_font_is_a_warning_and_an_older_one_a_note() {
        let text = "font-name=desk\nversion = 1\n[head a]\n[rules]\ndisplay * -> a\n";
        let (_dir, file, _) = heads_fixture("missing-warns", text, 0o644);
        let frits = file.with_file_name("frits");
        std::fs::create_dir_all(&frits).unwrap();
        std::fs::write(
            frits.join("desk.vtfrit"),
            "source=font:Gone Mono\nsource=font:Old Mono\nversion=3.1\nsource=builtin:unscii-8\n",
        )
        .unwrap();
        let host = Host {
            conf_file: file.with_file_name("absent.conf"),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: frits.clone(),
            frit_trust: font::trust().below(file.parent().unwrap()),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", file.to_str().unwrap()]
            .iter()
            .map(OsString::from)
            .collect();
        let check = |answer: &'static str| {
            let ask = move |_: &helper::FritText| Ok(answer.to_owned());
            outcome_with(
                &config::load(&args, &mut config::read_bytes),
                &host,
                None,
                Some(FontSources { ask: &ask }),
            )
        };
        let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
        let missing = check("missing\t1\tfont:Gone Mono\tno font of family Gone Mono");
        assert_eq!(missing.status, vtium_settings::CHECK_MISSING_FONTS);
        assert_eq!(
            missing.stderr,
            [format!(
                "Font set desk needs the font Gone Mono, and no font of that name is installed. {page}"
            )]
        );
        // a font named by its path is a file this machine lacks
        let file = check("missing\t4\tbdf:/nonexistent/gone.bdf\tnot on this machine");
        assert_eq!(
            file.stderr,
            [format!(
                "Font set desk needs the font /nonexistent/gone.bdf, and there is no file at that path. {page}"
            )]
        );
        // a font this machine has and vtium cannot read is a note, with why
        let refused = check("refused\t1\tfont:Gone Mono\tthe file is not a font");
        assert_eq!(
            (refused.status, refused.stderr.clone()),
            (0, Vec::<String>::new())
        );
        assert!(
            refused.stdout.contains(&"Font set desk: vtium cannot use the font Gone Mono: the file is not a font. It draws that font's characters from the next font that has them.".to_owned()),
            "{:#?}",
            refused.stdout
        );
        let older = check("older\t2\tfont:Old Mono\t3.000");
        assert_eq!((older.status, older.stderr), (0, Vec::<String>::new()));
        assert!(
            older.stdout.contains(&format!(
                "Font set desk asks for the font Old Mono, version 3.1 or later, and the installed font is version 3.000. vtium still uses it. {page}"
            )),
            "{:#?}",
            older.stdout
        );
    }

    // When every problem check-config finds is a font that a font set
    // needs and this machine lacks, it exits with a status of its own, so
    // vtium-config can install the file without reading the warnings'
    // words. The warnings may come from font-name's set or from a look's.
    // Any other problem beside them makes the status 1, and notes alone
    // leave it 0.
    //
    // References:
    // - ADR-1204 D7: check-config names each missing font.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn only_missing_fonts_exit_with_their_own_status() {
        let look = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
        let status = |name: &str, text: &str, answer: &'static str| {
            let (_dir, file, _) = heads_fixture(name, text, 0o644);
            let frits = file.with_file_name("frits");
            std::fs::create_dir_all(&frits).unwrap();
            std::fs::write(
                frits.join("desk.vtfrit"),
                "source=font:Gone Mono\nsource=font:Old Mono\nversion=3.1\nsource=builtin:unscii-8\n",
            )
            .unwrap();
            let host = Host {
                conf_file: file.with_file_name("absent.conf"),
                udev_control: file.clone(),
                roots: nowhere(),
                frits: frits.clone(),
                frit_trust: font::trust().below(file.parent().unwrap()),
                systemd_root: PathBuf::from("/nonexistent/vtium/root"),
            };
            let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", file.to_str().unwrap()]
                .iter()
                .map(OsString::from)
                .collect();
            let ask = move |_: &helper::FritText| Ok(answer.to_owned());
            let out = outcome_with(
                &config::load(&args, &mut config::read_bytes),
                &host,
                None,
                Some(FontSources { ask: &ask }),
            );
            (out.status, out.stderr.len())
        };
        let missing = "missing\t1\tfont:Gone Mono\tno font of family Gone Mono";
        let only = vtium_settings::CHECK_MISSING_FONTS;
        // font-name's set, at the top level
        let top = "font-name=desk\nversion = 1\n[head a]\n[rules]\ndisplay * -> a\n";
        assert_eq!(status("only-top", top, missing), (only, 1));
        // a look's set, among the sections, and both at once
        assert_eq!(status("only-look", look, missing), (only, 1));
        let both = format!("font-name=desk\n{look}");
        assert_eq!(status("only-both", &both, missing), (only, 2));
        // a font that lacks a character its check= names counts the same
        let lacks = "missing\t1\tfont:Gone Mono\tit lacks U+E0B0\nlacks\t1\tfont:Gone Mono\tU+E0B0";
        assert_eq!(status("only-lacks", look, lacks), (only, 1));
        // a missing font beside a top-level warning, or an error in a
        // section, is 1
        let unknown = format!("no-such-key=1\n{both}");
        assert_eq!(status("and-top", &unknown, missing).0, 1);
        let broken = look.replace("look = big\n", "look = big\nno-such-key = 1\n");
        assert_eq!(status("and-section", &broken, missing).0, 1);
        // notes alone: an older font, and a font the helper cannot read
        assert_eq!(
            status("notes", &both, "older\t2\tfont:Old Mono\t3.000"),
            (0, 0)
        );
        let refused = "refused\t1\tfont:Gone Mono\tthe file is not a font";
        assert_eq!(status("refused", &both, refused), (0, 0));
    }

    // heads.md: each file docs/heads holds checks with exit status 0, the
    // frits its looks name installed, away from any machine, but off.conf
    // covers: platform.heads.rules
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn each_documented_head_file_checks_with_exit_0() {
        let docs = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/heads"));
        let (_dir, anchor, _) = heads_fixture("docs", "", 0o644);
        let frits = anchor.with_file_name("frits");
        std::fs::create_dir_all(&frits).unwrap();
        let host = Host {
            conf_file: anchor.with_file_name("absent.conf"),
            udev_control: anchor.clone(),
            roots: nowhere(),
            frits: frits.clone(),
            frit_trust: font::trust().below(anchor.parent().unwrap()),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        let mut files: Vec<PathBuf> = std::fs::read_dir(docs)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "conf"))
            .collect();
        files.sort();
        // the frits the files' looks name are installed
        for f in &files {
            let text = std::fs::read_to_string(f).unwrap();
            let frits_named = text.lines().filter_map(|l| {
                let (key, value) = l.split('#').next()?.split_once('=')?;
                (key.trim() == "font-name").then(|| value.trim().to_owned())
            });
            for p in frits_named {
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
            // off.conf lights nothing and opens nothing on purpose, so it
            // wires no complete route, which check-config refuses
            // (ADR-1403 D3)
            let want = match f.file_name().and_then(|n| n.to_str()) {
                Some("off.conf") => (
                    1,
                    vec![format!(
                        "{}: No display rule sends a display to a head. Add a rule `display MATCH -> TARGET` whose target is a head.",
                        f.display()
                    )],
                ),
                _ => (0, Vec::new()),
            };
            assert_eq!((out.status, out.stderr), want, "{f:?}");
        }
    }

    /// Roots under which nothing is listed.
    fn nowhere() -> Roots {
        Roots {
            sys: PathBuf::from("/nonexistent/vtium/sys"),
            udev_data: PathBuf::from("/nonexistent/vtium/udev"),
        }
    }

    // the devices vtium-platform lists, through the mapping the start uses:
    // each card's displays, each framebuffer's as `fb`, then each input,
    // and a display's EDID as an `edid:` rule matches it
    #[test]
    fn listed_devices_map_as_the_start_maps_them() {
        use vtium_platform::display::{Edid, Identity as Display};
        use vtium_platform::input::{Class as Udev, Classes as UdevClasses};
        use vtium_platform::listing::{Card, Framebuffer, Listing};
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
            connector_path: None,
            lit_at_start: false,
        };
        // an EDID naming maker `AAA`, product 0x0001 and serial number 7
        let named = |connector: &str| {
            let mut base = [0u8; 128];
            base[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
            base[8..16].copy_from_slice(&[0x04, 0x21, 0x01, 0x00, 7, 0, 0, 0]);
            base[127] = 0u8.wrapping_sub(base.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)));
            Display {
                edid: Edid::parse(&base).map(|e| e.id().clone()),
                ..display(connector)
            }
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
                displays: vec![display("DP-2"), named("DP-1")],
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
        let edids: Vec<Option<&vtium_settings::Edid>> =
            devices.iter().map(|d| d.edid.as_ref()).collect();
        let seven = vtium_settings::Edid {
            maker: "AAA".to_owned(),
            product: 1,
            serial: "0x00000007".to_owned(),
        };
        assert_eq!(edids, [None, Some(&seven), None, None, None]);
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
        let (_dir, file, _) = heads_fixture(
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
            conf_file: file.with_file_name("absent.conf"),
            udev_control: file.clone(),
            roots: stand_in_machine(file.parent().unwrap()),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
            frit_trust: font::trust(),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        let out = outcome_on(&[file.to_str().unwrap()], &host);
        assert_eq!((out.status, out.stderr), (0, Vec::new()));
        let heads: Vec<&String> = out.stdout.iter().filter(|n| n.starts_with(&f)).collect();
        assert_eq!(
            heads,
            [
                &format!(
                    "{f}: \"{f}\" belongs to user {}, not to root. That is fine for a check: vtium reads the file once it is installed, owned by root and writable only by root.",
                    rustix::process::geteuid().as_raw()
                ),
                &format!("{f}:8: the rule matches no connected device"),
                &format!("{f}: head desk has only one keyboard, \"Keyboard A\""),
                &format!(
                    "{f}: head panel has mice but no keyboard: every keyboard goes to another head"
                ),
                &format!("{f}: {AS_WRITTEN}"),
                &format!("{f}: {UNSET}"),
            ]
        );
    }

    // a rule noted as matching nothing here for want of a udev daemon, or
    // as unchecked for want of a connector's PATH, takes no second note as
    // idle
    #[test]
    fn a_rule_noted_as_matching_nothing_is_noted_once() {
        let text = "version = 1\n[head desk]\n[rules]\n\
                    display connector-path:mst:63-1 -> desk\n\
                    input path:pci-0000:15:00.0-usb-0:3.3 -> desk\n\
                    input class:joystick -> desk\n";
        let (_dir, file, trust) = heads_fixture("once", text, 0o644);
        let f = file.display().to_string();
        let machine = Machine {
            udev_daemon: false,
            devices: Some(&[]),
            vts: VtsEnabled::FileAlone,
        };
        let out = conf_file(&file, false, &trust, &machine, &caps(4));
        assert_eq!(
            out.notes,
            [
                format!("{f}:5: `path:` matches nothing, because udev is not running"),
                format!(
                    "{f}:4: `connector-path:` is not checked against the connected displays, because check-config does not open the graphics card; vtium matches it when it finds the display"
                ),
                format!("{f}:6: the rule matches no connected device"),
                format!(
                    "{f}: No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated."
                ),
                format!("{f}: {NO_DISPLAY}"),
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
            conf_file(path, true, &Trust::ROOT, &machine(true), &caps(4)).counted,
            ["/tmp/vtium-check-heads-a\\nb\\u{1b}[2J.conf: does not exist"]
        );
    }

    #[test]
    fn help_says_it_only_checks() {
        let (lines, status) = run_with(&["--help"], "");
        assert_eq!(status, 0);
        assert_eq!(lines, [HELP.trim_end()]);
        assert!(HELP.contains("It only reads"));
        assert!(
            HELP.replace('\n', " ")
                .contains("It opens nothing under /dev and changes nothing.")
        );
    }

    // The looks of a file that runs each name their vtlook for the check
    // to read, but for the built-in one, which no file holds.
    #[test]
    fn each_look_s_vtlook_is_listed_for_the_check() {
        let text = "version = 1\n[look warm]\nvtlook = amber\n[look plain]\nvtlook = default\n[look none]\n[head desk]\nlook = warm\n[rules]\ndisplay * -> desk\ninput * -> desk\n";
        let (out, _) = checked("vtlooks", text, true);
        assert_eq!(out.vtlooks, [("warm".to_owned(), "amber".to_owned(), 3)]);
    }

    // A build without frame filters reads no vtlook: each place that names
    // one says which heads draw the built-in vtlook instead, and counts.
    #[cfg(not(feature = "filters"))]
    #[test]
    fn a_build_without_filters_warns_of_each_vtlook_named() {
        let conf = Path::new("/etc/vtium/vtium.conf");
        let looks = [("warm".to_owned(), "amber".to_owned(), 3)];
        let top = Some(("crt", "/etc/vtium/vtium.conf:1".to_owned()));
        assert_eq!(
            vtlook_findings(top, conf, &looks, Path::new("/nonexistent"), &font::trust()),
            [
                "/etc/vtium/vtium.conf:1: This build of vtium has no frame filters, so the heads whose look names no vtlook of its own draw the built-in vtlook instead of vtlook crt.",
                "/etc/vtium/vtium.conf:3: This build of vtium has no frame filters, so the heads that use look warm draw the built-in vtlook instead of vtlook amber.",
            ]
        );
        assert!(
            vtlook_findings(None, conf, &[], Path::new("/nonexistent"), &font::trust()).is_empty()
        );
    }

    // check-config lists every error of every vtlook named, each vtlook
    // read once however many looks name it, each line starting with the
    // vtlook's file and line, and then each place that names the vtlook.
    #[cfg(feature = "filters")]
    #[test]
    fn check_config_lists_every_error_of_each_vtlook_once() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("check-vtlooks");
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
        let write = |name: &str, text: &str| {
            let file = dir.join(format!("{name}.vtlook"));
            let _ = std::fs::write(&file, text);
            let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644));
            file.display().to_string()
        };
        let old = write("old", "# old.vtlook\n[color sepia]\namount = 0.5\n");
        write("plain", "version = 1\n[requires]\ngpu = any\n");
        let conf = Path::new("/etc/vtium/vtium.conf");
        let looks = [
            ("a".to_owned(), "old".to_owned(), 3),
            ("b".to_owned(), "old".to_owned(), 5),
            ("c".to_owned(), "plain".to_owned(), 7),
        ];
        let top = Some(("gone", "argument 2".to_owned()));
        assert_eq!(
            vtlook_findings(top, conf, &looks, &dir, &trust),
            [
                format!(
                    "{}: reading: There is no file gone.vtlook in {}.",
                    dir.join("gone.vtlook").display(),
                    dir.display()
                ),
                "argument 2: The vtlook gone has errors, so the heads whose look names no vtlook of its own draw the built-in vtlook instead.".to_owned(),
                format!(
                    "{old}:2: reading: found [color sepia] before version = 1, needs version = 1 first"
                ),
                format!(
                    "{old}:2: color filter 1 of 1, sepia: checking: found sepia, but this build of vtium has no color filters"
                ),
                "/etc/vtium/vtium.conf:3: The vtlook old has errors, so the heads that use look a draw the built-in vtlook instead.".to_owned(),
                "/etc/vtium/vtium.conf:5: The vtlook old has errors, so the heads that use look b draw the built-in vtlook instead.".to_owned(),
            ]
        );
        assert!(vtlook_findings(None, conf, &looks[2..], &dir, &trust).is_empty());
        // past the bound, the rest of a vtlook's errors are counted
        let many = write("many", &"a\n".repeat(100));
        let said = vtlook_findings(
            None,
            conf,
            &[("m".to_owned(), "many".to_owned(), 9)],
            &dir,
            &trust,
        );
        assert_eq!(said.len(), vtium_settings::ERRORS_MAX + 2, "{said:#?}");
        assert_eq!(
            said.get(vtium_settings::ERRORS_MAX).map(String::as_str),
            Some(format!("{many}: More errors: 37").as_str())
        );
        // a file named default.vtlook is reported though nothing names it
        write("default", "version = 1\n");
        assert_eq!(
            vtlook_findings(None, conf, &looks[2..], &dir, &trust),
            [format!(
                "{}: reading: vtium never reads a file named default.vtlook, because default is the name of the built-in vtlook. Give the file another name.",
                dir.join("default.vtlook").display()
            )]
        );
    }

    // `vtium check-config` reads the vtlook that the top level names and
    // the one each look names, prints what it finds on standard error, and
    // exits 1. The vtlooks are named so that no machine has them, so the
    // check reads the real looks folder and finds neither. A file that
    // fails the ownership check has no top level a start reads, so the
    // check reads no top-level vtlook from it.
    #[test]
    fn check_config_reports_the_vtlooks_vtium_conf_names() {
        let top = "vtium-check-config-test-top";
        let look = "vtium-check-config-test-look";
        let text = format!(
            "vtlook = {top}\nversion = 1\n[look warm]\nvtlook = {look}\n[head a]\nlook = warm\n[rules]\ndisplay * -> a\ninput path:pci-0000:00:14.0-usb-0:1 -> a\n"
        );
        let (_dir, file, _) = heads_fixture("vtlooks-out", &text, 0o644);
        let f = file.display().to_string();
        let host = Host {
            conf_file: file.with_file_name("installed.conf"),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
            frit_trust: font::trust(),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", f.as_str()]
            .iter()
            .map(OsString::from)
            .collect();
        let out = outcome(&config::load(&args, &mut config::read_bytes), &host);
        assert_eq!(out.status, 1, "{:#?}", out.stderr);
        let looks = vtium_settings::vtlook::LOOKS;
        let names_top = if cfg!(feature = "filters") {
            format!(
                "{f}:1: The vtlook {top} has errors, so the heads whose look names no vtlook of its own draw the built-in vtlook instead."
            )
        } else {
            format!(
                "{f}:1: This build of vtium has no frame filters, so the heads whose look names no vtlook of its own draw the built-in vtlook instead of vtlook {top}."
            )
        };
        let names_look = if cfg!(feature = "filters") {
            format!(
                "{f}:4: The vtlook {look} has errors, so the heads that use look warm draw the built-in vtlook instead."
            )
        } else {
            format!(
                "{f}:4: This build of vtium has no frame filters, so the heads that use look warm draw the built-in vtlook instead of vtlook {look}."
            )
        };
        for line in [&names_top, &names_look] {
            assert!(out.stderr.contains(line), "{line}\n{:#?}", out.stderr);
        }
        if cfg!(feature = "filters") {
            for name in [top, look] {
                let read = format!("{looks}/{name}.vtlook: reading: ");
                assert!(
                    out.stderr.iter().any(|l| l.starts_with(&read)),
                    "{read}\n{:#?}",
                    out.stderr
                );
            }
        }
        // an installed file the ownership check refuses: a start reads no
        // top-level vtlook from it, but takes one from the command line
        let (_dir, file, _) = heads_fixture("vtlooks-refused", &text, 0o644);
        let host = Host {
            conf_file: file.clone(),
            ..host
        };
        let installed = |args: &[&str]| {
            let args: Vec<OsString> = [SUBCOMMAND, "--no-devices"]
                .iter()
                .chain(args)
                .map(OsString::from)
                .collect();
            let text = text.clone();
            let loaded = config::load(&args, &mut |p: &Path| {
                Ok((p == Path::new("/etc/vtium/vtium.conf")).then(|| text.as_bytes().to_vec()))
            });
            outcome(&loaded, &host)
        };
        let out = installed(&[]);
        assert_eq!(out.status, 1);
        assert!(
            !out.stderr
                .iter()
                .any(|l| l.contains("vtium-check-config-test")),
            "{:#?}",
            out.stderr
        );
        let out = installed(&["--vtlook=vtium-check-config-test-arg"]);
        assert!(
            out.stderr.iter().any(|l| l.starts_with("argument 3: ")
                && l.contains("vtium-check-config-test-arg")),
            "{:#?}",
            out.stderr
        );
    }

    // A top-level name that cannot name a vtlook, such as a path, is an
    // error of vtium.conf at its line, or of the command-line argument that
    // gives it, and check-config reports it once, in every build.
    //
    // References:
    // - ADR-1304 D5: a vtlook's name, and the top level's `vtlook`.
    // - ADR-1304 D6: check-config reports each error whole, once.
    #[test]
    fn a_top_level_vtlook_that_is_no_name_is_reported_once_where_it_is_given() {
        let text = "vtlook = ../x\nversion = 1\n[head a]\n[rules]\ndisplay * -> a\n";
        let (_dir, file, _) = heads_fixture("vtlook-no-name", text, 0o644);
        let f = file.display().to_string();
        let host = Host {
            conf_file: file.with_file_name("installed.conf"),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: PathBuf::from("/nonexistent/vtium/frits"),
            frit_trust: font::trust(),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        };
        let check = |args: &[&str]| {
            let args: Vec<OsString> = [SUBCOMMAND, "--no-devices"]
                .iter()
                .chain(args)
                .chain([&f.as_str()])
                .map(OsString::from)
                .collect();
            outcome(&config::load(&args, &mut config::read_bytes), &host)
        };
        // every line that names the bad name, so a second report of it in
        // any words, such as a failed read of that vtlook, counts
        let said = |out: &Outcome, name: &str| -> Vec<String> {
            out.stderr
                .iter()
                .filter(|l| l.contains(name))
                .cloned()
                .collect()
        };
        let out = check(&[]);
        assert_eq!(out.status, 1, "{:#?}", out.stderr);
        let reported = said(&out, "../x");
        assert_eq!(reported.len(), 1, "{:#?}", out.stderr);
        assert!(
            reported.iter().all(|l| l.starts_with(&format!("{f}:1: "))),
            "{reported:#?}"
        );
        let out = check(&["--vtlook=a/b"]);
        let reported = said(&out, "a/b");
        assert_eq!(reported.len(), 1, "{:#?}", out.stderr);
        assert!(
            reported.iter().all(|l| l.starts_with("argument 3: ")),
            "{reported:#?}"
        );
    }
}
