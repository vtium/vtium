//! The list box that opens under a line with choices: a Flow card's lines,
//! a look card's lines, a setting's values and the roles of a new font
//! set. A tab hands it the line's choices through [`Choices`], and the
//! box draws them, moves its highlight, scrolls, and returns the change
//! the tab named for a kept pick. It knows nothing of fonts, looks or
//! settings.
//!
//! A [`ListBox`] is a value that the tab's state holds while the box is
//! open: its highlight, its offset and its filter. [`ListBox::place`]
//! finds its place in the area the tab gives it, under the line, or above
//! it when the rows below cannot hold every choice and the rows above
//! can. Where neither side holds them all, the larger side does, and the
//! box scrolls. Drawing takes the state read-only, and keeps the
//! highlight in sight in whatever place it is given. [`ListBox::places`]
//! gives the place of each row drawn, so a click lands on what it hits,
//! from the same layout the drawing uses.
//!
//! The keys: Up and Down move the highlight, Home, End, PgUp and PgDn
//! move it to the ends or by the rows shown, and Enter keeps it. Esc
//! closes the box and changes nothing. A click on a choice highlights it,
//! and a click on the choice already highlighted keeps it, as Enter does.
//! While the line has more choices than the box shows, and the box's node
//! names fields for the search, `?` or a click on the foot's `[?] Search`
//! opens the search row on the choices' text. A kept filter's first Esc
//! clears it.
//!
//! References:
//! - ADR-1206 D8: one widget in this crate, drawn in the place a tab
//!   gives it, its state in the tab's state, the choices through the
//!   choices trait, a kept pick as one change, and the search where the
//!   choices are many.
//! - ADR-1403 D11, "The list box": its keys and clicks, and the search
//!   it offers on a long list.
//! - ADR-1206 D10: a list box with many choices scrolls and stays
//!   readable.

mod draw;

use std::borrow::Cow;

use ratatui_core::layout::{Position, Rect};

use crate::canvas;
use crate::contract;
use crate::keys::{Code, Key};
use crate::search::{self, Search};
use crate::tab::{Choices, Searchable};

/// What a key or a click did to the list box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<C> {
    /// The highlight moved, or the box scrolled.
    Moved,
    /// The pick was kept: the change the tab named for it. The box
    /// closes.
    Kept(C),
    /// Esc closed the box, and nothing changed.
    Closed,
    /// The search opened, changed what it shows, kept its filter, or
    /// closed.
    Searched,
    /// The key or the click is not the list box's.
    Ignored,
    /// The click fell outside the box, which the tab decides on.
    Outside,
}

/// Where each part of a drawn list box is, for clicks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// The whole box, its border included.
    pub rect: Rect,
    /// Each row shown, and the number of the choice it holds.
    pub rows: Vec<(Rect, usize)>,
    /// The foot's `[?] Search`, where the box offers the search.
    pub search: Option<Rect>,
}

/// The state of an open list box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListBox {
    /// The node of the interface tree the box is, such as
    /// `looks.look.list`, whose `search` column names its search's fields.
    node: &'static str,
    /// The highlighted choice.
    highlight: usize,
    /// The first of the choices shown, counted among those the filter
    /// keeps.
    offset: usize,
    /// The widest choice's cells.
    widest: usize,
    filter: Option<Search>,
}

/// The cells around a choice's text inside the border: a blank, the
/// mark and a blank before it, and a blank after it.
const AROUND: usize = 4;

impl ListBox {
    /// The list box of the node `node` opened on `choices`, with the
    /// highlight on the line's choice now, or on the first.
    pub fn open<C>(node: &'static str, choices: &dyn Choices<C>) -> ListBox {
        let widest = (0..choices.count())
            .map(|c| canvas::cells(&choices.text(c)))
            .max()
            .unwrap_or(0);
        ListBox {
            node,
            highlight: choices.current().unwrap_or(0),
            offset: 0,
            widest,
            filter: None,
        }
    }

    /// The highlighted choice.
    pub fn highlight(&self) -> usize {
        self.highlight
    }

    /// The first row's place among the choices the box can show.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// The search over the choices, while it is open or its filter kept.
    /// The search row above the action bar draws it.
    pub fn filter(&self) -> Option<&Search> {
        self.filter.as_ref()
    }

