//! The scenario walk of the rebuilt TUI: vtium-config-tui's App with the
//! six tab crates' tabs, driven by script and checked after every key.
//! The explorer walks the TUI with no terminal, and its map of states and
//! keys is committed beside this file, one file for each kind of
//! terminal. README.md says how the map is read and written again.

mod checks;
mod explore;
mod reference;
mod run;
mod screens;
mod sim;
mod state;
mod tables;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use explore::{Map, Step};
use sim::{Sim, Term};
use state::{Abs, StateKind};

/// Work spread over the machine's cores, with its results in the order of
/// the items, so what a test reports does not depend on which thread
/// finished first.
mod par {
    /// The threads the walk uses: one for each core.
    pub fn threads() -> usize {
        std::thread::available_parallelism().map_or(1, usize::from)
    }

    /// Runs `work` on each of `items` on as many threads as the machine
    /// has cores. Each thread takes the next item no thread has taken yet,
    /// so a few slow items do not hold up the rest. A panic in `work`
    /// reaches the caller with its own message.
    pub fn map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let threads = threads().clamp(1, items.len().max(1));
        let next = AtomicUsize::new(0);
        let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut out = Vec::new();
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(item) = items.get(i) else {
                                return out;
                            };
                            out.push((i, work(item)));
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
                .collect()
        });
        done.sort_unstable_by_key(|(i, _)| *i);
        done.into_iter().map(|(_, r)| r).collect()
    }
}

fn map_path(term: Term) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/walk")
        .join(format!("map-{}.tsv", term.name()))
}

/// The header of a map file.
const MAP_HEAD: &str = "# The transitions of vtium-config's rebuilt TUI that tests/walk/main.rs explores: from-state, key, to-state, and what the key caused. A key that leaves the state as it was and causes nothing is left out.\nfrom\tkey\tto\teffects\n";

/// The map's text: every transition that changes the state or causes
/// something, sorted.
fn map_text(map: &Map) -> String {
    let lines: BTreeSet<String> = map
        .steps
        .iter()
        .filter(|s| !s.idle())
        .map(Step::line)
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
    let mut back: BTreeMap<&Abs, Vec<&Abs>> = BTreeMap::new();
    for s in &map.steps {
        back.entry(&s.to).or_default().push(&s.from);
    }
    let mut good: BTreeSet<&Abs> = map
        .states
        .iter()
        .filter(|s| s.kind == StateKind::Left)
        .collect();
    let mut todo: Vec<&Abs> = good.iter().copied().collect();
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

/// A key that no row of the key table names, F12, which acts nowhere but
/// clears the message line, as every key does.
const INERT: sim::Press = sim::Press::Key(vtium_config_widgets::keys::Key::plain(
    vtium_config_widgets::keys::Code::F(12),
));

/// The buttons each state's action bar offers, by the node they are on and
/// the row they act as, and whether one of the row's keys changes
/// something there: at once in some state of the node, or else after one
/// of the keys that move, which takes the highlight off the edge of a list
/// to an item the key acts on, or after a letter typed, which gives a
/// typed value something to delete.
fn buttons_that_act(map: &Map) -> BTreeMap<(String, String), bool> {
    let t = tables::tables();
    let reps: Vec<(&Abs, &explore::Reached)> = map
        .reps
        .iter()
        .filter(|(abs, _)| {
            !matches!(
                abs.kind,
                StateKind::Left | StateKind::TooSmall | StateKind::Menu | StateKind::Trial
            )
        })
        .collect();
    // whether `key` acts in the state `abs` after `first`, where `first`
    // keeps the TUI in the state's node
    let acts_after = |abs: &Abs,
                      (path, start): &explore::Reached,
                      first: Option<&'static str>,
                      key: &'static str| {
        let mut moved = Sim::reach(*start, path);
        let mut keys = path.clone();
        if let Some(f) = first {
            moved.press(explore::press_of(f));
            keys.push(f);
            if moved.exited.is_some() || moved.app.node() != abs.node {
                return false;
            }
        }
        // a key no row names clears the message line, which any key
        // clears, so the key under test is not credited with that
        moved.press(INERT);
        if moved.exited.is_some() || moved.app.node() != abs.node {
            return false;
        }
        let screen = moved.screen();
        let from = state::of(&moved);
        let did = moved.press(explore::press_of(key));
        let step = Step {
            from,
            key,
            to: state::of(&moved),
            did,
            path: keys,
            start: *start,
        };
        checks::acted(&screen, &moved.screen(), &step)
    };
    // each state's buttons, by their rows and the key each is pressed by
    let offered = par::map(&reps, |(_, (path, start))| {
        let rep = Sim::reach(*start, path);
        let layout = vtium_config_tui::view::layout(&rep.app);
        layout
            .bar
            .buttons
            .iter()
            .filter_map(|b| b.action)
            .filter_map(|id| Some((id, t.row(id)?.keys.first().copied()?)))
            .collect::<Vec<_>>()
    });
    let asks: Vec<(usize, &'static str, &'static str)> = offered
        .iter()
        .enumerate()
        .flat_map(|(i, b)| b.iter().map(move |(id, key)| (i, *id, *key)))
        .collect();
    let at_once = par::map(&asks, |(i, _, key)| {
        reps.get(*i)
            .is_some_and(|(abs, reached)| acts_after(abs, reached, None, key))
    });
    let mut all: BTreeMap<(String, String), bool> = BTreeMap::new();
    for ((i, id, _), acts) in asks.iter().zip(&at_once) {
        if let Some((abs, _)) = reps.get(*i) {
            *all.entry((abs.node.to_owned(), (*id).to_owned()))
                .or_default() |= acts;
        }
    }
    // the rest, after a key that moves or a letter, in every state
    let later: Vec<&(usize, &'static str, &'static str)> = asks
        .iter()
        .filter(|(i, id, _)| {
            reps.get(*i)
                .is_some_and(|(abs, _)| !all[&(abs.node.to_owned(), (*id).to_owned())])
        })
        .collect();
    let after = par::map(&later, |(i, _, key)| {
        reps.get(*i).is_some_and(|(abs, reached)| {
            checks::MOVING
                .iter()
                .chain(&["e"])
                .any(|m| acts_after(abs, reached, Some(*m), key))
        })
    });
    for ((i, id, _), acts) in later.iter().zip(after) {
        if let Some((abs, _)) = reps.get(*i) {
            *all.entry((abs.node.to_owned(), (*id).to_owned()))
                .or_default() |= acts;
        }
    }
    all
}

