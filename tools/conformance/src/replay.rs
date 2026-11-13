//! The replay: a byte stream fed to a vtium terminal with the reference
//! host's settings, test commands run where the stream reaches them, and a
//! block printed at each cut and at the end (INTERFACE.md, "Input").

use core::fmt::Write;
use std::path::{Path, PathBuf};

use vtium_term::{
    Age, Extent, Key, KeyEvent, Mods, MouseButton, MouseEvent, Palette, SB_SIZE_MAX,
    TERMCAP_ENTRIES_MAX, TERMCAP_NAME_MAX, TERMCAP_VALUE_MAX, Termcap, Terminal,
};

use crate::command::{self, Button, Command, END, Event, How, INTRO, KeyAction};
#[cfg(feature = "images")]
mod images;
mod links;
use crate::view::{self, FrameState, View};
use crate::{Failure, internal, usage};

/// The palette the reference host starts with.
const DEFAULT_PALETTE: &str = "legacy";

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

/// Replays `input` and returns what it prints, or the failure that stopped
/// it with what it printed first.
pub fn run(input: &Input) -> Result<String, Failure> {
    run_with(input, &mut |_, _| {})
}

/// Replays `input` as [`run`] does, and hands `at_block` each block's
/// number, from 0 for the first block, with the terminal as that block
/// prints it. `at_block` reads the terminal and changes nothing in it, so
/// the replay prints what [`run`] prints. `make tier1-shots` draws the
/// terminal it is handed.
pub fn run_with(
    input: &Input,
    at_block: &mut dyn FnMut(usize, &Terminal),
) -> Result<String, Failure> {
    let mut r = Replay::new(input);
    match r.play(input, at_block) {
        Ok(()) => Ok(r.out),
        Err(f) => Err(Failure { output: r.out, ..f }),
    }
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

/// What a block prints among its replies: the bytes the terminal wrote
/// back, and a `host:` line at each point a request was made.
#[derive(Clone, Debug)]
enum Reply {
    Bytes(Vec<u8>),
    #[cfg_attr(not(feature = "images"), allow(dead_code))]
    Host(String),
}

struct Replay<'a> {
    term: Terminal,
    view: View,
    out: String,
    replies: Vec<Reply>,
    /// The bells that reached the host since the last block.
    bells: u64,
    fixtures: &'a Path,
    frame: FrameState,
    callers: [Option<Age>; 8],
    #[cfg(feature = "images")]
    images: images::Images,
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
        #[cfg(feature = "images")]
        let images = images::Images::new(&mut term, &input.fixtures);
        Self {
            term,
            view: input.view,
            out: String::new(),
            replies: Vec::new(),
            bells: 0,
            fixtures: &input.fixtures,
            frame: FrameState::new(DEFAULT_PALETTE),
            callers: [None; 8],
            #[cfg(feature = "images")]
            images,
        }
    }

    /// Keeps bytes the terminal wrote back, after those before them.
    fn keep_replies(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        match self.replies.last_mut() {
            Some(Reply::Bytes(last)) => last.extend(bytes),
            _ => self.replies.push(Reply::Bytes(bytes)),
        }
    }

    /// Feeds the stream, printing a block at each cut and at the end, and
    /// hands `at_block` the terminal after each block.
    fn play(
        &mut self,
        input: &Input,
        at_block: &mut dyn FnMut(usize, &Terminal),
    ) -> Result<(), Failure> {
        let mut pos = 0;
        for (n, &cut) in input.cuts.iter().enumerate() {
            check_cut(&input.data, cut)?;
            self.feed(input.data.get(pos..cut).unwrap_or_default())?;
            pos = cut;
            let _ = writeln!(self.out, "checkpoint {cut}");
            self.block();
            at_block(n, &self.term);
        }
        self.feed(input.data.get(pos..).unwrap_or_default())?;
        if !input.cuts.is_empty() {
            let _ = writeln!(self.out, "checkpoint {}", input.data.len());
        }
        self.block();
        at_block(input.cuts.len(), &self.term);
        Ok(())
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

    /// Feeds bytes to the terminal, taking its replies as it goes, and with
    /// images answering each request the fixture host takes. The bytes go
    /// in pieces that each end at a BEL, so that each BEL that reaches the
    /// host is counted: the terminal keeps one bell flag, which several
    /// BELs in one piece would set once. A terminal reads a stream cut
    /// anywhere as it reads it whole, so the pieces change nothing else.
    fn input(&mut self, data: &[u8]) -> Result<(), Failure> {
        for piece in data.split_inclusive(|&b| b == 0x07) {
            self.feed_piece(piece)?;
            if self.term.take_bell() {
                self.bells = self.bells.saturating_add(1);
            }
        }
        Ok(())
    }

    fn feed_piece(&mut self, mut data: &[u8]) -> Result<(), Failure> {
        while !data.is_empty() {
            let taken = self.term.input(data);
            let replies = self.term.take_replies();
            let replied = !replies.is_empty();
            self.keep_replies(replies);
            let answered = self.answer();
            if taken == 0 && !replied && !answered {
                return Err(internal("input stopped with no host request out"));
            }
            data = data.get(taken..).unwrap_or_default();
        }
        Ok(())
    }

    /// Answers the request out, if the fixture host is installed: its
    /// `host:` line goes among the replies at the point it was made, then
    /// what the answer made the terminal write. Returns whether it did.
    #[cfg(feature = "images")]
    fn answer(&mut self) -> bool {
        let Some((line, replies)) = self.images.answer(&mut self.term) else {
            return false;
        };
        self.replies.push(Reply::Host(line));
        self.keep_replies(replies);
        true
    }

    #[cfg(not(feature = "images"))]
    fn answer(&mut self) -> bool {
        false
    }

    fn command(&mut self, cmd: Command) -> Result<(), Failure> {
        let t = &mut self.term;
        match cmd {
            Command::Resize { cols, rows } => t.resize(cols, rows),
            Command::SbMax(n) => t.screen_mut().set_scrollback_max(lines(n)),
            Command::SbUp(n) => t.screen_mut().scroll_view_up(lines(n)),
            Command::SbDown(n) => t.screen_mut().scroll_view_down(lines(n)),
            Command::SbReset => t.screen_mut().reset_view(),
            Command::Sync(on) => t.set_sync_enabled(on),
            Command::SyncFrames => {
                let _ = writeln!(self.out, "test sync-frames {}", t.sync_frames());
            }
            Command::Clipboard => {
                self.out.push_str("test clipboard ");
                match t.take_clipboard_write() {
                    Some(data) => data.iter().for_each(|b| {
                        let _ = write!(self.out, "{b:02x}");
                    }),
                    None => self.out.push_str("none"),
                }
                self.out.push('\n');
            }
            Command::FontOps(on) => t.set_font_ops(on),
            Command::FontName(name) => {
                t.set_font_name(&name);
            }
            Command::FontRequest => {
                let name = t.take_font_request();
                let _ = writeln!(
                    self.out,
                    "test font-request {}",
                    name.as_deref().unwrap_or("none")
                );
            }
            Command::WindowOps(on) => t.set_window_ops(on),
            Command::GridRequest => {
                let side = |e: Extent| match e {
                    Extent::Keep => "keep".to_owned(),
                    Extent::Display => "display".to_owned(),
                    Extent::Cells(n) => n.to_string(),
                };
                match t.take_grid_request() {
                    Some(r) => {
                        let _ = writeln!(
                            self.out,
                            "test grid-request {} {}",
                            side(r.rows),
                            side(r.cols)
                        );
                    }
                    None => self.out.push_str("test grid-request none\n"),
                }
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
                event,
                unshifted,
                base,
            } => {
                let unicode = unicode.and_then(char::from_u32);
                let event = match event {
                    KeyAction::Press => KeyEvent::Press,
                    KeyAction::Repeat => KeyEvent::Repeat,
                    KeyAction::Release => KeyEvent::Release,
                };
                t.key(&Key {
                    event,
                    unshifted,
                    base: base.and_then(char::from_u32),
                    ..Key::press(sym, ascii, Mods::from_bits(mods), unicode)
                });
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
                if let Some(p) = Palette::named(name) {
                    t.set_palette(p);
                    self.frame.palette = name;
                }
            }
            Command::Select { how, col, row } => {
                let cell = view_cell(t.screen(), col, row);
                let screen = t.screen_mut();
                let (x, y) = match (how, cell) {
                    (How::Clear, _) => (0, 0),
                    (_, (Some(x), Some(y))) => (x, y),
                    _ => {
                        return Err(usage("test command select: col or row past the screen"));
                    }
                };
                match how {
                    How::Start => screen.select_start(x, y),
                    How::To => screen.select_to(x, y),
                    How::Word => screen.select_word(x, y),
                    How::Line => screen.select_line(x, y),
                    // the host clears the selection when no command's
                    // output is at the cell
                    How::Output => {
                        if !screen.select_output(x, y) {
                            screen.clear_selection();
                        }
                    }
                    How::Clear => screen.clear_selection(),
                }
            }
            Command::Copy => {
                // the host copies nothing when nothing is selected or the
                // selection holds no text
                self.out.push_str("test copy ");
                match t.screen().selection_text().filter(|s| !s.is_empty()) {
                    Some(text) => text.bytes().for_each(|b| {
                        let _ = write!(self.out, "{b:02x}");
                    }),
                    None => self.out.push_str("none"),
                }
                self.out.push('\n');
            }
            Command::Prompt { up } => {
                t.screen_mut().jump_to_prompt(up);
            }
            Command::Link { col, row } => links::link(t, (col, row), &mut self.out)?,
            Command::Hover { at, ctrl } => links::hover(t, at, ctrl, &mut self.out)?,
            Command::Backspace { del } => t.set_backspace_sends_delete(del),
            Command::State => state(t, &mut self.out),
            Command::FrameSince(caller) => {
                let slot = self.callers.get_mut(usize::from(caller));
                if let Some(since) = slot {
                    let (n, age) = view::changed(t, *since);
                    *since = Some(age);
                    let _ = writeln!(self.out, "test frame-since {caller} changed {n}");
                }
            }
            #[cfg(feature = "images")]
            image => {
                if let Some(line) = self.images.command(&mut self.term, image) {
                    self.out.push_str(&line);
                }
            }
            #[cfg(not(feature = "images"))]
            _ => return Err(usage("an image command in a build without images")),
        }
        let replies = self.term.take_replies();
        self.keep_replies(replies);
        Ok(())
    }

    /// Prints a block: the head, the replies since the last block, then
    /// the view.
    fn block(&mut self) {
        let replies = self.term.take_replies();
        self.keep_replies(replies);
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
        for reply in self.replies.drain(..) {
            match reply {
                Reply::Bytes(bytes) => {
                    self.out.push_str("reply ");
                    for b in bytes {
                        let _ = write!(self.out, "{b:02x}");
                    }
                    self.out.push('\n');
                }
                Reply::Host(line) => {
                    self.out.push_str(&line);
                    self.out.push('\n');
                }
            }
        }
        let bells = core::mem::take(&mut self.bells);
        if bells > 0 {
            let _ = writeln!(self.out, "bell {bells}");
        }
        match self.view {
            View::Screen => view::screen(&self.term, &mut self.out),
            View::Frame => view::frame(&mut self.term, &mut self.frame, &mut self.out),
            #[cfg(feature = "bidi")]
            View::Display => view::display(&self.term, &mut self.out),
        }
    }
}

