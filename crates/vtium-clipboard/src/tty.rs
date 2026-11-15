//! The program's own terminal, `/dev/tty`: the copy and the two questions
//! go to it, and the replies come back from it.
//!
//! The program writes to its controlling terminal and never to its
//! standard output, so `vticlip copy < notes.txt > log` still reaches the
//! console. It writes the copy with the terminal's mode as it found it, so
//! Ctrl+C can still end a copy that the terminal is slow to take. Each
//! write waits at most [`WRITE_STALL`](crate::WRITE_STALL) for the terminal
//! to take another byte, so a terminal that stops reading cannot hold the
//! program forever.
//!
//! Only then does the program set the mode for the replies and ask its two
//! questions. While it waits for the replies, the terminal is in
//! noncanonical mode with echo off, so the replies reach the program at
//! once and never show on the screen. Signal keys are off too, so Ctrl+C
//! during the wait cannot end the program before it puts the terminal's
//! mode back. The wait lasts at most [`REPLY_WAIT`](crate::REPLY_WAIT).
//!
//! A program in a background job may not change the terminal's mode or
//! read from the terminal: the kernel stops it until the shell brings it to
//! the foreground. In the background, the program therefore writes the copy
//! and asks nothing.
//!
//! References:
//! - ADR-1007 D2: `/dev/tty`, never standard output, and the mode for the
//!   replies.

use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::OFlags;
use rustix::termios::{
    LocalModes, OptionalActions, QueueSelector, SpecialCodeIndex, Termios, tcflush, tcgetattr,
    tcgetpgrp, tcsetattr,
};

use crate::{REPLIES_MAX, answers};

/// Why the program could not send the copy.
#[derive(Debug)]
pub enum Failed {
    /// `/dev/tty` would not open: the process has no controlling terminal,
    /// as under cron.
    Open(io::Error),
    /// The terminal would not take the bytes.
    Write(io::Error),
    /// The terminal took no byte for the time the program allows, so the
    /// program gave up partway through the write.
    Stalled,
}

/// What came of an exchange whose bytes were all written.
#[derive(Debug, PartialEq, Eq)]
pub enum Sent {
    /// The program asked its questions, and these bytes came back before
    /// the DA1 reply ended them or the wait ran out.
    Replies(Vec<u8>),
    /// The program runs in a background job, so it wrote the copy and asked
    /// nothing.
    Background,
}

/// The terminal's mode while the program reads the replies, until it drops.
/// Drop puts the mode before back. Unless the replies ended with the DA1
/// reply, Drop first discards the input left unread, because that input can
/// be the rest of a reply that came late or was cut off by
/// [`REPLIES_MAX`]. Discarded, it never reaches the shell's prompt as if it
/// had been typed. After a full answer, input left unread was typed by the
/// person, and it stays for the shell.
struct Waiting<'a> {
    tty: &'a File,
    saved: Termios,
    discard: bool,
}

impl<'a> Waiting<'a> {
    /// Sets the mode, or `None` when the terminal's mode cannot be read or
    /// set; the program then reads no reply.
    fn new(tty: &'a File) -> Option<Waiting<'a>> {
        let saved = tcgetattr(tty).ok()?;
        let mut waiting = saved.clone();
        waiting
            .local_modes
            .remove(LocalModes::ICANON | LocalModes::ECHO | LocalModes::ISIG);
        // A read returns what has come. The file is non-blocking, so a read
        // with nothing to return fails with WouldBlock, and a read that
        // returns nothing means the terminal hung up.
        waiting.special_codes[SpecialCodeIndex::VMIN] = 1;
        waiting.special_codes[SpecialCodeIndex::VTIME] = 0;
        tcsetattr(tty, OptionalActions::Now, &waiting).ok()?;
        Some(Waiting {
            tty,
            saved,
            discard: true,
        })
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if self.discard {
            let _ = tcflush(self.tty, QueueSelector::IFlush);
        }
        let _ = tcsetattr(self.tty, OptionalActions::Now, &self.saved);
    }
}

/// Opens `/dev/tty` and runs [`exchange_on`] on it.
pub fn exchange(
    copy: &[u8],
    queries: &[u8],
    wait: Duration,
    stall: Duration,
) -> Result<Sent, Failed> {
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(OFlags::NONBLOCK.bits().cast_signed())
        .open("/dev/tty")
        .map_err(Failed::Open)?;
    exchange_on(&tty, copy, queries, wait, stall)
}

/// Writes `copy` to `tty` with its mode as it is, then sets the mode for
/// the replies, writes `queries`, and returns what the terminal sent back
/// before its DA1 reply ended or `wait` ran out. Each write gives up once
/// the terminal has taken no byte for `stall`. `tty` must be open
/// non-blocking. Once `copy` is written, the copy is sent, whatever comes
/// back.
fn exchange_on(
    tty: &File,
    copy: &[u8],
    queries: &[u8],
    wait: Duration,
    stall: Duration,
) -> Result<Sent, Failed> {
    if in_background(tty) {
        write_all(tty, copy, stall)?;
        return Ok(Sent::Background);
    }
    write_all(tty, copy, stall)?;
    let mut waiting = Waiting::new(tty);
    write_all(tty, queries, stall)?;
    let Some(waiting) = waiting.as_mut() else {
        return Ok(Sent::Replies(Vec::new()));
    };
    let replies = read_replies(tty, wait);
    waiting.discard = !answers(&replies).da1;
    Ok(Sent::Replies(replies))
}

/// Whether the program runs in a background job of `tty`: the terminal's
/// foreground process group is not the program's own. When the terminal
/// cannot say, the program counts as in the foreground.
fn in_background(tty: &File) -> bool {
    tcgetpgrp(tty).is_ok_and(|foreground| foreground != rustix::process::getpgrp())
}

/// Writes all of `bytes` to the non-blocking `tty`. When the terminal takes
/// nothing, the program waits until it can write again, but gives up once
/// the terminal has taken no byte for `stall`.
fn write_all(mut tty: &File, bytes: &[u8], stall: Duration) -> Result<(), Failed> {
    let mut rest = bytes;
    while !rest.is_empty() {
        match tty.write(rest) {
            Ok(0) => return Err(Failed::Write(ErrorKind::WriteZero.into())),
            Ok(n) => rest = rest.get(n..).unwrap_or_default(),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if !ready(tty, PollFlags::OUT, stall) {
                    return Err(Failed::Stalled);
                }
            }
            Err(e) => return Err(Failed::Write(e)),
        }
    }
    Ok(())
}

