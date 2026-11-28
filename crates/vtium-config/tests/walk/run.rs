//! The property walk of the rebuilt TUI's navigation: random runs of keys,
//! each checked against the reading of the tables in `reference.rs`.
//!
//! The reference state machine knows only what the tables say: the node
//! the person is in, the tab in front, the nodes a question or the small
//! screen opened over, and whether the TUI has left. In each state it
//! offers only the keys whose effect the tables decide, so every key it
//! presses has a prediction. After each key the test compares the TUI with
//! the reference: the node, the tab in front, and whether the TUI has
//! left. When a run fails, proptest shrinks it to the shortest that still
//! fails and prints it.
//!
//! The number of cases is a constant, so a run of `cargo test` takes the
//! same few seconds every time. proptest writes the seed of each failing
//! case to `run.proptest-regressions` beside this file, which is
//! committed, and runs those seeds first in every later run. A seed
//! replays its case only while the tables and the alphabet stay as they
//! are, because proptest draws each key again from the keys the reference
//! offers. So the seed's line also records the shrunk run of keys, and
//! [`the_runs_proptest_found_replay_as_the_tables_say`] presses each
//! recorded run as it is written.
//!
//! References:
//! - ADR-1206 D5: a property test checks the navigation against a reading
//!   of the tables of its own, with a fixed number of cases and committed
//!   failure-persistence files.

use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::FileFailurePersistence;
use proptest_state_machine::{ReferenceStateMachine, StateMachineTest, prop_state_machine};

use super::explore::{ALPHABET, press_of};
use super::reference::{self, Expect};
use super::sim::{Machine, Sim, Term};

/// How many random runs each test run tries.
const CASES: u32 = 48;

/// Where the reference says the TUI is.
#[derive(Clone, Debug)]
pub struct RefState {
    term: Term,
    /// The node of the tree the person is in.
    node: String,
    /// The tab in front.
    tab: String,
    /// The nodes under a question or the small screen, the newest last,
    /// where Esc at the question or growing the terminal goes back to.
    under: Vec<String>,
    /// The TUI has left.
    left: bool,
}

impl RefState {
    /// The state after `key`, by the alphabet's name, or `None` where the
    /// tables do not decide what the key does here.
    fn step(&self, key: &str) -> Option<RefState> {
        let mut next = self.clone();
        if self.left {
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
                // a level opened over the tab is not a question, so it
                // closes back to the tab, not to what is under it
                next.node = id;
            }
            // the tab's list, in the node of whichever item the highlight
            // is then on, which the reference does not follow
            Expect::InTab(tab) => {
                next.tab.clone_from(&tab);
                next.node = tab;
            }
            Expect::Quits { .. } => next.left = true,
            Expect::LeavesQuestion => next.node = next.under.pop()?,
            Expect::LeavesTrial | Expect::OneOf(_) => return None,
        }
        Some(next)
    }

    /// The reference at its start in `term`: the screen map's machine
    /// opens on the Fonts tab.
    fn start(term: Term) -> RefState {
        RefState {
            term,
            node: "fonts".to_owned(),
            tab: "fonts".to_owned(),
            under: Vec::new(),
            left: false,
        }
    }
}

pub struct Reference;

impl ReferenceStateMachine for Reference {
    type State = RefState;
    type Transition = &'static str;

    fn init_state() -> BoxedStrategy<RefState> {
        select(Term::ALL.to_vec()).prop_map(RefState::start).boxed()
    }

