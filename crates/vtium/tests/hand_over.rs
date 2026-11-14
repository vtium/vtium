//! Handing a head's displays to a graphical program (ADR-1401 D16): the
//! seat booted with two cards in memory, on the fake VT, with a real
//! child on a real pty in each console. Each card stands in for a DRM card
//! and keeps DRM master as the kernel does: one file at a time holds it,
//! and a set by another file fails while one holds it. A program, which
//! the test plays, takes a card's master only once vtium has let it go.
//!
//! The heads: `left` on card A's first display with two consoles,
//! `right` mirrored on card A's second display and card B's second
//! display, and `other` on card B's first display. Each has a keyboard of
//! its own. A setBackground from `left` gives up card A: `left` is wholly
//! paused, `right` pauses on card A and draws on card B, and `other` draws
//! on.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vtium::app::App;
use vtium::config;
use vtium::font::Sources;
use vtium::head::Start;
use vtium::video::Video;
use vtium_platform::display::{
    DisplayId, Dpms, Event, Identity, Memory as Shared, Rect, SwapError, Swapped, Token,
};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::{FakeVt, Switch, Vt, VtEvent};
use vtium_settings::Class;

#[path = "common/vt.rs"]
mod counting;
#[path = "common/scratch.rs"]
mod scratch;
use counting::CountingVt;
use scratch::Scratch;

/// How long a wait on a state that another thread or process brings may
/// run before the test fails: only a bound, since each wait ends at the
/// state it waits for.
const BOUND: Duration = Duration::from_secs(10);

const W: u32 = 320;
const H: u32 = 160;

/// Who holds a card's DRM master.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holder {
    Vtium,
    Program,
}

/// A card's DRM master as the kernel keeps it: at most one file holds it,
/// and a set by another file fails with EBUSY while one does
/// (drivers/gpu/drm/drm_auth.c, `drm_setmaster_ioctl`).
#[derive(Debug, Default)]
struct Master {
    holder: Option<Holder>,
}

impl Master {
    fn set(&mut self, who: Holder) -> io::Result<()> {
        match self.holder {
            Some(h) if h != who => Err(io::Error::from_raw_os_error(16)),
            _ => {
                self.holder = Some(who);
                Ok(())
            }
        }
    }

    fn drop_master(&mut self, who: Holder) {
        if self.holder == Some(who) {
            self.holder = None;
        }
    }
}

/// One display of a card, its frame buffer in memory.
struct Output {
    id: DisplayId,
    connector: &'static str,
    /// The buffer each token lends, drawn into on the render thread.
    drawn: Arc<Mutex<Vec<u8>>>,
    age: u32,
    /// Taken anew at each sleep and wake-up: a token from before comes
    /// back too late.
    lease: u64,
    dpms: Option<Dpms>,
    /// Each frame shown, with the age of the buffer it was drawn into.
    frames: Vec<u32>,
    announced: bool,
}

/// A card with displays, standing in for drm2d over a DRM card: it lends a
/// buffer only while awake, and the kernel refuses a frame or a power
/// state set without DRM master. It lets master go at a sleep, and takes
/// it at a wake-up, which fails while another file holds it. After a
/// failed wake-up it tries again while it is in the foreground, as drm2d's
/// retries do, at each [`Card::retry`]. A card made [`Card::slow`] ends
/// its sleep only some turns later, as drm2d waits for a flip in flight,
/// and a wake-up meanwhile cancels the sleep, as drm2d's does.
struct Card {
    name: &'static str,
    master: Rc<RefCell<Master>>,
    outputs: Vec<Output>,
    awake: bool,
    /// Woken last, not put to sleep since: drm2d retries a failed wake-up
    /// only then.
    foreground: bool,
    events: Vec<Event>,
    /// Frames swapped and power states set without DRM master, which the
    /// kernel refuses.
    refused: u32,
    /// How many turns, each a call of `events`, a sleep waits before it
    /// lets master go and reports `Asleep`.
    sleep_turns: u32,
    /// The turns left before the sleep under way ends.
    sleeping: Option<u32>,
}

impl Card {
    fn new(name: &'static str, connectors: &[&'static str]) -> Card {
        let outputs = connectors
            .iter()
            .zip(1..)
            .map(|(&connector, id)| Output {
                id: DisplayId(id),
                connector,
                drawn: Arc::new(Mutex::new(vec![0; (W * H * 4) as usize])),
                age: 0,
                lease: 0,
                dpms: None,
                frames: Vec::new(),
                announced: false,
            })
            .collect();
        Card {
            name,
            master: Rc::default(),
            outputs,
            awake: false,
            foreground: false,
            events: Vec::new(),
            refused: 0,
            sleep_turns: 0,
            sleeping: None,
        }
    }

    /// This card, whose sleep ends `turns` turns after it starts.
    fn slow(self, turns: u32) -> Card {
        Card {
            sleep_turns: turns,
            ..self
        }
    }

    /// Ends the sleep under way: master goes, and the owner hears
    /// `Asleep`.
    fn finish_sleep(&mut self) {
        self.sleeping = None;
        self.master.borrow_mut().drop_master(Holder::Vtium);
        self.events.push(Event::Asleep);
    }

    fn output(&self, id: DisplayId) -> Option<&Output> {
        self.outputs.iter().find(|o| o.id == id)
    }

    fn ours(&self) -> bool {
        self.master.borrow().holder == Some(Holder::Vtium)
    }

    /// A wake-up retry's timer fired: drm2d tries the wake-up again while
    /// the card is in the foreground and asleep.
    fn retry(&mut self) {
        if self.foreground && !self.awake {
            let _ = self.wake_up();
        }
    }
}

