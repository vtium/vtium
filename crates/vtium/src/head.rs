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

pub mod identity;

use vtium_config::{By, Place};

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use vtium_font::{Face, Font};
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
use crate::font::{self, Chosen};
use crate::helper::Helper;
use crate::layout::Zoom;
use crate::respawn::Respawn;
use crate::sync::Sync;
use crate::term::{Clock, Term, View};

/// The grid before any display is known.
pub const GRID: (usize, usize) = (80, 24);

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

/// One head: its session's terminal and pty, its displays, and its grid.
pub struct Head {
    name: String,
    pub(crate) term: Term,
    pub(crate) pty: Option<Pty>,
    pub(crate) views: BTreeMap<(PathBuf, DisplayId), View>,
    pub(crate) respawn: Option<Respawn>,
    pub(crate) attached: Attached,
    /// Every display of the head draws at once at the end of the turn.
    pub(crate) draw_now: bool,
    pub(crate) text_due: Option<Instant>,
    pub(crate) cursor_due: Option<(Instant, Duration)>,
    pub(crate) pointer: Pointer,
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
    /// OSC 50.
    pub(crate) font: Option<Chosen>,
    /// The font changed: every display's renderer is rebuilt.
    pub(crate) refont: bool,
    /// OSC 50's refusals since its last switch: only the first is logged.
    pub(crate) font_refused: u64,
    /// The helper that generates a frit's glyphs, started on the first
    /// need (ADR-1301 D2).
    pub(crate) helper: Option<Helper>,
    /// The helper cannot start: logged once, and the built-in font draws.
    pub(crate) helper_refused: bool,
    /// The cells whose glyphs the helper was asked for.
    pub(crate) asked: Vec<vtium_font::Cell>,
    pub(crate) orientation: Orientation,
    pub(crate) grid: (usize, usize),
}

impl Head {
    /// The head `name` for `config`, its terminal on [`GRID`] with the
    /// scrollback `config` allows, and the font keys `font` (ADR-1401 D3):
    /// its session starts at its first display.
    pub(crate) fn new(
        name: &str,
        config: &Config,
        font: &vtium_config::Font,
        cap: usize,
        lines: usize,
        clock: Clock,
    ) -> Head {
        let mut terminal = Terminal::new(GRID.0, GRID.1, cap);
        terminal.screen_mut().set_scrollback_max(lines);
        terminal
            .screen_mut()
            .set_scrollback_memory(crate::config::budget::sb_memory(config));
        terminal.set_backspace_sends_delete(config.switch(&keys::BACKSPACE_DELETE));
        terminal.set_palette(config.palette());
        terminal.set_font_ops(config.switch(&keys::ALLOW_FONT_OPS));
        if let Ok(table) = crate::termcap::termcap() {
            terminal.set_termcap(table);
        }
        let timeout = Duration::from_millis(config.number(&keys::SYNC_TIMEOUT).unwrap_or(200));
        terminal.set_sync_enabled(!timeout.is_zero());
        let sync = Sync::new(timeout, terminal.sync_frames());
        let blink = Blink::configured(config);
        let size = font
            .size
            .map(u64::from)
            .or_else(|| config.number(&keys::FONT_SIZE));
        Head {
            name: name.to_owned(),
            term: Term::new(terminal, sync, blink, clock),
            pty: None,
            views: BTreeMap::new(),
            respawn: None,
            attached: Attached::NotYet,
            draw_now: false,
            text_due: None,
            cursor_due: None,
            pointer: Pointer::default(),
            tracking: MouseTracking::Off,
            font_name: font.name.clone(),
            font_size: 0,
            points: size
                .filter(|&n| n != 0)
                .map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
            auto_size: size.unwrap_or(0) == 0,
            sized: size.unwrap_or(0) != 0,
            has_font: false,
            step: 16,
            cell: (8, 16),
            font: None,
            refont: false,
            font_refused: 0,
            helper: None,
            helper_refused: false,
            asked: Vec::new(),
            orientation: Orientation::Normal,
            grid: GRID,
        }
    }

