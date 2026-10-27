//! fbdev against a stand-in device: activation, the layouts it converts
//! into, the stand-in flip event, sleep and DPMS.

#![expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test arithmetic on bounded values; a test fails by panicking"
)]

use std::cell::{Cell, RefCell};

use super::*;
use crate::ioctl::FbBitfield;

/// A framebuffer in memory, 4x3. `put_var` takes the depths in `depths`
/// and refuses the rest.
struct FakeFb {
    var: RefCell<FbVarScreeninfo>,
    visual: u32,
    depths: Vec<u32>,
    blanks: RefCell<Vec<u32>>,
    /// `FBIOBLANK` calls to refuse, from the next.
    refuse_blanks: Cell<u32>,
}

fn field(offset: u32, length: u32) -> FbBitfield {
    FbBitfield {
        offset,
        length,
        msb_right: 0,
    }
}

impl FakeFb {
    fn new(bpp: u32) -> FakeFb {
        FakeFb {
            var: RefCell::new(FbVarScreeninfo {
                xres: 4,
                yres: 3,
                xres_virtual: 4,
                yres_virtual: 6,
                yoffset: 3,
                bits_per_pixel: bpp,
                // 5 columns by 6 lines at 60 Hz: 1e12 / 30 / 60 ps a pixel.
                pixclock: 555_555_555,
                left_margin: 1,
                upper_margin: 3,
                ..FbVarScreeninfo::default()
            }),
            visual: FB_VISUAL_TRUECOLOR,
            depths: vec![32, 24, 16],
            blanks: RefCell::new(Vec::new()),
            refuse_blanks: Cell::new(0),
        }
    }
}

impl Framebuffer for FakeFb {
    type Map = Vec<u8>;

    fn var(&self) -> io::Result<FbVarScreeninfo> {
        let mut var = *self.var.borrow();
        [var.red, var.green, var.blue] = match var.bits_per_pixel {
            16 => [field(11, 5), field(5, 6), field(0, 5)],
            _ => [field(16, 8), field(8, 8), field(0, 8)],
        };
        Ok(var)
    }

    fn put_var(&self, var: &mut FbVarScreeninfo) -> io::Result<()> {
        if !self.depths.contains(&var.bits_per_pixel) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        *self.var.borrow_mut() = *var;
        Ok(())
    }

    fn fix(&self) -> io::Result<FbFixScreeninfo> {
        let var = self.var.borrow();
        Ok(FbFixScreeninfo {
            visual: self.visual,
            // Each line padded 8 bytes past its pixels.
            line_length: var.xres * var.bits_per_pixel / 8 + 8,
            ..FbFixScreeninfo::default()
        })
    }

    fn blank(&self, level: u32) -> io::Result<()> {
        if let Some(left) = self.refuse_blanks.get().checked_sub(1) {
            self.refuse_blanks.set(left);
            return Err(io::ErrorKind::Other.into());
        }
        self.blanks.borrow_mut().push(level);
        Ok(())
    }

    fn map(&self, len: usize) -> io::Result<Vec<u8>> {
        // A mapping holds whatever the device memory held.
        Ok(vec![0xa5; len])
    }
}

/// The whole 4x3 screen.
const WHOLE: Rect = Rect {
    x: 0,
    y: 0,
    width: 4,
    height: 3,
};

fn awake(fb: FakeFb) -> Fbdev<FakeFb> {
    let mut v = Fbdev::with(fb);
    v.wake_up().unwrap();
    v
}

fn map(v: &mut Fbdev<FakeFb>) -> &mut Vec<u8> {
    &mut v.screen.as_mut().unwrap().map
}

