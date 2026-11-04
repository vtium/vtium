//! Which VTs the system's intended configuration gives to a unit that
//! starts at boot (ADR-1500 D7), by common convention: a display manager
//! that is enabled keeps VT 1, where systemd and the display managers' own
//! units put it, whatever VT it happens to use now; a unit holds each VT
//! whose `getty@ttyN.service` its `Conflicts=` names, the VT its `TTYPath=`
//! names and the VT it is an instance on; and a display manager's own
//! configuration adds the VT it starts its greeter on. Nothing frees a VT:
//! neither a configuration file nor a greeter running elsewhere.
//!
//! vtium reads the display managers' configuration files and never runs
//! them, as each reads them in its own source: [`sddm`], [`lightdm`] and
//! [`greetd`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::LAST_VT;

mod greetd;
mod lightdm;
mod sddm;

/// What holds a VT, so that `vtium console` leaves it alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Holder {
    /// A graphical session or greeter on the VT now.
    Session {
        user: String,
        service: String,
        /// The session's type, or `greeter` for a greeter.
        kind: String,
    },
    /// A unit enabled at boot that claims the VT, such as the display
    /// manager's.
    Unit {
        unit: String,
        /// Whether `display-manager.service` names it.
        display_manager: bool,
        reason: Reason,
    },
    /// Another console running on the VT now, `PREFIX@ttyN.service`.
    Console { unit: String },
}

/// A display manager's setting that starts its greeter on a VT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
    /// The setting and where it is: `minimum-vt=7 in /etc/lightdm/lightdm.conf`.
    pub source: String,
    /// What it does, in words.
    pub effect: String,
    pub vt: u16,
}

/// Why a unit holds a VT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A display manager is enabled, and by convention VT 1 stays its own
    /// and its getty's.
    Convention,
    /// Its unit names the VT's getty in `Conflicts=`.
    Conflicts,
    /// Its unit runs on the VT's TTY: `TTYPath=/dev/ttyN`.
    TtyPath,
    /// It is an instance on the VT, `PREFIX@ttyN.service`.
    Instance,
    /// Its configuration starts its greeter there.
    Configured(Setting),
}

/// What vtium read of one unit that starts at boot.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// The VTs whose getty its `Conflicts=` names.
    pub conflicts: BTreeSet<u16>,
    /// The VTs its `TTYPath=` names.
    pub tty_path: BTreeSet<u16>,
    /// The VT it is an instance on, `PREFIX@ttyN.service`.
    pub instance: Option<u16>,
    /// Its `[Service]` section's last `ExecStart=` command line.
    pub exec_start: Option<String>,
}

/// The VTs a unit holds at boot, each with its reason: `display_manager`
/// when `display-manager.service` names it, and `setting` the VT its
/// configuration starts its greeter on. A VT keeps the first reason found,
/// in the order the unit ties itself to it, then convention, then
/// configuration.
pub fn judge(
    facts: &Facts,
    display_manager: bool,
    setting: Option<&Setting>,
) -> BTreeMap<u16, Reason> {
    let mut held = BTreeMap::new();
    if let Some(n) = facts.instance {
        held.insert(n, Reason::Instance);
    }
    for &n in &facts.tty_path {
        held.entry(n).or_insert(Reason::TtyPath);
    }
    if display_manager {
        held.entry(1).or_insert(Reason::Convention);
    }
    for &n in &facts.conflicts {
        held.entry(n).or_insert(Reason::Conflicts);
    }
    if let Some(setting) = setting {
        held.entry(setting.vt)
            .or_insert_with(|| Reason::Configured(setting.clone()));
    }
    held
}

