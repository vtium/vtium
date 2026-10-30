//! drm2d against the stand-in card (aaronsb/kmscon#119): modesets, the
//! damage clips and cursor commits a frame sequence sends, failed swaps,
//! the waits on a pending flip, a monitor's power cycle and the wake-up
//! retries.

#![expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test arithmetic on bounded values; a test fails by panicking"
)]

use drm_ffi::drm_mode_modeinfo;
use log::Level;
use rustix::io::Errno;

use super::output::signed;
use super::*;
use crate::display::blank::Due;
use crate::display::fake::{Commit, FakeCard, Plane, RemoveFb, logs};
use crate::display::mode::mode;
use crate::display::{Blank, DPMS_TIMEOUT, Wanted};

const A: u32 = 41;
const B: u32 = 42;

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// A card with one display at 640x480, preferred, and 320x240.
fn one() -> Drm2d<FakeCard> {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true), mode(320, 240, 60, false)])]);
    Drm2d::with(card, ModePolicy::default()).unwrap()
}

fn events(v: &mut Drm2d<FakeCard>) -> Vec<Event> {
    v.events().collect()
}

/// Completes every flip and reads the events.
fn flip(v: &mut Drm2d<FakeCard>) {
    v.card().complete_flips();
    v.dispatch().unwrap();
}

/// Passes the next vblank and reads its events.
fn vblank(v: &mut Drm2d<FakeCard>) {
    v.card().vblank();
    v.dispatch().unwrap();
}

/// Wakes `v` and completes the modeset's flip.
fn awake(mut v: Drm2d<FakeCard>) -> Drm2d<FakeCard> {
    v.wake_up().unwrap();
    flip(&mut v);
    v.events().for_each(drop);
    v
}

fn primary(v: &Drm2d<FakeCard>, id: u32) -> u32 {
    v.outputs
        .iter()
        .find(|o| o.id.0 == id)
        .unwrap()
        .planes
        .as_ref()
        .unwrap()
        .primary
}

fn cursor_plane(v: &Drm2d<FakeCard>, id: u32) -> u32 {
    let o = v.outputs.iter().find(|o| o.id.0 == id).unwrap();
    o.planes.as_ref().unwrap().cursor.as_ref().unwrap().0
}

/// The rectangles of a commit's damage blob.
fn clips(c: &Commit, plane: u32) -> Option<Vec<Rect>> {
    let blob = c.blob(plane, "FB_DAMAGE_CLIPS")?;
    let int = |b: &[u8]| u32::try_from(i32::from_ne_bytes(b.try_into().unwrap())).unwrap();
    Some(
        blob.chunks_exact(16)
            .map(|r| {
                let (x1, y1) = (int(&r[0..4]), int(&r[4..8]));
                let (x2, y2) = (int(&r[8..12]), int(&r[12..16]));
                rect(x1, y1, x2 - x1, y2 - y1)
            })
            .collect(),
    )
}

/// A frame's damage, small enough to send as a clip.
const DOT: Rect = Rect {
    x: 0,
    y: 0,
    width: 8,
    height: 8,
};

/// Draws a frame and swaps it.
fn frame(v: &mut Drm2d<FakeCard>, damage: &[Rect]) -> Result<Swapped, SwapError> {
    let canvas = v.canvas(DisplayId(A)).expect("a canvas");
    canvas.pixels[0] = 1;
    v.swap(DisplayId(A), damage)
}

// covers: platform.drm2d-fbdev
#[test]
fn a_display_opens_in_its_preferred_mode() {
    let mut v = one();
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A)), Event::Awake]);
    let commits = v.card().state().commits.clone();
    assert_eq!(commits.len(), 2);
    assert!(commits[0].test_only());
    let c = &commits[1];
    assert_eq!(
        c.flags,
        DRM_MODE_ATOMIC_ALLOW_MODESET | DRM_MODE_ATOMIC_NONBLOCK | DRM_MODE_PAGE_FLIP_EVENT
    );
    let blob = c.blob(100, "MODE_ID").unwrap();
    assert_eq!(*blob, mode_bytes(&mode(640, 480, 60, true).0));
    assert_eq!(blob.len(), size_of::<drm_mode_modeinfo>());
    // The card reports the mode vtium set, as a later scan reads it.
    assert_eq!(
        v.card().crtc_mode(100).unwrap(),
        Some(mode(640, 480, 60, true))
    );
    let plane = primary(&v, A);
    assert_eq!(c.get(plane, "SRC_W"), Some(640 << 16));
    assert_eq!(c.get(plane, "CRTC_H"), Some(480));
    assert_eq!(c.get(plane, "FB_DAMAGE_CLIPS"), None);
    assert_eq!(c.get(A, "CRTC_ID"), Some(100));
    // The cursor and overlay planes are cleared.
    assert_eq!(c.get(399, "FB_ID"), Some(0));
    assert_eq!(c.get(cursor_plane(&v, A), "CRTC_ID"), Some(0));
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    assert_eq!(v.name(DisplayId(A)), Some("HDMI-A-1"));
    assert!(v.supports_damage(DisplayId(A)));
}

#[test]
fn the_first_frame_is_whole_and_drawn_into_cleared_buffers() {
    let mut v = one();
    v.wake_up().unwrap();
    assert!(
        v.canvas(DisplayId(A)).is_none(),
        "the modeset's flip is pending"
    );
    flip(&mut v);
    assert_eq!(
        events(&mut v),
        [
            Event::Added(DisplayId(A)),
            Event::Awake,
            Event::Flipped(DisplayId(A))
        ]
    );
    let canvas = v.canvas(DisplayId(A)).unwrap();
    assert_eq!(
        (canvas.width, canvas.height, canvas.stride),
        (640, 480, 2560)
    );
    assert_eq!(canvas.age, 0);
    assert!(canvas.pixels.iter().all(|&b| b == 0));
    v.swap(DisplayId(A), &[rect(0, 0, 8, 8)]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.flags, DRM_MODE_ATOMIC_NONBLOCK | DRM_MODE_PAGE_FLIP_EVENT);
    assert_eq!(clips(&c, primary(&v, A)), None);
}

// The clips a frame sequence sends (aaronsb/kmscon#119).
#[test]
fn each_frame_sends_its_damage_as_clips() {
    let mut v = awake(one());
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    let plane = primary(&v, A);
    for damage in [
        vec![rect(0, 0, 8, 16)],
        vec![rect(8, 16, 8, 16), rect(600, 400, 100, 100)],
        vec![rect(1, 2, 3, 4)],
    ] {
        frame(&mut v, &damage).unwrap();
        assert!(v.is_flipping(DisplayId(A)));
        let c = v.card().last_commit().unwrap();
        let mut want = damage.clone();
        want.iter_mut().for_each(|r| *r = r.clip(640, 480).unwrap());
        assert_eq!(clips(&c, plane), Some(want));
        flip(&mut v);
        assert_eq!(events(&mut v).last(), Some(&Event::Flipped(DisplayId(A))));
    }
    // A frame with no damage sends nothing.
    assert!(matches!(frame(&mut v, &[]), Ok(Swapped::Nothing)));
    // Every damage blob was destroyed after its commit; the mode blob stays.
    assert_eq!(v.card().state().blobs.len(), 1);
}

#[test]
fn buffer_ages_follow_the_flips() {
    let mut v = awake(one());
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 0);
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 2);
}

// A device with universal planes and no atomic commits presents whole
// frames through the legacy calls (aaronsb/kmscon#84).
#[test]
fn damage_clips_go_only_with_atomic_commits() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().atomic = false;
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert!(!v.supports_damage(DisplayId(A)));
    let set = v.card().state().set_crtcs.clone();
    assert_eq!(set.len(), 1);
    assert_eq!((set[0].0, set[0].2), (100, A));
    frame(&mut v, &[rect(0, 0, 8, 8)]).unwrap();
    assert_eq!(v.card().state().page_flips.len(), 1);
    assert!(v.card().state().commits.is_empty());
    assert!(v.card().state().blobs.is_empty());
    assert!(v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).is_err());
}

// A failed swap is not a queued frame: its error reaches the caller and
// the next swap carries its damage (aaronsb/kmscon#93).
#[test]
fn a_failed_swap_keeps_its_damage() {
    let mut v = awake(one());
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    v.events().for_each(drop);
    let plane = primary(&v, A);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    let failed = frame(&mut v, &[rect(0, 0, 8, 16)]);
    assert!(matches!(failed, Err(SwapError::Failed(_))));
    assert!(!v.is_flipping(DisplayId(A)));
    flip(&mut v);
    assert!(events(&mut v).is_empty(), "no flip for a failed swap");
    // The back buffer holds the frame that failed.
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 1);
    frame(&mut v, &[rect(100, 100, 8, 16)]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        clips(&c, plane),
        Some(vec![rect(0, 0, 8, 16), rect(100, 100, 8, 16)])
    );
    flip(&mut v);
    frame(&mut v, &[rect(5, 5, 1, 1)]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(clips(&c, plane), Some(vec![rect(5, 5, 1, 1)]));
}

#[test]
fn ebusy_is_busy() {
    let mut v = awake(one());
    v.card().state().commit_errors.push_back(Errno::BUSY);
    logs::capture();
    assert!(matches!(frame(&mut v, &[DOT]), Err(SwapError::Busy)));
    assert!(logs::at_least(Level::Warn).is_empty());
    vblank(&mut v);
    frame(&mut v, &[DOT]).unwrap();
    assert!(v.canvas(DisplayId(A)).is_none(), "the flip is pending");
    assert!(matches!(v.swap(DisplayId(A), &[]), Err(SwapError::Busy)));
}

#[test]
fn a_swap_while_powered_down_is_off() {
    let mut v = awake(one());
    v.set_dpms(DisplayId(A), Dpms::Off).unwrap();
    assert_eq!(v.card().state().dpms[&A], 3);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::Off));
    assert!(matches!(v.swap(DisplayId(A), &[]), Err(SwapError::Off)));
    v.set_dpms(DisplayId(A), Dpms::On).unwrap();
    frame(&mut v, &[DOT]).unwrap();
}

/// The blank as the owner wires it: it powers every display down when
/// due, and a wake-up or a hotplug counts as input, restarting its timer.
#[expect(
    clippy::disallowed_methods,
    reason = "a test's instants start from the clock once"
)]
fn blanked(v: &mut Drm2d<FakeCard>) -> (Blank, std::time::Instant) {
    let start = std::time::Instant::now();
    let mut blank = Blank::new(Some(DPMS_TIMEOUT), start);
    let at = start + DPMS_TIMEOUT;
    assert_eq!(blank.due(at), Due::Now);
    let ids: Vec<DisplayId> = v.displays().collect();
    for id in ids {
        v.set_dpms(id, Dpms::Off).unwrap();
    }
    (blank, at)
}

/// The owner's answer to input or a wake-up: input to the blank, which
/// unblanks the displays where they were blanked, and restarts its timer.
fn activity(v: &mut Drm2d<FakeCard>, blank: &mut Blank, now: std::time::Instant) {
    if blank.input(now) {
        v.unblank().unwrap();
    }
    assert_eq!(blank.due(now + DPMS_TIMEOUT / 2), Due::In(DPMS_TIMEOUT / 2));
}

// Finding 3, the operator's decision: a wake-up, the VT acquired, counts as
// user activity.
// Its modeset's ACTIVE=1 brings every display on, DPMS included, though
// the display was blanked and the monitor's power cycle turned its
// connector on; the recorded DPMS is On, and the blank restarts. Only tier
// 3 proves the kernel ties DPMS to ACTIVE so.
#[test]
fn a_wake_up_while_blanked_brings_every_display_on() {
    let mut v = awake(one());
    let (mut blank, at) = blanked(&mut v);
    assert_eq!(v.card().state().dpms[&A], 3);
    v.card().state().dpms.insert(A, 0);
    v.sleep();
    flip(&mut v);
    v.events().for_each(drop);
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A)), Event::Awake]);
    assert_eq!(v.card().last_commit().unwrap().get(100, "ACTIVE"), Some(1));
    assert_eq!(v.card().state().dpms[&A], 0);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
    let commits = v.card().state().commits.len();
    activity(&mut v, &mut blank, at);
    assert_eq!(v.card().state().commits.len(), commits, "already on");
    flip(&mut v);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 0);
}

