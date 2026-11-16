//! vtium-clipboard run on a pty that is its controlling terminal, as
//! `setsid --ctty` makes it. A thread on the pty's other end plays the
//! terminal: vtium-term's terminal as vtium sets it up, the same terminal
//! under another name, a terminal that never answers, one that floods the
//! program with bytes, or a person who presses Ctrl+C. Each test checks the
//! exit status, the bytes that reached the terminal, and that the
//! terminal's mode is back as it was. Other tests leave the pty's other end
//! unread, as a terminal that stops taking output does.
//!
//! References:
//! - ADR-1007 D3: the exit statuses and their messages.
//! - ADR-1007 D4: vtium, another terminal and no answer exit 0, 3 and 3.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::process::{Pid, Signal, kill_process};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{
    LocalModes, OptionalActions, SpecialCodeIndex, Termios, tcgetattr, tcsetattr,
};
use vtium_clipboard::{CLIPBOARD_MAX, QUERIES, REPLIES_MAX, WRITE_STALL, sequence};
use vtium_term::Terminal;

const BIN: &str = env!("CARGO_BIN_EXE_vtium-clipboard");

/// A pty: its master, and its slave opened without becoming this
/// process's controlling terminal.
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

/// Who plays the terminal on the pty's master.
#[derive(Clone, Copy)]
enum Other {
    /// vtium-term's terminal, as vtium's console sets it up: XTVERSION
    /// answers vtium(VERSION).
    Vtium,
    /// The same terminal answering XTVERSION as tmux, which answers for
    /// itself.
    Tmux,
    /// A terminal that never answers.
    Silent,
    /// A terminal that answers the questions with more bytes than the
    /// program keeps, and no DA1 reply.
    Flood,
    /// A terminal that never answers, while the person presses Ctrl+C once
    /// the questions have come.
    CtrlC,
}

/// Plays `other` on `master` until the slave's last holder closes it, and
/// returns every byte the program wrote.
fn play(mut master: File, other: Other) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut t = Terminal::new(80, 24, 0);
        if let Other::Tmux = other {
            t.set_version("tmux 3.5a").expect("printable");
        }
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        // the read fails with EIO once no process holds the slave
        while let Ok(n @ 1..) = master.read(&mut buf) {
            got.extend_from_slice(&buf[..n]);
            let asked = got.ends_with(QUERIES);
            match other {
                Other::Vtium | Other::Tmux => {
                    t.input(&buf[..n]);
                    let replies = t.take_replies();
                    if !replies.is_empty() {
                        master.write_all(&replies).expect("the replies");
                    }
                }
                Other::Silent => {}
                Other::Flood if asked => {
                    let flood = vec![b'x'; REPLIES_MAX + 1000];
                    master.write_all(&flood).expect("the flood");
                }
                Other::CtrlC if asked => master.write_all(b"\x03").expect("Ctrl+C"),
                Other::Flood | Other::CtrlC => {}
            }
        }
        got
    })
}

/// A file under the test's temporary directory holding `text`.
fn text_file(name: &str, text: &[u8]) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, text).expect("the text file");
    path
}

/// Whether the terminal's mode is back to canonical with echo and signals.
fn mode_back(slave: &File) -> bool {
    let modes = tcgetattr(slave).expect("tcgetattr").local_modes;
    modes.contains(LocalModes::ICANON | LocalModes::ECHO | LocalModes::ISIG)
}

/// The input the terminal still holds for the next program, such as the
/// shell: read in noncanonical mode, so that a line cut off partway counts
/// too, with the mode put back afterwards.
fn unread(mut slave: &File) -> Vec<u8> {
    let before: Termios = tcgetattr(slave).expect("tcgetattr");
    let mut raw = before.clone();
    raw.local_modes
        .remove(LocalModes::ICANON | LocalModes::ECHO);
    raw.special_codes[SpecialCodeIndex::VMIN] = 0;
    raw.special_codes[SpecialCodeIndex::VTIME] = 0;
    tcsetattr(slave, OptionalActions::Now, &raw).expect("tcsetattr");
    let mut left = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n @ 1..) = slave.read(&mut buf) {
        left.extend_from_slice(&buf[..n]);
    }
    tcsetattr(slave, OptionalActions::Now, &before).expect("tcsetattr");
    left
}

/// What a run on the pty left behind.
struct Run {
    /// The program's exit status and its standard output and error.
    out: Output,
    /// Every byte that reached the terminal.
    written: Vec<u8>,
    /// Whether the terminal's mode was back once the program ended.
    back: bool,
    /// The input left for the next program once the program ended.
    left: Vec<u8>,
}

