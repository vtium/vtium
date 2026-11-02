//! The VTs that graphics hold, which `vtium console` leaves alone: the VTs
//! a display manager claims, and the VTs a graphical session or a greeter
//! is on now (ADR-1500 D7).
//!
//! A display manager claims a VT by the `getty@ttyN.service` its unit's
//! `Conflicts=` names, as sddm's, gdm's and lightdm's units do for the VT
//! their greeter starts on. A session is graphical when logind gives it the
//! type `x11`, `wayland` or `mir`, or the class `greeter`; vtium asks
//! logind with `loginctl`, the interface systemd documents, and never reads
//! logind's files under `/run/systemd`, which ADR-1400 rejected.

use std::collections::{BTreeMap, BTreeSet};

use super::{GETTY, instance};

/// What holds a VT for graphics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Holder {
    /// A display manager's unit, such as `sddm.service`, whose
    /// `Conflicts=` names the VT's getty.
    DisplayManager(String),
    /// A graphical session or greeter on the VT now.
    Session {
        user: String,
        service: String,
        /// The session's type, or `greeter` for a greeter.
        kind: String,
    },
}

/// The VTs whose `getty@ttyN.service` a unit's `Conflicts=` lines name,
/// read from the unit file's text and then its drop-ins' texts, in order.
/// An empty `Conflicts=` clears what came before, as systemd reads it.
pub fn conflicts<'t>(texts: impl IntoIterator<Item = &'t str>) -> BTreeSet<u16> {
    let mut vts = BTreeSet::new();
    for text in texts {
        let mut in_unit = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_unit = line == "[Unit]";
                continue;
            }
            let Some(value) = line.strip_prefix("Conflicts=").filter(|_| in_unit) else {
                continue;
            };
            if value.trim().is_empty() {
                vts.clear();
            }
            for name in value.split_whitespace() {
                if let Some((GETTY, n)) = instance(name) {
                    vts.insert(n);
                }
            }
        }
    }
    vts
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

/// The graphical sessions and greeters on a VT in `loginctl show-session`'s
/// output, one block of `Key=value` lines a session, the blocks parted by
/// an empty line.
pub fn graphical(show: &str) -> Vec<(u16, Holder)> {
    let mut found = Vec::new();
    for block in show.split("\n\n") {
        let fields: BTreeMap<&str, &str> = block
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        let get = |key: &str| fields.get(key).copied().unwrap_or_default();
        let Ok(vt) = get("VTNr").parse::<u16>() else {
            continue;
        };
        let kind = match (get("Type"), get("Class")) {
            (_, "greeter") => "greeter",
            (kind @ ("x11" | "wayland" | "mir"), _) => kind,
            _ => continue,
        };
        if vt == 0 {
            continue;
        }
        found.push((
            vt,
            Holder::Session {
                user: get("Name").to_owned(),
                service: get("Service").to_owned(),
                kind: kind.to_owned(),
            },
        ));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_units_conflicts_name_the_gettys_vts() {
        let unit = "[Unit]\nConflicts=getty@tty1.service plymouth-quit.service\nAfter=getty@tty1.service\n[Service]\nConflicts=getty@tty3.service\n";
        assert_eq!(conflicts([unit]), BTreeSet::from([1]));
        let drop_in = "[Unit]\nConflicts=\nConflicts=getty@tty7.service\n";
        assert_eq!(conflicts([unit, drop_in]), BTreeSet::from([7]));
        assert!(conflicts(["[Unit]\nConflicts=getty@ttytty1.service\n"]).is_empty());
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
            session_ids(
                "1 1000 aaron -     1490 manager -    no -\n3 1000 aaron seat0 3362 user    tty7 no -\n"
            ),
            ["1", "3"]
        );
    }
}
