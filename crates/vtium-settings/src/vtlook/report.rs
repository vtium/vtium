//! The report of one error in a vtlook, which vtium's log, `vtium
//! check-config` and vtium-config all write from the same fields.
//!
//! References:
//! - ADR-1304 D6: what the report holds, and the log line that names the first error.

use std::fmt;
use std::path::PathBuf;

use vtium_render::filter::Layer;

use crate::bounds::{ERRORS_MAX, QUOTE_MAX};

/// The stage at which a vtlook fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Opening the file and reading its lines: the file is missing, is
    /// refused by the ownership check, is too large, or has a line vtium
    /// cannot read.
    Reading,
    /// Checking what the lines say against the filters' declarations and
    /// the display requirements.
    Checking,
    /// Building the color table.
    Building,
    /// Compiling the GPU path's shader with the vtlook's pieces in it.
    Compiling,
    /// Drawing: a GL error that the GPU path traces to a filter's step.
    Drawing,
}

impl Stage {
    fn word(self) -> &'static str {
        match self {
            Stage::Reading => "reading",
            Stage::Checking => "checking",
            Stage::Building => "building the color table",
            Stage::Compiling => "compiling the shader",
            Stage::Drawing => "drawing",
        }
    }
}

/// The part of a vtlook that an error is in, when it is in one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// The `[requires]` section.
    Requires,
    /// A filter's section: its layer, its name as the file writes it, its
    /// place among the layer's filters from 1, and how many filters the
    /// file gives that layer.
    Filter {
        layer: Layer,
        name: String,
        place: usize,
        of: usize,
    },
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Part::Requires => f.write_str("requires"),
            Part::Filter {
                layer,
                name,
                place,
                of,
            } => write!(f, "{} filter {place} of {of}, {name}", layer.word()),
        }
    }
}

/// One error in a vtlook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// The vtlook's name.
    pub vtlook: String,
    /// The file at fault: the vtlook, or a file it names.
    pub file: PathBuf,
    /// The line at fault, `None` for the file as a whole.
    pub line: Option<usize>,
    /// The section at fault, `None` for a line outside any section.
    pub part: Option<Part>,
    pub stage: Stage,
    /// What is wrong, ending with what vtium needs there. For a value it
    /// names the parameter, the value and what the parameter takes, as
    /// in "period = 12, needs an integer from 2 to 8". At the compiling
    /// and drawing stages it carries the driver's message as it is.
    pub message: String,
}

impl Report {
    /// The file and line, as `vtium check-config` starts each line:
    /// `/etc/vtium/looks/crt.vtlook:5`.
    pub fn at(&self) -> String {
        match self.line {
            Some(n) => format!("{}:{n}", self.file.display()),
            None => self.file.display().to_string(),
        }
    }
}

/// The report's fields after the file and line: the part, the stage and
/// the message, as in "spatial filter 1 of 1, scanlines: checking: period
/// = 12, needs an integer from 2 to 8".
impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(part) = &self.part {
            write!(f, "{part}: ")?;
        }
        write!(f, "{}: {}", self.stage.word(), self.message)
    }
}

/// Why a vtlook fails: its first errors by line, at most [`ERRORS_MAX`],
/// and how many more it holds. A file of 64 KiB can hold tens of
/// thousands of errors, and a check that printed each one would bury the
/// first, so vtium keeps the first ones and counts the rest, as it does
/// for vtium.conf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The errors kept, in the file's order, at least one.
    pub reports: Vec<Report>,
    /// The errors past those kept.
    pub more: usize,
}

impl Failure {
    /// The failure of one error.
    pub(super) fn one(report: Report) -> Failure {
        Failure {
            reports: vec![report],
            more: 0,
        }
    }

    /// The failure of `reports`, sorted by line and cut to the first
    /// [`ERRORS_MAX`], with `more` already dropped before. A stable sort
    /// keeps a line's errors in the order they were found, and an error
    /// with no line, about the file as a whole, comes first.
    pub(super) fn of(mut reports: Vec<Report>, more: usize) -> Failure {
        let more = more.saturating_add(keep_first(&mut reports));
        Failure { reports, more }
    }
}

/// Sorts `reports` by line, keeps the first [`ERRORS_MAX`], and returns
/// how many it dropped.
pub(super) fn keep_first(reports: &mut Vec<Report>) -> usize {
    reports.sort_by_key(|e| e.line.unwrap_or(0));
    let past = reports.len().saturating_sub(ERRORS_MAX);
    reports.truncate(ERRORS_MAX);
    past
}

