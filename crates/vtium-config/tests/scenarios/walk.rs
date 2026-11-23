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
//! committed, and runs those seeds first in every later run. A seed
//! replays its case only while the two tables and [`ALPHABET`] stay as
//! they are, because proptest draws each key again from the keys the
//! reference offers. So the seed's line also records the shrunk sequence
//! of keys, and [`the_sequences_proptest_found_replay_as_the_tables_say`]
//! presses each recorded sequence as it is written. That test fails when a
//! line no longer reads as a seed, or when the tables no longer decide a
//! key of its sequence, where proptest would skip the line with a warning
//! or replay some other case.
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
                // a node belongs to the tab its id starts with, as the form
                // for a new font set belongs to the Looks tab when `n`
                // opens it from the Fonts tab
                let root = id.split('.').next().unwrap_or("");
                if reference::kind(&id) == "view" {
                    next.tab.clone_from(&id);
                } else if !over && !id.starts_with("any.") && reference::kind(root) == "view" {
                    root.clone_into(&mut next.tab);
                }
                next.node = id;
            }
            // the tab's list, in the node of whichever item the highlight
            // is then on, which the reference does not follow
            Expect::InTab(tab) => {
                next.tab.clone_from(&tab);
                next.node = tab;
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

    /// The reference at its start in `term`: the fixture of
    /// `src/tui/fixture.rs` opens on the Fonts tab.
    fn start(term: Term) -> RefState {
        RefState {
            term,
            node: "fonts".to_owned(),
            tab: "fonts".to_owned(),
            under: Vec::new(),
            left: None,
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

/// Whether the TUI after a key agrees with the reference: the exit, the
/// node, the tab the tab bar highlights, and the node the screen shows,
/// which [`state::Shown`] reads from the screen alone.
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
    // the reference names a tab's list by the tab, and the list is in one
    // of its nodes by what the highlight is on
    let listed = reference::kind(&node) == "view" && reference::tab_of(&node) == want.node;
    if node != want.node && !listed {
        return Err(format!(
            "the TUI is in {node}, and the tables say {}",
            want.node
        ));
    }
    if node == "any.too-small" {
        return Ok(());
    }
    let shown = state::Shown::of(sim);
    if shown.tab != want.tab {
        return Err(format!(
            "the tab bar highlights {:?}, and the tables say {}",
            shown.tab, want.tab
        ));
    }
    shown.agrees(&want.node)
}

/// The keys that open the new-frit form from the start: the Looks tab,
/// Down to its first font set, and `n`. In the Looks tab, `n` makes a new
/// look among the looks and opens the form among the font sets. The
/// reference does not follow the tab's highlight, so it cannot tell the
/// two apart, and [`replay`] starts the reference in the form when a
/// sequence begins with these keys.
pub const TO_FORM: [&str; 3] = ["3", "Down", "n"];

/// Presses `keys`, by the alphabet's names, in a TUI started in `term`,
/// and compares it with the reference after each key. It fails where the
/// tables do not decide a key, so a sequence that no longer means what it
/// meant fails instead of testing something else. A sequence that begins
/// with [`TO_FORM`] starts the reference in the new-frit form.
pub fn replay(term: Term, keys: &[&str]) -> Result<(), String> {
    let mut want = RefState::start(term);
    let mut sim = Sim::start(term, 80, 25);
    let mut from = 0;
    if keys.starts_with(&TO_FORM) {
        for key in TO_FORM {
            sim.press(super::press_of(key));
        }
        "looks.new-frit".clone_into(&mut want.node);
        "looks".clone_into(&mut want.tab);
        compare(&sim, &want).map_err(|e| format!("{}: {}: {e}", term.name(), TO_FORM.join(" ")))?;
        from = TO_FORM.len();
    }
    for (i, key) in keys.iter().enumerate().skip(from) {
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

/// The term and the keys that a line of `walk.proptest-regressions`
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

/// Each sequence of keys that `walk.proptest-regressions` records,
/// pressed as it is written and compared with the reference after each
/// key. A seed alone replays its case only while the tables and the
/// alphabet stay as they are, so this test keeps each case found once.
/// It fails on a line that does not read as a seed and its sequence, and
/// on a key that the tables no longer decide.
#[test]
fn the_sequences_proptest_found_replay_as_the_tables_say() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/scenarios/walk.proptest-regressions");
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
            failures.push(format!(
                "line {}: no sequence of keys after the seed",
                n + 1
            ));
            continue;
        };
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        if let Err(e) = replay(term, &keys) {
            failures.push(format!("line {}: {e}", n + 1));
        }
        replayed += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        replayed >= 3,
        "the file records {replayed} sequences, and it holds at least 3"
    );
}

/// The new-frit form opens on its first row each time `n` opens it,
/// after Esc left it on its add row or on a row in the middle. Down moves
/// from row to row, seven times from the text font to the add row.
/// [`TO_FORM`] opens the form from the Looks tab's first font set, and Esc
/// goes back to that font set, where `n` opens the form again. The
/// reference cannot decide that second `n`, so the test replays the keys
/// after it from the form, as a sequence of its own, once it has checked
/// that the second `n` opens the form on its first row.
#[test]
fn the_new_frit_form_opens_on_its_first_row_after_esc() {
    let rows = ["Down"; 7];
    for term in Term::ALL {
        for before in [&rows[..], &rows[..2]] {
            let first = [&TO_FORM[..], before, &["Esc"]].concat();
            replay(term, &first).unwrap_or_else(|e| panic!("{e}"));
            let mut sim = Sim::start(term, 80, 25);
            for key in &first {
                sim.press(super::press_of(key));
            }
            sim.press(super::press_of("n"));
            assert_eq!(
                state::of(&sim).node(),
                "looks.new-frit",
                "{}: {} n",
                term.name(),
                first.join(" ")
            );
            assert_eq!(
                sim.app.looks.form().at,
                0,
                "{}: {}",
                term.name(),
                first.join(" ")
            );
            let again = [&TO_FORM[..], &rows, &["Esc"]].concat();
            replay(term, &again).unwrap_or_else(|e| panic!("{e}"));
        }
    }
}