// covers: platform.drm2d-fbdev
#[test]
fn the_display_keeps_its_mode_in_one_buffer() {
    let mut v = awake(FakeFb::new(32));
    assert_eq!(
        v.events().collect::<Vec<_>>(),
        [Event::Added(DISPLAY), Event::Awake]
    );
    let var = *v.dev().var.borrow();
    assert_eq!((var.yres_virtual, var.yoffset), (3, 0));
    assert_eq!(v.size(DISPLAY), Some((4, 3)));
    assert_eq!(v.refresh_mhz(DISPLAY), Some(60_000));
    assert_eq!(v.interval, Duration::from_millis(16));
    let canvas = v.canvas(DISPLAY).unwrap();
    assert_eq!((canvas.width, canvas.height, canvas.stride), (4, 3, 24));
    assert_eq!(canvas.age, 0);
    assert!(canvas.pixels.iter().all(|&b| b == 0), "cleared");
    canvas.pixels[0] = 0x12;
    v.swap(DISPLAY, &[WHOLE]).unwrap();
    assert_eq!(map(&mut v)[0], 0x12, "drawn straight into the framebuffer");
    assert!(v.is_flipping(DISPLAY));
    assert_eq!(v.canvas(DISPLAY).unwrap().age, 1);
    v.vblank();
    assert!(!v.is_flipping(DISPLAY));
    assert_eq!(v.events().collect::<Vec<_>>(), [Event::Flipped(DISPLAY)]);
}

#[test]
fn rgb565_is_converted_where_damaged() {
    let mut fb = FakeFb::new(16);
    fb.depths = vec![16];
    let mut v = awake(fb);
    let canvas = v.canvas(DISPLAY).unwrap();
    assert_eq!(canvas.stride, 16, "an XRGB8888 copy");
    canvas.pixels.fill(0xff);
    v.swap(DISPLAY, &[WHOLE]).unwrap();
    // Each line holds 4 pixels of 2 bytes, then 8 bytes of padding.
    assert_eq!(&map(&mut v)[0..8], &[0xff; 8]);
    assert_eq!(&map(&mut v)[8..16], &[0; 8]);
    let canvas = v.canvas(DISPLAY).unwrap();
    for (i, px) in [0x00ff_0000u32, 0x0000_ff00].iter().enumerate() {
        let at = 16 + 4 * (i + 1);
        canvas.pixels[at..at + 4].copy_from_slice(&px.to_ne_bytes());
    }
    let damage = Rect {
        x: 1,
        y: 1,
        width: 2,
        height: 1,
    };
    v.swap(DISPLAY, &[damage]).unwrap();
    let line = map(&mut v)[16..24].to_vec();
    assert_eq!(&line[0..2], &[0xff, 0xff]);
    assert_eq!(&line[2..4], &0xf800u16.to_le_bytes(), "red");
    assert_eq!(&line[4..6], &0x07e0u16.to_le_bytes(), "green");
    assert_eq!(&line[6..8], &[0xff, 0xff]);
}

#[test]
fn rgb888_is_packed_in_three_bytes() {
    let mut fb = FakeFb::new(24);
    fb.depths = vec![24];
    let mut v = awake(fb);
    let canvas = v.canvas(DISPLAY).unwrap();
    canvas.pixels[0..4].copy_from_slice(&0x0012_3456u32.to_ne_bytes());
    v.swap(DISPLAY, &[WHOLE]).unwrap();
    assert_eq!(&map(&mut v)[0..4], &[0x56, 0x34, 0x12, 0]);
}

#[test]
fn a_depth_the_device_refuses_is_tried_down_to_16_bits() {
    let mut fb = FakeFb::new(8);
    fb.depths = vec![8, 16];
    let v = awake(fb);
    assert_eq!(v.dev().var.borrow().bits_per_pixel, 16);
}

#[test]
fn a_device_without_true_colour_is_refused() {
    let mut fb = FakeFb::new(32);
    fb.visual = 3;
    let mut v = Fbdev::with(fb);
    assert!(v.wake_up().is_err());
    assert!(v.canvas(DISPLAY).is_none());
    assert!(v.events().next().is_none());
}

#[test]
fn sleep_clears_and_unmaps_and_waking_asks_for_a_redraw() {
    let mut v = awake(FakeFb::new(32));
    v.events().for_each(drop);
    v.canvas(DISPLAY).unwrap().pixels.fill(1);
    v.swap(DISPLAY, &[WHOLE]).unwrap();
    v.sleep();
    assert_eq!(v.events().collect::<Vec<_>>(), [Event::Asleep]);
    assert!(v.canvas(DISPLAY).is_none());
    assert!(matches!(v.swap(DISPLAY, &[]), Err(SwapError::Off)));
    v.vblank();
    assert!(v.events().next().is_none(), "no flip while asleep");
    v.wake_up().unwrap();
    assert_eq!(
        v.events().collect::<Vec<_>>(),
        [Event::Redraw(DISPLAY), Event::Awake]
    );
    assert_eq!(v.canvas(DISPLAY).unwrap().age, 0);
}