/// Text from a vtlook as a report quotes it: at most [`QUOTE_MAX`]
/// characters, marked `...` where it is cut, with each control or
/// formatting character, such as a right-to-left override, written as an
/// escape. So a line of the file never reaches a screen or the journal as
/// itself, and a long line cannot make a long report.
pub(super) fn quoted(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    for c in chars.by_ref().take(QUOTE_MAX) {
        match c {
            '"' | '\'' => out.push(c),
            _ => out.extend(c.escape_debug()),
        }
    }
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

/// vtium's one log line for a vtlook that fails on head `head`: the
/// first error whole, how many more there are, and that the head draws
/// the built-in vtlook instead.
pub fn log_line(failure: &Failure, head: &str) -> Option<String> {
    let first = failure.reports.first()?;
    let more = match failure
        .reports
        .len()
        .saturating_sub(1)
        .saturating_add(failure.more)
    {
        0 => String::new(),
        1 => "; 1 more error".to_owned(),
        n => format!("; {n} more errors"),
    };
    Some(format!(
        "vtlook {} ({}): {first}{more}; head {head} draws the built-in vtlook instead",
        first.vtlook,
        first.at()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanlines() -> Report {
        Report {
            vtlook: "crt".to_owned(),
            file: PathBuf::from("/etc/vtium/looks/crt.vtlook"),
            line: Some(5),
            part: Some(Part::Filter {
                layer: Layer::Spatial,
                name: "scanlines".to_owned(),
                place: 1,
                of: 1,
            }),
            stage: Stage::Checking,
            message: "period = 12, needs an integer from 2 to 8".to_owned(),
        }
    }

    // The log line names the vtlook, its file and line, the filter and its
    // place, the stage and the message, then how many more errors there
    // are and what the head draws instead.
    #[test]
    fn the_log_line_names_the_first_error_and_counts_the_rest() {
        let one = Failure::one(scanlines());
        assert_eq!(
            log_line(&one, "console").as_deref(),
            Some(
                "vtlook crt (/etc/vtium/looks/crt.vtlook:5): spatial filter 1 of 1, scanlines: checking: period = 12, needs an integer from 2 to 8; head console draws the built-in vtlook instead"
            )
        );
        let mut requires = scanlines();
        requires.part = Some(Part::Requires);
        requires.message = "min-depth = 12, needs any, 1, 8, 16 or 24".to_owned();
        let three = Failure::of(vec![scanlines(), requires.clone(), requires], 0);
        let line = log_line(&three, "desk").unwrap_or_default();
        assert!(
            line.ends_with("; 2 more errors; head desk draws the built-in vtlook instead"),
            "{line}"
        );
        // the errors past those kept count in the log line too
        let past = Failure {
            reports: vec![scanlines()],
            more: 70,
        };
        let line = log_line(&past, "desk").unwrap_or_default();
        assert!(line.contains("; 70 more errors;"), "{line}");
        let none = Failure {
            reports: Vec::new(),
            more: 0,
        };
        assert_eq!(log_line(&none, "desk"), None);
        assert_eq!(
            three.reports.get(1).map(|r| format!("{}: {r}", r.at())),
            Some(
                "/etc/vtium/looks/crt.vtlook:5: requires: checking: min-depth = 12, needs any, 1, 8, 16 or 24".to_owned()
            )
        );
    }

    // A quote from the file is cut at its bound and escapes what would
    // act on a screen, so neither a long line nor a right-to-left
    // override reaches the journal as itself.
    #[test]
    fn a_quote_from_the_file_is_cut_and_escaped() {
        assert_eq!(quoted("sepia"), "sepia");
        assert_eq!(quoted("say \"hi\" it's"), "say \"hi\" it's");
        assert_eq!(quoted("\u{202e}kool"), "\\u{202e}kool");
        assert_eq!(quoted("a\u{1b}[2Jb"), "a\\u{1b}[2Jb");
        let long = "a".repeat(QUOTE_MAX + 1);
        assert_eq!(quoted(&long), format!("{}...", "a".repeat(QUOTE_MAX)));
        assert_eq!(quoted(&long[1..]), "a".repeat(QUOTE_MAX));
    }

    // At most ERRORS_MAX errors are kept, the first ones by line, and the
    // rest are counted.
    #[test]
    fn a_failure_keeps_the_first_errors_by_line_and_counts_the_rest() {
        let at = |line: usize| Report {
            line: Some(line),
            ..scanlines()
        };
        let reports: Vec<Report> = (1..=ERRORS_MAX + 10).rev().map(at).collect();
        let failure = Failure::of(reports, 5);
        assert_eq!(failure.reports.len(), ERRORS_MAX);
        assert_eq!(failure.more, 15);
        assert_eq!(failure.reports.first().and_then(|r| r.line), Some(1));
        assert_eq!(
            failure.reports.last().and_then(|r| r.line),
            Some(ERRORS_MAX)
        );
    }
}