// The cursor commits (aaronsb/kmscon#119).
#[test]
fn the_cursor_goes_with_the_next_commit() {
    let mut v = awake(one());
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    let cp = cursor_plane(&v, A);
    let image: Vec<u32> = (0..4).collect();
    v.setup_cursor(DisplayId(A), &image, 2, 2, (1, 2)).unwrap();
    assert!(!v.need_commit(DisplayId(A)));
    v.set_cursor_offset(DisplayId(A), 5, 5);
    v.show_cursor(DisplayId(A), 10, 20).unwrap();
    assert!(v.need_commit(DisplayId(A)));
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Cursor);
    assert!(!v.need_commit(DisplayId(A)));
    let c = v.card().last_commit().unwrap();
    let fb = v.outputs[0].cursor.as_ref().unwrap().fb;
    assert_eq!(c.get(cp, "FB_ID"), Some(u64::from(fb)));
    assert_eq!(c.get(cp, "CRTC_ID"), Some(100));
    assert_eq!(c.get(cp, "CRTC_X"), Some(5 + 10 - 1));
    assert_eq!(c.get(cp, "CRTC_Y"), Some(5 + 20 - 2));
    assert_eq!(c.get(cp, "CRTC_W"), Some(64));
    assert_eq!(c.get(cp, "SRC_H"), Some(64 << 16));
    assert_eq!(c.get(cp, "HOTSPOT_X"), Some(1));
    // The same place again sends nothing new.
    flip(&mut v);
    v.show_cursor(DisplayId(A), 10, 20).unwrap();
    assert!(!v.need_commit(DisplayId(A)));
    v.show_cursor(DisplayId(A), -3, 0).unwrap();
    v.swap(DisplayId(A), &[]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(cp, "CRTC_X"), Some(signed(5 - 3 - 1)));
    flip(&mut v);
    v.hide_cursor(DisplayId(A));
    assert!(v.need_commit(DisplayId(A)));
    v.swap(DisplayId(A), &[]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(cp, "FB_ID"), Some(0));
    assert_eq!(c.get(cp, "CRTC_ID"), Some(0));
}

// The pointer's image on the cursor plane (vtium/vtium#92): the plane's
// framebuffer holds the image row by row at the buffer's pitch, clear
// around it on the plane's 64 by 64, and the commit that shows it names
// that framebuffer, with the hotspot on the pointer.
// covers: platform.drm2d-fbdev
#[test]
fn the_cursor_plane_shows_the_image_set_up() {
    let mut v = awake(one());
    let cp = cursor_plane(&v, A);
    let image: Vec<u32> = (0..60 * 2).map(|i| 0xff00_0000 | i).collect();
    v.setup_cursor(DisplayId(A), &image, 60, 2, (3, 1)).unwrap();
    v.show_cursor(DisplayId(A), 40, 30).unwrap();
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Cursor);
    let c = v.card().last_commit().unwrap();
    let cursor = v.outputs[0].cursor.as_ref().unwrap();
    assert_eq!(c.get(cp, "FB_ID"), Some(u64::from(cursor.fb)));
    assert_eq!(c.get(cp, "CRTC_X"), Some(40 - 3));
    assert_eq!(c.get(cp, "CRTC_Y"), Some(30 - 1));
    assert_eq!(c.get(cp, "HOTSPOT_X"), Some(3));
    assert_eq!(c.get(cp, "HOTSPOT_Y"), Some(1));
    let bytes = v.card().memory(cursor.dumb.handle).unwrap();
    assert_eq!(bytes.len(), 64 * 64 * 4);
    for (px, i) in bytes.chunks_exact(4).zip(0u32..) {
        let (x, y) = (i % 64, i / 64);
        let want = if y < 2 && x < 60 {
            0xff00_0000 | (y * 60 + x)
        } else {
            0
        };
        let got = u32::from_ne_bytes(px.try_into().unwrap());
        assert_eq!(got, want, "{x},{y}");
    }
}

// An image larger than the card's cursor plane is refused, so the
// pointer is drawn in software whole rather than cut.
#[test]
fn an_image_larger_than_the_plane_is_refused() {
    let mut v = awake(one());
    v.card().state().cursor_side = 32;
    let e = v
        .setup_cursor(DisplayId(A), &[0; 33 * 2], 33, 2, (0, 0))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    assert!(!v.hardware_cursor(DisplayId(A)));
    let e = v
        .setup_cursor(DisplayId(A), &[0; 2 * 33], 2, 33, (0, 0))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    v.setup_cursor(DisplayId(A), &[0; 32 * 32], 32, 32, (0, 0))
        .unwrap();
}

// A display not online refuses the image with a signal of its own, which
// no error from the card decodes to: the owner offers it again later.
#[test]
fn a_display_not_online_says_so() {
    let mut v = one();
    let e = v
        .setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotConnected);
    assert_eq!(e.raw_os_error(), None);
    let mut v = awake(one());
    v.card().state().dumb_errors.push_back(Errno::INVAL);
    let e = v
        .setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0))
        .unwrap_err();
    assert_ne!(e.kind(), io::ErrorKind::NotConnected);
}

// A commit the cursor plane makes fail (vtium/vtium#92): the same commit
// without it is tried, and when that takes, the plane is given up, so the
// owner draws the pointer in software and the display does not freeze.
#[test]
fn a_commit_the_cursor_plane_fails_is_sent_without_it() {
    let mut v = awake(one());
    let cp = cursor_plane(&v, A);
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    v.card().state().commit_errors.push_back(Errno::INVAL);
    assert_eq!(frame(&mut v, &[DOT]).unwrap(), Swapped::Frame);
    assert!(!v.hardware_cursor(DisplayId(A)));
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(cp, "FB_ID"), Some(0));
    assert_eq!(c.get(cp, "CRTC_ID"), Some(0));
    flip(&mut v);
    assert!(!v.need_commit(DisplayId(A)));
}

// A cursor-only commit that fails while the plane is off has nothing to
// send without it: the plane is given up and nothing is sent.
#[test]
fn a_cursor_only_commit_that_fails_gives_the_plane_up() {
    let mut v = awake(one());
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    v.card().state().commit_errors.push_back(Errno::INVAL);
    let commits = v.card().state().commits.len();
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Nothing);
    assert!(!v.hardware_cursor(DisplayId(A)));
    assert!(!v.need_commit(DisplayId(A)));
    assert!(v.card().state().commits.len() <= commits + 1);
}

// When the commit fails without the cursor too, the cursor was not the
// cause: it is kept, and the error reaches the caller.
#[test]
fn a_commit_that_fails_either_way_keeps_the_cursor() {
    let mut v = awake(one());
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    v.card()
        .state()
        .commit_errors
        .extend([Errno::INVAL, Errno::INVAL]);
    assert!(frame(&mut v, &[DOT]).is_err());
    assert!(v.hardware_cursor(DisplayId(A)));
    assert!(v.need_commit(DisplayId(A)));
    v.swap(DisplayId(A), &[]).unwrap();
    assert!(v.hardware_cursor(DisplayId(A)));
}

// A commit refused as busy is no fault of the cursor's: it waits for the
// swap after the next vblank.
#[test]
fn a_busy_commit_keeps_the_cursor_waiting() {
    let mut v = awake(one());
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    v.card().state().commit_errors.push_back(Errno::BUSY);
    assert!(v.swap(DisplayId(A), &[]).is_err());
    assert!(v.need_commit(DisplayId(A)));
    vblank(&mut v);
    v.swap(DisplayId(A), &[]).unwrap();
    assert!(!v.need_commit(DisplayId(A)));
}

// Round 4, finding 3: removing the framebuffer the cursor plane shows
// takes the plane off its CRTC in the kernel, so the plane is recorded
// hidden. A cursor-only commit hiding it again would name no CRTC, which
// the kernel refuses with a flip event.
#[test]
fn a_new_cursor_image_leaves_the_plane_recorded_hidden() {
    let mut v = awake(one());
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Cursor);
    flip(&mut v);
    let cp = cursor_plane(&v, A);
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    assert!(!v.card().state().plane_crtc.contains_key(&cp));
    assert!(!v.need_commit(DisplayId(A)));
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Nothing);
    v.show_cursor(DisplayId(A), 2, 2).unwrap();
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Cursor);
    let fb = v.outputs[0].cursor.as_ref().unwrap().fb;
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(cp, "FB_ID"), Some(u64::from(fb)));
    assert_eq!(c.get(cp, "CRTC_ID"), Some(100));
}

// Re-review finding 4: a cursor change undone before the swap leaves the
// plane as the last commit set it, so the swap sends nothing. With the
// plane off, a commit would name no CRTC, which the kernel refuses with a
// flip event.
#[test]
fn a_cursor_change_undone_before_the_swap_sends_nothing() {
    let mut v = awake(one());
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    let commits = v.card().state().commits.len();
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    v.hide_cursor(DisplayId(A));
    assert!(!v.need_commit(DisplayId(A)));
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Nothing);
    assert_eq!(v.card().state().commits.len(), commits);
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Cursor);
    flip(&mut v);
    let commits = v.card().state().commits.len();
    v.show_cursor(DisplayId(A), 2, 2).unwrap();
    v.set_cursor_offset(DisplayId(A), 1, 1);
    v.set_cursor_offset(DisplayId(A), 0, 0);
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Nothing);
    assert_eq!(v.card().state().commits.len(), commits);
}

// The plane is at most 64 a side, whatever larger size the card reports.
#[test]
fn the_cursor_plane_is_at_most_64_a_side() {
    let mut v = awake(one());
    let image = vec![0xff00_ff00u32; 64 * 64];
    v.setup_cursor(DisplayId(A), &image, 64, 64, (0, 0))
        .unwrap();
    let c = v.outputs[0].cursor.as_ref().unwrap();
    assert_eq!((c.width, c.height), (64, 64));
    let image = vec![0xff00_ff00u32; 65];
    let e = v
        .setup_cursor(DisplayId(A), &image, 65, 1, (0, 0))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
}

// The operator's decision on vtium/vtium#18: a modeset waits on the
// pending flip's completion event, without blocking the loop.
#[test]
fn a_modeset_waits_for_the_pending_flip() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c == A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    frame(&mut v, &[DOT]).unwrap();
    let before = v.card().state().commits.len();
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert_eq!(
        v.card().state().commits.len(),
        before,
        "nothing commits over a pending flip"
    );
    assert_eq!(v.waits(), (true, false));
    assert!(matches!(v.swap(DisplayId(A), &[]), Err(SwapError::Busy)));
    flip(&mut v);
    assert_eq!(v.waits(), (false, false));
    let c = v.card().last_commit().unwrap();
    assert_ne!(c.flags & DRM_MODE_ATOMIC_ALLOW_MODESET, 0);
    assert!(c.mentions(A) && c.mentions(B));
    // A keeps what it shows, so it needs no redraw.
    assert_eq!(events(&mut v), [Event::Added(DisplayId(B))]);
}

#[test]
fn a_modeset_goes_ahead_when_the_flip_never_completes() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c == A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    frame(&mut v, &[DOT]).unwrap();
    v.card().state().listed.push(B);
    v.poll().unwrap();
    // The flip's event is lost: the card forgets it.
    v.card().state().pending.clear();
    v.flip_expired(100);
    assert_eq!(v.waits(), (false, false));
    assert!(v.card().last_commit().unwrap().mentions(B));
}

// The VT release waits on the flip as the modeset does.
#[test]
fn a_sleep_waits_for_the_pending_flip() {
    let mut v = awake(one());
    frame(&mut v, &[DOT]).unwrap();
    v.sleep();
    assert!(
        events(&mut v).is_empty(),
        "not asleep while the flip is pending"
    );
    assert!(v.card().state().master);
    assert_eq!(v.waits(), (true, false));
    flip(&mut v);
    assert_eq!(events(&mut v), [Event::Asleep]);
    assert!(!v.card().state().master);
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(399, "CRTC_ID"), Some(0));
    assert_eq!(c.get(cursor_plane(&v, A), "FB_ID"), Some(0));
    assert!(v.canvas(DisplayId(A)).is_none());
    assert!(matches!(v.swap(DisplayId(A), &[]), Err(SwapError::Off)));
}

#[test]
fn a_sleep_goes_ahead_when_the_flip_never_completes() {
    let mut v = awake(one());
    frame(&mut v, &[DOT]).unwrap();
    v.sleep();
    v.flip_expired(100);
    assert_eq!(events(&mut v), [Event::Asleep]);
    assert!(!v.card().state().master);
    // A late event finds nothing flipping.
    flip(&mut v);
    assert!(events(&mut v).is_empty());
}

#[test]
fn a_sleep_blanks_the_shown_buffer() {
    let mut v = awake(one());
    let canvas = v.canvas(DisplayId(A)).unwrap();
    canvas.pixels.fill(0xee);
    v.swap(DisplayId(A), &[rect(0, 0, 640, 480)]).unwrap();
    flip(&mut v);
    v.sleep();
    let o = &mut v.outputs[0];
    let front = o.front();
    assert!(o.buffers[front].map.iter().all(|&b| b == 0));
}