/// Runs `program ARGS` with the pty as its controlling terminal and as its
/// standard input, while `other` plays the terminal.
fn on_pty(program: &str, args: &[&str], other: Other) -> Run {
    let (master, slave) = pair();
    let player = play(master, other);
    let out = Command::new("setsid")
        .arg("--ctty")
        .arg(program)
        .args(args)
        .stdin(Stdio::from(
            slave.as_fd().try_clone_to_owned().expect("dup"),
        ))
        .output()
        .expect("setsid runs");
    let back = mode_back(&slave);
    let left = unread(&slave);
    drop(slave);
    let written = player.join().expect("the terminal's thread");
    Run {
        out,
        written,
        back,
        left,
    }
}

/// Starts `BIN ARGS` with the pty's slave as its controlling terminal,
/// while nobody reads the pty's master, so the terminal stops taking what
/// the program writes once the pty's buffer is full.
fn start_unread(slave: &File, args: &[&str]) -> Child {
    Command::new("setsid")
        .arg("--ctty")
        .arg(BIN)
        .args(args)
        .stdin(Stdio::from(
            slave.as_fd().try_clone_to_owned().expect("dup"),
        ))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("setsid runs")
}

/// Waits until the program has written its first bytes to `master`, so
/// that it is writing the copy. The bound only keeps a broken test from
/// hanging.
fn wait_for_output(master: &File) {
    let bound = Timespec::try_from(Duration::from_secs(10)).expect("a time");
    let mut fds = [PollFd::new(master, PollFlags::IN)];
    let ready = poll(&mut fds, Some(&bound)).expect("poll");
    assert_eq!(ready, 1, "the program wrote nothing");
}

/// The program's output once it ends, or `None` when it is still running
/// after `bound`; it is then killed. The bound only keeps a test from
/// hanging on a program that does not end.
fn ended_within(child: Child, bound: Duration) -> Option<Output> {
    let pid = Pid::from_child(&child);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(child.wait_with_output()));
    match rx.recv_timeout(bound) {
        Ok(out) => Some(out.expect("the program's output")),
        Err(_) => {
            let _ = kill_process(pid, Signal::KILL);
            None
        }
    }
}

/// What the program writes to the terminal for a text whose base64 is
/// `base64`: the OSC 52 copy, then XTVERSION's and DA1's questions.
fn written_for(base64: &str) -> Vec<u8> {
    format!("\x1b]52;c;{base64}\x1b\\\x1b[>0q\x1b[c").into_bytes()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ADR-1007 D3: on vtium the copy exits 0 with no message, and the
// terminal received exactly the copy and the two questions.
// covers: clipboard.copy-tool
#[test]
fn a_copy_that_vtium_answers_exits_0() {
    let file = text_file("vtium.txt", b"hello\n");
    let run = on_pty(BIN, &["copy", file.to_str().unwrap()], Other::Vtium);
    assert_eq!(run.out.status.code(), Some(0), "{}", stderr(&run.out));
    assert_eq!(stderr(&run.out), "");
    assert_eq!(run.out.stdout, b"");
    assert_eq!(run.written, written_for("aGVsbG8K"));
    assert!(run.back, "the terminal's mode is back");
}

// ADR-1007 D3: another terminal's answer, and no answer within the wait,
// both exit 3 with the same message, after the copy was written.
#[test]
fn a_copy_that_another_terminal_or_nobody_answers_exits_3() {
    let file = text_file("other.txt", b"hello");
    for other in [Other::Tmux, Other::Silent] {
        let run = on_pty(BIN, &["copy", file.to_str().unwrap()], other);
        assert_eq!(run.out.status.code(), Some(3), "{}", stderr(&run.out));
        assert_eq!(
            stderr(&run.out),
            "vtium-clipboard: The copy was sent, but this terminal did not answer as vtium, so the text may not have reached a clipboard.\n"
        );
        assert_eq!(run.written, written_for("aGVsbG8="));
        assert!(run.back, "the terminal's mode is back");
    }
}

// ADR-1007 D2: without FILE the text comes from standard input, and text
// that is not valid UTF-8 is copied as it is, with a note.
#[test]
fn copy_reads_standard_input_and_notes_text_that_is_not_utf_8() {
    let file = text_file("stdin.txt", b"h\xffi");
    let (master, slave) = pair();
    let player = play(master, Other::Vtium);
    let out = Command::new("setsid")
        .args(["--ctty", "sh", "-c", "exec \"$0\" copy < \"$1\"", BIN])
        .arg(&file)
        .stdin(Stdio::from(
            slave.as_fd().try_clone_to_owned().expect("dup"),
        ))
        .output()
        .expect("setsid runs");
    drop(slave);
    let written = player.join().expect("the terminal's thread");
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stderr(&out),
        "vtium-clipboard: The text is not all valid UTF-8. vtium shows the replacement character U+FFFD in place of the bytes that are not valid UTF-8.\n"
    );
    assert_eq!(written, written_for("aP9p"));
}

