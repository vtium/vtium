//! Heads (ADR-1401 D1): a terminal for one or more displays, each holding
//! its own session, its pty, its screen and scrollback, and its own grid,
//! laid out over its own displays by the scaled rule ([`crate::layout`])
//! at its own density, zoom and rotation.
//!
//! [`identity`] maps vtium-platform's identities of a display and of an
//! input into the head file's, which the rules walk. The app holds the
//! heads, places each display in one, and drives each head's session as
//! it drove the seat's one session before heads, as the C reference's
//! seat held its sessions, each one terminal with its own grid
//! (`src/seat.c:53-108`, `src/terminal.c:1107-1170` at kmscon b1f7a7c5).

pub(crate) mod chord;
pub mod console;
pub mod identity;
pub(crate) mod route;
pub(crate) mod session;

use vtium_config::{By, OnLoss, Place};

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use vtium_font::Font;
use vtium_platform::display::DisplayId;
use vtium_platform::input::{Button, PointerEvent};
use vtium_platform::pty::{Pty, Size};
use vtium_render::{
    Config as RenderConfig, DecorationColors, DrawState, Orientation, PointerShape, Profile,
};
use vtium_term::{Content, Mods, MouseButton, MouseEvent, MouseTracking, Terminal};

use crate::blink::Blink;
use crate::config::{Config, keys};
use crate::deferred::Shown;
use crate::font::{self, Chosen, DisplayFace};
use crate::head::console::{Console, Fit};
use crate::layout::Zoom;
use crate::painter::{Figures, Painter};
use crate::sync::Sync;
use crate::term::{Clock, Term, View};

/// The grid before any display is known.
pub const GRID: (usize, usize) = (80, 24);

/// How long a notice shows with no key reaching its head (ADR-1401 D7,
/// probe 5).
pub const NOTICE_SHOWN: Duration = Duration::from_secs(30);

/// The most notices a head shows at once, the newest: an invalid head
/// file's, no keyboard found, the anti-lockout guard's and a rescue.
pub const NOTICES_MAX: usize = 4;

/// The lines of scrollback a wheel event scrolls (`src/terminal.c:1648`).
const WHEEL_LINES: usize = 3;

/// Whether a head has a display to draw on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attached {
    /// None yet since the start.
    NotYet,
    Yes,
    /// The last one went.
    Gone,
}

/// The pointer, in the grid's pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pointer {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) visible: bool,
    held: Option<Button>,
    /// The held button selects or pastes, until its release, rather than
    /// being reported: its press came while the program did not track the
    /// pointer, or with Shift.
    ours: bool,
    /// The left button's drag extends the selection.
    pub(crate) drag: Option<Drag>,
    /// Left presses in a row, each soon after the last: one selects
    /// characters, two a word, three a line, and a fourth starts again.
    clicks: u8,
}

/// A drag that extends the selection from the cell it started in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Drag {
    from: (usize, usize),
    /// It left that cell.
    moved: bool,
}

/// One head: its consoles, one shown, its displays, and the shown
/// console's grid.
pub struct Head {
    name: String,
    /// The console shown.
    pub(crate) console: Console,
    /// The other consoles in the ring's order, the shown one's place among
    /// them at [`Head::at`].
    pub(crate) hidden: Vec<Console>,
    /// The shown console's place in the ring.
    pub(crate) at: usize,
    /// The next console's id: a head numbers its consoles from 1 as it
    /// opens them and never reuses a number (ADR-1401 D5).
    next_id: u64,
    /// A new console's `font-size`, as configured: in points, or `0`.
    configured: (Option<u32>, bool),
    /// The consoles the head opens at its start, its file's `consoles`
    /// within the caps.
    pub(crate) wanted: usize,
    /// The configuration its consoles' command is read from: kmscon.conf's
    /// with the head's own `login` over it.
    pub(crate) login: Option<Config>,
    /// Its last console's program ended under `oneshot`, so the head runs
    /// no session any more.
    pub(crate) ended: bool,
    /// What the head shows once its session has ended, from the catalog.
    pub(crate) ended_notice: String,
    /// vtium's notices to the head (ADR-1401 D7), each once and at most
    /// [`NOTICES_MAX`], the oldest going first, until a key reaches the
    /// head or [`NOTICE_SHOWN`] after they are first drawn, when they end.
    notices: Vec<String>,
    /// The notices as one text, a notice a line.
    shown: String,
    /// When the notices end, armed at their first frame; an ended head's
    /// notice keeps no time.
    pub(crate) notice_due: Option<Instant>,
    /// The on-screen display shows (ADR-1401 D5): since a change, until
    /// its time runs out.
    pub(crate) osd_shown: bool,
    /// When the on-screen display goes, armed at its first frame.
    pub(crate) osd_due: Option<Instant>,
    /// The shown console's id and the number of consoles at the last
    /// frame: a difference is a change the on-screen display shows.
    pub(crate) osd_seen: Option<(u64, usize)>,
    /// The on-screen display's dot, while it shows under `osd = minimal`.
    pub(crate) mark: Option<vtium_render::Mark>,
    /// The on-screen display's line, while it shows under `osd = full`.
    pub(crate) status: Option<String>,
    /// What the head does when it loses its last display (ADR-1401 D8).
    pub(crate) on_loss: OnLoss,
    /// When its sessions hang up, while it has no display.
    pub(crate) hangup_due: Option<Instant>,
    pub(crate) views: BTreeMap<(PathBuf, DisplayId), View>,
    pub(crate) attached: Attached,
    /// Every display of the head draws at once at the end of the turn.
    pub(crate) draw_now: bool,
    pub(crate) text_due: Option<Instant>,
    pub(crate) cursor_due: Option<(Instant, Duration)>,
    pub(crate) pointer: Pointer,
    /// A keyboard delivers to the head: its terminal has mode 1004's
    /// focus.
    pub(crate) focused: bool,
    /// The mouse tracking the program asked for, last seen: a change
    /// clears the selection.
    tracking: MouseTracking,
    /// The head's font keys, kmscon.conf's with the head file's over them
    /// (ADR-1401 D3): the name, `None` for kmscon.conf's own choice.
    pub(crate) font_name: Option<String>,
    /// The base font's height; each display may draw larger
    /// ([`crate::layout::layout`]).
    pub(crate) font_size: u32,
    /// `font-size` in points: the base height follows the displays'
    /// density, until a zoom.
    pub(crate) points: Option<u32>,
    /// `font-size=0`: the base height follows the smallest display, until
    /// a zoom.
    pub(crate) auto_size: bool,
    /// A `font-size` other than 0 is configured.
    sized: bool,
    /// The base font was found once: a later failure keeps it.
    pub(crate) has_font: bool,
    /// The base font's size step, which a zoom takes.
    pub(crate) step: u32,
    /// The base font's cell, the grid's and the pty's.
    pub(crate) cell: (u32, u32),
    /// The font set, chosen at the head's first display and switched by
    /// OSC 50; a frit's store is the one every head drawing it shares
    /// ([`Frit`]).
    pub(crate) font: Option<Chosen>,
    /// The font changed: every display's renderer is rebuilt.
    pub(crate) refont: bool,
    /// OSC 50's refusals since its last switch: only the first is logged.
    pub(crate) font_refused: u64,
    pub(crate) orientation: Orientation,
    pub(crate) grid: (usize, usize),
    /// Runs before each of the head's frames is composed: a test's
    /// stand-in for a slow draw.
    pub(crate) draw_hook: Option<DrawHook>,
    /// The head's render thread, started at its first frame; `None` after
    /// it could not start, when the loop composes the head's frames.
    pub(crate) painter: Option<Painter>,
    pub(crate) painter_refused: bool,
    pub(crate) figures: Figures,
}

