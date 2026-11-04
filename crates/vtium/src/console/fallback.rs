//! `vtium console fallback N`: what `vtium-getty@.service` runs when vtium
//! on VT N fails (ADR-1500 D7, ADR-1401 D9).
//!
//! It starts `getty@ttyN.service` so that the VT has a login again, unless
//! that start would stop another program. Starting a unit stops every unit
//! it conflicts with, in either direction: the units its own `Conflicts=`
//! names, and the units whose `Conflicts=` names it, which systemd lists as
//! its `ConflictedBy=`. Arch's sddm.service, gdm.service and
//! lightdm.service name `getty@tty1.service` there, so a start of the
//! getty on VT 1 stopped the display manager. vtium asks the running
//! manager for both lists and for each unit's state, with `systemctl show`,
//! since only the manager knows which units are loaded, running or have a
//! job queued. A unit that is active, activating or reloading, has a job
//! queued, or is enabled holds the getty back: then vtium starts nothing,
//! says why, and the VT is left without a console. vtium's own units on VT
//! N, `vtium@ttyN` and `vtium-dev@ttyN`, do not hold it back: the getty
//! takes the VT in their place, which is what the fallback is for. When
//! vtium cannot ask the manager, it cannot tell what the start would stop,
//! so it starts nothing either.

use std::collections::BTreeMap;

use super::{DEV, GETTY, Run, Step, Stop, VTIUM, text};

/// A unit's state, from `systemctl show -p Id -p ActiveState -p
/// UnitFileState -p Job`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitState {
    pub id: String,
    pub active: String,
    pub file: String,
    pub job: String,
}

impl UnitState {
    /// Whether a start of the getty would stop the unit now, or would
    /// cancel its start at boot.
    fn holds_back(&self) -> bool {
        matches!(
            self.active.as_str(),
            "active" | "activating" | "reloading" | "refreshing"
        ) || !self.job.is_empty()
            || matches!(
                self.file.as_str(),
                "enabled" | "enabled-runtime" | "linked" | "linked-runtime" | "alias"
            )
    }
}

/// The blocks of `systemctl show`'s `Key=value` lines, one a unit, parted
/// by an empty line.
fn blocks(show: &str) -> Vec<BTreeMap<&str, &str>> {
    show.split("\n\n")
        .map(|block| {
            block
                .lines()
                .filter_map(|line| line.split_once('='))
                .collect()
        })
        .filter(|fields: &BTreeMap<&str, &str>| !fields.is_empty())
        .collect()
}

impl Run<'_, '_, '_> {
    pub(super) fn fallback(&mut self, n: u16) -> Step<i32> {
        let getty = format!("{GETTY}@tty{n}.service");
        let ours = [
            format!("{VTIUM}@tty{n}.service"),
            format!("{DEV}@tty{n}.service"),
        ];
        let show = self
            .systemctl_quiet(&["show", &getty, "-p", "Conflicts", "-p", "ConflictedBy"])
            .map_err(|Stop(why)| Stop(text::fallback_cannot_tell(n, &why)))?;
        let mut others: Vec<String> = Vec::new();
        for fields in blocks(&show.stdout) {
            for key in ["Conflicts", "ConflictedBy"] {
                for unit in fields
                    .get(key)
                    .copied()
                    .unwrap_or_default()
                    .split_whitespace()
                {
                    if !ours.iter().any(|o| o == unit) && !others.iter().any(|o| o == unit) {
                        others.push(unit.to_owned());
                    }
                }
            }
        }
        let mut held_back: Vec<UnitState> = Vec::new();
        if !others.is_empty() {
            let mut args = vec!["show"];
            args.extend(others.iter().map(String::as_str));
            args.extend([
                "-p",
                "Id",
                "-p",
                "ActiveState",
                "-p",
                "UnitFileState",
                "-p",
                "Job",
            ]);
            let show = self
                .systemctl_quiet(&args)
                .map_err(|Stop(why)| Stop(text::fallback_cannot_tell(n, &why)))?;
            let get = |fields: &BTreeMap<&str, &str>, key: &str| {
                fields.get(key).copied().unwrap_or_default().to_owned()
            };
            let states: Vec<UnitState> = blocks(&show.stdout)
                .iter()
                .map(|f| UnitState {
                    id: get(f, "Id"),
                    active: get(f, "ActiveState"),
                    file: get(f, "UnitFileState"),
                    job: get(f, "Job"),
                })
                .collect();
            if states.len() != others.len() {
                // systemctl gave a state for fewer units than it was asked
                // for, so vtium cannot tell what the start would stop.
                return Err(Stop(text::fallback_cannot_tell(
                    n,
                    &text::fallback_short_answer(&others),
                )));
            }
            held_back = states.into_iter().filter(UnitState::holds_back).collect();
        }
        if !held_back.is_empty() {
            self.say(&text::fallback_held_back(n, &held_back));
            return Ok(0);
        }
        self.say(&text::fallback_starting(n));
        self.systemctl_quiet(&["--no-block", "start", &getty])?;
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_holds_the_getty_back_while_it_runs_waits_or_is_enabled() {
        let state = |active: &str, file: &str, job: &str| UnitState {
            id: "sddm.service".into(),
            active: active.into(),
            file: file.into(),
            job: job.into(),
        };
        assert!(state("active", "disabled", "").holds_back());
        assert!(state("activating", "static", "").holds_back());
        assert!(state("inactive", "enabled", "").holds_back());
        assert!(state("inactive", "disabled", "41 start").holds_back());
        assert!(!state("inactive", "static", "").holds_back());
        assert!(!state("failed", "disabled", "").holds_back());
        assert!(!state("inactive", "", "").holds_back());
    }

    #[test]
    fn systemctl_shows_a_block_a_unit() {
        // As systemd 262's systemctl show printed two units (measured).
        let show = "Id=sddm.service\nActiveState=active\nUnitFileState=enabled\nJob=\n\nId=vtium@tty1.service\nActiveState=inactive\nUnitFileState=disabled\nJob=\n";
        let b = blocks(show);
        assert_eq!(b.len(), 2);
        assert_eq!(
            b.get(1).and_then(|f| f.get("Id")),
            Some(&"vtium@tty1.service")
        );
        assert_eq!(b.first().and_then(|f| f.get("Job")), Some(&""));
    }
}
