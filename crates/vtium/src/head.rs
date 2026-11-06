//! Heads (ADR-1401 D1): a terminal for one or more displays, each holding
//! its own session, its pty, its screen and scrollback, and its own grid,
//! laid out over its own displays by the scaled rule ([`crate::layout`])
//! at its own density, zoom and rotation.
//!
//! [`identity`] maps vtium-platform's identities of a display and of an
//! input into the head file's, which the rules walk. The app holds the
//! heads, places each display in one, and drives each head's session as
//! it drove the seat's one session before heads: each head is one
//! terminal with its own grid.

pub(crate) mod chord;
pub mod console;
pub mod images;
pub(crate) mod route;
pub(crate) mod session;

use vtium_settings::{By, Place, WhenDisplayLost};

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

/// The base font's cell before any font is chosen.
const CELL: (u32, u32) = (8, 16);

/// How long a notice shows with no key reaching its head (ADR-1401 D7,
/// probe 5).
pub const NOTICE_SHOWN: Duration = Duration::from_secs(30);

/// The most notices a head shows at once, the newest: an invalid head
/// file's, no keyboard found, the anti-lockout guard's and a rescue.
pub const NOTICES_MAX: usize = 4;

/// The most errors the panic screen lists, the file's first (ADR-1401
/// D7); it counts the rest. `vtium check-config` lists them all.
pub const PANIC_ERRORS_MAX: usize = 3;

/// The lines of scrollback a wheel event scrolls.
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
    /// The right button's press, with Ctrl, selected a command's output,
    /// which its release keeps (ADR-1002, "What the marks are for").
    output: bool,
    /// Ctrl was held at the last pointer event or key, which the hover a
    /// relayout finds again keeps.
    ctrl: bool,
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
    /// The configuration its consoles' command is read from, where the
    /// head's section sets its own `login`: the top level's with the
    /// head's sections over it.
    pub(crate) login: Option<Config>,
    /// What the head shows while its shown console's session has ended,
    /// from the catalog: the roaming chord's keys while no other console of
    /// the head runs, else the console chord's.
    ended_notice: String,
    ended_notice_console: String,
    /// vtium's notices to the head (ADR-1401 D7), each once and at most
    /// [`NOTICES_MAX`], the oldest going first, until a key reaches the
    /// head or [`NOTICE_SHOWN`] after they are first drawn, when they end.
    notices: Vec<String>,
    /// What the head shows of its own as one text, a line each: that the
    /// shown console's session ended, above the notices.
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
    /// The on-screen display's line, while it shows under `osd = full`,
    /// or its brief line while that shows.
    pub(crate) status: Option<String>,
    /// The on-screen display's brief line, which answers one press of the
    /// open chord at a cap, until `brief_due` (ADR-1401 D5). Only
    /// [`Head::show_brief`] sets it, which sets none under `osd = off`.
    brief: Option<String>,
    /// When the brief line goes, armed at its first frame.
    brief_due: Option<Instant>,
    /// The colors of the notices and the on-screen display, which
    /// vtium.conf sets for every head (ADR-1401 D5).
    overlay_colors: crate::term::OverlayColors,
    /// What the head does when it loses its last display (ADR-1401 D8).
    pub(crate) when_display_lost: WhenDisplayLost,
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
    /// The frit the head's look names with `font-name` (ADR-1403 D5),
    /// `None` for the top level's own choice.
    pub(crate) font_name: Option<String>,
    /// The keys the head's sections set over the top level, each once at
    /// the most specific level that sets it (ADR-1403 D5), which
    /// [`Config::for_head`] lays over the configuration wherever the
    /// console reads a key a section may set.
    pub(crate) settings: Vec<vtium_settings::Setting>,
    /// The base font's height; each display may draw larger
    /// ([`crate::layout::layout`]).
    pub(crate) font_size: u32,
    /// `font-size` in points: the base height follows the displays'
    /// density, until a zoom.
    pub(crate) points: Option<u32>,
    /// `font-size=0`: the base height follows the smallest display, until
    /// a zoom.
    pub(crate) auto_size: bool,
    /// A `font-size` other than 0 is configured, and the head's look
    /// states no cell.
    sized: bool,
    /// The cell the head's look states, `cell`, which the frit the head
    /// draws takes over its own `cell=` (ADR-1403 D5).
    look_cell: Option<vtium_font::Cell>,
    /// `font-line-spacing`, vtium.conf's or the head's look's, which
    /// overrides the frit's `line-spacing=` (ADR-1302 D13); `None` when
    /// neither sets it.
    pub(crate) line_spacing: Option<u32>,
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
    /// The console the last OSC 50 switch came from, while the head draws
    /// the font it chose: the end of that console's session, or of the
    /// session in the head's only console, returns the head to its
    /// configured font (ADR-1201 D11).
    pub(crate) switched: Option<u64>,
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
    /// The screen the display guard lights for a notice alone under the
    /// closed policy ([`Head::notice_screen`]).
    pub(crate) notice_only: bool,
    /// The report the notice screen shows, the panic screen's.
    report: Option<vtium_render::NoticeReport>,
    /// What the image keys give the head's consoles (ADR-1100 D11).
    #[cfg(feature = "images")]
    pub(crate) image_settings: images::Settings,
}

