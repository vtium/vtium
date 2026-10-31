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
use crate::ioctl::{FB_NONSTD_REV_PIX_IN_B, FB_VISUAL_MONO01, FB_VISUAL_MONO10, FbBitfield};

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
fn a_device_without_true_color_is_refused() {
    let mut fb = FakeFb::new(32);
    fb.visual = 3;
    let mut v = Fbdev::with(fb);
    assert!(v.wake_up().is_err());
    assert!(v.canvas(DISPLAY).is_none());
    assert!(v.events().next().is_none());
}

/// A 1-bit device of `visual`, which takes no other depth: 4 pixels, in
/// the low half of each line's first byte.
fn mono(visual: u32, nonstd: u32) -> Fbdev<FakeFb> {
    let mut fb = FakeFb::new(1);
    fb.visual = visual;
    fb.depths = vec![1];
    fb.var.borrow_mut().nonstd = nonstd;
    awake(fb)
}

/// Writes `pixels` to line `y` of the canvas, swaps `damage` and returns
/// the first byte of each line of the framebuffer.
fn packed(v: &mut Fbdev<FakeFb>, y: usize, pixels: [u32; 4], damage: Rect) -> [u8; 3] {
    let canvas = v.canvas(DISPLAY).unwrap();
    assert_eq!(canvas.stride, 16, "an XRGB8888 copy");
    for (x, px) in pixels.iter().enumerate() {
        let at = y * 16 + x * 4;
        canvas.pixels[at..at + 4].copy_from_slice(&px.to_ne_bytes());
    }
    v.swap(DISPLAY, &[damage]).unwrap();
    // Each line holds 4 bits of a byte, then 8 bytes of padding.
    let m = map(v);
    [m[0], m[8], m[16]]
}

const W: u32 = 0x00ff_ffff;

#[test]
fn a_1_bit_framebuffer_is_packed_where_damaged() {
    let mut v = mono(FB_VISUAL_MONO10, 0);
    assert_eq!(v.levels(DISPLAY), Some(2));
    assert_eq!(v.size(DISPLAY), Some((4, 3)));
    // the first pixel in the low bit, a set bit white
    assert_eq!(packed(&mut v, 0, [W, 0, W, 0], WHOLE), [0x05, 0, 0]);
    // damage inside a byte packs the whole byte from the copy
    let one = Rect {
        x: 3,
        y: 1,
        width: 1,
        height: 1,
    };
    assert_eq!(packed(&mut v, 1, [W, W, 0, W], one), [0x05, 0x0b, 0]);
    // undamaged lines stay as they were
    let canvas = v.canvas(DISPLAY).unwrap();
    canvas.pixels[32..36].copy_from_slice(&W.to_ne_bytes());
    v.swap(DISPLAY, &[one]).unwrap();
    assert_eq!(map(&mut v)[16], 0);
}

// a cleared MONO01 framebuffer is black: its unlit bits are set
#[test]
fn a_1_bit_framebuffer_is_cleared_unlit() {
    let mut v = mono(FB_VISUAL_MONO01, 0);
    assert!(map(&mut v).iter().all(|&b| b == 0xff));
    let mut v = mono(FB_VISUAL_MONO10, 0);
    assert!(map(&mut v).iter().all(|&b| b == 0));
}

#[test]
fn mono01_sets_the_unlit_bits() {
    let mut v = mono(FB_VISUAL_MONO01, 0);
    assert_eq!(packed(&mut v, 0, [W, 0, W, 0], WHOLE), [0x0a, 0x0f, 0x0f]);
}

#[test]
fn reversed_pixels_put_the_first_in_the_high_bit() {
    let mut v = mono(FB_VISUAL_MONO10, FB_NONSTD_REV_PIX_IN_B);
    assert_eq!(packed(&mut v, 0, [W, 0, W, W], WHOLE), [0xb0, 0, 0]);
}

// drm_format_helper.c's threshold: BT.601's luma at least 128
#[test]
fn a_pixel_is_lit_by_its_luma_as_the_kernel_lights_it() {
    let mut v = mono(FB_VISUAL_MONO10, 0);
    let px = [0x0080_8080, 0x007f_7f7f, 0x00ff_0000, 0x0000_ff00];
    assert_eq!(packed(&mut v, 0, px, WHOLE), [0x09, 0, 0]);
}

#[test]
fn a_device_that_takes_true_color_is_not_1_bit() {
    let v = awake(FakeFb::new(32));
    assert_eq!(v.levels(DISPLAY), None);
    let mut fb = FakeFb::new(8);
    fb.visual = FB_VISUAL_MONO10;
    fb.depths = vec![8];
    let mut v = Fbdev::with(fb);
    assert!(v.wake_up().is_err(), "8 bits of a 1-bit visual");
    assert_eq!(v.levels(DISPLAY), None);
}

/// The bytes of `lines` lines `stride` long packing `px`, `width` pixels
/// a line lit where white, as the kernel orders them: the reference the
/// packing is held to.
fn reference(px: &[u32], width: usize, stride: usize, visual: u32, nonstd: u32) -> Vec<u8> {
    let msb = cfg!(target_endian = "big") != (nonstd & FB_NONSTD_REV_PIX_IN_B != 0);
    let mut out = vec![0u8; stride * (px.len() / width)];
    for (i, &p) in px.iter().enumerate() {
        let (x, y) = (i % width, i / width);
        let lit = p == W;
        if lit == (visual == FB_VISUAL_MONO10) {
            out[y * stride + x / 8] |= if msb { 0x80 >> (x % 8) } else { 1 << (x % 8) };
        }
    }
    out
}