/// Waits at most `wait` for `tty` to be ready for `flags`, and says whether
/// it became ready. A signal that interrupts the wait counts as ready, and
/// the caller tries again.
fn ready(tty: &File, flags: PollFlags, wait: Duration) -> bool {
    let timeout = Timespec::try_from(wait).ok();
    let mut fds = [PollFd::new(tty, flags)];
    match poll(&mut fds, timeout.as_ref()) {
        Ok(n) => n > 0,
        Err(e) => e == rustix::io::Errno::INTR,
    }
}

/// Reads the replies until the DA1 reply has come, [`REPLIES_MAX`] bytes
/// have come, the terminal hangs up, or `wait` runs out. The clock bounds
/// only how long the program waits, and the replies decide the rest.
#[expect(
    clippy::disallowed_methods,
    reason = "the wait for the replies is bounded in time (ADR-1007 D2)"
)]
fn read_replies(mut tty: &File, wait: Duration) -> Vec<u8> {
    let start = Instant::now();
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    while !answers(&got).da1 && got.len() < REPLIES_MAX {
        let left = wait.saturating_sub(start.elapsed());
        if left.is_zero() || !ready(tty, PollFlags::IN, left) {
            break;
        }
        match tty.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => got.extend_from_slice(buf.get(..n).unwrap_or_default()),
            Err(e) if matches!(e.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) => {}
            Err(_) => break,
        }
    }
    got
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};

    /// A pty: its master, and its slave opened non-blocking without
    /// becoming this process's controlling terminal.
    fn pair() -> (File, File) {
        let master =
            openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC).expect("a pty");
        grantpt(&master).expect("grantpt");
        unlockpt(&master).expect("unlockpt");
        let name = ptsname(&master, Vec::new()).expect("ptsname");
        let slave = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags((OFlags::NOCTTY | OFlags::NONBLOCK).bits().cast_signed())
            .open(name.to_str().expect("a path"))
            .expect("the slave");
        (File::from(master), slave)
    }

    /// The input `tty` still holds for the next reader, read in
    /// noncanonical mode so that a line cut off partway counts too.
    fn unread(mut tty: &File) -> Vec<u8> {
        let mut mode = tcgetattr(tty).expect("tcgetattr");
        mode.local_modes.remove(LocalModes::ICANON);
        tcsetattr(tty, OptionalActions::Now, &mode).expect("tcsetattr");
        let mut left = Vec::new();
        let mut buf = [0u8; 256];
        while let Ok(n @ 1..) = tty.read(&mut buf) {
            left.extend_from_slice(&buf[..n]);
        }
        left
    }

    // A reply that comes after the wait has run out is discarded, so that
    // it never reaches the shell as if it had been typed. With no time to
    // wait, the program reads nothing, and the reply that is already there
    // stands for one that came late.
    #[test]
    fn a_wait_that_runs_out_discards_the_input_left_unread() {
        let (mut master, slave) = pair();
        master.write_all(b"\x1bP>|vt").expect("a reply cut off");
        let sent = exchange_on(&slave, b"", b"", Duration::ZERO, Duration::from_secs(1));
        assert_eq!(sent.expect("sent"), Sent::Replies(Vec::new()));
        assert_eq!(unread(&slave), b"");
    }
}
