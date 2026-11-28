//! The TUI's contract, compiled in: every node of the interface tree with
//! the rows of the key table that act there. The shell's navigation
//! module, the action bar, F1's list of keys and every tab read the same
//! tables through this module, so what a screen offers and what its help
//! says cannot drift apart.
//!
//! The tables are two TSV files beside ADR-1403, `ADR-1403-tree.tsv` and
//! `ADR-1403-keys.tsv`. The rows and nodes that the rebuilt TUI adds wait
//! in an overlay under `crates/vtium-config-widgets/contract/` until the
//! binary switches to the rebuilt TUI, because the old TUI still runs
//! against the two tables as they stand. The overlay's `d11-keys.tsv` and
//! `d11-tree.tsv` add rows and nodes or replace them by id, `d11-gone.tsv`
//! takes rows and nodes out, and `d11-search.tsv` gives the tree its
//! `search` column. The test `the_compiled_tables_match_the_tsvs` in
//! `tests/contract.rs` reads all six files and writes `text.rs`, and fails
//! when `text.rs` differs from what it writes.
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-widgets --test contract`
//! writes `text.rs` again.
//!
//! A node's rows come in the order its `keys` column names them, which is
//! their order of priority on the action bar: where the bar cannot hold
//! every button, it leaves out the last first.
//!
//! References:
//! - ADR-1206 D3: the tables are compiled into this crate beside the `Tab`
//!   trait, and the action bar's buttons are the node's labelled rows, in
//!   an order of priority, with each key in the bar's form.
//! - ADR-1403 D7, "The contract": the tree and the key table are the
//!   TUI's contract.
//! - ADR-1403 D11, "How the tables carry this section": the tables stay as
//!   they are until the work that builds the section lands.

mod text;

/// A row of the key table: one key, or several that act alike, in one
/// state of the TUI, and what it does there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The row's id, such as `ctrl-s.view`, which also names its action.
    pub id: &'static str,
    /// The key as the key table writes it, such as `Ctrl-S, Enter`.
    pub key: &'static str,
    /// The key as the action bar writes it inside its brackets, such as
    /// `Ctrl+S` ([`bar_form`]).
    pub bar_key: &'static str,
    /// The state the row acts in, such as `view` or `layer:menu`.
    pub state: &'static str,
    /// What the key does, in whole sentences: the help a person reads.
    pub means: &'static str,
    /// The button's or the key line's name for the action, or `-` where
    /// the row has none.
    pub label: &'static str,
    /// The code does what the row says now.
    pub supported: bool,
}

impl Row {
    /// The id of the action the row's key gives the open tab: the row's
    /// own id, since each row is one meaning of one key in one state.
    pub const fn action(&self) -> &'static str {
        self.id
    }

    /// Whether the row names itself on a key line or the action bar: it
    /// is supported and has a label.
    pub fn labelled(&self) -> bool {
        self.supported && !self.label.is_empty() && self.label != "-"
    }

    /// Whether the row is a button of the action bar: a labelled row
    /// whose key does not move the highlight, go to a tab or type, since
    /// those stay off the bar, and is not a click.
    pub fn on_bar(&self) -> bool {
        self.labelled() && !off_bar(first_key(self.key))
    }
}

/// A node of the interface tree: a screen, or a level open over one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// The node's id, dotted by its parent, such as `flow.card`.
    pub id: &'static str,
    /// The parent node's id, or `-` for the root.
    pub parent: &'static str,
    /// What the node is, in plain words.
    pub what: &'static str,
    /// The code has the node now.
    pub supported: bool,
    /// The fields that the `?` search matches on this node's items, such
    /// as a font's family and file. Empty where the node offers no search.
    pub search: &'static [&'static str],
    /// Each of the node's rows, as its index in [`rows`], in the order of
    /// the tree's `keys` column.
    rows: &'static [u16],
}

impl Node {
    /// Every row that acts in the node, in order of priority.
    pub fn rows(&self) -> impl Iterator<Item = &'static Row> + '_ {
        self.rows
            .iter()
            .filter_map(|&i| text::ROWS.get(usize::from(i)))
    }

    /// The node's labelled rows, in order of priority: what a key line and
    /// F1's list of keys show.
    pub fn labelled(&self) -> impl Iterator<Item = &'static Row> + '_ {
        self.rows().filter(|r| r.labelled())
    }

    /// The node's buttons on the action bar, in order of priority.
    pub fn buttons(&self) -> impl Iterator<Item = &'static Row> + '_ {
        self.rows().filter(|r| r.on_bar())
    }
}

/// Every node of the interface tree, in the tree's order.
pub fn nodes() -> &'static [Node] {
    text::NODES
}

/// Every row of the key table, in the table's order.
pub fn rows() -> &'static [Row] {
    text::ROWS
}

/// The node with the id `id`.
pub fn node(id: &str) -> Option<&'static Node> {
    text::NODES.iter().find(|n| n.id == id)
}

/// The row with the id `id`.
pub fn row(id: &str) -> Option<&'static Row> {
    text::ROWS.iter().find(|r| r.id == id)
}

/// The labelled rows of the node `id`, in order of priority, or none
/// where the tree has no such node.
pub fn labelled(id: &str) -> Vec<&'static Row> {
    node(id).map_or_else(Vec::new, |n| n.labelled().collect())
}

/// The action bar's buttons on the node `id`, in order of priority.
pub fn buttons(id: &str) -> Vec<&'static Row> {
    node(id).map_or_else(Vec::new, |n| n.buttons().collect())
}

/// The fields the `?` search matches on the node `id`: empty where the
/// node offers no search.
pub fn search(id: &str) -> &'static [&'static str] {
    node(id).map_or(&[], |n| n.search)
}

/// The first of the keys a row of the key table names, as in `Ctrl-S`
/// for `Ctrl-S, Enter`.
fn first_key(key: &str) -> &str {
    key.split(", ").next().unwrap_or(key)
}

/// Whether a key stays off the action bar: a key that moves the
/// highlight or scrolls, Tab and the digits that go to a tab, an Alt
/// letter of the top row, a letter typed into an entry, and a click or
/// the wheel, which act on what they hit.
fn off_bar(key: &str) -> bool {
    matches!(
        key,
        "Up" | "Down"
            | "Left"
            | "Right"
            | "Home"
            | "End"
            | "PgUp"
            | "PgDn"
            | "j"
            | "k"
            | "h"
            | "l"
            | "g"
            | "G"
            | "Ctrl-D"
            | "Ctrl-U"
            | "Tab"
            | "Shift-Tab"
            | "any letter"
    ) || key.len() == 1 && key.bytes().all(|b| b.is_ascii_digit())
        || key.starts_with("Alt-")
        || key.starts_with("Click")
        || key.starts_with("Wheel")
}

/// A key as the action bar and the menus write it, from the key table's
/// form: the first key where the table lists several, a chord joined with
/// a plus and a capital after the modifier, and `Delete` as `Del`. So
/// `Ctrl-S, Enter` is `Ctrl+S`, `Alt-f` is `Alt+F`, and `Delete` is `Del`.
/// The bar puts the result in brackets and the menus' column of shortcuts
/// writes it bare.
///
/// References:
/// - ADR-1206 D3, the action bar: the key column's forms and the bar's.
/// - ADR-1403 D11, "The action bar": a chord is one bracket, with a
///   capital after the modifier, as CUA writes it.
pub fn bar_form(key: &str) -> String {
    let key = first_key(key);
    if key == "Delete" {
        return "Del".to_owned();
    }
    let mut out = String::new();
    let mut rest = key;
    while let Some((modifier, after)) = rest
        .split_once('-')
        .filter(|(m, a)| matches!(*m, "Ctrl" | "Alt" | "Shift") && !a.is_empty())
    {
        out.push_str(modifier);
        out.push('+');
        rest = after;
    }
    if out.is_empty() {
        return key.to_owned();
    }
    let mut chars = rest.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_writes_a_chord_with_a_plus_and_a_capital() {
        assert_eq!(bar_form("Ctrl-S"), "Ctrl+S");
        assert_eq!(bar_form("Ctrl-s"), "Ctrl+S");
        assert_eq!(bar_form("Alt-F"), "Alt+F");
        assert_eq!(bar_form("Shift-Tab"), "Shift+Tab");
        assert_eq!(bar_form("Ctrl-Shift-z"), "Ctrl+Shift+Z");
    }

    #[test]
    fn the_bar_shortens_delete_and_names_the_first_of_several_keys() {
        assert_eq!(bar_form("Delete"), "Del");
        assert_eq!(bar_form("Ctrl-S, Enter"), "Ctrl+S");
        assert_eq!(bar_form("Alt-N, Alt-E, Alt-L"), "Alt+N");
        assert_eq!(bar_form("v, Click on View changes"), "v");
    }

    #[test]
    fn a_key_with_no_modifier_is_written_as_the_table_writes_it() {
        for key in [
            "q", "A", "?", ".", "Enter", "Space", "F1", "F10", "Esc", "-",
        ] {
            assert_eq!(bar_form(key), key);
        }
    }
}
