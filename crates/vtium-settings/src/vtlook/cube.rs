//! The `.cube` reader: a color table in the text format of Adobe's Cube
//! LUT Specification 1.0, read strictly, and the gate a `.cube` file
//! passes before vtium reads it.
//!
//! A `.cube` file is text in lines that end in LF or CRLF. A line whose
//! first character that is not a blank is `#` is a comment, and a blank
//! line is skipped. Keyword lines come first, each keyword at most once:
//! `TITLE "text"`, optional; exactly one of `LUT_3D_SIZE N`, N from 2 to
//! 65, and `LUT_1D_SIZE N`, N from 2 to 4096; and `DOMAIN_MIN 0 0 0` and
//! `DOMAIN_MAX 1 1 1`, optional, and only at those values. Then come the
//! data lines, N × N × N of them for a 3D table and N for a 1D table, each
//! three numbers separated by blanks. A number is a decimal with an
//! optional sign and an optional exponent, such as `0.5`, `-0.01` or
//! `1e-3`, and vtium holds a value outside 0 to 1 to that range.
//!
//! The reader reads every line whatever comes before it, so a check names
//! every fault the file holds, each with its line. Each fault says what
//! the reader found and what it needs there. The file is data, numbers in
//! a fixed grammar, and the reader decodes nothing else.
//!
//! vtium reads a `.cube` file only when it is active: the batch must list
//! it, and it must pass the trust rule that every resource passes, which
//! [`crate::cullet`] checks on the descriptor vtium then reads from. A file
//! that fails either check counts as missing.
//!
//! References:
//! - ADR-1304 D9: the format vtium reads, its sizes, its bound of 16 MiB and the clamp to 0 to 1.
//! - ADR-1304 D14: the grammar, its examples, and the report of each fault.
//! - ADR-1205 D4: a `.cube` file counts as missing unless the batch holds it and it passes the trust rule.

use std::io::Read;
use std::path::Path;

use vtium_render::filter::{Cube, SIDES_3D, SIZES_1D, Shape};

use super::report::quoted;
use crate::batch::{BATCH_FILE, Batch};
use crate::bounds::{CUBE_FILE_MAX, ERRORS_MAX};
use crate::cullet::{self, Rule};
use crate::mounts::Mounts;
use crate::split::BOM;

/// The keywords a `.cube` file may hold, in the order a fault lists them.
const KEYWORDS: [&str; 5] = [
    "TITLE",
    "LUT_3D_SIZE",
    "LUT_1D_SIZE",
    "DOMAIN_MIN",
    "DOMAIN_MAX",
];

/// One fault in a `.cube` file: its line, `None` for the file as a whole,
/// and what the reader found there and what it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    pub line: Option<usize>,
    pub message: String,
}

/// Why a `.cube` file does not read: its first faults by line, at most
/// `ERRORS_MAX`, and how many more it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Faults {
    /// The faults kept, in the file's order, at least one.
    pub faults: Vec<Fault>,
    /// The faults past those kept.
    pub more: usize,
}

/// The reader's state while it reads one file.
#[derive(Default)]
struct Reader {
    faults: Vec<Fault>,
    more: usize,
    /// The table's shape once a size line reads, with the keyword and its
    /// line.
    size: Option<(Shape, &'static str, usize)>,
    /// The line of each keyword read, in [`KEYWORDS`]' order.
    seen: [Option<usize>; 5],
    /// Whether a size keyword has come on any line, one that does not
    /// read included, so the file is not also said to lack a size line.
    sized: bool,
    /// Whether a data line has come.
    data: bool,
    /// The data lines counted, those that do not read included.
    count: usize,
    /// The data lines the size asks for, once a size line reads.
    want: usize,
    entries: Vec<[f64; 3]>,
}

impl Reader {
    /// Notes a fault on line `line`. The reader finds faults in line
    /// order, so it keeps the first [`ERRORS_MAX`] and counts the rest.
    fn fault(&mut self, line: Option<usize>, message: String) {
        if self.faults.len() < ERRORS_MAX {
            self.faults.push(Fault { line, message });
        } else {
            self.more = self.more.saturating_add(1);
        }
    }

