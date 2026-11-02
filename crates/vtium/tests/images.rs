//! Pictures in a build with images (ADR-1100 D5, D7, D9, D10, D11): the
//! seat booted with displays in memory, on the fake VT, with a real child
//! on a real pty. The child writes sixel and kitty graphics strings and
//! reads the terminal's replies, and the tests read what the display shows
//! and what the child printed.
#![cfg(feature = "images")]

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
use vtium::head::images::Helper;
use vtium::head::{Head, Start};
use vtium::video::Video;
use vtium_image::decode::{Medium, Payload, Reply, Request};
use vtium_platform::display::{
    DisplayId, Dpms, Event, Identity, Memory as Shared, Rect, SwapError, Swapped, Token,
};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::FakeVt;

const ID: DisplayId = DisplayId(3);

/// How long a wait on the child or a frame may run before the test fails:
/// only a bound, since each wait ends at the state it waits for.
const BOUND: Duration = Duration::from_secs(10);

/// A device with one display, its frame buffer in memory. It holds the
/// seat to what a framebuffer holds it to: one token lent at a time, a
/// token from before a sleep or a wake-up comes back too late and is
/// dropped, a dropped token leaves the buffer at age 0, and a frame that
/// changed nothing is not shown.
struct Memory {
    w: u32,
    h: u32,
    card: &'static str,
    /// What the display shows: the buffer lent, as the last swap left it.
    pixels: Vec<u8>,
    /// The buffer each token lends, drawn into on the render thread.
    drawn: Arc<Mutex<Vec<u8>>>,
    events: Vec<Event>,
    awake: bool,
    dpms: Option<Dpms>,
    frames: u32,
    age: u32,
    lease: u64,
    lent: bool,
}

impl Memory {
    fn new(w: u32, h: u32, card: &'static str) -> Memory {
        Memory {
            w,
            h,
            card,
            pixels: Vec::new(),
            drawn: Arc::default(),
            events: Vec::new(),
            awake: false,
            dpms: None,
            frames: 0,
            age: 0,
            lease: 0,
            lent: false,
        }
    }

    /// The pixels the display shows of `rgb`.
    fn count(&self, rgb: u32) -> usize {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]) & 0x00ff_ffff == rgb)
            .count()
    }
}