impl Video for Card {
    fn backend(&self) -> &'static str {
        "card"
    }
    fn events(&mut self) -> Vec<Event> {
        match self.sleeping {
            Some(0) => self.finish_sleep(),
            Some(n) => self.sleeping = Some(n - 1),
            None => {}
        }
        std::mem::take(&mut self.events)
    }
    fn displays(&self) -> Vec<DisplayId> {
        if self.awake {
            self.outputs.iter().map(|o| o.id).collect()
        } else {
            Vec::new()
        }
    }
    fn name(&self, id: DisplayId) -> String {
        self.output(id)
            .map_or_else(String::new, |o| o.connector.to_owned())
    }
    fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
        Some((W, H))
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(60_000)
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        Some((72.0, 72.0))
    }
    fn identity(&self, id: DisplayId) -> Option<Identity> {
        Some(Identity {
            connector: self.output(id)?.connector.to_owned(),
            card: Some(self.name.to_owned()),
            edid: None,
            connector_path: None,
            lit_at_start: false,
        })
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        if !self.ready_for_frame(id) {
            return None;
        }
        let o = self.outputs.iter().find(|o| o.id == id)?;
        let memory: Arc<Shared> = o.drawn.clone();
        Some(Token::new(id, (W, H), W * 4, o.age, memory).leased(o.lease))
    }
    fn give_back(&mut self, token: Token) {
        drop(token);
    }
    fn swap(&mut self, token: Token, _: &[Rect]) -> Result<Swapped, SwapError> {
        let id = token.id();
        let ours = self.ours();
        let Some(o) = self.outputs.iter_mut().find(|o| o.id == id) else {
            return Err(SwapError::Off);
        };
        if token.lease() != o.lease {
            o.age = 0;
            return Err(SwapError::Dropped);
        }
        drop(token);
        if !ours {
            self.refused += 1;
            return Err(SwapError::Off);
        }
        if o.dpms != Some(Dpms::On) {
            return Err(SwapError::Off);
        }
        o.frames.push(o.age);
        o.age = 1;
        self.events.push(Event::Flipped(id));
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        self.awake
            && self
                .output(id)
                .is_some_and(|o| o.dpms == Some(Dpms::On) && Arc::strong_count(&o.drawn) == 1)
    }
    fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        if !self.awake {
            return Err(io::Error::other("the display is not online"));
        }
        if !self.ours() {
            self.refused += 1;
            return Err(io::Error::from_raw_os_error(13));
        }
        if let Some(o) = self.outputs.iter_mut().find(|o| o.id == id) {
            o.dpms = Some(dpms);
        }
        Ok(())
    }
    fn sleep(&mut self) {
        self.awake = false;
        self.foreground = false;
        for o in &mut self.outputs {
            o.lease += 1;
        }
        self.sleeping = Some(self.sleep_turns);
        if self.sleep_turns == 0 {
            self.finish_sleep();
        }
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.foreground = true;
        // A sleep still under way is cancelled, and master stays.
        self.sleeping = None;
        self.master.borrow_mut().set(Holder::Vtium)?;
        self.awake = true;
        for o in &mut self.outputs {
            o.lease += 1;
            o.age = 0;
            o.dpms = Some(Dpms::On);
            let event = if o.announced {
                Event::Redraw(o.id)
            } else {
                Event::Added(o.id)
            };
            o.announced = true;
            self.events.push(event);
        }
        self.events.push(Event::Awake);
        Ok(())
    }
    fn unblank(&mut self) -> io::Result<()> {
        if !self.awake {
            return Ok(());
        }
        for o in &mut self.outputs {
            o.dpms = Some(Dpms::On);
        }
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

/// The fake VT, counting each release vtium acknowledges and each time
/// vtium puts the VT's modes back, and standing in for a seat manager's
/// seat where `seat_manager` holds.
struct TestVt {
    vt: CountingVt,
    seat_manager: bool,
    restores: Rc<Cell<u32>>,
    /// Events the test sends, as the kernel's VT reports a re-take.
    sent: Rc<RefCell<Vec<VtEvent>>>,
    events: Vec<VtEvent>,
}

impl Vt for TestVt {
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
        self.events.extend(self.vt.events());
        self.events.append(&mut self.sent.borrow_mut());
        self.events.drain(..)
    }
    fn acknowledge_release(&mut self) -> io::Result<()> {
        self.vt.acknowledge_release()
    }
    fn refuse_release(&mut self) -> io::Result<()> {
        self.vt.refuse_release()
    }
    fn activate(&mut self) -> io::Result<Switch> {
        self.vt.activate()
    }
    fn leave(&mut self) -> io::Result<Switch> {
        self.vt.leave()
    }
    fn wait(&mut self) -> io::Result<Switch> {
        self.vt.wait()
    }
    fn is_active(&self) -> bool {
        self.vt.is_active()
    }
    fn key(&mut self, key: &KeyEvent) -> bool {
        self.vt.key(key)
    }
    fn is_own_key(&self, key: &KeyEvent) -> bool {
        self.vt.is_own_key(key)
    }
    fn restore(&mut self) -> io::Result<()> {
        self.restores.set(self.restores.get() + 1);
        self.vt.restore()
    }
    fn seat_manager(&self) -> bool {
        self.seat_manager
    }
    fn bell(&mut self) {}
    fn number(&self) -> Option<u16> {
        self.vt.number()
    }
}

const HEADS: &str = "version = 1
[head left]
consoles = 2
[head right]
[head other]
[rules]
display connector:card-a/A-1 -> left
display connector:card-a/A-2 -> right
display connector:card-b/B-2 -> right
display connector:card-b/B-1 -> other
input path:kbd-left -> left
input path:kbd-right -> right
input path:kbd-other -> other
input class:keyboard -> roaming
";

const CARD_A: &str = "/dev/dri/card-a";
const CARD_B: &str = "/dev/dri/card-b";
const KBD_LEFT: &str = "/dev/input/event0";
const KBD_RIGHT: &str = "/dev/input/event1";
const KBD_OTHER: &str = "/dev/input/event2";
const KBD_ROAMING: &str = "/dev/input/event3";

