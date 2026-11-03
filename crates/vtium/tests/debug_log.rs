//! `debug` as the binary takes it: with it, the log holds debug messages.
//! Each run names a VT that does not exist, in a configuration directory
//! of its own, so vtium logs the kernel's font it cannot read, at debug,
//! and exits before it opens anything of the machine's.

use std::process::Command;

#[path = "common/scratch.rs"]
mod scratch;
use scratch::Scratch;

/// vtium's log for a run with `extra`.
fn log(extra: &[&str]) -> String {
    let dir = Scratch::new("debug-log");
    let out = Command::new(env!("CARGO_BIN_EXE_vtium"))
        .arg("-c")
        .arg(&*dir)
        .arg(format!("--vt={}", dir.join("no-tty").display()))
        .args(extra)
        .args(["--login", "--", "/bin/true"])
        .output()
        .expect("vtium runs");
    assert_eq!(out.status.code(), Some(1), "no VT to run on");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn debug_logs_the_debug_messages_and_its_absence_does_not() {
    let debug = "the kernel's font is not read from ";
    let with = log(&["--debug"]);
    assert!(with.contains(debug), "{with}");
    let without = log(&[]);
    assert!(!without.contains(debug), "{without}");
    assert!(without.contains("vt: using tty "), "{without}");
}