impl Video for Memory {
    fn backend(&self) -> &'static str {
        "memory"
    }
    fn events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    fn displays(&self) -> Vec<DisplayId> {
        if self.awake { vec![ID] } else { Vec::new() }
    }
    fn name(&self, _: DisplayId) -> String {
        "MEM-1".into()
    }
    fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
        Some((self.w, self.h))
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(60_000)
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        // the C reference's fixed density, where a point is a pixel
        Some((72.0, 72.0))
    }
    fn identity(&self, _: DisplayId) -> Option<Identity> {
        Some(Identity {
            connector: "MEM-1".to_owned(),
            card: Some(self.card.to_owned()),
            edid: None,
            connector_path: None,
            lit_at_start: false,
        })
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        if !self.ready_for_frame(id) {
            return None;
        }
        if std::mem::take(&mut self.lent) {
            // dropped without being given back
            self.age = 0;
        }
        self.lent = true;
        let memory: Arc<Shared> = self.drawn.clone();
        Some(Token::new(id, (self.w, self.h), self.w * 4, self.age, memory).leased(self.lease))
    }
    fn give_back(&mut self, token: Token) {
        self.lent = false;
        drop(token);
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let id = token.id();
        self.lent = false;
        if token.lease() != self.lease {
            drop(token);
            self.age = 0;
            return Err(SwapError::Dropped);
        }
        if damage.is_empty() && self.dpms == Some(Dpms::On) {
            return Ok(Swapped::Nothing);
        }
        if self.dpms != Some(Dpms::On) {
            return Err(SwapError::Off);
        }
        self.age = 1;
        self.frames += 1;
        self.pixels.clone_from(&self.drawn.lock().unwrap());
        self.events.push(Event::Flipped(id));
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn ready_for_frame(&self, _: DisplayId) -> bool {
        self.awake && self.dpms == Some(Dpms::On) && Arc::strong_count(&self.drawn) == 1
    }
    fn set_dpms(&mut self, _: DisplayId, dpms: Dpms) -> io::Result<()> {
        if dpms != Dpms::On && self.dpms == Some(Dpms::On) {
            self.lease += 1;
        }
        self.dpms = Some(dpms);
        if dpms == Dpms::On {
            self.age = 0;
            self.events.push(Event::Redraw(ID));
        }
        Ok(())
    }
    fn sleep(&mut self) {
        self.lease += 1;
        self.awake = false;
        self.events.push(Event::Asleep);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.lease += 1;
        let first = self.pixels.is_empty();
        self.pixels = vec![0; (self.w * self.h * 4) as usize];
        if Arc::strong_count(&self.drawn) == 1 {
            *self.drawn.lock().unwrap() = self.pixels.clone();
        }
        self.awake = true;
        self.dpms = Some(Dpms::On);
        self.age = 0;
        self.events.push(if first {
            Event::Added(ID)
        } else {
            Event::Redraw(ID)
        });
        self.events.push(Event::Awake);
        Ok(())
    }
    fn unblank(&mut self) -> io::Result<()> {
        self.set_dpms(ID, Dpms::On)
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

/// What a stand-in image helper was asked and answers: the requests it
/// was sent, the reply the test gives it, the helpers started and the
/// helpers stopped. Like the real helper it serves every request of its
/// console, one at a time, and answers only once the console asks.
#[derive(Default)]
struct Asked {
    sent: Vec<Request>,
    reply: Option<Reply>,
    /// A request was sent and not yet answered.
    out: bool,
    started: u32,
    stopped: u32,
}

struct StandIn(Rc<RefCell<Asked>>);

impl Helper for StandIn {
    fn send(&mut self, request: Request) {
        let mut asked = self.0.borrow_mut();
        assert!(!asked.out, "one request out at a time");
        asked.out = true;
        asked.sent.push(request);
    }
    fn reply(&mut self) -> Option<Reply> {
        let mut asked = self.0.borrow_mut();
        if !asked.out {
            return None;
        }
        let reply = asked.reply.take();
        asked.out = reply.is_none();
        reply
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        let mut asked = self.0.borrow_mut();
        asked.stopped += 1;
        asked.out = false;
    }
}

/// A picture of 8x6 pixels in pure red, as the helper decodes it.
fn red_picture() -> Reply {
    Reply::Ok(Payload::Rgba {
        width: 8,
        height: 6,
        data: [255, 0, 0, 255].repeat(48),
    })
}

#[expect(
    clippy::disallowed_methods,
    reason = "the harness runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

/// No frits and no vconsole font, so the seat draws its built-in font.
fn no_fonts() -> Sources {
    Sources {
        frits: PathBuf::from("/nonexistent/vtium-test-frits"),
        helper: None,
        vconsole_conf: PathBuf::from("/dev/null"),
        kernel_cmdline: PathBuf::from("/dev/null"),
    }
}

struct Seat {
    lp: Loop<App>,
    app: App,
    /// How far the seat's clock runs ahead of the real one.
    ahead: Rc<Cell<Duration>>,
}

impl Seat {
    /// The seat booted with `args` at 16 points, 16-pixel cells at 72 DPI,
    /// on `memory` as `/dev/mem0`, under the head file `heads` when given,
    /// its session running `script`, with the stand-in image helper
    /// `helper` when given.
    fn boot(
        args: &[&str],
        heads: Option<&str>,
        memory: Memory,
        script: &str,
        helper: Option<&Rc<RefCell<Asked>>>,
    ) -> Seat {
        let args: Vec<OsString> = ["--font-size=16"]
            .iter()
            .chain(args)
            .chain(&["--login", "--", "/bin/sh", "-c", script])
            .map(OsString::from)
            .collect();
        let loaded = config::load(&args, None, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the seat's command line");
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let ahead = Rc::new(Cell::new(Duration::ZERO));
        let clock = {
            let ahead = ahead.clone();
            Rc::new(move || now() + ahead.get())
        };
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            clock,
            env,
        );
        app.set_font_sources(no_fonts());
        if let Some(text) = heads {
            let start = Start::trusted(Path::new("heads.conf"), text.as_bytes());
            assert_eq!(start.line(), None, "a valid head file");
            app.set_heads(start);
        }
        if let Some(asked) = helper {
            let asked = Rc::clone(asked);
            app.set_image_helpers(Some(Rc::new(move || {
                asked.borrow_mut().started += 1;
                Box::new(StandIn(Rc::clone(&asked))) as Box<dyn Helper>
            })));
        }
        app.add_video(PathBuf::from("/dev/mem0"), Box::new(memory));
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Seat { lp, app, ahead }
    }

    fn at(&self) -> Instant {
        now() + self.ahead.get()
    }

    /// One turn of the loop: what is ready is dispatched, then the turn.
    fn turn(&mut self) {
        let wait = self
            .app
            .next_wake(self.at())
            .unwrap_or(Duration::from_millis(20))
            .min(Duration::from_millis(20));
        self.lp
            .dispatch(Some(wait), &mut self.app)
            .expect("dispatch");
        self.app.turn(self.at());
    }

    /// Turns the loop until `done` holds, at most [`BOUND`].
    fn until(&mut self, mut done: impl FnMut(&mut Seat) -> bool) -> bool {
        let end = now() + BOUND;
        while now() < end {
            if done(self) {
                return true;
            }
            self.turn();
        }
        done(self)
    }

    fn memory(&mut self, node: &str) -> &mut Memory {
        self.app
            .videos
            .get_mut(Path::new(node))
            .and_then(|v| v.as_any().downcast_mut::<Memory>())
            .expect("a memory device")
    }

    fn shows(&self, text: &str) -> bool {
        self.app.screen_text().iter().any(|l| l.contains(text))
    }

    /// The shown console's image budget.
    fn limit(&self) -> u64 {
        self.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .images()
            .limit()
    }

    fn placements(&self) -> usize {
        self.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .images()
            .placement_count()
    }
}

/// Pure red and pure green, the colors of the sixels the tests draw.
const RED: u32 = 0x00ff_0000;
const GREEN: u32 = 0x0000_ff00;

/// A sixel 8 dots wide and 6 high, every dot set in pure red.
const SIXEL: &str = r"\033Pq#1;2;100;0;0#1~~~~~~~~\033\\";

/// A script that reads what the terminal answers, up to `end`, a byte at a
/// time with no echo, and prints it with ESC shown as `E`.
fn read_reply(end: &str) -> String {
    format!(
        "s=; i=0; while [ $i -lt 64 ]; do c=$(dd bs=1 count=1 2>/dev/null); s=\"$s$c\"; \
         i=$((i+1)); [ \"$c\" = '{end}' ] && break; done; printf '%s\\n' \"$s\" | tr '\\033' E; "
    )
}

/// The child asks DA1, prints the answer, then draws [`SIXEL`] below it,
/// with the cursor hidden so that no cursor covers the picture, and prints
/// `drawn` after it.
fn da1_then_sixel() -> String {
    format!(
        "stty -echo -icanon min 1 time 0; printf '\\033[?25l\\033[c'; {}printf '{SIXEL}\\ndrawn'; read x",
        read_reply("c")
    )
}

/// D9, D10: in a build with images, a console offers sixel, its handler
/// draws what the program sends, and the head's snapshot carries the
/// picture to the display: DA1 reports attribute 4, and the display shows
/// all 48 of the sixel's red pixels at their own size.
// covers: image.pipeline
#[test]
fn a_sixel_the_program_writes_shows_on_the_display() {
    let mut s = Seat::boot(
        &[],
        None,
        Memory::new(320, 160, "mem"),
        &da1_then_sixel(),
        None,
    );
    assert!(
        s.until(|s| s.shows("E[?62;4;22c")),
        "{:?}",
        s.app.screen_text()
    );
    assert!(
        s.until(|s| s.memory("/dev/mem0").count(RED) == 48),
        "the display shows {} red pixels",
        s.memory("/dev/mem0").count(RED)
    );
    assert_eq!(s.placements(), 1);
}

/// A key that types `c`.
fn char_key(c: char) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        mods: Mods::empty(),
        keysyms: vec![u32::from(c)],
        codepoints: vec![Some(c)],
        repeat: false,
    })
}

