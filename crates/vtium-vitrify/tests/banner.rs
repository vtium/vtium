//! The banner through the binary: hidden when stdout is a pipe, whatever
//! the environment offers, and `--no-banner` is an option the command line
//! takes. Which plan a terminal gets is tested in `banner.rs`.

use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-vitrify");

fn help(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .arg("--help")
        .env_remove("NO_COLOR")
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .stdin(Stdio::inherit())
        .output()
        .expect("run vtium-vitrify")
}

#[test]
fn hidden_when_stdout_is_a_pipe() {
    let out = help(&[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout.starts_with("Usage: vtium-vitrify"), "{stdout:?}");
    assert!(!stdout.contains('\x1b'), "{stdout:?}");
}

#[test]
fn the_command_line_takes_no_banner() {
    let out = help(&["--no-banner"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout.starts_with("Usage: vtium-vitrify"), "{stdout:?}");
    assert!(stdout.contains("--no-banner"), "{stdout:?}");
}
