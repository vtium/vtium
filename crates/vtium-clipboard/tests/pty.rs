//! vtium-clipboard run on a pty that is its controlling terminal, as
//! `setsid --ctty` makes it. A thread on the pty's other end plays the
//! terminal: vtium-term's terminal as vtium sets it up, the same terminal
//! under another name, or a terminal that never answers. Each test checks
//! the exit status, the bytes that reached the terminal, and that the
//! terminal's mode is back as it was.
//!
//! References:
//! - ADR-1007 D3: the exit statuses and their messages.
//! - ADR-1007 D4: vtium, another terminal and no answer exit 0, 3 and 3.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread::JoinHandle;

use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{LocalModes, tcgetattr};
use vtium_clipboard::CLIPBOARD_MAX;
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
            if let Other::Silent = other {
                continue;
            }
            t.input(&buf[..n]);
            let replies = t.take_replies();
            if !replies.is_empty() {
                master.write_all(&replies).expect("the replies");
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

/// Runs `program ARGS` with the pty as its controlling terminal and as its
/// standard input, while `other` plays the terminal. Returns its output,
/// every byte that reached the terminal, and whether the terminal's mode
/// was back to canonical with echo and signals once it ended.
fn on_pty(program: &str, args: &[&str], other: Other) -> (Output, Vec<u8>, bool) {
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
    let modes = tcgetattr(&slave).expect("tcgetattr").local_modes;
    let back = modes.contains(LocalModes::ICANON | LocalModes::ECHO | LocalModes::ISIG);
    drop(slave);
    let written = player.join().expect("the terminal's thread");
    (out, written, back)
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
    let (out, written, back) = on_pty(BIN, &["copy", file.to_str().unwrap()], Other::Vtium);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
    assert_eq!(out.stdout, b"");
    assert_eq!(written, written_for("aGVsbG8K"));
    assert!(back, "the terminal's mode is back");
}

// ADR-1007 D3: another terminal's answer, and no answer within the wait,
// both exit 3 with the same message, after the copy was written.
#[test]
fn a_copy_that_another_terminal_or_nobody_answers_exits_3() {
    let file = text_file("other.txt", b"hello");
    for other in [Other::Tmux, Other::Silent] {
        let (out, written, back) = on_pty(BIN, &["copy", file.to_str().unwrap()], other);
        assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
        assert_eq!(
            stderr(&out),
            "vtium-clipboard: The copy was sent, but this terminal did not answer as vtium, so the text may not have reached a clipboard.\n"
        );
        assert_eq!(written, written_for("aGVsbG8="));
        assert!(back, "the terminal's mode is back");
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
    let (out, written, _) = on_pty(BIN, &["copy", file.to_str().unwrap()], Other::Vtium);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr(&out),
        "vtium-clipboard: The text is more than 524288 bytes, and vtium's clipboard holds at most 524288 bytes. Nothing was copied.\n"
    );
    assert_eq!(written, b"");
}

// ADR-1007 D2 and D3: a file that cannot be read, and a process with no
// controlling terminal, as under cron, exit 1.
#[test]
fn an_unreadable_file_or_no_terminal_exits_1() {
    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-such-file");
    let (out, written, _) = on_pty(BIN, &["copy", missing.to_str().unwrap()], Other::Vtium);
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
    let (out, written, _) = on_pty(link, &["paste"], Other::Vtium);
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
