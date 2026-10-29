//! `set NAME` (ADR-1200 D5): switches the running console among the frits
//! and the built-in fonts through xterm's font operation, OSC 50, on the
//! controlling terminal.
//!
//! NAME is checked first, as the console checks it: a built-in font, or a
//! frit's name with `NAME.vtfrit` a regular file in the frits' directory;
//! the command fails naming that file when it is neither. It then writes
//! `OSC 50 ; NAME ST` and the query `OSC 50 ; ? ST` to `/dev/tty`, and
//! reads the reply in raw mode for at most [`WAIT`], putting the
//! terminal's mode back after. SIGTERM, SIGHUP, SIGINT and SIGQUIT are
//! held while the terminal is raw, so one that comes then acts once the
//! mode is back; input left unread, such as a reply that came late, is
//! dropped first, so it never reaches the shell's prompt. It prints
//! `set NAME` and exits 0 when the reply names NAME; otherwise it says
//! what the console kept, or that no reply came: font operations are off
//! (`allow-font-ops`), or the terminal is not vtium.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::termios::{
    OptionalActions, QueueSelector, SpecialCodeIndex, Termios, tcflush, tcgetattr, tcsetattr,
};
use vtium_font::set::builtin;
use vtium_platform::calloop::signals::{Signal, Signals};

use crate::frit::{RESERVED, valid_name};

/// How long the reply is waited for.
pub const WAIT: Duration = Duration::from_secs(1);

/// The most bytes read while the reply is looked for.
const READ_MAX: usize = 4096;

/// What the terminal answered the query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The font in use, as the reply names it.
    Named(String),
    /// No reply within the wait.
    Silent,
}

/// The name the first OSC 50 reply in `bytes` gives, `OSC 50 ; NAME`
/// ended by ST or BEL, once it is whole.
pub fn reply_name(bytes: &[u8]) -> Option<&[u8]> {
    const OPEN: &[u8] = b"\x1b]50;";
    let at = bytes.windows(OPEN.len()).position(|w| w == OPEN)?;
    let rest = bytes.get(at.checked_add(OPEN.len())?..)?;
    let end = rest.iter().position(|&b| b == 0x07 || b == 0x1b)?;
    let name = rest.get(..end)?;
    match rest.get(end..)? {
        [0x07, ..] | [0x1b, b'\\', ..] => Some(name),
        _ => None,
    }
}

/// Asks the terminal behind `tty`, already raw, for `name`, and reads the
/// reply to the query until `deadline`.
pub fn switch(tty: &mut (impl Read + Write), name: &str, deadline: Instant) -> io::Result<Answer> {
    tty.write_all(format!("\x1b]50;{name}\x1b\\\x1b]50;?\x1b\\").as_bytes())?;
    tty.flush()?;
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    while now() < deadline && got.len() < READ_MAX {
        let n = match tty.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        got.extend_from_slice(buf.get(..n).unwrap_or_default());
        if let Some(name) = reply_name(&got) {
            return Ok(Answer::Named(String::from_utf8_lossy(name).into_owned()));
        }
    }
    Ok(Answer::Silent)
}

#[expect(
    clippy::disallowed_methods,
    reason = "the reply's wait is bounded in time (ADR-1200 D5)"
)]
fn now() -> Instant {
    Instant::now()
}

/// A terminal in raw mode, each read waiting at most a tenth of a second,
/// until it drops: the input left unread is dropped and the mode before
/// put back. Meanwhile the signals that end the command are held, and act
/// once it has dropped.
pub struct Raw<'a> {
    tty: &'a File,
    saved: Termios,
    /// Released after `drop` puts the mode back, as a field.
    _held: Signals,
}

impl<'a> Raw<'a> {
    pub fn new(tty: &'a File) -> io::Result<Raw<'a>> {
        let held = Signals::new(&[
            Signal::SIGTERM,
            Signal::SIGHUP,
            Signal::SIGINT,
            Signal::SIGQUIT,
        ])
        .map_err(io::Error::other)?;
        let saved = tcgetattr(tty.as_fd())?;
        let mut raw = saved.clone();
        raw.make_raw();
        raw.special_codes[SpecialCodeIndex::VMIN] = 0;
        raw.special_codes[SpecialCodeIndex::VTIME] = 1;
        tcsetattr(tty.as_fd(), OptionalActions::Now, &raw)?;
        Ok(Raw {
            tty,
            saved,
            _held: held,
        })
    }
}

impl Drop for Raw<'_> {
    fn drop(&mut self) {
        let _ = tcflush(self.tty.as_fd(), QueueSelector::IFlush);
        let _ = tcsetattr(self.tty.as_fd(), OptionalActions::Now, &self.saved);
    }
}

