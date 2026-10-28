//! The banner through the binary, at a pty: shown when stdin and stdout
//! are the terminal, hidden when stdout is a pipe though stdin is the
//! terminal, and hidden by `--no-banner`. Which plan a terminal gets is
//! tested in `banner.rs`.

use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::process::{Command, Stdio};

use rustix::fs::{Mode, OFlags};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{Winsize, tcsetwinsize};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-vitrify");

/// The kicker, which only the banner prints.
const KICKER: &str = "v  t  i  u  m";

/// What `vtium-vitrify ARGS --help` wrote to an 80-column pty and to its
/// stdout, with stdin the pty and stdout the pty or a pipe.
fn run(args: &[&str], stdout_is_pty: bool) -> (String, String) {
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
        .arg("--help")
        .env_clear()
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::from(slave.try_clone().expect("dup")))
        .stdout(stdout)
        .stderr(Stdio::piped())
        .spawn()
        .expect("run vtium-vitrify");
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
    assert!(out.status.success(), "{out:?}");
    (
        String::from_utf8_lossy(&screen).into_owned(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn shown_when_stdin_and_stdout_are_the_terminal() {
    let (screen, _) = run(&[], true);
    let banner = screen.find(KICKER).expect("the banner");
    let usage = screen.find("Usage: vtium-vitrify").expect("the help");
    assert!(banner < usage, "{screen:?}");
    assert!(screen.contains("\x1b[38;2;"), "{screen:?}");
}

#[test]
fn hidden_when_stdout_is_a_pipe() {
    let (screen, stdout) = run(&[], false);
    assert_eq!(screen, "");
    assert!(stdout.starts_with("Usage: vtium-vitrify"), "{stdout:?}");
    assert!(!stdout.contains('\x1b'), "{stdout:?}");
}

#[test]
fn hidden_by_no_banner() {
    let (screen, _) = run(&["--no-banner"], true);
    assert!(screen.contains("Usage: vtium-vitrify"), "{screen:?}");
    assert!(screen.contains("--no-banner"), "{screen:?}");
    assert!(!screen.contains(KICKER), "{screen:?}");
    assert!(!screen.contains('\x1b'), "{screen:?}");
}
