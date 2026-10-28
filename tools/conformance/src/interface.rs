//! The interface check: vtium-replay and the copied goldens against the
//! interface's own blocks, beside the verdict on each case.
//!
//! Every line of every golden is a form of its kind in the grammar, the
//! kinds come in the order of the view's `structure`, and each block keeps
//! the numbering rules of INTERFACE.md's "Blocks"; every worked example
//! replays to the output the interface shows, but one that needs a
//! feature or a view vtium-replay does not declare; the frame view
//! resolves each palette of the `palettes` block to its row; each
//! malformed or edge input of the refusals table exits as the interface
//! gives; and the grammar rejects outputs known to be bad and accepts one
//! known to be good, so a grammar grown too loose or too strict shows. The
//! C reference ran the same checks from `test/conformance/interface-check.py`;
//! `--describe` is checked by the runner (`check_describe`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::probes::unquote;
use crate::replay::{self, Input};
use crate::runner::Corpus;
use crate::spec::{Spec, fenced};
use crate::view::View;

mod refusals;

pub use refusals::{exit_code, refusals};

/// The copied goldens by view; the draw2 goldens were not copied, as
/// vtium has no draw2 view.
const GOLDENS: [(&str, &str); 2] = [("screen", "golden"), ("frame", "golden-frame")];

/// What one check counted, and the faults it found.
#[derive(Clone, Debug, Default)]
pub struct Tally {
    pub count: usize,
    /// Worked examples left out for a feature or view not declared.
    pub skipped: usize,
    pub faults: Vec<String>,
}

/// Every check's tally.
#[derive(Clone, Debug, Default)]
pub struct Check {
    pub goldens: usize,
    pub golden_faults: Vec<String>,
    pub examples: Tally,
    pub palettes: Tally,
    pub refusals: Tally,
    pub controls: Tally,
}

impl Check {
    pub fn faults(&self) -> Vec<String> {
        [
            &self.golden_faults,
            &self.examples.faults,
            &self.palettes.faults,
            &self.refusals.faults,
            &self.controls.faults,
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect()
    }

    /// The line a run prints when every check holds.
    pub fn summary(&self) -> String {
        let skipped = match self.examples.skipped {
            0 => String::new(),
            n => format!(", {n} left out for features or views not declared"),
        };
        format!(
            "replay-interface: {} goldens parse; {} examples replay as shown{skipped}; \
             {} palettes match; {} refusals exit as given; {} grammar controls hold",
            self.goldens,
            self.examples.count,
            self.palettes.count,
            self.refusals.count,
            self.controls.count
        )
    }
}

/// Runs every check: `texts` are the interface's documents, in order,
/// `describe` what vtium-replay's `--describe` prints.
pub fn check(spec: &Spec, texts: &[&str], corpus: &Corpus, describe: &str) -> Check {
    let declared: BTreeSet<String> = describe
        .lines()
        .filter_map(|l| l.strip_prefix("feature "))
        .map(str::to_owned)
        .collect();
    let (goldens, golden_faults) = goldens(spec, corpus);
    Check {
        goldens,
        golden_faults,
        examples: examples(texts, spec, &declared),
        palettes: palettes(texts),
        refusals: refusals(exit_code),
        controls: controls(spec),
    }
}

/// Every golden of the copied views against the grammar, sized by
/// `cases.list`.
fn goldens(spec: &Spec, corpus: &Corpus) -> (usize, Vec<String>) {
    let sizes: BTreeMap<&str, (usize, usize)> = corpus
        .cases
        .iter()
        .map(|c| (c.name.as_str(), (c.cols, c.rows)))
        .collect();
    let mut faults = Vec::new();
    let mut count = 0usize;
    for (view, dir) in GOLDENS {
        let mut files: Vec<PathBuf> = match std::fs::read_dir(corpus.root.join(dir)) {
            Ok(entries) => entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "txt"))
                .collect(),
            Err(e) => {
                faults.push(format!("{dir}/: {e}"));
                continue;
            }
        };
        files.sort();
        if files.is_empty() {
            faults.push(format!("no goldens in {dir}/"));
        }
        for path in files {
            count = count.saturating_add(1);
            let file = path.file_name().unwrap_or_default().to_string_lossy();
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let Some(&(cols, rows)) = sizes.get(stem.as_ref()) else {
                faults.push(format!("{dir}/{file}: no case in cases.list"));
                continue;
            };
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(e) => {
                    faults.push(format!("{dir}/{file}: {e}"));
                    continue;
                }
            };
            if let Some(fault) = check_output(spec, view, &text, cols, rows, false) {
                faults.push(format!("{dir}/{file}: {fault}"));
            }
        }
    }
    (count, faults)
}

