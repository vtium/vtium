//! `set` (ADR-1200 D5) on a real pty in raw mode, answered by the
//! console's own terminal: the request and the query written, the reply
//! read within the wait, and the terminal's mode put back.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{LocalModes, tcgetattr};
use vtium_term::Terminal;
use vtium_vitrify::switch::{Answer, Raw, WAIT, switch};

/// A pty's master and its slave, neither the test's controlling terminal.
fn pair() -> (File, File) {
    let master =
        openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC).expect("a pty");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    let name = ptsname(&master, Vec::new()).expect("ptsname");
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(rustix::fs::OFlags::NOCTTY.bits() as i32)
        .open(name.to_str().expect("a path"))
        .expect("the slave");
    (File::from(master), slave)
}

/// The console on `master`, as vtium answers: its terminal parses what
/// `set` writes, and a set asked for switches when it is in `built`. Ends
/// when the slave closes.
fn console(mut master: File, built: &'static [&'static str], ops: bool) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut t = Terminal::new(80, 24, 0);
        t.set_font_ops(ops);
        assert!(t.set_font_name("unifont"));
        let mut buf = [0u8; 256];
        while let Ok(n @ 1..) = master.read(&mut buf) {
            let mut rest = &buf[..n];
            while !rest.is_empty() {
                let taken = t.input(rest);
                if let Some(name) = t.take_font_request() {
                    if built.contains(&name.as_str()) {
                        assert!(t.set_font_name(&name));
                    }
                }
                rest = &rest[taken..];
            }
            let replies = t.take_replies();
            if !replies.is_empty() && master.write_all(&replies).is_err() {
                break;
            }
        }
    })
}

#[expect(clippy::disallowed_methods, reason = "the test times the wait")]
fn now() -> Instant {
    Instant::now()
}

/// `set NAME` on a pty whose console knows `built`, under `ops`, waiting
/// `wait`.
fn run(name: &str, built: &'static [&'static str], ops: bool, wait: Duration) -> Answer {
    let (master, slave) = pair();
    let echo = |f: &File| {
        tcgetattr(f)
            .expect("tcgetattr")
            .local_modes
            .contains(LocalModes::ICANON | LocalModes::ECHO)
    };
    assert!(echo(&slave), "a pty starts cooked");
    let answering = console(master, built, ops);
    let answer = {
        let _raw = Raw::new(&slave).expect("raw mode");
        assert!(!echo(&slave), "raw while set reads");
        switch(&mut &slave, name, now() + wait).expect("the switch")
    };
    assert!(echo(&slave), "the mode before is put back");
    drop(slave);
    answering.join().expect("the console");
    answer
}

#[test]
fn a_built_set_switches_and_the_reply_names_it() {
    assert_eq!(
        run("tiny", &["tiny"], true, WAIT),
        Answer::Named("tiny".into())
    );
}

#[test]
fn a_set_the_console_refuses_leaves_the_name_before() {
    assert_eq!(
        run("tiny", &[], true, WAIT),
        Answer::Named("unifont".into())
    );
}

#[test]
fn with_font_operations_off_no_reply_comes_within_the_wait() {
    let start = now();
    assert_eq!(
        run("tiny", &["tiny"], false, Duration::from_millis(300)),
        Answer::Silent
    );
    assert!(
        now() - start < Duration::from_secs(2),
        "the wait is bounded"
    );
}

/// `vtium-vitrify ARGS`: the exit status and stderr.
fn cli(args: &[&str]) -> (i32, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_vtium-vitrify"))
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the binary runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_name_list_does_not_give_fails_before_the_terminal_is_touched() {
    let store = concat!(env!("CARGO_TARGET_TMPDIR"), "/vtium-vitrify-set-no-store");
    let (status, err) = cli(&["--store", store, "set", "nosuch"]);
    assert_eq!(status, 1, "{err}");
    assert!(
        err.contains("no set is named nosuch") && err.contains("vtium-vitrify list"),
        "{err}"
    );
    let (status, err) = cli(&["set"]);
    assert_eq!(status, 2, "{err}");
    let (status, err) = cli(&["set", "a", "b"]);
    assert_eq!(status, 2, "{err}");
}

// Input left unread when raw mode ends, such as a reply that came late, is
// dropped, so it never reaches the shell's prompt.
#[test]
fn input_left_unread_is_dropped_before_the_mode_is_back() {
    let (mut master, slave) = pair();
    {
        let _raw = Raw::new(&slave).expect("raw mode");
        master.write_all(b"late\x1b]50;tiny\x07\n").expect("write");
        // the line discipline has taken the bytes once the slave can see them
        let start = now();
        while rustix::io::ioctl_fionread(&slave).expect("FIONREAD") == 0 {
            assert!(now() - start < Duration::from_secs(2), "no input came");
            std::thread::yield_now();
        }
    }
    assert_eq!(rustix::io::ioctl_fionread(&slave).expect("FIONREAD"), 0);
}

// A SIGTERM while `set` waits for the reply acts once the terminal's mode
// is back: the command ends by it, and the terminal is cooked again.
#[test]
fn a_sigterm_during_the_wait_ends_set_with_the_mode_back() {
    use std::os::fd::AsFd;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    let (mut master, slave) = pair();
    let store = concat!(env!("CARGO_TARGET_TMPDIR"), "/vtium-vitrify-set-no-store");
    // setsid --ctty makes the pty the command's controlling terminal, the
    // /dev/tty set opens
    let mut child = Command::new("setsid")
        .args([
            "--ctty",
            env!("CARGO_BIN_EXE_vtium-vitrify"),
            "--store",
            store,
        ])
        .args(["set", "unifont"])
        .stdin(Stdio::from(
            slave.as_fd().try_clone_to_owned().expect("dup"),
        ))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("setsid runs");
    // the request and the query, then the signal while it waits
    let mut got = Vec::new();
    let mut buf = [0u8; 64];
    while !got.windows(6).any(|w| w == b"\x1b]50;?") {
        let n = master.read(&mut buf).expect("the request");
        got.extend_from_slice(&buf[..n]);
    }
    let pid = rustix::process::Pid::from_raw(child.id() as i32).expect("a pid");
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).expect("kill");
    let status = child.wait().expect("wait");
    assert_eq!(status.signal(), Some(15), "{status:?}");
    assert!(
        tcgetattr(&slave)
            .expect("tcgetattr")
            .local_modes
            .contains(LocalModes::ICANON | LocalModes::ECHO),
        "the mode before is back"
    );
}
