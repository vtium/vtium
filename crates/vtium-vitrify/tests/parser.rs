//! The parser's child as its parent runs it (ADR-1200 D14): a child that
//! dies, fails, answers badly, overruns its deadline or floods its
//! stderr fails its source and nothing else, and it starts with an empty
//! environment and only its pipes. A child is reaped as it exits, and
//! what it started is ended with it, whatever holds its pipes.

#[path = "common/parser.rs"]
mod parser;

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use parser::Parser;
use vtium_vitrify::child::Request;

fn stand_in(script: &str) -> Parser {
    Parser::with_program(Path::new("/bin/sh"), &["-c", script])
}

#[test]
fn a_parser_that_dies_fails_the_source() {
    let e = stand_in("kill -ABRT $$")
        .parse(&Request::default())
        .expect_err("died");
    assert_eq!(e, "the parser was ended by signal 6");
    let e = stand_in("kill -SEGV $$")
        .parse(&Request::default())
        .expect_err("died");
    assert_eq!(e, "the parser was ended by signal 11");
}

#[test]
fn a_parser_that_fails_gives_its_message() {
    let e = stand_in("cat >/dev/null; echo 'bad PSF header' >&2; exit 1")
        .parse(&Request::default())
        .expect_err("failed");
    assert_eq!(e, "bad PSF header");
    let e = stand_in("exit 101")
        .parse(&Request::default())
        .expect_err("failed");
    assert!(e.contains("101"), "{e}");
}

#[test]
fn a_malformed_answer_is_refused() {
    let e = stand_in("cat >/dev/null; printf VTVGLYF1junk")
        .parse(&Request::default())
        .expect_err("refused");
    assert!(e.starts_with("the parser's answer was refused"), "{e}");
    // a parser that answers without reading its input is still read
    let e = stand_in("printf nonsense")
        .parse(&Request {
            font: vec![0; 1 << 20],
            ..Request::default()
        })
        .expect_err("refused");
    assert!(e.contains("refused") || e.contains("write"), "{e}");
}

#[test]
fn a_parser_past_its_deadline_is_ended() {
    let e = stand_in("sleep 30")
        .deadline(std::time::Duration::from_millis(300))
        .parse(&Request::default())
        .expect_err("ended");
    assert!(e.contains("longer than"), "{e}");
}

/// A stand-in that starts a process holding its pipes, `sleep 30`, then
/// runs `then`. It shows its own pid in `child` and that process's in
/// `descendant`, in the directory it is given.
const HOLDS: &str = r#"sleep 30 & echo $! >"$1/descendant"; echo $$ >"$1/child"; "#;

/// The directory a [`HOLDS`] stand-in named `name` shows its pids in.
fn holds_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("vtium-vitrify-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A [`HOLDS`] stand-in that then runs `then`, showing its pids in `dir`.
fn holding(dir: &Path, then: &str) -> Parser {
    let script = format!("{HOLDS}{then}");
    Parser::with_program(
        Path::new("/bin/sh"),
        &["-c", &script, "sh", &dir.to_string_lossy()],
    )
}

/// The pid the stand-in showed in `dir`'s `file`, and a pidfd that ends
/// that process when it drops, so a failing test leaves nothing behind.
fn shown(dir: &Path, file: &str) -> (i32, Ending) {
    let pid: i32 = std::fs::read_to_string(dir.join(file))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let fd = rustix::process::pidfd_open(
        rustix::process::Pid::from_raw(pid).unwrap(),
        rustix::process::PidfdFlags::empty(),
    )
    .ok();
    (pid, Ending(fd))
}

/// Ends the process its pidfd names when it drops.
struct Ending(Option<rustix::fd::OwnedFd>);

impl Drop for Ending {
    fn drop(&mut self) {
        if let Some(fd) = &self.0 {
            let _ = rustix::process::pidfd_send_signal(fd, rustix::process::Signal::KILL);
        }
    }
}

/// Whether the process `pid` is gone and reaped, waited for at most 2 s.
#[expect(
    clippy::disallowed_methods,
    reason = "the test bounds its wait on a process's state"
)]
fn gone(pid: i32) -> bool {
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return true;
        }
        if Instant::now() > until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

// vtium/vtium#163: a parser that exits is reaped as it exits, though a
// process it started holds its stdout, and that process is ended with it
#[test]
fn a_parser_that_exits_is_reaped_though_a_process_it_started_holds_its_stdout() {
    let dir = holds_dir("parser-exits-held");
    let e = holding(&dir, "echo 'bad PSF header' >&2; exit 1")
        .deadline(Duration::from_secs(5))
        .parse(&Request::default())
        .expect_err("failed");
    let (child, _c) = shown(&dir, "child");
    let (descendant, _d) = shown(&dir, "descendant");
    assert_eq!(e, "bad PSF header");
    assert!(gone(child), "the parser is reaped");
    assert!(gone(descendant), "what the parser started is ended");
}

// vtium/vtium#163: a parser ended at its deadline is reaped, though a
// process it started holds its stdout, and its process group is ended
#[test]
fn a_parser_past_its_deadline_is_reaped_with_what_it_started() {
    let dir = holds_dir("parser-deadline-held");
    let e = holding(&dir, "sleep 30")
        .deadline(Duration::from_millis(300))
        .parse(&Request::default())
        .expect_err("ended");
    let (child, _c) = shown(&dir, "child");
    let (descendant, _d) = shown(&dir, "descendant");
    assert!(e.contains("longer than"), "{e}");
    assert!(gone(child), "the parser is reaped");
    assert!(gone(descendant), "what the parser started is ended");
}

#[test]
fn a_parser_that_floods_stderr_does_not_hang_the_parent() {
    let e = stand_in("cat >/dev/null; head -c 1000000 /dev/zero | tr '\\0' x >&2; exit 1")
        .deadline(std::time::Duration::from_secs(10))
        .parse(&Request::default())
        .expect_err("failed");
    assert!(e.starts_with("xxxx") && e.len() <= 4096, "{}", e.len());
}

#[test]
fn the_parsers_environment_is_empty() {
    // the shell exports PWD, SHLVL and _ of its own; nothing else may
    // reach the parser, and it runs in /
    let e = stand_in("cat >/dev/null; env | grep -v -E '^(PWD|SHLVL|_)=' >&2; pwd >&2; exit 1")
        .parse(&Request::default())
        .expect_err("reports");
    assert_eq!(e, "/");
}

#[test]
fn the_parser_holds_only_its_pipes() {
    // a descriptor without close-on-exec, as one the invoking process
    // passes down may be, numbered high so it is never the one ls
    // opens
    let null = std::fs::File::open("/dev/null").unwrap();
    let held = rustix::io::fcntl_dupfd_cloexec(&null, 100).unwrap();
    rustix::io::fcntl_setfd(&held, rustix::io::FdFlags::empty()).unwrap();
    let e = stand_in("cat >/dev/null; ls /proc/self/fd >&2; exit 1")
        .parse(&Request::default())
        .expect_err("reports");
    // ls opens one descriptor of its own to read the directory
    let fds: Vec<&str> = e.lines().collect();
    let held = held.as_raw_fd().to_string();
    assert!(
        fds.len() <= 4
            && ["0", "1", "2"].iter().all(|f| fds.contains(f))
            && !fds.contains(&held.as_str()),
        "{fds:?}"
    );
}
