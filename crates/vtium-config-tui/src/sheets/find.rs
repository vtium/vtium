//! Find, on Ctrl+F: the search component's row with the prompt `Find`,
//! over every tab's items, with the matches in the rows above it. Each
//! tab answers for its own items through `Tab::find_items`, once, when
//! Find opens, so each key typed filters what was read then and reads no
//! tab again. The matches follow the tabs' order. Enter, or a click on a
//! match, closes Find and the levels open over the item's tab, opens that
//! tab, and hands it the match's item, so the tab highlights it.
//!
//! The rows above the search row are a rule, the matches, and a last row
//! that counts what was found or, with nothing typed, says what Find
//! searches.
//!
//! References:
//! - ADR-1403 D11, "Find, on Ctrl+F": what Find searches, its matches
//!   above the row, Enter going to the item, and where Ctrl+F acts.
//! - ADR-1206 D3: Find uses the one search component and its matcher,
//!   and the navigation module asks each tab for the items Find searches.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::ui::Action;
use vtium_config_widgets::keys::Key;
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::{self, Outcome, Search};
use vtium_config_widgets::tab::FindItem;

use super::{Act, bar_row, put, window};

/// The row Enter on a match hands the item's tab, carrying the item.
pub const GO: &str = "enter.typing.find";

/// Find's state: every tab's items, the tab each came from, and the
/// search over them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Find {
    items: Vec<FindItem>,
    /// For each item, the index of its tab.
    tabs: Vec<usize>,
    /// The names of the tabs that gave items, in the tabs' order.
    names: Vec<&'static str>,
    search: Search,
}

impl Find {
    /// Find over `found`: each tab's index, name and items, in the tabs'
    /// order.
    pub fn new(found: Vec<(usize, &'static str, Vec<FindItem>)>) -> Find {
        let mut items = Vec::new();
        let mut tabs = Vec::new();
        let mut names = Vec::new();
        for (tab, name, its) in found {
            if !its.is_empty() {
                names.push(name);
            }
            tabs.extend(std::iter::repeat_n(tab, its.len()));
            items.extend(its);
        }
        let search = Search::find(&items);
        Find {
            items,
            tabs,
            names,
            search,
        }
    }

    pub fn search(&self) -> &Search {
        &self.search
    }

    /// The highlighted match, with its tab.
    pub fn highlighted(&self) -> Option<(usize, &FindItem)> {
        let i = self.search.highlighted()?;
        Some((*self.tabs.get(i)?, self.items.get(i)?))
    }

    /// Acts on `key`.
    pub fn key(&mut self, key: &Key) -> Act {
        // with nothing typed no match shows, so Enter has nothing to go to
        if *key == Key::plain(vtium_config_widgets::keys::Code::Enter)
            && self.search.entry().is_empty()
        {
            return Act::Stay;
        }
        match self.search.key(key) {
            Outcome::Closed => Act::Close,
            Outcome::Kept => self.go(),
            Outcome::Typed | Outcome::Moved | Outcome::Resumed => Act::Stay,
            Outcome::Ignored => Act::Pass,
        }
    }

    /// A click on the line `line` of Find's rows: a match goes to it.
    pub fn click(&mut self, line: usize) -> Act {
        let Some(at) = line.checked_sub(1).filter(|&l| l < MATCH_ROWS) else {
            return Act::Stay;
        };
        let matches = self.search.matches().len();
        let top = window(matches, MATCH_ROWS, self.at());
        let want = top.saturating_add(at);
        if want >= matches {
            return Act::Stay;
        }
        let from = self.at().unwrap_or(0);
        let by = isize::try_from(want).unwrap_or(0) - isize::try_from(from).unwrap_or(0);
        self.search.step(by);
        self.go()
    }

    fn go(&self) -> Act {
        match self.highlighted() {
            Some((tab, item)) => Act::Go {
                tab,
                action: Some(Action::item(GO, item.index)),
            },
            None => Act::Stay,
        }
    }

    /// Where the highlight is among the matches.
    fn at(&self) -> Option<usize> {
        let h = self.search.highlighted()?;
        self.search.matches().iter().position(|&m| m == h)
    }

    /// The last row above the search row: how many were found and among
    /// what, or, with nothing typed, what Find searches.
    pub fn count_line(&self) -> String {
        let among = among(&self.names);
        if self.search.entry().is_empty() {
            return format!("Find searches the names in {among}.");
        }
        match self.search.matches().len() {
            0 => format!("Nothing found in {among}."),
            n => format!("{n} found in {among}"),
        }
    }

    /// Draws Find's rows into `area`: the rule, the matches, the count and
    /// the search row last.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) {
        if area.height == 0 {
            return;
        }
        let width = usize::from(area.width);
        put(buf, area, 0, 0, &"─".repeat(width), Role::Row, color);
        let rows = usize::from(area.height.saturating_sub(3)).min(MATCH_ROWS);
        let matches = self.search.matches();
        let shown = !self.search.entry().is_empty();
        let top = window(matches.len(), rows, self.at());
        let highlight = self.search.highlighted();
        let place_x = u16::try_from(NAME_COLUMNS + 2).unwrap_or(0);
        let fact_x = place_x.saturating_add(u16::try_from(PLACE_COLUMNS + 1).unwrap_or(0));
        for (y, &i) in matches.iter().skip(top).take(rows).enumerate() {
            let (Some(item), true) = (self.items.get(i), shown) else {
                break;
            };
            let y = u16::try_from(y + 1).unwrap_or(u16::MAX);
            let lit = highlight == Some(i);
            if lit {
                bar_row(buf, area, y, Role::RowHighlight, color);
            }
            let role = if lit { Role::RowHighlight } else { Role::Row };
            put(
                buf,
                area,
                1,
                y,
                &super::cut(&item.name, NAME_COLUMNS),
                role,
                color,
            );
            put(
                buf,
                area,
                place_x,
                y,
                &super::cut(&item.place, PLACE_COLUMNS),
                role,
                color,
            );
            let rest = width.saturating_sub(usize::from(fact_x));
            put(
                buf,
                area,
                fact_x,
                y,
                &super::cut(&item.fact, rest),
                role,
                color,
            );
        }
        let count_y = area.height.saturating_sub(2);
        put(buf, area, 1, count_y, &self.count_line(), Role::Dim, color);
        let row = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
        search::row::draw(&self.search, buf, row, color);
    }
}

/// The match rows above the count, which a click counts from.
const MATCH_ROWS: usize = 6;

/// The columns of a match's name and of its place.
const NAME_COLUMNS: usize = 22;
const PLACE_COLUMNS: usize = 32;

/// The tabs' names as a list in words: `Fonts`, `Fonts and Looks`, or
/// `Fonts, Looks and Flow`.
fn among(names: &[&str]) -> String {
    match names {
        [] => "no tab".to_owned(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tabs_are_named_as_a_list_in_words() {
        assert_eq!(among(&[]), "no tab");
        assert_eq!(among(&["Fonts"]), "Fonts");
        assert_eq!(among(&["Fonts", "Looks", "Flow"]), "Fonts, Looks and Flow");
    }
}
