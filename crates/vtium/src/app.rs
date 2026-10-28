//! The seat and its one session, wired: the VT's events, the video
//! devices and their displays, the input, the DPMS blank, the pty and its
//! child, and when the terminal draws. The loop calls [`App::turn`] after
//! each dispatch with the time, and waits at most [`App::next_wake`].
//!
//! Each step follows the C reference's flow (docs/c-reference.md, parts 1,
//! 2 and 5) and vtium-platform's host list:
//!
//! - **The VT.** `Acquired` wakes the input and every device, which draw
//!   whole; `Release` deactivates the terminal, puts the input and every
//!   device to sleep, and acknowledges once each reported `Asleep`;
//!   `Retaken` sets every device's displays up again, or while blanked
//!   at the input that ends the blank, and keeps the blank and its timer;
//!   `Hangup` exits.
//! - **Keys**, in order: the VT's, then the seat's (the power keys, which
//!   act on a press and never on its repeat), then the terminal's grabs,
//!   then DECARM's drop of a repeat, then the encoder. A key draws at once
//!   and holds the cursor's blink.
//! - **The pointer.** While the program tracks it, its events are
//!   reported, but a button pressed, a move without a button and the
//!   wheel with Shift held, as xterm's are. The others select, copy,
//!   paste and scroll ([`App::pointer_event`]).
//! - **The blank.** Each key press and pointer action (a move, a button
//!   or the wheel) goes to every device's `input`, which retries a failed
//!   wake-up, and to [`Blank::input`]; a device's `Awake` goes to
//!   [`Blank::input`] too, but a re-take's, and whenever it says the
//!   displays were blanked every device unblanks. The first display, the
//!   last going and the next after none count too, as a display change,
//!   which wakes nothing while the VT is not ours; another hotplug or a
//!   re-take never counts. The seat's clock drives the blank's countdown
//!   ([`App::next_wake`]). A signal from outside the session
//!   ([`crate::blank`]) blanks at once, or wakes as a program's
//!   `CSI 13 ]` does, at the turn's end.
//! - **The child.** It first starts once a display is set up, on the
//!   grid's size ([`respawn::first_start_due`]); when it exits the respawn
//!   policy starts it again or ends vtium.
//! - **No display.** From the first acquire, vtium waits `display-wait`
//!   for a frame on a display's screen, and with none by then it ends
//!   ([`crate::wait`]). Once one has come, the last going leaves the
//!   session running on the grid it had, with the keys reaching it and
//!   no blank, until the next comes and is drawn whole.
//! - **Exit.** A request (SIGTERM, SIGINT, a hangup, the policy) switches
//!   back to the VT shown at the start and serves the loop for up to
//!   [`LEAVE_WAIT`] while the switch is pending.

use std::any::Any;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium_font::{Face, Font};
use vtium_platform::calloop::LoopHandle;
use vtium_platform::display::{Blank, DisplayId, Dpms, Event};
use vtium_platform::input::{Button, InputEvent, KeyEvent, Mods as InputMods, PointerEvent};
use vtium_platform::pty::{Erase, Pty, Size};
use vtium_platform::udev::{DeviceEvent, DeviceKind};
use vtium_platform::vt::{Switch, Vt, VtEvent};
use vtium_render::{
    Config as RenderConfig, DecorationColors, Depth, DrawState, Orientation, PointerShape, Profile,
};
use vtium_term::{
    Age, BlankRequest, BlankTimeout, Content, Glyph, Mods, MouseButton, MouseEvent, MouseTracking,
    Terminal,
};

use crate::blank::Order;
use crate::blink::{Blink, Timer};
use crate::child::{self, Display as SessionDisplay, Place};
use crate::config::keys;
use crate::config::{Config, ModeSpec};
use crate::font::{self, Chosen};
use crate::grab::{self, Chord, Grab};
use crate::helper::{self, Answer, Helper};
use crate::layout::{self, Zoom};
use crate::power::{self, Power, PowerKey};
use crate::respawn::{self, Next, Respawn};
use crate::sync::{Action, Sync};
use crate::term::{Clock, Term, View};
use crate::video::Video;
use crate::wait::Wait;

/// How long the loop is served at exit while the switch back is pending
/// (`src/main.c:203-209` at kmscon b1f7a7c5).
pub const LEAVE_WAIT: Duration = Duration::from_millis(100);

/// The grid before any display is known.
const GRID: (usize, usize) = (80, 24);

/// Why vtium ends, and its exit status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// SIGTERM or SIGINT.
    Signal,
    /// The session's command exited under `oneshot`.
    Oneshot,
    /// The session's command exited quickly too many times running.
    TooQuick,
    /// The VT hung up.
    Hangup,
    /// The session's command could not be started.
    Spawn,
    /// No display came within `display-wait` of the start.
    NoDisplay,
}

impl Exit {
    pub fn status(self) -> i32 {
        match self {
            Exit::Signal | Exit::Oneshot => 0,
            Exit::TooQuick | Exit::Hangup | Exit::Spawn | Exit::NoDisplay => 1,
        }
    }
}

/// Whether the seat has a display to draw on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attached {
    /// None yet since the start.
    NotYet,
    Yes,
    /// The last one went.
    Gone,
}

/// Where the exit stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Leaving {
    /// The switch back is pending until this time.
    Until(Instant),
    Done,
}

/// The pointer, in the grid's pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Pointer {
    x: u32,
    y: u32,
    visible: bool,
    held: Option<Button>,
    /// The held button selects or pastes, until its release, rather than
    /// being reported: its press came while the program did not track the
    /// pointer, or with Shift.
    ours: bool,
    /// The left button's drag extends the selection.
    drag: Option<Drag>,
    /// Left presses in a row, each soon after the last: one selects
    /// characters, two a word, three a line, and a fourth starts again.
    clicks: u8,
}

/// A drag that extends the selection from the cell it started in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drag {
    from: (usize, usize),
    /// It left that cell.
    moved: bool,
}

/// A seat's VT, which the app holds behind one type: compiled once, and
/// given back as itself to the loop sources that need it, as the kernel
/// VT's signals do.
pub trait SeatVt: Vt + Any {
    fn as_any(&mut self) -> &mut dyn Any;
}

