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

use vtium_settings::{Identity, Trust};

use vtium_devices::{SEAT, UDEV_CONTROL, devices, udev_daemon_runs};
use vtium_platform::listing::{self, Roots};

use crate::config::{Config, Loaded, Origin, keys};
use crate::font;
use crate::helper;
use crate::notice::check_config;

mod conf;
#[cfg(feature = "split")]
mod drawer;
mod frits;
mod notes;
mod resources;
use resources::{
    batch_findings, font_files, font_needs, palette_findings, pointer_findings, vtlook_findings,
};

pub use conf::{
    AskCoverage, FontSources, HeadCheck, Settings, Squeeze, conf_file, device_findings,
};
use conf::{Helpers, same_file};
#[cfg(feature = "split")]
pub use drawer::Reader;
pub use frits::{CONSOLE_FONT_FOLDERS, DraftFrits, with_frit_name};
use frits::{font_name_frit, font_name_not_found, frit_layers, named_frit};
use notes::{counts, warnings_of};
pub use notes::{notes, semantics, warnings};

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

/// The subcommand's help.
pub const HELP: &str = check_config::HELP;

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
    /// The folders where the font helper finds a kbd console font by its
    /// name, which a `font-name` that names no font set is looked up in:
    /// [`CONSOLE_FONT_FOLDERS`], or scratch folders in a test.
    pub console_fonts: Vec<PathBuf>,
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
            console_fonts: CONSOLE_FONT_FOLDERS.iter().map(PathBuf::from).collect(),
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
        console_fonts: host.console_fonts.clone(),
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
    // a top-level font-name that gives no font vtium loads, which a start
    // reads only from a file it trusts or from the command line
    let not_found = font_name_not_found(&loaded.config, &settings).filter(|_| {
        !(heads.refused
            && matches!(
                loaded.config.origin(&keys::FONT_NAME),
                Some(Origin::File { .. })
            ))
    });
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
        .chain(not_found)
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

/// The warning when `reader`, the account of vtium's drawing process,
/// cannot read the vtium.conf that `loaded` checks on the machine `host`:
/// the installed file, judged with each folder above it, or a candidate,
/// judged by its own owner, group and mode.
///
/// References:
/// - ADR-1400 D4, "vtium.conf under the split": check-config warns when
///   the drawer's account cannot read vtium.conf.
#[cfg(feature = "split")]
pub fn unreadable(loaded: &Loaded, host: &Host, reader: &Reader) -> Option<String> {
    let path = loaded.candidate.as_deref().unwrap_or(&host.conf_file);
    let candidate = loaded.candidate.is_some() && !same_file(path, &host.conf_file);
    drawer::finding(path, !candidate, reader)
}

/// Adds [`unreadable`]'s warning to `out`, the outcome of `loaded` on
/// `host`, as a line on standard error that makes the status 1. The help
/// and a machine without the account `drawer` get none.
#[cfg(feature = "split")]
pub fn warn_unreadable(out: &mut Outcome, loaded: &Loaded, host: &Host, drawer: Option<&Reader>) {
    if loaded.config.switch(&keys::HELP) {
        return;
    }
    if let Some(line) = drawer.and_then(|reader| unreadable(loaded, host, reader)) {
        out.stderr.push(line);
        out.status = 1;
    }
}

/// The subcommand as the binary runs it on the machine's own files: it
/// prints [`outcome`] and returns the exit status. It asks the glyph
/// helper which of the locale's everyday letters each look's font set
/// covers, and which fonts each font set lacks, waiting by `clock`. In a
/// build with `split`, it also warns when `drawer`, the account of vtium's
/// drawing process, cannot read vtium.conf (`unreadable`); `drawer` is
/// `None` on a machine without the account, where vtium runs as one
/// process.
pub fn main(
    loaded: &Loaded,
    clock: &dyn Fn() -> std::time::Instant,
    #[cfg(feature = "split")] drawer: Option<Reader>,
) -> u8 {
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
    #[cfg(not(feature = "split"))]
    let out = outcome_with(
        loaded,
        &Host::system(&loaded.config),
        Some(squeeze),
        Some(sources),
    );
    #[cfg(feature = "split")]
    let out = {
        let host = Host::system(&loaded.config);
        let mut out = outcome_with(loaded, &host, Some(squeeze), Some(sources));
        warn_unreadable(&mut out, loaded, &host, drawer.as_ref());
        out
    };
    for line in &out.stderr {
        eprintln!("{line}");
    }
    for line in &out.stdout {
        println!("{line}");
    }
    out.status
}

#[cfg(test)]
mod lists_tests;
#[cfg(test)]
mod tests;
