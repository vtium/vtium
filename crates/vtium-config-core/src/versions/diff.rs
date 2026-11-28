//! The difference between two texts of vtium.conf, as Show changes and
//! History's View changes show it: hunks, each in one part of the file,
//! with the line numbers of its lines and a little context, and inside a
//! changed line the words that changed. [`diff`] gives the changes only,
//! and [`whole_file`] the whole file with each changed line marked.
//!
//! A hunk keeps to one part of the file: the top level, which the screen
//! names `settings`, or one section, which it names by its header, such as
//! `[look My-Look]`. Its context is up to [`CONTEXT`] lines on each side
//! from the same part, and a blank or comment line at either end of the
//! context is left out, since it says nothing about where the change is.
//!
//! The lines of a change are compared by the diff crate `imara-diff`,
//! which [`regions`] wraps, so the rest of vtium-config names no diff
//! crate.
//!
//! References:
//! - ADR-1403 D11, "Show changes and Edit as text": hunks, each with its line numbers and a little context, and the words that changed inside a line marked.
//! - ADR-1206 D10: the diff crate is the smaller of the two the record names that passes `make deny`.

use std::ops::Range;

/// How many lines of context a hunk shows on each side of a change.
pub const CONTEXT: usize = 2;

/// The name a hunk in the top level of vtium.conf shows, where the
/// settings every head inherits live.
pub const TOP_LEVEL: &str = "settings";

/// One place where two lists differ: the items `old` of the first stand
/// where the items `new` of the second are. Either range may be empty, for
/// an addition or a removal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

/// The places where `a` and `b` differ, in order, none touching the next.
/// This is the one call into the diff crate.
pub fn regions<T: Ord + std::hash::Hash>(a: &[T], b: &[T]) -> Vec<Region> {
    struct Items<'a, T>(&'a [T]);
    impl<'a, T: std::hash::Hash + Eq> imara_diff::TokenSource for Items<'a, T> {
        type Token = &'a T;
        type Tokenizer = std::slice::Iter<'a, T>;
        fn tokenize(&self) -> Self::Tokenizer {
            self.0.iter()
        }
        fn estimate_tokens(&self) -> u32 {
            u32::try_from(self.0.len()).unwrap_or(u32::MAX)
        }
    }
    let input = imara_diff::InternedInput::new(Items(a), Items(b));
    let diff = imara_diff::Diff::compute(imara_diff::Algorithm::Myers, &input);
    let mut out: Vec<Region> = Vec::new();
    for h in diff.hunks() {
        let r = |x: std::ops::Range<u32>| x.start as usize..x.end as usize;
        out.push(Region {
            old: r(h.before),
            new: r(h.after),
        });
    }
    out
}

/// What a line of a diff is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A line both texts hold: context.
    Same,
    /// A line the old text holds and the new one does not: red.
    Removed,
    /// A line the new text holds and the old one did not: green.
    Added,
}

/// One line of a diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub mark: Mark,
    /// The line's number in the old text, from 1, for a line it holds.
    pub old: Option<usize>,
    /// The line's number in the new text, from 1, for a line it holds.
    pub new: Option<usize>,
    /// The number the screen shows beside the line, in the new text's
    /// numbering: a removed line shows the number of the line that takes
    /// its place, or of the line it stood before.
    pub at: usize,
    /// The line, without its line ending.
    pub text: String,
    /// The byte ranges of `text` whose words changed, in a line changed in
    /// place; empty when the whole line is new, gone, or context.
    pub words: Vec<Range<usize>>,
}

/// The changed lines of one part of the file and their context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// The part of the file: [`TOP_LEVEL`] or a section's header line.
    pub section: String,
    pub lines: Vec<Line>,
}

/// The changes from one text to another, as hunks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    pub hunks: Vec<Hunk>,
}

impl Diff {
    /// Whether the two texts hold the same lines.
    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }
}