impl<T: Vt + Any> SeatVt for T {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl dyn SeatVt {
    /// The VT as a `T`, when it is one.
    pub fn get<T: Vt + Any>(&mut self) -> Option<&mut T> {
        self.as_any().downcast_mut::<T>()
    }
}

/// The seat, its devices and its session.
pub struct App {
    pub vt: Box<dyn SeatVt>,
    pub videos: BTreeMap<PathBuf, Box<dyn Video>>,
    pub input: Option<vtium_platform::input::Input>,
    pub udev: Option<vtium_platform::udev::Udev>,
    pub blank: Blank,
    pub term: Term,
    pub pty: Option<Pty>,
    views: BTreeMap<(PathBuf, DisplayId), View>,
    config: Config,
    handle: LoopHandle<'static, App>,
    clock: Clock,
    epoch: Instant,
    respawn: Option<Respawn>,
    /// [`App::start`] was called and the command has not started yet.
    start_due: bool,
    /// The VT is ours and the devices awake.
    awake: bool,
    /// The VT was re-taken while blanked: every device is set up again
    /// at the input that ends the blank.
    retake_owed: bool,
    /// Devices a re-take is setting up again: their `Awake` is not
    /// activity (the operator's decision on vtium/vtium#21).
    retaking: BTreeSet<PathBuf>,
    /// Devices a release waits on.
    releasing: Option<BTreeSet<PathBuf>>,
    /// The start's wait for a display.
    wait: Wait,
    /// `display-wait=forever`.
    forever: bool,
    attached: Attached,
    /// A frame has been on screen since the start.
    presented: bool,
    /// Every display draws at once at the end of the turn.
    draw_now: bool,
    text_due: Option<Instant>,
    cursor_due: Option<(Instant, Duration)>,
    timer_due: Option<Instant>,
    pointer: Pointer,
    /// The base font's height; each display may draw larger
    /// ([`layout::layout`]).
    font_size: u32,
    /// `font-size=0`: the base height follows the smallest display, until
    /// a zoom.
    auto_size: bool,
    /// The base font was found once: a later failure keeps it.
    has_font: bool,
    /// The base font's size step, which a zoom takes.
    step: u32,
    /// The base font's cell, the grid's and the pty's.
    cell: (u32, u32),
    /// The font set, chosen at the first display and switched by OSC 50.
    font: Option<Chosen>,
    /// Where the font's sets come from.
    font_sources: font::Sources,
    /// The font changed: every display's renderer is rebuilt.
    refont: bool,
    /// OSC 50's refusals since its last switch: only the first is logged.
    font_refused: u64,
    /// The helper that generates a frit's glyphs, started on the first
    /// need (ADR-1301 D2).
    helper: Option<Helper>,
    /// The helper cannot start: logged once, and the built-in font draws.
    helper_refused: bool,
    /// The cells whose glyphs the helper was asked for.
    asked: Vec<vtium_font::Cell>,
    /// The kernel's font, read from the VT at start for a frit's
    /// `console:` source (vtium/vtium#145).
    console_font: Option<vtium_platform::vt::font::ConsoleFont>,
    /// The locale, which keeps the built-in Unscii-8 from a zh, ja or ko
    /// locale (ADR-1200 D7).
    locale: String,
    orientation: Orientation,
    grid: (usize, usize),
    exit: Option<Exit>,
    leaving: Option<Leaving>,
    inherited: Vec<(OsString, OsString)>,
    /// `oneshot`: the child's exit ends vtium.
    oneshot: bool,
    /// The `mode` lines, for each DRM device opened.
    modes: Vec<ModeSpec>,
    /// `use-original-mode`, off when a mode is set.
    use_original: bool,
    /// `clipboard-write`: OSC 52's selection is taken.
    clipboard_write: bool,
    /// `program-blank`: a program's blank sequences are taken.
    program_blank: bool,
    /// `dpms-timeout`, which RIS returns the blank to.
    dpms_timeout: Option<Duration>,
    /// A blank or wake sent by signal, taken at the turn's end.
    order: Option<Order>,
    /// The selection buffer: what the user copied last, or OSC 52 set
    /// last, held for a paste.
    selection: Option<Vec<u8>>,
    /// The mouse tracking the program asked for, last seen: a change
    /// clears the selection.
    tracking: MouseTracking,
    /// The keys `grab-paste` binds.
    paste_keys: Vec<Chord>,
    /// `pointer-hide-while-typing`.
    hide_while_typing: bool,
    /// The power keys the configuration binds, and what they act through.
    power_keys: Vec<(PowerKey, Vec<Chord>)>,
    power: Box<dyn Power>,
    /// The child writing `/sys/power/state` has not exited: the machine
    /// sleeps, or is going to.
    sleeping: Rc<Cell<bool>>,
}

impl App {
    /// The seat for `config` on `vt`, with the session's terminal; the
    /// child starts at [`App::start`].
    pub fn new(
        config: Config,
        vt: Box<dyn SeatVt>,
        handle: LoopHandle<'static, App>,
        clock: Clock,
        inherited: Vec<(OsString, OsString)>,
    ) -> App {
        let now = clock();
        let cap = vtium_term::sb_size_cap(
            physical_memory(),
            config.number(&keys::SB_SIZE_CAP).map(to_usize),
        );
        let mut terminal = Terminal::new(GRID.0, GRID.1, cap);
        let lines = vtium_term::sb_size(
            config
                .number(&keys::SB_SIZE)
                .map_or(vtium_term::SB_SIZE_DEFAULT, to_usize),
            physical_memory(),
            config.number(&keys::SB_SIZE_CAP).map(to_usize),
        );
        terminal.screen_mut().set_scrollback_max(lines);
        terminal.set_backspace_sends_delete(config.switch(&keys::BACKSPACE_DELETE));
        terminal.set_palette(config.palette());
        terminal.set_font_ops(config.switch(&keys::ALLOW_FONT_OPS));
        if let Ok(table) = crate::termcap::termcap() {
            terminal.set_termcap(table);
        }
        let timeout = Duration::from_millis(config.number(&keys::SYNC_TIMEOUT).unwrap_or(200));
        terminal.set_sync_enabled(!timeout.is_zero());
        let sync = Sync::new(timeout, terminal.sync_frames());
        let blink = Blink::configured(&config);
        let dpms = config
            .number(&keys::DPMS_TIMEOUT)
            .map(Duration::from_secs)
            .filter(|d| !d.is_zero());
        let modes = config.modes(&keys::MODE).to_vec();
        let use_original = config.switch(&keys::USE_ORIGINAL_MODE) && modes.is_empty();
        let power_keys = [
            (PowerKey::Reboot, &keys::GRAB_REBOOT),
            (PowerKey::PowerOff, &keys::GRAB_POWER_OFF),
            (PowerKey::Suspend, &keys::GRAB_SUSPEND),
        ]
        .into_iter()
        .map(|(k, key)| (k, config.chords(key).to_vec()))
        .filter(|(_, chords)| !chords.is_empty())
        .collect();
        // The key holds text only for `forever`.
        let forever = config.text(&keys::DISPLAY_WAIT).is_some();
        let wait = Wait::new(
            (!forever)
                .then(|| Duration::from_secs(config.number(&keys::DISPLAY_WAIT).unwrap_or(10))),
        );
        App {
            oneshot: config.switch(&keys::ONESHOT),
            clipboard_write: config.switch(&keys::CLIPBOARD_WRITE),
            program_blank: config.switch(&keys::PROGRAM_BLANK),
            dpms_timeout: dpms,
            order: None,
            selection: None,
            tracking: MouseTracking::Off,
            paste_keys: config.chords(&keys::GRAB_PASTE).to_vec(),
            hide_while_typing: config.switch(&keys::POINTER_HIDE_WHILE_TYPING),
            power_keys,
            power: Box::new(power::System),
            sleeping: Rc::default(),
            modes,
            use_original,
            vt,
            videos: BTreeMap::new(),
            input: None,
            udev: None,
            blank: Blank::new(dpms, now),
            term: Term::new(terminal, sync, blink, clock.clone()),
            pty: None,
            views: BTreeMap::new(),
            font_size: config.number(&keys::FONT_SIZE).map_or(0, to_u32),
            auto_size: config.number(&keys::FONT_SIZE).unwrap_or(0) == 0,
            has_font: false,
            step: 16,
            cell: (8, 16),
            font: None,
            font_sources: font::Sources::default(),
            refont: false,
            font_refused: 0,
            helper: None,
            helper_refused: false,
            asked: Vec::new(),
            console_font: None,
            locale: locale(&inherited),
            orientation: Orientation::Normal,
            config,
            handle,
            clock,
            epoch: now,
            respawn: None,
            start_due: false,
            awake: false,
            retake_owed: false,
            retaking: BTreeSet::new(),
            releasing: None,
            wait,
            forever,
            attached: Attached::NotYet,
            presented: false,
            draw_now: false,
            text_due: None,
            cursor_due: None,
            timer_due: None,
            pointer: Pointer::default(),
            grid: GRID,
            exit: None,
            leaving: None,
            inherited,
        }
    }

    /// Puts the power keys' actions through `power`, a test's stand-in
    /// for PID 1, logind and the kernel.
    pub fn set_power(&mut self, power: Box<dyn Power>) {
        self.power = power;
    }

    /// Reads the font's sets from `sources` in place of the system's store
    /// and vconsole files: a test's stand-in. The font is chosen again at
    /// the next display.
    pub fn set_font_sources(&mut self, sources: font::Sources) {
        self.font_sources = sources;
        self.font = None;
    }

    /// The kernel's font, read from the VT while it was in text mode, for
    /// a frit's `console:` source (vtium/vtium#145).
    pub fn set_console_font(&mut self, font: vtium_platform::vt::font::ConsoleFont) {
        self.console_font = Some(font);
    }

    /// The selection buffer: what the user copied last, or a program set
    /// with OSC 52 while `clipboard-write` is on.
    pub fn selection(&self) -> Option<&[u8]> {
        self.selection.as_deref()
    }

    /// The configuration the seat runs with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Starts the session's command at the end of the first turn
    /// [`respawn::first_start_due`] allows.
    pub fn start(&mut self) {
        self.start_due = true;
    }

    /// Whether the session's command has started since [`App::start`].
    pub fn started(&self) -> bool {
        self.respawn.is_some()
    }

    /// The command's first start, when it is due.
    fn first_start(&mut self, now: Instant) {
        let display = !self.views.is_empty();
        if self.start_due && respawn::first_start_due(self.awake, display, self.forever) {
            self.start_due = false;
            self.respawn = Some(Respawn::new(now.saturating_duration_since(self.epoch)));
            self.spawn();
        }
    }

