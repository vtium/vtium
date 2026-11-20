//! A reading of ADR-1403's interface tree and key table that predicts
//! where a key takes the person, written apart from the TUI's navigation
//! module: it reads the two TSV files by its own code and never calls
//! `vtium_config::tui::nav`. The exhaustive walk and the property test
//! both judge the TUI against it.
//!
//! The reference predicts only what the tables decide whatever a tab
//! holds:
//!
//! - A key whose row in the node says it does nothing, or that it shows
//!   the key line's next page, leaves the TUI in the node.
//! - In a tab with nothing open, Tab and Shift-Tab go to the next tab and
//!   the tab before, in the order of ADR-1201's views table as the TUI
//!   has it today. A digit whose row is supported goes to the tab its row
//!   names, and a digit whose row is planned does nothing. The keys that
//!   move stay in the tab, and a key that is a child node's whole `reach`
//!   opens that child, as `v` opens the file view. A row whose meaning
//!   begins "Asks whether" opens a question. Two planned rows act by
//!   what their notes say the TUI does today, so the walks reach a value
//!   being typed.
//! - In a layer or a typed value, Esc goes where the node's `leave`
//!   column sends it.
//! - `q` quits or asks, Ctrl-C saves and quits, and at a question Esc and
//!   Ctrl-C go back without acting.
//! - Shrinking the terminal below 80×25 shows the screen below the
//!   minimum size.
//!
//! Every other key, such as Enter on a list, depends on what the tab holds,
//! and the reference predicts nothing for it.
//!
//! References:
//! - ADR-1206 D5: the exhaustive walk and the property test check the
//!   navigation against a reading of the tables that is not the
//!   navigation module.
//! - ADR-1403 D10: the tree of use, the same key for the same action, and
//!   the digits.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// What the tables say a key does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    /// The TUI is in this node after the key.
    At(String),
    /// The TUI is in this tab's list after the key, in whichever of the
    /// list's nodes the highlight then calls for.
    InTab(String),
    /// The TUI leaves with this status.
    Exits(u8),
    /// The TUI quits with status 0, or asks the quit question or the leave
    /// question first.
    QuitsOrAsks,
    /// The question closes without acting, back to the node under it.
    LeavesQuestion,
    /// The live trial ends, back to the node under it.
    LeavesTrial,
}

/// A node of the tree, with the columns the reference reads.
struct TreeRow {
    id: String,
    parent: String,
    reach: String,
    leave: String,
    keys: Vec<String>,
    status: String,
}

/// A row of the key table, with the columns the reference reads.
struct KeyRow {
    key: String,
    means: String,
    status: String,
}

struct Tables {
    tree: Vec<TreeRow>,
    rows: BTreeMap<String, KeyRow>,
    /// The tabs in the order Tab steps through them today.
    order: Vec<String>,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A TSV's rows, each as its columns by name.
fn read(path: &str) -> Vec<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(root().join(path))
        .unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
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

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let col = |r: &BTreeMap<String, String>, c: &str| r.get(c).cloned().unwrap_or_default();
        let tree = read("docs/architecture/platform/ADR-1403-tree.tsv")
            .iter()
            .map(|r| TreeRow {
                id: col(r, "id"),
                parent: col(r, "parent"),
                reach: col(r, "reach"),
                leave: col(r, "leave"),
                keys: col(r, "keys")
                    .split(", ")
                    .filter(|k| !k.is_empty() && *k != "-")
                    .map(str::to_owned)
                    .collect(),
                status: col(r, "status"),
            })
            .collect();
        let rows = read("docs/architecture/platform/ADR-1403-keys.tsv")
            .iter()
            .map(|r| {
                (
                    col(r, "id"),
                    KeyRow {
                        key: col(r, "key"),
                        means: col(r, "means"),
                        status: col(r, "status"),
                    },
                )
            })
            .collect();
        // a planned row of the views table gives what the TUI does today
        // in its `now` column, as `order=2`
        let mut order: Vec<(usize, String)> = read("docs/architecture/font/ADR-1201-views.tsv")
            .iter()
            .filter_map(|r| {
                let now = col(r, "now");
                let today = now
                    .split("; ")
                    .find_map(|p| p.strip_prefix("order="))
                    .map(str::to_owned)
                    .unwrap_or_else(|| col(r, "order"));
                Some((today.parse().ok()?, col(r, "id")))
            })
            .collect();
        order.sort();
        Tables {
            tree,
            rows,
            order: order.into_iter().map(|(_, id)| id).collect(),
        }
    })
}

