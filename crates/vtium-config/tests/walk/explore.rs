//! The explorer: from each start state it tries every key of a fixed
//! alphabet in every abstract state it reaches, and records each
//! transition. This is GUI ripping over [`super::sim::Sim`]: one key
//! sequence from the start stands for each abstract state, the first one
//! the breadth-first walk reaches, and each key is tried on a TUI that
//! those keys bring there again. Each transition keeps the shortest key
//! sequence that reaches it from its start, which is the 0-switch scenario
//! for it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use vtium_config_widgets::keys::{Code, Key, Mods};

use super::checks::{self, Before};
use super::par;
use super::sim::{self, Did, Machine, Press, Sim, Spot, Term};
use super::state::{self, Abs, StateKind};

/// What the explorer does in each state, each with the name the map
/// writes: every key the key table names, `z` for a letter no table
/// names, the mouse on the places the frame draws, and the terminal
/// shrinking below the minimum size and growing back to it.
pub const ALPHABET: &[(&str, Press)] = &[
    ("Up", Press::Key(Key::plain(Code::Up))),
    ("Down", Press::Key(Key::plain(Code::Down))),
    ("Left", Press::Key(Key::plain(Code::Left))),
    ("Right", Press::Key(Key::plain(Code::Right))),
    ("PgUp", Press::Key(Key::plain(Code::PageUp))),
    ("PgDn", Press::Key(Key::plain(Code::PageDown))),
    ("Home", Press::Key(Key::plain(Code::Home))),
    ("End", Press::Key(Key::plain(Code::End))),
    ("Enter", Press::Key(Key::plain(Code::Enter))),
    ("Tab", Press::Key(Key::plain(Code::Tab))),
    (
        "Shift-Tab",
        Press::Key(Key {
            mods: Mods::SHIFT,
            code: Code::Tab,
        }),
    ),
    ("Backspace", Press::Key(Key::plain(Code::Backspace))),
    ("Delete", Press::Key(Key::plain(Code::Delete))),
    ("Esc", Press::Key(Key::ESCAPE)),
    ("F1", Press::Key(Key::plain(Code::F(1)))),
    ("F10", Press::Key(Key::plain(Code::F(10)))),
    ("Ctrl-C", Press::Key(Key::ctrl('c'))),
    ("Ctrl-D", Press::Key(Key::ctrl('d'))),
    ("Ctrl-U", Press::Key(Key::ctrl('u'))),
    ("Ctrl-F", Press::Key(Key::ctrl('f'))),
    ("Ctrl-S", Press::Key(Key::ctrl('s'))),
    ("Ctrl-Z", Press::Key(Key::ctrl('z'))),
    ("Ctrl-Y", Press::Key(Key::ctrl('y'))),
    ("Alt-F", Press::Key(Key::alt('f'))),
    ("Alt-A", Press::Key(Key::alt('a'))),
    ("Alt-P", Press::Key(Key::alt('p'))),
    ("Space", Press::Key(Key::char(' '))),
    ("?", Press::Key(Key::char('?'))),
    ("a", Press::Key(Key::char('a'))),
    ("A", Press::Key(Key::char('A'))),
    ("c", Press::Key(Key::char('c'))),
    ("d", Press::Key(Key::char('d'))),
    ("e", Press::Key(Key::char('e'))),
    ("g", Press::Key(Key::char('g'))),
    ("G", Press::Key(Key::char('G'))),
    ("h", Press::Key(Key::char('h'))),
    ("i", Press::Key(Key::char('i'))),
    ("j", Press::Key(Key::char('j'))),
    ("k", Press::Key(Key::char('k'))),
    ("m", Press::Key(Key::char('m'))),
    ("n", Press::Key(Key::char('n'))),
    ("o", Press::Key(Key::char('o'))),
    ("p", Press::Key(Key::char('p'))),
    ("q", Press::Key(Key::char('q'))),
    ("r", Press::Key(Key::char('r'))),
    ("s", Press::Key(Key::char('s'))),
    ("t", Press::Key(Key::char('t'))),
    ("u", Press::Key(Key::char('u'))),
    ("v", Press::Key(Key::char('v'))),
    ("w", Press::Key(Key::char('w'))),
    ("y", Press::Key(Key::char('y'))),
    ("z", Press::Key(Key::char('z'))),
    ("Click tab", Press::Click(Spot::Tab(2))),
    ("Click File", Press::Click(Spot::File)),
    ("Click facts", Press::Click(Spot::Facts)),
    ("Click button", Press::Click(Spot::Button)),
    ("Click row", Press::Click(Spot::Row)),
    ("Click outside", Press::Click(Spot::Outside)),
    ("Wheel up", Press::Wheel(true)),
    ("Wheel down", Press::Wheel(false)),
    ("Shrink", Press::Resize(79, 24)),
    ("Grow", Press::Resize(80, 25)),
    // the key that opens More, the Alt letters of the tabs and the
    // digits that go straight to a tab, last, so that the walk reaches
    // every other state by the same keys as without them
    (".", Press::Key(Key::char('.'))),
    ("Alt-N", Press::Key(Key::alt('n'))),
    ("Alt-E", Press::Key(Key::alt('e'))),
    ("Alt-L", Press::Key(Key::alt('l'))),
    ("Alt-O", Press::Key(Key::alt('o'))),
    ("Alt-S", Press::Key(Key::alt('s'))),
    ("Alt-H", Press::Key(Key::alt('h'))),
    ("1", Press::Key(Key::char('1'))),
    ("2", Press::Key(Key::char('2'))),
    ("3", Press::Key(Key::char('3'))),
    ("4", Press::Key(Key::char('4'))),
    ("5", Press::Key(Key::char('5'))),
    ("6", Press::Key(Key::char('6'))),
];

