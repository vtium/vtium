//! What SDDM's configuration says about its greeter's VT, read as SDDM 0.21
//! reads it.
//!
//! SDDM reads every file in `/usr/lib/sddm/sddm.conf.d`, then in
//! `/etc/sddm.conf.d`, each in name order, then `/etc/sddm.conf`, a later
//! value winning (`src/common/ConfigReader.cpp`). On each line it drops
//! everything from a `#` on, then trims the rest, so a `#` ends a section's
//! header too, and it reads the sections `[XDisplay]` and
//! `[WaylandDisplay]`, which SDDM 0.14 renamed, as `[X11]` and `[Wayland]`.
//! It compares `[General] DisplayServer=` without regard to case, and takes
//! any value but `x11-user` and `wayland` as `x11` (`src/daemon/Display.cpp`).
//!
//! With `x11`, the greeter takes the first VT no program has open
//! (`VT_OPENQRY`), which no file names. With `wayland` or `x11-user`, it
//! takes SDDM's built-in first VT, `SDDM_INITIAL_VT`, a build option,
//! unless a login session is on it; SDDM's unit names that VT's getty in
//! `Conflicts=` (`services/sddm.service.in`), so vtium takes the built-in VT
//! from there. `MinimumVT=`, which SDDM before 0.20 read and sddm.conf(5)
//! says SDDM 0.20 dropped, holds the VT it names, but frees none, since
//! vtium cannot tell which SDDM it is.

use std::collections::BTreeMap;
use std::path::Path;

use super::{Config, Setting, files, read, vt};
use crate::console::text::sddm as text;

/// Each `[section] key` SDDM's files set last, and its value and file.
type Settings = BTreeMap<(String, String), (String, String)>;

/// The settings SDDM's files under `root` make.
fn settings(root: &Path) -> Result<Settings, Config> {
    let mut found = BTreeMap::new();
    for file in files(
        root,
        &["usr/lib/sddm/sddm.conf.d", "etc/sddm.conf.d"],
        None,
        "etc/sddm.conf",
    ) {
        let Some(text) = read(root, &file)? else {
            continue;
        };
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if section == "XDisplay" {
                section = "X11".to_owned();
            } else if section == "WaylandDisplay" {
                section = "Wayland".to_owned();
            }
            if let Some((key, value)) = line.split_once('=') {
                found.insert(
                    (section.clone(), key.trim().to_owned()),
                    (value.trim().to_owned(), format!("/{file}")),
                );
            } else if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.to_owned();
            }
        }
    }
    Ok(found)
}

/// What SDDM's files under `root` say, `getty` the VT whose getty its unit
/// names in `Conflicts=`.
pub fn config(root: &Path, getty: Option<u16>) -> Config {
    let found = match settings(root) {
        Ok(found) => found,
        Err(unreadable) => return unreadable,
    };
    let get = |section: &str, key: &str| found.get(&(section.to_owned(), key.to_owned()));
    if let Some((value, file)) = get("General", "DisplayServer") {
        let kind = value.to_lowercase();
        if kind == "wayland" || kind == "x11-user" {
            return match getty {
                Some(vt) => Config::Vt(Setting {
                    source: format!("DisplayServer={value} in {file}"),
                    effect: text::initial_vt(vt),
                    vt,
                    exact: true,
                }),
                None => Config::NoVt {
                    why: text::no_initial_vt(value, file),
                    also: None,
                },
            };
        }
    }
    let also = get("X11", "MinimumVT")
        .or_else(|| get("General", "MinimumVT"))
        .and_then(|(value, file)| {
            let n = vt(value.parse().ok()?)?;
            Some(Setting {
                source: format!("MinimumVT={n} in {file}"),
                effect: text::minimum_vt(n),
                vt: n,
                exact: false,
            })
        });
    Config::NoVt {
        why: text::x11(),
        also,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "a test's set-up fails loudly"
    )]

    use super::super::tests::root;
    use super::*;

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
        let Config::Vt(s) = config(&r, Some(1)) else {
            panic!("{:?}", config(&r, Some(1)));
        };
        assert_eq!(
            (s.vt, s.exact, s.source.as_str()),
            (
                1,
                true,
                "DisplayServer=wayland in /etc/sddm.conf.d/10-wayland.conf"
            )
        );
        // The built-in first VT is the one the unit's Conflicts= names.
        assert!(matches!(
            config(&r, Some(7)),
            Config::Vt(Setting { vt: 7, .. })
        ));
        assert!(matches!(config(&r, None), Config::NoVt { also: None, .. }));
        std::fs::write(r.join("etc/sddm.conf"), "[General]\nDisplayServer=x11\n").unwrap();
        assert!(matches!(config(&r, Some(1)), Config::NoVt { .. }));
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn sddm_reads_comments_case_and_old_section_names_as_sddm_does() {
        let r = root(
            "sddm-syntax",
            &[(
                "etc/sddm.conf",
                "[General] # the display server\n  DisplayServer = Wayland # not X\n",
            )],
        );
        assert!(matches!(
            config(&r, Some(1)),
            Config::Vt(Setting { vt: 1, .. })
        ));
        // An unknown value is x11 to SDDM.
        std::fs::write(r.join("etc/sddm.conf"), "[General]\nDisplayServer=weston\n").unwrap();
        assert!(matches!(config(&r, Some(1)), Config::NoVt { .. }));
        // [XDisplay] is [X11], and a MinimumVT holds its VT and frees none.
        std::fs::write(r.join("etc/sddm.conf"), "[XDisplay]\nMinimumVT=3\n").unwrap();
        let Config::NoVt { also: Some(s), .. } = config(&r, Some(1)) else {
            panic!("{:?}", config(&r, Some(1)));
        };
        assert_eq!((s.vt, s.exact), (3, false));
        // A VT out of range is no VT.
        std::fs::write(r.join("etc/sddm.conf"), "[X11]\nMinimumVT=64\n").unwrap();
        assert!(matches!(
            config(&r, Some(1)),
            Config::NoVt { also: None, .. }
        ));
        let _ = std::fs::remove_dir_all(r);
    }
}
