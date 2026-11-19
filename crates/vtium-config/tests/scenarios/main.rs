//! Scenarios that drive vtium-config's TUI by script and check how it
//! responds after every key. The explorer walks the TUI's state machine
//! with no terminal, and its map of states and keys is committed beside
//! this file, one file for each kind of terminal. README.md says how the
//! map is read and how it is written again.

mod checks;
mod explore;
mod reference;
mod sim;
mod state;
mod tables;
mod walk;

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

/// The keys a terminal's card acts on its line with, which do something
/// else on each kind of line.
const CARD_KEYS: [&str; 3] = ["Enter", "Left", "Right"];

/// The lines of each terminal card the walk opens, which its abstract
/// state does not tell apart: from the TUI that stands for each card, and
/// from NEW PATH's terminal box opened with Enter, Down to each line in
/// turn, then each of [`CARD_KEYS`] on it, each
/// transition checked as the walk's are. The map records none of them, so
/// it stays one line for each abstract state and key.
fn card_lines(map: &Map, checks: &mut Checks) {
    for (abs, (sim, path, start)) in &map.reps {
        let mut at = sim.clone();
        let mut keys = path.clone();
        // NEW PATH's card, which has lines of its own, opens from its box
        if abs.node() == "flow" && abs.focus == "new terminal" {
            at.press(press_of("Enter"));
            keys.push("Enter");
        } else if !matches!(abs.node().as_str(), "flow.card" | "flow.new-card") {
            continue;
        }
        let lines = Checks::card_line_count(&at);
        for line in 0..lines {
            if line > 0 {
                at.press(press_of("Down"));
                keys.push("Down");
            }
            for key in CARD_KEYS {
                let mut after = at.clone();
                let did = after.press(press_of(key));
                let step = explore::Step {
                    from: state::of(&at),
                    key,
                    to: state::of(&after),
                    did,
                    path: keys.clone(),
                    start: *start,
                };
                checks.see(&step, &at, &after);
            }
        }
    }
}

/// Every key from the second page of a paged key line, each transition
/// checked as the walk's are. The walk itself never presses `.` before a
/// key that leaves a node, so this is where the rule that a key line shows
/// its first page after a key that moves to another node is checked. The
/// map records none of these transitions.
fn from_a_second_page(map: &Map, checks: &mut Checks) {
    for (abs, (sim, path, start)) in &map.reps {
        let line = vtium_config::tui::layout::key_line(&sim.app);
        if !line
            .rsplit("  ")
            .next()
            .is_some_and(|i| i.starts_with(". more "))
        {
            continue;
        }
        let mut paged = sim.clone();
        paged.press(press_of("."));
        let mut keys = path.clone();
        keys.push(".");
        let from = state::of(&paged);
        if from != *abs {
            continue;
        }
        for (name, press) in explore::ALPHABET {
            let mut after = paged.clone();
            let did = after.press(*press);
            let step = explore::Step {
                from: from.clone(),
                key: name,
                to: state::of(&after),
                did,
                path: keys.clone(),
                start: *start,
            };
            checks.see(&step, &paged, &after);
        }
    }
}

/// Whether a key changed anything a person can see or that the TUI did:
/// the screen with its colors, so a highlight that moves counts, the
/// abstract state, an effect, or leaving the TUI.
fn acted(before: &Sim, after: &Sim, did: &sim::Did) -> bool {
    let drawn = |sim: &Sim| vtium_config::tui::view::screen(&sim.app);
    !did.effects.is_empty()
        || after.exited.is_some()
        || state::of(before) != state::of(after)
        || drawn(before) != drawn(after)
}

/// The key a person presses for one of the key table's names of keys:
/// the alphabet's own name, or a letter for `any letter`.
fn pressed_for(row: &tables::KeyRow) -> Vec<&'static str> {
    if row.letters {
        vec!["x"]
    } else {
        row.keys.clone()
    }
}

/// The keys that move a highlight or a cursor, which the check below
/// presses two at a time to leave the edge of a list: Right and then Left
/// moves a card's choice off its first one only once Down has reached a
/// line that has choices.
const MOVING: [&str; 8] = ["Up", "Down", "Left", "Right", "Home", "End", "PgUp", "PgDn"];

