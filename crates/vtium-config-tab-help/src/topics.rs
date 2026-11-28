//! The Help tab's topics and what each one says. The topics are the six
//! tabs in the top row's order, then Menus and Save, Mouse, Commands and
//! About. A tab's topic lists every key of the tab's screen, from the
//! compiled key table, so the page says only what the contract says and
//! cannot drift from what the keys do. The keys that work on every tab
//! close each tab's topic as a short grid, and the topic Menus and Save
//! gives each of them in full.
//!
//! A topic is a list of entries that do not depend on the screen's width:
//! a heading, a sentence, a key with what it does, a grid of keys, a
//! command with what it does, or a row of the logo. [`crate::page`] lays
//! them out for a width.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Help bullet: the topics, and
//!   a tab's topic as its keys and what each does.
//! - ADR-1403 D11, "F1, help for what is highlighted": F1's list of every
//!   key of a screen is the Help tab's topic for that tab.
//! - ADR-1403 D7, "Help that matches the code": the key table's `means`
//!   column is the help a person reads.

use vtium_config_widgets::contract::{self, Row};

/// What the Mouse topic says of selecting text. vtium hands a click or a
/// drag with Shift held to its own selection rather than to a program
/// that reads the mouse, as xterm does, unless the program asks for Shift
/// with XTSHIFTESCAPE, which vtium-config never sends.
pub const SELECT: &str =
    "To select text to copy while vtium-config has the mouse, hold [Shift] and drag.";

/// What the Commands topic says first.
pub const COMMANDS: &str =
    "Each command works from a shell, in a terminal of any size, as vtium-config COMMAND.";

/// What the Commands topic says last.
pub const ONE_COMMAND: &str = "Help on one command: vtium-config --help COMMAND.";

/// The heading over the grid of the keys that work on every tab.
pub const EVERYWHERE: &str = "Everywhere";

/// The rows of the key table the Mouse topic gives: what a click does on
/// a tab, in a menu and in a list box.
const CLICKS: [&str; 8] = [
    "click-menu.view",
    "click-tab.view",
    "click-row.view",
    "click-button.any",
    "click-facts.view",
    "click-item.layer.menu",
    "click-outside.layer.menu",
    "click-choice.layer.list",
];

/// The menus' nodes, whose description the topic Menus and Save gives.
const MENUS: [&str; 2] = ["any.menu.file", "any.menu.actions"];

/// The tabs' nodes, in the top row's order. A key that at least five of
/// them carry works everywhere: the digit that goes to a tab is on every
/// tab but that one, F1 is on every tab but Help, where it does nothing,
/// and `?` on every tab but Flow, which offers no search.
const TAB_NODES: [&str; 6] = ["fonts", "effects", "looks", "flow", "settings", "help"];

/// The tabs that carry a key that works everywhere, at the fewest.
const EVERYWHERE_AT_LEAST: usize = 5;

/// What a topic shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A tab's topic: the keys of the tab's nodes, the first being the
    /// tab's own. The Looks tab has two, as its list is `looks-resources`
    /// while a color scheme or a pointer theme is highlighted.
    Tab(&'static [&'static str]),
    /// The menus and the keys that work everywhere.
    Menus,
    /// What a click does, and how to select text.
    Mouse,
    /// The command line's commands.
    Commands,
    /// vtium-config's name, version and logo.
    About,
}

/// One topic of the Help tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Topic {
    /// The topic's name at the left, such as `Fonts`.
    pub name: &'static str,
    pub kind: Kind,
}

/// Every topic, in the order the left of the tab lists them. The first six
/// are the tabs in the top row's order, so a tab's index is its topic's.
pub const TOPICS: [Topic; 10] = [
    Topic {
        name: "Fonts",
        kind: Kind::Tab(&["fonts"]),
    },
    Topic {
        name: "Effects",
        kind: Kind::Tab(&["effects"]),
    },
    Topic {
        name: "Looks",
        kind: Kind::Tab(&["looks", "looks-resources"]),
    },
    Topic {
        name: "Flow",
        kind: Kind::Tab(&["flow"]),
    },
    Topic {
        name: "Settings",
        kind: Kind::Tab(&["settings"]),
    },
    Topic {
        name: "Help",
        kind: Kind::Tab(&["help"]),
    },
    Topic {
        name: "Menus and Save",
        kind: Kind::Menus,
    },
    Topic {
        name: "Mouse",
        kind: Kind::Mouse,
    },
    Topic {
        name: "Commands",
        kind: Kind::Commands,
    },
    Topic {
        name: "About",
        kind: Kind::About,
    },
];

/// The index of the About topic, which Left and Right switch to.
pub const ABOUT: usize = 9;