#[test]
fn waking_again_sets_the_mode_and_asks_for_a_redraw() {
    let mut v = awake(one());
    v.sleep();
    assert_eq!(events(&mut v), [Event::Asleep]);
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A)), Event::Awake]);
    assert!(v.card().last_commit().unwrap().flags & DRM_MODE_ATOMIC_ALLOW_MODESET != 0);
}

// The open makes the card DRM master; the seat puts a card opened while
// its VT is not shown to sleep, which lets master go for the shown VT's
// vtium, and the acquire takes it (vtium/vtium#139).
// covers: platform.vt-switch
#[test]
fn a_card_opened_while_the_vt_is_not_shown_lets_master_go_until_the_acquire() {
    let mut v = one();
    assert!(v.card().state().master, "the open made the card master");
    v.sleep();
    assert_eq!(events(&mut v), [Event::Asleep]);
    assert!(!v.card().state().master);
    v.wake_up().unwrap();
    assert!(v.card().state().master);
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A)), Event::Awake]);
}

// A card opened for the shown VT is woken at once and keeps the master
// its open gave it.
#[test]
fn a_card_opened_while_the_vt_is_shown_keeps_master() {
    let mut v = one();
    v.wake_up().unwrap();
    let s = v.card().state();
    assert!(s.master);
    assert_eq!(s.master_drops, 0, "master was never let go");
}

// A card opened while another process holds master, as after the shown
// VT's vtium took it, sleeps with nothing to let go.
#[test]
fn a_card_opened_without_master_sleeps_quietly() {
    let mut v = one();
    {
        let mut s = v.card().state();
        s.master = false;
        s.master_elsewhere = true;
    }
    v.sleep();
    assert_eq!(events(&mut v), [Event::Asleep]);
    assert!(!v.card().state().master);
}

// A monitor's power cycle: the connector goes, and another comes, as an
// MST connector is replaced (aaronsb/kmscon#237). With no display left the
// card keeps master, as its VT is still shown.
#[test]
fn a_power_cycle_skips_the_gone_connector() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(640, 480, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c == A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    logs::capture();
    // The kernel still lists the connector, but it answers no more.
    v.card().state().connectors.remove(&A);
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Removed(DisplayId(A))]);
    assert!(v.card().state().master, "no display left: master kept");
    let before = v.card().state().commits.len();
    {
        let mut s = v.card().state();
        s.listed = vec![B];
    }
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(B))]);
    let commits = v.card().state().commits[before..].to_vec();
    assert_eq!(commits.len(), 2, "one test commit and one modeset");
    assert!(commits.iter().all(|c| !c.mentions(A)));
    assert!(commits[1].mentions(B));
    assert!(
        logs::at_least(Level::Warn).is_empty(),
        "{:?}",
        logs::at_least(Level::Warn)
    );
}

// While another process holds master, the test-only commit is refused
// with EACCES; that is expected, so it is no error in the log, and the
// modeset is not tried again with default modes (aaronsb/kmscon#237).
#[test]
fn eacces_from_the_test_commit_is_not_an_error() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(640, 480, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c == A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    logs::capture();
    let before = v.card().state().commits.len();
    v.card().state().listed.push(B);
    v.card().state().commit_errors.push_back(Errno::ACCESS);
    assert!(v.poll().is_err());
    let commits = v.card().state().commits[before..].to_vec();
    assert_eq!(commits.len(), 1, "no retry with default modes");
    assert!(commits[0].test_only());
    assert!(
        logs::at_least(Level::Error).is_empty(),
        "{:?}",
        logs::at_least(Level::Error)
    );
    assert_eq!(v.waits(), (false, true), "the wake-up retries");
    v.retry_wake();
    assert_eq!(v.waits(), (false, false));
    assert!(events(&mut v).contains(&Event::Added(DisplayId(B))));
}

// The C reference retried a failed test commit with each display's default
// mode, but the retry sent the first attempt's mode blob and buffers.
#[test]
fn a_failed_test_commit_retries_with_the_default_mode() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true), mode(320, 240, 60, false)])]);
    let policy = ModePolicy {
        use_original: false,
        wanted: Some(Box::new(|_: &str| {
            Some(Wanted {
                width: 320,
                height: 240,
                rate: None,
            })
        })),
    };
    let mut v = Drm2d::with(card, policy).unwrap();
    v.card().state().commit_errors.push_back(Errno::INVAL);
    logs::capture();
    v.wake_up().unwrap();
    let commits = v.card().state().commits.clone();
    assert_eq!(commits.len(), 3);
    let first = commits[0].blob(100, "MODE_ID").unwrap();
    assert_eq!(*first, mode_bytes(&mode(320, 240, 60, false).0));
    let blob = commits[2].blob(100, "MODE_ID").unwrap();
    assert_eq!(*blob, mode_bytes(&mode(640, 480, 60, true).0));
    assert_eq!(
        v.card().state().blobs.len(),
        1,
        "the failed attempt's blob is destroyed"
    );
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    flip(&mut v);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().width, 640);
    assert_eq!(logs::at_least(Level::Error).len(), 1);
}

#[test]
fn a_configured_mode_is_set() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true), mode(320, 240, 60, false)])]);
    let policy = ModePolicy {
        use_original: false,
        wanted: Some(Box::new(|name: &str| {
            (name == "HDMI-A-1").then_some(Wanted {
                width: 320,
                height: 240,
                rate: Some(60),
            })
        })),
    };
    let mut v = awake(Drm2d::with(card, policy).unwrap());
    assert_eq!(v.size(DisplayId(A)), Some((320, 240)));
    assert_eq!(v.canvas(DisplayId(A)).unwrap().height, 240);
}

#[test]
fn use_original_keeps_the_crtcs_mode() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true), mode(320, 240, 60, false)])]);
    {
        let mut s = card.state();
        s.connectors.get_mut(&A).unwrap().encoder = 200;
        s.encoders.get_mut(&200).unwrap().crtc = 100;
        s.crtc_modes.insert(100, mode(320, 240, 60, false));
    }
    let policy = ModePolicy {
        use_original: true,
        wanted: None,
    };
    let v = awake(Drm2d::with(card, policy).unwrap());
    assert_eq!(v.size(DisplayId(A)), Some((320, 240)));
}

#[test]
fn the_wake_up_retries_while_master_is_held_elsewhere() {
    let mut v = one();
    v.card().state().master_elsewhere = true;
    assert!(v.wake_up().is_err());
    assert_eq!(v.waits(), (false, true));
    for _ in 0..3 {
        v.retry_wake();
    }
    assert!(events(&mut v).is_empty());
    v.card().state().master_elsewhere = false;
    v.retry_wake();
    assert_eq!(v.waits(), (false, false));
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A)), Event::Awake]);
}

#[test]
fn the_wake_up_retries_run_out() {
    let mut v = one();
    v.card().state().master_elsewhere = true;
    assert!(v.wake_up().is_err());
    let mut tries = 0;
    while v.waits().1 {
        v.retry_wake();
        tries += 1;
    }
    assert_eq!(tries, crate::WAKE_RETRIES);
    assert_eq!(events(&mut v), [Event::WakeFailed]);
}

// A card with no display keeps DRM master while its VT is shown, so the
// kernel's console never lights a display plugged in later.
#[test]
fn a_connector_without_modes_is_no_display() {
    let card = FakeCard::new(&[(A, vec![])]);
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Awake]);
    assert_eq!(v.displays().count(), 0);
    let s = v.card().state();
    assert!(
        s.master && s.master_drops == 0,
        "master kept with no display"
    );
}

#[test]
fn a_gone_display_frees_what_it_held() {
    let mut v = awake(one());
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    assert_eq!(v.card().state().dumbs.len(), 3);
    v.card().state().connectors.remove(&A);
    v.poll().unwrap();
    let s = v.card().state();
    assert!(s.dumbs.is_empty() && s.fbs.is_empty() && s.blobs.is_empty());
}

#[test]
fn mode_blobs_have_the_kernels_layout() {
    let mut m = mode(1920, 1080, 60, true).0;
    m.clock = 0x0102_0304;
    m.vscan = 7;
    m.name[0] = 0x41;
    let b = mode_bytes(&m);
    assert_eq!(b.len(), 68);
    assert_eq!(&b[0..4], &0x0102_0304u32.to_ne_bytes());
    assert_eq!(&b[4..6], &1920u16.to_ne_bytes());
    assert_eq!(&b[14..16], &1080u16.to_ne_bytes());
    assert_eq!(&b[22..24], &7u16.to_ne_bytes());
    assert_eq!(&b[24..28], &60u32.to_ne_bytes());
    assert_eq!(b[36], 0x41);
}

struct App(Drm2d<FakeCard>);

fn in_loop(v: Drm2d<FakeCard>) -> (crate::event_loop::Loop<App>, App) {
    let lp = crate::event_loop::Loop::new().unwrap();
    let mut app = App(v);
    app.0
        .insert(&lp.handle(), |app: &mut App| Some(&mut app.0))
        .unwrap();
    (lp, app)
}

/// Turns the loop until `done`, at most 5 s; the time it took.
#[expect(
    clippy::disallowed_methods,
    reason = "the test measures the loop's timers"
)]
fn turn_until(
    lp: &mut crate::event_loop::Loop<App>,
    app: &mut App,
    mut done: impl FnMut(&mut App) -> bool,
) -> std::time::Duration {
    let start = std::time::Instant::now();
    while !done(app) {
        assert!(start.elapsed().as_secs() < 5, "the loop never got there");
        lp.dispatch(Some(std::time::Duration::from_millis(100)), app)
            .unwrap();
    }
    start.elapsed()
}

// Through the real event loop: flip events arrive on the card's descriptor,
// and a sleep over a flip that never completes goes ahead after the bound.
#[test]
fn the_loop_delivers_flips_and_bounds_the_wait() {
    let (mut lp, mut app) = in_loop(one());
    app.0.wake_up().unwrap();
    app.0.card().complete_flips();
    let mut seen = Vec::new();
    turn_until(&mut lp, &mut app, |app| {
        seen.extend(app.0.events());
        seen.contains(&Event::Flipped(DisplayId(A)))
    });
    frame(&mut app.0, &[DOT]).unwrap();
    app.0.sleep();
    let took = turn_until(&mut lp, &mut app, |app| {
        app.0.events().any(|e| e == Event::Asleep)
    });
    assert!(took.as_millis() >= 900, "asleep after {took:?}");
    assert!(!app.0.card().state().master);
}

#[test]
fn the_loop_retries_the_wake_up() {
    let (mut lp, mut app) = in_loop(one());
    app.0.card().state().master_elsewhere = true;
    assert!(app.0.wake_up().is_err());
    turn_until(&mut lp, &mut app, |app| {
        // The other holder lets go after a few tries.
        let tried = app.0.retries.unwrap_or(0);
        app.0.card().state().master_elsewhere = tried < 3;
        app.0.events().any(|e| e == Event::Awake)
    });
    assert_eq!(app.0.waits(), (false, false));
}

// The frame contract, rule 3: a swap with nothing to send commits nothing.
#[test]
fn a_swap_with_nothing_to_send_commits_nothing() {
    let mut v = awake(one());
    let before = v.card().state().commits.len();
    assert_eq!(frame(&mut v, &[]).unwrap(), Swapped::Nothing);
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Nothing);
    assert!(!v.need_commit(DisplayId(A)));
    assert_eq!(v.card().state().commits.len(), before);
    assert!(!v.is_flipping(DisplayId(A)));
}

// Rule 3: the whole screen is a rectangle covering it, sent as no clips.
#[test]
fn a_rectangle_covering_the_screen_is_the_whole_screen() {
    let mut v = awake(one());
    frame(&mut v, &[rect(0, 0, 8, 8)]).unwrap();
    flip(&mut v);
    frame(&mut v, &[rect(0, 0, 640, 480)]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(clips(&c, primary(&v, A)), None);
}

// Rule 4: moving the pointer on the cursor plane keeps the front
// framebuffer on the primary plane, and the back buffer stays the back.
#[test]
fn a_cursor_move_commits_only_the_cursor_plane() {
    let mut v = awake(one());
    frame(&mut v, &[rect(0, 0, 8, 8)]).unwrap();
    flip(&mut v);
    let age = v.canvas(DisplayId(A)).unwrap().age;
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    assert!(v.hardware_cursor(DisplayId(A)));
    v.show_cursor(DisplayId(A), 3, 4).unwrap();
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Cursor);
    let c = v.card().last_commit().unwrap();
    let plane = primary(&v, A);
    assert_eq!(c.get(cursor_plane(&v, A), "CRTC_X"), Some(3));
    assert!(!c.mentions(plane), "the primary plane is left as it is");
    assert_eq!(c.get(100, "MODE_ID"), None);
    flip(&mut v);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, age);
}