/// The rows each key line names, by node, whose key acts in that node:
/// for every row a supported node lists with a label, so that its key line
/// shows it, each of its keys is pressed in every state of the node the
/// walk reached. Where the key changes nothing there, it is pressed again
/// after each key of the alphabet that keeps the TUI in the node, and then
/// after each pair of [`MOVING`] keys that does, which moves the highlight
/// away from the edge of a list. The set holds each node, row and key that
/// acted at least once.
fn named_keys_that_act(map: &Map) -> BTreeSet<(String, String, &'static str)> {
    let rows = tables::keys();
    let mut acted_once = BTreeSet::new();
    for node in tables::tree().iter().filter(|n| n.status == "supported") {
        let named: Vec<&tables::KeyRow> = rows
            .iter()
            .filter(|r| {
                node.keys.contains(&r.id)
                    && r.status == "supported"
                    && r.label != "-"
                    && !r.key_line
                    && r.keys != ["."]
            })
            .collect();
        let reps: Vec<&Sim> = map
            .reps
            .iter()
            .filter(|(abs, _)| abs.node() == node.id)
            .map(|(_, (sim, _, _))| sim)
            .collect();
        for row in named {
            for key in pressed_for(row) {
                // the key acts after `first`, where `first` keeps the TUI
                // in the node
                let acts_after = |rep: &Sim, first: &[sim::Press]| {
                    let mut moved = rep.clone();
                    for p in first {
                        moved.press(*p);
                    }
                    if moved.exited.is_some() || state::of(&moved).node() != node.id {
                        return false;
                    }
                    let mut after = moved.clone();
                    let did = after.press(press_of(key));
                    acted(&moved, &after, &did)
                };
                let singles = || explore::ALPHABET.iter().map(|(_, p)| vec![*p]);
                let pairs = || {
                    MOVING
                        .iter()
                        .flat_map(|a| MOVING.iter().map(move |b| vec![press_of(a), press_of(b)]))
                };
                let acts = reps.iter().any(|rep| acts_after(rep, &[]))
                    || reps
                        .iter()
                        .any(|rep| singles().any(|f| acts_after(rep, &f)))
                    || reps.iter().any(|rep| pairs().any(|f| acts_after(rep, &f)));
                if acts {
                    acted_once.insert((node.id.clone(), row.id.clone(), key));
                }
            }
        }
    }
    acted_once
}

