//! The property test of the TUI's navigation: random sequences of keys,
//! each checked against the reading of the tables in `reference.rs`.
//!
//! The reference state machine knows only what the tables say: the node
//! the person is in, the tab in front, the nodes a question or the small
//! screen opened over, and whether the TUI has left. In each state it
//! offers only the keys whose effect the tables decide, so every key it
//! presses has a prediction. After each key the test compares the TUI with
//! the reference: the node, the tab the tab bar highlights, and whether the
//! TUI has left and with which status. When a sequence fails, proptest
//! shrinks it to the shortest that still fails and prints it.
//!
//! The number of cases is a constant, so a run of `cargo test` takes the
//! same few seconds every time. proptest writes the seed of each failing
//! case to `walk.proptest-regressions` beside this file, which is
//! committed, so a case found once runs first in every later run.
//!
//! References:
//! - ADR-1206 D5: a property test checks the navigation against a reading
//!   of the tables of its own, with a fixed number of cases and committed
//!   failure-persistence files.

use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::FileFailurePersistence;
use proptest_state_machine::{ReferenceStateMachine, StateMachineTest, prop_state_machine};

use super::explore::ALPHABET;
use super::reference::{self, Expect, FORM_FIELDS};
use super::sim::{Sim, Term};
use super::state;

/// How many random sequences each run tries.
const CASES: u32 = 64;

/// Where the reference says the TUI is.
#[derive(Clone, Debug)]
pub struct RefState {
    term: Term,
    /// The node of the tree the person is in.
    node: String,
    /// The tab in front, as the tab bar highlights it.
    tab: String,
    /// The nodes under a question or the small screen, the newest last,
    /// where Esc at the question or growing the terminal goes back to.
    under: Vec<String>,
    /// The TUI has left, with this status.
    left: Option<u8>,
    /// The field of the new-frit form being typed, counted from 0. The
    /// form opens on its first field, Enter on a field goes to the next,
    /// and Enter on the last field goes to the form's add row.
    field: usize,
}

impl RefState {
    /// The state after `key`, by the alphabet's name, or `None` where the
    /// tables do not decide what the key does here.
    fn step(&self, key: &str) -> Option<RefState> {
        let mut next = self.clone();
        if self.left.is_some() {
            return Some(next);
        }
        if self.node == "any.too-small" && key == "Grow" {
            next.node = next.under.pop()?;
            return Some(next);
        }
        if self.node == "looks.new-frit.typing" && key == "Enter" {
            next.field = self.field.saturating_add(1);
            if next.field == FORM_FIELDS {
                "looks.new-frit".clone_into(&mut next.node);
            }
            return Some(next);
        }
        match reference::expect(&self.node, key)? {
            Expect::At(id) => {
                let over = reference::kind(&id) == "question" || id == "any.too-small";
                if over && id != self.node {
                    next.under.push(self.node.clone());
                }
                if reference::kind(&id) == "view" {
                    next.tab.clone_from(&id);
                }
                next.node = id;
            }
            Expect::Exits(status) => next.left = Some(status),
            // the keys this test presses change nothing the person made,
            // so `q` quits without asking
            Expect::QuitsOrAsks => next.left = Some(0),
            Expect::LeavesQuestion => next.node = next.under.pop()?,
            Expect::LeavesTrial => return None,
        }
        // the form opens on its first field each time, so a key that
        // leaves it for a tab starts it again from there
        if !next.node.starts_with("looks.new-frit") && !next.node.starts_with("any.") {
            next.field = 0;
        }
        Some(next)
    }

    /// The reference at its start in `term`: the fixture of
    /// `src/tui/fixture.rs` opens on the Fonts tab.
    fn start(term: Term) -> RefState {
        RefState {
            term,
            node: "fonts".to_owned(),
            tab: "fonts".to_owned(),
            under: Vec::new(),
            left: None,
            field: 0,
        }
    }
}

pub struct Reference;

impl ReferenceStateMachine for Reference {
    type State = RefState;
    type Transition = &'static str;

    /// The TUI as the scenarios start it, in each kind of terminal: the
    /// fixture of `src/tui/fixture.rs` opens on the Fonts tab.
    fn init_state() -> BoxedStrategy<RefState> {
        select(Term::ALL.to_vec()).prop_map(RefState::start).boxed()
    }

