//! The terminal: the bytes a program writes, parsed by `vtium-parse`, and
//! their actions applied to the screen, with the modes, SGR and the
//! replies; and what the terminal sends the program for keys, the pointer,
//! focus and pastes.
//!
//! The dispatch here carries the printing, the C0 and C1 controls, and the
//! control functions. The modes go through the mode table (`modes.rs`),
//! SGR through `sgr.rs`, the replies through `reply.rs`, the character
//! sets through `charset.rs`, OSC 52 through `clipboard.rs`, the colour
//! OSC strings through `color.rs`, and the keys,
//! the pointer, focus and pastes through `input.rs`.
//!
//! A control function is matched by its final byte, its private marker and
//! its intermediates exactly, as `vtium-parse`'s README asks: `CSI ? 5 A`
//! is not CUU.

mod charset;
mod clipboard;
mod color;
mod input;
mod modes;
mod rect;
mod reply;
mod sgr;

use vtium_parse::{Chunk, Consumer, InputMode, Intermediates, Params, Parser, StringEnd};

use crate::bounds::REPLIES_PENDING_MAX;
use crate::frame::CursorShape;
use crate::palette::Palette;
use crate::screen::{Erase, Screen, WidthMethod};
use crate::style::Style;
use crate::termcap::Termcap;

pub use input::{Mods, MouseButton, MouseEvent};
pub use modes::MouseTracking;
pub use reply::VersionError;

use charset::{Charset, Charsets};
use modes::Modes;
use reply::Query;

/// DECSCUSR's cursor styles: the host's default, or a shape that blinks or
/// stays steady.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorStyle {
    /// DECSCUSR 0: the host's style, blinking as the host says.
    #[default]
    Default,
    BlockBlink,
    BlockSteady,
    UnderlineBlink,
    UnderlineSteady,
    BarBlink,
    BarSteady,
}

impl CursorStyle {
    /// DECSCUSR's style for `Ps`: 0 to 6, and none for a value past 6,
    /// which xterm ignores.
    fn from_param(ps: u32) -> Option<Self> {
        Some(match ps {
            0 => Self::Default,
            1 => Self::BlockBlink,
            2 => Self::BlockSteady,
            3 => Self::UnderlineBlink,
            4 => Self::UnderlineSteady,
            5 => Self::BarBlink,
            6 => Self::BarSteady,
            _ => return None,
        })
    }

    /// DECSCUSR's `Ps` for the style.
    fn param(self) -> u8 {
        match self {
            Self::Default => 0,
            Self::BlockBlink => 1,
            Self::BlockSteady => 2,
            Self::UnderlineBlink => 3,
            Self::UnderlineSteady => 4,
            Self::BarBlink => 5,
            Self::BarSteady => 6,
        }
    }
}

/// A terminal: a parser and the state it drives.
#[derive(Clone)]
pub struct Terminal {
    parser: Parser,
    state: State,
}

/// What DECSC saves and DECRC restores (VT510, DECSC): the cursor, the
/// wrap pending at it, the pen, the autowrap and origin modes, and the
/// character sets.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Saved {
    /// The column is the screen's width while a wrap is pending.
    x: usize,
    y: usize,
    pen: Style,
    auto_wrap: bool,
    origin: bool,
    /// `None` until DECSC; DECRC then returns the sets to the default
    /// mapping, as xterm does.
    charsets: Option<Charsets>,
}

impl Saved {
    /// What DECRC restores when nothing was saved, or after a reset: home,
    /// the default pen, autowrap on, origin mode off and the default
    /// mapping.
    const HOME: Self = Self {
        x: 0,
        y: 0,
        pen: Style::DEFAULT,
        auto_wrap: true,
        origin: false,
        charsets: None,
    };
}