/// A console's terminal for `config`, on [`GRID`] with the scrollback cap
/// and length `scrollback` and `bytes` of scrollback memory.
pub(crate) fn terminal(
    config: &Config,
    scrollback: (usize, usize),
    bytes: usize,
    clock: Clock,
) -> Term {
    let (cap, lines) = scrollback;
    let mut terminal = Terminal::new(GRID.0, GRID.1, cap);
    terminal.screen_mut().set_scrollback_max(lines);
    terminal.screen_mut().set_scrollback_memory(bytes);
    terminal.set_backspace_sends_delete(config.switch(&keys::BACKSPACE_DELETE));
    terminal.set_palette(config.palette());
    terminal.set_font_ops(config.switch(&keys::ALLOW_FONT_OPS));
    if let Ok(table) = crate::termcap::termcap() {
        terminal.set_termcap(table);
    }
    let timeout = Duration::from_millis(config.number(&keys::SYNC_TIMEOUT).unwrap_or(200));
    terminal.set_sync_enabled(!timeout.is_zero());
    let sync = Sync::new(timeout, terminal.sync_frames());
    Term::new(terminal, sync, Blink::configured(config), clock)
}

/// The scrollback memory each console takes (ADR-1401 D10): `sb-memory`,
/// at most `cap`, the most `sb-memory` may be on the machine, shared
/// among the most consoles every head may hold: `consoles`, or the
/// `heads` when more, since each head keeps its first console. All of
/// them together stay within `cap`.
pub(crate) fn console_scrollback(
    sb_memory: usize,
    cap: u64,
    consoles: usize,
    heads: usize,
) -> usize {
    let most = u64::try_from(consoles.max(heads)).unwrap_or(u64::MAX);
    let share = cap.checked_div(most).unwrap_or(cap);
    sb_memory.min(usize::try_from(share).unwrap_or(usize::MAX))
}

/// The bytes each frit's glyph store holds: `store`, the machine's store
/// limit, shared among the `heads`, since a head draws one frit at a
/// time, so every frit's store together stays within `store`.
pub(crate) fn frit_store(store: u64, heads: usize) -> u64 {
    store
        .checked_div(u64::try_from(heads).unwrap_or(u64::MAX))
        .unwrap_or(store)
}

