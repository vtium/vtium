//! The search's pure matcher. [`pattern`] turns the entry's text into a
//! [`Pattern`]: the text splits into words at spaces, and each word's
//! first character may be a mark. `^` matches a field that starts with the
//! rest of the word, `$` one that ends with it, and `*`, or no mark,
//! one that holds it anywhere. A mark elsewhere in a word is a character
//! like any other. The match ignores case: the pattern is lowercased, and
//! the fields come lowercased in the [`Haystack`].
//!
//! An item matches when one of its fields matches every word. So
//! `^dejavu mono` finds DejaVu Sans Mono by its family, and `^dejavu $ttf`
//! finds nothing, because no one field both starts with "dejavu" and ends
//! with "ttf". A pattern with no word, or with marks alone, matches every
//! item.
//!
//! As a word grows, the items it matches can only shrink, except at an
//! end: "xvga" ends with "vga" but not with "vg". [`Pattern::narrows`]
//! says when a later pattern keeps only items the earlier one kept, and
//! [`Pattern::refine`] then reads only the earlier matches, so a key
//! typed reads no more of the list than still matches.
//!
//! References:
//! - ADR-1206 D3, "One search component": one pure function turns the
//!   entry into a matcher, which reads each word's mark and ignores case.
//! - ADR-1403 D11, "Search, on ?", "The patterns": the three marks, the
//!   words that each have to match, and an item that matches when one of
//!   its fields matches every word.

use crate::tab::Haystack;

/// Where a word must sit in a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// `^`: the field starts with the word.
    Start,
    /// `$`: the field ends with the word.
    End,
    /// `*` or no mark: the field holds the word anywhere.
    Anywhere,
}

/// One word of the pattern, in lowercase, without its mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub mark: Mark,
    pub text: String,
}

impl Word {
    /// Whether `field`, in lowercase, matches the word.
    fn matches(&self, field: &str) -> bool {
        match self.mark {
            Mark::Start => field.starts_with(&self.text),
            Mark::End => field.ends_with(&self.text),
            Mark::Anywhere => field.contains(&self.text),
        }
    }

    /// Whether every field that matches `self` also matches `earlier`.
    fn implies(&self, earlier: &Word) -> bool {
        self.mark == earlier.mark
            && match self.mark {
                Mark::Start => self.text.starts_with(&earlier.text),
                Mark::End => self.text.ends_with(&earlier.text),
                Mark::Anywhere => self.text.contains(&earlier.text),
            }
    }
}

/// The entry's text read as words, each with its mark.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pattern {
    words: Vec<Word>,
}

/// The pattern that `entry` writes.
pub fn pattern(entry: &str) -> Pattern {
    let words = entry
        .to_lowercase()
        .split(' ')
        .filter_map(|w| {
            let (mark, text) = match w.chars().next()? {
                '^' => (Mark::Start, w.get(1..)?),
                '$' => (Mark::End, w.get(1..)?),
                '*' => (Mark::Anywhere, w.get(1..)?),
                _ => (Mark::Anywhere, w),
            };
            (!text.is_empty()).then(|| Word {
                mark,
                text: text.to_owned(),
            })
        })
        .collect();
    Pattern { words }
}

impl Pattern {
    /// The pattern's words, a bare mark left out.
    pub fn words(&self) -> &[Word] {
        &self.words
    }

    /// Whether the pattern has no word, so it matches every item.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Whether an item with `fields`, each in lowercase, matches: one of
    /// its fields matches every word. Every item matches an empty pattern.
    pub fn matches(&self, fields: &[String]) -> bool {
        self.is_empty()
            || fields
                .iter()
                .any(|field| self.words.iter().all(|w| w.matches(field)))
    }

    /// The indexes of the items of `haystack` that match, in order.
    pub fn filter(&self, haystack: &Haystack) -> Vec<usize> {
        haystack
            .items
            .iter()
            .enumerate()
            .filter(|(_, fields)| self.matches(fields))
            .map(|(i, _)| i)
            .collect()
    }

    /// Whether every item that matches `self` also matches `earlier`:
    /// each earlier word has a word here with the same mark that holds it
    /// where the mark says, so the field that matches here matched there.
    pub fn narrows(&self, earlier: &Pattern) -> bool {
        earlier
            .words
            .iter()
            .all(|e| self.words.iter().any(|w| w.implies(e)))
    }

    /// The indexes among `earlier`, an earlier pattern's matches, that
    /// match `self`. Only right where [`Pattern::narrows`] holds.
    pub fn refine(&self, haystack: &Haystack, earlier: &[usize]) -> Vec<usize> {
        earlier
            .iter()
            .copied()
            .filter(|&i| haystack.items.get(i).is_some_and(|f| self.matches(f)))
            .collect()
    }
}
