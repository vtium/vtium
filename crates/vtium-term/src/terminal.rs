//! The terminal: the bytes a program writes, parsed by `vtium-parse`, and
//! their actions applied to the screen.
//!
//! The dispatch here carries the printing, the C0 and C1 controls, and the
//! control functions that move the cursor, edit, erase, scroll and set
//! tab stops and margins. SGR, the modes and the replies are added with
//! vtium/vtium#12, which drives the screen through its accessors, and the
//! character sets with vtium/vtium#13, through [`State::map_charset`].
//!
//! A control function is matched by its final byte, its private marker and
//! its intermediates exactly, as `vtium-parse`'s README asks: `CSI ? 5 A`
//! is not CUU.

use vtium_parse::{Chunk, Consumer, Intermediates, Params, Parser, StringEnd};

use crate::screen::{Erase, Screen};

/// A terminal: a parser and the state it drives.
#[derive(Clone)]
pub struct Terminal {
    parser: Parser,
    state: State,
}

/// What the parser drives: the screen, the replies waiting for the
/// program, and the terminal's own settings.
#[derive(Clone, Debug, PartialEq)]
struct State {
    screen: Screen,
    /// Bytes the terminal writes back to the program, in order.
    replies: Vec<u8>,
    /// LNM: a line feed also returns the carriage.
    newline_mode: bool,
}

impl Terminal {
    /// A terminal of `cols` by `rows` whose scrollback may be set to at most
    /// `scrollback_cap` lines (see [`Screen::new`]).
    pub fn new(cols: usize, rows: usize, scrollback_cap: usize) -> Self {
        Self {
            parser: Parser::new(),
            state: State {
                screen: Screen::new(cols, rows, scrollback_cap),
                replies: Vec::new(),
                newline_mode: false,
            },
        }
    }

    /// Feeds bytes from the program, and returns how many the parser took:
    /// fewer than all when the state asks it to pause.
    pub fn input(&mut self, bytes: &[u8]) -> usize {
        self.parser.advance(&mut self.state, bytes)
    }

    pub fn screen(&self) -> &Screen {
        &self.state.screen
    }

    pub fn screen_mut(&mut self) -> &mut Screen {
        &mut self.state.screen
    }

    /// The replies written since the last [`take_replies`](Self::take_replies).
    pub fn replies(&self) -> &[u8] {
        &self.state.replies
    }

    pub fn take_replies(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.state.replies)
    }

    /// LNM (mode 20): a line feed, vertical tab or form feed also returns
    /// the carriage.
    pub fn newline_mode(&self) -> bool {
        self.state.newline_mode
    }

    pub fn set_newline_mode(&mut self, on: bool) {
        self.state.newline_mode = on;
    }
}

