//! vtium.conf in its two parts (ADR-1403 D3): the top level, every line
//! above the first section heading, and the sections, from that heading
//! to the end of the file.
//!
//! The top level holds vtium's keys, which the console reads the forgiving
//! way, and the two keys of the flow, `version` and `policy`. The sections
//! hold the looks, the heads and the rules, which the section reader reads
//! the strict way of ADR-1401 D7. [`split`] is the one place that decides
//! where the top level ends, so the console, `vtium check-config`, the
//! line editor and the section writer all agree on it: a line is a section
//! heading when its first byte that is not a space or a tab is `[`, even a
//! heading vtium cannot read. Both parts keep the whole file's line
//! numbers, so an error in a section names its line in vtium.conf.

use crate::conf::key_line;

/// The UTF-8 byte-order mark, which is no part of the first line.
pub(crate) const BOM: &[u8] = b"\xef\xbb\xbf";

/// The top-level key that states the file's policy.
pub(crate) const POLICY_KEY: &str = "policy";

/// The top-level key that states the format's version.
pub(crate) const VERSION_KEY: &str = "version";

/// Whether a line's bytes are a section heading: its first byte that is
/// not a space or a tab is `[`.
pub(crate) fn heading(raw: &[u8]) -> bool {
    raw.iter()
        .find(|b| !matches!(b, b' ' | b'\t'))
        .is_some_and(|&b| b == b'[')
}

/// Whether a line's bytes, after their leading blanks, begin with `policy`
/// in any case: a policy line, wherever above the first section it sits,
/// whatever bytes follow (ADR-1401 D7).
pub(crate) fn begins_policy(raw: &[u8]) -> bool {
    let start = raw
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t'))
        .unwrap_or(raw.len());
    raw.get(start..)
        .and_then(|rest| rest.get(..POLICY_KEY.len()))
        .is_some_and(|word| word.eq_ignore_ascii_case(POLICY_KEY.as_bytes()))
}

/// The lines of `bytes`, each without its newline, numbered from `first`.
/// The empty piece after a last newline is no line, so no bytes are no
/// line either.
pub(crate) fn numbered(bytes: &[u8], first: usize) -> impl Iterator<Item = (usize, &[u8])> {
    let ends_in_newline = bytes.last().is_none_or(|&b| b == b'\n');
    let mut pieces = bytes.split(|&b| b == b'\n').peekable();
    let mut n = first;
    std::iter::from_fn(move || {
        let piece = pieces.next()?;
        if ends_in_newline && pieces.peek().is_none() {
            return None;
        }
        let at = n;
        n = n.saturating_add(1);
        Some((at, piece))
    })
}

/// vtium.conf's bytes in their two parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split<'a> {
    /// Every line above the first section heading, each with its newline,
    /// and a byte-order mark before the first line, even one before a
    /// heading on the first line. It is the whole file when the file has
    /// no section.
    pub top: &'a [u8],
    /// The first section heading and every line after it; empty when the
    /// file has no section.
    pub sections: &'a [u8],
    /// The line number of the first section heading, counting from 1;
    /// `None` when the file has no section.
    pub first_section: Option<usize>,
}

/// Splits vtium.conf's bytes at the first section heading. The top level
/// keeps the lines above it, and the sections take the heading and every
/// line after it, so a line holding a key after a heading belongs to that
/// heading's section.
pub fn split(bytes: &[u8]) -> Split<'_> {
    split_after(bytes, true)
}

/// [`split`], reading a byte-order mark before the first line as no part
/// of it only where `bom` says: the section reader has dropped the mark
/// already, so a second one is a byte of the first line, as [`split`]
/// reads it too.
pub(crate) fn split_after(bytes: &[u8], bom: bool) -> Split<'_> {
    let mut start = 0_usize;
    for (n, piece) in bytes.split(|&b| b == b'\n').enumerate() {
        let line = match n {
            0 if bom => piece.strip_prefix(BOM).unwrap_or(piece),
            _ => piece,
        };
        if heading(line) {
            // a mark before a heading on the first line stays the top's
            let at = start.saturating_add(piece.len().saturating_sub(line.len()));
            let (top, sections) = bytes.split_at_checked(at).unwrap_or((bytes, &[]));
            return Split {
                top,
                sections,
                first_section: Some(n.saturating_add(1)),
            };
        }
        start = start.saturating_add(piece.len()).saturating_add(1);
    }
    Split {
        top: bytes,
        sections: &[],
        first_section: None,
    }
}