// ADR-1007 D2 and D3: text one byte past the bound is refused before
// anything reaches the terminal, and exits 1.
#[test]
fn text_past_the_bound_writes_nothing_and_exits_1() {
    let file = text_file("large.txt", &vec![b'a'; CLIPBOARD_MAX + 1]);
    let run = on_pty(BIN, &["copy", file.to_str().unwrap()], Other::Vtium);
    assert_eq!(run.out.status.code(), Some(1));
    assert_eq!(
        stderr(&run.out),
        "vtium-clipboard: The text is more than 524288 bytes, and vtium's clipboard holds at most 524288 bytes. Nothing was copied.\n"
    );
    assert_eq!(run.written, b"");
}

// ADR-1007 D2 and D3: a file that cannot be read, and a process with no
// controlling terminal, as under cron, exit 1.
#[test]
fn an_unreadable_file_or_no_terminal_exits_1() {
    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-such-file");
    let Run { out, written, .. } = on_pty(BIN, &["copy", missing.to_str().unwrap()], Other::Vtium);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with(&format!(
            "vtium-clipboard: The file {} cannot be read: ",
            missing.display()
        )),
        "{}",
        stderr(&out)
    );
    assert!(stderr(&out).ends_with(". Nothing was copied.\n"));
    assert_eq!(written, b"");
    // setsid without --ctty: a new session with no controlling terminal
    let out = Command::new("setsid")
        .args([BIN, "copy"])
        .stdin(Stdio::null())
        .output()
        .expect("setsid runs");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with(
            "vtium-clipboard: This process has no terminal: /dev/tty cannot be opened ("
        ),
        "{}",
        stderr(&out)
    );
}

// ADR-1007 D1 and D3: paste and a wrong command line exit 2 and write
// nothing to the terminal. Under the name vticlip, the messages name
// vticlip.
#[test]
fn paste_and_a_wrong_command_line_exit_2_under_the_name_called() {
    let link = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vticlip");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(BIN, &link).expect("the link");
    let link = link.to_str().unwrap();
    let Run { out, written, .. } = on_pty(link, &["paste"], Other::Vtium);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        "vticlip: Paste is not available yet: vtium does not let programs read the clipboard. Paste with Shift+Insert or the middle mouse button.\n"
    );
    assert_eq!(written, b"");
    for args in [&[][..], &["cut"], &["copy", "a", "b"], &["--help", "copy"]] {
        let out = Command::new(link).args(args).output().expect("runs");
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(stderr(&out).starts_with("vticlip: "), "{}", stderr(&out));
        assert_eq!(out.stdout, b"");
    }
    let help = Command::new(link).arg("--help").output().expect("runs");
    assert_eq!(help.status.code(), Some(0));
    let page = String::from_utf8(help.stdout).expect("UTF-8");
    assert!(page.starts_with("Usage: vticlip copy [FILE]\n"), "{page}");
    assert!(page.contains("clipboard-write is off"), "{page}");
    let version = Command::new(link).arg("--version").output().expect("runs");
    assert_eq!(
        String::from_utf8_lossy(&version.stdout),
        format!("vticlip {}\n", env!("CARGO_PKG_VERSION"))
    );
}

// Text that fits the bound as it is, but not once vtium has put U+FFFD,
// 3 bytes, in place of each byte that is not valid UTF-8, is refused
// before anything reaches the terminal, and exits 1: vtium would drop it.
#[test]
fn text_past_the_bound_once_replaced_writes_nothing_and_exits_1() {
    let file = text_file("replaced.txt", &vec![0xff; 200_000]);
    let run = on_pty(BIN, &["copy", file.to_str().unwrap()], Other::Vtium);
    assert_eq!(run.out.status.code(), Some(1), "{}", stderr(&run.out));
    assert_eq!(
        stderr(&run.out),
        "vtium-clipboard: The text is too large once vtium replaces its bytes that are not valid UTF-8. vtium puts the replacement character U+FFFD, which takes 3 bytes, in place of each part that is not valid UTF-8, and that makes the text 600000 bytes. vtium's clipboard holds at most 524288 bytes. Nothing was copied.\n"
    );
    assert_eq!(run.written, b"");
}

