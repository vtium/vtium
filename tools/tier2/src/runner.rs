//! The run: each screen drawn and judged against its golden, the C
//! reference's at the pinned kmscon commit or vtium's own, and the report.
//!
//! Every golden of the C reference's tier 2 at the pin is a case: drawn by
//! a screen here, or listed in `pending.tsv` with the issue that owes it,
//! so a golden the pin brings that nobody looked at is `missing`, as is a
//! golden of vtium's that no screen draws. A difference from a golden is a
//! failure unless `departures.tsv` lists the case under a departure the
//! render crate's README states, and every differing pixel lies in the
//! rectangles it names; a listed departure the case no longer shows fails
//! too, until its row goes.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::compare::{self, Verdict};
use crate::cref;
use crate::draw;
use crate::ppm::{self, Image};
use crate::report::{self, CaseReport, Status};
use crate::screens::{Case, Source};
use crate::tol::{Rect, Tolerance};

/// Where the C reference keeps its tier 2 goldens.
pub const GOLDENS: &str = "tests/goldens/tier2";
/// The C reference's tier 2 screens.
pub const SCREENS: &str = "tests/test_pixels.c";

/// A repository read at a pinned commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub name: String,
    pub commit: String,
    pub url: String,
}

/// The non-comment lines of a TSV, numbered from 1, split at tabs.
fn rows(text: &str) -> impl Iterator<Item = (usize, Vec<&str>)> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|(i, l)| (i + 1, l.split('\t').collect()))
}

/// Reads `pin.tsv`: `NAME COMMIT URL` a line.
pub fn pins(text: &str) -> Result<Vec<Pin>, String> {
    rows(text)
        .map(|(n, f)| match f[..] {
            [name, commit, url]
                if commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                Ok(Pin {
                    name: name.to_owned(),
                    commit: commit.to_owned(),
                    url: url.to_owned(),
                })
            }
            _ => Err(format!("pin.tsv:{n}: want NAME, a 40-digit COMMIT and URL")),
        })
        .collect()
}

/// A case whose difference from its golden a departure explains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Departure {
    pub case: String,
    /// The departure's title, in bold under the render crate's README's
    /// `## Departures`.
    pub title: String,
    /// Where the difference may lie.
    pub rects: Vec<Rect>,
}

/// Reads `departures.tsv`: `CASE TITLE x0,y0,x1,y1[;...]` a line.
pub fn departures(text: &str) -> Result<Vec<Departure>, String> {
    let mut seen = BTreeSet::new();
    rows(text)
        .map(|(n, f)| {
            let [case, title, rects] = f[..] else {
                return Err(format!("departures.tsv:{n}: want CASE, TITLE and RECTS"));
            };
            if !seen.insert(case) {
                return Err(format!("departures.tsv:{n}: {case} is listed twice"));
            }
            let rects = rects
                .split(';')
                .map(Rect::parse)
                .collect::<Result<_, _>>()
                .map_err(|e| format!("departures.tsv:{n}: {e}"))?;
            Ok(Departure {
                case: case.to_owned(),
                title: title.to_owned(),
                rects,
            })
        })
        .collect()
}

/// The titles of the departures `readme` lists: each `- **Title**` under
/// its `## Departures` heading.
pub fn readme_departures(readme: &str) -> Vec<String> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in readme.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            inside = h.trim() == "Departures";
        } else if inside {
            if let Some(rest) = line.strip_prefix("- **") {
                if let Some((title, _)) = rest.split_once("**") {
                    out.push(title.to_owned());
                }
            }
        }
    }
    out
}

/// A golden of the C reference's that no screen draws yet.
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

/// Extracts the C reference's tier 2 goldens and screens at `pin` from the
/// checkout at `repo` into `out/<name>-<commit>/`, afresh, through `git
/// archive`, so the checkout's branch and working tree never reach the
/// judge; returns that directory.
pub fn extract(repo: &Path, pin: &Pin, out: &Path) -> Result<PathBuf, String> {
    let dest = out.join(format!("{}-{}", pin.name, pin.commit));
    let has = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "-e", &format!("{}^{{commit}}", pin.commit)])
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("git: {e}"))?;
    if !has.success() {
        return Err(format!(
            "{} has no commit {}: clone {} there, or fetch it (git -C {} fetch origin)",
            repo.display(),
            pin.commit,
            pin.url,
            repo.display()
        ));
    }
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut git = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "archive",
            "--format=tar",
            &pin.commit,
            "--",
            GOLDENS,
            SCREENS,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git archive: {e}"))?;
    let stdout = git.stdout.take().ok_or("git archive: no output")?;
    let tar = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(&dest)
        .stdin(stdout)
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    let git = git.wait().map_err(|e| format!("git archive: {e}"))?;
    if !git.success() || !tar.success() {
        return Err(format!(
            "extracting {GOLDENS} and {SCREENS} at {} failed",
            pin.commit
        ));
    }
    Ok(dest)
}

