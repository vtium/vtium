//! Draws each of vtium-config's TUI screen goldens as a PNG, the way a
//! vtium console shows it:
//!
//! ```text
//! tui-shots BYTES OUT
//! ```
//!
//! BYTES holds what the golden tests write under `VTIUM_CONFIG_SHOTS`: for
//! each golden `NAME-COLSxROWS`, `NAME-COLSxROWS.bytes`, every byte the TUI
//! wrote to reach the screen, and `NAME-COLSxROWS.txt`, the screen as the
//! golden test read it. Each byte stream goes through vtium-term and
//! vtium-render in unifont, as tier 2 draws its screens, into
//! `OUT/NAME-COLSxROWS.png`. Before it writes a picture, the tool plays the
//! same bytes into a terminal of its own and checks that its screen reads
//! as the `.txt` does, so a PNG never shows a state other than the one its
//! golden holds. It fails when BYTES holds no screen.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_specimens::draw::Image;
use vtium_term::{Glyph, Terminal};
use vtium_tier2::draw::{Screen, draw};

/// The grid a golden's name ends in, as `-100x30`.
fn grid(name: &str) -> Option<(usize, usize)> {
    let (_, size) = name.rsplit_once('-')?;
    let (cols, rows) = size.split_once('x')?;
    Some((cols.parse().ok()?, rows.parse().ok()?))
}

/// The screen `bytes` leave in a terminal of `cols` by `rows`, one line a
/// row with its trailing spaces cut, as the golden tests read it.
fn text(bytes: &[u8], cols: usize, rows: usize) -> String {
    let mut t = Terminal::new(cols, rows, 0);
    let mut rest = bytes;
    while !rest.is_empty() {
        let taken = t.input(rest);
        t.take_replies();
        if taken == 0 {
            break;
        }
        rest = rest.get(taken..).unwrap_or_default();
    }
    let s = t.screen();
    (0..s.rows())
        .filter_map(|r| s.line(r))
        .map(|line| {
            let row: String = line
                .cells()
                .iter()
                .map(|&c| match line.glyph(c) {
                    Glyph::Char(ch) => ch.to_string(),
                    Glyph::Cluster(s) => s.to_owned(),
                    Glyph::Empty => " ".to_owned(),
                })
                .collect();
            format!("{}\n", row.trim_end())
        })
        .collect()
}

/// Draws the golden `name` from `dir` into `out`.
fn shot(dir: &Path, out: &Path, name: &str) -> Result<PathBuf, String> {
    let (cols, rows) = grid(name).ok_or("its name ends in no COLSxROWS")?;
    let bytes = std::fs::read(dir.join(format!("{name}.bytes"))).map_err(|e| e.to_string())?;
    let want =
        std::fs::read_to_string(dir.join(format!("{name}.txt"))).map_err(|e| e.to_string())?;
    if text(&bytes, cols, rows) != want {
        return Err("its bytes do not draw the screen its golden test read".into());
    }
    // tier 2's screens hold their bytes for the whole run
    let screen = Screen::new(
        Box::leak(name.to_owned().into_boxed_str()),
        "a screen of vtium-config's TUI",
        Box::leak(bytes.into_boxed_slice()),
        cols,
        rows,
    );
    let img = draw(&screen)?;
    let png = vtium_specimens::png::encode(&Image {
        width: img.width(),
        height: img.height(),
        rgb: img.rgb().to_vec(),
    });
    let path = out.join(format!("{name}.png"));
    std::fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, out] = args.as_slice() else {
        eprintln!("usage: tui-shots BYTES OUT");
        return ExitCode::from(2);
    };
    let (dir, out) = (Path::new(dir), Path::new(out));
    let mut names: Vec<String> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|n| n.strip_suffix(".bytes").map(str::to_owned))
            .collect(),
        Err(e) => {
            eprintln!("tui-shots: {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };
    names.sort();
    if names.is_empty() {
        eprintln!("tui-shots: {} holds no screen", dir.display());
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::create_dir_all(out) {
        eprintln!("tui-shots: {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    let mut failed = 0usize;
    for name in &names {
        match shot(dir, out, name) {
            Ok(path) => println!("{}", path.display()),
            Err(why) => {
                eprintln!("tui-shots: {name}: {why}");
                failed = failed.saturating_add(1);
            }
        }
    }
    println!(
        "{} of {} screens drawn",
        names.len().saturating_sub(failed),
        names.len()
    );
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
