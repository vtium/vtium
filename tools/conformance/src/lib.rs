#![forbid(unsafe_code)]
//! vtium's side of tier 1.
//!
//! `vtium-replay` meets the replay interface, interface 1.3:
//! `corpus/INTERFACE.md` with vtium's additions in `INTERFACE-vtium.md`,
//! over `vtium-term`: a byte stream and the test commands
//! in it go in, and the screen or frame view comes out as text.
//! `conformance` replays every case of the corpus and judges each against
//! its golden, plays every behaviour scenario and judges each against its
//! transcript, and judges the capability probes against
//! `capabilities.list`. The crate's README describes each.

pub mod command;
pub mod coverage;
pub mod generate;
#[cfg(feature = "images")]
pub mod host;
pub mod interface;
pub mod probes;
#[cfg(feature = "record")]
pub mod record;
pub mod replay;
pub mod runner;
pub mod scenario;
pub mod spec;
pub mod view;

/// Why a replay printed no result: exit 2 for a usage failure, 3 for one
/// the implementation cannot print, with what it printed before it stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: u8,
    pub message: String,
    /// The blocks and test lines printed before the failure, which the
    /// command line prints as it goes.
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
/// the commands vtium offers, in the order of the interface's describe
/// blocks. vtium has no draw2 view, offers the image features and
/// `kitty-animation` in a build with images, the display view and `bidi`
/// in a build with the bidi feature, and declines `damage-exact`.
pub fn describe() -> String {
    // 1.0's image features, and vtium's kitty-animation
    let (images, animation): (&[&str], &[&str]) = if command::IMAGES {
        (
            &["images", "sixel", "kitty-graphics", "image-host"],
            &["kitty-animation"],
        )
    } else {
        (&[], &[])
    };
    let features = ["attrs-extended", "view-frame"]
        .iter()
        .chain(images)
        .chain(&[
            "sync",
            "identify",
            "input",
            "focus",
            "palette",
            "query",
            "clipboard",
        ])
        .chain(animation)
        .chain(if BIDI { &["bidi"][..] } else { &[] });
    let mut out = String::from("interface 1.3\nview screen\nview frame\n");
    if BIDI {
        out.push_str("view display\n");
    }
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

/// Whether vtium-replay is built with the bidi feature, which gives it the
/// display view.
pub const BIDI: bool = cfg!(feature = "bidi");

/// Runs the command line, `[--frame | --display] COLS ROWS FILE [CUTS]`
/// or `--describe`, and returns what it prints.
pub fn main(args: &[String]) -> Result<String, Failure> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if args == ["--describe"] {
        return Ok(describe());
    }
    let (view, rest) = match args.split_first() {
        Some((&"--frame", rest)) => (view::View::Frame, rest),
        #[cfg(feature = "bidi")]
        Some((&"--display", rest)) => (view::View::Display, rest),
        #[cfg(not(feature = "bidi"))]
        Some((&"--display", _)) => {
            return Err(usage(
                "this vtium-replay has no display view: it is built without the bidi feature",
            ));
        }
        Some((&"--draw2", _)) => return Err(usage("vtium has no draw2 view")),
        _ => (view::View::Screen, &args[..]),
    };
    let [cols, rows, file, cuts @ ..] = rest else {
        return Err(usage(
            "usage: vtium-replay [--frame | --display] COLS ROWS FILE [CUTS]\n       vtium-replay --describe",
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
