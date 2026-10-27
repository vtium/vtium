#![forbid(unsafe_code)]
//! vtium's side of tier 1.
//!
//! `vtium-replay` meets the C reference's replay interface, libtsm's
//! `test/conformance/INTERFACE.md` (interface 1.0), over `vtium-term`: a
//! byte stream and the test commands in it go in, and the screen or frame
//! view comes out as text. `conformance` replays every case of the C
//! reference's corpus at the commit `pin.tsv` names and judges each against
//! its golden, expecting vtium's own output for exactly the cases
//! `expected.tsv` lists. The crate's README describes both.

pub mod command;
pub mod coverage;
pub mod palettes;
pub mod replay;
pub mod runner;
pub mod view;

/// Why a replay printed no result: exit 2 for a usage failure, 3 for one
/// the implementation cannot print, with what it printed before it stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: u8,
    pub message: String,
    /// The blocks and test lines printed before the failure, which the
    /// command line prints as it goes, as the C reference did.
    pub output: String,
}

pub(crate) fn usage(message: impl Into<String>) -> Failure {
    Failure {
        code: 2,
        message: message.into(),
        output: String::new(),
    }
}

pub(crate) fn internal(message: impl Into<String>) -> Failure {
    Failure {
        code: 3,
        message: message.into(),
        output: String::new(),
    }
}

/// What `--describe` prints: the interface, the views, the features and
/// the commands vtium offers, in INTERFACE.md's order. vtium is built
/// without images and without the draw2 view, and declines `damage-exact`.
pub fn describe() -> String {
    let features = [
        "attrs-extended",
        "view-frame",
        "sync",
        "identify",
        "input",
        "focus",
        "palette",
        "query",
    ];
    let mut out = String::from("interface 1.0\nview screen\nview frame\n");
    for f in features {
        out.push_str("feature ");
        out.push_str(f);
        out.push('\n');
    }
    for spec in command::SPECS.iter().filter(|s| s.offered) {
        out.push_str("command ");
        out.push_str(spec.name);
        if let Some(f) = spec.feature {
            out.push(' ');
            out.push_str(f);
        }
        out.push('\n');
    }
    out
}

/// Runs the command line, `[--frame] COLS ROWS FILE [CUTS]` or
/// `--describe`, and returns what it prints.
pub fn main(args: &[String]) -> Result<String, Failure> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if args == ["--describe"] {
        return Ok(describe());
    }
    let (view, rest) = match args.split_first() {
        Some((&"--frame", rest)) => (view::View::Frame, rest),
        Some((&"--draw2", _)) => return Err(usage("vtium has no draw2 view")),
        _ => (view::View::Screen, &args[..]),
    };
    let [cols, rows, file, cuts @ ..] = rest else {
        return Err(usage(
            "usage: vtium-replay [--frame] COLS ROWS FILE [CUTS]\n       vtium-replay --describe",
        ));
    };
    let (Some(cols), Some(rows)) = (
        replay::size(cols, replay::COLS_MAX),
        replay::size(rows, replay::ROWS_MAX),
    ) else {
        return Err(usage(format!(
            "size {cols}x{rows} is not 1-{} by 1-{}",
            replay::COLS_MAX,
            replay::ROWS_MAX
        )));
    };
    let read = |path: &str| std::fs::read(path).map_err(|e| usage(format!("{path}: {e}")));
    let data = read(file)?;
    let cuts = match cuts {
        [] => Vec::new(),
        [path] => replay::cuts(&read(path)?, data.len())?,
        _ => return Err(usage("too many arguments")),
    };
    replay::run(&replay::Input {
        view,
        cols,
        rows,
        data,
        cuts,
        fixtures: replay::Input::fixtures_for(std::path::Path::new(file)),
    })
}
