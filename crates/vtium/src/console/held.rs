//! Which VTs a unit enabled at boot holds, judged from what is on the
//! machine (ADR-1500 D7): the `getty@ttyN.service` its unit names in
//! `Conflicts=`, the TTY its unit runs on, the VT a display manager's own
//! configuration starts its greeter on, and the VT its greeter runs on now.
//!
//! A `Conflicts=getty@ttyN.service` line only says that the unit stops that
//! VT's getty. Arch's sddm.service has `Conflicts=getty@tty1.service`
//! whatever VT SDDM ends up on, and an SDDM with an X11 greeter takes the
//! first VT no program has open, so its greeter can run on VT 2. So the
//! line holds VT N only while nothing says where the program goes: its
//! configuration naming another VT, or its greeter running on another VT
//! now while its configuration names none, frees VT N. When the
//! configuration and the running greeter disagree, or a configuration file
//! cannot be read, vtium cannot tell, and every VT either names counts as
//! held.
//!
//! vtium reads the display managers' configuration files and never runs
//! them, as each reads them in its own source: [`sddm`], [`lightdm`] and
//! [`greetd`]. GDM has no setting for its greeter's VT: its first greeter
//! always takes its built-in first VT, `GDM_INITIAL_VT`
//! (`daemon/gdm-session-worker.c`), and its unit names that VT's getty in
//! `Conflicts=` (`data/gdm.service.in`), so that getty's VT is GDM's
//! exactly, and a greeter running elsewhere frees nothing.

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

/// What a display manager's configuration says about its greeter's VT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Config {
    /// The program is no display manager whose configuration vtium knows.
    Unknown,
    /// The configuration names no VT; why, in words. `also` is a setting
    /// that holds a VT and frees none, as SDDM's `MinimumVT=`, which SDDM
    /// 0.20 and later ignore.
    NoVt { why: String, also: Option<Setting> },
    /// The configuration starts the greeter on a VT.
    Vt(Setting),
    /// A configuration file could not be read.
    Unreadable { path: String, why: String },
}

/// A display manager's setting that starts its greeter on a VT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
    /// The setting and where it is: `minimum-vt=7 in /etc/lightdm/lightdm.conf`.
    pub source: String,
    /// What it does, in words.
    pub effect: String,
    pub vt: u16,
    /// Whether the greeter takes exactly that VT, or that VT or a higher one.
    pub exact: bool,
}

impl Setting {
    /// Whether a greeter running on VT `g` agrees with the setting.
    fn agrees(&self, g: u16) -> bool {
        if self.exact {
            g == self.vt
        } else {
            g >= self.vt
        }
    }

    /// Whether the setting keeps the greeter off VT `n`.
    fn rules_out(&self, n: u16) -> bool {
        if self.exact {
            n != self.vt
        } else {
            n < self.vt
        }
    }
}

/// Why a unit holds a VT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Its configuration starts its greeter there.
    Configured(Setting),
    /// Its unit runs on the VT's TTY: `TTYPath=/dev/ttyN`.
    TtyPath,
    /// It is an instance on the VT, `PREFIX@ttyN.service`.
    Instance,
    /// Its unit names the VT's getty in `Conflicts=`, and nothing says
    /// which VT it takes.
    Unsure(Unsure),
    /// Its configuration names one VT and its greeter runs on another.
    Disagree { setting: Setting, greeter: u16 },
}

/// Why vtium cannot tell which VT a unit takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unsure {
    /// Its configuration names no VT, for this reason, and no greeter of
    /// its runs now.
    NoVt(String),
    /// A configuration file could not be read.
    Unreadable { path: String, why: String },
    /// vtium knows no configuration of the program's, and no greeter of
    /// its runs now.
    Unknown,
}

/// Why a VT whose getty a unit names in `Conflicts=` is not the unit's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Freed {
    /// Its configuration starts its greeter elsewhere.
    Configured(Setting),
    /// Its greeter runs on these VTs now, and its configuration names
    /// none, for this reason.
    Greeter { vts: BTreeSet<u16>, config: String },
}

/// What vtium read of one unit enabled at boot.
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

impl Facts {
    /// The one VT whose getty the unit's `Conflicts=` names, which a
    /// display manager's unit names for its built-in first VT.
    fn getty(&self) -> Option<u16> {
        match self.conflicts.iter().collect::<Vec<_>>()[..] {
            [&n] => Some(n),
            _ => None,
        }
    }
}

/// The VTs a unit holds, and the VTs it frees, each with its reason.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Judgment {
    pub held: BTreeMap<u16, Reason>,
    pub freed: BTreeMap<u16, Freed>,
}

