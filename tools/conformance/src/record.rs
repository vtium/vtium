//! Recordings of real programs for the corpus: each program runs in a pty
//! of a fixed size with a fixed environment, a script of keys is typed
//! into it, and every byte it writes is kept, with a cut where each screen
//! had settled, after start-up and after each key. The recordings are
//! committed, and their goldens reviewed as their own change.
//!
//! The C reference recorded the corpus's `prog-*` cases the same way
//! (libtsm's `test/conformance/record-programs.py`), inside its dev
//! container, so its recordings carry that image's program versions; a
//! recording made here carries this machine's.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process};
use vtium_platform::event_loop::Loop;
use vtium_platform::pty::{Consumer, Erase, Pty, Replies, Size};

/// A program's recording: its screen, its command line and its keys.
pub struct Recording {
    pub name: &'static str,
    pub cols: u16,
    pub rows: u16,
    pub argv: &'static [&'static str],
    pub keys: &'static [&'static str],
}

/// The quiet that counts as a screen settled.
pub const QUIET: Duration = Duration::from_millis(400);
/// The longest wait for a screen to settle.
pub const SETTLE_MAX: Duration = Duration::from_secs(5);
/// 2026-01-01 00:00:00 UTC, the demo files' times, so `ls -l` prints the
/// same dates on every run.
pub const STAMP: u64 = 1_767_225_600;

pub const RECORDINGS: &[Recording] = &[
    Recording {
        name: "prog-shell",
        cols: 80,
        rows: 24,
        argv: &["/usr/bin/bash", "--norc", "--noprofile", "-i"],
        keys: &[
            "ls --color=always -l\r",
            "ls --color=always src\r",
            "printf '\\e[1mbold\\e[0m \\e[31mred\\e[0m \\e[4munder\\e[0m\\n'\r",
            "seq 1 30 | paste -sd' '\r",
            "for i in 1 2 3; do echo \"loop $i\"; done\r",
            "exit\r",
        ],
    },
    Recording {
        name: "prog-less",
        cols: 80,
        rows: 24,
        argv: &["/usr/bin/less", "-R", "notes.txt"],
        keys: &[" ", " ", "b", "/fox\r", "n", "G", "g", "q"],
    },
    Recording {
        name: "prog-vim",
        cols: 80,
        rows: 24,
        argv: &["/usr/bin/vim", "--clean", "-N", "src/main.c"],
        keys: &[
            "jjj",
            "A // edited\x1b",
            "o\tputs(\"added\");\x1b",
            ":set number\r",
            "gg",
            "/printf\r",
            ":q!\r",
        ],
    },
    Recording {
        name: "prog-tmux",
        cols: 80,
        rows: 24,
        argv: &[
            "/usr/bin/tmux",
            "-f",
            "/dev/null",
            "-L",
            "conformance",
            "new-session",
            "/usr/bin/bash",
            "--norc",
            "--noprofile",
            "-i",
        ],
        keys: &[
            "echo left pane\r",
            "\x02%",
            "echo right pane\r",
            "\x02\"",
            "seq 1 5\r",
            "tmux -L conformance kill-server\r",
        ],
    },
];

/// The recording named `name`.
pub fn find(name: &str) -> Option<&'static Recording> {
    RECORDINGS.iter().find(|r| r.name == name)
}

/// The program's environment, with `home` as its home.
pub fn env(home: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("PATH", "/usr/bin:/bin".to_owned()),
        ("HOME", home.display().to_string()),
        ("TERM", "xterm-256color".to_owned()),
        ("LANG", "C.UTF-8".to_owned()),
        ("LC_ALL", "C.UTF-8".to_owned()),
        ("TZ", "UTC".to_owned()),
        (
            "PS1",
            "\\[\\e[1;32m\\]user@host\\[\\e[0m\\]:\\[\\e[1;34m\\]~\\[\\e[0m\\]$ ".to_owned(),
        ),
    ]
}

/// Writes the files the programs open under `home`/demo, every one dated
/// [`STAMP`], and returns the directory, where the program starts.
pub fn demo_files(home: &Path) -> std::io::Result<PathBuf> {
    use std::fmt::Write as _;
    use std::fs::{self, File, FileTimes, Permissions};
    use std::os::unix::fs::PermissionsExt;

    let root = home.join("demo");
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("src/main.c"),
        "#include <stdio.h>\n\n\
         /* A small file for the editor recording. */\n\
         int main(void)\n{\n\tfor (int i = 0; i < 3; i++)\n\
         \t\tprintf(\"line %d\\n\", i);\n\treturn 0;\n}\n",
    )?;
    let mut notes = String::new();
    for n in 1..=120_usize {
        let stars = "*".repeat(n % 7);
        let _ = writeln!(
            notes,
            "{n:3}  The quick brown fox jumps over the lazy dog. 漢字 {stars}"
        );
    }
    fs::write(root.join("notes.txt"), notes)?;
    fs::write(root.join("run.sh"), "#!/bin/sh\necho run\n")?;
    fs::set_permissions(root.join("run.sh"), Permissions::from_mode(0o755))?;
    let stamp = std::time::UNIX_EPOCH + Duration::from_secs(STAMP);
    let times = FileTimes::new().set_accessed(stamp).set_modified(stamp);
    // the files before the directories that hold them, whose times a
    // write inside them would move
    for path in [
        root.join("src/main.c"),
        root.join("notes.txt"),
        root.join("run.sh"),
        root.join("src"),
        root.clone(),
    ] {
        File::open(&path)?.set_times(times)?;
    }
    Ok(root)
}