/// The text of `console`'s screen rows.
fn screen_text(console: &Console) -> Vec<String> {
    console
        .term
        .terminal
        .screen()
        .lines()
        .iter()
        .map(|line| {
            line.cells()
                .iter()
                .map(|c| match c.content() {
                    Content::Char(ch) => ch,
                    Content::Empty | Content::Cluster(_) => ' ',
                })
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// What [`Head`]'s `draw_hook` runs.
pub type DrawHook = std::sync::Arc<dyn Fn() + Send + std::marker::Sync>;

impl Head {
    /// The head `name` for `config`, its one console's terminal `term`,
    /// and the font keys `font` (ADR-1401 D3): its session starts at its
    /// first display.
    pub(crate) fn new(name: &str, config: &Config, font: &vtium_config::Font, term: Term) -> Head {
        let size = font
            .size
            .map(u64::from)
            .or_else(|| config.number(&keys::FONT_SIZE));
        let points = size
            .filter(|&n| n != 0)
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
        let auto_size = size.unwrap_or(0) == 0;
        let fit = Fit {
            grid: GRID,
            font_size: 0,
            points,
            auto_size,
        };
        Head {
            name: name.to_owned(),
            console: Console::new(1, term, fit),
            hidden: Vec::new(),
            at: 0,
            next_id: 2,
            configured: (points, auto_size),
            wanted: 1,
            login: None,
            ended: false,
            ended_notice: crate::notice::session_ended(),
            notices: Vec::new(),
            shown: String::new(),
            notice_due: None,
            osd_shown: false,
            osd_due: None,
            osd_seen: None,
            mark: None,
            status: None,
            on_loss: OnLoss::Nohup,
            hangup_due: None,
            views: BTreeMap::new(),
            attached: Attached::NotYet,
            draw_now: false,
            text_due: None,
            cursor_due: None,
            pointer: Pointer::default(),
            focused: false,
            tracking: MouseTracking::Off,
            font_name: font.name.clone(),
            font_size: 0,
            points,
            auto_size,
            sized: size.unwrap_or(0) != 0,
            has_font: false,
            step: 16,
            cell: (8, 16),
            font: None,
            refont: false,
            font_refused: 0,
            orientation: Orientation::Normal,
            grid: GRID,
            draw_hook: None,
            painter: None,
            painter_refused: false,
            figures: Figures::default(),
        }
    }

    /// The head's name, as its `[head NAME]` gives it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The line of the head's frame times, once it is due at `now`.
    pub(crate) fn figures_line(&mut self, now: std::time::Instant) -> Option<String> {
        self.figures.line(&self.name, now)
    }

    /// The head's grid.
    pub fn grid(&self) -> (usize, usize) {
        self.grid
    }

    /// Each display's font cell, in the order of the head's displays.
    pub fn cells(&self) -> Vec<(u32, u32)> {
        self.views
            .values()
            .map(|v| {
                let c = v.cell();
                (c.width, c.height)
            })
            .collect()
    }

    /// The shown console's terminal.
    pub fn term(&self) -> &Term {
        &self.console.term
    }

    /// The shown console's pty, while its command runs.
    pub fn pty(&self) -> Option<&Pty> {
        self.console.pty.as_ref()
    }

    /// Whether a console's command has started.
    pub fn started(&self) -> bool {
        self.consoles().any(Console::started)
    }

    /// The head's consoles, the shown one first.
    pub(crate) fn consoles(&self) -> impl Iterator<Item = &Console> {
        std::iter::once(&self.console).chain(&self.hidden)
    }

    /// The head's consoles, the shown one first.
    pub(crate) fn consoles_mut(&mut self) -> impl Iterator<Item = &mut Console> {
        std::iter::once(&mut self.console).chain(&mut self.hidden)
    }

    /// The console `id`, shown or hidden.
    pub(crate) fn console_mut(&mut self, id: u64) -> Option<&mut Console> {
        self.consoles_mut().find(|c| c.id == id)
    }

    /// How many consoles the head has.
    pub fn console_count(&self) -> usize {
        self.hidden.len().saturating_add(1)
    }

    /// The shown console's place among the head's, in the order they were
    /// opened.
    pub fn shown_console(&self) -> usize {
        self.at
    }

    /// Where the pointer is on the head's grid, in its pixels, while shown.
    pub fn pointer(&self) -> Option<(u32, u32)> {
        self.pointer
            .visible
            .then_some((self.pointer.x, self.pointer.y))
    }

    /// What vtium shows on the head of its own: why its session ended
    /// for good, else its notices.
    pub fn notice(&self) -> Option<&str> {
        self.ended
            .then_some(self.ended_notice.as_str())
            .or_else(|| (!self.shown.is_empty()).then_some(self.shown.as_str()))
    }

    /// The on-screen display's dot, while it shows.
    pub fn mark(&self) -> Option<vtium_render::Mark> {
        self.mark
    }

    /// The on-screen display's line, while it shows.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// The on-screen display shows from the head's next frame, its time
    /// afresh (ADR-1401 D5).
    pub(crate) fn show_osd(&mut self) {
        self.osd_shown = true;
        self.osd_due = None;
        self.draw_now = true;
    }

    /// The on-screen display the head showed, or was to show, is dropped:
    /// its last display went (ADR-1401 D5).
    pub(crate) fn drop_osd(&mut self) {
        self.osd_shown = false;
        self.osd_due = None;
    }

    /// Adds `text` to the head's notices, which show from its next frame
    /// for [`NOTICE_SHOWN`] afresh; a notice the head already shows is
    /// left as it is, its time with it.
    pub(crate) fn add_notice(&mut self, text: &str) {
        if self.notices.iter().any(|n| n == text) {
            return;
        }
        if self.notices.len() >= NOTICES_MAX {
            self.notices.remove(0);
        }
        self.notices.push(text.to_owned());
        self.shown = self.notices.join("\n");
        self.notice_due = None;
        self.draw_now = true;
    }

    /// Whether the head has notices that end.
    pub(crate) fn has_notices(&self) -> bool {
        !self.notices.is_empty()
    }

    /// The head's notices end: a key reached it, or their time ran out.
    pub(crate) fn end_notices(&mut self) {
        self.notice_due = None;
        if !self.notices.is_empty() {
            self.notices.clear();
            self.shown.clear();
            self.draw_now = true;
        }
    }

    /// What vtium draws over the head's frames: one box at the top holding
    /// the on-screen display's line and the notices beneath it, a problem
    /// while a notice is in it, and the on-screen display's dot.
    pub(crate) fn overlay(&self) -> crate::term::Overlay {
        let notices: Vec<&str> = self.notice().into_iter().collect();
        crate::term::Overlay::new(self.status.as_deref(), &notices, self.mark)
    }

    /// How the shown console fits the displays.
    fn fit(&self) -> Fit {
        Fit {
            grid: self.grid,
            font_size: self.font_size,
            points: self.points,
            auto_size: self.auto_size,
        }
    }

    /// A new console's fit: the shown one's grid, at the configured
    /// `font-size`.
    pub(crate) fn fresh_fit(&self) -> Fit {
        let (points, auto_size) = self.configured;
        Fit {
            points,
            auto_size,
            ..self.fit()
        }
    }

    /// The next console's id.
    pub(crate) fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = id.saturating_add(1);
        id
    }

    /// Shows `new` in place of the console shown, which is returned: the
    /// head takes `new`'s fit, and the old keeps the head's. While
    /// `visible`, the new one is potentially visible (mode 2033), and it
    /// takes mode 1004's focus from the old while a keyboard delivers to
    /// the head. Every display draws the new one whole, as the C reference
    /// redrew a session's displays at its activation
    /// (`src/terminal.c:1750-1762` at kmscon b1f7a7c5), each drawing kept
    /// per session there and per head here.
    fn swap_shown(&mut self, new: Console, visible: bool) -> Console {
        let fit = self.fit();
        self.grid = new.fit.grid;
        self.font_size = new.fit.font_size;
        self.points = new.fit.points;
        self.auto_size = new.fit.auto_size;
        let mut old = std::mem::replace(&mut self.console, new);
        old.fit = fit;
        old.term.terminal.set_visible(false);
        self.console.term.terminal.set_visible(visible);
        if self.focused {
            old.term.terminal.focus(false);
            self.console.term.terminal.focus(true);
        }
        old.send_replies();
        self.send_user_bytes();
        self.pointer.drag = None;
        for v in self.views.values_mut() {
            v.show_another();
        }
        self.draw_now = true;
        old
    }

    /// Shows the console at ring position `to`.
    pub(crate) fn show(&mut self, to: usize, visible: bool) {
        let k = console::hidden_index(to, self.at);
        if to == self.at || k >= self.hidden.len() {
            return;
        }
        let new = self.hidden.remove(k);
        let old = self.swap_shown(new, visible);
        let back = console::hidden_index(self.at, to).min(self.hidden.len());
        self.hidden.insert(back, old);
        self.at = to;
    }

    /// Opens `console` at the ring's end and shows it.
    pub(crate) fn push(&mut self, console: Console, visible: bool) {
        self.hidden.push(console);
        self.show(self.hidden.len(), visible);
    }

    /// Takes the shown console out and shows the next, the ring's first
    /// after the last; `None`, changing nothing, when it is the only one.
    pub(crate) fn close_shown(&mut self, visible: bool) -> Option<Console> {
        if self.hidden.is_empty() {
            return None;
        }
        let next = if self.at < self.hidden.len() {
            self.at
        } else {
            0
        };
        let new = self.hidden.remove(next);
        let old = self.swap_shown(new, visible);
        self.at = next;
        Some(old)
    }

    /// Takes the hidden console `id` out.
    pub(crate) fn remove_hidden(&mut self, id: u64) -> Option<Console> {
        let k = self.hidden.iter().position(|c| c.id == id)?;
        if k < self.at {
            self.at = self.at.saturating_sub(1);
        }
        Some(self.hidden.remove(k))
    }

    /// The text of the shown console's screen rows, for tests and logs.
    pub fn screen_text(&self) -> Vec<String> {
        screen_text(&self.console)
    }

    /// The text of each console's screen rows, in the ring's order, for
    /// tests.
    pub fn consoles_text(&self) -> Vec<Vec<String>> {
        let mut all: Vec<Vec<String>> = self.hidden.iter().map(screen_text).collect();
        all.insert(self.at.min(all.len()), self.screen_text());
        all
    }

    /// Whether the head's `font-size` is configured, kmscon.conf's or its
    /// own, which wins over a frit's cell (ADR-1200 D16); `font-size=0`
    /// is the automatic size.
    pub(crate) fn sized(&self) -> bool {
        self.sized
    }

    /// The font set in use, the built-ins before one is chosen.
    pub(crate) fn chosen(&self) -> &Chosen {
        const BUILTIN: &Chosen = &Chosen::Builtin(None);
        self.font.as_ref().unwrap_or(BUILTIN)
    }

    /// The font set's face at `height`.
    pub(crate) fn face(&self, height: u32, locale: &str) -> Result<DisplayFace, String> {
        let chosen = self.chosen();
        chosen.face(height, locale).ok_or_else(|| {
            format!(
                "the set {} draws nothing at {height} pixels",
                chosen.name(height, locale)
            )
        })
    }

    /// The font at `height`, with the frit's sprite policy and the
    /// configuration's over it, and the missing glyph.
    pub(crate) fn font_at(&self, face: DisplayFace, config: &Config) -> Font<DisplayFace> {
        let font = Font::new(face);
        let own = match self.chosen() {
            Chosen::Frit { frit, .. } => font::frit_policy(&frit.text, font.policy()),
            Chosen::Builtin(_) => font.policy(),
        };
        let policy = font::policy(config, own);
        font.with_policy(policy).with_missing(font::missing(config))
    }

    pub(crate) fn render_config(&self, config: &Config) -> RenderConfig {
        let profile = match config.text(&keys::GLYPH_CACHE) {
            Some("performance") => Profile::Performance,
            Some("minspec") => Profile::Minspec,
            _ => Profile::default(),
        };
        RenderConfig {
            profile,
            orientation: self.orientation,
            blink: crate::blink::text_blink(config),
            decorations: DecorationColors {
                underline: config.deco_color(&keys::UNDERLINE_COLOR),
                strike: config.deco_color(&keys::STRIKE_COLOR),
                force: config.switch(&keys::FORCE_DECORATION_COLORS),
            },
            pointer: config
                .char(&keys::POINTER_CHAR)
                .map_or(PointerShape::Arrow, PointerShape::Glyph),
            ..RenderConfig::default()
        }
    }

    pub(crate) fn pty_size(&self) -> Size {
        let cell = self.cell;
        Size {
            cols: u16::try_from(self.grid.0).unwrap_or(u16::MAX),
            rows: u16::try_from(self.grid.1).unwrap_or(u16::MAX),
            cell_width: cell.0,
            cell_height: cell.1,
        }
    }

    /// The shown console's replies go to its program.
    pub(crate) fn send_user_bytes(&mut self) {
        self.console.send_replies();
    }

    /// The shown console is potentially visible while it draws: not while
    /// the VT is another's or the displays are blanked. Mode 2033's report
    /// goes out at once, to a program that may be waiting in a read.
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.console.term.terminal.set_visible(visible);
        self.send_user_bytes();
    }

    /// The shown console's terminal and pty at `grid`, the pointer's last
    /// pixel the grid's.
    pub(crate) fn resize(&mut self, grid: (usize, usize)) {
        let (cw, ch) = self.cell;
        let (w, h) = self.pixels(grid);
        self.console.term.terminal.set_cell_size(cw, ch);
        self.console.term.terminal.set_display_size(w, h);
        for v in self.views.values_mut() {
            v.lost = true;
        }
        self.draw_now = true;
        if grid != self.grid {
            self.grid = grid;
            let size = self.pty_size();
            if let Some(pty) = &mut self.console.pty {
                if let Err(e) = pty.resize(size) {
                    log::warn!("cannot resize the pty: {e}");
                }
            }
        }
        // Last, so mode 2048 reports the resize once it is complete, a new
        // font on the same grid included; the report goes out at once.
        self.console.term.terminal.resize(grid.0, grid.1);
        self.send_user_bytes();
    }

    /// `grid` in the base cell's pixels.
    pub(crate) fn pixels(&self, grid: (usize, usize)) -> (u32, u32) {
        let px = |n: usize, c: u32| u32::try_from(n).unwrap_or(u32::MAX).saturating_mul(c);
        (px(grid.0, self.cell.0), px(grid.1, self.cell.1))
    }

    /// What the displays show, read from the view each buffer draws,
    /// scrolled back or not (#149): the characters, every character of a
    /// cluster among them, and the clusters whole (ADR-1200 D21), each
    /// sorted and once.
    pub(crate) fn visible(&self) -> Shown {
        use vtium_term::{Age, Glyph};
        let palette = self.console.term.terminal.palette();
        let frame = self
            .console
            .term
            .terminal
            .screen()
            .frame(palette, Age::NEVER);
        let mut shown = Shown::default();
        for row in 0..frame.rows() {
            for cell in frame.cells(row) {
                match cell.glyph {
                    Glyph::Char(c) => shown.chars.push(c),
                    Glyph::Cluster(text) => {
                        shown.chars.extend(text.chars());
                        shown.clusters.push(text.chars().collect());
                    }
                    Glyph::Empty => {}
                }
            }
        }
        shown.chars.retain(|c| !c.is_whitespace());
        shown.chars.sort_unstable();
        shown.chars.dedup();
        shown.clusters.sort_unstable();
        shown.clusters.dedup();
        shown
    }

    pub(crate) fn draw_state(&self) -> DrawState {
        let palette = self.console.term.terminal.palette();
        DrawState {
            cursor_on: self.console.term.blink.cursor_shown(),
            blink_hidden: !self.console.term.blink.text_shown(),
            background: palette.bg(),
            pointer: self
                .pointer
                .visible
                .then_some((self.pointer.x, self.pointer.y)),
        }
    }

    /// A pointer event: reported to a program that tracks the pointer,
    /// unless Shift is held, as xterm's `ShiftOverride` lets it select,
    /// paste and scroll (`button.c`); otherwise it selects, copies, pastes
    /// and scrolls through the seat's `selection` buffer. A button keeps
    /// to the side its press took until its release, whatever Shift does
    /// meanwhile. Returns the zoom a wheel with Ctrl asks for.
    pub(crate) fn pointer_event(
        &mut self,
        event: PointerEvent,
        mods: Mods,
        selection: &mut Option<Vec<u8>>,
    ) -> Option<Zoom> {
        let tracking = self.console.term.terminal.mouse_tracking() != MouseTracking::Off;
        let shift = mods.contains(Mods::SHIFT);
        let moved = matches!(event, PointerEvent::Moved { .. });
        let mut zoom = None;
        match event {
            PointerEvent::Moved { x, y, held } => {
                self.pointer.x = u32::try_from(x).unwrap_or(0);
                self.pointer.y = u32::try_from(y).unwrap_or(0);
                self.pointer.visible = true;
                self.pointer.held = held;
                // A drag whose release was lost ends with the left button
                // no longer held.
                if held != Some(Button::Left) {
                    self.pointer.drag = None;
                }
                let (col, row) = self.pointer_cell();
                if let Some(drag) = &mut self.pointer.drag {
                    drag.moved |= (col, row) != drag.from;
                    self.console.term.terminal.screen_mut().select_to(col, row);
                } else if tracking
                    && if held.is_some() {
                        !self.pointer.ours
                    } else {
                        !shift
                    }
                {
                    let button = held.map_or(MouseButton::None, mouse_button);
                    self.report_pointer(button, MouseEvent::Move, mods);
                }
            }
            PointerEvent::Button {
                button,
                pressed,
                double_click,
            } => {
                if pressed {
                    self.pointer.ours = !tracking || shift;
                }
                if self.pointer.ours {
                    self.select_button(button, pressed, double_click, selection);
                } else {
                    self.pointer.clicks = 0;
                    let event = if pressed {
                        MouseEvent::Press
                    } else {
                        MouseEvent::Release
                    };
                    self.report_pointer(mouse_button(button), event, mods);
                }
            }
            PointerEvent::Wheel(steps) => {
                let ctrl = mods.contains(Mods::CTRL);
                match wheel(steps, tracking && !shift, ctrl) {
                    Wheel::Report(button) => self.report_pointer(button, MouseEvent::Press, mods),
                    Wheel::Zoom(dir) => zoom = Some(dir),
                    Wheel::Scroll { up: true } => {
                        self.console
                            .term
                            .terminal
                            .screen_mut()
                            .scroll_view_up(WHEEL_LINES);
                    }
                    Wheel::Scroll { up: false } => {
                        self.console
                            .term
                            .terminal
                            .screen_mut()
                            .scroll_view_down(WHEEL_LINES);
                    }
                }
            }
            PointerEvent::Sync => {}
            // The C reference cleared the selection as it hid the pointer
            // (`src/terminal.c:1655-1659` at kmscon b1f7a7c5).
            PointerEvent::Hide => {
                self.pointer.visible = false;
                self.console.term.terminal.screen_mut().clear_selection();
            }
        }
        // Only motion may be coalesced while the child does not read; a
        // press, a release, a wheel report or a paste is queued whole.
        let bytes = self.console.term.terminal.take_replies();
        if let Some(pty) = self.console.pty.as_mut().filter(|_| !bytes.is_empty()) {
            if moved {
                pty.write_motion(&bytes);
            } else {
                pty.write(&bytes);
            }
        }
        self.draw_now = true;
        zoom
    }

    /// A change of the mouse tracking the program asked for clears the
    /// selection, as the C reference's did (`src/terminal.c:890-897` at
    /// kmscon b1f7a7c5).
    pub(crate) fn tracking_changed(&mut self) {
        let tracking = self.console.term.terminal.mouse_tracking();
        if tracking != self.tracking {
            self.tracking = tracking;
            self.pointer.drag = None;
            self.console.term.terminal.screen_mut().clear_selection();
        }
    }

    /// The cell under the pointer, or the grid's nearest when the pointer
    /// is in the strip a display leaves past the grid, as the C reference
    /// kept it (`src/terminal.c:173-190` at kmscon b1f7a7c5).
    fn pointer_cell(&self) -> (usize, usize) {
        let cell = |p: u32, size: u32, cells: usize| {
            usize::try_from(p.checked_div(size).unwrap_or(0))
                .unwrap_or(0)
                .min(cells.saturating_sub(1))
        };
        (
            cell(self.pointer.x, self.cell.0, self.grid.0),
            cell(self.pointer.y, self.cell.1, self.grid.1),
        )
    }

    /// Reports a pointer event at the pointer to the program.
    fn report_pointer(&mut self, button: MouseButton, event: MouseEvent, mods: Mods) {
        let (col, row) = self.pointer_cell();
        let (x, y) = (self.pointer.x, self.pointer.y);
        self.console
            .term
            .terminal
            .mouse(col, row, x, y, button, event, mods);
    }

    /// A button that selects or pastes, as the C reference's did
    /// (`src/terminal.c:1509-1542` at kmscon b1f7a7c5): the left button
    /// starts a selection that its drag extends and its release copies, a
    /// double click selects and copies a word, and a triple click a line,
    /// as xterm's does; the middle button pastes at its release, as
    /// xterm's does, and the right clears the selection. A release that
    /// ends no drag clears the selection and leaves the buffer as it was,
    /// as xterm takes no empty selection.
    fn select_button(
        &mut self,
        button: Button,
        pressed: bool,
        double_click: bool,
        selection: &mut Option<Vec<u8>>,
    ) {
        let (col, row) = self.pointer_cell();
        match (button, pressed) {
            (Button::Left, true) => {
                self.pointer.clicks = match self.pointer.clicks {
                    1 if double_click => 2,
                    2 if double_click => 3,
                    _ => 1,
                };
                self.pointer.drag = None;
                let screen = self.console.term.terminal.screen_mut();
                match self.pointer.clicks {
                    1 => {
                        screen.select_start(col, row);
                        self.pointer.drag = Some(Drag {
                            from: (col, row),
                            moved: false,
                        });
                    }
                    2 => {
                        screen.select_word(col, row);
                        self.copy(selection);
                    }
                    _ => {
                        screen.select_line(col, row);
                        self.copy(selection);
                    }
                }
            }
            (Button::Left, false) => match self.pointer.drag.take() {
                Some(Drag { moved: true, .. }) => self.copy(selection),
                Some(Drag { moved: false, .. }) => {
                    self.console.term.terminal.screen_mut().clear_selection();
                }
                None => {}
            },
            (Button::Middle, true) => {}
            (Button::Middle, false) => {
                self.paste(selection.as_deref());
                self.console.term.terminal.screen_mut().clear_selection();
            }
            (Button::Right, _) => {
                self.pointer.drag = None;
                self.console.term.terminal.screen_mut().clear_selection();
            }
        }
    }

    /// Copies the selected text into the selection buffer; with nothing
    /// selected the buffer keeps what it holds.
    fn copy(&self, selection: &mut Option<Vec<u8>>) {
        if let Some(text) = self
            .console
            .term
            .terminal
            .screen()
            .selection_text()
            .filter(|t| !t.is_empty())
        {
            *selection = Some(text.into_bytes());
        }
    }

    /// Pastes the selection buffer to the program, filtered, and
    /// bracketed while it asks (`Terminal::paste`).
    pub(crate) fn paste(&mut self, selection: Option<&[u8]>) {
        if let Some(text) = selection {
            self.console.term.terminal.paste(text);
        }
    }
}

/// One frit in use, shared by every head that draws it (ADR-1401 D1):
/// one glyph helper for each frit, started on the first need (ADR-1301
/// D2), the store its glyphs go into, the cells it was asked for, and its
/// fonts' own cell once asked.
pub(crate) struct Frit {
    pub(crate) frit: crate::helper::FritText,
    pub(crate) generated: crate::deferred::Shared,
    pub(crate) helper: Option<crate::helper::Helper>,
    /// The helper cannot start: logged once, and the built-in font draws.
    pub(crate) refused: bool,
    /// The cells whose glyphs the helper was asked for, over every head
    /// drawing the frit.
    pub(crate) asked: Vec<vtium_font::Cell>,
    /// The fonts' own cell, or why it is not known, once asked.
    pub(crate) own: Option<Result<vtium_font::Cell, String>>,
}

impl Head {
    /// The name of the frit the head draws, a frit's as it was given.
    pub(crate) fn frit(&self) -> Option<&str> {
        match &self.font {
            Some(Chosen::Frit { name, .. }) => Some(name),
            _ => None,
        }
    }
}

/// The head file the start runs (ADR-1401 D3, D7): the file at `path`
/// read under `trust`, each warning logged with its line; the compiled-in
/// configuration when there is none; and the compiled-in configuration in
/// place of a file that fails a check, each error logged with its line and
/// shown on the console, the notice returned beside the rules. The file is
/// never written.
pub fn rules(
    path: &std::path::Path,
    trust: &vtium_config::Trust,
) -> (vtium_config::Config, Option<String>) {
    use vtium_config::at;
    let parsed = vtium_config::read(path, trust)
        .and_then(|text| text.map(|text| vtium_config::parse(&text)).transpose());
    match parsed {
        Ok(None) => (vtium_config::Config::builtin(), None),
        Ok(Some(parsed)) => {
            for w in &parsed.warnings {
                log::warn!("{}", at(path, Some(w.line()), w));
            }
            (parsed.config, None)
        }
        Err(rejected) => {
            let mut lines: Vec<String> = rejected
                .errors
                .iter()
                .map(|e| at(path, e.line(), e).to_string())
                .collect();
            if rejected.more > 0 {
                let more = crate::notice::more_errors(rejected.more);
                lines.push(at(path, None, more).to_string());
            }
            lines.push(at(path, None, crate::notice::head_file_ignored()).to_string());
            for line in &lines {
                log::error!("{line}");
            }
            (vtium_config::Config::builtin(), Some(lines.join("\n")))
        }
    }
}

/// The head file the start runs, as [`rules`] reads it, unless `cmdline`,
/// the kernel command line, carries `vtium.heads=builtin` (ADR-1401 D9):
/// then the file is not read for this boot and the compiled-in
/// configuration runs, every guard on.
pub fn start_rules(
    path: &std::path::Path,
    trust: &vtium_config::Trust,
    cmdline: &str,
) -> (vtium_config::Config, Option<String>) {
    match vtium_config::heads_word(cmdline) {
        vtium_config::HeadsWord::Builtin => {
            log::warn!(
                "vtium.heads=builtin: the head file is not read for this boot; the compiled-in rules run, every guard on"
            );
            return (vtium_config::Config::builtin(), None);
        }
        vtium_config::HeadsWord::Unknown => {
            log::warn!("vtium.heads= takes builtin alone: the head file is read");
        }
        vtium_config::HeadsWord::File => {}
    }
    rules(path, trust)
}

/// The start's log line for each guard the head file turns off
/// (ADR-1401 D9).
pub(crate) fn guards_off(guards: vtium_config::Guards) -> Vec<String> {
    [
        (guards.anti_lockout, "anti-lockout"),
        (guards.rescue_key, "rescue-key"),
        (guards.never_dark, "never-dark"),
    ]
    .into_iter()
    .filter(|&(on, _)| !on)
    .map(|(_, name)| format!("the head file turns the {name} guard off"))
    .collect()
}

/// The head a display joins: the one the first rule matching `display`
/// names (ADR-1401 D3), by its index. A display a rule sends `dark`, or
/// that no rule claims, joins none: the error is its log line, which for
/// a display no rule claims carries the rule that would give it the first
/// head, ready to paste.
pub fn place_display(
    rules: &vtium_config::Config,
    display: &vtium_config::Identity,
) -> Result<usize, String> {
    let placed = rules.walk(display);
    let name = display.connector.as_deref().unwrap_or("?").escape_debug();
    match (placed.place, placed.by) {
        (Place::Head(h), _) => Ok(h),
        (_, By::Rule(line)) => Err(format!(
            "display {name} is left dark by the head file's rule on line {line}"
        )),
        _ => {
            let target = rules.heads.first().map_or("console", |h| h.name.as_str());
            let rule = vtium_config::rule_for(display, target).map_or_else(
                || "it offers no value a rule can name".to_owned(),
                |rule| format!("the rule `{rule}` gives it a head"),
            );
            Err(format!(
                "display {name} matches no rule of the head file and is left dark; {rule}"
            ))
        }
    }
}

/// What a wheel event does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wheel {
    /// A report of the wheel's button to the program tracking the pointer.
    Report(MouseButton),
    Zoom(Zoom),
    /// [`WHEEL_LINES`] of scrollback.
    Scroll {
        up: bool,
    },
}