// Ctrl+C ends a copy that the terminal does not take, as over a stalled
// ssh connection: the program writes the copy with the terminal's mode as
// it found it, so the signal key works and leaves that mode as it was.
#[test]
fn ctrl_c_ends_a_copy_that_the_terminal_does_not_take() {
    let file = text_file("stalled-ctrl-c.txt", &vec![b'a'; 400_000]);
    let (mut master, slave) = pair();
    let child = start_unread(&slave, &["copy", file.to_str().unwrap()]);
    wait_for_output(&master);
    master.write_all(b"\x03").expect("Ctrl+C");
    let out = ended_within(child, Duration::from_secs(3)).expect("Ctrl+C ends the copy");
    assert_eq!(out.status.signal(), Some(2), "SIGINT ends it: {out:?}");
    assert!(mode_back(&slave), "the terminal's mode is as it was");
}

// A copy that the terminal stops taking ends once the terminal has taken
// nothing for WRITE_STALL, and exits 1 with the terminal's mode as it was.
#[test]
fn a_copy_that_the_terminal_stops_taking_ends_and_exits_1() {
    let file = text_file("stalled.txt", &vec![b'a'; 400_000]);
    let (master, slave) = pair();
    let child = start_unread(&slave, &["copy", file.to_str().unwrap()]);
    wait_for_output(&master);
    let bound = WRITE_STALL.saturating_add(Duration::from_secs(5));
    let out = ended_within(child, bound).expect("the copy ends");
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(
        stderr(&out),
        "vtium-clipboard: The terminal /dev/tty took nothing of the copy for 5 seconds, so vtium-clipboard stopped writing it. vtium-clipboard cannot tell how much of it was sent, so the clipboard may hold nothing new.\n"
    );
    assert!(mode_back(&slave), "the terminal's mode is as it was");
}

// Ctrl+C while the program waits for the replies is an ordinary byte: the
// signal keys are off, so the program cannot end before it puts the
// terminal's mode back. The terminal never answers, so it exits 3.
#[test]
fn ctrl_c_during_the_wait_for_the_replies_does_not_end_the_program() {
    let file = text_file("wait-ctrl-c.txt", b"hello");
    let run = on_pty(BIN, &["copy", file.to_str().unwrap()], Other::CtrlC);
    assert_eq!(run.out.status.signal(), None, "{:?}", run.out);
    assert_eq!(run.out.status.code(), Some(3), "{}", stderr(&run.out));
    assert_eq!(run.written, written_for("aGVsbG8="));
    assert!(run.back, "the terminal's mode is back");
    assert_eq!(run.left, b"", "the Ctrl+C was read with the replies");
}

// A terminal that sends more than REPLIES_MAX bytes and no DA1 reply ends
// the wait early. The program discards the rest of what it sent, so none
// of it reaches the shell as if it had been typed.
#[test]
fn a_flood_of_replies_leaves_nothing_for_the_shell() {
    let file = text_file("flood.txt", b"hello");
    let run = on_pty(BIN, &["copy", file.to_str().unwrap()], Other::Flood);
    assert_eq!(run.out.status.code(), Some(3), "{}", stderr(&run.out));
    assert!(run.back, "the terminal's mode is back");
    assert_eq!(run.left.len(), 0, "the flood is discarded");
}

// A copy in a background job of a shell with job control writes the copy
// and asks nothing, because a background job that sets the terminal's
// mode or reads from the terminal is stopped until it is brought to the
// foreground. It exits 3, with the terminal's mode as it was.
#[test]
fn a_copy_in_a_background_job_writes_the_copy_and_exits_3() {
    let file = text_file("background.txt", b"hello");
    let (master, slave) = pair();
    let player = play(master, Other::Vtium);
    // The shell kills the program if a stop leaves it behind, so that the
    // test ends either way. The program's standard error goes to the pipe
    // through descriptor 3, and the shell's own goes to /dev/null: with job
    // control on, some versions of bash report the job's end there
    // ("[1]+  Exit 3"), and that line is the shell's, not the program's.
    let script = "exec 3>&2 2>/dev/null; set -m; \"$0\" copy \"$1\" 2>&3 & pid=$!; wait $pid; status=$?; kill -KILL $pid; exit $status";
    let child = Command::new("setsid")
        .args(["--ctty", "bash", "-c", script, BIN])
        .arg(&file)
        .stdin(Stdio::from(
            slave.as_fd().try_clone_to_owned().expect("dup"),
        ))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("setsid runs");
    let out = ended_within(child, Duration::from_secs(10)).expect("the shell ends");
    let back = mode_back(&slave);
    drop(slave);
    let written = player.join().expect("the terminal's thread");
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert_eq!(
        stderr(&out),
        "vtium-clipboard: The copy was sent, but vtium-clipboard runs in a background job, so it cannot wait for this terminal's answer. It cannot tell whether the terminal is vtium, so the text may not have reached a clipboard.\n"
    );
    assert_eq!(written, sequence(b"hello"));
    assert!(back, "the terminal's mode is as it was");
}