    /// The head's name, as its `[head NAME]` gives it.
    pub fn name(&self) -> &str {
        &self.name
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
                let c = v.renderer.font().cell();
                (c.width, c.height)
            })
            .collect()
    }

    /// The head's terminal.
    pub fn term(&self) -> &Term {
        &self.term
    }

    /// The session's pty, while its command runs.
    pub fn pty(&self) -> Option<&Pty> {
        self.pty.as_ref()
    }

    /// Whether the session's command has started.
    pub fn started(&self) -> bool {
        self.respawn.is_some()
    }

    /// The text of the screen's rows, for tests and logs.
    pub fn screen_text(&self) -> Vec<String> {
        self.term
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
    pub(crate) fn face(&self, height: u32, locale: &str) -> Result<Box<dyn Face>, String> {
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
    pub(crate) fn font_at(&self, face: Box<dyn Face>, config: &Config) -> Font<Box<dyn Face>> {
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

    /// The terminal's replies go to the program.
    pub(crate) fn send_user_bytes(&mut self) {
        let bytes = self.term.terminal.take_replies();
        if let Some(pty) = &mut self.pty {
            if !bytes.is_empty() {
                pty.write(&bytes);
            }
        }
    }

    /// The console is potentially visible while it draws: not while the
    /// VT is another's or the displays are blanked. Mode 2033's report
    /// goes out at once, to a program that may be waiting in a read.
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.term.terminal.set_visible(visible);
        self.send_user_bytes();
    }

    /// The terminal and the pty at `grid`, the pointer's last pixel the
    /// grid's.
    pub(crate) fn resize(&mut self, grid: (usize, usize)) {
        let (cw, ch) = self.cell;
        let (w, h) = self.pixels(grid);
        self.term.terminal.set_cell_size(cw, ch);
        self.term.terminal.set_display_size(w, h);
        for v in self.views.values_mut() {
            v.lost = true;
        }
        self.draw_now = true;
        if grid != self.grid {
            self.grid = grid;
            let size = self.pty_size();
            if let Some(pty) = &mut self.pty {
                if let Err(e) = pty.resize(size) {
                    log::warn!("cannot resize the pty: {e}");
                }
            }
        }
        // Last, so mode 2048 reports the resize once it is complete, a new
        // font on the same grid included; the report goes out at once.
        self.term.terminal.resize(grid.0, grid.1);
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
        let palette = self.term.terminal.palette();
        let frame = self.term.terminal.screen().frame(palette, Age::NEVER);
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
        let palette = self.term.terminal.palette();
        DrawState {
            cursor_on: self.term.blink.cursor_shown(),
            blink_hidden: !self.term.blink.text_shown(),
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
        let tracking = self.term.terminal.mouse_tracking() != MouseTracking::Off;
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
                    self.term.terminal.screen_mut().select_to(col, row);
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
                        self.term.terminal.screen_mut().scroll_view_up(WHEEL_LINES);
                    }
                    Wheel::Scroll { up: false } => {
                        self.term
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
                self.term.terminal.screen_mut().clear_selection();
            }
        }
        // Only motion may be coalesced while the child does not read; a
        // press, a release, a wheel report or a paste is queued whole.
        let bytes = self.term.terminal.take_replies();
        if let Some(pty) = self.pty.as_mut().filter(|_| !bytes.is_empty()) {
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
        let tracking = self.term.terminal.mouse_tracking();
        if tracking != self.tracking {
            self.tracking = tracking;
            self.pointer.drag = None;
            self.term.terminal.screen_mut().clear_selection();
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
        self.term
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
                let screen = self.term.terminal.screen_mut();
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
                    self.term.terminal.screen_mut().clear_selection();
                }
                None => {}
            },
            (Button::Middle, true) => {}
            (Button::Middle, false) => {
                self.paste(selection.as_deref());
                self.term.terminal.screen_mut().clear_selection();
            }
            (Button::Right, _) => {
                self.pointer.drag = None;
                self.term.terminal.screen_mut().clear_selection();
            }
        }
    }

    /// Copies the selected text into the selection buffer; with nothing
    /// selected the buffer keeps what it holds.
    fn copy(&self, selection: &mut Option<Vec<u8>>) {
        if let Some(text) = self
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
            self.term.terminal.paste(text);
        }
    }
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
