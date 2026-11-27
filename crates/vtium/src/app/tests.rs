use std::any::Any;
use std::io;
use std::path::Path;

use std::sync::{Arc, Mutex};

use vtium_platform::display::{Dpms, Event, Identity, Memory, Rect, SwapError, Swapped, Token};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods as InputMods, PointerEvent};
use vtium_platform::vt::VtEvent;
use vtium_render::Depth;
#[cfg(feature = "filters")]
use vtium_render::Orientation;

use super::devices::depth;
use super::fonts::draw_height;
use super::*;
use crate::blink::Timer;
use crate::font::Chosen;
use crate::head::chord::HeadChord;
use crate::layout::Zoom;
#[cfg(all(feature = "filters", feature = "gpu"))]
use crate::painter::Painter;
use crate::scratch::Scratch;

/// A device in memory with the displays `ids`, in the order it lists
/// them, each 320x160 and named for the rules by its connector on the
/// card `mem`.
pub(super) struct Still {
    ids: Vec<(DisplayId, &'static str)>,
    pixels: Arc<Mutex<Vec<u8>>>,
    pub(super) events: Vec<Event>,
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
    /// The kernel removed it, as drm2d finds a card gone at its first
    /// ENODEV.
    gone: bool,
    /// Stands for the device's file, which closes when the device is
    /// dropped.
    file: Rc<()>,
    /// A flip is in flight. Like a DRM display with three buffers, it
    /// lends the third buffer meanwhile, refuses the swap of a frame
    /// drawn into it as busy, and keeps that frame for a commit.
    flipping: bool,
    kept: bool,
    /// The swaps, each a frame composed, and the frames a commit sent.
    swaps: u32,
    committed: u32,
    /// The damage each swap brought, in order, as drm2d receives it: a
    /// swap with no damage presents nothing there.
    pub(super) damage: Vec<Vec<Rect>>,
    /// The buffers it takes turns with, as drm2d does: with 2, each
    /// buffer it lends once two frames have gone out is 2 frames old, so
    /// the renderer draws into it the cells the last two frames changed.
    /// A swap with no damage presents nothing and ages no buffer. With
    /// 0, every buffer is new and every frame is drawn whole.
    pub(super) buffers: u32,
    /// The kernel removes it at the next commit, which answers as
    /// drm2d does when that commit finds the card gone.
    unplug_at_commit: bool,
    /// A draw no key caused waits for its flip in flight, as on a DRM
    /// display with two or three buffers (ADR-1401 D2).
    waits_for_flip: bool,
    /// The scans its hotplugs ran.
    pub(super) hotplugs: u32,
    /// Set during each scan, as when the kernel asks for the VT while
    /// a slow probe holds the loop.
    pub(super) asks_for_vt: Option<Rc<std::cell::Cell<bool>>>,
    /// The levels it reports for each of its displays: `None` for
    /// color, and 2 for black and white.
    pub(super) levels: Option<u8>,
    /// The mode of each of its displays, at most 320x160, the memory
    /// it draws into.
    pub(super) size: (u32, u32),
    /// The bits a pixel of each display's buffer holds, as it reports
    /// them.
    pub(super) bits: Option<u8>,
    /// The density of each display across and down its mode, as it
    /// reports it from the display's physical size.
    pub(super) dpi: (f64, f64),
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
            gone: false,
            file: Rc::new(()),
            flipping: false,
            kept: false,
            swaps: 0,
            committed: 0,
            damage: Vec::new(),
            buffers: 0,
            unplug_at_commit: false,
            waits_for_flip: false,
            hotplugs: 0,
            asks_for_vt: None,
            levels: None,
            size: (320, 160),
            bits: Some(32),
            dpi: (72.0, 72.0),
        }
    }

    /// The kernel removes the device as drm2d sees a card go: each
    /// display reported removed, none listed after, and the device
    /// gone.
    pub(super) fn unplug(&mut self) {
        self.events
            .extend(self.ids.iter().map(|&(id, _)| Event::Removed(id)));
        self.ids.clear();
        self.gone = true;
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
        Some(self.size)
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        None
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        Some(self.dpi)
    }
    fn levels(&self, _: DisplayId) -> Option<u8> {
        self.levels
    }
    fn buffer_bits(&self, _: DisplayId) -> Option<u8> {
        self.bits
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
        if !self.ready_for_frame(id) {
            return None;
        }
        let memory: Arc<Memory> = self.pixels.clone();
        let presented = self.damage.iter().filter(|d| !d.is_empty()).count();
        let age = u32::try_from(presented)
            .unwrap_or(u32::MAX)
            .min(self.buffers);
        Some(Token::new(
            id,
            self.size,
            self.size.0.saturating_mul(4),
            age,
            memory,
        ))
    }
    fn give_back(&mut self, _: Token) {}
    fn swap(&mut self, _: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.swaps = self.swaps.saturating_add(1);
        self.damage.push(damage.to_vec());
        if self.flipping {
            self.kept = true;
            return Err(SwapError::Busy);
        }
        self.kept = false;
        Ok(Swapped::Frame)
    }
    fn commit(&mut self, _: DisplayId) -> Result<Swapped, SwapError> {
        if self.flipping {
            return Err(SwapError::Busy);
        }
        if std::mem::take(&mut self.unplug_at_commit) {
            self.unplug();
            return Err(SwapError::Off);
        }
        if !std::mem::take(&mut self.kept) {
            return Ok(Swapped::Nothing);
        }
        self.committed = self.committed.saturating_add(1);
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        self.flipping
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        !self.is_flipping(id) || !self.kept
    }
    fn paced_draw_waits_for_flip(&self, _: DisplayId) -> bool {
        self.waits_for_flip
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
        self.hotplugs = self.hotplugs.saturating_add(1);
        if let Some(asks) = &self.asks_for_vt {
            asks.set(true);
        }
        Ok(())
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn gone(&self) -> bool {
        self.gone
    }
}

/// The seat at 16 points on 72 DPI, with the head file `heads`, the
/// font sources `fonts` and the device `video` at `/dev/mem0`, before
/// its first turn.
pub(super) fn seat(heads: Option<&str>, fonts: font::Sources) -> (Loop<App>, App) {
    seat_with(&["--font-size=16"], heads, fonts)
}

/// [`seat`] with the options `options` in place of `--font-size=16`.
pub(super) fn seat_with(
    options: &[&str],
    heads: Option<&str>,
    fonts: font::Sources,
) -> (Loop<App>, App) {
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

// The console starts the font helper with its own locale, read from
// the environment it inherited, so that the helper picks the built-in
// font the console draws. A test's stand-in helper starts with it too.
//
// References:
// - ADR-1200 D7: the built-in font for a cell height and locale.
// - ADR-1204 D5: the built-in font draws the text of a set whose text
//   font is missing.
#[test]
fn the_font_helper_starts_with_the_consoles_locale() {
    let shell = crate::helper::stand_in_shell();
    let args: Vec<OsString> = [
        format!("--font-helper={}", shell.display()).as_str(),
        "--login",
        "--",
        "/bin/true",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
    let lp: Loop<App> = Loop::new().unwrap();
    let mut app = App::new(
        loaded.config,
        Box::new(FakeVt::new()),
        lp.handle(),
        Rc::new(Instant::now),
        vec![("LANG".into(), "ja_JP.UTF-8".into())],
    );
    let started = |app: &App| app.launcher.as_ref().unwrap().args().to_vec();
    assert_eq!(started(&app), ["serve", "--locale", "ja_JP.UTF-8"]);
    app.set_font_sources(font::Sources {
        helper: Some(crate::helper::Command::system(shell).unwrap()),
        ..font::Sources::default()
    });
    assert_eq!(started(&app), ["serve", "--locale", "ja_JP.UTF-8"]);
}

/// What the start runs for a trusted head file holding `text`.
pub(super) fn start(text: &str) -> Start {
    Start::trusted(Path::new("heads.conf"), text.as_bytes())
}

/// ADR-1401 D9: on a VT where no head runs, the rescue key's line
/// names no roaming chord, which chooses nothing there. It says that
/// no terminal runs on this VT, and how to switch to a VT that runs
/// one.
#[test]
fn the_rescue_line_on_a_vt_with_no_head_names_the_vt_keys() {
    let (_lp, mut app) = seat(None, no_fonts());
    let text = "version = 1\n[head desk]\nvt = 3\n[rules]\ninput path:kbd-desk -> desk\n";
    app.set_heads(start(text).on_vt(Some(4)));
    let node = Path::new("/dev/input/event0");
    let keyboard = vtium_settings::Identity {
        path: Some("kbd-desk".to_owned()),
        ..vtium_settings::Identity::input().with_class(vtium_settings::Class::Keyboard)
    };
    app.add_input(node, keyboard);
    assert!(app.heads().is_empty(), "no head runs on VT 4");
    assert_eq!(
        app.rescue_line(node),
        "Keyboard /dev/input/event0 now types into the active screen until the next boot (the rescue key moved it), but no terminal runs on this VT. Press Ctrl-Alt and a function key to switch to a VT that runs one."
    );
}

pub(super) fn no_fonts() -> font::Sources {
    font::Sources {
        frits: PathBuf::from("/nonexistent/vtium-test-frits"),
        trust: font::trust(),
        helper: None,
        vconsole_conf: PathBuf::from("/dev/null"),
        kernel_cmdline: PathBuf::from("/dev/null"),
    }
}

/// Shows the VT and runs a turn, which sets every device up.
pub(super) fn shown(app: &mut App) {
    shown_at(app, Instant::now());
}

/// [`shown`] with its turn at `t`. The turn that shows the VT starts
/// the blink timers, so a test that counts turns at times measured from
/// `t` shows the VT at `t` too. A turn at the host's clock would start
/// the phases however long after `t` the seat took to set up, and the
/// test's timers would then depend on how loaded the machine is.
pub(super) fn shown_at(app: &mut App, t: Instant) {
    app.vt.activate().unwrap();
    app.turn(t);
}

// Each display's view asks for its pointer's cell height through the
// console's own pointer file store, the store the loader of an SVG
// pointer file reads, so the file is drawn at that height.
#[test]
fn a_display_asks_the_consoles_pointer_file_store_for_its_height() {
    let (_lp, mut app) = seat(None, no_fonts());
    app.top_pointer_store().take_heights();
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    assert_eq!(app.top_pointer_store().asked(), [16]);
}

// A display that joins a head takes the color table of the head's
// vtlook when it meets the vtlook's requirements; one that does not,
// here a display of the assumed 96 DPI under `min-dpi = 200`, draws
// the built-in vtlook.
#[cfg(feature = "filters")]
#[test]
fn a_display_that_joins_takes_the_vtlook_s_table_when_it_meets_its_requirements() {
    use std::sync::Arc;
    use vtium_settings::vtlook;
    let heads = "version = 1\n[head a]\n[rules]\ndisplay connector:mem/A -> a\n";
    for (requires, takes) in [("", true), ("[requires]\nmin-dpi = 200\n", false)] {
        let (_lp, mut app) = seat(Some(heads), no_fonts());
        let text = format!("version = 1\n{requires}[color tint]\ncolor = 255,176,0\n");
        let look = vtlook::read_text(
            "amber",
            Path::new("/etc/vtium/looks/amber.vtlook"),
            text.as_bytes(),
        )
        .unwrap();
        let table = Arc::new(vtlook::table(&look).unwrap());
        if let Some(head) = app.heads.first_mut() {
            head.vtlook = crate::head::look::Chosen {
                vtlook: Some(look),
                table: Some(Arc::clone(&table)),
            };
        }
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A")])),
        );
        shown(&mut app);
        let held: Vec<bool> = app
            .heads
            .iter()
            .flat_map(|h| h.views.values())
            .map(|v| v.colors().is_some_and(|t| Arc::ptr_eq(t, &table)))
            .collect();
        assert_eq!(held, [takes], "{requires:?}");
    }
}

/// Gives head `a` of `app` the vtlook amber, a tint under `requires`,
/// and returns the color table it draws with.
#[cfg(feature = "filters")]
fn amber(app: &mut App, requires: &str) -> std::sync::Arc<vtium_render::filter::Table> {
    use std::sync::Arc;
    use vtium_settings::vtlook;
    let text = format!("version = 1\n[requires]\n{requires}\n[color tint]\ncolor = 255,176,0\n");
    let look = vtlook::read_text(
        "amber",
        Path::new("/etc/vtium/looks/amber.vtlook"),
        text.as_bytes(),
    )
    .unwrap();
    let table = Arc::new(vtlook::table(&look).unwrap());
    let head = app.heads.first_mut().unwrap();
    head.vtlook = crate::head::look::Chosen {
        vtlook: Some(look),
        table: Some(Arc::clone(&table)),
    };
    table
}

/// Whether each display of `app` draws with `table`.
#[cfg(feature = "filters")]
fn holding(app: &App, table: &std::sync::Arc<vtium_render::filter::Table>) -> Vec<bool> {
    app.heads
        .iter()
        .flat_map(|h| h.views.values())
        .map(|v| v.colors().is_some_and(|t| std::sync::Arc::ptr_eq(t, table)))
        .collect()
}

#[cfg(feature = "filters")]
/// The Still at `/dev/mem0` of `app`.
fn still(app: &mut App) -> &mut Still {
    let video = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
    video.as_any().downcast_mut::<Still>().unwrap()
}

#[cfg(feature = "filters")]
const ONE_HEAD: &str = "version = 1\n[head a]\n[rules]\ndisplay connector:mem/A -> a\n";

// A modeset that gives a display a mode the head's vtlook does not
// accept puts that display on the built-in vtlook, and the next modeset
// back to a mode it accepts gives it the vtlook's table again, with no
// reread of the vtlook.
#[cfg(feature = "filters")]
#[test]
fn a_mode_change_checks_the_display_against_the_vtlook_s_min_size_again() {
    let (_lp, mut app) = seat(Some(ONE_HEAD), no_fonts());
    let table = amber(&mut app, "min-size = 320x160");
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    assert_eq!(holding(&app, &table), [true], "at 320x160");
    for (size, takes) in [((200, 100), false), ((320, 160), true)] {
        let s = still(&mut app);
        s.size = size;
        s.events.push(Event::Redraw(DisplayId(1)));
        app.turn(Instant::now());
        assert_eq!(holding(&app, &table), [takes], "at {size:?}");
    }
}

// A color display is judged by the bits a pixel of its buffer holds,
// so a 16-bit framebuffer does not meet `min-depth = 24`; when the
// device is set up again at 32 bits, the display takes the vtlook.
#[cfg(feature = "filters")]
#[test]
fn a_16_bit_buffer_does_not_meet_min_depth_24() {
    let (_lp, mut app) = seat(Some(ONE_HEAD), no_fonts());
    let table = amber(&mut app, "min-depth = 24");
    let mut s = Still::new(&[(1, "A")]);
    s.bits = Some(16);
    app.add_video(PathBuf::from("/dev/mem0"), Box::new(s));
    shown(&mut app);
    assert_eq!(holding(&app, &table), [false], "16 bits");
    let s = still(&mut app);
    s.bits = Some(32);
    s.events.push(Event::Redraw(DisplayId(1)));
    app.turn(Instant::now());
    assert_eq!(holding(&app, &table), [true], "32 bits");
}

// A display whose pixels are not square has one density across its
// mode and another down it, and a turn swaps which of the two the
// head's vtlook judges. Each turn checks the head's displays again,
// as a mode change does: here the display meets `min-dpi = 100` down
// its mode, loses the vtlook when the head turns a quarter, and takes
// the vtlook's table again when the head turns back.
#[cfg(feature = "filters")]
#[test]
fn a_turn_checks_the_display_against_the_vtlook_s_min_dpi_again() {
    let (_lp, mut app) = seat(Some(ONE_HEAD), no_fonts());
    let table = amber(&mut app, "min-dpi = 100");
    let mut s = Still::new(&[(1, "A")]);
    s.dpi = (72.0, 144.0);
    app.add_video(PathBuf::from("/dev/mem0"), Box::new(s));
    shown(&mut app);
    assert_eq!(holding(&app, &table), [true], "144 DPI down the mode");
    let turn = |mods: InputMods, keysym: u32| {
        InputEvent::Key(KeyEvent {
            keycode: 0,
            ascii: 0,
            unshifted: 0,
            shifted: 0,
            base: None,
            mods,
            keysyms: vec![keysym],
            codepoints: vec![None],
            repeat: false,
        })
    };
    // Logo+= turns the head a quarter clockwise, and Logo+- turns it
    // back.
    crate::getty::tests::logged();
    app.input_event(turn(InputMods::LOGO, 0x3d), Instant::now());
    assert!(
        app.heads
            .first()
            .is_some_and(|h| matches!(h.orientation, Orientation::Right | Orientation::Left)),
        "the head turned"
    );
    assert_eq!(holding(&app, &table), [false], "72 DPI across, turned");
    let lines: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.starts_with("vtlook amber:"))
        .collect();
    assert_eq!(
        lines,
        [
            "vtlook amber: display A (320x160, 24 bits) draws the built-in vtlook: requires min-dpi 100"
        ],
    );
    app.input_event(turn(InputMods::LOGO, 0x2d), Instant::now());
    assert_eq!(holding(&app, &table), [true], "turned back");
}

