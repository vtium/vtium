//! The abstraction: a TUI reduced to a short abstract state, which names
//! the kind of terminal, the node of ADR-1403's interface tree the person
//! is on, the tab under it, which question is open, and whether changes
//! wait to be saved and a step waits to be undone. Two TUIs with the same
//! abstract state answer the keys of the alphabet the same way, as far as
//! the map is concerned, so the explorer keeps one of them and tries every
//! key there.
//!
//! What a tab highlights is read from the screen, as the person sees it,
//! because a tab's own state is reached only through the `Tab` trait,
//! which says nothing of it. The abstract state keeps the group the
//! highlighted item belongs to, such as `Font` of `Font sets` in the
//! Looks tab, and
//! not the item itself: the item's text tells every item of every list
//! apart, which gives the walk tens of thousands of states. The checks
//! after the walk move the highlight in each node, so a key that acts on
//! one item of a group and not another is still pressed on both.
//!
//! References:
//! - ADR-1206 D5: the walk visits every node of the interface tree and
//!   presses every key in each.
//! - Decision 130 of 2026-11-27, recorded in ADR-1403 D11: the abstract
//!   state names the open question, so a path through one question cannot
//!   borrow another's answer.

use vtium_config_tui::nav::levels::{self, Kind};

use super::sim::{self, Sim};

/// The kind of state, as the key table names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StateKind {
    View,
    Layer,
    Typing,
    Menu,
    Question,
    Trial,
    TooSmall,
    /// The TUI has exited.
    Left,
}

/// An abstract state.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Abs {
    pub term: &'static str,
    /// The node of the interface tree the person is on, as the App names
    /// it; `left` once the TUI has left.
    pub node: &'static str,
    pub kind: StateKind,
    /// The group the highlighted item belongs to, as the screen heads it.
    pub focus: String,
    /// The open question, by the row whose action its yes hands back.
    pub question: String,
    /// The tab under everything, by its place in the top row; `None` over
    /// a sheet or a menu that does not depend on the tab, such as File or
    /// Find.
    pub tab: Option<usize>,
    /// The screen is below the minimum size.
    pub small: bool,
    /// The configuration holds changes that are not saved.
    pub unsaved: bool,
    /// The undo history holds a step to undo.
    pub undo: bool,
}

impl std::fmt::Display for Abs {
    /// The state as the map writes it, in the form the map's page reads:
    /// `vtium looks(Font; undo) +unsaved`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.kind == StateKind::Left {
            return write!(f, "{} left", self.term);
        }
        write!(f, "{} {}", self.term, self.node)?;
        let mut inside: Vec<String> = Vec::new();
        if !self.focus.is_empty() {
            inside.push(self.focus.replace(['(', ')'], ""));
        }
        if !self.question.is_empty() {
            inside.push(format!("yes {}", self.question));
        }
        if self.undo {
            inside.push("undo".to_owned());
        }
        if !inside.is_empty() {
            write!(f, "({})", inside.join("; "))?;
        }
        if self.small && self.kind != StateKind::TooSmall {
            f.write_str(" below 80×25")?;
        }
        if self.unsaved {
            f.write_str(" +unsaved")?;
        }
        Ok(())
    }
}

/// The nodes that show the same over every tab: the File menu and its
/// submenus, the sheets of the document, Find, the Problems list and the
/// screen below the minimum size. A state there does not say which tab is
/// under it, so the walk tries their keys over one tab.
const ANY_TAB: [&str; 9] = [
    "any.menu.file",
    "any.new",
    "any.find",
    "any.problems",
    "any.save",
    "any.too-small",
    "any.changes",
    "any.history",
    "any.editor",
];

/// The kind of state the node `node` is.
pub fn kind_of(node: &str) -> StateKind {
    if node == "any.trial" {
        return StateKind::Trial;
    }
    match levels::kind(node) {
        Kind::View => StateKind::View,
        Kind::Layer => StateKind::Layer,
        Kind::Typing => StateKind::Typing,
        Kind::Menu => StateKind::Menu,
        Kind::Question => StateKind::Question,
        Kind::TooSmall => StateKind::TooSmall,
    }
}

/// The abstract state of `sim`.
pub fn of(sim: &Sim) -> Abs {
    of_drawn(sim, &sim.draw())
}

/// The abstract state of `sim`, whose screen is `buf`.
pub fn of_drawn(sim: &Sim, buf: &ratatui_core::buffer::Buffer) -> Abs {
    let term = sim.term.name();
    if sim.exited.is_some() {
        return Abs {
            term,
            node: "left",
            kind: StateKind::Left,
            focus: String::new(),
            question: String::new(),
            tab: None,
            small: false,
            unsaved: false,
            undo: false,
        };
    }
    let app = &sim.app;
    let node = app.node();
    let kind = kind_of(node);
    let small = app.size().too_small();
    let document = app.shared().document();
    // the screen below the minimum size shows no tab and no highlight,
    // what is open over a tab does not depend on the tab's highlight, and
    // a menu's highlight is its item, by its place
    let focus = if small {
        String::new()
    } else if let Some(menu) = app.nav().menu() {
        let at = |h: Option<usize>| h.map_or_else(|| "-".to_owned(), |h| h.to_string());
        match &menu.sub {
            Some(sub) => format!("item {} › {}", sub.item, at(sub.highlight)),
            None => format!("item {}", at(menu.highlight)),
        }
    } else if node.starts_with("any.") {
        String::new()
    } else {
        sim::group(buf)
    };
    Abs {
        term,
        node,
        kind,
        focus,
        question: app
            .nav()
            .question()
            .map(|q| q.then.row.to_owned())
            .unwrap_or_default(),
        tab: (!ANY_TAB.iter().any(|p| node.starts_with(p))).then(|| app.nav().tab()),
        small,
        unsaved: document.count() > 0,
        undo: app.shared().documents.history().to_undo().is_some(),
    }
}