    /// Reads keyword line `n`, whose first word is `word` and whose whole
    /// text is `line`.
    fn keyword(&mut self, n: usize, word: &str, line: &str) {
        let Some(k) = KEYWORDS.iter().position(|k| *k == word) else {
            self.fault(
                Some(n),
                format!(
                    "found {}, needs TITLE, LUT_3D_SIZE, LUT_1D_SIZE, DOMAIN_MIN or DOMAIN_MAX",
                    quoted(word)
                ),
            );
            return;
        };
        let Some(&keyword) = KEYWORDS.get(k) else {
            return;
        };
        if keyword.ends_with("_SIZE") {
            self.sized = true;
        }
        if self.data {
            self.fault(
                Some(n),
                format!("found {keyword} after the first data line, needs every keyword line before the data"),
            );
            return;
        }
        if let Some(first) = self.seen.get(k).copied().flatten() {
            self.fault(
                Some(n),
                format!(
                    "found {keyword} again, needs each keyword once; the first is on line {first}"
                ),
            );
            return;
        }
        if let Some(slot) = self.seen.get_mut(k) {
            *slot = Some(n);
        }
        let rest = line
            .get(word.len()..)
            .unwrap_or_default()
            .trim_matches([' ', '\t']);
        match keyword {
            "TITLE" => {
                let quoted_title = rest.len() >= 2 && rest.starts_with('"') && rest.ends_with('"');
                if !quoted_title {
                    self.fault(
                        Some(n),
                        format!("found {}, needs TITLE \"text\"", quoted(line)),
                    );
                }
            }
            "LUT_3D_SIZE" | "LUT_1D_SIZE" => self.size_line(n, keyword, rest, line),
            _ => {
                let want = if keyword == "DOMAIN_MIN" { 0.0 } else { 1.0 };
                let values: Vec<Option<f64>> = rest
                    .split([' ', '\t'])
                    .filter(|w| !w.is_empty())
                    .map(number)
                    .collect();
                if values != [Some(want); 3] {
                    self.fault(
                        Some(n),
                        format!(
                            "found {}, needs {keyword} {w} {w} {w}",
                            quoted(line),
                            w = want
                        ),
                    );
                }
            }
        }
    }

    /// Reads the size line `n`, `keyword` with the value `rest`.
    fn size_line(&mut self, n: usize, keyword: &'static str, rest: &str, line: &str) {
        if let Some((_, other, _)) = self.size {
            self.fault(
                Some(n),
                format!("found {keyword} after {other}, needs one of the two"),
            );
            return;
        }
        let (range, shape): (_, fn(usize) -> Shape) = if keyword == "LUT_3D_SIZE" {
            (SIDES_3D, Shape::Three)
        } else {
            (SIZES_1D, Shape::One)
        };
        let size = rest
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| rest.parse::<usize>().ok())
            .flatten()
            .filter(|s| range.contains(s));
        let Some(size) = size else {
            self.fault(
                Some(n),
                format!(
                    "found {}, needs {} to {}",
                    quoted(line),
                    range.start(),
                    range.end()
                ),
            );
            return;
        };
        let shape = shape(size);
        let count = match shape {
            Shape::Three(s) => s.saturating_mul(s).saturating_mul(s),
            Shape::One(s) => s,
        };
        if self.entries.try_reserve_exact(count).is_err() {
            self.fault(
                Some(n),
                format!(
                    "found {}, and vtium has no memory for a table of that size",
                    quoted(line)
                ),
            );
            return;
        }
        self.size = Some((shape, keyword, n));
        self.want = count;
    }