impl<'a> Split<'a> {
    /// The top level's lines, each with its line number from 1 and without
    /// its newline, a byte-order mark before the first one dropped. A
    /// carriage return that ends a line is kept, since the console's line
    /// reading drops it as a blank.
    pub fn top_lines(&self) -> impl Iterator<Item = (usize, &'a [u8])> {
        numbered(self.top.strip_prefix(BOM).unwrap_or(self.top), 1)
    }

    /// The top level's lines that hold vtium's own keys, which the console
    /// reads the forgiving way: every top-level line but a policy line,
    /// one that begins with `policy` in any case, and a `version` line,
    /// which belong to the flow and which the section reader reads.
    pub fn key_lines(&self) -> impl Iterator<Item = (usize, &'a [u8])> {
        self.top_lines()
            .filter(|(_, line)| !begins_policy(line) && !version_line(line))
    }

    /// Whether the file holds a flow: a section, or a policy line at the
    /// top level. For a file with neither, as a copied kmscon.conf is,
    /// vtium runs its compiled-in flow (ADR-1403 D3); a policy line alone still
    /// counts, so a file that states `policy = closed` is never read as
    /// open.
    pub fn has_flow(&self) -> bool {
        self.first_section.is_some() || self.top_lines().any(|(_, line)| begins_policy(line))
    }
}

/// Whether a top-level line sets the key `version`.
fn version_line(line: &[u8]) -> bool {
    key_line(line).is_some_and(|l| l.key == VERSION_KEY.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_level_ends_where_the_first_heading_starts() {
        let text = b"font-size=12\nversion = 1\n\n  [head desk]\nlook = big\n[rules]\n";
        let s = split(text);
        assert_eq!(s.top, b"font-size=12\nversion = 1\n\n");
        assert_eq!(s.sections, b"  [head desk]\nlook = big\n[rules]\n");
        assert_eq!(s.first_section, Some(4));
        let top: Vec<_> = s.top_lines().collect();
        assert_eq!(
            top,
            [(1, &b"font-size=12"[..]), (2, b"version = 1"), (3, b""),]
        );
        let keys: Vec<_> = s.key_lines().map(|(n, _)| n).collect();
        assert_eq!(keys, [1, 3]);
    }

    #[test]
    fn a_file_with_no_heading_is_all_top_level() {
        let s = split(b"bell\nfont-size=12");
        assert_eq!(s.top, b"bell\nfont-size=12");
        assert_eq!(s.sections, b"");
        assert_eq!(s.first_section, None);
        assert!(!s.has_flow());
        assert!(split(b"version = 1\nPolicy = closed\n").has_flow());
        assert!(split(b"[rules]\n").has_flow());
    }

    // the fuzz targets' first finds: a file that opens with a heading has
    // no top-level line, not an empty line 1
    #[test]
    fn a_file_that_opens_with_a_heading_has_no_top_level_line() {
        for bytes in [&b"["[..], b"[\x0e\t\0", b"[head x]\n", b""] {
            let s = split(bytes);
            assert_eq!(s.top_lines().count(), 0, "{bytes:?}");
            assert_eq!(s.key_lines().count(), 0, "{bytes:?}");
        }
    }

    #[test]
    fn a_heading_after_a_byte_order_mark_still_starts_the_sections() {
        let s = split(b"\xef\xbb\xbf[head desk]\n");
        assert_eq!(s.first_section, Some(1));
        assert_eq!(s.top, b"\xef\xbb\xbf");
        assert_eq!(s.sections, b"[head desk]\n");
        assert_eq!(s.top_lines().count(), 0);
        let s = split(b"\xef\xbb\xbffont-size=1\n");
        assert_eq!(s.top_lines().next(), Some((1, &b"font-size=1"[..])));
    }

    #[test]
    fn a_policy_line_and_a_version_line_are_no_key_lines() {
        let s = split(b"POLICY=x\nversion=1\nversions=2\npolicy-ish\nbell\n");
        let keys: Vec<_> = s.key_lines().map(|(n, _)| n).collect();
        assert_eq!(keys, [3, 5]);
        assert_eq!(split(b"a\n\nb\n").top_lines().count(), 3);
        assert_eq!(
            split(b"a\n\nb\n").top_lines().nth(2).map(|l| l.1),
            Some(&b"b"[..])
        );
        assert_eq!(
            split(b"x\n").top_lines().map(|l| l.0).collect::<Vec<_>>(),
            [1]
        );
    }
}