    /// Asks vtium to end, as SIGTERM does.
    pub fn request_exit(&mut self, why: Exit) {
        self.exit.get_or_insert(why);
    }

    /// A blank or wake sent from outside the session ([`crate::blank`]),
    /// taken at the turn's end; of several in one turn the last holds.
    pub fn order(&mut self, order: Order) {
        self.order = Some(order);
    }

    /// Why vtium ends, once the exit is done.
    pub fn finished(&self) -> Option<Exit> {
        (self.leaving == Some(Leaving::Done))
            .then_some(self.exit)
            .flatten()
    }

    /// Adds a video device, asleep; the VT's acquire wakes it.
    pub fn add_video(&mut self, node: PathBuf, mut video: Box<dyn Video>) {
        if self.awake {
            if let Err(e) = video.wake_up() {
                log::warn!("cannot wake {}: {e}", node.display());
            }
        }
        self.videos.insert(node, video);
    }

    /// Removes a video device and its displays.
    pub fn remove_video(&mut self, node: &std::path::Path) {
        self.videos.remove(node);
        self.views.retain(|(n, _), _| n != node);
        self.retaking.remove(node);
        if let Some(r) = &mut self.releasing {
            r.remove(node);
        }
        self.relayout();
    }

    fn place(&self) -> Place<'_> {
        let kms = self.videos.values().any(|v| v.backend() == "drm2d");
        Place {
            seat: Some(self.vt.seat()),
            vtnr: self.vt.number().map(u32::from),
            display: if kms || self.videos.is_empty() {
                SessionDisplay::Kms
            } else {
                SessionDisplay::Fbdev
            },
        }
    }

    fn pty_size(&self) -> Size {
        let cell = self.cell;
        Size {
            cols: u16::try_from(self.grid.0).unwrap_or(u16::MAX),
            rows: u16::try_from(self.grid.1).unwrap_or(u16::MAX),
            cell_width: cell.0,
            cell_height: cell.1,
        }
    }

    fn spawn(&mut self) {
        let spec = child::child(&self.config, &self.place(), self.inherited.clone());
        let Some(command) = spec.command() else {
            log::error!("no command to run");
            self.request_exit(Exit::Spawn);
            return;
        };
        let erase = if self.config.switch(&keys::BACKSPACE_DELETE) {
            Erase::Delete
        } else {
            Erase::Backspace
        };
        let mut pty = match Pty::spawn(command, self.pty_size(), erase) {
            Ok(pty) => pty,
            Err(e) => {
                log::error!("cannot start the session: {e}");
                self.request_exit(Exit::Spawn);
                return;
            }
        };
        let inserted = pty.insert(&self.handle, |app: &mut App| {
            let App { pty, term, .. } = app;
            pty.as_mut().map(|p| (p, term))
        });
        if let Err(e) = inserted {
            log::error!("cannot watch the session's pty: {e}");
            self.request_exit(Exit::Spawn);
            return;
        }
        self.pty = Some(pty);
    }

    /// Chooses the font once, at the first display: a frit `font-name`
    /// names, generated at run time (ADR-1301 D2); else the set the
    /// configuration or vconsole names, from the store (ADR-1200 D9), else
    /// the built-ins, with one line naming the command that builds the set.
    fn choose_font(&mut self) {
        if self.font.is_some() {
            return;
        }
        let s = &self.font_sources;
        let wanted = font::wanted(
            &self.config,
            &font::read_text(&s.vconsole_conf),
            &font::read_text(&s.kernel_cmdline),
        );
        let frit = wanted
            .as_ref()
            .filter(|w| w.came == font::Came::Config)
            .and_then(|w| Some((w.name.clone(), font::frit(&s.frits, &w.name)?)));
        if let Some((name, path)) = frit {
            // The frit's cell is read as text before the first frame; its
            // fonts are the helper's to parse (#145).
            match font::frit_text(&path).and_then(|t| font::frit_cell(&t)) {
                Ok(cell) => {
                    log::info!(
                        "the font is the frit {name}, drawn in the built-in font until its glyphs are generated"
                    );
                    self.font = Some(Chosen::Frit {
                        name,
                        path,
                        cell,
                        generated: crate::deferred::Shared::default(),
                    });
                }
                Err(why) => {
                    log::warn!(
                        "the frit {name} is not drawn ({why}): vtium draws its built-in font"
                    );
                    self.font = Some(Chosen::Builtin(None));
                }
            }
            return;
        }
        let chosen = match wanted {
            None => Chosen::Builtin(None),
            Some(w) if vtium_font::set::builtin(&w.name).is_some() => {
                font::by_name(s, &w.name).unwrap_or(Chosen::Builtin(None))
            }
            Some(w) => match font::load(s, &w) {
                Ok((name, set)) => {
                    log::info!("the font is the set {name}");
                    Chosen::Set { name, set }
                }
                Err((why, held)) => {
                    log::warn!(
                        "the font {} is not drawn ({why}): vtium draws its built-in font; build the set with {}",
                        w.name,
                        w.command(held)
                    );
                    Chosen::Builtin(None)
                }
            },
        };
        self.font = Some(chosen);
    }

    /// The font set in use, the built-ins before one is chosen.
    fn chosen(&self) -> &Chosen {
        const BUILTIN: &Chosen = &Chosen::Builtin(None);
        self.font.as_ref().unwrap_or(BUILTIN)
    }

    /// The name of the font set in use, as OSC 50's query reports it.
    pub fn font_name(&self) -> &str {
        self.chosen().name(self.font_size, &self.locale)
    }

    /// The font set's face at `height`.
    fn face(&self, height: u32) -> Result<Box<dyn Face>, String> {
        let chosen = self.chosen();
        chosen.face(height, &self.locale).ok_or_else(|| {
            format!(
                "the set {} draws nothing at {height} pixels",
                chosen.name(height, &self.locale)
            )
        })
    }

    /// OSC 50 asked for the set `name` (ADR-1200 D5): a built set switches
    /// the live console as a zoom does, on a new grid; anything else
    /// leaves the font. The first refusal since the last switch is logged
    /// and the rest counted, so a program's flood of requests logs one
    /// line.
    fn switch_font(&mut self, name: &str) {
        self.choose_font();
        let same = match self.chosen() {
            Chosen::Set { name: n, .. } | Chosen::Frit { name: n, .. } => n == name,
            Chosen::Builtin(Some(n)) => *n == name,
            Chosen::Builtin(None) => false,
        };
        if same {
            return;
        }
        let new = font::by_name(&self.font_sources, name).and_then(|new| {
            let old = self.font.replace(new);
            self.refont = true;
            if self.relayout() {
                return Ok(());
            }
            self.font = old;
            self.refont = true;
            self.relayout();
            Err(format!("it draws nothing at {} pixels", self.font_size))
        });
        match new {
            Ok(()) => {
                let refused = std::mem::take(&mut self.font_refused);
                if refused > 1 {
                    log::info!(
                        "OSC 50 was refused {} more times before this switch",
                        refused.saturating_sub(1)
                    );
                }
                log::info!("OSC 50: the font is now the set {}", self.font_name());
            }
            Err(why) => {
                if self.font_refused == 0 {
                    log::warn!(
                        "OSC 50 asked for the font {name}: {why}; the font stays {}, and OSC 50's further refusals are counted until a switch",
                        self.font_name()
                    );
                }
                self.font_refused = self.font_refused.saturating_add(1);
            }
        }
    }

    /// The font at `height`, with the configuration's sprite policy and
    /// missing glyph.
    fn font_at(&self, face: Box<dyn Face>) -> Font<Box<dyn Face>> {
        let font = Font::new(face);
        let policy = font::policy(&self.config, font.policy());
        font.with_policy(policy)
            .with_missing(font::missing(&self.config))
    }

    fn render_config(&self) -> RenderConfig {
        let profile = match self.config.text(&keys::GLYPH_CACHE) {
            Some("performance") => Profile::Performance,
            Some("minspec") => Profile::Minspec,
            _ => Profile::default(),
        };
        RenderConfig {
            profile,
            orientation: self.orientation,
            blink: crate::blink::text_blink(&self.config),
            decorations: DecorationColors {
                underline: self.config.deco_color(&keys::UNDERLINE_COLOR),
                strike: self.config.deco_color(&keys::STRIKE_COLOR),
                force: self.config.switch(&keys::FORCE_DECORATION_COLORS),
            },
            pointer: self
                .config
                .char(&keys::POINTER_CHAR)
                .map_or(PointerShape::Arrow, PointerShape::Glyph),
            ..RenderConfig::default()
        }
    }