/// The topic of the tab that the node `node` belongs to: the tab's own
/// node, or a level open over it, as `looks.look` belongs to Looks. `None`
/// for a node over no tab, such as a menu.
pub fn for_node(node: &str) -> Option<usize> {
    let mut at = contract::node(node)?;
    loop {
        if let Some(i) = TOPICS.iter().position(|t| match t.kind {
            Kind::Tab(nodes) => nodes.contains(&at.id),
            _ => false,
        }) {
            return Some(i);
        }
        at = contract::node(at.parent)?;
    }
}

/// A key or several that act alike, with the name the grid gives them,
/// such as `[q] [Ctrl+C] Quit`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// Each key, as the screen writes it.
    pub keys: Vec<String>,
    /// The rows' label, with a capital.
    pub label: String,
    /// The ids of the rows the cell gives.
    pub rows: Vec<&'static str>,
}

/// One entry of a topic: a run of lines that belong together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A heading inside the topic.
    Heading(&'static str),
    /// A sentence, wrapped to the width.
    Text(String),
    /// Rows of the key table that mean the same: their keys and what
    /// they do, such as Left and Right on the Help tab.
    Key {
        keys: Vec<String>,
        means: &'static str,
        rows: Vec<&'static str>,
    },
    /// The keys that work everywhere, each with its label.
    Grid(Vec<Cell>),
    /// A command line form and what it does.
    Command {
        synopsis: &'static str,
        does: &'static str,
    },
    /// A row of the logo, styled by the banner's own SGR, which is never
    /// wrapped.
    Art(String),
    /// An empty line.
    Blank,
}

/// The keys of `row` as a screen writes them: each key the table names,
/// in brackets in the action bar's form, as `[Ctrl+S]`. A click, the
/// wheel and "any letter" are no key to press, and stay bare.
pub fn keys_of(row: &Row) -> Vec<String> {
    row.key
        .split(", ")
        .map(|k| {
            if k.starts_with("Click") || k.starts_with("Wheel") || k.starts_with("any ") {
                k.to_owned()
            } else {
                format!("[{}]", contract::bar_form(k))
            }
        })
        .collect()
}

/// The labelled rows of the nodes `nodes`, each once, in the order of the
/// first node and then the rows only the later ones carry.
pub fn rows_of_nodes(nodes: &[&str]) -> Vec<&'static Row> {
    let mut out: Vec<&'static Row> = Vec::new();
    for node in nodes {
        for row in contract::labelled(node) {
            if !out.iter().any(|r| r.id == row.id) {
                out.push(row);
            }
        }
    }
    out
}

/// Whether `row` works everywhere: at least five of the six tabs carry it.
pub fn everywhere(row: &Row) -> bool {
    TAB_NODES
        .iter()
        .filter(|n| contract::labelled(n).iter().any(|r| r.id == row.id))
        .count()
        >= EVERYWHERE_AT_LEAST
}

/// The rows that work everywhere, in the order the tabs first carry them.
pub fn everywhere_rows() -> Vec<&'static Row> {
    rows_of_nodes(&TAB_NODES)
        .into_iter()
        .filter(|r| everywhere(r))
        .collect()
}

/// `label` with its first letter a capital, as the action bar writes it.
fn capital(label: &str) -> String {
    let mut chars = label.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().chain(chars).collect::<String>()
    })
}

/// The grid of `rows`: rows that share a label make one cell, in the place
/// of the first of them.
fn grid(rows: &[&'static Row]) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for row in rows {
        let label = capital(row.label);
        match cells.iter_mut().find(|c| c.label == label) {
            Some(cell) => {
                cell.keys.extend(keys_of(row));
                cell.rows.push(row.id);
            }
            None => cells.push(Cell {
                keys: keys_of(row),
                label,
                rows: vec![row.id],
            }),
        }
    }
    cells
}

/// The key entries of `rows`: rows that mean the same make one entry, in
/// the place of the first of them.
fn keys(rows: &[&'static Row]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for row in rows {
        let same = out.iter_mut().find_map(|e| match e {
            Entry::Key { keys, means, rows } if *means == row.means => Some((keys, rows)),
            _ => None,
        });
        match same {
            Some((keys, rows)) => {
                keys.extend(keys_of(row));
                rows.push(row.id);
            }
            None => out.push(Entry::Key {
                keys: keys_of(row),
                means: row.means,
                rows: vec![row.id],
            }),
        }
    }
    out
}

/// What a tab's topic says: what the tab is, its own keys with what each
/// does, then the keys that work everywhere as a grid.
fn tab(nodes: &[&str]) -> Vec<Entry> {
    let mut out = Vec::new();
    if let Some(node) = nodes.first().and_then(|n| contract::node(n)) {
        out.push(Entry::Text(node.what.to_owned()));
        out.push(Entry::Blank);
    }
    let rows = rows_of_nodes(nodes);
    let (all, own): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| everywhere(r));
    out.extend(keys(&own));
    if !all.is_empty() {
        out.push(Entry::Blank);
        out.push(Entry::Heading(EVERYWHERE));
        out.push(Entry::Grid(grid(&all)));
    }
    out
}

