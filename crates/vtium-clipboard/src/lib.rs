//! vtium-clipboard copies text to the console's clipboard from the command
//! line, as `pbcopy` does on macOS: `git rev-parse HEAD | vticlip copy`.
//!
//! The program reads the text from a file or from its standard input,
//! encodes it in base64 and writes it to its own terminal as an OSC 52
//! sequence, the same sequence that tmux and vim send to copy text. vtium
//! takes that sequence into its one selection, so the copy also works over
//! ssh and through a multiplexer that passes OSC 52 on. After the sequence
//! the program asks the terminal who it is, and its exit status says
//! whether vtium answered.
//!
//! The program never reads the clipboard back. `paste` prints that paste is
//! not available and exits 2.
//!
//! References:
//! - ADR-1007: the program, its two names, what copy writes, its messages
//!   and its exit statuses, and paste left out while the decision on
//!   reading the clipboard is open.
#![forbid(unsafe_code)]

mod messages;
mod tty;

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

pub use vtium_term::CLIPBOARD_MAX;

/// How long the program waits for the terminal's replies. A terminal that
/// sends no reply within this time counts as not vtium.
///
/// References:
/// - ADR-1007 D2: the one-second wait.
pub const REPLY_WAIT: Duration = Duration::from_secs(1);

/// The most reply bytes the program keeps while it waits. vtium's two
/// replies take under 40 bytes, so a terminal that sends more than this
/// is not answering as vtium, and the program stops reading.
pub const REPLIES_MAX: usize = 4096;

/// The two questions the program asks after the copy: XTVERSION,
/// `CSI > 0 q`, which vtium answers with its name, then DA1, `CSI c`,
/// which every terminal answers. A terminal answers in the order it reads,
/// so the DA1 reply marks the end of the replies.
///
/// References:
/// - ADR-1007 D2: the two questions and their order.
pub const QUERIES: &[u8] = b"\x1b[>0q\x1b[c";

/// The OSC 52 sequence that copies `text`: `ESC ] 52 ; c ; BASE64 ESC \`.
/// BASE64 is the text in standard base64 with padding. The target `c` is
/// the clipboard, which vtium takes into its one selection. The bytes go
/// as they are, whether or not they are valid UTF-8.
///
/// References:
/// - ADR-1007 D2: the sequence, its target and its terminator.
pub fn sequence(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len().div_ceil(3).saturating_mul(4).saturating_add(9));
    out.extend_from_slice(b"\x1b]52;c;");
    base64(text, &mut out);
    out.extend_from_slice(b"\x1b\\");
    out
}

/// Appends `data` to `out` in base64 with padding, as RFC 4648 section 4
/// defines it: each group of three bytes becomes four characters, and a
/// last group of one or two bytes becomes two or three characters and
/// `=` to make four.
fn base64(data: &[u8], out: &mut Vec<u8>) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for group in data.chunks(3) {
        let at = |i: usize| u32::from(group.get(i).copied().unwrap_or(0));
        let n = (at(0) << 16) | (at(1) << 8) | at(2);
        for i in 0..4 {
            if i <= group.len() {
                let shift = 18 - 6 * i;
                out.push(ALPHABET[((n >> shift) & 63) as usize]);
            } else {
                out.push(b'=');
            }
        }
    }
}

/// The text to copy, as the program read it.
#[derive(Debug, PartialEq, Eq)]
pub enum Text {
    /// The text, at most [`CLIPBOARD_MAX`] bytes.
    Fits(Vec<u8>),
    /// The text is larger than [`CLIPBOARD_MAX`] bytes. vtium would drop
    /// it whole, so the program copies nothing.
    TooLarge,
}

/// Reads the text from `input` to its end, but never more than
/// [`CLIPBOARD_MAX`] bytes and one more, which is enough to tell that the
/// text is too large without reading all of it.
pub fn read_text(input: impl Read) -> io::Result<Text> {
    let limit = u64::try_from(CLIPBOARD_MAX).map_or(u64::MAX, |m| m.saturating_add(1));
    let mut text = Vec::new();
    input.take(limit).read_to_end(&mut text)?;
    Ok(if text.len() > CLIPBOARD_MAX {
        Text::TooLarge
    } else {
        Text::Fits(text)
    })
}

/// What the terminal's replies say, as far as they have come.
#[derive(Debug, PartialEq, Eq)]
pub struct Answers {
    /// The text of the XTVERSION reply, `DCS > | text ST`, once the whole
    /// reply has come.
    pub version: Option<Vec<u8>>,
    /// Whether a DA1 reply, `CSI ? Ps ; ... c`, has come. It is the last
    /// reply, so the program stops waiting once it is here.
    pub da1: bool,
}

/// Reads the XTVERSION and DA1 replies out of `bytes`, the bytes the
/// terminal has sent so far. Other bytes, such as keys typed meanwhile,
/// are skipped.
pub fn answers(bytes: &[u8]) -> Answers {
    let version = past(bytes, b"\x1bP>|").and_then(|text| {
        let end = text.windows(2).position(|w| w == b"\x1b\\")?;
        Some(text[..end].to_vec())
    });
    let mut da1 = false;
    let mut rest = bytes;
    while let Some(after) = past(rest, b"\x1b[?") {
        rest = after;
        let body = rest
            .iter()
            .take_while(|b| b.is_ascii_digit() || **b == b';')
            .count();
        if rest.get(body) == Some(&b'c') {
            da1 = true;
            break;
        }
    }
    Answers { version, da1 }
}