    /// Reads data line `n`, whose text is `line`.
    fn data_line(&mut self, n: usize, line: &str) {
        self.data = true;
        self.count = self.count.saturating_add(1);
        let words: Vec<&str> = line.split([' ', '\t']).filter(|w| !w.is_empty()).collect();
        let values: Option<Vec<f64>> = (words.len() == 3)
            .then(|| words.iter().copied().map(number).collect())
            .flatten();
        let Some(&[r, g, b]) = values.as_deref() else {
            self.fault(
                Some(n),
                format!(
                    "found \"{}\", needs three numbers separated by blanks",
                    quoted(line)
                ),
            );
            return;
        };
        if self.entries.len() < self.want {
            self.entries.push([r, g, b]);
        }
    }
}

/// A number as a `.cube` file writes it: a decimal with an optional sign
/// and an optional exponent, finite. `None` for anything else, such as
/// `nan`, `inf`, a hex float or `1e999`.
fn number(word: &str) -> Option<f64> {
    let b = word.as_bytes();
    let mut i = 0_usize;
    let at = |i: usize| b.get(i).copied();
    if matches!(at(i), Some(b'+' | b'-')) {
        i = i.saturating_add(1);
    }
    let mut digits = 0_usize;
    while at(i).is_some_and(|c| c.is_ascii_digit()) {
        i = i.saturating_add(1);
        digits = digits.saturating_add(1);
    }
    if at(i) == Some(b'.') {
        i = i.saturating_add(1);
        while at(i).is_some_and(|c| c.is_ascii_digit()) {
            i = i.saturating_add(1);
            digits = digits.saturating_add(1);
        }
    }
    if digits == 0 {
        return None;
    }
    if matches!(at(i), Some(b'e' | b'E')) {
        i = i.saturating_add(1);
        if matches!(at(i), Some(b'+' | b'-')) {
            i = i.saturating_add(1);
        }
        let start = i;
        while at(i).is_some_and(|c| c.is_ascii_digit()) {
            i = i.saturating_add(1);
        }
        if i == start {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    word.parse::<f64>().ok().filter(|x| x.is_finite())
}

/// Reads a `.cube` file's bytes into its table, or the faults that keep
/// it from reading.
pub fn read_cube(bytes: &[u8]) -> Result<Cube, Faults> {
    let mut r = Reader::default();
    let bytes = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let mut last = 0_usize;
    let mut pieces = bytes.split(|&b| b == b'\n').peekable();
    while let Some(raw) = pieces.next() {
        // the empty piece after the last line's newline is no line
        if raw.is_empty() && pieces.peek().is_none() {
            break;
        }
        last = last.saturating_add(1);
        let n = last;
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        let Ok(text) = std::str::from_utf8(raw) else {
            r.fault(
                Some(n),
                "found bytes that are not UTF-8 text, needs a line of text".to_owned(),
            );
            continue;
        };
        if text.chars().any(|c| c.is_control() && c != '\t') {
            r.fault(
                Some(n),
                "found a control character, needs a line with none but tab".to_owned(),
            );
            continue;
        }
        let line = text.trim_matches([' ', '\t']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let word = line.split([' ', '\t']).next().unwrap_or_default();
        if word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            r.keyword(n, word, line);
        } else {
            r.data_line(n, line);
        }
    }
    let Some((shape, keyword, _)) = r.size else {
        if r.sized {
            // the size line's own fault says what is wrong with it
            return Err(Faults {
                faults: r.faults,
                more: r.more,
            });
        }
        r.faults.insert(
            0,
            Fault {
                line: None,
                message: "found no LUT_3D_SIZE or LUT_1D_SIZE line, needs one of the two"
                    .to_owned(),
            },
        );
        if r.faults.len() > ERRORS_MAX {
            r.faults.truncate(ERRORS_MAX);
            r.more = r.more.saturating_add(1);
        }
        return Err(Faults {
            faults: r.faults,
            more: r.more,
        });
    };
    let (want, of) = match shape {
        Shape::Three(s) => (
            s.saturating_mul(s).saturating_mul(s),
            format!("{keyword} {s} cubed"),
        ),
        Shape::One(s) => (s, format!("{keyword} {s}")),
    };
    if r.count != want {
        let found = r.count;
        r.fault(
            Some(last),
            format!("found {found} data lines, needs {want}, {of}"),
        );
    }
    if !r.faults.is_empty() {
        return Err(Faults {
            faults: r.faults,
            more: r.more,
        });
    }
    Cube::new(shape, r.entries).ok_or_else(|| Faults {
        faults: vec![Fault {
            line: None,
            message: format!("found a table vtium cannot hold, needs {of}"),
        }],
        more: 0,
    })
}

/// Where a vtlook's `.cube` files come from, and the checks each passes
/// before vtium reads it: the folder that holds them, the batch that says
/// which resources are active, and the trust rule with the mount table it
/// judges filesystems by. vtium passes the looks folder, the batch it read
/// at start, its rule and the running system's mount table. A test passes
/// fixtures of its own.
#[derive(Clone, Copy, Debug)]
pub struct Gate<'a> {
    /// The folder a vtlook's `file = NAME.cube` names a file in.
    pub dir: &'a Path,
    pub batch: &'a Batch,
    pub rule: &'a Rule,
    pub mounts: &'a Mounts,
}

/// Why vtium did not read a `.cube` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Unread {
    /// A fault of the reading stage: the file is not active, fails the
    /// trust rule, is too large or cannot be read, and counts as missing.
    Missing(String),
    /// The file's faults, of the checking stage.
    Faults(Faults),
}