/// What a wheel event of `steps` does: one report while the program
/// tracks the pointer, else a zoom with Ctrl, else a scroll, whatever
/// the number of steps (`src/terminal.c:1492-1497, 1639-1650` at kmscon
/// b1f7a7c5).
fn wheel(steps: i32, tracking: bool, ctrl: bool) -> Wheel {
    let up = steps > 0;
    if tracking {
        Wheel::Report(if up {
            MouseButton::WheelUp
        } else {
            MouseButton::WheelDown
        })
    } else if ctrl {
        Wheel::Zoom(if up { Zoom::In } else { Zoom::Out })
    } else {
        Wheel::Scroll { up }
    }
}

fn mouse_button(b: Button) -> MouseButton {
    match b {
        Button::Left => MouseButton::Left,
        Button::Middle => MouseButton::Middle,
        Button::Right => MouseButton::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A head file in a fixture directory of its own, and the trust that
    /// reads it as the start reads root's files.
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn fixture(name: &str, text: Option<&str>) -> (PathBuf, vtium_config::Trust) {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("vtium-head-rules-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = dir.join("heads.conf");
        if let Some(text) = text {
            std::fs::write(&file, text).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let trust = vtium_config::Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
        (file, trust)
    }

    // ADR-1401 D7: the start runs a valid file, the compiled-in
    // configuration without one, and the compiled-in configuration in place
    // of an invalid one, whose bytes it leaves as they were.
    #[test]
    fn the_start_runs_the_head_file_else_the_shipped_rules_and_writes_none() {
        let valid = "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\n";
        let invalid = "version = 1\n[head desk]\n[rules]\ndisplay * -> desc\n";
        let (file, trust) = fixture("valid", Some(valid));
        let (ran, notice) = rules(&file, &trust);
        assert_eq!(notice, None);
        assert_eq!(
            ran.heads
                .iter()
                .map(|h| h.name.as_str())
                .collect::<Vec<_>>(),
            ["desk"]
        );
        let (absent, trust) = fixture("absent", None);
        assert_eq!(
            rules(&absent, &trust),
            (vtium_config::Config::builtin(), None)
        );
        assert!(!absent.exists(), "nothing written");
        let (bad, trust) = fixture("invalid", Some(invalid));
        let (ran, notice) = rules(&bad, &trust);
        assert_eq!(ran, vtium_config::Config::builtin());
        assert!(
            notice.as_deref().is_some_and(|n| n.contains(":4: ")
                && n.lines().last()
                    == Some(&*format!(
                        "{}: This file has errors, so vtium ignored it and uses its built-in rules. Fix the errors above and check the file with `vtium check-config`, then restart vtium or reboot to use it.",
                        bad.display()
                    )))
        );
        assert_eq!(std::fs::read_to_string(&bad).ok().as_deref(), Some(invalid));
        for f in [file, absent, bad] {
            if let Some(dir) = f.parent() {
                std::fs::remove_dir_all(dir).ok();
            }
        }
    }

    // A head shows each notice once and the newest NOTICES_MAX; an ended
    // head's notice stays when they end.
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_head_keeps_each_notice_once_and_the_newest_few() {
        let config = crate::config::load(&[], None, &mut |_: &std::path::Path| Ok(None)).config;
        let term = terminal(&config, (0, 0), 0, std::rc::Rc::new(Instant::now));
        let mut head = Head::new("h", &config, &vtium_config::Font::default(), term);
        for n in 0..6 {
            head.add_notice(&format!("n{n}"));
            head.add_notice(&format!("n{n}"));
        }
        assert_eq!(head.notice(), Some("n2\nn3\nn4\nn5"));
        head.ended = true;
        head.notice_due = Some(Instant::now());
        head.end_notices();
        assert_eq!(
            (head.notice(), head.notice_due),
            (Some(crate::notice::session_ended().as_str()), None)
        );
        assert!(!head.has_notices());
    }

    // ADR-1401 D9 and D11's switch row: each guard the file turns off is a
    // line in the start's log.
    #[test]
    fn each_guard_turned_off_is_a_line_in_the_starts_log() {
        let off = |text: &str| {
            let parsed = vtium_config::parse(&format!("version = 1\n[guards]\n{text}"));
            guards_off(parsed.map(|p| p.config.guards).unwrap_or_default())
        };
        assert_eq!(off(""), Vec::<String>::new());
        assert_eq!(
            off("never-dark = off\nanti-lockout = off\n"),
            [
                "the head file turns the anti-lockout guard off",
                "the head file turns the never-dark guard off"
            ]
        );
        assert_eq!(
            off("rescue-key = off\n"),
            ["the head file turns the rescue-key guard off"]
        );
    }

    #[test]
    fn a_display_left_dark_is_logged_and_one_no_rule_claims_with_the_rule_to_paste() {
        let rules = vtium_config::parse(
            "version = 1\n[head desk]\n[rules]\ndisplay connector:pci-0000:03:00.0/DP-1 -> dark\ndisplay connector:pci-0000:03:00.0/DP-2 -> desk\n",
        )
        .map(|p| p.config)
        .unwrap_or_default();
        let display = |connector: &str| vtium_config::Identity {
            connector: Some(connector.to_owned()),
            card: Some("pci-0000:03:00.0".to_owned()),
            ..vtium_config::Identity::display()
        };
        assert_eq!(place_display(&rules, &display("DP-2")), Ok(0));
        assert_eq!(
            place_display(&rules, &display("DP-1")),
            Err("display DP-1 is left dark by the head file's rule on line 4".to_owned())
        );
        assert_eq!(
            place_display(&rules, &display("HDMI-A-1")),
            Err("display HDMI-A-1 matches no rule of the head file and is left dark; the rule `display connector:pci-0000:03:00.0/HDMI-A-1 -> desk` gives it a head".to_owned())
        );
        let bare = vtium_config::Identity {
            connector: Some("fb\u{1b}".to_owned()),
            ..vtium_config::Identity::display()
        };
        assert_eq!(
            place_display(&rules, &bare),
            Err("display fb\\u{1b} matches no rule of the head file and is left dark; it offers no value a rule can name".to_owned())
        );
    }

    // ADR-1401 D10: every console's scrollback together stays within the
    // most `sb-memory` may be on the machine, and every frit's store within
    // the machine's store limit.
    #[test]
    fn the_memory_of_every_console_and_frit_together_is_bounded() {
        const MIB: u64 = 1 << 20;
        // 512 MiB: `sb-memory` may be 128 MiB. The default, 5,963,776
        // bytes, fits 8 times; 50 consoles take 2,684,354 bytes each.
        assert_eq!(console_scrollback(5_963_776, 128 * MIB, 8, 1), 5_963_776);
        assert_eq!(console_scrollback(5_963_776, 128 * MIB, 50, 1), 2_684_354);
        assert_eq!(console_scrollback(64 << 20, 128 * MIB, 8, 1), 16 << 20);
        assert_eq!(console_scrollback(5_963_776, 128 * MIB, 0, 0), 5_963_776);
        // 9 heads past `consoles-max`'s 8 each keep a console: 9 shares.
        assert_eq!(console_scrollback(64 << 20, 128 * MIB, 8, 9), 14_913_080);
        assert_eq!(frit_store(16 * MIB, 1), 16 * MIB);
        assert_eq!(frit_store(16 * MIB, 16), MIB);
        assert_eq!(frit_store(16 * MIB, 0), 16 * MIB);
    }

    #[test]
    fn a_wheel_event_is_one_report_a_zoom_or_a_scroll_whatever_its_steps() {
        for steps in [1, 7, i32::MAX] {
            assert_eq!(
                wheel(steps, true, true),
                Wheel::Report(MouseButton::WheelUp)
            );
            assert_eq!(wheel(steps, false, true), Wheel::Zoom(Zoom::In));
            assert_eq!(wheel(steps, false, false), Wheel::Scroll { up: true });
        }
        for steps in [-1, -7, i32::MIN] {
            assert_eq!(
                wheel(steps, true, false),
                Wheel::Report(MouseButton::WheelDown)
            );
            assert_eq!(wheel(steps, false, true), Wheel::Zoom(Zoom::Out));
            assert_eq!(wheel(steps, false, false), Wheel::Scroll { up: false });
        }
    }
}