// A vtlook with `gpu = yes` draws on a head the GPU path draws. When
// the GPU path ends, here at a lost context, the head draws on the CPU
// path and its display draws the built-in vtlook.
#[cfg(all(feature = "filters", feature = "gpu"))]
#[test]
fn a_head_that_leaves_the_gpu_path_drops_a_vtlook_that_requires_it() {
    use crate::painter::{Fault, GpuSource, TestGpu};
    let t = Instant::now();
    let (mut lp, mut app) = seat_with(&["--font-size=16", "--hwaccel"], Some(ONE_HEAD), no_fonts());
    let gpu = std::sync::Arc::new(TestGpu::default());
    app.set_gpu_source(GpuSource::Test(std::sync::Arc::clone(&gpu)));
    let table = amber(&mut app, "gpu = yes");
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown_at(&mut app, t);
    settle(&mut lp, &mut app, t);
    let path = |app: &App| {
        app.heads
            .first()
            .and_then(|h| h.painter.as_ref())
            .map(Painter::path)
    };
    assert_eq!(path(&app), Some(crate::painter::Path::Gpu));
    assert_eq!(holding(&app, &table), [true], "on the GPU path");
    gpu.faults.lock().unwrap().push_back(Fault::ContextLost);
    app.heads.first_mut().unwrap().draw_now = true;
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert_eq!(path(&app), Some(crate::painter::Path::Cpu));
    assert_eq!(holding(&app, &table), [false], "on the CPU path");
}

