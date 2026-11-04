//! What vtium reads from systemd's files and logind's answers about the
//! programs on the VTs: the VTs a unit file ties to itself, and the
//! sessions on a VT now (ADR-1500 D7).
//!
//! A unit ties itself to VT N by naming `getty@ttyN.service` in
//! `Conflicts=`, as sddm's, gdm's and lightdm's units do, by running on
//! `TTYPath=/dev/ttyN`, or by being an instance on it, `PREFIX@ttyN`.
//! [`super::held`] judges what each of those means. A session is graphical
//! when logind gives it the type `x11`, `wayland` or `mir`, or the class
//! `greeter`; vtium asks logind with `loginctl`, the interface systemd
//! documents, and never reads logind's files under `/run/systemd`, which
//! ADR-1400 rejected.

use std::collections::BTreeMap;

use super::held::{Facts, Holder};
use super::{GETTY, instance, vt_digits};

/// What a unit's file and drop-ins tie to a VT: the VTs whose
/// `getty@ttyN.service` its `[Unit]` section's `Conflicts=` lines name, and
/// the VTs its `[Service]` section's `TTYPath=` names, with `%i` and `%I`
/// read as `inst`, the unit's instance, as systemd expands them; and the VT
/// the unit `name` is an instance on. A drop-in only adds to `Conflicts=`:
/// systemd cannot reset a dependency to an empty list (systemd.unit(5)),
/// and an empty `Conflicts=` changes nothing (measured with systemd 262).
/// A later `TTYPath=` replaces an earlier one.
pub fn facts<'t>(name: &str, texts: impl IntoIterator<Item = &'t str>) -> Facts {
    let inst = name
        .strip_suffix(".service")
        .and_then(|n| n.split_once('@'))
        .map(|(_, i)| i)
        .unwrap_or_default();
    let expand = |value: &str| value.replace("%i", inst).replace("%I", inst);
    let mut facts = Facts {
        instance: instance(name).map(|(_, n)| n),
        ..Facts::default()
    };
    let mut tty_path = None;
    for text in texts {
        let mut section = "";
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                section = line;
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match (section, key.trim()) {
                ("[Unit]", "Conflicts") => {
                    for unit in expand(value).split_whitespace() {
                        if let Some((GETTY, n)) = instance(unit) {
                            facts.conflicts.insert(n);
                        }
                    }
                }
                ("[Service]", "TTYPath") => {
                    tty_path = expand(value.trim())
                        .strip_prefix("/dev/tty")
                        .and_then(vt_digits);
                }
                _ => {}
            }
        }
    }
    facts.tty_path.extend(tty_path);
    facts
}

/// The session ids in `loginctl list-sessions --no-legend`'s output: each
/// line's first word.
pub fn session_ids(list: &str) -> Vec<String> {
    list.lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|id| {
            id.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
        .map(str::to_owned)
        .collect()
}

/// One session in `loginctl show-session`'s output that is on a VT.
struct Session<'s> {
    vt: u16,
    user: &'s str,
    service: &'s str,
    kind: &'s str,
    class: &'s str,
}

/// The sessions on a VT in `loginctl show-session`'s output, one block of
/// `Key=value` lines a session, the blocks parted by an empty line.
fn sessions(show: &str) -> impl Iterator<Item = Session<'_>> {
    show.split("\n\n").filter_map(|block| {
        let fields: BTreeMap<&str, &str> = block
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        let get = |key: &str| fields.get(key).copied().unwrap_or_default();
        let vt = get("VTNr").parse::<u16>().ok().filter(|vt| *vt != 0)?;
        Some(Session {
            vt,
            user: get("Name"),
            service: get("Service"),
            kind: get("Type"),
            class: get("Class"),
        })
    })
}