/// The first fault of `text`, an output of `view` replayed at `cols` by
/// `rows`, or None: a line no form of its kind in the view allows (the
/// reserved keys allowed only when `with_reserved`), the kinds out of the
/// view's structure, or a break of the numbering rules.
pub fn check_output(
    spec: &Spec,
    view: &str,
    text: &str,
    cols: usize,
    rows: usize,
    with_reserved: bool,
) -> Option<String> {
    let Some(body) = text.strip_suffix('\n') else {
        return Some("does not end in a newline".to_owned());
    };
    let lines: Vec<&str> = body.split('\n').collect();
    let mut kinds = String::new();
    for (n, line) in lines.iter().enumerate() {
        let kind = line.split(' ').next().unwrap_or_default();
        if !spec.parses(line, view, with_reserved) {
            return Some(format!(
                "line {}: not a {kind} line of the {view} view: {line:?}",
                n.saturating_add(1)
            ));
        }
        kinds.push(' ');
        kinds.push_str(kind);
    }
    match spec.structured(view, &kinds) {
        None => return Some(format!("the grammar gives the {view} view no structure")),
        Some(false) => return Some("lines out of the order the structure gives".to_owned()),
        Some(true) => {}
    }
    numbering(&lines, view, cols, rows)
}

/// The numbering rules of INTERFACE.md's "Blocks": rows from 0 in order,
/// as many as the size; each run on its row, in order, apart from the run
/// before it and within the columns; placements and images numbered from
/// 0 after the last row; checkpoints not decreasing. The screen view is
/// printed at `cols` by `rows`, the command line's; the others at their
/// `size` line's.
pub fn numbering(lines: &[&str], view: &str, cols: usize, rows: usize) -> Option<String> {
    let (mut cols, mut rows) = (cols, rows);
    // the row of the block so far: None before any block, -1 before its first row
    let mut row: Option<i64> = None;
    let mut last: (i64, Vec<&str>) = (-1, Vec::new());
    let mut checkpoint: i64 = -1;
    let (mut placement, mut image) = (0i64, 0i64);
    let num = |s: Option<&&str>| s.and_then(|s| s.parse::<i64>().ok());
    for (n, line) in lines.iter().enumerate() {
        let n = n.saturating_add(1);
        let f: Vec<&str> = line.split(' ').collect();
        let kind = f.first().copied().unwrap_or_default();
        let last_row = i64::try_from(rows).unwrap_or(i64::MAX).saturating_sub(1);
        let unfinished = row.is_some_and(|r| r != last_row);
        if matches!(kind, "checkpoint" | "size" | "test") && unfinished {
            return Some(format!(
                "line {n}: a block ends after row {} of {rows}",
                row.unwrap_or_default()
            ));
        }
        match kind {
            "checkpoint" => {
                let at = num(f.get(1))?;
                if at < checkpoint {
                    return Some(format!("line {n}: checkpoint before the one above it"));
                }
                checkpoint = at;
            }
            "size" => {
                if view != "screen" {
                    let (c, r) = f.get(1)?.split_once('x')?;
                    (cols, rows) = (c.parse().ok()?, r.parse().ok()?);
                }
                row = Some(-1);
                (placement, image) = (0, 0);
            }
            "row" => {
                let r = row.unwrap_or(-1).saturating_add(1);
                row = Some(r);
                let rows = i64::try_from(rows).unwrap_or(i64::MAX);
                if num(f.get(1)) != Some(r) || r >= rows {
                    return Some(format!(
                        "line {n}: row {} where row {r} of {rows} belongs",
                        f.get(1).unwrap_or(&"")
                    ));
                }
                last = (-1, Vec::new());
            }
            "attr" | "cells" => {
                let run = f.get(2).copied().unwrap_or_default();
                let (a, b) = run.split_once('-')?;
                let (a, b): (i64, i64) = (a.parse().ok()?, b.parse().ok()?);
                let cols = i64::try_from(cols).unwrap_or(i64::MAX);
                let r = row.unwrap_or(-1);
                if num(f.get(1)) != Some(r) || a <= last.0 || b < a || b >= cols {
                    return Some(format!(
                        "line {n}: run {run} out of order or off row {r} of {cols} cells"
                    ));
                }
                let keys = f.get(3..).unwrap_or_default().to_vec();
                if a == last.0.saturating_add(1) && keys == last.1 {
                    return Some(format!("line {n}: run {run} continues the run before it"));
                }
                last = (b, keys);
            }
            "placement" | "image" => {
                let next = if kind == "placement" {
                    &mut placement
                } else {
                    &mut image
                };
                if num(f.get(1)) != Some(*next) {
                    return Some(format!(
                        "line {n}: {kind} {} where {kind} {next} belongs",
                        f.get(1).unwrap_or(&"")
                    ));
                }
                *next = next.saturating_add(1);
                if row != Some(last_row) {
                    return Some(format!("line {n}: {kind} before the last row"));
                }
            }
            _ => {}
        }
    }
    let last_row = i64::try_from(rows).unwrap_or(i64::MAX).saturating_sub(1);
    match row {
        Some(r) if r != last_row => Some(format!("the output ends after row {r} of {rows}")),
        _ => None,
    }
}

