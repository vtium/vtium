//! The run: each screen drawn and judged against its golden in
//! `goldens/`, and the report.
//!
//! Every row of `pending.tsv` is a case that no screen draws yet, with the
//! issue that owes it. A golden that no screen draws is `missing`. A
//! difference from a golden past its sidecar's tolerance is a failure.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::compare::{self, Verdict};
use crate::draw;
use crate::ppm::{self, Image};
use crate::report::{self, CaseReport, Status};
use crate::screens::Case;
use crate::tol::Tolerance;

/// The non-comment lines of a TSV, numbered from 1, split at tabs.
fn rows(text: &str) -> impl Iterator<Item = (usize, Vec<&str>)> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|(i, l)| (i + 1, l.split('\t').collect()))
}

/// A screen that tier 2 owes and that no screen draws yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub golden: String,
    pub issue: String,
    pub why: String,
}

/// Reads `pending.tsv`: `GOLDEN ISSUE WHY` a line.
pub fn pending(text: &str) -> Result<Vec<Pending>, String> {
    let mut seen = BTreeSet::new();
    rows(text)
        .map(|(n, f)| match f[..] {
            [golden, issue, why] if issue.starts_with("vtium/vtium#") => {
                if !seen.insert(golden) {
                    return Err(format!("pending.tsv:{n}: {golden} is listed twice"));
                }
                Ok(Pending {
                    golden: golden.to_owned(),
                    issue: issue.to_owned(),
                    why: why.to_owned(),
                })
            }
            _ => Err(format!(
                "pending.tsv:{n}: want GOLDEN, ISSUE (vtium/vtium#N) and WHY"
            )),
        })
        .collect()
}

/// Reads `provisional.tsv`: `NAME WHY` a line, each a golden of vtium's
/// the operator has not reviewed.
pub fn provisional(text: &str) -> Result<Vec<String>, String> {
    let mut seen = BTreeSet::new();
    rows(text)
        .map(|(n, f)| match f[..] {
            [name, why] if !why.trim().is_empty() => {
                if !seen.insert(name) {
                    return Err(format!("provisional.tsv:{n}: {name} is listed twice"));
                }
                Ok(name.to_owned())
            }
            _ => Err(format!("provisional.tsv:{n}: want NAME and WHY")),
        })
        .collect()
}

/// What a run reads besides the goldens.
#[derive(Clone, Debug)]
pub struct Lists {
    pub pending: Vec<Pending>,
    pub provisional: Vec<String>,
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads the lists of the tool at `root`'s `tools/tier2`.
pub fn load(root: &Path) -> Result<Lists, String> {
    let tool = root.join("tools/tier2");
    Ok(Lists {
        pending: pending(&read(&tool.join("pending.tsv"))?)?,
        provisional: provisional(&read(&tool.join("provisional.tsv"))?)?,
    })
}

/// The names of the `.ppm` files in `dir`.
fn goldens_in(dir: &Path) -> Result<BTreeSet<String>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = BTreeSet::new();
    for e in entries {
        let path = e.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.extension().is_some_and(|x| x == "ppm")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            out.insert(stem.to_owned());
        }
    }
    Ok(out)
}

/// What a run reads and where it writes.
pub struct Run<'a> {
    /// The screens, `screens::CASES` but in the runner's tests.
    pub cases: &'a [Case],
    /// The goldens, `tools/tier2/goldens`.
    pub goldens: &'a Path,
    pub pending: &'a [Pending],
    /// vtium's goldens the operator has not reviewed.
    pub provisional: &'a [String],
    /// Where `actual/` and `diff/` go.
    pub out: &'a Path,
    /// What report paths are relative to.
    pub root: &'a Path,
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