// Each visual and order at odd widths, under random partial damage: the
// bytes the damage touches match the reference, and the rest keep their
// last value (the review's case).
#[test]
fn packing_at_odd_widths_under_partial_damage_matches_the_reference() {
    let cases = [
        (FB_VISUAL_MONO10, 0),
        (FB_VISUAL_MONO01, 0),
        (FB_VISUAL_MONO10, FB_NONSTD_REV_PIX_IN_B),
    ];
    for (visual, nonstd) in cases {
        for width in [1u32, 7, 9, 13, 17] {
            let mut fb = FakeFb::new(1);
            fb.visual = visual;
            fb.depths = vec![1];
            {
                let mut var = fb.var.borrow_mut();
                var.nonstd = nonstd;
                var.xres = width;
                var.xres_virtual = width;
            }
            let mut v = awake(fb);
            let (w, stride) = (width as usize, (width / 8 + 8) as usize);
            let mut seed = 12345u32 ^ width;
            let mut rnd = || {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                seed >> 16
            };
            let mut px = vec![0u32; w * 3];
            let mut before: Option<Vec<u8>> = None;
            for round in 0..40 {
                let canvas = v.canvas(DISPLAY).unwrap();
                for (i, p) in px.iter_mut().enumerate() {
                    *p = if rnd() & 1 == 1 { W } else { 0 };
                    canvas.pixels[i * 4..i * 4 + 4].copy_from_slice(&p.to_ne_bytes());
                }
                let damage = if round == 0 {
                    Rect {
                        x: 0,
                        y: 0,
                        width,
                        height: 3,
                    }
                } else {
                    let (x, y) = (rnd() % width, rnd() % 3);
                    Rect {
                        x,
                        y,
                        width: 1 + rnd() % (width - x),
                        height: 1 + rnd() % (3 - y),
                    }
                };
                v.swap(DISPLAY, &[damage]).unwrap();
                let got = map(&mut v)[..stride * 3].to_vec();
                let want = reference(&px, w, stride, visual, nonstd);
                let bytes = damage.x / 8..(damage.x + damage.width).div_ceil(8);
                for y in 0..3u32 {
                    for b in 0..width.div_ceil(8) {
                        let i = y as usize * stride + b as usize;
                        let rows = damage.y..damage.y + damage.height;
                        let at = format!(
                            "visual {visual} order {nonstd} width {width} round {round} line {y} byte {b}"
                        );
                        if rows.contains(&y) && bytes.contains(&b) {
                            assert_eq!(got[i], want[i], "{at}");
                        } else if let Some(p) = &before {
                            assert_eq!(got[i], p[i], "untouched: {at}");
                        }
                    }
                }
                before = Some(got);
            }
        }
    }
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

// vtium/vtium#174: the display takes a frame while it is awake and
// powered on and no stand-in flip event is pending. It lends its buffer
// during that flip, but the frame waits for the event.
#[test]
fn the_display_is_ready_for_a_frame_while_awake_on_and_not_flipping() {
    let mut v = awake(FakeFb::new(32));
    assert!(v.ready_for_frame(DISPLAY));
    assert!(
        !v.ready_for_frame(DisplayId(DISPLAY.0 + 1)),
        "no such display"
    );
    v.canvas(DISPLAY).unwrap();
    v.swap(DISPLAY, &[WHOLE]).unwrap();
    assert!(v.is_flipping(DISPLAY) && !v.ready_for_frame(DISPLAY));
    assert!(v.canvas(DISPLAY).is_some());
    v.vblank();
    assert!(v.ready_for_frame(DISPLAY));
    v.set_dpms(DISPLAY, Dpms::Off).unwrap();
    assert!(!v.ready_for_frame(DISPLAY) && v.canvas(DISPLAY).is_none());
    v.unblank().unwrap();
    assert!(v.ready_for_frame(DISPLAY));
    v.sleep();
    assert!(!v.ready_for_frame(DISPLAY) && v.canvas(DISPLAY).is_none());
}

/// A 320x240 device that reports `mm`.
fn sized(mm: (u32, u32)) -> Fbdev<FakeFb> {
    let fb = FakeFb::new(32);
    {
        let mut var = fb.var.borrow_mut();
        (var.xres, var.yres) = (320, 240);
        (var.width, var.height) = mm;
    }
    awake(fb)
}

#[test]
fn the_density_comes_from_the_devices_physical_size() {
    // A 3.5-inch panel.
    let d = sized((71, 53)).density(DISPLAY).unwrap();
    assert!(d.measured());
    let (x, y) = d.dpi();
    assert_eq!((x.round(), y.round()), (114.0, 115.0));
}

#[test]
fn a_device_with_no_physical_size_assumes_the_density() {
    // fbdev's unknown size is 0 or all ones.
    for mm in [(0, 0), (u32::MAX, u32::MAX), (71, 0)] {
        let v = sized(mm);
        assert_eq!(v.density(DISPLAY), Some(Density::ASSUMED), "{mm:?}");
    }
    assert_eq!(Fbdev::with(FakeFb::new(32)).density(DISPLAY), None);
}
