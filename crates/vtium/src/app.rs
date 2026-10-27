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
//!   at the input that ends the blank; `Hangup` exits.
//! - **Keys**, in order: the VT's, then the seat's (none: one session),
//!   then the terminal's grabs, then DECARM's drop of a repeat, then the
//!   encoder. A key draws at once and holds the cursor's blink.
//! - **The blank.** Each key and pointer event goes to every device's
//!   `input`, which retries a failed wake-up, and to [`Blank::input`]; a
//!   device's `Awake` goes to [`Blank::input`] too, and whenever it says
//!   the displays were blanked every device unblanks. A hotplug never
//!   counts.
//! - **The child.** It runs on the pty at the grid's size; when it exits
//!   the respawn policy starts it again or ends vtium.
//! - **Exit.** A request (SIGTERM, SIGINT, a hangup, the policy) switches
//!   back to the VT shown at the start and serves the loop for up to
//!   [`LEAVE_WAIT`] while the switch is pending.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use vtium_font::{Face, Font, Psf, Unifont};
use vtium_platform::calloop::LoopHandle;
use vtium_platform::display::{Blank, DisplayId, Dpms, Event};
use vtium_platform::input::{Button, InputEvent, KeyEvent, PointerEvent};
use vtium_platform::pty::{Erase, Pty, Size};
use vtium_platform::udev::{DeviceEvent, DeviceKind};
use vtium_platform::vt::{Switch, Vt, VtEvent};
use vtium_render::{Config as RenderConfig, DrawState, Orientation, Profile};
use vtium_term::{Content, Mods, MouseButton, MouseEvent, MouseTracking, Terminal};

use crate::blink::{Blink, Timer};
use crate::child::{self, Display as SessionDisplay, Place};
use crate::config::keys;
use crate::config::{Config, ModeSpec};
use crate::font;
use crate::grab::{self, Grab};
use crate::layout::{self, Zoom};
use crate::respawn::{Next, Respawn};
use crate::sync::{Action, Sync};
use crate::term::{Clock, Term, View};
use crate::video::Video;

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
}