/// Reads the `.cube` file `file` through `gate`: the batch must hold it
/// and it must pass the trust rule, or it counts as missing.
pub(super) fn load(file: &Path, gate: &Gate) -> Result<Cube, Unread> {
    let missing = |why: String| {
        Err(Unread::Missing(format!(
            "vtium treats the file as missing, because {why}"
        )))
    };
    if !gate.batch.holds(file) {
        return missing(format!("it is not active: {BATCH_FILE} does not list it."));
    }
    let opened = match cullet::open(file, gate.rule, gate.mounts) {
        Ok(opened) => opened,
        Err(refused) => return missing(refused.check.to_string()),
    };
    let too_large = || {
        Err(Unread::Missing(format!(
            "The file is larger than {CUBE_FILE_MAX} bytes, the most a .cube file may hold."
        )))
    };
    if opened.meta.len() > u64::try_from(CUBE_FILE_MAX).unwrap_or(u64::MAX) {
        return too_large();
    }
    let mut bytes = Vec::new();
    let bound = u64::try_from(CUBE_FILE_MAX)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    if let Err(e) = opened.file.take(bound).read_to_end(&mut bytes) {
        return Err(Unread::Missing(format!(
            "vtium cannot read the file: {}.",
            e.kind()
        )));
    }
    if bytes.len() > CUBE_FILE_MAX {
        // the file grew after vtium looked at its size
        return too_large();
    }
    read_cube(&bytes).map_err(Unread::Faults)
}

#[cfg(test)]
#[expect(
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "a test fails by panicking, and its figures are small"
)]
mod tests {
    use super::*;

    fn faults(text: &str) -> Vec<String> {
        match read_cube(text.as_bytes()) {
            Ok(c) => panic!("read {:?}", c.shape()),
            Err(f) => f
                .faults
                .iter()
                .map(|f| match f.line {
                    Some(n) => format!("{n}: {}", f.message),
                    None => f.message.clone(),
                })
                .collect(),
        }
    }

    /// The data lines of a 3D table of `n` a side, each `0 0 0`.
    fn zeros(n: usize) -> String {
        "0 0 0\n".repeat(n * n * n)
    }

