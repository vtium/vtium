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
//! now, frees VT N. When the configuration and the running greeter
//! disagree, or a configuration file cannot be read, vtium cannot tell, and
//! every VT either names counts as held.
//!
//! vtium reads the display managers' configuration files and never runs
//! them. What each display manager does with its settings is taken from its
//! own source:
//!
//! - SDDM 0.21 (`src/daemon/Display.cpp`, `src/common/ConfigReader.cpp`)
//!   reads every file in `/usr/lib/sddm/sddm.conf.d`, then in
//!   `/etc/sddm.conf.d`, each in name order, then `/etc/sddm.conf`, a later
//!   value winning. With `[General] DisplayServer=x11`, the default, its
//!   greeter takes the first VT no program has open (`VT_OPENQRY`), which
//!   no file names. With `wayland` or `x11-user`, it takes its built-in
//!   first VT, 1 (`SDDM_INITIAL_VT`), unless a login session is on it.
//!   `MinimumVT=`, which SDDM before 0.20 read and sddm.conf(5) says is
//!   gone since 0.20, is read as the lowest VT the greeter takes.
//! - LightDM (`common/configuration.c`, `src/vt.c`) reads
//!   `lightdm/lightdm.conf.d/*.conf` under `/usr/share`, `/usr/local/share`
//!   and `/etc/xdg`, then `/etc/lightdm/lightdm.conf.d/*.conf`, then
//!   `/etc/lightdm/lightdm.conf`. Its first greeter takes `[LightDM]
//!   minimum-vt`, 7 when no file sets it, and 1 for a value below 1.
//! - greetd reads `/etc/greetd/config.toml`, whose `[terminal] vt` is a
//!   VT's number, or `"next"`, `"current"` or `"none"`, which name none.
//! - GDM has no setting for its greeter's VT.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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
    /// The configuration names no VT; why, in words.
    NoVt(String),
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
        Config::NoVt(_) | Config::Unknown => {
            for &n in &facts.conflicts {
                if j.held.contains_key(&n) || greeters.contains(&n) {
                    continue;
                }
                if !greeters.is_empty() {
                    let config = match config {
                        Config::NoVt(why) => why.clone(),
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
                            Config::NoVt(why) => Unsure::NoVt(why.clone()),
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
/// `gdm-launch-environment`, is the program `stem`'s.
pub fn service_of(service: &str, stem: &str) -> bool {
    !stem.is_empty() && service.starts_with(stem)
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

/// A setting's value and the file that set it, or a file that cannot be
/// read and why.
type Found = (String, String);

/// A key's last value in `[section]` of INI files and the file that set
/// it, read in order; `Err` names a file that is there but cannot be read.
fn ini<'k>(
    root: &Path,
    files: &[String],
    keys: &[(&str, &'k str)],
) -> Result<BTreeMap<&'k str, Found>, Found> {
    let mut found = BTreeMap::new();
    for file in files {
        let text = match std::fs::read_to_string(root.join(file)) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err((format!("/{file}"), e.to_string())),
        };
        let mut section = "";
        for line in text.lines() {
            let line = line.trim();
            if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = s;
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            for &(sect, key) in keys {
                if sect == section && k == key {
                    found.insert(key, (v.to_owned(), format!("/{file}")));
                }
            }
        }
    }
    Ok(found)
}

/// What the display manager `unit`'s configuration under `root` says about
/// its greeter's VT.
pub fn config(root: &Path, unit: &str) -> Config {
    match stem(unit) {
        "sddm" => sddm(root),
        "lightdm" => lightdm(root),
        "greetd" => greetd(root),
        "gdm" => Config::NoVt(super::text::GDM_NO_VT.to_owned()),
        _ => Config::Unknown,
    }
}

fn sddm(root: &Path) -> Config {
    let files = files(
        root,
        &["usr/lib/sddm/sddm.conf.d", "etc/sddm.conf.d"],
        None,
        "etc/sddm.conf",
    );
    let found = match ini(
        root,
        &files,
        &[
            ("General", "DisplayServer"),
            ("X11", "MinimumVT"),
            ("General", "MinimumVT"),
        ],
    ) {
        Ok(found) => found,
        Err((path, why)) => return Config::Unreadable { path, why },
    };
    let server = found.get("DisplayServer");
    use super::text::sddm as text;
    match server.map(|(v, f)| (v.as_str(), f)) {
        Some((kind @ ("wayland" | "x11-user"), file)) => Config::Vt(Setting {
            source: format!("DisplayServer={kind} in {file}"),
            effect: text::wayland_vt1(),
            vt: 1,
            exact: true,
        }),
        Some((other, file)) if other != "x11" => Config::NoVt(text::unknown_server(other, file)),
        _ => match found.get("MinimumVT") {
            Some((v, file)) => match v.parse::<u16>() {
                Ok(vt) if vt >= 1 => Config::Vt(Setting {
                    source: format!("MinimumVT={vt} in {file}"),
                    effect: text::minimum_vt(vt),
                    vt,
                    exact: false,
                }),
                _ => Config::NoVt(text::bad_minimum_vt(v, file)),
            },
            None => Config::NoVt(text::x11()),
        },
    }
}

fn lightdm(root: &Path) -> Config {
    let files = files(
        root,
        &[
            "usr/share/lightdm/lightdm.conf.d",
            "usr/local/share/lightdm/lightdm.conf.d",
            "etc/xdg/lightdm/lightdm.conf.d",
            "etc/lightdm/lightdm.conf.d",
        ],
        Some(".conf"),
        "etc/lightdm/lightdm.conf",
    );
    let found = match ini(root, &files, &[("LightDM", "minimum-vt")]) {
        Ok(found) => found,
        Err((path, why)) => return Config::Unreadable { path, why },
    };
    let (vt, source) = match found.get("minimum-vt") {
        None => (7, super::text::lightdm::DEFAULT.to_owned()),
        Some((v, file)) => match v.parse::<i64>() {
            Ok(n) => (
                u16::try_from(n.max(1)).unwrap_or(u16::MAX),
                format!("minimum-vt={v} in {file}"),
            ),
            Err(_) => return Config::NoVt(super::text::lightdm::bad(v, file)),
        },
    };
    Config::Vt(Setting {
        source,
        effect: super::text::lightdm::effect(vt),
        vt,
        exact: false,
    })
}

fn greetd(root: &Path) -> Config {
    const FILE: &str = "etc/greetd/config.toml";
    let text = match std::fs::read_to_string(root.join(FILE)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Config::NoVt(super::text::greetd::no_file(FILE));
        }
        Err(e) => {
            return Config::Unreadable {
                path: format!("/{FILE}"),
                why: e.to_string(),
            };
        }
    };
    let mut section = "";
    let mut vt = None;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = s.trim();
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && section == "terminal"
            && k.trim() == "vt"
        {
            vt = Some(v.trim().to_owned());
        }
    }
    match vt {
        Some(v) => match v.parse::<u16>() {
            Ok(n) if n >= 1 => Config::Vt(Setting {
                source: format!("vt = {n} in /{FILE}"),
                effect: super::text::greetd::effect(n),
                vt: n,
                exact: true,
            }),
            _ => Config::NoVt(super::text::greetd::not_a_vt(&v, FILE)),
        },
        None => Config::NoVt(super::text::greetd::no_vt(FILE)),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "a test's set-up fails loudly"
    )]

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
        let no_vt = Config::NoVt("x11".into());
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
        assert!(!service_of("lightdm-greeter", "sddm"));
        assert!(!service_of("anything", ""));
    }

    fn root(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("vtium-held-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root
    }

    #[test]
    fn sddm_reads_its_files_in_order_a_later_value_winning() {
        // Arch's packaged default, and a drop-in of the administrator's.
        let r = root(
            "sddm",
            &[
                (
                    "usr/lib/sddm/sddm.conf.d/default.conf",
                    "[General]\nDisplayServer=x11\n",
                ),
                (
                    "etc/sddm.conf.d/10-wayland.conf",
                    "[General]\nDisplayServer=wayland\n",
                ),
            ],
        );
        let Config::Vt(s) = config(&r, "sddm.service") else {
            panic!("{:?}", config(&r, "sddm.service"));
        };
        assert_eq!(
            (s.vt, s.exact, s.source.as_str()),
            (
                1,
                true,
                "DisplayServer=wayland in /etc/sddm.conf.d/10-wayland.conf"
            )
        );
        std::fs::write(r.join("etc/sddm.conf"), "[General]\nDisplayServer=x11\n").unwrap();
        assert!(matches!(config(&r, "sddm.service"), Config::NoVt(_)));
        std::fs::write(r.join("etc/sddm.conf"), "[X11]\nMinimumVT=3\n").unwrap();
        // The drop-in's wayland still stands, and wins over MinimumVT.
        assert!(matches!(
            config(&r, "sddm.service"),
            Config::Vt(Setting { vt: 1, .. })
        ));
        std::fs::remove_file(r.join("etc/sddm.conf.d/10-wayland.conf")).unwrap();
        assert!(matches!(
            config(&r, "sddm.service"),
            Config::Vt(Setting {
                vt: 3,
                exact: false,
                ..
            })
        ));
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn lightdm_starts_on_vt_7_unless_a_file_says_otherwise() {
        let r = root("lightdm", &[]);
        assert!(matches!(
            config(&r, "lightdm.service"),
            Config::Vt(Setting { vt: 7, .. })
        ));
        let r = root(
            "lightdm-set",
            &[
                (
                    "usr/share/lightdm/lightdm.conf.d/50-x.conf",
                    "[LightDM]\nminimum-vt=2\n",
                ),
                ("etc/lightdm/lightdm.conf", "[Seat:*]\nminimum-vt=9\n"),
            ],
        );
        assert!(matches!(
            config(&r, "lightdm.service"),
            Config::Vt(Setting { vt: 2, .. })
        ));
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn greetd_takes_a_numbered_vt_and_names_none_for_next() {
        let r = root(
            "greetd",
            &[(
                "etc/greetd/config.toml",
                "[terminal]\n# The VT\nvt = 1 # first\n[default_session]\ncommand = \"agreety\"\n",
            )],
        );
        assert!(matches!(
            config(&r, "greetd.service"),
            Config::Vt(Setting {
                vt: 1,
                exact: true,
                ..
            })
        ));
        std::fs::write(
            r.join("etc/greetd/config.toml"),
            "[terminal]\nvt = \"next\"\n",
        )
        .unwrap();
        assert!(matches!(config(&r, "greetd.service"), Config::NoVt(_)));
        assert!(matches!(config(&r, "gdm.service"), Config::NoVt(_)));
        assert_eq!(config(&r, "cage@tty1.service"), Config::Unknown);
        let _ = std::fs::remove_dir_all(r);
    }
}