/// The cuts of a recording `len` bytes long, from the offsets at which
/// each screen settled: sorted, each once, and inside the stream.
pub fn cuts(settled: &[usize], len: usize) -> Vec<usize> {
    let mut cuts: Vec<usize> = settled
        .iter()
        .copied()
        .filter(|&c| 0 < c && c < len)
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// The pty's output, and whether the child has exited.
#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    exited: bool,
}

impl Consumer for Output {
    /// Takes every byte, and answers nothing: no terminal reads the
    /// program's queries while it is recorded.
    fn output(&mut self, output: &[u8], _: &mut Replies<'_>) -> usize {
        self.bytes.extend_from_slice(output);
        output.len()
    }

    fn exited(&mut self, _: std::process::ExitStatus) {
        self.exited = true;
    }
}

struct Host {
    pty: Option<Pty>,
    output: Output,
}

fn parts(h: &mut Host) -> Option<(&mut Pty, &mut Output)> {
    let output = &mut h.output;
    h.pty.as_mut().map(|p| (p, output))
}

/// Turns the loop until the output has been quiet for [`QUIET`], the
/// child has exited, or [`SETTLE_MAX`] has passed.
#[expect(
    clippy::disallowed_methods,
    reason = "a recording is cut where the program's output went quiet, which only the clock tells; nothing replayed reads it"
)]
fn settle(lp: &mut Loop<Host>, h: &mut Host) -> std::io::Result<()> {
    let start = Instant::now();
    let mut last = (Instant::now(), h.output.bytes.len());
    while !h.output.exited {
        let now = Instant::now();
        if h.output.bytes.len() != last.1 {
            last = (now, h.output.bytes.len());
        }
        let quiet = now.duration_since(last.0);
        let spent = now.duration_since(start);
        if quiet >= QUIET || spent >= SETTLE_MAX {
            break;
        }
        let wait = (QUIET - quiet).min(SETTLE_MAX - spent);
        lp.dispatch(Some(wait), h)?;
    }
    Ok(())
}

/// Records `rec` with `home` as the program's home and `cwd` as its
/// directory: the bytes it wrote and the offsets at which each screen
/// settled, cut by [`cuts`].
pub fn record(rec: &Recording, home: &Path, cwd: &Path) -> std::io::Result<(Vec<u8>, Vec<usize>)> {
    let (program, args) = rec
        .argv
        .split_first()
        .ok_or_else(|| std::io::Error::other(format!("{}: no command", rec.name)))?;
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .envs(env(home))
        .current_dir(cwd);
    let size = Size {
        cols: rec.cols,
        rows: rec.rows,
        cell_width: 0,
        cell_height: 0,
    };
    let mut lp: Loop<Host> = Loop::new()?;
    // The kernel's default erase character, which the C reference's pty
    // kept.
    let mut pty = Pty::spawn(command, size, Erase::Delete)?;
    pty.insert(&lp.handle(), parts)?;
    let mut h = Host {
        pty: Some(pty),
        output: Output::default(),
    };
    let mut settled = Vec::new();
    settle(&mut lp, &mut h)?;
    settled.push(h.output.bytes.len());
    for key in rec.keys {
        if let Some(pty) = h.pty.as_mut() {
            pty.write(key.as_bytes());
        }
        settle(&mut lp, &mut h)?;
        settled.push(h.output.bytes.len());
    }
    let child = h
        .pty
        .as_ref()
        .and_then(Pty::child_id)
        .and_then(|id| i32::try_from(id).ok())
        .and_then(Pid::from_raw);
    if let Some(pid) = child {
        // the child may have exited and gone already
        let _ = kill_process(pid, Signal::HUP);
    }
    settle(&mut lp, &mut h)?;
    h.pty = None;
    let len = h.output.bytes.len();
    Ok((h.output.bytes, cuts(&settled, len)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::UNIX_EPOCH;

    #[test]
    fn cuts_are_sorted_once_each_and_inside_the_stream() {
        assert_eq!(cuts(&[0, 40, 12, 40, 12, 100, 120], 100), vec![12, 40]);
        assert_eq!(cuts(&[], 10), Vec::<usize>::new());
        assert_eq!(cuts(&[5, 5], 5), Vec::<usize>::new());
    }

    #[test]
    fn the_demo_files_hold_what_the_programs_open() {
        let home = std::env::temp_dir().join(format!("vtium-record-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let root = demo_files(&home).unwrap();
        assert_eq!(root, home.join("demo"));
        let main = std::fs::read_to_string(root.join("src/main.c")).unwrap();
        assert!(main.starts_with("#include <stdio.h>\n\n/* A small file"));
        assert!(main.ends_with("\treturn 0;\n}\n"));
        let notes = std::fs::read_to_string(root.join("notes.txt")).unwrap();
        assert_eq!(notes.lines().count(), 120);
        assert_eq!(
            notes.lines().next(),
            Some("  1  The quick brown fox jumps over the lazy dog. 漢字 *")
        );
        assert_eq!(
            notes.lines().nth(6),
            Some("  7  The quick brown fox jumps over the lazy dog. 漢字 ")
        );
        assert_eq!(
            notes.lines().last(),
            Some("120  The quick brown fox jumps over the lazy dog. 漢字 *")
        );
        let run = root.join("run.sh");
        assert_eq!(
            std::fs::read_to_string(&run).unwrap(),
            "#!/bin/sh\necho run\n"
        );
        assert_eq!(
            std::fs::metadata(&run).unwrap().permissions().mode() & 0o777,
            0o755
        );
        for path in [&root, &root.join("src"), &root.join("src/main.c"), &run] {
            let modified = std::fs::metadata(path).unwrap().modified().unwrap();
            assert_eq!(
                modified.duration_since(UNIX_EPOCH).unwrap().as_secs(),
                STAMP,
                "{}",
                path.display()
            );
        }
        std::fs::remove_dir_all(&home).unwrap();
    }
}