// Rule 5: a swap after a failed one, with nothing new drawn, presents the
// frame that failed with its damage.
#[test]
fn a_swap_after_a_failed_one_sends_the_kept_frame() {
    let mut v = awake(one());
    frame(&mut v, &[rect(0, 0, 8, 8)]).unwrap();
    flip(&mut v);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    assert!(frame(&mut v, &[rect(0, 0, 8, 16)]).is_err());
    assert!(v.need_commit(DisplayId(A)));
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Frame);
    let c = v.card().last_commit().unwrap();
    assert_eq!(clips(&c, primary(&v, A)), Some(vec![rect(0, 0, 8, 16)]));
}

// Rule 4: the display says whether a cursor plane carries the pointer.
#[test]
fn the_display_says_whether_a_plane_carries_the_pointer() {
    let mut v = awake(one());
    assert!(!v.hardware_cursor(DisplayId(A)));
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    assert!(v.hardware_cursor(DisplayId(A)));
    v.destroy_cursor(DisplayId(A));
    assert!(!v.hardware_cursor(DisplayId(A)));
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().atomic = false;
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert!(v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).is_err());
    assert!(!v.hardware_cursor(DisplayId(A)));
}

// Finding 5: the bound on a flip covers every flip in flight. A flip whose
// event is lost is taken as done after it, with one warning, and drawing
// resumes.
// covers: platform.drm2d-fbdev
#[test]
fn a_lost_flip_event_is_taken_as_done_after_the_bound() {
    let (mut lp, mut app) = in_loop(awake(one()));
    logs::capture();
    app.0.card().state().lose_flips = true;
    for _ in 0..2 {
        frame(&mut app.0, &[DOT]).unwrap();
        app.0.card().complete_flips();
        let took = turn_until(&mut lp, &mut app, |app| {
            app.0.events().any(|e| e == Event::Flipped(DisplayId(A)))
        });
        assert!(took.as_millis() >= 900, "flipped after {took:?}");
        assert!(app.0.canvas(DisplayId(A)).is_some());
    }
    assert_eq!(logs::at_least(Level::Warn).len(), 1, "one warning");
}

// An event that comes after the bound still tells the owner it may draw:
// the swap it tried meanwhile was busy.
#[test]
fn a_late_flip_event_after_the_bound_is_a_flip() {
    let (mut lp, mut app) = in_loop(awake(one()));
    frame(&mut app.0, &[DOT]).unwrap();
    turn_until(&mut lp, &mut app, |app| {
        app.0.events().any(|e| e == Event::Flipped(DisplayId(A)))
    });
    assert!(matches!(frame(&mut app.0, &[DOT]), Err(SwapError::Busy)));
    app.0.card().complete_flips();
    turn_until(&mut lp, &mut app, |app| {
        app.0.events().any(|e| e == Event::Flipped(DisplayId(A)))
    });
    assert_eq!(app.0.swap(DisplayId(A), &[]).unwrap(), Swapped::Frame);
}

// vtium/vtium#171: the card refuses a frame's commit as busy while no flip
// of vtium's is in flight, as while a commit vtium does not track holds the
// CRTC, and sends no flip event, since that commit is not vtium's. The
// display waits for its next vblank as for a flip in flight, the vblank's
// event wakes the loop, and the swap the owner tries after it puts the
// frame on the primary plane.
// covers: platform.drm2d-fbdev
#[test]
fn a_frame_refused_as_busy_with_no_flip_in_flight_reaches_the_screen() {
    let (mut lp, mut app) = in_loop(awake(one()));
    let o = app.0.outputs.iter().find(|o| o.id.0 == A).unwrap();
    let (back, fb) = (o.back, o.back_fb());
    app.0.card().state().commit_errors.push_back(Errno::BUSY);
    logs::capture();
    assert!(matches!(frame(&mut app.0, &[DOT]), Err(SwapError::Busy)));
    assert!(logs::at_least(Level::Warn).is_empty());
    assert!(
        app.0.is_flipping(DisplayId(A)),
        "the refused frame waits for the display's next vblank"
    );
    let sent = app.0.card().real_commits().len();
    assert!(matches!(
        app.0.swap(DisplayId(A), &[]),
        Err(SwapError::Busy)
    ));
    assert_eq!(app.0.card().real_commits().len(), sent, "nothing is sent");
    app.0.card().vblank();
    turn_until(&mut lp, &mut app, |app| !app.0.is_flipping(DisplayId(A)));
    assert!(
        logs::at_least(Level::Warn).is_empty(),
        "the vblank's event ended the wait, not the flip bound"
    );
    assert!(events(&mut app.0).is_empty(), "no frame of vtium's flipped");
    // The owner draws again: the back buffer still holds the frame.
    assert!(app.0.need_commit(DisplayId(A)));
    assert_eq!(app.0.canvas(DisplayId(A)).unwrap().age, 1);
    assert_eq!(app.0.swap(DisplayId(A), &[]).unwrap(), Swapped::Frame);
    flip(&mut app.0);
    assert!(app.0.card().shows(100, fb), "{:?}", app.0.card().state());
    let o = app.0.outputs.iter().find(|o| o.id.0 == A).unwrap();
    assert_eq!(o.front(), back);
    assert_eq!(o.buffers[back].map[0], 1, "the frame drawn is shown");
}

// A card that refuses every commit as busy is sent one commit a vblank:
// each refusal waits for the next vblank, the loop sleeps until it passes,
// and nothing is sent meanwhile.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the test measures that the loop sleeps"
)]
fn a_card_refusing_every_commit_is_sent_one_a_vblank() {
    let (mut lp, mut app) = in_loop(awake(one()));
    let before = app.0.card().real_commits().len();
    logs::capture();
    for n in 1..=3 {
        app.0.card().state().commit_errors.push_back(Errno::BUSY);
        assert!(matches!(frame(&mut app.0, &[DOT]), Err(SwapError::Busy)));
        assert!(app.0.canvas(DisplayId(A)).is_none());
        assert!(matches!(
            app.0.swap(DisplayId(A), &[]),
            Err(SwapError::Busy)
        ));
        assert_eq!(app.0.card().real_commits().len(), before + n);
        let start = std::time::Instant::now();
        lp.dispatch(Some(std::time::Duration::from_millis(100)), &mut app)
            .unwrap();
        assert!(start.elapsed().as_millis() >= 90, "the loop slept");
        assert!(app.0.is_flipping(DisplayId(A)), "still waiting");
        app.0.card().vblank();
        turn_until(&mut lp, &mut app, |app| !app.0.is_flipping(DisplayId(A)));
    }
    assert!(events(&mut app.0).is_empty());
    assert!(logs::at_least(Level::Warn).is_empty());
}

// A vblank waited for whose event is lost is taken as done after the flip
// bound, with one warning and no `Flipped`, since no frame flipped.
#[test]
fn a_lost_vblank_event_is_taken_as_done_after_the_bound() {
    let (mut lp, mut app) = in_loop(awake(one()));
    logs::capture();
    app.0.card().state().commit_errors.push_back(Errno::BUSY);
    assert!(matches!(frame(&mut app.0, &[DOT]), Err(SwapError::Busy)));
    let took = turn_until(&mut lp, &mut app, |app| !app.0.is_flipping(DisplayId(A)));
    assert!(took.as_millis() >= 900, "taken as done after {took:?}");
    assert!(events(&mut app.0).is_empty(), "no frame flipped");
    assert_eq!(logs::at_least(Level::Warn).len(), 1, "one warning");
    assert_eq!(app.0.swap(DisplayId(A), &[]).unwrap(), Swapped::Frame);
    // The vblank's event, arriving late, ends nothing: a flip is in flight.
    app.0.card().state().vblanks.push(100);
    app.0.card().vblank();
    app.0.dispatch().unwrap();
    assert!(app.0.is_flipping(DisplayId(A)));
}

// A refused frame whose vblank the card will not wait for, as on a CRTC
// that is not active, has nothing to wait for: the display lends its
// canvas again, and the frame goes with the next swap.
#[test]
fn a_refused_frame_whose_vblank_cannot_be_waited_for_waits_for_no_event() {
    let mut v = awake(one());
    v.card().state().commit_errors.push_back(Errno::BUSY);
    v.card().state().failing.push("wait_vblank");
    logs::capture();
    assert!(matches!(frame(&mut v, &[DOT]), Err(SwapError::Busy)));
    assert!(logs::at_least(Level::Warn).is_empty());
    assert!(!v.is_flipping(DisplayId(A)));
    assert_eq!(v.swap(DisplayId(A), &[]).unwrap(), Swapped::Frame);
}

/// A card with A online and B known to the card but not yet plugged in.
fn a_then_b(policy: ModePolicy) -> Drm2d<FakeCard> {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true), mode(320, 240, 60, false)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c == A);
    awake(Drm2d::with(card, policy).unwrap())
}

/// The framebuffers of a display's buffers, and their size.
fn fbs(v: &Drm2d<FakeCard>, id: u32) -> (Vec<u32>, (u32, u32)) {
    let o = v.outputs.iter().find(|o| o.id.0 == id).unwrap();
    (o.buffers.iter().map(|b| b.fb).collect(), o.buffers[0].size)
}

// Finding 6: a hotplug's modeset keeps what a running display shows: its
// front buffer stays on the primary plane, and its buffers keep their
// frames and ages, so it needs no redraw.
#[test]
fn a_hotplug_keeps_what_the_running_display_shows() {
    let mut v = a_then_b(ModePolicy::default());
    for _ in 0..2 {
        v.canvas(DisplayId(A)).unwrap().pixels.fill(0xee);
        v.swap(DisplayId(A), &[rect(0, 0, 640, 480)]).unwrap();
        flip(&mut v);
    }
    v.events().for_each(drop);
    let o = &v.outputs[0];
    let shown = o.buffers[o.front()].fb;
    let held = fbs(&v, A);
    v.card().state().listed.push(B);
    v.poll().unwrap();
    let c = v.card().last_commit().unwrap();
    assert_ne!(c.flags & DRM_MODE_ATOMIC_ALLOW_MODESET, 0);
    assert_eq!(c.get(primary(&v, A), "FB_ID"), Some(u64::from(shown)));
    assert_eq!(fbs(&v, A), held);
    let o = &v.outputs[0];
    assert!(o.buffers[o.front()].map.iter().all(|&b| b == 0xee));
    assert_eq!(events(&mut v), [Event::Added(DisplayId(B))]);
    flip(&mut v);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 2);
}

// Finding 6: a modeset whose default-mode retry would resize a running
// display, and fails, leaves the display its buffers.
#[test]
fn a_failed_resize_keeps_the_running_displays_buffers() {
    let policy = ModePolicy {
        use_original: false,
        wanted: Some(Box::new(|name: &str| {
            (name == "HDMI-A-1").then_some(Wanted {
                width: 320,
                height: 240,
                rate: None,
            })
        })),
    };
    let mut v = a_then_b(policy);
    let held = fbs(&v, A);
    assert_eq!(held.1, (320, 240));
    v.card().state().listed.push(B);
    v.card()
        .state()
        .commit_errors
        .extend([Errno::INVAL, Errno::INVAL]);
    let _ = v.poll();
    assert_eq!(fbs(&v, A), held);
    assert_eq!(v.size(DisplayId(A)), Some((320, 240)));
}

// Finding 1: a display plugged in whose modeset keeps failing beside the
// displays online is left off until the next hotplug; the displays online
// keep DRM master and what they show, and no retries start.
// covers: platform.drm2d-fbdev
#[test]
fn a_new_display_that_cannot_be_set_is_left_off() {
    let mut v = a_then_b(ModePolicy::default());
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    v.events().for_each(drop);
    v.card().state().listed.push(B);
    v.card().state().refused.push(101);
    logs::capture();
    v.poll().unwrap();
    assert_eq!(v.waits(), (false, false), "no retries");
    assert!(v.card().state().master);
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
    assert!(events(&mut v).is_empty());
    let warned = logs::at_least(Level::Warn);
    assert!(warned.iter().any(|w| w.contains("HDMI-A-2")), "{warned:?}");
    let c = v.card().last_commit().unwrap();
    assert!(c.mentions(A) && !c.mentions(B));
    flip(&mut v);
    assert_eq!(events(&mut v), [Event::Flipped(DisplayId(A))]);
    assert!(v.canvas(DisplayId(A)).is_some());
    // B is tried again at the next hotplug.
    v.card().state().refused.clear();
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(B))]);
}

// Finding 1: once the wake-up retries run out, the next hotplug tries the
// wake-up again.
#[test]
fn a_hotplug_after_the_retries_ran_out_wakes_the_card() {
    let mut v = one();
    v.card().state().master_elsewhere = true;
    assert!(v.wake_up().is_err());
    while v.waits().1 {
        v.retry_wake();
    }
    assert_eq!(events(&mut v), [Event::WakeFailed]);
    v.card().state().master_elsewhere = false;
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A)), Event::Awake]);
    assert!(v.card().state().master);
}