    /// The keys the tables decide here, the keys that leave the TUI
    /// pressed a twelfth as often as the rest, so most sequences run long.
    /// In a field of the new-frit form, Enter comes as often as every other
    /// key together, so that sequences reach the form's add row.
    fn transitions(state: &RefState) -> BoxedStrategy<&'static str> {
        let (mut end, mut go) = (Vec::new(), Vec::new());
        for (name, _) in ALPHABET {
            match state.step(name) {
                Some(next) if next.left.is_some() && state.left.is_none() => end.push(*name),
                Some(_) => go.push(*name),
                None => {}
            }
        }
        let keys = match (go.is_empty(), end.is_empty()) {
            (false, false) => prop_oneof![12 => select(go), 1 => select(end)].boxed(),
            (false, true) => select(go).boxed(),
            _ => select(end).boxed(),
        };
        if state.node == "looks.new-frit.typing" {
            return prop_oneof![1 => keys, 1 => Just("Enter")].boxed();
        }
        keys
    }

    fn preconditions(state: &RefState, key: &&'static str) -> bool {
        state.step(key).is_some()
    }

    fn apply(state: RefState, key: &&'static str) -> RefState {
        state.step(key).unwrap_or(state)
    }
}

/// Whether the TUI after a key agrees with the reference: the exit, the
/// node, and the tab the tab bar highlights.
fn compare(sim: &Sim, want: &RefState) -> Result<(), String> {
    if let Some(status) = want.left {
        return if sim.exited == Some(status) {
            Ok(())
        } else {
            Err(format!(
                "the tables say the TUI left with {status}, and it is at {}",
                state::of(sim).node()
            ))
        };
    }
    if sim.exited.is_some() {
        return Err("the TUI left, and the tables say it stays".to_owned());
    }
    let node = state::of(sim).node();
    if node != want.node {
        return Err(format!(
            "the TUI is in {node}, and the tables say {}",
            want.node
        ));
    }
    if node == "any.too-small" {
        return Ok(());
    }
    let lit = highlighted(sim);
    if lit != want.tab {
        return Err(format!(
            "the tab bar highlights {lit:?}, and the tables say {}",
            want.tab
        ));
    }
    Ok(())
}

/// The tab the tab bar highlights, by its id: the reversed text on row 0.
fn highlighted(sim: &Sim) -> String {
    vtium_config::tui::view::screen(&sim.app)
        .first()
        .map(|row| {
            row.iter()
                .filter(|s| s.style.reverse)
                .map(|s| s.text.as_str())
                .collect::<String>()
                .trim()
                .to_lowercase()
        })
        .unwrap_or_default()
}

/// Presses `keys`, by the alphabet's names, in a TUI started in `term`,
/// and compares it with the reference after each key. It fails where the
/// tables do not decide a key, so a sequence that no longer means what it
/// meant fails instead of testing something else.
pub fn replay(term: Term, keys: &[&str]) -> Result<(), String> {
    let mut want = RefState::start(term);
    let mut sim = Sim::start(term, 80, 25);
    for (i, key) in keys.iter().enumerate() {
        let done = keys.get(..=i).unwrap_or_default().join(" ");
        want = want.step(key).ok_or_else(|| {
            format!(
                "{}: {done}: the tables do not decide {key} in {}",
                term.name(),
                want.node
            )
        })?;
        sim.press(super::press_of(key));
        compare(&sim, &want).map_err(|e| format!("{}: {done}: {e}", term.name()))?;
    }
    Ok(())
}

pub struct Walk;

impl StateMachineTest for Walk {
    type SystemUnderTest = Sim;
    type Reference = Reference;

    fn init_test(state: &RefState) -> Sim {
        Sim::start(state.term, 80, 25)
    }

    fn apply(mut sim: Sim, _: &RefState, key: &'static str) -> Sim {
        sim.press(super::press_of(key));
        sim
    }

    fn check_invariants(sim: &Sim, want: &RefState) {
        if let Err(e) = compare(sim, want) {
            panic!("{e}");
        }
    }
}

prop_state_machine! {
    #![proptest_config(ProptestConfig {
        cases: CASES,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("proptest-regressions"))),
        .. ProptestConfig::default()
    })]
    #[test]
    fn random_keys_move_as_the_tables_say(sequential 1..40 => Walk);
}

/// The new-frit form opens on its first field each time `n` opens it,
/// after Esc left it on its add row or on a field in the middle. The
/// reference counts the six fields, so it knows on which Enter the form
/// reaches its add row.
#[test]
fn the_new_frit_form_opens_on_its_first_field_after_esc() {
    let fields = ["Enter"; FORM_FIELDS];
    let from_the_add_row = [&["n"][..], &fields, &["Esc", "n"], &fields, &["Esc"]].concat();
    let from_a_field = [&["n", "Enter", "Enter", "Esc", "n"][..], &fields, &["Esc"]].concat();
    for term in Term::ALL {
        for keys in [&from_the_add_row, &from_a_field] {
            replay(term, keys).unwrap_or_else(|e| panic!("{e}"));
        }
    }
}