/// The changes from `old` to `new`, as Show changes' Changes only shows
/// them.
pub fn diff(old: &str, new: &str) -> Diff {
    let rows = whole(old, new);
    let mut shown = vec![false; rows.len()];
    for (k, row) in rows.iter().enumerate() {
        if row.line.mark == Mark::Same {
            continue;
        }
        if let Some(s) = shown.get_mut(k) {
            *s = true;
        }
        // the context before and after, in the same part of the file
        let same_part = |r: &&Row| r.line.mark == Mark::Same && r.section == row.section;
        for (j, _) in rows
            .iter()
            .enumerate()
            .take(k)
            .rev()
            .take_while(|(_, r)| same_part(r))
            .take(CONTEXT)
            .chain(
                rows.iter()
                    .enumerate()
                    .skip(k.saturating_add(1))
                    .take_while(|(_, r)| same_part(r))
                    .take(CONTEXT),
            )
        {
            if let Some(s) = shown.get_mut(j) {
                *s = true;
            }
        }
    }
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut last: Option<usize> = None;
    for (k, row) in rows.into_iter().enumerate() {
        if !shown.get(k).copied().unwrap_or(false) {
            continue;
        }
        let joins = last.is_some_and(|l| l.saturating_add(1) == k);
        match hunks.last_mut() {
            Some(hunk) if joins && hunk.section == row.section => hunk.lines.push(row.line),
            _ => hunks.push(Hunk {
                section: row.section,
                lines: vec![row.line],
            }),
        }
        last = Some(k);
    }
    for hunk in &mut hunks {
        trim(&mut hunk.lines);
    }
    hunks.retain(|h| h.lines.iter().any(|l| l.mark != Mark::Same));
    Diff { hunks }
}

/// Leaves out the blank and comment lines at either end of a hunk's
/// context.
fn trim(lines: &mut Vec<Line>) {
    let quiet = |l: &Line| {
        l.mark == Mark::Same && {
            let t = l.text.trim();
            t.is_empty() || t.starts_with('#')
        }
    };
    while lines.last().is_some_and(quiet) {
        lines.pop();
    }
    let lead = lines.iter().take_while(|l| quiet(l)).count();
    lines.drain(..lead);
}

/// One line of the whole file, with the part of the file it is in.
struct Row {
    line: Line,
    section: String,
}

/// The whole of `new`, with each line `old` held that `new` does not shown
/// where it stood, as Show changes' Whole file shows it.
pub fn whole_file(old: &str, new: &str) -> Vec<Line> {
    whole(old, new).into_iter().map(|r| r.line).collect()
}

/// Every line of both texts in order, each with its part of the file: a
/// removed line in the part the old text has it in, every other line in
/// the part the new text has it in.
fn whole(old: &str, new: &str) -> Vec<Row> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let (parts_a, parts_b) = (sections(&a), sections(&b));
    let part = |parts: &[String], i: usize| {
        parts
            .get(i)
            .cloned()
            .unwrap_or_else(|| TOP_LEVEL.to_owned())
    };
    let mut rows = Vec::with_capacity(b.len());
    let same = |rows: &mut Vec<Row>, i: usize, j: usize| {
        rows.push(Row {
            line: Line {
                mark: Mark::Same,
                old: Some(i.saturating_add(1)),
                new: Some(j.saturating_add(1)),
                at: j.saturating_add(1),
                text: b.get(j).copied().unwrap_or_default().to_owned(),
                words: Vec::new(),
            },
            section: part(&parts_b, j),
        });
    };
    let (mut i, mut j) = (0, 0);
    for region in regions(&a, &b) {
        while i < region.old.start && j < region.new.start {
            same(&mut rows, i, j);
            i = i.saturating_add(1);
            j = j.saturating_add(1);
        }
        let gone: Vec<&str> = a.get(region.old.clone()).unwrap_or_default().to_vec();
        let came: Vec<&str> = b.get(region.new.clone()).unwrap_or_default().to_vec();
        for (n, text) in gone.iter().enumerate() {
            let at = if n < came.len() {
                region.new.start.saturating_add(n).saturating_add(1)
            } else {
                region.new.end.saturating_add(1)
            };
            let words = came.get(n).map(|c| words(text, c).0).unwrap_or_default();
            rows.push(Row {
                line: Line {
                    mark: Mark::Removed,
                    old: Some(region.old.start.saturating_add(n).saturating_add(1)),
                    new: None,
                    at,
                    text: (*text).to_owned(),
                    words,
                },
                section: part(&parts_a, region.old.start.saturating_add(n)),
            });
        }
        for (n, text) in came.iter().enumerate() {
            let line = region.new.start.saturating_add(n);
            let words = gone.get(n).map(|g| words(g, text).1).unwrap_or_default();
            rows.push(Row {
                line: Line {
                    mark: Mark::Added,
                    old: None,
                    new: Some(line.saturating_add(1)),
                    at: line.saturating_add(1),
                    text: (*text).to_owned(),
                    words,
                },
                section: part(&parts_b, line),
            });
        }
        i = region.old.end;
        j = region.new.end;
    }
    while j < b.len() {
        same(&mut rows, i, j);
        i = i.saturating_add(1);
        j = j.saturating_add(1);
    }
    rows
}