/// The fenced blocks of `text` whose info line is `kind NAME`, by name.
fn named_blocks(text: &str, kind: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(info) = line.strip_prefix("```") else {
            continue;
        };
        let mut body = String::new();
        for l in lines.by_ref() {
            if l == "```" {
                break;
            }
            body.push_str(l);
            body.push('\n');
        }
        if let [k, name] = info.split_whitespace().collect::<Vec<_>>()[..] {
            if k == kind {
                out.push((name.to_owned(), body));
            }
        }
    }
    out
}

/// A worked example's command line and stream.
struct Example {
    cols: usize,
    rows: usize,
    view: String,
    needs: BTreeSet<String>,
    data: Vec<u8>,
    cuts: Vec<usize>,
}

fn example(body: &str) -> Result<Example, String> {
    let mut size = None;
    let mut e = Example {
        cols: 0,
        rows: 0,
        view: "screen".to_owned(),
        needs: BTreeSet::new(),
        data: Vec::new(),
        cuts: Vec::new(),
    };
    for line in body.lines() {
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        match word {
            "size" => {
                let mut v = rest.split_whitespace().map(str::parse::<usize>);
                size = match (v.next(), v.next(), v.next()) {
                    (Some(Ok(c)), Some(Ok(r)), None) => Some((c, r)),
                    _ => return Err(format!("size takes COLS ROWS: {line:?}")),
                };
            }
            "view" => rest.clone_into(&mut e.view),
            "needs" => e.needs.extend(rest.split_whitespace().map(str::to_owned)),
            "send" => e.data.extend(
                unquote(rest).ok_or_else(|| format!("send takes a quoted string: {line:?}"))?,
            ),
            "cmd" => {
                e.data.extend_from_slice(b"\x1b_tsm-test;");
                e.data.extend_from_slice(rest.as_bytes());
                e.data.extend_from_slice(b"\x1b\\");
            }
            "check" => e.cuts.push(e.data.len()),
            _ => return Err(format!("example line {line:?}")),
        }
    }
    (e.cols, e.rows) = size.ok_or("an example has no size")?;
    Ok(e)
}

