//! The tables the records keep beside them, read by the walk's own code:
//! ADR-1403's interface tree and key table, with the overlay of rows and
//! nodes that ADR-1403 D11 adds, rewrites and takes out, which
//! vtium-config-widgets keeps under `contract/` until the tables carry it.
//! The walk reads the files, not the compiled contract the TUI reads, so
//! a fault in the compiled tables fails the walk.
//!
//! References:
//! - ADR-1403 D11, "How the tables carry this section": the overlay's four
//!   files, which rebuild-switch moves into the two tables.
//! - ADR-1206 D5: the walks read the tables by their own code.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A TSV's rows, each as its columns by name; none where the file is
/// absent, as the overlay is once the tables carry it.
fn read(path: &str) -> Vec<BTreeMap<String, String>> {
    let Ok(text) = std::fs::read_to_string(root().join(path)) else {
        return Vec::new();
    };
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            head.iter()
                .zip(l.split('\t').chain(std::iter::repeat("")))
                .map(|(h, v)| ((*h).to_owned(), v.to_owned()))
                .collect()
        })
        .collect()
}

/// The base table at `base`, with the overlay's rows of the same id in
/// their place, its other rows after them, and the ids in `gone` taken
/// out.
fn merged(base: &str, over: &str, gone: &BTreeSet<String>) -> Vec<BTreeMap<String, String>> {
    let id = |r: &BTreeMap<String, String>| r.get("id").cloned().unwrap_or_default();
    let mut over: Vec<BTreeMap<String, String>> = read(over);
    let mut out: Vec<BTreeMap<String, String>> = read(base)
        .into_iter()
        .map(|r| match over.iter().position(|o| id(o) == id(&r)) {
            Some(i) => over.remove(i),
            None => r,
        })
        .collect();
    out.extend(over);
    out.retain(|r| !gone.contains(&id(r)));
    out
}

/// A row of the key table.
#[derive(Clone, Debug)]
pub struct KeyRow {
    pub id: String,
    /// The key column as the table writes it.
    pub key: String,
    /// The alphabet's names of the keys the row is about; empty for
    /// `any other key`, `any letter` and the action bar.
    pub keys: Vec<&'static str>,
    pub other: bool,
    /// The row is about any letter typed, as `any letter` says.
    pub letters: bool,
    /// The kind of state the row applies in, as `view:flow` or `trial`.
    pub state: String,
    pub means: String,
    pub supported: bool,
    /// The short name the action bar shows for the key, or `-` where it
    /// shows none.
    pub label: String,
}

impl KeyRow {
    /// Whether the action bar shows the row as a button: a supported row
    /// with a label, whose first key does not move the highlight, scroll,
    /// go to a tab, type a letter, or click, since those act on what they
    /// hit. The live trial's countdown moves nothing, so its keys are
    /// buttons there.
    ///
    /// References:
    /// - ADR-1403 D11, "The action bar": the keys that move the highlight
    ///   stay off the bar.
    pub fn on_bar(&self) -> bool {
        const MOVES: [&str; 18] = [
            "Up",
            "Down",
            "Left",
            "Right",
            "Home",
            "End",
            "PgUp",
            "PgDn",
            "j",
            "k",
            "h",
            "l",
            "g",
            "G",
            "Ctrl-D",
            "Ctrl-U",
            "Tab",
            "Shift-Tab",
        ];
        let first = self.key.split(", ").next().unwrap_or_default();
        let off = MOVES.contains(&first)
            || self.letters
            || first.chars().all(|c| c.is_ascii_digit())
            || first.starts_with("Alt-")
            || first.starts_with("Click")
            || first.starts_with("Wheel");
        let trial = self.state == "trial";
        self.supported && !self.label.is_empty() && self.label != "-" && (trial || !off)
    }

    /// Whether the row's meaning starts with `start`.
    pub fn means(&self, start: &str) -> bool {
        self.means.starts_with(start)
    }
}

/// The alphabet's names for one of the table's names of keys: a mouse
/// action by its words, or a key.
fn alphabet_names(part: &str) -> Vec<&'static str> {
    let names: Vec<&str> = match part {
        "Click on a view's name" | "Click on a tab's name" => vec!["Click tab"],
        "Click on a menu's name" => vec!["Click File"],
        "Click on the top row's counts" => vec!["Click facts"],
        "Click on a button of the action bar" => vec!["Click button"],
        "Click on a row of a list" | "Click on a box" | "Click on a choice" => vec!["Click row"],
        "Click outside the menu" => vec!["Click outside"],
        "Click" | "Click or wheel" => vec!["Click row"],
        _ if part.starts_with("Click") => Vec::new(),
        _ => part.split(' ').collect(),
    };
    names
        .into_iter()
        .filter_map(super::explore::name_of)
        .collect()
}