    /// The box's place in `area`, for the line on row `line` and the box's
    /// left edge at column `x`. The box is as wide as its widest choice
    /// with its mark and border, and as high as every choice and the
    /// border, each within `area`.
    pub fn place<C>(&self, choices: &dyn Choices<C>, area: Rect, line: u16, x: u16) -> Rect {
        let below = area.bottom().saturating_sub(line.saturating_add(1));
        let above = line.saturating_sub(area.top());
        let all = choices.height(u16::MAX);
        let (top, height) = if all <= below || (all > above && below >= above) {
            (line.saturating_add(1), choices.height(below))
        } else {
            let height = choices.height(above);
            (line.saturating_sub(height), height)
        };
        // a box that scrolls is wide enough for its foot's count and its
        // search
        let inner = if choices.more_than_shown(height) {
            self.widest
                .saturating_add(AROUND)
                .max(draw::foot_cells(choices.count()))
        } else {
            self.widest.saturating_add(AROUND)
        };
        let width = u16::try_from(inner.saturating_add(2))
            .unwrap_or(u16::MAX)
            .min(area.width);
        let x = x.min(area.right().saturating_sub(width)).max(area.left());
        Rect::new(x, top, width, height)
    }

    /// Whether the box, drawn in `rect`, offers the search: its line has
    /// more choices than the box shows, and its node names the fields the
    /// search matches.
    pub fn offers_search<C>(&self, choices: &dyn Choices<C>, rect: Rect) -> bool {
        choices.more_than_shown(rect.height) && !contract::search(self.node).is_empty()
    }

    /// Acts on `key`, for the box drawn in `rect`.
    pub fn key<C>(&mut self, key: &Key, choices: &dyn Choices<C>, rect: Rect) -> Outcome<C> {
        // the key acts on what the person saw drawn in `rect`
        self.settle(choices, rect);
        if let Some(filter) = self.filter.as_mut().filter(|f| f.typing()) {
            return match filter.key(key) {
                search::Outcome::Typed | search::Outcome::Kept => {
                    self.follow_filter(choices, rect);
                    Outcome::Searched
                }
                search::Outcome::Moved => {
                    self.follow_filter(choices, rect);
                    Outcome::Moved
                }
                search::Outcome::Closed => {
                    self.filter = None;
                    self.settle(choices, rect);
                    Outcome::Searched
                }
                search::Outcome::Resumed | search::Outcome::Ignored => Outcome::Ignored,
            };
        }
        if !key.mods.is_empty() {
            return Outcome::Ignored;
        }
        let rows = shown_rows(rect);
        let far = isize::try_from(choices.count()).unwrap_or(isize::MAX);
        let page = isize::try_from(rows.max(1)).unwrap_or(1);
        match key.code {
            Code::Up => self.step(-1, choices, rect),
            Code::Down => self.step(1, choices, rect),
            Code::Home => self.step(-far, choices, rect),
            Code::End => self.step(far, choices, rect),
            Code::PageUp => self.step(-page, choices, rect),
            Code::PageDown => self.step(page, choices, rect),
            Code::Enter => self.kept(choices),
            Code::Escape if self.filter.is_some() => {
                self.filter = None;
                self.settle(choices, rect);
                Outcome::Searched
            }
            Code::Escape => Outcome::Closed,
            Code::Char('?') => self.search(choices, rect),
            _ => Outcome::Ignored,
        }
    }

    /// Acts on a click at `at`, for the box drawn in `rect`.
    pub fn click<C>(&mut self, at: Position, choices: &dyn Choices<C>, rect: Rect) -> Outcome<C> {
        if !rect.contains(at) {
            return Outcome::Outside;
        }
        // the click lands on what the person saw drawn in `rect`
        self.settle(choices, rect);
        let places = self.places(choices, rect);
        if places.search.is_some_and(|s| s.contains(at)) {
            return self.search(choices, rect);
        }
        let Some(&(_, choice)) = places.rows.iter().find(|(r, _)| r.contains(at)) else {
            return Outcome::Ignored;
        };
        if choice == self.highlight {
            return self.kept(choices);
        }
        self.highlight = choice;
        if let Some(filter) = self.filter.as_mut() {
            let from = filter.highlighted().unwrap_or(0);
            let by = signed_gap(filter.matches(), from, choice);
            filter.step(by);
        }
        self.settle(choices, rect);
        Outcome::Moved
    }

    /// The place of each part of the box drawn in `rect`.
    pub fn places<C>(&self, choices: &dyn Choices<C>, rect: Rect) -> Places {
        let inner = Rect::new(
            rect.x.saturating_add(1),
            rect.y.saturating_add(1),
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(2),
        );
        let first = self.settled(choices, rect);
        let rows = (0..inner.height)
            .filter_map(|r| {
                let choice = self.choice_at(first.saturating_add(usize::from(r)), choices)?;
                Some((
                    Rect::new(inner.x, inner.y.saturating_add(r), inner.width, 1),
                    choice,
                ))
            })
            .collect();
        let search = self
            .offers_search(choices, rect)
            .then(|| draw::search_place(rect))
            .flatten();
        Places { rect, rows, search }
    }