/// A key with `mods` held, its keysym `keysym`.
fn chord(mods: Mods, keysym: u32) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        mods,
        keysyms: vec![keysym],
        codepoints: vec![None],
        repeat: false,
    })
}

/// Ctrl-= zooms the console shown in; Ctrl-Super-Right shows the next.
fn zoom_in() -> InputEvent {
    chord(Mods::CTRL, u32::from('='))
}

fn next_console() -> InputEvent {
    chord(Mods::CTRL.union(Mods::LOGO), 0xff53)
}

/// The child prints `ready` and its console's number, waits for a key,
/// then draws [`SIXEL`]'s shape in pure red in console 1 and pure green in
/// any other.
const AFTER_A_KEY: &str = "stty -echo -icanon min 1 time 0; printf \"ready$VTIUM_CONSOLE\\n\"; \
     dd bs=1 count=1 >/dev/null 2>&1; \
     if [ \"$VTIUM_CONSOLE\" = 1 ]; then c='100;0;0'; else c='0;100;0'; fi; \
     printf \"\\033[?25l\\033Pq#1;2;$c#1~~~~~~~~\\033\\\\\"; read x";

/// The cell size is a host event: a picture is placed in the cell its
/// console has when it arrives, and a zoom after it draws it scaled to the
/// new cell, 16x12 pixels for the 8x6 sixel made in 8x16 cells and drawn
/// in 16x32 ones.
#[test]
fn a_sixel_is_placed_in_its_consoles_cell_and_scaled_by_a_zoom() {
    let mut s = Seat::boot(&[], None, Memory::new(320, 160, "mem"), AFTER_A_KEY, None);
    assert!(s.until(|s| s.shows("ready1")), "{:?}", s.app.screen_text());
    s.app.input_event(zoom_in(), s.at());
    assert_eq!(s.app.cells(), [(16, 32)]);
    // The resize gives the store the new cell with the terminal's, before
    // any turn.
    let cell = |s: &Seat| {
        s.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .images()
            .cell()
    };
    assert_eq!(cell(&s), (16, 32));
    s.app.input_event(char_key('g'), s.at());
    assert!(s.until(|s| s.placements() == 1));
    assert!(
        s.until(|s| s.memory("/dev/mem0").count(RED) == 48),
        "placed in 16x32 cells, the sixel draws at its own size: {} red pixels",
        s.memory("/dev/mem0").count(RED)
    );
    let mut s = Seat::boot(&[], None, Memory::new(320, 160, "mem"), AFTER_A_KEY, None);
    assert!(s.until(|s| s.shows("ready1")));
    s.app.input_event(char_key('g'), s.at());
    assert!(s.until(|s| s.memory("/dev/mem0").count(RED) == 48));
    s.app.input_event(zoom_in(), s.at());
    assert!(
        s.until(|s| s.memory("/dev/mem0").count(RED) == 16 * 12),
        "zoomed, it draws twice its size: {} red pixels",
        s.memory("/dev/mem0").count(RED)
    );
}

