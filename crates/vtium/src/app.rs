//! The seat and its heads, wired: the VT's events, the video devices and
//! their displays, the input, the DPMS blank, each head's pty and child,
//! and when each head draws. The loop calls [`App::turn`] after each
//! dispatch with the time, and waits at most [`App::next_wake`].
//!
//! Each step below says what vtium does at that point of the turn, with
//! the devices vtium-platform's host list names:
//!
//! - **Heads** (ADR-1401 D1, D7). What the head file resolves to
//!   ([`Start`]): its heads, or the compiled-in configuration's one
//!   `console` head, each a [`Head`] with its own session, grid, density,
//!   zoom and rotation, and its console line on every head. Each display
//!   walks the rules alone when it comes, and joins the head the first
//!   match names; a display the rules send `off`, or none claims, is left
//!   off and logged, with the rule line that would claim it ready to
//!   paste. Within a head the displays mirror one grid by the scaled rule
//!   ([`crate::layout`]). Under the closed policy the display guard lights
//!   the boot display for a notice alone on a screen no head holds.
//! - **The VT.** `Acquired` wakes the input and every device, which draw
//!   whole; `Release` deactivates the terminals, puts the input and every
//!   device to sleep, and acknowledges once each reported `Asleep`;
//!   `Retaken` sets every device's displays up again, or while blanked
//!   at the input that ends the blank, and keeps the blank and its timer;
//!   `Hangup` exits.
//! - **Inputs** (D3, D5). Each input device walks the rules alone when it
//!   comes, by its identity: a device a rule sends to a head delivers to
//!   that head alone, one sent to `roaming`, or claimed by no rule under
//!   the open policy, delivers to the active head, and one sent to
//!   `ignore` is never opened, or under open opened for the rescue key
//!   alone. The focus starts on the
//!   head of the boot display, else the first head in the file with a
//!   display, starting again as each display lights until the first key
//!   reaching a head or the first focus chord, over every display lit
//!   since the start, one that left included; then only the focus chord
//!   moves it. The terminal of each head a keyboard delivers to has mode
//!   1004's focus.
//! - **Keys**, in order: the VT's, from a keyboard asking a head that is
//!   not isolated (D5), then the seat's (the power keys, which act on a
//!   press and never on its repeat), then the heads' chords ([`chord`]),
//!   the roaming chord from a roaming keyboard alone, then the terminal's
//!   grabs, then DECARM's drop of a repeat, then the encoder. A key draws
//!   at once and holds the cursor's blink.
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
//!   console chords show them. A console's program exiting follows the
//!   respawn policy, which starts it again in its place or ends the
//!   console, and vtium ends once every head has. A crash loop ends the
//!   run: under open vtium exits, and under closed it holds its VT with a
//!   notice and delivers nothing.
//! - **No display.** From the acquire, vtium waits `display-wait` for a
//!   frame on a display's screen, and with none by then it ends; a
//!   release before the first frame stops the wait until the VT comes
//!   back ([`crate::wait`]); under the closed policy it waits however long it
//!   takes (D8). Once one has come, a head's last going does what
//!   its `when-display-lost` says (D8): under `keep` its sessions run on at the
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

use vtium_devices::Card;
use vtium_font::spacing::SpacedCell;
use vtium_platform::calloop::LoopHandle;
use vtium_platform::display::{Blank, Buffers, DPI_ASSUMED, DisplayId, Dpms, Event};
use vtium_platform::input::{
    DeviceId, DeviceInput, InputEvent, KeyEvent, Mods as InputMods, PointerEvent,
};
use vtium_platform::pty::{Erase, Pty};
use vtium_platform::udev::{DeviceEvent, DeviceKind};
use vtium_platform::vt::{FakeVt, Switch, Vt, VtEvent};
use vtium_render::{Config as RenderConfig, Depth, DrawState, Orientation};
use vtium_settings::{Policy, WhenDisplayLost};
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
use crate::head::route::{self, Routes};
use crate::head::session::{self, AfterExit, Caps};
use crate::head::{Attached, Frit, Head, Start, place_display};
use crate::helper::{self, Answer, Helper};
use crate::layout::{self, Zoom};
use crate::osd::{self, Osd};
use crate::painter::Painter;
use crate::power::{self, Power, PowerKey};
use crate::respawn;
use crate::sync::Action;
use crate::term::{Clock, Prepared, Term, View};
use crate::video::Video;
use crate::wait::Wait;

mod survive;

/// How long the loop is served at exit while the switch back is pending,
/// so the kernel's SIGUSR2 for the switch back is answered before vtium
/// ends.
pub const LEAVE_WAIT: Duration = Duration::from_millis(100);

/// How long the on-screen display's brief line shows where `osd-timeout`
/// is `never` (ADR-1401 D5): it answers one press, so it still goes.
const BRIEF_NEVER: Duration = Duration::from_secs(3);

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
    rules: vtium_settings::Config,
    /// What the start's head file resolved to (ADR-1401 D7).
    outcome: vtium_settings::Outcome,
    /// The panic screen's report, for a broken closed head file (D7).
    report: Option<vtium_render::NoticeReport>,
    /// The index in [`App::heads`] of the screen the display guard lights
    /// for a notice alone, which a closed policy has, after every head the
    /// file declares (D9).
    screen: Option<usize>,
    /// A crash loop ended every head's sessions under the closed policy:
    /// no session starts again, no input is delivered, and the boot
    /// display shows why for as long as vtium runs (D1).
    stopped: bool,
    /// The head the inputs following the focus deliver to.
    focus: Option<usize>,
    /// Where each input device's events go.
    routes: Routes,
    /// The head the focus moved to since [`App::take_focus_move`].
    focus_move: Option<usize>,
    /// The on-screen display, `osd` and `osd-timeout` (ADR-1401 D5).
    osd: Osd,
    /// A key came or the focus chord moved the focus: the focus no longer
    /// follows a boot display lighting late.
    focus_settled: bool,
    /// A display lit since the focus was last placed.
    focus_due: bool,
    /// What the rules and the display guard read of each card.
    cards: BTreeMap<PathBuf, Card>,
    /// Each display lit in a head, as the rules saw it.
    lit: BTreeMap<(PathBuf, DisplayId), vtium_settings::Identity>,
    /// Each display lit since the start until the focus settled, gone or
    /// not: the focus starts over them, so a display leaving never moves
    /// it (ADR-1401 D8).
    start_lit: BTreeMap<(PathBuf, DisplayId), vtium_settings::Identity>,
    /// The displays a device announced that the rules leave dark, as the
    /// rules saw them.
    dark: BTreeMap<(PathBuf, DisplayId), vtium_settings::Identity>,
    /// The display the display guard lit, and its head (ADR-1401 D9).
    guard_lit: Option<((PathBuf, DisplayId), usize)>,
    /// A device came or went: the guards run at the turn's end.
    guard_due: bool,
    /// The anti-lockout guard holds every keyboard on the focus.
    locked_out: bool,
    /// The start's scan is queued: the turn says whether it found a
    /// keyboard.
    keyboard_check: bool,
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
    /// What makes each console's image helper host, from the program
    /// opened at start (ADR-1101 D1); with none, every terminal is told no
    /// helper runs.
    #[cfg(feature = "images")]
    image_helpers: Option<crate::head::images::Helpers>,
    /// The displays whose density is assumed, each warned of once.
    assumed: BTreeSet<(PathBuf, DisplayId)>,
    /// Where the font's sets come from.
    font_sources: font::Sources,
    /// The GL context a head with `hwaccel` on makes, in place of its
    /// card's render node: a test's GPU.
    gpu_source: Option<crate::painter::GpuSource>,
    /// Reads the configuration again as the start read it, for the font
    /// keys a session's end returns its console to (ADR-1201 D11); with
    /// none, the configuration vtium started with stands.
    reread: Option<Reread>,
    /// The glyph helper's program, opened at the start, which every helper
    /// starts from; why not, when it cannot be.
    launcher: Result<helper::Launcher, String>,
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
    /// `oneshot`: a console's exit ends that console, and a head ends once
    /// every one of its consoles has.
    oneshot: bool,
    /// The `mode` lines, for each DRM device opened.
    modes: Vec<ModeSpec>,
    /// `use-original-mode`, off when a mode is set.
    use_original: bool,
    /// The buffers each DRM display keeps (ADR-1401 D2).
    display_buffers: Buffers,
    /// `clipboard-write`: OSC 52's selection is taken.
    clipboard_write: bool,
    /// Where each session's pty master is kept through a restart, and
    /// the masters an earlier vtium left (ADR-1401 D13).
    survival: survive::Survival,
    /// `program-blank`: a program's blank sequences are taken.
    program_blank: bool,
    /// `bell`: a BEL from a program rings the VT's bell.
    bell: bool,
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
    /// The wheel scrolls and reports the other way (vtium/vtium#203).
    natural_scrolling: bool,
    /// The power keys the configuration binds, and what they act through.
    power_keys: Vec<(PowerKey, Vec<Chord>)>,
    power: Box<dyn Power>,
    /// The child writing `/sys/power/state` has not exited: the machine
    /// sleeps, or is going to.
    sleeping: Rc<Cell<bool>>,
}

/// Reads the configuration again, as [`crate::config::load`] read it at
/// the start.
pub type Reread = Box<dyn FnMut() -> crate::config::Loaded>;

impl Drop for App {
    /// Lets go of every head's render thread at once before the heads drop,
    /// so heads whose GPU driver hangs hold vtium's exit
    /// [`crate::painter::LET_GO_MS`] in all, not that for each head.
    fn drop(&mut self) {
        self.let_go_of_painters();
    }
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
        let rules = route::in_force(vtium_settings::Config::builtin(), &config);
        let caps = Caps {
            head: config.number(&keys::HEAD_CONSOLES_MAX).map_or(
                to_usize(vtium_settings::HEAD_CONSOLES_DEFAULT.into()),
                to_usize,
            ),
            total: config.number(&keys::CONSOLES_MAX).map_or(
                to_usize(vtium_settings::CONSOLES_TOTAL_DEFAULT.into()),
                to_usize,
            ),
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
        let image_program = crate::head::images::program(&config);
        #[cfg(feature = "images")]
        let image_helpers = Some(crate::head::images::hosts(
            &image_program,
            crate::head::images::tmpdir(&inherited),
        ));
        #[cfg(not(feature = "images"))]
        let _ = image_program;
        App {
            oneshot: config.switch(&keys::ONESHOT),
            clipboard_write: config.switch(&keys::CLIPBOARD_WRITE),
            program_blank: config.switch(&keys::PROGRAM_BLANK),
            bell: config.switch(&keys::BELL),
            dpms_timeout: dpms,
            order: None,
            selection: None,
            paste_keys: config.chords(&keys::GRAB_PASTE).to_vec(),
            hide_while_typing: config.switch(&keys::POINTER_HIDE_WHILE_TYPING),
            natural_scrolling: config.switch(&keys::NATURAL_SCROLLING),
            power_keys,
            power: Box::new(power::System),
            sleeping: Rc::default(),
            modes,
            use_original,
            display_buffers: crate::video::display_buffers(config.display_buffers()),
            vt,
            videos: BTreeMap::new(),
            input: None,
            udev: None,
            blank: Blank::new(dpms, now),
            heads,
            rules,
            outcome: vtium_settings::resolve(&vtium_settings::Read::Missing),
            report: None,
            screen: None,
            stopped: false,
            focus: None,
            routes: Routes::default(),
            focus_move: None,
            osd: Osd::configured(&config),
            focus_settled: false,
            focus_due: false,
            cards: BTreeMap::new(),
            lit: BTreeMap::new(),
            start_lit: BTreeMap::new(),
            dark: BTreeMap::new(),
            guard_lit: None,
            guard_due: false,
            locked_out: false,
            keyboard_check: false,
            any_lit: Rc::default(),
            scrollback: (cap, lines),
            scrollback_bytes: bytes,
            caps,
            assumed: BTreeSet::new(),
            font_sources: font::Sources::configured(&config),
            gpu_source: None,
            reread: None,
            launcher: helper::Launcher::configured(&config),
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
            #[cfg(feature = "images")]
            image_helpers,
            exit: None,
            leaving: None,
            // No session inherits systemd's word to vtium (ADR-1401 D13).
            inherited: crate::survive::session_env(inherited),
            survival: survive::Survival::default(),
        }
    }

    /// Runs what the start's head file resolved to (ADR-1401 D7) in place
    /// of the compiled-in configuration, before any display comes: its
    /// heads, each with no session yet, its rules for each display that
    /// comes, and its console line on every head. Under the closed policy
    /// vtium waits for a display however long it takes (D8), and keeps a
    /// screen for the display guard to light for a notice alone (D9).
    pub fn set_heads(&mut self, start: Start) {
        let notice = start.notice();
        let Start {
            outcome,
            rules,
            report,
        } = start;
        let rules = route::in_force(rules, &self.config);
        // The display guard's default head, where the file declares none,
        // which it lights under open alone (D9).
        let mut declared = rules.clone();
        if declared.heads.is_empty() && rules.policy == Policy::Open {
            declared.heads = vtium_settings::Config::builtin().heads;
        }
        let caps = self.caps;
        self.scrollback_bytes = session::scrollback_bytes(&self.config, caps, declared.heads.len());
        let scrollback = (self.scrollback, self.scrollback_bytes);
        self.let_go_of_painters();
        self.heads = session::heads(&declared, &self.config, scrollback, caps, &self.clock);
        self.screen = (rules.policy == Policy::Closed).then(|| {
            let term = crate::head::terminal(&self.config, (0, 0), 0, Rc::clone(&self.clock));
            self.heads.push(Head::notice_screen(&self.config, term));
            self.heads.len().saturating_sub(1)
        });
        if outcome.display_wait() == vtium_settings::DisplayWait::Forever {
            self.wait = Wait::new(None);
        }
        self.rules = rules;
        self.outcome = outcome;
        self.report = report;
        self.focus = None;
        self.lit.clear();
        self.start_lit.clear();
        self.dark.clear();
        self.frits.clear();
        // a report's errors were logged as the file was read, each with
        // its line, so the log gets its console line alone
        if let Some(line) = self.report.as_ref().and(self.outcome.line) {
            log::warn!("{}", line.text());
        } else if let Some(notice) = notice {
            log::warn!("{notice}");
            self.notice(&notice);
        }
    }

    /// The start's device scan is queued: once the turn has placed its
    /// devices, a seat with no keyboard among them says so on every head
    /// (ADR-1401 D8).
    pub fn scanned(&mut self) {
        self.keyboard_check = true;
    }

    /// Shows vtium's notice `text` on every head, before its login, until
    /// a key reaches the head or for [`crate::head::NOTICE_SHOWN`]
    /// (ADR-1401 D7).
    pub fn notice(&mut self, text: &str) {
        for head in self.heads.iter_mut().filter(|h| !h.notice_only) {
            head.add_notice(text);
        }
    }

    /// Puts the power keys' actions through `power`, a test's stand-in
    /// for PID 1, logind and the kernel.
    pub fn set_power(&mut self, power: Box<dyn Power>) {
        self.power = power;
    }

    /// Makes each console's image helper host with `helpers` in place of
    /// the hosts the start made, or, with none, tells every terminal that
    /// no helper runs, from the next turn on (ADR-1100 D7): a test's
    /// stand-in for the image helper. A console keeps the host it already
    /// has.
    #[cfg(feature = "images")]
    pub fn set_image_helpers(&mut self, helpers: Option<crate::head::images::Helpers>) {
        self.image_helpers = helpers;
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
        if let Some(command) = &sources.helper {
            self.launcher = helper::Launcher::open(command.clone());
        }
        self.font_sources = sources;
        for head in &mut self.heads {
            head.font = None;
        }
        self.frits.clear();
    }

    /// Makes every head whose `hwaccel` is on make its GL context from
    /// `source`, in place of the render node of its card: a test's GPU.
    pub fn set_gpu_source(&mut self, source: crate::painter::GpuSource) {
        self.gpu_source = Some(source);
    }

    /// How the configuration is read again at a session's end, for its
    /// font keys (ADR-1201 D11): `main` gives the reader it starts with,
    /// on the same command line and seat.
    pub fn set_config_reader(&mut self, read: Reread) {
        self.reread = Some(read);
    }

    /// The seat's keyboards and pointers. On a fake VT, the app holds one
    /// wake-up of them for as long as it runs, so they stay open while the
    /// VT is released and the
    /// Ctrl+Super+F12 that takes it back can arrive. A kernel VT's
    /// release closes them, and its acquire opens them again.
    pub fn set_input(&mut self, mut input: vtium_platform::input::Input) {
        if self.vt.get::<FakeVt>().is_some() {
            input.wake_up();
        }
        self.input = Some(input);
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
        let declared = self.screen.unwrap_or(self.heads.len());
        self.heads.get(..declared).unwrap_or_default()
    }

    /// The head named `name`.
    pub fn head(&self, name: &str) -> Option<&Head> {
        self.heads().iter().find(|h| h.name() == name)
    }

    /// The screen the display guard lights for a notice alone under the
    /// closed policy (ADR-1401 D9), which a closed file has.
    pub fn notice_screen(&self) -> Option<&Head> {
        self.heads.get(self.screen?)
    }

