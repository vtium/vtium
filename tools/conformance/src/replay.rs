//! The replay: a byte stream fed to a vtium terminal with the reference
//! host's settings, test commands run where the stream reaches them, and a
//! block printed at each cut and at the end (INTERFACE.md, "Input").

use core::fmt::Write;
use std::path::{Path, PathBuf};

use vtium_term::{
    Age, Mods, MouseButton, MouseEvent, SB_SIZE_MAX, TERMCAP_ENTRIES_MAX, TERMCAP_NAME_MAX,
    TERMCAP_VALUE_MAX, Termcap, Terminal,
};

use crate::command::{self, Button, Command, END, Event, INTRO};
use crate::palettes::{self, Named};
use crate::view::{self, FrameState, View};
use crate::{Failure, internal, usage};

/// The largest screen the command line takes.
pub const COLS_MAX: usize = 2048;
pub const ROWS_MAX: usize = 1024;

/// One replay's input: the view, the size, the stream, the cuts and the
/// fixture directory beside the stream.
#[derive(Clone, Debug)]
pub struct Input {
    pub view: View,
    pub cols: usize,
    pub rows: usize,
    pub data: Vec<u8>,
    pub cuts: Vec<usize>,
    pub fixtures: PathBuf,
}

impl Input {
    /// The fixture directory for the stream at `file`: a final `.bin`
    /// removed and `.files` appended.
    pub fn fixtures_for(file: &Path) -> PathBuf {
        let name = file.as_os_str().to_string_lossy();
        PathBuf::from(format!(
            "{}.files",
            name.strip_suffix(".bin").unwrap_or(&name)
        ))
    }
}

/// A decimal number from 1 to `max` with no sign or leading zero.
pub fn size(s: &str, max: usize) -> Option<usize> {
    let n = command::number(s, 10, u64::try_from(max).ok()?)?;
    usize::try_from(n).ok().filter(|&n| n > 0)
}

/// CUTS: one offset a line, each at least the one before and at most `len`.
pub fn cuts(text: &[u8], len: usize) -> Result<Vec<usize>, Failure> {
    let max = u64::try_from(len).unwrap_or(u64::MAX);
    let mut cuts: Vec<usize> = Vec::new();
    if text.is_empty() {
        return Ok(cuts);
    }
    let text = text.strip_suffix(b"\n").unwrap_or(text);
    for (i, line) in text.split(|&b| b == b'\n').enumerate() {
        let prev = cuts.last().copied().unwrap_or(0);
        let cut = core::str::from_utf8(line)
            .ok()
            .and_then(|l| command::number(l, 10, max))
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n >= prev);
        let Some(cut) = cut else {
            return Err(usage(format!(
                "CUTS line {} is not an offset from {prev} to {len}",
                i.saturating_add(1)
            )));
        };
        cuts.push(cut);
    }
    Ok(cuts)
}

/// Replays `input` and returns what it prints.
pub fn run(input: &Input) -> Result<String, Failure> {
    let mut r = Replay::new(input);
    let mut pos = 0;
    for &cut in &input.cuts {
        check_cut(&input.data, cut)?;
        r.feed(input.data.get(pos..cut).unwrap_or_default())?;
        pos = cut;
        let _ = writeln!(r.out, "checkpoint {cut}");
        r.block();
    }
    r.feed(input.data.get(pos..).unwrap_or_default())?;
    if !input.cuts.is_empty() {
        let _ = writeln!(r.out, "checkpoint {}", input.data.len());
    }
    r.block();
    Ok(r.out)
}

/// A cut inside a test command's introducer is a usage failure; one later
/// in the command leaves its segment without the ST, which [`Replay::feed`]
/// refuses.
fn check_cut(data: &[u8], cut: usize) -> Result<(), Failure> {
    for k in 1..INTRO.len().min(cut.saturating_add(1)) {
        let start = cut.saturating_sub(k);
        if data.get(start..).is_some_and(|d| d.starts_with(INTRO)) {
            return Err(usage(format!("cut {cut} inside a test command")));
        }
    }
    Ok(())
}

struct Replay<'a> {
    term: Terminal,
    view: View,
    out: String,
    replies: Vec<u8>,
    fixtures: &'a Path,
    frame: FrameState,
    callers: [Option<Age>; 8],
}

impl<'a> Replay<'a> {
    /// A terminal with the reference host's settings.
    fn new(input: &'a Input) -> Self {
        let mut term = Terminal::new(input.cols, input.rows, SB_SIZE_MAX);
        term.set_sync_enabled(true);
        let _ = term.set_version("tsm-replay");
        term.set_cell_size(8, 16);
        term.set_cursor_blink(true);
        term.set_backspace_sends_delete(false);
        Self {
            term,
            view: input.view,
            out: String::new(),
            replies: Vec::new(),
            fixtures: &input.fixtures,
            frame: FrameState::new(palettes::DEFAULT),
            callers: [None; 8],
        }
    }