/// A renderer draws one console's store, whose image keys another
/// console's store reuses, so a console switch draws with a new renderer:
/// each console's sixel is image 0 of its store, both drawn zoomed from a
/// scaled copy of the same size, and the first console shows its own red
/// picture again, never the second's green copy.
#[test]
fn a_console_switch_draws_each_consoles_own_pictures() {
    let heads = "version = 1\n[head main]\nconsoles = 2\n[rules]\ndisplay card:mem -> main\n";
    let mut s = Seat::boot(
        &[],
        Some(heads),
        Memory::new(320, 160, "mem"),
        AFTER_A_KEY,
        None,
    );
    let both = |s: &mut Seat, red: usize, green: usize| {
        s.until(|s| {
            let m = s.memory("/dev/mem0");
            m.count(RED) == red && m.count(GREEN) == green
        })
    };
    assert!(s.until(|s| s.shows("ready1")), "{:?}", s.app.screen_text());
    s.app.input_event(char_key('g'), s.at());
    assert!(both(&mut s, 48, 0));
    s.app.input_event(zoom_in(), s.at());
    assert!(both(&mut s, 192, 0), "console 1's picture, zoomed");
    s.app.input_event(next_console(), s.at());
    assert!(s.until(|s| s.shows("ready2")), "{:?}", s.app.screen_text());
    s.app.input_event(char_key('g'), s.at());
    assert!(both(&mut s, 0, 48));
    s.app.input_event(zoom_in(), s.at());
    assert!(both(&mut s, 0, 192), "console 2's picture, zoomed");
    s.app.input_event(next_console(), s.at());
    assert!(
        both(&mut s, 192, 0),
        "console 1 shows its own picture: {} red and {} green pixels",
        s.memory("/dev/mem0").count(RED),
        s.memory("/dev/mem0").count(GREEN)
    );
}

