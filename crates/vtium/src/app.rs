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
//!   (`Head::pointer_event`).
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
//!   ([`crate::painter`]). The loop takes each frame from the screen with
//!   the buffer its display lends, and the thread composes the frame
//!   straight into that buffer; no copy is kept. At the next turn's
//!   start, after the VT's events, the loop gives the buffer back to the
//!   display, which presents it ([`crate::term::View::present`]).
//! - **Exit.** A request (SIGTERM, SIGINT, a hangup, the policy) switches
//!   back to the VT shown at the start and serves the loop for up to
//!   [`LEAVE_WAIT`] while the switch is pending.

use std::any::Any;
use std::cell::{Cell, RefCell};
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
use vtium_term::{BlankRequest, BlankTimeout, Key, KeyEvent as TermKeyEvent, Mods};

use crate::blank::Order;
use crate::blink::Timer;
use crate::child::{self, Display as SessionDisplay, Place};
use crate::config::keys;
use crate::config::{Config, ModeSpec};
use crate::font::{self, Chosen};
use crate::grab::{self, Bindings, Grab};
use crate::head::chord::{self, HeadChord};
use crate::head::console::Console;
use crate::head::route::{self, Routes};
use crate::head::session::{self, AfterExit, Caps};
use crate::head::vt::VtHeads;
use crate::head::{Attached, Frit, Head, Start, place_display};
use crate::helper::{self, Answer, FritText, Helper};
use crate::layout::{self, Zoom};
use crate::ordered;
use crate::osd::{self, Osd};
use crate::painter::{Note, Painter};
use crate::power::{self, Power};
use crate::respawn;
use crate::sync::Action;
use crate::term::{Clock, Prepared, ScrollDraw, Term, View};
use crate::video::Video;
use crate::wait::Wait;

mod batch;
#[cfg(test)]
mod batch_login;
#[cfg(test)]
mod colorterm;
#[cfg(test)]
mod cursor_blink;
mod enter_switch;
mod hand_over;
mod held;
#[cfg(feature = "logind")]
pub mod keyboard_layout;
#[cfg(test)]
mod login_scope;
mod pointers;
#[cfg(test)]
mod rapid;
mod survive;
mod window_ops;

pub use enter_switch::{RunningVts, VtiumVts};

/// How long the loop is served at exit while the switch back is pending,
/// so the kernel's SIGUSR2 for the switch back is answered before vtium
/// ends.
pub const LEAVE_WAIT: Duration = Duration::from_millis(100);

/// How long the on-screen display's brief line shows where `osd-timeout`
/// is `never` (ADR-1401 D5): it answers one press, so it still goes.
const BRIEF_NEVER: Duration = Duration::from_secs(3);

/// The longest part of a font's name, in bytes, that an OSC 50 log line
/// shows.
const LOGGED_NAME_MAX: usize = 200;

/// A font's name as an OSC 50 log line shows it. A program chooses the
/// name, so vtium escapes each character that is not printable, such as
/// DEL, a C1 control or a right-to-left override, as `\u{7f}`. Such a
/// character could otherwise hide or reorder what a person reads in the
/// log. A name longer than [`LOGGED_NAME_MAX`] bytes is cut short at a
/// character's boundary, and a mark says so and gives the whole name's
/// length.
fn logged_name(name: &str) -> String {
    let mut end = name.len().min(LOGGED_NAME_MAX);
    while !name.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let (kept, cut) = name.split_at(end);
    let shown = kept.escape_debug().to_string();
    if cut.is_empty() {
        shown
    } else {
        format!("{shown}... (cut short: the name has {} bytes)", name.len())
    }
}

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

/// A display that shows a notice of its own, under open, while no path
/// catches it on this VT (ADR-1401 D3).
#[derive(Debug)]
struct Unused {
    /// The head of [`App::heads`] that draws the notice, after the heads
    /// of this VT and the notice screen. It goes to
    /// [`App::spare_notices`] when the display goes.
    head: usize,
    /// When the notice's time is up: `osd-timeout` from the first turn it
    /// is on the display, or from the last switch back to this VT, wake
    /// from the blank or key press since.
    due: Option<Instant>,
    /// Set off and its buffers freed once the time was up, until a key on
    /// this VT or a switch back to it.
    parked: bool,
    /// What the notice says.
    text: String,
}

/// How a session's child gets a systemd scope of its own: given the VT's
/// or the seat's name and the child's process id, the scope's name, or why
/// it has none.
pub type LoginScope = Box<dyn FnMut(&str, u32) -> Result<String, String>>;

/// Asks systemd's manager for the child's scope (ADR-1400 D3).
#[cfg(feature = "logind")]
fn login_scope(place: &str, pid: u32) -> Result<String, String> {
    use vtium_platform::logind::scope;
    let unit = scope::unit_name(place, pid);
    scope::start(&unit, pid)
        .map(|()| unit)
        .map_err(|e| e.to_string())
}

/// A build without sd-bus cannot ask systemd's manager for the scope.
#[cfg(not(feature = "logind"))]
fn login_scope(_: &str, _: u32) -> Result<String, String> {
    Err("this vtium is built without the logind feature, which asks systemd for it".to_owned())
}

/// The log's line for a session on `place` that runs in the scope `unit`.
fn in_scope(place: &str, unit: &str) -> String {
    format!("The session on {place} runs in the systemd scope {unit}.")
}

