//! Draws each of vtium-config's TUI screen goldens as a PNG, the way a
//! vtium console shows it:
//!
//! ```text
//! tui-shots SCREENS BYTES OUT
//! ```
//!
//! BYTES holds what the golden tests write under `VTIUM_CONFIG_SHOTS`: for
//! each golden `NAME-COLSxROWS`, `NAME-COLSxROWS.bytes`, every byte the TUI
//! wrote to reach the screen, and `NAME-COLSxROWS.txt`, the screen as the
//! golden test read it. Each byte stream goes through vtium-term and
//! vtium-render in unifont, as tier 2 draws its screens, into
//! `OUT/NAME-COLSxROWS.png`. SCREENS is the directory of the committed
//! goldens, `crates/vtium-config/tests/screens`. Before it writes a
//! picture, the tool checks that the same bytes played into a terminal of
//! its own read as the screen the test read.
//!
//! A settled golden must also be the screen the test read: one that
//! drifted is named and not drawn, and the tool exits non-zero. A
//! provisional golden, one that `SCREENS/provisional.tsv` lists, judges
//! nothing until the maintainer accepts it, so the tool draws it whatever
//! its committed text says. The maintainer reviews those pictures, so each
//! one is marked provisional in the tool's output, with a note where the
//! screen differs from its committed golden. The tool also fails when
//! BYTES holds no screen, and names each golden in SCREENS that BYTES
//! holds no bytes for, because a golden test that `make tui-shots` does
//! not run would otherwise leave its screens out of the review unseen.

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

/// The names `provisional.tsv` in `screens` lists: the first field of each
/// line that is not blank and not a comment. A missing file lists none.
fn provisional(screens: &Path) -> Vec<String> {
    std::fs::read_to_string(screens.join("provisional.tsv"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split('\t').next())
        .map(str::to_owned)
        .collect()
}

/// The goldens in `screens`: the name of each `NAME-COLSxROWS.txt` file,
/// sorted.
fn goldens(screens: &Path) -> std::io::Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(screens)?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".txt").map(str::to_owned))
        .filter(|n| grid(n).is_some())
        .collect();
    names.sort();
    Ok(names)
}

/// The goldens that have no bytes among `drawn`, the names BYTES holds.
fn without_bytes<'a>(goldens: &'a [String], drawn: &[String]) -> Vec<&'a str> {
    goldens
        .iter()
        .filter(|g| !drawn.contains(g))
        .map(String::as_str)
        .collect()
}

/// Whether `bytes` may be drawn as a golden: the bytes draw `read`, the
/// screen the golden test read, at `cols` by `rows`, and `read` is
/// `golden`, the committed golden, unless the golden is `provisional`.
fn check(
    bytes: &[u8],
    read: &str,
    golden: &str,
    provisional: bool,
    cols: usize,
    rows: usize,
) -> Result<(), String> {
    if read != golden && !provisional {
        return Err(
            "the screen the golden test drew differs from its committed golden, so it is not drawn"
                .into(),
        );
    }
    if text(bytes, cols, rows) != read {
        return Err("its bytes do not draw the screen its golden test read".into());
    }
    Ok(())
}