/// The `state` command's lines (INTERFACE-vtium.md, "The state"): the
/// scroll margins, the tab stops, the modes in force, the scrollback, the
/// pen and the colors, each a `test state` line.
fn state(t: &Terminal, out: &mut String) {
    let screen = t.screen();
    let (top, bottom) = screen.margins();
    let (left, right) = screen.lr_margins();
    let _ = writeln!(out, "test state margins {top}-{bottom} {left}-{right}");
    let tabs: Vec<String> = screen
        .tab_stops()
        .iter()
        .enumerate()
        .filter(|(_, on)| **on)
        .map(|(x, _)| x.to_string())
        .collect();
    let _ = writeln!(out, "test state tabs {}", list(&tabs));
    let modes = t.modes_in_force();
    let numbers = |dec: bool| -> Vec<String> {
        modes
            .iter()
            .filter(|(d, _)| *d == dec)
            .map(|(_, n)| n.to_string())
            .collect()
    };
    let _ = writeln!(
        out,
        "test state modes ansi {} dec {}",
        list(&numbers(false)),
        list(&numbers(true))
    );
    let _ = writeln!(
        out,
        "test state scrollback lines {} view {} max {}",
        screen.scrollback_lines(),
        screen.scrolled_back(),
        screen.scrollback_max()
    );
    let _ = writeln!(
        out,
        "test state pen{}",
        view::attrs(screen.pen(), &Palette::default())
    );
    let palette = t.palette();
    let cursor = palette
        .cursor()
        .map_or_else(|| "default".to_owned(), view::hex);
    let _ = writeln!(
        out,
        "test state colors fg={} bg={} cursor={cursor}",
        view::hex(palette.fg()),
        view::hex(palette.bg())
    );
}

