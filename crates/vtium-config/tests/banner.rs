//! The banner through the binary, at a pty: `--help about` draws it when stdin
//! and stdout are the terminal and prints plain text to a pipe, and no
//! other command prints it. Which plan a terminal gets is tested in
//! `banner.rs`.

use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::process::{Command, Stdio};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
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
    let mut child = Command::new(BIN)
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
    // The wait is bounded only so that a command that never ends fails the
    // test instead of hanging it: it counts one-second turns without output.
    let mut quiet_turns = 0;
    loop {
        let mut fds = [PollFd::new(&master, PollFlags::IN)];
        let turn = Timespec {
            tv_sec: 1,
            tv_nsec: 0,
        };
        if poll(&mut fds, Some(&turn)).expect("poll the pty") == 0 {
            quiet_turns += 1;
            if quiet_turns == 20 {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "vtium-config {args:?} wrote nothing for 20 turns; it wrote {:?}",
                    String::from_utf8_lossy(&screen)
                );
            }
            continue;
        }
        quiet_turns = 0;
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
    let (screen, _, status) = run(&["--help", "about"], true);
    assert_eq!(status, Some(0), "{screen:?}");
    assert!(screen.contains(KICKER), "{screen:?}");
    assert!(screen.contains("\x1b[38;2;"), "{screen:?}");
    assert!(screen.contains("sets up the vtium console"), "{screen:?}");
    assert!(!screen.contains("Usage:"), "{screen:?}");
}

#[test]
fn about_prints_plain_text_to_a_pipe() {
    let (screen, stdout, status) = run(&["--help", "about"], false);
    assert_eq!(status, Some(0));
    assert_eq!(screen, "");
    assert!(stdout.starts_with("vtium-config "), "{stdout:?}");
    assert!(stdout.contains("sets up the vtium console"), "{stdout:?}");
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
    // about is a topic like the commands, and --no-banner is gone.
    assert!(
        screen.contains("vtium-config's name, version and logo"),
        "{screen:?}"
    );
    assert!(!screen.contains("--no-banner"), "{screen:?}");
}

#[test]
fn bare_command_prints_the_usage_and_no_banner() {
    for stdout_is_pty in [true, false] {
        let (screen, stdout, status) = run(&[], stdout_is_pty);
        assert_eq!(status, Some(0), "{stdout_is_pty}");
        let shown = if stdout_is_pty { screen } else { stdout };
        assert!(shown.contains("Usage: vtium-config"), "{shown:?}");
        assert!(shown.contains("vtium-config tui"), "{shown:?}");
        assert!(
            !shown.contains(KICKER) && !shown.contains('\x1b'),
            "{shown:?}"
        );
    }
}

#[test]
fn tui_is_refused_away_from_a_terminal() {
    let (_, stdout, status) = run(&["tui"], false);
    assert_eq!(status, Some(2));
    assert_eq!(stdout, "");
}

#[test]
fn no_banner_is_still_accepted_and_silent() {
    let (screen, _, status) = run(&["--no-banner", "--help"], true);
    assert_eq!(status, Some(0));
    assert!(screen.contains("Usage: vtium-config"), "{screen:?}");
    assert!(!screen.contains(KICKER), "{screen:?}");
}

#[test]
fn help_and_bare_print_the_same_usage() {
    let (_, bare, _) = run(&[], false);
    let (_, help, _) = run(&["--help"], false);
    assert_eq!(bare, help);
    assert!(help.starts_with("Usage: vtium-config"), "{help:?}");
}

#[test]
fn help_for_each_command_prints_its_own_text() {
    for command in [
        "fonts",
        "check",
        "config",
        "new",
        "tui",
        "remove-frit",
        "preview",
        "set",
        "apply",
        "serve",
    ] {
        let (_, stdout, status) = run(&["--help", command], false);
        assert_eq!(status, Some(0), "{command}");
        assert!(
            stdout.starts_with(&format!("Usage:\n  vtium-config {command}")),
            "{command}: {stdout:?}"
        );
        assert!(stdout.contains("Example:\n  "), "{command}: {stdout:?}");
        assert!(!stdout.contains("Options:"), "{command}: {stdout:?}");
    }
}

#[test]
fn help_on_an_unknown_topic_exits_2_and_lists_the_topics() {
    let master = Command::new(BIN)
        .args(["--help", "frobnicate"])
        .output()
        .expect("run vtium-config");
    assert_eq!(master.status.code(), Some(2));
    let err = String::from_utf8_lossy(&master.stderr);
    assert!(err.contains("no help on frobnicate"), "{err}");
    assert!(err.contains("fonts") && err.contains("about"), "{err}");
    assert!(master.stdout.is_empty());
}

#[test]
fn no_banner_does_not_stop_help_about() {
    let (screen, _, status) = run(&["--no-banner", "--help", "about"], true);
    assert_eq!(status, Some(0));
    assert!(screen.contains(KICKER), "{screen:?}");
}