/// The part of the file each line is in: [`TOP_LEVEL`] until the first
/// section's header, then each section's header line, which is in its
/// own section.
fn sections(lines: &[&str]) -> Vec<String> {
    let mut part = TOP_LEVEL.to_owned();
    lines
        .iter()
        .map(|l| {
            let t = l.trim();
            if t.starts_with('[') && t.ends_with(']') {
                t.clone_into(&mut part);
            }
            part.clone()
        })
        .collect()
}

/// The byte ranges of the words that changed from `old` to `new`, in each.
/// A word is a run of letters, digits and `_`, a run of spaces, or one
/// other character. Two lines that share no word changed whole, and mark
/// nothing.
pub fn words(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let (a, b) = (split_words(old), split_words(new));
    let pieces = |line: &str, spans: &[Range<usize>]| -> Vec<String> {
        spans
            .iter()
            .map(|r| line.get(r.clone()).unwrap_or_default().to_owned())
            .collect()
    };
    let (wa, wb) = (pieces(old, &a), pieces(new, &b));
    let changed = regions(&wa, &wb);
    let kept: usize = wa
        .len()
        .saturating_sub(changed.iter().map(|r| r.old.len()).sum());
    if kept == 0 || kept == wa.len() && kept == wb.len() {
        return (Vec::new(), Vec::new());
    }
    let span = |spans: &[Range<usize>], r: &Range<usize>| -> Option<Range<usize>> {
        let first = spans.get(r.start)?;
        let last = spans.get(r.end.checked_sub(1)?)?;
        Some(first.start..last.end)
    };
    (
        changed.iter().filter_map(|r| span(&a, &r.old)).collect(),
        changed.iter().filter_map(|r| span(&b, &r.new)).collect(),
    )
}

/// `line`'s words as byte ranges, in order, covering the whole line.
fn split_words(line: &str) -> Vec<Range<usize>> {
    #[derive(PartialEq)]
    enum Kind {
        Word,
        Space,
        Other,
    }
    let kind = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Kind::Word
        } else if c.is_whitespace() {
            Kind::Space
        } else {
            Kind::Other
        }
    };
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut last: Option<Kind> = None;
    for (at, c) in line.char_indices() {
        let k = kind(c);
        let end = at.saturating_add(c.len_utf8());
        match out.last_mut() {
            Some(r) if last.as_ref() == Some(&k) && k != Kind::Other => r.end = end,
            _ => out.push(at..end),
        }
        last = Some(k);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_is_a_run_of_letters_a_run_of_spaces_or_one_sign() {
        let line = "font-size = 10";
        let words: Vec<&str> = split_words(line)
            .into_iter()
            .map(|r| line.get(r).unwrap_or_default())
            .collect();
        assert_eq!(words, ["font", "-", "size", " ", "=", " ", "10"]);
        assert!(split_words("").is_empty());
    }

    #[test]
    fn lines_with_no_word_in_common_mark_nothing() {
        assert_eq!(words("abc", "xyz"), (Vec::new(), Vec::new()));
        assert_eq!(words("same", "same"), (Vec::new(), Vec::new()));
    }

    #[test]
    fn regions_join_a_removal_and_an_addition_at_one_place() {
        assert_eq!(
            regions(&["a", "b", "c"], &["a", "B", "c", "d"]),
            [
                Region {
                    old: 1..2,
                    new: 1..2
                },
                Region {
                    old: 3..3,
                    new: 3..4
                }
            ]
        );
    }
}
