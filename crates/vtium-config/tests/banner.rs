//! The banner through the binary, at a pty: `--about` draws it when stdin
//! and stdout are the terminal and prints plain text to a pipe, and no
//! other command prints it. Which plan a terminal gets is tested in
//! `banner.rs`.

use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::process::{Command, Stdio};

use rustix::fs::{Mode, OFlags};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{Winsize, tcsetwinsize};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The kicker, which only the banner prints.
const KICKER: &str = "v  t  i  u  m";

/// What `vtium-config ARGS` wrote to an 80-column pty and to its stdout,
/// and its exit status, with stdin the pty and stdout the pty or a pipe.
fn run(args: &[&str], stdout_is_pty: bool) -> (String, String, Option<i32>) {
    let master =
        openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC).expect("openpt");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    let name = ptsname(&master, Vec::new()).expect("ptsname");
    let slave: OwnedFd = rustix::fs::open(
        name.as_c_str(),
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .expect("open the pty's slave");
    let size = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    tcsetwinsize(&slave, size).expect("set the pty's size");
    let stdout = if stdout_is_pty {
        Stdio::from(slave.try_clone().expect("dup"))
    } else {
        Stdio::piped()
    };
    let child = Command::new(BIN)
        .args(args)
        .env_clear()
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::from(slave.try_clone().expect("dup")))
        .stdout(stdout)
        .stderr(Stdio::piped())
        .spawn()
        .expect("run vtium-config");
    // With every slave closed but the child's, reading the master ends
    // when the child exits.
    drop(slave);
    let mut screen = Vec::new();
    let mut master = File::from(master);
    let mut chunk = [0; 4096];
    loop {
        match master.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => screen.extend_from_slice(&chunk[..n]),
            Err(e) if e.raw_os_error() == Some(rustix::io::Errno::IO.raw_os_error()) => break,
            Err(e) => panic!("read the pty: {e}"),
        }
    }
    let out = child.wait_with_output().expect("wait");
    (
        String::from_utf8_lossy(&screen).into_owned(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
    )
}

#[test]
fn about_draws_the_banner_at_a_terminal() {
    let (screen, _, status) = run(&["--about"], true);
    assert_eq!(status, Some(0), "{screen:?}");
    assert!(screen.contains(KICKER), "{screen:?}");
    assert!(screen.contains("\x1b[38;2;"), "{screen:?}");
    assert!(screen.contains("Run vtium-config --help"), "{screen:?}");
    assert!(!screen.contains("Usage:"), "{screen:?}");
}

#[test]
fn about_prints_plain_text_to_a_pipe() {
    let (screen, stdout, status) = run(&["--about"], false);
    assert_eq!(status, Some(0));
    assert_eq!(screen, "");
    assert!(stdout.starts_with("vtium-config "), "{stdout:?}");
    assert!(stdout.contains("fonts and settings"), "{stdout:?}");
    assert!(
        !stdout.contains('\x1b') && !stdout.contains(KICKER),
        "{stdout:?}"
    );
}

#[test]
fn help_at_a_terminal_prints_the_usage_and_no_banner() {
    let (screen, _, status) = run(&["--help"], true);
    assert_eq!(status, Some(0));
    assert!(screen.contains("Usage: vtium-config"), "{screen:?}");
    assert!(!screen.contains(KICKER), "{screen:?}");
    assert!(!screen.contains('\x1b'), "{screen:?}");
    // --about stays out of the usage, and --no-banner is gone from it.
    assert!(!screen.contains("--about"), "{screen:?}");
    assert!(!screen.contains("--no-banner"), "{screen:?}");
}

#[test]
fn bare_command_prints_no_banner_away_from_a_terminal() {
    let (screen, stdout, status) = run(&[], false);
    assert_eq!(status, Some(2));
    assert_eq!(screen, "");
    assert!(
        !stdout.contains(KICKER) && !stdout.contains('\x1b'),
        "{stdout:?}"
    );
}

#[test]
fn no_banner_is_still_accepted_and_silent() {
    let (screen, _, status) = run(&["--no-banner", "--help"], true);
    assert_eq!(status, Some(0));
    assert!(screen.contains("Usage: vtium-config"), "{screen:?}");
    assert!(!screen.contains(KICKER), "{screen:?}");
}