    /// The head the keys type into, else the first: the one whose session
    /// [`App::grid`], [`App::screen_text`] and [`App::term`] read.
    fn shown(&self) -> Option<&Head> {
        self.heads().get(self.focus.unwrap_or(0))
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
                !head.notice_only
                    && !head.started()
                    && respawn::first_start_due(self.awake, !head.views.is_empty())
            });
            if !due {
                continue;
            }
            // A head with sessions that outlived the last vtium opens those
            // consoles alone, keeping their ids (ADR-1401 D13).
            let restored = self.open_kept(h);
            let App {
                heads,
                config,
                scrollback,
                scrollback_bytes,
                clock,
                ..
            } = self;
            let Some(head) = heads.get_mut(h) else {
                continue;
            };
            if !restored {
                head.open_wanted(|| {
                    crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock))
                });
            }
            for id in head.start_consoles(since) {
                self.start_console(h, id, since);
            }
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
            let lost = self
                .heads
                .get_mut(h)
                .is_some_and(|head| head.drop_views(|(n, _)| n == node));
            if lost {
                self.relayout(h);
            }
        }
        self.retaking.remove(node);
        if let Some(r) = &mut self.releasing {
            r.remove(node);
        }
    }

    /// Where head `h`'s console `id` runs its session.
    fn place(&self, h: usize, id: u64) -> Place<'_> {
        let kms = self.videos.values().any(|v| v.backend() == "drm2d");
        Place {
            seat: Some(self.vt.seat()),
            vtnr: self.vt.number().map(u32::from),
            display: if kms || self.videos.is_empty() {
                SessionDisplay::Kms
            } else {
                SessionDisplay::Fbdev
            },
            console: self.heads.get(h).map(|head| (head.name(), id)),
        }
    }

    /// Starts the session of head `h`'s console `id`, unless a crash loop
    /// stopped every head under the closed policy (ADR-1401 D1). A session
    /// that cannot start ends vtium, so the unit's getty fallback takes the
    /// VT.
    fn spawn(&mut self, h: usize, id: u64) {
        if let Err(why) = self.start_session(h, id) {
            log::error!("{why}");
            self.request_exit(Exit::Spawn);
        }
    }

    /// Starts the session of head `h`'s console `id`, as [`App::spawn`]
    /// does; the error is why it cannot start, for the caller to act on.
    fn start_session(&mut self, h: usize, id: u64) -> Result<(), String> {
        if self.stopped {
            return Ok(());
        }
        let config = self
            .heads
            .get(h)
            .and_then(|head| head.login.as_ref())
            .unwrap_or(&self.config);
        let spec = child::child(config, &self.place(h, id), self.inherited.clone());
        let command = spec.command().ok_or("no command to run")?;
        let erase = if self.config.switch(&keys::BACKSPACE_DELETE) {
            Erase::Delete
        } else {
            Erase::Backspace
        };
        let Some(size) = self.heads.get(h).and_then(|head| head.console_pty_size(id)) else {
            return Ok(());
        };
        let mut pty = Pty::spawn(command, size, erase)
            .map_err(|e| format!("cannot start the session: {e}"))?;
        self.watch(h, id, &mut pty)
            .map_err(|e| format!("cannot watch the session's pty: {e}"))?;
        let pty_id = self.next_pty_id();
        if let Some(console) = self.heads.get_mut(h).and_then(|head| head.console_mut(id)) {
            console.pty_id = Some(pty_id);
            console.pty = Some(pty);
        }
        Ok(())
    }

    /// Puts `pty`, the session of head `h`'s console `id`, in the loop,
    /// which feeds its output to the console's terminal.
    fn watch(&self, h: usize, id: u64, pty: &mut Pty) -> io::Result<()> {
        pty.insert(&self.handle, move |app: &mut App| {
            let Console { pty, term, .. } = app.heads.get_mut(h)?.console_mut(id)?;
            pty.as_mut().map(|p| (p, term))
        })
    }

    /// Chooses head `h`'s font once, at its first display (ADR-1200 D9,
    /// ADR-1401 D3), as [`App::configured_font`] gives it.
    fn choose_font(&mut self, h: usize) {
        if self.heads.get(h).is_none_or(|head| head.font.is_some()) {
            return;
        }
        let chosen = self.configured_font(h);
        if let Some(head) = self.heads.get_mut(h) {
            head.font = Some(chosen);
        }
        self.share(h);
    }

    /// The configuration head `h` reads the keys a section may set from:
    /// the top level, with the command line over it, and the head's
    /// sections over both (ADR-1403 D5). With no such head, the top level.
    fn head_config(&self, h: usize) -> Config {
        match self.heads.get(h) {
            Some(head) if !head.settings.is_empty() => {
                let path = crate::config::path(&self.config);
                self.config.for_head(&path, &head.settings).0
            }
            _ => self.config.clone(),
        }
    }

    /// The font the configuration gives head `h` (ADR-1200 D9, ADR-1401
    /// D3): the head's own font, else the frit `font-name` or vconsole
    /// names, or the one-source frit made on the fly for a kbd name, a
    /// path or vconsole's font, generated at run time (ADR-1301 D2); else
    /// the built-in font, with one line naming the font and why. The font
    /// draws in the cell the head's look states, the built-in font as a
    /// frit does (ADR-1200 D7, ADR-1403 D5).
    fn configured_font(&self, h: usize) -> Chosen {
        let Some(head) = self.heads.get(h) else {
            return Chosen::BUILTINS;
        };
        let look = head.look_cell();
        let s = &self.font_sources;
        let wanted = match &head.font_name {
            Some(name) => Some(font::Wanted {
                name: name.clone(),
                came: font::Came::Config,
            }),
            None => font::wanted(
                &self.head_config(h),
                &font::read_text(&s.vconsole_conf),
                &font::read_text(&s.kernel_cmdline),
            ),
        };
        let Some(wanted) = wanted else {
            return Chosen::builtins(look);
        };
        if let Some(builtin) = font::builtin_named(&wanted.name, look) {
            return builtin;
        }
        // The frit's cell is read as text before the first frame; its fonts
        // are the helper's to parse (#145).
        let chosen = font::frit_named(&s.frits, &wanted)
            .and_then(|frit| Chosen::frit(frit, head.sized(), look));
        match chosen {
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
                Chosen::builtins(look)
            }
        }
    }

    /// A frit that states no cell takes the cell its first source gives at
    /// `height`, the height head `h` draws at, which the helper is asked
    /// for and waited for at most [`helper::OWN_CELL_MS`], the one wait on
    /// the helper (ADR-1200 D17): a bitmap source's own cell, where no
    /// `font-size` is configured, so a bitmap font draws at its own size
    /// (the operator, 2026-10-28), or an outline source's cell at
    /// `height` (the maintainer, 2026-11-01). Each answer is kept for the
    /// frit, by the height asked at, for every head drawing it. A refusal,
    /// or no answer in time, leaves the display's cell, or for an outline
    /// frit the built-in font's width, with one line saying why; an answer
    /// that comes after the wait is taken by [`App::late_cell`].
    fn own_cell(&mut self, h: usize, height: u32) {
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let sized = head.sized();
        if !head
            .font
            .as_ref()
            .is_some_and(|f| f.wants_own_cell(height, sized))
        {
            return;
        }
        let outline = head.chosen().outline();
        let Some(name) = head.frit().map(str::to_owned) else {
            return;
        };
        let known = self
            .frits
            .get(&name)
            .and_then(|f| f.answer_at(height, outline));
        let own = match known {
            Some(own) => own,
            None => {
                let clock = Rc::clone(&self.clock);
                let Some(helper) = self.helper(&name) else {
                    return;
                };
                let wait = Duration::from_millis(helper::OWN_CELL_MS);
                let own = helper.own_cell(height, wait, &*clock);
                match (&own, outline) {
                    (Ok(helper::OwnCell::Bitmap(cell)), _) => log::info!(
                        "the frit {name} draws in its fonts' own {}x{} cell",
                        cell.width,
                        cell.height
                    ),
                    (Ok(helper::OwnCell::Outline(cell)), _) => log::info!(
                        "the frit {name} draws in its outline font's {}x{} cell at {height} pixels",
                        cell.width,
                        cell.height
                    ),
                    (Err(why), false) => log::warn!(
                        "the frit {name}'s own cell is not known ({why}): it draws in the display's cell"
                    ),
                    (Err(why), true) => log::warn!(
                        "the frit {name}'s cell at {height} pixels is not known ({why}): it draws as wide as the built-in font"
                    ),
                }
                if let Some(frit) = self.frits.get_mut(&name) {
                    frit.own.insert(height, own.clone());
                }
                own
            }
        };
        if let (Ok(answer), Some(head)) = (own, self.heads.get_mut(h))
            && let Some(font) = &mut head.font
        {
            font.set_own_cell(answer, sized);
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
                        own: BTreeMap::new(),
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
            Chosen::Builtin { name: Some(n), .. } => *n == name,
            Chosen::Builtin { name: None, .. } => false,
        };
        if same {
            return;
        }
        let (sized, look) = (head.sized(), head.look_cell());
        let was = head.frit().map(str::to_owned);
        let new = font::by_name(&self.font_sources, name, sized, look).and_then(|new| {
            let Some(head) = self.heads.get_mut(h) else {
                return Ok(());
            };
            let old = head.font.replace(new);
            self.share(h);
            self.retry_late_cell(h);
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
                head.switched = Some(head.console.id);
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

    /// What `render-timer` asks for: whether the console writes each
    /// display's figures to the log, and whether it draws them on screen.
    fn render_timer(&self) -> (bool, bool) {
        match self.config.text(&keys::RENDER_TIMER) {
            Some("log") => (true, false),
            Some("osd") => (false, true),
            Some("both") => (true, true),
            _ => (false, false),
        }
    }

    /// Whether the console measures each display's frames, for the log,
    /// the screen or both.
    fn measure(&self) -> bool {
        let (log, osd) = self.render_timer();
        log || osd
    }

    /// The rules' identity of the display `id` of `node`, its place in
    /// its card's list the place among the displays its device lists, in
    /// the kernel's order of connectors, whatever their object ids.
    fn identity(&self, node: &Path, id: DisplayId) -> Option<vtium_settings::Identity> {
        let video = self.videos.get(node)?;
        let card = self.cards.get(node).cloned().unwrap_or_default();
        let order = video
            .displays()
            .iter()
            .position(|&d| d == id)
            .map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX));
        Some(match video.identity(id) {
            Some(named) => vtium_devices::display(&named, &card, order),
            None => vtium_devices::unnamed_display(&video.name(id), &card, order),
        })
    }

    /// The head the display `id` of `node` joins, by the first rule that
    /// matches it, with the display as the rules saw it; a display the
    /// rules leave dark is set off and logged ([`place_display`]).
    fn head_for(
        &mut self,
        node: &Path,
        id: DisplayId,
    ) -> Option<(usize, vtium_settings::Identity)> {
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
            if let Some(video) = self.videos.get_mut(node)
                && let Err(e) = video.set_dpms(*id, Dpms::Off)
            {
                log::warn!("cannot set display {} off: {e}", video.name(*id));
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
            && self.head_config(h).number(&keys::DISPLAY_DPI).is_none()
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
                depth: depth(&self.head_config(h), levels),
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
                head.drop_views(|k| *k == key);
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
        // the line names the path that draws the display (ADR-1303 D10):
        // a render thread that has not told the loop its path yet logs it
        // with the head's other display lines once it has
        self.painter(h);
        if let Some(head) = self.heads.get(h)
            && let Some(v) = head.views.get(&key)
            && head.painter.as_ref().is_none_or(Painter::known)
        {
            let path = head.painter.as_ref().map(Painter::path).unwrap_or_default();
            display_line(&name, backend, path, v);
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
        if let Some(height) = self
            .heads
            .get(h)
            .map(|head| draw_height(head, dpi, &self.locale))
        {
            self.own_cell(h, height);
        }
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
        head.font_size = draw_height(head, dpi, locale);
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
        if let Some(dpi) = self.head_config(h).number(&keys::DISPLAY_DPI) {
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
        // each cell with the rows line spacing adds to it (ADR-1302 D13):
        // two heads can draw one cell size with different bands, and each
        // band has glyphs of its own
        let mut cells: Vec<SpacedCell> = drawing
            .iter()
            .flat_map(|head| head.views.values().map(View::spaced))
            .collect();
        cells.sort_unstable_by_key(|a| (a.cell.height, a.cell.width, a.band));
        cells.dedup();
        let now = (self.clock)();
        let Some(frit) = self.frits.get_mut(&name) else {
            return;
        };
        if let Some(helper) = &mut frit.helper {
            helper.retain(&cells, now);
        }
        frit.asked.retain(|c| cells.contains(c));
        let new: Vec<(SpacedCell, Vec<crate::deferred::Shown>)> = cells
            .into_iter()
            .filter(|c| !frit.asked.contains(c))
            .map(|at| {
                let showing = drawing
                    .iter()
                    .filter(|head| head.views.values().any(|v| v.spaced() == at));
                (at, shown(showing))
            })
            .collect();
        if new.is_empty() {
            return;
        }
        let Some(helper) = self.helper(&name) else {
            return;
        };
        for (at, visible) in &new {
            for shown in visible {
                helper.ask(at.cell, at.band, shown, now);
            }
            // the fill is regular, and draws the regular glyphs shown first
            let first = visible
                .first()
                .map(|s| s.chars.as_slice())
                .unwrap_or_default();
            helper.fill(at.cell, at.band, first, now);
        }
        if let Some(frit) = self.frits.get_mut(&name) {
            frit.asked.extend(new.into_iter().map(|(at, _)| at));
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
        for &at in &frit.asked {
            if head.views.values().any(|v| v.spaced() == at) {
                for shown in &visible {
                    crate::deferred::lock(&frit.generated).touch(at, shown);
                    helper.ask(at.cell, at.band, shown, now);
                }
            }
        }
    }

    /// The helper of the frit `name`, started on the first need with a
    /// wake-up into the loop for its answers and the store its glyphs go
    /// into; `None`, logged once, when it cannot be.
    fn helper(&mut self, name: &str) -> Option<&mut Helper> {
        let frit = self.frits.get(name)?;
        if frit.helper.is_none() && !frit.refused {
            let launcher = self.launcher.clone();
            let wake = vtium_platform::calloop::ping::make_ping()
                .map_err(|e| format!("cannot make its wake-up: {e}"))
                .and_then(|(ping, source)| {
                    self.handle
                        .insert_source(source, |(), (), _: &mut App| {})
                        .map_err(|e| format!("cannot watch its wake-up: {e}"))?;
                    Ok(ping)
                });
            let started = match launcher.and_then(|l| wake.map(|w| (l, w))) {
                Ok((launcher, ping)) => {
                    let wake = std::sync::Mutex::new(ping);
                    Ok(Helper::new(
                        launcher,
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
                        "the font helper could not draw the font ({why}), so vtium uses its built-in font, one character at a time with no combined characters"
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
    /// (ADR-1200 D7), and redraws the cells that show one. An answer to a
    /// cell query that came after its wait goes to [`App::late_cell`].
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
        let mut late = Vec::new();
        for (name, frit) in &mut self.frits {
            let Some(helper) = &mut frit.helper else {
                continue;
            };
            let answers = helper.poll(now);
            late.extend(
                helper
                    .late_cells()
                    .into_iter()
                    .map(|(height, answer)| (name.clone(), height, answer)),
            );
            if answers.is_empty() {
                continue;
            }
            // Arrival is damage, as program output is: the answers since
            // the last turn merge into one set per cell, which the next
            // frame draws once, and only the cells showing their
            // characters draw.
            let mut arrived: Vec<(SpacedCell, Vec<char>)> = Vec::new();
            for Answer { at, chars, .. } in answers {
                match arrived.iter_mut().find(|(of, _)| *of == at) {
                    Some((_, all)) => all.extend(chars),
                    None => arrived.push((at, chars)),
                }
            }
            for head in &mut self.heads {
                if head.frit() != Some(name.as_str()) {
                    continue;
                }
                // every character shown, in any style
                let mut visible: Vec<char> =
                    head.visible().into_iter().flat_map(|s| s.chars).collect();
                visible.sort_unstable();
                visible.dedup();
                for (at, chars) in &arrived {
                    let (shown, hidden): (Vec<char>, Vec<char>) =
                        chars.iter().partition(|c| visible.binary_search(c).is_ok());
                    let mut missing = false;
                    for v in head.views.values_mut() {
                        if v.spaced() == *at {
                            v.forget_glyphs(&hidden);
                            if !shown.is_empty() {
                                v.glyphs_changed(&shown);
                            }
                            missing |= v.redraws_missing();
                        }
                    }
                    // a cell that drew the font's missing glyph draws it
                    // again from the set that arrived, though no cell shows
                    // a character of the set
                    head.draw_now |= !shown.is_empty() || missing;
                }
            }
        }
        for (name, height, answer) in late {
            self.late_cell(&name, height, answer);
        }
    }

    /// The helper's answer to frit `name`'s cell query at `height`, which
    /// came after the console stopped waiting for it (ADR-1200 D17, the
    /// maintainer, 2026-11-03). It takes the place of a missing answer
    /// alone, a timeout or no answer at all, and is then logged in one
    /// line, so a refusal holds as one in time does. A cell is missing
    /// where [`Frit::answer_at`] finds no answer in time for it: for a
    /// bitmap first source's cell, at any height, and otherwise at
    /// `height`. An answer the frit already holds, as when an OSC 50 switch
    /// back asked again and its answer came in time behind this one, stays,
    /// and the late one is dropped. A cell taken lays out again each head
    /// drawing the frit that takes it: for a bitmap first source's cell
    /// every such head, whatever its height, and for an outline source's
    /// those drawing at `height` alone, so the next frame draws in it.
    fn late_cell(
        &mut self,
        name: &str,
        height: u32,
        answer: Result<helper::OwnCell, helper::NoCell>,
    ) {
        let Some(frit) = self.frits.get_mut(name) else {
            return;
        };
        let outline = match &answer {
            Ok(helper::OwnCell::Bitmap(_)) => Some(false),
            Ok(helper::OwnCell::Outline(_)) => Some(true),
            Err(_) => None,
        };
        let held = match outline {
            Some(outline) => frit.answer_at(height, outline),
            None => frit.own.get(&height).cloned(),
        };
        if held.is_some_and(|known| !known.is_err_and(|no| no.late)) {
            return;
        }
        match &answer {
            Ok(helper::OwnCell::Bitmap(cell)) => log::info!(
                "the frit {name}'s own {}x{} cell came after the wait: it draws in it from the next frame",
                cell.width,
                cell.height
            ),
            Ok(helper::OwnCell::Outline(cell)) => log::info!(
                "the frit {name}'s {}x{} cell at {height} pixels came after the wait: it draws in it from the next frame",
                cell.width,
                cell.height
            ),
            Err(why) => log::info!(
                "the helper refused the frit {name}'s own cell after the wait ({why}): it keeps the cell it draws in"
            ),
        }
        frit.own.insert(height, answer);
        let Some(outline) = outline else {
            return;
        };
        for h in 0..self.heads.len() {
            let dpi = self.dpi(h);
            let takes = self.heads.get(h).is_some_and(|head| {
                let at = draw_height(head, dpi, &self.locale);
                head.frit() == Some(name)
                    && (!outline || at == height)
                    && head
                        .font
                        .as_ref()
                        .is_some_and(|f| f.wants_own_cell(at, head.sized()))
            });
            // The cell's width can change at the height a view already
            // draws at, so each view is rebuilt with the new face.
            if takes {
                if let Some(head) = self.heads.get_mut(h) {
                    head.refont = true;
                }
                self.relayout(h);
            }
        }
    }

    /// What the console knows of frit `name`'s cell: the helper's answer to
    /// each cell query, or why there was none, by the height asked at
    /// (ADR-1200 D17).
    pub fn frit_cells(&self, name: &str) -> Vec<(u32, Result<helper::OwnCell, helper::NoCell>)> {
        self.frits.get(name).map_or_else(Vec::new, |frit| {
            frit.own.iter().map(|(h, a)| (*h, a.clone())).collect()
        })
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
            // A line measured before the VT switched away no longer
            // describes this second, so each display drops it.
            for v in head.views.values_mut() {
                v.lost = true;
                v.timer_line = None;
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
                if retake && let Err(e) = video.wake_up() {
                    log::warn!("cannot set up {} again: {e}", node.display());
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
                // The keyboards start afresh, holding no modifier, so a
                // Ctrl held before the switch shows no overlay now.
                self.rehover();
                for (node, video) in &mut self.videos {
                    if let Err(e) = video.wake_up() {
                        log::warn!("cannot wake {}: {e}", node.display());
                    }
                }
                self.darken();
                self.wait.shown(now);
                // The blank's countdown starts afresh from the acquire, and
                // a blank in force ends; while the VT was another's it ran
                // no blank, since its displays were not vtium's.
                self.user_input(now);
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
                // Before the first frame showed, the start's wait stops
                // until the VT comes back (vtium/vtium#270).
                self.wait.released();
                if let Some(input) = &mut self.input {
                    input.sleep();
                }
                self.releasing = Some(self.videos.keys().cloned().collect());
                for video in self.videos.values_mut() {
                    video.sleep();
                }
                // A head on the GPU path gives back its textures while the
                // VT is another's (ADR-1303 D9).
                for head in &mut self.heads {
                    if let Some(painter) = &mut head.painter {
                        painter.release();
                    }
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
                        head.drop_views(|k| *k == key);
                    }
                    self.relayout(h);
                }
            }
            // A modeset or a wake-up may have given the display a new
            // mode in place, on the same connector. Its view takes the
            // display's refresh rate again, which paces its frames. At a
            // new size it draws with a new renderer at that size, and the
            // head is laid out again: a view left at the old size never
            // draws, because every token the display lends has the new
            // size.
            Event::Redraw(id) => {
                let key = (node.to_owned(), id);
                let mode = self
                    .videos
                    .get(node)
                    .and_then(|video| Some((video.size(id)?, video.refresh_mhz(id))));
                let name = self.videos.get(node).map(|video| video.name(id));
                let App {
                    heads,
                    config,
                    locale,
                    ..
                } = self;
                let mut resized = None;
                for (h, head) in heads.iter_mut().enumerate() {
                    let Some(v) = head.views.get(&key) else {
                        continue;
                    };
                    let height = v.height;
                    let new = mode.filter(|&(size, _)| size != v.size);
                    let font = new.map(|_| {
                        head.face(height, locale)
                            .map(|face| head.font_at(face, config))
                    });
                    let render = head.render_config(config);
                    head.draw_now = true;
                    let Some(v) = head.views.get_mut(&key) else {
                        continue;
                    };
                    v.lost = true;
                    if let Some((_, refresh)) = mode {
                        v.set_refresh(refresh);
                    }
                    match (new, font) {
                        (Some((size, _)), Some(Ok(font))) => {
                            v.resize(font, size, render);
                            resized = Some(h);
                        }
                        // With no font at its height the view keeps its
                        // size and its renderer, so it never draws at a
                        // size its renderer was not made for; it draws
                        // nothing until the next set-up.
                        (Some((size, _)), Some(Err(e))) => log::warn!(
                            "cannot draw display {} at its new size {}x{}: {e}",
                            name.as_deref().unwrap_or("?"),
                            size.0,
                            size.1
                        ),
                        _ => {}
                    }
                }
                if let Some(h) = resized {
                    self.relayout(h);
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
                if self.blank.blanked()
                    && let Some(video) = self.videos.get_mut(node)
                {
                    power_down(video.as_mut());
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
        // The display guard's display counts as lit; given no keyboard, the
        // anti-lockout guard moves nothing ([`App::guard`] runs it).
        let lit: Vec<vtium_settings::Identity> = self.start_lit.values().cloned().collect();
        self.focus = head_index(self.rules.place(&lit, None).focus);
        self.bound_pointers();
        self.rehover();
    }

    /// The guards (ADR-1401 D9), after a device came or went: every device
    /// present placed by the rules in force, then the display guard and the
    /// anti-lockout guard over the placement.
    ///
    /// The display guard lights the boot display in the first head while
    /// the rules light none, and lets it go dark once they light another.
    /// The anti-lockout guard sends every keyboard to the focus, opened to
    /// deliver, while none reaches a head with a lit display, and moves the
    /// focus to a lit head, with a notice and the on-screen display; once one
    /// reaches a lit head each goes back to its rule. A keyboard the rescue
    /// key moved follows the focus throughout, and reaches it while the
    /// focus's head is lit.
    fn guard(&mut self) {
        if !std::mem::take(&mut self.guard_due) {
            return;
        }
        let displays: Vec<(PathBuf, DisplayId)> =
            self.lit.keys().chain(self.dark.keys()).cloned().collect();
        // The displays a device left dark at its bind and never announced,
        // each with its device and connector; their place among their
        // card's connectors is unknown, so they sort last on it.
        let held: Vec<(PathBuf, String, vtium_settings::Identity)> = self
            .videos
            .iter()
            .flat_map(|(node, video)| {
                let card = self.cards.get(node).cloned().unwrap_or_default();
                video.dark().into_iter().map(move |named| {
                    let display = vtium_devices::display(&named, &card, u32::MAX);
                    (node.clone(), named.connector, display)
                })
            })
            .collect();
        let devices: Vec<vtium_settings::Identity> = self
            .lit
            .values()
            .chain(self.dark.values())
            .chain(held.iter().map(|(_, _, d)| d))
            .chain(self.routes.unrescued())
            .cloned()
            .collect();
        // A keyboard the rescue key moved roams, and reaches the focus's
        // head while it is lit.
        let rescued: Vec<vtium_settings::Identity> = self.routes.rescued().cloned().collect();
        // The default head is the head of a file that declares none.
        let focus = self.focus.map(|h| {
            if self.rules.heads.is_empty() {
                vtium_settings::Place::DefaultHead
            } else {
                vtium_settings::Place::Head(h)
            }
        });
        let placement = self.rules.place_rescued(&devices, &rescued, focus);
        let (shown, rest) = placement
            .placed
            .split_at(displays.len().min(placement.placed.len()));
        let (unannounced, inputs) = rest.split_at(held.len().min(rest.len()));
        // under closed the guard lights the boot display for a notice
        // alone, on the screen no head holds
        let screen = self.screen;
        let lighting = displays
            .iter()
            .zip(shown)
            .find(|(_, p)| p.by == vtium_settings::By::NeverDark)
            .and_then(|(key, p)| {
                let h = match p.place {
                    vtium_settings::Place::Notice => screen,
                    place => head_index(Some(place)),
                };
                Some((key.clone(), h?))
            });
        if lighting != self.guard_lit {
            if let Some((key, _)) = self.guard_lit.take() {
                self.unlight(key);
            }
            if let Some((key, h)) = lighting {
                if Some(h) == screen {
                    let report = self.report.clone().filter(|_| !self.stopped);
                    let text = self.screen_notice();
                    // a report's console line, as the start logged it
                    let logged = match (&report, self.outcome.line) {
                        (Some(_), Some(line)) => line.text(),
                        _ => text.as_str(),
                    };
                    log::warn!(
                        "the display guard lights display {} for the notice alone: {logged}",
                        key.0.display()
                    );
                    if let Some(head) = self.heads.get_mut(h) {
                        match report {
                            Some(report) => head.set_report(report),
                            None => head.set_notice(&text),
                        }
                    }
                } else {
                    log::warn!(
                        "the display guard lights display {} in head {}",
                        key.0.display(),
                        self.heads.get(h).map_or("?", Head::name)
                    );
                }
                self.dark.remove(&key);
                self.guard_lit = Some((key.clone(), h));
                if let Some(video) = self.videos.get_mut(&key.0)
                    && let Err(e) = video.set_dpms(key.1, Dpms::On)
                {
                    log::warn!("cannot set display {} on: {e}", video.name(key.1));
                }
                self.add_display(&key.0, key.1);
            }
        }
        // A display the guard lights that its device never announced is
        // let go and scanned again, announced while none is lit, and lit
        // once it comes.
        let release = held
            .iter()
            .zip(unannounced)
            .find(|(_, p)| p.by == vtium_settings::By::NeverDark);
        if let Some(((node, connector, _), _)) = release {
            if let Some((key, _)) = self.guard_lit.take() {
                self.unlight(key);
            }
            self.any_lit.set(!self.lit.is_empty());
            if let Some(video) = self.videos.get_mut(node)
                && video.release_dark(connector)
            {
                log::warn!(
                    "the display guard asks {} for display {connector} again",
                    node.display()
                );
                if let Err(e) = video.hotplug() {
                    log::warn!("{}: hotplug: {e}", node.display());
                }
            }
        }
        let guarded = inputs
            .iter()
            .map(|p| p.by == vtium_settings::By::AntiLockout);
        for (id, disposition) in self.routes.guard(&self.rules, guarded) {
            if let Some(input) = &mut self.input {
                input.set_disposition(id, disposition);
            }
        }
        if placement.anti_lockout {
            if let Some(to) = head_index(placement.focus).filter(|&h| Some(h) != self.focus) {
                self.move_focus(to);
            }
            if !self.locked_out {
                let to = self
                    .focus
                    .and_then(|h| self.heads.get(h))
                    .map_or("", Head::name);
                let line = crate::notice::anti_lockout(to);
                log::warn!("anti-lockout: {line}");
                if let Some(head) = self.focus.and_then(|h| self.heads.get_mut(h)) {
                    head.add_notice(&line);
                }
            }
        }
        self.locked_out = placement.anti_lockout;
        self.any_lit.set(!self.lit.is_empty());
        self.bound_pointers();
        // The guard may have sent a keyboard to another head.
        self.rehover();
    }

    /// What the screen the display guard lights for a notice alone shows
    /// (ADR-1401 D7, D9): that a crash loop stopped every head, else the
    /// panic screen for a broken closed file, else the start's console
    /// line, else that no head has a display, each with its next step.
    fn screen_notice(&self) -> String {
        if self.stopped {
            return crate::notice::stopped();
        }
        self.report
            .as_ref()
            .map(vtium_render::NoticeReport::text)
            .or_else(|| self.outcome.dark_notice().map(crate::head::outcome_notice))
            .unwrap_or_default()
    }

    /// The display the display guard lit goes dark again: the rules light
    /// another.
    fn unlight(&mut self, key: (PathBuf, DisplayId)) {
        let Some(display) = self.lit.remove(&key) else {
            return;
        };
        if let Some(h) = self.head_of(&key.0, key.1) {
            if let Some(head) = self.heads.get_mut(h) {
                head.drop_views(|k| *k == key);
            }
            self.relayout(h);
        }
        log::info!("display {} goes dark again", key.0.display());
        self.dark.insert(key, display);
        self.darken();
    }

    /// A head's first display came, its last went, or its next came. With
    /// none the head's session runs on at the grid it had, which no
    /// display resizes, until its `when-display-lost` hangs it up (ADR-1401 D8), and
    /// the blank does not run: the blank in force ends, and the countdown
    /// starts afresh when a display comes, as at the user's input for now
    /// (vtium/vtium#87 tells the two apart). The next is drawn whole, and
    /// resizes the grid when its own differs, and stops the hang-up's
    /// time.
    fn attachment(&mut self, now: Instant) {
        let several = self.heads.len() > 1;
        let full = self.osd.mode == osd::Mode::Full && self.osd.shows_on_change();
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
                    head.drop_osd();
                    let hangup = match head.when_display_lost {
                        WhenDisplayLost::Keep => None,
                        WhenDisplayLost::Hangup => Some(0),
                        WhenDisplayLost::HangupAfter(secs) => Some(secs),
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
                    if full {
                        head.show_osd();
                    }
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
    /// `when-display-lost = hangup` (ADR-1401 D8); each console then
    /// follows the respawn policy, its session starting again in the same
    /// console, after the end of its session returned what it switched or
    /// zoomed (ADR-1201 D11).
    fn hang_up(&mut self, h: usize, now: Instant) {
        let since = now.saturating_duration_since(self.epoch);
        let oneshot = self.oneshot;
        let ended = self
            .heads
            .get_mut(h)
            .map(|head| head.hang_up(since, oneshot))
            .unwrap_or_default();
        for (id, after) in ended {
            if matches!(after, AfterExit::Restart | AfterExit::Ended) {
                self.return_font(h, id);
            }
            if after == AfterExit::Restart {
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
                        .map_or_else(vtium_settings::Identity::input, vtium_devices::input);
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
                DeviceKind::Input => self.remove_input(&node),
                DeviceKind::Drm | DeviceKind::Fbdev => self.remove_video(&node),
            },
            DeviceEvent::Hotplug { node, .. } => {
                if let Some(video) = self.videos.get_mut(&node)
                    && let Err(e) = video.hotplug()
                {
                    log::warn!("{}: hotplug: {e}", node.display());
                }
            }
        }
    }

    /// An input device udev reported, placed by the first rule matching
    /// `identity` (ADR-1401 D3): opened to deliver, opened for the rescue
    /// key alone, or never opened.
    pub fn add_input(&mut self, node: &Path, identity: vtium_settings::Identity) {
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

    /// The input device at `node` went.
    pub fn remove_input(&mut self, node: &Path) {
        self.routes.remove(node);
        self.guard_due = true;
        if let Some(input) = &mut self.input
            && let Some(id) = input.find(node)
        {
            input.remove(id);
        }
        // The keyboard's route is gone before the end of its modifiers is
        // drained, and that event finds no head, so the Ctrl it held is let
        // go here.
        self.rehover();
    }

    /// Each head hovers what its pointer is over with Ctrl as the
    /// keyboards delivering to it hold it now (ADR-1002, "What a link
    /// does"). The seat reads the keyboards, since a release sends no key,
    /// a keyboard that goes sends its modifiers' end after its route has
    /// gone, and a keyboard following the focus sends nothing as the focus
    /// moves.
    fn rehover(&mut self) {
        for h in 0..self.heads.len() {
            let ctrl = self.input.as_ref().is_some_and(|input| {
                input
                    .mods_of(&self.routes.devices_of(h, self.focus))
                    .contains(InputMods::CTRL)
            });
            if let Some(head) = self.heads.get_mut(h) {
                head.rehover(ctrl);
            }
        }
    }

    /// Natural scrolling, on or off (vtium/vtium#203): the wheel scrolls the
    /// view and reports to a program the other way, and the cursor keys
    /// mode 1007 sends reverse, while Ctrl's zoom keeps the wheel's own
    /// direction. vtium.conf's `natural-scrolling` sets it at the start.
    pub fn set_natural_scrolling(&mut self, on: bool) {
        self.natural_scrolling = on;
    }

    /// The name of the head holding the focus.
    pub fn focus_head(&self) -> Option<&str> {
        self.heads.get(self.focus?).map(Head::name)
    }

    /// The head the focus chord last moved the focus to, by its name,
    /// once (ADR-1401 D5).
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

    /// An event as the input drains it, from the device with the handle
    /// `e.device` or from none, which every turn delivers.
    pub fn drained_input(&mut self, e: DeviceInput, now: Instant) {
        // A device watched for the rescue key sends its presses alone.
        let node = e
            .device
            .and_then(|id| self.routes.node(id))
            .map(Path::to_owned);
        // Under closed an event of a device the seat no longer routes,
        // queued before it went, delivers nothing (D3, D5).
        if e.device.is_some() && node.is_none() && self.rules.policy == Policy::Closed {
            return;
        }
        self.deliver(node.as_deref(), e.event, now);
    }

    /// An event from the device at `node`, or from none, for the head its
    /// rule names or the head holding the focus (ADR-1401 D5). A key press
    /// is the user's activity; a pointer event is when
    /// [`App::pointer_event`] says so.
    fn deliver(&mut self, node: Option<&Path>, event: InputEvent, now: Instant) {
        // A seat a crash loop stopped delivers nothing (D1), and under
        // closed neither does a device with no route, such as one whose
        // event was queued before it went: no rule placed it (D3, D5).
        let unknown = node.is_some_and(|n| !self.routes.knows(n));
        if self.stopped || unknown && self.rules.policy == Policy::Closed {
            return;
        }
        // The shown VT's own key does nothing else (D9).
        if let (Some(node), InputEvent::Key(key)) = (node, &event)
            && !key.repeat
            && self.vt.is_own_key(key)
        {
            self.activity(now);
            self.rescue(node);
            return;
        }
        // The head the device asks: its rule's, or the active head's for a
        // roaming device, which is never isolated (D5).
        let asked = self.routes.head(node, self.focus);
        let from = KeyFrom {
            vt_keys: asked.is_some_and(|h| !self.rules.isolated(h)),
            roaming: self.routes.roams(node),
        };
        // A head whose display never lit receives nothing (D8).
        let h = asked.filter(|&h| {
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
                self.key(h, &key, from, now);
            }
            InputEvent::Pointer(p) => {
                let mods = match (&self.input, h) {
                    (Some(input), Some(h)) => input.mods_of(&self.routes.devices_of(h, self.focus)),
                    _ => InputMods::empty(),
                };
                self.pointer_to(h, p, mods, now);
            }
            // A keyboard's modifiers changed. Only the overlay follows
            // them, read from each head's own keyboards, and a modifier is
            // not the user's activity.
            InputEvent::Mods(_) => self.rehover(),
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
        if let (Some(input), Some(id)) = (&mut self.input, id) {
            input.set_disposition(id, vtium_platform::input::Disposition::Deliver);
        }
        // The guards run at once, so the notice names the head the keyboard
        // types into after them, which the anti-lockout guard may move.
        self.guard_due = true;
        self.guard();
        let line = match self.focus.and_then(|h| self.heads.get(h)) {
            Some(head) => crate::notice::rescue(node.display(), head.name()),
            None => crate::notice::rescue_no_head(node.display()),
        };
        log::warn!("{line}");
        match self.focus {
            Some(h) => {
                if let Some(head) = self.heads.get_mut(h) {
                    head.add_notice(&line);
                }
            }
            None => self.notice(&line),
        }
    }

    /// A pointer event, with the modifiers held on the keyboards as it
    /// came, for the head holding the focus.
    pub fn pointer_event(&mut self, event: PointerEvent, mods: InputMods, now: Instant) {
        self.pointer_to(self.focus, event, mods, now);
    }

    /// A pointer event for head `h`, with the modifiers held on the
    /// keyboards delivering to it. A move, a button or the wheel is the
    /// user's activity; the end of a frame and the hide are vtium's own
    /// events, so they neither wake the displays nor restart the blank's
    /// countdown.
    fn pointer_to(&mut self, h: Option<usize>, event: PointerEvent, mods: InputMods, now: Instant) {
        match event {
            // While the terminals are not awake, a VT the user released or
            // displays blanked, the pointer's action reaches no program and
            // selects, pastes or zooms nothing, as a key reaches none. It
            // may end a blank,
            // which wakes the terminals first.
            PointerEvent::Moved { .. } | PointerEvent::Button { .. } | PointerEvent::Wheel(_) => {
                self.activity(now);
                if !self.awake {
                    return;
                }
            }
            PointerEvent::Sync if !self.awake => return,
            PointerEvent::Sync => {}
            // The seat's pointers hide together, on every head: the hide
            // comes from no one device. While the VT is another's, as a
            // released fake VT is with its pointers still open, the mouse
            // moving in the other session arms the hide. That hide clears no
            // selection, so the selection stays for the user's return. It
            // still hides the arrow, which stays hidden at the return until
            // the pointer moves: the input keeps adding up the moves made in
            // the other session, so an arrow at the old place would jump at
            // the next move (the README's "A fake VT").
            PointerEvent::Hide => {
                let ours = self.vt.is_active();
                for head in &mut self.heads {
                    if ours {
                        head.pointer_event(event, Mods::empty(), false, &mut self.selection);
                    } else {
                        head.hide_pointer();
                    }
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
        let zoom = head.pointer_event(
            event,
            Mods::from_bits(mods.bits()),
            self.natural_scrolling,
            &mut self.selection,
        );
        if let Some(dir) = zoom {
            self.zoom(h, dir);
        }
    }

    /// The user's input reaches every device, which retries a failed
    /// wake-up, and the blank. While the VT is another's, as a released
    /// fake VT is with its keyboards still open, it reaches neither: the
    /// displays are not vtium's to wake.
    fn activity(&mut self, now: Instant) {
        if !self.vt.is_active() {
            return;
        }
        for video in self.videos.values_mut() {
            if let Err(e) = video.input() {
                log::warn!("cannot wake the displays: {e}");
            }
        }
        self.user_input(now);
    }

    /// The key order: the VT's keys, the seat's, then the terminal of
    /// head `h`, the head the key's device delivers to: its grabs, DECARM,
    /// the encoder. Each key that passes the seat's clears the selection.
    ///
    /// The VT's keys switch the VT only `from` a keyboard asking a head that
    /// is not isolated; from any other, the press goes on down the key
    /// order to the session as an ordinary key, as a serial terminal sends
    /// it (ADR-1401 D5). The roaming chord is taken only from a roaming
    /// keyboard; from a claimed one it reaches the head's session as a key.
    fn key(&mut self, h: Option<usize>, key: &KeyEvent, from: KeyFrom, now: Instant) {
        if from.vt_keys && self.vt.key(key) || !self.awake {
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
        let chord =
            chord::chord(bits, &key.keysyms).filter(|&chord| from.roaming || chord.is_console());
        if let Some(chord) = chord {
            // One action a press, as the power keys.
            if !key.repeat {
                self.chord(h, chord, key.mods.contains(InputMods::CTRL), now);
            }
            return;
        }
        let Some(h) = h else {
            return;
        };
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        // A press of Ctrl shows the hovered link's URI, and a key pressed
        // without Ctrl hides it; letting go of Ctrl sends no key.
        head.hover(key.mods.contains(InputMods::CTRL));
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
        // The prompt jumps are the main screen's: on the alternate screen
        // the keys go to the program (ADR-1002, "What the marks are for").
        let alternate = head.console.term.terminal.screen().alternate();
        if let Some(g) =
            grab::grab(bits, &key.keysyms).filter(|g| !(g.is_prompt_jump() && alternate))
        {
            let screen = head.console.term.terminal.screen_mut();
            let page = head.grid.1.saturating_sub(1).max(1);
            head.draw_now = true;
            match g {
                Grab::ScrollUp => screen.scroll_view_up(1),
                Grab::ScrollDown => screen.scroll_view_down(1),
                Grab::PageUp => screen.scroll_view_up(page),
                Grab::PageDown => screen.scroll_view_down(page),
                Grab::PromptUp | Grab::PromptDown => {
                    screen.jump_to_prompt(g == Grab::PromptUp);
                }
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
                head.hide_pointer();
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

    /// A head's chord (ADR-1401 D1, D5): a console chord shows another of
    /// head `h`'s consoles, the head the key's device delivers to, and
    /// under an open policy opens or closes one at `now`; the focus chord
    /// moves the focus to the next or the previous head with a lit
    /// display. A console chord leaves the console it shows hovering what
    /// the pointer is over, with `ctrl` as held now.
    fn chord(&mut self, h: Option<usize>, chord: HeadChord, ctrl: bool, now: Instant) {
        let visible = self.awake;
        let back = match chord {
            HeadChord::NextHead => false,
            HeadChord::PreviousHead => true,
            HeadChord::NextConsole
            | HeadChord::PreviousConsole
            | HeadChord::OpenConsole
            | HeadChord::CloseConsole => {
                let changed = match (h, chord) {
                    // Under closed the count is fixed: the chord is taken,
                    // and does nothing (D1, D5).
                    (_, HeadChord::OpenConsole | HeadChord::CloseConsole)
                        if self.rules.policy == Policy::Closed =>
                    {
                        false
                    }
                    (Some(h), HeadChord::OpenConsole) => self.open_console(h, now),
                    (Some(h), HeadChord::CloseConsole) => {
                        let closed = self
                            .heads
                            .get_mut(h)
                            .and_then(|head| head.close_shown(visible))
                            .map(|console| console.id);
                        // Its session ended, and with it the switch it made
                        // (ADR-1201 D11).
                        if let Some(id) = closed.filter(|&id| {
                            self.heads
                                .get(h)
                                .is_some_and(|head| head.switched == Some(id))
                        }) {
                            self.return_font(h, id);
                        }
                        closed.is_some()
                    }
                    (h, _) => h
                        .and_then(|h| self.heads.get_mut(h))
                        .is_some_and(|head| head.console_chord(chord, visible)),
                };
                if let Some(h) = h.filter(|_| changed) {
                    self.relayout(h);
                    if let Some(head) = self.heads.get_mut(h) {
                        head.hover(ctrl);
                    }
                }
                return;
            }
        };
        self.focus_settled = true;
        // only to a lit head that can be the active head (D5)
        let (heads, rules) = (self.heads(), &self.rules);
        let lit = |h: usize| {
            heads.get(h).is_some_and(|head| !head.views.is_empty()) && !rules.isolated(h)
        };
        if let Some(to) = chord::step(self.focus, heads.len(), back, lit) {
            self.move_focus(to);
            // The keyboard that pressed the chord follows the focus, with
            // the Ctrl it holds for the chord.
            if let Some(head) = self.heads.get_mut(to).filter(|_| ctrl) {
                head.rehover(true);
            }
        }
    }

    /// Opens a console on head `h` at `now`, after the console it shows,
    /// and starts the head's login in it (ADR-1401 D1, D5), unless the head
    /// has `head-consoles-max` consoles or every head together has
    /// `consoles-max`: then nothing opens, and vtium logs a line and shows
    /// a brief line on the head's on-screen display. A console whose login
    /// cannot start leaves, and the head shows a notice. Returns whether
    /// one opened.
    fn open_console(&mut self, h: usize, now: Instant) -> bool {
        let total: usize = self
            .heads
            .iter()
            .filter(|head| !head.notice_only)
            .map(Head::console_count)
            .sum();
        let caps = self.caps;
        let osd = self.osd;
        let App {
            heads,
            config,
            scrollback,
            scrollback_bytes,
            clock,
            ..
        } = self;
        let Some(head) = heads.get_mut(h) else {
            return false;
        };
        let n = head.console_count();
        let full = if n >= caps.head {
            log::warn!(
                "head {}: no console opens: the head has {n}, head-consoles-max",
                head.name()
            );
            Some(crate::notice::no_more_consoles(head.name(), n))
        } else if total >= caps.total {
            log::warn!(
                "head {}: no console opens: every head together has {total}, consoles-max",
                head.name()
            );
            Some(crate::notice::no_more_consoles_together(total))
        } else {
            None
        };
        if let Some(line) = full {
            head.show_brief(line, osd);
            return false;
        }
        let term = crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock));
        let since = now.saturating_duration_since(self.epoch);
        let (id, at) = head.open_console(term, since);
        log::info!("head {}: console {id} opens", head.name());
        // The session starts while the console is hidden, so a session that
        // cannot start leaves the console shown before told nothing. That
        // leaves the run alive: the console leaves, and the head says so,
        // since vtium never keeps a console with no session (the README's
        // "The session").
        // That the console did not open is a notice of a problem, which
        // shows whatever `osd` says, and goes as the guards' notices go
        // (ADR-1401 D5, D7).
        if let Err(why) = self.start_session(h, id) {
            if let Some(head) = self.heads.get_mut(h) {
                log::warn!("head {}: console {id} does not open: {why}", head.name());
                head.drop_hidden(id);
                head.add_notice(crate::notice::NO_CONSOLE_OPENED);
            }
            return false;
        }
        let visible = self.awake;
        if let Some(head) = self.heads.get_mut(h) {
            head.show(at, visible);
        }
        // The head lays the console it shows out on its displays.
        self.relayout(h);
        true
    }

    /// The focus moves to head `h`, by the focus chord or the anti-lockout
    /// guard, the only moves after its start (ADR-1401 D5): every head's
    /// on-screen display shows.
    fn move_focus(&mut self, h: usize) {
        self.focus = Some(h);
        // The keyboards following the focus now reach head `h`, and no
        // longer the head that lost it, which keeps the hover its pointer
        // shows with Ctrl as its own keyboards hold it.
        self.rehover();
        self.focus_move = Some(h);
        if self.osd.shows_on_change() {
            for head in self
                .heads
                .iter_mut()
                .filter(|head| !head.views.is_empty() && !head.notice_only)
            {
                head.show_osd();
            }
        }
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
        let outline = chosen.outline();
        let (old, auto, points) = (head.font_size, head.auto_size, head.points);
        head.auto_size = false;
        head.points = None;
        head.font_size = size;
        // An outline frit asks the helper again at each zoom, a height
        // whose answer failed before included.
        if let Some(frit) = head
            .frit()
            .filter(|_| outline)
            .and_then(|n| self.frits.get_mut(n))
        {
            frit.own.retain(|_, answer| answer.is_ok());
        }
        if !self.relayout(h)
            && let Some(head) = self.heads.get_mut(h)
        {
            head.font_size = old;
            head.auto_size = auto;
            head.points = points;
        }
    }

    /// The programs of head `h`'s consoles that exited
    /// ([`session`]): each starts again in its console, ends its console
    /// under `oneshot`, or ends vtium after too many quick exits; under
    /// `oneshot` vtium ends once no head that started still runs.
    fn child_exited(&mut self, h: usize, now: Instant) {
        let since = now.saturating_duration_since(self.epoch);
        let exited: Vec<(u64, std::process::ExitStatus, bool)> = self
            .heads
            .get_mut(h)
            .into_iter()
            .flat_map(Head::consoles_mut)
            .filter_map(|c| {
                let adopted = c.pty.as_ref().is_some_and(Pty::adopted);
                c.term.exited.take().map(|status| (c.id, status, adopted))
            })
            .collect();
        for (id, status, adopted) in exited {
            if adopted {
                // vtium did not start it, so it cannot read its status
                // (ADR-1401 D13).
                let name = self.heads.get(h).map(Head::name).unwrap_or_default();
                log::info!(
                    "head {name}: console {id}: its session, which outlived the last vtium, ended; vtium did not start it, so its exit status is not known"
                );
            } else {
                log::info!("child exited: {status}");
            }
            let Some(head) = self.heads.get_mut(h) else {
                return;
            };
            let after = head.exited(id, since, self.oneshot);
            // Before the console's next session starts, at the size it
            // returns to.
            if matches!(after, Some(AfterExit::Restart | AfterExit::Ended)) {
                self.return_font(h, id);
            }
            match after {
                // No login starts in the turn vtium exits, which hangs
                // every session up.
                Some(AfterExit::Restart) if self.exit.is_none() => self.spawn(h, id),
                Some(AfterExit::GaveUp) => self.gave_up(),
                Some(AfterExit::Restart | AfterExit::Ended) | None => {}
            }
        }
        if let Some(exit) = session::exit_due(&self.heads) {
            self.request_exit(exit);
        }
    }

    /// The session in head `h`'s console `id` ended, or the console
    /// closed (ADR-1201 D11). Where
    /// an OSC 50 switch from that console, or from any console of a head
    /// left with one, changed the head's font, vtium returns the head to
    /// the font the configuration gives it, read again; and where the font
    /// returns or a zoom left the console off the configured size, the
    /// console returns to that size. Where the `font-size` read again
    /// turns the configured size on or off, the frit the head draws is
    /// rebuilt to drop or take its own cell (ADR-1200 D16). Nothing else is
    /// read or reloaded. A configured font that draws nothing at that size
    /// leaves the font and the size as they were, with one line saying
    /// why.
    fn return_font(&mut self, h: usize, id: u64) {
        let Some(due) = self.heads.get(h).map(|head| head.returns(id)) else {
            return;
        };
        if !due.size {
            return;
        }
        self.reread_font_keys();
        let config = self.head_config(h);
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let mut kept = head.keep(id);
        let was_sized = head.sized();
        head.reconfigure(&config);
        let shown = head.console.id == id;
        let new = if due.font {
            head.switched = None;
            Some(self.configured_font(h))
        } else if head.sized() != was_sized {
            // The frit drawn, with its own cell or without it as the
            // `font-size` read again now says (ADR-1200 D16).
            match &head.font {
                Some(Chosen::Frit { frit, .. }) => {
                    Chosen::frit(frit.clone(), head.sized(), head.look_cell()).ok()
                }
                _ => None,
            }
        } else {
            None
        };
        // The frit the head drew, where the font changed.
        let mut left = None;
        if let Some(new) = new {
            let Some(head) = self.heads.get_mut(h) else {
                return;
            };
            if !same_font(head.chosen(), &new) || head.sized() != was_sized {
                left = Some(head.frit().map(str::to_owned));
                kept.font = Some(head.font.replace(new));
                head.refont = true;
                self.share(h);
                self.retry_late_cell(h);
            }
        }
        if let Some(head) = self.heads.get_mut(h) {
            let fresh = head.fresh_fit();
            head.configured_size(id, fresh);
        }
        if (shown || left.is_some()) && !self.relayout(h) {
            let Some(head) = self.heads.get_mut(h) else {
                return;
            };
            let tried = head.font_size;
            head.put_back(kept);
            self.relayout(h);
            self.prune();
            if let Some(head) = self.heads.get(h) {
                log::warn!(
                    "head {}: the configured font draws nothing at {tried} pixels: the font and the size stay {} at {}",
                    head.name(),
                    head.chosen().name(head.font_size, &self.locale),
                    head.font_size
                );
            }
            return;
        }
        let hidden = self
            .heads
            .get(h)
            .is_some_and(|head| head.hidden.iter().any(|c| c.id == id));
        if hidden
            && let Some(hidden) = self.configured_fit(h)
            && let Some(head) = self.heads.get_mut(h)
        {
            head.configured_size(id, hidden);
        }
        if let Some(was) = left {
            self.prune();
            // The frit the head left, where another head still draws it,
            // keeps the cells of the heads drawing it alone.
            if let Some(other) = was.and_then(|was| {
                self.heads
                    .iter()
                    .position(|head| head.frit() == Some(was.as_str()))
            }) {
                self.ask_glyphs(other);
            }
            if let Some(head) = self.heads.get(h) {
                log::info!(
                    "head {}: the session in console {id} ended: the font returns to {}",
                    head.name(),
                    head.chosen().name(head.font_size, &self.locale)
                );
            }
        }
    }

    /// Reads the configuration again with the reader vtium started with,
    /// for its font keys alone (ADR-1201 D11): each report naming a font
    /// key, a file that could not be read whole, or a file the ownership
    /// check refuses, with why and the command that mends it (ADR-1403
    /// D3), is logged as the start logs it, and the key is dropped as it
    /// is at the start. Every other key, its reports and
    /// the count of reports past the most kept wait for the next start.
    /// With no reader, the configuration vtium started with stands.
    fn reread_font_keys(&mut self) {
        const FONT_KEYS: [&crate::config::Key; 4] = [
            &keys::FONT_NAME,
            &keys::FONT_SIZE,
            &keys::FONT_ENGINE,
            &keys::FONT_LINE_SPACING,
        ];
        let Some(read) = self.reread.as_mut() else {
            return;
        };
        let loaded = read();
        use crate::config::Problem;
        let font_key = |k: &str| FONT_KEYS.iter().any(|f| f.name == k);
        for r in &loaded.reports {
            let file = matches!(
                r.problem,
                Problem::Unreadable(_)
                    | Problem::NotAFile
                    | Problem::FileTooLong
                    | Problem::Untrusted(_)
            );
            if file || r.problem.key().is_some_and(font_key) {
                crate::warn::warn(r);
            }
        }
        self.config.take_keys(&loaded.config, &FONT_KEYS);
    }

    /// Head `h`'s frit asks the helper for its cell again where the last
    /// ask had no answer in time, even while another head draws the frit; a
    /// refusal holds (ADR-1200 D17).
    fn retry_late_cell(&mut self, h: usize) {
        if let Some(frit) = self
            .heads
            .get(h)
            .and_then(Head::frit)
            .and_then(|name| self.frits.get_mut(name))
        {
            frit.own
                .retain(|_, answer| !answer.as_ref().is_err_and(|no| no.late));
        }
    }

    /// The fit a hidden console of head `h` takes at the configured size:
    /// the grid and cell the head's displays give its font there, as
    /// [`App::relayout`] lays the shown console out. `None` while the head
    /// has no display, when the console keeps the shown console's grid and
    /// cell until it is shown, as a console the open chord opens does.
    fn configured_fit(&mut self, h: usize) -> Option<crate::head::console::Fit> {
        let dpi = self.dpi(h);
        let head = self.heads.get(h)?;
        if head.views.is_empty() {
            return None;
        }
        let fresh = head.fresh_fit();
        let height = height_at(head, (fresh.points, fresh.auto_size), dpi, &self.locale);
        self.own_cell(h, height);
        let head = self.heads.get(h)?;
        let locale = self.locale.as_str();
        let (face, base) = font::select(height, head.has_font, |x| head.face(x, locale)).ok()?;
        let cell = |at: u32| {
            head.face(at, locale).ok().map(|f| {
                let c = f.cell();
                (c.width, c.height)
            })
        };
        let sizes: Vec<(u32, u32)> = head.views.values().map(|v| v.size).collect();
        let turned = matches!(head.orientation, Orientation::Right | Orientation::Left);
        let l = layout::layout(&sizes, turned, base, face.size_step(), cell)?;
        Some(crate::head::console::Fit {
            grid: l.grid,
            font_size: base,
            cell: cell(base)?,
            ..fresh
        })
    }

    /// A session kept ending as soon as it started, which ends the run
    /// (ADR-1401 D1): every head's sessions hang up as vtium leaves, and
    /// under the open policy vtium exits with status 1 for the unit's
    /// getty. Under the closed policy a getty's login would take any
    /// keyboard (D9), so vtium hangs every session up and holds its VT: no
    /// session starts again, no input is delivered, every display a head
    /// held goes dark, and the display guard lights the boot display for a
    /// notice alone, which stays as long as vtium runs.
    fn gave_up(&mut self) {
        if self.rules.policy == Policy::Open {
            log::error!(
                "a session kept ending as soon as it started, so vtium exits. Check the login command in vtium.conf."
            );
            self.request_exit(Exit::TooQuick);
            return;
        }
        log::error!(
            "a session kept ending as soon as it started, so vtium stops every head; with the closed policy it keeps the VT"
        );
        self.stopped = true;
        self.focus = None;
        for head in &mut self.heads {
            for console in head.consoles_mut() {
                console.pty = None;
            }
            head.drop_views(|_| true);
            head.hangup_due = None;
            head.attached = Attached::NotYet;
        }
        self.guard_lit = None;
        let lit = std::mem::take(&mut self.lit);
        self.dark.extend(lit);
        self.darken();
        // No rule, so no display joins a head again and the display guard
        // lights the boot display for the notice alone. The heads stay, each
        // isolated as the file wrote it, so a keyboard asking an isolated
        // head still switches no VT, whatever else delivers.
        self.rules = vtium_settings::Config {
            policy: Policy::Closed,
            heads: std::mem::take(&mut self.rules.heads),
            survivors: true,
            ..vtium_settings::Config::default()
        };
        self.guard_due = true;
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
        if std::mem::take(&mut self.keyboard_check) && !self.routes.any_keyboard() {
            let line = crate::notice::NO_KEYBOARD;
            log::warn!("{line}");
            self.notice(line);
        }
        self.take_focus();
        self.guard();
        self.attachment(now);
        self.first_start(now);
        if let Some(r) = &self.releasing
            && r.is_empty()
        {
            self.releasing = None;
            if let Err(e) = self.vt.acknowledge_release() {
                log::warn!("cannot let the VT go: {e}");
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
            self.drained_input(e, now);
        }
        self.report_focus();
        // One ring a turn, however many consoles of however many heads
        // sent BEL: the heads share the one VT.
        let mut bell = false;
        for h in 0..self.heads.len() {
            bell |= self.session_turn(h, now);
        }
        if bell && self.bell {
            self.vt.bell();
        }
        // After the sessions' turns, whose respawns drop a request out,
        // and before the draw, which shows what a reply placed.
        #[cfg(feature = "images")]
        for head in &mut self.heads {
            head.image_turn(now, self.image_helpers.as_ref(), &self.handle);
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
            log::error!(
                "no display appeared within {} s (display-wait), so vtium exits",
                wait.as_secs()
            );
            self.request_exit(Exit::NoDisplay);
        }
        self.sync_kept();
        self.exiting(now);
    }

    /// What head `h`'s programs asked this turn: each console's OSC 52
    /// selection and BEL; the shown console's OSC 50 font and blank sequences,
    /// which a hidden console's wait for it to be shown, since the font
    /// and the displays are the head's; and their exits. Returns whether
    /// any of its consoles sent BEL, which [`App::turn`] rings.
    fn session_turn(&mut self, h: usize, now: Instant) -> bool {
        let Some(head) = self.heads.get_mut(h) else {
            return false;
        };
        // The console has no system clipboard: a selection OSC 52 set is
        // held for a paste, or dropped under `no-clipboard-write`.
        // Any console's BEL rings the VT under `bell`, a hidden console's as
        // well as the shown one's.
        let mut bell = false;
        for console in head.consoles_mut() {
            if let Some(selection) = console.term.terminal.take_clipboard_write()
                && self.clipboard_write
            {
                self.selection = Some(selection);
            }
            bell |= console.term.terminal.take_bell();
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
        if let Some(request) = request
            && self.program_blank
        {
            self.blank_request(request, now);
        }
        bell
    }

    fn timers(&mut self, now: Instant) {
        let awake = self.awake;
        for head in &mut self.heads {
            if head.notice_due.is_some_and(|d| d <= now) {
                head.end_notices();
            }
            if head.osd_due.is_some_and(|d| d <= now) {
                head.osd_due = None;
                head.osd_shown = false;
            }
            head.brief_turn(now);
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
        // While the VT is another's its displays are not vtium's to blank,
        // and its countdown waits for the acquire.
        if self.vt.is_active() && self.blank.fire(now) {
            self.blank_now(now);
        }
        #[cfg(feature = "images")]
        self.animate(now);
    }

    /// Each console's animation clock at this turn (ADR-1102 D3): the
    /// shown console's plays while the VT is ours, its displays are lit
    /// and it has one, and a hidden console's only answers its program's
    /// commands. A frame that changes on a shown console draws its head.
    #[cfg(feature = "images")]
    fn animate(&mut self, now: Instant) {
        let lit = self.awake && !self.blank.blanked();
        for head in &mut self.heads {
            let shown = lit && !head.views.is_empty();
            let term = &mut head.console.term;
            let frames = term.terminal.screen().images();
            head.draw_now |= term.animation.turn(frames, now, shown);
            for console in &mut head.hidden {
                let term = &mut console.term;
                term.animation
                    .turn(term.terminal.screen().images(), now, false);
            }
        }
    }

    fn draw(&mut self, now: Instant) {
        self.osd(now);
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

    /// Each head's on-screen display for this turn's frames (ADR-1401 D5):
    /// under `osd = full` it shows after a change to the shown console or
    /// the number of consoles once the head's sessions have started, which
    /// `minimal`'s dot does not draw. It shows for `osd-timeout` from the
    /// first frame that draws it, or always under `never`.
    fn osd(&mut self, now: Instant) {
        let Osd { mode, timeout } = self.osd;
        let changes = mode == osd::Mode::Full && self.osd.shows_on_change();
        // the notice screen is no head, and shows no status
        for (h, head) in self
            .heads
            .iter_mut()
            .enumerate()
            .filter(|(_, h)| !h.notice_only)
        {
            // A head opens its other consoles as it starts, which is no
            // change to show.
            let seen = head
                .started()
                .then(|| (head.console.id, head.console_count()));
            let was = std::mem::replace(&mut head.osd_seen, seen);
            if changes && was.is_some_and(|was| Some(was) != seen) {
                head.show_osd();
            }
            let shown = mode != osd::Mode::Off && (timeout.is_none() || head.osd_shown);
            if let Some(time) = timeout.filter(|_| shown && head.osd_due.is_none())
                && !head.views.is_empty()
            {
                head.osd_due = Some(after(now, time));
            }
            let active = self.focus == Some(h);
            let mark = (shown && mode == osd::Mode::Minimal).then_some(if active {
                vtium_render::Mark::Focus
            } else {
                vtium_render::Mark::NoFocus
            });
            let full = (shown && mode == osd::Mode::Full).then(|| {
                let at = head.shown_console().saturating_add(1);
                crate::notice::osd_line(active, head.name(), at, head.console_count())
            });
            // The brief line answers one press at a cap: for `osd-timeout`,
            // or 3 s under `never`, from its first frame, in place of the
            // full line. [`Head::show_brief`] sets none under `off`.
            head.arm_brief(after(now, timeout.unwrap_or(BRIEF_NEVER)));
            let status = head.brief().map(str::to_owned).or(full);
            if head.mark != mark || head.status != status {
                head.mark = mark;
                head.status = status;
                head.draw_now = true;
            }
        }
    }

    /// Lets go of every head's render thread at once, as at a head file
    /// reload and at vtium's exit: heads whose GPU driver hangs hold the loop
    /// [`crate::painter::LET_GO_MS`] in all (ADR-1303 D9).
    fn let_go_of_painters(&mut self) {
        let clock = Rc::clone(&self.clock);
        crate::painter::let_go(
            self.heads.iter_mut().filter_map(|h| h.painter.take()),
            &*clock,
        );
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
        let gpu = self.gpu_source(h);
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let started = vtium_platform::calloop::ping::make_ping()
            .map_err(|e| format!("cannot make its wake-up: {e}"))
            .and_then(|(ping, source)| {
                self.handle
                    .insert_source(source, |(), (), _: &mut App| {})
                    .map_err(|e| format!("cannot watch its wake-up: {e}"))?;
                Painter::start(head.name(), gpu, move || ping.ping()).map_err(|e| e.to_string())
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

    /// Where head `h`'s render thread makes its GL context: nowhere unless
    /// `hwaccel` is on in a build with `gpu` (ADR-1303 D9); a test's GPU
    /// where one is set; else the render node of the card that drives the
    /// head's first display. A card with no render node, such as a
    /// framebuffer, leaves the head on the CPU path, and says so.
    fn gpu_source(&self, h: usize) -> Option<crate::painter::GpuSource> {
        if !self.config.switch(&keys::HWACCEL) || !cfg!(feature = "gpu") {
            return None;
        }
        if let Some(source) = &self.gpu_source {
            return Some(source.clone());
        }
        let head = self.heads.get(h)?;
        let (node, _) = head.views.keys().next()?;
        #[cfg(feature = "gpu")]
        match vtium_platform::gpu::render_node(node) {
            Ok(render) => return Some(crate::painter::GpuSource::RenderNode(render)),
            Err(e) => log::warn!(
                "head {}: {} has no render node ({e}): it draws on the CPU path",
                head.name(),
                node.display()
            ),
        }
        #[cfg(not(feature = "gpu"))]
        let _ = node;
        None
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
            let frames = painter.take();
            // the render thread told the loop which path draws the head
            if painter.take_known() {
                display_lines(head, videos);
            }
            for mut composed in frames {
                if let Some(why) = composed.left_gpu.take() {
                    left_gpu(head, videos, &why);
                }
                if let Some(why) = composed.refused_gpu.take() {
                    refused_gpu_display(head, videos, &composed.key, &why);
                }
                if composed.back_on_gpu {
                    back_on_gpu(head, videos);
                }
                let (Some(view), Some(video)) = (
                    head.views.get_mut(&composed.key),
                    videos.get_mut(&composed.key.0),
                ) else {
                    continue;
                };
                if let Some(why) = composed.failed_gpu.take() {
                    failed_gpu_frame(view, video.as_ref(), composed.key.1, &why);
                }
                let compose = composed.stats.cpu;
                let shown = view.present(
                    composed,
                    &mut head.console.term,
                    video.as_mut(),
                    *awake,
                    &**clock,
                );
                if shown.queued {
                    head.figures.frame(compose, now);
                }
            }
        }
    }

    /// The render timer's figures for each display, once a second: its
    /// line in the log under `render-timer=log` or `both`, and its line in
    /// the on-screen display's box under `osd` or `both`, and the console
    /// redraws the display so the new line shows. That frame counts in the
    /// next second's figures, so they include the cost of showing them.
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
        let (log, osd) = self.render_timer();
        for head in &mut self.heads {
            let mut shows = false;
            for ((node, id), view) in &mut head.views {
                let px = u64::from(view.size.0).saturating_mul(u64::from(view.size.1));
                let Some(report) = view
                    .timer
                    .as_mut()
                    .and_then(|t| t.report(Duration::from_secs(1), px))
                else {
                    continue;
                };
                if log {
                    log::info!(target: "terminal", "{}", render_timer_line(&report, node, *id));
                }
                if osd {
                    view.timer_line = Some(report.osd());
                    shows = true;
                }
            }
            head.draw_now |= shows;
        }
    }

    fn exiting(&mut self, now: Instant) {
        if self.exit.is_none() {
            return;
        }
        match self.leaving {
            None => {
                self.deactivate();
                // The sessions a keeper holds run on for the next vtium
                // (ADR-1401 D13); every other session is hung up.
                self.release_sessions();
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
        if self.vt.is_active() {
            due.extend(self.blank.deadline());
        }
        for head in &self.heads {
            due.extend(head.text_due);
            due.extend(head.cursor_due.map(|(d, _)| d));
            due.extend(head.console.term.sync.deadline());
            due.extend(head.views.values().filter_map(|v| v.wait.due()));
            due.extend(head.figures.due);
            due.extend(head.hangup_due);
            due.extend(head.notice_due);
            due.extend(head.osd_due);
            due.extend(head.brief_due());
            #[cfg(feature = "images")]
            due.extend(head.image_due());
            #[cfg(feature = "images")]
            {
                let term = &head.console.term;
                due.extend(term.animation.due(term.terminal.screen().images()));
            }
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

/// What the device a key came from may do (ADR-1401 D5).
#[derive(Clone, Copy, Debug)]
struct KeyFrom {
    /// It asks a head that is not isolated, so the VT's keys switch the
    /// VT.
    vt_keys: bool,
    /// It roams, so it takes the roaming chord.
    roaming: bool,
}

/// The index of the head at `place`: the default head is the first.
fn head_index(place: Option<vtium_settings::Place>) -> Option<usize> {
    match place? {
        vtium_settings::Place::Head(h) => Some(h),
        vtium_settings::Place::DefaultHead => Some(0),
        _ => None,
    }
}

/// What the screens of `heads` show, merged style by style: each character
/// and cluster any of them shows in that style, sorted and once, the
/// regular style first.
fn shown<'a>(heads: impl Iterator<Item = &'a &'a Head>) -> Vec<crate::deferred::Shown> {
    let mut all = vec![crate::deferred::Shown::default()];
    for one in heads.flat_map(|head| head.visible()) {
        match all.iter_mut().find(|s| s.style == one.style) {
            Some(s) => {
                s.chars.extend(one.chars);
                s.clusters.extend(one.clusters);
            }
            None => all.push(one),
        }
    }
    for s in &mut all {
        s.chars.sort_unstable();
        s.chars.dedup();
        s.clusters.sort_unstable();
        s.clusters.dedup();
    }
    all
}

/// The height `head` draws at with its display's density `dpi`: the
/// configured `font-size` in points, else the automatic rule, else the
/// height it has, as a bitmap frit's whole multiples snap it.
fn draw_height(head: &Head, dpi: f64, locale: &str) -> u32 {
    height_at(head, (head.points, head.auto_size), dpi, locale)
}

/// The height `head` draws at with its display's density `dpi` at the
/// size `(points, auto_size)`, a console's [`crate::head::console::Fit`]:
/// as [`draw_height`] has it.
fn height_at(head: &Head, size: (Option<u32>, bool), dpi: f64, locale: &str) -> u32 {
    let (points, auto_size) = size;
    let height = if let Some(points) = points {
        font::points_to_pixels(points, dpi)
    } else if auto_size {
        let sizes = head.views.values().map(|v| v.size);
        let w = sizes.clone().map(|s| s.0).min().unwrap_or(0);
        let ht = sizes.map(|s| s.1).min().unwrap_or(0);
        head.chosen().auto_height(w, ht, locale, dpi)
    } else {
        head.font_size
    };
    head.chosen().snap(height)
}

/// Whether `a` and `b` are the same font: the same built-in by name, or
/// the same frit, by its name and its text.
fn same_font(a: &Chosen, b: &Chosen) -> bool {
    match (a, b) {
        (Chosen::Builtin { name: x, cell: a }, Chosen::Builtin { name: y, cell: b }) => {
            x == y && a == b
        }
        (Chosen::Frit { frit: x, .. }, Chosen::Frit { frit: y, .. }) => {
            x.name == y.name && x.text == y.text
        }
        _ => false,
    }
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
    // each display draws its own render timer's line in the box
    let overlays: Vec<crate::term::Overlay> = head
        .views
        .values()
        .map(|v| head.overlay(v.timer_line.as_deref()))
        .collect();
    // The notice screen's notice stays for as long as it is lit (D7).
    if head.has_notices()
        && head.notice_due.is_none()
        && !head.views.is_empty()
        && !head.notice_only
    {
        head.notice_due = Some(after(now, crate::head::NOTICE_SHOWN));
    }
    let present = std::mem::take(&mut head.console.term.present);
    let now_all = std::mem::take(&mut head.draw_now) || present;
    let term = &mut head.console.term;
    let changed = term.changed;
    let holding = term.sync.holding(&term.terminal);
    let base = head.cell;
    let mut snapshot = None;
    for ((key, view), overlay) in head.views.iter_mut().zip(&overlays) {
        let Some(video) = videos.get_mut(&key.0) else {
            continue;
        };
        let due = view.wait.due().is_some_and(|d| d <= now);
        if due {
            view.wait.end();
        }
        let owed = view.owed && !holding;
        // A pending view waits for its display to take a frame, so a
        // display that lends no token never turns the loop.
        let wants =
            now_all || view.lost || owed || due || (view.pending && video.ready_for_frame(view.id));
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
            if !view.owed
                && let Some(t) = &mut view.timer
            {
                t.held();
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
        view.overlay.clone_from(overlay);
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
        match &mut head.painter {
            Some(painter) => {
                if let Some(job) = painter.send(job) {
                    view.refused(*job, video.as_mut());
                }
            }
            None => {
                let composed = job.compose(None);
                let compose = composed.stats.cpu;
                let shown = view.present(composed, term, video.as_mut(), true, clock);
                if shown.queued {
                    head.figures.frame(compose, now);
                }
            }
        }
    }
}

/// Head `head`'s GPU path ended during the run, for `why`: it draws on the
/// CPU path until vtium restarts, which the log says once, with each of its
/// displays' lines again naming the path (ADR-1303 D9, D10).
fn left_gpu(head: &mut Head, videos: &BTreeMap<PathBuf, Box<dyn Video>>, why: &str) {
    log::warn!(
        "head {}: the GPU path failed ({why}): it draws on the CPU path until vtium restarts",
        head.name()
    );
    let Some(painter) = &mut head.painter else {
        return;
    };
    painter.left_gpu();
    display_lines(head, videos);
}

/// The GPU path failed a frame of `view`, the display `id` of `video`,
/// for `why`, and the CPU path drew it instead (ADR-1303 D9). The log says
/// so for the display's first such frame, naming its connector, and is
/// silent for the rest.
fn failed_gpu_frame(view: &mut View, video: &dyn Video, id: DisplayId, why: &str) {
    if view.gpu_failure_logged {
        return;
    }
    view.gpu_failure_logged = true;
    log::warn!(
        "display {}: a frame failed on the GPU path ({why}): the CPU path draws it, and later failures of this display's frames are not logged",
        video.name(id)
    );
}

/// The GPU path could not set up the display `key` of `head` at its size,
/// for `why`: the whole head draws on the CPU path until that display's
/// size changes or it leaves the head (ADR-1303 D9). The log says so once,
/// naming the display's connector, and gives each of the head's display
/// lines again, naming `cpu`.
fn refused_gpu_display(
    head: &mut Head,
    videos: &BTreeMap<PathBuf, Box<dyn Video>>,
    key: &(PathBuf, DisplayId),
    why: &str,
) {
    let name = videos
        .get(&key.0)
        .map(|v| v.name(key.1))
        .unwrap_or_default();
    let (w, h) = head.views.get(key).map(|v| v.size).unwrap_or_default();
    log::warn!(
        "display {name}: the GPU path cannot set it up at {w}x{h} ({why}): head {} draws on the CPU path until the display's size changes or it leaves the head",
        head.name()
    );
    if let Some(painter) = &mut head.painter {
        painter.hold_on_cpu(true);
    }
    display_lines(head, videos);
}

/// No display of `head` is at a size the GPU path could not set up any
/// more: the head draws on the GPU path again (ADR-1303 D9). The log says
/// so and gives each of the head's display lines again, naming the path
/// that draws it.
fn back_on_gpu(head: &mut Head, videos: &BTreeMap<PathBuf, Box<dyn Video>>) {
    log::info!(
        "head {}: the GPU path can set up each of its displays again: it draws on the GPU path",
        head.name()
    );
    if let Some(painter) = &mut head.painter {
        painter.hold_on_cpu(false);
    }
    display_lines(head, videos);
}

/// Each of `head`'s display lines, naming the path that draws each one
/// now (ADR-1303 D10).
fn display_lines(head: &Head, videos: &BTreeMap<PathBuf, Box<dyn Video>>) {
    let path = head.painter.as_ref().map(Painter::path).unwrap_or_default();
    for ((node, id), view) in &head.views {
        if let Some(video) = videos.get(node) {
            display_line(&video.name(*id), video.backend(), path, view);
        }
    }
}

/// The line vtium logs for `view`, a display it draws on a head on
/// `path`, naming the path that draws the display (ADR-1303 D10).
fn display_line(name: &str, backend: &str, path: crate::painter::Path, view: &View) {
    let cell = view.cell();
    log::info!(
        "Display {name} with backend [{backend}] text renderer [{}], cell size {}x{}",
        path.for_depth(view.depth()).name(),
        cell.width,
        cell.height
    );
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
            let mut drm = Drm2d::new(Drm::new(fd)?, policy, app.display_buffers)?;
            drm.set_card_path(card.id_path.clone());
            // A display the rules leave dark is set off before it is
            // announced, a frame another master left included (D3).
            let (rules, at, lit) = (app.rules.clone(), card.clone(), Rc::clone(&app.any_lit));
            drm.set_dark(Box::new(move |named| {
                // While none is lit, it is announced for the display guard
                // to light (D9), which leaves dark again the ones it lights
                // not.
                if !lit.get() {
                    return false;
                }
                let display = vtium_devices::display(named, &at, 0);
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

    use std::sync::{Arc, Mutex};

    use vtium_platform::display::{Identity, Memory, Rect, SwapError, Swapped, Token};
    use vtium_platform::event_loop::Loop;

    use super::*;

    /// A device in memory with the displays `ids`, in the order it lists
    /// them, each 320x160 and named for the rules by its connector on the
    /// card `mem`.
    pub(super) struct Still {
        ids: Vec<(DisplayId, &'static str)>,
        pixels: Arc<Mutex<Vec<u8>>>,
        events: Vec<Event>,
        awake: bool,
        /// It sleeps as a real device does: it lists the displays it
        /// announced while asleep, and reports each sleep, so a release is
        /// acknowledged.
        like_a_device: bool,
        /// The user's input told it, its displays unblanked, and its
        /// displays powered down.
        inputs: u32,
        unblanks: u32,
        powered_down: u32,
    }

    impl Still {
        pub(super) fn new(ids: &[(u32, &'static str)]) -> Still {
            Still {
                ids: ids
                    .iter()
                    .map(|&(id, name)| (DisplayId(id), name))
                    .collect(),
                pixels: Arc::new(Mutex::new(vec![0; 320 * 160 * 4])),
                events: Vec::new(),
                awake: false,
                like_a_device: false,
                inputs: 0,
                unblanks: 0,
                powered_down: 0,
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
            if self.awake || self.like_a_device {
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
                connector_path: None,
                lit_at_start: false,
            })
        }
        fn lend(&mut self, id: DisplayId) -> Option<Token> {
            let memory: Arc<Memory> = self.pixels.clone();
            Some(Token::new(id, (320, 160), 320 * 4, 0, memory))
        }
        fn give_back(&mut self, _: Token) {}
        fn swap(&mut self, _: Token, _: &[Rect]) -> Result<Swapped, SwapError> {
            Ok(Swapped::Frame)
        }
        fn is_flipping(&self, _: DisplayId) -> bool {
            false
        }
        fn ready_for_frame(&self, id: DisplayId) -> bool {
            !self.is_flipping(id)
        }
        fn set_dpms(&mut self, _: DisplayId, dpms: Dpms) -> io::Result<()> {
            if dpms == Dpms::Off {
                self.powered_down = self.powered_down.saturating_add(1);
            }
            Ok(())
        }
        fn sleep(&mut self) {
            self.awake = false;
            if self.like_a_device {
                self.events.push(Event::Asleep);
            }
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
            self.unblanks = self.unblanks.saturating_add(1);
            Ok(())
        }
        fn input(&mut self) -> io::Result<()> {
            self.inputs = self.inputs.saturating_add(1);
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
        seat_with(&["--font-size=16"], heads, fonts)
    }

    /// [`seat`] with the options `options` in place of `--font-size=16`.
    fn seat_with(options: &[&str], heads: Option<&str>, fonts: font::Sources) -> (Loop<App>, App) {
        let args: Vec<OsString> = options
            .iter()
            .chain(&["--login", "--", "/bin/true"])
            .map(OsString::from)
            .collect();
        let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
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
            app.set_heads(start(text));
        }
        (lp, app)
    }

    /// What the start runs for a trusted head file holding `text`.
    fn start(text: &str) -> Start {
        Start::trusted(Path::new("heads.conf"), text.as_bytes())
    }

    pub(super) fn no_fonts() -> font::Sources {
        font::Sources {
            frits: PathBuf::from("/nonexistent/vtium-test-frits"),
            helper: None,
            vconsole_conf: PathBuf::from("/dev/null"),
            kernel_cmdline: PathBuf::from("/dev/null"),
        }
    }

    /// Shows the VT and runs a turn, which sets every device up.
    pub(super) fn shown(app: &mut App) {
        app.vt.activate().unwrap();
        app.turn(Instant::now());
    }

    // ADR-1401 D9 and vtium-settings's Identity: a display's order is its
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
            head.font = Some(Chosen::Builtin {
                name: Some("no-such-set"),
                cell: None,
            });
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
    // it, here `t` for both heads' look, b zoomed to a cell of its own.
    // Once one of the two switches away by OSC 50, `t` keeps the other
    // head's cell alone.
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
        let heads = "version = 1\n[look t]\nfont-name = t\n[head a]\nlook = t\n[head b]\nlook = t\n[rules]\ndisplay connector:mem/A -> a\ndisplay connector:mem/B -> b\n";
        let (_lp, mut app) = seat(Some(heads), fonts);
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A"), (2, "B")])),
        );
        shown(&mut app);
        app.zoom(1, Zoom::In);
        let asked = |app: &App| {
            let mut cells: Vec<(u32, u32)> = app
                .frits
                .get("t")
                .map(|f| {
                    f.asked
                        .iter()
                        .map(|c| (c.cell.width, c.cell.height))
                        .collect()
                })
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

    // ADR-1302 D13: one cell size can carry two bands on two heads. Here
    // head a sizes the frit `t` with `font-size` to text 16 rows high and
    // adds 25% line spacing, an 8x20 cell with 2 rows above and 2 below,
    // while head b draws `t` in the 8x20 cell it states, with no spacing.
    // The helper is asked for each, and each keeps glyphs of its own.
    #[test]
    fn one_cell_size_with_two_bands_is_asked_for_twice() {
        let dir = std::env::temp_dir().join(format!("vtium-app-bands-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.vtfrit"), "source=builtin:unifont\ncell=8x20\n").unwrap();
        let fonts = font::Sources {
            frits: dir.clone(),
            helper: Some(helper::Command {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".into(), "exec sleep 30".into()],
                account: None,
            }),
            ..no_fonts()
        };
        let heads = "version = 1\n[look spaced]\nfont-name = t\nfont-size = 16\ndisplay-dpi = 72\nfont-line-spacing = 25\n[look stated]\nfont-name = t\nfont-size = 0\n[head a]\nlook = spaced\n[head b]\nlook = stated\n[rules]\ndisplay connector:mem/A -> a\ndisplay connector:mem/B -> b\n";
        let (_lp, mut app) = seat(Some(heads), fonts);
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A"), (2, "B")])),
        );
        shown(&mut app);
        let mut asked: Vec<(u32, u32, u32, u32)> = app
            .frits
            .get("t")
            .map(|f| {
                f.asked
                    .iter()
                    .map(|a| (a.cell.width, a.cell.height, a.band.above, a.band.below))
                    .collect()
            })
            .unwrap_or_default();
        asked.sort_unstable();
        assert_eq!(asked, [(8, 20, 0, 0), (8, 20, 2, 2)]);
        drop(app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ADR-1401 D3: vtium.conf's `mouse` off drops the pointer rules from
    // the shipped rules, so a mouse falls to `input * -> ignore` and is
    // never opened; a keyboard still follows the focus.
    #[test]
    fn mouse_off_closes_a_pointer_the_shipped_rules_would_deliver() {
        let args: Vec<OsString> = ["--no-mouse"].iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().unwrap();
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            Rc::new(Instant::now),
            Vec::new(),
        );
        let input = vtium_settings::Identity::input;
        app.add_input(
            Path::new("/m"),
            input().with_class(vtium_settings::Class::Mouse),
        );
        app.add_input(
            Path::new("/k"),
            input().with_class(vtium_settings::Class::Keyboard),
        );
        assert_eq!(app.routes.head(Some(Path::new("/m")), Some(0)), None);
        assert_eq!(app.routes.head(Some(Path::new("/k")), Some(0)), Some(0));
        // A mouse whose node also has udev's key class, its consumer keys,
        // is still a pointer the shipped rules leave closed.
        let keyed = input()
            .with_class(vtium_settings::Class::Mouse)
            .with_class(vtium_settings::Class::Key);
        let rules = app.rules.clone();
        assert_eq!(
            app.routes.add(&rules, Path::new("/mk"), &keyed),
            vtium_platform::input::Disposition::Closed
        );
    }

    // ADR-1401 D3 and D7: vtium.conf's `mouse` off reaches the shipped
    // rules' tail, which a partly broken open file's inputs walk after its
    // surviving rules: a pointer there is never opened, and a keyboard
    // still roams.
    #[test]
    fn mouse_off_closes_a_pointer_the_shipped_rules_tail_would_deliver() {
        let args: Vec<OsString> = ["--no-mouse"].iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().unwrap();
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            Rc::new(Instant::now),
            Vec::new(),
        );
        let partly = "version = 1\n[head a]\nbogus = 1\n[head b]\n[rules]\ndisplay * -> b\n";
        app.set_heads(start(partly));
        assert_eq!(app.outcome.runs, vtium_settings::Runs::Survivors);
        let input = vtium_settings::Identity::input;
        let mouse = app.routes.add(
            &app.rules.clone(),
            Path::new("/m"),
            &input().with_class(vtium_settings::Class::Mouse),
        );
        let keyboard = app.routes.add(
            &app.rules.clone(),
            Path::new("/k"),
            &input().with_class(vtium_settings::Class::Keyboard),
        );
        assert_eq!(mouse, vtium_platform::input::Disposition::Closed);
        assert_eq!(keyboard, vtium_platform::input::Disposition::Deliver);
        assert_eq!(app.routes.head(Some(Path::new("/m")), Some(0)), None);
        assert_eq!(app.routes.head(Some(Path::new("/k")), Some(0)), Some(0));
    }

    // ADR-1401 D10: a head file's heads past `consoles-max` each keep a
    // console, and every console's scrollback together stays within the
    // most `sb-memory` may be: 9 heads take a ninth each.
    #[test]
    fn a_head_file_sets_each_consoles_scrollback_by_its_heads() {
        let args: Vec<OsString> = ["--sb-memory=1G"].iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
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
        app.set_heads(start(&text));
        let cap = usize::try_from(crate::config::budget::sb_memory_cap()).unwrap();
        assert_eq!(app.scrollback_bytes, cap / 9);
    }

    /// A shown VT that counts its bell's rings.
    #[derive(Default)]
    struct Bells {
        events: Vec<VtEvent>,
        rang: u32,
    }

    impl Vt for Bells {
        fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
            self.events.drain(..)
        }
        fn acknowledge_release(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn refuse_release(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn activate(&mut self) -> io::Result<vtium_platform::vt::Switch> {
            Ok(vtium_platform::vt::Switch::Done)
        }
        fn leave(&mut self) -> io::Result<vtium_platform::vt::Switch> {
            Ok(vtium_platform::vt::Switch::Done)
        }
        fn wait(&mut self) -> io::Result<vtium_platform::vt::Switch> {
            Ok(vtium_platform::vt::Switch::Done)
        }
        fn is_active(&self) -> bool {
            true
        }
        fn key(&mut self, _: &KeyEvent) -> bool {
            false
        }
        fn restore(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn bell(&mut self) {
            self.rang = self.rang.saturating_add(1);
        }
        fn number(&self) -> Option<u16> {
            None
        }
    }

    // `bell`: the heads share the one VT, so BELs from two heads' sessions
    // in one turn ring it once, and a turn with none rings nothing.
    #[test]
    fn two_heads_ringing_in_one_turn_ring_the_vt_once() {
        let args: Vec<OsString> = ["--bell"].iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().unwrap();
        let mut app = App::new(
            loaded.config,
            Box::new(Bells::default()),
            lp.handle(),
            Rc::new(Instant::now),
            Vec::new(),
        );
        app.set_heads(start("version = 1\n[head one]\n[head two]\n"));
        assert_eq!(app.heads.len(), 2);
        let rang = |app: &mut App| app.vt.get::<Bells>().map(|v| v.rang);
        for head in &mut app.heads {
            head.console.term.terminal.input(b"\x07");
        }
        app.turn(Instant::now());
        assert_eq!(rang(&mut app), Some(1));
        app.turn(Instant::now());
        assert_eq!(rang(&mut app), Some(1), "no BEL, no ring");
    }

    // `bell`, the maintainer's answer of 2026-11-01: a BEL from a console
    // the head does not show rings the VT too, since the bell is the
    // VT's and a hidden console's program may need the user's attention.
    #[test]
    fn a_hidden_consoles_bel_rings_the_vt() {
        let args: Vec<OsString> = ["--bell"].iter().map(OsString::from).collect();
        let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().unwrap();
        let mut app = App::new(
            loaded.config,
            Box::new(Bells::default()),
            lp.handle(),
            Rc::new(Instant::now),
            Vec::new(),
        );
        app.set_heads(start("version = 1\n[head one]\nconsoles = 2\n"));
        let App {
            heads,
            config,
            scrollback,
            scrollback_bytes,
            clock,
            ..
        } = &mut app;
        let head = heads.first_mut().unwrap();
        head.open_wanted(|| {
            crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock))
        });
        assert_eq!(head.console_count(), 2);
        let shown = head.console.id;
        let hidden = head.consoles_mut().find(|c| c.id != shown).unwrap();
        hidden.term.terminal.input(b"\x07");
        let rang = |app: &mut App| app.vt.get::<Bells>().map(|v| v.rang);
        app.turn(Instant::now());
        assert_eq!(rang(&mut app), Some(1));
    }

    // ADR-1401 D2, the maintainer's answer of 2026-11-01: vtium.conf's
    // `display-buffers` sets how many buffers each DRM display the seat
    // opens keeps, from the file and from the command line; unset, the
    // build's count
    #[test]
    fn display_buffers_reaches_the_count_each_display_keeps() {
        use vtium_platform::display::Buffers;
        let count = |args: &[&str], file: Option<&[u8]>| {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            let loaded = crate::config::load(&args, &mut |p: &Path| {
                Ok(file
                    .filter(|_| p == Path::new("/etc/vtium/vtium.conf"))
                    .map(<[u8]>::to_vec))
            });
            assert_eq!(loaded.reports, [], "{args:?}");
            let lp: Loop<App> = Loop::new().unwrap();
            let app = App::new(
                loaded.config,
                Box::new(FakeVt::new()),
                lp.handle(),
                Rc::new(Instant::now),
                Vec::new(),
            );
            app.display_buffers
        };
        assert_eq!(count(&[], None), crate::video::BUILD_BUFFERS, "unset");
        assert_eq!(
            count(&[], Some(b"display-buffers=1\n")),
            Buffers::One,
            "the file"
        );
        assert_eq!(
            count(&["--display-buffers=3"], None),
            Buffers::Three,
            "the command line"
        );
        assert_eq!(
            count(&["--display-buffers=2"], Some(b"display-buffers=1\n")),
            Buffers::Two,
            "the command line over the file"
        );
    }

    // vtium/vtium#203: vtium.conf's `natural-scrolling`, off by default,
    // turns the seat's
    // natural scrolling on, from the file and from the command line
    #[test]
    fn natural_scrolling_comes_from_the_file_and_the_command_line() {
        let natural = |args: &[&str], file: Option<&[u8]>| {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            let loaded = crate::config::load(&args, &mut |p: &Path| {
                Ok(file
                    .filter(|_| p == Path::new("/etc/vtium/vtium.conf"))
                    .map(<[u8]>::to_vec))
            });
            assert_eq!(loaded.reports, [], "{args:?}");
            let lp: Loop<App> = Loop::new().unwrap();
            let app = App::new(
                loaded.config,
                Box::new(FakeVt::new()),
                lp.handle(),
                Rc::new(Instant::now),
                Vec::new(),
            );
            app.natural_scrolling
        };
        assert!(!natural(&[], None), "off by default");
        assert!(natural(&[], Some(b"natural-scrolling\n")), "the file");
        assert!(natural(&["--natural-scrolling"], None), "the command line");
        assert!(
            !natural(&["--no-natural-scrolling"], Some(b"natural-scrolling\n")),
            "the command line over the file"
        );
    }

    /// Image 1, 8 by 16 pixels, with a second frame: the root frame shows
    /// 30 ms and frame 2 40 ms, and the animation loops; with `still`, the
    /// picture alone.
    #[cfg(feature = "images")]
    fn animated(still: bool) -> Vec<u8> {
        let pixels = "AAAA".repeat(8 * 16 * 4 / 3) + "AAA=";
        let mut apcs = vec![format!("a=T,f=32,s=8,v=16,i=1,q=2;{pixels}")];
        if !still {
            apcs.push(format!("a=f,i=1,f=32,s=8,v=16,z=40,q=2;{pixels}"));
            apcs.push("a=a,i=1,r=1,z=30,q=2".to_owned());
            apcs.push("a=a,i=1,s=3,q=2".to_owned());
        }
        apcs.iter()
            .flat_map(|a| format!("\x1b_G{a}\x1b\\").into_bytes())
            .collect()
    }

    /// Turns `app` at `t` until its head's render thread has handed back
    /// every frame it composed, so the next turn's frame can be taken.
    #[cfg(feature = "images")]
    fn settle(lp: &mut Loop<App>, app: &mut App, t: Instant) {
        let back = |app: &App| app.heads.iter().all(|h| h.frames_out() == 0);
        for _ in 0..1000 {
            if back(app) {
                break;
            }
            lp.dispatch(Some(Duration::from_millis(10)), app).unwrap();
            app.turn(t);
        }
        assert!(back(app), "the render thread handed every frame back");
    }

    /// The seat on one display, shown, with `bytes` written to its console
    /// and drawn at a turn at `t`.
    #[cfg(feature = "images")]
    fn drawn(bytes: &[u8], t: Instant) -> (Loop<App>, App) {
        drawn_under(None, bytes, t)
    }

    /// [`drawn`] under the head file `heads`.
    #[cfg(feature = "images")]
    fn drawn_under(heads: Option<&str>, bytes: &[u8], t: Instant) -> (Loop<App>, App) {
        let (mut lp, mut app) = seat(heads, no_fonts());
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A")])),
        );
        shown(&mut app);
        settle(&mut lp, &mut app, t);
        let term = app.term_mut().unwrap();
        let taken = term.terminal.input(bytes);
        assert_eq!(taken, bytes.len());
        assert_eq!(
            term.terminal.screen().placements().len(),
            1,
            "kitty is offered"
        );
        // as the pty's output does
        term.changed = true;
        app.turn(t);
        settle(&mut lp, &mut app, t);
        (lp, app)
    }

    /// What the animation clock of the shown console arms, and the frame it
    /// shows of the console's one picture.
    #[cfg(feature = "images")]
    fn clock(app: &App) -> (Option<Instant>, Option<u32>) {
        let term = app.term().unwrap();
        let store = term.terminal.screen().images();
        let key = term.terminal.screen().placements().first().unwrap().image;
        (term.animation.due(store), term.animation.frame(key))
    }

    // ADR-1102 D3: the loop wakes for the clock's next frame change, and a
    // still picture arms nothing, so the loop waits for its other timers.
    // covers: image.kitty-animation
    #[cfg(feature = "images")]
    #[test]
    fn the_loop_wakes_for_an_animations_frames_and_never_for_a_still_picture() {
        let t = Instant::now();
        let (_lp, app) = drawn(&animated(false), t);
        assert_eq!(clock(&app), (Some(t + Duration::from_millis(30)), Some(1)));
        assert_eq!(app.next_wake(t), Some(Duration::from_millis(30)));
        let (_lp, still) = drawn(&animated(true), t);
        assert_eq!(clock(&still), (None, None));
        // the blink timers, armed after `t`, come next
        assert!(still.next_wake(t) >= Some(Duration::from_millis(500)));
    }

    // ADR-1102 D3: a turn when a frame is due moves the clock on; while
    // the VT is another's the clock arms no timer, and shown again it
    // shows the frame the time puts it on at the first turn, armed for the
    // next change.
    // covers: image.kitty-animation
    #[cfg(feature = "images")]
    #[test]
    fn a_vt_switch_away_stops_the_clock_and_back_it_skips_to_the_time() {
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        let (_lp, mut app) = drawn(&animated(false), t);
        app.turn(ms(30));
        assert_eq!(clock(&app), (Some(ms(70)), Some(2)));
        app.vt.leave().unwrap();
        app.turn(ms(40));
        // the device reports itself asleep, and the VT goes
        let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
        let still = still.as_any().downcast_mut::<Still>().unwrap();
        still.events.push(Event::Asleep);
        app.turn(ms(40));
        assert!(!app.awake && !app.vt.is_active(), "the VT is let go");
        assert_eq!(clock(&app), (None, Some(2)));
        assert!(
            app.next_wake(ms(40))
                .is_none_or(|d| d >= Duration::from_secs(1))
        );
        app.turn(ms(5000));
        assert_eq!(clock(&app), (None, Some(2)), "no frame while away");
        app.vt.activate().unwrap();
        // 10,045 ms is 143 loops of 70 ms and 35 ms more: frame 2, from 10,040.
        app.turn(ms(10_045));
        assert_eq!(clock(&app), (Some(ms(10_080)), Some(2)));
        app.turn(ms(10_080));
        assert_eq!(clock(&app), (Some(ms(10_110)), Some(1)));
    }

    // ADR-1102 D3: a hidden console's clock arms no timer and moves no
    // frame, however many turns pass, while the shown console's plays.
    // covers: image.kitty-animation
    #[cfg(feature = "images")]
    #[test]
    fn a_hidden_consoles_clock_arms_no_timer() {
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        let heads = "version = 1\n[head one]\nconsoles = 2\n";
        let (_lp, mut app) = drawn_under(Some(heads), &animated(false), t);
        assert_eq!(clock(&app), (Some(ms(30)), Some(1)));
        let App {
            heads,
            config,
            scrollback,
            scrollback_bytes,
            clock: seat_clock,
            ..
        } = &mut app;
        let head = heads.first_mut().unwrap();
        head.open_wanted(|| {
            crate::head::terminal(
                config,
                *scrollback,
                *scrollback_bytes,
                Rc::clone(seat_clock),
            )
        });
        assert_eq!(head.console_count(), 2);
        head.show(1, true);
        let hidden = |app: &App| {
            let term = &app.heads.first().unwrap().hidden.first().unwrap().term;
            let store = term.terminal.screen().images();
            let key = term.terminal.screen().placements().first().unwrap().image;
            (term.animation.due(store), term.animation.frame(key))
        };
        for n in [30, 70, 1000] {
            app.turn(ms(n));
            assert_eq!(hidden(&app), (None, Some(1)), "hidden at {n} ms");
        }
    }

    // ADR-1401 D8: the loop wakes when a head's `when-display-lost` time runs out.
    #[test]
    fn the_loop_wakes_when_a_heads_hangup_falls_due() {
        let (_lp, mut app) = seat(Some("version = 1\n[head a]\n"), no_fonts());
        // The VT is vtium's, so the blank's countdown sets a wake later
        // than 30 s, and the earliest of the two wins. The test takes the
        // VT without a turn, which would start shorter timers, such as the
        // blink's.
        app.vt.activate().unwrap();
        let t = Instant::now();
        assert!(
            app.next_wake(t)
                .is_some_and(|w| w > Duration::from_secs(30))
        );
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
        let keyboard =
            vtium_settings::Identity::input().with_class(vtium_settings::Class::Keyboard);
        app.add_input(Path::new("/nonexistent/vtium-test/event9"), keyboard);
        app.report_focus();
        assert_eq!(app.heads.first().map(|h| h.focused), Some(false));
    }

    // vtium/vtium#229: on a fake VT the app holds one wake-up of the input
    // for as long as it runs, so the keyboards stay open after
    // Ctrl+Super+F12 releases it, and the same chord can take it back; a
    // kernel VT's release closes them
    #[test]
    fn a_released_fake_vt_keeps_the_keyboards_open_to_take_it_back() {
        use vtium_platform::input::{Direct, Input, InputConfig, Keymap};
        let f12 = InputEvent::Key(KeyEvent {
            keycode: 0,
            ascii: 0,
            mods: InputMods::CTRL.union(InputMods::LOGO),
            keysyms: vec![0xffc9],
            codepoints: vec![None],
            repeat: false,
        });
        let awake = |app: &App| app.input.as_ref().is_some_and(Input::is_awake);
        let (_lp, mut app) = seat(None, no_fonts());
        let input = || Input::new(Direct, Keymap::fallback().unwrap(), InputConfig::default());
        app.set_input(input());
        assert!(awake(&app), "open before the VT is first taken");
        // the roaming keyboard's keys go to the head with the focus
        app.focus = Some(0);
        shown(&mut app);
        assert!(app.vt.is_active() && awake(&app));
        app.input_event(f12.clone(), Instant::now());
        app.turn(Instant::now());
        assert!(!app.vt.is_active(), "Ctrl+Super+F12 released the VT");
        assert!(awake(&app), "the keyboards stay open while it is released");
        app.input_event(f12, Instant::now());
        app.turn(Instant::now());
        assert!(app.vt.is_active() && awake(&app), "the chord took it back");
        // A VT that is not the fake one holds nothing: its acquire opens the
        // keyboards and its release closes them.
        let (_lp, mut app) = seat(None, no_fonts());
        app.vt = Box::new(Bells::default());
        app.set_input(input());
        assert!(!awake(&app), "closed until the VT is taken");
    }

    // vtium/vtium#229: while the fake VT is released, with its keyboards
    // and pointers still open, the user's input wakes none of the displays
    // the other session owns, a blank that fired meanwhile included, and a
    // key or a pointer action tells no device
    #[test]
    fn input_on_a_released_fake_vt_wakes_no_display() {
        let key = |keysym: u32, mods: InputMods| {
            InputEvent::Key(KeyEvent {
                keycode: 0,
                ascii: 0,
                mods,
                keysyms: vec![keysym],
                codepoints: vec![None],
                repeat: false,
            })
        };
        let still = |app: &mut App| {
            let v = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
            let s = v.as_any().downcast_mut::<Still>().unwrap();
            (s.inputs, s.unblanks)
        };
        let (_lp, mut app) = seat(None, no_fonts());
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A")])),
        );
        shown(&mut app);
        app.focus = Some(0);
        app.input_event(
            key(0xffc9, InputMods::CTRL.union(InputMods::LOGO)),
            Instant::now(),
        );
        app.turn(Instant::now());
        assert!(!app.vt.is_active(), "Ctrl+Super+F12 released the VT");
        // The blank's timer fires while the VT is another's.
        assert!(app.blank.blank_now());
        let before = still(&mut app);
        app.input_event(key(u32::from('a'), InputMods::empty()), Instant::now());
        let moved = PointerEvent::Moved {
            x: 4,
            y: 4,
            held: None,
        };
        app.input_event(InputEvent::Pointer(moved), Instant::now());
        app.turn(Instant::now());
        assert_eq!(
            still(&mut app),
            before,
            "no device told, no display unblanked"
        );
        assert!(app.blank.blanked(), "the blank stands");
    }

    // vtium/vtium#229: while the fake VT is released the blank's timer
    // blanks nothing, however long it runs past its timeout, and wakes no
    // loop for it; taken back, the countdown starts afresh from the
    // acquire. The device lists its displays while asleep, as a real one
    // does, so a power-down would reach it.
    #[test]
    fn a_released_fake_vt_blanks_no_display_and_counts_afresh_at_the_acquire() {
        let chord = InputEvent::Key(KeyEvent {
            keycode: 0,
            ascii: 0,
            mods: InputMods::CTRL.union(InputMods::LOGO),
            keysyms: vec![0xffc9],
            codepoints: vec![None],
            repeat: false,
        });
        let powered_down = |app: &mut App| {
            let v = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
            v.as_any().downcast_mut::<Still>().unwrap().powered_down
        };
        let (_lp, mut app) = seat_with(&["--font-size=16", "--dpms-timeout=1"], None, no_fonts());
        let mut still = Still::new(&[(1, "A")]);
        still.like_a_device = true;
        app.add_video(PathBuf::from("/dev/mem0"), Box::new(still));
        shown(&mut app);
        app.focus = Some(0);
        let t = Instant::now();
        let at = |s: u64| t + Duration::from_millis(s);
        app.input_event(chord.clone(), t);
        app.turn(t);
        assert!(!app.vt.is_active(), "Ctrl+Super+F12 released the VT");
        let before = powered_down(&mut app);
        app.turn(at(5_000));
        assert_eq!(
            powered_down(&mut app),
            before,
            "nothing blanked while released"
        );
        assert!(!app.blank.blanked());
        assert!(
            app.next_wake(at(5_000)).is_none_or(|w| w > Duration::ZERO),
            "the blank wakes no loop while released"
        );
        app.input_event(chord, at(5_000));
        app.turn(at(5_000));
        assert!(app.vt.is_active(), "taken back");
        app.turn(at(5_500));
        assert_eq!(
            powered_down(&mut app),
            before,
            "the countdown started at the acquire"
        );
        app.turn(at(6_100));
        assert!(app.blank.blanked(), "a second after the acquire, it blanks");
        assert!(powered_down(&mut app) > before);
    }

    /// Two heads on one memory device at `--font-size=0`: `a` draws the
    /// bitmap frit `t` and `b` the bitmap frit `u`. Their helpers never
    /// answer, so each frit's cell query at the start has no answer in
    /// time, and each head still wants its frit's cell.
    /// The seat is shown, and the frit's directory kept for the test.
    fn late_seat(test: &str) -> (Loop<App>, App, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vtium-app-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.vtfrit"), "source=kbd:x\n").unwrap();
        std::fs::write(dir.join("u.vtfrit"), "source=kbd:y\n").unwrap();
        let fonts = font::Sources {
            frits: dir.clone(),
            helper: Some(helper::Command {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".into(), "exec sleep 30".into()],
                account: None,
            }),
            ..no_fonts()
        };
        let heads = "version = 1\n[look t]\nfont-name = t\n[look u]\nfont-name = u\n[head a]\nlook = t\n[head b]\nlook = u\n[rules]\ndisplay connector:mem/A -> a\ndisplay connector:mem/B -> b\n";
        let (lp, mut app) = seat_with(&["--font-size=0"], Some(heads), fonts);
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A"), (2, "B")])),
        );
        shown(&mut app);
        (lp, app, dir)
    }

    // ADR-1200 D17: a late outline answer at the height the head draws at
    // changes the cell's width, and the head's grid and every one of its
    // views take the new cell together, so the next frame draws each cell
    // as wide as the grid lays it out. The cases are the ones the issue
    // about late cells lists: 18x40 and 42x40 at 40 pixels, and 13x32 at
    // 32 pixels. A view is rebuilt only when its height changes or the
    // head is marked `refont`, so a grid that took the new width alone
    // would leave each view drawing the built-in font's width.
    #[test]
    fn a_late_outline_cell_is_drawn_at_its_own_width() {
        for (width, height) in [(18, 40), (42, 40), (13, 32)] {
            let test = format!("late-outline-{width}x{height}");
            let dir = std::env::temp_dir().join(format!("vtium-app-{test}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("t.vtfrit"), "source=outline:/nonexistent/t.ttf\n").unwrap();
            let fonts = font::Sources {
                frits: dir.clone(),
                helper: Some(helper::Command {
                    program: PathBuf::from("/bin/sh"),
                    args: vec!["-c".into(), "exec sleep 30".into()],
                    account: None,
                }),
                ..no_fonts()
            };
            let size = format!("--font-size={height}");
            let (_lp, mut app) = seat_with(&[&size, "--font-name=t"], None, fonts);
            app.add_video(
                PathBuf::from("/dev/mem0"),
                Box::new(Still::new(&[(1, "A")])),
            );
            shown(&mut app);
            let head = app.heads.first().unwrap();
            let at = draw_height(head, app.dpi(0), &app.locale);
            assert_eq!(at, height, "the head draws at {height} pixels");
            let view = |app: &App| {
                let head = app.heads.first().unwrap();
                let v = head.views.values().next().unwrap();
                (v.cell().width, v.cell().height)
            };
            let fallback = view(&app);
            assert_ne!(fallback.0, width, "the fallback's width differs");
            let known = app.frits.get("t").map(|f| f.own.clone()).unwrap();
            assert!(
                known
                    .get(&at)
                    .is_some_and(|k| k.as_ref().is_err_and(|no| no.late)),
                "the start's query had no answer in time: {known:?}"
            );
            let cell = vtium_font::Cell { width, height };
            app.late_cell("t", at, Ok(helper::OwnCell::Outline(cell)));
            let head = app.heads.first().unwrap();
            assert_eq!(
                head.cell,
                (width, height),
                "{width}x{height}: the head's cell"
            );
            assert_eq!(
                view(&app),
                (width, height),
                "{width}x{height}: the view's cell"
            );
            let columns = usize::try_from(320 / width).unwrap();
            let rows = usize::try_from(160 / height).unwrap();
            assert_eq!(head.grid(), (columns, rows), "{width}x{height}: the grid");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    // ADR-1200 D17 (the maintainer, 2026-11-03): a late answer lays out
    // again only the heads drawing the frit that take a cell from it. An
    // outline cell for a height no head draws at, given to frit `u`, lays
    // out nothing; a bitmap first source's own cell, given to frit `t`,
    // lays out the head drawing `t` and not the head drawing `u`, which
    // still wants its own cell. Each frit gets one kind of answer, as one
    // frit's first source answers one kind for the whole boot. The views'
    // cells and the heads' grids show what was laid out: the late cell is
    // as tall as the cell drawn before it and wider, so a layout that
    // rebuilt no view leaves the view drawing the old width.
    #[test]
    fn a_late_cell_lays_out_only_the_heads_that_take_it() {
        let (_lp, mut app, dir) = late_seat("late-heads");
        let at = app.heads.first().map(|h| h.font_size).unwrap();
        let known = app.frits.get("t").map(|f| f.own.clone()).unwrap();
        assert!(
            known
                .get(&at)
                .is_some_and(|k| k.as_ref().is_err_and(|no| no.late)),
            "the start's query had no answer in time: {known:?}"
        );
        let drawn = |app: &App| {
            app.heads
                .iter()
                .map(|h| (h.cells(), h.grid()))
                .collect::<Vec<_>>()
        };
        let before = drawn(&app);
        let b_before = before.get(1).unwrap();
        let &(width, height) = before.first().unwrap().0.first().unwrap();
        let other = at + 11;
        let outline = vtium_font::Cell {
            width: 9,
            height: other,
        };
        app.late_cell("u", other, Ok(helper::OwnCell::Outline(outline)));
        assert_eq!(
            drawn(&app),
            before,
            "an outline cell at {other} lays out nothing"
        );
        let own = vtium_font::Cell {
            width: width + 2,
            height,
        };
        app.late_cell("t", at, Ok(helper::OwnCell::Bitmap(own)));
        let after = drawn(&app);
        let (a_cells, (a_columns, _)) = after.first().unwrap();
        let wide = (own.width, own.height);
        assert_eq!(app.heads.first().map(|h| h.cell), Some(wide));
        assert_eq!(a_cells.as_slice(), [wide], "a is laid out in the late cell");
        let columns = usize::try_from(320 / own.width).unwrap();
        assert_eq!(*a_columns, columns, "a's grid takes the late cell");
        assert_eq!(after.get(1).unwrap(), b_before, "b is not laid out");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ADR-1200 D17: a late answer takes the place of a missing answer
    // alone. Where an OSC 50 switch back asked again at the same height
    // and its answer came in time, the helper hands on the first query's
    // answer late, behind it (the helper's test
    // `a_query_asked_again_keeps_its_own_answer_and_the_earlier_one_late`);
    // the answer in time stays, and the late one lays out nothing. A
    // refusal in time stays too; a late refusal fills a missing answer.
    #[test]
    fn a_late_cell_answer_fills_only_a_missing_answer() {
        let (_lp, mut app, dir) = late_seat("late-missing");
        let at = app.heads.first().map(|h| h.font_size).unwrap();
        let cell = |w, h| vtium_font::Cell {
            width: w,
            height: h,
        };
        let in_time = Ok(helper::OwnCell::Bitmap(cell(8, 16)));
        let refused = Err(helper::NoCell {
            why: "no such font".to_owned(),
            late: false,
        });
        if let Some(frit) = app.frits.get_mut("t") {
            frit.own.insert(at, in_time.clone());
            frit.own.insert(at + 1, refused.clone());
        }
        for head in &mut app.heads {
            head.refont = true;
        }
        app.late_cell("t", at, Ok(helper::OwnCell::Bitmap(cell(16, 32))));
        app.late_cell("t", at + 1, Ok(helper::OwnCell::Bitmap(cell(16, 32))));
        let late_refusal = Err(helper::NoCell {
            why: "late".to_owned(),
            late: false,
        });
        app.late_cell("t", at + 2, late_refusal.clone());
        let known = app.frits.get("t").map(|f| f.own.clone()).unwrap();
        assert_eq!(known.get(&at), Some(&in_time));
        assert_eq!(known.get(&(at + 1)), Some(&refused));
        assert_eq!(known.get(&(at + 2)), Some(&late_refusal));
        assert!(app.heads.iter().all(|h| h.refont), "nothing is laid out");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ADR-1200 D17: a bitmap frit's answer holds at every height, so a
    // refusal that came in time at one height holds at all of them. A late
    // bitmap cell is then dropped, at the start's height, whose query had
    // no answer in time, and at a height no query was asked at, and it
    // never becomes the cell that `answer_at`, which looks for a bitmap
    // cell first, finds.
    #[test]
    fn a_late_bitmap_cell_yields_to_a_refusal_in_time_at_any_height() {
        let (_lp, mut app, dir) = late_seat("late-refused");
        let at = app.heads.first().map(|h| h.font_size).unwrap();
        let refused = Err(helper::NoCell {
            why: "no such font".to_owned(),
            late: false,
        });
        if let Some(frit) = app.frits.get_mut("t") {
            assert!(
                frit.own
                    .get(&at)
                    .is_some_and(|k| k.as_ref().is_err_and(|no| no.late)),
                "the start's query had no answer in time: {:?}",
                frit.own
            );
            frit.own.insert(at + 5, refused.clone());
        }
        for head in &mut app.heads {
            head.refont = true;
        }
        let own = Ok(helper::OwnCell::Bitmap(vtium_font::Cell {
            width: 8,
            height: 16,
        }));
        app.late_cell("t", at, own.clone());
        app.late_cell("t", at + 3, own);
        let frit = app.frits.get("t").unwrap();
        assert!(
            frit.own.values().all(Result::is_err),
            "no late cell kept: {:?}",
            frit.own
        );
        assert_eq!(frit.answer_at(at, false), Some(refused.clone()));
        assert_eq!(frit.answer_at(at + 3, false), Some(refused));
        assert!(app.heads.iter().all(|h| h.refont), "nothing is laid out");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ADR-1200 D17: the answers, and not the frit's text, say which kind a
    // frit is. Frit `t`'s text guesses bitmap (`source=kbd:x`), as
    // `font-name=/path/x.ttf` makes a frit whose first source is in fact an
    // outline font. With an outline cell held at one height, a head at
    // another height has no answer and asks at its own, and a head at the
    // first height, still guessing bitmap, gets that cell, whatever failed
    // at the other height.
    #[test]
    fn an_outline_answer_holds_at_its_height_alone_whatever_the_text_guesses() {
        let (_lp, mut app, dir) = late_seat("late-kind");
        let at = 16;
        let outline = Ok(helper::OwnCell::Outline(vtium_font::Cell {
            width: 9,
            height: at,
        }));
        let failures = [
            Err(helper::NoCell {
                why: "no answer in time".to_owned(),
                late: true,
            }),
            Err(helper::NoCell {
                why: "no such font".to_owned(),
                late: false,
            }),
        ];
        let frit = app.frits.get_mut("t").unwrap();
        frit.own.clear();
        frit.own.insert(at, outline.clone());
        // The second head, at 20, asks at its own height.
        assert_eq!(frit.answer_at(20, false), None, "the second head asks");
        for failed in failures {
            frit.own.insert(20, failed.clone());
            // A third head at 16 takes the outline cell held there.
            assert_eq!(
                frit.answer_at(at, false),
                Some(outline.clone()),
                "the third head, beside {failed:?} at 20"
            );
            assert_eq!(frit.answer_at(20, false), Some(failed));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A seat on one display under `render-timer=value`, shown at the
    /// returned time, its first frames handed back.
    fn timer_seat(value: &str) -> (Loop<App>, App, Instant) {
        let option = format!("--render-timer={value}");
        let (mut lp, mut app) = seat_with(&["--font-size=16", &option], None, no_fonts());
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A")])),
        );
        let t = Instant::now();
        app.vt.activate().unwrap();
        app.turn(t);
        settle_frames(&mut lp, &mut app, t);
        (lp, app, t)
    }

    /// Turns `app` at `at` until every head's render thread has handed
    /// back each frame it composed.
    fn settle_frames(lp: &mut Loop<App>, app: &mut App, at: Instant) {
        let back = |app: &App| app.heads.iter().all(|h| h.frames_out() == 0);
        for _ in 0..1000 {
            if back(app) {
                break;
            }
            lp.dispatch(Some(Duration::from_millis(10)), app).unwrap();
            app.turn(at);
        }
        assert!(back(app), "the render thread handed every frame back");
    }

    /// The overlay the first display draws next.
    fn first_overlay(app: &App) -> crate::term::Overlay {
        let head = app.heads.first().unwrap();
        head.views.values().next().unwrap().overlay.clone()
    }

    /// The text of the box the first display draws next.
    fn box_text(app: &App) -> Option<String> {
        first_overlay(app).notice.map(|n| n.text().to_owned())
    }

    // ADR-1401 D5: under `render-timer=osd` the console measures each
    // display's frames, and once a second draws the render timer's line,
    // `Report::osd`'s, in the on-screen display's box, in the on-screen
    // display's colors; under `log` it draws none. The display reports
    // three flips in the second, so the line reads 3.0 frames a second.
    #[test]
    fn render_timer_osd_draws_each_seconds_figures_in_the_box() {
        for (value, shows) in [("osd", true), ("both", true), ("log", false)] {
            let (mut lp, mut app, t) = timer_seat(value);
            let measures = app
                .heads
                .first()
                .and_then(|h| h.views.values().next())
                .is_some_and(|v| v.timer.is_some());
            assert!(measures, "{value}: the console measures");
            assert_eq!(box_text(&app), None, "{value}: no line before a second");
            let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
            let still = still.as_any().downcast_mut::<Still>().unwrap();
            still.events.extend([Event::Flipped(DisplayId(1)); 3]);
            let half = t + Duration::from_millis(500);
            app.turn(half);
            settle_frames(&mut lp, &mut app, half);
            let second = t + Duration::from_secs(1);
            app.turn(second);
            settle_frames(&mut lp, &mut app, second);
            let line = box_text(&app);
            if !shows {
                assert_eq!(line, None, "{value}: no line on screen");
                continue;
            }
            let line = line.unwrap_or_default();
            assert!(
                line.starts_with("3.0 fps ") && line.contains(" ms ") && line.ends_with('%'),
                "{value}: the line drawn is {line:?}"
            );
            assert!(
                !first_overlay(&app).problem,
                "{value}: in the on-screen display's colors"
            );
        }
    }

    // ADR-1401 D5: a render timer line measured before the VT switched
    // away describes no second after it. Shown again, a display that kept
    // its view across the switch, as on a DRM card, draws no line until
    // the first report after the switch back, a second later, which draws
    // one again.
    #[test]
    fn a_vt_switch_drops_the_render_timer_line_until_the_next_report() {
        let (mut lp, mut app, t) = timer_seat("osd");
        let second = t + Duration::from_secs(1);
        app.turn(second);
        settle_frames(&mut lp, &mut app, second);
        assert!(
            box_text(&app).is_some_and(|l| l.contains(" fps ")),
            "the first second's line: {:?}",
            box_text(&app)
        );
        app.vt.leave().unwrap();
        let away = second + Duration::from_millis(10);
        app.turn(away);
        let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
        let still = still.as_any().downcast_mut::<Still>().unwrap();
        still.events.push(Event::Asleep);
        app.turn(away);
        assert!(!app.awake && !app.vt.is_active(), "the VT is let go");
        // A DRM card keeps its displays across the switch and reports
        // `Awake` at the wake-up, so each display keeps its view and the
        // line on it. The stand-in would announce its displays again,
        // which builds new views, so it wakes as the card does.
        let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
        let still = still.as_any().downcast_mut::<Still>().unwrap();
        still.awake = true;
        still.events.push(Event::Awake);
        let kept = app.heads.first().map(|h| h.views.len());
        app.vt.activate().unwrap();
        let back = second + Duration::from_secs(5);
        app.turn(back);
        settle_frames(&mut lp, &mut app, back);
        assert!(app.awake, "the VT is shown again");
        assert_eq!(
            app.heads.first().map(|h| h.views.len()),
            kept,
            "the display kept its view"
        );
        assert!(
            app.heads
                .first()
                .and_then(|h| h.views.values().next())
                .is_some_and(|v| v.timer.is_some()),
            "the view measures still"
        );
        assert_eq!(box_text(&app), None, "no line until the next report");
        let next = back + Duration::from_secs(1);
        app.turn(next);
        settle_frames(&mut lp, &mut app, next);
        assert!(
            box_text(&app).is_some_and(|l| l.contains(" fps ")),
            "the next report's line: {:?}",
            box_text(&app)
        );
    }

    #[test]
    fn display_depth_overrides_what_the_device_says() {
        let config = |args: &[&str]| {
            let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
            crate::config::load(&args, &mut |_: &std::path::Path| Ok(None)).config
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