/// The log's line for a session on `place` that gets no scope, because of
/// `why`.
fn no_scope(place: &str, why: &str) -> String {
    format!(
        "The session on {place} gets no logind session of its own: vtium cannot move it into a systemd scope, because {why}. The session runs inside vtium's own logind session instead."
    )
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
    /// Which heads of [`App::rules`] vtium built, the heads of its VT
    /// (ADR-1401 D2), and the VT it places devices for.
    vt_heads: VtHeads,
    /// What the start's head file resolved to (ADR-1401 D7).
    outcome: vtium_settings::Outcome,
    /// The panic screen's report, for a broken closed head file (D7).
    report: Option<vtium_render::NoticeReport>,
    /// How many heads of [`App::heads`] are this VT's own heads, which come
    /// first (ADR-1401 D2).
    built: usize,
    /// The index in [`App::heads`] of the screen the display guard lights
    /// for a notice alone, which a closed policy has, after every head of
    /// this VT (D9).
    screen: Option<usize>,
    /// Under open, the notice of each display that no path catches on this
    /// VT (ADR-1401 D3), by its device and its connector.
    notices: BTreeMap<(PathBuf, String), Unused>,
    /// The heads of [`App::heads`] that drew the notice of a display that
    /// has gone, or now has a path, each kept for the next display that
    /// needs a notice, so a display unplugged and plugged in again adds no
    /// head.
    spare_notices: Vec<usize>,
    /// Under `osd = silent`, each display that no path catches on this VT,
    /// by its device and its connector, whose notice vtium logged in place
    /// of showing it. Each is logged once while it stays unused.
    silenced: BTreeSet<(PathBuf, String)>,
    /// The displays a DRM card announces for their notices though the rules
    /// leave them dark, by device and connector, which each card's dark
    /// policy reads.
    wanted: Rc<RefCell<BTreeSet<(PathBuf, String)>>>,
    /// A crash loop ended every head's sessions under the closed policy:
    /// no session starts again, no input is delivered, and the boot
    /// display shows why for as long as vtium runs (D1).
    stopped: bool,
    /// The head the inputs following the focus deliver to.
    focus: Option<usize>,
    /// Where each input device's events go.
    routes: Routes,
    /// The keys each console received that are still held (ADR-1005 D7).
    held: held::HeldKeys,
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
    /// Each video device the seat manager is not ready to give yet, with
    /// what udev reported of it, opened once the VT names it settled
    /// (vtium/vtium#34).
    waiting_videos: BTreeMap<PathBuf, (DeviceKind, Card)>,
    /// Each display lit in a head, as the rules saw it.
    lit: BTreeMap<(PathBuf, DisplayId), vtium_settings::Identity>,
    /// Each display in a head that its device cannot light, still plugged
    /// in, as the rules saw it. Its head keeps it, but the guards do not
    /// count it as lit until it lights again (ADR-1401 D8, D9).
    unlit: BTreeMap<(PathBuf, DisplayId), vtium_settings::Identity>,
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
    /// The kernel's messages, which vtium shows on the boot display's
    /// head (ADR-1401 D15); `None` where vtium cannot read them.
    kernel: Option<crate::kernel::Messages>,
    /// Who shows them while Plymouth runs, vtium.conf's
    /// `kernel-messages`.
    kernel_deferral: crate::kernel::Deferral,
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
    /// Device events a test hands the next turn, as udev would.
    #[cfg(test)]
    queued_devices: Vec<DeviceEvent>,
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
    /// Where the console reads its batch, which lists the active
    /// resources, and the trust rule each resource passes: this
    /// machine's, or a test's.
    batch_places: (vtium_settings::batch::Places, vtium_settings::cullet::Rule),
    /// How many times a reading of the batch found it changed since vtium
    /// started. Each head keeps the count it last caught up with.
    batch_changes: u64,
    /// The pointer files the console loads: the top level's
    /// `pointer-theme`, and each other one a head's look names, each with
    /// its loader and the store its displays' views read.
    pointers: Vec<pointers::Theme>,
    /// Each frit a head draws, by its name, with its one helper.
    frits: BTreeMap<String, Frit>,
    /// What each frit's helpers have used up this boot, by the frit's
    /// name: the chances, and the fonts left out because they made a
    /// helper fail. A frit that no head draws any more is dropped from
    /// `frits` with its helper, and its record stays here, so a frit that
    /// comes back has no fresh chances and leaves out the same fonts.
    ///
    /// References:
    /// - ADR-1200 D15, "Restarted after a failure", and its note of
    ///   2026-11-21: three chances a boot bound the restarts, because each
    ///   start re-randomizes the helper's address layout.
    failures: BTreeMap<String, helper::FailureRecord>,
    /// The kernel's font, read from the VT at start for a frit's
    /// `console:` source (vtium/vtium#145).
    console_font: Option<vtium_platform::vt::font::ConsoleFont>,
    /// The locale, which keeps the built-in Unscii-8 from a zh, ja or ko
    /// locale (ADR-1200 D7).
    locale: String,
    exit: Option<Exit>,
    leaving: Option<Leaving>,
    inherited: Vec<(OsString, OsString)>,
    /// Moves a session's child, by its place and process id, into a systemd
    /// scope of its own on a seat that libseat's logind backend opened,
    /// answering the scope's name or why not; the tests stand in for it.
    pub login_scope: LoginScope,
    /// Asks which VTs vtium runs on, for Enter on a keyboard of a head on
    /// another VT (ADR-1401 D3); the tests stand in for it.
    running_vts: RunningVts,
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
    /// The keys each key binding of vtium.conf binds, in the key order
    /// (ADR-1005 D10).
    bindings: Bindings,
    /// `pointer-hide-while-typing`.
    hide_while_typing: bool,
    /// The wheel scrolls and reports the other way (vtium/vtium#203).
    natural_scrolling: bool,
    /// What the power keys act through.
    power: Box<dyn Power>,
    /// The child writing `/sys/power/state` has not exited: the machine
    /// sleeps, or is going to.
    sleeping: Rc<Cell<bool>>,
    /// The hand-overs of the display in force (ADR-1401 D16).
    hand_overs: hand_over::HandOvers,
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
        // The batch the start reads, which lists the active resources, and
        // what vtium found reading its file, logged once.
        let trust = font::trust();
        if let Some(found) = font::start_batch(
            &vtium_settings::batch::Places::system(),
            &trust,
            vtium_settings::cullet::Rule::new(trust.clone()),
        ) {
            found.log();
        }
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
        // The head file's heads replace these before any display comes.
        let heads = session::interim_heads(&rules, &config, ((cap, lines), bytes), caps, &clock);
        let built = heads.len();
        let dpms = config
            .number(&keys::DPMS_TIMEOUT)
            .map(Duration::from_secs)
            .filter(|d| !d.is_zero());
        let modes = config.modes(&keys::MODE).to_vec();
        let use_original = config.switch(&keys::USE_ORIGINAL_MODE) && modes.is_empty();
        // The key holds text only for `forever`.
        let forever = config.text(&keys::DISPLAY_WAIT).is_some();
        let wait = Wait::new(
            (!forever)
                .then(|| Duration::from_secs(config.number(&keys::DISPLAY_WAIT).unwrap_or(10))),
        );
        #[cfg(feature = "images")]
        let image_helpers = Some(crate::head::images::hosts(&config, &inherited));
        #[cfg(not(feature = "images"))]
        let _ = crate::head::images::program(&config);
        App {
            oneshot: config.switch(&keys::ONESHOT),
            clipboard_write: config.switch(&keys::CLIPBOARD_WRITE),
            program_blank: config.switch(&keys::PROGRAM_BLANK),
            bell: config.switch(&keys::BELL),
            dpms_timeout: dpms,
            order: None,
            selection: None,
            bindings: Bindings::of(&config),
            hide_while_typing: config.switch(&keys::POINTER_HIDE_WHILE_TYPING),
            natural_scrolling: config.switch(&keys::NATURAL_SCROLLING),
            power: Box::new(power::System),
            sleeping: Rc::default(),
            hand_overs: hand_over::HandOvers::default(),
            modes,
            use_original,
            display_buffers: crate::video::display_buffers(config.display_buffers()),
            vt,
            videos: BTreeMap::new(),
            input: None,
            udev: None,
            blank: Blank::new(dpms, now),
            heads,
            vt_heads: VtHeads::new(&rules, None),
            rules,
            outcome: vtium_settings::resolve(&vtium_settings::Read::Missing),
            report: None,
            built,
            screen: None,
            notices: BTreeMap::new(),
            spare_notices: Vec::new(),
            silenced: BTreeSet::new(),
            kernel: None,
            kernel_deferral: crate::kernel::Deferral::configured(&config),
            wanted: Rc::default(),
            stopped: false,
            focus: None,
            routes: Routes::default(),
            held: held::HeldKeys::default(),
            focus_move: None,
            osd: Osd::configured(&config),
            focus_settled: false,
            focus_due: false,
            cards: BTreeMap::new(),
            waiting_videos: BTreeMap::new(),
            lit: BTreeMap::new(),
            unlit: BTreeMap::new(),
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
            launcher: helper::Launcher::configured(&config)
                .map(|l| l.with_locale(&locale(&inherited))),
            batch_places: (
                vtium_settings::batch::Places::system(),
                vtium_settings::cullet::Rule::new(font::trust()),
            ),
            batch_changes: 0,
            pointers: Vec::new(),
            frits: BTreeMap::new(),
            failures: BTreeMap::new(),
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
            #[cfg(test)]
            queued_devices: Vec::new(),
            wait,
            presented: false,
            timer_due: None,
            #[cfg(feature = "images")]
            image_helpers,
            exit: None,
            leaving: None,
            // No session inherits systemd's word to vtium (ADR-1401 D13).
            inherited: crate::survive::session_env(inherited),
            login_scope: Box::new(login_scope),
            running_vts: Box::new(enter_switch::running_vts),
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
            vt,
        } = start;
        let rules = route::in_force(rules, &self.config);
        // The heads of this VT (D2), or the default head alone where the
        // policy table gives the built-in default (D3), or with no VT the
        // display guard's default head, which it lights under open (D9).
        let vt_heads = VtHeads::new(&rules, vt);
        let declared = vtium_settings::Config {
            heads: vt_heads.heads(&rules),
            ..rules.clone()
        };
        let caps = self.caps;
        self.scrollback_bytes = session::scrollback_bytes(&self.config, caps, declared.heads.len());
        let scrollback = (self.scrollback, self.scrollback_bytes);
        self.let_go_of_painters();
        self.heads = session::heads(&declared, &self.config, scrollback, caps, &self.clock);
        for head in &mut self.heads {
            head.batch = self.batch_changes;
        }
        self.built = self.heads.len();
        self.notices.clear();
        self.spare_notices.clear();
        self.silenced.clear();
        self.wanted.borrow_mut().clear();
        self.screen = (rules.policy == Policy::Closed).then(|| {
            let term = crate::head::terminal(&self.config, (0, 0), 0, Rc::clone(&self.clock));
            self.heads.push(Head::notice_screen(&self.config, term));
            self.heads.len().saturating_sub(1)
        });
        if outcome.display_wait() == vtium_settings::DisplayWait::Forever {
            self.wait = Wait::new(None);
        }
        self.rules = rules;
        self.vt_heads = vt_heads;
        self.outcome = outcome;
        self.report = report;
        self.focus = None;
        self.lit.clear();
        self.unlit.clear();
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
    /// (ADR-1401 D7). On a VT where no head runs, `text` shows instead
    /// under each display's own notice (D3), until vtium parks that
    /// display, so the person at the keyboard still reads it.
    pub fn notice(&mut self, text: &str) {
        if self.heads().is_empty() {
            for n in self.notices.values() {
                if let Some(head) = self.heads.get_mut(n.head) {
                    head.add_notice(text);
                }
            }
            return;
        }
        for head in self.heads.iter_mut().filter(|h| !h.notice_only) {
            head.add_notice(text);
        }
    }

    /// Shows the kernel's messages that `kernel` gives (ADR-1401 D15),
    /// under vtium.conf's `kernel-messages`, each read at
    /// [`App::kernel_ready`].
    pub fn set_kernel(&mut self, kernel: Box<dyn crate::kernel::Kernel>) {
        self.kernel = Some(crate::kernel::Messages::new(kernel, self.kernel_deferral));
    }

    /// The kernel has messages to read (ADR-1401 D15). vtium reads every
    /// one, and shows those it keeps over the console of the head that
    /// holds the boot display, while its VT is shown, as the kernel's own
    /// console shows them on the VT that is shown. vtium never writes them
    /// to a session's pty or into its screen. A message that arrives
    /// while the VT is not shown, or while no head holds a display, is
    /// never shown later. It returns false once vtium cannot read the
    /// kernel's messages any more, and logs why.
    pub fn kernel_ready(&mut self) -> bool {
        let Some(messages) = self.kernel.as_mut() else {
            return false;
        };
        let lines = match messages.take() {
            Ok(lines) => lines,
            Err(e) => {
                log::warn!(
                    "vtium cannot read the kernel's messages from {} any more: {e}. It shows no more of them on screen.",
                    crate::kernel::KMSG
                );
                self.kernel = None;
                return false;
            }
        };
        if lines.is_empty() || !self.vt.is_active() {
            return true;
        }
        if let Some(head) = self.kernel_head().and_then(|h| self.heads.get_mut(h)) {
            for line in &lines {
                head.add_kernel(line);
            }
        }
        true
    }

    /// The head that holds the boot display (ADR-1401 D7, D9), the head
    /// the kernel's messages show on: of the displays lit in a head, the
    /// one lit at start, else the first on the boot card, else the first
    /// on the other cards, in the kernel's order. The screen that shows a
    /// notice alone draws no kernel message.
    fn kernel_head(&self) -> Option<usize> {
        let (key, _) = self
            .lit
            .iter()
            .min_by_key(|(_, d)| (!d.lit_at_start, !d.on_boot_card, d.card_order, d.order))?;
        self.heads
            .iter()
            .position(|h| !h.notice_only && h.views.contains_key(key))
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
            self.launcher =
                helper::Launcher::open(command.clone()).map(|l| l.with_locale(&self.locale));
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
        // a libseat build opens the keyboards and pointers through the
        // seat manager that gave it the VT (ADR-1400's L2 layer)
        #[cfg(feature = "libseat")]
        if let Some(seat) = self.vt.get::<vtium_platform::seat::LibseatVt>() {
            input.open_through(seat.input());
        }
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
        self.heads.get(..self.built).unwrap_or_default()
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

    /// The notice the display named `connector` shows of its own, under
    /// open, while no path catches it on this VT (ADR-1401 D3): its words,
    /// and whether it is on the display now.
    pub fn display_notice(&self, connector: &str) -> Option<(&str, bool)> {
        let head = self.notice_head(connector)?;
        Some((head.notice()?, !head.views.is_empty()))
    }

    /// The screen that draws the notice of the display named `connector`
    /// ([`App::display_notice`]).
    pub fn notice_head(&self, connector: &str) -> Option<&Head> {
        let (_, n) = self.notices.iter().find(|((_, c), _)| c == connector)?;
        self.heads.get(n.head)
    }

    /// How many screens vtium keeps: the heads of its VT, the notice
    /// screen of a closed policy, and the screens that draw the notices
    /// of displays no path catches.
    pub fn screen_count(&self) -> usize {
        self.heads.len()
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
            // The first login prompt waits for the login picture, which the
            // head asks for at its first display, so that a banner is in
            // place before the prompt's first line.
            //
            // References:
            // - ADR-1100 D17, "When vtium places the picture": the wait.
            #[cfg(feature = "images")]
            if self.heads.get(h).is_some_and(Head::login_image_waits) {
                continue;
            }
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

    /// Adds a video device, woken while the VT is ours and no hand-over
    /// holds its place, and else put to sleep, which lets go of the DRM
    /// master a card's open took; the VT's acquire, or the hand-over's
    /// end, wakes it (vtium/vtium#139).
    pub fn add_video(&mut self, node: PathBuf, mut video: Box<dyn Video>) {
        if let Some(check) = self.vt.release_check() {
            video.set_interrupt(check);
        }
        // A card a hand-over gave up stays asleep until the hand-over ends
        // (ADR-1401 D16).
        if self.awake && !self.hand_overs.given_up(&node) {
            if let Err(e) = video.wake_up() {
                log::warn!("cannot wake {}: {e}", node.display());
            }
        } else {
            video.sleep();
        }
        self.videos.insert(node, video);
    }

    /// Lets go of each card the kernel removed, as drm2d finds at the first
    /// ioctl that answers ENODEV, which closes its file, so a card added
    /// next can take its number. udev's report of the removal may come
    /// later, or early in boot never. Its displays' removals, drained or
    /// not, go with it.
    fn let_go_of_gone_cards(&mut self) {
        let gone: Vec<PathBuf> = self
            .videos
            .iter()
            .filter(|(_, v)| v.gone())
            .map(|(node, _)| node.clone())
            .collect();
        for node in gone {
            log::info!(
                "{}: the kernel removed the card, and vtium released the card",
                node.display()
            );
            self.remove_video(&node);
        }
    }

    /// Removes a video device and its displays, or its wait for the seat
    /// manager, and lets the VT close it.
    pub fn remove_video(&mut self, node: &Path) {
        self.waiting_videos.remove(node);
        self.videos.remove(node);
        self.vt.close_device(node);
        self.cards.remove(node);
        self.lit.retain(|(n, _), _| n != node);
        self.unlit.retain(|(n, _), _| n != node);
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
            logind_seat: self.vt.logind_seat(),
            truecolor: self.truecolor(h),
        }
    }

    /// Whether a program that starts now in head `h` is told
    /// `COLORTERM=truecolor`, as [`Head::truecolor`] decides from the
    /// head's displays. With no display set up, the head counts the depth
    /// its `display-depth` names, or color at `auto`.
    ///
    /// References:
    /// - ADR-1301 D10: the displays that count.
    fn truecolor(&self, h: usize) -> bool {
        let unlit = depth(&self.head_config(h), None);
        self.heads.get(h).is_none_or(|head| head.truecolor(unlit))
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
        let truecolor = spec.truecolor();
        let command = spec.command().ok_or("no command to run")?;
        let erase = if self.config.switch(&keys::BACKSPACE_DELETE) {
            Erase::Delete
        } else {
            Erase::Backspace
        };
        let Some(size) = self.heads.get(h).and_then(|head| head.console_pty_size(id)) else {
            return Ok(());
        };
        // The login picture is placed before the program starts, so a
        // banner is in place before its first line.
        //
        // References:
        // - ADR-1100 D17, "When vtium places the picture": one placement
        //   for each login prompt.
        #[cfg(feature = "images")]
        if let Some(head) = self.heads.get_mut(h) {
            head.place_login_image(id);
        }
        let spawned = if self.vt.logind_seat() {
            // ADR-1400 D3, its note on each login's own session: the child
            // waits before its exec until systemd has moved it into a scope
            // of its own, outside vtium's logind session.
            let place = match self.vt.number() {
                Some(n) => format!("tty{n}"),
                None => self.vt.seat().to_owned(),
            };
            let scope = &mut self.login_scope;
            Pty::spawn_gated(command, size, erase, |pid| match scope(&place, pid) {
                Ok(unit) => log::debug!(target: "login", "{}", in_scope(&place, &unit)),
                Err(why) => log::warn!(target: "login", "{}", no_scope(&place, &why)),
            })
        } else {
            Pty::spawn(command, size, erase)
        };
        let mut pty = spawned.map_err(|e| format!("cannot start the session: {e}"))?;
        self.watch(h, id, &mut pty)
            .map_err(|e| format!("cannot watch the session's pty: {e}"))?;
        let pty_id = self.next_pty_id();
        if let Some(console) = self.heads.get_mut(h).and_then(|head| head.console_mut(id)) {
            console.pty_id = Some(pty_id);
            console.pty = Some(pty);
        }
        if let Some(head) = self.heads.get_mut(h) {
            head.told_truecolor(id, truecolor);
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
        self.choose_font_adding(h, None);
    }

    /// [`App::choose_font`] while a display of size `adding` is added to
    /// head `h`, before the display has a view, so that the choice counts
    /// its size.
    fn choose_font_adding(&mut self, h: usize, adding: Option<(u32, u32)>) {
        if self.heads.get(h).is_none_or(|head| head.font.is_some()) {
            return;
        }
        let (chosen, made) = self.configured_font(h, adding);
        if let Some(head) = self.heads.get_mut(h) {
            head.font = Some(chosen);
            head.made = made;
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
    /// frit does (ADR-1200 D7, ADR-1403 D5). With it comes the frit made
    /// on the fly, where no frit bears the name, which OSC 50 switches
    /// back to by that name ([`font::by_name`]). With no font named, a head
    /// on a small display, its views' sizes and `adding` among them, draws
    /// the built-in font in place of the default set, but only at the
    /// automatic height, where the head's rows are 8 pixels high. A
    /// configured `font-size`, or a cell the head's look states, sets the
    /// height itself, and the default set draws there.
    ///
    /// References:
    /// - ADR-1204 D2: the default set, and the built-in font's 8×8 strike
    ///   where the small-display rule picks 8-pixel rows.
    fn configured_font(&self, h: usize, adding: Option<(u32, u32)>) -> (Chosen, Option<FritText>) {
        let Some(head) = self.heads.get(h) else {
            return (Chosen::BUILTINS, None);
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
        let sizes = head.views.values().map(|v| v.size).chain(adding);
        let automatic = !head.sized() && look.is_none();
        let Some(wanted) = wanted.filter(|w| {
            w.came != font::Came::Default || !automatic || font::default_set_fits(sizes)
        }) else {
            return (Chosen::builtins(look), None);
        };
        if let Some(builtin) = font::builtin_named(&wanted.name, look) {
            return (builtin, None);
        }
        // The frit's cell is read as text before the first frame; its fonts
        // are the helper's to parse (#145).
        let on_the_fly = font::frit(&s.frits, &wanted.name).is_none();
        let frit = match font::frit_named(s, &wanted) {
            Ok(frit) => frit,
            // the one line a frit vtium does not read gets at the start: the
            // start's warnings leave it to this read
            Err(refusal) => {
                log::warn!("{refusal} vtium draws its built-in font instead.");
                return (Chosen::builtins(look), None);
            }
        };
        let made = on_the_fly.then(|| frit.clone());
        match Chosen::frit(frit, head.sized(), look).map(|chosen| (chosen, made)) {
            Ok((chosen, made)) => {
                log::info!(
                    "the font is the frit {}, drawn in the built-in font until its glyphs are generated",
                    chosen.name(0, &self.locale)
                );
                (chosen, made)
            }
            Err(why) => {
                log::warn!(
                    "the font {} is not drawn ({why}): vtium draws its built-in font",
                    wanted.name
                );
                (Chosen::builtins(look), None)
            }
        }
    }

    /// A frit that states no cell takes the cell its first source gives at
    /// `height`, the height head `h` draws at, which the helper is asked
    /// for and waited for at most [`helper::OWN_CELL_MS`] (ADR-1200 D17):
    /// a bitmap source's own cell, where no
    /// `font-size` is configured, so a bitmap font draws at its own size
    /// (the operator, 2026-10-28), or an outline source's cell at
    /// `height` (the maintainer, 2026-11-01). Each answer is kept for the
    /// frit, by the height asked at, for every head drawing it. A refusal,
    /// or no answer in time, leaves the display's cell, or for an outline
    /// frit the built-in font's width, with one line saying why; an answer
    /// that comes after the wait is taken by [`App::late_cell`]. A frit
    /// whose first source is a bitmap font with strikes asks at each
    /// height, as an outline frit does, since each height can draw another
    /// strike, and its answers hold at the height asked at alone.
    ///
    /// References:
    /// - ADR-1204 D2: the strike for the automatic height, for a configured
    ///   font size and for each zoom step.
    fn own_cell(&mut self, h: usize, height: u32) {
        self.own_cell_within(h, height, Duration::from_millis(helper::OWN_CELL_MS));
    }

    /// [`App::own_cell`], waiting at most `wait` for the helper's answer.
    fn own_cell_within(&mut self, h: usize, height: u32, wait: Duration) {
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
        let struck = head.chosen().struck();
        let Some(name) = head.frit().map(str::to_owned) else {
            return;
        };
        let known = self.frits.get(&name).and_then(|f| {
            if struck {
                f.own.get(&height).cloned()
            } else {
                f.answer_at(height, outline)
            }
        });
        let own = match known {
            Some(own) => own,
            None => {
                let clock = Rc::clone(&self.clock);
                let Some(helper) = self.helper(&name) else {
                    return;
                };
                let own = helper.own_cell(height, wait, &*clock);
                // A font set with strikes, like an outline one, draws a
                // cell at each height, as wide as the built-in font where
                // the helper gives none.
                match (&own, outline || struck) {
                    (Ok(helper::OwnCell::Bitmap(cell)), _) if struck => log::info!(
                        "the font set {name} draws its {}x{} strike at {height} pixels",
                        cell.width,
                        cell.height
                    ),
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
            font.set_own_cell_at(answer, height, sized);
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

    /// Drops each frit no head draws any more, and with it its helper. The
    /// record of what the frit's helpers used up stays in `failures`.
    fn prune(&mut self) {
        let used = ordered::set(self.heads.iter().filter_map(Head::frit).map(str::to_owned));
        self.frits.retain(|name, _| used.contains(name));
    }

    /// The name of the font set the shown head draws, as OSC 50's query
    /// reports it.
    pub fn font_name(&self) -> &str {
        self.shown().map_or("unifont", |head| {
            head.chosen().name(head.font_size, &self.locale)
        })
    }

    /// OSC 50 asked head `h` for the font `name`. A frit, a built-in font,
    /// or the path of a font that root installed and trusts switches the
    /// head's live console as a zoom does, on a new grid. A frit, the one
    /// vtium makes for such a path included, comes with the helper and the
    /// store that every head drawing it shares. Anything else leaves the
    /// font as it was. The first refusal since the last switch is logged
    /// with its reason and the rest are counted, so a program's flood of
    /// requests logs one line.
    ///
    /// References:
    /// - ADR-1200 D5: what OSC 50 switches to, and the four checks a font's path passes.
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
        let new = font::by_name(
            &self.font_sources,
            name,
            sized,
            look,
            head.made.as_ref(),
            &font::font_folders(&self.config),
            &font::gate(),
        );
        let new = new.and_then(|new| {
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
        let wait = new.is_ok() && self.waits_for_cell(h);
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let current = head.chosen().name(head.font_size, &self.locale).to_owned();
        match new {
            Ok(()) => {
                head.switched = Some(head.console.id);
                if wait {
                    head.console.term.terminal.hold_replies();
                }
                let refused = std::mem::take(&mut head.font_refused);
                if refused > 1 {
                    log::info!(
                        "OSC 50 was refused {} more times before this switch",
                        refused.saturating_sub(1)
                    );
                }
                log::info!("OSC 50: the font is now {}", logged_name(&current));
            }
            Err(why) => {
                if head.font_refused == 0 {
                    log::warn!(
                        "OSC 50 asked for the font {}: {}; the font stays {}, and OSC 50's further refusals are counted until a switch",
                        logged_name(name),
                        why.trim_end_matches('.'),
                        logged_name(&current)
                    );
                }
                head.font_refused = head.font_refused.saturating_add(1);
            }
        }
    }

    /// What `render-timer` asks for: whether the console writes each
    /// display's figures to the log, and whether it draws them on screen.
    /// The figures describe the state of the system, which `osd = silent`
    /// hides from whoever is at the screen, so under `silent` they go to
    /// the log alone, or nowhere under `render-timer = osd`.
    ///
    /// References:
    /// - ADR-1401 D5, "The key `osd` and the render timer".
    fn render_timer(&self) -> (bool, bool) {
        let (log, screen) = match self.config.text(&keys::RENDER_TIMER) {
            Some("log") => (true, false),
            Some("osd") => (false, true),
            Some("both") => (true, true),
            _ => (false, false),
        };
        (log, screen && !self.osd.silent())
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
        let notice = display
            .connector
            .clone()
            .and_then(|c| self.notices.get(&(node.to_owned(), c)))
            .filter(|n| !n.parked);
        if let Some(n) = notice {
            return Some((n.head, display));
        }
        match place_display(&self.rules, &self.vt_heads, &display) {
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
            if self.hand_overs.given_up(node) {
                continue;
            }
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
        self.choose_font_adding(h, Some(size));
        // the display draws the pointer theme of its head's look
        let pointers = self.pointer_store(h);
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
            pointers,
        );
        if self.config.switch(&keys::SOFT_CURSOR) {
            view.pointer = crate::pointer::Pointer::software();
        }
        let key = (node.to_owned(), id);
        if let Some(head) = self.heads.get_mut(h) {
            head.views.insert(key.clone(), view);
        }
        #[cfg(feature = "filters")]
        self.give_colors(h, &key);
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
        if let Some(head) = self.heads.get_mut(h) {
            head.black_and_white_joins(&key, &name);
        }
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
            // nothing to lay out, but a switch still changed the font
            let locale = self.locale.as_str();
            if let Some(head) = self.heads.get_mut(h) {
                let name = head.chosen().name(head.font_size, locale).to_owned();
                head.name_font(&name);
            }
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
        // A console that the head shows while it waits for the cell of the
        // font set an OSC 50 switch chose would otherwise report the new
        // name with the cell the head keeps. So that console holds its
        // replies too, until the wait ends and `end_cell_waits` sends
        // them from the new cell. The console that made the switch holds
        // its replies already, and a second hold changes nothing.
        if self
            .heads
            .get(h)
            .is_some_and(|head| head.switched.is_some())
            && self.waits_for_cell(h)
            && let Some(head) = self.heads.get_mut(h)
        {
            head.console.term.terminal.hold_replies();
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
        head.name_font(&name);
        let cell = |head: &Head, at: u32| {
            head.face(at, locale).ok().map(|f| {
                let c = f.cell();
                (c.width, c.height)
            })
        };
        let turned = matches!(head.orientation, Orientation::Right | Orientation::Left);
        let Some(l) = layout::layout(&sizes, turned, head.asked, base, head.step, |at| {
            cell(head, at)
        }) else {
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

    /// Mode 1004's focus (ADR-1401 D5): the shown console of each head a
    /// keyboard delivers to has it, lit or not, so several hold it at
    /// once. It holds only while vtium's VT is shown, and only while the
    /// head is not wholly paused by a hand-over of its displays (D16). So
    /// a VT release, under a seat manager the seat's disable, takes it
    /// from every head, and setBackground from each head it wholly
    /// pauses. Each change goes to a program that asked for it.
    pub(super) fn report_focus(&mut self) {
        // A keyboard that could not be opened, or went at a wake-up, is
        // gone from the seat's input; with no seat input, as a test drives
        // the seat, a device counts as open.
        let input = self.input.as_ref();
        let open = |id: Option<DeviceId>| {
            input.is_none_or(|i| id.is_some_and(|id| i.disposition(id).is_some()))
        };
        // A release under way counts as the VT gone. The VT stays ours
        // until the turn lets it go, once the displays are asleep, and the
        // release reports the focus-out before that.
        let shown = self.vt.is_active() && self.releasing.is_none();
        let paused = self.paused_heads();
        for (h, head) in self.heads.iter_mut().enumerate() {
            let focused = shown
                && !paused.contains(&h)
                && self
                    .routes
                    .keyboard_reaches(&self.vt_heads, h, self.focus, open);
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
            input.set_pointer_max_of(
                &self.routes.devices_of(&self.vt_heads, h, self.focus),
                w,
                ht,
            );
        }
    }

    /// Checks display `key` of head `h` against the requirements of the
    /// head's vtlook, and hands the display's view the vtlook's color table
    /// when the display meets them, else the built-in vtlook. vtium runs
    /// the check when the display joins the head, at each modeset that may
    /// change the display's size, refresh or depth, and each time the head
    /// moves between the GPU path and the CPU path. A display that stops
    /// meeting the requirements draws the built-in vtlook, with the line
    /// that names the requirement it misses, and a display that meets them
    /// again draws the vtlook again, with no reread of the vtlook. vtium
    /// logs each new answer once, so a check that gives the same answer
    /// logs nothing.
    ///
    /// The display's depth is its drawing's: 1 bit, gray, or for a color
    /// display the bits a pixel of its buffer holds, so a 16-bit
    /// framebuffer counts as 16. Its density comes from `display-dpi`,
    /// else from the size the display reports, else vtium assumes 96.
    ///
    /// References:
    /// - ADR-1304 D7: when vtium checks a display against the requirements, and how it judges depth, size, density and the path.
    #[cfg(feature = "filters")]
    fn give_colors(&mut self, h: usize, key: &(PathBuf, DisplayId)) {
        use vtium_settings::vtlook::Depth as Needs;
        let Some(video) = self.videos.get(&key.0) else {
            return;
        };
        let name = video.name(key.1);
        let (levels, bits, reported) = (
            video.levels(key.1),
            video.buffer_bits(key.1),
            video.dpi(key.1),
        );
        let Some(size) = video
            .size(key.1)
            .or_else(|| Some(self.heads.get(h)?.views.get(key)?.size))
        else {
            return;
        };
        let depth = match depth(&self.head_config(h), levels) {
            Depth::One => Needs::One,
            Depth::Gray(_) => Needs::Gray,
            Depth::Color => match bits {
                Some(1) => Needs::One,
                Some(b) if b <= 16 => Needs::Sixteen,
                _ => Needs::TwentyFour,
            },
        };
        let turned = self
            .heads
            .get(h)
            .is_some_and(|head| matches!(head.orientation, Orientation::Right | Orientation::Left));
        let dpi = self
            .head_config(h)
            .number(&keys::DISPLAY_DPI)
            .map(|d| d as f64)
            .or_else(|| {
                let (across, down) = reported?;
                Some(if turned { across } else { down })
            })
            .unwrap_or(DPI_ASSUMED);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the clamp holds the rounded density within u32"
        )]
        let dpi = dpi.round().clamp(0.0, f64::from(u32::MAX)) as u32;
        let display = crate::head::look::Display {
            name: &name,
            size,
            depth,
            dpi,
            gpu: self.draws_on_gpu(h),
        };
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let (table, line) = head.vtlook.table_for(&display);
        let again = head.vtlook.met_again(&display);
        let Some(view) = head.views.get_mut(key) else {
            return;
        };
        view.set_colors(table);
        if view.set_unmet(line.clone()).is_some()
            && let Some(said) = line.or(again)
        {
            log::info!(target: "look", "{said}");
        }
    }

    /// Checks each display of head `h` against the requirements of the
    /// head's vtlook again, as [`App::give_colors`] does, after the head
    /// moved between the GPU path and the CPU path.
    #[cfg(feature = "filters")]
    fn give_colors_again(&mut self, h: usize) {
        let keys: Vec<_> = self
            .heads
            .get(h)
            .map(|head| head.views.keys().cloned().collect())
            .unwrap_or_default();
        for key in &keys {
            self.give_colors(h, key);
        }
    }

    /// Whether head `h` draws on the GPU path, as a vtlook's `gpu`
    /// requirement judges it: the path its render thread reported, once it
    /// has, and the path the head is set up for until then, so that a
    /// display that joins before the render thread starts is judged by
    /// the path it is about to draw on. A head whose render thread could
    /// not start draws on the CPU path.
    #[cfg(feature = "filters")]
    fn draws_on_gpu(&self, h: usize) -> bool {
        let Some(head) = self.heads.get(h) else {
            return false;
        };
        match &head.painter {
            Some(painter) if painter.known() => painter.path() == crate::painter::Path::Gpu,
            _ if head.painter_refused => false,
            _ => matches!(self.gpu_source(h), Ok(Some(_))),
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
            let record = Rc::clone(self.failures.entry(name.to_owned()).or_default());
            let launcher = self.launcher.clone();
            let started = match launcher.and_then(|l| self.helper_wake().map(|w| (l, w))) {
                Ok((launcher, wake)) => Ok(Helper::new(
                    launcher,
                    wake,
                    self.epoch,
                    frit.frit.clone(),
                    self.console_font.clone(),
                    std::sync::Arc::clone(&frit.generated),
                    crate::head::frit_store(
                        crate::deferred::store_limit(physical_memory()),
                        self.heads.len(),
                    ),
                )
                .remembering(record)),
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

    /// A wake-up into the loop for a helper's answers, which the helper's
    /// reader calls after each frame.
    fn helper_wake(&self) -> Result<std::sync::Arc<dyn Fn() + Send + Sync>, String> {
        let (ping, source) = vtium_platform::calloop::ping::make_ping().map_err(|e| {
            format!("vtium cannot make a wake-up for the font helper's answers: {e}")
        })?;
        self.handle
            .insert_source(source, |(), (), _: &mut App| {})
            .map_err(|e| format!("vtium cannot watch for the font helper's answers: {e}"))?;
        let ping = std::sync::Mutex::new(ping);
        Ok(std::sync::Arc::new(move || {
            if let Ok(p) = ping.lock() {
                p.ping();
            }
        }))
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
        let mut blamed = Vec::new();
        for (name, frit) in &mut self.frits {
            let Some(helper) = &mut frit.helper else {
                continue;
            };
            let answers = helper.poll(now);
            if helper.take_blamed() {
                blamed.push(name.clone());
            }
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
        for name in blamed {
            self.ask_cells_again(&name, now);
        }
        for (name, height, answer) in late {
            self.late_cell(&name, height, answer);
        }
        self.end_cell_waits();
    }

    /// Whether head `h` draws a font set whose cell at the head's height
    /// the font helper has yet to give: the helper gave no answer in time,
    /// it still runs or starts again, and no later answer has come. The
    /// head draws in the cell it kept until the answer comes.
    fn waits_for_cell(&self, h: usize) -> bool {
        let Some(head) = self.heads.get(h) else {
            return false;
        };
        let at = draw_height(head, self.dpi(h), &self.locale);
        let wants = head
            .font
            .as_ref()
            .is_some_and(|f| f.wants_own_cell(at, head.sized()));
        let Some(frit) = head.frit().and_then(|name| self.frits.get(name)) else {
            return false;
        };
        if !wants || frit.helper.as_ref().is_none_or(|h| h.failed().is_some()) {
            return false;
        }
        let known = if head.chosen().struck() {
            frit.own.get(&at).cloned()
        } else {
            frit.answer_at(at, head.chosen().outline())
        };
        known.is_some_and(|k| k.is_err_and(|no| no.late))
    }

    /// Sends the shown console of each head the replies it held since its
    /// font switch, once the head waits no more for the new font set's
    /// cell: the helper's late answer laid the head out in the new cell,
    /// or the helper failed and the head keeps its cell. A zoom or a new
    /// font that leaves nothing to wait for ends the wait too. The terminal
    /// answers the held size reports from the cell and the grid the head
    /// has now, so once the program has the new font set's name, every
    /// size it is told is the new cell's. The state alone ends the wait,
    /// never a timer.
    ///
    /// A hidden console keeps its own grid and cell until it is shown, so
    /// the late cell does not lay it out. Its hold therefore goes on until
    /// the head shows it. Every switch of the console shown lays the head
    /// out again before this turn's end, so the console sends its replies
    /// from the new cell.
    ///
    /// References:
    /// - ADR-1401 D1: a hidden console keeps its own grid and cell.
    fn end_cell_waits(&mut self) {
        for h in 0..self.heads.len() {
            let held = self
                .heads
                .get(h)
                .is_some_and(|head| head.console.term.terminal.replies_held());
            if !held || self.waits_for_cell(h) {
                continue;
            }
            if let Some(head) = self.heads.get_mut(h) {
                head.console.send_held_replies();
            }
        }
    }

    /// Frit `name` left out a source that made its helper fail. That source
    /// may be the one that gave the frit's cell, so the console forgets
    /// every cell the helper answered and asks for each again without
    /// waiting. The late answers decide the cell ([`App::late_cell`]).
    ///
    /// References:
    /// - ADR-1200 D14: a font that makes the helper fail is left out, and
    ///   the font set's other fonts go on drawing.
    /// - ADR-1200 D17: an answer that comes after the wait takes the place
    ///   of the missing one.
    fn ask_cells_again(&mut self, name: &str, now: Instant) {
        let Some(frit) = self.frits.get_mut(name) else {
            return;
        };
        let heights: Vec<u32> = frit.own.keys().copied().collect();
        frit.own.clear();
        if let Some(helper) = &mut frit.helper {
            helper.ask_cells_again(&heights, now);
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
        // A frit with strikes holds each answer at its own height, as an
        // outline frit does.
        let struck = self
            .heads
            .iter()
            .any(|head| head.frit() == Some(name) && head.chosen().struck());
        let Some(frit) = self.frits.get_mut(name) else {
            return;
        };
        let outline = match &answer {
            Ok(helper::OwnCell::Bitmap(_)) => Some(struck),
            Ok(helper::OwnCell::Outline(_)) => Some(true),
            Err(_) => None,
        };
        let held = match outline {
            Some(outline) if !struck => frit.answer_at(height, outline),
            _ => frit.own.get(&height).cloned(),
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
            Err(_) => {}
        }
        let refused = answer.as_ref().err().map(ToString::to_string);
        frit.own.insert(height, answer);
        if let Some(why) = refused {
            return self.late_refusal(name, height, &why);
        }
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

    /// The helper refused frit `name`'s cell at `height` after the wait,
    /// for `why`. A head that draws in a cell the frit gave it earlier at
    /// that height stops drawing in it: the frit is chosen afresh for the
    /// head and laid out again, so the head draws in the display's cell, or
    /// as wide as the built-in font, as after a refusal in time. This
    /// happens when vtium left out the font that gave the cell, after the
    /// font made the font helper fail, and asked for the cell again. A
    /// head that drew in no such cell keeps the cell it draws in.
    ///
    /// References:
    /// - ADR-1200 D14: a font that makes the helper fail is left out, and
    ///   the font set's other fonts go on drawing.
    /// - ADR-1204 D5: a missing text font leaves the cell to the
    ///   configuration.
    fn late_refusal(&mut self, name: &str, height: u32, why: &str) {
        let mut drawing = Vec::new();
        for h in 0..self.heads.len() {
            let dpi = self.dpi(h);
            let Some(head) = self.heads.get(h) else {
                continue;
            };
            let at = draw_height(head, dpi, &self.locale);
            let took = match &head.font {
                Some(Chosen::Frit {
                    name: drawn,
                    own,
                    heights,
                    ..
                }) if drawn == name => own.is_some() || (at == height && heights.contains_key(&at)),
                _ => false,
            };
            if took {
                drawing.push(h);
            }
        }
        if drawing.is_empty() {
            log::info!(
                "the helper refused the frit {name}'s own cell after the wait ({why}): it keeps the cell it draws in"
            );
            return;
        }
        log::info!(
            "the helper refused the frit {name}'s own cell after the wait ({why}): it no longer draws in the cell the frit gave earlier, from the next frame"
        );
        for h in drawing {
            let Some(head) = self.heads.get_mut(h) else {
                continue;
            };
            let fresh = match &head.font {
                Some(Chosen::Frit { frit, .. }) => {
                    Chosen::frit(frit.clone(), head.sized(), head.look_cell()).ok()
                }
                _ => None,
            };
            let Some(fresh) = fresh else {
                continue;
            };
            head.font = Some(fresh);
            head.refont = true;
            self.share(h);
            self.relayout(h);
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
            // The cursor's first phase lasts the interval its program set,
            // so a phase longer than 500 ms is not cut short.
            let terminal = &head.console.term.terminal;
            let cursor = crate::blink::cursor_period(terminal);
            let blinks = terminal.cursor_blinks();
            for (timer, arm) in head.console.term.blink.activate(cursor, blinks) {
                match timer {
                    Timer::Text => head.text_due = Some(after(now, arm.first)),
                    Timer::Cursor => {
                        head.cursor_due = Some((after(now, arm.first), arm.every));
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
            head.console.term.blink.rapid_stop();
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
        // A card a hand-over gave up is the program's to power.
        for (node, video) in &mut self.videos {
            if !self.hand_overs.given_up(node) {
                power_down(video.as_mut());
            }
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
        self.wake_notices();
    }

    /// A key on this VT, a switch back to it or a wake from the blank
    /// starts each notice's full `osd-timeout` again (ADR-1401 D3), whether
    /// the notice is still showing or parked. The guards light a parked
    /// notice's display at their next run, and its time starts at the
    /// first turn it is on the display again.
    fn wake_notices(&mut self) {
        for n in self.notices.values_mut() {
            n.due = None;
            if std::mem::take(&mut n.parked) {
                self.guard_due = true;
            }
        }
    }

    /// Lights the notice of each display of `unused`, each with the head of
    /// other VTs it is configured for, if any, and lets go of each notice
    /// whose display has gone or now has a path (ADR-1401 D3). A parked
    /// notice stays parked. An announced display joins its notice's head; a
    /// display its card left dark is let go and asked for again, and joins
    /// it when it comes.
    fn show_notices(&mut self, unused: &[((PathBuf, String), Option<usize>)]) {
        // While the VT is another's, a card lists none of the displays it
        // left dark, a parked one included, so the guard can tell nothing
        // of the notices: each stays as it is, and the cards' wake-up runs
        // the guards again.
        if !self.vt.is_active() {
            return;
        }
        if self.osd.silent() {
            self.silence_notices(unused);
            return;
        }
        let gone: Vec<(PathBuf, String)> = self
            .notices
            .keys()
            .filter(|k| !unused.iter().any(|(u, _)| u == *k))
            .cloned()
            .collect();
        for key in gone {
            self.wanted.borrow_mut().remove(&key);
            let Some(n) = self.notices.remove(&key) else {
                continue;
            };
            // A display that now has a path leaves the notice's head for
            // the head its rules give it, lit as it was: it is never set
            // off on the way.
            let views: Vec<(PathBuf, DisplayId)> = self
                .heads
                .get_mut(n.head)
                .map(|h| {
                    let views: Vec<_> = h.views.keys().cloned().collect();
                    h.drop_views(|_| true);
                    views
                })
                .unwrap_or_default();
            self.spare_notices.push(n.head);
            for view in views {
                self.lit.remove(&view);
                self.add_display(&view.0, view.1);
            }
        }
        let vt = self.vt_heads.vt();
        // Which VTs vtium runs on, asked once for every notice that needs
        // it (ADR-1401 D3, "Enter on a head's own keyboard").
        let mut running = None;
        for (key, owner) in unused {
            let head = match self.notices.get(key) {
                Some(n) if n.parked => continue,
                Some(n) => {
                    let head = n.head;
                    let text = self.unused_text(vt, &key.1, *owner, &mut running);
                    if let Some(n) = self.notices.get_mut(key)
                        && n.text != text
                    {
                        if let Some(h) = self.heads.get_mut(head) {
                            h.set_notice(&text);
                        }
                        log::info!("display {}: {text}", key.1);
                        n.text = text;
                    }
                    head
                }
                None => {
                    let text = self.unused_text(vt, &key.1, *owner, &mut running);
                    let spare = self
                        .spare_notices
                        .pop()
                        .filter(|&h| self.heads.get(h).is_some_and(|h| h.views.is_empty()));
                    let head = match spare {
                        Some(h) => h,
                        None => {
                            let term = crate::head::terminal(
                                &self.config,
                                (0, 0),
                                0,
                                Rc::clone(&self.clock),
                            );
                            self.heads.push(Head::notice_screen(&self.config, term));
                            self.heads.len().saturating_sub(1)
                        }
                    };
                    if let Some(h) = self.heads.get_mut(head) {
                        h.set_notice(&text);
                    }
                    log::info!("display {}: {text}", key.1);
                    self.notices.insert(
                        key.clone(),
                        Unused {
                            head,
                            due: None,
                            parked: false,
                            text,
                        },
                    );
                    head
                }
            };
            if self.heads.get(head).is_some_and(|h| !h.views.is_empty()) {
                continue;
            }
            // an announced display the rules left dark joins its notice
            let announced = self
                .dark
                .iter()
                .find(|((node, _), d)| *node == key.0 && d.connector.as_ref() == Some(&key.1))
                .map(|(k, _)| k.clone());
            if let Some(k) = announced {
                self.dark.remove(&k);
                if let Some(video) = self.videos.get_mut(&k.0)
                    && let Err(e) = video.set_dpms(k.1, Dpms::On)
                {
                    log::warn!("cannot set display {} on: {e}", video.name(k.1));
                }
                self.add_display(&k.0, k.1);
                continue;
            }
            // a display its card left dark is asked for again
            self.wanted.borrow_mut().insert(key.clone());
            if let Some(video) = self.videos.get_mut(&key.0)
                && video.release_dark(&key.1)
                && let Err(e) = video.hotplug()
            {
                log::warn!("{}: hotplug: {e}", key.0.display());
            }
        }
    }

    /// Under `osd = silent`, a display of `unused`, which no path catches
    /// on this VT, shows no notice. vtium leaves it off, since the rules
    /// place it nowhere, and a key press or a switch back to this VT leaves
    /// it off too. The log names each display and what its notice would
    /// say, once while the display stays unused. Such a display ends the
    /// start's wait for a display: a display came, and vtium leaves it off
    /// on purpose, so vtium must not exit and hand the VT to a getty.
    ///
    /// References:
    /// - ADR-1401 D3, "What a display with no path shows".
    fn silence_notices(&mut self, unused: &[((PathBuf, String), Option<usize>)]) {
        let vt = self.vt_heads.vt();
        self.silenced
            .retain(|key| unused.iter().any(|(u, _)| u == key));
        for (key, owner) in unused {
            if !self.silenced.insert(key.clone()) {
                continue;
            }
            let named =
                owner
                    .and_then(|h| self.rules.heads.get(h))
                    .map(|head| crate::notice::Owner {
                        head: &head.name,
                        vts: &head.vts,
                    });
            let text = crate::notice::unused_display(vt, &key.1, named.as_ref());
            log::info!(
                "display {}: {text} Because osd = silent, vtium shows no notice and leaves the display off.",
                key.1
            );
            self.presented = true;
        }
    }

    /// Each notice on its display for `osd-timeout` since the first turn it
    /// was there is put away: vtium parks its display, so it sends no signal
    /// and holds no memory, or sets it off where the device cannot park it
    /// (ADR-1401 D3). Under `osd-timeout = never` a notice stays.
    fn notice_cycle(&mut self, now: Instant) {
        let Some(timeout) = self.osd.timeout else {
            return;
        };
        let mut due = Vec::new();
        for (key, n) in self.notices.iter_mut().filter(|(_, n)| !n.parked) {
            let Some(head) = self.heads.get(n.head) else {
                continue;
            };
            let Some(view) = head.views.keys().next() else {
                continue;
            };
            let at = *n.due.get_or_insert_with(|| after(now, timeout));
            if now >= at {
                due.push((key.clone(), view.clone()));
            }
        }
        for (key, view) in due {
            self.park_notice(&key, view);
        }
    }

    /// Puts away the notice of the display at `view`, named `key`: parked
    /// by its device, which reports it removed and keeps it dark until let
    /// go, or set off where the device cannot park a display.
    fn park_notice(&mut self, key: &(PathBuf, String), view: (PathBuf, DisplayId)) {
        let Some(n) = self.notices.get_mut(key) else {
            return;
        };
        n.parked = true;
        n.due = None;
        // The notices that [`App::notice`] added under the display's own
        // go with the display.
        if let Some(head) = self.heads.get_mut(n.head) {
            head.set_notice(&n.text);
        }
        self.wanted.borrow_mut().remove(key);
        let Some(video) = self.videos.get_mut(&view.0) else {
            return;
        };
        match video.park(view.1) {
            Ok(()) => log::info!(
                "display {}: the notice's time is up, so the display is off and its buffers freed until a key on this VT",
                key.1
            ),
            Err(e) => {
                if e.kind() != io::ErrorKind::Unsupported {
                    log::warn!("cannot park display {}: {e}", key.1);
                }
                if let Err(e) = video.set_dpms(view.1, Dpms::Off) {
                    log::warn!("cannot set display {} off: {e}", key.1);
                }
                let h = self.notices.get(key).map(|n| n.head);
                if let Some(head) = h.and_then(|h| self.heads.get_mut(h)) {
                    head.drop_views(|k| *k == view);
                }
                if let Some(display) = self.lit.remove(&view) {
                    self.dark.insert(view, display);
                }
            }
        }
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
                // A card a hand-over gave up stays asleep, and its end
                // wakes it whole.
                if self.hand_overs.given_up(node) {
                    continue;
                }
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
            self.wake_notices();
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
                // vtium's VT shown again ends every hand-over, and the
                // wake-up below takes their cards back (ADR-1401 D16).
                self.hand_over_vt_shown();
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
                // The focus-out goes now, before the turn lets the VT go
                // (ADR-1401 D5).
                self.report_focus();
            }
            VtEvent::Retaken if self.blank.blanked() => {
                // As a hotplug while blanked: the set-up waits for the
                // input that ends the blank, and the displays stay dark
                // (the operator's decision on vtium/vtium#21).
                self.retake_owed = true;
            }
            VtEvent::Retaken => {
                for (node, video) in &mut self.videos {
                    // A card a hand-over gave up stays the program's.
                    if self.hand_overs.given_up(node) {
                        continue;
                    }
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
                self.unlit.remove(&key);
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
            // A display the device cannot light beside the others is still
            // plugged in, so its head keeps it, and its sessions with it,
            // whatever `when-display-lost` says (ADR-1401 D8). It draws
            // nothing until the modeset that lights it says to redraw, and
            // until then the guards do not count it as lit (D9).
            Event::Unlit(id) => {
                let key = (node.to_owned(), id);
                if let Some(display) = self.lit.remove(&key) {
                    self.unlit.insert(key.clone(), display);
                }
                self.guard_lit.take_if(|(k, _)| *k == key);
                self.guard_due = true;
                // A device names a display it cannot name "?".
                let name = self
                    .videos
                    .get(node)
                    .map(|video| video.name(id))
                    .filter(|name| !name.is_empty() && name != "?")
                    .map_or_else(
                        || "an unnamed display".to_owned(),
                        |name| format!("display {name}"),
                    );
                log::info!(
                    "{name} is dark but still connected, so its head keeps its sessions until the display lights"
                );
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
                // A display left unlit is lit again.
                if let Some(display) = self.unlit.remove(&key) {
                    self.lit.insert(key.clone(), display);
                    self.guard_due = true;
                }
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
                // The modeset may have given the display a size, refresh or
                // depth that the head's vtlook judges differently.
                #[cfg(feature = "filters")]
                if let Some(h) = self.head_of(node, id) {
                    self.give_colors(h, &key);
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
                        // A key's window on the display closes (ADR-1401
                        // D2).
                        v.flipped(&head.console.term);
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
            Event::Awake => {
                // The card lists again the displays it left dark, which
                // the notices read (ADR-1401 D3).
                if !self.notices.is_empty() {
                    self.guard_due = true;
                }
                self.user_input(now);
            }
            Event::Asleep => {
                if let Some(r) = &mut self.releasing {
                    r.remove(node);
                }
            }
            Event::WakeFailed => {
                self.retaking.remove(node);
                // A DRM card goes on trying once a second while its VT is
                // shown; a framebuffer waits for the next key.
                let slowly = self
                    .videos
                    .get(node)
                    .is_some_and(|video| video.backend() == "drm2d");
                if slowly {
                    log::warn!(
                        "{}: the displays did not wake. vtium tries again once a second while this VT is shown, and at once at the next key press",
                        node.display()
                    );
                } else {
                    log::warn!(
                        "{}: the displays did not wake. vtium tries again at the next key press",
                        node.display()
                    );
                }
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
        let placement = self.vt_heads.placement(&self.rules, &lit, &[], None);
        self.focus = placement.focus.and_then(|p| self.vt_heads.head(p));
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
        let placement = self
            .vt_heads
            .placement(&self.rules, &devices, &rescued, self.focus);
        // Under open, each display no path catches shows its own notice
        // (ADR-1401 D3).
        let on_screens = displays.len().saturating_add(held.len());
        let unused: Vec<((PathBuf, String), Option<usize>)> = displays
            .iter()
            .map(|(node, _)| node)
            .chain(held.iter().map(|(node, _, _)| node))
            .zip(devices.iter().zip(&placement.placed))
            .take(on_screens)
            .filter_map(|(node, (d, p))| match p.place {
                vtium_settings::Place::Unused(owner) => {
                    Some(((node.clone(), d.connector.clone()?), owner))
                }
                _ => None,
            })
            .collect();
        self.show_notices(&unused);
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
                    place => self.vt_heads.head(place),
                };
                Some((key.clone(), h?))
            })
            .filter(|&(_, h)| Some(h) != screen || self.screen_shows());
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
            let to = placement.focus.and_then(|p| self.vt_heads.head(p));
            if let Some(to) = to.filter(|&h| Some(h) != self.focus) {
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

    /// Whether the screen the display guard lights for a notice alone has
    /// anything to show. Under `osd = silent` the file's console line does
    /// not show, so the screen shows only the two states in which vtium can
    /// serve nothing: the panic screen and the notice of a crash loop that
    /// stopped every head. They show at every level, since the person at
    /// the console has no other way to learn why nothing works.
    ///
    /// References:
    /// - ADR-1401 D9: what the display guard lights under `osd = silent`.
    /// - ADR-1401 D5, "What every level shows".
    fn screen_shows(&self) -> bool {
        !self.osd.silent() || self.stopped || self.report.is_some()
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
        let several = self.heads.iter().filter(|h| !h.notice_only).count() > 1;
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
                // A screen that shows a notice alone is not a terminal: it
                // has no session to hang up or keep, and no OSD.
                (Attached::Yes, Attached::Gone) | (Attached::Gone, Attached::Yes)
                    if head.notice_only =>
                {
                    changed = true;
                }
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

    /// The device events of one turn. Each card scans once for all the
    /// hotplugs it had in the turn. Between a card's display work and the
    /// next, the loop reads a request for the VT, and once one comes it
    /// lets the VT go in this turn and skips the scans that remain. The
    /// VT comes back to vtium only through an acquire, whose wake-up scans
    /// every card, so a skipped scan needs no record (ADR-1401 D2, "A VT
    /// release while display work runs").
    fn device_events(&mut self, events: Vec<DeviceEvent>, now: Instant) {
        let mut hotplugs: Vec<PathBuf> = Vec::new();
        for e in events {
            match e {
                DeviceEvent::Hotplug { node, .. } => {
                    if !hotplugs.contains(&node) {
                        hotplugs.push(node);
                    }
                }
                DeviceEvent::Added(dev) if dev.kind != DeviceKind::Input => {
                    // A card added wakes up, and so reads its connectors,
                    // only while the VT is vtium's.
                    self.release_now(now);
                    self.device_event(DeviceEvent::Added(dev));
                }
                e => self.device_event(e),
            }
        }
        for node in hotplugs {
            if self.release_now(now) {
                continue;
            }
            if let Some(video) = self.videos.get_mut(&node)
                && let Err(e) = video.hotplug()
            {
                log::warn!("{}: hotplug: {e}", node.display());
            }
        }
        self.release_now(now);
    }

    /// Reads a request for the VT that waits, as a card's display work may
    /// have stopped for, and handles it now, so the turn lets the VT go.
    /// Returns whether the VT is being released.
    fn release_now(&mut self, now: Instant) -> bool {
        let asked = self.releasing.is_none()
            && self.vt.is_active()
            && self.vt.release_check().is_some_and(|asked| asked());
        if asked {
            self.vt.take_release();
            let events: Vec<VtEvent> = self.vt.events().collect();
            for e in events {
                self.vt_event(e, now);
            }
        }
        self.releasing.is_some()
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
                    self.add_video_device(&dev.node, kind, card);
                }
            },
            DeviceEvent::Removed { kind, node } => match kind {
                DeviceKind::Input => self.remove_input(&node),
                DeviceKind::Drm | DeviceKind::Fbdev => {
                    if self.videos.contains_key(&node) {
                        let what = if kind == DeviceKind::Drm {
                            "card"
                        } else {
                            "framebuffer"
                        };
                        log::info!(
                            "{}: udev reports the {what} removed, and vtium released the {what}",
                            node.display()
                        );
                    }
                    self.remove_video(&node);
                }
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

    /// Opens a video device udev reported. One the seat manager is not
    /// ready to give waits, and opens once the VT names it settled
    /// ([`App::settled_videos`]).
    fn add_video_device(&mut self, node: &Path, kind: DeviceKind, card: Card) {
        match open_video(self, node, kind, card.clone()) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                log::debug!("{} waits for the seat manager", node.display());
                self.waiting_videos.insert(node.to_owned(), (kind, card));
            }
            Err(e) => log::warn!("cannot open {}: {e}", node.display()),
        }
    }

    /// The waiting video devices whose open the VT has settled, opened now
    /// or reported as failed.
    fn settled_videos(&mut self) {
        for node in self.vt.settled() {
            if let Some((kind, card)) = self.waiting_videos.remove(&node) {
                self.add_video_device(&node, kind, card);
            }
        }
    }

    /// An input device udev reported, placed by the first rule matching
    /// `identity` (ADR-1401 D3): opened to deliver, opened for the rescue
    /// key alone, or never opened.
    pub fn add_input(&mut self, node: &Path, identity: vtium_settings::Identity) {
        self.guard_due = true;
        let disposition = self
            .routes
            .add(&self.rules, &self.vt_heads, node, &identity);
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
        // go here, and so are the keys it held (ADR-1005 D7).
        self.rehover();
        self.let_go_of_held_keys();
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
                    .mods_of(&self.routes.devices_of(&self.vt_heads, h, self.focus))
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
        self.let_go_of_held_keys();
    }

    /// A key press or pointer event from the input device at `node`.
    pub fn device_input(&mut self, node: &Path, event: InputEvent, now: Instant) {
        self.deliver(Some(node), event, now);
        self.let_go_of_held_keys();
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
        self.let_go_of_held_keys();
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
        // A release goes to the console that received its press, and
        // nowhere else (ADR-1005 D7).
        if let InputEvent::Release(key) = &event {
            self.release(node, key);
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
        // A key of a keyboard that stays with a head on another VT reaches
        // no session: its VT keys switch the VT as every keyboard's do
        // (D3, "Who switches VTs"), Enter may switch to that head's VT
        // (D3, "Enter on a head's own keyboard"), and every key wakes the
        // notices.
        if let InputEvent::Key(key) = &event
            && let Some(head) = self.routes.elsewhere(node)
        {
            self.activity(now);
            let vt_keys = self.routes.vt_keys(&self.rules, node);
            if !(vt_keys && self.vt.key(key)) {
                self.key_elsewhere(head, node, key);
            }
            return;
        }
        // The head the device types into: its rule's, or the active head's
        // for a roaming device (D5). Whether its VT keys switch the VT is
        // the policy's and its own head's to say, wherever that head runs
        // (D3, "Who switches VTs").
        let asked = self.routes.head(&self.vt_heads, node, self.focus);
        let from = KeyFrom {
            vt_keys: self.routes.vt_keys(&self.rules, node),
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
                // ends its notices. An Esc that dismisses the kernel's
                // messages stays with vtium (ADR-1401 D15).
                self.focus_settled |= h.is_some();
                // A head a program holds the displays of keeps its notices
                // (ADR-1401 D16).
                let shown = h.filter(|&h| !self.wholly_paused(h));
                let kept = shown
                    .and_then(|h| self.heads.get_mut(h))
                    .is_some_and(|head| {
                        let dismissed = head.has_kernel() && dismisses(&key);
                        head.end_notices();
                        dismissed
                    });
                self.activity(now);
                if !kept {
                    self.key(h, node, &key, from, now);
                }
            }
            InputEvent::Pointer(p) => {
                let mods = match (&self.input, h) {
                    (Some(input), Some(h)) => {
                        input.mods_of(&self.routes.devices_of(&self.vt_heads, h, self.focus))
                    }
                    _ => InputMods::empty(),
                };
                self.pointer_to(h, p, mods, now);
            }
            // A keyboard's modifiers changed. Only the overlay follows
            // them, read from each head's own keyboards, and a modifier is
            // not the user's activity.
            InputEvent::Mods(_) => self.rehover(),
            InputEvent::Release(_) => {}
        }
    }

    /// The release of a key of the keyboard at `node`: sent to the console
    /// that received its press, through its encoder, which sends nothing
    /// for it unless the program asked for releases. A key whose press
    /// reached no encoder, as a key vtium took, is released nowhere
    /// (ADR-1005 D7).
    fn release(&mut self, node: Option<&Path>, key: &KeyEvent) {
        let Some(to) = self.held.released(node, key.keycode) else {
            return;
        };
        let Some(release) = term_key(key, TermKeyEvent::Release) else {
            return;
        };
        self.send_release(to, &release);
    }

    /// `release` to the console `to`, shown or hidden.
    fn send_release(&mut self, to: held::Console, release: &Key) {
        let console = self
            .heads
            .get_mut(to.head)
            .and_then(|head| head.console_mut(to.id));
        if let Some(console) = console
            && console.term.terminal.key(release)
        {
            console.send_replies();
        }
    }

    /// Each console that stopped receiving a keyboard while one of its
    /// keys was held gets a release for each such key (ADR-1005 D7): a
    /// console receives a keyboard while vtium's terminals are awake on
    /// the VT, the keyboard is still routed to the console's head on this
    /// VT, and the console is the one its head shows. So a VT switch, a
    /// console chord, a focus move and an unplugged keyboard each let go
    /// of the keys held, as the next call after them finds.
    fn let_go_of_held_keys(&mut self) {
        let awake = self.awake && self.vt.is_active();
        let paused = self.paused_heads();
        let (heads, routes, vt_heads, focus) =
            (&self.heads, &self.routes, &self.vt_heads, self.focus);
        let owed = self.held.leave(|node, to| {
            awake
                && !paused.contains(&to.head)
                && node.is_none_or(|n| routes.knows(n))
                && routes.elsewhere(node).is_none()
                && routes.head(vt_heads, node, focus) == Some(to.head)
                && heads.get(to.head).is_some_and(|h| h.console.id == to.id)
        });
        for o in owed {
            self.send_release(o.to, &o.release);
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
        let line = self.rescue_line(node);
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

    /// What vtium says when the rescue key moved the keyboard at `node`
    /// (ADR-1401 D9): the head it types into now; on a VT where no head
    /// runs, that no terminal runs here and the VT keys that leave (D3);
    /// else that no head is active and the roaming chord that chooses one.
    fn rescue_line(&self, node: &Path) -> String {
        match self.focus.and_then(|h| self.heads.get(h)) {
            Some(head) => crate::notice::rescue(node.display(), head.name()),
            None if self.heads().is_empty() => crate::notice::rescue_no_terminal(node.display()),
            None => crate::notice::rescue_no_head(node.display(), &self.bindings),
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
        // While a program holds every display of the head, its pointer
        // events reach no console (ADR-1401 D16).
        let Some(h) = h.filter(|&h| !self.wholly_paused(h)) else {
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
    /// The VT's keys switch the VT only `from` a keyboard whose VT keys
    /// count (ADR-1401 D3, "Who switches VTs"); from any other, the press
    /// goes on down the key order to the session as an ordinary key, as a
    /// serial terminal sends it (D5). The roaming chord is taken only from a roaming
    /// keyboard; from a claimed one it reaches the head's session as a key.
    /// A key that reaches the encoder is recorded as held on the console
    /// shown, with the keyboard at `node` it came from, so its release
    /// follows it there; a repeat reaches the encoder only while its
    /// press's record stands, so a key vtium took keeps its repeats
    /// (ADR-1005 D7).
    fn key(
        &mut self,
        h: Option<usize>,
        node: Option<&Path>,
        key: &KeyEvent,
        from: KeyFrom,
        now: Instant,
    ) {
        if from.vt_keys && self.vt.key(key) || !self.awake {
            return;
        }
        let bits = key.mods.bits();
        let action = self.bindings.action(bits, &key.keysyms);
        if let Some(grab::Action::Power(power_key)) = action {
            // One action a press: a held chord's repeats would ask again,
            // and systemd takes seven Ctrl+Alt+Del in two seconds as an
            // immediate reboot.
            if key.repeat {
                return;
            }
            if let Some(child) = power::press(power_key, self.power.as_mut(), self.sleeping.get()) {
                let sleeping = Rc::clone(&self.sleeping);
                if let Err(e) = vtium_platform::power::watch(&self.handle, child, sleeping) {
                    log::error!("key-suspend: cannot watch the write to /sys/power/state: {e}");
                }
            }
            return;
        }
        // While a program holds every display of the head, the head takes
        // no key, console chord or grab: only the roaming chord, which
        // moves the roaming inputs to a head that draws (ADR-1401 D16).
        let paused = h.is_some_and(|h| self.wholly_paused(h));
        // The link hints key starts or ends link hints on the head the key's
        // device delivers to. While they are on, that head takes every key
        // after the power keys, one action a press, and none reaches the
        // program. Otherwise a key press ends vtium's note on the overlay
        // row and goes on as usual.
        //
        // References:
        // - ADR-1002, section 7: the keys read while link hints are on.
        if let Some(head) = h.filter(|_| !paused).and_then(|h| self.heads.get_mut(h)) {
            if action == Some(grab::Action::LinkHints) {
                if !key.repeat {
                    head.link_hints_key();
                }
                return;
            }
            if head.hints.is_some() {
                if !key.repeat {
                    let command = [InputMods::CTRL, InputMods::ALT, InputMods::LOGO]
                        .into_iter()
                        .any(|m| key.mods.contains(m));
                    head.hint_key(&key.keysyms, command, &mut self.selection);
                }
                return;
            }
            head.end_note();
        }
        if let Some(grab::Action::Head(chord)) = action
            && (from.roaming || chord.is_console())
            && !(paused && chord.is_console())
        {
            // One action a press, as the power keys.
            if !key.repeat {
                self.chord(h, chord, key.mods.contains(InputMods::CTRL), now);
            }
            return;
        }
        let Some(h) = h.filter(|_| !paused) else {
            return;
        };
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        // What the displays show before the key, so that the key draws a
        // frame only when it changed that (`KeyShown`).
        let before = KeyShown::now(head);
        // A press of Ctrl shows the hovered link's URI, and a key pressed
        // without Ctrl hides it; letting go of Ctrl sends no key.
        head.hover(key.mods.contains(InputMods::CTRL));
        head.console.term.terminal.screen_mut().clear_selection();
        if action == Some(grab::Action::Paste) {
            // One paste a press, however long the key repeats.
            if !key.repeat {
                head.paste(self.selection.as_deref());
                head.send_user_bytes();
            }
            head.draw_now = true;
            return;
        }
        // The scroll keys and the prompt jumps are the main screen's: on
        // the alternate screen they reach the program (ADR-1005 D7,
        // ADR-1002, "What the marks are for").
        let alternate = head.console.term.terminal.screen().alternate();
        let grab = match action {
            Some(grab::Action::Grab(g)) if !(g.main_screen_only() && alternate) => Some(g),
            _ => None,
        };
        if let Some(g) = grab {
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
                    // A turn swaps which of a display's two densities the
                    // head's vtlook judges, so each display is checked
                    // against the vtlook's requirements again, as at a mode
                    // change.
                    #[cfg(feature = "filters")]
                    self.give_colors_again(h);
                }
                // One reset a press, of the console shown alone.
                Grab::Reset if !key.repeat => head.reset_shown(),
                Grab::Reset => {}
            }
            return;
        }
        // The key has already cleared the selection and changed the hovered
        // link, so each way out of this block reaches the frame decision
        // below, even when vtium drops the key itself: a repeat whose press
        // it never delivered, or a key with no keysym.
        'send: {
            if key.repeat
                && (!head.console.term.terminal.auto_repeat()
                    || !self.held.delivered(node, key.keycode))
            {
                break 'send;
            }
            let event = if key.repeat {
                TermKeyEvent::Repeat
            } else {
                TermKeyEvent::Press
            };
            let Some(pressed) = term_key(key, event) else {
                break 'send;
            };
            if !key.repeat {
                let to = held::Console {
                    head: h,
                    id: head.console.id,
                };
                self.held.pressed(node, key.keycode, to, pressed);
            }
            if head.console.term.terminal.key(&pressed) {
                // The key opens a window on each display: the program's
                // output draws at once until the display's next flip
                // completes.
                //
                // References:
                // - ADR-1401 D2: the key's window and when it closes.
                head.console.term.key_sent();
                head.send_user_bytes();
                head.console.term.terminal.screen_mut().reset_view();
                if self.hide_while_typing {
                    head.hide_pointer();
                }
            }
            let rate = crate::blink::cursor_period(&head.console.term.terminal);
            let blinks = head.console.term.terminal.cursor_blinks();
            if let Some(arm) = head.console.term.blink.key(rate, blinks) {
                head.cursor_due = Some((after(now, arm.first), arm.every));
            }
        }
        head.draw_now |= before.changed(head);
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
                // setBackground and setForeground are judged against the
                // console shown when they came, before the chord shows
                // another (ADR-1401 D16). A setBackground that pauses the
                // head takes the chord with it, as a paused head takes no
                // chord.
                if let Some(h) = h {
                    self.display_requests(h);
                    if self.wholly_paused(h) {
                        return;
                    }
                }
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
                        // Its session ended, and with it the hand-over it
                        // began (ADR-1401 D16).
                        if let Some(id) = closed {
                            self.hand_over_session_ended(h, id);
                        }
                        // And the switch it made (ADR-1201 D11).
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
        // only to a lit head that can be the active head (D5), and never
        // to one a program holds every display of (ADR-1401 D16)
        let paused = self.paused_heads();
        let (heads, rules, vt_heads) = (self.heads(), &self.rules, &self.vt_heads);
        let lit = |h: usize| {
            heads.get(h).is_some_and(|head| !head.views.is_empty())
                && !vt_heads.isolated(rules, h)
                && !paused.contains(&h)
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
        // That the console did not open is a notice of a problem. It shows
        // at every level of `osd` but `silent`, where the log line alone
        // holds it, and it goes as the guards' notices go.
        //
        // References:
        // - ADR-1401 D5: the notice of a console that did not open.
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
        if self.heads.get(h).is_some_and(|head| head.chosen().struck()) {
            self.zoom_struck(h, dir);
            return;
        }
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let chosen = head.chosen();
        let Some(size) = chosen.zoom(head.font_size, dir, &self.locale, dpi) else {
            if dir == Zoom::Out {
                let smallest = chosen.smallest(&self.locale);
                let name = chosen.name(smallest, &self.locale).to_owned();
                self.zoom_stops(h, smallest, None, &name);
            }
            return;
        };
        // The zoom fills the displays again, in place of a grid a program
        // asked for.
        self.take_size(h, size, None);
    }

    /// A zoom of head `h` whose frit's first source is a bitmap font with
    /// strikes: the head draws at the next height in the zoom's direction
    /// whose cell differs, the height of a strike or a whole multiple of
    /// the tallest, which `strike_step` finds by asking the helper. A zoom
    /// out stops where the helper says that no strike is that short, with
    /// a line naming the smallest height, and where the helper could not
    /// answer, with a line saying why.
    ///
    /// References:
    /// - ADR-1204 D2: the cell of the tallest strike no taller than each
    ///   zoom step's height, and a whole multiple of the tallest strike's
    ///   cell above it.
    fn zoom_struck(&mut self, h: usize, dir: Zoom) {
        let Some(size) = self.heads.get(h).map(|head| head.font_size) else {
            return;
        };
        let end = after((self.clock)(), Duration::from_millis(helper::OWN_CELL_MS));
        if let Some(next) = self.strike_step(h, size, dir, end) {
            self.take_size(h, next, None);
            return;
        }
        if dir != Zoom::Out {
            return;
        }
        let Some(head) = self.heads.get(h) else {
            return;
        };
        // strike_step asks one pixel below `size` for a step out, so its
        // answer there says why there is no step.
        let answer = size.checked_sub(1).and_then(|at| {
            head.frit()
                .and_then(|name| self.frits.get(name))
                .and_then(|frit| frit.own.get(&at))
                .cloned()
        });
        let name = head.chosen().name(size, &self.locale).to_owned();
        match answer {
            Some(Err(no)) if no.from_frit => self.zoom_stops(h, size, None, &name),
            Some(Err(no)) if !no.late => self.zoom_stops(h, size, Some(&no.why), &name),
            _ => {}
        }
    }

    /// Logs that a zoom out of head `h` stops at `size`, the smallest
    /// height of the font set `name`, or with `why`, because the font
    /// helper could not answer. A head logs the stop once for each height,
    /// however often a key asks for it.
    fn zoom_stops(&mut self, h: usize, size: u32, why: Option<&str>, name: &str) {
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        if head.zoom_stop_logged == Some(size) {
            return;
        }
        head.zoom_stop_logged = Some(size);
        match why {
            None => log::info!(
                "head {}: the font set {name} draws no smaller than {size} pixels, so a zoom out stops there",
                head.name()
            ),
            Some(why) => log::warn!(
                "head {}: the font helper could not say which strike of the font set {name} draws below {size} pixels ({why}), so a zoom out stops there",
                head.name()
            ),
        }
    }

    /// Head `h` draws at the base height `size`, which ends `font-size`'s
    /// following of the displays (ADR-1200 D9), on the grid `asked`, or
    /// filling the displays with `None`: the zoom path, which a zoom and a
    /// program's XTWINOPS 8 take. A height with no font keeps the size in
    /// use. Returns whether the head draws at `size`.
    fn take_size(&mut self, h: usize, size: u32, asked: Option<(usize, usize)>) -> bool {
        let Some(head) = self.heads.get_mut(h) else {
            return false;
        };
        let outline = head.chosen().outline();
        let (old, auto, points, was) = (head.font_size, head.auto_size, head.points, head.asked);
        head.auto_size = false;
        head.points = None;
        head.font_size = size;
        head.asked = asked;
        // An outline frit asks the helper again at each zoom, a height
        // whose answer failed before included.
        if let Some(frit) = head
            .frit()
            .filter(|_| outline)
            .and_then(|n| self.frits.get_mut(n))
        {
            frit.own.retain(|_, answer| answer.is_ok());
        }
        if self.relayout(h) {
            return true;
        }
        if let Some(head) = self.heads.get_mut(h) {
            head.font_size = old;
            head.auto_size = auto;
            head.points = points;
            head.asked = was;
        }
        false
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
            // The session that sent setBackground ended: its hand-over
            // ends (ADR-1401 D16).
            self.hand_over_session_ended(h, id);
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

    /// The session in head `h`'s console `id` ended, or the console closed.
    /// vtium first reads the batch again, as at each new login. When the
    /// batch changed, the font helpers start again with the new active
    /// fonts, every stored glyph is dropped and asked for again, the palette
    /// files are read again, every head takes its new palette, and the
    /// pointer file may load again. Head `h` then chooses its vtlook again
    /// and resolves its font again when the batch changed since the head
    /// last caught up with it.
    ///
    /// Where an OSC 50 switch from that console, or from any console of a
    /// head left with one, changed the head's font, vtium returns the head
    /// to the font the configuration gives it, read again. Where the font
    /// returns or a zoom left the console off the configured size, the
    /// console returns to that size. Where the `font-size` read again turns
    /// the configured size on or off, vtium rebuilds the frit the head draws
    /// so that it drops or takes its own character cell. A configured font
    /// that draws nothing at that size leaves the font and the size as they
    /// were, with one line saying why.
    ///
    /// References:
    /// - ADR-1201 D11: what the end of a session returns, the font and the size.
    /// - ADR-1200 D16: a frit's own character cell follows whether `font-size` is configured.
    /// - ADR-1205 D4, its note of 2026-11-19: the batch is read again at each new login.
    fn return_font(&mut self, h: usize, id: u64) {
        // the batch changed since the head's font was resolved, so the
        // font is resolved again, as a newly active first source can set
        // its cell
        let rebatched = self.reread_batch(Some(h));
        let Some(due) = self.heads.get(h).map(|head| head.returns(id)) else {
            return;
        };
        if !due.size && !rebatched {
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
        // The frit made on the fly for the configured font, the head's
        // once the return holds.
        let mut made = None;
        let new = if due.font {
            head.switched = None;
            let (chosen, configured) = self.configured_font(h, None);
            made = Some(configured);
            Some(chosen)
        } else if head.sized() != was_sized || rebatched {
            // The frit drawn, with its own cell or without it as the
            // `font-size` read again now says, and with its own cell asked
            // for again under a changed batch.
            //
            // References:
            // - ADR-1200 D16: a frit's own character cell follows whether `font-size` is configured.
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
            if !same_font(head.chosen(), &new) || head.sized() != was_sized || rebatched {
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
        if let Some(made) = made
            && let Some(head) = self.heads.get_mut(h)
        {
            head.made = made;
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
            if let Some(head) = self.heads.get(h)
                && (due.font || head.sized() != was_sized)
            {
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
        let l = layout::layout(&sizes, turned, fresh.asked, base, face.size_step(), cell)?;
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
        self.unlit.clear();
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
        #[cfg_attr(not(test), expect(unused_mut, reason = "a test adds its events"))]
        let mut devices: Vec<DeviceEvent> = self
            .udev
            .as_mut()
            .map(|u| u.events().collect())
            .unwrap_or_default();
        #[cfg(test)]
        devices.splice(0..0, std::mem::take(&mut self.queued_devices));
        self.device_events(devices, now);
        self.settled_videos();
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
        self.let_go_of_gone_cards();
        if std::mem::take(&mut self.keyboard_check) && !self.routes.any_keyboard() {
            let line = crate::notice::NO_KEYBOARD;
            log::warn!("{line}");
            self.notice(line);
        }
        self.take_focus();
        self.guard();
        self.attachment(now);
        self.first_start(now);
        // After the VT's events and the guards, which may have taken the
        // keyboards from a console.
        self.let_go_of_held_keys();
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
        self.pointer_turn(now);
        if self.awake {
            self.draw(now);
        }
        // The draw's commits and the timers' power changes may find a card
        // removed after its events were drained.
        self.let_go_of_gone_cards();
        self.report(now);
        for console in self.heads.iter_mut().flat_map(Head::consoles_mut) {
            console.term.coalesce.turn_end();
            console.term.changed = false;
        }
        // A synchronized update's end stops the terminal only until the
        // displays take the finished frame. No display draws a hidden
        // console, nor any console while the VT is not shown or the
        // displays are blanked, so those take output again at once.
        for head in &mut self.heads {
            if !self.awake {
                head.console.term.unshown = false;
            }
            for console in &mut head.hidden {
                console.term.unshown = false;
            }
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
        // And XTWINOPS 8 only under `allow-window-ops`.
        let grid = self
            .heads
            .get_mut(h)
            .and_then(|head| head.console.term.terminal.take_grid_request());
        if let Some(request) = grid {
            self.request_grid(h, request);
        }
        // setBackground and setForeground under `allow-display-ops`, from
        // every console, before the exits, which end a hand-over too.
        self.display_requests(h);
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
                head.notices_timed_out();
            }
            head.kernel_turn(now);
            if head.osd_due.is_some_and(|d| d <= now) {
                head.osd_due = None;
                head.osd_shown = false;
            }
            head.brief_turn(now);
            if let Some(due) = head.text_due.filter(|&d| d <= now) {
                head.draw_now |= head.console.term.blink.fired(Timer::Text, awake);
                head.text_due = Some(after(due, crate::blink::TEXT_PERIOD));
            }
            // The cursor's interval can differ from the one its timer was
            // armed with: a program set another with `CSI 16 ; n ]`, or
            // another console of the head is shown now. The phase that is
            // running keeps its end, so a program that keeps changing the
            // interval never stops the blink, and a longer interval never
            // stretches that phase. A shorter interval ends the phase
            // within the new interval. Each later phase lasts the new
            // interval.
            let every = crate::blink::cursor_period(&head.console.term.terminal);
            // A program changes the cursor's style at any moment: a style
            // that blinks starts the timer from now, and a steady one
            // stops it with the cursor shown.
            let blinks = head.console.term.terminal.cursor_blinks();
            match (
                head.console.term.blink.cursor_runs(blinks, awake),
                head.cursor_due,
            ) {
                (true, None) => head.cursor_due = Some((after(now, every), every)),
                (false, Some(_)) => {
                    head.cursor_due = None;
                    head.draw_now = true;
                }
                _ => {}
            }
            if let Some((due, armed)) = head.cursor_due {
                if armed != every && due > now {
                    head.cursor_due = Some((due.min(after(now, every)), every));
                } else if due <= now {
                    head.draw_now |= head.console.term.blink.fired(Timer::Cursor, awake);
                    head.cursor_due = Some((after(due, every), every));
                }
            }
            let term = &mut head.console.term;
            // SGR 6's timer runs while a shown cell carries it
            if term.changed || head.draw_now {
                term.blink
                    .rapid_seen(crate::blink::rapid_on_screen(&term.terminal));
            }
            head.draw_now |= term.blink.rapid_turn(now, awake);
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
        self.notice_cycle(now);
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
        let draws = self.osd.draws_status();
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
            let shown = draws && (timeout.is_none() || head.osd_shown);
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
            // full line. [`Head::show_brief`] sets none under `off` or
            // `silent`.
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
        let gpu = self.gpu_source(h).unwrap_or_else(|why| {
            log::warn!("{why}");
            None
        });
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
                // The head draws on the CPU path, which a vtlook that
                // requires the GPU path does not accept.
                #[cfg(feature = "filters")]
                self.give_colors_again(h);
            }
        }
    }

    /// Where head `h`'s render thread makes its GL context: nowhere unless
    /// `hwaccel` is on in a build with `gpu`; a test's GPU where one is
    /// set; else the render node of the card that drives the head's first
    /// display. A card with no render node, such as a framebuffer, leaves
    /// the head on the CPU path, and the error is the line that says so,
    /// for the caller that starts the render thread to log once. The
    /// function itself logs nothing, so a check that only asks which path
    /// the head draws on can call it as often as it needs.
    ///
    /// References:
    /// - ADR-1303 D9: `hwaccel` and the `gpu` feature decide whether a head draws on the GPU path.
    fn gpu_source(&self, h: usize) -> Result<Option<crate::painter::GpuSource>, String> {
        if !self.config.switch(&keys::HWACCEL) || !cfg!(feature = "gpu") {
            return Ok(None);
        }
        if let Some(source) = &self.gpu_source {
            return Ok(Some(source.clone()));
        }
        let Some(head) = self.heads.get(h) else {
            return Ok(None);
        };
        let Some((node, _)) = head.views.keys().next() else {
            return Ok(None);
        };
        #[cfg(feature = "gpu")]
        return match vtium_platform::gpu::render_node(node) {
            Ok(render) => Ok(Some(crate::painter::GpuSource::RenderNode(render))),
            Err(e) => Err(format!(
                "head {}: {} has no render node ({e}): it draws on the CPU path",
                head.name(),
                node.display()
            )),
        };
        #[cfg(not(feature = "gpu"))]
        {
            let _ = node;
            Ok(None)
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
        let mut moved = Vec::new();
        for (h, head) in heads.iter_mut().enumerate() {
            let Some(painter) = &mut head.painter else {
                continue;
            };
            let path = |p: &Painter| (p.known(), p.path());
            let before = path(painter);
            let frames = painter.take();
            let notes = painter.take_notes();
            // the render thread told the loop which path draws the head
            if painter.take_known() {
                display_lines(head, videos);
            }
            gpu_notes(head, videos, notes);
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
                let shown = view.present(
                    composed,
                    &mut head.console.term,
                    video.as_mut(),
                    *awake,
                    &**clock,
                );
                if shown.queued {
                    head.figures.frame(shown.compose, now);
                }
            }
            if head.painter.as_ref().map(path) != Some(before) {
                moved.push(h);
            }
        }
        // A head that moved between the GPU path and the CPU path, or
        // whose render thread first said which path draws it, has its
        // displays checked against the `gpu` requirement of its vtlook.
        #[cfg(feature = "filters")]
        for h in moved {
            self.give_colors_again(h);
        }
        #[cfg(not(feature = "filters"))]
        let _ = moved;
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
            due.extend(head.console.term.blink.rapid_due());
            due.extend(head.console.term.sync.deadline());
            due.extend(head.views.values().filter_map(|v| v.wait.due()));
            due.extend(head.figures.due);
            due.extend(head.hangup_due);
            due.extend(head.notice_due);
            due.extend(head.kernel_due());
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
        due.extend(self.pointer_deadlines());
        // A notice's time runs out while the displays draw, and its screen
        // has no other timer, since its cursor is hidden (ADR-1401 D3).
        if self.awake {
            due.extend(
                self.notices
                    .values()
                    .filter(|n| !n.parked)
                    .filter_map(|n| n.due),
            );
        }
        if let Some(Leaving::Until(d)) = self.leaving {
            due.push(d);
        }
        // A card let go after the turn's guard leaves the guard owed.
        if self.leaving == Some(Leaving::Done) || self.guard_due {
            return Some(Duration::ZERO);
        }
        // The rest of a read waits in the pty while the terminal takes no
        // output, as after a synchronized update's end. Once the turn has
        // drawn and the terminal takes output again, the loop feeds it at
        // once. calloop does not shorten its wait for the idle callback that
        // feeds it, so the wait itself is zero. While the terminal still
        // takes nothing, the loop waits for what it waits on, such as the
        // display's flip, and does not spin.
        //
        // References:
        // - ADR-1401 D2, the note of 2026-11-17: the loop wakes at once for
        //   the rest of the read.
        let owed = self
            .heads
            .iter()
            .flat_map(Head::consoles)
            .any(|c| c.term.takes_input() && c.pty.as_ref().is_some_and(Pty::feed_owed));
        if owed {
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

/// Whether `key` is the Esc that dismisses the kernel's messages
/// (ADR-1401 D15): Esc pressed, with Shift and Lock at most.
fn dismisses(key: &KeyEvent) -> bool {
    const ESCAPE: u32 = 0xff1b;
    let held = [InputMods::CTRL, InputMods::ALT, InputMods::LOGO];
    !key.repeat && key.keysyms.contains(&ESCAPE) && !held.iter().any(|&m| key.mods.contains(m))
}

/// What the device a key came from may do (ADR-1401 D5).
#[derive(Clone, Copy, Debug)]
struct KeyFrom {
    /// The VT's keys switch the VT: every keyboard's under open, and under
    /// closed a keyboard's that a path places, but never those of a
    /// keyboard a rule sends to an isolated head (ADR-1401 D3).
    vt_keys: bool,
    /// It roams, so it takes the roaming chord.
    roaming: bool,
}

/// What a head's displays show before a key, to tell whether the key
/// changed it. A key can clear the selection, return the view from the
/// scrollback, change the link the pointer hovers, hide the pointer and
/// show a blinking cursor again, and each of these needs a frame. A key
/// that changes none of them draws no frame of its own: the program's
/// echo draws the next one. A frame drawn for such a key would still have
/// damage on a display with two or three buffers, because the buffer it
/// goes into lacks the frames drawn since that buffer was last shown. The
/// display would then present the same picture again, a frame early, and
/// the echo would wait for that frame's flip.
struct KeyShown {
    /// The screen's cells, its cursor and the pointer's shape.
    screen: crate::term::Shown,
    /// Whether the pointer shows.
    pointer: bool,
    /// Whether the cursor is in the shown phase of its blink.
    cursor: bool,
}

impl KeyShown {
    fn now(head: &mut Head) -> KeyShown {
        KeyShown {
            screen: crate::term::Shown::now(&mut head.console.term.terminal),
            pointer: head.pointer.visible,
            cursor: head.console.term.blink.cursor_shown(),
        }
    }

    /// Whether the key changed what `head`'s displays show.
    fn changed(&self, head: &Head) -> bool {
        self.screen.changed(&head.console.term.terminal)
            || self.pointer != head.pointer.visible
            || self.cursor != head.console.term.blink.cursor_shown()
    }
}

/// What the screens of `heads` show, merged variant by variant: each
/// character and cluster any of them shows in that variant, sorted and
/// once, the regular normal variant first.
fn shown<'a>(heads: impl Iterator<Item = &'a &'a Head>) -> Vec<crate::deferred::Shown> {
    let mut all = vec![crate::deferred::Shown::default()];
    for one in heads.flat_map(|head| head.visible()) {
        match all.iter_mut().find(|s| s.variant == one.variant) {
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
    head.arm_kernel(now);
    let frame_end = std::mem::take(&mut head.console.term.present);
    let now_all = std::mem::take(&mut head.draw_now);
    let term = &mut head.console.term;
    let holding = term.sync.holding(&term.terminal);
    let base = head.cell;
    let mut snapshot = None;
    // A head that holds a GPU path draws every scroll again, into buffers
    // the screen's size, even while a display holds it on the CPU path for
    // now: the GPU path then sees each frame, and takes the head back once
    // it can set the display up. Until its render thread says which path
    // draws it, each view keeps the way it has.
    let scroll = match &head.painter {
        Some(p) if !p.known() => None,
        Some(p) if p.may_hold_gpu() => Some(ScrollDraw::Redraw),
        _ => Some(head.scroll_draw),
    };
    for ((key, view), overlay) in head.views.iter_mut().zip(&overlays) {
        let Some(video) = videos.get_mut(&key.0) else {
            continue;
        };
        let draws = view.draws(term, video.as_ref(), now_all, frame_end, holding, now);
        // A frame the display kept from a busy swap goes out once its flip
        // ends, unless this pass composes a newer frame, which is drawn
        // over it and carries it.
        if !draws || (holding && !view.lost) {
            let sent = view.send_kept(term, video.as_mut());
            if sent.queued {
                head.figures.frame(sent.compose, now);
            }
        }
        if !draws {
            continue;
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
        if let Some(way) = scroll {
            view.scroll = way;
        }
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
                let shown = view.present(composed, term, video.as_mut(), true, clock);
                if shown.queued {
                    head.figures.frame(shown.compose, now);
                }
            }
        }
    }
    // The output after a synchronized update's end goes on once every
    // display that can draw has taken the finished frame. A display that
    // waits for its flip takes the frame at the flip's end. A display that
    // cannot take a frame by itself, such as one on a card handed to
    // another program, owes none.
    for (key, view) in &mut head.views {
        view.settle(videos.get(&key.0).map(|v| &**v));
    }
    term.drawn(head.views.values());
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

/// Logs the GPU path's `notes` about the displays of `head`, each under
/// its display's connector, so that on a head of several displays the log
/// says which display a note is about.
fn gpu_notes(head: &Head, videos: &BTreeMap<PathBuf, Box<dyn Video>>, notes: Vec<Note>) {
    for note in notes {
        let (node, id) = &note.display;
        let name = videos.get(node).map(|v| v.name(*id)).unwrap_or_default();
        log::warn!("head {}, display {name}: {}", head.name(), note.text);
    }
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
pub(crate) fn locale(env: &[(OsString, OsString)]) -> String {
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

/// Opens a device through the VT and wraps it as a video device in the
/// loop, for [`App::add_video`], keeping what the rules read of its
/// `card`. A device opened that cannot be wrapped is closed through the VT
/// again.
pub fn open_video(app: &mut App, node: &Path, kind: DeviceKind, card: Card) -> io::Result<()> {
    let fd = app.vt.open_device(node)?;
    wrap_video(app, node, kind, card, fd).inspect_err(|_| app.vt.close_device(node))
}

/// The device `fd`, opened from `node`, wrapped as a video device.
fn wrap_video(
    app: &mut App,
    node: &Path,
    kind: DeviceKind,
    card: Card,
    fd: std::os::fd::OwnedFd,
) -> io::Result<()> {
    use vtium_platform::display::{Card as Drm, Drm2d, Fb, Fbdev, ModeLookup, ModePolicy, Wanted};
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
            let vt_heads = app.vt_heads.clone();
            let (wanted, node) = (Rc::clone(&app.wanted), key.clone());
            drm.set_dark(Box::new(move |named| {
                // While none is lit, it is announced for the display guard
                // to light (D9), which leaves dark again the ones it lights
                // not. One asked for its notice is announced (D3).
                if !lit.get()
                    || wanted
                        .borrow()
                        .contains(&(node.clone(), named.connector.clone()))
                {
                    return false;
                }
                let display = vtium_devices::display(named, &at, 0);
                place_display(&rules, &vt_heads, &display)
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

/// vtium-term's description of the key event `key` from vtium-platform,
/// as `event`: its first keysym and that keysym's character, the ASCII
/// layout's keysym, the unshifted key, the shifted key, the base layout key
/// and the modifiers, whose bits the two crates share (ADR-1005, D1 and
/// D2). `None` for an event with no keysym.
pub fn term_key(key: &KeyEvent, event: TermKeyEvent) -> Option<Key> {
    let &keysym = key.keysyms.first()?;
    let unicode = key.codepoints.first().copied().flatten();
    let mods = Mods::from_bits(key.mods.bits());
    Some(Key {
        event,
        unshifted: key.unshifted,
        shifted: key.shifted,
        base: key.base,
        ..Key::press(keysym, key.ascii, mods, unicode)
    })
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]
mod tests;