/// Checks that every case ported from a C screen carries that screen's
/// bytes and second part, as `test_pixels.c` at the pin gives them.
pub fn check_ported(cases: &[Case], test_pixels: &str) -> Result<(), String> {
    let c = cref::screens(test_pixels)?;
    for case in cases {
        let Some(from) = case.ported else { continue };
        let s = &case.screen;
        let want = c
            .get(from)
            .ok_or_else(|| format!("{}: {SCREENS} has no screen {from} to port", s.name))?;
        if s.bytes != want.bytes.as_slice() {
            let at = s
                .bytes
                .iter()
                .zip(&want.bytes)
                .position(|(a, b)| a != b)
                .unwrap_or(s.bytes.len().min(want.bytes.len()));
            return Err(format!(
                "{}: its bytes differ from {from}'s in {SCREENS} at byte {at}",
                s.name
            ));
        }
        if s.next != want.next.as_deref() {
            return Err(format!(
                "{}: its second part differs from {from}'s in {SCREENS}",
                s.name
            ));
        }
    }
    Ok(())
}

/// The names of the `.ppm` files in `dir`.
fn goldens_in(dir: &Path) -> Result<BTreeSet<String>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = BTreeSet::new();
    for e in entries {
        let path = e.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.extension().is_some_and(|x| x == "ppm") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.insert(stem.to_owned());
            }
        }
    }
    Ok(out)
}

/// What a run reads and where it writes.
pub struct Run<'a> {
    /// The screens, `screens::CASES` but in the runner's tests.
    pub cases: &'a [Case],
    /// The C reference's goldens, extracted at the pin.
    pub kmscon_goldens: &'a Path,
    /// vtium's goldens, `tools/tier2/goldens`.
    pub vtium_goldens: &'a Path,
    pub departures: &'a [Departure],
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
    pub fn check_lists(&self, readme_titles: &[String]) -> Result<(), String> {
        let kmscon = goldens_in(self.kmscon_goldens)?;
        let mut names = BTreeSet::new();
        for c in self.cases {
            if !names.insert(c.screen.name) {
                return Err(format!("two screens are named {}", c.screen.name));
            }
            if c.source == Source::Vtium && kmscon.contains(c.screen.name) {
                return Err(format!(
                    "the screen {} has vtium's golden but the C reference's name",
                    c.screen.name
                ));
            }
        }
        for p in self.pending {
            if !kmscon.contains(&p.golden) {
                return Err(format!(
                    "pending.tsv lists {}, which the pin has no golden for",
                    p.golden
                ));
            }
            if self.cases.iter().any(|c| c.screen.name == p.golden) {
                return Err(format!(
                    "pending.tsv lists {}, which a screen draws",
                    p.golden
                ));
            }
        }
        for p in self.provisional {
            if !self
                .cases
                .iter()
                .any(|c| c.source == Source::Vtium && c.screen.name == *p)
            {
                return Err(format!(
                    "provisional.tsv lists {p}, which no screen with vtium's golden draws"
                ));
            }
        }
        for d in self.departures {
            if !self.cases.iter().any(|c| c.screen.name == d.case) {
                return Err(format!(
                    "departures.tsv lists {}, which no screen draws",
                    d.case
                ));
            }
            if !readme_titles.contains(&d.title) {
                return Err(format!(
                    "departures.tsv: {}: the render crate's README lists no departure {:?}",
                    d.case, d.title
                ));
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
            let dir = match case.source {
                Source::Kmscon => self.kmscon_goldens,
                Source::Vtium => self.vtium_goldens,
            };
            let mut r = CaseReport::new(s.name, Status::Error);
            r.description = Some(s.desc.to_owned());
            let golden = dir.join(format!("{}.ppm", s.name));
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
        // goldens no screen draws
        let drawn = |src: Source, name: &str| {
            self.cases
                .iter()
                .any(|c| c.source == src && c.screen.name == name)
        };
        for name in goldens_in(self.kmscon_goldens)? {
            if drawn(Source::Kmscon, &name) {
                continue;
            }
            let golden = self.kmscon_goldens.join(format!("{name}.ppm"));
            let mut r = match self.pending.iter().find(|p| p.golden == name) {
                Some(p) => {
                    let mut r = CaseReport::new(&name, Status::Pending);
                    r.note = Some(format!("{}: {}", p.issue, p.why));
                    r
                }
                None => {
                    let mut r = CaseReport::new(&name, Status::Missing);
                    r.note = Some("a golden of the C reference's that no screen draws and pending.tsv does not list".to_owned());
                    r
                }
            };
            r.golden = Some(rel(self.root, &golden));
            out.push(r);
        }
        for name in goldens_in(self.vtium_goldens)? {
            if !drawn(Source::Vtium, &name) {
                let mut r = CaseReport::new(&name, Status::Missing);
                r.golden = Some(rel(
                    self.root,
                    &self.vtium_goldens.join(format!("{name}.ppm")),
                ));
                r.note = Some("a golden of vtium's that no screen draws".to_owned());
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
        let departure = self.departures.iter().find(|d| d.case == r.name);
        r.status = match (c.verdict, departure) {
            (Verdict::Pass, None) => Status::Pass,
            (Verdict::Pass, Some(d)) => {
                notes.push(format!(
                    "listed under the departure {:?}, which it no longer shows",
                    d.title
                ));
                Status::Fail
            }
            (Verdict::Near, None) => Status::Near,
            (_, Some(d)) if c.size_mismatch.is_none() => {
                r.departure = Some(d.title.clone());
                let outside = c
                    .points
                    .iter()
                    .filter(|&&(x, y)| !d.rects.iter().any(|q| q.contains(x, y)))
                    .count();
                if outside == 0 {
                    Status::Departure
                } else {
                    notes.push(format!(
                        "{outside} differing pixel(s) outside the departure's rectangles"
                    ));
                    Status::Fail
                }
            }
            _ => Status::Fail,
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