/// Each worked example of `texts` replayed in-process and compared with
/// its output block; one that needs a feature not in `declared`, or a view
/// vtium-replay lacks, is counted as skipped.
pub fn examples(texts: &[&str], spec: &Spec, declared: &BTreeSet<String>) -> Tally {
    let mut tally = Tally::default();
    let outputs: BTreeMap<String, String> = texts
        .iter()
        .flat_map(|t| named_blocks(t, "output"))
        .collect();
    let examples: Vec<(String, String)> = texts
        .iter()
        .flat_map(|t| named_blocks(t, "example"))
        .collect();
    for name in outputs.keys() {
        if !examples.iter().any(|(n, _)| n == name) {
            tally
                .faults
                .push(format!("output {name}: no example block"));
        }
    }
    for (name, body) in &examples {
        let e = match example(body) {
            Ok(e) => e,
            Err(why) => {
                tally.faults.push(format!("example {name}: {why}"));
                continue;
            }
        };
        let view = match e.view.as_str() {
            "screen" => View::Screen,
            "frame" => View::Frame,
            _ => {
                tally.skipped = tally.skipped.saturating_add(1);
                continue;
            }
        };
        if !e.needs.is_subset(declared) {
            tally.skipped = tally.skipped.saturating_add(1);
            continue;
        }
        let out = replay::run(&Input {
            view,
            cols: e.cols,
            rows: e.rows,
            data: e.data,
            cuts: e.cuts,
            fixtures: PathBuf::from("/nonexistent"),
        });
        let out = match out {
            Ok(out) => out,
            Err(f) => {
                tally
                    .faults
                    .push(format!("example {name}: exited {}: {}", f.code, f.message));
                continue;
            }
        };
        tally.count = tally.count.saturating_add(1);
        match outputs.get(name) {
            None => tally
                .faults
                .push(format!("example {name}: no output block")),
            Some(want) if *want != out => tally
                .faults
                .push(format!("example {name}: replays to\n{out}")),
            Some(_) => {}
        }
        if let Some(fault) = check_output(spec, &e.view, &out, e.cols, e.rows, false) {
            tally.faults.push(format!("example {name}: {fault}"));
        }
    }
    if tally.count == 0 {
        tally.faults.push("no worked example ran".to_owned());
    }
    tally
}

/// Each palette of the `palettes` blocks of `texts`: the frame view,
/// under it, resolves the sixteen colors and the default foreground and
/// background to its row.
pub fn palettes(texts: &[&str]) -> Tally {
    let mut tally = Tally::default();
    let rows: Vec<Vec<&str>> = texts
        .iter()
        .filter_map(|t| fenced(t, "palettes"))
        .flatten()
        .filter(|l| !l.is_empty())
        .map(|l| l.split_whitespace().collect())
        .collect();
    if rows.is_empty() {
        tally.faults.push("no palettes block".to_owned());
    }
    for row in rows {
        tally.count = tally.count.saturating_add(1);
        let [name, colors @ ..] = &row[..] else {
            continue;
        };
        if colors.len() != 18 {
            tally
                .faults
                .push(format!("palette {name}: {} colors, not 18", colors.len()));
            continue;
        }
        match resolved(name) {
            Ok(got) if got == colors => {}
            Ok(got) => tally.faults.push(format!(
                "palette {name}: the frame view resolves {}",
                got.join(" ")
            )),
            Err(why) => tally.faults.push(format!("palette {name}: {why}")),
        }
    }
    tally
}

