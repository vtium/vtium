//! The runner: every case of the corpus, copied from the C reference,
//! replayed through vtium and judged against its golden.
//!
//! A case matches its golden, after the masking INTERFACE.md's "Comparing
//! with the goldens" asks for, unless `expected.tsv` lists it. A listed
//! departure must print exactly vtium's expected output, kept under
//! `expected/`, and that output must differ from the golden; a listed
//! pending case must still differ from its golden. Any other difference
//! fails, so a departure is never silent and a list never goes stale.

pub mod chunking;
pub mod lists;
pub mod provenance;

use std::path::{Path, PathBuf};

use crate::replay::{self, Input};
use crate::view::View;
use lists::{Expectation, Expected};

/// The views vtium owes, with the golden directory and the expected
/// directory of each.
pub const VIEWS: [(View, &str, &str); 2] = [
    (View::Screen, "screen", "golden"),
    (View::Frame, "frame", "golden-frame"),
];

/// A case of `cases.list`: its name and the screen it starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub name: String,
    pub cols: usize,
    pub rows: usize,
}

/// The conformance corpus: `cases.list`, `cases/` and the goldens.
#[derive(Debug)]
pub struct Corpus {
    pub root: PathBuf,
    pub cases: Vec<Case>,
}

impl Corpus {
    /// Reads `cases.list` under `root`.
    pub fn open(root: &Path) -> Result<Self, String> {
        let path = root.join("cases.list");
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut cases = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            let case = match fields[..] {
                [name, cols, rows] => replay::size(cols, replay::COLS_MAX)
                    .zip(replay::size(rows, replay::ROWS_MAX))
                    .map(|(cols, rows)| Case {
                        name: name.to_owned(),
                        cols,
                        rows,
                    }),
                _ => None,
            };
            cases.push(case.ok_or_else(|| {
                format!(
                    "{}:{}: want NAME COLS ROWS",
                    path.display(),
                    n.saturating_add(1)
                )
            })?);
        }
        if cases.is_empty() {
            return Err(format!(
                "{}: no cases; refusing to report a pass",
                path.display()
            ));
        }
        Ok(Self {
            root: root.to_owned(),
            cases,
        })
    }

    /// The replay input of `case` in `view`, its cuts included.
    pub fn input(&self, case: &Case, view: View) -> Result<Input, String> {
        let file = self.root.join("cases").join(format!("{}.bin", case.name));
        let data = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let cuts_file = self.root.join("cases").join(format!("{}.cuts", case.name));
        let cuts = match std::fs::read(&cuts_file) {
            Ok(text) => replay::cuts(&text, data.len()).map_err(|f| f.message)?,
            Err(_) => Vec::new(),
        };
        Ok(Input {
            view,
            cols: case.cols,
            rows: case.rows,
            data,
            cuts,
            fixtures: Input::fixtures_for(&file),
        })
    }

    pub fn golden(&self, case: &Case, golden_dir: &str) -> Result<String, String> {
        let path = self
            .root
            .join(golden_dir)
            .join(format!("{}.txt", case.name));
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// The counts INTERFACE.md lets an implementation draw differently, masked
/// on both sides: a nonzero `changed N` or `frame-since` count becomes the
/// letter N, and a placement's ` changed` flag goes.
pub fn mask(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        out.push_str(&mask_line(line));
        out.push('\n');
    }
    out
}

/// One line of [`mask`].
pub fn mask_line(line: &str) -> String {
    match line.rsplit_once(' ') {
        Some((head, n))
            if (head == "changed" || is_frame_since(head))
                && n.bytes().all(|b| b.is_ascii_digit())
                && n != "0" =>
        {
            format!("{head} N")
        }
        _ if line.starts_with("placement ") => {
            line.strip_suffix(" changed").unwrap_or(line).to_owned()
        }
        _ => line.to_owned(),
    }
}

fn is_frame_since(head: &str) -> bool {
    head.strip_prefix("test frame-since ")
        .and_then(|rest| rest.strip_suffix(" changed"))
        .is_some_and(|k| k.len() == 1 && k.bytes().all(|b| b.is_ascii_digit()))
}

/// A case's verdict in one view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The output matches the golden.
    Pass,
    /// The output differs from the golden as a listed departure says.
    Departure,
    /// A listed pending case still differs from its golden, and prints or
    /// exits as its snapshot says.
    Pending,
    /// Anything else, with why.
    Fail(String),
}

/// Judges vtium's output, or its failure, against the golden and the
/// expectation listed for the case.
pub fn judge(
    got: &Result<String, crate::Failure>,
    golden: &str,
    expected: Option<&Expected>,
) -> Verdict {
    let golden = mask(golden);
    let got = got.as_ref().map(|out| mask(out));
    let matches = got.as_ref().is_ok_and(|out| *out == golden);
    match (expected.map(|e| &e.kind), &got) {
        (None, _) if matches => Verdict::Pass,
        (None, Ok(out)) => Verdict::Fail(format!(
            "differs from the golden, unlisted\n{}",
            diff(&golden, out)
        )),
        (None, Err(f)) => Verdict::Fail(format!("exits {}: {}", f.code, f.message)),
        (Some(Expectation::Pending(_)), _) if matches => {
            Verdict::Fail("listed as pending, and matches its golden: drop its rows".to_owned())
        }
        (Some(Expectation::Pending(want)), _) => {
            let got = got.map_or_else(|f| snapshot(&Err(f.clone())), |out| out.clone());
            if got == mask(want) {
                Verdict::Pending
            } else {
                Verdict::Fail(format!(
                    "pending, and changed from its snapshot\n{}",
                    diff(&mask(want), &got)
                ))
            }
        }
        (Some(Expectation::Departure(_)), _) if matches => {
            Verdict::Fail("listed as a departure, and matches its golden: drop its rows".to_owned())
        }
        (Some(Expectation::Departure(want)), Ok(out)) if *out == mask(want) => Verdict::Departure,
        (Some(Expectation::Departure(want)), Ok(out)) => Verdict::Fail(format!(
            "differs from its listed departure\n{}",
            diff(&mask(want), out)
        )),
        (Some(Expectation::Departure(_)), Err(f)) => {
            Verdict::Fail(format!("exits {}: {}", f.code, f.message))
        }
    }
}

/// What a pending case's snapshot holds: the replay's output, or what it
/// printed before it stopped and then its exit code and message.
pub fn snapshot(got: &Result<String, crate::Failure>) -> String {
    match got {
        Ok(out) => out.clone(),
        Err(f) => format!("{}exit {}: {}\n", mask(&f.output), f.code, f.message),
    }
}

/// The first lines where `got` differs from `want`, with a line of context.
pub fn diff(want: &str, got: &str) -> String {
    let (w, g): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
    let first = w
        .iter()
        .zip(&g)
        .position(|(a, b)| a != b)
        .unwrap_or(w.len().min(g.len()));
    let from = first.saturating_sub(1);
    let mut out = String::new();
    for (sign, lines) in [("-", &w), ("+", &g)] {
        for line in lines.iter().skip(from).take(6) {
            out.push_str(&format!("    {sign} {line}\n"));
        }
    }
    out
}