    fn measure(&self) -> bool {
        matches!(self.config.text(&keys::RENDER_TIMER), Some("log" | "both"))
    }

    fn add_display(&mut self, node: &PathBuf, id: DisplayId) {
        let Some(video) = self.videos.get(node) else {
            return;
        };
        let Some(size) = video.size(id) else {
            return;
        };
        let refresh = video.refresh_mhz(id);
        let (backend, name) = (video.backend(), video.name(id));
        let levels = video.levels(id);
        self.choose_font();
        let Ok(face) = self.face(self.font_size.max(1)) else {
            log::error!("cannot load the font for display {name}");
            return;
        };
        let font = self.font_at(face);
        let mut view = View::new(
            id,
            font,
            size,
            refresh,
            RenderConfig {
                depth: depth(&self.config, levels),
                ..self.render_config()
            },
            self.measure(),
        );
        if self.config.switch(&keys::SOFT_CURSOR) {
            view.pointer = crate::pointer::Pointer::software();
        }
        self.views.insert((node.clone(), id), view);
        if !self.relayout() {
            self.views.remove(&(node.clone(), id));
            return;
        }
        if let Some(v) = self.views.get(&(node.clone(), id)) {
            let cell = v.renderer.font().cell();
            log::info!(
                "Display {name} with backend [{backend}] text renderer [bbulk], cell size {}x{}",
                cell.width,
                cell.height
            );
        }
    }

    /// Sets every display's font and the grid from the base height, as
    /// [`layout::layout`] decides, and the terminal and the pty at the
    /// grid. Returns false, changing nothing, when the base font cannot be
    /// found.
    fn relayout(&mut self) -> bool {
        let sizes: Vec<(u32, u32)> = self.views.values().map(|v| v.size).collect();
        if sizes.is_empty() {
            return true;
        }
        self.choose_font();
        if self.auto_size {
            let w = sizes.iter().map(|s| s.0).min().unwrap_or(0);
            let h = sizes.iter().map(|s| s.1).min().unwrap_or(0);
            self.font_size = self.chosen().auto_height(w, h, &self.locale);
        }
        self.font_size = self.chosen().snap(self.font_size);
        let base = match font::select(self.font_size, self.has_font, |h| self.face(h)) {
            Ok((face, h)) => {
                if !self.has_font {
                    for (range, n) in vtium_font::Coverage::new(|r| face.coverage(r)).partial() {
                        crate::warn::warn(format_args!(
                            "the font covers {} of {} characters of {range:?}; that range draws the sprites unless glyph-source says otherwise",
                            n.covered, n.total
                        ));
                    }
                }
                self.has_font = true;
                self.step = face.size_step();
                h
            }
            Err(e) => {
                log::error!("cannot load the font: {e}");
                return false;
            }
        };
        self.font_size = base;
        let name = self.chosen().name(base, &self.locale).to_owned();
        self.term.terminal.set_font_name(&name);
        let cell = |h: u32| {
            self.face(h).ok().map(|f| {
                let c = f.cell();
                (c.width, c.height)
            })
        };
        let turned = matches!(self.orientation, Orientation::Right | Orientation::Left);
        let Some(l) = layout::layout(&sizes, turned, base, self.step, cell) else {
            return true;
        };
        self.cell = cell(base).unwrap_or(self.cell);
        let config = self.render_config();
        let heights: Vec<u32> = l.heights.clone();
        let keys: Vec<(PathBuf, DisplayId)> = self.views.keys().cloned().collect();
        let refont = std::mem::take(&mut self.refont);
        for (key, height) in keys.into_iter().zip(heights) {
            let stale = self
                .views
                .get(&key)
                .is_some_and(|v| refont || v.height != height || v.orientation != self.orientation);
            if !stale {
                continue;
            }
            let Ok(face) = self.face(height) else {
                continue;
            };
            let font = self.font_at(face);
            if let Some(v) = self.views.get_mut(&key) {
                v.rebuild(font, height, config);
            }
        }
        self.resize(l.grid);
        self.ask_glyphs();
        true
    }

    /// The characters the displays show, sorted, each once: read from the
    /// view each buffer draws, scrolled back or not, every character of a
    /// cluster among them (#149).
    fn visible_chars(&self) -> Vec<char> {
        let palette = self.term.terminal.palette();
        let frame = self.term.terminal.screen().frame(palette, Age::NEVER);
        let mut chars: Vec<char> = Vec::new();
        for row in 0..frame.rows() {
            for cell in frame.cells(row) {
                match cell.glyph {
                    Glyph::Char(c) => chars.push(c),
                    Glyph::Cluster(text) => chars.extend(text.chars()),
                    Glyph::Empty => {}
                }
            }
        }
        chars.retain(|c| !c.is_whitespace());
        chars.sort_unstable();
        chars.dedup();
        chars
    }

    /// While the font is a frit, fills each cell a display draws at and
    /// that was not filled yet, the screen's characters first, and asks for
    /// the screen's characters there; drops the glyphs of cells no display
    /// draws at any more (ADR-1301 D2, the operator's fill model).
    fn ask_glyphs(&mut self) {
        if !matches!(self.font, Some(Chosen::Frit { .. })) {
            return;
        }
        let mut cells: Vec<vtium_font::Cell> = self
            .views
            .values()
            .map(|v| v.renderer.font().cell())
            .collect();
        cells.sort_unstable_by_key(|c| (c.height, c.width));
        cells.dedup();
        let now = (self.clock)();
        if let Some(helper) = &mut self.helper {
            helper.retain(&cells, now);
        }
        self.asked.retain(|c| cells.contains(c));
        let new: Vec<vtium_font::Cell> = cells
            .into_iter()
            .filter(|c| !self.asked.contains(c))
            .collect();
        if new.is_empty() {
            return;
        }
        let visible = self.visible_chars();
        let Some(helper) = self.helper() else {
            return;
        };
        for &cell in &new {
            helper.ask(cell, &visible, now);
            helper.fill(cell, &visible, now);
        }
        self.asked.extend(new);
    }

    /// While the font is a frit and the screen changed, asks for each
    /// character it shows that the store does not hold, and marks the sets
    /// that hold the others as used.
    fn ask_visible(&mut self, now: Instant) {
        let Some(Chosen::Frit { generated, .. }) = &self.font else {
            return;
        };
        let generated = Rc::clone(generated);
        if self.helper.is_none() {
            return;
        }
        let visible = self.visible_chars();
        let cells = self.asked.clone();
        for cell in cells {
            generated.borrow_mut().touch(cell, &visible);
            if let Some(helper) = &mut self.helper {
                helper.ask(cell, &visible, now);
            }
        }
    }

    /// The helper, started on the first need with a wake-up into the loop
    /// for its answers and the store its glyphs go into; `None`, logged
    /// once, when it cannot be.
    fn helper(&mut self) -> Option<&mut Helper> {
        let Some(Chosen::Frit {
            path, generated, ..
        }) = &self.font
        else {
            return None;
        };
        let (path, generated) = (path.clone(), Rc::clone(generated));
        if self.helper.is_none() && !self.helper_refused {
            let command = self
                .font_sources
                .helper
                .clone()
                .map_or_else(helper::Command::system, Ok);
            let wake = vtium_platform::calloop::ping::make_ping()
                .map_err(|e| format!("cannot make its wake-up: {e}"))
                .and_then(|(ping, source)| {
                    self.handle
                        .insert_source(source, |(), (), _: &mut App| {})
                        .map_err(|e| format!("cannot watch its wake-up: {e}"))?;
                    Ok(ping)
                });
            match command.and_then(|c| wake.map(|w| (c, w))) {
                Ok((command, ping)) => {
                    let wake = std::sync::Mutex::new(ping);
                    self.helper = Some(Helper::new(
                        command,
                        std::sync::Arc::new(move || {
                            if let Ok(p) = wake.lock() {
                                p.ping();
                            }
                        }),
                        self.epoch,
                        path,
                        self.console_font.clone(),
                        generated,
                        crate::deferred::store_limit(physical_memory()),
                    ));
                }
                Err(why) => {
                    log::warn!(
                        "the font's glyphs are not generated ({why}): vtium draws its built-in font"
                    );
                    self.helper_refused = true;
                }
            }
        }
        self.helper.as_mut()
    }

