//! The report of one error in a vtlook, which vtium's log, `vtium
//! check-config` and vtium-config all write from the same fields.
//!
//! References:
//! - ADR-1304 D6: what the report holds, and the log line that names the first error.

use std::fmt;
use std::path::PathBuf;

use vtium_render::filter::Layer;

use super::BUILT_IN;

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

/// vtium's one log line for a vtlook that fails on head `head`: the
/// first error whole, how many more there are, and that the head draws
/// the built-in vtlook. `None` when `reports` is empty.
pub fn log_line(reports: &[Report], head: &str) -> Option<String> {
    let first = reports.first()?;
    let more = match reports.len().saturating_sub(1) {
        0 => String::new(),
        1 => "; 1 more error".to_owned(),
        n => format!("; {n} more errors"),
    };
    Some(format!(
        "vtlook {} ({}): {first}{more}; head {head} draws the vtlook {BUILT_IN}",
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
        let one = [scanlines()];
        assert_eq!(
            log_line(&one, "console").as_deref(),
            Some(
                "vtlook crt (/etc/vtium/looks/crt.vtlook:5): spatial filter 1 of 1, scanlines: checking: period = 12, needs an integer from 2 to 8; head console draws the vtlook default (built in)"
            )
        );
        let mut requires = scanlines();
        requires.part = Some(Part::Requires);
        requires.message = "min-depth = 12, needs any, 1, 8, 16 or 24".to_owned();
        let three = [scanlines(), requires.clone(), requires];
        let line = log_line(&three, "desk").unwrap_or_default();
        assert!(
            line.ends_with("; 2 more errors; head desk draws the vtlook default (built in)"),
            "{line}"
        );
        assert_eq!(log_line(&[], "desk"), None);
        assert_eq!(
            format!("{}: {}", three[1].at(), three[1]),
            "/etc/vtium/looks/crt.vtlook:5: requires: checking: min-depth = 12, needs any, 1, 8, 16 or 24"
        );
    }
}
