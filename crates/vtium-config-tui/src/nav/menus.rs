//! The menu that is open: which one, its highlighted item, and the
//! submenu open beside it, with the keys that move through them. The
//! items themselves come from [`crate::menus`], which the navigation
//! module is handed each time, so the state here holds only indexes.
//!
//! References:
//! - ADR-1206 D3: the navigation state holds which menu is open, its
//!   highlighted item and any open submenu.
//! - ADR-1403 D11, "The menus": Up and Down move past separators and
//!   greyed items, Left and Right move between the menus or into and out
//!   of a submenu, Enter chooses, Esc closes the submenu or the menu.

use crate::menus::{Line, Submenu, Then};

/// Which menu is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// The File menu, from the top row.
    File,
    /// The Actions menu, from the top row.
    Actions,
    /// `[.] More`, the buttons the action bar has no room for, over the
    /// bar.
    More,
}

impl Which {
    /// The node of the interface tree the menu is. More's menu has no node
    /// of its own, and acts by the Actions menu's rows, which are those of
    /// every menu but its letters.
    pub const fn node(self) -> &'static str {
        match self {
            Which::File => "any.menu.file",
            Which::Actions | Which::More => "any.menu.actions",
        }
    }

    /// The menu beside this one on the top row, to the right where
    /// `right`. More's menu has none.
    fn beside(self) -> Which {
        match self {
            Which::File => Which::Actions,
            Which::Actions => Which::File,
            Which::More => Which::More,
        }
    }
}

/// A submenu that is open: the item of the menu it belongs to, and its own
/// highlighted item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sub {
    pub item: usize,
    pub which: Submenu,
    pub highlight: Option<usize>,
}

/// An open menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open {
    pub which: Which,
    pub highlight: Option<usize>,
    pub sub: Option<Sub>,
}

/// What a key or a click did in an open menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    /// The menu stays open, perhaps with a new highlight or submenu.
    Stay,
    /// The menu closed and nothing else happens.
    Closed,
    /// The menu beside this one opens in its place.
    Beside(Which),
    /// An item was chosen: the menu closes and its action follows.
    Chose(Then),
}

impl Open {
    /// `which`, open with its first item that can act highlighted.
    pub fn new(which: Which, lines: &[Line]) -> Open {
        Open {
            which,
            highlight: step(lines, None, true),
            sub: None,
        }
    }

    /// Up or Down: moves the highlight of the submenu, where one is open,
    /// or of the menu, to the item before or after that can act.
    pub fn move_by(&mut self, down: bool, lines: &[Line], sub_lines: &[Line]) {
        match &mut self.sub {
            Some(sub) => sub.highlight = step(sub_lines, sub.highlight, down),
            None => self.highlight = step(lines, self.highlight, down),
        }
    }

    /// Right: opens the highlighted item's submenu, or moves to the menu
    /// beside this one. `sub_of` gives a submenu's lines.
    pub fn right(&mut self, lines: &[Line], sub_of: &dyn Fn(Submenu) -> Vec<Line>) -> Done {
        if self.sub.is_some() {
            return Done::Stay;
        }
        match self.highlighted(lines).map(|c| &c.then) {
            Some(Then::Sub(which)) => {
                self.open_sub(*which, sub_of);
                Done::Stay
            }
            _ => self.beside(),
        }
    }

    /// Left: closes the submenu, or moves to the menu beside this one.
    pub fn left(&mut self) -> Done {
        if self.sub.take().is_some() {
            Done::Stay
        } else {
            self.beside()
        }
    }

    /// Enter: chooses the highlighted item, or opens its submenu.
    pub fn enter(&mut self, lines: &[Line], sub_of: &dyn Fn(Submenu) -> Vec<Line>) -> Done {
        if let Some(sub) = self.sub {
            let sub_lines = sub_of(sub.which);
            return match sub
                .highlight
                .and_then(|h| sub_lines.get(h)?.choice().cloned())
            {
                Some(c) if c.acts() => Done::Chose(c.then),
                _ => Done::Stay,
            };
        }
        self.pick(self.highlight, lines, sub_of)
    }