// Checking a display against the vtlook asks which path the head draws
// on, and that question logs nothing. A head with `hwaccel` on, on a
// card with no render node, logs that it draws on the CPU path once:
// when its render thread starts, not again at each display that joins
// before then.
#[cfg(all(feature = "filters", feature = "gpu"))]
#[test]
fn a_card_with_no_render_node_is_logged_once() {
    crate::getty::tests::logged();
    let (_lp, mut app) = seat_with(&["--font-size=16", "--hwaccel"], Some(ONE_HEAD), no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    let lines: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.contains("has no render node"))
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
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
    let dir = Scratch::new("app-left");
    std::fs::write(dir.join("t.vtfrit"), "source=builtin:unifont\n").unwrap();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: font::trust().below(&dir),
        helper: Some(helper::Command {
            program: helper::stand_in_shell().to_path_buf(),
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
}

// ADR-1302 D13: one cell size can carry two bands on two heads. Here
// head a sizes the frit `t` with `font-size` to text 16 rows high and
// adds 25% line spacing, an 8x20 cell with 2 rows above and 2 below,
// while head b draws `t` in the 8x20 cell it states, with no spacing.
// The helper is asked for each, and each keeps glyphs of its own.
#[test]
fn one_cell_size_with_two_bands_is_asked_for_twice() {
    let dir = Scratch::new("app-bands");
    std::fs::write(dir.join("t.vtfrit"), "source=builtin:unifont\ncell=8x20\n").unwrap();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: font::trust().below(&dir),
        helper: Some(helper::Command {
            program: helper::stand_in_shell().to_path_buf(),
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
    assert_eq!(
        app.routes
            .head(&app.vt_heads, Some(Path::new("/m")), Some(0)),
        None
    );
    assert_eq!(
        app.routes
            .head(&app.vt_heads, Some(Path::new("/k")), Some(0)),
        Some(0)
    );
    // A mouse whose node also has udev's key class, its consumer keys,
    // is still a pointer the shipped rules leave closed.
    let keyed = input()
        .with_class(vtium_settings::Class::Mouse)
        .with_class(vtium_settings::Class::Key);
    let rules = app.rules.clone();
    assert_eq!(
        app.routes
            .add(&rules, &app.vt_heads.clone(), Path::new("/mk"), &keyed),
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
    let partly = "version = 1\n[head a]\nconsoles = 0\n[head b]\n[rules]\ndisplay * -> b\n";
    app.set_heads(start(partly));
    assert_eq!(app.outcome.runs, vtium_settings::Runs::Survivors);
    let input = vtium_settings::Identity::input;
    let mouse = app.routes.add(
        &app.rules.clone(),
        &app.vt_heads.clone(),
        Path::new("/m"),
        &input().with_class(vtium_settings::Class::Mouse),
    );
    let keyboard = app.routes.add(
        &app.rules.clone(),
        &app.vt_heads.clone(),
        Path::new("/k"),
        &input().with_class(vtium_settings::Class::Keyboard),
    );
    assert_eq!(mouse, vtium_platform::input::Disposition::Closed);
    assert_eq!(keyboard, vtium_platform::input::Disposition::Deliver);
    assert_eq!(
        app.routes
            .head(&app.vt_heads, Some(Path::new("/m")), Some(0)),
        None
    );
    assert_eq!(
        app.routes
            .head(&app.vt_heads, Some(Path::new("/k")), Some(0)),
        Some(0)
    );
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
pub(super) fn settle(lp: &mut Loop<App>, app: &mut App, t: Instant) {
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
    let heads = "version = 1\n[head one]\nconsoles = 2\n[rules]\ndisplay * -> one\n";
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
    let keyboard = vtium_settings::Identity::input().with_class(vtium_settings::Class::Keyboard);
    app.add_input(Path::new("/nonexistent/vtium-test/event9"), keyboard);
    app.report_focus();
    assert_eq!(app.heads.first().map(|h| h.focused), Some(false));
}

// ADR-1401 D5: the focus holds only while vtium's VT is shown. A
// release under way takes it, though the VT stays vtium's until the
// displays are asleep and vtium lets the VT go.
// covers: platform.heads.focus
#[test]
fn a_release_under_way_takes_the_focus() {
    let (_lp, mut app) = seat(None, no_fonts());
    app.vt.activate().unwrap();
    app.focus = Some(0);
    let keyboard = vtium_settings::Identity::input().with_class(vtium_settings::Class::Keyboard);
    app.add_input(Path::new("/dev/input/event9"), keyboard);
    app.report_focus();
    assert_eq!(app.heads.first().map(|h| h.focused), Some(true));
    app.releasing = Some(BTreeSet::from([PathBuf::from("/dev/dri/card0")]));
    assert!(app.vt.is_active(), "the VT is still vtium's");
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
        unshifted: 0,
        shifted: 0,
        base: None,
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

// ADR-1401 D2: a key the terminal sends to the program opens a key's
// window on each display of the head, and the display's next flip that
// completes closes it. A key that sends the program nothing opens
// none: a scroll key on the main screen, or a modifier pressed alone.
#[test]
fn a_key_sent_to_the_program_opens_a_window_its_next_flip_closes() {
    let key = |keysym: u32, mods: InputMods| {
        InputEvent::Key(KeyEvent {
            keycode: 0,
            ascii: 0,
            unshifted: 0,
            shifted: 0,
            base: None,
            mods,
            keysyms: vec![keysym],
            codepoints: char::from_u32(keysym)
                .filter(|c| !c.is_control() && keysym < 0xff00)
                .into_iter()
                .map(Some)
                .collect(),
            repeat: false,
        })
    };
    let open = |app: &App| {
        let head = app.heads.first().unwrap();
        assert!(!head.views.is_empty(), "the head has a display");
        head.views.values().all(|v| v.after_key(&head.console.term))
    };
    let (_lp, mut app) = seat(None, no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    app.focus = Some(0);
    let t = Instant::now();
    assert!(!open(&app), "no key yet");
    // Shift+Page Up scrolls the main screen and sends nothing.
    app.input_event(key(0xff55, InputMods::SHIFT), t);
    assert!(!open(&app), "the scroll key sent nothing");
    // Shift pressed alone sends nothing either.
    app.input_event(key(0xffe1, InputMods::empty()), t);
    assert!(!open(&app), "Shift_L sent nothing");
    app.input_event(key(u32::from('c'), InputMods::empty()), t);
    assert!(open(&app), "c opens a window");
    let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
    let still = still.as_any().downcast_mut::<Still>().unwrap();
    still.events.push(Event::Flipped(DisplayId(1)));
    app.turn(t);
    assert!(!open(&app), "the flip closed it");
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
            unshifted: 0,
            shifted: 0,
            base: None,
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
        unshifted: 0,
        shifted: 0,
        base: None,
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
/// The seat is shown, and the frits' directory kept for the test,
/// which removes it when the test ends.
// ADR-1304 D6: a start reads and logs each head's vtlook once. App::new
// builds the compiled-in head `console` until the head file's heads
// replace it, and that interim head reads no vtlook, so the one line
// about the missing vtlook names head `desk`, which the file declares.
#[test]
fn a_start_reads_and_logs_a_vtlook_once_for_the_files_head() {
    crate::getty::tests::logged();
    let heads = "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> desk\n";
    let (_lp, _app) = seat_with(&["--vtlook=gone"], Some(heads), no_fonts());
    let lines: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.contains("gone"))
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines.iter().all(|l| l.contains("head desk")), "{lines:?}");
}

/// A seat whose one head draws the font set `t`, whose first source
/// is a bitmap font with strikes, through a stand-in helper that never
/// answers. The first ask, at the head's height, had no answer in time.
fn struck_seat(test: &str) -> (Loop<App>, App, Scratch) {
    let dir = Scratch::new(&format!("app-{test}"));
    std::fs::write(
        dir.join("t.vtfrit"),
        "source=bdf:/nonexistent/16.bdf\nstrike=bdf:/nonexistent/12.bdf\n",
    )
    .unwrap();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: font::trust().below(&dir),
        helper: Some(helper::Command {
            program: helper::stand_in_shell().to_path_buf(),
            args: vec!["-c".into(), "exec sleep 30".into()],
            account: None,
        }),
        ..no_fonts()
    };
    let (lp, mut app) = seat_with(&["--font-name=t"], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    assert!(
        app.heads.first().unwrap().chosen().struck(),
        "the set has strikes"
    );
    (lp, app, dir)
}

// ADR-1204 D2: a font set with strikes draws a cell at each height,
// so where the helper gives no cell at the head's height, the set
// draws as wide as the built-in font at that height, and the log says
// so, as it does for an outline font.
#[test]
fn a_struck_set_with_no_cell_logs_that_it_draws_as_wide_as_the_built_in_font() {
    crate::getty::tests::logged();
    let (_lp, app, _dir) = struck_seat("struck-late");
    let size = app.heads.first().unwrap().font_size;
    let lines: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.contains("is not known"))
        .collect();
    assert_eq!(
        lines,
        [format!(
            "the frit t's cell at {size} pixels is not known (the helper gave no answer in 100 ms): it draws as wide as the built-in font"
        )]
    );
}

// ADR-1204 D2: a zoom out of a font set with strikes stops at its
// shortest strike, where the font helper says that no strike is
// shorter, and the log says so once however often a key asks. A helper
// that could not answer stops the zoom too, and the log then gives the
// helper's reason, not the set's smallest height. Each case puts the
// answer below the head's height in the frit's answers, as the
// helper's answer would.
#[test]
fn a_zoom_out_at_the_shortest_strike_logs_why_it_stops_once() {
    for (from_frit, why) in [
        (true, "no strike is that short"),
        (false, "the helper ended"),
    ] {
        let (_lp, mut app, _dir) = struck_seat(&format!("zoom-stop-{from_frit}"));
        let size = app.heads.first().unwrap().font_size;
        let refusal = Err(helper::NoCell {
            why: why.to_owned(),
            late: false,
            from_frit,
        });
        app.frits
            .get_mut("t")
            .unwrap()
            .own
            .insert(size - 1, refusal);
        crate::getty::tests::logged();
        app.zoom(0, Zoom::Out);
        app.zoom(0, Zoom::Out);
        assert_eq!(app.heads.first().unwrap().font_size, size, "the zoom stops");
        let name = app.heads.first().unwrap().name().to_owned();
        let want = if from_frit {
            format!(
                "head {name}: the font set t draws no smaller than {size} pixels, so a zoom out stops there"
            )
        } else {
            format!(
                "head {name}: the font helper could not say which strike of the font set t draws below {size} pixels ({why}), so a zoom out stops there"
            )
        };
        let stops: Vec<String> = crate::getty::tests::logged()
            .into_iter()
            .filter(|l| l.contains("zoom out stops"))
            .collect();
        assert_eq!(stops, [want]);
    }
}

// ADR-1204 D2: a helper that could not answer at the head's height,
// because it failed or ended, stops a zoom in at once. vtium asks it at
// no other height, so a helper that refuses every ask cannot send the
// step up through every height to the largest cell.
#[test]
fn a_zoom_in_through_a_helper_that_cannot_answer_asks_no_further() {
    let (_lp, mut app, _dir) = struck_seat("zoom-in-ended");
    let size = app.heads.first().unwrap().font_size;
    let ended = Err(helper::NoCell {
        why: "the helper ended".to_owned(),
        late: false,
        from_frit: false,
    });
    app.frits.get_mut("t").unwrap().own.insert(size, ended);
    let asked: Vec<u32> = app.frits.get("t").unwrap().own.keys().copied().collect();
    app.zoom(0, Zoom::In);
    assert_eq!(app.heads.first().unwrap().font_size, size, "no step");
    let now: Vec<u32> = app.frits.get("t").unwrap().own.keys().copied().collect();
    assert_eq!(now, asked, "no other height asked");
}

/// One row of one column, which every display holds at every height.
const ONE_CELL: vtium_term::GridRequest = vtium_term::GridRequest {
    rows: vtium_term::Extent::Cells(1),
    cols: vtium_term::Extent::Cells(1),
};

// ctlseqs, XTWINOPS 8: a text area request on a font set with strikes
// waits while the font helper still owes an answer at a height its
// search needs, and the console takes none of its program's output
// meanwhile. A helper that vtium gives up never answers, so the search
// ends at the next turn that takes the helper's answers. It ends as it
// does when the helper could not answer in time: at the height the
// search reached, here the start's, on exactly the grid asked for. The
// console then takes its program's output again.
#[test]
fn a_text_area_request_ends_once_the_font_helper_is_given_up() {
    let (_lp, mut app, _dir) = struck_seat("climb-given-up");
    let head = app.heads.first().unwrap();
    let (size, grid) = (head.font_size, head.grid);
    app.request_grid(0, ONE_CELL);
    let head = app.heads.first().unwrap();
    assert!(!head.console.term.takes_input(), "the console waits");
    assert_eq!((head.font_size, head.grid), (size, grid), "no resize yet");
    let frit = app.frits.get_mut("t").unwrap();
    frit.helper.as_mut().unwrap().give_up("the helper failed");
    app.helper_turn((app.clock)());
    let head = app.heads.first().unwrap();
    assert!(head.console.term.takes_input(), "the console takes output");
    assert_eq!((head.font_size, head.grid), (size, (1, 1)), "the request");
}

// ctlseqs, XTWINOPS 8: a console that is hidden while its text area
// request waits for the font helper goes on holding the request and its
// program's output, as a hidden console holds a request it has not yet
// carried out. Shown again, the console carries the request out. Here
// the helper is given up meanwhile, so the request ends at the height
// the search reached, on exactly the grid asked for.
//
// References:
// - ADR-1401 D1: a hidden console keeps its own grid and cell.
#[test]
fn a_text_area_request_from_a_console_hidden_while_it_waits_is_carried_out_once_shown() {
    let (_lp, mut app, _dir) = struck_seat("climb-hidden");
    let head = app.heads.first().unwrap();
    let (first, size, grid) = (head.console.id, head.font_size, head.grid);
    app.request_grid(0, ONE_CELL);
    let now = Instant::now();
    assert!(app.open_console(0, now), "a second console opens");
    app.turn(now);
    let frit = app.frits.get_mut("t").unwrap();
    frit.helper.as_mut().unwrap().give_up("the helper failed");
    app.turn(Instant::now());
    let head = app.heads.first().unwrap();
    assert_eq!(head.grid, grid, "the second console keeps the grid");
    let hidden = head.hidden.iter().find(|c| c.id == first).unwrap();
    assert!(!hidden.term.takes_input(), "the hidden console waits");
    app.chord(Some(0), HeadChord::NextConsole, false, Instant::now());
    app.turn(Instant::now());
    let head = app.heads.first().unwrap();
    assert_eq!(head.console.id, first, "console 1 is shown again");
    assert!(head.console.term.takes_input(), "the console takes output");
    assert_eq!((head.font_size, head.grid), (size, (1, 1)), "the request");
}

// ADR-1200 D14: once vtium leaves out a font of a frit, here its text
// font, after the font made the helper fail, the console forgets the
// cells the helper answered and asks for each again. The helper now
// refuses the cell, so the head that drew in the text font's cell goes
// back to the cell it drew in before the frit gave it one.
#[test]
fn a_font_left_out_asks_for_the_cells_again_and_a_refusal_drops_the_cell() {
    let (_lp, mut app, _dir) = late_seat("late-left-out");
    let at = app.heads.first().map(|h| h.font_size).unwrap();
    let drawn = |app: &App| {
        let head = app.heads.first().unwrap();
        (head.cell, head.cells(), head.grid())
    };
    let before = drawn(&app);
    let (width, height) = before.0;
    let own = vtium_font::Cell {
        width: width + 2,
        height,
    };
    app.late_cell("t", at, Ok(helper::OwnCell::Bitmap(own)));
    assert_eq!(drawn(&app).0, (own.width, own.height), "the frit's cell");
    app.ask_cells_again("t", (app.clock)());
    assert!(
        app.frits.get("t").unwrap().own.is_empty(),
        "the cells are forgotten"
    );
    let refusal = Err(helper::NoCell {
        why: "vtium left out the text font source=kbd:x after it made the font helper fail, so the built-in font draws the text in its place".to_owned(),
        late: false,
        from_frit: true,
    });
    app.late_cell("t", at, refusal);
    assert_eq!(drawn(&app), before, "the head draws in its earlier cell");
}

// The console's turn of the helpers asks for the frit's cells again
// when its helper says it left a source out: the turn forgets the cell
// the frit held, and the running helper is asked for it once more.
#[test]
fn a_turn_after_a_font_was_left_out_asks_for_the_cells_again() {
    let (_lp, mut app, _dir) = late_seat("late-turn-blamed");
    let at = app.heads.first().map(|h| h.font_size).unwrap();
    let (width, height) = app.heads.first().unwrap().cell;
    let own = vtium_font::Cell {
        width: width + 2,
        height,
    };
    app.late_cell("t", at, Ok(helper::OwnCell::Bitmap(own)));
    let frit = app.frits.get_mut("t").unwrap();
    assert!(frit.own.contains_key(&at), "the frit's cell");
    let helper = frit.helper.as_mut().unwrap();
    let mut asked = helper.asked_late();
    helper.set_blamed();
    app.helper_turn((app.clock)());
    let frit = app.frits.get("t").unwrap();
    assert!(frit.own.is_empty(), "the cells are forgotten");
    asked.push(at);
    let helper = frit.helper.as_ref().unwrap();
    assert_eq!(helper.asked_late(), asked, "the cell is asked again");
}

fn late_seat(test: &str) -> (Loop<App>, App, Scratch) {
    let dir = Scratch::new(&format!("app-{test}"));
    std::fs::write(dir.join("t.vtfrit"), "source=kbd:x\n").unwrap();
    std::fs::write(dir.join("u.vtfrit"), "source=kbd:y\n").unwrap();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: font::trust().below(&dir),
        helper: Some(helper::Command {
            program: helper::stand_in_shell().to_path_buf(),
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

/// A program on one display, 320×160 pixels and drawing the built-in
/// font in its 8×8 cell, switches with OSC 50 to the bitmap frit `t`,
/// whose helper never answers in time. The program then asks for the
/// font's name and, with XTWINOPS 16, 14 and 18, for the cell, the text
/// area and the grid, and writes `hi`. `settle` makes the state change
/// that ends the wait for the cell. Returns the replies the program got
/// and the screen's first row before `settle` ran, and all the replies
/// it got.
fn late_switch(test: &str, settle: impl FnOnce(&mut App)) -> (String, String, String) {
    let dir = Scratch::new(&format!("app-{test}"));
    std::fs::write(dir.join("t.vtfrit"), "source=kbd:x\n").unwrap();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: font::trust().below(&dir),
        helper: Some(helper::Command {
            program: helper::stand_in_shell().to_path_buf(),
            args: vec!["-c".into(), "exec sleep 30".into()],
            account: None,
        }),
        ..no_fonts()
    };
    let (_lp, mut app) = seat_with(&[], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    assert_eq!(app.heads.first().unwrap().cell, (8, 8), "the built-in cell");
    let program = b"\x1b]50;t\x07\x1b]50;?\x07\x1b[16t\x1b[14t\x1b[18thi";
    let mut replies = Vec::new();
    let mut fed = 0;
    let mut feed = |app: &mut App, replies: &mut Vec<u8>| {
        let term = app.term_mut().unwrap();
        let rest = program.get(fed..).unwrap_or_default();
        fed = fed.saturating_add(term.parse(rest, &mut |r| replies.extend_from_slice(r)));
        fed
    };
    // The terminal stops after the switch's request, and the turn
    // switches the font, whose cell is not known in time.
    feed(&mut app, &mut replies);
    app.turn(Instant::now());
    assert_eq!(app.heads.first().unwrap().frit(), Some("t"), "switched");
    assert_eq!(app.heads.first().unwrap().cell, (8, 8), "in the old cell");
    // The terminal reads the rest of the program's output and draws
    // it in the old cell.
    let read = feed(&mut app, &mut replies);
    assert_eq!(read, program.len(), "the program's output is all read");
    app.turn(Instant::now());
    let early = String::from_utf8(replies.clone()).unwrap();
    let row = app.screen_text().first().cloned().unwrap_or_default();
    settle(&mut app);
    app.turn(Instant::now());
    feed(&mut app, &mut replies);
    (early, row, String::from_utf8(replies).unwrap())
}

// A program that switches to a bitmap font set whose cell comes after
// the wait, then asks for the set's name and its cell, and then writes
// text, sees its text drawn at once. The answers wait until the cell is
// known. Every answer after the new name then reports the new 10×8
// cell, on the grid of 32×20 cells it lays out, in the order the
// program asked.
//
// References:
// - ADR-1200 D5: OSC 50's query names the font set in use.
// - ADR-1200 D17: the cell that comes after the wait.
// covers: text.font-change
#[test]
fn a_switch_answers_once_its_late_cell_is_known() {
    let (early, row, replies) = late_switch("late-switch-cell", |app| {
        let own = vtium_font::Cell {
            width: 10,
            height: 8,
        };
        let frit = app.frits.get_mut("t").unwrap();
        let running = frit.helper.as_mut().unwrap();
        assert!(running.answer_late(Ok(helper::OwnCell::Bitmap(own))));
    });
    assert_eq!(row.trim_end(), "hi", "the text is drawn during the wait");
    assert_eq!(early, "", "nothing is answered while the cell is not known");
    assert_eq!(
        replies,
        "\x1b]50;t\x07\x1b[6;8;10t\x1b[4;160;320t\x1b[8;20;32t"
    );
}

// When the font helper fails before the cell comes, the console stops
// waiting for it. The program's text is drawn during the wait, and the
// program then gets the answers in the 8×8 cell the head keeps, in the
// order it asked.
// covers: text.font-change
#[test]
fn a_switch_answers_in_the_kept_cell_once_the_helper_fails() {
    let (early, row, replies) = late_switch("late-switch-failed", |app| {
        let frit = app.frits.get_mut("t").unwrap();
        frit.helper.as_mut().unwrap().give_up("the helper failed");
    });
    assert_eq!(row.trim_end(), "hi", "the text is drawn during the wait");
    assert_eq!(early, "", "nothing is answered while the cell is not known");
    assert_eq!(
        replies,
        "\x1b]50;t\x07\x1b[6;8;8t\x1b[4;160;320t\x1b[8;20;40t"
    );
}

// A font set's helper crashed on its second font, so vtium used one of
// the helper's three chances and left that font out. A program then
// switches with OSC 50 to the built-in font and back. No head draws the
// font set in between, so vtium drops it and its helper. The helper
// that vtium starts when the program switches back still has one
// chance used, and it leaves the same font out. Otherwise each round
// trip would give the helper three new chances, and a program could
// make vtium start the helper as often as it likes.
//
// References:
// - ADR-1200 D15, "Restarted after a failure": three chances a boot.
#[test]
fn a_font_set_switched_away_and_back_keeps_its_chances_and_left_out_fonts() {
    let dir = Scratch::new("app-failures-kept");
    std::fs::write(dir.join("t.vtfrit"), "source=kbd:x\nsource=kbd:y\n").unwrap();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: font::trust().below(&dir),
        helper: Some(helper::Command {
            program: helper::stand_in_shell().to_path_buf(),
            args: vec!["-c".into(), "exec sleep 30".into()],
            account: None,
        }),
        ..no_fonts()
    };
    let (_lp, mut app) = seat_with(&[], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    let osc50 = |app: &mut App, name: &str| {
        let request = format!("\x1b]50;{name}\x07");
        app.term_mut()
            .unwrap()
            .parse(request.as_bytes(), &mut |_| {});
        app.turn(Instant::now());
    };
    osc50(&mut app, "t");
    let helper = app.helper("t").unwrap();
    helper.crashed_on(1, Instant::now());
    assert_eq!(helper.used(), (1, 0b10));
    osc50(&mut app, "unifont");
    assert!(!app.frits.contains_key("t"), "no head draws the font set");
    osc50(&mut app, "t");
    assert_eq!(app.font_name(), "t");
    let helper = app.helper("t").unwrap();
    assert_eq!(
        helper.used(),
        (1, 0b10),
        "the chance and the font stay used"
    );
}

// A program on console 1 switches to a bitmap font set whose cell comes
// after the wait, and asks for the set's name and its cell. The user
// opens console 2 before the cell comes, which hides console 1. The
// late 10×8 cell lays out the shown console alone, so console 1 keeps
// its 8×8 fit while it is hidden and goes on holding its replies.
// Shown again, console 1 is laid out in the new cell, and only then
// does it answer, with the new name and the new cell together.
//
// References:
// - ADR-1200 D17: the cell that comes after the wait.
// - ADR-1401 D1: a hidden console keeps its own grid and cell.
// covers: text.font-change
#[test]
fn a_console_hidden_during_the_wait_answers_once_shown_in_the_new_cell() {
    let dir = Scratch::new("app-late-hidden");
    std::fs::write(dir.join("t.vtfrit"), "source=kbd:x\n").unwrap();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: font::trust().below(&dir),
        helper: Some(helper::Command {
            program: helper::stand_in_shell().to_path_buf(),
            args: vec!["-c".into(), "exec sleep 30".into()],
            account: None,
        }),
        ..no_fonts()
    };
    let (_lp, mut app) = seat_with(&[], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    let mut replies = Vec::new();
    let term = app.term_mut().unwrap();
    term.parse(b"\x1b]50;t\x07", &mut |r| replies.extend_from_slice(r));
    app.turn(Instant::now());
    let term = app.term_mut().unwrap();
    term.parse(b"\x1b]50;?\x07\x1b[16t", &mut |r| {
        replies.extend_from_slice(r)
    });
    app.turn(Instant::now());
    assert_eq!(replies, b"", "the replies wait for the cell");
    let first = app.heads.first().unwrap().console.id;
    let now = Instant::now();
    assert!(app.open_console(0, now), "a second console opens");
    app.turn(now);
    let own = vtium_font::Cell {
        width: 10,
        height: 8,
    };
    let frit = app.frits.get_mut("t").unwrap();
    assert!(
        frit.helper
            .as_mut()
            .unwrap()
            .answer_late(Ok(helper::OwnCell::Bitmap(own)))
    );
    app.turn(Instant::now());
    let head = app.heads.first_mut().unwrap();
    assert_eq!(head.cell, (10, 8), "the shown console takes the new cell");
    let hidden = head.hidden.iter_mut().find(|c| c.id == first).unwrap();
    assert_eq!(hidden.fit.cell, (8, 8), "the hidden console keeps its fit");
    assert!(hidden.term.terminal.replies_held(), "and its hold");
    assert_eq!(hidden.term.terminal.take_replies(), b"", "nothing went out");
    app.chord(Some(0), HeadChord::NextConsole, false, Instant::now());
    app.turn(Instant::now());
    let head = app.heads.first_mut().unwrap();
    assert_eq!(head.console.id, first, "console 1 is shown again");
    assert_eq!(head.cell, (10, 8), "in the new cell");
    assert!(!head.console.term.terminal.replies_held(), "the hold ended");
    assert_eq!(
        String::from_utf8(head.console.term.terminal.take_replies()).unwrap(),
        "\x1b]50;t\x07\x1b[6;8;10t"
    );
}

/// The OSC 50 and XTWINOPS 16 replies that the console `id` of head 0
/// gives at once to a query of the font set's name and of its cell.
fn name_and_cell(app: &mut App, id: u64) -> String {
    let head = app.heads.first_mut().unwrap();
    let console = head.consoles_mut().find(|c| c.id == id).unwrap();
    let mut replies = Vec::new();
    console.term.parse(b"\x1b]50;?\x07\x1b[16t", &mut |r| {
        replies.extend_from_slice(r)
    });
    String::from_utf8(replies).unwrap()
}

/// Frits `a` and `b`, which state their cells of 10×20 and 16×32, and
/// `t`, whose cell the stand-in helper never gives in time.
fn two_cells_and_a_late_one(dir: &Scratch) -> font::Sources {
    std::fs::write(dir.join("a.vtfrit"), "source=kbd:x\ncell=10x20\n").unwrap();
    std::fs::write(dir.join("b.vtfrit"), "source=kbd:x\ncell=16x32\n").unwrap();
    std::fs::write(dir.join("t.vtfrit"), "source=kbd:x\n").unwrap();
    font::Sources {
        frits: dir.to_path_buf(),
        trust: font::trust().below(dir),
        helper: Some(helper::Command {
            program: helper::stand_in_shell().to_path_buf(),
            args: vec!["-c".into(), "exec sleep 30".into()],
            account: None,
        }),
        ..no_fonts()
    }
}

/// A seat whose head draws frit `a` at a configured size of 16 points,
/// so that a switch waits for no font's own cell. A font file
/// `fonts/sub/a.bdf` sits in the scratch folder `dir`, which
/// `font-paths` names. The batch lists the file, under the rule of the
/// account the test runs as, below `dir`. The result is the seat and
/// the font file's path.
fn with_a_font_path(dir: &Scratch) -> (Loop<App>, App, String) {
    use std::os::unix::fs::PermissionsExt;
    let sub = dir.join("fonts/sub");
    std::fs::create_dir_all(&sub).unwrap();
    for folder in [dir.to_path_buf(), dir.join("fonts"), sub.clone()] {
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let file = sub.join("a.bdf");
    std::fs::write(&file, "a font").unwrap();
    let batch = dir.join("batch.conf");
    std::fs::write(&batch, format!("{}\n", file.display())).unwrap();
    let fonts = two_cells_and_a_late_one(dir);
    let paths = format!("--font-paths={}", dir.join("fonts").display());
    let (lp, mut app) = seat_with(&["--font-name=a", "--font-size=16", &paths], None, fonts);
    let me = rustix::process::geteuid().as_raw();
    app.set_batch_places(
        vtium_settings::batch::Places {
            file: batch,
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            palettes: dir.to_path_buf(),
            ..vtium_settings::batch::Places::system()
        },
        vtium_settings::cullet::Rule::new(vtium_settings::Trust::owner(me)).below(dir),
    );
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    (lp, app, file.display().to_string())
}

// A program switches the console to a font that root installed and
// trusts by the font file's path. vtium makes a font set of that one
// file, named by its path, and the head draws it. A switch to another
// font set and back by the path works again, and so does the switch
// back to the font set the head started in, by its name.
//
// References:
// - ADR-1200 D5, "A qualifying font": OSC 50 takes the path of a font that passes the four checks.
// - ADR-1200 D5: every name the query reports is a name a switch accepts.
// covers: text.font-change
#[test]
fn osc_50_switches_to_a_qualifying_font_path_and_back() {
    let dir = Scratch::new("app-font-path");
    let (_lp, mut app, path) = with_a_font_path(&dir);
    assert_eq!(app.font_name(), "a");
    assert_eq!(osc50(&mut app, &path), format!("\x1b]50;{path}\x07"));
    let head = app.heads.first().unwrap();
    assert_eq!(
        head.frit(),
        Some(path.as_str()),
        "the head draws the font file"
    );
    assert_eq!(head.font_refused, 0, "nothing was refused");
    let text = match head.chosen() {
        Chosen::Frit { frit, .. } => frit.text.clone(),
        Chosen::Builtin { .. } => String::new(),
    };
    assert_eq!(
        text,
        format!("source=bdf:{path}\n"),
        "a font set made for the path"
    );
    assert!(app.frits.contains_key(&path), "with a helper of its own");
    assert_eq!(osc50(&mut app, "b"), "\x1b]50;b\x07");
    assert!(!app.frits.contains_key(&path), "no head draws the path");
    assert_eq!(
        osc50(&mut app, &path),
        format!("\x1b]50;{path}\x07"),
        "the path the query reported switches back"
    );
    assert_eq!(
        osc50(&mut app, "a"),
        "\x1b]50;a\x07",
        "the start's font set"
    );
}

/// Sends `OSC 50 ; name ST` and the query from the shown console, runs
/// a turn after each, and gives the query's reply.
fn osc50(app: &mut App, name: &str) -> String {
    let request = format!("\x1b]50;{name}\x07");
    app.term_mut()
        .unwrap()
        .parse(request.as_bytes(), &mut |_| {});
    app.turn(Instant::now());
    let mut replies = Vec::new();
    app.term_mut()
        .unwrap()
        .parse(b"\x1b]50;?\x07", &mut |r| replies.extend_from_slice(r));
    app.turn(Instant::now());
    String::from_utf8(replies).unwrap()
}

// A program asks for a font file by a path that fails one of the four
// checks, each in turn. vtium leaves the font as it was each time, and
// counts each refusal until the next switch.
//
// References:
// - ADR-1200 D5, "A qualifying font": the four checks, in their order.
// covers: text.font-change
#[test]
fn osc_50_leaves_the_font_for_a_path_that_fails_a_check() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("app-font-path-refused");
    let (_lp, mut app, path) = with_a_font_path(&dir);
    let outside = dir.join("elsewhere.bdf");
    std::fs::write(&outside, "a font").unwrap();
    let inactive = dir.join("fonts/sub/b.bdf");
    std::fs::write(&inactive, "a font").unwrap();
    let open = dir.join("fonts/open");
    std::fs::create_dir_all(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
    let untrusted = open.join("c.bdf");
    std::fs::write(&untrusted, "a font").unwrap();
    std::fs::write(
        dir.join("batch.conf"),
        format!("{}\n{}\n{}\n", path, outside.display(), untrusted.display()),
    )
    .unwrap();
    app.return_font(0, app.heads.first().unwrap().console.id);
    assert!(app.gate().holds(&untrusted), "the batch is read again");
    let relative = path.trim_start_matches('/').to_owned();
    for (n, bad) in [
        relative,
        outside.display().to_string(),
        inactive.display().to_string(),
        untrusted.display().to_string(),
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            osc50(&mut app, bad),
            "\x1b]50;a\x07",
            "check {}: {bad}",
            n + 1
        );
        assert_eq!(
            app.heads.first().unwrap().font_refused,
            u64::try_from(n + 1).unwrap(),
            "each refusal is counted"
        );
    }
    assert_eq!(
        osc50(&mut app, &path),
        format!("\x1b]50;{path}\x07"),
        "a path that passes still switches"
    );
    assert_eq!(app.heads.first().unwrap().font_refused, 0);
}

// Head 0 draws frit a and has two consoles; console 2 is hidden. A
// program on console 1 switches the head to frit b. While console 2 is
// hidden it keeps the grid and the 10×20 cell it last drew in, so it
// goes on naming a, and its name and its cell agree. Once shown, the
// head lays console 2 out in b, and its name and its cell change
// together.
//
// References:
// - ADR-1200 D5: the name OSC 50's query reports.
// - ADR-1401 D1: a hidden console keeps its own grid and cell.
// covers: text.font-change
#[test]
fn a_hidden_console_names_the_font_it_last_drew_in_until_shown() {
    let dir = Scratch::new("app-hidden-name");
    let fonts = two_cells_and_a_late_one(&dir);
    let (_lp, mut app) = seat_with(&["--font-name=a", "--font-size=0"], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    let first = app.heads.first().unwrap().console.id;
    assert!(
        app.open_console(0, Instant::now()),
        "a second console opens"
    );
    app.turn(Instant::now());
    let second = app.heads.first().unwrap().console.id;
    app.chord(Some(0), HeadChord::NextConsole, false, Instant::now());
    app.turn(Instant::now());
    assert_eq!(app.heads.first().unwrap().console.id, first);
    assert_eq!(
        name_and_cell(&mut app, second),
        "\x1b]50;a\x07\x1b[6;20;10t"
    );
    let mut replies = Vec::new();
    let term = app.term_mut().unwrap();
    term.parse(b"\x1b]50;b\x07", &mut |r| replies.extend_from_slice(r));
    app.turn(Instant::now());
    assert_eq!(
        app.heads.first().unwrap().cell,
        (16, 32),
        "the head draws b"
    );
    assert_eq!(name_and_cell(&mut app, first), "\x1b]50;b\x07\x1b[6;32;16t");
    assert_eq!(
        name_and_cell(&mut app, second),
        "\x1b]50;a\x07\x1b[6;20;10t",
        "console 2, hidden, keeps the font it last drew in"
    );
    app.chord(Some(0), HeadChord::NextConsole, false, Instant::now());
    app.turn(Instant::now());
    assert_eq!(app.heads.first().unwrap().console.id, second, "shown");
    assert_eq!(
        name_and_cell(&mut app, second),
        "\x1b]50;b\x07\x1b[6;32;16t"
    );
}

// Head 0 draws frit a and has two consoles; console 2 is hidden. A
// program on console 1 switches the head to frit t, whose cell comes
// after the wait. Console 2, hidden, names a and its 10×20 cell during
// the wait and after the late 12×24 cell lays console 1 out. Once
// shown, console 2 is laid out in t's cell, and names t with that cell.
//
// References:
// - ADR-1200 D5: the name OSC 50's query reports.
// - ADR-1200 D17: the cell that comes after the wait.
// - ADR-1401 D1: a hidden console keeps its own grid and cell.
// covers: text.font-change
#[test]
fn a_hidden_console_takes_the_late_cell_and_its_name_together_once_shown() {
    let dir = Scratch::new("app-hidden-late-name");
    let fonts = two_cells_and_a_late_one(&dir);
    let (_lp, mut app) = seat_with(&["--font-name=a", "--font-size=0"], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    let first = app.heads.first().unwrap().console.id;
    assert!(
        app.open_console(0, Instant::now()),
        "a second console opens"
    );
    app.turn(Instant::now());
    let second = app.heads.first().unwrap().console.id;
    app.chord(Some(0), HeadChord::NextConsole, false, Instant::now());
    app.turn(Instant::now());
    assert_eq!(app.heads.first().unwrap().console.id, first);
    let mut replies = Vec::new();
    let term = app.term_mut().unwrap();
    term.parse(b"\x1b]50;t\x07", &mut |r| replies.extend_from_slice(r));
    app.turn(Instant::now());
    assert!(
        app.heads
            .first()
            .unwrap()
            .console
            .term
            .terminal
            .replies_held(),
        "console 1 waits for t's cell"
    );
    assert_eq!(
        name_and_cell(&mut app, second),
        "\x1b]50;a\x07\x1b[6;20;10t",
        "console 2, hidden during the wait"
    );
    let own = vtium_font::Cell {
        width: 12,
        height: 24,
    };
    let frit = app.frits.get_mut("t").unwrap();
    assert!(
        frit.helper
            .as_mut()
            .unwrap()
            .answer_late(Ok(helper::OwnCell::Bitmap(own)))
    );
    app.turn(Instant::now());
    let head = app.heads.first_mut().unwrap();
    assert_eq!(head.cell, (12, 24), "the late cell");
    assert_eq!(
        String::from_utf8(head.console.term.terminal.take_replies()).unwrap(),
        "",
        "console 1 asked nothing"
    );
    assert_eq!(
        name_and_cell(&mut app, second),
        "\x1b]50;a\x07\x1b[6;20;10t",
        "console 2, hidden after the late cell"
    );
    app.chord(Some(0), HeadChord::NextConsole, false, Instant::now());
    app.turn(Instant::now());
    assert_eq!(app.heads.first().unwrap().console.id, second, "shown");
    assert_eq!(
        name_and_cell(&mut app, second),
        "\x1b]50;t\x07\x1b[6;24;12t"
    );
}

// A program on console 1 switches head 0 from frit a to frit t, whose
// cell comes late. The user opens console 2 during the wait, and a
// program there asks for the font set's name and for the cell at once.
// Console 2 holds its replies, as console 1 does, so it never reports
// t's name with the 8×8 cell the head keeps during the wait. The user goes back to
// console 1 before the late 12×24 cell comes. Console 2 is hidden and
// keeps its hold. Once shown, it is laid out in t's cell, it sends its
// held replies, and it answers with t's name and t's cell together.
//
// References:
// - ADR-1200 D5: the name OSC 50's query reports.
// - ADR-1200 D17: the cell that comes after the wait.
// - ADR-1401 D1: a hidden console keeps its own grid and cell.
// covers: text.font-change
#[test]
fn a_console_shown_during_the_wait_holds_its_replies_until_the_new_cell() {
    let dir = Scratch::new("app-shown-during-wait");
    let fonts = two_cells_and_a_late_one(&dir);
    let (_lp, mut app) = seat_with(&["--font-name=a", "--font-size=0"], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    let first = app.heads.first().unwrap().console.id;
    app.term_mut().unwrap().parse(b"\x1b]50;t\x07", &mut |_| {});
    app.turn(Instant::now());
    assert!(
        app.open_console(0, Instant::now()),
        "a second console opens"
    );
    app.turn(Instant::now());
    let head = app.heads.first().unwrap();
    let second = head.console.id;
    assert_eq!(head.cell, (8, 8), "the cell the head keeps during the wait");
    assert_eq!(
        name_and_cell(&mut app, second),
        "",
        "console 2 holds its replies"
    );
    app.chord(Some(0), HeadChord::NextConsole, false, Instant::now());
    app.turn(Instant::now());
    assert_eq!(app.heads.first().unwrap().console.id, first);
    let own = vtium_font::Cell {
        width: 12,
        height: 24,
    };
    let frit = app.frits.get_mut("t").unwrap();
    assert!(
        frit.helper
            .as_mut()
            .unwrap()
            .answer_late(Ok(helper::OwnCell::Bitmap(own)))
    );
    app.turn(Instant::now());
    let head = app.heads.first_mut().unwrap();
    assert_eq!(head.cell, (12, 24), "the late cell");
    let hidden = head.hidden.iter_mut().find(|c| c.id == second).unwrap();
    assert!(
        hidden.term.terminal.replies_held(),
        "hidden console 2 keeps its hold"
    );
    assert_eq!(hidden.term.terminal.take_replies(), b"", "nothing went out");
    app.chord(Some(0), HeadChord::NextConsole, false, Instant::now());
    app.turn(Instant::now());
    let head = app.heads.first_mut().unwrap();
    assert_eq!(head.console.id, second, "console 2 is shown again");
    assert_eq!(head.cell, (12, 24), "in t's cell");
    assert!(!head.console.term.terminal.replies_held(), "the hold ended");
    assert_eq!(
        name_and_cell(&mut app, second),
        "\x1b]50;t\x07\x1b[6;24;12t"
    );
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
        let dir = Scratch::new(&format!("app-{test}"));
        std::fs::write(dir.join("t.vtfrit"), "source=outline:/nonexistent/t.ttf\n").unwrap();
        let fonts = font::Sources {
            frits: dir.to_path_buf(),
            trust: font::trust().below(&dir),
            helper: Some(helper::Command {
                program: helper::stand_in_shell().to_path_buf(),
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
    let (_lp, mut app, _dir) = late_seat("late-heads");
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
    let (_lp, mut app, _dir) = late_seat("late-missing");
    let at = app.heads.first().map(|h| h.font_size).unwrap();
    let cell = |w, h| vtium_font::Cell {
        width: w,
        height: h,
    };
    let in_time = Ok(helper::OwnCell::Bitmap(cell(8, 16)));
    let refused = Err(helper::NoCell {
        why: "no such font".to_owned(),
        late: false,
        from_frit: true,
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
        from_frit: true,
    });
    app.late_cell("t", at + 2, late_refusal.clone());
    let known = app.frits.get("t").map(|f| f.own.clone()).unwrap();
    assert_eq!(known.get(&at), Some(&in_time));
    assert_eq!(known.get(&(at + 1)), Some(&refused));
    assert_eq!(known.get(&(at + 2)), Some(&late_refusal));
    assert!(app.heads.iter().all(|h| h.refont), "nothing is laid out");
}

// The font helper's late answer that a font set has no character cell
// of its own is logged with the font set named first, then the helper's
// reason, which names the font file, then what vtium does and how to see
// and activate the fonts the set lacks. A helper that could not answer
// at all blames no font, so its line gives no advice about fonts.
//
// References:
// - ADR-1200 D17: a refusal leaves the display's cell in use.
// - ADR-1205 D4: a font draws only while it is among the active fonts.
#[test]
fn a_late_refusal_names_the_font_set_and_the_helpers_reason_and_what_to_do() {
    let (_lp, mut app, _dir) = late_seat("late-refusal-line");
    let at = app.heads.first().map(|h| h.font_size).unwrap();
    crate::getty::tests::logged();
    let why = "the text font source=outline:/usr/share/fonts/TTF/DejaVuSansMono.ttf is missing or cannot be read, so the built-in font draws the text in its place, and the cell comes from the configuration";
    app.late_cell(
        "t",
        at + 2,
        Err(helper::NoCell {
            why: why.to_owned(),
            late: false,
            from_frit: true,
        }),
    );
    app.late_cell(
        "t",
        at + 3,
        Err(helper::NoCell {
            why: "the helper ended".to_owned(),
            late: false,
            from_frit: false,
        }),
    );
    let lines: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.starts_with("The font set t "))
        .collect();
    assert_eq!(
        lines,
        [
            format!(
                "The font set t has no character cell of its own. The font helper answered after vtium stopped waiting for it: {why}. The font set keeps the character cell it draws in now. To see each font the font set lacks, and to activate a font that is not active, run vtium config fonts."
            ),
            "The font set t has no character cell of its own. The font helper could not answer: the helper ended. The font set keeps the character cell it draws in now.".to_owned(),
        ]
    );
}

// ADR-1200 D17: a bitmap frit's answer holds at every height, so a
// refusal that came in time at one height holds at all of them. A late
// bitmap cell is then dropped, at the start's height, whose query had
// no answer in time, and at a height no query was asked at, and it
// never becomes the cell that `answer_at`, which looks for a bitmap
// cell first, finds.
#[test]
fn a_late_bitmap_cell_yields_to_a_refusal_in_time_at_any_height() {
    let (_lp, mut app, _dir) = late_seat("late-refused");
    let at = app.heads.first().map(|h| h.font_size).unwrap();
    let refused = Err(helper::NoCell {
        why: "no such font".to_owned(),
        late: false,
        from_frit: true,
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
    let (_lp, mut app, _dir) = late_seat("late-kind");
    let at = 16;
    let outline = Ok(helper::OwnCell::Outline(vtium_font::Cell {
        width: 9,
        height: at,
    }));
    let failures = [
        Err(helper::NoCell {
            why: "no answer in time".to_owned(),
            late: true,
            from_frit: false,
        }),
        Err(helper::NoCell {
            why: "no such font".to_owned(),
            late: false,
            from_frit: true,
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
}

/// A seat on one display under `render-timer=value`, shown at the
/// returned time, its first frames handed back.
fn timer_seat(value: &str) -> (Loop<App>, App, Instant) {
    timer_seat_with(value, &[])
}

/// [`timer_seat`], with the options `more` too.
fn timer_seat_with(value: &str, more: &[&str]) -> (Loop<App>, App, Instant) {
    let option = format!("--render-timer={value}");
    let options: Vec<&str> = ["--font-size=16", option.as_str()]
        .into_iter()
        .chain(more.iter().copied())
        .collect();
    let (mut lp, mut app) = seat_with(&options, None, no_fonts());
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

/// A stand-in for the kernel: the records the test queues, at the
/// console log level 7, with no Plymouth.
struct QueuedKernel(Rc<RefCell<Vec<Vec<u8>>>>);

impl crate::kernel::Kernel for QueuedKernel {
    fn drain(&mut self) -> io::Result<Vec<Vec<u8>>> {
        Ok(std::mem::take(&mut *self.0.borrow_mut()))
    }
    fn console_level(&self) -> u8 {
        7
    }
    fn plymouth_runs(&self) -> bool {
        false
    }
}

/// The records a test queues for its stand-in kernel.
type Queued = Rc<RefCell<Vec<Vec<u8>>>>;

/// Queues a kernel warning `text` on `queued`, as `/dev/kmsg` gives it.
fn kernel_log(queued: &RefCell<Vec<Vec<u8>>>, text: &str) {
    queued
        .borrow_mut()
        .push(format!("4,1,2,-;{text}\n").into_bytes());
}

/// A seat on one display under `kernel-messages=always`, its kernel a
/// stand-in that reads `queued`, shown at the returned time. The
/// display reports no flip, so `display-wait=forever` keeps vtium from
/// giving up on it while the count runs, and `no-blink` arms no blink
/// timer, so the box's count is the loop's only timer.
fn kernel_seat() -> (Loop<App>, App, Queued, Instant) {
    kernel_seat_with(&[])
}

/// [`kernel_seat`], with the options `more` too.
fn kernel_seat_with(more: &[&str]) -> (Loop<App>, App, Queued, Instant) {
    let queued: Queued = Rc::default();
    let options: Vec<&str> = [
        "--font-size=16",
        "--kernel-messages=always",
        "--display-wait=forever",
        "--no-blink",
    ]
    .into_iter()
    .chain(more.iter().copied())
    .collect();
    let (mut lp, mut app) = seat_with(&options, None, no_fonts());
    app.set_kernel(Box::new(QueuedKernel(Rc::clone(&queued))));
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    let t = Instant::now();
    app.vt.activate().unwrap();
    app.turn(t);
    settle_frames(&mut lp, &mut app, t);
    (lp, app, queued, t)
}

/// `app` reads the kernel's messages and draws at `at`.
fn kernel_turn(lp: &mut Loop<App>, app: &mut App, at: Instant) {
    assert!(app.kernel_ready());
    app.turn(at);
    settle_frames(lp, app, at);
}

/// A key press of `keysym` with no modifier.
fn press(keysym: u32) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        unshifted: 0,
        shifted: 0,
        base: None,
        mods: InputMods::empty(),
        keysyms: vec![keysym],
        codepoints: char::from_u32(keysym)
            .filter(|c| !c.is_control() && keysym < 0xff00)
            .into_iter()
            .map(Some)
            .collect(),
        repeat: false,
    })
}

/// Whether the key `app` last took went on to the shown console's
/// program.
fn key_sent(app: &App) -> bool {
    let head = app.heads.first().unwrap();
    head.views.values().all(|v| v.after_key(&head.console.term))
}

// ADR-1401 D15: vtium shows a kernel warning over the console of the
// head on the boot display, in the notices' box, and never writes it
// into the console's screen or its pty. A message that arrives while
// the VT is not shown is never shown.
#[test]
fn a_kernel_warning_shows_over_the_console_and_never_in_it() {
    let queued: Rc<RefCell<Vec<Vec<u8>>>> = Rc::default();
    let (mut lp, mut app) = seat_with(
        &["--font-size=16", "--kernel-messages=always"],
        None,
        no_fonts(),
    );
    app.set_kernel(Box::new(QueuedKernel(Rc::clone(&queued))));
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    kernel_log(&queued, "before the VT is shown");
    assert!(app.kernel_ready());
    let t = Instant::now();
    app.vt.activate().unwrap();
    app.turn(t);
    settle_frames(&mut lp, &mut app, t);
    assert_eq!(box_text(&app), None, "nothing from before the VT was shown");

    kernel_log(&queued, "usb 1-2: device descriptor read/64, error -71");
    kernel_turn(&mut lp, &mut app, t);
    assert_eq!(
        box_text(&app).as_deref(),
        Some(
            "usb 1-2: device descriptor read/64, error -71\nPress Esc to dismiss this message (15)"
        )
    );
    assert!(first_overlay(&app).problem, "in the notices' colors");
    assert!(
        app.screen_text().iter().all(|row| !row.contains("usb 1-2")),
        "the console's screen holds no kernel message: {:?}",
        app.screen_text()
    );
}

// ADR-1401 D15: vtium shows the kernel's messages only while its VT
// is shown. A message that arrives while the VT is released, its
// display still lit, is never shown, not even once the VT comes back.
#[test]
fn a_kernel_message_while_the_vt_is_released_never_shows() {
    let (mut lp, mut app, queued, t) = kernel_seat();
    // Ctrl+Super+F12, the fake VT's own key, releases it
    let f12 = InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        unshifted: 0,
        shifted: 0,
        base: None,
        mods: InputMods::CTRL.union(InputMods::LOGO),
        keysyms: vec![0xffc9],
        codepoints: vec![None],
        repeat: false,
    });
    // it sleeps as a DRM card does, so the release is acknowledged
    let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
    still
        .as_any()
        .downcast_mut::<Still>()
        .unwrap()
        .like_a_device = true;
    app.focus = Some(0);
    app.input_event(f12, t);
    app.turn(t);
    app.turn(t);
    assert!(!app.vt.is_active(), "released");
    assert!(
        !app.heads.first().unwrap().views.is_empty(),
        "the display stays lit"
    );
    kernel_log(&queued, "while released");
    kernel_turn(&mut lp, &mut app, t);
    app.vt.activate().unwrap();
    app.turn(t);
    settle_frames(&mut lp, &mut app, t);
    assert!(app.vt.is_active(), "taken back");
    assert_eq!(app.heads.first().unwrap().kernel_messages().next(), None);
    assert_eq!(box_text(&app), None, "no stale box once the VT is back");
}

// ADR-1401 D15: the box's last line counts the seconds left, one a
// second on the loop's timer, and at 0 the box goes on its own.
#[test]
fn the_kernel_box_counts_down_each_second_and_goes_at_0() {
    let (mut lp, mut app, queued, t) = kernel_seat();
    kernel_log(&queued, "a warning");
    kernel_turn(&mut lp, &mut app, t);
    for n in 1..15u64 {
        let at = t + Duration::from_secs(n);
        assert_eq!(
            app.heads.first().unwrap().kernel_due(),
            Some(at),
            "the box's timer is due at second {n}"
        );
        assert_eq!(
            app.next_wake(at - Duration::from_secs(1)),
            Some(Duration::from_secs(1)),
            "the loop wakes for second {n}, on the box's timer alone"
        );
        app.turn(at);
        settle_frames(&mut lp, &mut app, at);
        assert_eq!(
            box_text(&app),
            Some(format!(
                "a warning\nPress Esc to dismiss this message ({})",
                15 - n
            )),
            "the frame after {n} ticks"
        );
    }
    let gone = t + crate::head::KERNEL_SHOWN;
    app.turn(gone);
    settle_frames(&mut lp, &mut app, gone);
    assert_eq!(box_text(&app), None, "gone at 0");
}

// ADR-1401 D15: a new message starts the count again from 15.
#[test]
fn a_new_kernel_message_restarts_the_count() {
    let (mut lp, mut app, queued, t) = kernel_seat();
    kernel_log(&queued, "first");
    kernel_turn(&mut lp, &mut app, t);
    let later = t + Duration::from_secs(5);
    app.turn(later);
    settle_frames(&mut lp, &mut app, later);
    assert_eq!(
        box_text(&app).as_deref(),
        Some("first\nPress Esc to dismiss this message (10)")
    );
    kernel_log(&queued, "second");
    kernel_turn(&mut lp, &mut app, later);
    assert_eq!(
        box_text(&app).as_deref(),
        Some("first\nsecond\nPress Esc to dismiss these messages (15)")
    );
    let end = later + crate::head::KERNEL_SHOWN;
    app.turn(end - Duration::from_secs(1));
    assert!(
        app.heads
            .first()
            .unwrap()
            .kernel_messages()
            .next()
            .is_some()
    );
    app.turn(end);
    assert_eq!(app.heads.first().unwrap().kernel_messages().next(), None);
}

// ADR-1401 D15: Esc dismisses the kernel's messages, and vtium keeps
// that Esc from the console's program; the frame after it draws no box.
#[test]
fn esc_dismisses_the_kernel_box_and_stays_with_vtium() {
    let (mut lp, mut app, queued, t) = kernel_seat();
    kernel_log(&queued, "a warning");
    kernel_turn(&mut lp, &mut app, t);
    app.focus = Some(0);
    app.input_event(press(0xff1b), t);
    assert_eq!(app.heads.first().unwrap().kernel_messages().next(), None);
    assert!(!key_sent(&app), "the Esc stays with vtium");
    app.turn(t);
    settle_frames(&mut lp, &mut app, t);
    assert_eq!(box_text(&app), None, "the frame after Esc draws no box");
    // with no kernel message on screen, Esc is the program's again
    app.input_event(press(0xff1b), t);
    assert!(key_sent(&app), "an Esc with no box goes to the program");
}

// ADR-1401 D15: any other key dismisses the kernel's messages too, and
// goes on to the console's program.
#[test]
fn another_key_dismisses_the_kernel_box_and_reaches_the_program() {
    let (mut lp, mut app, queued, t) = kernel_seat();
    kernel_log(&queued, "a warning");
    kernel_turn(&mut lp, &mut app, t);
    app.focus = Some(0);
    app.input_event(press(u32::from('x')), t);
    assert_eq!(app.heads.first().unwrap().kernel_messages().next(), None);
    assert!(key_sent(&app), "the key went on to the program");
    app.turn(t);
    settle_frames(&mut lp, &mut app, t);
    assert_eq!(box_text(&app), None, "the key's frame draws no box");
}

// ADR-1002, section 7: Ctrl+Shift+O starts link hints, every key
// while they are on stays with vtium, the label's letter copies the
// link's address, and the next key ends the line that shows the copy
// and goes on to the program.
#[test]
fn link_hints_take_the_keys_and_a_label_copies_its_link() {
    let (_lp, mut app, _queued, t) = kernel_seat();
    app.focus = Some(0);
    let head = app.heads.first_mut().unwrap();
    head.console
        .term
        .terminal
        .input(b"\x1b]8;;x:there\x1b\\link\x1b]8;;\x1b\\");
    let mut chord = press(u32::from('O'));
    if let InputEvent::Key(k) = &mut chord {
        k.mods = InputMods::CTRL.union(InputMods::SHIFT);
    }
    app.input_event(chord, t);
    assert!(app.heads.first().unwrap().hints.is_some());
    assert!(!key_sent(&app), "the hints key stays with vtium");
    app.input_event(press(u32::from('q')), t);
    assert!(!key_sent(&app), "a letter no label begins stays too");
    assert!(app.heads.first().unwrap().hints.is_some());
    app.input_event(press(u32::from('a')), t);
    assert!(!key_sent(&app), "the label's letter stays with vtium");
    assert!(app.heads.first().unwrap().hints.is_none());
    assert_eq!(app.selection.as_deref(), Some(&b"x:there"[..]));
    let head = app.heads.first().unwrap();
    assert_eq!(head.link_overlay().as_deref(), Some("Copied: x:there"));
    app.input_event(press(u32::from('x')), t);
    assert!(key_sent(&app), "the next key reaches the program");
    assert_eq!(app.heads.first().unwrap().link_overlay(), None);
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

/// Under `osd = silent`, vtium draws no render timer figures on screen.
/// `render-timer = osd` then shows nothing, so vtium measures nothing,
/// and `render-timer = both` measures for the log line alone.
///
/// References:
/// - ADR-1401 D5, "The key `osd` and the render timer".
#[test]
fn under_osd_silent_the_render_timer_draws_no_figures() {
    for (value, measures) in [("osd", false), ("both", true)] {
        let (mut lp, mut app, t) = timer_seat_with(value, &["--osd=silent"]);
        let measured = app
            .heads
            .first()
            .and_then(|h| h.views.values().next())
            .is_some_and(|v| v.timer.is_some());
        assert_eq!(measured, measures, "{value}: the console measures");
        let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
        let still = still.as_any().downcast_mut::<Still>().unwrap();
        still.events.extend([Event::Flipped(DisplayId(1)); 3]);
        let second = t + Duration::from_secs(1);
        app.turn(second);
        settle_frames(&mut lp, &mut app, second);
        assert_eq!(box_text(&app), None, "{value}: no line on screen");
    }
}

/// Under `osd = silent`, the kernel's messages still show. They are the
/// kernel's text, not vtium's own notices, and `kernel-messages`
/// governs them.
///
/// References:
/// - ADR-1401 D5, "The kernel's messages".
#[test]
fn under_osd_silent_the_kernels_messages_still_show() {
    let (mut lp, mut app, queued, t) = kernel_seat_with(&["--osd=silent"]);
    kernel_log(&queued, "usb 1-2: device descriptor read/64, error -71");
    kernel_turn(&mut lp, &mut app, t);
    assert_eq!(
        box_text(&app).as_deref(),
        Some(
            "usb 1-2: device descriptor read/64, error -71\nPress Esc to dismiss this message (15)"
        )
    );
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

/// A shown VT whose seat manager is not ready for a device until the
/// test says so, recording what the app asks of it.
#[derive(Default)]
struct Waits {
    ready: bool,
    settled: Vec<PathBuf>,
    asked: Rc<std::cell::RefCell<Vec<PathBuf>>>,
    closed: Rc<std::cell::RefCell<Vec<PathBuf>>>,
    events: Vec<VtEvent>,
}

impl Vt for Waits {
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
    fn bell(&mut self) {}
    fn number(&self) -> Option<u16> {
        None
    }
    fn open_device(&mut self, path: &Path) -> io::Result<std::os::fd::OwnedFd> {
        self.asked.borrow_mut().push(path.to_owned());
        if !self.ready {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        Ok(std::fs::File::open("/dev/null")?.into())
    }
    fn close_device(&mut self, path: &Path) {
        self.closed.borrow_mut().push(path.to_owned());
    }
    fn settled(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.settled)
    }
}

// vtium/vtium#34: a framebuffer the seat manager is not ready to give
// waits, opens in the turn the VT names it settled, and is let go
// through the VT when udev reports it gone.
#[test]
fn a_video_device_the_seat_manager_is_not_ready_for_opens_once_settled() {
    let loaded = crate::config::load(&[], &mut |_: &Path| Ok(None));
    let lp: Loop<App> = Loop::new().unwrap();
    let (asked, closed) = (Rc::default(), Rc::default());
    let waits = Waits {
        asked: Rc::clone(&asked),
        closed: Rc::clone(&closed),
        ..Waits::default()
    };
    let mut app = App::new(
        loaded.config,
        Box::new(waits),
        lp.handle(),
        Rc::new(Instant::now),
        Vec::new(),
    );
    let fb = PathBuf::from("/dev/fb0");
    let added = vtium_platform::udev::Device {
        kind: DeviceKind::Fbdev,
        node: fb.clone(),
        flags: vtium_platform::udev::DeviceFlags::default(),
        id_path: None,
        input: None,
    };
    app.device_event(DeviceEvent::Added(added));
    assert_eq!(*asked.borrow(), std::slice::from_ref(&fb));
    assert!(!app.videos.contains_key(&fb), "it waits");
    app.turn(Instant::now());
    assert_eq!(asked.borrow().len(), 1, "not asked again before it settles");
    {
        let waits = app.vt.get::<Waits>().unwrap();
        waits.ready = true;
        waits.settled = vec![fb.clone()];
    }
    app.turn(Instant::now());
    assert_eq!(*asked.borrow(), [fb.clone(), fb.clone()]);
    assert!(app.videos.contains_key(&fb), "opened once settled");
    app.device_event(DeviceEvent::Removed {
        kind: DeviceKind::Fbdev,
        node: fb.clone(),
    });
    assert!(!app.videos.contains_key(&fb));
    assert_eq!(*closed.borrow(), [fb]);
}

/// The devices a [`Waits`] VT was asked for or told to close.
type Recorded = Rc<std::cell::RefCell<Vec<PathBuf>>>;

/// An app on a [`Waits`] VT, ready or not, with what it asks and closes.
fn waiting_app(ready: bool) -> (Loop<App>, App, Recorded, Recorded) {
    let loaded = crate::config::load(&[], &mut |_: &Path| Ok(None));
    let lp: Loop<App> = Loop::new().unwrap();
    let (asked, closed) = (Rc::default(), Rc::default());
    let waits = Waits {
        ready,
        asked: Rc::clone(&asked),
        closed: Rc::clone(&closed),
        ..Waits::default()
    };
    let app = App::new(
        loaded.config,
        Box::new(waits),
        lp.handle(),
        Rc::new(Instant::now),
        Vec::new(),
    );
    (lp, app, asked, closed)
}

fn added(kind: DeviceKind, node: &Path) -> DeviceEvent {
    DeviceEvent::Added(vtium_platform::udev::Device {
        kind,
        node: node.to_owned(),
        flags: vtium_platform::udev::DeviceFlags::default(),
        id_path: None,
        input: None,
    })
}

// vtium/vtium#34: a device the VT opened that is no DRM card, here
// /dev/null, fails to wrap, and the VT is told to close it.
#[test]
fn a_device_that_fails_to_wrap_is_closed_through_the_vt() {
    let (_lp, mut app, asked, closed) = waiting_app(true);
    let card = PathBuf::from("/dev/dri/card0");
    app.device_event(added(DeviceKind::Drm, &card));
    assert_eq!(*asked.borrow(), std::slice::from_ref(&card));
    assert!(!app.videos.contains_key(&card));
    assert_eq!(*closed.borrow(), [card]);
}

// vtium/vtium#246: with three buffers, a frame the render thread
// composed while the display's flip was in flight is refused as busy,
// and the display keeps it. The loop sends that frame as it is once the
// flip ends, and composes nothing for it.
#[test]
fn the_loop_sends_a_frame_kept_from_a_busy_swap_once_the_flip_ends() {
    let (mut lp, mut app) = seat(None, no_fonts());
    let node = PathBuf::from("/dev/mem0");
    app.add_video(node.clone(), Box::new(Still::new(&[(1, "A")])));
    shown(&mut app);
    let t = Instant::now();
    settle(&mut lp, &mut app, t);
    let counts = |app: &mut App| {
        let still = app
            .videos
            .get_mut(&node)
            .and_then(|v| v.as_any().downcast_mut::<Still>())
            .unwrap();
        (still.swaps, still.committed)
    };
    let flipping = |app: &mut App, on: bool| {
        let still = app.videos.get_mut(&node).unwrap();
        still.as_any().downcast_mut::<Still>().unwrap().flipping = on;
    };
    let (swaps, _) = counts(&mut app);
    assert!(swaps >= 1, "the first frame");
    flipping(&mut app, true);
    // as the pty's output does
    app.term_mut().unwrap().changed = true;
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert_eq!(counts(&mut app), (swaps + 1, 0), "the frame waits");
    app.turn(t);
    assert_eq!(counts(&mut app), (swaps + 1, 0), "while the flip lasts");
    flipping(&mut app, false);
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert_eq!(counts(&mut app), (swaps + 1, 1), "sent, not composed again");
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert_eq!(counts(&mut app), (swaps + 1, 1), "once");
}

// A row of history that output moves above the screen in one read, so
// that no row of the screen ever shows it, has its glyphs asked for before
// the frame that draws it in the overscan. The view asked its frames to
// hold the rows above the screen when it took the first frame, and what
// the displays show, which the asks to the font helper read, holds those
// rows from then on. Here salam's lam-alef ligature is such a glyph.
//
// References:
// - ADR-1310 D2: the rows above the screen that a view asks for.
// - ADR-1004 D6: the key of the lam-alef ligature.
#[cfg(all(feature = "bidi", feature = "scroll-pan"))]
#[test]
fn a_row_of_history_no_screen_row_showed_is_asked_for_before_it_is_drawn() {
    let (mut lp, mut app) = seat(None, no_fonts());
    let node = PathBuf::from("/dev/mem0");
    app.add_video(node.clone(), Box::new(Still::new(&[(1, "A")])));
    shown(&mut app);
    let t = Instant::now();
    settle(&mut lp, &mut app, t);
    let swaps = app
        .videos
        .get_mut(&node)
        .and_then(|v| v.as_any().downcast_mut::<Still>())
        .map_or(0, |still| still.swaps);
    assert_eq!(swaps, 1, "one frame drawn");
    let ligature = vec!['\u{200d}', '\u{644}', '\u{627}'];
    let asked = |app: &App| {
        app.heads
            .first()
            .and_then(|head| head.visible().into_iter().next())
            .is_some_and(|shown| shown.clusters.contains(&ligature))
    };
    let head = app.heads.first_mut().unwrap();
    let mut output = "سلام".to_owned();
    for _ in 0..head.grid.1 {
        output.push_str("\r\nx");
    }
    head.console.term.terminal.input(output.as_bytes());
    assert!(
        !head.screen_text().iter().any(|l| l.contains('\u{644}')),
        "salam lies above the screen"
    );
    assert!(asked(&app), "asked before the next frame draws it");
}

// ADR-1401 D2: the loop hands a synchronized update's end to each
// display as the end of an update, not as a draw asked at once, so on a
// display whose draws wait for the flip, an end no key caused composes
// nothing while the flip is in flight, and one frame once it ends.
#[test]
fn the_loop_holds_an_update_no_key_caused_for_the_flip() {
    let (mut lp, mut app) = seat(None, no_fonts());
    let node = PathBuf::from("/dev/mem0");
    let mut device = Still::new(&[(1, "A")]);
    device.waits_for_flip = true;
    app.add_video(node.clone(), Box::new(device));
    shown(&mut app);
    let t = Instant::now();
    settle(&mut lp, &mut app, t);
    fn still<'a>(app: &'a mut App, node: &Path) -> &'a mut Still {
        app.videos
            .get_mut(node)
            .and_then(|v| v.as_any().downcast_mut::<Still>())
            .unwrap()
    }
    let swaps = still(&mut app, &node).swaps;
    assert!(swaps >= 1, "the first frame");
    still(&mut app, &node).flipping = true;
    // as the end of an update in the pty's output does
    app.term_mut().unwrap().present = true;
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert_eq!(
        still(&mut app, &node).swaps,
        swaps,
        "nothing composed in the flip"
    );
    still(&mut app, &node).flipping = false;
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert_eq!(
        still(&mut app, &node).swaps,
        swaps + 1,
        "one frame at its end"
    );
}

/// The seat at 16 points on a [`Waits`] VT it has acquired, and the
/// devices the VT is told to close.
fn acquired_seat() -> (Loop<App>, App, Recorded) {
    let loaded = crate::config::load(&["--font-size=16".into()], &mut |_: &Path| Ok(None));
    let lp: Loop<App> = Loop::new().unwrap();
    let closed: Recorded = Rc::default();
    let waits = Waits {
        ready: true,
        closed: Rc::clone(&closed),
        events: vec![VtEvent::Acquired],
        ..Waits::default()
    };
    let mut app = App::new(
        loaded.config,
        Box::new(waits),
        lp.handle(),
        Rc::new(Instant::now),
        Vec::new(),
    );
    app.set_font_sources(no_fonts());
    (lp, app, closed)
}

// vtium/vtium#295: a card whose removal the loop first meets at the end
// of a turn, here as the commit of a frame kept from a busy swap, is let
// go in that turn: the loop may wait for no timer, key or udev event
// after it, and the card's file must not stay open meanwhile. The guard
// owed for its displays wakes the loop at once.
#[test]
fn a_card_found_gone_after_the_drain_is_let_go_in_that_turn() {
    let (mut lp, mut app, closed) = acquired_seat();
    let card = PathBuf::from("/dev/dri/card0");
    let device = Still::new(&[(1, "A")]);
    let file = Rc::clone(&device.file);
    app.add_video(card.clone(), Box::new(device));
    let t = Instant::now();
    app.turn(t);
    settle(&mut lp, &mut app, t);
    fn still<'a>(app: &'a mut App, card: &Path) -> &'a mut Still {
        app.videos
            .get_mut(card)
            .and_then(|v| v.as_any().downcast_mut::<Still>())
            .unwrap()
    }
    still(&mut app, &card).flipping = true;
    app.term_mut().unwrap().changed = true;
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert!(still(&mut app, &card).kept, "a frame waits for the flip");
    let device = still(&mut app, &card);
    device.flipping = false;
    device.unplug_at_commit = true;
    app.turn(t);
    assert!(!app.videos.contains_key(&card), "let go in that turn");
    assert_eq!(Rc::strong_count(&file), 1, "its file is closed");
    assert_eq!(*closed.borrow(), std::slice::from_ref(&card));
    assert_eq!(
        app.next_wake(t),
        Some(Duration::ZERO),
        "the guard runs next"
    );
}

// vtium/vtium#295: a card the kernel removed, which drm2d found gone at
// its first ENODEV, is let go in the turn that drains its displays'
// removals: its file is closed and the VT is told to close it, with no
// report from udev, which early in boot may never come. A new card the
// kernel then adds at the same node opens and draws.
#[test]
fn a_card_found_gone_is_let_go_without_udev_and_a_new_one_opens() {
    let (_lp, mut app, closed) = acquired_seat();
    let card = PathBuf::from("/dev/dri/card0");
    let old = Still::new(&[(1, "A")]);
    let file = Rc::clone(&old.file);
    app.add_video(card.clone(), Box::new(old));
    app.turn(Instant::now());
    let views = |app: &App| app.heads.iter().map(|h| h.views.len()).sum::<usize>();
    assert_eq!(views(&app), 1, "the card's display draws");
    app.videos
        .get_mut(&card)
        .and_then(|v| v.as_any().downcast_mut::<Still>())
        .unwrap()
        .unplug();
    app.turn(Instant::now());
    assert!(!app.videos.contains_key(&card), "the card is let go");
    assert_eq!(Rc::strong_count(&file), 1, "its file is closed");
    assert_eq!(*closed.borrow(), std::slice::from_ref(&card));
    assert_eq!(views(&app), 0);
    app.add_video(card.clone(), Box::new(Still::new(&[(1, "A")])));
    app.turn(Instant::now());
    assert!(app.videos.contains_key(&card), "the new card opens");
    assert_eq!(views(&app), 1, "the new card's display draws");
    assert_eq!(closed.borrow().len(), 1, "the new card stays open");
}

// vtium/vtium#34: a device udev reports removed while it waits for the
// seat manager is not opened when the VT later names it settled.
#[test]
fn a_waiting_video_device_removed_is_not_opened_once_settled() {
    let (_lp, mut app, asked, closed) = waiting_app(false);
    let fb = PathBuf::from("/dev/fb0");
    app.device_event(added(DeviceKind::Fbdev, &fb));
    app.device_event(DeviceEvent::Removed {
        kind: DeviceKind::Fbdev,
        node: fb.clone(),
    });
    assert_eq!(*closed.borrow(), std::slice::from_ref(&fb));
    {
        let waits = app.vt.get::<Waits>().unwrap();
        waits.ready = true;
        waits.settled = vec![fb.clone()];
    }
    app.turn(Instant::now());
    assert_eq!(asked.borrow().len(), 1, "not asked again");
    assert!(!app.videos.contains_key(&fb));
}

/// A VT the kernel asks for while the test says, as SIGUSR2 waits in
/// the signalfd, counting the releases vtium acknowledged.
struct Asks {
    active: bool,
    asked: Rc<std::cell::Cell<bool>>,
    released: Rc<std::cell::Cell<u32>>,
    events: Vec<VtEvent>,
}

impl Vt for Asks {
    fn release_check(&self) -> Option<vtium_platform::display::Interrupt> {
        let asked = Rc::clone(&self.asked);
        Some(Rc::new(move || asked.get()))
    }
    fn take_release(&mut self) {
        if self.asked.replace(false) {
            self.events.push(VtEvent::Release);
        }
    }
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
        self.events.drain(..)
    }
    fn acknowledge_release(&mut self) -> io::Result<()> {
        self.active = false;
        self.released.set(self.released.get().saturating_add(1));
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
        self.active
    }
    fn key(&mut self, _: &KeyEvent) -> bool {
        false
    }
    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn bell(&mut self) {}
    fn number(&self) -> Option<u16> {
        Some(1)
    }
}

/// The seat on an [`Asks`] VT it has acquired, with two cards. Returns
/// the app, the cell through which the kernel asks for the VT, the
/// count of releases vtium acknowledged and both cards.
fn asking_seat() -> (
    App,
    Rc<std::cell::Cell<bool>>,
    Rc<std::cell::Cell<u32>>,
    PathBuf,
    PathBuf,
) {
    let loaded = crate::config::load(&["--font-size=16".into()], &mut |_: &Path| Ok(None));
    let lp: Loop<App> = Loop::new().unwrap();
    let released = Rc::new(std::cell::Cell::new(0));
    let vt = Asks {
        active: true,
        asked: Rc::new(std::cell::Cell::new(false)),
        released: Rc::clone(&released),
        events: vec![VtEvent::Acquired],
    };
    let check = Rc::clone(&vt.asked);
    let mut app = App::new(
        loaded.config,
        Box::new(vt),
        lp.handle(),
        Rc::new(Instant::now),
        Vec::new(),
    );
    app.set_font_sources(no_fonts());
    let (card0, card1) = (
        PathBuf::from("/dev/dri/card0"),
        PathBuf::from("/dev/dri/card1"),
    );
    let mut first = Still::new(&[(1, "A")]);
    first.like_a_device = true;
    let mut second = Still::new(&[(2, "B")]);
    second.like_a_device = true;
    app.add_video(card0.clone(), Box::new(first));
    app.add_video(card1.clone(), Box::new(second));
    app.turn(Instant::now());
    assert!(app.awake, "the VT is vtium's");
    (app, check, released, card0, card1)
}

fn hotplug(node: &Path) -> DeviceEvent {
    DeviceEvent::Hotplug {
        kind: DeviceKind::Drm,
        node: node.to_owned(),
    }
}

fn hotplugs(app: &mut App, card: &Path) -> u32 {
    app.videos
        .get_mut(card)
        .and_then(|v| v.as_any().downcast_mut::<Still>())
        .unwrap()
        .hotplugs
}

// A request for the VT that comes while a card scans, as during a
// slow connector probe, is answered in the same turn: the loop lets the
// VT go before the turn ends and leaves the other card's scan to the
// acquire (ADR-1401 D2, "A VT release while display work runs").
// covers: platform.vt-switch
#[test]
fn a_release_asked_during_a_scan_is_acknowledged_in_that_turn() {
    let (mut app, asked, released, card0, card1) = asking_seat();
    app.videos
        .get_mut(&card0)
        .and_then(|v| v.as_any().downcast_mut::<Still>())
        .unwrap()
        .asks_for_vt = Some(asked);
    app.queued_devices = vec![hotplug(&card0), hotplug(&card1)];
    app.turn(Instant::now());
    assert_eq!(hotplugs(&mut app, &card0), 1, "card0 scanned");
    assert_eq!(hotplugs(&mut app, &card1), 0, "left to the acquire");
    assert_eq!(released.get(), 1, "the VT went in the same turn");
}

// A request for the VT that comes during the turn's last scan is
// answered in that turn too, though no other card's work follows it.
// covers: platform.vt-switch
#[test]
fn a_release_asked_during_the_last_scan_is_acknowledged_in_that_turn() {
    let (mut app, asked, released, card0, _) = asking_seat();
    app.videos
        .get_mut(&card0)
        .and_then(|v| v.as_any().downcast_mut::<Still>())
        .unwrap()
        .asks_for_vt = Some(asked);
    app.queued_devices = vec![hotplug(&card0)];
    app.turn(Instant::now());
    assert_eq!(hotplugs(&mut app, &card0), 1, "card0 scanned");
    assert_eq!(released.get(), 1, "the VT went in the same turn");
}

// A card scans once for all the hotplugs udev reported for it in one
// turn (ADR-1401 D2, "A VT release while display work runs").
#[test]
fn a_card_scans_once_for_the_hotplugs_of_a_turn() {
    let (mut app, _, released, card0, card1) = asking_seat();
    app.queued_devices = vec![hotplug(&card1), hotplug(&card1), hotplug(&card1)];
    app.turn(Instant::now());
    assert_eq!(hotplugs(&mut app, &card1), 1, "one scan for three hotplugs");
    assert_eq!(hotplugs(&mut app, &card0), 0);
    assert_eq!(released.get(), 0, "the VT stays");
}

// ADR-1401 D2: the pty's channel, read through calloop as `main` turns
// the loop, shows every frame of a stream of synchronized updates whose
// reads each hold the end of one update and the start of the next. The
// test writes each read to the session's pty and turns the loop until
// the frame composes. The loop then asks to wake at once, and its next
// turn parses the next update's start.
#[test]
fn the_pty_channel_shows_each_frame_of_back_to_back_updates() {
    use std::io::Write;
    let args: Vec<OsString> = ["--font-size=16", "--login", "--", "/bin/sh", "-c", "read x"]
        .iter()
        .map(OsString::from)
        .collect();
    let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
    let mut lp: Loop<App> = Loop::new().unwrap();
    // The seat's clock moves only by the short waits the loop asks for, the
    // refresh's pacing among them, and stands still while the loop waits
    // for the pty or a render thread. So an update's hold, whose bound is
    // SYNC_WAIT, never runs out while a loaded machine is slow to compose.
    let at = Rc::new(std::cell::Cell::new(Instant::now()));
    let clock = {
        let at = Rc::clone(&at);
        move || at.get()
    };
    let mut app = App::new(
        loaded.config,
        Box::new(FakeVt::new()),
        lp.handle(),
        Rc::new(clock.clone()),
        vec![("PATH".into(), "/usr/bin:/bin".into())],
    );
    app.set_font_sources(no_fonts());
    let node = PathBuf::from("/dev/mem0");
    app.add_video(node.clone(), Box::new(Still::new(&[(1, "A")])));
    shown(&mut app);
    app.start();
    // Turns the loop as `main` does, waiting at most 50 ms, until
    // `done` holds, for at most 5 s of the host's time. A wait of at most
    // 50 ms that the loop asks for moves the seat's clock by that wait.
    let turn_until = |lp: &mut Loop<App>, app: &mut App, done: &dyn Fn(&mut App) -> bool| {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if done(app) {
                return true;
            }
            let short = Duration::from_millis(50);
            let asked = app.next_wake(at.get());
            let wait = asked.map_or(short, |w| w.min(short));
            lp.dispatch(Some(wait), app).unwrap();
            if let Some(w) = asked.filter(|&w| w <= short) {
                at.set(at.get() + w);
            }
            app.turn(at.get());
        }
        done(app)
    };
    assert!(
        turn_until(&mut lp, &mut app, &|app| app.pty().is_some()),
        "the session starts"
    );
    assert!(
        app.term_mut().unwrap().sync.enabled(),
        "mode 2026 is offered"
    );
    let name = rustix::pty::ptsname(app.pty().unwrap().master(), Vec::new()).unwrap();
    let flags = rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY;
    let slave = rustix::fs::open(name.as_c_str(), flags, rustix::fs::Mode::empty()).unwrap();
    let mut slave = std::fs::File::from(slave);
    fn swaps(app: &mut App) -> u32 {
        let node = Path::new("/dev/mem0");
        let video = app.videos.get_mut(node).unwrap();
        video.as_any().downcast_mut::<Still>().unwrap().swaps
    }
    let start = b"\x1b[?2026h\x1b[1;1H\x1b[2K";
    slave.write_all(start).unwrap();
    let mut written = start.len() as u64;
    assert!(
        turn_until(&mut lp, &mut app, &|app| app.term_mut().unwrap().parsed
            >= written),
        "the first update begins"
    );
    assert!(turn_until(&mut lp, &mut app, &|app| app
        .heads
        .iter()
        .all(|h| h.frames_out() == 0)));
    let first = swaps(&mut app);
    for k in 0..60u32 {
        let end = format!("\x1b[1;{}HX\x1b[?2026l", k % 4 + 1).into_bytes();
        slave.write_all(&[end.as_slice(), start].concat()).unwrap();
        let frame = written + end.len() as u64;
        written = frame + start.len() as u64;
        // The turns until the display takes the finished frame: the
        // read is parsed up to the frame's end, and the rest waits.
        assert!(
            turn_until(&mut lp, &mut app, &|app| {
                let term = app.term_mut().unwrap();
                term.parsed >= frame && !term.unshown
            }),
            "frame {k} is taken"
        );
        let term = app.term_mut().unwrap();
        assert_eq!(
            term.parsed, frame,
            "frame {k}: the next update waits in the pty"
        );
        let row = app.screen_text().first().cloned().unwrap_or_default();
        assert_eq!(
            row.find('X'),
            Some(k as usize % 4),
            "frame {k} is the one taken"
        );
        assert_eq!(
            app.next_wake(at.get()),
            Some(Duration::ZERO),
            "frame {k}: the loop wakes at once for the rest of the read"
        );
        lp.dispatch(Some(Duration::ZERO), &mut app).unwrap();
        app.turn(at.get());
        let term = app.term_mut().unwrap();
        assert_eq!(term.parsed, written, "frame {k}: the next update begins");
        assert!(
            term.sync.holding(&term.terminal),
            "frame {k}: its hold is in force"
        );
        assert!(
            turn_until(&mut lp, &mut app, &|app| swaps(app) == first + k + 1),
            "frame {k} composes"
        );
    }
    assert!(turn_until(&mut lp, &mut app, &|app| app
        .heads
        .iter()
        .all(|h| h.frames_out() == 0)));
    assert_eq!(swaps(&mut app), first + 60, "one frame for each update");
}

// A key that a program echoes presents one frame on a display with two
// buffers: the frame that shows the echo. The key itself changes nothing
// on the screen, so vtium draws no frame for it. A frame drawn there
// would still have damage, because the buffer it goes into is two frames
// old and lacks the last echo, and the display would present it as a
// second frame with the same picture. The program here is the test,
// which reads each key from the pty in raw mode and writes it back, as
// tier 3's key-echo case does with dd.
//
// References:
// - ADR-1401 D2: a key's window, inside which the echo draws at once.
#[test]
fn an_echoed_key_presents_one_frame_on_a_double_buffered_display() {
    use std::io::{Read, Write};
    let args: Vec<OsString> = [
        "--font-size=16",
        "--no-blink",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "exec sleep 30",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
    let mut lp: Loop<App> = Loop::new().unwrap();
    let mut app = App::new(
        loaded.config,
        Box::new(FakeVt::new()),
        lp.handle(),
        Rc::new(Instant::now),
        vec![("PATH".into(), "/usr/bin:/bin".into())],
    );
    app.set_font_sources(no_fonts());
    let mut still = Still::new(&[(1, "A")]);
    still.buffers = 2;
    app.add_video(PathBuf::from("/dev/mem0"), Box::new(still));
    shown(&mut app);
    app.focus = Some(0);
    app.start();
    // Turns the loop as `main` does, waiting at most 50 ms, until
    // `done` holds, for at most 5 s.
    fn turn_until(lp: &mut Loop<App>, app: &mut App, done: impl Fn(&mut App) -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if done(app) {
                return true;
            }
            let wait = app
                .next_wake(Instant::now())
                .map_or(Duration::from_millis(50), |w| {
                    w.min(Duration::from_millis(50))
                });
            lp.dispatch(Some(wait), app).unwrap();
            app.turn(Instant::now());
        }
        done(app)
    }
    // The frames the display presented: its swaps that brought damage.
    fn presented(app: &mut App) -> usize {
        let video = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
        let still = video.as_any().downcast_mut::<Still>().unwrap();
        still.damage.iter().filter(|d| !d.is_empty()).count()
    }
    let idle = |app: &mut App| app.heads.iter().all(|h| h.frames_out() == 0);
    assert!(
        turn_until(&mut lp, &mut app, |app| app.pty().is_some()),
        "the session starts"
    );
    let name = rustix::pty::ptsname(app.pty().unwrap().master(), Vec::new()).unwrap();
    let flags = rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY;
    let slave = rustix::fs::open(name.as_c_str(), flags, rustix::fs::Mode::empty()).unwrap();
    // Raw and with no echo, so the test's writes are the only echo. A
    // read returns as soon as a byte is there, and returns nothing after
    // 5 s, so a key that never reaches the pty fails the test instead of
    // hanging it. Each read comes after the key's turn has written the
    // key, so the 5 s only bounds a failure.
    let mut termios = rustix::termios::tcgetattr(&slave).unwrap();
    termios.make_raw();
    termios.special_codes[rustix::termios::SpecialCodeIndex::VMIN] = 0;
    termios.special_codes[rustix::termios::SpecialCodeIndex::VTIME] = 50;
    rustix::termios::tcsetattr(&slave, rustix::termios::OptionalActions::Now, &termios).unwrap();
    let mut slave = std::fs::File::from(slave);
    assert!(
        turn_until(&mut lp, &mut app, |app| presented(app) > 0 && idle(app)),
        "the first frame goes out"
    );
    for c in ['a', 'b', 'c', 'd'] {
        let before = presented(&mut app);
        let key = InputEvent::Key(KeyEvent {
            keycode: 30,
            ascii: u32::from(c),
            unshifted: u32::from(c),
            shifted: u32::from(c),
            base: None,
            mods: InputMods::empty(),
            keysyms: vec![u32::from(c)],
            codepoints: vec![Some(c)],
            repeat: false,
        });
        app.input_event(key, Instant::now());
        app.turn(Instant::now());
        assert!(
            turn_until(&mut lp, &mut app, idle),
            "{c}: the key's turn ends"
        );
        let mut read = [0u8; 8];
        let n = slave.read(&mut read).unwrap();
        assert_ne!(n, 0, "{c}: the key reaches the pty within 5 s");
        let echo = read.get(..n).unwrap();
        assert_eq!(echo, [c as u8], "{c} reached the program");
        slave.write_all(echo).unwrap();
        let want = app.term_mut().unwrap().parsed + n as u64;
        assert!(
            turn_until(&mut lp, &mut app, |app| {
                app.term_mut().unwrap().parsed >= want && idle(app)
            }),
            "{c}: the echo is parsed and drawn"
        );
        // A few more turns, in which no other frame may go out.
        for _ in 0..4 {
            lp.dispatch(Some(Duration::ZERO), &mut app).unwrap();
            app.turn(Instant::now());
        }
        assert!(turn_until(&mut lp, &mut app, idle), "{c}: nothing is out");
        assert_eq!(
            presented(&mut app) - before,
            1,
            "{c}: one frame for the key and its echo"
        );
    }
    let row = app.screen_text().first().cloned().unwrap_or_default();
    assert!(row.starts_with("abcd"), "the echoes show: {row:?}");
}

// A key that changes what the displays show draws a frame at once,
// before the program answers: it clears the selection, returns the view
// from the scrollback, hides the pointer under
// `pointer-hide-while-typing`, or shows a blinking cursor again in its
// hidden phase. A key that changes none of these draws no frame of its
// own, and the program's echo draws the next one.
#[test]
fn a_key_draws_a_frame_only_when_it_changes_what_shows() {
    let key = |app: &mut App, c: char| {
        app.heads.first_mut().unwrap().draw_now = false;
        app.input_event(
            InputEvent::Key(KeyEvent {
                keycode: 46,
                ascii: u32::from(c),
                unshifted: u32::from(c),
                shifted: u32::from(c),
                base: None,
                mods: InputMods::empty(),
                keysyms: vec![u32::from(c)],
                codepoints: vec![Some(c)],
                repeat: false,
            }),
            Instant::now(),
        );
        let head = app.heads.first().unwrap();
        assert!(
            head.views.values().all(|v| v.after_key(&head.console.term)),
            "{c} went to the program"
        );
        head.draw_now
    };
    let (_lp, mut app) = seat(None, no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    app.focus = Some(0);
    app.hide_while_typing = true;
    app.turn(Instant::now());
    let lines = b"line\r\n".repeat(40);
    let term = app.term_mut().unwrap();
    assert_eq!(term.parse(&lines, &mut |_| {}), lines.len());
    app.heads.first_mut().unwrap().pointer.visible = false;
    key(&mut app, 'c');
    assert!(
        !key(&mut app, 'c'),
        "a key that changes nothing draws nothing"
    );
    // The selection.
    let screen = app.term_mut().unwrap().terminal.screen_mut();
    screen.select_start(0, 0);
    screen.select_to(3, 0);
    assert!(screen.has_selection());
    assert!(key(&mut app, 'c'), "the key cleared the selection");
    assert!(!app.term_mut().unwrap().terminal.screen().has_selection());
    // The scrollback.
    app.term_mut()
        .unwrap()
        .terminal
        .screen_mut()
        .scroll_view_up(3);
    assert!(key(&mut app, 'c'), "the key returned the view");
    assert_eq!(app.term_mut().unwrap().terminal.screen().scrolled_back(), 0);
    // The pointer.
    app.heads.first_mut().unwrap().pointer.visible = true;
    assert!(key(&mut app, 'c'), "the key hid the pointer");
    assert!(!app.heads.first().unwrap().pointer.visible);
    // The cursor's hidden phase.
    let blink = &mut app.term_mut().unwrap().blink;
    blink.fired(Timer::Cursor, true);
    assert!(!blink.cursor_shown(), "the cursor's hidden phase");
    assert!(key(&mut app, 'c'), "the key showed the cursor");
    assert!(!key(&mut app, 'c'), "and the next key changes nothing");
}

// A key that vtium drops still clears the selection, and the selection
// it cleared draws a frame. vtium drops a repeat whose press it never
// delivered, and a key with no keysym, which no program can be sent.
#[test]
fn a_key_vtium_drops_draws_the_selection_it_cleared() {
    let (_lp, mut app) = seat(None, no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    app.focus = Some(0);
    app.hide_while_typing = false;
    app.turn(Instant::now());
    let c = u32::from('c');
    let repeat = KeyEvent {
        keycode: 46,
        ascii: c,
        unshifted: c,
        shifted: c,
        base: None,
        mods: InputMods::empty(),
        keysyms: vec![c],
        codepoints: vec![Some('c')],
        repeat: true,
    };
    let no_keysym = KeyEvent {
        keysyms: Vec::new(),
        codepoints: Vec::new(),
        repeat: false,
        ..repeat.clone()
    };
    for (what, key) in [
        ("a dropped repeat", repeat),
        ("a key with no keysym", no_keysym),
    ] {
        let screen = app.term_mut().unwrap().terminal.screen_mut();
        screen.select_start(0, 0);
        screen.select_to(3, 0);
        assert!(screen.has_selection());
        app.heads.first_mut().unwrap().draw_now = false;
        app.input_event(InputEvent::Key(key), Instant::now());
        let head = app.heads.first().unwrap();
        assert!(
            !head.views.values().any(|v| v.after_key(&head.console.term)),
            "{what} does not reach the program"
        );
        assert!(
            !head.console.term.terminal.screen().has_selection(),
            "{what} clears the selection"
        );
        assert!(head.draw_now, "{what} draws the cleared selection");
    }
}

// A key pressed with Ctrl while the pointer is over a link shows the
// link's URI on the overlay row, and that draws a frame before the
// program answers the key.
//
// References:
// - ADR-1002, "What a link does": Ctrl shows the hovered link's URI.
#[test]
fn a_ctrl_key_over_a_link_draws_the_uri_it_shows() {
    let (_lp, mut app) = seat(None, no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    app.focus = Some(0);
    app.hide_while_typing = false;
    app.turn(Instant::now());
    let head = app.heads.first_mut().unwrap();
    head.console
        .term
        .terminal
        .input(b"\x1b]8;;x:there\x1b\\link\x1b]8;;\x1b\\");
    head.pointer.visible = true;
    head.pointer.x = head.cell.0 / 2;
    head.pointer.y = head.cell.1 / 2;
    head.hover(false);
    assert_eq!(head.link_overlay(), None, "no URI shows without Ctrl");
    head.draw_now = false;
    let e = u32::from('e');
    app.input_event(
        InputEvent::Key(KeyEvent {
            keycode: 18,
            ascii: e,
            unshifted: e,
            shifted: e,
            base: None,
            mods: InputMods::CTRL,
            keysyms: vec![e],
            codepoints: vec![Some('e')],
            repeat: false,
        }),
        Instant::now(),
    );
    let head = app.heads.first().unwrap();
    assert!(
        head.views.values().all(|v| v.after_key(&head.console.term)),
        "Ctrl+e went to the program"
    );
    assert_eq!(head.link_overlay().as_deref(), Some("x:there"));
    assert!(head.draw_now, "the key draws the URI it shows");
}

// ADR-1401 D2: a synchronized update's end stops the terminal only
// until the displays take the finished frame. No display draws a hidden
// console, nor the shown one while the displays are off, so each takes
// output again at the end of the turn.
#[test]
fn a_console_no_display_draws_takes_output_after_an_update_ends() {
    let heads = "version = 1\n[head one]\nconsoles = 2\n";
    let (_lp, mut app) = seat(Some(heads), no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
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
    // An update ends in `term`, which offers mode 2026.
    fn ended(term: &mut Term) {
        term.terminal.set_sync_enabled(true);
        term.sync = crate::sync::Sync::new(Duration::from_millis(200), term.terminal.sync_frames());
        let update = b"\x1b[?2026h\x1b[1;2Hx\x1b[?2026l";
        assert_eq!(term.parse(update, &mut |_| {}), update.len());
        assert!(!term.takes_input(), "the finished frame waits");
    }
    let hidden = |app: &mut App| {
        let head = app.heads.first_mut().unwrap();
        let shown = head.console.id;
        head.consoles_mut()
            .find(|c| c.id != shown)
            .map(|c| c.term.takes_input())
            .unwrap()
    };
    {
        let head = app.heads.first_mut().unwrap();
        let shown = head.console.id;
        let console = head.consoles_mut().find(|c| c.id != shown).unwrap();
        ended(&mut console.term);
    }
    app.turn(Instant::now());
    assert!(hidden(&mut app), "a hidden console");
    app.deactivate();
    ended(app.term_mut().unwrap());
    app.turn(Instant::now());
    assert!(
        app.term_mut().unwrap().takes_input(),
        "the shown console while the displays are off"
    );
}