impl core::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Terminal")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl State {
    /// The character-set hook (vtium/vtium#13): maps a printed character
    /// through the set in GL or GR. Until the sets land every character
    /// prints as itself, but DEL (U+007F), which only a 96-character set in
    /// GL draws, prints nothing.
    fn map_charset(&mut self, c: char) -> Option<char> {
        (c != '\u{7F}').then_some(c)
    }

    /// A control function's count: an omitted parameter or 0 is 1.
    fn count(params: &Params, i: usize) -> usize {
        params
            .get(i)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0)
            .max(1)
    }

    /// A parameter as a number, 0 when omitted.
    fn param(params: &Params, i: usize) -> usize {
        params
            .get(i)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0)
    }

    /// The control functions with no private marker and no intermediates.
    fn csi_plain(&mut self, params: &Params, final_byte: u8) {
        let s = &mut self.screen;
        let n = Self::count(params, 0);
        match final_byte {
            b'A' => s.move_up(n, false),
            b'B' => s.move_down(n, false),
            b'C' | b'a' => s.move_right(n),
            b'D' => s.move_left(n),
            b'E' => {
                s.move_down(n, false);
                s.move_line_home();
            }
            b'F' => {
                s.move_up(n, false);
                s.move_line_home();
            }
            b'G' | b'`' => s.move_to_column(n.saturating_sub(1)),
            b'd' => s.move_to(s.cursor().0, n.saturating_sub(1)),
            b'e' => s.move_rows_down(n),
            b'H' | b'f' => {
                let col = Self::count(params, 1);
                s.move_to(col.saturating_sub(1), n.saturating_sub(1));
            }
            b'J' | b'K' => self.erase(params, final_byte, Erase::All),
            b'X' => s.erase_chars(n),
            b'@' => s.insert_chars(n),
            b'P' => s.delete_chars(n),
            b'L' => s.insert_lines(n),
            b'M' => s.delete_lines(n),
            b'S' => s.scroll_up(n),
            b'T' => s.scroll_down(n),
            b'I' => s.tab_right(n),
            b'Z' => s.tab_left(n),
            b'b' => s.repeat(n),
            b'g' => match Self::param(params, 0) {
                0 => s.clear_tab_stop(),
                3 => s.clear_tab_stops(),
                _ => {}
            },
            b'r' => s.set_margins(Self::param(params, 0), Self::param(params, 1)),
            _ => {}
        }
    }

    /// ED and EL, and with `Erase::Unprotected` DECSED and DECSEL.
    fn erase(&mut self, params: &Params, final_byte: u8, which: Erase) {
        let s = &mut self.screen;
        match (final_byte, Self::param(params, 0)) {
            (b'J', 0) => s.erase_below(which),
            (b'J', 1) => s.erase_above(which),
            (b'J', 2) => s.erase_screen(which),
            (b'J', 3) => s.clear_scrollback(),
            (b'K', 0) => s.erase_line_right(which),
            (b'K', 1) => s.erase_line_left(which),
            (b'K', 2) => s.erase_line(which),
            _ => {}
        }
    }
}

impl Consumer for State {
    fn print(&mut self, c: char) {
        if let Some(c) = self.map_charset(c) {
            self.screen.print(c);
        }
    }

    fn execute(&mut self, control: u8) {
        // Text after a control starts a cell of its own.
        if control != 0 {
            self.screen.break_cluster();
        }
        let s = &mut self.screen;
        match control {
            0x08 => s.move_left(1),
            0x09 => s.tab_right(1),
            0x0a..=0x0c if self.newline_mode => s.newline(),
            0x0a..=0x0c => s.move_down(1, true),
            0x0d => s.move_line_home(),
            // SUB cancels a sequence and shows its error character.
            0x1a => s.print('\u{BF}'),
            0x84 => s.move_down(1, true),
            0x85 => s.newline(),
            0x88 => s.set_tab_stop(),
            0x8d => s.move_up(1, true),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: Intermediates, final_byte: u8) {
        if !intermediates.is_empty() {
            return;
        }
        let s = &mut self.screen;
        match final_byte {
            b'D' => s.move_down(1, true),
            b'E' => s.newline(),
            b'H' => s.set_tab_stop(),
            b'M' => s.move_up(1, true),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        // Only SGR reads sub-parameters; any other sequence carrying them
        // does nothing, as in the C reference.
        if final_byte != b'm' && params.has_subparams() {
            return;
        }
        if !intermediates.is_empty() {
            return;
        }
        match params.marker() {
            None => self.csi_plain(params, final_byte),
            Some(b'?') if matches!(final_byte, b'J' | b'K') => {
                self.erase(params, final_byte, Erase::Unprotected);
            }
            _ => {}
        }
    }

    fn osc_end(&mut self, _: &[u8], _: bool, _: StringEnd) {}

    fn dcs_hook(&mut self, _: &Params, _: Intermediates, _: u8) {}

    fn dcs_put(&mut self, _: Chunk<'_>) {}

    fn dcs_unhook(&mut self, _: StringEnd) {}

    fn apc_start(&mut self) {}

    fn apc_put(&mut self, _: Chunk<'_>) {}

    fn apc_end(&mut self, _: StringEnd) {}

    fn xtgettcap(&mut self, _: Option<&[u8]>) {}
}
