//! The controls the resource tabs share. A resource is a file vtium draws
//! from: a font, a color map, a shader, a color scheme or a pointer theme.
//! vtium uses a resource only while the batch lists it, and a resource
//! that the system does not hold yet can be activated only when it passes
//! the trust rule. The trust rule asks that the file, and every folder
//! above it, belong to root and that nobody else can write them.
//!
//! The tabs that list resources show the same marks for each: whether it
//! is active, whether it passes the trust rule, and whether it is in the
//! person's own folder, where vtium cannot use it. Space activates or
//! deactivates the highlighted resource in the draft through the desk,
//! which keeps the change beside the draft until the install. Before it
//! deactivates a resource that a look uses, the TUI asks, and the question
//! names those looks. [`Resource`] holds the marks, [`space`] turns Space
//! into the tab's commands, and [`Resource::row`] gives the marks as a
//! list's detail, so the Fonts tab and the Looks tab's color schemes call
//! the same code as the Effects tab.
//!
//! References:
//! - ADR-1205 D1: the two resource tabs share their controls.
//! - ADR-1205 D4: the batch, the trust rule, and a draft keeping its
//!   changes to the batch beside it.
//! - ADR-1205 D8: deactivating a resource warns and lists the looks that
//!   use it.

use std::path::PathBuf;

use crate::batch::{Change as BatchChange, State, Use};

use super::desk::Op;
use super::model::{Effect, Then};
use super::tabs::{Change, Command, Request};
use super::words;

/// One resource a tab lists, with its marks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub path: PathBuf,
    /// What kind of resource it is, in a person's words: `color map`,
    /// `shader`, `font` and so on.
    pub kind: &'static str,
    /// Where it stands in the draft.
    pub state: State,
    /// Whether it passes the trust rule, or the check it fails, file
    /// first.
    pub trust: Result<(), String>,
    /// It is in the person's own folder, so vtium cannot use it.
    pub own: bool,
    /// A packaged file that the administrator's file of the same name
    /// hides, so vtium reads the administrator's file in its place.
    pub hidden: bool,
}

impl Resource {
    /// Whether vtium uses it once the draft is installed.
    pub fn active(&self) -> bool {
        matches!(self.state, State::Always | State::Active | State::Activates)
    }

    /// The file's name, as a list shows it.
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// The resource's marks, as a list's detail: not usable for one in
    /// the person's own folder; otherwise whether it is active, whether
    /// it passes the trust rule, and whether the administrator's file of
    /// its name hides it.
    pub fn row(&self) -> String {
        if self.own {
            return words::NOT_USABLE.to_owned();
        }
        let trust = match self.trust {
            Ok(()) => words::TRUSTED,
            Err(_) => words::UNTRUSTED,
        };
        let mut row = format!("{}, {trust}", words::active_mark(self.state));
        if self.hidden {
            row.push_str(", ");
            row.push_str(&words::resource_hidden(self.kind));
        }
        row
    }
}

/// Space on `resource`, which the looks in `uses` use: activates it, or
/// deactivates it when it is active, through the desk. Before a
/// deactivation of a resource that a look uses, the TUI asks, naming
/// those looks. A resource in the person's own folder, the built-in font,
/// and one that the system does not hold and that fails the trust rule
/// change nothing, and the message line says why. A pending deactivation
/// can always be taken back, as the batch judges trust only for a file the
/// system does not hold, and the desk checks the trust rule again on the
/// file itself.
pub fn space(resource: &Resource, uses: &[Use]) -> Vec<Command> {
    let path = resource.path.display().to_string();
    let say = |line: String| vec![Command::Change(Change::Say(line))];
    if resource.own {
        return say(words::own_resource(resource.kind, &path));
    }
    if resource.state == State::Always {
        return Vec::new();
    }
    if !resource.active() {
        if resource.state == State::Inactive
            && let Err(why) = &resource.trust
        {
            return say(words::untrusted(resource.kind, why));
        }
        return vec![Command::Effect(Effect::Desk(Op::Resource {
            path: resource.path.clone(),
            change: BatchChange::Activate,
        }))];
    }
    let op = Op::Resource {
        path: resource.path.clone(),
        change: BatchChange::Deactivate,
    };
    if uses.is_empty() {
        return vec![Command::Effect(Effect::Desk(op))];
    }
    vec![Command::Request(Request::Ask {
        question: words::deactivate_question(resource.kind, &path, uses),
        then: Then::Desk(op),
    })]
}

/// Gives each of `resources` its state in the draft from `states`, the
/// resources the desk says are active or changed. A resource the list
/// leaves out is not active.
pub fn set_states(resources: &mut [Resource], states: &[(PathBuf, State)]) {
    for r in resources {
        r.state = states
            .iter()
            .find(|(p, _)| *p == r.path)
            .map_or(State::Inactive, |(_, s)| *s);
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::batch::Through;

    fn map(state: State, own: bool) -> Resource {
        Resource {
            path: PathBuf::from("/etc/vtium/looks/sepia.cube"),
            kind: "color map",
            state,
            trust: Ok(()),
            own,
            hidden: false,
        }
    }

    // Space activates a resource that is not active, deactivates one that
    // is, and asks first when a look uses it
    #[test]
    fn space_activates_deactivates_or_asks() {
        let op = |change| Op::Resource {
            path: "/etc/vtium/looks/sepia.cube".into(),
            change,
        };
        assert_eq!(
            space(&map(State::Inactive, false), &[]),
            [Command::Effect(Effect::Desk(op(BatchChange::Activate)))]
        );
        assert_eq!(
            space(&map(State::Active, false), &[]),
            [Command::Effect(Effect::Desk(op(BatchChange::Deactivate)))]
        );
        let desk = Use {
            look: Some("desk".into()),
            through: Through::Vtlook("sepia".into()),
        };
        let asked = space(&map(State::Activates, false), &[desk]);
        let [Command::Request(Request::Ask { question, then })] = asked.as_slice() else {
            panic!("{asked:?}");
        };
        assert!(
            question.contains("Look desk uses it through the vtlook sepia."),
            "{question}"
        );
        assert_eq!(*then, Then::Desk(op(BatchChange::Deactivate)));
        // a resource in the person's own folder is not usable
        let own = space(&map(State::Inactive, true), &[]);
        assert!(
            matches!(own.as_slice(), [Command::Change(Change::Say(s))] if s.contains("own folder"))
        );
        // one that the system does not hold and that fails the trust rule
        // is not activated
        let mut bad = map(State::Inactive, false);
        bad.trust = Err("the file belongs to user 1000".into());
        let refused = space(&bad, &[]);
        assert!(
            matches!(refused.as_slice(), [Command::Change(Change::Say(s))] if s.contains("user 1000"))
        );
        // a pending deactivation of such a file is taken back through the
        // desk, which judges the file itself
        bad.state = State::Deactivates;
        assert_eq!(
            space(&bad, &[]),
            [Command::Effect(Effect::Desk(op(BatchChange::Activate)))]
        );
    }
}