    /// Feeds a segment between two cuts, running each test command in it.
    fn feed(&mut self, mut data: &[u8]) -> Result<(), Failure> {
        while let Some(start) = find(data, INTRO) {
            let (before, rest) = data.split_at(start);
            self.input(before)?;
            let text = rest.get(INTRO.len()..).unwrap_or_default();
            let end = find(text, END).ok_or_else(|| usage("test command without ST"))?;
            let cmd = command::parse(text.get(..end).unwrap_or_default())?;
            self.command(cmd)?;
            data = text
                .get(end.saturating_add(END.len())..)
                .unwrap_or_default();
        }
        self.input(data)
    }

    /// Feeds bytes to the terminal, taking its replies as it goes.
    fn input(&mut self, mut data: &[u8]) -> Result<(), Failure> {
        while !data.is_empty() {
            let taken = self.term.input(data);
            let replies = self.term.take_replies();
            if taken == 0 && replies.is_empty() {
                return Err(internal("input stopped with no reply to take"));
            }
            self.replies.extend(replies);
            data = data.get(taken..).unwrap_or_default();
        }
        Ok(())
    }

    fn command(&mut self, cmd: Command) -> Result<(), Failure> {
        let t = &mut self.term;
        match cmd {
            Command::Resize { cols, rows } => t.screen_mut().resize(cols, rows),
            Command::SbMax(n) => t.screen_mut().set_scrollback_max(lines(n)),
            Command::SbUp(n) => t.screen_mut().scroll_view_up(lines(n)),
            Command::SbDown(n) => t.screen_mut().scroll_view_down(lines(n)),
            Command::SbReset => t.screen_mut().reset_view(),
            Command::Sync(on) => t.set_sync_enabled(on),
            Command::SyncFrames => {
                let _ = writeln!(self.out, "test sync-frames {}", t.sync_frames());
            }
            Command::Version(text) => {
                if t.set_version(&text).is_err() {
                    let _ = writeln!(self.out, "test version error -22");
                }
            }
            Command::Termcap(file) => {
                let caps = termcap(&self.fixtures.join(file))?;
                match Termcap::new(caps.iter().map(|(n, v)| (n.as_slice(), v.as_deref()))) {
                    Ok(caps) => t.set_termcap(caps),
                    Err(_) => {
                        let _ = writeln!(self.out, "test termcap error -22");
                    }
                }
            }
            Command::Key {
                sym,
                ascii,
                unicode,
                mods,
            } => {
                let unicode = unicode.and_then(char::from_u32);
                t.key(sym, ascii, Mods::from_bits(mods), unicode);
            }
            Command::Mouse {
                col,
                row,
                px,
                py,
                button,
                event,
                mods,
            } => {
                let screen = t.screen();
                let (Some(col), Some(row)) = (
                    usize::try_from(col).ok().filter(|&c| c < screen.cols()),
                    usize::try_from(row).ok().filter(|&r| r < screen.rows()),
                ) else {
                    return Err(usage("test command mouse: col or row past the screen"));
                };
                t.mouse(
                    col,
                    row,
                    px,
                    py,
                    button_of(button),
                    event_of(event),
                    Mods::from_bits(mods),
                );
            }
            Command::Paste(bytes) => t.paste(&bytes),
            Command::Focus(on) => {
                t.focus(on);
            }
            Command::Palette(name) => {
                if let Some(p) = Named::find(name) {
                    t.set_palette(p.palette());
                    self.frame.palette = p.name;
                }
            }
            Command::FrameSince(caller) => {
                let slot = self.callers.get_mut(usize::from(caller));
                if let Some(since) = slot {
                    let (n, age) = view::changed(t, *since);
                    *since = Some(age);
                    let _ = writeln!(self.out, "test frame-since {caller} changed {n}");
                }
            }
        }
        self.replies.extend(self.term.take_replies());
        Ok(())
    }