// Round 4, finding 2: once the wake-up retries run out, the next input
// tries the wake-up again, which brings every display on, blanked ones
// included. Input while the retries run, or with nothing failed, does
// nothing.
#[test]
fn input_after_the_retries_ran_out_wakes_the_card() {
    let mut v = a_then_b(ModePolicy::default());
    v.card().state().listed.push(B);
    v.poll().unwrap();
    flip(&mut v);
    let (mut blank, at) = blanked(&mut v);
    v.sleep();
    flip(&mut v);
    v.events().for_each(drop);
    v.card().state().master_elsewhere = true;
    assert!(v.wake_up().is_err());
    v.input().unwrap();
    v.retry_wake();
    assert_eq!(v.retries, Some(1), "input leaves the retries be");
    while v.waits().1 {
        v.retry_wake();
    }
    assert_eq!(events(&mut v), [Event::WakeFailed]);
    assert!(!v.card().state().master);
    v.card().state().master_elsewhere = false;
    assert!(blank.input(at));
    v.input().unwrap();
    v.unblank().unwrap();
    assert!(v.card().state().master);
    assert_eq!(
        events(&mut v),
        [
            Event::Redraw(DisplayId(A)),
            Event::Redraw(DisplayId(B)),
            Event::Awake
        ]
    );
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        (c.get(100, "ACTIVE"), c.get(101, "ACTIVE")),
        (Some(1), Some(1))
    );
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
    assert_eq!(v.dpms(DisplayId(B)), Some(Dpms::On));
    flip(&mut v);
    let commits = v.card().state().commits.len();
    v.input().unwrap();
    assert_eq!(v.card().state().commits.len(), commits, "nothing failed");
}

// Round 4, finding 2: once a failed hotplug's retries run out, master is
// kept, and the next input tries the modeset again. Blanked, it is owed to
// the `unblank` of the same input, so nothing is committed while blanked.
#[test]
fn input_after_a_hotplugs_retries_ran_out_sets_the_modes() {
    let mut v = a_then_b(ModePolicy::default());
    v.card().state().listed.push(B);
    v.card().state().commit_errors.extend([Errno::INVAL; 200]);
    assert!(v.poll().is_err());
    while v.waits().1 {
        v.retry_wake();
    }
    assert_eq!(events(&mut v), [Event::WakeFailed]);
    assert!(v.card().state().master);
    v.card().state().commit_errors.clear();
    let (mut blank, at) = blanked(&mut v);
    let commits = v.card().state().commits.len();
    assert!(blank.input(at));
    v.input().unwrap();
    assert_eq!(v.card().state().commits.len(), commits, "blanked");
    v.unblank().unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        (c.get(100, "ACTIVE"), c.get(101, "ACTIVE")),
        (Some(1), Some(1))
    );
    assert_eq!(
        events(&mut v),
        [Event::Redraw(DisplayId(A)), Event::Added(DisplayId(B))]
    );
}

// Finding 2 and the frame contract, rule 6: a display powered down lends
// no canvas, so no frame is drawn and lost; powered on, every buffer is
// age 0, its next frame goes whole, and the owner is told to redraw.
// covers: platform.drm2d-fbdev
#[test]
fn a_powered_down_display_lends_no_canvas() {
    let mut v = awake(one());
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    v.events().for_each(drop);
    v.set_dpms(DisplayId(A), Dpms::Off).unwrap();
    assert!(v.canvas(DisplayId(A)).is_none());
    v.set_dpms(DisplayId(A), Dpms::On).unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A))]);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 0);
    frame(&mut v, &[DOT]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(clips(&c, primary(&v, A)), None, "the whole frame");
}

// Finding 3, the operator's decision: a hotplug while blanked records the
// change and commits nothing; the display plugged in stays dark, and the
// next input runs the modeset owed, with every display on.
#[test]
fn a_hotplug_while_blanked_commits_nothing_until_input() {
    let mut v = a_then_b(ModePolicy::default());
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    v.swap(DisplayId(A), &[]).unwrap();
    flip(&mut v);
    let (mut blank, at) = blanked(&mut v);
    v.events().for_each(drop);
    let commits = v.card().state().commits.len();
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert!(events(&mut v).is_empty(), "no Redraw, nothing added yet");
    assert_eq!(v.card().state().commits.len(), commits, "nothing committed");
    assert_eq!(v.card().state().dpms[&A], 3);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::Off));
    assert!(blank.blanked(), "a hotplug is no input");
    activity(&mut v, &mut blank, at);
    let c = v.card().last_commit().unwrap();
    assert_ne!(c.flags & DRM_MODE_ATOMIC_ALLOW_MODESET, 0);
    assert_eq!(
        (c.get(100, "ACTIVE"), c.get(101, "ACTIVE")),
        (Some(1), Some(1))
    );
    assert_eq!(
        events(&mut v),
        [Event::Redraw(DisplayId(A)), Event::Added(DisplayId(B))]
    );
    assert_eq!(v.card().state().dpms[&A], 0);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
    flip(&mut v);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 0);
}

// A monitor asleep can leave and come back as a hotplug pulse. While
// blanked that changes nothing on screen: nothing is committed, and the
// blank's timer runs on.
#[test]
fn a_hotplug_pulse_while_blanked_changes_nothing_visible() {
    let mut v = awake(one());
    let (mut blank, at) = blanked(&mut v);
    v.events().for_each(drop);
    let commits = v.card().state().commits.len();
    let conn = v.card().state().connectors.remove(&A).unwrap();
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Removed(DisplayId(A))]);
    v.card().state().connectors.insert(A, conn);
    v.poll().unwrap();
    assert!(events(&mut v).is_empty());
    assert_eq!(v.card().state().commits.len(), commits, "nothing committed");
    assert_eq!(v.card().state().dpms[&A], 3);
    assert!(blank.blanked());
    assert_eq!(blank.due(at + DPMS_TIMEOUT / 2), Due::In(DPMS_TIMEOUT));
    activity(&mut v, &mut blank, at);
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A))]);
    assert_eq!(v.card().state().dpms[&A], 0);
}

// Re-review finding 5: the blank holds whether or not a display's connector
// has a DPMS property, so a hotplug while blanked commits nothing.
#[test]
fn a_display_without_dpms_keeps_the_blank() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c == A);
    card.state().no_dpms.push(A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(v.dpms(DisplayId(A)), None);
    let (mut blank, at) = blanked(&mut v);
    v.events().for_each(drop);
    let commits = v.card().state().commits.len();
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert_eq!(v.card().state().commits.len(), commits, "nothing committed");
    assert!(events(&mut v).is_empty());
    activity(&mut v, &mut blank, at);
    let c = v.card().last_commit().unwrap();
    assert_ne!(c.flags & DRM_MODE_ATOMIC_ALLOW_MODESET, 0);
    assert_eq!(events(&mut v), [Event::Added(DisplayId(B))]);
}

// Re-review finding 3: a hotplug's modeset deferred behind a flip in
// flight, when the blank comes before the flip completes, commits nothing
// and is owed to the input that ends the blank.
#[test]
fn a_deferred_modeset_waits_for_input_once_blanked() {
    let mut v = a_then_b(ModePolicy::default());
    frame(&mut v, &[DOT]).unwrap();
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert_eq!(v.waits(), (true, false), "the modeset waits on the flip");
    let (mut blank, at) = blanked(&mut v);
    let commits = v.card().state().commits.len();
    flip(&mut v);
    assert_eq!(v.card().state().commits.len(), commits, "nothing committed");
    assert_eq!(v.card().state().dpms[&A], 3);
    assert_eq!(v.waits(), (false, false));
    v.events().for_each(drop);
    activity(&mut v, &mut blank, at);
    let c = v.card().last_commit().unwrap();
    assert_ne!(c.flags & DRM_MODE_ATOMIC_ALLOW_MODESET, 0);
    assert_eq!(
        (c.get(100, "ACTIVE"), c.get(101, "ACTIVE")),
        (Some(1), Some(1))
    );
    assert_eq!(v.card().state().dpms[&A], 0);
}

// Re-review finding 3: the retries of a hotplug that failed commit nothing
// once blanked; the modeset is owed to the input that ends the blank.
#[test]
fn a_hotplug_retry_waits_for_input_once_blanked() {
    let mut v = a_then_b(ModePolicy::default());
    v.card().state().listed.push(B);
    v.card().state().commit_errors.extend([Errno::INVAL; 4]);
    assert!(v.poll().is_err());
    assert_eq!(v.waits(), (false, true), "the retries");
    let (mut blank, at) = blanked(&mut v);
    v.events().for_each(drop);
    let commits = v.card().state().commits.len();
    v.retry_wake();
    assert_eq!(v.card().state().commits.len(), commits, "nothing committed");
    assert_eq!(v.waits(), (false, false));
    assert!(events(&mut v).is_empty());
    assert_eq!(v.card().state().dpms[&A], 3);
    activity(&mut v, &mut blank, at);
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        (c.get(100, "ACTIVE"), c.get(101, "ACTIVE")),
        (Some(1), Some(1))
    );
    assert_eq!(
        events(&mut v),
        [Event::Redraw(DisplayId(A)), Event::Added(DisplayId(B))]
    );
}

// Round 4, finding 4: a retry of a failed hotplug that fails again keeps
// DRM master and the displays online, so a blank in the retries' window
// powers them down and the next retry commits nothing; the input ending
// the blank runs the modeset with every display on.
#[test]
fn a_failed_retry_keeps_master_through_the_blank() {
    let mut v = a_then_b(ModePolicy::default());
    v.card().state().listed.push(B);
    v.card().state().commit_errors.extend([Errno::INVAL; 8]);
    assert!(v.poll().is_err());
    v.retry_wake();
    assert_eq!(v.waits(), (false, true), "the retries");
    assert!(v.card().state().master);
    let (mut blank, at) = blanked(&mut v);
    assert_eq!(v.card().state().dpms[&A], 3);
    v.events().for_each(drop);
    let commits = v.card().state().commits.len();
    v.retry_wake();
    assert_eq!(v.card().state().commits.len(), commits, "nothing committed");
    assert_eq!(v.card().state().dpms[&A], 3);
    activity(&mut v, &mut blank, at);
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        (c.get(100, "ACTIVE"), c.get(101, "ACTIVE")),
        (Some(1), Some(1))
    );
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
}

// A wake-up runs the modeset a hotplug while blanked left owed, with every
// display on, and leaves nothing owed to the input after it.
#[test]
fn a_wake_up_runs_the_modeset_owed() {
    let mut v = a_then_b(ModePolicy::default());
    let (mut blank, at) = blanked(&mut v);
    let commits = v.card().state().commits.len();
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert_eq!(v.card().state().commits.len(), commits);
    v.sleep();
    flip(&mut v);
    v.events().for_each(drop);
    v.wake_up().unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        (c.get(100, "ACTIVE"), c.get(101, "ACTIVE")),
        (Some(1), Some(1))
    );
    assert_eq!(
        events(&mut v),
        [
            Event::Redraw(DisplayId(A)),
            Event::Added(DisplayId(B)),
            Event::Awake
        ]
    );
    assert_eq!(v.card().state().dpms[&A], 0);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
    flip(&mut v);
    let commits = v.card().state().commits.len();
    activity(&mut v, &mut blank, at);
    assert_eq!(v.card().state().commits.len(), commits, "nothing owed");
}

// A display plugged in and out again while blanked commits nothing; the
// input that ends the blank brings the display left on.
#[test]
fn a_display_in_and_out_while_blanked_commits_nothing() {
    let mut v = a_then_b(ModePolicy::default());
    let (mut blank, at) = blanked(&mut v);
    v.events().for_each(drop);
    let commits = v.card().state().commits.len();
    for _ in 0..2 {
        v.card().state().listed.push(B);
        v.poll().unwrap();
        v.card().state().listed.retain(|&c| c == A);
        v.poll().unwrap();
    }
    assert_eq!(v.card().state().commits.len(), commits, "nothing committed");
    assert!(events(&mut v).is_empty());
    activity(&mut v, &mut blank, at);
    assert_eq!(v.card().state().dpms[&A], 0);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
}

// A wake-up whose modeset fails, with a modeset owed, keeps the recorded
// DPMS as the hardware has it, and its retry brings every display on.
#[test]
fn a_failed_wake_up_keeps_the_recorded_dpms() {
    let mut v = a_then_b(ModePolicy::default());
    blanked(&mut v);
    v.card().state().listed.push(B);
    v.poll().unwrap();
    v.sleep();
    flip(&mut v);
    v.events().for_each(drop);
    v.card().state().commit_errors.extend([Errno::INVAL; 4]);
    assert!(v.wake_up().is_err());
    assert_eq!(v.card().state().dpms[&A], 3);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::Off));
    assert_eq!(v.waits(), (false, true), "the wake-up retries");
    v.retry_wake();
    assert_eq!(v.card().state().dpms[&A], 0);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
    assert_eq!(
        events(&mut v),
        [
            Event::Redraw(DisplayId(A)),
            Event::Added(DisplayId(B)),
            Event::Awake
        ]
    );
}