const A1: DisplayId = DisplayId(1);
const A2: DisplayId = DisplayId(2);
const B1: DisplayId = DisplayId(1);
const B2: DisplayId = DisplayId(2);

/// Each console's session: it echoes each line typed, and it acts on the
/// files the test makes, each named for its head and console: `.bg` sends
/// setBackground, `.fg` setForeground, `.focus` turns on focus reporting
/// (mode 1004) and `.exit` ends the session. Where the seat boots with
/// `F` set, the session reads its input three bytes at a time instead of
/// by lines, and prints each read as hex after `k:`, so a focus report
/// shows as `k: 1b 5b 49` (in) or `k: 1b 5b 4f` (out).
const SESSION: &str = r#"P="$D/$VTIUM_HEAD-$VTIUM_CONSOLE"
( while kill -0 $$ 2>/dev/null; do
    if [ -e "$P.bg" ]; then rm -f "$P.bg"; printf '\033]setBackground\007'; echo sent:bg; fi
    if [ -e "$P.fg" ]; then rm -f "$P.fg"; printf '\033]setForeground\007'; echo sent:fg; fi
    if [ -e "$P.focus" ]; then rm -f "$P.focus"; printf '\033[?1004h'; echo focus:on; fi
    if [ -e "$P.exit" ]; then rm -f "$P.exit"; kill $$; fi
    sleep 0.02
  done ) &
if [ -n "$F" ]; then stty -icanon -echo; fi
echo "ready:$VTIUM_HEAD-$VTIUM_CONSOLE"
if [ -n "$F" ]; then
  while :; do echo "k:$(head -c 3 | od -An -tx1)"; done
fi
while read a; do echo "got:$a"; done"#;

/// A focus-in and a focus-out, as a session booted with `F` prints them.
const IN: &str = "k: 1b 5b 49";
const OUT: &str = "k: 1b 5b 4f";

#[expect(
    clippy::disallowed_methods,
    reason = "the harness runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

fn no_fonts() -> Sources {
    Sources {
        frits: PathBuf::from("/nonexistent/vtium-test-frits"),
        trust: vtium::font::trust(),
        helper: None,
        vconsole_conf: PathBuf::from("/dev/null"),
        kernel_cmdline: PathBuf::from("/dev/null"),
    }
}

/// A keyboard at the USB port `path`, as the rules read it.
fn keyboard(path: &str) -> vtium_settings::Identity {
    vtium_settings::Identity {
        path: Some(path.to_owned()),
        ..vtium_settings::Identity::input().with_class(Class::Keyboard)
    }
}

/// The key that types `c`, `\r` Return.
fn char_key(c: char) -> KeyEvent {
    let keysym = if c == '\r' { 0xff0d } else { u32::from(c) };
    KeyEvent {
        keycode: 0,
        ascii: 0,
        unshifted: 0,
        shifted: 0,
        base: None,
        mods: Mods::empty(),
        keysyms: vec![keysym],
        codepoints: vec![(c != '\r').then_some(c)],
        repeat: false,
    }
}

/// Ctrl+Super+F12, the fake VT's switch key.
fn switch_key() -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        unshifted: 0,
        shifted: 0,
        base: None,
        mods: Mods::CTRL.union(Mods::LOGO),
        keysyms: vec![0xffc9],
        codepoints: vec![None],
        repeat: false,
    })
}

/// The key `keysym` held with Ctrl and Super: with Up the roaming chord,
/// with Right a console chord.
fn ctrl_super(keysym: u32) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        unshifted: 0,
        shifted: 0,
        base: None,
        mods: Mods::CTRL.union(Mods::LOGO),
        keysyms: vec![keysym],
        codepoints: vec![None],
        repeat: false,
    })
}

const UP: u32 = 0xff52;
const RIGHT: u32 = 0xff53;

/// The rows of every console of head `name`, in the ring's order.
fn consoles(app: &App, name: &str) -> Vec<Vec<String>> {
    app.head(name)
        .map(vtium::head::Head::consoles_text)
        .unwrap_or_default()
}

/// Whether a row of console `n` (from 1) of head `name` holds `text`.
fn console_shows(app: &App, name: &str, n: usize, text: &str) -> bool {
    consoles(app, name)
        .get(n - 1)
        .is_some_and(|rows| rows.iter().any(|l| l.contains(text)))
}