fn tree_row(id: &str) -> Option<&'static TreeRow> {
    tables().tree.iter().find(|n| n.id == id)
}

/// The kind of state the node `id` is, as the tree's ids show it.
pub fn kind(id: &str) -> &'static str {
    if id.starts_with("any.question.") {
        "question"
    } else if id == "any.trial" {
        "trial"
    } else if id == "any.too-small" {
        "too-small"
    } else if id.ends_with(".typing") {
        "typing"
    } else if id.contains('.') {
        "layer"
    } else {
        "view"
    }
}

/// The node's rows that name `key`, else its rows for any letter (for a
/// letter) or for any other key, each supported.
fn rows_for(node: &TreeRow, key: &str) -> Vec<&'static KeyRow> {
    let listed: Vec<&KeyRow> = node
        .keys
        .iter()
        .filter_map(|id| tables().rows.get(id))
        .filter(|r| r.status == "supported")
        .collect();
    let named: Vec<&KeyRow> = listed
        .iter()
        .copied()
        .filter(|r| r.key.split(", ").any(|k| k == key))
        .collect();
    if !named.is_empty() {
        return named;
    }
    let letter = key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic());
    let letters: Vec<&KeyRow> = listed
        .iter()
        .copied()
        .filter(|r| r.key == "any letter")
        .collect();
    if letter && !letters.is_empty() {
        return letters;
    }
    listed
        .into_iter()
        .filter(|r| r.key == "any other key")
        .collect()
}

/// The planned row, if any, that the node lists for `key` by its name.
fn planned_row(node: &TreeRow, key: &str) -> bool {
    node.keys
        .iter()
        .filter_map(|id| tables().rows.get(id))
        .any(|r| r.status != "supported" && r.key.split(", ").any(|k| k == key))
}

/// The node Esc goes to, by the node's `leave` column: "Esc to flow",
/// "Esc goes to flow", "Esc closes, both to settings.section", "Esc drops
/// it and goes to flow.maker.picker".
fn esc_target(node: &TreeRow) -> Option<String> {
    let clause = node.leave.split("; ").find(|c| c.contains("Esc"))?;
    let after = ["Esc to ", "Esc goes to ", "both to ", "and goes to "]
        .iter()
        .find_map(|m| clause.split_once(m).map(|(_, rest)| rest))?;
    let target = after.split([' ', ',']).next()?;
    tree_row(target).map(|n| n.id.clone())
}

/// The tab a node of the view kind belongs to: the node itself, or its
/// parent where the node is the tab's list in another focus, such as
/// `looks-resources` under `looks`.
pub fn tab_of(id: &str) -> String {
    match tree_row(id) {
        Some(n) if n.parent != "tui" && n.parent != "-" && kind(&n.parent) == "view" => {
            n.parent.clone()
        }
        _ => id.to_owned(),
    }
}