/// D10, D11: `no-sixel` offers no sixel, so DA1 reports no attribute 4
/// and the string is ignored; `image-storage-limit=0` turns both
/// protocols off. Neither draws a picture.
#[test]
fn no_sixel_and_a_limit_of_0_offer_no_sixel_and_draw_no_picture() {
    for args in [&["--no-sixel"][..], &["--image-storage-limit=0"][..]] {
        let mut s = Seat::boot(
            args,
            None,
            Memory::new(320, 160, "mem"),
            &da1_then_sixel(),
            None,
        );
        assert!(
            s.until(|s| s.shows("E[?62;22c")),
            "{args:?}: {:?}",
            s.app.screen_text()
        );
        // `drawn` follows the sixel in the stream, so once it shows the
        // string has been read, and the store holds no picture of it.
        assert!(s.until(|s| s.shows("drawn") && s.memory("/dev/mem0").frames > 0));
        assert_eq!(s.placements(), 0, "{args:?}");
        for _ in 0..20 {
            s.turn();
        }
        assert_eq!(s.memory("/dev/mem0").count(RED), 0, "{args:?}");
    }
}

/// D5: under `auto`, each console's budget is four frames of its head's
/// displays, at least 16 MiB, worked out again when a display joins or
/// leaves; a limit the configuration sets applies as set.
#[test]
fn the_auto_budget_follows_the_heads_displays_and_a_set_limit_holds() {
    let mut s = Seat::boot(&[], None, Memory::new(1500, 1000, "mem"), "read x", None);
    let one = 1500 * 1000 * 4 * 4;
    assert!(s.until(|s| s.limit() == one), "{}", s.limit());
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(1500, 1000, "mem1")),
    );
    assert!(s.until(|s| s.limit() == 2 * one), "{}", s.limit());
    s.app.remove_video(Path::new("/dev/mem1"));
    assert!(s.until(|s| s.limit() == one), "{}", s.limit());
    let mut s = Seat::boot(&[], None, Memory::new(320, 160, "mem"), "read x", None);
    assert!(s.until(|s| s.limit() == vtium_image::AUTO_FLOOR));
    let mut s = Seat::boot(
        &["--image-storage-limit=5M"],
        None,
        Memory::new(1500, 1000, "mem"),
        "read x",
        None,
    );
    assert!(s.until(|s| s.limit() == 5 << 20), "{}", s.limit());
}

/// A kitty transmission of a PNG file named `/x.png`, with image id 7, put
/// at the cursor, then `after` on a line of its own, in one write.
const KITTY_FILE: &str = r"\033_Ga=T,t=f,f=100,i=7;L3gucG5n\033\\after\n";