    /// Takes the helper's answers: each display at a set's cell redraws
    /// the cells that show one of its characters.
    fn helper_turn(&mut self, now: Instant) {
        if self.term.changed {
            self.ask_visible(now);
        }
        let Some(helper) = &mut self.helper else {
            return;
        };
        let answers = helper.poll(now);
        if answers.is_empty() {
            return;
        }
        // Arrival is damage, as program output is: the answers since the
        // last turn merge into one set per cell, which the next frame draws
        // once, and only the cells showing their characters draw.
        let visible = self.visible_chars();
        let mut damage: Vec<(vtium_font::Cell, Vec<char>)> = Vec::new();
        for Answer { cell, face } in answers {
            let shown = visible.iter().copied().filter(|&c| face.has_glyph(c));
            match damage.iter_mut().find(|(at, _)| *at == cell) {
                Some((_, chars)) => chars.extend(shown),
                None => damage.push((cell, shown.collect())),
            }
        }
        for (cell, mut chars) in damage {
            chars.sort_unstable();
            chars.dedup();
            if chars.is_empty() {
                continue;
            }
            for v in self.views.values_mut() {
                if v.renderer.font().cell() == cell {
                    v.renderer.glyphs_changed(&chars);
                }
            }
            self.draw_now = true;
        }
    }