// Re-review finding 2: the flip of the modeset the input ending the blank
// runs is bounded, as every flip in flight is.
#[test]
fn the_flip_of_the_modeset_owed_is_bounded() {
    let (mut lp, mut app) = in_loop(a_then_b(ModePolicy::default()));
    let (mut blank, at) = blanked(&mut app.0);
    app.0.events().for_each(drop);
    app.0.card().state().listed.push(B);
    app.0.poll().unwrap();
    app.0.card().state().lose_flips = true;
    activity(&mut app.0, &mut blank, at);
    assert!(app.0.is_flipping(DisplayId(A)));
    app.0.card().complete_flips();
    let took = turn_until(&mut lp, &mut app, |app| {
        app.0.events().any(|e| e == Event::Flipped(DisplayId(A)))
    });
    assert!(took.as_millis() >= 900, "flipped after {took:?}");
}

// Re-review finding 1: where the modeset the input ending the blank runs
// fails, it is tried again as a failed hotplug is, and the displays come
// on; the blank, which the input ended, stays ended.
#[test]
fn a_failed_modeset_owed_is_tried_again() {
    let (mut lp, mut app) = in_loop(a_then_b(ModePolicy::default()));
    let (mut blank, at) = blanked(&mut app.0);
    app.0.events().for_each(drop);
    app.0.card().state().listed.push(B);
    app.0.poll().unwrap();
    app.0.card().state().commit_errors.extend([Errno::INVAL; 4]);
    assert!(blank.input(at));
    assert!(app.0.unblank().is_err());
    assert_eq!(app.0.card().state().dpms[&A], 3);
    let mut seen = Vec::new();
    turn_until(&mut lp, &mut app, |app| {
        seen.extend(app.0.events());
        seen.contains(&Event::Added(DisplayId(B)))
    });
    assert_eq!(app.0.card().state().dpms[&A], 0);
    assert_eq!(app.0.dpms(DisplayId(A)), Some(Dpms::On));
    assert!(!seen.contains(&Event::Awake), "no wake-up: {seen:?}");
    assert_eq!(app.0.waits(), (false, false));
}

// Input while blanked with no modeset owed powers the displays on through
// their DPMS, as before.
#[test]
fn input_with_no_modeset_owed_powers_on_through_dpms() {
    let mut v = awake(one());
    let (mut blank, at) = blanked(&mut v);
    v.events().for_each(drop);
    let commits = v.card().state().commits.len();
    activity(&mut v, &mut blank, at);
    assert_eq!(v.card().state().commits.len(), commits, "no modeset");
    assert_eq!(v.card().state().dpms[&A], 0);
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A))]);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 0);
}

// Round 4, finding 1: input with no modeset owed powers every display on
// through its DPMS, though one refuses; the one refused is left owed a
// modeset, which the retries run with every display on.
#[test]
fn a_refused_dpms_unblank_is_tried_again_as_a_modeset() {
    let mut v = a_then_b(ModePolicy::default());
    v.card().state().listed.push(B);
    v.poll().unwrap();
    flip(&mut v);
    let (mut blank, at) = blanked(&mut v);
    v.events().for_each(drop);
    // The stand-in refuses every connector property but DPMS.
    for o in v.outputs.iter_mut().filter(|o| o.id.0 == A) {
        o.dpms = o.dpms.map(|(p, d)| (p + 5000, d));
    }
    assert!(blank.input(at));
    assert!(v.unblank().is_err());
    assert_eq!(v.card().state().dpms[&B], 0, "B powered on");
    assert_eq!(v.card().state().dpms[&A], 3);
    assert_eq!(v.waits(), (false, true), "the retries");
    v.retry_wake();
    let c = v.card().last_commit().unwrap();
    assert_ne!(c.flags & DRM_MODE_ATOMIC_ALLOW_MODESET, 0);
    assert_eq!(
        (c.get(100, "ACTIVE"), c.get(101, "ACTIVE")),
        (Some(1), Some(1))
    );
    assert_eq!(v.card().state().dpms[&A], 0);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
    assert_eq!(v.waits(), (false, false));
    assert!(!blank.blanked());
}

// Input that ends the blank while asleep does nothing and starts no
// retries: the wake-up brings every display on.
#[test]
fn an_unblank_while_asleep_does_nothing() {
    let mut v = awake(one());
    let (mut blank, at) = blanked(&mut v);
    v.sleep();
    flip(&mut v);
    let commits = v.card().state().commits.len();
    assert!(blank.input(at));
    v.unblank().unwrap();
    assert_eq!(v.waits(), (false, false));
    assert_eq!(v.card().state().commits.len(), commits);
}

// A display plugged in is set on, whatever DPMS its connector was left at.
#[test]
fn a_new_display_is_set_on() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().dpms.insert(A, 3);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::On));
    assert_eq!(v.card().state().dpms[&A], 0);
    assert!(frame(&mut v, &[DOT]).is_ok());
}

// A key that ends the blank while a sleep waits on a flip starts nothing:
// once the VT is released, no retry takes master back or commits.
#[test]
fn an_unblank_during_a_sleep_starts_no_retry() {
    let mut v = a_then_b(ModePolicy::default());
    v.card().state().listed.push(B);
    v.poll().unwrap();
    flip(&mut v);
    v.events().for_each(drop);
    frame(&mut v, &[DOT]).unwrap();
    let (mut blank, at) = blanked(&mut v);
    v.sleep();
    assert_eq!(v.waits(), (true, false), "the sleep waits on A's flip");
    for o in v.outputs.iter_mut().filter(|o| o.id.0 == B) {
        o.dpms = o.dpms.map(|(p, d)| (p + 5000, d));
    }
    assert!(blank.input(at));
    v.input().unwrap();
    v.unblank().unwrap();
    flip(&mut v);
    assert!(events(&mut v).contains(&Event::Asleep));
    let commits = v.card().state().commits.len();
    v.retry_wake();
    assert!(
        !v.card().state().master,
        "master taken back after the release"
    );
    assert_eq!(
        v.card().state().commits.len(),
        commits,
        "a commit after the release"
    );
}

/// tier 3's guest: QEMU's virtio-gpu, one virtual connector (40) on its
/// encoder (41) and CRTC (39, index 0), a primary plane (35) and a cursor
/// plane (36) on that CRTC, and no overlay.
fn virtio_gpu() -> FakeCard {
    let card = FakeCard::default();
    {
        let mut s = card.state();
        s.atomic = true;
        s.universal = true;
        s.hotspot = true;
        s.crtcs.push(39);
        s.encoders.insert(
            41,
            Encoder {
                crtc: 0,
                possible_crtcs: 1,
            },
        );
        for (id, kind) in [(35, PLANE_PRIMARY), (36, PLANE_CURSOR)] {
            s.planes.insert(
                id,
                Plane {
                    crtcs: 1,
                    kind,
                    damage: false,
                },
            );
        }
        s.listed.push(40);
        s.dpms.insert(40, 0);
        s.connectors.insert(
            40,
            Connector {
                id: 40,
                kind: 15,
                kind_id: 1,
                connected: true,
                modes: vec![mode(1366, 768, 60, true)],
                mm: (0, 0),
                encoder: 0,
                encoders: vec![41],
            },
        );
    }
    card
}

#[test]
fn the_tier3_guests_display_comes_up_on_its_primary_plane() {
    let mut v = Drm2d::with(virtio_gpu(), ModePolicy::default()).unwrap();
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(40)), Event::Awake]);
    assert_eq!(v.name(DisplayId(40)), Some("Virtual-1"));
    assert_eq!(primary(&v, 40), 35);
    assert_eq!(cursor_plane(&v, 40), 36);
}

// The plane's properties hold its type, so a display whose planes cannot
// be read has no primary plane; the log names the read that failed.
#[test]
fn a_plane_whose_properties_cannot_be_read_is_named() {
    let card = virtio_gpu();
    card.state().unreadable.extend([35, 36]);
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    logs::capture();
    v.wake_up().unwrap();
    assert!(v.outputs.is_empty());
    let errors = logs::at_least(Level::Warn);
    assert!(
        errors
            .iter()
            .any(|l| l == "cannot read the properties of plane 35: Bad address (os error 14)"),
        "{errors:?}"
    );
}

#[test]
fn a_plane_list_that_cannot_be_read_is_named() {
    let card = virtio_gpu();
    card.state().failing.push("planes");
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    logs::capture();
    v.wake_up().unwrap();
    assert!(v.outputs.is_empty());
    let errors = logs::at_least(Level::Warn);
    assert!(
        errors
            .iter()
            .any(|l| l == "cannot list the card's planes: Input/output error (os error 5)"),
        "{errors:?}"
    );
}

#[test]
fn a_plane_whose_crtcs_cannot_be_read_is_named() {
    let card = virtio_gpu();
    card.state().crtcs_unreadable.push(35);
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    logs::capture();
    v.wake_up().unwrap();
    assert!(v.outputs.is_empty());
    let errors = logs::at_least(Level::Warn);
    assert!(
        errors
            .iter()
            .any(|l| l == "cannot read the CRTCs of plane 35: Input/output error (os error 5)"),
        "{errors:?}"
    );
}

// An overlay plane on no CRTC of the display is read only to hide it.
#[test]
fn a_plane_that_cannot_be_hidden_for_its_unread_type_is_named() {
    let card = virtio_gpu();
    card.state().planes.insert(
        37,
        Plane {
            crtcs: 0b10,
            kind: PLANE_OVERLAY,
            damage: false,
        },
    );
    card.state().unreadable.push(37);
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    logs::capture();
    v.wake_up().unwrap();
    assert_eq!(primary(&v, 40), 35);
    let errors = logs::at_least(Level::Warn);
    assert_eq!(
        errors,
        ["cannot read the properties of plane 37: Bad address (os error 14)"]
    );
}

#[test]
fn an_encoder_that_cannot_be_read_is_named() {
    let card = virtio_gpu();
    {
        let mut s = card.state();
        let conn = s.connectors.get_mut(&40).unwrap();
        conn.encoder = 99;
        conn.encoders = vec![98, 99, 41];
    }
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    logs::capture();
    v.wake_up().unwrap();
    assert_eq!(primary(&v, 40), 35, "the readable encoder drives it");
    // Each unreadable encoder is read, and logged, once.
    assert_eq!(
        logs::at_least(Level::Warn),
        [
            "cannot read encoder 99: No such file or directory (os error 2)",
            "cannot read encoder 98: No such file or directory (os error 2)",
        ]
    );
}

#[test]
fn a_crtc_mode_that_cannot_be_read_is_named() {
    let card = virtio_gpu();
    {
        let mut s = card.state();
        s.connectors.get_mut(&40).unwrap().encoder = 41;
        s.encoders.get_mut(&41).unwrap().crtc = 39;
        s.failing.push("crtc_mode");
    }
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    logs::capture();
    v.wake_up().unwrap();
    assert_eq!(primary(&v, 40), 35);
    assert_eq!(
        logs::at_least(Level::Warn),
        ["cannot read the mode of CRTC 39: Input/output error (os error 5)"]
    );
}

#[test]
fn a_dumb_buffer_capability_that_cannot_be_read_is_named() {
    let card = virtio_gpu();
    card.state().failing.push("cap");
    logs::capture();
    assert!(Drm2d::with(card, ModePolicy::default()).is_err());
    assert_eq!(
        logs::at_least(Level::Warn),
        ["cannot read the card's dumb buffer capability: Input/output error (os error 5)"]
    );
}

/// One display at 2560x1440 whose connector reports `mm`.
fn sized(mm: (u32, u32)) -> Drm2d<FakeCard> {
    let card = FakeCard::new(&[(A, vec![mode(2560, 1440, 60, true)])]);
    card.state().connectors.get_mut(&A).unwrap().mm = mm;
    awake(Drm2d::with(card, ModePolicy::default()).unwrap())
}

#[test]
fn the_density_comes_from_the_connectors_physical_size() {
    // A 27-inch monitor.
    let d = sized((597, 336)).density(DisplayId(A)).unwrap();
    assert!(d.measured());
    let (x, y) = d.dpi();
    assert_eq!((x.round(), y.round()), (109.0, 109.0));
}

#[test]
fn a_connector_with_no_physical_size_assumes_the_density() {
    let v = sized((0, 0));
    assert_eq!(v.density(DisplayId(A)), Some(Density::ASSUMED));
    assert_eq!(v.density(DisplayId(B)), None);
}

