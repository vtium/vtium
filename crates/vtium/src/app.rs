//! The seat and its heads, wired: the VT's events, the video devices and
//! their displays, the input, the DPMS blank, each head's pty and child,
//! and when each head draws. The loop calls [`App::turn`] after each
//! dispatch with the time, and waits at most [`App::next_wake`].
//!
//! Each step follows the C reference's flow (docs/c-reference.md, parts 1,
//! 2 and 5) and vtium-platform's host list:
//!
//! - **Heads** (ADR-1401 D1). The head file's heads, or the compiled-in
//!   configuration's one `console` head, each a [`Head`] with its own
//!   session, grid, density, zoom and rotation. Each display walks the
//!   rules alone when it comes, and joins the head the first match names;
//!   a display the rules send `dark`, or none claims, is left off and
//!   logged, with the rule line that would claim it ready to paste. Within
//!   a head the displays mirror one grid by the scaled rule
//!   ([`crate::layout`]).
//! - **The VT.** `Acquired` wakes the input and every device, which draw
//!   whole; `Release` deactivates the terminals, puts the input and every
//!   device to sleep, and acknowledges once each reported `Asleep`;
//!   `Retaken` sets every device's displays up again, or while blanked
//!   at the input that ends the blank, and keeps the blank and its timer;
//!   `Hangup` exits.
//! - **Inputs** (D3, D5). Each input device walks the rules alone when it
//!   comes, by its identity: a device a rule sends to a head delivers to
//!   that head alone, one sent to `focus` or claimed by no rule delivers
//!   to the head holding the focus, and one sent to `ignore` is never
//!   opened, or opened for the rescue key alone. The focus starts on the
//!   head of the boot display, else the first head in the file with a
//!   display, starting again as each display lights until the first key
//!   reaching a head or the first focus chord, over every display lit
//!   since the start, one that left included; then only the focus chord
//!   moves it. The terminal of each head a keyboard delivers to has mode
//!   1004's focus.
//! - **Keys**, in order: the VT's, then the seat's (the power keys, which
//!   act on a press and never on its repeat), then the heads' chords
//!   ([`chord`]), then the terminal's grabs, then DECARM's drop of a
//!   repeat, then the encoder. A key draws at once and holds the cursor's
//!   blink.
//! - **The pointer.** It moves on the head it delivers to, within that
//!   head's grid. While the program tracks it, its events are reported,
//!   but a button pressed, a move without a button and the wheel with
//!   Shift held, as xterm's are. The others select, copy, paste and scroll
//!   ([`Head::pointer_event`]).
//! - **The blank** is the seat's. Each key press and pointer action (a
//!   move, a button or the wheel) goes to every device's `input`, which
//!   retries a failed wake-up, and to [`Blank::input`]; a device's `Awake`
//!   goes to [`Blank::input`] too, but a re-take's, and whenever it says
//!   the displays were blanked every device unblanks. A head's first
//!   display, its last going and its next after none count too, as a
//!   display change, which wakes nothing while the VT is not ours; another
//!   hotplug or a re-take never counts. The seat's clock drives the
//!   blank's countdown ([`App::next_wake`]). A signal from outside the
//!   session ([`crate::blank`]) blanks at once, or wakes as a program's
//!   `CSI 13 ]` does, at the turn's end.
//! - **The consoles** (D1). A head's consoles start once one of its
//!   displays is set up, on its grid ([`respawn::first_start_due`]); the
//!   console chords open, show and close them. A console's program exiting
//!   closes it while others stay open; the last's exit follows the respawn
//!   policy, which starts it again or ends the head, and vtium ends once
//!   every head has.
//! - **No display.** From the first acquire, vtium waits `display-wait`
//!   for a frame on a display's screen, and with none by then it ends
//!   ([`crate::wait`]). Once one has come, a head's last going does what
//!   its `on-loss` says (D8): under `nohup` its sessions run on at the
//!   grid they had, with the keys of the devices reaching it and no
//!   blank, until a display its rules match comes and is drawn whole;
//!   under `hangup` their ptys close at once, and under `hangup
//!   DURATION` once the time runs out with no display back, after which
//!   the head follows the respawn policy. A mirrored head keeps its
//!   sessions while one of its displays remains. A head whose display
//!   never lit receives nothing, and the focus never moves because a
//!   display left or came back.
//! - **Drawing** (D2). Each head's render thread composes its frames
//!   ([`crate::painter`]); the loop takes each frame from the screen,
//!   and at the next turn's start, after the VT's events, copies it into
//!   the display's buffer and presents it.
//! - **Exit.** A request (SIGTERM, SIGINT, a hangup, the policy) switches
//!   back to the VT shown at the start and serves the loop for up to
//!   [`LEAVE_WAIT`] while the switch is pending.

use std::any::Any;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium_config::OnLoss;
use vtium_platform::calloop::LoopHandle;
use vtium_platform::display::{Blank, DPI_ASSUMED, DisplayId, Dpms, Event};
use vtium_platform::input::{
    DeviceId, DeviceInput, InputEvent, KeyEvent, Mods as InputMods, PointerEvent,
};
use vtium_platform::pty::{Erase, Pty};
use vtium_platform::udev::{DeviceEvent, DeviceKind};
use vtium_platform::vt::{Switch, Vt, VtEvent};
use vtium_render::{Config as RenderConfig, Depth, DrawState, Orientation};
use vtium_term::{BlankRequest, BlankTimeout, Mods};

use crate::blank::Order;
use crate::blink::Timer;
use crate::child::{self, Display as SessionDisplay, Place};
use crate::config::keys;
use crate::config::{Config, ModeSpec};
use crate::font::{self, Chosen};
use crate::grab::{self, Chord, Grab};
use crate::head::chord::{self, HeadChord};
use crate::head::console::Console;
use crate::head::identity::{self, Card};
use crate::head::route::{self, Routes};
use crate::head::session::{self, AfterExit, Caps};
use crate::head::{Attached, Frit, Head, place_display};
use crate::helper::{self, Answer, Helper};
use crate::layout::{self, Zoom};
use crate::painter::Painter;
use crate::power::{self, Power, PowerKey};
use crate::respawn;
use crate::sync::Action;
use crate::term::{Clock, Prepared, Term, View};
use crate::video::Video;
use crate::wait::Wait;

/// How long the loop is served at exit while the switch back is pending
/// (`src/main.c:203-209` at kmscon b1f7a7c5).
pub const LEAVE_WAIT: Duration = Duration::from_millis(100);

/// Why vtium ends, and its exit status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// SIGTERM or SIGINT.
    Signal,
    /// A session's command exited under `oneshot`.
    Oneshot,
    /// A session's command exited quickly too many times running.
    TooQuick,
    /// The VT hung up.
    Hangup,
    /// A session's command could not be started.
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

/// Where the exit stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Leaving {
    /// The switch back is pending until this time.
    Until(Instant),
    Done,
}

/// A seat's VT, which the app holds behind one type: compiled once, and
/// given back as itself to the loop sources that need it, as the kernel
/// VT's signals do.
pub trait SeatVt: Vt + Any {
    fn as_any(&mut self) -> &mut dyn Any;
    fn as_vt(&self) -> &dyn Vt;
}

impl<T: Vt + Any> SeatVt for T {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn as_vt(&self) -> &dyn Vt {
        self
    }
}

impl dyn SeatVt {
    /// The VT as a `T`, when it is one.
    pub fn get<T: Vt + Any>(&mut self) -> Option<&mut T> {
        self.as_any().downcast_mut::<T>()
    }
}