/// Numbers joined by commas, or `none`.
/// The view's cell at `col`, `row`, each `None` past the screen.
pub(crate) fn view_cell(
    screen: &vtium_term::Screen,
    col: u64,
    row: u64,
) -> (Option<usize>, Option<usize>) {
    (
        usize::try_from(col).ok().filter(|&c| c < screen.cols()),
        usize::try_from(row).ok().filter(|&r| r < screen.rows()),
    )
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_owned()
    } else {
        items.join(",")
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

    // run_with hands over the terminal at each block, in order, as that
    // block prints it, and prints what run prints
    #[test]
    fn run_with_hands_over_the_terminal_at_each_block() {
        let data = b"ab\x1b[2Jcd";
        let mut seen = Vec::new();
        let out = run_with(&input(data, &[2, 2]), &mut |block, term| {
            seen.push((block, term.screen().cursor()));
        });
        assert_eq!(out, run(&input(data, &[2, 2])));
        assert_eq!(seen, [(0, (2, 0)), (1, (2, 0)), (2, (4, 0))]);
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

    // INTERFACE-vtium.md: the host takes what OSC 52 set, once
    #[test]
    fn the_clipboard_command_takes_what_osc_52_set() {
        let take = b"\x1b_tsm-test;clipboard\x1b\\";
        let mut data = b"\x1b]52;c;aGk=\x07".to_vec();
        data.extend_from_slice(take);
        data.extend_from_slice(take);
        let out = run(&input(&data, &[])).unwrap_or_default();
        assert!(
            out.starts_with("test clipboard 6869\ntest clipboard none\nsize "),
            "{out}"
        );
    }

    fn cmd(text: &str) -> Vec<u8> {
        let mut out = INTRO.to_vec();
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(END);
        out
    }

    fn sized(cols: usize, rows: usize, data: &[u8]) -> String {
        let mut i = input(data, &[]);
        i.cols = cols;
        i.rows = rows;
        run(&i).unwrap_or_else(|f| format!("exit {}: {}", f.code, f.message))
    }

    // INTERFACE-vtium.md: the host selects a word and copies it, and a copy
    // with nothing selected copies nothing
    #[test]
    fn select_and_copy_print_the_selected_text() {
        let mut data = b"ab cd".to_vec();
        data.extend(cmd("copy"));
        data.extend(cmd("select;how=word;col=4;row=0"));
        data.extend(cmd("copy"));
        data.extend(cmd("select;how=clear"));
        data.extend(cmd("copy"));
        let out = sized(6, 1, &data);
        assert!(
            out.starts_with("test copy none\ntest copy 6364\ntest copy none\nsize "),
            "{out}"
        );
    }

    // a line selection's copy ends with a line end; a drag selects from
    // where it started to where it ends, whichever comes first
    #[test]
    fn select_line_and_a_drag_copy_their_text() {
        let mut data = b"ab cd\r\nef".to_vec();
        data.extend(cmd("select;how=line;col=1;row=0"));
        data.extend(cmd("copy"));
        data.extend(cmd("select;how=start;col=1;row=1"));
        data.extend(cmd("select;how=to;col=3;row=0"));
        data.extend(cmd("copy"));
        let out = sized(6, 2, &data);
        assert!(
            out.starts_with("test copy 61622063640a\ntest copy 63640a6566\nsize "),
            "{out}"
        );
    }

    // OSC 133: the view jumps to the marked prompt above, and back down
    #[test]
    fn prompt_moves_the_view_to_a_marked_prompt() {
        let mut data = cmd("sb-max;n=10");
        data.extend_from_slice(b"\x1b]133;A\x07$ a\r\nout\r\n\x1b]133;A\x07$ b\r\n1\r\n2\r\n3");
        data.extend(cmd("prompt;dir=up"));
        data.extend(cmd("state"));
        data.extend(cmd("prompt;dir=down"));
        data.extend(cmd("state"));
        let out = sized(4, 2, &data);
        let views: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("test state scrollback"))
            .collect();
        assert_eq!(
            views,
            [
                "test state scrollback lines 4 view 2 max 10",
                "test state scrollback lines 4 view 0 max 10"
            ],
            "{out}"
        );
    }

    // the host's BackSpace setting: 0x08, then 0x7f
    #[test]
    fn backspace_chooses_what_backspace_sends() {
        let mut data = cmd("key;sym=ff08");
        data.extend(cmd("backspace;del=1"));
        data.extend(cmd("key;sym=ff08"));
        let out = sized(4, 1, &data);
        assert!(out.contains("reply 087f\n"), "{out}");
    }

    // each BEL that reaches the host counts, however the stream is cut,
    // and one that ends an OSC string is no bell
    #[test]
    fn a_bell_line_counts_each_bel_after_the_replies() {
        let data = b"\x07\x1b]0;t\x07\x1b[c\x07";
        let out = sized(4, 1, data);
        let lines: Vec<&str> = out.lines().collect();
        let bell = lines.iter().position(|l| *l == "bell 2");
        assert!(
            bell.is_some_and(|i| lines[i - 1].starts_with("reply ") && lines[i + 1] == "row 0 ||"),
            "{out}"
        );
        let cut = run(&input(data, &[1])).unwrap_or_default();
        assert_eq!(cut.matches("bell 1\n").count(), 2, "{cut}");
        // with mode 1042 reset, BEL reaches no host
        let out = sized(4, 1, b"\x1b[?1042l\x07");
        assert!(!out.contains("bell"), "{out}");
    }

    // the state lines, in their order, at the reference host's start
    #[test]
    fn state_prints_the_screens_state_lines() {
        let mut data = b"\x1b[2;3r\x1b[3g\x1bH\x1b[1;31m".to_vec();
        data.extend(cmd("state"));
        let out = sized(10, 4, &data);
        let state: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("test state "))
            .collect();
        assert_eq!(state.len(), 6, "{out}");
        assert_eq!(state[0], "test state margins 1-2 0-9");
        assert_eq!(state[1], "test state tabs 0");
        // the modes the reference host starts with, by the mode table's
        // initial states: vtium-term built with bidi adds BDSM (8) and
        // 1243, and a host that installs the image pipeline (this crate's
        // images feature) the private sixel registers (1070). vtium-term's
        // features can differ from this crate's when a workspace build
        // unifies them, so DECRQM asks the terminal whether it knows 1243:
        // it answers 0 for a mode its table lacks.
        let knows = |mode: u32| {
            let mut ask = format!("\x1b[?{mode}$p").into_bytes();
            ask.extend(cmd("state"));
            let unknown: String = format!("\x1b[?{mode};0$y")
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect();
            !sized(10, 4, &ask).contains(&format!("reply {unknown}\n"))
        };
        let bidi = knows(1243);
        let images = cfg!(feature = "images");
        let modes = format!(
            "test state modes ansi {}12 dec 7,8,12,25,1007,1034,1035,1036,1042,{}{}2027,2028",
            if bidi { "8," } else { "" },
            if images { "1070," } else { "" },
            if bidi { "1243," } else { "" },
        );
        assert_eq!(state[2], modes);
        assert_eq!(state[3], "test state scrollback lines 0 view 0 max 0");
        assert_eq!(state[4], "test state pen fg=1 bold");
        assert_eq!(
            state[5],
            "test state colors fg=#e5e5e5 bg=#000000 cursor=default"
        );
    }
}
