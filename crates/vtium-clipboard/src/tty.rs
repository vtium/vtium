//! The program's own terminal, `/dev/tty`: the copy and the two questions
//! go to it, and the replies come back from it.
//!
//! The program writes to its controlling terminal and never to its
//! standard output, so `vticlip copy < notes.txt > log` still reaches the
//! console. While it waits for the replies, the terminal is in
//! noncanonical mode with echo off, so the replies reach the program at
//! once and never show on the screen. Signal keys are off too, so Ctrl+C
//! during the wait cannot end the program before it puts the terminal's
//! mode back. The wait lasts at most [`REPLY_WAIT`](crate::REPLY_WAIT).
//!
//! References:
//! - ADR-1007 D2: `/dev/tty`, never standard output, and the mode for the
//!   replies.

use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use rustix::termios::{
    LocalModes, OptionalActions, QueueSelector, SpecialCodeIndex, Termios, tcflush, tcgetattr,
    tcsetattr,
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
}

/// The terminal's mode while the program reads the replies, until it drops.
/// Drop puts the mode before back. When the wait ran out, Drop first
/// discards the input left unread, a reply cut off partway, so it never
/// reaches the shell's prompt. After a full answer, input left unread is
/// typed by the person, and it stays for the shell.
struct Waiting<'a> {
    tty: &'a File,
    saved: Termios,
    timed_out: bool,
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
        // a read returns what has come, or nothing after a tenth of a second
        waiting.special_codes[SpecialCodeIndex::VMIN] = 0;
        waiting.special_codes[SpecialCodeIndex::VTIME] = 1;
        tcsetattr(tty, OptionalActions::Now, &waiting).ok()?;
        Some(Waiting {
            tty,
            saved,
            timed_out: false,
        })
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if self.timed_out {
            let _ = tcflush(self.tty, QueueSelector::IFlush);
        }
        let _ = tcsetattr(self.tty, OptionalActions::Now, &self.saved);
    }
}

/// Opens `/dev/tty`, writes `bytes` to it and returns what the terminal
/// sent back before its DA1 reply ended or `wait` ran out. Once the bytes
/// are written, the copy is sent, whatever comes back.
pub fn exchange(bytes: &[u8], wait: Duration) -> Result<Vec<u8>, Failed> {
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(Failed::Open)?;
    let mut waiting = Waiting::new(&tty);
    (&tty).write_all(bytes).map_err(Failed::Write)?;
    let _ = (&tty).flush();
    let Some(waiting) = waiting.as_mut() else {
        return Ok(Vec::new());
    };
    let replies = read_replies(&tty, wait, &mut waiting.timed_out);
    Ok(replies)
}

/// Reads the replies until the DA1 reply has come, [`REPLIES_MAX`] bytes
/// have come, or `wait` runs out, which sets `timed_out`. The clock bounds
/// only how long the program waits, and the replies decide the rest.
#[expect(
    clippy::disallowed_methods,
    reason = "the wait for the replies is bounded in time (ADR-1007 D2)"
)]
fn read_replies(mut tty: &File, wait: Duration, timed_out: &mut bool) -> Vec<u8> {
    let start = Instant::now();
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    while !answers(&got).da1 && got.len() < REPLIES_MAX {
        if start.elapsed() >= wait {
            *timed_out = true;
            break;
        }
        match tty.read(&mut buf) {
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    got
}