    /// Prints a block: the head, the replies since the last block, then
    /// the view.
    fn block(&mut self) {
        self.replies.extend(self.term.take_replies());
        let screen = self.term.screen();
        let (x, y) = screen.cursor();
        let _ = writeln!(self.out, "size {}x{}", screen.cols(), screen.rows());
        let _ = writeln!(self.out, "cursor {x},{y}");
        let flags: Vec<&str> = [
            (screen.insert_mode(), "INSERT_MODE"),
            (screen.auto_wrap(), "AUTO_WRAP"),
            (screen.origin_mode(), "REL_ORIGIN"),
            (screen.reverse_video(), "INVERSE"),
            (!screen.cursor_visible(), "HIDE_CURSOR"),
            (screen.alternate(), "ALTERNATE"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        let flags = if flags.is_empty() {
            "none".to_owned()
        } else {
            flags.join("|")
        };
        let _ = writeln!(self.out, "flags {flags}");
        if !self.replies.is_empty() {
            self.out.push_str("reply ");
            for b in self.replies.drain(..) {
                let _ = write!(self.out, "{b:02x}");
            }
            self.out.push('\n');
        }
        match self.view {
            View::Screen => view::screen(&self.term, &mut self.out),
            View::Frame => view::frame(&mut self.term, &mut self.frame, &mut self.out),
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn lines(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

fn button_of(b: Button) -> MouseButton {
    match b {
        Button::Left => MouseButton::Left,
        Button::Middle => MouseButton::Middle,
        Button::Right => MouseButton::Right,
        Button::WheelUp => MouseButton::WheelUp,
        Button::WheelDown => MouseButton::WheelDown,
        Button::None => MouseButton::None,
    }
}

fn event_of(e: Event) -> MouseEvent {
    match e {
        Event::Press => MouseEvent::Press,
        Event::Release => MouseEvent::Release,
        Event::Move => MouseEvent::Move,
    }
}

/// A capability and its value; `None` for a boolean.
type Cap = (Vec<u8>, Option<Vec<u8>>);

/// A termcap fixture: one capability a line, `NAME` or `NAME=HEX`, each
/// within the interface's bounds, or a usage failure.
fn termcap(path: &Path) -> Result<Vec<Cap>, Failure> {
    let text = std::fs::read(path)
        .map_err(|e| usage(format!("termcap fixture {}: {e}", path.display())))?;
    let bad = |what: &str| usage(format!("termcap fixture {}: {what}", path.display()));
    let mut caps: Vec<Cap> = Vec::new();
    for line in text.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        let (name, value) = match line.iter().position(|&b| b == b'=') {
            Some(i) => {
                let (name, rest) = line.split_at(i);
                let hex = core::str::from_utf8(rest.get(1..).unwrap_or_default())
                    .map_err(|_| bad("a value that is not hex"))?;
                let value =
                    command::hex_bytes(hex).ok_or_else(|| bad("a value that is not hex bytes"))?;
                if value.len() > TERMCAP_VALUE_MAX {
                    return Err(bad("a value over the bound"));
                }
                (name, Some(value))
            }
            None => (line, None),
        };
        if name.is_empty()
            || name.len() > TERMCAP_NAME_MAX
            || !name.iter().all(|b| (0x21..=0x7e).contains(b))
        {
            return Err(bad(
                "a name of no bytes, too many, or a byte outside 0x21-0x7e",
            ));
        }
        if caps.iter().any(|(n, _)| n == name) {
            return Err(bad("a name given twice"));
        }
        if caps.len() == TERMCAP_ENTRIES_MAX {
            return Err(bad("more capabilities than a table holds"));
        }
        caps.push((name.to_vec(), value));
    }
    Ok(caps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(data: &[u8], cuts: &[usize]) -> Input {
        Input {
            view: View::Screen,
            cols: 6,
            rows: 1,
            data: data.to_vec(),
            cuts: cuts.to_vec(),
            fixtures: PathBuf::from("/nonexistent.files"),
        }
    }

    #[test]
    fn cuts_follow_their_rules() {
        assert_eq!(cuts(b"", 5), Ok(vec![]));
        assert_eq!(cuts(b"0\n3\n3\n5\n", 5), Ok(vec![0, 3, 3, 5]));
        assert_eq!(cuts(b"0\n3\n3\n5", 5), Ok(vec![0, 3, 3, 5]));
        for bad in [&b"3\n2\n"[..], b"6\n", b"03\n", b"x\n", b"\n", b"1\n\n2\n"] {
            assert_eq!(cuts(bad, 5).map_err(|f| f.code), Err(2), "{bad:?}");
        }
    }

    #[test]
    fn sizes_follow_their_rules() {
        assert_eq!(size("80", COLS_MAX), Some(80));
        assert_eq!(size("0", COLS_MAX), None);
        assert_eq!(size("2049", COLS_MAX), None);
        assert_eq!(size("+8", COLS_MAX), None);
    }

    #[test]
    fn a_cut_at_the_end_prints_its_own_block_and_the_end_block() {
        let out = run(&input(b"ab", &[2])).unwrap_or_default();
        assert_eq!(out.matches("checkpoint 2\n").count(), 2, "{out}");
    }

    #[test]
    fn a_cut_inside_a_test_command_fails() {
        let data = b"a\x1b_tsm-test;sync-frames\x1b\\";
        assert_eq!(run(&input(data, &[3])).map_err(|f| f.code), Err(2));
        assert_eq!(run(&input(data, &[20])).map_err(|f| f.code), Err(2));
        assert!(run(&input(data, &[1])).is_ok());
        assert_eq!(
            run(&input(&data[..data.len() - 1], &[])).map_err(|f| f.code),
            Err(2)
        );
    }

    #[test]
    fn a_test_line_comes_before_the_block_that_follows() {
        let out = run(&input(b"a\x1b_tsm-test;sync-frames\x1b\\", &[])).unwrap_or_default();
        assert!(out.starts_with("test sync-frames 0\nsize 6x1\n"), "{out}");
    }
}