/// The host's settings: what the program embedding the terminal chose.
#[derive(Clone, Debug, PartialEq)]
struct HostSettings {
    /// The default cursor style blinks; mode 12's initial state.
    cursor_blink: bool,
    /// Mode 2027's initial state.
    width_method: WidthMethod,
    /// The host presents frames by mode 2026.
    sync_enabled: bool,
    /// BackSpace sends DEL in place of BS.
    backspace_delete: bool,
    /// XTVERSION's text.
    version: String,
    /// XTGETTCAP's table.
    termcap: Termcap,
    /// A cell's width and height in pixels, once the host knows them.
    cell: Option<(u32, u32)>,
    /// The display the grid fills, in pixels, once the host knows it.
    display: Option<(u32, u32)>,
    /// The palette the host configured, which the colour resets return
    /// to.
    palette: Palette,
}

/// A DCS string being collected.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Dcs {
    /// One the terminal does not act on.
    Ignored,
    /// DECRQSS, `DCS $ q Pt ST`: the request's first bytes, and whether it
    /// was longer than any vtium answers.
    Decrqss {
        text: [u8; 2],
        len: usize,
        over: bool,
    },
    /// `DCS = Ps s`, a synchronized update's begin (1) or end (2).
    SyncUpdate(u32),
}

/// What the parser drives: the screen, the bytes waiting for the program,
/// the modes, and the terminal's own settings.
#[derive(Clone, Debug, PartialEq)]
struct State {
    screen: Screen,
    /// Bytes the terminal writes to the program, in order: replies, and
    /// what keys, the pointer, focus and pastes send.
    out: Vec<u8>,
    modes: Modes,
    host: HostSettings,
    /// The configured palette with the colours the program set: what
    /// frames draw through and the colour queries answer from.
    colors: Palette,
    cursor_style: CursorStyle,
    /// DECSC's slots, the main screen's and the alternate screen's, as
    /// xterm keeps them.
    saved: [Saved; 2],
    /// How bytes are decoded: UTF-8 unless DECSCL chose 7-bit or 8-bit.
    input_mode: InputMode,
    /// The level DECSCL set, 61 to 64; 62, DA1's, until one is set.
    conformance: u8,
    charsets: Charsets,
    /// DECKPAM: the keypad sends application sequences.
    keypad_application: bool,
    /// xterm's modifyOtherKeys level, 0 to 2.
    modify_other_keys: u8,
    /// The cell the pointer was last reported in, for motion reports.
    mouse_last: Option<(usize, usize)>,
    /// Mode 2026 holds that have ended, by any cause.
    sync_frames: u64,
    /// The input call returns: a synchronized update just ended.
    pause: bool,
    dcs: Dcs,
    /// The cursor's blink interval, `CSI 16 ; Pn ]` (the Linux console's).
    cursor_blink_ms: Option<u32>,
    /// The selection OSC 52 set last, until the host takes it.
    clipboard: Option<clipboard::Selection>,
    /// The queries answered, in order, for the reply rule's test.
    #[cfg(test)]
    answered: Vec<Query>,
}

impl Terminal {
    /// A terminal of `cols` by `rows` whose scrollback may be set to at most
    /// `scrollback_cap` lines (see [`Screen::new`]).
    pub fn new(cols: usize, rows: usize, scrollback_cap: usize) -> Self {
        let mut state = State {
            screen: Screen::new(cols, rows, scrollback_cap),
            out: Vec::new(),
            modes: Modes::INITIAL,
            host: HostSettings {
                cursor_blink: true,
                width_method: WidthMethod::Unicode,
                sync_enabled: false,
                backspace_delete: false,
                version: reply::DEFAULT_VERSION.into(),
                termcap: Termcap::default(),
                cell: None,
                display: None,
                palette: Palette::default(),
            },
            colors: Palette::default(),
            cursor_style: CursorStyle::Default,
            saved: [Saved::HOME; 2],
            input_mode: InputMode::Utf8,
            conformance: 62,
            charsets: Charsets::initial(false),
            keypad_application: false,
            modify_other_keys: 0,
            mouse_last: None,
            sync_frames: 0,
            pause: false,
            dcs: Dcs::Ignored,
            cursor_blink_ms: None,
            clipboard: None,
            #[cfg(test)]
            answered: Vec::new(),
        };
        state.set_cursor_style(CursorStyle::Default);
        Self {
            parser: Parser::new(),
            state,
        }
    }

