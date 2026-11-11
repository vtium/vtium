//! What a save changes (ADR-1201 D5): the files of the configuration it
//! writes, each with its text before and after, the frits it removes, and
//! the lines that differ, which the TUI's save view and the commands show
//! before anything is written.

use std::path::PathBuf;

/// One file a save replaces or writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEdit {
    /// Where the file is installed, such as `/etc/vtium/vtium.conf`.
    pub path: PathBuf,
    /// The file as it was read, `None` when it did not exist.
    pub old: Option<String>,
    /// The whole file as the save writes it.
    pub new: String,
}

/// A frit a save removes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removal {
    /// The installed frit, such as `/etc/vtium/fonts/old.vtfrit`.
    pub path: PathBuf,
    /// Its bytes as they were read, so `apply` refuses the removal when the
    /// frit changed since.
    pub old: Vec<u8>,
}

/// Everything one save changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// The files it writes: frits, vtium.conf and the head file.
    pub files: Vec<FileEdit>,
    pub removals: Vec<Removal>,
    /// Whether the install accepts a vtium.conf that wires no complete
    /// route, because the person confirmed it or ran `install --force`
    /// (ADR-1403 D8). The draft's `MANIFEST` records it, and `apply` then
    /// accepts that one problem from `vtium check-config`.
    pub no_route: bool,
}

impl Plan {
    /// Whether the save changes nothing.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.removals.is_empty()
    }

    /// How many files the save writes or removes.
    pub fn len(&self) -> usize {
        self.files.len().saturating_add(self.removals.len())
    }
}

/// A line that differs between a file before and after: a line removed
/// (`new` is `None`), added (`old` is `None`), or changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The line's number in the file after, from 1; for a removed line,
    /// the number of the line before which it was.
    pub line: usize,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// The lines that differ between `old` and `new`, in order: the longest
/// run of lines both keep stays, and every other line is removed or
/// added, a removal followed by an addition at one place shown as one
/// changed line.
pub fn changes(old: &str, new: &str) -> Vec<Change> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // the common prefix and suffix need no table
    let lead = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let trail = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(a.len().min(b.len()).saturating_sub(lead))
        .take_while(|(x, y)| x == y)
        .count();
    let a_mid = a
        .get(lead..a.len().saturating_sub(trail))
        .unwrap_or_default();
    let b_mid = b
        .get(lead..b.len().saturating_sub(trail))
        .unwrap_or_default();
    // longest common subsequence of the middles, by a table of lengths
    let width = b_mid.len().saturating_add(1);
    let mut table = vec![0usize; a_mid.len().saturating_add(1).saturating_mul(width)];
    for i in (0..a_mid.len()).rev() {
        for j in (0..b_mid.len()).rev() {
            let here = i.saturating_mul(width).saturating_add(j);
            let value = if a_mid.get(i) == b_mid.get(j) {
                table
                    .get(here.saturating_add(width).saturating_add(1))
                    .map_or(1, |v| v.saturating_add(1))
            } else {
                let down = table.get(here.saturating_add(width)).copied().unwrap_or(0);
                let right = table.get(here.saturating_add(1)).copied().unwrap_or(0);
                down.max(right)
            };
            if let Some(slot) = table.get_mut(here) {
                *slot = value;
            }
        }
    }
    let at = |i: usize, j: usize| {
        table
            .get(i.saturating_mul(width).saturating_add(j))
            .copied()
            .unwrap_or(0)
    };
    let mut out: Vec<Change> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a_mid.len() || j < b_mid.len() {
        let line = lead.saturating_add(j).saturating_add(1);
        match (a_mid.get(i), b_mid.get(j)) {
            (Some(x), Some(y)) if x == y => {
                i = i.saturating_add(1);
                j = j.saturating_add(1);
            }
            (Some(x), y)
                if y.is_none() || at(i.saturating_add(1), j) >= at(i, j.saturating_add(1)) =>
            {
                out.push(Change {
                    line,
                    old: Some((*x).to_owned()),
                    new: None,
                });
                i = i.saturating_add(1);
            }
            (_, Some(y)) => {
                // an addition right after a removal at the same place is a
                // change of that line
                match out.last_mut() {
                    Some(last) if last.new.is_none() && last.line == line => {
                        last.new = Some((*y).to_owned());
                    }
                    _ => out.push(Change {
                        line,
                        old: None,
                        new: Some((*y).to_owned()),
                    }),
                }
                j = j.saturating_add(1);
            }
            (_, None) => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(line: usize, old: Option<&str>, new: Option<&str>) -> Change {
        Change {
            line,
            old: old.map(str::to_owned),
            new: new.map(str::to_owned),
        }
    }

    #[test]
    fn a_changed_line_is_one_change() {
        assert_eq!(
            changes("a\nb\nc\n", "a\nB\nc\n"),
            [change(2, Some("b"), Some("B"))]
        );
    }

    #[test]
    fn an_added_line_and_a_removed_line_are_each_one_change() {
        assert_eq!(changes("a\nc\n", "a\nb\nc\n"), [change(2, None, Some("b"))]);
        assert_eq!(changes("a\nb\nc\n", "a\nc\n"), [change(2, Some("b"), None)]);
        assert_eq!(changes("", "x\n"), [change(1, None, Some("x"))]);
    }

    #[test]
    fn the_same_text_has_no_change() {
        assert_eq!(changes("a\nb\n", "a\nb\n"), []);
        assert_eq!(changes("", ""), []);
    }

    #[test]
    fn changes_far_apart_are_each_shown() {
        assert_eq!(
            changes("1\n2\n3\n4\n5\n", "1\nX\n3\n4\n5\nY\n"),
            [change(2, Some("2"), Some("X")), change(6, None, Some("Y"))]
        );
    }
}
