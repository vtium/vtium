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

mod resources;
use resources::{
    batch_findings, font_files, font_needs, palette_findings, pointer_findings, vtlook_findings,
};

/// The subcommand's name, which stands among the options.
pub const SUBCOMMAND: &str = "check-config";

/// The subcommand's option that judges the rules alone, without the
/// devices present (ADR-1403 D3).
pub const NO_DEVICES: &str = "--no-devices";

/// The subcommand's option that counts the font set file it names,
/// `NAME.vtfrit`, as the installed font set NAME.
pub const WITH_FRIT: &str = "--with-frit";

/// The subcommand's option that counts the font set it names as not
/// installed.
pub const WITHOUT_FRIT: &str = "--without-frit";

/// The font sets a draft adds, replaces and removes, as the subcommand's
/// [`WITH_FRIT`] and [`WITHOUT_FRIT`] give them. vtium-config keeps a
/// draft's font sets in a folder beside the draft until the install, and
/// passes them here, so the check judges the draft against the frits the
/// install leaves. A frit named here takes the place of the one of its
/// name in the frits' directory, and a removed frit counts as missing.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": the two options, and that the check counts a frit as installed when the directory holds it or `--with-frit` names it, and not when `--without-frit` names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftFrits {
    /// Each frit the draft adds or replaces: its name and its file.
    pub with: Vec<(String, PathBuf)>,
    /// Each installed frit the draft removes, by name.
    pub without: Vec<String>,
}

/// Where the check finds a frit by its name.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Found {
    file: PathBuf,
    /// The file is the draft's, which the install copies into the frits'
    /// directory as root's.
    draft: bool,
}

impl DraftFrits {
    /// Takes `name`'s file as a frit the draft adds or replaces, in place
    /// of any earlier option for that name.
    pub fn add(&mut self, name: String, file: PathBuf) {
        self.forget(&name);
        self.with.push((name, file));
    }

    /// Takes `name` as a frit the draft removes, in place of any earlier
    /// option for that name.
    pub fn remove(&mut self, name: String) {
        self.forget(&name);
        self.without.push(name);
    }

    fn forget(&mut self, name: &str) {
        self.with.retain(|(n, _)| n != name);
        self.without.retain(|n| n != name);
    }

    /// The frit `name` as the check sees it in `frits` with the draft's
    /// changes: the draft's file for a frit it adds or replaces, nothing
    /// for one it removes, and otherwise [`font::frit`]'s lookup.
    fn find(&self, frits: &Path, name: &str) -> Option<Found> {
        if self.without.iter().any(|n| n == name) {
            return None;
        }
        if let Some((_, file)) = self.with.iter().find(|(n, _)| n == name) {
            return Some(Found {
                file: file.clone(),
                draft: true,
            });
        }
        font::frit(frits, name).map(|file| Found { file, draft: false })
    }
}

/// The frit name of the font set file `path` that [`WITH_FRIT`] names:
/// its file name without `.vtfrit`, when that is a name a frit can have
/// and `path` is a regular file. The frits' own lookup decides both, in
/// the folder that holds `path`, so a name the lookup refuses, such as
/// one that reaches outside a folder, never names a draft's frit.
pub fn with_frit_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.strip_suffix(".vtfrit")?;
    let folder = path.parent()?;
    (font::frit(folder, name)? == folder.join(format!("{name}.vtfrit"))).then(|| name.to_owned())
}

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
    let Some((name, Found { file: path, .. })) = named_frit(config, frits, &DraftFrits::default())
    else {
        return Vec::new();
    };
    font::frit_text(&path, trust)
        .map_or_else(|_| Vec::new(), |text| font::frit_warnings(&name, &text))
}