/// Judges which VTs a unit holds at boot, from what its unit file says,
/// what its configuration says, and the VTs its greeters run on now.
pub fn judge(facts: &Facts, config: &Config, greeters: &BTreeSet<u16>) -> Judgment {
    let mut j = Judgment::default();
    if let Some(n) = facts.instance {
        j.held.insert(n, Reason::Instance);
    }
    for &n in &facts.tty_path {
        j.held.entry(n).or_insert(Reason::TtyPath);
    }
    match config {
        Config::Vt(setting) => {
            if let Some(&g) = greeters.iter().find(|&&g| !setting.agrees(g)) {
                let disagree = Reason::Disagree {
                    setting: setting.clone(),
                    greeter: g,
                };
                let vts = facts
                    .conflicts
                    .iter()
                    .chain(greeters)
                    .chain(std::iter::once(&setting.vt));
                for &n in vts {
                    j.held.entry(n).or_insert_with(|| disagree.clone());
                }
                return j;
            }
            j.held
                .entry(setting.vt)
                .or_insert_with(|| Reason::Configured(setting.clone()));
            for &n in &facts.conflicts {
                if j.held.contains_key(&n) {
                    continue;
                }
                if setting.rules_out(n) {
                    j.freed.insert(n, Freed::Configured(setting.clone()));
                } else {
                    // A setting that names the lowest VT leaves VT n open.
                    j.held.insert(n, Reason::Configured(setting.clone()));
                }
            }
        }
        Config::Unreadable { path, why } => {
            for &n in &facts.conflicts {
                j.held.entry(n).or_insert_with(|| {
                    Reason::Unsure(Unsure::Unreadable {
                        path: path.clone(),
                        why: why.clone(),
                    })
                });
            }
        }
        Config::NoVt { .. } | Config::Unknown => {
            if let Config::NoVt {
                also: Some(setting),
                ..
            } = config
            {
                j.held
                    .entry(setting.vt)
                    .or_insert_with(|| Reason::Configured(setting.clone()));
            }
            for &n in &facts.conflicts {
                if j.held.contains_key(&n) || greeters.contains(&n) {
                    continue;
                }
                if !greeters.is_empty() {
                    let config = match config {
                        Config::NoVt { why, .. } => why.clone(),
                        _ => String::new(),
                    };
                    j.freed.insert(
                        n,
                        Freed::Greeter {
                            vts: greeters.clone(),
                            config,
                        },
                    );
                } else {
                    j.held.insert(
                        n,
                        Reason::Unsure(match config {
                            Config::NoVt { why, .. } => Unsure::NoVt(why.clone()),
                            _ => Unsure::Unknown,
                        }),
                    );
                }
            }
        }
    }
    j
}

/// The program a unit runs, by its name: `sddm` for `sddm.service`, `gdm`
/// for Debian's `gdm3.service`, `cage` for `cage@tty1.service`.
pub fn stem(unit: &str) -> &str {
    let name = unit.strip_suffix(".service").unwrap_or(unit);
    let name = name.split_once('@').map_or(name, |(p, _)| p);
    name.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// Whether a logind session's PAM service, such as `sddm-greeter` or
/// `gdm-launch-environment`, is the program `stem`'s: the stem itself, or
/// the stem and a dash before more.
pub fn service_of(service: &str, stem: &str) -> bool {
    !stem.is_empty()
        && service
            .strip_prefix(stem)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
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

/// A file's text, `None` when it is missing, and the path and the reason
/// when it is there but cannot be read.
fn read(root: &Path, file: &str) -> Result<Option<String>, Config> {
    match std::fs::read_to_string(root.join(file)) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Config::Unreadable {
            path: format!("/{file}"),
            why: e.to_string(),
        }),
    }
}