/// A node of the interface tree.
#[derive(Clone, Debug)]
pub struct TreeNode {
    pub id: String,
    pub parent: String,
    pub reach: String,
    pub leave: String,
    /// The ids of the key table's rows that apply in the node.
    pub keys: Vec<String>,
    pub supported: bool,
    /// The fields the `?` search matches in the node's list.
    pub search: Vec<String>,
}

/// The two tables, read once.
pub struct Tables {
    pub rows: Vec<KeyRow>,
    pub tree: Vec<TreeNode>,
}

impl Tables {
    pub fn row(&self, id: &str) -> Option<&KeyRow> {
        self.rows.iter().find(|r| r.id == id)
    }

    pub fn node(&self, id: &str) -> Option<&TreeNode> {
        self.tree.iter().find(|n| n.id == id)
    }

    /// The supported rows the node `node` lists, in its order.
    pub fn rows_of<'a>(&'a self, node: &'a TreeNode) -> impl Iterator<Item = &'a KeyRow> + 'a {
        node.keys
            .iter()
            .filter_map(|id| self.row(id))
            .filter(|r| r.supported)
    }

    /// The rows that say what `key`, by the alphabet's name, means in the
    /// node `node`: every supported row the node lists that names the key,
    /// else its rows for any letter (for a letter), else its rows for any
    /// other key.
    pub fn rows_for<'a>(&'a self, node: &'a TreeNode, key: &str) -> Vec<&'a KeyRow> {
        let named: Vec<&KeyRow> = self
            .rows_of(node)
            .filter(|r| r.keys.contains(&key))
            .collect();
        if !named.is_empty() {
            return named;
        }
        let letter = key.chars().count() == 1 && key.chars().all(char::is_alphabetic);
        let letters: Vec<&KeyRow> = self.rows_of(node).filter(|r| r.letters).collect();
        if letter && !letters.is_empty() {
            return letters;
        }
        self.rows_of(node).filter(|r| r.other).collect()
    }
}

/// The tables, read once for every check.
pub fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let gone = read("crates/vtium-config-widgets/contract/d11-gone.tsv");
        let gone_of = |kind: &str| -> BTreeSet<String> {
            gone.iter()
                .filter(|g| g.get("kind").is_some_and(|k| k == kind))
                .filter_map(|g| g.get("id").cloned())
                .collect()
        };
        let col = |r: &BTreeMap<String, String>, c: &str| r.get(c).cloned().unwrap_or_default();
        let rows = merged(
            "docs/architecture/platform/ADR-1403-keys.tsv",
            "crates/vtium-config-widgets/contract/d11-keys.tsv",
            &gone_of("row"),
        )
        .iter()
        .map(|r| {
            let key = col(r, "key");
            let other = key == "any other key";
            let letters = key == "any letter";
            let shown = matches!(
                key.as_str(),
                "key line" | "Help and key line" | "action bar"
            );
            let keys = if other || letters || shown {
                Vec::new()
            } else {
                key.split(", ").flat_map(alphabet_names).collect()
            };
            KeyRow {
                id: col(r, "id"),
                key,
                keys,
                other,
                letters,
                state: col(r, "state"),
                means: col(r, "means"),
                supported: col(r, "status") == "supported",
                label: col(r, "label"),
            }
        })
        .collect();
        // the search fields: the tree's own column once it has one, else
        // the overlay's file
        let search: BTreeMap<String, String> =
            read("crates/vtium-config-widgets/contract/d11-search.tsv")
                .iter()
                .map(|r| (col(r, "id"), col(r, "search")))
                .collect();
        let fields = |s: &str| -> Vec<String> {
            s.split(", ")
                .filter(|f| !f.is_empty() && *f != "-")
                .map(str::to_owned)
                .collect()
        };
        let tree = merged(
            "docs/architecture/platform/ADR-1403-tree.tsv",
            "crates/vtium-config-widgets/contract/d11-tree.tsv",
            &gone_of("node"),
        )
        .iter()
        .map(|r| {
            let id = col(r, "id");
            let own = col(r, "search");
            TreeNode {
                search: fields(if own.is_empty() {
                    search.get(&id).map_or("", String::as_str)
                } else {
                    &own
                }),
                parent: col(r, "parent"),
                reach: col(r, "reach"),
                leave: col(r, "leave"),
                keys: col(r, "keys")
                    .split(", ")
                    .filter(|k| !k.is_empty() && *k != "-")
                    .map(str::to_owned)
                    .collect(),
                supported: col(r, "status") == "supported",
                id,
            }
        })
        .collect();
        Tables { rows, tree }
    })
}
