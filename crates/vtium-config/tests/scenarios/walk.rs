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
use super::reference::{self, Expect};
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
        Some(next)
    }
}

pub struct Reference;

impl ReferenceStateMachine for Reference {
    type State = RefState;
    type Transition = &'static str;

    /// The TUI as the scenarios start it, in each kind of terminal: the
    /// fixture of `src/tui/fixture.rs` opens on the Fonts tab.
    fn init_state() -> BoxedStrategy<RefState> {
        select(Term::ALL.to_vec())
            .prop_map(|term| RefState {
                term,
                node: "fonts".to_owned(),
                tab: "fonts".to_owned(),
                under: Vec::new(),
                left: None,
            })
            .boxed()
    }

    /// The keys the tables decide here, the keys that leave the TUI
    /// pressed a twelfth as often as the rest, so most sequences run long.
    fn transitions(state: &RefState) -> BoxedStrategy<&'static str> {
        let (mut end, mut go) = (Vec::new(), Vec::new());
        for (name, _) in ALPHABET {
            match state.step(name) {
                Some(next) if next.left.is_some() && state.left.is_none() => end.push(*name),
                Some(_) => go.push(*name),
                None => {}
            }
        }
        match (go.is_empty(), end.is_empty()) {
            (false, false) => prop_oneof![12 => select(go), 1 => select(end)].boxed(),
            (false, true) => select(go).boxed(),
            _ => select(end).boxed(),
        }
    }

    fn preconditions(state: &RefState, key: &&'static str) -> bool {
        state.step(key).is_some()
    }

    fn apply(state: RefState, key: &&'static str) -> RefState {
        state.step(key).unwrap_or(state)
    }
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
        if let Some(status) = want.left {
            assert_eq!(sim.exited, Some(status), "the tables say the TUI left");
            return;
        }
        assert_eq!(
            sim.exited, None,
            "the TUI left, and the tables say it stays"
        );
        let node = state::of(sim).node();
        assert_eq!(
            node, want.node,
            "the TUI is in {node}, and the tables say {}",
            want.node
        );
        if node != "any.too-small" {
            let lit = highlighted(sim);
            assert_eq!(
                lit, want.tab,
                "the tab bar highlights {lit:?}, and the tables say {}",
                want.tab
            );
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