/// The sixteen colors, the default foreground and the default background
/// vtium's frame view resolves under palette `name`.
fn resolved(name: &str) -> Result<Vec<String>, String> {
    let mut data = format!("\x1b_tsm-test;palette;name={name}\x1b\\").into_bytes();
    for i in 0..16u8 {
        let sgr = if i < 8 { 30 + i } else { 82 + i };
        data.extend_from_slice(format!("\x1b[{sgr}mx").as_bytes());
    }
    let out = replay::run(&Input {
        view: View::Frame,
        cols: 16,
        rows: 1,
        data,
        cuts: Vec::new(),
        fixtures: PathBuf::from("/nonexistent"),
    })
    .map_err(|f| format!("the replay exited {}: {}", f.code, f.message))?;
    let pal = out
        .lines()
        .find_map(|l| l.strip_prefix("palette "))
        .ok_or("the frame view prints no palette line")?;
    let mut words = pal.split(' ');
    let (shown, fg, bg) = (
        words.next().unwrap_or_default(),
        words.next().and_then(|w| w.strip_prefix("fg=#")),
        words.next().and_then(|w| w.strip_prefix("bg=#")),
    );
    let (Some(fg), Some(bg)) = (fg, bg) else {
        return Err(format!("palette line {pal:?}"));
    };
    if shown != name {
        return Err(format!("the frame view shows palette {shown}"));
    }
    let mut got = vec![fg.to_owned(); 16];
    for line in out.lines() {
        let Some(rest) = line.strip_prefix("cells 0 ") else {
            continue;
        };
        let mut w = rest.split(' ');
        let (Some(run), Some("w=1"), Some(color)) = (
            w.next(),
            w.next(),
            w.next().and_then(|c| c.strip_prefix("fg=#")),
        ) else {
            continue;
        };
        let Some((a, b)) = run.split_once('-') else {
            continue;
        };
        let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) else {
            continue;
        };
        for slot in got.iter_mut().take(b.saturating_add(1)).skip(a) {
            color.clone_into(slot);
        }
    }
    got.extend([fg.to_owned(), bg.to_owned()]);
    Ok(got)
}

/// An output the grammar must accept, checked as the screen view of a 4
/// by 2 screen.
const GOOD: &str = "size 4x2\ncursor 0,0\nflags none\nrow 0 |ab|\nattr 0 0-1 bold\nrow 1 ||\n";

/// Outputs the grammar must reject, so that a check grown too loose
/// fails, and the good output, so that one grown too strict does; and a
/// reserved key, accepted only by the grammar with them allowed.
pub fn controls(spec: &Spec) -> Tally {
    let bad = [
        ("cursor X Y", GOOD.replace("cursor 0,0", "cursor 0 0")),
        ("a missing row", GOOD.replace("row 1 ||\n", "")),
        ("a row past the size", format!("{GOOD}row 2 ||\n")),
        (
            "a split run",
            GOOD.replace("attr 0 0-1 bold", "attr 0 0-0 bold\nattr 0 1-1 bold"),
        ),
        (
            "keys out of order",
            GOOD.replace("attr 0 0-1 bold", "attr 0 0-1 bold fg=1"),
        ),
        (
            "a run past the columns",
            GOOD.replace("attr 0 0-1", "attr 0 3-4"),
        ),
        (
            "a reserved key",
            GOOD.replace("attr 0 0-1 bold", "attr 0 0-1 strike overline"),
        ),
        (
            "a frame line",
            GOOD.replace("flags none\n", "flags none\nchanged 8\n"),
        ),
        (
            "flags out of order",
            GOOD.replace("flags none", "flags AUTO_WRAP|INSERT_MODE"),
        ),
        (
            "a color code past 15",
            GOOD.replace("attr 0 0-1 bold", "attr 0 0-1 fg=16"),
        ),
        ("no final newline", GOOD.trim_end_matches('\n').to_owned()),
        (
            "a block cut short",
            format!(
                "checkpoint 0\n{}checkpoint 1\n{GOOD}",
                GOOD.replace("row 1 ||\n", "")
            ),
        ),
    ];
    let mut tally = Tally {
        count: bad.len().saturating_add(2),
        ..Tally::default()
    };
    if let Some(fault) = check_output(spec, "screen", GOOD, 4, 2, false) {
        tally.faults.push(format!(
            "control: the grammar rejects a good output: {fault}"
        ));
    }
    for (what, text) in &bad {
        if check_output(spec, "screen", text, 4, 2, false).is_none() {
            tally
                .faults
                .push(format!("control: the grammar accepts {what}"));
        }
    }
    let reserved = GOOD.replace("attr 0 0-1 bold", "attr 0 0-1 strike overline");
    if check_output(spec, "screen", &reserved, 4, 2, true).is_some() {
        tally
            .faults
            .push("control: the reserved grammar rejects a reserved key".to_owned());
    }
    tally
}
