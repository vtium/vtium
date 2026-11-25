//! The check of vtium.conf's sections against the machine: the heads, the
//! rules and the devices.

use std::cell::RefCell;
use std::path::Path;

use vtium_settings::{
    Against, AtStart, Config as Heads, Identity, Lockout, Match, Note, Parsed, Problem, Read,
    Rejected, Remark, Runs, Stating, Token, Trust, Untrusted, at,
};

use crate::config::{Config, keys};
#[cfg(doc)]
use crate::font;
use crate::helper;
use crate::notice::check_config;

use super::frits::{DraftFrits, look_frits};
#[cfg(feature = "images")]
use super::notes::look_login_images;
use super::{Machine, VtsEnabled};

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

/// The glyph helpers `vtium check-config` starts: one for each distinct
/// font set, started the first time a question names the set and asked
/// every later question on it, so each start, which loads every font of
/// the set, happens once. Each helper logs none of the frit's warnings,
/// since check-config names each missing font itself.
pub(super) struct Helpers<'a> {
    /// What starts each helper, or why none can start.
    pub(super) launcher: Result<helper::Launcher, String>,
    pub(super) clock: &'a dyn Fn() -> std::time::Instant,
    pub(super) started: RefCell<Vec<(helper::FritText, helper::Helper)>>,
}

impl Helpers<'_> {
    /// Asks the helper of `frit`, started if it is not yet, the
    /// `question`, which waits at most `helper::REQUEST_MS` for its answer.
    pub(super) fn ask<T>(
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
pub(super) fn same_file(a: &Path, b: &Path) -> bool {
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