/// What `key`, by the alphabet's name, does in the node `from`, as far as
/// the tables decide it whatever the tab holds; `None` where they do not.
pub fn expect(from: &str, key: &str) -> Option<Expect> {
    let node = tree_row(from)?;
    let kind = kind(from);
    if let Some((_, _, to)) = super::tabs::TABS
        .iter()
        .flat_map(|t| t.today)
        .find(|(f, k, _)| *f == from && *k == key)
    {
        return Some(Expect::At((*to).to_owned()));
    }
    if key == "Shrink" {
        return matches!(kind, "view" | "layer" | "typing")
            .then(|| Expect::At("any.too-small".to_owned()));
    }
    if key == "Grow" {
        return (kind != "too-small").then(|| Expect::At(from.to_owned()));
    }
    let rows = rows_for(node, key);
    let means = |start: &str| rows.iter().any(|r| r.means.starts_with(start));
    match kind {
        "question" if matches!(key, "Esc" | "Ctrl-C") => return Some(Expect::LeavesQuestion),
        "question" if means("Does nothing") => return Some(Expect::At(from.to_owned())),
        "question" => return None,
        "trial" if key == "Ctrl-C" => return Some(Expect::Exits(130)),
        "trial" if key.starts_with("Click") || key.starts_with("Wheel") => return None,
        "trial" => return Some(Expect::LeavesTrial),
        _ => {}
    }
    if key == "Ctrl-C" && means("Saves the changes") {
        return Some(Expect::Exits(130));
    }
    if key == "q" && kind != "typing" && means("Quits") {
        return Some(Expect::QuitsOrAsks);
    }
    if means("Does nothing") || means("Shows the key line's next page") {
        return Some(Expect::At(from.to_owned()));
    }
    // a value being typed takes every character as text
    if kind == "typing" && key.chars().count() == 1 {
        return Some(Expect::At(from.to_owned()));
    }
    if matches!(kind, "layer" | "typing") && key == "Esc" && node.status == "supported" {
        return esc_target(node).map(Expect::At);
    }
    if kind != "view" {
        return None;
    }
    let order = &tables().order;
    let tab = tab_of(from);
    let at = order.iter().position(|t| *t == tab);
    match (key, at) {
        ("Tab", Some(i)) => {
            return order.get((i + 1) % order.len()).cloned().map(Expect::At);
        }
        ("Shift-Tab", Some(i)) => {
            return order
                .get((i + order.len() - 1) % order.len())
                .cloned()
                .map(Expect::At);
        }
        _ => {}
    }
    // a digit's row names its tab: "Goes straight to the Fonts tab."
    if let Some(row) = rows
        .iter()
        .find(|r| r.means.starts_with("Goes straight to the "))
    {
        let tab = row
            .means
            .trim_start_matches("Goes straight to the ")
            .trim_end_matches(" tab.")
            .to_lowercase();
        return order.contains(&tab).then_some(Expect::At(tab));
    }
    if planned_row(node, key) && key.chars().all(|c| c.is_ascii_digit()) {
        return Some(Expect::At(from.to_owned()));
    }
    if means("Asks whether") {
        return Some(Expect::At("any.question.confirm".to_owned()));
    }
    if let Some(child) = tables()
        .tree
        .iter()
        .find(|n| n.parent == from && n.status == "supported" && n.reach == key)
    {
        return Some(Expect::At(child.id.clone()));
    }
    // the keys that move stay in the tab
    let moves = ["Moves", "Goes to the", "Scrolls"];
    if matches!(key, "Up" | "Down" | "Home" | "End" | "PgUp" | "PgDn")
        && !rows.is_empty()
        && rows
            .iter()
            .all(|r| moves.iter().any(|m| r.means.starts_with(m)))
    {
        return Some(Expect::InTab(tab));
    }
    None
}

/// Whether the TUI after a key agrees with `want`: `to` is the node it is
/// in, empty once it has left, and `exit` its status then.
pub fn agrees(want: &Expect, to: &str, exit: Option<u8>) -> Result<(), String> {
    let ok = match want {
        Expect::At(id) => exit.is_none() && to == id,
        Expect::InTab(tab) => exit.is_none() && kind(to) == "view" && tab_of(to) == *tab,
        Expect::Exits(status) => exit == Some(*status),
        Expect::QuitsOrAsks => {
            exit == Some(0) || matches!(to, "any.question.quit" | "any.question.leave")
        }
        Expect::LeavesQuestion => exit.is_none() && kind(to) != "question",
        Expect::LeavesTrial => exit.is_none() && to != "any.trial",
    };
    if ok {
        Ok(())
    } else {
        let now = exit.map_or_else(|| to.to_owned(), |s| format!("exit {s}"));
        Err(format!("the tables say {want:?}, and the TUI is at {now}"))
    }
}
