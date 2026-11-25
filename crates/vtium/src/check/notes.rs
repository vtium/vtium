//! The warnings and notes that check-config prints beside its reports.

use std::path::Path;

#[cfg(feature = "images")]
use vtium_settings::{Parsed, at};
use vtium_settings::{Trust, Untrusted};

use crate::config::{Config, Loaded, Origin, keys};
use crate::font;
use crate::helper;
use crate::notice::check_config;

#[cfg(feature = "images")]
use super::conf::HeadCheck;
use super::frits::frit;

/// What a value means beyond its own key, which the start reports too.
pub fn semantics(config: &Config) -> Vec<String> {
    // Any user may read PID 1's status; `SigCgt` says what it catches.
    let status = std::fs::read_to_string("/proc/1/status").map_err(|e| e.to_string());
    let mut out = semantics_under(config, &status, vtium_platform::power::power_off_signal());
    // read as the start reads it, for the account vtium runs as
    out.extend(frit(config, &font::frits(config), &font::trust()));
    out
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
pub(super) fn semantics_under(
    config: &Config,
    status: &Result<String, String>,
    signal: i32,
) -> Vec<String> {
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
pub(super) fn counts(r: &crate::config::Report) -> bool {
    !matches!(r.problem, crate::config::Problem::Renamed { .. })
}

/// [`warnings`], with only the reports `keep` keeps.
pub(super) fn warnings_of(
    loaded: &Loaded,
    keep: impl Fn(&crate::config::Report) -> bool,
) -> Vec<String> {
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
pub(super) fn look_login_images(out: &mut HeadCheck, path: &Path, parsed: &Parsed, me: u32) {
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
pub(super) fn font_path_note(
    dir: &Path,
    mounts: &vtium_settings::mounts::Mounts,
) -> Option<String> {
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
pub(super) fn helper_note(program: &Path, me: u32) -> Option<String> {
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
pub(super) fn image_helper_note(program: &Path, me: u32) -> Option<String> {
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