/// A console's terminal for `config`, on [`GRID`] with the scrollback cap
/// and length `scrollback`. `bytes` is the console's scrollback memory
/// when the configuration sets `sb-memory`; left to its default, the
/// budget follows the console's grid, as vtium-term works it out at each
/// resize, and `bytes` is the most it may be.
pub(crate) fn terminal(
    config: &Config,
    scrollback: (usize, usize),
    bytes: usize,
    clock: Clock,
) -> Term {
    let (cap, lines) = scrollback;
    let mut terminal = Terminal::new(GRID.0, GRID.1, cap);
    terminal.screen_mut().set_scrollback_max(lines);
    if crate::config::budget::sb_memory_set(config) {
        terminal.screen_mut().set_scrollback_memory(bytes);
    } else {
        terminal.screen_mut().set_scrollback_memory_from_grid(bytes);
    }
    terminal.set_backspace_sends_delete(config.switch(&keys::BACKSPACE_DELETE));
    terminal.set_palette(config.palette());
    terminal.set_font_ops(config.switch(&keys::ALLOW_FONT_OPS));
    if let Some(Ok(table)) = crate::termcap::for_term(config.text(&keys::TERM).unwrap_or_default())
    {
        terminal.set_termcap(table);
    }
    let timeout = Duration::from_millis(config.number(&keys::SYNC_TIMEOUT).unwrap_or(200));
    terminal.set_sync_enabled(!timeout.is_zero());
    let sync = Sync::new(timeout, terminal.sync_frames());
    // Every build reads the image keys; a build with images gives the
    // terminal its handlers and their limits.
    let pictures = images::Settings::read(config);
    #[cfg(feature = "images")]
    images::install(&mut terminal, &pictures);
    #[cfg(not(feature = "images"))]
    let _ = pictures;
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

/// A pty's size for `grid` in cells of `cell` pixels.
fn pty_size(grid: (usize, usize), cell: (u32, u32)) -> Size {
    Size {
        cols: u16::try_from(grid.0).unwrap_or(u16::MAX),
        rows: u16::try_from(grid.1).unwrap_or(u16::MAX),
        cell_width: cell.0,
        cell_height: cell.1,
    }
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

/// The size a console of a head draws at, from the `font-size` of the
/// head's configuration `config`, the one [`Config::for_head`] gives, so
/// its look's size wins over the top level's (ADR-1403 D5): in points,
/// unless `font-size=0` or unset picks it from the displays; and whether
/// a `font-size` other than 0 is configured. A cell the head's look
/// states wins over the top level's `font-size`, since the look is the
/// more specific level (ADR-1200 D16): the head then draws at that cell's
/// height, as at `font-size=0` with a frit that states a cell, and counts
/// as sized by none. [`Config::for_head`] has already dropped a look's
/// cell where the same look sets a `font-size`.
pub(crate) fn configured_size(config: &Config) -> (Option<u32>, bool, bool) {
    if look_cell(config).is_some() {
        return (None, true, false);
    }
    let size = config.number(&keys::FONT_SIZE);
    let points = size
        .filter(|&n| n != 0)
        .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
    let auto_size = size.unwrap_or(0) == 0;
    (points, auto_size, !auto_size)
}

/// The cell the head's look states, from the head's configuration
/// `config`, the one [`Config::for_head`] gives.
pub(crate) fn look_cell(config: &Config) -> Option<vtium_font::Cell> {
    config
        .size(&keys::CELL)
        .map(|(width, height)| vtium_font::Cell { width, height })
}

/// What [`Head`]'s `draw_hook` runs.
pub type DrawHook = std::sync::Arc<dyn Fn() + Send + std::marker::Sync>;

impl Head {
    /// The head `name` for its configuration `config`, the one
    /// [`Config::for_head`] gives for the keys `settings` its sections set,
    /// its one console's terminal `term`: it draws at `config`'s
    /// `font-size`, its look's where its look sets one (ADR-1401 D3,
    /// ADR-1403 D5), and its session starts at its first display.
    pub(crate) fn new(
        name: &str,
        config: &Config,
        settings: Vec<vtium_settings::Setting>,
        term: Term,
    ) -> Head {
        let own = |key: &keys::Key| {
            settings
                .iter()
                .find(|s| s.key.name == key.name)
                .map(|s| s.value.clone())
        };
        let font_name = own(&keys::FONT_NAME);
        let (points, auto_size, sized) = configured_size(config);
        let fit = Fit {
            grid: GRID,
            font_size: 0,
            cell: CELL,
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
            ended_notice: crate::notice::session_ended(),
            ended_notice_console: crate::notice::session_ended_console(),
            notices: Vec::new(),
            shown: String::new(),
            notice_due: None,
            osd_shown: false,
            osd_due: None,
            osd_seen: None,
            mark: None,
            status: None,
            brief: None,
            brief_due: None,
            overlay_colors: crate::term::OverlayColors::configured(config),
            when_display_lost: WhenDisplayLost::Keep,
            hangup_due: None,
            views: BTreeMap::new(),
            attached: Attached::NotYet,
            draw_now: false,
            text_due: None,
            cursor_due: None,
            pointer: Pointer::default(),
            focused: false,
            tracking: MouseTracking::Off,
            font_name,
            settings,
            font_size: 0,
            points,
            auto_size,
            sized,
            look_cell: look_cell(config),
            line_spacing: config.font_line_spacing().map(u32::from),
            has_font: false,
            step: 16,
            cell: CELL,
            font: None,
            refont: false,
            font_refused: 0,
            switched: None,
            orientation: Orientation::Normal,
            grid: GRID,
            draw_hook: None,
            painter: None,
            painter_refused: false,
            figures: Figures::default(),
            notice_only: false,
            report: None,
            #[cfg(feature = "images")]
            image_settings: images::Settings::read(config),
        }
    }

    /// The screen the display guard lights on the boot display for a
    /// notice alone, under the closed policy (ADR-1401 D9): a head the file
    /// does not declare, with no name and no session, which no input
    /// reaches. Its notice stays for as long as it is lit (D7), on a
    /// screen with no cursor.
    pub(crate) fn notice_screen(config: &Config, term: Term) -> Head {
        let mut head = Head::new("", config, Vec::new(), term);
        head.notice_only = true;
        head.console.term.terminal.input(b"\x1b[?25l");
        head
    }

    /// Shows `text` alone as the notice screen's notice.
    pub(crate) fn set_notice(&mut self, text: &str) {
        self.report = None;
        self.notices.clear();
        self.notices.push(text.to_owned());
        self.compose_notices();
    }

    /// Shows `report` alone as the notice screen's notice, the panic
    /// screen's, which keeps its count and its next step on a display too
    /// small for all of it.
    pub(crate) fn set_report(&mut self, report: vtium_render::NoticeReport) {
        self.set_notice(&report.text());
        self.report = Some(report);
    }

    /// The head's name, as its `[head NAME]` gives it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Drops each of the head's displays that `gone` names, and tells the
    /// render thread, whose GPU path frees what it kept for each one.
    /// Gives whether any went.
    pub(crate) fn drop_views(&mut self, gone: impl Fn(&(PathBuf, DisplayId)) -> bool) -> bool {
        let keys: Vec<_> = self.views.keys().filter(|k| gone(k)).cloned().collect();
        for key in &keys {
            self.views.remove(key);
        }
        if let Some(painter) = &mut self.painter {
            for key in &keys {
                painter.forget(key.clone());
            }
        }
        !keys.is_empty()
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

    /// Whether every console's program ended under `oneshot`, so the head
    /// runs no session any more (ADR-1401 D1).
    pub(crate) fn ended(&self) -> bool {
        self.consoles().all(|c| c.ended)
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

    /// What vtium shows on the head of its own: that the shown console's
    /// session ended for good, above its notices, which always show too
    /// (ADR-1401 D5).
    pub fn notice(&self) -> Option<&str> {
        (!self.shown.is_empty()).then_some(self.shown.as_str())
    }

    /// Writes what the head shows of its own again, after its notices,
    /// its shown console or a console's session changed.
    pub(crate) fn compose_notices(&mut self) {
        let ended = self.console.ended.then(|| {
            if self.hidden.iter().any(|c| !c.ended) {
                &self.ended_notice_console
            } else {
                &self.ended_notice
            }
        });
        let text = ended
            .into_iter()
            .chain(&self.notices)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        if text != self.shown {
            self.shown = text;
            self.draw_now = true;
        }
    }

    /// The on-screen display's dot, while it shows.
    pub fn mark(&self) -> Option<vtium_render::Mark> {
        self.mark
    }

    /// The on-screen display's line, while it shows.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// The on-screen display shows `line` as its brief line from the
    /// head's next frame, which answers one press (ADR-1401 D5). Under
    /// `osd = off`, which `osd` gives, vtium shows no brief line, so the
    /// press leaves the screen as it was and draws no frame.
    pub(crate) fn show_brief(&mut self, line: String, osd: crate::osd::Osd) {
        if osd.mode == crate::osd::Mode::Off {
            return;
        }
        self.brief = Some(line);
        self.brief_due = None;
        self.draw_now = true;
    }

    /// The brief line, while it shows.
    pub(crate) fn brief(&self) -> Option<&str> {
        self.brief.as_deref()
    }

    /// When the brief line goes.
    pub(crate) fn brief_due(&self) -> Option<Instant> {
        self.brief_due
    }

    /// The brief line's time starts at its first frame, to end at `due`:
    /// once, while the head has a display to draw it on.
    pub(crate) fn arm_brief(&mut self, due: Instant) {
        if self.brief.is_some() && self.brief_due.is_none() && !self.views.is_empty() {
            self.brief_due = Some(due);
        }
    }

    /// The brief line goes once its time has passed at `now`.
    pub(crate) fn brief_turn(&mut self, now: Instant) {
        if self.brief_due.is_some_and(|d| d <= now) {
            self.brief_due = None;
            self.brief = None;
        }
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
        self.notice_due = None;
        self.compose_notices();
    }

    /// Whether the head has notices that end.
    pub(crate) fn has_notices(&self) -> bool {
        !self.notices.is_empty()
    }

    /// The head's notices end: a key reached it, or their time ran out.
    pub(crate) fn end_notices(&mut self) {
        self.notice_due = None;
        self.notices.clear();
        self.compose_notices();
    }

    /// What vtium draws over a display of the head: one box at the top
    /// holding the on-screen display's line, the display's render timer
    /// line `timer` under it, and the notices beneath them, a problem
    /// while a notice is in it, and the on-screen display's dot, in the
    /// colors vtium.conf sets. The notice screen draws its notice alone,
    /// its report as a report.
    pub(crate) fn overlay(&self, timer: Option<&str>) -> crate::term::Overlay {
        if let Some(text) = self.notice().filter(|_| self.notice_only) {
            let notice = match &self.report {
                Some(report) => vtium_render::Notice::report(report.clone()),
                None => vtium_render::Notice::new(text),
            };
            return crate::term::Overlay::alone(notice, self.overlay_colors);
        }
        let notices: Vec<&str> = self.notice().into_iter().collect();
        let lines: Vec<&str> = self.status.as_deref().into_iter().chain(timer).collect();
        let status = (!lines.is_empty()).then(|| lines.join("\n"));
        crate::term::Overlay::new(status.as_deref(), &notices, self.mark, self.overlay_colors)
    }

    /// How the shown console fits the displays.
    fn fit(&self) -> Fit {
        Fit {
            grid: self.grid,
            font_size: self.font_size,
            cell: self.cell,
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
    /// the head; an old console that is `closing` is told nothing, since
    /// its pty is closing and no program reads a focus-out. Every display
    /// draws the new one whole, since what each display last drew belongs
    /// to the old console.
    fn swap_shown(&mut self, new: Console, visible: bool, closing: bool) -> Console {
        let fit = self.fit();
        self.grid = new.fit.grid;
        self.font_size = new.fit.font_size;
        self.cell = new.fit.cell;
        self.points = new.fit.points;
        self.auto_size = new.fit.auto_size;
        let mut old = std::mem::replace(&mut self.console, new);
        old.fit = fit;
        self.console.term.terminal.set_visible(visible);
        if self.focused {
            self.console.term.terminal.focus(true);
        }
        if !closing {
            old.term.terminal.set_visible(false);
            if self.focused {
                old.term.terminal.focus(false);
            }
            old.send_replies();
        }
        self.send_user_bytes();
        self.pointer.drag = None;
        // The pointer is over the shown console alone: the hidden one
        // hovers nothing, and the caller sets the shown one's hover once
        // its grid is laid out ([`Head::hover`]).
        old.term.terminal.screen_mut().set_pointer(None, false);
        for v in self.views.values_mut() {
            v.show_another();
        }
        // A renderer draws the pictures of one console's store, whose keys
        // another console's store reuses, so each display draws the new
        // console with a new renderer, which the relayout that follows
        // every switch makes (vtium-render's README, "One image store for
        // each renderer").
        #[cfg(feature = "images")]
        {
            self.refont = true;
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
        let old = self.swap_shown(new, visible, false);
        let back = console::hidden_index(self.at, to).min(self.hidden.len());
        self.hidden.insert(back, old);
        self.at = to;
        self.compose_notices();
    }

    /// The text of the shown console's screen rows, for tests and logs.
    pub fn screen_text(&self) -> Vec<String> {
        screen_text(&self.console)
    }

    /// The URI of the link each console's screen hovers, in the ring's
    /// order, for tests.
    pub fn consoles_hovered(&self) -> Vec<Option<String>> {
        let hovered = |c: &Console| {
            let screen = c.term.terminal.screen();
            screen
                .hovered_link()
                .and_then(|l| screen.link_uri(l))
                .map(str::to_owned)
        };
        let mut all: Vec<Option<String>> = self.hidden.iter().map(hovered).collect();
        all.insert(self.at.min(all.len()), hovered(&self.console));
        all
    }

    /// How many of the head's displays have a frame composing on its
    /// render thread, for tests: none, and the next frame a display shows
    /// is one composed after it.
    pub fn frames_out(&self) -> usize {
        self.views.values().filter(|v| v.away()).count()
    }

    /// The refresh interval that paces each of the head's displays'
    /// frames, in the order of their nodes and ids, for tests.
    pub fn refresh_intervals(&self) -> Vec<Option<Duration>> {
        self.views.values().map(|v| v.refresh).collect()
    }

    /// The URI the shown console's overlay row shows, for tests.
    pub fn link_overlay(&self) -> Option<String> {
        let terminal = &self.console.term.terminal;
        let frame = terminal
            .screen()
            .frame(terminal.palette(), vtium_term::Age::NEVER);
        frame.overlay().map(|o| o.uri.to_owned())
    }

    /// The text of each console's screen rows, in the ring's order, for
    /// tests.
    pub fn consoles_text(&self) -> Vec<Vec<String>> {
        let mut all: Vec<Vec<String>> = self.hidden.iter().map(screen_text).collect();
        all.insert(self.at.min(all.len()), self.screen_text());
        all
    }

    /// Whether the head's `font-size` is configured, vtium.conf's or its
    /// own, which wins over a frit's cell (ADR-1200 D16); `font-size=0`
    /// is the automatic size.
    pub(crate) fn sized(&self) -> bool {
        self.sized
    }

    /// The cell the head's look states, which the frit it draws takes.
    pub(crate) fn look_cell(&self) -> Option<vtium_font::Cell> {
        self.look_cell
    }

    /// The font set in use, the built-ins before one is chosen.
    pub(crate) fn chosen(&self) -> &Chosen {
        self.font.as_ref().unwrap_or(&Chosen::BUILTINS)
    }

    /// The font set's face at `height`, its cell taller by the head's
    /// line spacing where that applies (ADR-1302 D13).
    pub(crate) fn face(&self, height: u32, locale: &str) -> Result<DisplayFace, String> {
        let chosen = self.chosen();
        chosen
            .face_spaced(height, locale, self.line_spacing)
            .ok_or_else(|| {
                format!(
                    "font set {} has no characters at {height} pixels high",
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
            Chosen::Builtin { .. } => font.policy(),
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

    /// The shown console's pty size: the head's grid and cell.
    pub(crate) fn pty_size(&self) -> Size {
        pty_size(self.grid, self.cell)
    }

    /// The pty size of console `id`: the shown console's, or a hidden
    /// one's own grid and cell, which its zoom may have set apart from the
    /// shown console's.
    pub(crate) fn console_pty_size(&self, id: u64) -> Option<Size> {
        if self.console.id == id {
            return Some(self.pty_size());
        }
        self.hidden
            .iter()
            .find(|c| c.id == id)
            .map(|c| pty_size(c.fit.grid, c.fit.cell))
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
        // The cell pictures are placed in too, so a picture that arrives
        // before the next image turn takes the new cell.
        #[cfg(feature = "images")]
        self.console
            .pictures
            .give_cell(&mut self.console.term, (cw, ch));
        self.console.term.terminal.set_display_size(w, h);
        for v in self.views.values_mut() {
            v.lost = true;
        }
        self.draw_now = true;
        if grid != self.grid {
            self.grid = grid;
            let size = self.pty_size();
            if let Some(pty) = &mut self.console.pty
                && let Err(e) = pty.resize(size)
            {
                log::warn!("cannot resize the pty: {e}");
            }
        }
        // Last, so mode 2048 reports the resize once it is complete, a new
        // font on the same grid included; the report goes out at once.
        self.console.term.terminal.resize(grid.0, grid.1);
        self.send_user_bytes();
        // A new cell or grid puts another cell under the pointer: the
        // overlay and a Ctrl+click name the same link.
        self.hover(self.pointer.ctrl);
    }

    /// `grid` in the base cell's pixels.
    pub(crate) fn pixels(&self, grid: (usize, usize)) -> (u32, u32) {
        let px = |n: usize, c: u32| u32::try_from(n).unwrap_or(u32::MAX).saturating_mul(c);
        (px(grid.0, self.cell.0), px(grid.1, self.cell.1))
    }

    /// What the displays show, read from the view each buffer draws,
    /// scrolled back or not (#149), in each style a cell draws its glyph
    /// in, bold, italic, both or neither (ADR-1200 D21): the characters,
    /// every character of a cluster among them, and the clusters whole,
    /// each sorted and once. The regular style comes first, shown or not,
    /// and each other style only where a cell shows it.
    pub(crate) fn visible(&self) -> Vec<Shown> {
        use vtium_term::{Age, Attrs, Glyph};
        let palette = self.console.term.terminal.palette();
        let frame = self
            .console
            .term
            .terminal
            .screen()
            .frame(palette, Age::NEVER);
        let mut all = vec![Shown::default()];
        for row in 0..frame.rows() {
            for cell in frame.cells(row) {
                let style = vtium_font::Style {
                    bold: cell.attrs.contains(Attrs::BOLD),
                    italic: cell.attrs.contains(Attrs::ITALIC),
                };
                if !all.iter().any(|s| s.style == style) {
                    all.push(Shown {
                        style,
                        ..Shown::default()
                    });
                }
                let Some(shown) = all.iter_mut().find(|s| s.style == style) else {
                    continue;
                };
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
        for shown in &mut all {
            shown.chars.retain(|c| !c.is_whitespace());
            shown.chars.sort_unstable();
            shown.chars.dedup();
            shown.clusters.sort_unstable();
            shown.clusters.dedup();
        }
        all
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
    /// and scrolls through the seat's `selection` buffer. A program that
    /// asked with XTSHIFTESCAPE 1 that Shift reach it takes Shift as any
    /// modifier, so then Shift selects nothing. A wheel step vtium does
    /// not report goes to the program as cursor keys while mode 1007 asks
    /// on the alternate screen, and scrolls the view otherwise. A button keeps
    /// to the side its press took until its release, whatever Shift does
    /// meanwhile. Under `natural` scrolling the wheel scrolls and reports
    /// the other way. Returns the zoom a wheel with Ctrl asks for.
    pub(crate) fn pointer_event(
        &mut self,
        event: PointerEvent,
        mods: Mods,
        natural: bool,
        selection: &mut Option<Vec<u8>>,
    ) -> Option<Zoom> {
        let tracking = self.console.term.terminal.mouse_tracking() != MouseTracking::Off;
        let terminal = &self.console.term.terminal;
        let shift = mods.contains(Mods::SHIFT) && !terminal.mouse_captures_shift();
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
                    let ctrl = mods.contains(Mods::CTRL);
                    self.select_button(button, pressed, double_click, ctrl, selection);
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
                match wheel(steps, natural, tracking && !shift, ctrl) {
                    Wheel::Report(button) => self.report_pointer(button, MouseEvent::Press, mods),
                    Wheel::Zoom(dir) => zoom = Some(dir),
                    Wheel::Scroll { up } => {
                        let terminal = &mut self.console.term.terminal;
                        if !terminal.alternate_scroll(up, WHEEL_LINES) {
                            let screen = terminal.screen_mut();
                            if up {
                                screen.scroll_view_up(WHEEL_LINES);
                            } else {
                                screen.scroll_view_down(WHEEL_LINES);
                            }
                        }
                    }
                }
            }
            PointerEvent::Sync => {}
            // Hiding the pointer clears the selection.
            PointerEvent::Hide => {
                self.pointer.visible = false;
                self.console.term.terminal.screen_mut().clear_selection();
            }
        }
        self.hover(mods.contains(Mods::CTRL));
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
    /// selection.
    pub(crate) fn tracking_changed(&mut self) {
        let tracking = self.console.term.terminal.mouse_tracking();
        if tracking != self.tracking {
            self.tracking = tracking;
            self.pointer.drag = None;
            self.console.term.terminal.screen_mut().clear_selection();
        }
    }

    /// The cell under the pointer, or the grid's nearest when the pointer
    /// is in the strip a display leaves past the grid, so a click there
    /// still selects.
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

    /// Tells the shown console's screen which cell the pointer is over,
    /// and whether `ctrl` is held, which shows the hovered link's URI on an
    /// overlay row (ADR-1002, "What a link does"). A pointer that hides,
    /// or that is in the strip a display leaves past the grid, is over no
    /// cell, so it hovers no link.
    pub(crate) fn hover(&mut self, ctrl: bool) {
        self.pointer.ctrl = ctrl;
        let at = self.pointer_over();
        self.console
            .term
            .terminal
            .screen_mut()
            .set_pointer(at, ctrl);
    }

    /// [`Head::hover`] with `ctrl` as the head's keyboards hold it now,
    /// drawing the head again when that shows or hides the overlay.
    pub(crate) fn rehover(&mut self, ctrl: bool) {
        self.draw_now |= ctrl != self.pointer.ctrl;
        self.hover(ctrl);
    }

    /// Hides the pointer, as `pointer-hide-while-typing` does at a key:
    /// it hovers nothing from then on.
    pub(crate) fn hide_pointer(&mut self) {
        self.pointer.visible = false;
        self.hover(self.pointer.ctrl);
    }

    /// The cell under the pointer while it shows and is inside the grid.
    fn pointer_over(&self) -> Option<(usize, usize)> {
        if !self.pointer.visible {
            return None;
        }
        let col = usize::try_from(self.pointer.x.checked_div(self.cell.0)?).ok()?;
        let row = usize::try_from(self.pointer.y.checked_div(self.cell.1)?).ok()?;
        (col < self.grid.0 && row < self.grid.1).then_some((col, row))
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

    /// A button that selects or pastes: the left button
    /// starts a selection that its drag extends and its release copies, a
    /// double click selects and copies a word, and a triple click a line,
    /// as xterm's does; the middle button pastes at its release, as
    /// xterm's does, and the right clears the selection. A release that
    /// ends no drag clears the selection and leaves the buffer as it was,
    /// as xterm takes no empty selection.
    ///
    /// With `ctrl` held (ADR-1002, "What a link does" and "What the marks
    /// are for"), the left button on a link copies its URI into the
    /// buffer, replacing what it held, and clears the selection; nothing
    /// is opened and nothing is sent to the program. The right button on a
    /// line of a command's output, as OSC 133 marked it, selects that
    /// output as whole lines and copies it, and its release keeps the
    /// selection. Anywhere else the buttons do what they do without Ctrl.
    fn select_button(
        &mut self,
        button: Button,
        pressed: bool,
        double_click: bool,
        ctrl: bool,
        selection: &mut Option<Vec<u8>>,
    ) {
        let (col, row) = self.pointer_cell();
        match (button, pressed) {
            (Button::Left, true) if ctrl && self.copy_link(selection) => {
                self.pointer.clicks = 0;
                self.pointer.drag = None;
            }
            (Button::Right, true) if ctrl => {
                self.pointer.drag = None;
                let screen = self.console.term.terminal.screen_mut();
                self.pointer.output = screen.select_output(col, row);
                if self.pointer.output {
                    self.copy(selection);
                } else {
                    screen.clear_selection();
                }
            }
            (Button::Right, false) if std::mem::take(&mut self.pointer.output) => {}
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

    /// Copies the URI of the link under the pointer into the selection
    /// buffer and clears the selection. Returns false, changing nothing,
    /// when the pointer is over no link.
    fn copy_link(&mut self, selection: &mut Option<Vec<u8>>) -> bool {
        let over = self.pointer_over();
        let screen = self.console.term.terminal.screen_mut();
        let Some(uri) = over
            .and_then(|(x, y)| screen.link_at(x, y))
            .and_then(|link| screen.link_uri(link))
        else {
            return false;
        };
        *selection = Some(uri.as_bytes().to_vec());
        screen.clear_selection();
        true
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
    pub(crate) asked: Vec<vtium_font::spacing::SpacedCell>,
    /// The helper's answer to each cell query, or why there was none, by
    /// the height asked at.
    pub(crate) own: BTreeMap<u32, Result<crate::helper::OwnCell, crate::helper::NoCell>>,
}

impl Frit {
    /// The answer that holds at `height` for a frit whose text guesses it
    /// is `outline`, by what the console knows of it now. The answers, and
    /// not the guess, say which kind the frit is: one outline cell held at
    /// any height makes it an outline frit. A bitmap first source's own
    /// cell holds at every height. So does a failure to answer a bitmap
    /// frit's query, since a bitmap frit asks, and may wait, only at its
    /// first frame and at an OSC 50 switch (ADR-1200 D17): a refusal that
    /// came in time holds before a query that had no answer in time. An
    /// outline frit's answer holds at the height it was asked at alone.
    pub(crate) fn answer_at(
        &self,
        height: u32,
        outline: bool,
    ) -> Option<Result<crate::helper::OwnCell, crate::helper::NoCell>> {
        let outline = outline
            || self
                .own
                .values()
                .any(|a| matches!(a, Ok(crate::helper::OwnCell::Outline(_))));
        let failed = |late: bool| {
            self.own
                .values()
                .find(move |a| a.as_ref().is_err_and(|no| no.late == late))
        };
        let bitmap = self
            .own
            .values()
            .find(|a| matches!(a, Ok(crate::helper::OwnCell::Bitmap(_))));
        let any = (!outline)
            .then(|| failed(false).or_else(|| failed(true)))
            .flatten();
        bitmap.or(any).or_else(|| self.own.get(&height)).cloned()
    }
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

/// What the start runs for vtium.conf's sections, the flow (ADR-1401 D7,
/// ADR-1403 D3): the outcome their three levels resolve to, and the rules
/// that outcome runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub outcome: vtium_settings::Outcome,
    /// The file's own rules, what survives of them, the compiled-in
    /// configuration, or, for a run of nothing, no rule under the closed
    /// policy, so no input is delivered and the display guard lights the
    /// boot display for the notice alone.
    pub rules: vtium_settings::Config,
    /// The panic screen's report, for a broken closed file (ADR-1401 D7).
    pub(crate) report: Option<vtium_render::NoticeReport>,
}

impl Start {
    /// The start of a trusted vtium.conf at `path` whose bytes are
    /// `bytes`: each error, warning and note of its sections logged with
    /// its line. A file with no flow, one with no section and no policy
    /// line, runs the compiled-in flow, as a missing file does.
    pub fn trusted(path: &std::path::Path, bytes: &[u8]) -> Start {
        use vtium_settings::at;
        let Some(reading) = vtium_settings::sections(bytes) else {
            return Start::of(
                &vtium_settings::Read::Missing,
                vtium_settings::Config::builtin(),
            );
        };
        for e in &reading.errors.errors {
            log::error!("{}", at(path, e.line(), e));
        }
        if reading.errors.more > 0 {
            let more = crate::notice::more_errors(reading.errors.more);
            log::error!("{}", at(path, None, more));
        }
        for w in &reading.parsed.warnings {
            log::warn!("{}", at(path, Some(w.line()), w));
        }
        for n in &reading.parsed.notes {
            log::info!("{}", at(path, Some(n.line()), n));
        }
        let read = vtium_settings::Read::Trusted(reading.trusted());
        let mut start = Start::of(&read, reading.parsed.config);
        if start.outcome.line == Some(vtium_settings::Line::ClosedOff) {
            // a policy line vtium cannot read is why the file is closed and
            // broken, so its error comes first, then the rest in their order
            let unreadable = |e: &&vtium_settings::Error| {
                matches!(e.problem, vtium_settings::Problem::UnreadablePolicy(_))
            };
            let errors = &reading.errors.errors;
            let first: Vec<String> = errors
                .iter()
                .filter(unreadable)
                .chain(errors.iter().filter(|e| !unreadable(e)))
                .take(PANIC_ERRORS_MAX)
                .map(|e| at(path, e.line(), e).to_string())
                .collect();
            let more = errors
                .len()
                .saturating_sub(first.len())
                .saturating_add(reading.errors.more);
            start.report = Some(crate::notice::panic(path, first, more));
        }
        start
    }

    /// The start on the path `read`, `file` being what survives of a
    /// trusted file.
    fn of(read: &vtium_settings::Read, file: vtium_settings::Config) -> Start {
        use vtium_settings::Runs;
        let outcome = vtium_settings::resolve(read);
        let rules = match outcome.runs {
            Runs::File | Runs::NoHead | Runs::Survivors => file,
            Runs::Builtin => vtium_settings::Config::builtin(),
            Runs::Off => vtium_settings::Config {
                policy: vtium_settings::Policy::Closed,
                survivors: true,
                ..vtium_settings::Config::default()
            },
        };
        Start {
            outcome,
            rules,
            report: None,
        }
    }

    /// The console line the start shows, from the catalog: `None` for a
    /// silent start.
    pub fn line(&self) -> Option<&'static str> {
        self.outcome.line.map(vtium_settings::Line::text)
    }

    /// The notice the start shows: its console line and the next step,
    /// or for a broken closed file the panic screen's report, `None` for a
    /// silent start.
    pub fn notice(&self) -> Option<String> {
        self.report
            .as_ref()
            .map(vtium_render::NoticeReport::text)
            .or_else(|| self.outcome.line.map(outcome_notice))
    }

    /// The panic screen's report, for a broken closed file (ADR-1401 D7).
    pub fn report(&self) -> Option<&vtium_render::NoticeReport> {
        self.report.as_ref()
    }
}

/// The notice of an outcome's console line `line` (ADR-1401 D7): the line
/// as the catalog words it, then what to do next about the installed
/// vtium.conf.
pub fn outcome_notice(line: vtium_settings::Line) -> String {
    let next = line.next(std::path::Path::new(vtium_settings::CONF_FILE));
    format!("{}\n{next}", line.text())
}

/// The flow the start runs (ADR-1401 D3, D7; ADR-1403 D3): vtium.conf's
/// sections as [`crate::config::load`] found the file at `path`, resolved
/// at their three levels. A missing file, and one with no flow, runs the
/// compiled-in flow, silently; a file that fails the ownership check, or
/// that cannot be read as a file, is never read and runs the shipped
/// rules, `load` having reported why; a trusted file runs what its policy
/// and its errors leave ([`vtium_settings::resolve`]). Each error is
/// logged with its line, and the file is never written.
pub fn rules(path: &std::path::Path, file: &crate::config::File) -> Start {
    use crate::config::File;
    use vtium_settings::Read;
    let builtin = vtium_settings::Config::builtin;
    match file {
        File::Missing => Start::of(&Read::Missing, builtin()),
        File::Read(bytes) => Start::trusted(path, bytes),
        File::Untrusted(_) => Start::of(&Read::Untrusted, builtin()),
        File::Inaccessible => Start::of(&Read::Inaccessible, builtin()),
    }
}

/// The head a display joins: the one the first rule matching `display`
/// names (ADR-1401 D3), by its index. A display a rule sends `dark`, or
/// that no rule claims, joins none: the error is its log line, which for
/// a display no rule claims carries the rule that would give it the first
/// head, ready to paste.
pub fn place_display(
    rules: &vtium_settings::Config,
    display: &vtium_settings::Identity,
) -> Result<usize, String> {
    let placed = rules.walk(display);
    let name = display.connector.as_deref().unwrap_or("?").escape_debug();
    match (placed.place, placed.by) {
        (Place::Head(h), _) => Ok(h),
        (_, By::Rule(line)) => Err(format!(
            "display {name} stays off: the rule on line {line} of vtium.conf sends it to off"
        )),
        _ => {
            let target = rules.heads.first().map_or("console", |h| h.name.as_str());
            let rule = vtium_settings::rule_for(display, target).map_or_else(
                || "it reports nothing a rule can match".to_owned(),
                |rule| format!("add the rule `{rule}` to give it a head"),
            );
            Err(format!(
                "display {name} stays off: no rule in vtium.conf matches it; {rule}"
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
/// the number of steps. Under `natural` scrolling the report and the
/// scroll go the other way, and the zoom keeps the wheel's own direction.
fn wheel(steps: i32, natural: bool, tracking: bool, ctrl: bool) -> Wheel {
    let up = (steps > 0) != natural;
    if tracking {
        Wheel::Report(if up {
            MouseButton::WheelUp
        } else {
            MouseButton::WheelDown
        })
    } else if ctrl {
        Wheel::Zoom(if steps > 0 { Zoom::In } else { Zoom::Out })
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

    /// The panic screen of [`a_broken_closed_files_start_shows_the_panic_screens_report`]'s file.
    const PANIC: &str = concat!(
        "vtium.conf has errors and counts as closed: all input is off.\n",
        "No head can run because of the errors in /etc/vtium/vtium.conf, listed below.\n",
        "/etc/vtium/vtium.conf:4: This section has no key named \"focs\". Check the spelling, or remove the line.\n",
        "/etc/vtium/vtium.conf:5: This section has no key named \"font-weight\". Check the spelling, or remove the line.\n",
        "/etc/vtium/vtium.conf:7: This rule sends devices to head \"k\\u{202e}x\", but the file has no head of that name. Correct the name, or add a `[head NAME]` section for it.\n",
        "More errors: 2\n",
        "This console takes no input. For the full report, run vtium check-config /etc/vtium/vtium.conf over ssh or from other boot media.",
    );

    // ADR-1401 D7, ADR-1403 D3: the start runs vtium.conf's sections, the
    // compiled-in flow for a file with none and for no file, what survives
    // of a partly broken one with its console line, and the shipped rules,
    // with the skipped line, for a file the ownership check fails
    #[test]
    fn the_start_runs_vtium_conf_s_sections_else_the_shipped_rules() {
        use crate::config::File;
        let path = std::path::Path::new("/etc/vtium/vtium.conf");
        let read = |text: &str| rules(path, &File::Read(text.as_bytes().to_vec()));
        let valid = "font-size=12\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desk\n";
        let ran = read(valid);
        assert_eq!(ran.line(), None);
        assert_eq!(
            ran.rules
                .heads
                .iter()
                .map(|h| h.name.as_str())
                .collect::<Vec<_>>(),
            ["desk"]
        );
        let builtin = (vtium_settings::Config::builtin(), None);
        for no_flow in ["font-size=12\nmouse\n", ""] {
            let ran = read(no_flow);
            assert_eq!((ran.rules, ran.outcome.line), builtin, "{no_flow:?}");
        }
        let ran = rules(path, &File::Missing);
        assert_eq!((ran.rules, ran.outcome.line), builtin);
        let partly = "font-size=12\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desc\ndisplay * -> desk\n";
        let ran = read(partly);
        assert_eq!(ran.outcome.runs, vtium_settings::Runs::Survivors);
        assert_eq!(
            ran.line(),
            Some("vtium.conf has errors: running the parts without errors.")
        );
        // the rule keeps its line in the whole file
        assert_eq!(
            ran.rules.rules.iter().map(|r| r.line).collect::<Vec<_>>(),
            [6]
        );
        let why = vtium_settings::Untrusted::Owner {
            path: vtium_settings::Quoted::path(path),
            uid: 1000,
        };
        let ran = rules(path, &File::Untrusted(why));
        assert_eq!(ran.rules, vtium_settings::Config::builtin());
        assert_eq!(
            ran.notice().as_deref(),
            Some(
                "vtium.conf not used: running the built-in configuration.\nTo see why, run vtium check-config /etc/vtium/vtium.conf."
            )
        );
        let ran = rules(path, &File::Inaccessible);
        assert_eq!(
            ran.line(),
            Some("vtium.conf not used: running the built-in configuration.")
        );
    }

    // ADR-1403 D3: the start never judges the complete route, since
    // missing hardware never makes a file invalid at start: a closed file
    // whose rules send no keyboard to its head runs as written, silently
    // covers: platform.heads.policy
    #[test]
    fn a_file_with_no_complete_route_runs_at_start() {
        let text = "version = 1\npolicy = closed\n[head desk]\n[rules]\ndisplay * -> desk\n";
        let start = Start::trusted(
            std::path::Path::new("/etc/vtium/vtium.conf"),
            text.as_bytes(),
        );
        assert_eq!(start.outcome.runs, vtium_settings::Runs::File);
        assert_eq!(start.notice(), None);
        assert_eq!(
            start
                .rules
                .heads
                .iter()
                .map(|h| h.name.as_str())
                .collect::<Vec<_>>(),
            ["desk"]
        );
    }

    // ADR-1401 D7: a broken closed file's start shows the panic screen:
    // its console line, that no head is left, its first errors with their
    // file and line, escaped, the count of the rest, and what to do next
    // covers: platform.heads.policy
    #[test]
    fn a_broken_closed_files_start_shows_the_panic_screens_report() {
        let text = "version = 1\npolicy = closed\n[head kiosk]\nfocs = 1\nfont-weight = bold\n[rules]\ndisplay * -> k\u{202e}x\ninput * -> desc\ninput * -> nowhere\n";
        let start = Start::trusted(
            std::path::Path::new("/etc/vtium/vtium.conf"),
            text.as_bytes(),
        );
        assert_eq!(start.outcome.line, Some(vtium_settings::Line::ClosedOff));
        let shown = start.notice().unwrap_or_default();
        assert_eq!(shown, PANIC);
        assert!(
            !shown
                .chars()
                .any(|c| c != '\n' && (c.is_control() || c == '\u{202e}')),
            "escaped: {shown:?}"
        );
        // any other start shows its console line and next step alone
        let partly = "version = 1\npolicy = closed\n[head desk]\n[head kiosk]\nfocs = 1\n[rules]\ndisplay * -> desk\n";
        let start = Start::trusted(
            std::path::Path::new("/etc/vtium/vtium.conf"),
            partly.as_bytes(),
        );
        assert_eq!(
            start.notice(),
            Some(outcome_notice(vtium_settings::Line::Partly))
        );
    }

    // ADR-1401 D7: when a policy line vtium cannot read is why the file is
    // closed and broken, the panic screen lists that line's error first,
    // ahead of the errors that name no line and sort first otherwise, and
    // then the rest in their order
    // covers: platform.heads.policy
    #[test]
    fn the_panic_screen_lists_an_unreadable_policy_lines_error_first() {
        // past the file bound, with a section and no `version`: the two
        // errors that name no line would sort ahead of it
        let mut text = String::from("junk\npolicy = opne\n[head a]\n");
        while text.len() <= vtium_settings::FILE_MAX {
            text.push_str("# padding past the bound\n");
        }
        let start = Start::trusted(
            std::path::Path::new("/etc/vtium/vtium.conf"),
            text.as_bytes(),
        );
        assert_eq!(start.outcome.line, Some(vtium_settings::Line::ClosedOff));
        let items = start.report().map(|r| r.items.clone()).unwrap_or_default();
        assert_eq!(items.len(), PANIC_ERRORS_MAX, "{items:?}");
        assert!(
            items.first().is_some_and(
                |i| i.starts_with("/etc/vtium/vtium.conf:2: `policy` cannot be \"opne\"")
            ),
            "{items:?}"
        );
        // the others follow in their order: the two errors with no line
        assert!(
            items
                .get(1)
                .is_some_and(|i| i.starts_with("/etc/vtium/vtium.conf: This file is too long")),
            "{items:?}"
        );
        assert!(
            items.get(2).is_some_and(
                |i| i.starts_with("/etc/vtium/vtium.conf: `version = 1` must be at the top")
            ),
            "{items:?}"
        );
        assert_eq!(start.report().map(|r| r.more), Some(0));
    }

    // ADR-1401 D7, D9: the screen the display guard lights for a notice
    // alone draws it as a report, which keeps its next step on a display
    // too small for all of it; a head draws its notices as they come
    // covers: platform.heads.guards
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn the_notice_screen_draws_its_notice_as_a_report() {
        let config = crate::config::load(&[], &mut |_: &std::path::Path| Ok(None)).config;
        let clock: Clock = std::rc::Rc::new(Instant::now);
        let term = || terminal(&config, (0, 0), 0, std::rc::Rc::clone(&clock));
        let report = crate::notice::panic(
            std::path::Path::new("vtium.conf"),
            vec!["vtium.conf:4: an error".to_owned()],
            0,
        );
        let mut screen = Head::notice_screen(&config, term());
        assert_eq!(screen.overlay(None).notice, None, "nothing yet");
        screen.set_report(report.clone());
        assert_eq!(screen.notice(), Some(report.text().as_str()));
        let overlay = screen.overlay(None);
        assert_eq!(
            overlay.notice,
            Some(vtium_render::Notice::report(report.clone()))
        );
        assert!(overlay.problem);
        // a notice that is no report draws as it is, and replaces the report
        screen.set_notice("No head has a connected display.");
        assert_eq!(
            screen.overlay(None).notice,
            Some(vtium_render::Notice::new(
                "No head has a connected display."
            ))
        );
        let mut head = Head::new("h", &config, Vec::new(), term());
        head.add_notice(&report.text());
        assert_eq!(
            head.overlay(None).notice,
            Some(vtium_render::Notice::new(report.text()))
        );
    }

    // A console shown on a head with no display takes its own cell with
    // its grid, since no relayout runs to set it until a display comes, so
    // its pty's size is its own (ADR-1401 D8, `when-display-lost = keep`).
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_console_shown_on_an_unlit_head_brings_its_own_cell() {
        let config = crate::config::load(&[], &mut |_: &std::path::Path| Ok(None)).config;
        let clock: Clock = std::rc::Rc::new(Instant::now);
        let term = || terminal(&config, (0, 0), 0, std::rc::Rc::clone(&clock));
        let mut head = Head::new("h", &config, Vec::new(), term());
        let zoomed = Fit {
            grid: (40, 10),
            font_size: 32,
            cell: (16, 32),
            points: None,
            auto_size: false,
        };
        head.hidden.push(Console::new(2, term(), zoomed));
        assert!(head.views.is_empty(), "unlit");
        head.show(1, false);
        assert_eq!(head.console.id, 2);
        let size = head.pty_size();
        assert_eq!(
            (size.cols, size.rows, size.cell_width, size.cell_height),
            (40, 10, 16, 32)
        );
        head.show(0, false);
        assert_eq!(head.pty_size().cell_width, CELL.0, "the first's own again");
    }

    // ADR-1201 D11: where the configured font draws nothing at its size,
    // the return at a session's end puts back all it changed: the font and
    // the switch that chose it, the shown console's size, the ended hidden
    // console's fit, and the configured size, so the next console opens
    // and the next session ends as before.
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_return_that_cannot_draw_puts_back_all_it_changed() {
        let load = |args: &[&str]| {
            let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
            crate::config::load(&args, &mut |_: &std::path::Path| Ok(None)).config
        };
        let config = load(&["--font-size=0"]);
        let clock: Clock = std::rc::Rc::new(Instant::now);
        let term = || terminal(&config, (0, 0), 0, std::rc::Rc::clone(&clock));
        let mut head = Head::new("h", &config, Vec::new(), term());
        let zoomed = Fit {
            grid: (20, 5),
            font_size: 32,
            cell: (16, 32),
            points: None,
            auto_size: false,
        };
        head.hidden.push(Console::new(2, term(), zoomed));
        head.font = Some(Chosen::Builtin {
            name: Some("unscii-8"),
            cell: None,
        });
        head.switched = Some(2);
        (head.points, head.auto_size, head.font_size) = (None, false, 24);
        let before = |h: &Head| {
            (
                h.fresh_fit(),
                h.sized(),
                h.switched,
                (h.points, h.auto_size, h.font_size),
                h.hidden.first().map(|c| c.fit),
                h.chosen().name(16, "C").to_owned(),
            )
        };
        let was = before(&head);
        let mut kept = head.keep(2);
        head.reconfigure(&load(&["--font-size=12"]));
        head.switched = None;
        kept.font = Some(head.font.replace(Chosen::Builtin {
            name: Some("unifont"),
            cell: None,
        }));
        let fresh = head.fresh_fit();
        head.configured_size(
            2,
            Fit {
                grid: (40, 10),
                ..fresh
            },
        );
        head.configured_size(1, fresh);
        head.font_size = 16;
        assert_ne!(before(&head), was, "the return changed each");
        head.refont = false;
        head.put_back(kept);
        assert_eq!(before(&head), was);
        assert!(head.refont, "every renderer is rebuilt");
    }

    // ADR-1401 D5, binary-review's finding 3: the console the close chord
    // closes is told nothing: no focus-out joins what it owed its program,
    // and nothing it owed is sent; the console shown next gains the focus.
    #[test]
    #[expect(
        clippy::disallowed_methods,
        clippy::unwrap_used,
        reason = "a test's clock is the host's, and a test fails by panicking"
    )]
    fn the_console_the_close_chord_closes_is_told_nothing() {
        let config = crate::config::load(&[], &mut |_: &std::path::Path| Ok(None)).config;
        let clock: Clock = std::rc::Rc::new(Instant::now);
        let term = || terminal(&config, (0, 0), 0, std::rc::Rc::clone(&clock));
        let mut head = Head::new("h", &config, Vec::new(), term());
        head.hidden.push(Console::new(2, term(), head.fresh_fit()));
        head.focused = true;
        for console in head.consoles_mut() {
            console.term.terminal.input(b"\x1b[?1004h");
        }
        // a status report the closing console owes its program
        head.console.term.terminal.input(b"\x1b[5n");
        let mut closed = head.close_shown(false).unwrap();
        assert_eq!(closed.id, 1);
        assert_eq!(
            closed.term.terminal.take_replies(),
            b"\x1b[0n",
            "no focus-out joined, and nothing owed was sent"
        );
        // the console shown next is console 2; its focus-in goes to its
        // pty, which closing_a_console_gives_the_console_shown_next_the_focus
        // reads through a real session
        assert_eq!(head.console.id, 2);
        assert!(head.close_shown(false).is_none(), "the last never closes");
    }

    // A head shows each notice once and the newest NOTICES_MAX; an ended
    // console's notice stands above them and stays when they end.
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_head_keeps_each_notice_once_and_the_newest_few() {
        let config = crate::config::load(&[], &mut |_: &std::path::Path| Ok(None)).config;
        let term = terminal(&config, (0, 0), 0, std::rc::Rc::new(Instant::now));
        let mut head = Head::new("h", &config, Vec::new(), term);
        for n in 0..6 {
            head.add_notice(&format!("n{n}"));
            head.add_notice(&format!("n{n}"));
        }
        assert_eq!(head.notice(), Some("n2\nn3\nn4\nn5"));
        head.console.ended = true;
        head.compose_notices();
        let ended = crate::notice::session_ended();
        assert_eq!(
            head.notice(),
            Some(format!("{ended}\nn2\nn3\nn4\nn5").as_str()),
            "the ended session's notice above the others"
        );
        head.notice_due = Some(Instant::now());
        head.end_notices();
        assert_eq!(
            (head.notice(), head.notice_due),
            (Some(crate::notice::session_ended().as_str()), None)
        );
        assert!(!head.has_notices());
    }

    /// The colors a head built from the command line `args` draws its
    /// overlay in, each read off a 320x96 display in unifont, 40 by 6
    /// cells: a notice's text, box and edge; the full line's text, box and
    /// edge; and the minimal dot's circle and the cell behind it.
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn overlay_colors(args: &[&str]) -> [[u8; 3]; 8] {
        let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        let loaded = crate::config::load(&args, &mut |_: &std::path::Path| Ok(None));
        assert_eq!(loaded.reports, [], "{args:?}");
        let config = loaded.config;
        let term = terminal(&config, (0, 0), 0, std::rc::Rc::new(Instant::now));
        let mut head = Head::new("h", &config, Vec::new(), term);
        let draw = |head: &Head| {
            let face: DisplayFace = Box::new(vtium_font::Unifont::new(16));
            let renderer =
                vtium_render::Renderer::new(Font::new(face), 320, 96, RenderConfig::default());
            let mut canvas = vtium_render::Canvas::new(320, 96, 0);
            head.overlay(None)
                .draw(&renderer, (40, 6), Orientation::Normal, &mut canvas);
            canvas
        };
        let rgb = |p: Option<u32>| {
            let [_, r, g, b] = p.unwrap_or_default().to_be_bytes();
            [r, g, b]
        };
        // the edge at the box's corner, the box inside the edge and above
        // the text, and the first pixel of the text's row that is neither
        let boxed = |c: &vtium_render::Canvas| {
            let (edge, paper) = (c.pixel(0, 0), c.pixel(3, 3));
            let ink = (6..22)
                .flat_map(|y| (6..300).map(move |x| (x, y)))
                .map(|(x, y)| c.pixel(x, y))
                .find(|&p| p != paper && p != edge);
            [rgb(ink.flatten()), rgb(paper), rgb(edge)]
        };
        head.add_notice("No keyboard was found.");
        let [n_ink, n_paper, n_edge] = boxed(&draw(&head));
        head.end_notices();
        head.status = Some("desk · console 1 of 1".to_owned());
        let [o_ink, o_paper, o_edge] = boxed(&draw(&head));
        head.status = None;
        head.mark = Some(vtium_render::Mark::Focus);
        let dot = draw(&head);
        let paper = dot.pixel(312, 0);
        let ink = (0..16)
            .flat_map(|y| (312..320).map(move |x| (x, y)))
            .map(|(x, y)| dot.pixel(x, y))
            .find(|&p| p != paper);
        [
            n_ink,
            n_paper,
            n_edge,
            o_ink,
            o_paper,
            o_edge,
            rgb(ink.flatten()),
            rgb(paper),
        ]
    }

    // ADR-1401 D5: with no color key set, a head draws the colors vtium
    // drew before the keys, but for the dot, which takes the on-screen
    // display's colors where it took the notice's.
    #[test]
    fn unset_color_keys_draw_vtiums_own_colors() {
        assert_eq!(
            overlay_colors(&[]),
            [
                [0x1f, 0x29, 0x33],
                [0xfe, 0xe2, 0xe2],
                [0xb9, 0x1c, 0x1c],
                [0xe5, 0xe7, 0xeb],
                [0x1f, 0x29, 0x33],
                [0x61, 0x6e, 0x7c],
                [0xe5, 0xe7, 0xeb],
                [0x1f, 0x29, 0x33],
            ]
        );
    }

    // ADR-1401 D5: each of the six color keys sets its own part of what a
    // head draws and no other: the notice's three, the full line's three,
    // and the dot in the on-screen display's text and box.
    #[test]
    fn each_color_key_sets_its_part_of_what_a_head_draws() {
        let unset = overlay_colors(&[]);
        let parts: [(&crate::config::keys::Key, &[usize]); 6] = [
            (&keys::NOTICE_FOREGROUND, &[0]),
            (&keys::NOTICE_BACKGROUND, &[1]),
            (&keys::NOTICE_BORDER, &[2]),
            (&keys::OSD_FOREGROUND, &[3, 6]),
            (&keys::OSD_BACKGROUND, &[4, 7]),
            (&keys::OSD_BORDER, &[5]),
        ];
        for (key, at) in parts {
            let mut want = unset;
            for (i, part) in want.iter_mut().enumerate() {
                if at.contains(&i) {
                    *part = [1, 2, 3];
                }
            }
            let arg = format!("--{}=1,2,3", key.name);
            assert_eq!(overlay_colors(&[&arg]), want, "{}", key.name);
        }
    }

    #[test]
    fn a_display_left_dark_is_logged_and_one_no_rule_claims_with_the_rule_to_paste() {
        let rules = vtium_settings::parse(
            "version = 1\n[head desk]\n[rules]\ndisplay connector:pci-0000:03:00.0/DP-1 -> off\ndisplay connector:pci-0000:03:00.0/DP-2 -> desk\n",
        )
        .map(|p| p.config)
        .unwrap_or_default();
        let display = |connector: &str| vtium_settings::Identity {
            connector: Some(connector.to_owned()),
            card: Some("pci-0000:03:00.0".to_owned()),
            ..vtium_settings::Identity::display()
        };
        assert_eq!(place_display(&rules, &display("DP-2")), Ok(0));
        assert_eq!(
            place_display(&rules, &display("DP-1")),
            Err(
                "display DP-1 stays off: the rule on line 4 of vtium.conf sends it to off"
                    .to_owned()
            )
        );
        assert_eq!(
            place_display(&rules, &display("HDMI-A-1")),
            Err("display HDMI-A-1 stays off: no rule in vtium.conf matches it; add the rule `display connector:pci-0000:03:00.0/HDMI-A-1 -> desk` to give it a head".to_owned())
        );
        let bare = vtium_settings::Identity {
            connector: Some("fb\u{1b}".to_owned()),
            ..vtium_settings::Identity::display()
        };
        assert_eq!(
            place_display(&rules, &bare),
            Err("display fb\\u{1b} stays off: no rule in vtium.conf matches it; it reports nothing a rule can match".to_owned())
        );
    }

    // ADR-1001, "Memory": left to its default, a console's scrollback
    // budget follows its grid, what the 16 MiB minspec peak leaves after
    // its two screens, so the 170 by 48 minspec grid gets less than a
    // 640x480 display's 80 by 30; under the console's share, and an
    // explicit `sb-memory` keeps its value through every resize.
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_consoles_scrollback_budget_follows_its_grid_unless_set() {
        let budget = |args: &[&str], share: usize, grid: (usize, usize)| {
            let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
            let loaded = crate::config::load(&args, &mut |_: &std::path::Path| Ok(None));
            assert_eq!(loaded.reports, [], "{args:?}");
            let mut term = terminal(
                &loaded.config,
                (1000, 1000),
                share,
                std::rc::Rc::new(Instant::now),
            );
            term.terminal.resize(grid.0, grid.1);
            term.terminal.screen().scrollback_memory()
        };
        let share = 128 << 20;
        let minspec = budget(&[], share, (170, 48));
        let small = budget(&[], share, (80, 30));
        assert_eq!(minspec, vtium_term::sb_memory_default(170, 48));
        assert_eq!(small, vtium_term::sb_memory_default(80, 30));
        assert!(minspec < small);
        assert_eq!(budget(&[], 2 << 20, (80, 30)), 2 << 20);
        for grid in [(170, 48), (80, 30)] {
            assert_eq!(budget(&["--sb-memory=6M"], 6 << 20, grid), 6 << 20);
        }
    }

    // ADR-1401 D10: every console's scrollback together stays within the
    // most `sb-memory` may be on the machine, and every frit's store within
    // the machine's store limit.
    #[test]
    fn the_memory_of_every_console_and_frit_together_is_bounded() {
        const MIB: u64 = 1 << 20;
        // 512 MiB: `sb-memory` may be 128 MiB. The default at the minspec
        // grid, 5,242,880 bytes, fits 8 times; 50 consoles take 2,684,354
        // bytes each.
        assert_eq!(console_scrollback(5_242_880, 128 * MIB, 8, 1), 5_242_880);
        assert_eq!(console_scrollback(5_242_880, 128 * MIB, 50, 1), 2_684_354);
        assert_eq!(console_scrollback(64 << 20, 128 * MIB, 8, 1), 16 << 20);
        assert_eq!(console_scrollback(5_242_880, 128 * MIB, 0, 0), 5_242_880);
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
                wheel(steps, false, true, true),
                Wheel::Report(MouseButton::WheelUp)
            );
            assert_eq!(wheel(steps, false, false, true), Wheel::Zoom(Zoom::In));
            assert_eq!(
                wheel(steps, false, false, false),
                Wheel::Scroll { up: true }
            );
        }
        for steps in [-1, -7, i32::MIN] {
            assert_eq!(
                wheel(steps, false, true, false),
                Wheel::Report(MouseButton::WheelDown)
            );
            assert_eq!(wheel(steps, false, false, true), Wheel::Zoom(Zoom::Out));
            assert_eq!(
                wheel(steps, false, false, false),
                Wheel::Scroll { up: false }
            );
        }
    }

    // vtium/vtium#203: natural scrolling turns the report and the scroll
    // around, and leaves the zoom as the wheel turns
    #[test]
    fn natural_scrolling_turns_the_report_and_the_scroll_but_not_the_zoom() {
        assert_eq!(
            wheel(1, true, true, false),
            Wheel::Report(MouseButton::WheelDown)
        );
        assert_eq!(
            wheel(-1, true, true, false),
            Wheel::Report(MouseButton::WheelUp)
        );
        assert_eq!(wheel(1, true, false, false), Wheel::Scroll { up: false });
        assert_eq!(wheel(-1, true, false, false), Wheel::Scroll { up: true });
        assert_eq!(wheel(1, true, false, true), Wheel::Zoom(Zoom::In));
        assert_eq!(wheel(-1, true, false, true), Wheel::Zoom(Zoom::Out));
    }
}