    /// The terminal and the pty at `grid`, the pointer within it.
    fn resize(&mut self, grid: (usize, usize)) {
        let (cw, ch) = self.cell;
        let px = |n: usize, c: u32| u32::try_from(n).unwrap_or(u32::MAX).saturating_mul(c);
        if let Some(input) = &mut self.input {
            input.set_pointer_max(px(grid.0, cw), px(grid.1, ch));
        }
        self.term.terminal.set_cell_size(cw, ch);
        self.term
            .terminal
            .set_display_size(px(grid.0, cw), px(grid.1, ch));
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

    /// The terminal starts drawing and blinking.
    fn activate(&mut self, now: Instant) {
        self.awake = true;
        self.set_visible(true);
        if let Some(arms) = self.term.blink.activate() {
            for (timer, arm) in arms {
                match timer {
                    Timer::Text => self.text_due = Some(after(now, arm.first)),
                    Timer::Cursor => self.cursor_due = Some((after(now, arm.first), arm.every)),
                }
            }
        }
        if self.measure() {
            self.timer_due = Some(after(now, Duration::from_secs(1)));
        }
        for v in self.views.values_mut() {
            v.lost = true;
        }
        self.draw_now = true;
    }

    /// The terminal stops drawing and blinking.
    fn deactivate(&mut self) {
        self.awake = false;
        self.set_visible(false);
        self.text_due = None;
        self.cursor_due = None;
        self.timer_due = None;
    }

    /// The console is potentially visible while it draws: not while the
    /// VT is another's or the displays are blanked. Mode 2033's report
    /// goes out at once, to a program that may be waiting in a read.
    fn set_visible(&mut self, visible: bool) {
        self.term.terminal.set_visible(visible);
        self.send_user_bytes();
    }

    /// The blank is due: every display powers down. With none, the
    /// countdown starts again.
    fn blank_now(&mut self, now: Instant) {
        if self.views.is_empty() {
            self.blank.input(now);
            return;
        }
        for video in self.videos.values_mut() {
            power_down(video.as_mut());
        }
        self.deactivate();
    }

    /// The user's input: a key press or a pointer action (a move, a
    /// button or the wheel), or a device's wake-up but a re-take's, which
    /// follows the user's VT switch. It restarts
    /// the blank's countdown, as a display change does for now; the idle
    /// lock (vtium/vtium#87) will count the user's input alone.
    fn user_input(&mut self, now: Instant) {
        self.restart_blank(now);
    }

    /// What a program asked of the blank with the Linux console's
    /// sequences, taken under `program-blank`: a timeout of minutes, or
    /// `dpms-timeout` again after RIS, and a wake, which a key or pointer
    /// event would give. A timeout lasts until changed (the operator's
    /// decisions on vtium/vtium#79).
    fn blank_request(&mut self, request: BlankRequest, now: Instant) {
        let timeout = request.timeout.map(|t| match t {
            BlankTimeout::Minutes(m) => Some(Duration::from_secs(60 * u64::from(m))),
            BlankTimeout::Configured => self.dpms_timeout,
        });
        if let Some(timeout) = timeout {
            self.blank.set_timeout(timeout, now);
        }
        if request.wake {
            self.wake(now);
        }
    }

    /// A blank or wake from outside the session: the blank the timer runs,
    /// at once, or the wake a program's `CSI 13 ]` gives. While the VT is
    /// another's its displays are not vtium's to blank, and the blank is
    /// dropped.
    fn outside_order(&mut self, order: Order, now: Instant) {
        match order {
            Order::Blank if !self.vt.is_active() => {
                log::debug!("the VT is another's: the blank sent is dropped");
            }
            Order::Blank => {
                if self.blank.blank_now() {
                    self.blank_now(now);
                }
            }
            Order::Wake => self.wake(now),
        }
    }

    /// The blank's countdown starts again and a blank in force ends. While
    /// the VT is not ours nothing is woken: the acquire sets every device
    /// up.
    fn wake(&mut self, now: Instant) {
        if self.vt.is_active() {
            self.restart_blank(now);
        } else {
            self.blank.input(now);
        }
    }

    /// The blank's countdown starts again, and a blank in force ends:
    /// every device unblanks, and one a re-take owes is set up again.
    fn restart_blank(&mut self, now: Instant) {
        if self.blank.input(now) {
            let retake = std::mem::take(&mut self.retake_owed);
            for (node, video) in &mut self.videos {
                if let Err(e) = video.unblank() {
                    log::warn!("cannot unblank: {e}");
                }
                if retake {
                    if let Err(e) = video.wake_up() {
                        log::warn!("cannot set up {} again: {e}", node.display());
                    }
                }
            }
            if self.vt.is_active() {
                self.activate(now);
            }
        }
    }

    fn vt_event(&mut self, event: VtEvent, now: Instant) {
        match event {
            VtEvent::Acquired => {
                // The wake-up sets every device up, an owed re-take's too,
                // and each `Awake` it brings counts as activity.
                self.retake_owed = false;
                self.retaking.clear();
                if let Some(input) = &mut self.input {
                    input.wake_up();
                }
                for (node, video) in &mut self.videos {
                    if let Err(e) = video.wake_up() {
                        log::warn!("cannot wake {}: {e}", node.display());
                    }
                }
                self.wait.shown(now);
                self.activate(now);
            }
            VtEvent::Release => {
                // The acquire sets every device up: no re-take is owed
                // to the VT that is not ours.
                self.retake_owed = false;
                // The drag's release reaches the VT that is ours next.
                self.pointer.drag = None;
                self.deactivate();
                if let Some(input) = &mut self.input {
                    input.sleep();
                }
                self.releasing = Some(self.videos.keys().cloned().collect());
                for video in self.videos.values_mut() {
                    video.sleep();
                }
            }
            VtEvent::Retaken if self.blank.blanked() => {
                // As a hotplug while blanked: the set-up waits for the
                // input that ends the blank, and the displays stay dark
                // (the operator's decision on vtium/vtium#21).
                self.retake_owed = true;
            }
            VtEvent::Retaken => {
                for (node, video) in &mut self.videos {
                    self.retaking.insert(node.clone());
                    if let Err(e) = video.wake_up() {
                        log::warn!("cannot set up {} again: {e}", node.display());
                    }
                }
                for v in self.views.values_mut() {
                    v.lost = true;
                }
                self.draw_now = true;
            }
            VtEvent::Hangup => self.request_exit(Exit::Hangup),
        }
    }

    fn video_event(&mut self, node: &PathBuf, event: Event, now: Instant) {
        match event {
            Event::Added(id) => self.add_display(node, id),
            Event::Removed(id) => {
                self.views.remove(&(node.clone(), id));
                self.relayout();
            }
            Event::Redraw(id) => {
                if let Some(v) = self.views.get_mut(&(node.clone(), id)) {
                    v.lost = true;
                }
                self.draw_now = true;
            }
            // A frame waiting on this flip is drawn at the turn's end,
            // as a pending view is; the other displays keep their own
            // coalescing.
            // A frame on screen ends the start's wait.
            Event::Flipped(id) => {
                if let Some(v) = self.views.get_mut(&(node.clone(), id)) {
                    self.presented = true;
                    if let Some(t) = v.timer.as_mut() {
                        t.flip();
                    }
                }
            }
            // A re-take's set-up keeps the blank and its timer: a retry
            // that lands after the blank powers the displays down again.
            Event::Awake if self.retaking.remove(node) => {
                if self.blank.blanked() {
                    if let Some(video) = self.videos.get_mut(node) {
                        power_down(video.as_mut());
                    }
                }
            }
            Event::Awake => self.user_input(now),
            Event::Asleep => {
                if let Some(r) = &mut self.releasing {
                    r.remove(node);
                }
            }
            Event::WakeFailed => {
                self.retaking.remove(node);
                log::warn!(
                    "{}: the displays did not wake; the next key tries again",
                    node.display()
                );
            }
        }
    }

    /// The first display came, the last went, or the next came. With none
    /// the session runs on at the grid it had, which no display resizes,
    /// and the blank does not run: the blank in force ends, and the
    /// countdown starts afresh when a display comes, as at the user's
    /// input for now (vtium/vtium#87 tells the two apart). The next is
    /// drawn whole, and resizes the grid when its own differs.
    fn attachment(&mut self, now: Instant) {
        let attached = if self.views.is_empty() {
            match self.attached {
                Attached::NotYet => Attached::NotYet,
                Attached::Yes | Attached::Gone => Attached::Gone,
            }
        } else {
            Attached::Yes
        };
        let (cols, rows) = self.grid;
        match (self.attached, attached) {
            (Attached::Yes, Attached::Gone) => {
                log::warn!(
                    "the last display is gone: the session runs on at {cols}x{rows} until the next comes"
                );
                self.wake(now);
            }
            (Attached::Gone, Attached::Yes) => {
                log::info!("a display came: the session is drawn again at {cols}x{rows}");
                self.wake(now);
            }
            (Attached::NotYet, Attached::Yes) => self.wake(now),
            _ => {}
        }
        self.attached = attached;
    }

    /// A device of the seat came, went or changed. A framebuffer a DRM
    /// driver backs is skipped for its card.
    fn device_event(&mut self, event: DeviceEvent) {
        match event {
            DeviceEvent::Added(dev) => match dev.kind {
                DeviceKind::Input => {
                    if let Some(input) = &mut self.input {
                        input.add(&dev.node);
                    }
                }
                DeviceKind::Fbdev if dev.flags.drm_backed => {
                    log::debug!("{}: backed by a DRM card, skipped", dev.node.display());
                }
                kind => {
                    if let Err(e) = open_video(self, &dev.node, kind, dev.id_path.as_deref()) {
                        log::warn!("cannot open {}: {e}", dev.node.display());
                    }
                }
            },
            DeviceEvent::Removed { kind, node } => match kind {
                DeviceKind::Input => {
                    if let Some(input) = &mut self.input {
                        if let Some(id) = input.find(&node) {
                            input.remove(id);
                        }
                    }
                }
                DeviceKind::Drm | DeviceKind::Fbdev => self.remove_video(&node),
            },
            DeviceEvent::Hotplug { node, .. } => {
                if let Some(video) = self.videos.get_mut(&node) {
                    if let Err(e) = video.hotplug() {
                        log::warn!("{}: hotplug: {e}", node.display());
                    }
                }
            }
        }
    }

    /// A key press or pointer event from the input devices. A key press
    /// is the user's activity; a pointer event is when
    /// [`App::pointer_event`] says so.
    pub fn input_event(&mut self, event: InputEvent, now: Instant) {
        match event {
            InputEvent::Key(key) => {
                self.activity(now);
                self.key(&key, now);
            }
            InputEvent::Pointer(p) => {
                let mods = self.input.as_ref().map_or(InputMods::empty(), |i| i.mods());
                self.pointer_event(p, mods, now);
            }
        }
    }

    /// A pointer event, with the modifiers held on the keyboards as it
    /// came. A move, a button or the wheel is the user's activity; the
    /// end of a frame and the hide are vtium's own, as the C reference
    /// counted them (`src/seat.c:578-590` at kmscon b1f7a7c5).
    pub fn pointer_event(&mut self, event: PointerEvent, mods: InputMods, now: Instant) {
        match event {
            PointerEvent::Moved { .. } | PointerEvent::Button { .. } | PointerEvent::Wheel(_) => {
                self.activity(now);
            }
            PointerEvent::Sync | PointerEvent::Hide => {}
        }
        self.pointer(event, Mods::from_bits(mods.bits()));
    }

    /// The user's input reaches every device, which retries a failed
    /// wake-up, and the blank.
    fn activity(&mut self, now: Instant) {
        for video in self.videos.values_mut() {
            if let Err(e) = video.input() {
                log::warn!("cannot wake the displays: {e}");
            }
        }
        self.user_input(now);
    }

    /// The key order: the VT's keys, the seat's, the terminal's grabs,
    /// DECARM, the encoder. Each key that passes the seat's clears the
    /// selection, as the C reference's did (`src/terminal.c:1370` at
    /// kmscon b1f7a7c5).
    fn key(&mut self, key: &KeyEvent, now: Instant) {
        if self.vt.key(key) || !self.awake {
            return;
        }
        let bits = key.mods.bits();
        if let Some(&(power_key, _)) = self
            .power_keys
            .iter()
            .find(|(_, chords)| grab::matches(chords, bits, &key.keysyms))
        {
            // One action a press: a held chord's repeats would ask again,
            // and systemd takes seven Ctrl+Alt+Del in two seconds as an
            // immediate reboot.
            if key.repeat {
                return;
            }
            if let Some(child) = power::press(power_key, self.power.as_mut(), self.sleeping.get()) {
                let sleeping = Rc::clone(&self.sleeping);
                if let Err(e) = vtium_platform::power::watch(&self.handle, child, sleeping) {
                    log::error!("grab-suspend: cannot watch the write to /sys/power/state: {e}");
                }
            }
            return;
        }
        self.term.terminal.screen_mut().clear_selection();
        if grab::matches(&self.paste_keys, bits, &key.keysyms) {
            // One paste a press, however long the key repeats.
            if !key.repeat {
                self.paste();
                self.send_user_bytes();
            }
            self.draw_now = true;
            return;
        }
        if let Some(g) = grab::grab(bits, &key.keysyms) {
            let screen = self.term.terminal.screen_mut();
            let page = self.grid.1.saturating_sub(1).max(1);
            match g {
                Grab::ScrollUp => screen.scroll_view_up(1),
                Grab::ScrollDown => screen.scroll_view_down(1),
                Grab::PageUp => screen.scroll_view_up(page),
                Grab::PageDown => screen.scroll_view_down(page),
                Grab::ZoomIn | Grab::ZoomOut => {
                    let dir = if g == Grab::ZoomIn {
                        Zoom::In
                    } else {
                        Zoom::Out
                    };
                    self.zoom(dir);
                }
                Grab::RotateCw | Grab::RotateCcw => {
                    self.orientation = layout::rotate(self.orientation, g == Grab::RotateCw);
                    self.relayout();
                }
            }
            self.draw_now = true;
            return;
        }
        if key.repeat && !self.term.terminal.auto_repeat() {
            return;
        }
        let Some(&keysym) = key.keysyms.first() else {
            return;
        };
        let unicode = key.codepoints.first().copied().flatten();
        let mods = Mods::from_bits(key.mods.bits());
        if self.term.terminal.key(keysym, key.ascii, mods, unicode) {
            self.send_user_bytes();
            self.term.terminal.screen_mut().reset_view();
            if self.hide_while_typing {
                self.pointer.visible = false;
            }
        }
        let rate = self
            .term
            .terminal
            .cursor_blink_ms()
            .map_or(crate::blink::TEXT_PERIOD, |ms| {
                Duration::from_millis(ms.into())
            });
        if let Some(arm) = self.term.blink.key(rate) {
            self.cursor_due = Some((after(now, arm.first), arm.every));
        }
        self.draw_now = true;
    }

    /// A zoom: the base height the next of the set's strikes and their
    /// whole multiples, larger or smaller, which ends `font-size=0`'s
    /// following of the display (ADR-1200 D9). Below the smallest it
    /// stops, with a line naming the set and its smallest height. A
    /// height with no font keeps the one in use.
    fn zoom(&mut self, dir: Zoom) {
        self.choose_font();
        let chosen = self.chosen();
        let Some(size) = font::zoom(&chosen.rungs(&self.locale), self.font_size, dir) else {
            if dir == Zoom::Out {
                let smallest = chosen.smallest(&self.locale);
                log::info!(
                    "the set {} draws no smaller than {smallest} pixels: zoom stops there",
                    chosen.name(smallest, &self.locale)
                );
            }
            return;
        };
        let (old, auto) = (self.font_size, self.auto_size);
        self.auto_size = false;
        self.font_size = size;
        if !self.relayout() {
            self.font_size = old;
            self.auto_size = auto;
        }
    }

    fn send_user_bytes(&mut self) {
        let bytes = self.term.terminal.take_replies();
        if let Some(pty) = &mut self.pty {
            if !bytes.is_empty() {
                pty.write(&bytes);
            }
        }
    }

    /// A pointer event: reported to a program that tracks the pointer,
    /// unless Shift is held, as xterm's `ShiftOverride` lets it select,
    /// paste and scroll (`button.c`); otherwise it selects, copies, pastes
    /// and scrolls. A button keeps to the side its press took until its
    /// release, whatever Shift does meanwhile.
    fn pointer(&mut self, event: PointerEvent, mods: Mods) {
        let tracking = self.term.terminal.mouse_tracking() != MouseTracking::Off;
        let shift = mods.contains(Mods::SHIFT);
        let moved = matches!(event, PointerEvent::Moved { .. });
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
                    self.select_button(button, pressed, double_click);
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
                    Wheel::Zoom(dir) => self.zoom(dir),
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
    }

    /// A change of the mouse tracking the program asked for clears the
    /// selection, as the C reference's did (`src/terminal.c:890-897` at
    /// kmscon b1f7a7c5).
    fn tracking_changed(&mut self) {
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
    fn select_button(&mut self, button: Button, pressed: bool, double_click: bool) {
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
                        self.copy();
                    }
                    _ => {
                        screen.select_line(col, row);
                        self.copy();
                    }
                }
            }
            (Button::Left, false) => match self.pointer.drag.take() {
                Some(Drag { moved: true, .. }) => self.copy(),
                Some(Drag { moved: false, .. }) => {
                    self.term.terminal.screen_mut().clear_selection();
                }
                None => {}
            },
            (Button::Middle, true) => {}
            (Button::Middle, false) => {
                self.paste();
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
    fn copy(&mut self) {
        if let Some(text) = self
            .term
            .terminal
            .screen()
            .selection_text()
            .filter(|t| !t.is_empty())
        {
            self.selection = Some(text.into_bytes());
        }
    }

    /// Pastes the selection buffer to the program, filtered, and
    /// bracketed while it asks (`Terminal::paste`).
    fn paste(&mut self) {
        if let Some(text) = &self.selection {
            self.term.terminal.paste(text);
        }
    }

    /// The child exited: start it again, or end vtium.
    fn child_exited(&mut self, now: Instant) {
        let Some(status) = self.term.exited.take() else {
            return;
        };
        log::info!("child exited: {status}");
        // The pty closes with what is queued for the child.
        self.pty = None;
        let oneshot = self.oneshot;
        let since = now.saturating_duration_since(self.epoch);
        let next = self
            .respawn
            .as_mut()
            .map_or(Next::Restart, |r| r.exited(since, oneshot));
        match next {
            Next::Restart => {
                // A new session starts from a hard reset (RIS), drawn at
                // once, with nothing of the last session's output left
                // (`terminal_close` at kmscon b1f7a7c5).
                self.term.terminal.input(b"\x1bc");
                self.term.terminal.take_replies();
                self.term.unparsed = false;
                self.draw_now = true;
                self.spawn();
            }
            Next::Oneshot => self.request_exit(Exit::Oneshot),
            Next::TooQuick => {
                log::error!("the session exited too quickly too many times running");
                self.request_exit(Exit::TooQuick);
            }
        }
    }

    fn draw_state(&self) -> DrawState {
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

    /// One turn's end: every source drained, the timers due run, and the
    /// displays drawn where something asks.
    pub fn turn(&mut self, now: Instant) {
        let events: Vec<VtEvent> = self.vt.events().collect();
        for e in events {
            self.vt_event(e, now);
        }
        let devices: Vec<DeviceEvent> = self
            .udev
            .as_mut()
            .map(|u| u.events().collect())
            .unwrap_or_default();
        for e in devices {
            self.device_event(e);
        }
        let nodes: Vec<PathBuf> = self.videos.keys().cloned().collect();
        for node in nodes {
            let events = self
                .videos
                .get_mut(&node)
                .map(|v| v.events())
                .unwrap_or_default();
            for e in events {
                self.video_event(&node, e, now);
            }
        }
        self.attachment(now);
        self.first_start(now);
        if let Some(r) = &self.releasing {
            if r.is_empty() {
                self.releasing = None;
                if let Err(e) = self.vt.acknowledge_release() {
                    log::warn!("cannot let the VT go: {e}");
                }
            }
        }
        let inputs: Vec<InputEvent> = self
            .input
            .as_mut()
            .map(|i| i.events().collect())
            .unwrap_or_default();
        self.tracking_changed();
        for e in inputs {
            self.input_event(e, now);
        }
        // The console has no system clipboard: a selection OSC 52 set is
        // held for a paste, or dropped under `no-clipboard-write`.
        if let Some(selection) = self.term.terminal.take_clipboard_write() {
            if self.clipboard_write {
                self.selection = Some(selection);
            }
        }
        // The terminal takes OSC 50 only under `allow-font-ops`.
        if let Some(name) = self.term.terminal.take_font_request() {
            self.switch_font(&name);
        }
        // After the exit, whose respawn's RIS returns the blank to
        // `dpms-timeout` for the next session.
        self.child_exited(now);
        if let Some(request) = self.term.terminal.take_blank_request() {
            if self.program_blank {
                self.blank_request(request, now);
            }
        }
        if let Some(order) = self.order.take() {
            self.outside_order(order, now);
        }
        self.timers(now);
        self.helper_turn(now);
        if self.awake {
            self.draw(now);
        }
        self.report(now);
        self.term.coalesce.turn_end();
        self.term.changed = false;
        let flipping = self
            .views
            .iter()
            .any(|((node, _), v)| self.videos.get(node).is_some_and(|d| d.is_flipping(v.id)));
        if let Some(wait) = self.wait.turn(now, self.presented, flipping) {
            log::error!("no display appeared within {} s", wait.as_secs());
            self.request_exit(Exit::NoDisplay);
        }
        self.exiting(now);
    }

    fn timers(&mut self, now: Instant) {
        if let Some(due) = self.text_due.filter(|&d| d <= now) {
            self.draw_now |= self.term.blink.fired(Timer::Text, self.awake);
            self.text_due = Some(after(due, crate::blink::TEXT_PERIOD));
        }
        if let Some((due, every)) = self.cursor_due.filter(|&(d, _)| d <= now) {
            self.draw_now |= self.term.blink.fired(Timer::Cursor, self.awake);
            self.cursor_due = Some((after(due, every), every));
        }
        if self.term.sync.deadline().is_some_and(|d| d <= now) {
            let action = self
                .term
                .sync
                .timer(&mut self.term.terminal, now, self.term.unparsed);
            self.term.act(action);
            if action == Action::Expired {
                for v in self.views.values_mut() {
                    if let Some(t) = &mut v.timer {
                        t.expired();
                    }
                }
            }
        }
        if self.blank.fire(now) {
            self.blank_now(now);
        }
    }

    fn draw(&mut self, now: Instant) {
        let state = self.draw_state();
        let present = std::mem::take(&mut self.term.present);
        let now_all = std::mem::take(&mut self.draw_now) || present;
        let changed = self.term.changed;
        let holding = self.term.sync.holding(&self.term.terminal);
        let base = self.cell;
        let App {
            views,
            videos,
            term,
            clock,
            ..
        } = self;
        for ((node, _), view) in views.iter_mut() {
            let Some(video) = videos.get_mut(node) else {
                continue;
            };
            let due = view.wait.due().is_some_and(|d| d <= now);
            if due {
                view.wait.end();
            }
            let owed = view.owed && !holding;
            let wants = now_all
                || view.lost
                || owed
                || due
                || (view.pending && !video.is_flipping(view.id));
            if !wants {
                if !changed {
                    continue;
                }
                if term
                    .coalesce
                    .decide(holding, &mut view.wait, term.unparsed, now, view.refresh)
                    .is_some()
                {
                    continue;
                }
            }
            if holding && !view.lost {
                if !view.owed {
                    if let Some(t) = &mut view.timer {
                        t.held();
                    }
                }
                view.owed = true;
                continue;
            }
            // The pointer is kept in the base cell's pixels; a display
            // drawing larger cells draws it where its own cells put it.
            let c = view.renderer.font().cell();
            let state = DrawState {
                pointer: state
                    .pointer
                    .map(|(x, y)| (scale(x, c.width, base.0), scale(y, c.height, base.1))),
                ..state
            };
            view.draw(term, video.as_mut(), &state, &**clock);
        }
    }

    /// The render timer's line, once a second.
    fn report(&mut self, now: Instant) {
        let Some(due) = self.timer_due.filter(|&d| d <= now) else {
            return;
        };
        self.timer_due = Some(after(due, Duration::from_secs(1)));
        for ((node, id), view) in &mut self.views {
            let px = u64::from(view.size.0).saturating_mul(u64::from(view.size.1));
            if let Some(report) = view
                .timer
                .as_mut()
                .and_then(|t| t.report(Duration::from_secs(1), px))
            {
                log::info!(target: "terminal", "{}", render_timer_line(&report, node, *id));
            }
        }
    }

    fn exiting(&mut self, now: Instant) {
        if self.exit.is_none() {
            return;
        }
        match self.leaving {
            None => {
                self.deactivate();
                self.pty = None;
                self.leaving = Some(match self.vt.leave() {
                    Ok(Switch::Pending) => Leaving::Until(after(now, LEAVE_WAIT)),
                    Ok(Switch::Done) => Leaving::Done,
                    Err(e) => {
                        log::warn!("cannot switch back: {e}");
                        Leaving::Done
                    }
                });
            }
            Some(Leaving::Until(deadline)) => {
                if now >= deadline || !self.vt.is_active() && self.releasing.is_none() {
                    self.leaving = Some(Leaving::Done);
                }
            }
            Some(Leaving::Done) => {}
        }
    }

    /// The longest the loop may wait before the next turn: until the next
    /// timer the app keeps is due.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        let mut due: Vec<Instant> = Vec::new();
        due.extend(self.text_due);
        due.extend(self.cursor_due.map(|(d, _)| d));
        due.extend(self.timer_due);
        due.extend(self.wait.due());
        due.extend(self.term.sync.deadline());
        due.extend(self.blank.deadline());
        due.extend(self.views.values().filter_map(|v| v.wait.due()));
        due.extend(self.helper.as_ref().and_then(Helper::deadline));
        if let Some(Leaving::Until(d)) = self.leaving {
            due.push(d);
        }
        if self.leaving == Some(Leaving::Done) {
            return Some(Duration::ZERO);
        }
        due.into_iter()
            .min()
            .map(|d| d.saturating_duration_since(now))
    }

    /// The sprite policy and missing glyph of the first display's font.
    pub fn font_policy(&self) -> Option<(vtium_font::SpritePolicy, vtium_font::Missing)> {
        let font = self.views.values().next()?.renderer.font();
        Some((font.policy(), font.missing()))
    }

    /// Each display's font cell, in the order of the displays.
    pub fn cells(&self) -> Vec<(u32, u32)> {
        self.views
            .values()
            .map(|v| {
                let c = v.renderer.font().cell();
                (c.width, c.height)
            })
            .collect()
    }

    /// The terminal's grid.
    pub fn grid(&self) -> (usize, usize) {
        self.grid
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
}

/// Powers every display of `video` down.
fn power_down(video: &mut dyn Video) {
    for id in video.displays() {
        if let Err(e) = video.set_dpms(id, Dpms::Off) {
            log::warn!("cannot blank a display: {e}");
        }
    }
}

/// The render timer's log text for a display, after the `terminal:`
/// target: the figures, then `display=NODE[ID]` (tier 3's `t3_bench.py`
/// reads each `key=value`).
pub fn render_timer_line(
    report: &vtium_render::Report,
    node: &std::path::Path,
    id: DisplayId,
) -> String {
    format!("{report} display={}[{}]", node.display(), id.0)
}

/// The lines of scrollback a wheel event scrolls (`src/terminal.c:1648`).
const WHEEL_LINES: usize = 3;

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

/// `v`, a position in cells of `from` pixels, in cells of `to` pixels.
fn scale(v: u32, to: u32, from: u32) -> u32 {
    u64::from(v)
        .saturating_mul(u64::from(to))
        .checked_div(u64::from(from))
        .map_or(v, |s| u32::try_from(s).unwrap_or(u32::MAX))
}

/// `t` plus `d`, or `t` where the clock cannot hold it.
fn after(t: Instant, d: Duration) -> Instant {
    t.checked_add(d).unwrap_or(t)
}

fn to_usize(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// The locale in `env`, found as locale(7) says: the first of `LC_ALL`,
/// `LC_CTYPE` and `LANG` set and not empty, else `C`.
fn locale(env: &[(OsString, OsString)]) -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| {
            env.iter()
                .rev()
                .find(|(k, _)| k == name)
                .and_then(|(_, v)| v.to_str())
                .filter(|v| !v.is_empty())
        })
        .unwrap_or("C")
        .to_owned()
}

fn to_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The machine's memory in bytes, from `/proc/meminfo`; 0 when unread,
/// which takes the smallest scrollback cap.
fn physical_memory() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
            let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            kib.checked_mul(1024)
        })
        .unwrap_or(0)
}

