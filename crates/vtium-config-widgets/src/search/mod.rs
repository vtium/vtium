//! The search component, which every list's `?` search and Ctrl+F's Find
//! use. It owns the row stacked on the action bar, the entry, the syntax
//! help and the row's keys, and [`matcher`] turns the entry into a
//! pattern. A list hands it anything [`Searchable`], and the component
//! reads every item's fields once, when the row opens, into a haystack.
//! Each key typed then filters that haystack, so nothing is read again
//! from the tab, and a list of ten thousand fonts filters as the person
//! types.
//!
//! The component keeps the indexes that match, so a filter kept with
//! Enter survives every redraw, and the highlighted item among them. A
//! [`Search`] is a value that the state holding it keeps, as the
//! navigation state keeps a menu's, and [`row::draw`] draws it.
//!
//! The row's keys: a character is typed into the entry, `?` and `q`
//! included; Backspace deletes the last character, and on an empty entry
//! closes the row; Enter keeps the filter and goes back to the list; Esc
//! closes the row and clears the filter; Up and Down move the list's
//! highlight while the cursor stays in the entry, and in Find PgUp and
//! PgDn move it by a page. With the filter kept, `?` puts the cursor back
//! in the entry, Esc clears the filter, and every other key is the list's.
//!
//! References:
//! - ADR-1206 D3, "One search component": the component owns the row,
//!   the entry, the help and the keys, takes anything `Searchable`, and
//!   keeps the indexes that match.
//! - ADR-1206 D10: filtering as the person types on a small board.
//! - ADR-1403 D11, "Search, on ?" and "Find, on Ctrl+F": the keys of the
//!   row and of Find, and Find's prompt.

pub mod matcher;
pub mod row;

use std::borrow::Cow;

use crate::keys::{Code, Key};
use crate::tab::{FindItem, Haystack, Searchable};
use matcher::Pattern;

/// The prompt before the entry, which names what the row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompt {
    /// `?`: the row filters the list on screen.
    Search,
    /// `Find`: the row finds items across the tabs.
    Find,
}

impl Prompt {
    /// The prompt as the row writes it.
    pub fn text(self) -> &'static str {
        match self {
            Prompt::Search => "?",
            Prompt::Find => "Find",
        }
    }
}

/// What a key did to the search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The entry changed, and the matches with it.
    Typed,
    /// The highlight moved among the matches.
    Moved,
    /// Enter kept the filter: the cursor leaves the entry, and the list's
    /// keys act on what matches.
    Kept,
    /// `?` put the cursor back in the entry of a kept filter.
    Resumed,
    /// The row closes and the filter is cleared: the holder drops the
    /// search.
    Closed,
    /// The key is not the row's.
    Ignored,
}

/// The rows Find's matches take above the row, and so the step of PgUp
/// and PgDn there.
pub const FIND_ROWS: usize = 8;

/// The state of one search row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Search {
    prompt: Prompt,
    entry: String,
    typing: bool,
    pattern: Pattern,
    haystack: Haystack,
    matches: Vec<usize>,
    /// Where the highlight is among `matches`.
    at: Option<usize>,
    help: String,
}

impl Search {
    /// Opens the `?` row over `list`, with the cursor in an empty entry,
    /// every item matching, and the highlight on `highlighted`, the item
    /// the list highlights now.
    pub fn open(list: &dyn Searchable, highlighted: Option<usize>) -> Search {
        Search::over(Prompt::Search, list.haystack(), list.help(), highlighted)
    }

    /// Opens Find over `items`, the items every tab answers for, matched
    /// on each one's name and description.
    pub fn find(items: &[FindItem]) -> Search {
        let list = FindList(items);
        Search::over(Prompt::Find, list.haystack(), String::new(), None)
    }

    fn over(
        prompt: Prompt,
        haystack: Haystack,
        help: String,
        highlighted: Option<usize>,
    ) -> Search {
        let matches: Vec<usize> = (0..haystack.items.len()).collect();
        let at = highlighted.filter(|&h| h < matches.len());
        Search {
            prompt,
            entry: String::new(),
            typing: true,
            pattern: Pattern::default(),
            haystack,
            matches,
            at,
            help,
        }
    }

    /// The prompt the row writes before the entry.
    pub fn prompt(&self) -> Prompt {
        self.prompt
    }