impl Exit {
    pub fn status(self) -> i32 {
        match self {
            Exit::Signal | Exit::Oneshot => 0,
            Exit::TooQuick | Exit::Hangup | Exit::Spawn => 1,
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

/// The pointer, in the grid's pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Pointer {
    x: u32,
    y: u32,
    visible: bool,
    held: Option<Button>,
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
    /// The VT is ours and the devices awake.
    awake: bool,
    /// The VT was re-taken while blanked: every device is set up again
    /// at the input that ends the blank.
    retake_owed: bool,
    /// Devices a release waits on.
    releasing: Option<BTreeSet<PathBuf>>,
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
        if let Some(p) = palette(&config) {
            terminal.set_palette(p);
        }
        if let Ok(table) = crate::termcap::termcap() {
            terminal.set_termcap(table);
        }
        let timeout = Duration::from_millis(config.number(&keys::SYNC_TIMEOUT).unwrap_or(200));
        terminal.set_sync_enabled(!timeout.is_zero());
        let sync = Sync::new(timeout, terminal.sync_frames());
        let blink = Blink::new(config.switch(&keys::BLINK));
        let dpms = config
            .number(&keys::DPMS_TIMEOUT)
            .map(Duration::from_secs)
            .filter(|d| !d.is_zero());
        let modes = config.modes(&keys::MODE).to_vec();
        let use_original = config.switch(&keys::USE_ORIGINAL_MODE) && modes.is_empty();
        App {
            oneshot: config.switch(&keys::ONESHOT),
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
            orientation: Orientation::Normal,
            config,
            handle,
            clock,
            epoch: now,
            respawn: None,
            awake: false,
            retake_owed: false,
            releasing: None,
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

    /// The configuration the seat runs with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Puts the DPMS blank's timer in the loop.
    pub fn insert_blank(&self) {
        self.blank.insert(
            &self.handle,
            |app: &mut App| Some(&mut app.blank),
            |app: &mut App| app.blank_now(),
        );
    }

    /// Starts the session's command.
    pub fn start(&mut self) {
        let now = (self.clock)();
        self.respawn = Some(Respawn::new(now.saturating_duration_since(self.epoch)));
        self.spawn();
    }

    /// Asks vtium to end, as SIGTERM does.
    pub fn request_exit(&mut self, why: Exit) {
        self.exit.get_or_insert(why);
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

    /// A face at `height`, as the configuration names it.
    fn face(&self, height: u32) -> Result<Box<dyn Face>, String> {
        let name = self.config.text(&keys::FONT_NAME).unwrap_or_default();
        match self.config.text(&keys::FONT_ENGINE) {
            Some("psf") => {
                let mut psf = Psf::load(name).map_err(|e| format!("{name}: {e}"))?;
                psf.set_height(height);
                Ok(Box::new(psf))
            }
            _ => Ok(Box::new(Unifont::new(height))),
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
        let Ok(face) = self.face(self.font_size.max(1)) else {
            log::error!("cannot load the font for display {name}");
            return;
        };
        let font = self.font_at(face);
        let view = View::new(
            id,
            font,
            size,
            refresh,
            self.render_config(),
            self.measure(),
        );
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
        if self.auto_size {
            let w = sizes.iter().map(|s| s.0).min().unwrap_or(0);
            let h = sizes.iter().map(|s| s.1).min().unwrap_or(0);
            self.font_size = font::auto_size(w, h);
        }
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
        for (key, height) in keys.into_iter().zip(heights) {
            let stale = self
                .views
                .get(&key)
                .is_some_and(|v| v.height != height || v.orientation != self.orientation);
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
        true
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
        if grid == self.grid {
            return;
        }
        self.grid = grid;
        self.term.terminal.screen_mut().resize(grid.0, grid.1);
        let size = self.pty_size();
        if let Some(pty) = &mut self.pty {
            if let Err(e) = pty.resize(size) {
                log::warn!("cannot resize the pty: {e}");
            }
        }
    }

    /// The terminal starts drawing and blinking.
    fn activate(&mut self, now: Instant) {
        self.awake = true;
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
        self.text_due = None;
        self.cursor_due = None;
        self.timer_due = None;
    }

    /// The blank's timer: every display powers down.
    fn blank_now(&mut self) {
        for video in self.videos.values_mut() {
            for id in video.displays() {
                if let Err(e) = video.set_dpms(id, Dpms::Off) {
                    log::warn!("cannot blank a display: {e}");
                }
            }
        }
        self.deactivate();
    }

    /// A key or pointer event, or a device's wake-up, counts as activity.
    fn activity(&mut self, now: Instant) {
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
                // The wake-up sets every device up, an owed re-take's too.
                self.retake_owed = false;
                if let Some(input) = &mut self.input {
                    input.wake_up();
                }
                for (node, video) in &mut self.videos {
                    if let Err(e) = video.wake_up() {
                        log::warn!("cannot wake {}: {e}", node.display());
                    }
                }
                self.activate(now);
            }
            VtEvent::Release => {
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
            Event::Flipped(id) => {
                if let Some(v) = self.views.get_mut(&(node.clone(), id)) {
                    if let Some(t) = &mut v.timer {
                        t.flip();
                    }
                    if v.pending {
                        self.draw_now = true;
                    }
                }
            }
            Event::Awake => self.activity(now),
            Event::Asleep => {
                if let Some(r) = &mut self.releasing {
                    r.remove(node);
                }
            }
            Event::WakeFailed => {
                log::warn!(
                    "{}: the displays did not wake; the next key tries again",
                    node.display()
                );
            }
        }
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

    /// A key or pointer event from the input devices.
    pub fn input_event(&mut self, event: InputEvent, now: Instant) {
        for video in self.videos.values_mut() {
            if let Err(e) = video.input() {
                log::warn!("cannot wake the displays: {e}");
            }
        }
        self.activity(now);
        match event {
            InputEvent::Key(key) => self.key(&key, now),
            InputEvent::Pointer(p) => self.pointer(p),
        }
    }

    /// The key order: the VT's keys, the seat's (none), the terminal's
    /// grabs, DECARM, the encoder.
    fn key(&mut self, key: &KeyEvent, now: Instant) {
        if self.vt.key(key) || !self.awake {
            return;
        }
        let screen = self.term.terminal.screen_mut();
        if let Some(g) = grab::grab(key.mods.bits(), &key.keysyms) {
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

    /// A zoom: the base height a size step larger or smaller, which ends
    /// `font-size=0`'s following of the display. A height with no font
    /// keeps the one in use.
    fn zoom(&mut self, dir: Zoom) {
        let Some(size) = layout::zoom(self.font_size, self.step, dir) else {
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

    fn pointer(&mut self, event: PointerEvent) {
        let cell = (self.cell.0.max(1), self.cell.1.max(1));
        let at = |p: &Pointer| {
            (
                usize::try_from(p.x.checked_div(cell.0).unwrap_or(0)).unwrap_or(0),
                usize::try_from(p.y.checked_div(cell.1).unwrap_or(0)).unwrap_or(0),
            )
        };
        let tracking = self.term.terminal.mouse_tracking() != MouseTracking::Off;
        let mods = Mods::from_bits(self.input.as_ref().map_or(0, |i| i.mods().bits()));
        let moved = matches!(event, PointerEvent::Moved { .. });
        match event {
            PointerEvent::Moved { x, y, held } => {
                self.pointer.x = u32::try_from(x).unwrap_or(0);
                self.pointer.y = u32::try_from(y).unwrap_or(0);
                self.pointer.visible = true;
                self.pointer.held = held;
                if tracking {
                    let (col, row) = at(&self.pointer);
                    let button = held.map_or(MouseButton::None, mouse_button);
                    self.term.terminal.mouse(
                        col,
                        row,
                        self.pointer.x,
                        self.pointer.y,
                        button,
                        MouseEvent::Move,
                        mods,
                    );
                }
            }
            PointerEvent::Button {
                button, pressed, ..
            } => {
                if tracking {
                    let (col, row) = at(&self.pointer);
                    let event = if pressed {
                        MouseEvent::Press
                    } else {
                        MouseEvent::Release
                    };
                    self.term.terminal.mouse(
                        col,
                        row,
                        self.pointer.x,
                        self.pointer.y,
                        mouse_button(button),
                        event,
                        mods,
                    );
                }
            }
            PointerEvent::Wheel(steps) => {
                let ctrl = mods.contains(Mods::CTRL);
                match wheel(steps, tracking, ctrl) {
                    Wheel::Report(button) => {
                        let (col, row) = at(&self.pointer);
                        self.term.terminal.mouse(
                            col,
                            row,
                            self.pointer.x,
                            self.pointer.y,
                            button,
                            MouseEvent::Press,
                            mods,
                        );
                    }
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
            PointerEvent::Hide => self.pointer.visible = false,
        }
        // Only motion may be coalesced while the child does not read; a
        // press, a release or a wheel report is queued whole.
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
                // A new session starts from a hard reset (RIS).
                self.term.terminal.input(b"\x1bc");
                self.term.terminal.take_replies();
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
            foreground: palette.fg(),
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
        for e in inputs {
            self.input_event(e, now);
        }
        self.child_exited(now);
        self.timers(now);
        if self.awake {
            self.draw(now);
        }
        self.report(now);
        self.term.coalesce.turn_end();
        self.term.changed = false;
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
        due.extend(self.term.sync.deadline());
        due.extend(self.views.values().filter_map(|v| v.wait.due()));
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

/// The palette `palette` names: `None` for `legacy`, the terminal's own,
/// and the `palette-*` keys for `custom`. vtium-term holds no other named
/// palette yet, so another name keeps `legacy`.
fn palette(config: &Config) -> Option<vtium_term::Palette> {
    match config.text(&keys::PALETTE) {
        None | Some("legacy") => None,
        Some("custom") => {
            let mut p = vtium_term::Palette::default();
            let rgb = |k| {
                let [r, g, b] = config.rgb(k).unwrap_or_default();
                vtium_term::Rgb::new(r, g, b)
            };
            for (i, key) in PALETTE_KEYS.iter().enumerate() {
                p.set_color(u8::try_from(i).unwrap_or(0), rgb(key));
            }
            p.set_fg(rgb(&keys::PALETTE_FOREGROUND));
            p.set_bg(rgb(&keys::PALETTE_BACKGROUND));
            Some(p)
        }
        // Reported at start, through `check::semantics`.
        Some(_) => None,
    }
}

/// The sixteen colours' keys, in order.
const PALETTE_KEYS: [&crate::config::Key; 16] = [
    &keys::PALETTE_BLACK,
    &keys::PALETTE_RED,
    &keys::PALETTE_GREEN,
    &keys::PALETTE_YELLOW,
    &keys::PALETTE_BLUE,
    &keys::PALETTE_MAGENTA,
    &keys::PALETTE_CYAN,
    &keys::PALETTE_LIGHT_GREY,
    &keys::PALETTE_DARK_GREY,
    &keys::PALETTE_LIGHT_RED,
    &keys::PALETTE_LIGHT_GREEN,
    &keys::PALETTE_LIGHT_YELLOW,
    &keys::PALETTE_LIGHT_BLUE,
    &keys::PALETTE_LIGHT_MAGENTA,
    &keys::PALETTE_LIGHT_CYAN,
    &keys::PALETTE_WHITE,
];

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

#[cfg(test)]
mod tests {
    use super::*;

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
