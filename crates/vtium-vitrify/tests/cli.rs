//! vtium-vitrify's command line, run as the invoking user: its usage
//! errors, the store's commands gone, and the parser's child. The source
//! forms and decoders are judged through the helper's engine
//! (`tests/engine.rs`).

use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-vitrify");

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn status(o: &Output) -> i32 {
    o.status.code().expect("exited")
}

#[test]
fn usage_errors_exit_2() {
    for args in [
        &["frobnicate"][..],
        &[],
        &["--bogus", "set", "x"],
        &["--frits"],
        &["serve", "x"],
    ] {
        let o = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(status(&o), 2, "{args:?}: {}", stderr(&o));
        assert!(stderr(&o).contains("--help"), "{args:?}");
    }
    let o = Command::new(BIN).arg("--help").output().unwrap();
    assert_eq!(status(&o), 0);
    assert!(stdout(&o).starts_with("Usage: vtium-vitrify"));
}

/// The parser, started as the tool starts it, sets no-new-privileges and
/// its limits before it reads anything.
#[test]
fn the_parser_runs_without_new_privileges_and_within_its_limits() {
    let mut child = Command::new(BIN)
        .arg("--parse")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The parser reads its request only once every limit is set, so the
    // limits are read when a byte of it has left the pipe, unless the
    // parser ended first.
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&[0]).unwrap();
    let read = loop {
        if rustix::io::ioctl_fionread(&stdin).unwrap() == 0 {
            break true;
        }
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        std::thread::yield_now();
    };
    let proc_status =
        fs::read_to_string(format!("/proc/{}/status", child.id())).unwrap_or_default();
    let limits = fs::read_to_string(format!("/proc/{}/limits", child.id())).unwrap_or_default();
    drop(stdin);
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let _ = child.stdout.take().unwrap().read_to_end(&mut Vec::new());
    assert!(!child.wait().unwrap().success());
    assert!(read, "the parser never read its request");
    assert!(
        proc_status
            .lines()
            .any(|l| l.split_whitespace().eq(["NoNewPrivs:", "1"])),
        "NoNewPrivs not set"
    );
    let limit = |name: &str| {
        limits
            .lines()
            .find(|l| l.starts_with(name))
            .map(|l| {
                l[name.len()..]
                    .split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    };
    assert_eq!(limit("Max address space"), "536870912 536870912");
    assert_eq!(limit("Max cpu time"), "30 30");
    assert_eq!(limit("Max processes"), "0 0");
    assert_eq!(limit("Max file size"), "0 0");
    // an empty request is refused, not parsed
    assert!(err.contains("malformed parse request"), "{err}");
    let _ = std::io::stdout().flush();
}

// ADR-1200 (#145): the prerendered store is gone, and with it the commands
// that built, installed, listed, verified and removed its sets. Each is now
// an unknown command, a usage error.
#[test]
fn the_store_commands_are_gone() {
    for command in ["create", "install", "list", "verify", "remove"] {
        let o = Command::new(BIN).arg(command).output().unwrap();
        assert_eq!(status(&o), 2, "{command}: {}", stderr(&o));
        assert!(
            stderr(&o).contains(&format!("unknown command {command}")),
            "{command}: {}",
            stderr(&o)
        );
    }
}