/// The bytes of `haystack` after the first `needle` in it, or `None`.
fn past<'a>(haystack: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
    let at = haystack.windows(needle.len()).position(|w| w == needle)?;
    haystack.get(at.saturating_add(needle.len())..)
}

/// Whether an XTVERSION text is vtium's: its name, the text before the
/// first `(` or space, is `vtium`. vtium answers `vtium(VERSION)`.
pub fn names_vtium(version: &[u8]) -> bool {
    let name = version
        .split(|b| *b == b'(' || *b == b' ')
        .next()
        .unwrap_or(version);
    name == b"vtium"
}

/// How the program ends, as its exit status.
///
/// References:
/// - ADR-1007 D3: what each status means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// The copy was written, and the terminal answered as vtium.
    Copied = 0,
    /// Nothing was written: the text was too large, the file could not be
    /// read, or there was no terminal.
    NothingCopied = 1,
    /// The command line was wrong, or the command was `paste`.
    Usage = 2,
    /// The copy was written, but the terminal did not answer as vtium.
    Unconfirmed = 3,
}

/// The name the program was called by, for its help and its messages:
/// the last part of `argv[0]`, so `vticlip` when the link started it.
fn called_by(argv0: Option<&OsString>) -> String {
    argv0
        .and_then(|a| Path::new(a).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "vtium-clipboard".to_owned())
}

/// Runs the program with its command line `argv`, `argv[0]` included,
/// reading the text from `stdin` when no file is named. Help goes to
/// `stdout`, and every other message to `stderr`.
pub fn run(
    argv: &[OsString],
    stdin: impl Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Exit {
    let name = called_by(argv.first());
    let args = argv.get(1..).unwrap_or_default();
    let say = |stderr: &mut dyn Write, text: String| {
        let _ = writeln!(stderr, "{name}: {text}");
    };
    let Some(command) = args.first() else {
        say(stderr, messages::no_command(&name));
        return Exit::Usage;
    };
    match command.to_str() {
        Some("--help" | "-h") if args.len() == 1 => {
            let _ = stdout.write_all(messages::help(&name, CLIPBOARD_MAX).as_bytes());
            Exit::Copied
        }
        Some("--version") if args.len() == 1 => {
            let _ = writeln!(stdout, "{name} {}", env!("CARGO_PKG_VERSION"));
            Exit::Copied
        }
        Some("paste") => {
            say(stderr, messages::paste());
            Exit::Usage
        }
        Some("copy") if args.len() <= 2 => copy(&name, args.get(1), stdin, stderr),
        Some("copy") => {
            say(stderr, messages::copy_takes_one_file(&name));
            Exit::Usage
        }
        Some("--help" | "-h" | "--version") => {
            say(
                stderr,
                messages::takes_nothing_more(&command.to_string_lossy(), &name),
            );
            Exit::Usage
        }
        _ => {
            say(
                stderr,
                messages::unknown_command(&command.to_string_lossy(), &name),
            );
            Exit::Usage
        }
    }
}

/// `copy [FILE]`: reads the text, checks its size, writes the sequence and
/// the two questions to the terminal, and reads who answered.
///
/// References:
/// - ADR-1007 D2: the steps of a copy and their order.
fn copy(name: &str, file: Option<&OsString>, stdin: impl Read, stderr: &mut dyn Write) -> Exit {
    let say = |stderr: &mut dyn Write, text: String| {
        let _ = writeln!(stderr, "{name}: {text}");
    };
    let read = match file {
        Some(path) => std::fs::File::open(path).and_then(read_text).map_err(|e| {
            messages::file_unreadable(&Path::new(path).display().to_string(), &e.to_string())
        }),
        None => read_text(stdin).map_err(|e| messages::stdin_unreadable(&e.to_string())),
    };
    let text = match read {
        Ok(Text::Fits(text)) => text,
        Ok(Text::TooLarge) => {
            say(stderr, messages::too_large(CLIPBOARD_MAX));
            return Exit::NothingCopied;
        }
        Err(message) => {
            say(stderr, message);
            return Exit::NothingCopied;
        }
    };
    let mut bytes = sequence(&text);
    bytes.extend_from_slice(QUERIES);
    let replies = match tty::exchange(&bytes, REPLY_WAIT) {
        Ok(replies) => replies,
        Err(tty::Failed::Open(e)) => {
            say(stderr, messages::no_terminal(name, &e.to_string()));
            return Exit::NothingCopied;
        }
        Err(tty::Failed::Write(e)) => {
            say(stderr, messages::write_failed(name, &e.to_string()));
            return Exit::NothingCopied;
        }
    };
    if std::str::from_utf8(&text).is_err() {
        say(stderr, messages::not_utf8());
    }
    if answers(&replies).version.is_some_and(|v| names_vtium(&v)) {
        Exit::Copied
    } else {
        say(stderr, messages::unconfirmed());
        Exit::Unconfirmed
    }
}
