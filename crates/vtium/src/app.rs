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
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium_devices::Card;
use vtium_platform::calloop::LoopHandle;
use vtium_platform::display::{Blank, Buffers, DisplayId};
use vtium_platform::pty::Pty;
#[cfg(test)]
use vtium_platform::udev::DeviceEvent;
use vtium_platform::udev::DeviceKind;
use vtium_platform::vt::{FakeVt, Vt};
use vtium_settings::Policy;

use crate::blank::Order;
use crate::config::keys;
use crate::config::{Config, ModeSpec};
use crate::font;
use crate::grab::Bindings;
#[cfg(doc)]
use crate::head::chord;
use crate::head::route::{self, Routes};
use crate::head::session::{self, Caps};
use crate::head::vt::VtHeads;
use crate::head::{Frit, Head, Start};
use crate::helper;
use crate::osd::Osd;
use crate::power::{self, Power};
#[cfg(doc)]
use crate::respawn;
use crate::term::{Clock, Term, View};
use crate::video::Video;
use crate::wait::Wait;

mod batch;
#[cfg(test)]
mod batch_login;
mod blanking;
pub mod broker;
#[cfg(test)]
mod colorterm;
#[cfg(test)]
mod cursor_blink;
mod devices;
mod displays;
mod drawing;
mod enter_switch;
mod fonts;
mod glyph_asks;
mod guard;
mod hand_over;
mod held;
mod input;
mod key_order;
#[cfg(feature = "logind")]
pub mod keyboard_layout;
#[cfg(test)]
mod login_scope;
mod notices;
mod pointers;
#[cfg(test)]
mod rapid;
mod sessions;
mod survive;
mod turn;
mod window_ops;

pub use devices::open_video;
pub use drawing::render_timer_line;
pub use enter_switch::{RunningVts, VtiumVts};
pub use input::term_key;

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
    /// Under the privilege split, vtium's root process, which starts the
    /// sessions and runs the power keys; `None` when vtium runs as one
    /// process.
    broker: Option<Rc<vtium_broker_protocol::Courier>>,
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
            broker: None,
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
pub fn locale(env: &[(OsString, OsString)]) -> String {
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

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]
mod tests;