/// The press the alphabet names `name`, or the character a name of one
/// character types, as a seed's keys type a name; `z`, a letter that no
/// table names, for any other name.
pub fn press_of(name: &str) -> Press {
    if let Some((_, p)) = ALPHABET.iter().find(|(n, _)| *n == name) {
        return *p;
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Press::Key(Key::char(c)),
        _ => Press::Key(Key::char('z')),
    }
}

/// The alphabet's own copy of `name`, which a step keeps.
pub fn name_of(name: &str) -> Option<&'static str> {
    ALPHABET.iter().map(|(n, _)| *n).find(|n| *n == name)
}

/// Key sequences from the start at 80×25 that the walk also explores
/// from, because the abstract state does not tell their highlighted item
/// apart from another of its group: each reaches an item whose Enter or
/// Space opens a node that the group's first item does not.
pub const SEEDS: [&[&str]; 8] = [
    // a look card's Font set line, whose Enter opens its list
    &["3", "Enter", "Down"],
    // a color scheme, whose Enter opens its detail
    &["3", "Down", "Down", "Down", "Down", "Down", "Down"],
    // the new font set form's Name line, whose Enter types it
    &["3", "Down", "n", "End", "Up"],
    // the detail of a font in the person's own folders
    &["End", "Up", "Enter"],
    // the values of allow-window-ops, whose yes asks first
    &["Ctrl-F", "w", "i", "n", "d", "o", "w", "Enter", "Enter"],
    // NEW PATH's terminal box, whose Enter opens its card
    &["Alt-O", "e"],
    // a display's part in NEW PATH, whose Enter opens its choices
    &["Alt-O", "e", "Left"],
    // the box of the other inputs, whose Enter lists them
    &["Alt-O", "End", "Up"],
];

/// The grids the explorer starts at: the minimum, and one column and one
/// row below it.
pub const SIZES: [(u16, u16); 2] = [(80, 25), (79, 24)];

/// Where a walk starts: the kind of terminal, the machine and the grid.
pub type Start = (Term, Machine, u16, u16);

/// The keys that reach a state, and where they start.
pub type Reached = (Vec<&'static str>, Start);

/// One transition the explorer took.
#[derive(Clone, Debug)]
pub struct Step {
    pub from: Abs,
    pub key: &'static str,
    pub to: Abs,
    pub did: Did,
    /// The keys from the start that reach `from`.
    pub path: Vec<&'static str>,
    pub start: Start,
}

impl Step {
    /// Whether the key changed nothing the map records: the same abstract
    /// state, and no effect.
    pub fn idle(&self) -> bool {
        self.from == self.to && self.did.effects.is_empty()
    }

    /// The map's line for this transition.
    pub fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.from,
            self.key,
            self.to,
            self.did.effects.join(", ")
        )
    }

    /// The keys from the start that take this transition.
    pub fn keys(&self) -> Vec<&'static str> {
        let mut keys = self.path.clone();
        keys.push(self.key);
        keys
    }

    /// The scenario that takes this transition, as the keys a person
    /// presses.
    pub fn scenario(&self) -> String {
        scenario(self.start, &self.keys())
    }
}