/// The program a unit runs, by its name: `sddm` for `sddm.service`, `gdm`
/// for Debian's `gdm3.service`, `cage` for `cage@tty1.service`.
pub fn stem(unit: &str) -> &str {
    let name = unit.strip_suffix(".service").unwrap_or(unit);
    let name = name.split_once('@').map_or(name, |(p, _)| p);
    name.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// The display managers vtium knows by name, which may be enabled without
/// the `display-manager.service` alias: ly ships `ly@.service`, wanted by
/// `multi-user.target` for its `DefaultInstance=tty2`, with no such alias.
const DISPLAY_MANAGERS: [&str; 10] = [
    "sddm", "gdm", "lightdm", "greetd", "ly", "lemurs", "lxdm", "xdm", "slim", "emptty",
];

/// Whether `unit` runs a display manager vtium knows by name.
pub fn is_display_manager(unit: &str) -> bool {
    DISPLAY_MANAGERS.contains(&stem(unit))
}

/// A VT's number from a setting, when it is one from 1 to [`LAST_VT`].
fn vt(n: i64) -> Option<u16> {
    u16::try_from(n).ok().filter(|n| (1..=LAST_VT).contains(n))
}

/// The files a program reads its configuration from, in the order it reads
/// them, a later value winning: each relative to the root.
fn files(root: &Path, dirs: &[&str], suffix: Option<&str>, last: &str) -> Vec<String> {
    let mut out = Vec::new();
    for dir in dirs {
        let mut names: Vec<String> = std::fs::read_dir(root.join(dir))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().to_str().map(str::to_owned))
                    .filter(|n| suffix.is_none_or(|s| n.ends_with(s)))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        out.extend(names.into_iter().map(|n| format!("{dir}/{n}")));
    }
    out.push(last.to_owned());
    out
}

/// A file's text, `None` when it is missing or cannot be read: a setting
/// vtium cannot read adds no VT, and frees none, since none ever frees one.
fn read(root: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(root.join(file)).ok()
}

/// The VT the display manager `unit`'s configuration under `root` starts
/// its greeter on, when it names one, with what its unit file says,
/// `facts`.
pub fn config(root: &Path, unit: &str, facts: &Facts) -> Option<Setting> {
    match stem(unit) {
        "sddm" => sddm::config(root),
        "lightdm" => lightdm::config(root),
        "greetd" => greetd::config(root, unit, facts.exec_start.as_deref()),
        _ => None,
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A scratch root with `files` written under it, each path relative to
    /// it.
    pub fn root(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("vtium-held-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let path = root.join(path);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            std::fs::write(path, text).ok();
        }
        root
    }

    fn facts(conflicts: &[u16]) -> Facts {
        Facts {
            conflicts: conflicts.iter().copied().collect(),
            ..Facts::default()
        }
    }

    fn setting(vt: u16) -> Setting {
        Setting {
            source: "s".into(),
            effect: "e".into(),
            vt,
        }
    }

    #[test]
    fn a_display_manager_keeps_vt_1_whatever_its_unit_names() {
        // Debian's lightdm.service names no getty, and LightDM starts its
        // greeter on VT 7: VT 1 is still the display manager's.
        let held = judge(&facts(&[]), true, Some(&setting(7)));
        assert_eq!(held.get(&1), Some(&Reason::Convention));
        assert!(matches!(held.get(&7), Some(Reason::Configured(_))));
        // Another unit that names getty@tty1 holds it too, by its line.
        let held = judge(&facts(&[1]), false, None);
        assert_eq!(held.get(&1), Some(&Reason::Conflicts));
        assert!(judge(&facts(&[]), false, None).is_empty());
    }

    #[test]
    fn a_setting_adds_a_vt_and_frees_none() {
        let held = judge(&facts(&[1, 2]), true, Some(&setting(2)));
        assert_eq!(
            held.into_iter().collect::<Vec<_>>(),
            [(1, Reason::Convention), (2, Reason::Conflicts)]
        );
    }

    #[test]
    fn a_units_program_is_known_by_its_stem() {
        assert_eq!(stem("sddm.service"), "sddm");
        assert_eq!(stem("gdm3.service"), "gdm");
        assert_eq!(stem("cage@tty1.service"), "cage");
        assert!(is_display_manager("ly@tty2.service"));
        assert!(is_display_manager("gdm3.service"));
        assert!(!is_display_manager("cage@tty1.service"));
        assert!(!is_display_manager("lyrics.service"));
    }
}