    /// Feeds bytes from the program, and returns how many the parser took:
    /// fewer than all when a synchronized update ended, right after the
    /// byte that ended it, so the host can present the frame; and while
    /// the replies waiting for the host reach
    /// [`REPLIES_PENDING_MAX`](crate::REPLIES_PENDING_MAX), until it takes
    /// them: at the bound it takes nothing.
    pub fn input(&mut self, bytes: &[u8]) -> usize {
        if self.state.out.len() >= REPLIES_PENDING_MAX {
            return 0;
        }
        self.state.pause = false;
        self.parser.advance(&mut self.state, bytes)
    }

    pub fn screen(&self) -> &Screen {
        &self.state.screen
    }

    pub fn screen_mut(&mut self) -> &mut Screen {
        &mut self.state.screen
    }

    /// The bytes written to the program since the last
    /// [`take_replies`](Self::take_replies).
    pub fn replies(&self) -> &[u8] {
        &self.state.out
    }

    pub fn take_replies(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.state.out)
    }

    /// The selection a program set with OSC 52 since the last call, decoded,
    /// at most [`CLIPBOARD_MAX`](crate::CLIPBOARD_MAX) bytes; a later set
    /// replaced an earlier one. The host puts it in its selection buffer, or
    /// drops it when its configuration turns copy-out off.
    pub fn take_clipboard_write(&mut self) -> Option<Vec<u8>> {
        self.state.clipboard.take().map(|s| s.0)
    }

    /// DECSCUSR's style.
    pub fn cursor_style(&self) -> CursorStyle {
        self.state.cursor_style
    }

    /// The host's cursor blink: whether the default cursor style blinks,
    /// mode 12's initial state.
    pub fn set_cursor_blink(&mut self, on: bool) {
        self.state.host.cursor_blink = on;
        let style = self.state.cursor_style;
        self.state.set_cursor_style(style);
    }

    /// The cursor's blink interval the program asked for with the Linux
    /// console's `CSI 16 ; Pn ]`, as the C reference took it.
    pub fn cursor_blink_ms(&self) -> Option<u32> {
        self.state.cursor_blink_ms
    }

    /// The width method RIS and DECSTR return mode 2027 to.
    pub fn set_default_width_method(&mut self, method: WidthMethod) {
        self.state.host.width_method = method;
    }

    /// DECARM: whether the host repeats a held key.
    pub fn auto_repeat(&self) -> bool {
        self.state.modes.auto_repeat()
    }

    /// Which pointer events the program asked for.
    pub fn mouse_tracking(&self) -> MouseTracking {
        self.state.modes.tracking()
    }

    /// Whether the host presents frames by mode 2026; turning it off ends a
    /// hold.
    pub fn set_sync_enabled(&mut self, on: bool) {
        self.state.host.sync_enabled = on;
        if !on {
            self.state.end_sync();
        }
    }

    /// Whether a synchronized update is holding frames.
    pub fn synchronized(&self) -> bool {
        self.state.modes.synchronized()
    }

    /// Ends a synchronized update, as the host's timeout does.
    pub fn end_sync(&mut self) {
        self.state.end_sync();
    }

    /// The synchronized updates that have ended, by any cause.
    pub fn sync_frames(&self) -> u64 {
        self.state.sync_frames
    }

    /// Sets the text XTVERSION replies: at most
    /// [`VERSION_MAX`](crate::VERSION_MAX) bytes, each printable ASCII.
    pub fn set_version(&mut self, text: &str) -> Result<(), VersionError> {
        self.state.host.version = reply::checked_version(text)?.into();
        Ok(())
    }

    /// Sets the table XTGETTCAP answers from.
    pub fn set_termcap(&mut self, termcap: Termcap) {
        self.state.host.termcap = termcap;
    }

    /// The cell's size in pixels, which XTWINOPS 14 and 16 report.
    pub fn set_cell_size(&mut self, width: u32, height: u32) {
        self.state.host.cell = Some((width, height));
    }

    /// The display the grid fills, in pixels, which XTWINOPS 15 reports.
    pub fn set_display_size(&mut self, width: u32, height: u32) {
        self.state.host.display = Some((width, height));
    }

    /// The palette frames draw through and the colour queries answer
    /// from: the configured one with the colours the program set (OSC 4,
    /// 10 to 19 and 21). The host draws the pointer in its
    /// [`pointer`](Palette::pointer) colours.
    pub fn palette(&self) -> &Palette {
        &self.state.colors
    }

    /// The palette the host configured: a named one or a custom one
    /// ([`Palette::named`], [`Palette::custom`]). It replaces the colours
    /// the program set, and is what the colour resets and RIS return to. A
    /// palette that differs from the one drawn recolours every cell, so
    /// every cell is marked changed.
    pub fn set_palette(&mut self, palette: Palette) {
        if self.state.colors != palette {
            self.state.colors = palette.clone();
            self.state.screen.recolour();
        }
        self.state.host.palette = palette;
    }

    /// Whether BackSpace sends DEL (0x7F) in place of BS (0x08).
    pub fn set_backspace_sends_delete(&mut self, on: bool) {
        self.state.host.backspace_delete = on;
    }

    /// Writes bytes for the program from the keyboard, the pointer, focus
    /// or a paste. With SRM reset they are also shown, as local echo; a
    /// reply the echo provokes goes to the program and is not echoed.
    fn send(&mut self, bytes: &[u8]) {
        self.state.out.extend_from_slice(bytes);
        if self.state.modes.send_receive() {
            return;
        }
        let mut rest = bytes;
        while !rest.is_empty() {
            self.state.pause = false;
            let taken = self.parser.advance(&mut self.state, rest);
            if taken == 0 {
                // The replies are at their bound: the rest of the echo is
                // not shown.
                break;
            }
            rest = rest.get(taken..).unwrap_or_default();
        }
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
    /// The default mapping for the input: DEC supplemental graphics in GR
    /// only in 8-bit input, where GR is reachable.
    fn default_charsets(&self) -> Charsets {
        Charsets::initial(self.input_mode == InputMode::EightBit)
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

    /// Sets DECSCUSR's style, and draws it: the default style is a block
    /// that blinks as the host says.
    fn set_cursor_style(&mut self, style: CursorStyle) {
        self.cursor_style = style;
        let (shape, blink) = match style {
            CursorStyle::Default => (CursorShape::Block, self.host.cursor_blink),
            CursorStyle::BlockBlink => (CursorShape::Block, true),
            CursorStyle::BlockSteady => (CursorShape::Block, false),
            CursorStyle::UnderlineBlink => (CursorShape::Underline, true),
            CursorStyle::UnderlineSteady => (CursorShape::Underline, false),
            CursorStyle::BarBlink => (CursorShape::Bar, true),
            CursorStyle::BarSteady => (CursorShape::Bar, false),
        };
        self.screen.set_cursor_style(shape, blink);
    }

    /// DECSC, into the slot of the screen in use.
    fn save_cursor(&mut self) {
        let (x, y) = self.screen.cursor();
        let slot = usize::from(self.screen.alternate());
        if let Some(saved) = self.saved.get_mut(slot) {
            *saved = Saved {
                x,
                y,
                pen: *self.screen.pen(),
                auto_wrap: self.screen.auto_wrap(),
                origin: self.screen.origin_mode(),
                charsets: Some(self.charsets),
            };
        }
    }

    /// DECRC, from the slot of the screen in use: the modes go through the
    /// mode table, then the cursor returns to where it was on the screen,
    /// its pending wrap included.
    fn restore_cursor(&mut self) {
        let slot = usize::from(self.screen.alternate());
        let saved = self.saved.get(slot).copied().unwrap_or(Saved::HOME);
        self.charsets = saved.charsets.unwrap_or_else(|| self.default_charsets());
        self.screen.set_pen(saved.pen);
        modes::set(self, true, 7, saved.auto_wrap);
        modes::set(self, true, 6, saved.origin);
        self.screen.restore_cursor(saved.x, saved.y);
    }

    /// DECSTR, and the first half of RIS: the modes, the pen, the cursor
    /// style, the character sets, the saved cursors, the margins, the tab
    /// stops and the 256 palette colours return to their initial state;
    /// the text, the cursor and the colours OSC 10 to 19 set stay.
    fn soft_reset(&mut self) {
        self.screen.reset();
        self.screen.set_pen(Style::DEFAULT);
        self.set_cursor_style(CursorStyle::Default);
        self.input_mode = InputMode::Utf8;
        self.conformance = 62;
        self.charsets = self.default_charsets();
        self.keypad_application = false;
        self.modify_other_keys = 0;
        self.dcs = Dcs::Ignored;
        modes::reset(self, false);
        self.saved = [Saved::HOME; 2];
        self.reset_colors(false);
    }

    /// RIS: DECSTR's reset, the modes only RIS resets, every colour the
    /// program set, the screen and the scrollback erased, and the cursor
    /// home.
    fn hard_reset(&mut self) {
        self.soft_reset();
        self.reset_colors(true);
        self.screen.erase_screen(Erase::All);
        self.screen.clear_scrollback();
        self.screen
            .set_attribute_extent(crate::screen::Extent::Stream);
        modes::reset(self, true);
        self.screen.move_to(0, 0);
    }

    /// DECSCL, `CSI Pl ; Pc " p`: a soft reset, then 7-bit input for a
    /// VT100 level (61) and 8-bit input for VT200 to VT400 (62 to 64), as
    /// the C reference took it, with the input's default mapping.
    fn set_conformance(&mut self, level: Option<u32>) {
        self.soft_reset();
        self.input_mode = match level {
            Some(61) => InputMode::SevenBit,
            Some(62..=64) => InputMode::EightBit,
            _ => InputMode::Utf8,
        };
        if let Some(level @ 61..=64) = level {
            self.conformance = u8::try_from(level).unwrap_or(62);
        }
        self.charsets = self.default_charsets();
    }

    /// SM or RM of each parameter, in order.
    fn set_modes(&mut self, params: &Params, dec: bool, on: bool) {
        for i in 0..params.count() {
            if let Some(n) = params.get(i) {
                modes::set(self, dec, n, on);
            }
        }
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
            // XTHIMOUSE takes five parameters, and is refused (ADR-1000).
            b'T' if params.count() == 1 => s.scroll_down(n),
            b'I' => s.tab_right(n),
            b'Z' => s.tab_left(n),
            b'b' => s.repeat(n),
            b'g' => match Self::param(params, 0) {
                0 => s.clear_tab_stop(),
                3 => s.clear_tab_stops(),
                _ => {}
            },
            b'r' => s.set_margins(Self::param(params, 0), Self::param(params, 1)),
            b'h' => self.set_modes(params, false, true),
            b'l' => self.set_modes(params, false, false),
            b'm' => sgr::apply(&mut self.screen, params),
            b'c' if params.get(0).unwrap_or(0) == 0 => self.reply_da1(),
            b'n' => self.reply_dsr(params),
            b't' => self.reply_window(params),
            b']' if params.get(0) == Some(16) => {
                if let Some(ms) = params.get(1) {
                    self.cursor_blink_ms = Some(ms);
                }
            }
            // DECLL: a console's LEDs are the keyboard's, and stay as the
            // user set them (ADR-1000, ctl.decll).
            _ => {}
        }
    }

    /// The ESC sequences with no intermediates.
    fn esc_plain(&mut self, final_byte: u8) {
        let s = &mut self.screen;
        match final_byte {
            b'D' => s.move_down(1, true),
            b'E' => s.newline(),
            b'H' => s.set_tab_stop(),
            b'M' => s.move_up(1, true),
            b'N' => self.charsets.single_shift(2),
            b'O' => self.charsets.single_shift(3),
            b'n' => self.charsets.lock_left(2),
            b'o' => self.charsets.lock_left(3),
            b'~' => self.charsets.lock_right(1),
            b'}' => self.charsets.lock_right(2),
            b'|' => self.charsets.lock_right(3),
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'=' => self.keypad_application = true,
            b'>' => self.keypad_application = false,
            b'Z' => self.reply_da1(),
            b'c' => self.hard_reset(),
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

    /// XTMODKEYS, `CSI > Pp ; Pv m`, for modifyOtherKeys (Pp 4): levels 0
    /// to 2 are taken and Pv omitted is 0; `CSI > m` returns every
    /// resource to its initial value, level 0. The other resources are
    /// xterm's alone and ignored.
    fn set_modify_other_keys(&mut self, params: &Params) {
        match (params.count(), params.get(0), params.get(1)) {
            (1, None, _) | (_, Some(4), None) => self.modify_other_keys = 0,
            (_, Some(4), Some(level @ 0..=2)) => {
                self.modify_other_keys = u8::try_from(level).unwrap_or(0);
            }
            _ => {}
        }
    }
}

impl Consumer for State {
    fn print(&mut self, c: char) {
        let eight_bit = self.input_mode == InputMode::EightBit;
        if let Some(c) = self.charsets.map(c, eight_bit) {
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
            // ENQ answers an empty answerback (ADR-1000), so nothing.
            0x05 => {}
            0x08 => s.move_left(1),
            0x09 => s.tab_right(1),
            0x0a..=0x0c if self.modes.newline() => s.newline(),
            0x0a..=0x0c => s.move_down(1, true),
            0x0d => s.move_line_home(),
            0x0e => self.charsets.lock_left(1),
            0x0f => self.charsets.lock_left(0),
            // SUB cancels a sequence and shows its error character.
            0x1a => s.print('\u{BF}'),
            0x84 => s.move_down(1, true),
            0x85 => s.newline(),
            0x88 => s.set_tab_stop(),
            0x8d => s.move_up(1, true),
            0x8e => self.charsets.single_shift(2),
            0x8f => self.charsets.single_shift(3),
            0x9a => self.reply_da1(),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: Intermediates, final_byte: u8) {
        // SCS for a 94-character set: ESC (, ), * or + designates G0 to G3.
        let slot = match intermediates.as_bytes() {
            [] => return self.esc_plain(final_byte),
            [b'('] => 0,
            [b')'] => 1,
            [b'*'] => 2,
            [b'+'] => 3,
            // S7C1T and S8C1T (ESC SP F, ESC SP G) are taken and change
            // nothing: replies stay 7-bit, as the C reference sent them.
            _ => return,
        };
        let eight_bit = self.input_mode == InputMode::EightBit;
        if let Some(set) = Charset::from_final(final_byte, eight_bit) {
            self.charsets.designate(slot, set);
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        // Only SGR and DECCARA read sub-parameters; any other sequence
        // carrying them does nothing, as in the C reference.
        let reads_subparams = matches!(
            (intermediates.as_bytes(), final_byte),
            (_, b'm') | ([b'$'], b'r')
        );
        if !reads_subparams && params.has_subparams() {
            return;
        }
        match (params.marker(), intermediates.as_bytes(), final_byte) {
            (None, [], _) => self.csi_plain(params, final_byte),
            (Some(b'?'), [], b'J' | b'K') => self.erase(params, final_byte, Erase::Unprotected),
            (Some(b'?'), [], b'h') => self.set_modes(params, true, true),
            (Some(b'?'), [], b'l') => self.set_modes(params, true, false),
            (None, [b'$'], b'p') => self.reply_mode(params, false),
            (Some(b'?'), [b'$'], b'p') => self.reply_mode(params, true),
            (None, [b'!'], b'p') => self.soft_reset(),
            (None, [b'"'], b'p') => self.set_conformance(params.get(0)),
            (None, [b'"'], b'q') => self.set_protection(params),
            (None, [b'$'], b'r' | b't' | b'v' | b'x' | b'z' | b'{') => {
                self.rect_op(params, final_byte);
            }
            (None, [b'*'], b'x') => self.set_attribute_extent(params),
            (None, [b' '], b'q') => {
                if let Some(style) = CursorStyle::from_param(params.get(0).unwrap_or(0)) {
                    self.set_cursor_style(style);
                }
            }
            (Some(b'>'), [], b'q') if params.get(0).unwrap_or(0) == 0 => self.reply_version(),
            (Some(b'>'), [], b'c') if params.get(0).unwrap_or(0) == 0 => self.reply_da2(),
            (Some(b'='), [], b'c') if params.get(0).unwrap_or(0) == 0 => self.reply_da3(),
            (Some(b'?'), [], b'n') if params.get(0) == Some(6) => self.reply_cursor(true),
            (Some(b'>'), [], b'm') if !params.has_subparams() => {
                self.set_modify_other_keys(params);
            }
            (Some(b'?'), [], b'm') if params.get(0) == Some(4) && !params.has_subparams() => {
                self.reply_modify_other_keys();
            }
            (Some(b'>'), [], b'n') if params.get(0) == Some(4) => self.modify_other_keys = 0,
            _ => {}
        }
    }

    /// A truncated string is dropped whole, as vtium-parse's README asks.
    fn osc_end(&mut self, data: &[u8], truncated: bool, end: StringEnd) {
        if !end.is_complete() || truncated {
            return;
        }
        match data.strip_prefix(b"52;") {
            Some(args) => self.set_clipboard(args),
            None => {
                self.osc_color(data, end);
            }
        }
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        self.dcs = match (params.marker(), intermediates.as_bytes(), final_byte) {
            (None, [b'$'], b'q') if params.count() == 1 && params.get(0).is_none() => {
                Dcs::Decrqss {
                    text: [0; 2],
                    len: 0,
                    over: false,
                }
            }
            (Some(b'='), [], b's') if params.count() == 1 => match params.get(0) {
                Some(n @ (1 | 2)) => Dcs::SyncUpdate(n),
                _ => Dcs::Ignored,
            },
            _ => Dcs::Ignored,
        };
    }

    fn dcs_put(&mut self, data: Chunk<'_>) {
        if let Dcs::Decrqss { text, len, over } = &mut self.dcs {
            match data {
                Chunk::Bytes(bytes) => {
                    for &b in bytes {
                        match text.get_mut(*len) {
                            Some(slot) => {
                                *slot = b;
                                *len = len.saturating_add(1);
                            }
                            None => *over = true,
                        }
                    }
                }
                Chunk::Char(_) => *over = true,
            }
        }
    }

    fn dcs_unhook(&mut self, end: StringEnd) {
        let dcs = core::mem::replace(&mut self.dcs, Dcs::Ignored);
        if !end.is_complete() {
            return;
        }
        match dcs {
            Dcs::Decrqss { text, len, over } => {
                let request = if over { None } else { text.get(..len) };
                self.reply_status(request);
            }
            // The same hold as mode 2026, through the mode table.
            Dcs::SyncUpdate(n) => modes::set(self, true, 2026, n == 1),
            Dcs::Ignored => {}
        }
    }

    fn apc_start(&mut self) {}

    fn apc_put(&mut self, _: Chunk<'_>) {}

    fn apc_end(&mut self, _: StringEnd) {}

    fn xtgettcap(&mut self, payload: Option<&[u8]>) {
        let reply = self.host.termcap.answer(payload);
        self.reply(Query::Xtgettcap, &reply);
    }

    fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    fn paused(&self) -> bool {
        self.pause || self.out.len() >= REPLIES_PENDING_MAX
    }
}