/// Opens a device and wraps it as a video device in the loop, for
/// [`App::add_video`].
pub fn open_video(
    app: &mut App,
    node: &std::path::Path,
    kind: DeviceKind,
    id_path: Option<&str>,
) -> io::Result<()> {
    use vtium_platform::display::{Card, Drm2d, Fb, Fbdev, ModeLookup, ModePolicy, Wanted};
    let fd = app.vt.open_device(node)?;
    let key = node.to_owned();
    let video: Box<dyn Video> = match kind {
        DeviceKind::Drm => {
            let modes = app.modes.clone();
            let gpu = id_path.map(str::to_owned);
            let policy = ModePolicy {
                use_original: app.use_original,
                wanted: (!modes.is_empty()).then(|| {
                    Box::new(move |connector: &str| {
                        ModeSpec::find(&modes, gpu.as_deref(), connector).map(|m| Wanted {
                            width: m.width,
                            height: m.height,
                            rate: m.rate,
                        })
                    }) as Box<ModeLookup>
                }),
            };
            let mut drm = Drm2d::new(Card::new(fd)?, policy)?;
            let k = key.clone();
            drm.insert(&app.handle, move |app: &mut App| {
                app.videos
                    .get_mut(&k)
                    .and_then(|v| v.as_any().downcast_mut::<Drm2d<Card>>())
            })?;
            Box::new(drm)
        }
        DeviceKind::Fbdev => {
            let mut fb = Fbdev::new(Fb::new(fd));
            let k = key.clone();
            fb.insert(&app.handle, move |app: &mut App| {
                app.videos
                    .get_mut(&k)
                    .and_then(|v| v.as_any().downcast_mut::<Fbdev<Fb>>())
            });
            Box::new(fb)
        }
        DeviceKind::Input => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
    };
    app.add_video(key, video);
    Ok(())
}