impl Run<'_> {
    /// Checks the lists against the goldens, before any case runs.
    pub fn check_lists(&self) -> Result<(), String> {
        let goldens = goldens_in(self.goldens)?;
        let mut names = BTreeSet::new();
        for c in self.cases {
            if !names.insert(c.screen.name) {
                return Err(format!("two screens are named {}", c.screen.name));
            }
        }
        for p in self.pending {
            if self.cases.iter().any(|c| c.screen.name == p.golden) {
                return Err(format!(
                    "pending.tsv lists {}, which a screen draws",
                    p.golden
                ));
            }
            if goldens.contains(&p.golden) {
                return Err(format!(
                    "pending.tsv lists {}, which has a golden: a screen draws it, or the golden goes",
                    p.golden
                ));
            }
        }
        for p in self.provisional {
            if !self.cases.iter().any(|c| c.screen.name == *p) {
                return Err(format!("provisional.tsv lists {p}, which no screen draws"));
            }
        }
        Ok(())
    }

    /// Draws and judges every case, writing the images; returns the cases'
    /// reports.
    pub fn cases(&self) -> Result<Vec<CaseReport>, String> {
        let actual_dir = self.out.join("actual");
        let diff_dir = self.out.join("diff");
        for d in [&actual_dir, &diff_dir] {
            if d.exists() {
                fs::remove_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
            }
            fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        let mut out = Vec::new();
        for case in self.cases {
            let s = &case.screen;
            let mut r = CaseReport::new(s.name, Status::Error);
            r.description = Some(s.desc.to_owned());
            let golden = self.goldens.join(format!("{}.ppm", s.name));
            r.golden = Some(rel(self.root, &golden));
            let actual = actual_dir.join(format!("{}.ppm", s.name));
            match draw::draw(s) {
                Ok(img) => {
                    write(&actual, &img.to_ppm())?;
                    write(
                        &actual_dir.join(format!("{}.txt", s.name)),
                        format!("{}\n", s.desc).as_bytes(),
                    )?;
                    r.actual = Some(rel(self.root, &actual));
                    if golden.exists() {
                        self.judge(&mut r, &golden, &img, &diff_dir);
                    } else {
                        r.status = Status::Missing;
                        r.note = Some("no golden".to_owned());
                    }
                }
                Err(e) => r.note = Some(format!("not drawn: {e}")),
            }
            out.push(r);
        }
        // the screens tier 2 owes, and goldens no screen draws
        let mut pending = self.pending.to_vec();
        pending.sort_by(|a, b| a.golden.cmp(&b.golden));
        for p in pending {
            let mut r = CaseReport::new(&p.golden, Status::Pending);
            r.note = Some(format!("{}: {}", p.issue, p.why));
            out.push(r);
        }
        for name in goldens_in(self.goldens)? {
            if !self.cases.iter().any(|c| c.screen.name == name) {
                let mut r = CaseReport::new(&name, Status::Missing);
                r.golden = Some(rel(self.root, &self.goldens.join(format!("{name}.ppm"))));
                r.note = Some("a golden that no screen draws".to_owned());
                out.push(r);
            }
        }
        Ok(out)
    }

    /// Judges `img` against the golden at `golden`, with its sidecar.
    fn judge(&self, r: &mut CaseReport, golden: &Path, img: &Image, diff_dir: &Path) {
        let read = || -> Result<(Image, Tolerance), String> {
            let bytes = fs::read(golden).map_err(|e| format!("{}: {e}", golden.display()))?;
            let g = ppm::parse(&bytes).map_err(|e| format!("golden {}: {e}", golden.display()))?;
            let tol_path = golden.with_extension("tol");
            let tol = if tol_path.exists() {
                let text = fs::read_to_string(&tol_path)
                    .map_err(|e| format!("{}: {e}", tol_path.display()))?;
                Tolerance::parse(&text, &tol_path.display().to_string())?
            } else {
                Tolerance::default()
            };
            Ok((g, tol))
        };
        let (g, tol) = match read() {
            Ok(x) => x,
            Err(e) => {
                r.status = Status::Error;
                r.note = Some(e);
                return;
            }
        };
        let c = match compare::compare(&g, img, &tol) {
            Ok(c) => c,
            Err(e) => {
                r.status = Status::Error;
                r.note = Some(e);
                return;
            }
        };
        if !tol.is_exact() {
            r.tolerance = Some(tol.clone());
        }
        r.pixels = c.pixels;
        r.regions = c.regions.clone();
        r.max_delta = c.max_delta;
        let mut notes = Vec::new();
        if let Some(((gw, gh), (aw, ah))) = c.size_mismatch {
            notes.push(format!("size differs: golden {gw}x{gh}, actual {aw}x{ah}"));
        }
        for (m, n) in tol.masks.iter().zip(&c.masked) {
            if *n == 0 {
                notes.push(format!("mask {m} hides no difference"));
            }
        }
        r.status = match c.verdict {
            Verdict::Pass => Status::Pass,
            Verdict::Near => Status::Near,
            Verdict::Fail => Status::Fail,
        };
        if self.provisional.contains(&r.name) {
            notes.push(format!(
                "provisional: not yet reviewed, so not judged; judged, it would be {}",
                r.status.name()
            ));
            r.status = Status::Provisional;
        }
        if c.pixels > 0 && c.size_mismatch.is_none() {
            let diff = diff_dir.join(format!("{}.ppm", r.name));
            if write(&diff, &compare::diff_image(img, &tol, &c.points).to_ppm()).is_ok() {
                r.diff = Some(rel(self.root, &diff));
            }
        }
        if !notes.is_empty() {
            r.note = Some(notes.join("; "));
        }
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// A fresh id for a run, so a comparison can tell two runs from one run
/// saved twice.
#[expect(
    clippy::disallowed_methods,
    reason = "a run's id is the report's, never reaches what is drawn"
)]
pub fn run_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{:016x}{:08x}", nanos as u64, std::process::id())
}

/// The report's JSON, written to `path` after removing any stale one.
pub fn write_report(
    path: &Path,
    tools: &[(&str, String)],
    cases: &[CaseReport],
) -> Result<(), String> {
    write(path, report::json(&run_id(), tools, cases).as_bytes())
}
