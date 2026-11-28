//! The opt-in trait for the `?` search over a tab's list. A tab hands the
//! search its items and the text of each field the search matches, and the
//! one search component and its matcher do the rest, so no tab draws a
//! search row or reads a pattern itself.
//!
//! References:
//! - ADR-1206 D3, the search component: one component and one matcher,
//!   which each list hands its items and the fields to match.
//! - ADR-1206 D9: `Searchable` and its methods with defaults.
//! - ADR-1403 D11, "Search, on ?": each screen says what its items are
//!   matched on, and the help names those fields.

use std::borrow::Cow;

use crate::contract;

/// A list that the `?` search filters.
pub trait Searchable {
    /// The node of the interface tree the list is on, whose `search`
    /// column names the fields the search matches.
    fn search_node(&self) -> &'static str;

    /// How many items the list holds.
    fn count(&self) -> usize;

    /// The text of the field numbered `field`, in the order of
    /// [`Searchable::fields`], of the item numbered `item`. `None` where
    /// the item has nothing in that field, or no such item exists.
    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>>;

    /// The names of the fields the search matches, such as `family` and
    /// `file` for a font, from the node's `search` column.
    fn fields(&self) -> &'static [&'static str] {
        contract::search(self.search_node())
    }

    /// The help's words for what the search matches, as in
    /// `family, file`.
    fn help(&self) -> String {
        self.fields().join(", ")
    }

    /// The matcher's input: every item's fields in lowercase, built once
    /// when the search row opens, so that each key typed matches against
    /// it without reading the tab again.
    fn haystack(&self) -> Haystack {
        let fields = self.fields().len();
        Haystack {
            items: (0..self.count())
                .map(|item| {
                    (0..fields)
                        .map(|field| {
                            self.text(item, field)
                                .map(|t| t.to_lowercase())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .collect(),
        }
    }
}

/// What the search's matcher reads: each item's fields, in lowercase, so
/// the match ignores case. An item's index here is its index in the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Haystack {
    /// For each item of the list, the text of each field in order.
    pub items: Vec<Vec<String>>,
}