// ADR-1301 D3: a connector reporting 0 mm takes the size its EDID in
// sysfs states, and with none there still assumes the density
#[test]
fn a_connector_with_no_physical_size_takes_its_edids_in_sysfs() {
    let dir = std::env::temp_dir().join(format!("vtium-drm2d-edid-{}", std::process::id()));
    let conn = dir.join(format!("card0-{}", output::connector_name(11, 1)));
    std::fs::create_dir_all(&conn).unwrap();
    // a 27-inch monitor's base block: 60 by 34 cm
    let mut edid = vec![0u8; 128];
    edid[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    (edid[21], edid[22]) = (60, 34);
    edid[127] = 0u8.wrapping_sub(edid.iter().fold(0u8, |s, &b| s.wrapping_add(b)));
    std::fs::write(conn.join("edid"), &edid).unwrap();
    let density = |mm, sysfs: Option<&std::path::Path>| {
        let card = FakeCard::new(&[(A, vec![mode(2560, 1440, 60, true)])]);
        {
            let mut s = card.state();
            let c = s.connectors.get_mut(&A).unwrap();
            (c.kind, c.kind_id, c.mm) = (11, 1, mm);
            s.sysfs = sysfs.map(std::path::Path::to_path_buf);
        }
        awake(Drm2d::with(card, ModePolicy::default()).unwrap())
            .density(DisplayId(A))
            .unwrap()
    };
    let (from_edid, one_side, measured, none) = (
        density((0, 0), Some(&dir)),
        density((597, 0), Some(&dir)),
        density((299, 168), Some(&dir)),
        density((0, 0), Some(&dir.join("gone"))),
    );
    let _ = std::fs::remove_dir_all(&dir);
    let round = |d: Density| (d.dpi().0.round(), d.dpi().1.round());
    assert!(from_edid.measured());
    assert_eq!(round(from_edid), (108.0, 108.0));
    assert_eq!(round(one_side), (108.0, 108.0));
    // the connector's own size comes first: 13.6 inches across
    assert_eq!(round(measured), (217.0, 218.0));
    assert_eq!(none, Density::ASSUMED);
}

#[test]
fn a_connector_with_an_implausible_size_assumes_the_density() {
    // A projector that reports 1 mm.
    let v = sized((1, 1));
    assert_eq!(v.density(DisplayId(A)), Some(Density::ASSUMED));
}

/// Leaves connector `conn` on CRTC 100 + `at` as another master would: a
/// mode set, active, and its primary plane showing a frame.
fn left_lit(card: &FakeCard, conn: u32, at: u32) {
    let mut s = card.state();
    s.connectors.get_mut(&conn).unwrap().encoder = 200 + at;
    s.encoders.get_mut(&(200 + at)).unwrap().crtc = 100 + at;
    s.crtc_modes.insert(100 + at, mode(640, 480, 60, true));
    s.bound.insert(conn, 100 + at);
    s.active.insert(100 + at, true);
    s.enabled.insert(100 + at, true);
    s.plane_crtc.insert(300 + 2 * at, 100 + at);
    s.plane_fb.insert(300 + 2 * at, 77);
}

/// A card with displays A and B at 640x480, B left lit by another master.
fn dark_card() -> FakeCard {
    let modes = vec![mode(640, 480, 60, true)];
    let card = FakeCard::new(&[(A, modes.clone()), (B, modes)]);
    left_lit(&card, B, 1);
    card
}

/// [`dark_card`] with the policy leaving `dark` dark.
fn with_dark(dark: &'static str) -> Drm2d<FakeCard> {
    dark_on(dark_card(), dark)
}

/// `card`, with the policy leaving `dark` dark.
fn dark_on(card: FakeCard, dark: &'static str) -> Drm2d<FakeCard> {
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    v.set_card_path(Some(String::from("pci-0000:03:00.0")));
    v.set_dark(Box::new(move |d: &Identity| d.connector == dark));
    v
}

/// The identity of connector `name` on the card `with_dark` makes.
fn identity(name: &str, lit_at_start: bool) -> Identity {
    Identity {
        connector: String::from(name),
        card: Some(String::from("pci-0000:03:00.0")),
        edid: None,
        lit_at_start,
    }
}

// ADR-1401 D3: a display the rules send to dark is set off, a frame
// another master left included
#[test]
fn a_connector_the_policy_leaves_dark_is_committed_off_and_announced_as_no_display() {
    let mut v = with_dark("HDMI-A-2");
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A)), Event::Awake]);
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
    assert_eq!(v.name(DisplayId(B)), None);
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(B, "CRTC_ID"), Some(0));
    assert_eq!(
        (c.get(101, "MODE_ID"), c.get(101, "ACTIVE")),
        (Some(0), Some(0))
    );
    assert_eq!(
        (c.get(302, "FB_ID"), c.get(302, "CRTC_ID")),
        (Some(0), Some(0))
    );
    {
        let s = v.card().state();
        assert_eq!(s.active.get(&101), Some(&false));
        assert_eq!(s.plane_crtc.get(&302), None);
    }
    assert_eq!(v.identity(DisplayId(A)), Some(&identity("HDMI-A-1", false)));
    assert_eq!(v.identity(DisplayId(B)), None);
    assert_eq!(v.dark().collect::<Vec<_>>(), [&identity("HDMI-A-2", true)]);
}

// A wake-up sets a dark connector off again, as another master may have
// lit it while vtium slept.
#[test]
fn a_dark_connector_another_master_lit_again_is_set_off_at_the_wake_up() {
    let mut v = awake(with_dark("HDMI-A-2"));
    v.sleep();
    flip(&mut v);
    assert_eq!(events(&mut v), [Event::Asleep]);
    left_lit(v.card(), B, 1);
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A)), Event::Awake]);
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(B, "CRTC_ID"), Some(0));
    assert_eq!(c.get(101, "ACTIVE"), Some(0));
    assert_eq!(v.card().state().active.get(&101), Some(&false));
}

// A CRTC that shows nothing already is left out, so the commit asks no
// flip event of a CRTC that stays off.
#[test]
fn a_dark_connector_on_a_crtc_already_off_stays_out_of_the_commit() {
    let v = with_dark("HDMI-A-2");
    v.card().state().active.insert(101, false);
    let v = awake(v);
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
    let c = v.card().last_commit().unwrap();
    assert!(!c.mentions(B) && !c.mentions(101), "{c:?}");
    assert_eq!(v.dark().count(), 1);
}

// A lit display may take the CRTC a dark connector leaves, here one
// another master left powered down: the connector leaves it, so it shows
// no copy of the display, and the CRTC comes on for the display.
#[test]
fn a_crtc_a_dark_connector_leaves_drives_a_lit_display() {
    let modes = vec![mode(640, 480, 60, true)];
    let card = FakeCard::new(&[(A, modes.clone()), (B, modes)]);
    left_lit(&card, B, 0);
    card.state().active.insert(100, false);
    card.state().encoders.get_mut(&200).unwrap().possible_crtcs = 0b11;
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    v.set_dark(Box::new(|d: &Identity| d.connector == "HDMI-A-2"));
    let v = awake(v);
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        (c.get(A, "CRTC_ID"), c.get(B, "CRTC_ID")),
        (Some(100), Some(0))
    );
    assert_eq!(c.get(100, "ACTIVE"), Some(1));
    assert_ne!(c.get(100, "MODE_ID"), Some(0));
    let s = v.card().state();
    assert_eq!((s.bound.get(&A), s.bound.get(&B)), (Some(&100), Some(&0)));
}

// With every display dark, vtium sets them off and keeps master, as with
// no display at all, while its VT is shown.
#[test]
fn every_display_dark_is_set_off_and_master_kept() {
    let card = FakeCard::new(&[(B, vec![mode(640, 480, 60, true)])]);
    left_lit(&card, B, 0);
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    v.set_dark(Box::new(|_: &Identity| true));
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Awake]);
    assert_eq!(v.displays().count(), 0);
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        c.flags,
        DRM_MODE_ATOMIC_ALLOW_MODESET | DRM_MODE_ATOMIC_NONBLOCK
    );
    assert_eq!(
        (c.get(B, "CRTC_ID"), c.get(100, "ACTIVE")),
        (Some(0), Some(0))
    );
    assert_eq!(
        (c.get(300, "FB_ID"), c.get(300, "CRTC_ID")),
        (Some(0), Some(0))
    );
    let s = v.card().state();
    assert!(s.master && s.master_drops == 0, "master kept");
    assert_eq!(s.active.get(&100), Some(&false));
}

// A dark connector unplugged is forgotten, and plugged back it is asked
// of the policy again, no longer lit at start.
#[test]
fn a_dark_connector_unplugged_is_forgotten_and_asked_again_when_back() {
    let mut v = awake(with_dark("HDMI-A-2"));
    v.card().state().connectors.get_mut(&B).unwrap().connected = false;
    v.poll().unwrap();
    assert_eq!(v.dark().count(), 0);
    v.card().state().connectors.get_mut(&B).unwrap().connected = true;
    v.poll().unwrap();
    assert_eq!(v.dark().collect::<Vec<_>>(), [&identity("HDMI-A-2", false)]);
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
}

// ADR-1401 D9: a dark connector let go is asked of the policy again at
// the next scan, which lights it once the policy does; until let go the
// scan keeps it dark whatever the policy now says.
#[test]
fn a_dark_connector_let_go_is_asked_again_at_the_next_scan() {
    use std::cell::Cell;
    use std::rc::Rc;
    let dark = Rc::new(Cell::new(true));
    let mut v = Drm2d::with(dark_card(), ModePolicy::default()).unwrap();
    v.set_card_path(Some(String::from("pci-0000:03:00.0")));
    let policy = Rc::clone(&dark);
    v.set_dark(Box::new(move |d: &Identity| {
        policy.get() && d.connector == "HDMI-A-2"
    }));
    let mut v = awake(v);
    dark.set(false);
    v.poll().unwrap();
    assert_eq!(v.dark().count(), 1, "kept until let go");
    assert!(v.release_dark("HDMI-A-2"));
    assert!(!v.release_dark("HDMI-A-2"), "let go once");
    v.poll().unwrap();
    assert_eq!(v.dark().count(), 0);
    assert_eq!(
        v.displays().collect::<Vec<_>>(),
        [DisplayId(A), DisplayId(B)]
    );
    flip(&mut v);
    assert!(events(&mut v).contains(&Event::Added(DisplayId(B))));
}

// The legacy calls turn the dark connector's CRTC off.
#[test]
fn a_dark_connector_on_a_legacy_card_has_its_crtc_turned_off() {
    let card = dark_card();
    card.state().atomic = false;
    let v = awake(dark_on(card, "HDMI-A-2"));
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
    let s = v.card().state();
    assert_eq!(s.disabled_crtcs, [101]);
    assert_eq!(s.set_crtcs.iter().map(|c| c.2).collect::<Vec<_>>(), [A]);
    assert_eq!((s.bound.get(&B), s.active.get(&101)), (None, Some(&false)));
}

// ADR-1401 D4 and D9: a display's identity names its connector, its card
// and its monitor, read from the EDID the density reads, and marks the
// connector lit at start; with no policy every display is lit.
#[test]
fn a_displays_identity_names_its_connector_card_and_monitor() {
    let dir = std::env::temp_dir().join(format!("vtium-drm2d-identity-{}", std::process::id()));
    let conn = dir.join("card1-HDMI-A-2");
    std::fs::create_dir_all(&conn).unwrap();
    // maker AAA, product 1, serial string LARGE00001, 60 by 34 cm
    let mut edid = vec![0u8; 128];
    edid[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    edid[8..12].copy_from_slice(&[0x04, 0x21, 0x01, 0x00]);
    (edid[21], edid[22]) = (60, 34);
    edid[54..72].copy_from_slice(b"\0\0\0\xff\0LARGE00001\n  ");
    edid[127] = 0u8.wrapping_sub(edid.iter().fold(0u8, |s, &b| s.wrapping_add(b)));
    std::fs::write(conn.join("edid"), &edid).unwrap();
    let card = dark_card();
    card.state().sysfs = Some(dir.clone());
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    v.set_card_path(Some(String::from("pci-0000:03:00.0")));
    let v = awake(v);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        v.displays().collect::<Vec<_>>(),
        [DisplayId(A), DisplayId(B)]
    );
    assert_eq!(v.identity(DisplayId(A)), Some(&identity("HDMI-A-1", false)));
    let b = v.identity(DisplayId(B)).unwrap();
    assert_eq!((b.connector.as_str(), b.lit_at_start), ("HDMI-A-2", true));
    assert_eq!(b.card.as_deref(), Some("pci-0000:03:00.0"));
    let id = b.edid.as_ref().unwrap();
    assert_eq!(id.to_string(), "AAA:0x0001:LARGE00001");
    // the same read gives the density, the connector reporting no size
    assert_eq!(
        v.density(DisplayId(B)).map(|d| d.dpi().0.round()),
        Some(27.0)
    );
    assert_eq!(v.dark().count(), 0);
}

