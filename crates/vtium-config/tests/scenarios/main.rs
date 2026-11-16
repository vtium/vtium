//! Scenarios that drive vtium-config's TUI by script and check how it
//! responds after every key. The explorer walks the TUI's state machine
//! with no terminal, and its map of states and keys is committed beside
//! this file, one file for each kind of terminal. README.md says how the
//! map is read and how it is written again.

mod checks;
mod explore;
mod sim;
mod state;
mod tables;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use vtium_config::tui::input::Key;

use checks::Checks;
use explore::Map;
use sim::{Sim, Term};
use state::StateKind;

fn map_path(term: Term) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/scenarios")
        .join(format!("map-{}.tsv", term.name()))
}

/// The header of a map file.
const MAP_HEAD: &str = "# The transitions of vtium-config's TUI that tests/scenarios/main.rs explores: from-state, key, to-state, and what the key caused. A key that leaves the state as it was and causes nothing is left out.\nfrom\tkey\tto\teffects\n";

/// The map's text: every transition that changes the state or causes
/// something, sorted.
fn map_text(map: &Map) -> String {
    let lines: BTreeSet<String> = map
        .steps
        .iter()
        .filter(|s| !s.idle())
        .map(explore::Step::line)
        .collect();
    let mut text = MAP_HEAD.to_owned();
    for line in lines {
        text.push_str(&line);
        text.push('\n');
    }
    text
}

/// Every state the walk reached gets back to a state that leaves, so no
/// state traps the person.
fn unreachable_exits(map: &Map) -> Vec<String> {
    let mut back: BTreeMap<&state::Abs, Vec<&state::Abs>> = BTreeMap::new();
    for s in &map.steps {
        back.entry(&s.to).or_default().push(&s.from);
    }
    let mut good: BTreeSet<&state::Abs> = map
        .states
        .iter()
        .filter(|s| s.kind == StateKind::Left)
        .collect();
    let mut todo: Vec<&state::Abs> = good.iter().copied().collect();
    while let Some(s) = todo.pop() {
        for from in back.get(s).into_iter().flatten() {
            if good.insert(from) {
                todo.push(from);
            }
        }
    }
    map.states
        .iter()
        .filter(|s| !good.contains(s))
        .map(|s| format!("no key sequence leaves from {s}"))
        .collect()
}