// A wake-up counts as user activity (the operator's decision on
// vtium/vtium#18): a blanked display comes on.
#[test]
fn dpms_blanks_through_the_device_and_a_wake_up_unblanks() {
    let mut v = awake(FakeFb::new(32));
    v.set_dpms(DISPLAY, Dpms::Off).unwrap();
    v.set_dpms(DISPLAY, Dpms::Standby).unwrap();
    assert_eq!(
        *v.dev().blanks.borrow(),
        [FB_BLANK_POWERDOWN, FB_BLANK_NORMAL]
    );
    v.sleep();
    assert!(v.set_dpms(DISPLAY, Dpms::On).is_err());
    v.events().for_each(drop);
    v.wake_up().unwrap();
    assert_eq!(
        *v.dev().blanks.borrow(),
        [FB_BLANK_POWERDOWN, FB_BLANK_NORMAL, FB_BLANK_UNBLANK]
    );
    assert_eq!(v.dpms(DISPLAY), Some(Dpms::On));
    assert_eq!(
        v.events().collect::<Vec<_>>(),
        [Event::Redraw(DISPLAY), Event::Awake]
    );
    assert_eq!(v.canvas(DISPLAY).unwrap().age, 0);
}

#[test]
fn the_refresh_rate_is_held_in_range() {
    let var = FbVarScreeninfo::default();
    assert_eq!(refresh_mhz(&var), 60_000);
    let fast = FbVarScreeninfo {
        xres: 1,
        yres: 1,
        pixclock: 1,
        ..var
    };
    assert_eq!(refresh_mhz(&fast), 200_000);
    assert_eq!(refresh_interval(200_000), Duration::from_millis(5));
    assert_eq!(refresh_interval(1), Duration::from_millis(999));
    assert_eq!(refresh_interval(2_000_000), Duration::from_millis(15));
}

/// Through the real event loop: the stand-in flip event is a timer at the
/// refresh interval.
#[test]
fn the_loop_fires_the_stand_in_flip() {
    struct App(Fbdev<FakeFb>);
    let mut lp: crate::event_loop::Loop<App> = crate::event_loop::Loop::new().unwrap();
    let mut app = App(Fbdev::with(FakeFb::new(32)));
    app.0.insert(&lp.handle(), |app: &mut App| Some(&mut app.0));
    app.0.wake_up().unwrap();
    app.0.events().for_each(drop);
    app.0.canvas(DISPLAY).unwrap();
    app.0.swap(DISPLAY, &[WHOLE]).unwrap();
    for _ in 0..50 {
        lp.dispatch(Some(Duration::from_millis(100)), &mut app)
            .unwrap();
        if !app.0.is_flipping(DISPLAY) {
            break;
        }
    }
    assert_eq!(
        app.0.events().collect::<Vec<_>>(),
        [Event::Flipped(DISPLAY)]
    );
}

// Re-review finding 1: an unblank that fails is tried again, since the
// blank took the input that ended it and no later input comes to
// `unblank`; so is a wake-up's.
#[test]
fn a_failed_unblank_is_tried_again() {
    struct App(Fbdev<FakeFb>);
    let mut lp: crate::event_loop::Loop<App> = crate::event_loop::Loop::new().unwrap();
    let mut app = App(Fbdev::with(FakeFb::new(32)));
    app.0.insert(&lp.handle(), |app: &mut App| Some(&mut app.0));
    app.0.wake_up().unwrap();
    for wake in [false, true] {
        app.0.set_dpms(DISPLAY, Dpms::Off).unwrap();
        app.0.events().for_each(drop);
        app.0.dev().refuse_blanks.set(2);
        if wake {
            app.0.sleep();
            app.0.wake_up().unwrap();
        } else {
            assert!(app.0.unblank().is_err());
        }
        assert_eq!(app.0.dpms(DISPLAY), Some(Dpms::Off));
        for _ in 0..50 {
            if app.0.dpms(DISPLAY) == Some(Dpms::On) {
                break;
            }
            lp.dispatch(Some(Duration::from_millis(100)), &mut app)
                .unwrap();
        }
        assert_eq!(app.0.dpms(DISPLAY), Some(Dpms::On), "woken: {wake}");
        assert_eq!(app.0.dev().blanks.borrow().last(), Some(&FB_BLANK_UNBLANK));
        assert!(app.0.events().any(|e| e == Event::Redraw(DISPLAY)));
        assert_eq!(app.0.canvas(DISPLAY).unwrap().age, 0);
    }
}