/// A scenario as a failure names it: the terminal, the grid and the keys
/// from the start, as `vtium 80×25: 3 Down Enter`.
pub fn scenario(start: Start, keys: &[&str]) -> String {
    let (term, machine, cols, rows) = start;
    let on = if machine == Machine::Map {
        String::new()
    } else {
        format!(" on the {} machine", machine.name())
    };
    format!("{} {cols}×{rows}{on}: {}", term.name(), keys.join(" "))
}

/// What the explorer found: every transition, and the abstract states in
/// the order the walk reached them.
pub struct Map {
    pub steps: Vec<Step>,
    pub states: Vec<Abs>,
    /// The keys that reach each abstract state, and where they start.
    pub reps: BTreeMap<Abs, Reached>,
    /// What the checks after every key found, in the order of the walk.
    pub failures: Vec<String>,
}

/// The most states one walk takes before it fails, so an abstraction that
/// tells too much apart fails at once instead of running for hours.
const STATES_MAX: usize = 20_000;

/// Explores from the start states in `term`, on the screen map's machine
/// at each size, and runs the checks after every key on each transition.
///
/// The walk is breadth-first. It takes a batch of states from the front
/// of the queue, presses every key in each of them on the machine's cores,
/// and then records the batch's transitions in queue order. A state's
/// transitions depend only on the keys that reach it, never on what the
/// walk has seen, so the map is the one a walk on one thread makes.
pub fn explore(term: Term) -> Map {
    let mut steps = Vec::new();
    let mut seen: BTreeSet<Abs> = BTreeSet::new();
    let mut states = Vec::new();
    let mut reps = BTreeMap::new();
    let mut failures = Vec::new();
    let mut queue: VecDeque<(Abs, Reached)> = VecDeque::new();
    for (cols, rows) in SIZES {
        let start = (term, Machine::Map, cols, rows);
        let abs = state::of(&Sim::reach(start, &[]));
        if seen.insert(abs.clone()) {
            states.push(abs.clone());
            queue.push_back((abs, (Vec::new(), start)));
        }
    }
    // a seed is explored even where its abstract state was seen, since its
    // highlight acts otherwise
    for seed in SEEDS {
        let start = (term, Machine::Map, 80, 25);
        let path: Vec<&'static str> = seed.to_vec();
        let abs = state::of(&Sim::reach(start, &path));
        if seen.insert(abs.clone()) {
            states.push(abs.clone());
        }
        queue.push_back((abs, (path, start)));
    }
    while !queue.is_empty() {
        assert!(
            states.len() < STATES_MAX,
            "{} states in {}: the abstract state tells too much apart",
            states.len(),
            term.name()
        );
        let take = queue.len().min(par::threads() * 8);
        let batch: Vec<(Abs, Reached)> = queue.drain(..take).collect();
        let pressed = par::map(&batch, |(from, (path, start))| {
            press_all(from, *start, path)
        });
        for ((from, (path, start)), presses) in batch.into_iter().zip(pressed) {
            reps.entry(from.clone())
                .or_insert_with(|| (path.clone(), start));
            for ((name, _), (did, to, found)) in ALPHABET.iter().zip(presses) {
                failures.extend(found);
                if seen.insert(to.clone()) {
                    states.push(to.clone());
                    if to.kind != StateKind::Left {
                        let mut next = path.clone();
                        next.push(name);
                        queue.push_back((to.clone(), (next, start)));
                    }
                }
                steps.push(Step {
                    from: from.clone(),
                    key: name,
                    to,
                    did,
                    path: path.clone(),
                    start,
                });
            }
        }
    }
    Map {
        steps,
        states,
        reps,
        failures,
    }
}

/// Presses each key of the alphabet once on a TUI that `path` brings from
/// `start` to `from`, and returns what each did, the state it left, and
/// what the checks after the key found.
fn press_all(from: &Abs, start: Start, path: &[&'static str]) -> Vec<(Did, Abs, Vec<String>)> {
    let before = Before::of(&Sim::reach(start, path));
    ALPHABET
        .iter()
        .map(|(name, press)| {
            let mut sim = Sim::reach(start, path);
            let did = sim.press(*press);
            let buf = sim.draw();
            let to = state::of_drawn(&sim, &buf);
            let step = Step {
                from: from.clone(),
                key: name,
                to: to.clone(),
                did: did.clone(),
                path: path.to_vec(),
                start,
            };
            let found = checks::see(&step, &before, &sim, &sim::rows(&buf));
            (did, to, found)
        })
        .collect()
}