/// The seat, its devices and its heads.
pub struct App {
    pub vt: Box<dyn SeatVt>,
    pub videos: BTreeMap<PathBuf, Box<dyn Video>>,
    pub input: Option<vtium_platform::input::Input>,
    pub udev: Option<vtium_platform::udev::Udev>,
    pub blank: Blank,
    /// The heads, in the head file's order.
    heads: Vec<Head>,
    /// The head file in force: the file's, or the compiled-in
    /// configuration.
    rules: vtium_config::Config,
    /// The head the inputs following the focus deliver to.
    focus: Option<usize>,
    /// Where each input device's events go.
    routes: Routes,
    /// The head the focus moved to since [`App::take_focus_move`].
    focus_move: Option<usize>,
    /// `focus-mark`: `Some(true)` always, `Some(false)` for
    /// [`crate::head::MARK_SHOWN`] after the focus moves, `None` never.
    focus_mark: Option<bool>,
    /// When the focus marks hide, under `focus-mark = auto`.
    mark_due: Option<Instant>,
    /// A key came or the focus chord moved the focus: the focus no longer
    /// follows a boot display lighting late.
    focus_settled: bool,
    /// A display lit since the focus was last placed.
    focus_due: bool,
    /// What the rules and the display guard read of each card.
    cards: BTreeMap<PathBuf, Card>,
    /// Each display lit in a head, as the rules saw it.
    lit: BTreeMap<(PathBuf, DisplayId), vtium_config::Identity>,
    /// Each display lit since the start until the focus settled, gone or
    /// not: the focus starts over them, so a display leaving never moves
    /// it (ADR-1401 D8).
    start_lit: BTreeMap<(PathBuf, DisplayId), vtium_config::Identity>,
    /// The displays a device announced that the rules leave dark, as the
    /// rules saw them.
    dark: BTreeMap<(PathBuf, DisplayId), vtium_config::Identity>,
    /// The display the display guard lit, and its head (ADR-1401 D9).
    guard_lit: Option<((PathBuf, DisplayId), usize)>,
    /// A device came or went: the guards run at the turn's end.
    guard_due: bool,
    /// The anti-lockout guard holds every keyboard on the focus.
    locked_out: bool,
    /// A display is lit, which a DRM card's dark rule reads: while none
    /// is, a display the rules leave dark is announced for the display
    /// guard to light.
    any_lit: Rc<Cell<bool>>,
    /// The scrollback cap and length each console takes.
    scrollback: (usize, usize),
    /// The scrollback memory each console takes.
    scrollback_bytes: usize,
    /// `head-consoles-max` and `consoles-max`.
    caps: Caps,
    config: Config,
    handle: LoopHandle<'static, App>,
    clock: Clock,
    epoch: Instant,
    /// [`App::start`] was called: each head's command starts at its first
    /// display.
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
    /// A frame has been on screen since the start.
    presented: bool,
    timer_due: Option<Instant>,
    /// The displays whose density is assumed, each warned of once.
    assumed: BTreeSet<(PathBuf, DisplayId)>,
    /// Where the font's sets come from.
    font_sources: font::Sources,
    /// Each frit a head draws, by its name, with its one helper.
    frits: BTreeMap<String, Frit>,
    /// The kernel's font, read from the VT at start for a frit's
    /// `console:` source (vtium/vtium#145).
    console_font: Option<vtium_platform::vt::font::ConsoleFont>,
    /// The locale, which keeps the built-in Unscii-8 from a zh, ja or ko
    /// locale (ADR-1200 D7).
    locale: String,
    exit: Option<Exit>,
    leaving: Option<Leaving>,
    inherited: Vec<(OsString, OsString)>,
    /// `oneshot`: a head's last console's exit ends the head.
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
    /// The seat's selection buffer: what the user copied last, or OSC 52
    /// set last, held for a paste.
    selection: Option<Vec<u8>>,
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
    /// The seat for `config` on `vt`, with the compiled-in configuration's
    /// heads ([`App::set_heads`] gives the head file's); each head's child
    /// starts at [`App::start`].
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
        let lines = vtium_term::sb_size(
            config
                .number(&keys::SB_SIZE)
                .map_or(vtium_term::SB_SIZE_DEFAULT, to_usize),
            physical_memory(),
            config.number(&keys::SB_SIZE_CAP).map(to_usize),
        );
        let rules = route::in_force(vtium_config::Config::builtin(), &config);
        let caps = Caps {
            head: config.number(&keys::HEAD_CONSOLES_MAX).map_or(4, to_usize),
            total: config.number(&keys::CONSOLES_MAX).map_or(8, to_usize),
        };
        let bytes = session::scrollback_bytes(&config, caps, rules.heads.len());
        let heads = session::heads(&rules, &config, ((cap, lines), bytes), caps, &clock);
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
            heads,
            rules,
            focus: None,
            routes: Routes::default(),
            focus_move: None,
            focus_mark: match config.text(&keys::FOCUS_MARK) {
                Some("always") => Some(true),
                Some("off") => None,
                _ => Some(false),
            },
            mark_due: None,
            focus_settled: false,
            focus_due: false,
            cards: BTreeMap::new(),
            lit: BTreeMap::new(),
            start_lit: BTreeMap::new(),
            dark: BTreeMap::new(),
            guard_lit: None,
            guard_due: false,
            locked_out: false,
            any_lit: Rc::default(),
            scrollback: (cap, lines),
            scrollback_bytes: bytes,
            caps,
            assumed: BTreeSet::new(),
            font_sources: font::Sources::default(),
            frits: BTreeMap::new(),
            console_font: None,
            locale: locale(&inherited),
            config,
            handle,
            clock,
            epoch: now,
            start_due: false,
            awake: false,
            retake_owed: false,
            retaking: BTreeSet::new(),
            releasing: None,
            wait,
            presented: false,
            timer_due: None,
            exit: None,
            leaving: None,
            inherited,
        }
    }

    /// Runs the head file `rules` in place of the compiled-in
    /// configuration, before any display comes: its heads, each with no
    /// session yet, and its rules for each display that comes.
    pub fn set_heads(&mut self, rules: vtium_config::Config) {
        let rules = route::in_force(rules, &self.config);
        for (on, name) in [
            (rules.guards.anti_lockout, "anti-lockout"),
            (rules.guards.rescue_key, "rescue-key"),
            (rules.guards.never_dark, "never-dark"),
        ] {
            if !on {
                log::warn!("the head file turns the {name} guard off");
            }
        }
        // The display guard's default head, where the file declares none
        // (D9).
        let mut declared = rules.clone();
        if declared.heads.is_empty() {
            declared.heads = vtium_config::Config::builtin().heads;
        }
        let caps = self.caps;
        self.scrollback_bytes = session::scrollback_bytes(&self.config, caps, declared.heads.len());
        let scrollback = (self.scrollback, self.scrollback_bytes);
        self.heads = session::heads(&declared, &self.config, scrollback, caps, &self.clock);
        self.rules = rules;
        self.focus = None;
        self.lit.clear();
        self.start_lit.clear();
        self.dark.clear();
        self.frits.clear();
    }

    /// Shows vtium's notice `text` on every head, before its login, until
    /// a key reaches the head or for [`crate::head::NOTICE_SHOWN`]
    /// (ADR-1401 D7).
    pub fn notice(&mut self, text: &str) {
        for head in &mut self.heads {
            head.add_notice(text);
        }
    }

    /// Puts the power keys' actions through `power`, a test's stand-in
    /// for PID 1, logind and the kernel.
    pub fn set_power(&mut self, power: Box<dyn Power>) {
        self.power = power;
    }

    /// Runs `hook` before each frame of the head `name` is composed, a
    /// test's stand-in for a head whose frames are slow to draw.
    pub fn set_draw_hook(&mut self, name: &str, hook: Option<crate::head::DrawHook>) {
        if let Some(head) = self.heads.iter_mut().find(|h| h.name() == name) {
            head.draw_hook = hook;
        }
    }

    /// Reads the font's sets from `sources` in place of the system's store
    /// and vconsole files: a test's stand-in. Each head's font is chosen
    /// again at its next display.
    pub fn set_font_sources(&mut self, sources: font::Sources) {
        self.font_sources = sources;
        for head in &mut self.heads {
            head.font = None;
        }
        self.frits.clear();
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

    /// The heads, in the head file's order.
    pub fn heads(&self) -> &[Head] {
        &self.heads
    }

    /// The head named `name`.
    pub fn head(&self, name: &str) -> Option<&Head> {
        self.heads.iter().find(|h| h.name() == name)
    }

    /// The head the keys type into, else the first: the one whose session
    /// [`App::grid`], [`App::screen_text`] and [`App::term`] read.
    fn shown(&self) -> Option<&Head> {
        self.heads.get(self.focus.unwrap_or(0))
    }

    /// Starts each head's command at the end of the first turn
    /// [`respawn::first_start_due`] allows it.
    pub fn start(&mut self) {
        self.start_due = true;
    }

    /// Whether any head's command has started since [`App::start`].
    pub fn started(&self) -> bool {
        self.heads.iter().any(Head::started)
    }

    /// Each head's first start, when it is due: its consoles open and
    /// their sessions start.
    fn first_start(&mut self, now: Instant) {
        if !self.start_due {
            return;
        }
        let since = now.saturating_duration_since(self.epoch);
        for h in 0..self.heads.len() {
            let due = self.heads.get(h).is_some_and(|head| {
                !head.started() && respawn::first_start_due(self.awake, !head.views.is_empty())
            });
            let App {
                heads,
                config,
                scrollback,
                scrollback_bytes,
                clock,
                ..
            } = self;
            let Some(head) = heads.get_mut(h).filter(|_| due) else {
                continue;
            };
            head.open_wanted(|| {
                crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock))
            });
            for id in head.start_consoles(since) {
                self.spawn(h, id);
            }
        }
    }

    /// Opens a console in head `h` and shows it, within the caps
    /// (ADR-1401 D10); its session starts at once once the head's has.
    fn new_console(&mut self, h: usize, now: Instant) {
        let total = session::console_total(&self.heads);
        let caps = self.caps;
        let term = crate::head::terminal(
            &self.config,
            self.scrollback,
            self.scrollback_bytes,
            Rc::clone(&self.clock),
        );
        let since = now.saturating_duration_since(self.epoch);
        let Some(head) = self
            .heads
            .get_mut(h)
            .filter(|head| head.may_open(caps, total))
        else {
            return;
        };
        let id = head.next_id();
        let starts = head.open(id, term, since, self.awake);
        self.relayout(h);
        if starts {
            self.spawn(h, id);
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

    /// Adds a video device, woken while the VT is ours and else put to
    /// sleep, which lets go of the DRM master a card's open took; the VT's
    /// acquire wakes it (vtium/vtium#139).
    pub fn add_video(&mut self, node: PathBuf, mut video: Box<dyn Video>) {
        if self.awake {
            if let Err(e) = video.wake_up() {
                log::warn!("cannot wake {}: {e}", node.display());
            }
        } else {
            video.sleep();
        }
        self.videos.insert(node, video);
    }

    /// Removes a video device and its displays.
    pub fn remove_video(&mut self, node: &Path) {
        self.videos.remove(node);
        self.cards.remove(node);
        self.lit.retain(|(n, _), _| n != node);
        self.dark.retain(|(n, _), _| n != node);
        self.guard_lit.take_if(|((n, _), _)| n == node);
        self.guard_due = true;
        for h in 0..self.heads.len() {
            let lost = self.heads.get_mut(h).is_some_and(|head| {
                let before = head.views.len();
                head.views.retain(|(n, _), _| n != node);
                head.views.len() != before
            });
            if lost {
                self.relayout(h);
            }
        }
        self.retaking.remove(node);
        if let Some(r) = &mut self.releasing {
            r.remove(node);
        }
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

    /// Starts the session of head `h`'s console `id`.
    fn spawn(&mut self, h: usize, id: u64) {
        let config = self
            .heads
            .get(h)
            .and_then(|head| head.login.as_ref())
            .unwrap_or(&self.config);
        let spec = child::child(config, &self.place(), self.inherited.clone());
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
        let Some(size) = self.heads.get(h).map(Head::pty_size) else {
            return;
        };
        let mut pty = match Pty::spawn(command, size, erase) {
            Ok(pty) => pty,
            Err(e) => {
                log::error!("cannot start the session: {e}");
                self.request_exit(Exit::Spawn);
                return;
            }
        };
        let inserted = pty.insert(&self.handle, move |app: &mut App| {
            let Console { pty, term, .. } = app.heads.get_mut(h)?.console_mut(id)?;
            pty.as_mut().map(|p| (p, term))
        });
        if let Err(e) = inserted {
            log::error!("cannot watch the session's pty: {e}");
            self.request_exit(Exit::Spawn);
            return;
        }
        if let Some(console) = self.heads.get_mut(h).and_then(|head| head.console_mut(id)) {
            console.pty = Some(pty);
        }
    }

    /// Chooses head `h`'s font once, at its first display (ADR-1200 D9,
    /// ADR-1401 D3): the head's own font, else the frit `font-name` or
    /// vconsole names, or the one-source frit made on the fly for a kbd
    /// name, a path or vconsole's font, generated at run time (ADR-1301
    /// D2); else the built-in font, with one line naming the font and why.
    fn choose_font(&mut self, h: usize) {
        let Some(head) = self.heads.get(h) else {
            return;
        };
        if head.font.is_some() {
            return;
        }
        let s = &self.font_sources;
        let wanted = match &head.font_name {
            Some(name) => Some(font::Wanted {
                name: name.clone(),
                came: font::Came::Config,
            }),
            None => font::wanted(
                &self.config,
                &font::read_text(&s.vconsole_conf),
                &font::read_text(&s.kernel_cmdline),
            ),
        };
        let chosen = match wanted {
            None => Chosen::Builtin(None),
            Some(wanted) => match font::builtin_named(&wanted.name) {
                Some(builtin) => builtin,
                None => {
                    // The frit's cell is read as text before the first
                    // frame; its fonts are the helper's to parse (#145).
                    let chosen = font::frit_named(&s.frits, &wanted)
                        .and_then(|frit| Chosen::frit(frit, head.sized()));
                    let chosen = match chosen {
                        Ok(chosen) => {
                            log::info!(
                                "the font is the frit {}, drawn in the built-in font until its glyphs are generated",
                                chosen.name(0, &self.locale)
                            );
                            chosen
                        }
                        Err(why) => {
                            log::warn!(
                                "the font {} is not drawn ({why}): vtium draws its built-in font",
                                wanted.name
                            );
                            Chosen::Builtin(None)
                        }
                    };
                    if let Some(head) = self.heads.get_mut(h) {
                        head.font = Some(chosen);
                    }
                    self.share(h);
                    self.own_cell(h);
                    return;
                }
            },
        };
        if let Some(head) = self.heads.get_mut(h) {
            head.font = Some(chosen);
        }
    }

    /// A frit of bitmap sources that states no cell, with no `font-size`
    /// configured, takes its fonts' own cell, as the C reference drew a
    /// PSF font at its own size (the operator, 2026-10-28): the helper is
    /// asked for it before the first frame and waited for at most
    /// [`helper::OWN_CELL_MS`], the one wait on the helper before a frame.
    /// A refusal, or no answer in time, leaves the display's cell, with
    /// one line saying why.
    fn own_cell(&mut self, h: usize) {
        let wants = self.heads.get(h).is_some_and(|head| {
            !head.sized() && head.font.as_ref().is_some_and(Chosen::wants_own_cell)
        });
        let Some(name) = self.heads.get(h).and_then(Head::frit).map(str::to_owned) else {
            return;
        };
        if !wants {
            return;
        }
        let known = self.frits.get(&name).and_then(|f| f.own.clone());
        let own = match known {
            Some(own) => own,
            None => {
                let now = (self.clock)();
                let Some(helper) = self.helper(&name) else {
                    return;
                };
                let own = helper.own_cell(Duration::from_millis(helper::OWN_CELL_MS), now);
                match &own {
                    Ok(cell) => log::info!(
                        "the frit {name} draws in its fonts' own {}x{} cell",
                        cell.width,
                        cell.height
                    ),
                    Err(why) => log::warn!(
                        "the frit {name}'s own cell is not known ({why}): it draws in the display's cell"
                    ),
                }
                if let Some(frit) = self.frits.get_mut(&name) {
                    frit.own = Some(own.clone());
                }
                own
            }
        };
        if let (Ok(cell), Some(head)) = (own, self.heads.get_mut(h)) {
            if let Some(font) = &mut head.font {
                font.set_own_cell(cell);
            }
        }
    }

    /// Head `h`'s frit joins the frits in use: a frit another head draws
    /// already gives the head its store, and one no head draws yet is
    /// added, its helper started at its first need.
    fn share(&mut self, h: usize) {
        let Some(Chosen::Frit {
            name,
            frit,
            generated,
            ..
        }) = self.heads.get_mut(h).and_then(|head| head.font.as_mut())
        else {
            return;
        };
        match self.frits.get(name.as_str()) {
            Some(shared) => *generated = std::sync::Arc::clone(&shared.generated),
            None => {
                self.frits.insert(
                    name.clone(),
                    Frit {
                        frit: frit.clone(),
                        generated: std::sync::Arc::clone(generated),
                        helper: None,
                        refused: false,
                        asked: Vec::new(),
                        own: None,
                    },
                );
            }
        }
    }

    /// Drops each frit no head draws any more, and with it its helper.
    fn prune(&mut self) {
        let used: BTreeSet<String> = self
            .heads
            .iter()
            .filter_map(Head::frit)
            .map(str::to_owned)
            .collect();
        self.frits.retain(|name, _| used.contains(name));
    }

    /// The name of the font set the shown head draws, as OSC 50's query
    /// reports it.
    pub fn font_name(&self) -> &str {
        self.shown().map_or("unifont", |head| {
            head.chosen().name(head.font_size, &self.locale)
        })
    }

    /// OSC 50 asked head `h` for the font `name` (ADR-1200 D5): a frit or
    /// a built-in switches the head's live console as a zoom does, on a
    /// new grid, a frit with the helper and the store every head drawing
    /// it shares; anything
    /// else leaves the font. The first refusal since the last switch is
    /// logged and the rest counted, so a program's flood of requests logs
    /// one line.
    fn switch_font(&mut self, h: usize, name: &str) {
        self.choose_font(h);
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let same = match head.chosen() {
            Chosen::Frit { name: now, .. } => now == name,
            Chosen::Builtin(Some(n)) => *n == name,
            Chosen::Builtin(None) => false,
        };
        if same {
            return;
        }
        let sized = head.sized();
        let was = head.frit().map(str::to_owned);
        let new = font::by_name(&self.font_sources, name).and_then(|new| {
            let new = match new {
                Chosen::Frit { frit, .. } => Chosen::frit(frit, sized)?,
                builtin => builtin,
            };
            let Some(head) = self.heads.get_mut(h) else {
                return Ok(());
            };
            let old = head.font.replace(new);
            self.share(h);
            self.own_cell(h);
            if let Some(head) = self.heads.get_mut(h) {
                head.refont = true;
            }
            if self.relayout(h) {
                return Ok(());
            }
            let Some(head) = self.heads.get_mut(h) else {
                return Ok(());
            };
            head.font = old;
            head.refont = true;
            let size = head.font_size;
            self.relayout(h);
            Err(format!("it draws nothing at {size} pixels"))
        });
        self.prune();
        // The frit the head left, where another head still draws it, keeps
        // the cells of the heads drawing it alone.
        if let Some(other) = was.and_then(|was| {
            self.heads
                .iter()
                .position(|head| head.frit() == Some(was.as_str()))
        }) {
            self.ask_glyphs(other);
        }
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let current = head.chosen().name(head.font_size, &self.locale).to_owned();
        match new {
            Ok(()) => {
                let refused = std::mem::take(&mut head.font_refused);
                if refused > 1 {
                    log::info!(
                        "OSC 50 was refused {} more times before this switch",
                        refused.saturating_sub(1)
                    );
                }
                log::info!("OSC 50: the font is now {current}");
            }
            Err(why) => {
                if head.font_refused == 0 {
                    log::warn!(
                        "OSC 50 asked for the font {name}: {why}; the font stays {current}, and OSC 50's further refusals are counted until a switch"
                    );
                }
                head.font_refused = head.font_refused.saturating_add(1);
            }
        }
    }

    fn measure(&self) -> bool {
        matches!(self.config.text(&keys::RENDER_TIMER), Some("log" | "both"))
    }

    /// The rules' identity of the display `id` of `node`, its place in
    /// its card's list the place among the displays its device lists, in
    /// the kernel's order of connectors, whatever their object ids.
    fn identity(&self, node: &Path, id: DisplayId) -> Option<vtium_config::Identity> {
        let video = self.videos.get(node)?;
        let card = self.cards.get(node).cloned().unwrap_or_default();
        let order = video
            .displays()
            .iter()
            .position(|&d| d == id)
            .map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX));
        Some(match video.identity(id) {
            Some(named) => identity::display(&named, &card, order),
            None => identity::unnamed_display(&video.name(id), &card, order),
        })
    }

    /// The head the display `id` of `node` joins, by the first rule that
    /// matches it, with the display as the rules saw it; a display the
    /// rules leave dark is set off and logged ([`place_display`]).
    fn head_for(&mut self, node: &Path, id: DisplayId) -> Option<(usize, vtium_config::Identity)> {
        let display = self.identity(node, id)?;
        let key = (node.to_owned(), id);
        if let Some((_, h)) = self.guard_lit.as_ref().filter(|(k, _)| *k == key) {
            return Some((*h, display));
        }
        match place_display(&self.rules, &display) {
            Ok(h) => Some((h, display)),
            Err(line) => {
                log::info!("{line}");
                self.dark.insert(key, display);
                self.guard_due = true;
                self.darken();
                None
            }
        }
    }

    /// Sets off each display a device announced that the rules leave
    /// dark, after a wake-up or an unblank lit it: a DRM card never
    /// announces one ([`open_video`]), a framebuffer does.
    fn darken(&mut self) {
        for (node, id) in self.dark.keys() {
            if let Some(video) = self.videos.get_mut(node) {
                if let Err(e) = video.set_dpms(*id, Dpms::Off) {
                    log::warn!("cannot set display {} off: {e}", video.name(*id));
                }
            }
        }
    }

    fn add_display(&mut self, node: &Path, id: DisplayId) {
        let Some(video) = self.videos.get(node) else {
            return;
        };
        let Some(size) = video.size(id) else {
            return;
        };
        let refresh = video.refresh_mhz(id);
        let (backend, name) = (video.backend(), video.name(id));
        let levels = video.levels(id);
        let measured = video.dpi(id).is_some();
        let Some((h, display)) = self.head_for(node, id) else {
            return;
        };
        if !measured
            && self.config.number(&keys::DISPLAY_DPI).is_none()
            && self.assumed.insert((node.to_owned(), id))
        {
            crate::warn::warn(format_args!(
                "display {name} reports no physical size: vtium assumes {DPI_ASSUMED} DPI, which display-dpi can state"
            ));
        }
        self.choose_font(h);
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let Ok(face) = head.face(head.font_size.max(1), &self.locale) else {
            log::error!("cannot load the font for display {name}");
            return;
        };
        let font = head.font_at(face, &self.config);
        let mut view = View::new(
            id,
            font,
            size,
            refresh,
            RenderConfig {
                depth: depth(&self.config, levels),
                ..head.render_config(&self.config)
            },
            self.measure(),
        );
        if self.config.switch(&keys::SOFT_CURSOR) {
            view.pointer = crate::pointer::Pointer::software();
        }
        let key = (node.to_owned(), id);
        if let Some(head) = self.heads.get_mut(h) {
            head.views.insert(key.clone(), view);
        }
        if !self.relayout(h) {
            if let Some(head) = self.heads.get_mut(h) {
                head.views.remove(&key);
            }
            return;
        }
        // Lit once it draws, so the focus never starts on a head whose
        // display could not be set up.
        if !self.focus_settled {
            self.start_lit.insert(key.clone(), display.clone());
        }
        self.lit.insert(key.clone(), display);
        self.focus_due = true;
        self.guard_due = true;
        if let Some(v) = self.heads.get(h).and_then(|head| head.views.get(&key)) {
            let cell = v.cell();
            log::info!(
                "Display {name} with backend [{backend}] text renderer [bbulk], cell size {}x{}",
                cell.width,
                cell.height
            );
        }
    }

    /// Sets each of head `h`'s displays' font and its grid from the base
    /// height, as [`layout::layout`] decides, and its terminal and its pty
    /// at the grid. Returns false, changing nothing, when the base font
    /// cannot be found.
    fn relayout(&mut self, h: usize) -> bool {
        if self.heads.get(h).is_none_or(|head| head.views.is_empty()) {
            return true;
        }
        self.choose_font(h);
        let dpi = self.dpi(h);
        let App {
            heads,
            config,
            locale,
            ..
        } = self;
        let Some(head) = heads.get_mut(h) else {
            return true;
        };
        let locale = locale.as_str();
        let sizes: Vec<(u32, u32)> = head.views.values().map(|v| v.size).collect();
        if let Some(points) = head.points {
            head.font_size = font::points_to_pixels(points, dpi);
        } else if head.auto_size {
            let w = sizes.iter().map(|s| s.0).min().unwrap_or(0);
            let ht = sizes.iter().map(|s| s.1).min().unwrap_or(0);
            head.font_size = head.chosen().auto_height(w, ht, locale, dpi);
        }
        head.font_size = head.chosen().snap(head.font_size);
        let selected = font::select(head.font_size, head.has_font, |x| head.face(x, locale));
        let base = match selected {
            Ok((face, base)) => {
                if !head.has_font {
                    for (range, n) in vtium_font::Coverage::new(|r| face.coverage(r)).partial() {
                        crate::warn::warn(format_args!(
                            "the font covers {} of {} characters of {range:?}; that range draws the sprites unless glyph-source says otherwise",
                            n.covered, n.total
                        ));
                    }
                }
                head.has_font = true;
                head.step = face.size_step();
                base
            }
            Err(e) => {
                log::error!("cannot load the font: {e}");
                return false;
            }
        };
        head.font_size = base;
        let name = head.chosen().name(base, locale).to_owned();
        head.console.term.terminal.set_font_name(&name);
        let cell = |head: &Head, at: u32| {
            head.face(at, locale).ok().map(|f| {
                let c = f.cell();
                (c.width, c.height)
            })
        };
        let turned = matches!(head.orientation, Orientation::Right | Orientation::Left);
        let Some(l) = layout::layout(&sizes, turned, base, head.step, |at| cell(head, at)) else {
            return true;
        };
        head.cell = cell(head, base).unwrap_or(head.cell);
        let render = head.render_config(config);
        let keys: Vec<(PathBuf, DisplayId)> = head.views.keys().cloned().collect();
        let refont = std::mem::take(&mut head.refont);
        for (key, height) in keys.into_iter().zip(l.heights) {
            let orientation = head.orientation;
            let stale = head
                .views
                .get(&key)
                .is_some_and(|v| refont || v.height != height || v.orientation != orientation);
            if !stale {
                continue;
            }
            let Ok(face) = head.face(height, locale) else {
                continue;
            };
            let font = head.font_at(face, config);
            if let Some(v) = head.views.get_mut(&key) {
                v.rebuild(font, height, render);
            }
        }
        head.resize(l.grid);
        self.bound_pointers();
        self.ask_glyphs(h);
        true
    }

    /// Mode 1004's focus (ADR-1401 D5): the terminal of each head a
    /// keyboard delivers to has it, lit or not, so several hold it at
    /// once; each change goes to a program that asked for it.
    fn report_focus(&mut self) {
        // A keyboard that could not be opened, or went at a wake-up, is
        // gone from the seat's input; with no seat input, as a test drives
        // the seat, a device counts as open.
        let input = self.input.as_ref();
        let open = |id: Option<DeviceId>| {
            input.is_none_or(|i| id.is_some_and(|id| i.disposition(id).is_some()))
        };
        for (h, head) in self.heads.iter_mut().enumerate() {
            let focused = self.routes.keyboard_reaches(h, self.focus, open);
            if focused != head.focused {
                head.focused = focused;
                if head.console.term.terminal.focus(focused) {
                    head.send_user_bytes();
                }
            }
        }
    }

    /// Each pointer moves within the grid of the head it delivers to, and
    /// a pointer added later within the focus's until it is placed.
    fn bound_pointers(&mut self) {
        let Some(input) = &mut self.input else {
            return;
        };
        for (h, head) in self.heads.iter().enumerate() {
            let (w, ht) = head.pixels(head.grid);
            if self.focus == Some(h) {
                input.set_pointer_max(w, ht);
            }
            input.set_pointer_max_of(&self.routes.devices_of(h, self.focus), w, ht);
        }
    }

    /// The density head `h`'s points convert at (the operator's decision
    /// on vtium/vtium#145): `display-dpi`; else the lowest of the head's
    /// displays', each one's measured down its rows as vtium draws it, or
    /// [`DPI_ASSUMED`] where it measures none; [`DPI_ASSUMED`] with no
    /// display.
    fn dpi(&self, h: usize) -> f64 {
        if let Some(dpi) = self.config.number(&keys::DISPLAY_DPI) {
            return dpi as f64;
        }
        let Some(head) = self.heads.get(h) else {
            return DPI_ASSUMED;
        };
        let turned = matches!(head.orientation, Orientation::Right | Orientation::Left);
        head.views
            .keys()
            .map(|(node, id)| {
                let measured = self.videos.get(node).and_then(|v| v.dpi(*id));
                measured.map_or(
                    DPI_ASSUMED,
                    |(across, down)| if turned { across } else { down },
                )
            })
            .reduce(f64::min)
            .unwrap_or(DPI_ASSUMED)
    }

    /// While head `h`'s font is a frit, fills each cell a display of a
    /// head drawing that frit draws at and that was not filled yet, the
    /// screen's characters first, and asks for the screen's characters
    /// there; drops the glyphs of cells no display draws at any more
    /// (ADR-1301 D2, the operator's fill model).
    fn ask_glyphs(&mut self, h: usize) {
        let Some(name) = self.heads.get(h).and_then(Head::frit).map(str::to_owned) else {
            return;
        };
        let drawing: Vec<&Head> = self
            .heads
            .iter()
            .filter(|head| head.frit() == Some(name.as_str()))
            .collect();
        let mut cells: Vec<vtium_font::Cell> = drawing
            .iter()
            .flat_map(|head| head.views.values().map(View::cell))
            .collect();
        cells.sort_unstable_by_key(|c| (c.height, c.width));
        cells.dedup();
        let now = (self.clock)();
        let Some(frit) = self.frits.get_mut(&name) else {
            return;
        };
        if let Some(helper) = &mut frit.helper {
            helper.retain(&cells, now);
        }
        frit.asked.retain(|c| cells.contains(c));
        let new: Vec<(vtium_font::Cell, crate::deferred::Shown)> = cells
            .into_iter()
            .filter(|c| !frit.asked.contains(c))
            .map(|cell| {
                let at = drawing
                    .iter()
                    .filter(|head| head.views.values().any(|v| v.cell() == cell));
                (cell, shown(at))
            })
            .collect();
        if new.is_empty() {
            return;
        }
        let Some(helper) = self.helper(&name) else {
            return;
        };
        for (cell, visible) in &new {
            helper.ask(*cell, visible, now);
            helper.fill(*cell, &visible.chars, now);
        }
        if let Some(frit) = self.frits.get_mut(&name) {
            frit.asked.extend(new.into_iter().map(|(cell, _)| cell));
        }
    }

    /// While head `h`'s font is a frit and its screen changed, asks for
    /// each character and cluster it shows that the store does not hold,
    /// at each cell its displays draw at, and marks the glyphs of the
    /// others as used.
    fn ask_visible(&mut self, h: usize, now: Instant) {
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let Some(frit) = head.frit().and_then(|name| self.frits.get_mut(name)) else {
            return;
        };
        let Some(helper) = &mut frit.helper else {
            return;
        };
        let visible = head.visible();
        for &cell in &frit.asked {
            if head.views.values().any(|v| v.cell() == cell) {
                crate::deferred::lock(&frit.generated).touch(cell, &visible);
                helper.ask(cell, &visible, now);
            }
        }
    }

    /// The helper of the frit `name`, started on the first need with a
    /// wake-up into the loop for its answers and the store its glyphs go
    /// into; `None`, logged once, when it cannot be.
    fn helper(&mut self, name: &str) -> Option<&mut Helper> {
        let frit = self.frits.get(name)?;
        if frit.helper.is_none() && !frit.refused {
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
            let started = match command.and_then(|c| wake.map(|w| (c, w))) {
                Ok((command, ping)) => {
                    let wake = std::sync::Mutex::new(ping);
                    Ok(Helper::new(
                        command,
                        std::sync::Arc::new(move || {
                            if let Ok(p) = wake.lock() {
                                p.ping();
                            }
                        }),
                        self.epoch,
                        frit.frit.clone(),
                        self.console_font.clone(),
                        std::sync::Arc::clone(&frit.generated),
                        crate::head::frit_store(
                            crate::deferred::store_limit(physical_memory()),
                            self.heads.len(),
                        ),
                    ))
                }
                Err(why) => {
                    log::warn!(
                        "the font's glyphs are not generated ({why}): vtium draws its built-in font, and each cluster unshaped, a glyph a codepoint"
                    );
                    Err(())
                }
            };
            let frit = self.frits.get_mut(name)?;
            match started {
                Ok(helper) => frit.helper = Some(helper),
                Err(()) => frit.refused = true,
            }
        }
        self.frits.get_mut(name)?.helper.as_mut()
    }

    /// Takes each helper's answers: each display at a set's cell, of every
    /// head drawing the frit, forgets the glyphs it cached for the set's
    /// characters, so none draws the built-in font once the frit sent it
    /// (ADR-1200 D7), and redraws the cells that show one.
    fn helper_turn(&mut self, now: Instant) {
        for h in 0..self.heads.len() {
            if self
                .heads
                .get(h)
                .is_some_and(|head| head.console.term.changed)
            {
                self.ask_visible(h, now);
            }
        }
        for (name, frit) in &mut self.frits {
            let Some(helper) = &mut frit.helper else {
                continue;
            };
            let answers = helper.poll(now);
            if answers.is_empty() {
                continue;
            }
            // Arrival is damage, as program output is: the answers since
            // the last turn merge into one set per cell, which the next
            // frame draws once, and only the cells showing their
            // characters draw.
            let mut arrived: Vec<(vtium_font::Cell, Vec<char>)> = Vec::new();
            for Answer { cell, chars, .. } in answers {
                match arrived.iter_mut().find(|(at, _)| *at == cell) {
                    Some((_, all)) => all.extend(chars),
                    None => arrived.push((cell, chars)),
                }
            }
            for head in &mut self.heads {
                if head.frit() != Some(name.as_str()) {
                    continue;
                }
                let visible = head.visible().chars;
                for (cell, chars) in &arrived {
                    let (shown, hidden): (Vec<char>, Vec<char>) =
                        chars.iter().partition(|c| visible.binary_search(c).is_ok());
                    for v in head.views.values_mut() {
                        if v.cell() == *cell {
                            v.forget_glyphs(&hidden);
                            if !shown.is_empty() {
                                v.glyphs_changed(&shown);
                            }
                        }
                    }
                    head.draw_now |= !shown.is_empty();
                }
            }
        }
    }

    /// The terminals start drawing and blinking.
    fn activate(&mut self, now: Instant) {
        self.awake = true;
        for head in &mut self.heads {
            head.set_visible(true);
            if let Some(arms) = head.console.term.blink.activate() {
                for (timer, arm) in arms {
                    match timer {
                        Timer::Text => head.text_due = Some(after(now, arm.first)),
                        Timer::Cursor => {
                            head.cursor_due = Some((after(now, arm.first), arm.every));
                        }
                    }
                }
            }
            for v in head.views.values_mut() {
                v.lost = true;
            }
            head.draw_now = true;
        }
        if self.measure() {
            self.timer_due = Some(after(now, Duration::from_secs(1)));
        }
    }

    /// The terminals stop drawing and blinking.
    fn deactivate(&mut self) {
        self.awake = false;
        for head in &mut self.heads {
            head.set_visible(false);
            head.text_due = None;
            head.cursor_due = None;
        }
        self.timer_due = None;
    }

    /// Whether any head has a display.
    fn any_display(&self) -> bool {
        self.heads.iter().any(|head| !head.views.is_empty())
    }

    /// The blank is due: every display powers down. With none, the
    /// countdown starts again.
    fn blank_now(&mut self, now: Instant) {
        if !self.any_display() {
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
                // A display the rules leave dark is never unblanked: a
                // device holding one powers up its other displays alone.
                let dark: Vec<DisplayId> = self
                    .dark
                    .keys()
                    .filter(|(n, _)| n == node)
                    .map(|&(_, id)| id)
                    .collect();
                let unblanked = if dark.is_empty() {
                    video.unblank()
                } else {
                    video
                        .displays()
                        .into_iter()
                        .filter(|id| !dark.contains(id))
                        .try_for_each(|id| video.set_dpms(id, Dpms::On))
                };
                if let Err(e) = unblanked {
                    log::warn!("cannot unblank: {e}");
                }
                if retake {
                    if let Err(e) = video.wake_up() {
                        log::warn!("cannot set up {} again: {e}", node.display());
                    }
                }
            }
            self.darken();
            if self.vt.is_active() {
                self.activate(now);
            }
        }
    }

    /// Every display of every head draws whole at the turn's end.
    fn lose_every_view(&mut self) {
        for head in &mut self.heads {
            for v in head.views.values_mut() {
                v.lost = true;
            }
            head.draw_now = true;
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
                self.darken();
                self.wait.shown(now);
                self.activate(now);
            }
            VtEvent::Release => {
                // The acquire sets every device up: no re-take is owed
                // to the VT that is not ours.
                self.retake_owed = false;
                // The drag's release reaches the VT that is ours next.
                for head in &mut self.heads {
                    head.pointer.drag = None;
                }
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
                self.darken();
                self.lose_every_view();
            }
            VtEvent::Hangup => self.request_exit(Exit::Hangup),
        }
    }

    /// The head holding the display `id` of `node`.
    fn head_of(&self, node: &Path, id: DisplayId) -> Option<usize> {
        let key = (node.to_owned(), id);
        self.heads
            .iter()
            .position(|head| head.views.contains_key(&key))
    }

    fn video_event(&mut self, node: &Path, event: Event, now: Instant) {
        match event {
            Event::Added(id) => self.add_display(node, id),
            Event::Removed(id) => {
                let key = (node.to_owned(), id);
                self.lit.remove(&key);
                self.dark.remove(&key);
                self.guard_lit.take_if(|(k, _)| *k == key);
                self.guard_due = true;
                if let Some(h) = self.head_of(node, id) {
                    if let Some(head) = self.heads.get_mut(h) {
                        head.views.remove(&(node.to_owned(), id));
                    }
                    self.relayout(h);
                }
            }
            Event::Redraw(id) => {
                let key = (node.to_owned(), id);
                for head in &mut self.heads {
                    if let Some(v) = head.views.get_mut(&key) {
                        v.lost = true;
                        head.draw_now = true;
                    }
                }
            }
            // A frame waiting on this flip is drawn at the turn's end,
            // as a pending view is; the other displays keep their own
            // coalescing.
            // A frame on screen ends the start's wait.
            Event::Flipped(id) => {
                let key = (node.to_owned(), id);
                for head in &mut self.heads {
                    if let Some(v) = head.views.get_mut(&key) {
                        self.presented = true;
                        if let Some(t) = v.timer.as_mut() {
                            t.flip();
                        }
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

    /// The focus, once a head has a display (ADR-1401 D5): it starts on
    /// the head holding the boot display, the connector lit at start, else
    /// the first connected one on the boot card, else on the other cards,
    /// in the kernel's order; else on the first head in the file with a
    /// display. Until the first key reaching a head or the first focus
    /// chord it starts again as each display lights, over every display lit
    /// since the start, one that left included, so a boot display lighting
    /// a turn late takes it whatever order the displays came in (the
    /// operator's answer on vtium/vtium#156) and a display leaving never
    /// moves it (D8); after, it moves only by the chord.
    fn take_focus(&mut self) {
        let due = std::mem::take(&mut self.focus_due);
        if self.start_lit.is_empty() || self.focus.is_some() && (self.focus_settled || !due) {
            return;
        }
        // The display guard's display counts as lit; the anti-lockout
        // guard moves the focus itself ([`App::guard`]).
        let mut rules = self.rules.clone();
        rules.guards.anti_lockout = false;
        let lit: Vec<vtium_config::Identity> = self.start_lit.values().cloned().collect();
        self.focus = head_index(rules.place(&lit, None).focus);
        self.bound_pointers();
    }

    /// The guards (ADR-1401 D9), after a device came or went: every device
    /// present placed by the rules in force, then the display guard and the
    /// anti-lockout guard over the placement.
    ///
    /// The display guard lights the boot display in the first head while
    /// the rules light none, and lets it go dark once they light another.
    /// The anti-lockout guard sends every keyboard to the focus, opened to
    /// deliver, while none reaches a head with a lit display, and moves the
    /// focus to a lit head, with a notice and the focus marks; once one
    /// reaches a lit head each goes back to its rule. A keyboard the rescue
    /// key moved follows the focus throughout, and reaches it while the
    /// focus's head is lit.
    fn guard(&mut self, now: Instant) {
        if !std::mem::take(&mut self.guard_due) {
            return;
        }
        let displays: Vec<(PathBuf, DisplayId)> =
            self.lit.keys().chain(self.dark.keys()).cloned().collect();
        let devices: Vec<vtium_config::Identity> = self
            .lit
            .values()
            .chain(self.dark.values())
            .chain(self.routes.unrescued())
            .cloned()
            .collect();
        let focus_lit = self
            .focus
            .and_then(|h| self.heads.get(h))
            .is_some_and(|head| !head.views.is_empty());
        let mut rules = self.rules.clone();
        rules.guards.anti_lockout &= !(focus_lit && self.routes.rescued_keyboard());
        let focus = self.focus.map(vtium_config::Place::Head);
        let placement = rules.place(&devices, focus);
        let (shown, inputs) = placement
            .placed
            .split_at(displays.len().min(placement.placed.len()));
        let lighting = displays
            .iter()
            .zip(shown)
            .find(|(_, p)| p.by == vtium_config::By::NeverDark)
            .and_then(|(key, p)| Some((key.clone(), head_index(Some(p.place))?)));
        if lighting != self.guard_lit {
            if let Some((key, _)) = self.guard_lit.take() {
                self.unlight(key);
            }
            if let Some((key, h)) = lighting {
                log::warn!(
                    "the display guard lights display {} in head {}",
                    key.0.display(),
                    self.heads.get(h).map_or("?", Head::name)
                );
                self.dark.remove(&key);
                self.guard_lit = Some((key.clone(), h));
                if let Some(video) = self.videos.get_mut(&key.0) {
                    if let Err(e) = video.set_dpms(key.1, Dpms::On) {
                        log::warn!("cannot set display {} on: {e}", video.name(key.1));
                    }
                }
                self.add_display(&key.0, key.1);
            }
        }
        let guarded = inputs.iter().map(|p| p.by == vtium_config::By::AntiLockout);
        for (id, disposition) in self.routes.guard(&self.rules, guarded) {
            if let Some(input) = &mut self.input {
                input.set_disposition(id, disposition);
            }
        }
        if placement.anti_lockout {
            if let Some(to) = head_index(placement.focus).filter(|&h| Some(h) != self.focus) {
                self.move_focus(to, now);
            }
            if !self.locked_out {
                let line = "the anti-lockout guard: no keyboard reaches a head with a lit display, so every keyboard follows the focus";
                log::warn!("{line}");
                if let Some(head) = self.focus.and_then(|h| self.heads.get_mut(h)) {
                    head.add_notice(line);
                }
            }
        }
        self.locked_out = placement.anti_lockout;
        self.any_lit.set(!self.lit.is_empty());
        self.bound_pointers();
    }

    /// The display the display guard lit goes dark again: the rules light
    /// another.
    fn unlight(&mut self, key: (PathBuf, DisplayId)) {
        let Some(display) = self.lit.remove(&key) else {
            return;
        };
        if let Some(h) = self.head_of(&key.0, key.1) {
            if let Some(head) = self.heads.get_mut(h) {
                head.views.remove(&key);
            }
            self.relayout(h);
        }
        log::info!("display {} goes dark again", key.0.display());
        self.dark.insert(key, display);
        self.darken();
    }

    /// A head's first display came, its last went, or its next came. With
    /// none the head's session runs on at the grid it had, which no
    /// display resizes, until its `on-loss` hangs it up (ADR-1401 D8), and
    /// the blank does not run: the blank in force ends, and the countdown
    /// starts afresh when a display comes, as at the user's input for now
    /// (vtium/vtium#87 tells the two apart). The next is drawn whole, and
    /// resizes the grid when its own differs, and stops the hang-up's
    /// time.
    fn attachment(&mut self, now: Instant) {
        let several = self.heads.len() > 1;
        let mut changed = false;
        for head in &mut self.heads {
            let attached = if head.views.is_empty() {
                match head.attached {
                    Attached::NotYet => Attached::NotYet,
                    Attached::Yes | Attached::Gone => Attached::Gone,
                }
            } else {
                Attached::Yes
            };
            let (cols, rows) = head.grid;
            let of = if several {
                format!(" of head {}", head.name())
            } else {
                String::new()
            };
            match (head.attached, attached) {
                (Attached::Yes, Attached::Gone) => {
                    let hangup = match head.on_loss {
                        OnLoss::Nohup => None,
                        OnLoss::Hangup => Some(0),
                        OnLoss::HangupAfter(secs) => Some(secs),
                    };
                    head.hangup_due =
                        hangup.map(|secs| after(now, Duration::from_secs(secs.into())));
                    match hangup {
                        None => log::warn!(
                            "the last display{of} is gone: the session runs on at {cols}x{rows} until the next comes"
                        ),
                        // The hang-up logs its own line.
                        Some(0) => {}
                        Some(secs) => log::warn!(
                            "the last display{of} is gone: the session hangs up in {secs} s unless one comes"
                        ),
                    }
                    changed = true;
                }
                (Attached::Gone, Attached::Yes) => {
                    head.hangup_due = None;
                    log::info!("a display{of} came: the session is drawn again at {cols}x{rows}");
                    changed = true;
                }
                (Attached::NotYet, Attached::Yes) => changed = true,
                _ => {}
            }
            head.attached = attached;
        }
        if changed {
            self.wake(now);
        }
        for h in 0..self.heads.len() {
            let due = self
                .heads
                .get(h)
                .and_then(|head| head.hangup_due)
                .is_some_and(|d| d <= now);
            if due {
                self.hang_up(h, now);
            }
        }
    }

    /// Hangs up head `h`'s sessions, which lost their last display under
    /// `on-loss = hangup` (ADR-1401 D8); the head then follows the respawn
    /// policy, coming back with the consoles its file asks for, as at its
    /// start.
    fn hang_up(&mut self, h: usize, now: Instant) {
        let since = now.saturating_duration_since(self.epoch);
        let App {
            heads,
            config,
            scrollback,
            scrollback_bytes,
            clock,
            ..
        } = self;
        let Some(head) = heads.get_mut(h) else {
            return;
        };
        if head.hang_up(since, self.oneshot) {
            head.open_wanted(|| {
                crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock))
            });
            let ids: Vec<u64> = std::iter::once(head.console.id)
                .chain(head.start_consoles(since))
                .collect();
            for id in ids {
                self.spawn(h, id);
            }
        }
        if let Some(exit) = session::exit_due(&self.heads) {
            self.request_exit(exit);
        }
    }

    /// A device of the seat came, went or changed. A framebuffer a DRM
    /// driver backs is skipped for its card.
    fn device_event(&mut self, event: DeviceEvent) {
        match event {
            DeviceEvent::Added(dev) => match dev.kind {
                DeviceKind::Input => {
                    let identity = dev
                        .input
                        .as_deref()
                        .map_or_else(vtium_config::Identity::input, identity::input);
                    self.add_input(&dev.node, identity);
                }
                DeviceKind::Fbdev if dev.flags.drm_backed => {
                    log::debug!("{}: backed by a DRM card, skipped", dev.node.display());
                }
                kind => {
                    let card = Card::at(&dev.node, dev.id_path.as_deref(), dev.flags.primary);
                    if let Err(e) = open_video(self, &dev.node, kind, card) {
                        log::warn!("cannot open {}: {e}", dev.node.display());
                    }
                }
            },
            DeviceEvent::Removed { kind, node } => match kind {
                DeviceKind::Input => {
                    self.routes.remove(&node);
                    self.guard_due = true;
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

    /// An input device udev reported, placed by the first rule matching
    /// `identity` (ADR-1401 D3): opened to deliver, opened for the rescue
    /// key alone, or never opened.
    pub fn add_input(&mut self, node: &Path, identity: vtium_config::Identity) {
        self.guard_due = true;
        let disposition = self.routes.add(&self.rules, node, &identity);
        if let Some(id) = self
            .input
            .as_mut()
            .and_then(|i| i.add_with(node, disposition))
        {
            self.routes.opened(node, id);
            self.bound_pointers();
        }
    }

    /// The name of the head holding the focus.
    pub fn focus_head(&self) -> Option<&str> {
        self.heads.get(self.focus?).map(Head::name)
    }

    /// The head the focus chord last moved the focus to, by its name,
    /// once: what draws the focus mark reads it (ADR-1401 D5).
    pub fn take_focus_move(&mut self) -> Option<String> {
        let h = self.focus_move.take()?;
        self.heads.get(h).map(|head| head.name().to_owned())
    }

    /// A key press or pointer event from no one device, which follows the
    /// focus.
    pub fn input_event(&mut self, event: InputEvent, now: Instant) {
        self.deliver(None, event, now);
    }

    /// A key press or pointer event from the input device at `node`.
    pub fn device_input(&mut self, node: &Path, event: InputEvent, now: Instant) {
        self.deliver(Some(node), event, now);
    }

    /// An event from the device at `node`, or from none, for the head its
    /// rule names or the head holding the focus (ADR-1401 D5). A key press
    /// is the user's activity; a pointer event is when
    /// [`App::pointer_event`] says so.
    fn deliver(&mut self, node: Option<&Path>, event: InputEvent, now: Instant) {
        // The shown VT's own key does nothing else (D9).
        if let (Some(node), InputEvent::Key(key)) = (node, &event) {
            if !key.repeat && self.vt.is_own_key(key) {
                self.activity(now);
                self.rescue(node);
                return;
            }
        }
        // A head whose display never lit receives nothing (D8).
        let h = self.routes.head(node, self.focus).filter(|&h| {
            self.heads
                .get(h)
                .is_some_and(|head| head.attached != Attached::NotYet)
        });
        match event {
            InputEvent::Key(key) => {
                // Only a key reaching a head settles the focus's start, and
                // ends its notices.
                self.focus_settled |= h.is_some();
                if let Some(head) = h.and_then(|h| self.heads.get_mut(h)) {
                    head.end_notices();
                }
                self.activity(now);
                self.key(h, &key, now);
            }
            InputEvent::Pointer(p) => {
                let mods = match (&self.input, h) {
                    (Some(input), Some(h)) => input.mods_of(&self.routes.devices_of(h, self.focus)),
                    _ => InputMods::empty(),
                };
                self.pointer_to(h, p, mods, now);
            }
        }
    }

    /// The rescue key, `Ctrl-Alt-F<n>` for vtium's own VT, pressed on the
    /// keyboard at `node` (ADR-1401 D9): it follows the focus for the rest
    /// of the boot, opened to deliver where it was watched for the rescue
    /// key alone, unless the `rescue-key` guard or its head's `rescue` is
    /// off. Nothing is written.
    fn rescue(&mut self, node: &Path) {
        let Some(id) = self.routes.rescue(&self.rules, node) else {
            return;
        };
        self.guard_due = true;
        let line = format!(
            "the rescue key: {} follows the focus until the next boot",
            node.display()
        );
        log::warn!("{line}");
        if let Some(head) = self.focus.and_then(|h| self.heads.get_mut(h)) {
            head.add_notice(&line);
        }
        if let (Some(input), Some(id)) = (&mut self.input, id) {
            input.set_disposition(id, vtium_platform::input::Disposition::Deliver);
        }
        self.bound_pointers();
    }

    /// A pointer event, with the modifiers held on the keyboards as it
    /// came, for the head holding the focus.
    pub fn pointer_event(&mut self, event: PointerEvent, mods: InputMods, now: Instant) {
        self.pointer_to(self.focus, event, mods, now);
    }

    /// A pointer event for head `h`, with the modifiers held on the
    /// keyboards delivering to it. A move, a button or the wheel is the
    /// user's activity; the end of a frame and the hide are vtium's own,
    /// as the C reference counted them (`src/seat.c:578-590` at kmscon
    /// b1f7a7c5).
    fn pointer_to(&mut self, h: Option<usize>, event: PointerEvent, mods: InputMods, now: Instant) {
        match event {
            PointerEvent::Moved { .. } | PointerEvent::Button { .. } | PointerEvent::Wheel(_) => {
                self.activity(now);
            }
            PointerEvent::Sync => {}
            // The seat's pointers hide together, on every head: the hide
            // comes from no one device.
            PointerEvent::Hide => {
                for head in &mut self.heads {
                    head.pointer_event(event, Mods::empty(), &mut self.selection);
                }
                return;
            }
        }
        let Some(h) = h else {
            return;
        };
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let zoom = head.pointer_event(event, Mods::from_bits(mods.bits()), &mut self.selection);
        if let Some(dir) = zoom {
            self.zoom(h, dir);
        }
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

    /// The key order: the VT's keys, the seat's, then the terminal of
    /// head `h`, the head the key's device delivers to: its grabs, DECARM,
    /// the encoder. Each key that passes the seat's clears the selection,
    /// as the C reference's did (`src/terminal.c:1370` at kmscon
    /// b1f7a7c5).
    fn key(&mut self, h: Option<usize>, key: &KeyEvent, now: Instant) {
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
        if let Some(chord) = chord::chord(bits, &key.keysyms) {
            // One action a press, as the power keys.
            if !key.repeat {
                self.chord(h, chord, now);
            }
            return;
        }
        let Some(h) = h else {
            return;
        };
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        head.console.term.terminal.screen_mut().clear_selection();
        if grab::matches(&self.paste_keys, bits, &key.keysyms) {
            // One paste a press, however long the key repeats.
            if !key.repeat {
                head.paste(self.selection.as_deref());
                head.send_user_bytes();
            }
            head.draw_now = true;
            return;
        }
        if let Some(g) = grab::grab(bits, &key.keysyms) {
            let screen = head.console.term.terminal.screen_mut();
            let page = head.grid.1.saturating_sub(1).max(1);
            head.draw_now = true;
            match g {
                Grab::ScrollUp => screen.scroll_view_up(1),
                Grab::ScrollDown => screen.scroll_view_down(1),
                Grab::PageUp => screen.scroll_view_up(page),
                Grab::PageDown => screen.scroll_view_down(page),
                Grab::ZoomIn => self.zoom(h, Zoom::In),
                Grab::ZoomOut => self.zoom(h, Zoom::Out),
                Grab::RotateCw | Grab::RotateCcw => {
                    head.orientation = layout::rotate(head.orientation, g == Grab::RotateCw);
                    self.relayout(h);
                }
            }
            return;
        }
        if key.repeat && !head.console.term.terminal.auto_repeat() {
            return;
        }
        let Some(&keysym) = key.keysyms.first() else {
            return;
        };
        let unicode = key.codepoints.first().copied().flatten();
        let mods = Mods::from_bits(key.mods.bits());
        if head
            .console
            .term
            .terminal
            .key(keysym, key.ascii, mods, unicode)
        {
            head.send_user_bytes();
            head.console.term.terminal.screen_mut().reset_view();
            if self.hide_while_typing {
                head.pointer.visible = false;
            }
        }
        let rate = head
            .console
            .term
            .terminal
            .cursor_blink_ms()
            .map_or(crate::blink::TEXT_PERIOD, |ms| {
                Duration::from_millis(ms.into())
            });
        if let Some(arm) = head.console.term.blink.key(rate) {
            head.cursor_due = Some((after(now, arm.first), arm.every));
        }
        head.draw_now = true;
    }

    /// A head's chord (ADR-1401 D5): a console chord acts on head `h`'s
    /// consoles, the head the key's device delivers to; the focus chord
    /// moves the focus to the next or the previous head with a lit
    /// display.
    fn chord(&mut self, h: Option<usize>, chord: HeadChord, now: Instant) {
        let visible = self.awake;
        let back = match chord {
            HeadChord::NextHead => false,
            HeadChord::PreviousHead => true,
            HeadChord::NewConsole => {
                if let Some(h) = h {
                    self.new_console(h, now);
                }
                return;
            }
            HeadChord::NextConsole | HeadChord::PreviousConsole | HeadChord::CloseConsole => {
                let changed = h
                    .and_then(|h| self.heads.get_mut(h))
                    .is_some_and(|head| head.console_chord(chord, visible));
                if let Some(h) = h.filter(|_| changed) {
                    self.relayout(h);
                }
                return;
            }
        };
        let heads = &self.heads;
        let lit = |h: usize| heads.get(h).is_some_and(|head| !head.views.is_empty());
        self.focus_settled = true;
        if let Some(to) = chord::step(self.focus, heads.len(), back, lit) {
            self.move_focus(to, now);
        }
    }

    /// The focus moves to head `h`, by the focus chord or the anti-lockout
    /// guard, the only moves after its start (ADR-1401 D5): the focus marks
    /// light.
    fn move_focus(&mut self, h: usize, now: Instant) {
        self.focus = Some(h);
        self.focus_move = Some(h);
        self.mark_due = Some(after(now, crate::head::MARK_SHOWN));
        self.bound_pointers();
    }

    /// A zoom of head `h`: the base height the font's next step, larger or
    /// smaller ([`Chosen::zoom`]), which ends `font-size`'s following of
    /// the displays (ADR-1200 D9). Below the smallest it stops, with a
    /// line naming the set and its smallest height. A height with no font
    /// keeps the one in use.
    fn zoom(&mut self, h: usize, dir: Zoom) {
        self.choose_font(h);
        let dpi = self.dpi(h);
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let chosen = head.chosen();
        let Some(size) = chosen.zoom(head.font_size, dir, &self.locale, dpi) else {
            if dir == Zoom::Out {
                let smallest = chosen.smallest(&self.locale);
                log::info!(
                    "the set {} draws no smaller than {smallest} pixels: zoom stops there",
                    chosen.name(smallest, &self.locale)
                );
            }
            return;
        };
        let (old, auto, points) = (head.font_size, head.auto_size, head.points);
        head.auto_size = false;
        head.points = None;
        head.font_size = size;
        if !self.relayout(h) {
            if let Some(head) = self.heads.get_mut(h) {
                head.font_size = old;
                head.auto_size = auto;
                head.points = points;
            }
        }
    }

    /// The programs of head `h`'s consoles that exited
    /// ([`session`]): each closes its console, starts again or ends the
    /// head, and vtium ends once no head that started still runs.
    fn child_exited(&mut self, h: usize, now: Instant) {
        let since = now.saturating_duration_since(self.epoch);
        let exited: Vec<(u64, std::process::ExitStatus)> = self
            .heads
            .get_mut(h)
            .into_iter()
            .flat_map(Head::consoles_mut)
            .filter_map(|c| c.term.exited.take().map(|status| (c.id, status)))
            .collect();
        for (id, status) in exited {
            log::info!("child exited: {status}");
            let Some(head) = self.heads.get_mut(h) else {
                return;
            };
            match head.exited(id, since, self.oneshot, self.awake) {
                AfterExit::Closed { shown: true } => {
                    self.relayout(h);
                }
                AfterExit::Restart => self.spawn(h, id),
                AfterExit::Closed { shown: false } | AfterExit::Ended => {}
            }
        }
        if let Some(exit) = session::exit_due(&self.heads) {
            self.request_exit(exit);
        }
    }

    /// One turn's end: every source drained, the timers due run, and the
    /// displays drawn where something asks.
    pub fn turn(&mut self, now: Instant) {
        let events: Vec<VtEvent> = self.vt.events().collect();
        for e in events {
            self.vt_event(e, now);
        }
        // After the VT's events, so none shows once the VT is released, and
        // before the devices', so a flip they queue is read this turn.
        self.frames(now);
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
        self.take_focus();
        self.guard(now);
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
        let inputs: Vec<DeviceInput> = match &mut self.input {
            Some(input) => input.device_events(self.vt.as_vt()).collect(),
            None => Vec::new(),
        };
        for head in &mut self.heads {
            head.tracking_changed();
        }
        for e in inputs {
            // A device watched for the rescue key sends its presses alone.
            let node = e
                .device
                .and_then(|id| self.routes.node(id))
                .map(Path::to_owned);
            self.deliver(node.as_deref(), e.event, now);
        }
        self.report_focus();
        for h in 0..self.heads.len() {
            self.session_turn(h, now);
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
        for console in self.heads.iter_mut().flat_map(Head::consoles_mut) {
            console.term.coalesce.turn_end();
            console.term.changed = false;
        }
        let flipping = self.heads.iter().any(|head| {
            head.views.iter().any(|((node, _), v)| {
                v.away() || self.videos.get(node).is_some_and(|d| d.is_flipping(v.id))
            })
        });
        if let Some(wait) = self.wait.turn(now, self.presented, flipping) {
            log::error!("no display appeared within {} s", wait.as_secs());
            self.request_exit(Exit::NoDisplay);
        }
        self.exiting(now);
    }

    /// What head `h`'s programs asked this turn: each console's OSC 52
    /// selection; the shown console's OSC 50 font and blank sequences,
    /// which a hidden console's wait for it to be shown, since the font
    /// and the displays are the head's; and their exits.
    fn session_turn(&mut self, h: usize, now: Instant) {
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        // The console has no system clipboard: a selection OSC 52 set is
        // held for a paste, or dropped under `no-clipboard-write`.
        for console in head.consoles_mut() {
            if let Some(selection) = console.term.terminal.take_clipboard_write() {
                if self.clipboard_write {
                    self.selection = Some(selection);
                }
            }
        }
        // The terminal takes OSC 50 only under `allow-font-ops`.
        if let Some(name) = head.console.term.terminal.take_font_request() {
            self.switch_font(h, &name);
        }
        // After the exit, whose respawn's RIS returns the blank to
        // `dpms-timeout` for the next session.
        self.child_exited(h, now);
        let request = self
            .heads
            .get_mut(h)
            .and_then(|head| head.console.term.terminal.take_blank_request());
        if let Some(request) = request {
            if self.program_blank {
                self.blank_request(request, now);
            }
        }
    }

    fn timers(&mut self, now: Instant) {
        let awake = self.awake;
        for head in &mut self.heads {
            if head.notice_due.is_some_and(|d| d <= now) {
                head.end_notices();
            }
            if let Some(due) = head.text_due.filter(|&d| d <= now) {
                head.draw_now |= head.console.term.blink.fired(Timer::Text, awake);
                head.text_due = Some(after(due, crate::blink::TEXT_PERIOD));
            }
            if let Some((due, every)) = head.cursor_due.filter(|&(d, _)| d <= now) {
                head.draw_now |= head.console.term.blink.fired(Timer::Cursor, awake);
                head.cursor_due = Some((after(due, every), every));
            }
            let term = &mut head.console.term;
            if term.sync.deadline().is_some_and(|d| d <= now) {
                let action = term.sync.timer(&mut term.terminal, now, term.unparsed);
                term.act(action);
                if action == Action::Expired {
                    for v in head.views.values_mut() {
                        if let Some(t) = &mut v.timer {
                            t.expired();
                        }
                    }
                }
            }
        }
        if self.blank.fire(now) {
            self.blank_now(now);
        }
        if self.mark_due.is_some_and(|d| d <= now) {
            self.mark_due = None;
        }
    }

    fn draw(&mut self, now: Instant) {
        let shown = self
            .focus_mark
            .is_some_and(|always| always || self.mark_due.is_some());
        for (h, head) in self.heads.iter_mut().enumerate() {
            let mark = shown.then_some(if self.focus == Some(h) {
                vtium_render::Mark::Focus
            } else {
                vtium_render::Mark::NoFocus
            });
            if head.mark != mark {
                head.mark = mark;
                head.draw_now = true;
            }
        }
        for h in 0..self.heads.len() {
            self.painter(h);
        }
        let App {
            heads,
            videos,
            clock,
            ..
        } = self;
        for head in heads.iter_mut() {
            draw_head(head, videos, &**clock, now);
        }
    }

    /// Starts head `h`'s render thread at its first frame (ADR-1401 D2),
    /// with a wake-up into the loop for each frame it composes. One that
    /// cannot start is logged once, and the loop composes the head's
    /// frames.
    fn painter(&mut self, h: usize) {
        let Some(head) = self.heads.get(h) else {
            return;
        };
        if head.painter.is_some() || head.painter_refused || head.views.is_empty() {
            return;
        }
        let started = vtium_platform::calloop::ping::make_ping()
            .map_err(|e| format!("cannot make its wake-up: {e}"))
            .and_then(|(ping, source)| {
                self.handle
                    .insert_source(source, |(), (), _: &mut App| {})
                    .map_err(|e| format!("cannot watch its wake-up: {e}"))?;
                Painter::start(head.name(), move || ping.ping()).map_err(|e| e.to_string())
            });
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        match started {
            Ok(painter) => head.painter = Some(painter),
            Err(why) => {
                log::warn!(
                    "head {}: the render thread cannot start ({why}): the loop draws its frames",
                    head.name()
                );
                head.painter_refused = true;
            }
        }
    }

    /// Takes the frames each head's render thread composed: each display
    /// gets its drawing back and, while the VT is ours and awake, shows
    /// its frame.
    fn frames(&mut self, now: Instant) {
        let App {
            heads,
            videos,
            clock,
            awake,
            ..
        } = self;
        for head in heads.iter_mut() {
            let Some(painter) = &mut head.painter else {
                continue;
            };
            for composed in painter.take() {
                let (Some(view), Some(video)) = (
                    head.views.get_mut(&composed.key),
                    videos.get_mut(&composed.key.0),
                ) else {
                    continue;
                };
                let compose = composed.stats.cpu;
                let shown = view.present(
                    composed,
                    &mut head.console.term,
                    video.as_mut(),
                    *awake,
                    &**clock,
                );
                if let Some(copy) = shown.copy {
                    head.figures.frame(compose, copy, now);
                }
            }
        }
    }

    /// The render timer's line, once a second.
    fn report(&mut self, now: Instant) {
        for head in &mut self.heads {
            if let Some(line) = head.figures_line(now) {
                log::debug!(target: "render", "{line}");
            }
        }
        let Some(due) = self.timer_due.filter(|&d| d <= now) else {
            return;
        };
        self.timer_due = Some(after(due, Duration::from_secs(1)));
        for head in &mut self.heads {
            for ((node, id), view) in &mut head.views {
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
    }

    fn exiting(&mut self, now: Instant) {
        if self.exit.is_none() {
            return;
        }
        match self.leaving {
            None => {
                self.deactivate();
                for console in self.heads.iter_mut().flat_map(Head::consoles_mut) {
                    console.pty = None;
                }
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
        due.extend(self.timer_due);
        due.extend(self.wait.due());
        due.extend(self.blank.deadline());
        due.extend(self.mark_due);
        for head in &self.heads {
            due.extend(head.text_due);
            due.extend(head.cursor_due.map(|(d, _)| d));
            due.extend(head.console.term.sync.deadline());
            due.extend(head.views.values().filter_map(|v| v.wait.due()));
            due.extend(head.figures.due);
            due.extend(head.hangup_due);
            due.extend(head.notice_due);
        }
        for frit in self.frits.values() {
            due.extend(frit.helper.as_ref().and_then(Helper::deadline));
        }
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
        self.heads
            .iter()
            .find_map(|head| head.views.values().next())
            .map(View::font_policy)
    }

    /// Each display's font cell, head by head in the head file's order,
    /// each head's in the order of its displays.
    pub fn cells(&self) -> Vec<(u32, u32)> {
        self.heads.iter().flat_map(Head::cells).collect()
    }

    /// The shown head's grid.
    pub fn grid(&self) -> (usize, usize) {
        self.shown().map_or(crate::head::GRID, Head::grid)
    }

    /// The shown head's terminal.
    pub fn term(&self) -> Option<&Term> {
        self.shown().map(Head::term)
    }

    /// The shown head's terminal, for a test to take what it queued.
    pub fn term_mut(&mut self) -> Option<&mut Term> {
        let h = self.focus.unwrap_or(0);
        self.heads.get_mut(h).map(|head| &mut head.console.term)
    }

    /// The shown head's pty, while its command runs.
    pub fn pty(&self) -> Option<&Pty> {
        self.shown().and_then(Head::pty)
    }

    /// The text of the shown head's screen rows, for tests and logs.
    pub fn screen_text(&self) -> Vec<String> {
        self.shown().map(Head::screen_text).unwrap_or_default()
    }
}

/// The index of the head at `place`: the default head is the first.
fn head_index(place: Option<vtium_config::Place>) -> Option<usize> {
    match place? {
        vtium_config::Place::Head(h) => Some(h),
        vtium_config::Place::DefaultHead => Some(0),
        _ => None,
    }
}

/// What the screens of `heads` show, merged: each character and cluster
/// any of them shows, sorted and once.
fn shown<'a>(heads: impl Iterator<Item = &'a &'a Head>) -> crate::deferred::Shown {
    let mut all = crate::deferred::Shown::default();
    for head in heads {
        let one = head.visible();
        all.chars.extend(one.chars);
        all.clusters.extend(one.clusters);
    }
    all.chars.sort_unstable();
    all.chars.dedup();
    all.clusters.sort_unstable();
    all.clusters.dedup();
    all
}

/// Draws each of `head`'s displays that something asks to: its render
/// thread composes the frame, or the loop where the thread could not
/// start.
fn draw_head(
    head: &mut Head,
    videos: &mut BTreeMap<PathBuf, Box<dyn Video>>,
    clock: &dyn Fn() -> Instant,
    now: Instant,
) {
    let state = head.draw_state();
    let overlay = head.overlay();
    if overlay.notice.is_some() && head.notice_due.is_none() && !head.views.is_empty() {
        head.notice_due = Some(after(now, crate::head::NOTICE_SHOWN));
    }
    let present = std::mem::take(&mut head.console.term.present);
    let now_all = std::mem::take(&mut head.draw_now) || present;
    let term = &mut head.console.term;
    let changed = term.changed;
    let holding = term.sync.holding(&term.terminal);
    let base = head.cell;
    let mut snapshot = None;
    for (key, view) in head.views.iter_mut() {
        let Some(video) = videos.get_mut(&key.0) else {
            continue;
        };
        let due = view.wait.due().is_some_and(|d| d <= now);
        if due {
            view.wait.end();
        }
        let owed = view.owed && !holding;
        let wants =
            now_all || view.lost || owed || due || (view.pending && !video.is_flipping(view.id));
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
        let c = view.cell();
        let state = DrawState {
            pointer: state
                .pointer
                .map(|(x, y)| (scale(x, c.width, base.0), scale(y, c.height, base.1))),
            ..state
        };
        let hook = head.draw_hook.as_ref();
        view.overlay.clone_from(&overlay);
        let Prepared::Job(job) = view.prepare(
            key,
            term,
            &mut snapshot,
            video.as_mut(),
            &state,
            hook,
            clock,
        ) else {
            continue;
        };
        match &head.painter {
            Some(painter) => {
                if let Some(job) = painter.send(job) {
                    view.refused(job);
                }
            }
            None => {
                let composed = job.compose();
                let compose = composed.stats.cpu;
                let shown = view.present(composed, term, video.as_mut(), true, clock);
                if let Some(copy) = shown.copy {
                    head.figures.frame(compose, copy, now);
                }
            }
        }
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
pub fn render_timer_line(report: &vtium_render::Report, node: &Path, id: DisplayId) -> String {
    format!("{report} display={}[{}]", node.display(), id.0)
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

/// The machine's memory in bytes; 0 when unread, which takes the
/// smallest scrollback cap.
fn physical_memory() -> u64 {
    crate::config::budget::physical_memory().unwrap_or(0)
}

/// Opens a device and wraps it as a video device in the loop, for
/// [`App::add_video`], keeping what the rules read of its `card`.
pub fn open_video(app: &mut App, node: &Path, kind: DeviceKind, card: Card) -> io::Result<()> {
    use vtium_platform::display::{Card as Drm, Drm2d, Fb, Fbdev, ModeLookup, ModePolicy, Wanted};
    let fd = app.vt.open_device(node)?;
    let key = node.to_owned();
    let video: Box<dyn Video> = match kind {
        DeviceKind::Drm => {
            let modes = app.modes.clone();
            let gpu = card.id_path.clone();
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
            let mut drm = Drm2d::new(Drm::new(fd)?, policy)?;
            drm.set_card_path(card.id_path.clone());
            // A display the rules leave dark is set off before it is
            // announced, a frame another master left included (D3).
            let (rules, at, lit) = (app.rules.clone(), card.clone(), Rc::clone(&app.any_lit));
            drm.set_dark(Box::new(move |named| {
                // While none is lit, it is announced for the display guard
                // to light (D9), which leaves dark again the ones it lights
                // not.
                if rules.guards.never_dark && !lit.get() {
                    return false;
                }
                let display = identity::display(named, &at, 0);
                place_display(&rules, &display)
                    .map_err(|line| log::info!("{line}"))
                    .is_err()
            }));
            let k = key.clone();
            drm.insert(&app.handle, move |app: &mut App| {
                app.videos
                    .get_mut(&k)
                    .and_then(|v| v.as_any().downcast_mut::<Drm2d<Drm>>())
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
    app.cards.insert(key.clone(), card);
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
#[expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]
mod tests {
    use std::any::Any;

    use vtium_platform::display::{Canvas, Identity, Rect, SwapError, Swapped};
    use vtium_platform::event_loop::Loop;
    use vtium_platform::vt::FakeVt;

    use super::*;

    /// A device in memory with the displays `ids`, in the order it lists
    /// them, each 320x160 and named for the rules by its connector on the
    /// card `mem`.
    struct Still {
        ids: Vec<(DisplayId, &'static str)>,
        pixels: Vec<u8>,
        events: Vec<Event>,
        awake: bool,
    }

    impl Still {
        fn new(ids: &[(u32, &'static str)]) -> Still {
            Still {
                ids: ids
                    .iter()
                    .map(|&(id, name)| (DisplayId(id), name))
                    .collect(),
                pixels: vec![0; 320 * 160 * 4],
                events: Vec::new(),
                awake: false,
            }
        }
    }

    impl Video for Still {
        fn backend(&self) -> &'static str {
            "still"
        }
        fn events(&mut self) -> Vec<Event> {
            std::mem::take(&mut self.events)
        }
        fn displays(&self) -> Vec<DisplayId> {
            if self.awake {
                self.ids.iter().map(|&(id, _)| id).collect()
            } else {
                Vec::new()
            }
        }
        fn name(&self, id: DisplayId) -> String {
            self.ids
                .iter()
                .find(|&&(i, _)| i == id)
                .map_or("?", |&(_, n)| n)
                .to_owned()
        }
        fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
            Some((320, 160))
        }
        fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
            None
        }
        fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
            Some((72.0, 72.0))
        }
        fn identity(&self, id: DisplayId) -> Option<Identity> {
            Some(Identity {
                connector: self.name(id),
                card: Some("mem".to_owned()),
                edid: None,
                lit_at_start: false,
            })
        }
        fn canvas(&mut self, _: DisplayId) -> Option<Canvas<'_>> {
            Some(Canvas {
                pixels: &mut self.pixels,
                width: 320,
                height: 160,
                stride: 320 * 4,
                age: 0,
            })
        }
        fn swap(&mut self, _: DisplayId, _: &[Rect]) -> Result<Swapped, SwapError> {
            Ok(Swapped::Frame)
        }
        fn is_flipping(&self, _: DisplayId) -> bool {
            false
        }
        fn set_dpms(&mut self, _: DisplayId, _: Dpms) -> io::Result<()> {
            Ok(())
        }
        fn sleep(&mut self) {
            self.awake = false;
        }
        fn wake_up(&mut self) -> io::Result<()> {
            if !self.awake {
                self.awake = true;
                self.events
                    .extend(self.ids.iter().map(|&(id, _)| Event::Added(id)));
            }
            Ok(())
        }
        fn unblank(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn input(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn hotplug(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn as_any(&mut self) -> &mut dyn Any {
            self
        }
    }

    /// The seat at 16 points on 72 DPI, with the head file `heads`, the
    /// font sources `fonts` and the device `video` at `/dev/mem0`, before
    /// its first turn.
    fn seat(heads: Option<&str>, fonts: font::Sources) -> (Loop<App>, App) {
        let args: Vec<OsString> = ["--font-size=16", "--login", "--", "/bin/true"]
            .iter()
            .map(OsString::from)
            .collect();
        let loaded = crate::config::load(&args, None, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().unwrap();
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            Rc::new(Instant::now),
            Vec::new(),
        );
        app.set_font_sources(fonts);
        if let Some(text) = heads {
            app.set_heads(vtium_config::parse(text).unwrap().config);
        }
        (lp, app)
    }

    fn no_fonts() -> font::Sources {
        font::Sources {
            frits: PathBuf::from("/nonexistent/vtium-test-frits"),
            helper: None,
            vconsole_conf: PathBuf::from("/dev/null"),
            kernel_cmdline: PathBuf::from("/dev/null"),
        }
    }

    /// Shows the VT and runs a turn, which sets every device up.
    fn shown(app: &mut App) {
        app.vt.activate().unwrap();
        app.turn(Instant::now());
    }

    // ADR-1401 D9 and vtium-config's Identity: a display's order is its
    // place in its card's connector list, which the boot display is picked
    // by, whatever its KMS object id.
    #[test]
    fn a_displays_order_is_its_place_in_the_list_not_its_object_id() {
        let (_lp, mut app) = seat(None, no_fonts());
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(9, "A"), (3, "B")])),
        );
        shown(&mut app);
        let order = |id: u32| {
            app.lit
                .get(&(PathBuf::from("/dev/mem0"), DisplayId(id)))
                .map(|d| d.order)
        };
        assert_eq!((order(9), order(3)), (Some(0), Some(1)));
    }

    // ADR-1401 D5: the focus starts on a head with a display that draws; a
    // display whose font cannot be set up is no lit display of its head.
    #[test]
    fn a_display_whose_font_fails_takes_no_focus() {
        let heads = "version = 1\n[head a]\n[head b]\n[rules]\ndisplay connector:mem/A -> a\ndisplay connector:mem/B -> b\n";
        let (_lp, mut app) = seat(Some(heads), no_fonts());
        if let Some(head) = app.heads.first_mut() {
            head.font = Some(Chosen::Builtin(Some("no-such-set")));
        }
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A"), (2, "B")])),
        );
        shown(&mut app);
        assert!(
            app.heads.first().is_some_and(|h| h.views.is_empty()),
            "a draws nothing"
        );
        assert_eq!(app.lit.len(), 1);
        assert_eq!(app.focus, Some(1), "the focus starts on b");
    }

    // ADR-1401 D1: a frit's helper fills the cells of the heads drawing
    // it. Once one of two heads drawing `t` switches away by OSC 50, `t`
    // keeps the other head's cell alone.
    #[test]
    fn a_frit_a_head_left_keeps_the_cells_of_the_heads_still_drawing_it() {
        let dir = std::env::temp_dir().join(format!("vtium-app-left-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.vtfrit"), "source=builtin:unifont\n").unwrap();
        let fonts = font::Sources {
            frits: dir.clone(),
            helper: Some(helper::Command {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".into(), "exec sleep 30".into()],
                account: None,
            }),
            ..no_fonts()
        };
        let heads = "version = 1\n[head a]\nfont-name = t\n[head b]\nfont-name = t\nfont-size = 32\n[rules]\ndisplay connector:mem/A -> a\ndisplay connector:mem/B -> b\n";
        let (_lp, mut app) = seat(Some(heads), fonts);
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A"), (2, "B")])),
        );
        shown(&mut app);
        let asked = |app: &App| {
            let mut cells: Vec<(u32, u32)> = app
                .frits
                .get("t")
                .map(|f| f.asked.iter().map(|c| (c.width, c.height)).collect())
                .unwrap_or_default();
            cells.sort_unstable();
            cells
        };
        assert_eq!(asked(&app), [(8, 16), (16, 32)]);
        app.switch_font(0, "unifont");
        assert_eq!(app.heads.first().and_then(Head::frit), None);
        assert_eq!(asked(&app), [(16, 32)], "b's cell alone");
        drop(app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ADR-1401 D3: kmscon.conf's `mouse` off drops the pointer rules from
    // the shipped rules, so a mouse falls to `input * -> ignore` and is
    // never opened; a keyboard still follows the focus.
    #[test]
    fn mouse_off_closes_a_pointer_the_shipped_rules_would_deliver() {
        let args: Vec<OsString> = ["--no-mouse"].iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, None, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().unwrap();
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            Rc::new(Instant::now),
            Vec::new(),
        );
        let input = vtium_config::Identity::input;
        app.add_input(
            Path::new("/m"),
            input().with_class(vtium_config::Class::Mouse),
        );
        app.add_input(
            Path::new("/k"),
            input().with_class(vtium_config::Class::Keyboard),
        );
        assert_eq!(app.routes.head(Some(Path::new("/m")), Some(0)), None);
        assert_eq!(app.routes.head(Some(Path::new("/k")), Some(0)), Some(0));
        // A mouse whose node also has udev's key class, its consumer keys,
        // is still a pointer the shipped rules leave closed.
        let keyed = input()
            .with_class(vtium_config::Class::Mouse)
            .with_class(vtium_config::Class::Key);
        let rules = app.rules.clone();
        assert_eq!(
            app.routes.add(&rules, Path::new("/mk"), &keyed),
            vtium_platform::input::Disposition::Closed
        );
    }

    // ADR-1401 D10: a head file's heads past `consoles-max` each keep a
    // console, and every console's scrollback together stays within the
    // most `sb-memory` may be: 9 heads take a ninth each.
    #[test]
    fn a_head_file_sets_each_consoles_scrollback_by_its_heads() {
        let args: Vec<OsString> = ["--sb-memory=1G"].iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, None, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().unwrap();
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            Rc::new(Instant::now),
            Vec::new(),
        );
        let mut text = String::from("version = 1\n");
        for n in 0..9 {
            text.push_str(&format!("[head h{n}]\n"));
        }
        app.set_heads(vtium_config::parse(&text).unwrap().config);
        let cap = usize::try_from(crate::config::budget::sb_memory_cap()).unwrap();
        assert_eq!(app.scrollback_bytes, cap / 9);
    }

    // ADR-1401 D8: the loop wakes when a head's `on-loss` time runs out.
    #[test]
    fn the_loop_wakes_when_a_heads_hangup_falls_due() {
        let (_lp, mut app) = seat(Some("version = 1\n[head a]\n"), no_fonts());
        let t = Instant::now();
        assert!(app.next_wake(t) > Some(Duration::from_secs(30)));
        if let Some(head) = app.heads.first_mut() {
            head.hangup_due = Some(t + Duration::from_secs(30));
        }
        assert_eq!(app.next_wake(t), Some(Duration::from_secs(30)));
    }

    // ADR-1401 D5: mode 1004's focus goes to a head a keyboard delivers
    // to; a keyboard that could not be opened delivers nothing.
    #[test]
    fn a_keyboard_that_failed_to_open_gives_no_head_the_focus() {
        use vtium_platform::input::{Direct, Input, InputConfig, Keymap};
        let (_lp, mut app) = seat(None, no_fonts());
        let mut input = Input::new(Direct, Keymap::fallback().unwrap(), InputConfig::default());
        input.wake_up();
        app.input = Some(input);
        app.focus = Some(0);
        let keyboard = vtium_config::Identity::input().with_class(vtium_config::Class::Keyboard);
        app.add_input(Path::new("/nonexistent/vtium-test/event9"), keyboard);
        app.report_focus();
        assert_eq!(app.heads.first().map(|h| h.focused), Some(false));
    }

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
}
