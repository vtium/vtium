//! The two tables the records keep beside them, read as the scenarios
//! check them: ADR-1403's key table, which says what each key means in
//! each kind of state, and ADR-1201's views table, which says which views
//! there are, in what order, what each tab bar shows and what each view
//! writes.

use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A TSV's rows, each as its columns by name.
fn read(path: &str) -> Vec<BTreeMap<String, String>> {
    let path = root().join(path);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
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

/// A row of the key table.
#[derive(Clone, Debug)]
pub struct KeyRow {
    pub id: String,
    /// The alphabet's names of the keys the row is about; empty for
    /// `any other key` and for the key line.
    pub keys: Vec<&'static str>,
    pub other: bool,
    pub key_line: bool,
    pub status: String,
}

/// The alphabet's name for the table's name of a key.
fn key_name(key: &str) -> Option<&'static str> {
    super::explore::ALPHABET
        .iter()
        .map(|(n, _)| *n)
        .find(|n| *n == key)
}

pub fn keys() -> Vec<KeyRow> {
    read("docs/architecture/platform/ADR-1403-keys.tsv")
        .into_iter()
        .map(|r| {
            let col = |c: &str| r.get(c).cloned().unwrap_or_default();
            let key = col("key");
            let other = key == "any other key";
            // rows about what the screen shows, not about a key
            let key_line = matches!(key.as_str(), "key line" | "Help and key line");
            let keys = if other || key_line {
                Vec::new()
            } else {
                key.split([' ', ','])
                    .filter(|k| !k.is_empty())
                    .map(|k| {
                        key_name(k).unwrap_or_else(|| {
                            panic!(
                                "the key table's row {} names {k}, which the alphabet lacks",
                                col("id")
                            )
                        })
                    })
                    .collect()
            };
            KeyRow {
                id: col("id"),
                keys,
                other,
                key_line,
                status: col("status"),
            }
        })
        .collect()
}

/// The rows that say what `key` means in the tree node `node`: every row
/// the node lists that names the key, else the node's `any other key`
/// row. Several rows can name one key in a node, such as Enter on a card,
/// which opens a list for each kind of field; each of them is checked.
pub fn rows_for<'a>(rows: &'a [KeyRow], node: &TreeNode, key: &str) -> Vec<&'a KeyRow> {
    let listed = || {
        rows.iter()
            .filter(|r| !r.key_line && node.keys.iter().any(|k| *k == r.id))
    };
    let named: Vec<&KeyRow> = listed().filter(|r| r.keys.contains(&key)).collect();
    if named.is_empty() {
        listed().filter(|r| r.other).collect()
    } else {
        named
    }
}

/// A node of ADR-1403's interface tree, `ADR-1403-tree.tsv`.
#[derive(Clone, Debug)]
pub struct TreeNode {
    pub id: String,
    pub parent: String,
    /// The ids of the key table's rows that apply in the node.
    pub keys: Vec<String>,
    pub status: String,
    pub now: String,
}

pub fn tree() -> Vec<TreeNode> {
    read("docs/architecture/platform/ADR-1403-tree.tsv")
        .into_iter()
        .map(|r| {
            let col = |c: &str| r.get(c).cloned().unwrap_or_default();
            TreeNode {
                id: col("id"),
                parent: col("parent"),
                keys: col("keys")
                    .split(", ")
                    .filter(|k| !k.is_empty() && *k != "-")
                    .map(str::to_owned)
                    .collect(),
                status: col("status"),
                now: col("now"),
            }
        })
        .collect()
}

/// A row of the views table, with the `now` column's values in place of
/// the row's own where the row is planned: what the TUI does today.
#[derive(Clone, Debug)]
pub struct ViewRow {
    pub id: String,
    pub under: String,
    pub order: Option<usize>,
    /// The actions' names, without their keys: `Save`, `View file`.
    pub actions: Vec<String>,
    pub writes: Vec<String>,
}

pub fn views() -> Vec<ViewRow> {
    read("docs/architecture/font/ADR-1201-views.tsv")
        .into_iter()
        .map(|r| {
            let col = |c: &str| r.get(c).cloned().unwrap_or_default();
            let mut now: BTreeMap<String, String> = BTreeMap::new();
            for part in col("now").split("; ").filter(|p| !p.is_empty()) {
                if let Some((k, v)) = part.split_once('=') {
                    now.insert(k.to_owned(), v.to_owned());
                }
            }
            let mut get = |c: &str| now.remove(c).unwrap_or_else(|| col(c));
            let list = |v: String| -> Vec<String> {
                v.split(", ")
                    .filter(|a| !a.is_empty() && *a != "-")
                    .map(str::to_owned)
                    .collect()
            };
            let actions = list(get("actions"))
                .into_iter()
                .map(|a| a.split_once(' ').map_or(a.clone(), |(_, n)| n.to_owned()))
                .collect();
            ViewRow {
                id: col("id"),
                under: get("under"),
                order: get("order").parse().ok(),
                actions,
                writes: list(get("writes")),
            }
        })
        .collect()
}