    /// An item's own letter: chooses the item of the menu with that
    /// letter, or opens its submenu. A greyed item's letter does nothing.
    pub fn letter(
        &mut self,
        letter: char,
        lines: &[Line],
        sub_of: &dyn Fn(Submenu) -> Vec<Line>,
    ) -> Done {
        let at = lines.iter().position(|l| {
            l.choice().is_some_and(|c| {
                c.item
                    .letter
                    .is_some_and(|m| m.eq_ignore_ascii_case(&letter))
            })
        });
        match at {
            Some(i) => {
                self.highlight = Some(i);
                self.sub = None;
                self.pick(Some(i), lines, sub_of)
            }
            None => Done::Stay,
        }
    }

    /// Esc: closes the submenu, or the menu.
    pub fn escape(&mut self) -> Done {
        if self.sub.take().is_some() {
            Done::Stay
        } else {
            Done::Closed
        }
    }

    /// A click on the menu's item `i`.
    pub fn click_item(
        &mut self,
        i: usize,
        lines: &[Line],
        sub_of: &dyn Fn(Submenu) -> Vec<Line>,
    ) -> Done {
        self.sub = None;
        self.highlight = Some(i);
        self.pick(Some(i), lines, sub_of)
    }

    /// A click on the open submenu's item `i`.
    pub fn click_sub(&mut self, i: usize, sub_of: &dyn Fn(Submenu) -> Vec<Line>) -> Done {
        let Some(sub) = &mut self.sub else {
            return Done::Stay;
        };
        sub.highlight = Some(i);
        match sub_of(sub.which).get(i).and_then(Line::choice) {
            Some(c) if c.acts() => Done::Chose(c.then.clone()),
            _ => Done::Stay,
        }
    }

    fn highlighted<'l>(&self, lines: &'l [Line]) -> Option<&'l crate::menus::Choice> {
        lines.get(self.highlight?)?.choice()
    }

    fn pick(
        &mut self,
        at: Option<usize>,
        lines: &[Line],
        sub_of: &dyn Fn(Submenu) -> Vec<Line>,
    ) -> Done {
        let Some(choice) = at.and_then(|i| lines.get(i)?.choice()) else {
            return Done::Stay;
        };
        match &choice.then {
            Then::Sub(which) => {
                self.open_sub(*which, sub_of);
                Done::Stay
            }
            then if choice.acts() => Done::Chose(then.clone()),
            _ => Done::Stay,
        }
    }

    fn open_sub(&mut self, which: Submenu, sub_of: &dyn Fn(Submenu) -> Vec<Line>) {
        if let Some(item) = self.highlight {
            let lines = sub_of(which);
            self.sub = Some(Sub {
                item,
                which,
                highlight: step(&lines, None, true),
            });
        }
    }

    fn beside(&self) -> Done {
        match self.which {
            Which::More => Done::Stay,
            which => Done::Beside(which.beside()),
        }
    }
}

/// The next item after `from` that can act, going down where `down`, and
/// round from the other end; from nothing, the first or the last. `None`
/// where no item can act.
fn step(lines: &[Line], from: Option<usize>, down: bool) -> Option<usize> {
    let n = lines.len();
    let acts = |i: usize| {
        lines
            .get(i)
            .and_then(Line::choice)
            .is_some_and(|c| c.acts())
    };
    let start = match from {
        Some(f) => f,
        None if down => n.checked_sub(1)?,
        None => 0,
    };
    (1..=n)
        .map(|k| {
            if down {
                (start + k) % n
            } else {
                (start + n - k % n) % n
            }
        })
        .find(|&i| acts(i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::Choice;
    use vtium_config_core::ui::Action;
    use vtium_config_widgets::menu::Item;

    fn item(label: &str, greyed: bool) -> Line {
        let mut item = Item::new(label);
        if greyed {
            item = item.greyed("no");
        }
        Line::Choice(Choice {
            item,
            then: Then::Act(Action::of("q.view")),
        })
    }

    #[test]
    fn the_highlight_steps_past_separators_and_greyed_items_and_wraps() {
        let lines = [
            item("a", true),
            item("b", false),
            Line::Separator,
            item("c", true),
            item("d", false),
        ];
        assert_eq!(step(&lines, None, true), Some(1));
        assert_eq!(step(&lines, Some(1), true), Some(4));
        assert_eq!(step(&lines, Some(4), true), Some(1));
        assert_eq!(step(&lines, Some(1), false), Some(4));
        assert_eq!(step(&[item("x", true)], None, true), None);
        assert_eq!(step(&[], None, true), None);
    }
}