/// Draws the golden `name` from `dir` into `out`, when [`check`] allows it.
fn shot(
    screens: &Path,
    dir: &Path,
    out: &Path,
    name: &str,
    provisional: bool,
) -> Result<PathBuf, String> {
    let (cols, rows) = grid(name).ok_or("its name ends in no COLSxROWS")?;
    let bytes = std::fs::read(dir.join(format!("{name}.bytes"))).map_err(|e| e.to_string())?;
    let read =
        std::fs::read_to_string(dir.join(format!("{name}.txt"))).map_err(|e| e.to_string())?;
    let committed = screens.join(format!("{name}.txt"));
    let golden =
        std::fs::read_to_string(&committed).map_err(|e| format!("{}: {e}", committed.display()))?;
    check(&bytes, &read, &golden, provisional, cols, rows)?;
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
    let [screens, dir, out] = args.as_slice() else {
        eprintln!("usage: tui-shots SCREENS BYTES OUT");
        return ExitCode::from(2);
    };
    let (screens, dir, out) = (Path::new(screens), Path::new(dir), Path::new(out));
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
    let committed = match goldens(screens) {
        Ok(committed) => committed,
        Err(e) => {
            eprintln!("tui-shots: {}: {e}", screens.display());
            return ExitCode::FAILURE;
        }
    };
    let listed = provisional(screens);
    let (mut failed, mut drawn_provisional) = (0usize, 0usize);
    for name in &names {
        let is_provisional = listed.contains(name);
        match shot(screens, dir, out, name, is_provisional) {
            Ok(path) if is_provisional => {
                drawn_provisional = drawn_provisional.saturating_add(1);
                let moved = std::fs::read_to_string(dir.join(format!("{name}.txt"))).ok()
                    != std::fs::read_to_string(screens.join(format!("{name}.txt"))).ok();
                let note = if moved {
                    ", differs from its committed golden"
                } else {
                    ""
                };
                println!("{}\tprovisional{note}", path.display());
            }
            Ok(path) => println!("{}\tsettled", path.display()),
            Err(why) => {
                eprintln!("tui-shots: {name}: {why}");
                failed = failed.saturating_add(1);
            }
        }
    }
    let missing = without_bytes(&committed, &names);
    for name in &missing {
        eprintln!(
            "tui-shots: {name}: the bytes directory holds no bytes for this golden, so it is not drawn. Its golden test may be one that make tui-shots does not run."
        );
    }
    println!(
        "{} of {} screens drawn, {drawn_provisional} of them provisional",
        names.len().saturating_sub(failed),
        names.len().saturating_add(missing.len())
    );
    let failed = failed.saturating_add(missing.len());
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A screen of 4 by 2 that reads `ab` on its first row.
    const BYTES: &[u8] = b"\x1b[2J\x1b[Hab";
    const SCREEN: &str = "ab\n\n";

    #[test]
    fn bytes_that_draw_their_golden_are_drawn() {
        assert_eq!(check(BYTES, SCREEN, SCREEN, false, 4, 2), Ok(()));
    }

    #[test]
    fn bytes_that_do_not_draw_the_screen_the_test_read_are_refused() {
        assert!(check(BYTES, "ba\n\n", "ba\n\n", false, 4, 2).is_err());
        assert!(check(BYTES, "ba\n\n", "ba\n\n", true, 4, 2).is_err());
    }

    #[test]
    fn a_settled_screen_that_drifted_from_its_committed_golden_is_refused() {
        let why = check(BYTES, SCREEN, "ab c\n\n", false, 4, 2).expect_err("a drifted golden");
        assert!(why.contains("golden"), "{why}");
    }

    #[test]
    fn a_provisional_screen_that_drifted_from_its_committed_golden_is_drawn() {
        assert_eq!(check(BYTES, SCREEN, "ab c\n\n", true, 4, 2), Ok(()));
    }

    #[test]
    fn a_golden_without_bytes_is_named() {
        let dir = std::env::temp_dir().join(format!("tui-shots-goldens-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory");
        for file in [
            "help-80x25.txt",
            "help-100x30.txt",
            "provisional.tsv",
            "notes.txt",
        ] {
            std::fs::write(dir.join(file), "").expect("a file");
        }
        let committed = goldens(&dir);
        std::fs::remove_dir_all(&dir).expect("the directory removed");
        let committed = committed.expect("the goldens");
        assert_eq!(committed, ["help-100x30", "help-80x25"]);
        assert_eq!(
            without_bytes(&committed, &["help-80x25".to_owned()]),
            ["help-100x30"]
        );
        assert!(without_bytes(&committed, &committed).is_empty());
    }

    #[test]
    fn the_provisional_list_is_the_first_field_of_each_line_that_is_not_a_comment() {
        let dir = std::env::temp_dir().join(format!("tui-shots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory");
        std::fs::write(
            dir.join("provisional.tsv"),
            "# names\nhelp-80x25\tthe Help view\n\nflow-default-120x32\tthe Flow view\n",
        )
        .expect("the list");
        let got = provisional(&dir);
        std::fs::remove_dir_all(&dir).expect("the directory removed");
        assert_eq!(got, ["help-80x25", "flow-default-120x32"]);
    }
}