    /// The choice that the filter's `i`th match is, or the `i`th choice
    /// with no filter.
    fn choice_at<C>(&self, i: usize, choices: &dyn Choices<C>) -> Option<usize> {
        match &self.filter {
            Some(f) => f.matches().get(i).copied(),
            None => (i < choices.count()).then_some(i),
        }
    }

    /// How many choices the box can show: those the filter keeps, or all.
    fn showable<C>(&self, choices: &dyn Choices<C>) -> usize {
        self.filter
            .as_ref()
            .map_or(choices.count(), |f| f.matches().len())
    }

    /// Where the highlight is among the choices the box can show.
    fn highlight_at(&self) -> Option<usize> {
        match &self.filter {
            Some(f) => f.matches().binary_search(&self.highlight).ok(),
            None => Some(self.highlight),
        }
    }

    /// The offset that keeps the highlight in sight in `rect`, from the
    /// offset the state holds, without changing the state.
    fn settled<C>(&self, choices: &dyn Choices<C>, rect: Rect) -> usize {
        let rows = shown_rows(rect).max(1);
        let len = self.showable(choices);
        let mut offset = self.offset;
        if let Some(at) = self.highlight_at() {
            if at < offset {
                offset = at;
            } else if at >= offset.saturating_add(rows) {
                offset = at.saturating_add(1).saturating_sub(rows);
            }
        }
        offset.min(len.saturating_sub(rows))
    }

    /// Takes the settled offset as the state's own.
    fn settle<C>(&mut self, choices: &dyn Choices<C>, rect: Rect) {
        self.offset = self.settled(choices, rect);
    }

    /// Moves the highlight `by` among the choices the box can show, and
    /// stops at the ends.
    fn step<C>(&mut self, by: isize, choices: &dyn Choices<C>, rect: Rect) -> Outcome<C> {
        match self.filter.as_mut() {
            Some(filter) => {
                if filter.step(by) == search::Outcome::Ignored {
                    return Outcome::Ignored;
                }
                if let Some(h) = filter.highlighted() {
                    self.highlight = h;
                }
            }
            None => {
                let Some(last) = choices.count().checked_sub(1) else {
                    return Outcome::Ignored;
                };
                self.highlight = self.highlight.saturating_add_signed(by).min(last);
            }
        }
        self.settle(choices, rect);
        Outcome::Moved
    }

    /// The highlight follows the filter's, after a key the search took.
    fn follow_filter<C>(&mut self, choices: &dyn Choices<C>, rect: Rect) {
        if let Some(h) = self.filter.as_ref().and_then(Search::highlighted) {
            self.highlight = h;
        }
        self.settle(choices, rect);
    }

    /// Enter, or a click on the highlighted choice: the change the tab
    /// named for it, where the box shows it.
    fn kept<C>(&self, choices: &dyn Choices<C>) -> Outcome<C> {
        let shown = self.highlight < choices.count() && self.highlight_at().is_some();
        if shown {
            Outcome::Kept(choices.keep(self.highlight))
        } else {
            Outcome::Ignored
        }
    }

    /// `?`: opens the search where the box offers it, or puts the cursor
    /// back in a kept filter's entry.
    fn search<C>(&mut self, choices: &dyn Choices<C>, rect: Rect) -> Outcome<C> {
        if let Some(filter) = self.filter.as_mut() {
            filter.key(&Key::char('?'));
            return Outcome::Searched;
        }
        if !self.offers_search(choices, rect) {
            return Outcome::Ignored;
        }
        let list = ChoiceList {
            node: self.node,
            choices,
        };
        self.filter = Some(Search::open(&list, Some(self.highlight)));
        Outcome::Searched
    }
}

/// The rows a box drawn in `rect` shows choices on, inside its border.
fn shown_rows(rect: Rect) -> usize {
    usize::from(rect.height.saturating_sub(2))
}

/// How far `to` is from `from` among `matches`, as a signed count.
fn signed_gap(matches: &[usize], from: usize, to: usize) -> isize {
    let at = |c: usize| matches.binary_search(&c).unwrap_or(0);
    let (a, b) = (at(from), at(to));
    let gap = isize::try_from(a.abs_diff(b)).unwrap_or(0);
    if b >= a { gap } else { -gap }
}

/// A line's choices as a list the search matches on each choice's text.
struct ChoiceList<'a, C> {
    node: &'static str,
    choices: &'a dyn Choices<C>,
}

impl<C> Searchable for ChoiceList<'_, C> {
    fn search_node(&self) -> &'static str {
        self.node
    }

    fn count(&self) -> usize {
        self.choices.count()
    }

    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        (field == 0 && item < self.choices.count()).then(|| self.choices.text(item))
    }
}