/// The 0-switch run: every key in every state of every kind of terminal,
/// with every check after every key, the map compared with the committed
/// one, and the tables met. The explorer walks each kind of terminal in
/// turn, each on all the machine's cores.
///
/// References:
/// - ADR-1206 D5: the exhaustive walk is the deterministic gate for
///   navigation.
#[test]
fn every_key_in_every_state_keeps_the_rules() {
    let bless = std::env::var_os("VTIUM_CONFIG_BLESS").is_some();
    let mut failures = Vec::new();
    let mut reached: BTreeSet<&'static str> = BTreeSet::new();
    let mut acting: BTreeMap<(String, String), bool> = BTreeMap::new();
    for term in Term::ALL {
        let map = explore::explore(term);
        failures.extend(map.failures.iter().cloned());
        failures.extend(unreachable_exits(&map));
        reached.extend(map.states.iter().map(|s| s.node));
        for (k, acts) in buttons_that_act(&map) {
            *acting.entry(k).or_default() |= acts;
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
    for ((node, row), acts) in &acting {
        if !acts {
            failures.push(format!(
                "the action bar of {node} offers the row {row}, and its key changes nothing there in any state the walk reached"
            ));
        }
    }
    failures.extend(checks::unreached(&reached));
    let failures = checks::known(failures);
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// ADR-1403's interface tree holds together with its key table, the
/// overlay of D11 included: every parent is a node, every key row a node
/// names is a row of the table, and every key row applies in at least one
/// node.
#[test]
fn the_interface_tree_and_the_key_table_name_each_other() {
    let t = tables::tables();
    let mut failures = Vec::new();
    for node in &t.tree {
        if node.parent != "-" && t.node(&node.parent).is_none() {
            failures.push(format!("{}'s parent {} is no node", node.id, node.parent));
        }
        for key in &node.keys {
            if t.row(key).is_none() {
                failures.push(format!(
                    "{} names the key row {key}, which the key table lacks",
                    node.id
                ));
            }
        }
    }
    for row in &t.rows {
        if !t.tree.iter().any(|n| n.keys.contains(&row.id)) {
            failures.push(format!("the key row {} applies in no node", row.id));
        }
    }
    assert!(
        t.tree.len() > 40 && t.rows.len() > 100,
        "{} nodes, {} rows",
        t.tree.len(),
        t.rows.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The tab a node of the tree belongs to, by its place in the top row: the
/// tab whose node is the node's first part, or that part's parent where
/// the part is the tab's list in another focus, as `looks-resources` is
/// for `looks`. `None` for the nodes over every tab, `any.` and `tui`.
fn tab_of_node(tabs: &[vtium_config_tui::BoxTab], id: &str) -> Option<usize> {
    let root = reference::tab_of(id.split('.').next()?);
    tabs.iter().position(|t| t.node() == root)
}

/// The trait and the tree agree: a node whose `search` column names fields
/// belongs to a tab that is `Searchable`, and a `Searchable` tab's search
/// node names fields in the tree. A list box's search, the field `choice`,
/// is the `Choices` trait's, which every list box implements, so its
/// nodes are left out, as are the nodes over every tab.
///
/// References:
/// - ADR-1206 D9, "The trait and the interface tree agree": a test checks
///   both directions.
#[test]
fn the_searchable_tabs_and_the_tree_s_search_fields_agree() {
    let t = tables::tables();
    let tabs = sim::tabs(true);
    let mut failures = Vec::new();
    let list_box = |n: &tables::TreeNode| n.search == ["choice"];
    for node in t
        .tree
        .iter()
        .filter(|n| n.supported && !n.search.is_empty() && !list_box(n))
    {
        let Some(i) = tab_of_node(&tabs, &node.id) else {
            continue;
        };
        let Some(tab) = tabs.get(i) else { continue };
        if tab.searchable().is_none() {
            failures.push(format!(
                "the node {} names the search fields {:?}, and its tab {} is not Searchable",
                node.id,
                node.search,
                tab.name()
            ));
        }
    }
    for tab in &tabs {
        let Some(s) = tab.searchable() else { continue };
        let at = s.search_node();
        match t.node(at) {
            Some(n) if !n.search.is_empty() => {}
            _ => failures.push(format!(
                "the {} tab is Searchable on {at}, whose node names no search fields",
                tab.name()
            )),
        }
        if tab_of_node(&tabs, at)
            .and_then(|i| tabs.get(i))
            .map(|t| t.name())
            != Some(tab.name())
        {
            failures.push(format!(
                "the {} tab is Searchable on {at}, a node of another tab",
                tab.name()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every item of the File and Actions menus is routed over every tab: it
/// has a row of the key table, or is greyed out with the reason it cannot
/// act there, the shell acts on that row or hands it to the tab, and no
/// item is greyed out as not built yet.
///
/// References:
/// - ADR-1206 D9: every File and Actions item reaches its action.
/// - ADR-1403 D11, "The menus": the items of File and Actions.
#[test]
fn every_file_and_actions_item_is_routed() {
    use vtium_config_tui::app::update::{Route, route};
    use vtium_config_tui::menus::{self, Context, Line, NOT_BUILT, Submenu, Then};
    let sim = Sim::start(Term::Vtium, sim::Machine::Map, 100, 30);
    let shared = sim.app.shared();
    let mut failures = BTreeSet::new();
    for tab in sim.app.tabs() {
        let cx = Context {
            shared,
            node: tab.node(),
        };
        let own = tab.own_font(shared).is_some();
        let lines: Vec<(&str, Line)> = [
            ("File", menus::file::lines(&cx)),
            ("File › Open", menus::submenu(Submenu::Open, &cx)),
            ("File › History", menus::submenu(Submenu::History, &cx)),
            (
                "Actions",
                menus::actions::lines(&cx, tab.menu_items(shared), own),
            ),
        ]
        .into_iter()
        .flat_map(|(menu, lines)| lines.into_iter().map(move |l| (menu, l)))
        .collect();
        for (menu, line) in lines {
            let Some(choice) = line.choice() else {
                continue;
            };
            let label = &choice.item.label;
            if choice.item.greyed.as_deref() == Some(NOT_BUILT) {
                failures.insert(format!("{menu} › {label} is greyed out as {NOT_BUILT}"));
            }
            match &choice.then {
                Then::Nothing if choice.item.greyed.is_none() => {
                    failures.insert(format!("{menu} › {label} has no row of the key table"));
                }
                Then::Nothing => {}
                Then::Act(action) => {
                    if let Route::Unbuilt(what) = route(action.row) {
                        failures.insert(format!(
                            "{menu} › {label}'s row {} is routed to nothing yet ({what})",
                            action.row
                        ));
                    }
                }
                Then::Sub(_) => {}
            }
        }
    }
    let failures: Vec<String> = failures.into_iter().collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The 1-switch run over questions, typing and leaving: from the state
/// after each transition into a question or into typing, every key; and
/// after every transition, the keys that leave, `q` and Ctrl-C. It runs at
/// the end of a card that changes the TUI, in a release build:
/// `cargo test --release -p vtium-config --test walk -- --ignored`.
#[test]
#[ignore = "the 1-switch run needs a release build; it runs at the end of a card"]
fn every_pair_of_keys_through_questions_typing_and_leaving_keeps_the_rules() {
    let mut failures = Vec::new();
    for term in Term::ALL {
        let map = explore::explore(term);
        let firsts: Vec<&Step> = map
            .steps
            .iter()
            .filter(|s| s.to.kind != StateKind::Left)
            .collect();
        let found = par::map(&firsts, |first| {
            let into = matches!(first.to.kind, StateKind::Question | StateKind::Typing);
            let path = first.keys();
            let before = checks::Before::of(&Sim::reach(first.start, &path));
            explore::ALPHABET
                .iter()
                .filter(|(name, _)| into || matches!(*name, "q" | "Ctrl-C"))
                .flat_map(|(name, press)| {
                    let mut sim = Sim::reach(first.start, &path);
                    let did = sim.press(*press);
                    let buf = sim.draw();
                    let step = Step {
                        from: first.to.clone(),
                        key: name,
                        to: state::of_drawn(&sim, &buf),
                        did,
                        path: path.clone(),
                        start: first.start,
                    };
                    checks::see(&step, &before, &sim, &sim::rows(&buf))
                })
                .collect::<Vec<_>>()
        });
        failures.extend(found.into_iter().flatten());
    }
    let failures = checks::expected(failures);
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
