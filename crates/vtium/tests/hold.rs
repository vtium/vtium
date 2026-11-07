//! The holder (ADR-1401 D13) as vtium starts it: the vtium binary run as
//! `vtium hold`, on a socket in a VT's run directory, keeping a session's
//! pty master from one vtium for the next and exiting once it holds none.

use std::os::fd::AsFd;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::process::{Pid, PidfdFlags, Signal, WaitOptions, pidfd_open, waitpid};
use vtium::survive::{Kept, Name, hold};
use vtium_platform::pty::{Erase, Pty, Size};

/// The directory the holder's run directory goes in: `vtium-hold-<pid>`
/// under the system's temporary directory, removed when the test ends.
/// The build's own temporary directory, where other tests keep their
/// scratch, can sit deep enough that the socket's path passes the 108
/// bytes a Unix socket's address holds, as it does under the gates' target
/// directories. vtium's own run directory, `/run/vtium/ttyN`, is short.
struct ShortDir(std::path::PathBuf);

impl ShortDir {
    fn new() -> ShortDir {
        let dir = std::env::temp_dir().join(format!("vtium-hold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test's short directory");
        ShortDir(dir)
    }
}

impl Drop for ShortDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SIZE: Size = Size {
    cols: 80,
    rows: 25,
    cell_width: 8,
    cell_height: 16,
};

/// The holder's process: the test thread's child running `vtium hold`.
/// The socket's credentials name the process that bound it, vtium, so
/// they cannot name the holder.
fn holder_pid() -> Pid {
    let children = std::fs::read_to_string("/proc/thread-self/children").expect("the children");
    children
        .split_whitespace()
        .filter_map(|p| p.parse::<i32>().ok())
        .find(|p| {
            std::fs::read(format!("/proc/{p}/cmdline")).is_ok_and(|c| c.ends_with(b"\0hold\0"))
        })
        .and_then(Pid::from_raw)
        .expect("the holder runs")
}

#[test]
fn the_holder_keeps_a_session_from_one_vtium_for_the_next() {
    let dir = ShortDir::new();
    let run = dir.0.join("tty9");
    let exe = Path::new(env!("CARGO_BIN_EXE_vtium"));
    let mut sleeper = Command::new("sleep");
    sleeper.arg("1000");
    let pty = Pty::spawn(sleeper, SIZE, Erase::Delete).expect("a session");
    let name = Name::new("console", 1).encode();

    let (first, kept) = hold::start(&run, exe).expect("the first vtium starts a holder");
    assert!(kept.is_empty(), "a new holder holds nothing");
    // The holder answers once it runs as `vtium hold`, so it can be found.
    assert!(first.take().expect("the holder answers").is_empty());
    let holder = holder_pid();
    let mode = std::fs::metadata(hold::socket_path(&run))
        .expect("the socket")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "only root may open the holder's socket"
    );
    first.keep(&name, pty.master()).expect("the master kept");
    // the first vtium ends
    drop(first);

    let (next, kept) = hold::start(&run, exe).expect("the next vtium finds the holder");
    assert_eq!(holder_pid(), holder, "the same holder answers");
    let mut kept = Kept::new(kept);
    let back = kept.take("console", 1).expect("the holder kept console 1");
    let adopted = Pty::adopt(back).expect("the session still runs");
    let pts = |fd| rustix::pty::ptsname(fd, Vec::new()).expect("the pty's name");
    assert_eq!(pts(adopted.master()), pts(pty.master()), "the same pty");
    adopted.release();

    next.forget(&name).expect("the master forgotten");
    drop(next);
    let pidfd = pidfd_open(holder, PidfdFlags::empty()).expect("the holder's pidfd");
    let mut fds = [PollFd::new(&pidfd, PollFlags::IN)];
    let timeout = rustix::time::Timespec {
        tv_sec: 10,
        tv_nsec: 0,
    };
    assert_eq!(
        poll(&mut fds, Some(&timeout)).expect("a poll"),
        1,
        "the holder exits once it holds nothing"
    );
    let _ = waitpid(Some(holder), WaitOptions::empty());
    let _ = rustix::process::pidfd_send_signal(pidfd.as_fd(), Signal::KILL);
    drop(pty);
}