/// The name of the frit `font-name` names in `frits` with the draft's
/// changes `draft`, and where it is; none when the name is no frit's
/// there.
fn named_frit(config: &Config, frits: &Path, draft: &DraftFrits) -> Option<(String, Found)> {
    let wanted = font::wanted(config, "", "").filter(|w| w.came == font::Came::Config)?;
    let found = draft.find(frits, &wanted.name)?;
    Some((wanted.name, found))
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

/// How the frit `found` reads under `trust`, and, where `trust` is not
/// root's, under root's too, as `helper_note` judges the font helper. A
/// draft's frit is read under `trust` alone: the install copies it into
/// the frits' directory as root's, so who owns the draft's file says
/// nothing of the start.
fn judge(found: &Found, trust: &Trust) -> Judged {
    let file = &found.file;
    match font::frit_text(file, trust) {
        Err(why) => Judged::Refused(why),
        Ok(_) if *trust == Trust::ROOT || found.draft => Judged::Read,
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
    draft: &DraftFrits,
) -> (Option<String>, Option<String>) {
    let Some((name, found)) = named_frit(config, frits, draft) else {
        return (None, None);
    };
    let file = &found.file;
    match judge(&found, trust) {
        Judged::Read => (None, None),
        Judged::Refused(why) => (Some(check_config::frit_unreadable(&name, file, &why)), None),
        Judged::RootRefuses(why) => (
            None,
            Some(check_config::frit_refused_as_root(&name, file, &why)),
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
        .chain(sandbox_note(config))
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

/// What check-config notes of each folder of `font-paths`: a folder that
/// does not exist; a folder whose fonts can never be active, because the
/// folder fails the trust rule that vtium, run as root, applies to every
/// font; and a folder that accounts other than its owner cannot read and
/// search. The last matters for a folder that passes the rule, because
/// before anyone logs in the font helper runs as `nobody`. A user's own
/// folder fails the rule on its owner, so its note says how to install
/// one of its fonts as a system font instead.
///
/// References:
/// - ADR-1200 D25: `font-paths` names the folders vtium searches for fonts.
/// - ADR-1205 D4: a font draws only from a folder that passes the trust rule.
fn font_paths_notes(config: &Config) -> Vec<String> {
    let Some(paths) = config.text(&keys::FONT_PATHS) else {
        return Vec::new();
    };
    let mounts = vtium_settings::mounts::Mounts::system().unwrap_or_default();
    paths
        .split(':')
        .filter_map(|dir| font_path_note(Path::new(dir), &mounts))
        .collect()
}

/// What check-config notes of the `font-paths` folder `dir`, judging its
/// filesystem by `mounts` ([`font_paths_notes`]). The folder is judged by
/// [`font::trusted_folder`], the check OSC 50 applies to the folder of a
/// font it is given by path, so check-config notes exactly the folders
/// whose fonts OSC 50 refuses. The rule is root's, because the console
/// runs as root, whoever runs the check.
///
/// References:
/// - ADR-1200 D5, "A qualifying font": OSC 50's fourth check, the trust rule on the font's folder.
fn font_path_note(dir: &Path, mounts: &vtium_settings::mounts::Mounts) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let shown = dir.to_string_lossy();
    let Ok(meta) = std::fs::metadata(dir) else {
        return Some(check_config::font_path_missing(&shown));
    };
    let rule = vtium_settings::cullet::Rule::new(Trust::ROOT);
    if let Err(refused) = font::trusted_folder(dir, rule, Some(mounts.clone())) {
        let why = match &refused.check {
            vtium_settings::cullet::Check::Untrusted(Untrusted::Owner { path, uid }) => {
                check_config::font_path_owner(path, *uid)
            }
            check => check.to_string(),
        };
        return Some(check_config::font_path_untrusted(&shown, &why));
    }
    (meta.permissions().mode() & 0o005 != 0o005).then(|| check_config::font_path_private(&shown))
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

/// What check-config notes when `config` turns the helpers' sandbox off:
/// that the font helper and the image helper run without their seccomp
/// filters and landlock domains, and what that gives up. Nothing when the
/// sandbox is on.
///
/// References:
/// - ADR-1200 D15, "What turning it off gives up": the note's content.
fn sandbox_note(config: &Config) -> Option<String> {
    (!helper::sandbox_on(config)).then(|| check_config::HELPER_SANDBOX_OFF.to_owned())
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
    /// The batch file, the list of active fonts, color schemes, effects and
    /// pointer themes:
    /// [`vtium_settings::batch::BATCH_FILE`], or a scratch file in a test.
    pub batch: PathBuf,
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
            batch: PathBuf::from(vtium_settings::batch::BATCH_FILE),
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
    // the batch a start reads, and the gate each resource file passes
    let places = vtium_settings::batch::Places {
        file: host.batch.clone(),
        ..vtium_settings::batch::Places::system()
    };
    let batch = vtium_settings::batch::Batch::system(&places, &host.frit_trust);
    let (batch_counted, batch_notes) = batch_findings(&batch);
    let gate = vtium_settings::cullet::Gate::new(
        vtium_settings::cullet::Rule::new(host.frit_trust.clone()),
        Some(vtium_settings::mounts::Mounts::system().unwrap_or_default()),
        batch.batch,
    );
    let settings = Settings {
        squeeze,
        sources,
        gate: Some(&gate),
        draft: loaded.draft_frits.clone(),
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
    let (refused, root_refuses) = font_name_frit(
        &loaded.config,
        settings.frits,
        &settings.trust,
        &settings.draft,
    );
    let font_name = named_frit(&loaded.config, settings.frits, &settings.draft);
    let layers = font_name
        .as_ref()
        .map(|(name, found)| frit_layers(name, &found.file, settings.frits, &settings.trust))
        .unwrap_or_default();
    // the fonts the font-name frit needs and this machine lacks: a missing
    // font is a warning, and an older one a note
    let mut needs = font_name
        .as_ref()
        .map(|(name, found)| font_needs(name, &found.file, &settings))
        .unwrap_or_default();
    // the font files the font-name frit names by path that a start would
    // treat as missing; each line names the font set's file and line, so a
    // line a look's check already holds, for a font set the top level and
    // a look share, is said once
    if let Some((_, found)) = &font_name {
        needs.warnings.extend(
            font_files(&found.file, &settings)
                .into_iter()
                .filter(|said| !heads.counted.contains(said)),
        );
    }
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
        &vtium_settings::vtlook::folders(),
        &settings.trust,
        &gate,
    );
    // each color scheme a start would read, read as the start reads it
    let top_palette = loaded
        .config
        .text(&keys::PALETTE)
        .zip(loaded.config.origin(&keys::PALETTE))
        .filter(|(_, origin)| !(heads.refused && matches!(origin, Origin::File { .. })))
        .map(|(name, origin)| (name, origin.to_string()));
    // the pointer file a start would read, read as the start reads it
    let top_pointer = loaded
        .config
        .text(&keys::POINTER_THEME)
        .zip(loaded.config.origin(&keys::POINTER_THEME))
        .filter(|(_, origin)| !(heads.refused && matches!(origin, Origin::File { .. })))
        .map(|(value, origin)| (value, origin.to_string()));
    let pointers = pointer_findings(top_pointer, path, &heads.pointers, &gate);
    let folders = vtium_settings::palette::FOLDERS.map(Path::new);
    let (schemes, scheme_notes) =
        palette_findings(top_palette, path, &heads.palettes, &folders, &gate);
    let stderr: Vec<String> = stderr
        .into_iter()
        .chain(refused)
        .chain(layers)
        .chain(needs.warnings)
        .chain(vtlooks)
        .chain(schemes)
        .chain(pointers)
        .chain(batch_counted)
        .collect();
    let problems = stderr.len().saturating_add(heads.counted.len());
    let status = match problems {
        0 => 0,
        _ if problems == missing_fonts => vtium_settings::CHECK_MISSING_FONTS,
        _ => 1,
    };
    Outcome {
        stderr: stderr.into_iter().chain(heads.counted).collect(),
        stdout: batch_notes
            .into_iter()
            .chain(renamed)
            .chain(notes(&loaded.config))
            .chain(root_refuses)
            .chain(needs.notes)
            .chain(heads.notes)
            .chain(scheme_notes)
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
    /// The palette file each look of the file that a start runs names,
    /// other than a built-in palette: the look's name, the palette's and
    /// the line of its `palette`, for check-config to read.
    pub palettes: Vec<(String, String, usize)>,
    /// The pointer file each look of the file that a start runs names:
    /// the look's name, the file's path and the line of its
    /// `pointer-theme`, for check-config to read.
    pub pointers: Vec<(String, String, usize)>,
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
    /// The trust rule, the mount table and the batch that a font file a
    /// font set names by its path passes; `None` checks no font file.
    pub gate: Option<&'a vtium_settings::cullet::Gate>,
    /// The font sets the draft adds, replaces and removes, which a look's
    /// frit is looked up with.
    pub draft: DraftFrits,
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
            gate: None,
            draft: DraftFrits::default(),
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
        let Some(found) = settings.draft.find(settings.frits, &named.value) else {
            continue;
        };
        let line = Some(named.line);
        let judged = judge(&found, &settings.trust);
        let file = found.file;
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
            // each line names its font set's file and line, so a font set
            // that several looks use is said once
            for said in font_files(&file, settings) {
                if !out.counted.contains(&said) {
                    out.missing_fonts = out.missing_fonts.saturating_add(1);
                    out.counted.push(said);
                }
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
    let has_frit = |name: &str| settings.draft.find(settings.frits, name).is_some();
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
        out.palettes = parsed
            .config
            .looks
            .iter()
            .filter_map(|l| {
                let s = l
                    .settings
                    .iter()
                    .find(|s| s.key.name == keys::PALETTE.name)?;
                let file = !vtium_settings::palette::built_in(&s.value);
                file.then(|| (l.name.clone(), s.value.clone(), s.line))
            })
            .collect();
        out.pointers = parsed
            .config
            .looks
            .iter()
            .filter_map(|l| {
                let s = l
                    .settings
                    .iter()
                    .find(|s| s.key.name == keys::POINTER_THEME.name)?;
                Some((l.name.clone(), s.value.clone(), s.line))
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
mod tests;
