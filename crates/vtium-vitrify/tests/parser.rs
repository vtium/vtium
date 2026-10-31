//! The parser's child as its parent runs it (ADR-1200 D14): a child that
//! dies, fails, answers badly, overruns its deadline or floods its
//! stderr fails its source and nothing else, and it starts with an empty
//! environment and only its pipes. A child is reaped as it exits, and
//! what it started is ended with it, whatever holds its pipes.

#[path = "common/parser.rs"]
mod parser;

use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parser::Parser;
use rustix::process::{
    Pid, PidfdFlags, Signal, WaitId, WaitIdOptions, pidfd_open, pidfd_send_signal, waitid,
};
use vtium_vitrify::child::Request;
use vtium_vitrify::glyphs::Glyphs;

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

/// A deadline long enough for a test to watch the stand-in's processes
/// before it passes.
const WATCHED_DEADLINE: Duration = Duration::from_secs(2);

// a parser that overruns its deadline is ended, and the test ends it
// itself should the parent fail to
#[test]
fn a_parser_past_its_deadline_is_ended() {
    let dir = scratch("parser-deadline");
    let parsing = parse_on_a_thread(
        in_dir(&dir, r#"echo $$ >"$1/child"; sleep 30"#).deadline(WATCHED_DEADLINE),
    );
    let child = watch(&dir, "child");
    assert!(running(&child), "the stand-in runs as the test watches it");
    let e = parsing.join().unwrap().expect_err("ended");
    assert!(e.contains("longer than"), "{e}");
}

/// The start of a stand-in that starts a process holding its pipes,
/// `sleep 30`. It writes that process's pid to the file `descendant`, and
/// then its own pid to the file `child`, in the directory it is given.
const HOLDS: &str = r#"sleep 30 & echo $! >"$1/descendant"; echo $$ >"$1/child"; "#;

/// An empty directory for the stand-in named `name` to write to.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("vtium-vitrify-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A stand-in that runs `script` with `dir` as its `$1`.
fn in_dir(dir: &Path, script: &str) -> Parser {
    Parser::with_program(
        Path::new("/bin/sh"),
        &["-c", script, "sh", &dir.to_string_lossy()],
    )
}

/// `parser` parsing an empty request on a thread of its own, so the test
/// can watch it meanwhile.
fn parse_on_a_thread(parser: Parser) -> JoinHandle<Result<Glyphs, String>> {
    std::thread::spawn(move || parser.parse(&Request::default()))
}

/// A process the stand-in started, with a pidfd that ends it when the
/// test ends, so a failing test leaves nothing behind.
struct Watched {
    pid: i32,
    fd: OwnedFd,
}

impl Drop for Watched {
    fn drop(&mut self) {
        let _ = pidfd_send_signal(&self.fd, Signal::KILL);
    }
}

/// The process whose pid the stand-in writes to `dir`'s `file`, waited
/// for at most 10 s. The pidfd names the process the pid named as it
/// opened, so a test checks with [`running`] that it opened in time.
fn watch(dir: &Path, file: &str) -> Watched {
    let path = dir.join(file);
    let read = || {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
    };
    assert!(
        within(Duration::from_secs(10), || read().is_some()),
        "the stand-in writes {file}"
    );
    let pid = read().unwrap();
    let fd = pidfd_open(Pid::from_raw(pid).unwrap(), PidfdFlags::empty()).unwrap();
    Watched { pid, fd }
}

/// Whether `child` is this process's child and has yet to exit. While it
/// has not, its process group has not been ended, so a process of that
/// group watched before this check was the stand-in's.
fn running(child: &Watched) -> bool {
    matches!(
        waitid(
            WaitId::PidFd(child.fd.as_fd()),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ),
        Ok(None)
    )
}

/// Whether the process `pid` is gone and reaped, waited for at most 2 s.
fn gone(pid: i32) -> bool {
    within(Duration::from_secs(2), || {
        !Path::new(&format!("/proc/{pid}")).exists()
    })
}

/// Whether `done` holds within `bound`, checked every 10 ms.
#[expect(
    clippy::disallowed_methods,
    reason = "the test bounds its wait on a state"
)]
fn within(bound: Duration, mut done: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + bound;
    loop {
        if done() {
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
    let dir = scratch("parser-exits-held");
    // the stand-in exits once the test has watched it and written `go`
    let script = format!(
        r#"{HOLDS}while [ ! -e "$1/go" ]; do sleep 0.01; done; echo 'bad PSF header' >&2; exit 1"#
    );
    let parsing = parse_on_a_thread(in_dir(&dir, &script).deadline(Duration::from_secs(5)));
    let descendant = watch(&dir, "descendant");
    let child = watch(&dir, "child");
    assert!(running(&child), "the stand-in runs as the test watches it");
    std::fs::write(dir.join("go"), "").unwrap();
    let e = parsing.join().unwrap().expect_err("failed");
    assert_eq!(e, "bad PSF header");
    assert!(gone(child.pid), "the parser is reaped");
    assert!(gone(descendant.pid), "what the parser started is ended");
}

// vtium/vtium#163: a parser ended at its deadline is reaped, though a
// process it started holds its stdout, and its process group is ended
#[test]
fn a_parser_past_its_deadline_is_reaped_with_what_it_started() {
    let dir = scratch("parser-deadline-held");
    let parsing =
        parse_on_a_thread(in_dir(&dir, &format!("{HOLDS}sleep 30")).deadline(WATCHED_DEADLINE));
    let descendant = watch(&dir, "descendant");
    let child = watch(&dir, "child");
    assert!(running(&child), "the stand-in runs as the test watches it");
    let e = parsing.join().unwrap().expect_err("ended");
    assert!(e.contains("longer than"), "{e}");
    assert!(gone(child.pid), "the parser is reaped");
    assert!(gone(descendant.pid), "what the parser started is ended");
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