    /// What is typed.
    pub fn entry(&self) -> &str {
        &self.entry
    }

    /// Whether the cursor is in the entry, rather than the filter kept.
    pub fn typing(&self) -> bool {
        self.typing
    }

    /// The help's words for the fields the search matches, such as
    /// `family, file`, or empty where the row names none, as in Find.
    pub fn help(&self) -> &str {
        &self.help
    }

    /// The indexes of the items that match, in the list's order.
    pub fn matches(&self) -> &[usize] {
        &self.matches
    }

    /// How many items the list holds, matching or not.
    pub fn count(&self) -> usize {
        self.haystack.items.len()
    }

    /// The highlighted item's index in the list, `None` where nothing
    /// matches.
    pub fn highlighted(&self) -> Option<usize> {
        self.at.and_then(|at| self.matches.get(at).copied())
    }

    /// Whether the item numbered `item` matches.
    pub fn is_match(&self, item: usize) -> bool {
        self.matches.binary_search(&item).is_ok()
    }

    /// Acts on `key`, as the row's keys say.
    pub fn key(&mut self, key: &Key) -> Outcome {
        if !self.typing {
            return match key.code {
                Code::Char('?') if key.typed().is_some() => {
                    self.typing = true;
                    Outcome::Resumed
                }
                Code::Escape if key.mods.is_empty() => Outcome::Closed,
                _ => Outcome::Ignored,
            };
        }
        if let Some(c) = key.typed() {
            self.entry.push(c);
            self.refilter();
            return Outcome::Typed;
        }
        if !key.mods.is_empty() {
            return Outcome::Ignored;
        }
        match key.code {
            Code::Backspace if self.entry.pop().is_some() => {
                self.refilter();
                Outcome::Typed
            }
            Code::Backspace | Code::Escape => Outcome::Closed,
            Code::Enter if self.prompt == Prompt::Find && self.highlighted().is_none() => {
                Outcome::Ignored
            }
            Code::Enter => {
                self.typing = false;
                Outcome::Kept
            }
            Code::Up => self.step(-1),
            Code::Down => self.step(1),
            Code::PageUp if self.prompt == Prompt::Find => self.step(-page()),
            Code::PageDown if self.prompt == Prompt::Find => self.step(page()),
            _ => Outcome::Ignored,
        }
    }

    /// Moves the highlight `by` matches, down for a positive `by`, and
    /// stops at the first and the last. A list whose filter is kept moves
    /// its highlight with this.
    pub fn step(&mut self, by: isize) -> Outcome {
        let Some(last) = self.matches.len().checked_sub(1) else {
            return Outcome::Ignored;
        };
        let at = self.at.unwrap_or(0);
        self.at = Some(at.saturating_add_signed(by).min(last));
        Outcome::Moved
    }

    /// Matches the haystack against the entry as it is now. A pattern
    /// that narrows the last one reads only the last matches. The
    /// highlighted item keeps the highlight while it matches, and
    /// otherwise the first match takes it.
    fn refilter(&mut self) {
        let was = self.highlighted();
        let next = matcher::pattern(&self.entry);
        self.matches = if next.narrows(&self.pattern) {
            next.refine(&self.haystack, &self.matches)
        } else {
            next.filter(&self.haystack)
        };
        self.pattern = next;
        self.at = match was.and_then(|w| self.matches.binary_search(&w).ok()) {
            Some(at) => Some(at),
            None => (!self.matches.is_empty()).then_some(0),
        };
    }
}

/// Find's step for PgUp and PgDn, as a signed count.
fn page() -> isize {
    isize::try_from(FIND_ROWS).unwrap_or(1)
}

/// Find's items as a list the search matches on each item's name and
/// description.
struct FindList<'a>(&'a [FindItem]);

impl Searchable for FindList<'_> {
    fn search_node(&self) -> &'static str {
        "any.find"
    }

    fn count(&self) -> usize {
        self.0.len()
    }

    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        let item = self.0.get(item)?;
        match field {
            0 => Some(Cow::Borrowed(item.name.as_str())),
            1 => item.description.as_deref().map(Cow::Borrowed),
            _ => None,
        }
    }

    fn fields(&self) -> &'static [&'static str] {
        &["name", "description"]
    }
}