/// Every key a key line names does something in the node whose key line
/// names it, in at least one kind of terminal: a key line that offers a
/// key that does nothing tells the person something false. A key that
/// does nothing in a node has no label there, so its key line leaves it
/// out.
///
/// References:
/// - ADR-1403 D7, "The contract": the key lines are generated from the
///   key table, and the scenario test proves that the code does what the
///   table says.
fn every_named_key_acts(acted_once: &BTreeSet<(String, String, &'static str)>) -> Vec<String> {
    let rows = tables::keys();
    let mut out = Vec::new();
    for node in tables::tree().iter().filter(|n| n.status == "supported") {
        for row in rows.iter().filter(|r| {
            node.keys.contains(&r.id)
                && r.status == "supported"
                && r.label != "-"
                && !r.key_line
                && r.keys != ["."]
        }) {
            for key in pressed_for(row) {
                if !acted_once.contains(&(node.id.clone(), row.id.clone(), key)) {
                    out.push(format!(
                        "the key line of {} shows {key} as \"{}\" (row {}), and {key} changes nothing there in any state the walk reached",
                        node.id, row.label, row.id
                    ));
                }
            }
        }
    }
    out
}

/// Where the key took the TUI, against where the reading of the tables in
/// `reference.rs` says it goes, for the keys whose effect the tables
/// decide whatever a tab holds. A failure names the scenario, the shortest
/// keys from the start that take the transition.
fn as_the_tables_say(step: &explore::Step) -> Option<String> {
    if step.from.kind == StateKind::Left {
        return None;
    }
    let want = reference::expect(&step.from.node(), step.key)?;
    let to = if step.to.kind == StateKind::Left {
        String::new()
    } else {
        step.to.node()
    };
    let mut agrees = reference::agrees(&want, &to, step.did.exit);
    // a key that leaves the TUI where it was leaves the tab under it too
    // below the minimum size no tab shows, so there is none to compare
    let shown = !step.from.view.is_empty() && !step.to.view.is_empty();
    if agrees.is_ok()
        && shown
        && want == reference::Expect::At(step.from.node())
        && step.to.view != step.from.view
    {
        agrees = Err(format!(
            "the tables keep the TUI at {to}, and the tab under it went from {} to {}",
            step.from.view, step.to.view
        ));
    }
    agrees
        .err()
        .map(|e| format!("{}: from {}: {e}", step.scenario(), step.from.node()))
}

/// The 0-switch run: every key in every state of every kind of terminal,
/// each terminal on a thread of its own, with every check after every
/// key, the map compared with the committed one, and the tables met.
#[test]
fn every_key_in_every_state_keeps_the_rules() {
    type Run = (Term, Map, Checks, BTreeSet<(String, String, &'static str)>);
    let runs: Vec<Run> = std::thread::scope(|scope| {
        let handles: Vec<_> = Term::ALL
            .into_iter()
            .map(|term| {
                scope.spawn(move || {
                    let mut checks = Checks::new();
                    let mut moved = Vec::new();
                    let map = explore::explore(term, |step, before, after| {
                        checks.see(step, before, after);
                        moved.extend(as_the_tables_say(step));
                    });
                    card_lines(&map, &mut checks);
                    from_a_second_page(&map, &mut checks);
                    let acts = named_keys_that_act(&map);
                    checks.found.failures.extend(moved);
                    (term, map, checks, acts)
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
    let mut acted_once = BTreeSet::new();
    for (term, map, checks, acts) in runs {
        acted_once.extend(acts);
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
    failures.extend(every_named_key_acts(&acted_once));
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
    assert_eq!(sim.app.shared.unsaved, 1);
    assert_eq!(sim.app.shared.pending.removals, ["tiny5"]);
    let question = sim
        .app
        .nav
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
        assert!(
            asked.app.nav.confirm.is_some(),
            "{stray} closed the question"
        );
        assert_eq!(asked.desk.writes, 0, "{stray} saved");
    }
    for no in ["Esc", "Ctrl-C"] {
        let mut asked = sim.clone();
        asked.press(press_of(no));
        assert_eq!(asked.exited, None, "{no} left");
        assert!(
            asked.app.nav.confirm.is_none(),
            "{no} kept the question open"
        );
        assert_eq!(
            (asked.desk.writes, asked.app.shared.unsaved),
            (0, 1),
            "{no}"
        );
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
        .nav
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
    assert_eq!(sim.app.shared.unsaved, 1);
    assert_eq!(sim.exited, None);
    let first = sim.screen().first().cloned().unwrap_or_default();
    assert!(
        first.starts_with("1 change is not saved.  y save and quit"),
        "{first:?}"
    );
    let mut back = sim.clone();
    back.press(press_of("Esc"));
    assert_eq!((back.exited, back.app.shared.unsaved), (None, 1));
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
    assert_eq!(sim.app.nav.view, vtium_config::tui::model::View::New);
    assert_eq!(
        sim.app.fonts.form().fields.first().map(String::as_str),
        Some("x")
    );
    for moving in ["Up", "Down", "Home", "End", "PgUp", "PgDn", "Left", "Right"] {
        let mut typed = sim.clone();
        let did = typed.press(press_of(moving));
        assert_eq!(
            typed.app.fonts.form(),
            sim.app.fonts.form(),
            "{moving} moved"
        );
        assert!(did.effects.is_empty(), "{moving}: {did:?}");
    }
    let mut kept = sim.clone();
    press(&mut kept, &["Enter"]);
    assert_eq!(kept.app.fonts.form().at, 1);
    press(&mut sim, &["Esc"]);
    assert_eq!(sim.app.nav.view, vtium_config::tui::model::View::Fonts);
}

/// A Settings key's list of values opens on the value the key has now, and
/// Enter there writes the highlighted value. Esc from a section goes back
/// to the sections with that section highlighted.
///
/// References:
/// - ADR-1201 D3, the Settings bullet: Esc goes back to the sections with
///   the same section highlighted.
/// - ADR-1403 D7, "One list for every set of items": a key's values open
///   in the shared list.
#[test]
fn a_value_list_opens_on_the_value_now_and_esc_keeps_the_section() {
    use vtium_config::tui::model::View;
    let mut sim = Sim::start(Term::Vtium, 80, 25);
    for _ in 0..View::TABS.len() {
        if sim.app.nav.view != View::Settings {
            press(&mut sim, &["Tab"]);
        }
    }
    assert_eq!(sim.app.nav.view, View::Settings);
    // the first section's last key is cluster-width, which takes double,
    // its default, or single
    press(&mut sim, &["Enter", "End"]);
    assert_eq!(
        sim.app.settings.key().map(|k| k.name),
        Some("cluster-width")
    );
    press(&mut sim, &["Enter"]);
    assert_eq!(sim.app.settings.values().map(|l| l.at), Some(0));
    press(&mut sim, &["Down", "Enter"]);
    assert_eq!(sim.app.settings.values(), None);
    let draft = sim
        .app
        .shared
        .draft
        .as_ref()
        .map(|d| d.text.clone())
        .unwrap_or_default();
    assert!(draft.contains("\ncluster-width=single\n"), "{draft}");
    press(&mut sim, &["Enter"]);
    assert_eq!(sim.app.settings.values().map(|l| l.at), Some(1));
    press(&mut sim, &["Esc", "Esc"]);
    assert_eq!(sim.app.settings.open(), None);
    press(&mut sim, &["Down", "Down", "Enter"]);
    assert_eq!(sim.app.settings.open().map(|(at, _)| at), Some(2));
    press(&mut sim, &["Esc"]);
    assert_eq!(sim.app.settings.open(), None);
    assert_eq!(sim.app.settings.section_at(), 2);
}

/// The leave question's `y` installs a draft that wires no complete route
/// only after the person answers the question that says so, and quits
/// once that install ends well. Esc at that question keeps the TUI, and a
/// later `i` asks before it installs.
///
/// References:
/// - ADR-1403 D7: a draft with no complete route installs only on yes.
#[test]
fn the_leave_question_asks_before_it_installs_a_draft_with_no_complete_route() {
    let mut sim = Sim::start(Term::Vtium, 80, 25);
    sim.desk.no_route = true;
    press(&mut sim, &["Up", "r", "q"]);
    let did = sim.press(press_of("y"));
    assert!(!did.effects.iter().any(|e| e == "install"), "{did:?}");
    assert_eq!(sim.exited, None);
    let question = sim
        .app
        .nav
        .confirm
        .clone()
        .expect("the no-route question")
        .question;
    assert!(question.contains("complete"), "{question}");
    let mut back = sim.clone();
    back.press(press_of("Esc"));
    let did = back.press(press_of("i"));
    assert!(!did.effects.iter().any(|e| e == "install"), "{did:?}");
    assert!(back.app.nav.confirm.is_some(), "i installed without asking");
    let did = sim.press(press_of("y"));
    assert!(did.effects.iter().any(|e| e == "install"), "{did:?}");
    assert_eq!(sim.exited, Some(0));
}

/// With no draft, `y` at the leave question installs nothing and stays,
/// and once a draft opens, `i` asks before it installs, as it always does.
#[test]
fn the_leave_question_without_a_draft_leaves_no_install_waiting() {
    let mut sim = Sim::start(Term::Vtium, 80, 25);
    let draft = sim.app.shared.draft.take();
    press(&mut sim, &["Up", "r", "q", "y"]);
    assert_eq!(sim.exited, None);
    sim.app.shared.draft = draft;
    let did = sim.press(press_of("i"));
    assert!(!did.effects.iter().any(|e| e == "install"), "{did:?}");
    assert_eq!(sim.exited, None);
    assert!(sim.app.nav.confirm.is_some(), "i installed without asking");
}

/// Below the minimum size, the leave question says what quitting loses
/// and that both answers save the change that is not saved, above its
/// keys.
#[test]
fn below_the_minimum_size_the_leave_question_says_what_it_does() {
    let mut sim = Sim::start(Term::Vtium, 80, 25);
    press(&mut sim, &["Up", "Up", "Enter", "Down", "r", "Shrink", "q"]);
    let screen = sim.screen();
    let top = screen
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(top.contains("Font sets wait to be installed"), "{top}");
    assert!(top.contains("1 change"), "{top}");
    assert!(top.contains("y install and quit"), "{top}");
}

/// The screen's rows joined with single spaces, so a line that wraps
/// reads as one.
fn joined(screen: &[String]) -> String {
    screen
        .iter()
        .flat_map(|row| row.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Below the minimum size, a question shows whole however narrow or short
/// the screen: what it says and every key it takes wrap to the width and
/// come first, a key whose words are wider than the screen included. While
/// the question is open the screen shows the question's keys, never the
/// screen's own key line, since q does nothing under a question.
///
/// References:
/// - ADR-1201 D3: below the minimum size only `q`, `u` and Ctrl-C act.
/// - ADR-1403 D7, "The same keys in every view": the answer keys of every
///   question.
#[test]
fn below_the_minimum_size_a_question_wraps_and_keeps_its_keys() {
    let quit = [
        "1 change is not saved.",
        "y save and quit",
        "n quit without saving",
        "Esc Ctrl-C cancel",
    ];
    let leave = [
        "Font sets wait to be installed; y and n save 1 change first.",
        "y install and quit",
        "n quit without installing",
        "Esc Ctrl-C cancel",
    ];
    for (name, keys, said) in [
        ("quit", &["Up", "Enter"][..], quit),
        ("leave", &["Up", "Up", "Enter", "Down", "r"][..], leave),
    ] {
        for (cols, rows) in [
            (60, 30),
            (50, 25),
            (40, 12),
            (20, 12),
            (12, 30),
            (80, 3),
            (80, 2),
        ] {
            let mut sim = Sim::start(Term::Vtium, 80, 25);
            press(&mut sim, keys);
            sim.press(sim::Press::Resize(cols, rows));
            press(&mut sim, &["q"]);
            assert!(sim.app.nav.confirm.is_some(), "{name} at {cols}x{rows}");
            let screen = sim.screen();
            let text = joined(&screen);
            for part in said {
                assert!(
                    text.contains(part),
                    "{name} at {cols}x{rows}: {part:?} in {screen:#?}"
                );
            }
            assert!(
                !text.contains("q quit") && !text.contains("press u"),
                "{name} at {cols}x{rows}: {screen:#?}"
            );
            assert!(
                screen.iter().all(|r| r.trim_end().chars().count() <= cols),
                "{screen:#?}"
            );
        }
    }
}
