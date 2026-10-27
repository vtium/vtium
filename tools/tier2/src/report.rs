//! The report: one JSON shape for tiers 2 and 3, the C reference's tier 2
//! report (`tests/tier2/run.py`) with tier 3's fields where they meet
//! (aaronsb/kmscon#18).
//!
//! ```text
//! {"tier": 2, "run_id": "...", "tools": {...}, "cases": [
//!   {"name", "status", "description", "golden", "actual", "diff",
//!    "tolerance": null | {"max_delta", "max_pixels", "masks"},
//!    "stats": {"pixels", "bbox", "max_delta"}, "note", "departure"}]}
//! ```
//!
//! Statuses: `pass` (identical outside the masks), `near` (within the
//! tolerance), `departure` (differs only where a listed departure says),
//! `pending` (a golden of the C reference's that vtium cannot draw yet,
//! with the issue that owes it), `provisional` (a golden of vtium's the
//! operator has not reviewed: compared, its difference noted, never
//! failing), `fail`, `missing` (no golden, or a golden
//! no screen draws) and `error` (a case that could not be compared). The
//! run fails on `fail`, `missing` and `error`.

use std::fmt::Write as _;

use crate::tol::{Rect, Tolerance};

/// A case's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Near,
    Departure,
    Pending,
    Provisional,
    Fail,
    Missing,
    Error,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Near => "near",
            Status::Departure => "departure",
            Status::Pending => "pending",
            Status::Provisional => "provisional",
            Status::Fail => "fail",
            Status::Missing => "missing",
            Status::Error => "error",
        }
    }

    /// Whether the run passes with a case of this status.
    pub fn passes(self) -> bool {
        matches!(
            self,
            Status::Pass | Status::Near | Status::Departure | Status::Pending | Status::Provisional
        )
    }
}

/// One case of the report.
#[derive(Clone, Debug)]
pub struct CaseReport {
    pub name: String,
    pub status: Status,
    pub description: Option<String>,
    pub golden: Option<String>,
    pub actual: Option<String>,
    pub diff: Option<String>,
    pub tolerance: Option<Tolerance>,
    pub pixels: u64,
    pub regions: Vec<Rect>,
    pub max_delta: u8,
    pub note: Option<String>,
    pub departure: Option<String>,
}

impl CaseReport {
    pub fn new(name: &str, status: Status) -> CaseReport {
        CaseReport {
            name: name.to_owned(),
            status,
            description: None,
            golden: None,
            actual: None,
            diff: None,
            tolerance: None,
            pixels: 0,
            regions: Vec::new(),
            max_delta: 0,
            note: None,
            departure: None,
        }
    }
}

/// `s` as a JSON string.
pub fn string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opt(s: &Option<String>) -> String {
    s.as_deref().map_or_else(|| "null".to_owned(), string)
}

fn rects(r: &[Rect]) -> String {
    let parts: Vec<String> = r
        .iter()
        .map(|b| format!("[{}, {}, {}, {}]", b.x0, b.y0, b.x1, b.y1))
        .collect();
    format!("[{}]", parts.join(", "))
}

/// The report's JSON: `tools` holds each name and value given.
pub fn json(run_id: &str, tools: &[(&str, String)], cases: &[CaseReport]) -> String {
    let mut out = format!(
        "{{\n  \"tier\": 2,\n  \"run_id\": {},\n  \"tools\": {{",
        string(run_id)
    );
    let tools: Vec<String> = tools
        .iter()
        .map(|(k, v)| format!("{}: {}", string(k), string(v)))
        .collect();
    out.push_str(&tools.join(", "));
    out.push_str("},\n  \"cases\": [");
    for (i, c) in cases.iter().enumerate() {
        let tol = c.tolerance.as_ref().map_or_else(
            || "null".to_owned(),
            |t| {
                format!(
                    "{{\"max_delta\": {}, \"max_pixels\": {}, \"masks\": {}}}",
                    t.max_delta,
                    t.max_pixels,
                    rects(&t.masks)
                )
            },
        );
        let _ = write!(
            out,
            "{}\n    {{\"name\": {}, \"status\": {}, \"description\": {}, \"golden\": {}, \
             \"actual\": {}, \"diff\": {}, \"tolerance\": {}, \"stats\": {{\"pixels\": {}, \
             \"bbox\": {}, \"max_delta\": {}}}, \"note\": {}, \"departure\": {}}}",
            if i == 0 { "" } else { "," },
            string(&c.name),
            string(c.status.name()),
            opt(&c.description),
            opt(&c.golden),
            opt(&c.actual),
            opt(&c.diff),
            tol,
            c.pixels,
            rects(&c.regions),
            c.max_delta,
            opt(&c.note),
            opt(&c.departure),
        );
    }
    out.push_str("\n  ]\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        assert_eq!(string("a\"b\\c\nd\u{1}é"), "\"a\\\"b\\\\c\\nd\\u0001é\"");
    }

    #[test]
    fn a_report_names_its_cases() {
        let mut c = CaseReport::new("x", Status::Fail);
        c.regions = vec![Rect {
            x0: 1,
            y0: 2,
            x1: 3,
            y1: 4,
        }];
        let j = json("id", &[("vtium", "abc".to_owned())], &[c]);
        assert!(j.contains("\"status\": \"fail\""));
        assert!(j.contains("\"bbox\": [[1, 2, 3, 4]]"));
        assert!(j.contains("\"tools\": {\"vtium\": \"abc\"}"));
    }
}
