//! A reading of ADR-1403's interface tree and key table that predicts
//! where a key takes the person, written apart from vtium-config-tui's
//! navigation module: it reads the tables through `tables.rs`, by the
//! walk's own code, and never calls the module. The exhaustive walk and
//! the property walk both judge the TUI against it.
//!
//! The reference predicts only what the tables decide whatever a tab
//! holds:
//!
//! - A key whose row in the node says it does nothing leaves the TUI in
//!   the node.
//! - On a tab with nothing open, Tab and Shift-Tab go to the next tab and
//!   the tab before, in the order the digits' rows give, and a digit or a
//!   tab's Alt letter goes to the tab its row names. The keys that move
//!   stay in the tab.
//! - On a tab, Alt-F and F10 open the File menu, Alt-A the Actions menu,
//!   Alt-P the Problems list, Ctrl-S Save's review, F1 the help, and `?`
//!   the search row where the node's `search` column names fields. Ctrl-F
//!   opens Find on any screen but a question or an entry.
//! - In a level, Esc goes where the node's `leave` column sends it.
//! - `q` and Ctrl-C quit at once, with nothing asked, where their rows say
//!   so, and at a question Esc and Ctrl-C go back without acting.
//! - Shrinking the terminal below 80×25 shows the screen below the
//!   minimum size, under everything but a question.
//!
//! Every other key, such as Enter on a list, depends on what the tab
//! holds, and the reference predicts nothing for it.
//!
//! References:
//! - ADR-1206 D5: the exhaustive walk and the property test check the
//!   navigation against a reading of the tables that is not the
//!   navigation module.
//! - ADR-1403 D10: the tree of use, the same key for the same action, and
//!   the digits.
//! - ADR-1403 D11: the menus, the Alt letters, quitting at once, F1, Find
//!   and the Problems list.

use std::sync::OnceLock;

use super::tables::{self, TreeNode};

/// What the tables say a key does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    /// The TUI is in this node after the key.
    At(String),
    /// The TUI is on this tab with nothing open after the key, in
    /// whichever of the tab's nodes its highlight calls for.
    InTab(String),
    /// The TUI leaves, with status 0 where the row says it quits at once.
    Quits { at_once: bool },
    /// The question closes without acting, back to the node under it.
    LeavesQuestion,
    /// The live trial ends, back to the node under it.
    LeavesTrial,
    /// The TUI is in one of these nodes, as `.` opens More's menu only
    /// where the bar has buttons it cannot show.
    OneOf(Vec<String>),
}

/// The kind of state the node `id` is, as the tree's ids show it.
pub fn kind(id: &str) -> &'static str {
    match id {
        "left" => "left",
        "any.trial" => "trial",
        "any.too-small" => "too-small",
        "any.search" | "any.find" => "typing",
        _ if id.starts_with("any.question.") => "question",
        _ if id.starts_with("any.menu.") && !id.starts_with("any.menu.file.manage") => "menu",
        _ if id.ends_with(".typing") => "typing",
        _ if !id.contains('.') => "view",
        _ => "layer",
    }
}

/// The tabs in the top row's order, by the digits' rows: "Goes straight
/// to the Fonts tab." for 1 names `fonts`.
pub fn order() -> &'static [String] {
    static ORDER: OnceLock<Vec<String>> = OnceLock::new();
    ORDER.get_or_init(|| {
        let t = tables::tables();
        (1..=9)
            .filter_map(|d| t.row(&format!("{d}.view")))
            .filter_map(|r| goes_to(&r.means))
            .collect()
    })
}

/// The tab a row's meaning names, as "Goes straight to the Looks tab.".
fn goes_to(means: &str) -> Option<String> {
    let tab = means
        .strip_prefix("Goes straight to the ")?
        .strip_suffix(" tab.")?;
    Some(tab.to_lowercase())
}

/// The tab a node of the view kind belongs to: the node itself, or its
/// parent where the node is the tab's list in another focus, such as
/// `looks-resources` under `looks`.
pub fn tab_of(id: &str) -> String {
    let t = tables::tables();
    match t.node(id) {
        Some(n) if n.parent != "tui" && n.parent != "-" && kind(&n.parent) == "view" => {
            n.parent.clone()
        }
        _ => id.to_owned(),
    }
}

/// The node Esc goes to, by the node's `leave` column: "Esc to flow",
/// "Esc goes to flow", "both to settings.section".
fn esc_target(node: &TreeNode) -> Option<String> {
    let clause = node.leave.split("; ").find(|c| c.contains("Esc"))?;
    let after = ["Esc to ", "Esc goes to ", "both to ", "and goes to "]
        .iter()
        .find_map(|m| clause.split_once(m).map(|(_, rest)| rest))?;
    let target = after.split([' ', ',']).next()?;
    tables::tables().node(target).map(|n| n.id.clone())
}

/// The node of the tree whose `reach` opens it with `key` on a tab with
/// nothing open, as "Alt-F or F10 on a tab with nothing open" opens
/// `any.menu.file` and "Ctrl-S on a tab" opens `any.save`.
fn opened_from_a_tab(key: &str) -> Option<String> {
    tables::tables()
        .tree
        .iter()
        .filter(|n| n.supported && n.parent == "any")
        .find(|n| {
            let first = n.reach.split(", ").next().unwrap_or_default();
            let keys = first
                .strip_suffix(" on a tab with nothing open")
                .or_else(|| first.strip_suffix(" on a tab"))
                .unwrap_or(if n.reach == key { key } else { "" });
            keys.split(" or ").any(|k| k == key)
        })
        .map(|n| n.id.clone())
}