/// A display's depth: `display-depth`, else, at `auto`, the levels the
/// device says it shows.
fn depth(config: &Config, levels: Option<u8>) -> Depth {
    match config.text(&keys::DISPLAY_DEPTH) {
        Some("1") => Depth::One,
        Some("gray4") => Depth::Gray(4),
        Some("gray16") => Depth::Gray(16),
        Some("color") => Depth::Color,
        _ => match levels {
            None => Depth::Color,
            Some(2) => Depth::One,
            Some(n) => Depth::Gray(n),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_depth_overrides_what_the_device_says() {
        let config = |args: &[&str]| {
            let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
            crate::config::load(&args, None, &mut |_: &std::path::Path| Ok(None)).config
        };
        for (args, levels, want) in [
            (&[][..], None, Depth::Color),
            (&[][..], Some(2), Depth::One),
            (&[][..], Some(16), Depth::Gray(16)),
            (&["--display-depth=auto"][..], Some(4), Depth::Gray(4)),
            (&["--display-depth=1"][..], None, Depth::One),
            (&["--display-depth=1"][..], Some(16), Depth::One),
            (&["--display-depth=gray4"][..], Some(2), Depth::Gray(4)),
            (&["--display-depth=gray16"][..], None, Depth::Gray(16)),
            (&["--display-depth=color"][..], Some(2), Depth::Color),
        ] {
            assert_eq!(depth(&config(args), levels), want, "{args:?} {levels:?}");
        }
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
