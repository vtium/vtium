//! Update's three-way merge. The draft and the installed file both started
//! from the draft's base, and each changed it since. [`merge`] takes the
//! base, the person's text and the installed file's text, and keeps every
//! change of either side. Where only one side changed some lines, the
//! merge takes that side's lines. Where both changed the same lines in the
//! same way, it takes them once. Where both changed the same lines in
//! different ways, the merge holds both and waits for the person to settle
//! them, one at a time, with their own lines or the installed file's.
//!
//! Two changes touch the same lines when the base lines they replace
//! overlap, or when both add lines at one place, where the merge cannot
//! tell which goes first. Changes to lines next to each other do not
//! touch.
//!
//! The merge works on whole lines with their line endings, so a text that
//! neither side changed comes out byte for byte as it went in.
//!
//! References:
//! - ADR-1403 D8, "When the installed file changed on disk": Update merges three ways from the base, the draft and the installed file, and where both changed the same lines the person keeps theirs or the installed file's, one at a time.

use super::diff::{Region, regions};
use super::words;
use crate::document::{Base, Change, Edit};

/// Which side's lines settle a place both sides changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The person's lines: `[m]`.
    Mine,
    /// The installed file's lines: `[i]`.
    Installed,
}

/// A place where both sides changed the same lines in different ways.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Both {
    /// The base's lines there, which both sides replaced.
    pub base: String,
    /// The person's lines there.
    pub mine: String,
    /// The installed file's lines there.
    pub installed: String,
    /// The number, from 1, of the first of these lines in the person's
    /// text, or of the line they stand before when the person removed
    /// them.
    pub mine_line: usize,
    /// The same in the installed file.
    pub installed_line: usize,
    /// The side the person chose, `None` until they choose.
    pub settled: Option<Side>,
}

impl Both {
    /// The lines the merge takes here once settled.
    fn kept(&self) -> Option<&str> {
        match self.settled? {
            Side::Mine => Some(&self.mine),
            Side::Installed => Some(&self.installed),
        }
    }
}

/// One stretch of the merged text.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    /// Lines the merge takes as they are.
    Kept(String),
    /// Lines both sides changed in different ways.
    Both(Both),
}

/// The merge of the installed file's change into the person's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    pieces: Vec<Piece>,
}

impl Merge {
    /// Each place both sides changed, in the order of the text.
    pub fn both(&self) -> impl Iterator<Item = &Both> {
        self.pieces.iter().filter_map(|p| match p {
            Piece::Both(b) => Some(b),
            Piece::Kept(_) => None,
        })
    }

    /// Whether the two changes touch the same lines anywhere.
    pub fn touches(&self) -> bool {
        self.both().next().is_some()
    }

    /// How many places both sides changed, and how many of them are
    /// settled.
    pub fn settled(&self) -> (usize, usize) {
        self.both().fold((0, 0), |(n, s), b| {
            (
                n.saturating_add(1),
                s.saturating_add(usize::from(b.settled.is_some())),
            )
        })
    }

    /// The first place both sides changed that is not settled yet, by its
    /// index among [`Merge::both`]: the one the review shows next.
    pub fn next(&self) -> Option<usize> {
        self.both().position(|b| b.settled.is_none())
    }

    /// Settles place `n` of [`Merge::both`] with `side`'s lines, or changes
    /// how it is settled. Returns whether there is such a place.
    pub fn settle(&mut self, n: usize, side: Side) -> bool {
        let place = self
            .pieces
            .iter_mut()
            .filter_map(|p| match p {
                Piece::Both(b) => Some(b),
                Piece::Kept(_) => None,
            })
            .nth(n);
        match place {
            Some(b) => {
                b.settled = Some(side);
                true
            }
            None => false,
        }
    }

    /// The merged text, once every place both sides changed is settled.
    pub fn text(&self) -> Option<String> {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Kept(t) => out.push_str(t),
                Piece::Both(b) => out.push_str(b.kept()?),
            }
        }
        Some(out)
    }
}

