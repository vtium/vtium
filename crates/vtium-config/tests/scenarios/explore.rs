//! The explorer: from each start state it tries every key of a fixed
//! alphabet in every abstract state it reaches, over the pure state
//! machine, and records each transition. This is GUI ripping over
//! [`super::sim::Sim`]: one concrete TUI stands for each abstract state,
//! the first one the breadth-first walk reaches, and each key is tried on
//! a clone of it. Each transition keeps the shortest key sequence that
//! reaches it from its start, which is the 0-switch scenario for it.

use std::collections::{BTreeMap, VecDeque};

use vtium_config::tui::input::Key;

use super::sim::{Did, Sim, Term};
use super::state::{self, Abs};

/// The keys the explorer tries, each with the name the map writes. It
/// holds every key the TUI's code names, a letter it names nowhere (`z`)
/// for "any other key", and Ctrl-C.
pub const ALPHABET: &[(&str, Key)] = &[
    ("Up", Key::Up),
    ("Down", Key::Down),
    ("Left", Key::Left),
    ("Right", Key::Right),
    ("PgDn", Key::PageDown),
    ("Home", Key::Home),
    ("End", Key::End),
    ("Enter", Key::Enter),
    ("Tab", Key::Tab),
    ("Shift-Tab", Key::BackTab),
    ("Backspace", Key::Backspace),
    ("Esc", Key::Escape),
    ("Ctrl-C", Key::Interrupt),
    ("Ctrl-D", Key::Ctrl('d')),
    ("Space", Key::Char(' ')),
    ("a", Key::Char('a')),
    ("d", Key::Char('d')),
    ("e", Key::Char('e')),
    ("i", Key::Char('i')),
    ("k", Key::Char('k')),
    ("l", Key::Char('l')),
    ("n", Key::Char('n')),
    ("o", Key::Char('o')),
    ("p", Key::Char('p')),
    ("q", Key::Char('q')),
    ("r", Key::Char('r')),
    ("s", Key::Char('s')),
    ("t", Key::Char('t')),
    ("u", Key::Char('u')),
    ("v", Key::Char('v')),
    ("x", Key::Char('x')),
    ("y", Key::Char('y')),
    ("z", Key::Char('z')),
];

/// The grids the explorer starts at: the minimum, and one column and one
/// row below it.
pub const SIZES: [(usize, usize); 2] = [(80, 25), (79, 24)];

/// Where a walk starts: the kind of terminal and the grid.
pub type Start = (Term, usize, usize);

/// A TUI the walk reached, with the keys that reach it and where they
/// start.
pub type Reached = (Sim, Vec<&'static str>, Start);

/// One transition the explorer took.
#[derive(Clone, Debug)]
pub struct Step {
    pub from: Abs,
    pub key: &'static str,
    pub to: Abs,
    pub did: Did,
    /// The keys from the start that reach `before`, the size and terminal
    /// first.
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

    /// The scenario that takes this transition, as the keys a person
    /// presses.
    pub fn scenario(&self) -> String {
        let (term, cols, rows) = self.start;
        let mut keys = self.path.clone();
        keys.push(self.key);
        format!("{} {cols}×{rows}: {}", term.name(), keys.join(" "))
    }
}

/// What the explorer found: every transition, and the abstract states in
/// the order the walk reached them.
pub struct Map {
    pub steps: Vec<Step>,
    pub states: Vec<Abs>,
    /// The TUI that stands for each abstract state, the keys that reach
    /// it and where they start.
    pub reps: BTreeMap<Abs, Reached>,
}

/// Explores from the start states in `term`: the fixture at each size.
/// `visit` sees each transition as it is taken, with the TUI before and
/// after the key.
pub fn explore(term: Term, mut visit: impl FnMut(&Step, &Sim, &Sim)) -> Map {
    let mut steps = Vec::new();
    let mut seen: BTreeMap<Abs, ()> = BTreeMap::new();
    let mut states = Vec::new();
    let mut reps = BTreeMap::new();
    let mut queue: VecDeque<Reached> = VecDeque::new();
    for (cols, rows) in SIZES {
        let sim = Sim::start(term, cols, rows);
        let abs = state::of(&sim);
        if seen.insert(abs.clone(), ()).is_none() {
            states.push(abs);
            queue.push_back((sim, Vec::new(), (term, cols, rows)));
        }
    }
    while let Some((sim, path, start)) = queue.pop_front() {
        let from = state::of(&sim);
        reps.insert(from.clone(), (sim.clone(), path.clone(), start));
        for (name, key) in ALPHABET {
            let mut after = sim.clone();
            let did = after.key(*key);
            let to = state::of(&after);
            if seen.insert(to.clone(), ()).is_none() {
                states.push(to.clone());
                if after.exited.is_none() {
                    let mut next = path.clone();
                    next.push(name);
                    queue.push_back((after.clone(), next, start));
                }
            }
            let step = Step {
                from: from.clone(),
                key: name,
                to,
                did,
                path: path.clone(),
                start,
            };
            visit(&step, &sim, &after);
            steps.push(step);
        }
    }
    Map {
        steps,
        states,
        reps,
    }
}