impl Session<'_> {
    /// The session as a [`Holder`], its kind `greeter` for a greeter.
    fn holder(&self, kind: &str) -> (u16, Holder) {
        (
            self.vt,
            Holder::Session {
                user: self.user.to_owned(),
                service: self.service.to_owned(),
                kind: kind.to_owned(),
            },
        )
    }

    /// `greeter`, `x11`, `wayland` or `mir` for a graphical session or a
    /// greeter.
    fn graphical(&self) -> Option<&str> {
        match (self.kind, self.class) {
            (_, "greeter") => Some("greeter"),
            (kind @ ("x11" | "wayland" | "mir"), _) => Some(kind),
            _ => None,
        }
    }
}

/// The graphical sessions and greeters on a VT in `loginctl show-session`'s
/// output.
pub fn graphical(show: &str) -> Vec<(u16, Holder)> {
    sessions(show)
        .filter_map(|s| s.graphical().map(|kind| s.holder(kind)))
        .collect()
}

/// The other sessions on a VT in `loginctl show-session`'s output, such as
/// a user's login at a getty, its kind the session's type, such as `tty`.
pub fn logins(show: &str) -> Vec<(u16, Holder)> {
    sessions(show)
        .filter(|s| s.graphical().is_none())
        .map(|s| s.holder(s.kind))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_units_conflicts_and_tty_name_its_vts() {
        let unit = "[Unit]\nConflicts=getty@tty1.service plymouth-quit.service\nAfter=getty@tty1.service\n[Service]\nConflicts=getty@tty3.service\n";
        let vts = |f: Facts| {
            (
                f.conflicts.into_iter().collect::<Vec<_>>(),
                f.tty_path.into_iter().collect::<Vec<_>>(),
                f.instance,
            )
        };
        assert_eq!(vts(facts("sddm.service", [unit])), (vec![1], vec![], None));
        let drop_in = "[Unit]\nConflicts=\nConflicts=getty@tty7.service\n";
        assert_eq!(
            vts(facts("sddm.service", [unit, drop_in])),
            (vec![1, 7], vec![], None)
        );
        assert!(
            facts("x.service", ["[Unit]\nConflicts=getty@ttytty1.service\n"])
                .conflicts
                .is_empty()
        );
        // A template's specifiers, read for the instance, and its TTY.
        let kiosk =
            "[Unit]\nConflicts=getty@%i.service\n[Service]\nTTYPath=/dev/tty9\nTTYPath=/dev/%I\n";
        assert_eq!(
            vts(facts("cage@tty2.service", [kiosk])),
            (vec![2], vec![2], Some(2))
        );
        assert_eq!(
            vts(facts(
                "debug-shell.service",
                ["[Service]\nTTYPath=/dev/tty9\n"]
            )),
            (vec![], vec![9], None)
        );
        assert!(
            facts("x.service", ["[Service]\nTTYPath=/dev/console\n"])
                .tty_path
                .is_empty()
        );
    }

    #[test]
    fn graphical_sessions_and_greeters_are_found_by_type_and_class() {
        // As systemd 262's loginctl show-session prints them.
        let show = "Id=1\nName=aaron\nVTNr=0\nService=systemd-user\nType=unspecified\nClass=manager\n\nId=3\nName=aaron\nVTNr=7\nService=sddm\nType=wayland\nClass=user\n\nId=4\nName=sddm\nVTNr=1\nService=sddm-greeter\nType=x11\nClass=greeter\n\nId=5\nName=bob\nVTNr=3\nService=login\nType=tty\nClass=user\n";
        assert_eq!(
            graphical(show),
            [
                (
                    7,
                    Holder::Session {
                        user: "aaron".into(),
                        service: "sddm".into(),
                        kind: "wayland".into()
                    }
                ),
                (
                    1,
                    Holder::Session {
                        user: "sddm".into(),
                        service: "sddm-greeter".into(),
                        kind: "greeter".into()
                    }
                ),
            ]
        );
        assert_eq!(
            logins(show),
            [(
                3,
                Holder::Session {
                    user: "bob".into(),
                    service: "login".into(),
                    kind: "tty".into()
                }
            )]
        );
        assert_eq!(
            session_ids(
                "1 1000 aaron -     1490 manager -    no -\n3 1000 aaron seat0 3362 user    tty7 no -\n"
            ),
            ["1", "3"]
        );
    }
}