/// The child sends [`KITTY_FILE`], then prints the terminal's answer.
fn kitty_file() -> String {
    format!(
        "stty -echo -icanon min 1 time 0; printf '\\033[?25l{KITTY_FILE}'; {}read x",
        read_reply("\\")
    )
}

/// D7: with no image helper, which is how vtium runs until the helper is
/// built, a kitty file is answered at once with EBADF, and what follows it
/// is read on.
#[test]
fn with_no_helper_a_kitty_file_is_answered_ebadf_at_once() {
    let mut s = Seat::boot(&[], None, Memory::new(320, 160, "mem"), &kitty_file(), None);
    assert!(
        s.until(|s| s.shows("E_Gi=7;EBADF:Failed to read image file")),
        "{:?}",
        s.app.screen_text()
    );
    assert!(s.shows("after"));
    assert_eq!(s.placements(), 0);
}

/// D7: a request goes to the console's helper and its pty is paused, so
/// the bytes after the string wait for the reply however many turns pass;
/// the reply places the picture, the kitty handler's OK goes to the
/// program, and the bytes after it are read.
#[test]
fn a_request_pauses_the_console_until_its_helper_replies() {
    let asked = Rc::new(RefCell::new(Asked::default()));
    let mut s = Seat::boot(
        &[],
        None,
        Memory::new(320, 160, "mem"),
        &kitty_file(),
        Some(&asked),
    );
    assert!(
        s.until(|_| asked.borrow().sent.len() == 1),
        "a request was sent"
    );
    let sent = asked.borrow().sent[0].clone();
    assert_eq!(sent.medium, Medium::File(b"/x.png".to_vec()));
    assert!(sent.png);
    for _ in 0..50 {
        s.turn();
    }
    assert!(!s.shows("after"), "{:?}", s.app.screen_text());
    assert_eq!(s.placements(), 0);
    asked.borrow_mut().reply = Some(Reply::Ok(Payload::Rgba {
        width: 1,
        height: 1,
        data: vec![255, 0, 0, 255],
    }));
    assert!(
        s.until(|s| s.shows("after") && s.shows("E_Gi=7;OKE")),
        "{:?}",
        s.app.screen_text()
    );
    assert_eq!(s.placements(), 1);
    assert_eq!((asked.borrow().started, asked.borrow().stopped), (1, 0));
}

/// D7, ADR-1101 D8: a request the helper has not answered when the
/// 2,000 ms timer fires fails, the helper is stopped, the kitty handler
/// answers EBADF and the console reads on. The seat's clock moves ahead
/// past the timer, so the test waits on no real time.
#[test]
fn a_request_unanswered_for_two_seconds_fails_and_stops_the_helper() {
    let asked = Rc::new(RefCell::new(Asked::default()));
    let mut s = Seat::boot(
        &["--no-blink"],
        None,
        Memory::new(320, 160, "mem"),
        &kitty_file(),
        Some(&asked),
    );
    assert!(
        s.until(|_| asked.borrow().sent.len() == 1),
        "a request was sent"
    );
    for _ in 0..20 {
        s.turn();
    }
    assert!(!s.shows("after"));
    assert_eq!(asked.borrow().stopped, 0);
    // The loop wakes for the timer: 10 ms before it is due, the next wake
    // is at most those 10 ms away.
    let timeout = vtium::head::images::REPLY_TIMEOUT;
    s.ahead.set(timeout - Duration::from_millis(10));
    let wake = s.app.next_wake(s.at());
    assert!(
        wake.is_some_and(|w| w <= Duration::from_millis(10)),
        "{wake:?}"
    );
    s.ahead.set(timeout);
    assert!(
        s.until(|s| s.shows("after") && s.shows("E_Gi=7;EBADF:Failed to read image file")),
        "{:?}",
        s.app.screen_text()
    );
    assert_eq!(asked.borrow().stopped, 1);
    assert_eq!(s.placements(), 0);
}