/// What Menus and Save says: what each menu holds, then every key that
/// works everywhere with what it does.
fn menus() -> Vec<Entry> {
    let mut out: Vec<Entry> = MENUS
        .iter()
        .filter_map(|n| contract::node(n))
        .map(|n| Entry::Text(n.what.to_owned()))
        .collect();
    out.push(Entry::Blank);
    out.extend(keys(&everywhere_rows()));
    out
}

/// What Mouse says: what each click does, then how to select text.
fn mouse() -> Vec<Entry> {
    let clicks: Vec<&'static Row> = CLICKS.iter().filter_map(|id| contract::row(id)).collect();
    let mut out = keys(&clicks);
    out.push(Entry::Blank);
    out.push(Entry::Text(SELECT.to_owned()));
    out
}

/// What Commands says: each form of each command the usage lists, with
/// what it does, as `vtium-config --help` prints them.
fn commands() -> Vec<Entry> {
    let mut out = vec![Entry::Text(COMMANDS.to_owned()), Entry::Blank];
    for topic in vtium_config_core::help::topics() {
        for form in topic.forms {
            out.push(Entry::Command {
                synopsis: form.synopsis,
                does: form.does,
            });
        }
    }
    out.push(Entry::Blank);
    out.push(Entry::Text(ONE_COMMAND.to_owned()));
    out
}

/// What About says: each line of `about`, the text `vtium-config --help
/// about` prints. A line that styles itself with SGR is a row of the logo,
/// which the banner draws only in a terminal that shows color.
fn about(about: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in about.lines() {
        if line.contains('\x1b') {
            out.push(Entry::Art(line.to_owned()));
        } else if line.trim().is_empty() {
            if !out.is_empty() && out.last() != Some(&Entry::Blank) {
                out.push(Entry::Blank);
            }
        } else {
            out.push(Entry::Text(line.to_owned()));
        }
    }
    out
}

/// Every topic's entries, in the order of [`TOPICS`]. `about` is the text
/// the About topic gives, as `vtium-config --help about` prints it.
pub fn entries(about_text: &str) -> Vec<Vec<Entry>> {
    TOPICS
        .iter()
        .map(|t| match t.kind {
            Kind::Tab(nodes) => tab(nodes),
            Kind::Menus => menus(),
            Kind::Mouse => mouse(),
            Kind::Commands => commands(),
            Kind::About => about(about_text),
        })
        .collect()
}

/// The ids of the key table's rows that `entries` gives, in the order it
/// gives them.
pub fn rows_shown(entries: &[Entry]) -> Vec<&'static str> {
    let mut out = Vec::new();
    for e in entries {
        match e {
            Entry::Key { rows, .. } => out.extend(rows.iter().copied()),
            Entry::Grid(cells) => out.extend(cells.iter().flat_map(|c| c.rows.iter().copied())),
            _ => {}
        }
    }
    out
}

/// One line the `?` search matches: an entry of a topic, or one cell of a
/// topic's grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The topic's index in [`TOPICS`].
    pub topic: usize,
    /// The entry's index in the topic.
    pub entry: usize,
    /// The cell's index in a grid, `None` for any other entry.
    pub cell: Option<usize>,
    /// The `topic` field the search matches: the topic's name, then the
    /// line's keys and words.
    pub text: String,
}

/// Every line of every topic the `?` search matches, in the topics'
/// order: each key, sentence, cell of a grid and command. Headings, blank
/// lines and the logo are not matched.
pub fn items(entries: &[Vec<Entry>]) -> Vec<Item> {
    let mut out = Vec::new();
    for (topic, list) in entries.iter().enumerate() {
        let name = TOPICS.get(topic).map_or("", |t| t.name);
        for (entry, e) in list.iter().enumerate() {
            let mut push = |cell: Option<usize>, line: String| {
                out.push(Item {
                    topic,
                    entry,
                    cell,
                    text: format!("{name} {line}"),
                });
            };
            match e {
                Entry::Text(t) => push(None, t.clone()),
                Entry::Key { keys, means, .. } => push(None, format!("{} {means}", keys.join(" "))),
                Entry::Command { synopsis, does } => push(None, format!("{synopsis} {does}")),
                Entry::Grid(cells) => {
                    for (i, c) in cells.iter().enumerate() {
                        push(Some(i), format!("{} {}", c.keys.join(" "), c.label));
                    }
                }
                Entry::Heading(_) | Entry::Art(_) | Entry::Blank => {}
            }
        }
    }
    out
}