// With no display, the card keeps master; a dark connector plugged in then
// is set off under it, and master is still kept after.
#[test]
fn a_dark_connector_plugged_in_with_no_display_is_set_off_under_the_master_kept() {
    let card = dark_card();
    card.state().connectors.get_mut(&B).unwrap().connected = false;
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    v.set_dark(Box::new(|_: &Identity| true));
    v.wake_up().unwrap();
    let commits = v.card().state().commits.len();
    assert!(v.card().state().master);
    v.card().state().connectors.get_mut(&B).unwrap().connected = true;
    v.poll().unwrap();
    let s = v.card().state();
    assert_eq!(s.commits.len(), commits + 1);
    let c = s.commits.last().unwrap();
    assert_eq!(
        (c.get(B, "CRTC_ID"), c.get(101, "ACTIVE")),
        (Some(0), Some(0))
    );
    assert_eq!(s.active.get(&101), Some(&false));
    assert!(s.master);
    assert_eq!(s.master_drops, 0, "master never let go");
}

// ADR-1401 D9: lit at start is visible at start. A CRTC another master
// left with a mode and powered down, by ACTIVE on an atomic card or by
// the connector's DPMS on a legacy one, is not lit.
#[test]
fn a_crtc_another_master_left_powered_down_is_not_lit_at_start() {
    let lit = |legacy: bool, on: bool| {
        let card = dark_card();
        {
            let mut s = card.state();
            s.atomic = !legacy;
            if legacy {
                s.dpms.insert(B, if on { 0 } else { 3 });
            } else {
                s.active.insert(101, on);
            }
        }
        let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
        v.identity(DisplayId(B)).unwrap().lit_at_start
    };
    assert!(lit(false, true));
    assert!(!lit(false, false), "atomic, ACTIVE 0");
    assert!(lit(true, true));
    assert!(!lit(true, false), "legacy, DPMS off");
}

/// [`dark_card`] with connector 43, unplugged, still on B's CRTC as
/// another master left it.
fn shared_dark_card() -> FakeCard {
    let card = dark_card();
    {
        let mut s = card.state();
        s.listed.push(43);
        s.dpms.insert(43, 0);
        s.bound.insert(43, 101);
        s.connectors.insert(
            43,
            Connector {
                id: 43,
                kind: 10,
                kind_id: 1,
                connected: false,
                encoder: 201,
                encoders: vec![201],
                ..Connector::default()
            },
        );
    }
    card
}

// review-h1b: turning a dark connector's CRTC off unbinds every connector
// on it, an unplugged one vtium neither binds nor darkens included, so the
// commit passes the kernel's check and the lit display comes up.
#[test]
fn a_dark_crtc_is_turned_off_with_every_connector_on_it() {
    let mut v = dark_on(shared_dark_card(), "HDMI-A-2");
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A)), Event::Awake]);
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        (c.get(B, "CRTC_ID"), c.get(43, "CRTC_ID")),
        (Some(0), Some(0))
    );
    assert_eq!(c.get(101, "ACTIVE"), Some(0));
    assert_eq!(v.card().state().active.get(&101), Some(&false));
}

// review-h1b: a dark rule never costs the lit displays. Where the commit
// setting a dark connector off still fails, the modeset is tried again
// without it, logged once, and the lit displays come up.
#[test]
fn a_dark_connector_that_cannot_be_set_off_leaves_the_lit_displays_up() {
    logs::capture();
    let card = dark_card();
    card.state().refused.push(101);
    let mut v = dark_on(card, "HDMI-A-2");
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A)), Event::Awake]);
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(A, "CRTC_ID"), Some(100));
    assert!(!c.mentions(B) && !c.mentions(101), "{c:?}");
    assert_eq!(v.card().state().active.get(&101), Some(&true));
    let warned = |logs: &[String]| logs.iter().filter(|l| l.contains("HDMI-A-2")).count();
    assert_eq!(warned(&logs::at_least(Level::Warn)), 1);
    // the next modeset that fails the same way logs nothing more
    v.sleep();
    flip(&mut v);
    v.wake_up().unwrap();
    assert_eq!(warned(&logs::at_least(Level::Warn)), 1);
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
}

// ADR-1401 D9: the boot display is the first connected one in the kernel's
// order, so a display plugged back in keeps its place.
#[test]
fn the_displays_keep_the_cards_connector_order_across_a_replug() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    let both = [DisplayId(A), DisplayId(B)];
    assert_eq!(v.displays().collect::<Vec<_>>(), both);
    v.card().state().listed.retain(|&c| c == B);
    v.poll().unwrap();
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(B)]);
    v.card().state().listed = vec![A, B];
    v.poll().unwrap();
    assert_eq!(v.displays().collect::<Vec<_>>(), both);
}

/// The kernel console's mode for a display whose preferred mode is
/// `w`x`h`, the one drm2d picks too.
fn console_mode(w: u16, h: u16) -> Vec<u8> {
    mode_bytes(&mode(w, h, 60, true).0)
}

/// Draws a frame of `byte` on display `id` once its flip is in, swaps it
/// and returns the framebuffer it went on, which holds the frame; the
/// events it raised are dropped.
fn drawn(v: &mut Drm2d<FakeCard>, id: u32, byte: u8) -> u32 {
    flip(v);
    v.canvas(DisplayId(id)).expect("a canvas").pixels.fill(byte);
    let o = v.outputs.iter().find(|o| o.id.0 == id).unwrap();
    let (back, fb) = (o.back, o.back_fb());
    v.swap(DisplayId(id), &[rect(0, 0, 640, 480)]).unwrap();
    flip(v);
    let o = v.outputs.iter().find(|o| o.id.0 == id).unwrap();
    assert_eq!(o.front(), back, "the frame is the front buffer");
    assert!(o.buffers[back].map.iter().all(|&b| b == byte));
    v.events().for_each(drop);
    fb
}

// The small panel on the test seat (vtium/vtium#18's seat, the operator's
// VT pass of 2026-10-30): unplugged, its CRTC went off with vtium's
// framebuffers; plugged back while vtium held no master, the kernel's
// console lit it first, and vtium's frames on the console's mode left it
// dark until a VT switch. While its VT is shown vtium keeps master with no
// display, so the console never takes the panel, and the panel's return
// sets its CRTC from off: its first frame shows.
#[test]
fn a_display_plugged_back_in_shows_its_first_frame() {
    let mut v = awake(one());
    drawn(&mut v, A, 0x11);
    v.card().state().connectors.get_mut(&A).unwrap().connected = false;
    v.card().hotplug(&console_mode(640, 480));
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Removed(DisplayId(A))]);
    {
        let s = v.card().state();
        assert_eq!(s.active.get(&100), Some(&false), "the CRTC went off");
        assert!(s.master, "no display, and still master: the VT is shown");
        assert_eq!(s.master_drops, 0);
    }
    v.card().state().connectors.get_mut(&A).unwrap().connected = true;
    v.card().hotplug(&console_mode(640, 480));
    assert!(
        v.card().state().console_fbs.is_empty(),
        "the console stays off"
    );
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A))]);
    let fb = drawn(&mut v, A, 0x22);
    assert!(v.card().shows(100, fb), "{:?}", v.card().state());
}

// A display that returns gets a full modeset whatever its CRTC shows: one
// plugged in while the VT was away, which the kernel's console lit with
// no process holding master, is set off and set again at the wake-up, so
// its first frame shows rather than stay dark on the console's mode.
#[test]
fn a_display_a_console_lit_while_away_is_set_anew_and_shows() {
    let mut v = a_then_b(ModePolicy::default());
    v.sleep();
    flip(&mut v);
    v.events().for_each(drop);
    v.card().state().listed.push(B);
    v.card().hotplug(&console_mode(800, 600));
    assert_eq!(v.card().state().console_lit, [101]);
    v.wake_up().unwrap();
    let off = v
        .card()
        .real_commits()
        .into_iter()
        .rev()
        .find(|c| c.get(101, "ACTIVE") == Some(0))
        .expect("a commit that sets B's CRTC off");
    assert_eq!(off.get(B, "CRTC_ID"), Some(0));
    let fb = drawn(&mut v, B, 0x33);
    assert!(v.card().shows(101, fb), "{:?}", v.card().state());
}

/// Whether a commit `v`'s card took sets `crtc` off.
fn sets_off(v: &Drm2d<FakeCard>, crtc: u32) -> bool {
    v.card()
        .real_commits()
        .iter()
        .any(|c| c.get(crtc, "ACTIVE") == Some(0))
}

// On a driver that allows a CRTC with no primary plane, i915 among them,
// the unplug leaves the panel's CRTC on, with its mode and the unplugged
// connector still on it. The return sets that CRTC off first, then sets
// the display anew, and its first frame shows.
#[test]
fn a_display_plugged_back_in_on_a_crtc_left_on_shows_its_first_frame() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().remove_fb = RemoveFb::PlaneOff;
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    drawn(&mut v, A, 0x11);
    v.card().state().connectors.get_mut(&A).unwrap().connected = false;
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Removed(DisplayId(A))]);
    {
        let s = v.card().state();
        assert_eq!(s.active.get(&100), Some(&true), "the CRTC stayed on");
        assert_eq!(s.bound.get(&A), Some(&100));
    }
    v.card().state().connectors.get_mut(&A).unwrap().connected = true;
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A))]);
    let off = v
        .card()
        .real_commits()
        .into_iter()
        .find(|c| c.get(100, "ACTIVE") == Some(0))
        .expect("a commit that sets the CRTC off");
    assert_eq!(off.get(A, "CRTC_ID"), Some(0));
    let fb = drawn(&mut v, A, 0x22);
    assert!(v.card().shows(100, fb), "{:?}", v.card().state());
}

/// [`a_then_b`]'s card with B's connector on A's CRTC, 100, as another
/// master left the two cloned there, and the card's calls atomic or not.
fn b_cloned_on_a(atomic: bool) -> Drm2d<FakeCard> {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    {
        let mut s = card.state();
        s.atomic = atomic;
        s.listed.retain(|&c| c == A);
        s.encoders.get_mut(&201).unwrap().possible_crtcs = 0b11;
    }
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    v.card().state().bind(B, 100);
    v
}

// A display that returns on a CRTC a display online drives only leaves
// that CRTC: the display online stays on, and the one returning is set on
// a CRTC of its own.
#[test]
fn a_display_returning_on_a_crtc_another_display_drives_leaves_it_on() {
    let mut v = b_cloned_on_a(true);
    let shown = drawn(&mut v, A, 0x44);
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Added(DisplayId(B))]);
    assert!(!sets_off(&v, 100), "A's CRTC is never set off");
    assert!(v.card().shows(100, shown), "{:?}", v.card().state());
    assert_eq!(v.card().state().bound.get(&B), Some(&101));
}

// The legacy calls keep the same rule: a CRTC a display online drives is
// never turned off for a display that returns, while one only the
// returning display's connector is on is.
#[test]
fn a_legacy_display_returning_turns_off_only_a_crtc_no_display_drives() {
    let mut v = b_cloned_on_a(false);
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert!(
        v.card().state().disabled_crtcs.is_empty(),
        "A's CRTC stays on"
    );
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(640, 480, 60, true)]),
    ]);
    card.state().atomic = false;
    card.state().listed.retain(|&c| c == A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    left_lit(v.card(), B, 1);
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert_eq!(v.card().state().disabled_crtcs, [101]);
}

// The first scan's displays are set without an off commit first: a boot
// display keeps the frame the console left on it until vtium's first
// modeset puts vtium's own there.
#[test]
fn the_first_scans_displays_are_set_without_an_off_commit() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    left_lit(&card, A, 0);
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    let commits = v.card().real_commits();
    assert_eq!(commits.len(), 1, "the modeset alone");
    assert_eq!(commits[0].get(100, "ACTIVE"), Some(1));
}

// A display that returns is set from off once: its modeset clears the mark,
// so the next wake-up sets it as any display online, with no off commit.
#[test]
fn a_returned_display_is_set_from_off_only_once() {
    let mut v = awake(one());
    v.card().state().connectors.get_mut(&A).unwrap().connected = false;
    v.poll().unwrap();
    v.card().state().connectors.get_mut(&A).unwrap().connected = true;
    v.poll().unwrap();
    flip(&mut v);
    assert!(v.outputs.iter().all(|o| !o.returning));
    v.sleep();
    flip(&mut v);
    let before = v.card().real_commits().len();
    v.wake_up().unwrap();
    let after = &v.card().real_commits()[before..];
    assert_eq!(after.len(), 1, "the wake-up's modeset alone: {after:?}");
}