    /// The keys the tables decide here, the keys that leave the TUI
    /// pressed a twelfth as often as the rest, so most runs are long.
    fn transitions(state: &RefState) -> BoxedStrategy<&'static str> {
        let (mut end, mut go) = (Vec::new(), Vec::new());
        for (name, _) in ALPHABET {
            match state.step(name) {
                Some(next) if next.left && !state.left => end.push(*name),
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

/// Whether the TUI after a key agrees with the reference: the exit, the
/// node, and the tab in front, which the top row shows.
fn compare(sim: &Sim, want: &RefState) -> Result<(), String> {
    if want.left {
        return if sim.exited.is_some() {
            Ok(())
        } else {
            Err(format!(
                "the tables say the TUI left, and it is at {}",
                sim.app.node()
            ))
        };
    }
    if sim.exited.is_some() {
        return Err("the TUI left, and the tables say it stays".to_owned());
    }
    let node = sim.app.node();
    // the reference names a tab's list by the tab, and the list is in one
    // of its nodes by what the highlight is on
    let listed = reference::kind(node) == "view" && reference::tab_of(node) == want.node;
    if node != want.node && !listed {
        return Err(format!(
            "the TUI is in {node}, and the tables say {}",
            want.node
        ));
    }
    if node == "any.too-small" {
        return Ok(());
    }
    let tab = reference::order()
        .get(sim.app.nav().tab())
        .cloned()
        .unwrap_or_default();
    if tab != want.tab {
        return Err(format!(
            "the top row has {tab:?} in front, and the tables say {}",
            want.tab
        ));
    }
    Ok(())
}

/// Presses `keys`, by the alphabet's names, in a TUI started in `term` at
/// 80×25, and compares it with the reference after each key. It fails
/// where the tables do not decide a key, so a run that no longer means
/// what it meant fails instead of testing something else.
pub fn replay(term: Term, keys: &[&str]) -> Result<(), String> {
    let mut want = RefState::start(term);
    let mut sim = Sim::start(term, Machine::Map, 80, 25);
    for (i, key) in keys.iter().enumerate() {
        let done = keys.get(..=i).unwrap_or_default().join(" ");
        want = want.step(key).ok_or_else(|| {
            format!(
                "{}: {done}: the tables do not decide {key} in {}",
                term.name(),
                want.node
            )
        })?;
        sim.press(press_of(key));
        compare(&sim, &want).map_err(|e| format!("{}: {done}: {e}", term.name()))?;
    }
    Ok(())
}

pub struct Walk;

impl StateMachineTest for Walk {
    type SystemUnderTest = Sim;
    type Reference = Reference;

    fn init_test(state: &RefState) -> Sim {
        Sim::start(state.term, Machine::Map, 80, 25)
    }

    fn apply(mut sim: Sim, _: &RefState, key: &'static str) -> Sim {
        sim.press(press_of(key));
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

/// The terminal and the keys that a line of `run.proptest-regressions`
/// records after its seed: `# shrinks to (RefState { term: Vtium, … },
/// ["4", "o", "Esc"], None)`.
fn recorded(comment: &str) -> Option<(Term, Vec<String>)> {
    let term = comment.split_once("term: ")?.1.split([',', ' ']).next()?;
    let term = Term::ALL.into_iter().find(|t| format!("{t:?}") == term)?;
    let list = comment.split_once("}, [")?.1.split_once(']')?.0;
    let keys = list
        .split(", ")
        .filter(|k| !k.is_empty())
        .map(|k| k.trim_matches('"').to_owned())
        .collect();
    Some((term, keys))
}

/// Each run of keys that `run.proptest-regressions` records, pressed as
/// it is written and compared with the reference after each key. A seed
/// alone replays its case only while the tables and the alphabet stay as
/// they are, so this test keeps each case found once. It fails on a line
/// that does not read as a seed and its run, and on a key the tables no
/// longer decide.
#[test]
fn the_runs_proptest_found_replay_as_the_tables_say() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/walk/run.proptest-regressions");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut failures = Vec::new();
    let mut replayed = 0usize;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (seed, comment) = line.split_once('#').unwrap_or((line, ""));
        let seed = seed.trim().strip_prefix("cc ").unwrap_or(seed.trim());
        if <proptest::test_runner::PersistedSeed as std::str::FromStr>::from_str(&format!(
            "cc {seed}"
        ))
        .is_err()
        {
            failures.push(format!(
                "line {}: proptest cannot read the seed {seed:?}",
                n + 1
            ));
            continue;
        }
        let Some((term, keys)) = recorded(comment) else {
            failures.push(format!("line {}: no run of keys after the seed", n + 1));
            continue;
        };
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        if let Err(e) = replay(term, &keys) {
            failures.push(format!("line {}: {e}", n + 1));
        }
        replayed += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(replayed >= 1, "the file records no run of keys");
}