    // The smallest identity table reads, its title, domain lines and
    // comments with it, with LF or CRLF, and a byte-order mark before it.
    #[test]
    fn a_3d_table_reads_with_its_keywords_and_comments() {
        let text = "# identity.cube\nTITLE \"identity\"\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1.0 1 1e0\nLUT_3D_SIZE 2\n\n  # corners\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let cube = read_cube(text.as_bytes()).unwrap_or_else(|f| panic!("{f:?}"));
        assert_eq!(cube.shape(), Shape::Three(2));
        let crlf = format!("\u{feff}{}", text.replace('\n', "\r\n"));
        assert_eq!(read_cube(crlf.as_bytes()), Ok(cube));
        let one = read_cube(b"LUT_1D_SIZE 2\n0 0 0\n1 1 1").unwrap_or_else(|f| panic!("{f:?}"));
        assert_eq!(one.shape(), Shape::One(2));
    }

    // Each fault of ADR-1304 D14's table gives its report, on its line.
    #[test]
    fn each_fault_names_its_line_what_it_found_and_what_it_needs() {
        assert_eq!(
            faults(&format!(
                "TITLE \"x\"\nLUT_3D_SIZE 2\nLUT_3D_INPUT_RANGE 0 1\n{}",
                zeros(2)
            )),
            [
                "3: found LUT_3D_INPUT_RANGE, needs TITLE, LUT_3D_SIZE, LUT_1D_SIZE, DOMAIN_MIN or DOMAIN_MAX"
            ]
        );
        assert_eq!(
            faults("TITLE \"x\"\nLUT_3D_SIZE 129\n0 0 0\n"),
            ["2: found LUT_3D_SIZE 129, needs 2 to 65"]
        );
        assert_eq!(
            faults("TITLE \"x\"\nLUT_1D_SIZE 8192\n"),
            ["2: found LUT_1D_SIZE 8192, needs 2 to 4096"]
        );
        assert_eq!(
            faults(&format!(
                "TITLE \"x\"\nLUT_3D_SIZE 2\nLUT_1D_SIZE 2\n{}",
                zeros(2)
            )),
            ["3: found LUT_1D_SIZE after LUT_3D_SIZE, needs one of the two"]
        );
        assert_eq!(
            faults("LUT_1D_SIZE 2\n0 0 0\n0.5,0.2,0.1\n"),
            ["3: found \"0.5,0.2,0.1\", needs three numbers separated by blanks"]
        );
        assert_eq!(
            faults(&format!("TITLE \"x\"\nLUT_3D_SIZE 3\n{}", zeros(2))),
            ["10: found 8 data lines, needs 27, LUT_3D_SIZE 3 cubed"]
        );
        assert_eq!(
            faults("LUT_1D_SIZE 3\n0 0 0\n1 1 1\n"),
            ["3: found 2 data lines, needs 3, LUT_1D_SIZE 3"]
        );
        assert_eq!(
            faults(&format!(
                "TITLE \"x\"\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1.2\n{}",
                zeros(2)
            )),
            ["4: found DOMAIN_MAX 1 1 1.2, needs DOMAIN_MAX 1 1 1"]
        );
    }

    // A size line that is present but fails gives its own fault and no
    // fault saying the file lacks a size line: a value that is not a
    // number, a comment after the value, and a size line after the data.
    #[test]
    fn a_size_line_that_fails_is_not_reported_missing() {
        assert_eq!(
            faults("LUT_3D_SIZE abc\n0 0 0\n"),
            ["1: found LUT_3D_SIZE abc, needs 2 to 65"]
        );
        assert_eq!(
            faults("LUT_1D_SIZE 2 # two\n0 0 0\n1 1 1\n"),
            ["1: found LUT_1D_SIZE 2 # two, needs 2 to 4096"]
        );
        assert_eq!(
            faults("TITLE \"x\"\n0 0 0\nLUT_1D_SIZE 1\n"),
            [
                "3: found LUT_1D_SIZE after the first data line, needs every keyword line before the data"
            ]
        );
        // a keyword in the wrong case is no size line
        assert_eq!(
            faults("lut_1d_size 2\n"),
            [
                "found no LUT_3D_SIZE or LUT_1D_SIZE line, needs one of the two",
                "1: found lut_1d_size, needs TITLE, LUT_3D_SIZE, LUT_1D_SIZE, DOMAIN_MIN or DOMAIN_MAX",
            ]
        );
    }