/// ADR-1101 D8: a session that ends with a request out drops it with no
/// reply and stops its helper, so the reset that starts the next session
/// takes: here the session is hung up when its head loses its display,
/// and the next one finds the first session's text gone.
#[test]
fn a_session_ending_with_a_request_out_drops_it_and_the_next_starts_clean() {
    let dir = std::env::temp_dir().join(format!("vtium-images-hangup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let mark = dir.join("ran");
    let script = dir.join("login");
    std::fs::write(
        &script,
        format!(
            "if [ -e '{mark}' ]; then printf 'second\\n'; read x; fi\n\
             : > '{mark}'\n\
             printf 'first\\n{KITTY_FILE}'\n\
             read x\n",
            mark = mark.display()
        ),
    )
    .expect("the script");
    let heads = format!(
        "version = 1\n[head main]\nwhen-display-lost = hangup\nlogin = /bin/sh {}\n[rules]\ndisplay card:mem -> main\n",
        script.display()
    );
    let asked = Rc::new(RefCell::new(Asked::default()));
    let mut s = Seat::boot(
        &[],
        Some(&heads),
        Memory::new(320, 160, "mem"),
        "read x",
        Some(&asked),
    );
    let main = |s: &Seat, text: &str| {
        s.app
            .head("main")
            .is_some_and(|h| h.screen_text().iter().any(|l| l.contains(text)))
    };
    assert!(
        s.until(|_| asked.borrow().sent.len() == 1),
        "a request was sent"
    );
    assert!(main(&s, "first"));
    s.memory("/dev/mem0").events.push(Event::Removed(ID));
    assert!(s.until(|s| main(s, "second")), "the next session runs");
    assert!(!main(&s, "first"), "the reset took");
    assert_eq!(asked.borrow().stopped, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// ADR-1101 D8: any failure stops the helper, not the timer alone. A
/// FAILED reply, as from a lost socket, stops it, and so does a reply
/// that fails its checks, here a 2x2 picture holding one pixel's bytes;
/// each is answered EBADF and the console reads on. The next request
/// starts another helper.
#[test]
fn a_failed_reply_or_one_that_fails_its_checks_stops_the_helper() {
    let script = format!(
        "stty -echo -icanon min 1 time 0; \
         printf '\\033[?25l\\033_Ga=t,t=f,f=100,i=7;L3gucG5n\\033\\\\\\033_Ga=t,t=f,f=100,i=8;L3gucG5n\\033\\\\after\\n'; \
         {}{}read x",
        read_reply("\\"),
        read_reply("\\")
    );
    let asked = Rc::new(RefCell::new(Asked::default()));
    let mut s = Seat::boot(
        &[],
        None,
        Memory::new(320, 160, "mem"),
        &script,
        Some(&asked),
    );
    assert!(
        s.until(|_| asked.borrow().sent.len() == 1),
        "the first request"
    );
    asked.borrow_mut().reply = Some(Reply::Failed);
    assert!(
        s.until(|_| asked.borrow().sent.len() == 2),
        "the second request"
    );
    assert_eq!((asked.borrow().started, asked.borrow().stopped), (2, 1));
    asked.borrow_mut().reply = Some(Reply::Ok(Payload::Rgba {
        width: 2,
        height: 2,
        data: vec![255, 0, 0, 255],
    }));
    assert!(
        s.until(|s| s.shows("after") && s.shows("E_Gi=8;EBADF:Failed to read image file")),
        "{:?}",
        s.app.screen_text()
    );
    assert!(s.shows("E_Gi=7;EBADF:Failed to read image file"));
    assert_eq!((asked.borrow().started, asked.borrow().stopped), (2, 2));
    assert_eq!(s.placements(), 0);
}

/// The script of console 2 sends a quiet kitty file, `q=2`, which gets no
/// reply, then `after`; console 1's waits silent.
const HIDDEN_REQUEST: &str = "stty -echo -icanon min 1 time 0; \
     if [ \"$VTIUM_CONSOLE\" = 2 ]; then \
     printf '\\033_Ga=T,t=f,f=100,i=7,q=2;L3gucG5n\\033\\\\after\\n'; fi; read x";

/// D7: a hidden console's request goes to its own helper while another
/// console is shown, and its pty is paused meanwhile, so the reply's
/// resume wakes the loop: the bytes after the string are read in the
/// first wake after the reply, with no other source waking it. A timer
/// in the loop, due after [`BOUND`], only bounds the wait and marks that
/// nothing else woke it.
#[test]
fn a_hidden_consoles_request_is_answered_and_the_resume_wakes_the_loop() {
    let heads = "version = 1\n[head main]\nconsoles = 2\n[rules]\ndisplay card:mem -> main\n";
    let asked = Rc::new(RefCell::new(Asked::default()));
    let mut s = Seat::boot(
        &["--no-blink"],
        Some(heads),
        Memory::new(320, 160, "mem"),
        HIDDEN_REQUEST,
        Some(&asked),
    );
    let hidden = |s: &Seat, text: &str| {
        s.app.head("main").is_some_and(|h| {
            h.shown_console() == 0 && h.consoles_text()[1].iter().any(|l| l.contains(text))
        })
    };
    assert!(
        s.until(|_| asked.borrow().sent.len() == 1),
        "a request was sent"
    );
    for _ in 0..20 {
        s.turn();
    }
    assert!(!hidden(&s, "after"));
    assert!(s.until(|s| s.app.head("main").is_some_and(|h| h.frames_out() == 0)));
    asked.borrow_mut().reply = Some(red_picture());
    // The turn that hands the reply back, then the loop's next wake.
    s.app.turn(s.at());
    let bounded = Rc::new(Cell::new(false));
    let fired = Rc::clone(&bounded);
    let timer =
        s.lp.handle()
            .insert_source(
                calloop::timer::Timer::from_duration(BOUND),
                move |_, _, _| {
                    fired.set(true);
                    calloop::timer::TimeoutAction::Drop
                },
            )
            .expect("a timer");
    s.lp.dispatch(None, &mut s.app).expect("dispatch");
    s.app.turn(s.at());
    s.lp.handle().remove(timer);
    assert!(!bounded.get(), "the resume woke the loop");
    assert!(
        hidden(&s, "after"),
        "{:?}",
        s.app.head("main").map(Head::consoles_text)
    );
    assert_eq!((asked.borrow().started, asked.borrow().stopped), (1, 0));
}

/// The image turn comes before the draw: the turn that hands a reply back
/// draws the picture it placed, so the first frame after it shows all 48
/// of the picture's red pixels.
#[test]
fn the_turn_that_hands_a_reply_back_draws_its_picture() {
    let asked = Rc::new(RefCell::new(Asked::default()));
    let mut s = Seat::boot(
        &["--no-blink"],
        None,
        Memory::new(320, 160, "mem"),
        &kitty_file(),
        Some(&asked),
    );
    assert!(
        s.until(|_| asked.borrow().sent.len() == 1),
        "a request was sent"
    );
    let quiet = |s: &mut Seat| {
        let frames = s.memory("/dev/mem0").frames;
        s.app.heads()[0].frames_out() == 0
            && (0..5).all(|_| {
                s.turn();
                s.app.heads()[0].frames_out() == 0 && s.memory("/dev/mem0").frames == frames
            })
    };
    assert!(s.until(quiet), "the display settles");
    let frames = s.memory("/dev/mem0").frames;
    asked.borrow_mut().reply = Some(red_picture());
    s.app.turn(s.at());
    assert!(
        s.app.heads()[0].frames_out() > 0 || s.memory("/dev/mem0").frames > frames,
        "the reply's turn draws"
    );
    // Turns with no dispatch, so the loop reads nothing more of the
    // child's output while the frame composes and comes back.
    let end = now() + BOUND;
    while s.app.heads()[0].frames_out() > 0 && now() < end {
        std::thread::yield_now();
        s.app.turn(s.at());
    }
    assert_eq!(s.memory("/dev/mem0").frames, frames + 1);
    assert_eq!(s.memory("/dev/mem0").count(RED), 48);
}
