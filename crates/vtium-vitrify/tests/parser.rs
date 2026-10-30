//! The parser's child as its parent runs it (ADR-1200 D14): a child that
//! dies, fails, answers badly, overruns its deadline or floods its
//! stderr fails its source and nothing else, and it starts with an empty
//! environment and only its pipes.

#[path = "common/parser.rs"]
mod parser;

use std::os::fd::AsRawFd;
use std::path::Path;

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
