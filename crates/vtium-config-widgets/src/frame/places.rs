//! What a click on the frame hits. The places come from the same layout
//! the view draws, so update reads where a click lands from what is on
//! the screen.
//!
//! References:
//! - ADR-1206 D4: update reads the same places the view draws, and the
//!   frame's layout function returns the places of the top row's menus
//!   and tabs and of the action bar's buttons.
//! - ADR-1403 D11, "The mouse": a click on a menu's or a tab's name opens
//!   it, a click on a count opens what it counts, and a click on a button
//!   acts.

use ratatui_core::layout::{Position, Rect};

use super::layout::Regions;
use super::top::MenuName;

/// What a click on the frame hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// The name of the File or the Actions menu, which opens the menu.
    Menu(MenuName),
    /// A tab's name, by its index in the top row's tabs.
    Tab(usize),
    /// The count of changes not saved, which opens Show changes.
    NotSaved,
    /// The count of problems, which opens the Problems list.
    Problems,
    /// A button of the action bar, by its index in the bar's buttons.
    Button(usize),
    /// `[.] More`, which opens the buttons the bar has no room for.
    More,
}

/// The places of everything a click on the frame can hit, and the
/// frame's regions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// The File and Actions menus' names, in that order.
    pub menus: [Rect; 2],
    /// Each tab's name, in the top row's order.
    pub tabs: Vec<Rect>,
    /// The count of changes not saved, where it shows and can be opened.
    pub not_saved: Option<Rect>,
    /// The count of problems, where it shows and can be opened.
    pub problems: Option<Rect>,
    /// Each button the bar shows: its index in the bar's buttons and its
    /// place, the bracketed key and the label together.
    pub buttons: Vec<(usize, Rect)>,
    /// `[.] More`, where the bar shows it.
    pub more: Option<Rect>,
    /// The index of each button the bar has no room for, in order of
    /// priority: what More opens.
    pub hidden: Vec<usize>,
    /// The frame's regions.
    pub regions: Regions,
}

impl Places {
    /// The rows an open menu may cover: from the rule under the top row
    /// to the rule over the action bar, so the menus' names, the tabs and
    /// the bar's buttons stay in view while a menu is open.
    pub fn menu_area(&self) -> Rect {
        let r = &self.regions;
        let top = r.top_rule.y;
        Rect::new(
            r.top.x,
            top,
            r.top.width,
            r.bottom_rule.bottom().saturating_sub(top),
        )
    }

    /// What a click at `at` hits, or `None` where it hits nothing of the
    /// frame, such as the viewer, a rule or the space between buttons.
    pub fn hit(&self, at: Position) -> Option<Hit> {
        let on = |r: &Rect| r.contains(at);
        if let Some(i) = self.menus.iter().position(on) {
            return Some(Hit::Menu(if i == 0 {
                MenuName::File
            } else {
                MenuName::Actions
            }));
        }
        if let Some(i) = self.tabs.iter().position(on) {
            return Some(Hit::Tab(i));
        }
        if self.not_saved.as_ref().is_some_and(on) {
            return Some(Hit::NotSaved);
        }
        if self.problems.as_ref().is_some_and(on) {
            return Some(Hit::Problems);
        }
        if let Some((i, _)) = self.buttons.iter().find(|(_, r)| on(r)) {
            return Some(Hit::Button(*i));
        }
        self.more.as_ref().is_some_and(on).then_some(Hit::More)
    }
}