/// The focus reports that console 1 of head `name` received, in order,
/// as a session booted with `F` prints them.
fn reports(app: &App, name: &str) -> Vec<String> {
    consoles(app, name)
        .first()
        .map(|rows| {
            rows.iter()
                .filter(|l| l.starts_with("k:"))
                .map(|l| l.trim_end().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The focus reports of console 1 of each head, `left`, `right` and
/// `other` in that order.
fn every_report(app: &App) -> [Vec<String>; 3] {
    ["left", "right", "other"].map(|h| reports(app, h))
}

/// `texts` as owned lines, to compare with [`reports`].
fn lines(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|t| (*t).to_owned()).collect()
}

struct Seat {
    lp: Loop<App>,
    app: App,
    /// How many VT releases vtium has acknowledged.
    released: Rc<Cell<u32>>,
    restores: Rc<Cell<u32>>,
    master_a: Rc<RefCell<Master>>,
    master_b: Rc<RefCell<Master>>,
    /// Events sent to the VT.
    vt_sent: Rc<RefCell<Vec<VtEvent>>>,
    dir: Scratch,
}

impl Drop for Seat {
    /// Ends every console's session with its process group before the
    /// app drops, so no session outlives its test.
    fn drop(&mut self) {
        for name in ["left", "right", "other"] {
            if let Some(head) = self.app.head(name)
                && let Some(pid) = head.pty().and_then(|p| p.child_id())
                && let Ok(pid) = i32::try_from(pid)
                && let Some(pid) = rustix::process::Pid::from_raw(pid)
            {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
        }
    }
}

impl Seat {
    /// The seat booted with `args` on [`HEADS`], each console running
    /// [`SESSION`], on a fake VT that stands in for a seat manager's seat
    /// where `seat_manager` holds; it returns once every head's first
    /// console is ready.
    fn boot(test: &str, args: &[&str], seat_manager: bool) -> Seat {
        Seat::boot_with(test, args, seat_manager, 0)
    }

    /// [`Seat::boot`], with card A's sleep ending `sleep_turns` turns
    /// after it starts.
    fn boot_with(test: &str, args: &[&str], seat_manager: bool, sleep_turns: u32) -> Seat {
        Seat::boot_full(test, args, seat_manager, sleep_turns, false)
    }

    /// [`Seat::boot`], each session reading its input three bytes at a
    /// time, so that it prints the focus reports it receives. The test
    /// turns on each console's focus reporting with [`Seat::report_focus`].
    fn boot_reporting(test: &str, seat_manager: bool) -> Seat {
        Seat::boot_full(test, &[], seat_manager, 0, true)
    }

    fn boot_full(
        test: &str,
        args: &[&str],
        seat_manager: bool,
        sleep_turns: u32,
        reporting: bool,
    ) -> Seat {
        let dir = Scratch::new(test);
        let f = if reporting { "F=1\n" } else { "" };
        let script = format!("D='{}'\n{f}{SESSION}", dir.display());
        let args: Vec<OsString> = ["--font-size=16"]
            .iter()
            .chain(args)
            .chain(&["--login", "--", "/bin/sh", "-c", &script])
            .map(OsString::from)
            .collect();
        let loaded = config::load(&args, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the seat's command line");
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let (vt, released) = CountingVt::new(FakeVt::with_number(1));
        let restores = Rc::new(Cell::new(0));
        let vt_sent = Rc::new(RefCell::new(Vec::new()));
        let vt = TestVt {
            vt,
            seat_manager,
            restores: Rc::clone(&restores),
            sent: Rc::clone(&vt_sent),
            events: Vec::new(),
        };
        let mut app = App::new(loaded.config, Box::new(vt), lp.handle(), Rc::new(now), env);
        app.set_font_sources(no_fonts());
        let start = Start::trusted(Path::new("heads.conf"), HEADS.as_bytes());
        assert_eq!(start.line(), None, "a valid head file");
        app.set_heads(start);
        let a = Card::new("card-a", &["A-1", "A-2"]).slow(sleep_turns);
        let b = Card::new("card-b", &["B-1", "B-2"]);
        let (master_a, master_b) = (Rc::clone(&a.master), Rc::clone(&b.master));
        app.add_video(PathBuf::from(CARD_A), Box::new(a));
        app.add_video(PathBuf::from(CARD_B), Box::new(b));
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        app.add_input(Path::new(KBD_LEFT), keyboard("kbd-left"));
        app.add_input(Path::new(KBD_RIGHT), keyboard("kbd-right"));
        app.add_input(Path::new(KBD_OTHER), keyboard("kbd-other"));
        app.add_input(Path::new(KBD_ROAMING), keyboard("kbd-roaming"));
        let mut s = Seat {
            lp,
            app,
            released,
            restores,
            master_a,
            master_b,
            vt_sent,
            dir,
        };
        assert!(
            s.until(|a| ["left", "right", "other"].iter().all(|h| console_shows(
                a,
                h,
                1,
                &format!("ready:{h}-1")
            )) && console_shows(a, "left", 2, "ready:left-2")),
            "every console is ready: {:?}",
            ["left", "right", "other"].map(|h| consoles(&s.app, h))
        );
        s
    }

    /// Turns the loop until `done` holds, at most [`BOUND`].
    fn until(&mut self, mut done: impl FnMut(&App) -> bool) -> bool {
        let end = now() + BOUND;
        while now() < end {
            if done(&self.app) {
                return true;
            }
            let wait = self
                .app
                .next_wake(now())
                .unwrap_or(Duration::from_millis(20))
                .min(Duration::from_millis(20));
            self.lp
                .dispatch(Some(wait), &mut self.app)
                .expect("dispatch");
            self.app.turn(now());
        }
        done(&self.app)
    }

    /// Reads the sessions' output with no turn of the seat, at most
    /// [`BOUND`], until head `name` holds a setBackground or setForeground
    /// that the seat has not taken.
    fn read_until_requested(&mut self, name: &str) -> bool {
        let end = now() + BOUND;
        while now() < end {
            if self.app.head(name).is_some_and(|h| h.display_requested()) {
                return true;
            }
            self.lp
                .dispatch(Some(Duration::from_millis(20)), &mut self.app)
                .expect("dispatch");
        }
        false
    }

    /// Turns the loop for `turns` turns of at most 20 ms each, for a
    /// state that must not come.
    fn turns(&mut self, turns: u32) {
        for _ in 0..turns {
            self.lp
                .dispatch(Some(Duration::from_millis(20)), &mut self.app)
                .expect("dispatch");
            self.app.turn(now());
        }
    }

    fn card(&mut self, node: &str) -> &mut Card {
        self.app
            .videos
            .get_mut(Path::new(node))
            .and_then(|v| v.as_any().downcast_mut::<Card>())
            .expect("a card")
    }

    /// The frames shown on display `id` of the card at `node`.
    fn frames(&mut self, node: &str, id: DisplayId) -> usize {
        self.card(node).output(id).map_or(0, |o| o.frames.len())
    }

    /// Turns the loop until display `id` of the card at `node` shows more
    /// than `frames` frames, at most [`BOUND`].
    fn until_frames(&mut self, node: &str, id: DisplayId, frames: usize) -> bool {
        let end = now() + BOUND;
        while self.frames(node, id) <= frames {
            if now() > end {
                return false;
            }
            self.turns(1);
        }
        true
    }

    /// Makes the file that tells console `n` of head `head` to act.
    fn tell(&self, head: &str, n: usize, what: &str) {
        std::fs::write(self.dir.join(format!("{head}-{n}.{what}")), "").expect("the trigger file");
    }

    fn type_on(&mut self, node: &str, text: &str) {
        for c in text.chars() {
            self.app
                .device_input(Path::new(node), InputEvent::Key(char_key(c)), now());
        }
    }

    /// Console 1 of each head named turns on focus reporting, and the
    /// test waits until each says so. Each head already has the focus, so
    /// no session has a report yet.
    fn report_focus(&mut self, heads: &[&str]) {
        for h in heads {
            self.tell(h, 1, "focus");
        }
        assert!(
            self.until(|a| heads.iter().all(|h| console_shows(a, h, 1, "focus:on"))),
            "focus reporting is on: {:?}",
            heads
                .iter()
                .map(|h| consoles(&self.app, h))
                .collect::<Vec<_>>()
        );
    }

    /// Console 1 of head `left` sends setBackground, and the test waits
    /// until vtium let card A go.
    fn left_sends_background(&mut self) {
        self.tell("left", 1, "bg");
        let master = Rc::clone(&self.master_a);
        assert!(
            self.until(
                |a| console_shows(a, "left", 1, "sent:bg") && master.borrow().holder.is_none()
            ),
            "vtium let card A go: {:?}",
            master.borrow().holder
        );
    }
}

/// D16: setBackground from the console a head shows gives up every card
/// that drives a display of that head. vtium drops DRM master there, so a
/// program can take it, and draws nothing there. A head with a display
/// on another card draws on it, and a head on another card draws on.
/// Keys from the paused head's keyboard reach no console; the other heads
/// still take theirs. setForeground from the same console ends the
/// hand-over: vtium puts the VT's modes back and wakes the card, whose
/// retry takes it back once the program lets go, and each display draws
/// whole.
// covers: platform.hand-back
#[test]
fn set_background_gives_up_the_senders_cards_until_set_foreground() {
    let mut s = Seat::boot("hand-over-foreground", &[], false);
    s.left_sends_background();
    assert_eq!(
        s.master_a.borrow_mut().set(Holder::Program).ok(),
        Some(()),
        "the program takes card A"
    );
    assert_eq!(
        s.master_b.borrow().holder,
        Some(Holder::Vtium),
        "card B stays vtium's"
    );
    // The heads go on: right on card B's display, other on card B.
    let (right_b, other_b) = (s.frames(CARD_B, B2), s.frames(CARD_B, B1));
    let right_a = s.frames(CARD_A, A2);
    s.type_on(KBD_RIGHT, "r\r");
    s.type_on(KBD_OTHER, "o\r");
    s.type_on(KBD_LEFT, "k\r");
    assert!(
        s.until(|a| console_shows(a, "right", 1, "got:r") && console_shows(a, "other", 1, "got:o")),
        "the heads that still draw take their keys"
    );
    // The turns the paused head's key would need to reach its console.
    s.turns(10);
    let (now_right_b, now_other_b) = (s.frames(CARD_B, B2), s.frames(CARD_B, B1));
    assert!(now_right_b > right_b, "right draws on card B");
    assert!(now_other_b > other_b, "other draws on");
    assert_eq!(s.frames(CARD_A, A2), right_a, "nothing draws on card A");
    assert!(
        !console_shows(&s.app, "left", 1, "got:k"),
        "the paused head took no key"
    );
    assert_eq!(
        s.card(CARD_A).refused,
        0,
        "vtium swapped nothing without master"
    );
    assert_eq!(s.restores.get(), 0);
    // setForeground while the program still holds the card: the wake-up
    // fails, and the card keeps trying.
    s.tell("left", 1, "fg");
    assert!(s.until(|a| console_shows(a, "left", 1, "sent:fg")));
    s.turns(5);
    assert_eq!(s.restores.get(), 1, "vtium put the VT's modes back");
    assert!(!s.card(CARD_A).awake, "the program still holds card A");
    // The program lets go, and the next retry takes the card back.
    s.master_a.borrow_mut().drop_master(Holder::Program);
    let left_a = s.frames(CARD_A, A1);
    s.card(CARD_A).retry();
    assert!(
        s.until_frames(CARD_A, A1, left_a),
        "left draws on card A again"
    );
    let left_frames = s.card(CARD_A).output(A1).map(|o| o.frames.clone()).unwrap();
    assert_eq!(
        left_frames.last(),
        Some(&0),
        "the first frame after draws whole"
    );
    // The keys reach left again, and the one typed while paused never did.
    s.type_on(KBD_LEFT, "after\r");
    assert!(s.until(|a| console_shows(a, "left", 1, "got:after")));
    assert!(!console_shows(&s.app, "left", 1, "got:k"));
}

/// Whether vtium still holds card A and draws on it, so the program
/// cannot take it.
fn card_a_stays_vtiums(s: &mut Seat) {
    assert_eq!(s.master_a.borrow().holder, Some(Holder::Vtium));
    assert!(s.card(CARD_A).awake);
    assert!(
        s.master_a.borrow_mut().set(Holder::Program).is_err(),
        "the program cannot take card A"
    );
}

/// D16: vtium ignores setBackground from a console that no head shows.
/// Console 2 of `left` is hidden behind console 1, so its setBackground
/// gives up nothing, and the keys still reach `left`.
// covers: platform.hand-back
#[test]
fn a_hidden_consoles_set_background_is_ignored() {
    let mut s = Seat::boot("hand-over-hidden", &[], false);
    s.tell("left", 2, "bg");
    assert!(s.until(|a| console_shows(a, "left", 2, "sent:bg")));
    s.turns(5);
    card_a_stays_vtiums(&mut s);
    s.type_on(KBD_LEFT, "k\r");
    assert!(
        s.until(|a| console_shows(a, "left", 1, "got:k")),
        "left takes its keys"
    );
}

/// D16: under a seat manager, vtium ignores setBackground, so the card
/// stays vtium's and the keys reach the head.
// covers: platform.hand-back
#[test]
fn under_a_seat_manager_set_background_is_ignored() {
    let mut s = Seat::boot("hand-over-seat-manager", &[], true);
    s.tell("left", 1, "bg");
    assert!(s.until(|a| console_shows(a, "left", 1, "sent:bg")));
    s.turns(5);
    card_a_stays_vtiums(&mut s);
    s.type_on(KBD_LEFT, "k\r");
    assert!(
        s.until(|a| console_shows(a, "left", 1, "got:k")),
        "left takes its keys"
    );
}

/// ADR-1000, "Replies and policy": with `allow-display-ops` off, vtium
/// ignores setBackground.
// covers: osc.kmscon-display
#[test]
fn with_allow_display_ops_off_set_background_is_ignored() {
    let mut s = Seat::boot("hand-over-off", &["--no-allow-display-ops"], false);
    s.tell("left", 1, "bg");
    assert!(s.until(|a| console_shows(a, "left", 1, "sent:bg")));
    s.turns(5);
    card_a_stays_vtiums(&mut s);
}

/// D16: the VT switch keys keep working during a hand-over, from the
/// paused head's own keyboard, and vtium's VT shown again ends the
/// hand-over, as when the program quit without setForeground: the acquire
/// takes card A back, and the keys reach `left` again.
// covers: platform.hand-back
#[test]
fn the_vt_shown_again_ends_the_hand_over() {
    let mut s = Seat::boot("hand-over-vt-switch", &[], false);
    s.left_sends_background();
    // The program takes the card, draws and quits without setForeground.
    s.master_a
        .borrow_mut()
        .set(Holder::Program)
        .expect("the program takes card A");
    s.master_a.borrow_mut().drop_master(Holder::Program);
    let released = s.released.get();
    s.app.device_input(Path::new(KBD_LEFT), switch_key(), now());
    let count = Rc::clone(&s.released);
    assert!(
        s.until(|_| count.get() > released),
        "the paused head's keyboard switched the VT away"
    );
    s.app.device_input(Path::new(KBD_LEFT), switch_key(), now());
    let master = Rc::clone(&s.master_a);
    assert!(
        s.until(|a| a.vt.is_active() && master.borrow().holder == Some(Holder::Vtium)),
        "the switch back takes card A back"
    );
    assert_eq!(s.restores.get(), 0, "the acquire sets the VT up itself");
    s.type_on(KBD_LEFT, "back\r");
    assert!(s.until(|a| console_shows(a, "left", 1, "got:back")));
}

/// D16: the end of the session that sent setBackground ends the
/// hand-over, and the console's next session draws on card A.
// covers: platform.hand-back
#[test]
fn the_senders_session_ending_ends_the_hand_over() {
    let mut s = Seat::boot("hand-over-session-end", &[], false);
    s.left_sends_background();
    let left_a = s.frames(CARD_A, A1);
    s.tell("left", 1, "exit");
    let master = Rc::clone(&s.master_a);
    assert!(
        s.until(|_| master.borrow().holder == Some(Holder::Vtium)),
        "the session's end takes card A back"
    );
    assert_eq!(s.restores.get(), 1, "vtium put the VT's modes back");
    assert!(
        s.until_frames(CARD_A, A1, left_a),
        "left draws on card A again"
    );
    s.type_on(KBD_LEFT, "k\r");
    assert!(s.until(|a| console_shows(a, "left", 1, "got:k")));
}

/// D16: vtium ignores setForeground from any console but the one that
/// sent setBackground: here the hidden console of the same head, and the
/// shown console of another head.
// covers: platform.hand-back
#[test]
fn set_foreground_from_another_console_is_ignored() {
    let mut s = Seat::boot("hand-over-other-foreground", &[], false);
    s.left_sends_background();
    s.tell("left", 2, "fg");
    s.tell("right", 1, "fg");
    assert!(s.until(
        |a| console_shows(a, "left", 2, "sent:fg") && console_shows(a, "right", 1, "sent:fg")
    ));
    s.turns(5);
    assert_eq!(s.master_a.borrow().holder, None, "card A stays given up");
    assert!(!s.card(CARD_A).awake);
    s.type_on(KBD_LEFT, "k\r");
    s.turns(10);
    assert!(
        !console_shows(&s.app, "left", 1, "got:k"),
        "left stays paused"
    );
}

/// D5 and D16: the roaming chord never makes a wholly paused head the
/// active head. From `left`, which a hand-over pauses, the chord steps to
/// `right` and `other`, and from `other` past `left` to `right`.
// covers: platform.hand-back
#[test]
fn the_roaming_chord_skips_a_wholly_paused_head() {
    let mut s = Seat::boot("hand-over-roaming", &[], false);
    s.left_sends_background();
    let mut moved = Vec::new();
    for _ in 0..3 {
        s.app
            .device_input(Path::new(KBD_ROAMING), ctrl_super(UP), now());
        moved.push(s.app.take_focus_move());
    }
    let names = |n: &[&str]| n.iter().map(|n| Some((*n).to_owned())).collect::<Vec<_>>();
    assert_eq!(moved, names(&["right", "other", "right"]));
}

/// D16: a card that two hand-overs hold stays given up until both end.
/// `left` gives up card A; `right`, which still draws on card B, gives up
/// cards A and B. The end of `left`'s hand-over keeps card A given up, and
/// the end of `right`'s wakes both cards.
// covers: platform.hand-back
#[test]
fn a_card_two_hand_overs_hold_stays_given_up_until_both_end() {
    let mut s = Seat::boot("hand-over-overlap", &[], false);
    s.left_sends_background();
    s.tell("right", 1, "bg");
    let master_b = Rc::clone(&s.master_b);
    assert!(
        s.until(|a| console_shows(a, "right", 1, "sent:bg") && master_b.borrow().holder.is_none()),
        "right's hand-over gives up card B"
    );
    s.tell("left", 1, "fg");
    assert!(s.until(|a| console_shows(a, "left", 1, "sent:fg")));
    s.turns(5);
    assert_eq!(s.master_a.borrow().holder, None, "right still holds card A");
    assert!(!s.card(CARD_A).awake);
    let (left_a, other_b) = (s.frames(CARD_A, A1), s.frames(CARD_B, B1));
    s.tell("right", 1, "fg");
    let (master_a, master_b) = (Rc::clone(&s.master_a), Rc::clone(&s.master_b));
    assert!(
        s.until(|_| master_a.borrow().holder == Some(Holder::Vtium)
            && master_b.borrow().holder == Some(Holder::Vtium)),
        "the end of right's hand-over wakes both cards"
    );
    assert!(s.until_frames(CARD_A, A1, left_a), "left draws on card A");
    assert!(s.until_frames(CARD_B, B1, other_b), "other draws on card B");
    assert_eq!(s.card(CARD_A).refused + s.card(CARD_B).refused, 0);
}

/// D16: a card removed and added again at the same place during a
/// hand-over stays asleep, and the hand-over's end wakes it.
// covers: platform.hand-back
#[test]
fn a_card_added_again_during_a_hand_over_stays_asleep_until_it_ends() {
    let mut s = Seat::boot("hand-over-re-added", &[], false);
    s.left_sends_background();
    s.app.remove_video(Path::new(CARD_A));
    let mut again = Card::new("card-a", &["A-1", "A-2"]);
    again.master = Rc::clone(&s.master_a);
    s.app.add_video(PathBuf::from(CARD_A), Box::new(again));
    s.turns(5);
    assert_eq!(
        s.master_a.borrow().holder,
        None,
        "the card added again stays given up"
    );
    assert!(!s.card(CARD_A).awake);
    s.tell("left", 1, "fg");
    let master = Rc::clone(&s.master_a);
    assert!(
        s.until(|_| master.borrow().holder == Some(Holder::Vtium)),
        "the hand-over's end wakes the card"
    );
    assert!(s.until_frames(CARD_A, A1, 0), "left draws on it");
}

/// D16: a re-take of the VT while a hand-over lasts, as input on the
/// kernel VT's tty brings, sets up no given-up card again.
// covers: platform.hand-back
#[test]
fn a_re_take_of_the_vt_wakes_no_given_up_card() {
    let mut s = Seat::boot("hand-over-retake", &[], false);
    s.left_sends_background();
    s.vt_sent.borrow_mut().push(VtEvent::Retaken);
    s.turns(5);
    assert_eq!(s.master_a.borrow().holder, None, "card A stays given up");
    assert!(!s.card(CARD_A).awake);
    assert_eq!(s.master_b.borrow().holder, Some(Holder::Vtium));
}

/// D16: a setBackground is judged against the console shown when it came.
/// Console 2 of `left`, hidden, sends it, and a console chord shows console
/// 2 before the seat's turn takes the request: vtium ignores it.
// covers: platform.hand-back
#[test]
fn a_hidden_consoles_set_background_stays_ignored_when_a_chord_shows_it() {
    let mut s = Seat::boot("hand-over-chord-shows", &[], false);
    s.tell("left", 2, "bg");
    assert!(s.read_until_requested("left"), "console 2's request waits");
    s.app
        .device_input(Path::new(KBD_LEFT), ctrl_super(RIGHT), now());
    s.turns(5);
    assert_eq!(s.app.head("left").map(|h| h.shown_console()), Some(1));
    card_a_stays_vtiums(&mut s);
}

/// D16: a setBackground from the shown console acts though a console chord
/// comes before the seat's turn takes it; the hand-over pauses the head,
/// so the chord shows no other console.
// covers: platform.hand-back
#[test]
fn a_shown_consoles_set_background_acts_before_a_chord_hides_it() {
    let mut s = Seat::boot("hand-over-chord-hides", &[], false);
    s.tell("left", 1, "bg");
    assert!(s.read_until_requested("left"), "console 1's request waits");
    s.app
        .device_input(Path::new(KBD_LEFT), ctrl_super(RIGHT), now());
    let master = Rc::clone(&s.master_a);
    assert!(
        s.until(|_| master.borrow().holder.is_none()),
        "card A is given up"
    );
    assert_eq!(s.app.head("left").map(|h| h.shown_console()), Some(0));
}

/// D16: a setForeground that comes while card A's sleep still waits, as
/// drm2d's waits for a flip in flight, cancels the sleep. Card A keeps DRM
/// master and wakes, so the display draws and `left` takes its keys, and
/// the program never had a moment to take the card.
// covers: platform.hand-back
#[test]
fn a_set_foreground_before_the_card_is_asleep_keeps_the_card() {
    let mut s = Seat::boot_with("hand-over-before-asleep", &[], false, 50);
    let left_a = s.frames(CARD_A, A1);
    s.tell("left", 1, "bg");
    let card_asleep = |s: &mut Seat| !s.card(CARD_A).awake;
    let end = now() + BOUND;
    while !card_asleep(&mut s) && now() < end {
        s.turns(1);
    }
    assert!(card_asleep(&mut s), "the sleep started");
    assert_eq!(s.master_a.borrow().holder, Some(Holder::Vtium), "and waits");
    s.tell("left", 1, "fg");
    assert!(s.until(|a| console_shows(a, "left", 1, "sent:fg")));
    s.turns(60);
    assert!(
        s.card(CARD_A).awake,
        "card A is awake after the sleep's time"
    );
    assert_eq!(s.master_a.borrow().holder, Some(Holder::Vtium));
    assert!(s.until_frames(CARD_A, A1, left_a), "left draws");
    s.type_on(KBD_LEFT, "k\r");
    assert!(s.until(|a| console_shows(a, "left", 1, "got:k")));
    assert_eq!(s.card(CARD_A).refused, 0);
}

/// D16 and D5: the shown console of a head that setBackground wholly
/// pauses gets a focus-out, and a focus-in when setForeground ends the
/// hand-over. `right` pauses on card A only and keeps drawing on card B,
/// so it keeps its focus, and `other` draws on and keeps its focus.
// covers: platform.hand-back
#[test]
fn a_wholly_paused_head_gets_a_focus_out_and_a_focus_in_at_the_end() {
    let mut s = Seat::boot_reporting("hand-over-focus", false);
    s.report_focus(&["left", "right", "other"]);
    s.left_sends_background();
    assert!(
        s.until(|a| every_report(a) == [lines(&[OUT]), lines(&[]), lines(&[])]),
        "left lost the focus, and the others kept theirs: {:?}",
        every_report(&s.app)
    );
    s.tell("left", 1, "fg");
    assert!(
        s.until(|a| every_report(a) == [lines(&[OUT, IN]), lines(&[]), lines(&[])]),
        "left has the focus again: {:?}",
        every_report(&s.app)
    );
    s.turns(10);
    assert_eq!(
        every_report(&s.app),
        [lines(&[OUT, IN]), lines(&[]), lines(&[])],
        "no report more"
    );
}

/// D16: every wholly paused head gets a focus-out, not only the one that
/// sent setBackground. `right` gives up cards A and B, which pauses all
/// three heads. The end of `right`'s session ends the hand-over, and the
/// two heads whose sessions run on get a focus-in.
// covers: platform.hand-back
#[test]
fn each_head_a_hand_over_pauses_gets_a_focus_out() {
    let mut s = Seat::boot_reporting("hand-over-focus-every", false);
    s.report_focus(&["left", "right", "other"]);
    s.tell("right", 1, "bg");
    assert!(
        s.until(|a| every_report(a) == [lines(&[OUT]), lines(&[OUT]), lines(&[OUT])]),
        "every head lost the focus: {:?}",
        every_report(&s.app)
    );
    s.tell("right", 1, "exit");
    assert!(
        s.until(|a| {
            let [left, _, other] = every_report(a);
            [left, other] == [lines(&[OUT, IN]), lines(&[OUT, IN])]
        }),
        "the session's end gives left and other the focus again: {:?}",
        every_report(&s.app)
    );
}

/// D5: at a VT release every shown console that has the focus gets a
/// focus-out, and at the acquire a focus-in. Under a seat manager, its
/// disable and enable count as the release and the acquire.
// covers: platform.heads.focus
#[test]
fn a_vt_switch_sends_a_focus_out_and_a_focus_in_back() {
    for seat_manager in [false, true] {
        let mut s = Seat::boot_reporting("hand-over-focus-vt", seat_manager);
        s.report_focus(&["left", "right", "other"]);
        let released = s.released.get();
        s.app.device_input(Path::new(KBD_LEFT), switch_key(), now());
        let count = Rc::clone(&s.released);
        assert!(s.until(|_| count.get() > released), "the VT switched away");
        assert!(
            s.until(|a| every_report(a) == [lines(&[OUT]), lines(&[OUT]), lines(&[OUT])]),
            "every head lost the focus, under a seat manager {seat_manager}: {:?}",
            every_report(&s.app)
        );
        s.app.device_input(Path::new(KBD_LEFT), switch_key(), now());
        let both = lines(&[OUT, IN]);
        assert!(
            s.until(|a| a.vt.is_active()
                && every_report(a) == [both.clone(), both.clone(), both.clone()]),
            "every head has the focus again, under a seat manager {seat_manager}: {:?}",
            every_report(&s.app)
        );
    }
}

/// D5 and D16: a head that a hand-over paused lost its focus already, so
/// the VT release that follows sends it nothing. The acquire ends the
/// hand-over, and every head gets one focus-in.
// covers: platform.hand-back
#[test]
fn a_vt_switch_during_a_hand_over_sends_each_report_once() {
    let mut s = Seat::boot_reporting("hand-over-focus-vt-paused", false);
    s.report_focus(&["left", "right", "other"]);
    s.left_sends_background();
    assert!(s.until(|a| every_report(a) == [lines(&[OUT]), lines(&[]), lines(&[])]));
    let released = s.released.get();
    s.app.device_input(Path::new(KBD_LEFT), switch_key(), now());
    let count = Rc::clone(&s.released);
    assert!(s.until(|_| count.get() > released), "the VT switched away");
    assert!(
        s.until(|a| every_report(a) == [lines(&[OUT]), lines(&[OUT]), lines(&[OUT])]),
        "right and other lost the focus: {:?}",
        every_report(&s.app)
    );
    s.app.device_input(Path::new(KBD_LEFT), switch_key(), now());
    let both = lines(&[OUT, IN]);
    assert!(
        s.until(
            |a| a.vt.is_active() && every_report(a) == [both.clone(), both.clone(), both.clone()]
        ),
        "every head has the focus again: {:?}",
        every_report(&s.app)
    );
    s.turns(10);
    assert_eq!(
        every_report(&s.app),
        [both.clone(), both.clone(), both],
        "no report more"
    );
}

/// D16: under a seat manager, vtium ignores setBackground and sends no
/// focus report.
// covers: platform.hand-back
#[test]
fn under_a_seat_manager_set_background_sends_no_focus_report() {
    let mut s = Seat::boot_reporting("hand-over-focus-seat-manager", true);
    s.report_focus(&["left"]);
    s.tell("left", 1, "bg");
    assert!(s.until(|a| console_shows(a, "left", 1, "sent:bg")));
    s.turns(10);
    assert_eq!(reports(&s.app, "left"), lines(&[]));
}