// Round 4, finding 2: once an unblank's retries run out, the device
// reports `WakeFailed`, and the next input tries the unblank again.
#[test]
fn input_after_the_unblank_retries_ran_out_unblanks() {
    let mut v = awake(FakeFb::new(32));
    v.set_dpms(DISPLAY, Dpms::Off).unwrap();
    v.events().for_each(drop);
    v.dev().refuse_blanks.set(u32::MAX);
    assert!(v.unblank().is_err());
    v.input().unwrap();
    v.retry_unblank();
    assert_eq!(v.retries, Some(1), "input leaves the retries be");
    while v.retries.is_some() {
        v.retry_unblank();
    }
    assert_eq!(v.events().collect::<Vec<_>>(), [Event::WakeFailed]);
    v.dev().refuse_blanks.set(0);
    v.input().unwrap();
    assert_eq!(v.dpms(DISPLAY), Some(Dpms::On));
    assert_eq!(v.events().collect::<Vec<_>>(), [Event::Redraw(DISPLAY)]);
    let blanks = v.dev().blanks.borrow().len();
    v.set_dpms(DISPLAY, Dpms::Off).unwrap();
    v.input().unwrap();
    assert_eq!(v.dpms(DISPLAY), Some(Dpms::Off), "nothing failed");
    assert_eq!(v.dev().blanks.borrow().len(), blanks + 1);
}

// The frame contract, rules 3 and 4: no damage, no swap; no cursor plane.
#[test]
fn a_swap_with_no_damage_does_nothing() {
    let mut fb = FakeFb::new(16);
    fb.depths = vec![16];
    let mut v = awake(fb);
    v.events().for_each(drop);
    v.canvas(DISPLAY).unwrap().pixels.fill(0xff);
    assert_eq!(v.swap(DISPLAY, &[]).unwrap(), Swapped::Nothing);
    assert!(!v.is_flipping(DISPLAY));
    assert!(map(&mut v).iter().all(|&b| b == 0), "nothing converted");
    assert!(!v.hardware_cursor(DISPLAY));
}

// Re-review finding 6 and the frame contract, rule 3: damage passed
// without a canvas lent is ignored.
#[test]
fn damage_without_a_canvas_is_ignored() {
    let mut fb = FakeFb::new(16);
    fb.depths = vec![16];
    let mut v = awake(fb);
    v.events().for_each(drop);
    assert_eq!(v.swap(DISPLAY, &[WHOLE]).unwrap(), Swapped::Nothing);
    assert!(!v.is_flipping(DISPLAY));
    assert!(map(&mut v).iter().all(|&b| b == 0), "nothing converted");
    v.canvas(DISPLAY).unwrap().pixels.fill(0xff);
    assert_eq!(v.swap(DISPLAY, &[WHOLE]).unwrap(), Swapped::Frame);
    assert_eq!(v.swap(DISPLAY, &[WHOLE]).unwrap(), Swapped::Nothing);
}

// The frame contract, rule 6: no canvas while blanked; unblanked, the
// buffer is age 0 and the owner is told to redraw.
#[test]
fn a_blanked_display_lends_no_canvas() {
    let mut v = awake(FakeFb::new(32));
    v.canvas(DISPLAY).unwrap();
    v.swap(DISPLAY, &[WHOLE]).unwrap();
    v.events().for_each(drop);
    v.set_dpms(DISPLAY, Dpms::Off).unwrap();
    assert!(v.canvas(DISPLAY).is_none());
    assert!(matches!(v.swap(DISPLAY, &[WHOLE]), Err(SwapError::Off)));
    v.unblank().unwrap();
    assert_eq!(v.dev().blanks.borrow().last(), Some(&FB_BLANK_UNBLANK));
    assert_eq!(v.events().collect::<Vec<_>>(), [Event::Redraw(DISPLAY)]);
    assert_eq!(v.canvas(DISPLAY).unwrap().age, 0);
    v.unblank().unwrap();
    assert!(v.events().next().is_none(), "on already");
}