    // Faults the record's table does not list: a keyword twice, a keyword
    // after the data, a title without quotes, a number that is not finite,
    // a line of four numbers, a control character, bytes that are not
    // UTF-8 and a file with no size. Every one is reported, in line order.
    #[test]
    fn every_fault_in_the_file_is_reported() {
        let text = "TITLE x\nTITLE \"x\"\nLUT_1D_SIZE 3\n0 0 0\nnan 0 0\n1e999 0 0\n0 0 0 0\nDOMAIN_MIN 0 0 0\n\u{7}\n0x1 0 0\n";
        assert_eq!(
            faults(text),
            [
                "1: found TITLE x, needs TITLE \"text\"",
                "2: found TITLE again, needs each keyword once; the first is on line 1",
                "5: found nan, needs TITLE, LUT_3D_SIZE, LUT_1D_SIZE, DOMAIN_MIN or DOMAIN_MAX",
                "6: found \"1e999 0 0\", needs three numbers separated by blanks",
                "7: found \"0 0 0 0\", needs three numbers separated by blanks",
                "8: found DOMAIN_MIN after the first data line, needs every keyword line before the data",
                "9: found a control character, needs a line with none but tab",
                "10: found \"0x1 0 0\", needs three numbers separated by blanks",
                "10: found 4 data lines, needs 3, LUT_1D_SIZE 3",
            ]
        );
        let bad = read_cube(b"LUT_1D_SIZE 2\n\xff\n").map_err(|f| {
            f.faults
                .into_iter()
                .map(|f| format!("{}: {}", f.line.unwrap_or(0), f.message))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            bad.err().unwrap_or_default(),
            [
                "2: found bytes that are not UTF-8 text, needs a line of text",
                "2: found 0 data lines, needs 2, LUT_1D_SIZE 2"
            ]
        );
        assert_eq!(
            faults(""),
            ["found no LUT_3D_SIZE or LUT_1D_SIZE line, needs one of the two"]
        );
    }

    // A number is a decimal with an optional sign and exponent, and
    // nothing else.
    #[test]
    fn a_number_is_a_decimal_with_an_optional_sign_and_exponent() {
        for (word, want) in [
            ("0.5", Some(0.5)),
            ("-0.01", Some(-0.01)),
            ("1e-3", Some(0.001)),
            ("+2E2", Some(200.0)),
            (".5", Some(0.5)),
            ("5.", Some(5.0)),
            ("1", Some(1.0)),
            ("", None),
            (".", None),
            ("-", None),
            ("1e", None),
            ("1e+", None),
            ("inf", None),
            ("NaN", None),
            ("1e400", None),
            ("0x10", None),
            ("1,5", None),
            ("1.5.2", None),
        ] {
            assert_eq!(number(word), want, "{word:?}");
        }
    }

    // The values are held to 0 to 1 once read.
    #[test]
    fn a_value_outside_0_to_1_is_held_to_the_range() {
        let a = read_cube(b"LUT_1D_SIZE 2\n-0.5 0 2\n1 1 1\n").unwrap_or_else(|f| panic!("{f:?}"));
        let b = read_cube(b"LUT_1D_SIZE 2\n0 0 1\n1 1 1\n").unwrap_or_else(|f| panic!("{f:?}"));
        assert_eq!(a, b);
    }

    // A file of tens of thousands of faults keeps the first ERRORS_MAX and
    // counts the rest.
    #[test]
    fn the_faults_kept_are_bounded() {
        let text = format!("LUT_1D_SIZE 2\n{}", "x,y\n".repeat(1000));
        let Err(f) = read_cube(text.as_bytes()) else {
            panic!("read");
        };
        assert_eq!(f.faults.len(), ERRORS_MAX);
        assert_eq!(f.faults.len() + f.more, 1001);
    }
}