/// What `key`, by the alphabet's name, does in the node `from`, as far as
/// the tables decide it whatever the tab holds; `None` where they do not.
pub fn expect(from: &str, key: &str) -> Option<Expect> {
    let t = tables::tables();
    let node = t.node(from)?;
    let kind = kind(from);
    if key == "Shrink" {
        return Some(if kind == "question" {
            Expect::At(from.to_owned())
        } else {
            Expect::At("any.too-small".to_owned())
        });
    }
    if key == "Grow" {
        return (kind != "too-small").then(|| Expect::At(from.to_owned()));
    }
    let rows = t.rows_for(node, key);
    let means = |start: &str| rows.iter().any(|r| r.means(start));
    match kind {
        "question" if matches!(key, "Esc" | "Ctrl-C") => return Some(Expect::LeavesQuestion),
        "question" if means("Does nothing") => return Some(Expect::At(from.to_owned())),
        "question" => return None,
        "trial" if key.starts_with("Click") || key.starts_with("Wheel") => return None,
        "trial" if means("Puts back") => return Some(Expect::LeavesTrial),
        "trial" if key == "Ctrl-C" => return Some(Expect::Quits { at_once: false }),
        "trial" => return None,
        _ => {}
    }
    if means("Quits vtium-config at once") {
        return Some(Expect::Quits { at_once: true });
    }
    if matches!(key, "q" | "Ctrl-C") && means("Saves the changes") {
        return Some(Expect::Quits { at_once: false });
    }
    if means("Does nothing") {
        return Some(Expect::At(from.to_owned()));
    }
    if means("Opens the buttons the action bar has no room for") {
        return Some(Expect::OneOf(vec![
            from.to_owned(),
            "any.menu.actions".to_owned(),
        ]));
    }
    // a value being typed takes every character as text
    if kind == "typing" && key.chars().count() == 1 {
        return Some(Expect::At(from.to_owned()));
    }
    if key == "Ctrl-F" && means("Opens Find") || means("Closes the menu and opens Find") {
        return Some(Expect::At("any.find".to_owned()));
    }
    if matches!(kind, "layer" | "typing") && key == "Esc" && node.supported {
        return esc_target(node).map(Expect::At);
    }
    if kind != "view" {
        return None;
    }
    if key == "?" && means("Opens the search row") {
        return Some(if node.search.is_empty() {
            Expect::At(from.to_owned())
        } else {
            Expect::At("any.search".to_owned())
        });
    }
    if key != "?"
        && !rows.is_empty()
        && let Some(to) = opened_from_a_tab(key)
    {
        // Save's review opens only where there is something to save
        return Some(if to.starts_with("any.save") {
            Expect::OneOf(vec![to, "any.save.changed".to_owned(), from.to_owned()])
        } else {
            Expect::At(to)
        });
    }
    let order = order();
    let tab = tab_of(from);
    let at = order.iter().position(|t| *t == tab);
    match (key, at) {
        ("Tab", Some(i)) => {
            return order.get((i + 1) % order.len()).cloned().map(Expect::InTab);
        }
        ("Shift-Tab", Some(i)) => {
            return order
                .get((i + order.len() - 1) % order.len())
                .cloned()
                .map(Expect::InTab);
        }
        _ => {}
    }
    if let Some(to) = rows.iter().find_map(|r| goes_to(&r.means)) {
        return order.contains(&to).then_some(Expect::InTab(to));
    }
    // a tab's Alt letter: the row lists the keys and the tabs in one order
    if let Some(row) = rows
        .iter()
        .find(|r| r.means("Goes straight to the tab whose letter"))
    {
        let names = row
            .means
            .split(": ")
            .nth(1)
            .unwrap_or_default()
            .trim_end_matches('.')
            .replace(" or ", ", ");
        let i = row.key.split(", ").position(|k| k == key)?;
        let to = names.split(", ").nth(i)?.to_lowercase();
        return Some(Expect::InTab(to));
    }
    // the keys that move stay in the tab
    let moves = ["Moves", "Goes to the", "Scrolls", "Move "];
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
/// in, `left` once it has left, and `exit` its status then.
pub fn agrees(want: &Expect, to: &str, exit: Option<u8>) -> Result<(), String> {
    let ok = match want {
        Expect::At(id) => exit.is_none() && to == id,
        Expect::InTab(tab) => exit.is_none() && kind(to) == "view" && tab_of(to) == *tab,
        Expect::Quits { at_once } => exit.is_some() && (!at_once || exit == Some(0)),
        Expect::LeavesQuestion => exit.is_none() && kind(to) != "question",
        Expect::LeavesTrial => exit.is_none() && to != "any.trial",
        Expect::OneOf(ids) => exit.is_none() && ids.iter().any(|id| id == to),
    };
    if ok {
        Ok(())
    } else {
        let now = exit.map_or_else(|| to.to_owned(), |s| format!("exit {s}"));
        Err(format!("the tables say {want:?}, and the TUI is at {now}"))
    }
}