/// The 0-switch run: every key in every state of every kind of terminal,
/// each terminal on a thread of its own, with every check after every
/// key, the map compared with the committed one, and the tables met.
#[test]
fn every_key_in_every_state_keeps_the_rules() {
    let runs: Vec<(Term, Map, Checks)> = std::thread::scope(|scope| {
        let handles: Vec<_> = Term::ALL
            .into_iter()
            .map(|term| {
                scope.spawn(move || {
                    let mut checks = Checks::new();
                    let map = explore::explore(term, |step, before, after| {
                        checks.see(step, before, after);
                    });
                    (term, map, checks)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("an explorer thread"))
            .collect()
    });
    let mut all = Checks::new();
    let mut failures = Vec::new();
    let bless = std::env::var_os("VTIUM_CONFIG_BLESS").is_some();
    for (term, map, checks) in runs {
        failures.extend(unreachable_exits(&map));
        let found = checks.found;
        failures.extend(found.failures);
        all.found.reached.extend(found.reached);
        for (node, keys) in found.nodes {
            all.found.nodes.entry(node).or_insert(keys);
        }
        for (view, parts) in found.writes {
            all.found.writes.entry(view).or_default().extend(parts);
        }
        for (view, shown) in found.actions {
            all.found.actions.entry(view).or_default().extend(shown);
        }
        let path = map_path(term);
        let text = map_text(&map);
        if bless {
            std::fs::write(&path, &text).expect("the map");
            continue;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        if committed != text {
            let (old, new): (BTreeSet<&str>, BTreeSet<&str>) =
                (committed.lines().collect(), text.lines().collect());
            let gone: Vec<&&str> = old.difference(&new).take(10).collect();
            let came: Vec<&&str> = new.difference(&old).take(10).collect();
            failures.push(format!(
                "the map of {} moved; with VTIUM_CONFIG_BLESS=1 the test writes it again.\n  gone: {gone:#?}\n  new: {came:#?}",
                path.display()
            ));
        }
    }
    all.finish(&tables::views());
    failures.extend(all.found.failures);
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// ADR-1403's interface tree holds together with its key table: every
/// parent is a node, every key row a node names is a row of the table,
/// every key row applies in at least one node, and every planned node says
/// in its `now` column what the TUI does today.
#[test]
fn the_interface_tree_and_the_key_table_name_each_other() {
    let tree = tables::tree();
    let rows = tables::keys();
    let mut failures = Vec::new();
    for node in &tree {
        if node.parent != "-" && !tree.iter().any(|n| n.id == node.parent) {
            failures.push(format!("{}'s parent {} is no node", node.id, node.parent));
        }
        for key in &node.keys {
            if !rows.iter().any(|r| r.id == *key) {
                failures.push(format!(
                    "{} names the key row {key}, which the key table lacks",
                    node.id
                ));
            }
        }
        if node.status == "planned" && node.now.is_empty() {
            failures.push(format!(
                "the planned node {} says nothing in its now column",
                node.id
            ));
        }
    }
    for row in &rows {
        if !tree.iter().any(|n| n.keys.contains(&row.id)) {
            failures.push(format!("the key row {} applies in no node", row.id));
        }
    }
    assert!(
        tree.len() > 20 && rows.len() > 50,
        "{} nodes, {} rows",
        tree.len(),
        rows.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The 1-switch run over questions, typing and leaving: from the state
/// after each transition into a question or into typing, every key; and
/// after every transition, the keys that leave, `q` and Ctrl-C. It runs
/// at the end of a card that changes the TUI, in a release build:
/// `cargo test --release -p vtium-config --test scenarios -- --ignored`.
#[test]
#[ignore = "the 1-switch run needs a release build; it runs at the end of a card"]
fn every_pair_of_keys_through_questions_typing_and_leaving_keeps_the_rules() {
    let failures: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = Term::ALL
            .into_iter()
            .map(|term| scope.spawn(move || one_switch(term)))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("an explorer thread"))
            .collect()
    });
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn one_switch(term: Term) -> Vec<String> {
    let map = explore::explore(term, |_, _, _| {});
    let mut checks = Checks::new();
    for first in &map.steps {
        let Some((rep, path, start)) = map.reps.get(&first.from) else {
            continue;
        };
        let into = matches!(first.to.kind, StateKind::Question | StateKind::Typing);
        let mut middle = rep.clone();
        middle.press(press_of(first.key));
        if middle.exited.is_some() {
            continue;
        }
        let from = state::of(&middle);
        let mut keys = path.clone();
        keys.push(first.key);
        for (name, press) in explore::ALPHABET {
            if !into && !matches!(*name, "q" | "Ctrl-C") {
                continue;
            }
            let mut after = middle.clone();
            let did = after.press(*press);
            let step = explore::Step {
                from: from.clone(),
                key: name,
                to: state::of(&after),
                did,
                path: keys.clone(),
                start: *start,
            };
            checks.see(&step, &middle, &after);
        }
    }
    checks.found.failures
}

fn press_of(name: &str) -> sim::Press {
    explore::ALPHABET
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, p)| *p)
        .unwrap_or(sim::Press::Key(Key::Char('z')))
}

/// Presses `keys` in `sim`, by the alphabet's names.
fn press(sim: &mut Sim, keys: &[&str]) {
    for k in keys {
        sim.press(press_of(k));
    }
}

/// The leave question of #411: with a font set marked for removal and a
/// change not saved, `q` asks the leave question, which names the change.
/// `y` saves the change and installs before the TUI quits, `n` saves the
/// change and quits without installing, Esc and Ctrl-C cancel the
/// question, and every other key, `l` and `i` among them, leaves it open.
///
/// References:
/// - ADR-1403 D7, "The same keys in every view": the answer keys of every
///   question, and the leave question's `y` and `n`.
#[test]
fn the_leave_question_names_the_unsaved_change_and_both_answers_save_it() {
    let mut sim = Sim::start(Term::Vtium, 80, 25);
    // in Fonts: proof as the font, one change; tiny5 marked for removal
    press(&mut sim, &["Up", "Up", "Enter", "Down", "r", "q"]);
    assert_eq!(sim.app.unsaved, 1);
    assert_eq!(sim.app.pending.removals, ["tiny5"]);
    let question = sim
        .app
        .confirm
        .clone()
        .expect("the leave question")
        .question;
    assert!(question.contains("the removal of tiny5"), "{question}");
    assert!(
        question.contains("the 1 change that is not saved"),
        "{question}"
    );
    for stray in ["l", "i", "z", "Tab"] {
        let mut asked = sim.clone();
        asked.press(press_of(stray));
        assert_eq!(asked.exited, None, "{stray} left");
        assert!(asked.app.confirm.is_some(), "{stray} closed the question");
        assert_eq!(asked.desk.writes, 0, "{stray} saved");
    }
    for no in ["Esc", "Ctrl-C"] {
        let mut asked = sim.clone();
        asked.press(press_of(no));
        assert_eq!(asked.exited, None, "{no} left");
        assert!(asked.app.confirm.is_none(), "{no} kept the question open");
        assert_eq!((asked.desk.writes, asked.app.unsaved), (0, 1), "{no}");
    }
    let mut n = sim.clone();
    n.press(press_of("n"));
    assert_eq!((n.exited, n.desk.writes), (Some(0), 1));
    assert_eq!(n.desk.installed, sim.desk.installed, "n installed");
    sim.press(press_of("y"));
    assert_eq!((sim.exited, sim.desk.writes), (Some(0), 1));
    let font = vtium_settings::setting(&sim.desk.installed, "font-name");
    assert_eq!(font.as_deref(), Some("proof"), "{}", sim.desk.installed);
}

/// Without a change waiting, the leave question's `y` installs the font
/// set's removal and quits, and saves nothing.
#[test]
fn the_leave_question_with_nothing_unsaved_installs_on_y() {
    let mut sim = Sim::start(Term::Vtium, 80, 25);
    press(&mut sim, &["Up", "r", "q"]);
    let question = sim
        .app
        .confirm
        .clone()
        .expect("the leave question")
        .question;
    assert!(!question.contains("not saved"), "{question}");
    let did = sim.press(press_of("y"));
    assert_eq!(sim.exited, Some(0));
    assert_eq!(sim.desk.writes, 0);
    assert!(did.effects.iter().any(|e| e == "install"), "{did:?}");
}

/// Below the minimum size, `q` with a change not saved asks the quit
/// question on one row: `y` saves and quits, `n` quits without saving,
/// and Esc goes back.
///
/// References:
/// - ADR-1201 D3: below the minimum size only `q`, `u` and Ctrl-C act.
/// - ADR-1403 D7, "What carries over": `q` asks before it drops changes
///   that are not saved.
#[test]
fn below_the_minimum_size_q_asks_the_quit_question_on_one_row() {
    let mut sim = Sim::start(Term::Vtium, 80, 25);
    press(&mut sim, &["Up", "Enter", "Shrink", "q"]);
    assert_eq!(sim.app.unsaved, 1);
    assert_eq!(sim.exited, None);
    let first = sim.screen().first().cloned().unwrap_or_default();
    assert!(
        first.starts_with("1 change is not saved.  y save and quit"),
        "{first:?}"
    );
    let mut back = sim.clone();
    back.press(press_of("Esc"));
    assert_eq!((back.exited, back.app.unsaved), (None, 1));
    let mut drop = sim.clone();
    drop.press(press_of("n"));
    assert_eq!((drop.exited, drop.desk.writes), (Some(0), 0));
    sim.press(press_of("y"));
    assert_eq!((sim.exited, sim.desk.writes), (Some(0), 1));
}

/// While a field of the new-frit form is typed, only Enter and Esc act:
/// the moving keys leave the field as it is, Enter keeps it and moves to
/// the next field, and Esc closes the form.
///
/// References:
/// - ADR-1403 D7, "The same keys in every view": a value being typed takes
///   Enter and Esc, as in the Settings view, and the moving keys do
///   nothing.
#[test]
fn a_typed_field_of_the_new_frit_form_takes_only_enter_and_esc() {
    let mut sim = Sim::start(Term::Vtium, 80, 25);
    press(&mut sim, &["n", "x"]);
    assert_eq!(sim.app.view, vtium_config::tui::model::View::New);
    assert_eq!(
        sim.app.new_frit.fields.first().map(String::as_str),
        Some("x")
    );
    for moving in ["Up", "Down", "Home", "End", "PgUp", "PgDn", "Left", "Right"] {
        let mut typed = sim.clone();
        let did = typed.press(press_of(moving));
        assert_eq!(typed.app.new_frit, sim.app.new_frit, "{moving} moved");
        assert!(did.effects.is_empty(), "{moving}: {did:?}");
    }
    let mut kept = sim.clone();
    press(&mut kept, &["Enter"]);
    assert_eq!(kept.app.new_frit.at, 1);
    press(&mut sim, &["Esc"]);
    assert_eq!(sim.app.view, vtium_config::tui::model::View::Fonts);
}