/// Merges `installed`'s change since `base` into `mine`, three ways.
pub fn merge(base: &str, mine: &str, installed: &str) -> Merge {
    let b: Vec<&str> = base.split_inclusive('\n').collect();
    let m: Vec<&str> = mine.split_inclusive('\n').collect();
    let o: Vec<&str> = installed.split_inclusive('\n').collect();
    let mut all: Vec<(Side, Region)> = regions(&b, &m)
        .into_iter()
        .map(|r| (Side::Mine, r))
        .chain(regions(&b, &o).into_iter().map(|r| (Side::Installed, r)))
        .collect();
    all.sort_by_key(|(_, r)| (r.old.start, r.old.end));
    let lines = |of: &[&str], range: std::ops::Range<usize>| -> String {
        of.get(range).unwrap_or_default().concat()
    };
    let mut pieces = Vec::new();
    // where the base, the person's text and the installed file stand
    let (mut at, mut at_mine, mut at_installed) = (0usize, 0usize, 0usize);
    let mut i = 0;
    while let Some((_, first)) = all.get(i) {
        let mut span = first.old.clone();
        let mut j = i.saturating_add(1);
        while let Some((_, r)) = all.get(j) {
            let inside = r.old.start < span.end;
            let one_place = r.old.is_empty() && span.is_empty() && r.old.start == span.start;
            if !inside && !one_place {
                break;
            }
            span.end = span.end.max(r.old.end);
            j = j.saturating_add(1);
        }
        let group = all.get(i..j).unwrap_or_default();
        pieces.push(Piece::Kept(lines(&b, at..span.start)));
        let start_mine = at_mine.saturating_add(span.start.saturating_sub(at));
        let start_installed = at_installed.saturating_add(span.start.saturating_sub(at));
        let side = |which: Side, of: &[&str]| -> (String, usize) {
            let mut text = String::new();
            let (mut pos, mut len) = (span.start, 0usize);
            for (_, r) in group.iter().filter(|(s, _)| *s == which) {
                text.push_str(&lines(&b, pos..r.old.start));
                text.push_str(&lines(of, r.new.clone()));
                len = len
                    .saturating_add(r.old.start.saturating_sub(pos))
                    .saturating_add(r.new.len());
                pos = r.old.end;
            }
            text.push_str(&lines(&b, pos..span.end));
            len = len.saturating_add(span.end.saturating_sub(pos));
            (text, len)
        };
        let (mine_text, mine_len) = side(Side::Mine, &m);
        let (installed_text, installed_len) = side(Side::Installed, &o);
        let changed = |which: Side| group.iter().any(|(s, _)| *s == which);
        pieces.push(match (changed(Side::Mine), changed(Side::Installed)) {
            (true, true) if mine_text != installed_text => Piece::Both(Both {
                base: lines(&b, span.clone()),
                mine: mine_text,
                installed: installed_text,
                mine_line: start_mine.saturating_add(1),
                installed_line: start_installed.saturating_add(1),
                settled: None,
            }),
            (_, true) => Piece::Kept(installed_text),
            _ => Piece::Kept(mine_text),
        });
        at = span.end;
        at_mine = start_mine.saturating_add(mine_len);
        at_installed = start_installed.saturating_add(installed_len);
        i = j;
    }
    pieces.push(Piece::Kept(lines(&b, at..b.len())));
    pieces.retain(|p| !matches!(p, Piece::Kept(t) if t.is_empty()));
    Merge { pieces }
}

/// Update, once every place both sides changed is settled: the merged
/// text in place of the document's, and the installed file the review
/// showed as the base, in one step. `None` while a place is not settled.
pub fn update(merged: &Merge, installed: &str) -> Option<Change> {
    Some(Change::new(
        words::UPDATE,
        Edit::All(vec![
            Edit::Text(merged.text()?),
            Edit::Base(Base::of(Some(installed))),
        ]),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_next_to_each_other_do_not_touch() {
        let merged = merge("a\nb\nc\n", "A\nb\nc\n", "a\nB\nc\n");
        assert!(!merged.touches());
        assert_eq!(merged.text().as_deref(), Some("A\nB\nc\n"));
    }

    #[test]
    fn two_additions_at_one_place_touch() {
        let merged = merge("a\nc\n", "a\nmine\nc\n", "a\ntheirs\nc\n");
        assert_eq!(merged.settled(), (1, 0));
        let both = merged.both().next().cloned();
        assert_eq!(
            both.map(|b| (b.mine, b.installed, b.mine_line, b.installed_line)),
            Some(("mine\n".to_owned(), "theirs\n".to_owned(), 2, 2))
        );
    }

    #[test]
    fn the_same_change_on_both_sides_is_taken_once() {
        let merged = merge("a\nb\n", "a\nB\n", "a\nB\n");
        assert!(!merged.touches());
        assert_eq!(merged.text().as_deref(), Some("a\nB\n"));
    }

    #[test]
    fn a_text_without_a_last_line_ending_keeps_it_that_way() {
        let merged = merge("a\nb", "a\nb\nc", "z\na\nb");
        assert_eq!(merged.text().as_deref(), Some("z\na\nb\nc"));
    }
}
