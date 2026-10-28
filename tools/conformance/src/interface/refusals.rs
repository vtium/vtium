//! The refusals: each malformed or edge input of INTERFACE.md exits with
//! the status the interface gives it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Tally;

/// What a refusal is, its COLS and ROWS, its stream, its CUTS file's
/// text if it has one, and the exit status it takes.
type Refusal = (
    &'static str,
    &'static str,
    &'static str,
    Vec<u8>,
    Option<&'static str>,
    u8,
);

/// vtium-replay's exit status for its command line `args`, run in-process.
pub fn exit_code(args: &[String]) -> u8 {
    match crate::main(args) {
        Ok(_) => 0,
        Err(f) => f.code,
    }
}

/// A directory of its own for each run, under the system's temporary
/// directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> std::io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "vtium-conformance-interface-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    fn write(&self, name: &str, data: &[u8]) -> std::io::Result<String> {
        let path: &Path = &self.0.join(name);
        std::fs::write(path, data)?;
        Ok(path.to_string_lossy().into_owned())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The exit status INTERFACE.md gives each malformed or edge input,
/// against `exit`, a replay's status for its command line.
pub fn refusals(exit: impl Fn(&[String]) -> u8) -> Tally {
    fn cmd(text: &[u8]) -> Vec<u8> {
        [b"\x1b_tsm-test;", text, b"\x1b\\"].concat()
    }
    let intro = b"\x1b_tsm-test;".to_vec();
    let after_a = |text: &[u8]| [b"a".as_slice(), &cmd(text)].concat();
    let cases: Vec<Refusal> = vec![
        ("COLS 0", "0", "2", vec![], None, 2),
        ("COLS 2049", "2049", "2", vec![], None, 2),
        ("ROWS 1025", "4", "1025", vec![], None, 2),
        ("COLS with a leading zero", "04", "2", vec![], None, 2),
        ("COLS with a sign", "+4", "2", vec![], None, 2),
        ("COLS 2048", "2048", "1", vec![], None, 0),
        ("ROWS 1024", "1", "1024", vec![], None, 0),
        (
            "a CUTS line with a letter",
            "4",
            "2",
            b"ab".to_vec(),
            Some("1x\n"),
            2,
        ),
        (
            "a CUTS line with a space",
            "4",
            "2",
            b"ab".to_vec(),
            Some(" 1\n"),
            2,
        ),
        (
            "a cut inside an introducer",
            "4",
            "2",
            after_a(b"sb-reset"),
            Some("4\n"),
            2,
        ),
        (
            "a cut inside a command",
            "4",
            "2",
            after_a(b"sb-reset"),
            Some("13\n"),
            2,
        ),
        (
            "a cut before a command",
            "4",
            "2",
            after_a(b"sb-reset"),
            Some("1\n"),
            0,
        ),
        (
            "a command with no ST",
            "4",
            "2",
            [intro.as_slice(), b"sb-reset"].concat(),
            None,
            2,
        ),
        ("a tab in a command", "4", "2", cmd(b"sb-up;n=1\t"), None, 2),
        ("a leading ;", "4", "2", cmd(b";sb-reset"), None, 2),
        ("= inside a value", "4", "2", cmd(b"sb-up;n=1=2"), None, 2),
        (
            "a decimal leading zero",
            "4",
            "2",
            cmd(b"sb-up;n=01"),
            None,
            2,
        ),
        ("a decimal sign", "4", "2", cmd(b"sb-up;n=+1"), None, 2),
        ("a decimal 0", "4", "2", cmd(b"sb-up;n=0"), None, 0),
        ("a signed -0", "4", "2", cmd(b"place;z=-0"), None, 2),
        (
            "a signed leading zero",
            "4",
            "2",
            cmd(b"place;z=-01"),
            None,
            2,
        ),
        ("an empty CUTS file", "4", "2", b"ab".to_vec(), Some(""), 0),
        ("an unknown command", "4", "2", cmd(b"no-such"), None, 2),
        ("an unknown key", "4", "2", cmd(b"sb-up;m=1"), None, 2),
        (
            "a key given twice",
            "4",
            "2",
            cmd(b"sb-up;n=1;n=1"),
            None,
            2,
        ),
        (
            "a missing required key",
            "4",
            "2",
            cmd(b"resize;c=4"),
            None,
            2,
        ),
        (
            "resize to 0 columns",
            "4",
            "2",
            cmd(b"resize;c=0;r=2"),
            None,
            2,
        ),
        (
            "resize to 2049 columns",
            "4",
            "2",
            cmd(b"resize;c=2049;r=2"),
            None,
            2,
        ),
        (
            "resize to 1025 rows",
            "4",
            "2",
            cmd(b"resize;c=4;r=1025"),
            None,
            2,
        ),
        (
            "resize within bounds",
            "4",
            "2",
            cmd(b"resize;c=6;r=3"),
            None,
            0,
        ),
        (
            "a caller past 7",
            "4",
            "2",
            cmd(b"frame-since;caller=8"),
            None,
            2,
        ),
    ];
    let mut tally = Tally {
        count: cases.len(),
        ..Tally::default()
    };
    let scratch = match Scratch::new() {
        Ok(s) => s,
        Err(e) => {
            tally
                .faults
                .push(format!("refusal: a scratch directory: {e}"));
            return tally;
        }
    };
    for (what, cols, rows, data, cuts, want) in cases {
        let files = scratch.write("refusal.bin", &data).and_then(|stream| {
            let mut args = vec![cols.to_owned(), rows.to_owned(), stream];
            if let Some(cuts) = cuts {
                args.push(scratch.write("refusal.cuts", cuts.as_bytes())?);
            }
            Ok(args)
        });
        match files {
            Ok(args) => {
                let got = exit(&args);
                if got != want {
                    tally
                        .faults
                        .push(format!("refusal: {what} exits {got}, not {want}"));
                }
            }
            Err(e) => tally.faults.push(format!("refusal: {what}: {e}")),
        }
    }
    tally
}