/// The frits' directory, set at build time with `VTIUM_FONT_FRITS` as
/// the console's is.
pub fn default_frits() -> PathBuf {
    PathBuf::from(option_env!("VTIUM_FONT_FRITS").unwrap_or("/etc/vtium/fonts"))
}

/// The frit file `name` names in `frits`, for a frit's name.
fn frit_file(frits: &Path, name: &str) -> Option<PathBuf> {
    valid_name(name).then(|| frits.join(format!("{name}.vtfrit")))
}

/// Whether the console takes `name`: a built-in font, or a frit's name
/// whose file in `frits` is a regular file.
fn known(name: &str, frits: &Path) -> bool {
    (RESERVED.contains(&name) && builtin(name).is_some())
        || frit_file(frits, name).is_some_and(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))
}

/// `set` with its words, the frits' directory `--frits` named, if any,
/// writing to `out` and `err`; returns the exit status.
pub fn command(
    words: &[String],
    frits: Option<&Path>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let [name] = words else {
        let _ = writeln!(
            err,
            "vtium-vitrify: set takes one frit's name\nTry vtium-vitrify --help."
        );
        return 2;
    };
    let frits = frits.map_or_else(default_frits, Path::to_owned);
    let fail = |err: &mut dyn Write, why: &str| {
        let _ = writeln!(err, "vtium-vitrify: {why}");
        1
    };
    if !known(name, &frits) {
        let file = frits.join(format!("{name}.vtfrit"));
        return fail(
            err,
            &format!(
                "no frit is named {name}: a frit is {}, and the built-in fonts are {}",
                file.display(),
                RESERVED.join(" and ")
            ),
        );
    }
    let tty = match OpenOptions::new().read(true).write(true).open("/dev/tty") {
        Ok(tty) => tty,
        Err(e) => {
            return fail(
                err,
                &format!("set needs the console's terminal: /dev/tty: {e}"),
            );
        }
    };
    let answer = Raw::new(&tty).and_then(|_raw| {
        let deadline = now().checked_add(WAIT).unwrap_or_else(now);
        switch(&mut &tty, name, deadline)
    });
    match answer {
        Ok(Answer::Named(n)) if n == *name => {
            let _ = writeln!(out, "set\t{name}");
            0
        }
        Ok(Answer::Named(n)) => {
            let _ = writeln!(
                out,
                "failed\t{name}\tthe console kept the font {n}; its log says why"
            );
            1
        }
        Ok(Answer::Silent) => {
            let _ = writeln!(
                out,
                "failed\t{name}\tno reply: the console's allow-font-ops is off, or the terminal is not vtium"
            );
            1
        }
        Err(e) => fail(err, &format!("/dev/tty: {e}")),
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn the_reply_is_found_whole_after_other_bytes() {
        assert_eq!(reply_name(b"\x1b]50;tiny\x1b\\"), Some(&b"tiny"[..]));
        assert_eq!(reply_name(b"typed\x1b]50;tiny\x07rest"), Some(&b"tiny"[..]));
        assert_eq!(reply_name(b"\x1b]50;\x07"), Some(&b""[..]));
        assert_eq!(reply_name(b"\x1b]50;tiny"), None, "not ended");
        assert_eq!(reply_name(b"\x1b]50;tiny\x1b"), None, "ST cut short");
        assert_eq!(reply_name(b"\x1b]51;tiny\x07"), None);
        assert_eq!(reply_name(b""), None);
    }

    /// A terminal that answers the query with `name` once it has read the
    /// request and the query.
    struct Answering {
        name: &'static [u8],
        written: Vec<u8>,
    }

    impl Write for Answering {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.written.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for Answering {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let reply = [b"\x1b]50;", self.name, b"\x07"].concat();
            let n = reply.len().min(buf.len());
            buf.get_mut(..n)
                .unwrap()
                .copy_from_slice(reply.get(..n).unwrap());
            Ok(n)
        }
    }

    #[test]
    fn the_request_and_the_query_are_written_and_the_reply_read() {
        let mut t = Answering {
            name: b"tiny",
            written: Vec::new(),
        };
        let deadline = now() + WAIT;
        assert_eq!(
            switch(&mut t, "tiny", deadline).unwrap(),
            Answer::Named("tiny".into())
        );
        assert_eq!(t.written, b"\x1b]50;tiny\x1b\\\x1b]50;?\x1b\\");
    }

    #[test]
    fn with_no_reply_the_wait_ends() {
        let mut silent = io::Cursor::new(Vec::new());
        let deadline = now() + Duration::from_millis(50);
        assert_eq!(
            switch(&mut silent, "tiny", deadline).unwrap(),
            Answer::Silent
        );
    }
}