/// What the display manager `unit`'s configuration under `root` says about
/// its greeter's VT, with what its unit file says, `facts`.
pub fn config(root: &Path, unit: &str, facts: &Facts) -> Config {
    match stem(unit) {
        "sddm" => sddm::config(root, facts.getty()),
        "lightdm" => lightdm::config(root),
        "greetd" => greetd::config(root, unit, facts.exec_start.as_deref()),
        "gdm" => match facts.getty() {
            Some(vt) => Config::Vt(Setting {
                source: super::text::gdm::source(unit, vt),
                effect: super::text::gdm::effect(vt),
                vt,
                exact: true,
            }),
            None => Config::NoVt {
                why: super::text::gdm::NO_VT.to_owned(),
                also: None,
            },
        },
        _ => Config::Unknown,
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "a test's set-up fails loudly")]

    use super::*;

    fn setting(vt: u16, exact: bool) -> Setting {
        Setting {
            source: "s".into(),
            effect: "e".into(),
            vt,
            exact,
        }
    }

    fn facts(conflicts: &[u16]) -> Facts {
        Facts {
            conflicts: conflicts.iter().copied().collect(),
            ..Facts::default()
        }
    }

    #[test]
    fn a_conflicts_line_alone_holds_its_vt() {
        let j = judge(&facts(&[1]), &Config::Unknown, &BTreeSet::new());
        assert_eq!(j.held.get(&1), Some(&Reason::Unsure(Unsure::Unknown)));
        assert!(j.freed.is_empty());
    }

    #[test]
    fn a_greeter_on_another_vt_frees_the_conflicts_vt() {
        let no_vt = Config::NoVt {
            why: "x11".into(),
            also: None,
        };
        let j = judge(&facts(&[1]), &no_vt, &BTreeSet::from([2]));
        assert!(j.held.is_empty(), "{j:?}");
        assert!(matches!(j.freed.get(&1), Some(Freed::Greeter { .. })));
        // A greeter on the conflicts VT leaves it held by the session, not
        // freed.
        let j = judge(&facts(&[1]), &no_vt, &BTreeSet::from([1]));
        assert!(j.freed.is_empty() && j.held.is_empty(), "{j:?}");
    }

    #[test]
    fn a_configured_vt_frees_the_vts_it_rules_out_and_holds_its_own() {
        let j = judge(
            &facts(&[1]),
            &Config::Vt(setting(7, false)),
            &BTreeSet::new(),
        );
        assert!(matches!(j.held.get(&7), Some(Reason::Configured(_))));
        assert!(matches!(j.freed.get(&1), Some(Freed::Configured(_))));
        // The lowest VT leaves a VT above it open.
        let j = judge(
            &facts(&[3]),
            &Config::Vt(setting(2, false)),
            &BTreeSet::new(),
        );
        assert!(j.held.contains_key(&3) && j.held.contains_key(&2), "{j:?}");
    }

    #[test]
    fn a_configuration_and_a_greeter_that_disagree_hold_every_vt_they_name() {
        let j = judge(
            &facts(&[1]),
            &Config::Vt(setting(2, true)),
            &BTreeSet::from([3]),
        );
        for n in [1, 2, 3] {
            assert!(
                matches!(j.held.get(&n), Some(Reason::Disagree { greeter: 3, .. })),
                "{n}: {j:?}"
            );
        }
        assert!(j.freed.is_empty());
    }

    #[test]
    fn a_setting_that_frees_nothing_holds_its_vt_beside_the_conflicts_one() {
        // SDDM's MinimumVT=3, which SDDM 0.20 and later ignore.
        let config = Config::NoVt {
            why: "x11".into(),
            also: Some(setting(3, false)),
        };
        let j = judge(&facts(&[1]), &config, &BTreeSet::new());
        assert!(
            matches!(j.held.get(&3), Some(Reason::Configured(_))),
            "{j:?}"
        );
        assert!(matches!(j.held.get(&1), Some(Reason::Unsure(_))), "{j:?}");
        assert!(j.freed.is_empty());
    }

    #[test]
    fn gdm_holds_the_vt_its_unit_names_whatever_greeter_runs_elsewhere() {
        let r = root("gdm", &[]);
        let gdm = config(&r, "gdm.service", &facts(&[1]));
        assert!(matches!(
            gdm,
            Config::Vt(Setting {
                vt: 1,
                exact: true,
                ..
            })
        ));
        // A greeter of GDM's on VT 2 does not free VT 1.
        let j = judge(&facts(&[1]), &gdm, &BTreeSet::from([2]));
        assert!(j.held.contains_key(&1) && j.freed.is_empty(), "{j:?}");
        assert!(matches!(
            config(&r, "gdm3.service", &facts(&[])),
            Config::NoVt { .. }
        ));
        assert_eq!(
            config(&r, "cage@tty1.service", &facts(&[1])),
            Config::Unknown
        );
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn an_unreadable_configuration_holds_the_conflicts_vt_whatever_runs() {
        let config = Config::Unreadable {
            path: "/etc/sddm.conf".into(),
            why: "denied".into(),
        };
        let j = judge(&facts(&[1]), &config, &BTreeSet::from([2]));
        assert!(matches!(
            j.held.get(&1),
            Some(Reason::Unsure(Unsure::Unreadable { .. }))
        ));
    }

    #[test]
    fn a_units_program_and_its_sessions_are_known_by_its_stem() {
        assert_eq!(stem("sddm.service"), "sddm");
        assert_eq!(stem("gdm3.service"), "gdm");
        assert_eq!(stem("cage@tty1.service"), "cage");
        assert!(service_of("gdm-launch-environment", stem("gdm3.service")));
        assert!(service_of("greetd", "greetd"));
        assert!(!service_of("lightdm-greeter", "sddm"));
        assert!(!service_of("sddmx-greeter", "sddm"));
        assert!(!service_of("anything", ""));
    }

    /// A scratch root with `files` written under it, each path relative to
    /// it.
    pub fn root(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("vtium-held-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root
    }
}
