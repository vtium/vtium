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
use crate::display::fake::{BufferCall, Commit, FakeCard, ListRead, Plane, RemoveFb, logs};
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
        blob.as_chunks::<16>()
            .0
            .iter()
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
    fill(v, A, 1, damage)
}

/// Draws a frame of `byte` on display `id` and swaps it with `damage`.
fn fill(v: &mut Drm2d<FakeCard>, id: u32, byte: u8, damage: &[Rect]) -> Result<Swapped, SwapError> {
    let mut token = v.lend(DisplayId(id)).expect("a token");
    token.draw(|c| c.pixels.fill(byte)).expect("its memory");
    v.swap(token, damage)
}

/// The age of the token display `id` lends now, given back unused.
fn age(v: &mut Drm2d<FakeCard>, id: u32) -> u32 {
    let token = v.lend(DisplayId(id)).expect("a token");
    let age = token.age();
    v.give_back(token);
    age
}

/// Whether display `id` lends a token now, given back unused.
fn lends(v: &mut Drm2d<FakeCard>, id: u32) -> bool {
    v.lend(DisplayId(id)).map(|t| v.give_back(t)).is_some()
}

/// The size of the token display `id` lends now, given back unused.
fn token_size(v: &mut Drm2d<FakeCard>, id: u32) -> (u32, u32) {
    let token = v.lend(DisplayId(id)).expect("a token");
    let size = token.size();
    v.give_back(token);
    size
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
    assert!(!lends(&mut v, A), "the modeset's flip is pending");
    flip(&mut v);
    assert_eq!(
        events(&mut v),
        [
            Event::Added(DisplayId(A)),
            Event::Awake,
            Event::Flipped(DisplayId(A))
        ]
    );
    let mut token = v.lend(DisplayId(A)).unwrap();
    assert_eq!(token.age(), 0);
    token
        .draw(|canvas| {
            assert_eq!(
                (canvas.width, canvas.height, canvas.stride),
                (640, 480, 2560)
            );
            assert!(canvas.pixels.iter().all(|&b| b == 0));
        })
        .unwrap();
    v.swap(token, &[rect(0, 0, 8, 8)]).unwrap();
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
    assert_eq!(age(&mut v, A), 0);
    frame(&mut v, &[DOT]).unwrap();
    flip(&mut v);
    assert_eq!(age(&mut v, A), 2);
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
    assert_eq!(age(&mut v, A), 1);
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
    assert!(!lends(&mut v, A), "the flip is pending");
    assert!(matches!(v.commit(DisplayId(A)), Err(SwapError::Busy)));
}

#[test]
fn a_swap_while_powered_down_is_off() {
    let mut v = awake(one());
    v.set_dpms(DisplayId(A), Dpms::Off).unwrap();
    assert_eq!(v.card().state().dpms[&A], 3);
    assert_eq!(v.dpms(DisplayId(A)), Some(Dpms::Off));
    assert!(matches!(v.commit(DisplayId(A)), Err(SwapError::Off)));
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
    assert_eq!(age(&mut v, A), 0);
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
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Cursor);
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
    v.commit(DisplayId(A)).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(cp, "CRTC_X"), Some(signed(5 - 3 - 1)));
    flip(&mut v);
    v.hide_cursor(DisplayId(A));
    assert!(v.need_commit(DisplayId(A)));
    v.commit(DisplayId(A)).unwrap();
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
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Cursor);
    let c = v.card().last_commit().unwrap();
    let cursor = v.outputs[0].cursor.as_ref().unwrap();
    assert_eq!(c.get(cp, "FB_ID"), Some(u64::from(cursor.fb)));
    assert_eq!(c.get(cp, "CRTC_X"), Some(40 - 3));
    assert_eq!(c.get(cp, "CRTC_Y"), Some(30 - 1));
    assert_eq!(c.get(cp, "HOTSPOT_X"), Some(3));
    assert_eq!(c.get(cp, "HOTSPOT_Y"), Some(1));
    let bytes = v.card().memory(cursor.dumb.handle).unwrap();
    assert_eq!(bytes.len(), 64 * 64 * 4);
    for (&px, i) in bytes.as_chunks::<4>().0.iter().zip(0u32..) {
        let (x, y) = (i % 64, i / 64);
        let want = if y < 2 && x < 60 {
            0xff00_0000 | (y * 60 + x)
        } else {
            0
        };
        let got = u32::from_ne_bytes(px);
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
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Nothing);
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
    v.commit(DisplayId(A)).unwrap();
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
    assert!(v.commit(DisplayId(A)).is_err());
    assert!(v.need_commit(DisplayId(A)));
    vblank(&mut v);
    v.commit(DisplayId(A)).unwrap();
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
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Cursor);
    flip(&mut v);
    let cp = cursor_plane(&v, A);
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    assert!(!v.card().state().plane_crtc.contains_key(&cp));
    assert!(!v.need_commit(DisplayId(A)));
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Nothing);
    v.show_cursor(DisplayId(A), 2, 2).unwrap();
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Cursor);
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
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Nothing);
    assert_eq!(v.card().state().commits.len(), commits);
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Cursor);
    flip(&mut v);
    let commits = v.card().state().commits.len();
    v.show_cursor(DisplayId(A), 2, 2).unwrap();
    v.set_cursor_offset(DisplayId(A), 1, 1);
    v.set_cursor_offset(DisplayId(A), 0, 0);
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Nothing);
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
    assert!(matches!(v.commit(DisplayId(A)), Err(SwapError::Busy)));
    flip(&mut v);
    assert_eq!(v.waits(), (false, false));
    let c = v.card().last_commit().unwrap();
    assert_ne!(c.flags & DRM_MODE_ATOMIC_ALLOW_MODESET, 0);
    assert!(c.mentions(A) && c.mentions(B));
    // A keeps what it shows, so it needs no redraw.
    assert_eq!(events(&mut v), [Event::Added(DisplayId(B))]);
}

/// Whether each of `ids` is ready for a frame, after checking that the
/// display lends a token exactly when it says it is.
fn ready(v: &mut Drm2d<FakeCard>, ids: &[u32]) -> Vec<bool> {
    ids.iter()
        .map(|&id| {
            let ready = v.ready_for_frame(DisplayId(id));
            // A token lent here draws nothing, and goes back unused.
            assert_eq!(lends(v, id), ready, "display {id}");
            ready
        })
        .collect()
}

// vtium/vtium#174: a display says whether it takes a frame, and lends a
// token exactly then. A display kept through a modeset that waits for
// another display's flip takes none until that flip and the modeset's own
// are done; a display powered down, one waiting for a vblank, and every
// display of a card asleep take none either.
#[test]
fn a_display_lends_a_canvas_exactly_while_it_is_ready_for_a_frame() {
    const C: u32 = 43;
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
        (C, vec![mode(1024, 768, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c != C);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(ready(&mut v, &[A, B, C]), [true, true, false]);
    frame(&mut v, &[DOT]).unwrap();
    assert_eq!(ready(&mut v, &[A, B]), [false, true], "A's flip in flight");
    v.card().state().listed.push(C);
    v.poll().unwrap();
    assert_eq!(v.waits(), (true, false));
    assert_eq!(
        ready(&mut v, &[A, B]),
        [false, false],
        "the modeset waits for A's flip"
    );
    flip(&mut v);
    assert_eq!(v.waits(), (false, false));
    assert_eq!(
        ready(&mut v, &[A, B, C]),
        [false, false, false],
        "the modeset's flip"
    );
    flip(&mut v);
    assert_eq!(ready(&mut v, &[A, B, C]), [true, true, true]);
    v.set_dpms(DisplayId(B), Dpms::Off).unwrap();
    assert_eq!(ready(&mut v, &[A, B]), [true, false], "B powered down");
    v.set_dpms(DisplayId(B), Dpms::On).unwrap();
    assert_eq!(ready(&mut v, &[B]), [true]);
    v.card().state().commit_errors.push_back(Errno::BUSY);
    assert!(matches!(frame(&mut v, &[DOT]), Err(SwapError::Busy)));
    assert_eq!(
        ready(&mut v, &[A, B]),
        [false, true],
        "A waits for a vblank"
    );
    vblank(&mut v);
    assert_eq!(ready(&mut v, &[A]), [true]);
    v.sleep();
    assert_eq!(events(&mut v).last(), Some(&Event::Asleep));
    assert_eq!(ready(&mut v, &[A, B, C]), [false, false, false], "asleep");
    assert!(!v.ready_for_frame(DisplayId(99)), "no such display");
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
    assert!(!lends(&mut v, A));
    assert!(matches!(v.commit(DisplayId(A)), Err(SwapError::Off)));
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
    fill(&mut v, A, 0xee, &[rect(0, 0, 640, 480)]).unwrap();
    flip(&mut v);
    v.sleep();
    let o = &mut v.outputs[0];
    let front = o.front();
    assert!(lock(&o.buffers[front].map).iter().all(|&b| b == 0));
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
    assert_eq!(token_size(&mut v, A).0, 640);
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
    assert_eq!(token_size(&mut v, A).1, 240);
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

/// The calls on buffers since the `from`th, as `v`'s card logged them.
fn buffer_calls(v: &Drm2d<FakeCard>, from: usize) -> Vec<BufferCall> {
    v.card().state().buffer_calls[from..].to_vec()
}

// A framebuffer the card refuses: the dumb buffer made for it is
// destroyed, and the display holds nothing.
#[test]
fn a_framebuffer_refused_destroys_its_dumb_buffer() {
    let mut v = one();
    v.card().state().fb_errors.push_back(Errno::INVAL);
    let _ = v.wake_up();
    let calls = buffer_calls(&v, 0);
    let h = match calls.first() {
        Some(&BufferCall::CreateDumb(h)) => h,
        _ => 0,
    };
    assert_eq!(
        calls[..3],
        [
            BufferCall::CreateDumb(h),
            BufferCall::AddFb(h),
            BufferCall::DestroyDumb(h)
        ],
        "{calls:?}"
    );
    let s = v.card().state();
    assert!(s.dumbs.is_empty() && s.fbs.is_empty(), "{calls:?}");
}

// A dumb buffer the card cannot map: its framebuffer is removed before the
// buffer is destroyed, and the display holds nothing.
#[test]
fn a_dumb_buffer_that_cannot_be_mapped_frees_its_framebuffer_first() {
    let mut v = one();
    v.card().state().map_errors.push_back(Errno::NOMEM);
    let _ = v.wake_up();
    let calls = buffer_calls(&v, 0);
    let h = match calls.first() {
        Some(&BufferCall::CreateDumb(h)) => h,
        _ => 0,
    };
    let fb = match calls.get(3) {
        Some(&BufferCall::RmFb(fb)) => fb,
        _ => 0,
    };
    assert_eq!(
        calls[..5],
        [
            BufferCall::CreateDumb(h),
            BufferCall::AddFb(h),
            BufferCall::MapDumb(h),
            BufferCall::RmFb(fb),
            BufferCall::DestroyDumb(h)
        ],
        "{calls:?}"
    );
    let s = v.card().state();
    assert!(s.dumbs.is_empty() && s.fbs.is_empty(), "{calls:?}");
}

// A display's buffers freed: each one's framebuffer is removed before
// its dumb buffer is destroyed.
#[test]
fn freed_buffers_lose_their_framebuffers_first() {
    let mut v = awake(one());
    let o = v.outputs.iter().find(|o| o.id.0 == A).unwrap();
    let want: Vec<BufferCall> = o
        .buffers
        .iter()
        .flat_map(|b| [BufferCall::RmFb(b.fb), BufferCall::DestroyDumb(b.handle)])
        .collect();
    let from = v.card().state().buffer_calls.len();
    v.card().state().connectors.remove(&A);
    v.poll().unwrap();
    assert_eq!(buffer_calls(&v, from), want);
    let s = v.card().state();
    assert!(s.dumbs.is_empty() && s.fbs.is_empty());
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
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Nothing);
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
    let before = age(&mut v, A);
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    assert!(v.hardware_cursor(DisplayId(A)));
    v.show_cursor(DisplayId(A), 3, 4).unwrap();
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Cursor);
    let c = v.card().last_commit().unwrap();
    let plane = primary(&v, A);
    assert_eq!(c.get(cursor_plane(&v, A), "CRTC_X"), Some(3));
    assert!(!c.mentions(plane), "the primary plane is left as it is");
    assert_eq!(c.get(100, "MODE_ID"), None);
    flip(&mut v);
    assert_eq!(age(&mut v, A), before);
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
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Frame);
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
        assert!(lends(&mut app.0, A));
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
    assert_eq!(app.0.commit(DisplayId(A)).unwrap(), Swapped::Frame);
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
    assert!(matches!(app.0.commit(DisplayId(A)), Err(SwapError::Busy)));
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
    assert_eq!(age(&mut app.0, A), 1);
    assert_eq!(app.0.commit(DisplayId(A)).unwrap(), Swapped::Frame);
    flip(&mut app.0);
    assert!(app.0.card().shows(100, fb), "{:?}", app.0.card().state());
    let o = app.0.outputs.iter().find(|o| o.id.0 == A).unwrap();
    assert_eq!(o.front(), back);
    assert_eq!(lock(&o.buffers[back].map)[0], 1, "the frame drawn is shown");
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
        assert!(!lends(&mut app.0, A));
        assert!(matches!(app.0.commit(DisplayId(A)), Err(SwapError::Busy)));
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
    assert_eq!(app.0.commit(DisplayId(A)).unwrap(), Swapped::Frame);
    // The vblank's event, arriving late, ends nothing: a flip is in flight.
    app.0.card().state().vblanks.push(100);
    app.0.card().vblank();
    app.0.dispatch().unwrap();
    assert!(app.0.is_flipping(DisplayId(A)));
}

// A card with no vblank interrupt and no software vblank timer, as simpledrm,
// udl or gud, or virtio-gpu before Linux 6.19, refuses
// the wait for a vblank with EOPNOTSUPP while its commits can still be
// refused as busy. The display then has nothing to wait for: it lends its
// token again, and the frame goes with the next swap the owner tries,
// which nothing asks for until the next draw (vtium/vtium#176).
#[test]
fn a_frame_refused_on_a_card_without_vblank_events_goes_with_the_next_swap() {
    let mut v = awake(one());
    v.card().state().commit_errors.push_back(Errno::BUSY);
    v.card().state().no_vblank = true;
    logs::capture();
    assert!(matches!(frame(&mut v, &[DOT]), Err(SwapError::Busy)));
    assert!(logs::at_least(Level::Warn).is_empty());
    assert!(!v.is_flipping(DisplayId(A)));
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Frame);
}

// The stand-in ends vblank waits as the kernel does: a wait on a CRTC that
// is not active is refused (`drm_queue_vblank_event`), one on a card with no
// vblank interrupt too (`drm_wait_vblank_ioctl`), and a CRTC that goes off
// sends the events still waiting at once (`drm_crtc_vblank_off`).
#[test]
fn the_stand_in_ends_vblank_waits_as_the_kernel_does() {
    let mut v = awake(one());
    v.card().wait_vblank(0).unwrap();
    v.set_dpms(DisplayId(A), Dpms::Off).unwrap();
    let mut read = Vec::new();
    v.card().read_events(&mut read).unwrap();
    assert_eq!(read, [CrtcEvent::Vblank(100)]);
    let e = v.card().wait_vblank(0).unwrap_err();
    assert_eq!(e.raw_os_error(), Some(Errno::INVAL.raw_os_error()));
    v.set_dpms(DisplayId(A), Dpms::On).unwrap();
    v.card().state().no_vblank = true;
    let e = v.card().wait_vblank(0).unwrap_err();
    assert_eq!(e.raw_os_error(), Some(Errno::OPNOTSUPP.raw_os_error()));
}

// A display powered down while a refused frame waits for its vblank ends
// the wait at once, as the kernel sends the vblank's event when the CRTC
// goes off: no bound runs out, and nothing is warned.
#[test]
fn a_display_powered_down_during_a_vblank_wait_ends_it_at_once() {
    let (mut lp, mut app) = in_loop(awake(one()));
    logs::capture();
    app.0.card().state().commit_errors.push_back(Errno::BUSY);
    assert!(matches!(frame(&mut app.0, &[DOT]), Err(SwapError::Busy)));
    app.0.set_dpms(DisplayId(A), Dpms::Off).unwrap();
    let took = turn_until(&mut lp, &mut app, |app| !app.0.is_flipping(DisplayId(A)));
    assert!(took.as_millis() < 500, "the wait ended after {took:?}");
    assert!(logs::at_least(Level::Warn).is_empty());
    assert!(events(&mut app.0).is_empty(), "no frame flipped");
    assert!(matches!(app.0.commit(DisplayId(A)), Err(SwapError::Off)));
}

// A vblank wait that spans a VT leave ends before the sleep does, as a flip
// in flight does. Its event, arriving during the wake-up's modeset flip,
// must not end that flip early nor lend a token before it.
#[test]
fn a_vblank_event_left_from_before_a_sleep_ends_no_later_flip() {
    let mut v = awake(one());
    v.card().state().commit_errors.push_back(Errno::BUSY);
    assert!(matches!(frame(&mut v, &[DOT]), Err(SwapError::Busy)));
    v.sleep();
    assert!(events(&mut v).is_empty(), "the sleep waits for the vblank");
    // The bound runs out on the wait, so the sleep goes ahead.
    v.flip_expired(100);
    assert_eq!(events(&mut v), [Event::Asleep]);
    v.wake_up().unwrap();
    assert!(v.is_flipping(DisplayId(A)), "the modeset's flip");
    vblank(&mut v);
    assert!(v.is_flipping(DisplayId(A)), "the old vblank ends no flip");
    assert!(!lends(&mut v, A));
    flip(&mut v);
    assert!(lends(&mut v, A));
}

// The warning for a lost event goes once for a run of lost events, and
// the next event the card delivers ends the run: a vblank's event lost,
// then a flip's event that arrives, then a flip's event lost, warn twice.
#[test]
fn each_run_of_lost_events_warns_once() {
    let (mut lp, mut app) = in_loop(awake(one()));
    logs::capture();
    app.0.card().state().commit_errors.push_back(Errno::BUSY);
    assert!(matches!(frame(&mut app.0, &[DOT]), Err(SwapError::Busy)));
    turn_until(&mut lp, &mut app, |app| !app.0.is_flipping(DisplayId(A)));
    assert_eq!(logs::at_least(Level::Warn).len(), 1, "the lost vblank");
    assert_eq!(app.0.commit(DisplayId(A)).unwrap(), Swapped::Frame);
    app.0.card().complete_flips();
    turn_until(&mut lp, &mut app, |app| !app.0.is_flipping(DisplayId(A)));
    app.0.card().state().lose_flips = true;
    frame(&mut app.0, &[DOT]).unwrap();
    app.0.card().complete_flips();
    turn_until(&mut lp, &mut app, |app| !app.0.is_flipping(DisplayId(A)));
    assert_eq!(logs::at_least(Level::Warn).len(), 2, "the lost flip too");
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
        fill(&mut v, A, 0xee, &[rect(0, 0, 640, 480)]).unwrap();
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
    assert!(lock(&o.buffers[o.front()].map).iter().all(|&b| b == 0xee));
    assert_eq!(events(&mut v), [Event::Added(DisplayId(B))]);
    flip(&mut v);
    assert_eq!(age(&mut v, A), 2);
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
    assert!(lends(&mut v, A));
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
// no token, so no frame is drawn and lost; powered on, every buffer is
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
    assert!(!lends(&mut v, A));
    v.set_dpms(DisplayId(A), Dpms::On).unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A))]);
    assert_eq!(age(&mut v, A), 0);
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
    v.commit(DisplayId(A)).unwrap();
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
    assert_eq!(age(&mut v, A), 0);
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
    assert_eq!(age(&mut v, A), 0);
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
        connector_path: None,
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

// ADR-1401 D4: a DisplayPort multi-stream hub's port carries its connector's
// `PATH` in its identity, the text before the blob's NUL, read through the
// card as any other connector property is; a connector with no `PATH`
// carries none.
#[test]
fn a_hub_ports_identity_carries_its_connectors_path() {
    let card = dark_card();
    card.set_path(B, b"mst:213-1\0");
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    let b = v.identity(DisplayId(B)).unwrap();
    assert_eq!(b.connector_path.as_deref(), Some("mst:213-1"));
    assert_eq!(b.connector, "HDMI-A-2");
    assert_eq!(v.identity(DisplayId(A)).unwrap().connector_path, None);
}

// A `PATH` naming no blob (0) leaves the display bound and without a path,
// with nothing logged; one naming a blob the card no longer holds does the
// same with one warning naming its connector; an empty one gives no path.
#[test]
fn a_path_the_card_cannot_give_leaves_the_display_bound_without_one() {
    let card = dark_card();
    card.state().paths.insert(A, 0);
    logs::capture();
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(logs::at_least(Level::Warn), [] as [String; 0]);
    assert_eq!(
        v.displays().collect::<Vec<_>>(),
        [DisplayId(A), DisplayId(B)]
    );
    assert_eq!(v.identity(DisplayId(A)).unwrap().connector_path, None);

    let card = dark_card();
    card.state().paths.insert(B, 999_999);
    logs::capture();
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(
        logs::at_least(Level::Warn),
        ["cannot read the PATH of connector HDMI-A-2: No such file or directory (os error 2)"]
    );
    assert_eq!(
        v.displays().collect::<Vec<_>>(),
        [DisplayId(A), DisplayId(B)]
    );
    assert_eq!(v.identity(DisplayId(B)).unwrap().connector_path, None);

    let card = dark_card();
    card.set_path(B, b"\0");
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(v.identity(DisplayId(B)).unwrap().connector_path, None);
}

// A `PATH` blob freed between its two reads and its id given to a longer
// blob is refused, never read past the buffer its first length sized: the
// display is bound without a path and one warning names its connector.
#[test]
fn a_path_blob_that_changes_length_between_its_reads_gives_no_path() {
    let card = dark_card();
    card.set_path(B, b"mst:213-1\0");
    let blob = *card.state().paths.get(&B).unwrap();
    card.state()
        .replaced
        .insert(blob, b"mst:213-2-1-1-1\0".to_vec());
    logs::capture();
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(
        logs::at_least(Level::Warn),
        [
            "cannot read the PATH of connector HDMI-A-2: the blob changed from 10 to 16 bytes between two reads"
        ]
    );
    assert_eq!(
        v.displays().collect::<Vec<_>>(),
        [DisplayId(A), DisplayId(B)]
    );
    assert_eq!(v.identity(DisplayId(B)).unwrap().connector_path, None);
}

// A `PATH` blob of `BLOB_MAX` bytes is read, and a longer one is refused
// unread with one warning naming its connector.
#[test]
fn a_path_blob_past_blob_max_is_refused_unread() {
    let card = dark_card();
    card.set_path(A, &vec![b'1'; crate::bounds::BLOB_MAX]);
    card.set_path(B, &vec![b'1'; crate::bounds::BLOB_MAX + 1]);
    logs::capture();
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(
        logs::at_least(Level::Warn),
        [
            "cannot read the PATH of connector HDMI-A-2: the blob is 4097 bytes, past the 4096 vtium reads"
        ]
    );
    let a = v.identity(DisplayId(A)).unwrap();
    assert_eq!(a.connector_path.as_deref().map(str::len), Some(4096));
    assert_eq!(v.identity(DisplayId(B)).unwrap().connector_path, None);
}

// vtium/vtium#192: a connector is read in two, its counts and then its
// lists into buffers of those counts. A connector that changes between the
// two reads, as one torn down and its id given to another, is refused,
// whichever list changed and whichever way, and the next read gives it
// whole.
#[test]
fn a_connector_whose_lists_change_between_its_reads_is_refused() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.after_read(ListRead::Connector(A), |s| {
        let c = s.connectors.get_mut(&A).unwrap();
        c.modes.push(mode(320, 240, 60, false));
    });
    let e = card.connector(A).unwrap_err();
    assert_eq!(
        e.to_string(),
        "its modes changed from 1 to 2 between two reads"
    );
    let read = card.connector(A).unwrap();
    assert_eq!(
        read.modes,
        [mode(640, 480, 60, true), mode(320, 240, 60, false)]
    );

    card.after_read(ListRead::Connector(A), |s| {
        s.connectors.get_mut(&A).unwrap().modes.pop();
    });
    let e = card.connector(A).unwrap_err();
    assert_eq!(
        e.to_string(),
        "its modes changed from 2 to 1 between two reads"
    );

    card.after_read(ListRead::Connector(A), |s| {
        s.connectors.get_mut(&A).unwrap().encoders.push(299);
    });
    let e = card.connector(A).unwrap_err();
    assert_eq!(
        e.to_string(),
        "its encoders changed from 1 to 2 between two reads"
    );
    assert_eq!(card.connector(A).unwrap().encoders, [200, 299]);
}

// The first read of a connector probes it, as one with no room for modes
// does; the second gives room for one mode where the first found none, so
// it does not probe again and find modes the first did not count.
#[test]
fn a_connector_is_probed_once_a_read() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)]), (B, Vec::new())]);
    assert_eq!(card.connector(A).unwrap().modes.len(), 1);
    assert_eq!(card.connector(B).unwrap().modes.len(), 0);
    assert_eq!(card.state().probes, [A, B]);
    // Without DRM master the kernel probes nothing.
    card.state().master = false;
    assert_eq!(card.connector(A).unwrap().modes.len(), 1);
    assert_eq!(card.state().probes, [A, B]);
}

// The card writes a connector's modes and encoders only into a buffer that
// holds the whole list, and gives the full counts either way.
#[test]
fn a_connector_read_into_short_buffers_writes_neither_list() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true), mode(320, 240, 60, false)])]);
    card.state().connectors.get_mut(&A).unwrap().encoders = vec![200, 201];
    let mut modes = [Mode::default()];
    let mut encoders = [7u32];
    let read = card.read_connector(A, &mut modes, &mut encoders).unwrap();
    assert_eq!((read.modes, read.encoders), (2, 2));
    assert_eq!(modes, [Mode::default()]);
    assert_eq!(encoders, [7]);
}

// The CRTC a dark connector is on is read without probing the connector:
// only its encoder is wanted.
#[test]
fn a_dark_connectors_crtc_is_read_without_a_probe() {
    let v = with_dark("HDMI-A-2");
    v.card().state().probes.clear();
    assert_eq!(v.connector_crtc(B), Some(101));
    assert_eq!(v.connector_crtc(A), None);
    assert_eq!(v.card().state().probes, [] as [u32; 0]);
}

// A connector with more modes than `MODES_MAX`, or more encoders than
// `ENCODERS_MAX`, is refused unread; one with as many is read.
#[test]
fn a_connector_past_its_bounds_is_refused_unread() {
    use crate::bounds::{ENCODERS_MAX, MODES_MAX};
    let modes = |n: usize| -> Vec<Mode> {
        (0..n)
            .map(|i| mode(640 + u16::try_from(i).unwrap(), 480, 60, i == 0))
            .collect()
    };
    let card = FakeCard::new(&[(A, modes(MODES_MAX)), (B, modes(MODES_MAX + 1))]);
    assert_eq!(card.connector(A).unwrap().modes.len(), MODES_MAX);
    assert_eq!(
        card.connector(B).unwrap_err().to_string(),
        "it has 1025 modes, past the 1024 vtium reads"
    );
    card.state().connectors.get_mut(&A).unwrap().encoders = (0..33).collect();
    assert_eq!(
        card.connector(A).unwrap_err().to_string(),
        "it has 33 encoders, past the 32 vtium reads"
    );
    card.state().connectors.get_mut(&A).unwrap().encoders = (0..32).collect();
    assert_eq!(card.connector(A).unwrap().encoders.len(), ENCODERS_MAX);
}

// The scan leaves a connector that changed between its reads unbound, with
// a warning naming it at that scan, and the next scan reads it again and
// binds it.
#[test]
fn a_connector_refused_by_the_scan_is_bound_at_the_next_scan() {
    let card = dark_card();
    card.after_read(ListRead::Connector(B), |s| {
        let c = s.connectors.get_mut(&B).unwrap();
        c.modes.push(mode(320, 240, 60, false));
    });
    logs::capture();
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(
        logs::at_least(Level::Warn),
        ["cannot read connector 42: its modes changed from 1 to 2 between two reads"]
    );
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
    logs::capture();
    v.poll().unwrap();
    flip(&mut v);
    assert_eq!(logs::at_least(Level::Warn), [] as [String; 0]);
    assert_eq!(
        v.displays().collect::<Vec<_>>(),
        [DisplayId(A), DisplayId(B)]
    );
}

// An object's properties are read in two, their count and then their ids
// and values into buffers of that count. A count that changed between the
// two reads is refused, and so is one past `OBJECT_PROPERTIES_MAX`.
#[test]
fn properties_that_change_between_their_reads_are_refused() {
    use crate::bounds::OBJECT_PROPERTIES_MAX;
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    let plane = 300;
    let own = card.properties(plane, Object::Plane).unwrap().len();
    card.after_read(ListRead::Properties(plane), |s| {
        s.planes.get_mut(&300).unwrap().damage = false;
    });
    let e = card.properties(plane, Object::Plane).unwrap_err();
    assert_eq!(
        e.to_string(),
        format!(
            "its properties changed from {own} to {} between two reads",
            own - 1
        )
    );
    let names: Vec<String> = card
        .properties(plane, Object::Plane)
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert!(!names.iter().any(|n| n == "FB_DAMAGE_CLIPS"), "{names:?}");

    card.state()
        .extra_props
        .insert(plane, OBJECT_PROPERTIES_MAX - (own - 1));
    assert_eq!(
        card.properties(plane, Object::Plane).unwrap().len(),
        OBJECT_PROPERTIES_MAX
    );
    card.state()
        .extra_props
        .insert(plane, OBJECT_PROPERTIES_MAX - (own - 1) + 1);
    assert_eq!(
        card.properties(plane, Object::Plane)
            .unwrap_err()
            .to_string(),
        "it has 65 properties, past the 64 vtium reads"
    );
}

// The scan leaves a connector whose properties it cannot read unbound, with
// a warning naming it at that scan, and binds it at the next scan, its `PATH` read
// with the rest.
#[test]
fn a_connector_whose_properties_are_refused_is_bound_at_the_next_scan() {
    let card = dark_card();
    card.set_path(B, b"mst:213-1\0");
    let blob = *card.state().paths.get(&B).unwrap();
    card.state().paths.remove(&B);
    card.after_read(ListRead::Properties(B), move |s| {
        s.paths.insert(B, blob);
    });
    logs::capture();
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(
        logs::at_least(Level::Warn),
        [
            "cannot read the properties of connector HDMI-A-2: its properties changed from 2 to 3 between two reads"
        ]
    );
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
    logs::capture();
    v.poll().unwrap();
    flip(&mut v);
    assert_eq!(logs::at_least(Level::Warn), [] as [String; 0]);
    assert_eq!(
        v.displays().collect::<Vec<_>>(),
        [DisplayId(A), DisplayId(B)]
    );
    assert_eq!(
        v.identity(DisplayId(B)).unwrap().connector_path.as_deref(),
        Some("mst:213-1")
    );
}

// The card's planes are read in two, their count and then their ids into a
// buffer of that count. A count that changed between the reads is refused,
// and so is one past `PLANES_MAX`.
#[test]
fn planes_that_change_between_their_reads_are_refused() {
    use crate::bounds::PLANES_MAX;
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    let overlay = || Plane {
        crtcs: 1,
        kind: PLANE_OVERLAY,
        damage: false,
    };
    card.after_read(ListRead::Planes, move |s| {
        s.planes.insert(398, overlay());
    });
    let e = card.planes().unwrap_err();
    assert_eq!(
        e.to_string(),
        "its planes changed from 3 to 4 between two reads"
    );
    assert_eq!(card.planes().unwrap(), [300, 301, 398, 399]);
    for id in 0..u32::try_from(PLANES_MAX - 4).unwrap() {
        card.state().planes.insert(500 + id, overlay());
    }
    assert_eq!(card.planes().unwrap().len(), PLANES_MAX);
    card.state().planes.insert(499, overlay());
    assert_eq!(
        card.planes().unwrap_err().to_string(),
        "it has 33 planes, past the 32 vtium reads"
    );
}

// A display whose planes the scan cannot list finds no primary plane: the
// refusal is a warning, and the next scan reads the planes again and binds
// the display.
#[test]
fn a_display_whose_planes_are_refused_is_bound_at_the_next_scan() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.after_read(ListRead::Planes, |s| {
        s.planes.remove(&399);
    });
    logs::capture();
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    assert_eq!(
        logs::at_least(Level::Warn),
        [
            "cannot list the card's planes: its planes changed from 3 to 2 between two reads",
            "no primary plane for HDMI-A-1",
        ]
    );
    assert!(v.outputs.is_empty());
    v.poll().unwrap();
    flip(&mut v);
    assert_eq!(v.displays().collect::<Vec<_>>(), [DisplayId(A)]);
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
    let o = v.outputs.iter().find(|o| o.id.0 == id).unwrap();
    let (back, fb) = (o.back, o.back_fb());
    fill(v, id, byte, &[rect(0, 0, 640, 480)]).unwrap();
    flip(v);
    let o = v.outputs.iter().find(|o| o.id.0 == id).unwrap();
    assert_eq!(o.front(), back, "the frame is the front buffer");
    assert!(lock(&o.buffers[back].map).iter().all(|&b| b == byte));
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

/// The debug lines since [`logs::capture`] that say a display's modeset
/// is on screen. `tools/vt/run-vt.sh` copies the card's state at each.
fn modesets_shown() -> Vec<String> {
    logs::at_least(Level::Debug)
        .into_iter()
        .filter(|l| l.ends_with(": the first flip after its modeset is done"))
        .collect()
}

const A_SHOWN: &str = "display HDMI-A-1: the first flip after its modeset is done";
const B_SHOWN: &str = "display HDMI-A-2: the first flip after its modeset is done";

// The stall the VT debug capture looks into: a display plugged back in,
// then its VT switched away and back. Each modeset logs its line once,
// when its flip's event arrives, and the frames after it log none.
#[test]
fn each_modeset_of_a_display_says_once_that_it_is_shown() {
    let mut v = awake(one());
    drawn(&mut v, A, 0x11);
    v.card().state().connectors.get_mut(&A).unwrap().connected = false;
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Removed(DisplayId(A))]);
    logs::capture();
    v.card().state().connectors.get_mut(&A).unwrap().connected = true;
    v.poll().unwrap();
    assert!(
        modesets_shown().is_empty(),
        "the modeset's flip is in flight"
    );
    drawn(&mut v, A, 0x22);
    drawn(&mut v, A, 0x33);
    assert_eq!(modesets_shown(), [A_SHOWN], "the replug's modeset");
    v.sleep();
    assert_eq!(events(&mut v).last(), Some(&Event::Asleep));
    logs::capture();
    v.wake_up().unwrap();
    assert!(
        modesets_shown().is_empty(),
        "the modeset's flip is in flight"
    );
    drawn(&mut v, A, 0x44);
    drawn(&mut v, A, 0x55);
    assert_eq!(modesets_shown(), [A_SHOWN], "the wake-up's modeset");
}

// A display plugged in sets every display again, and each says so once.
#[test]
fn a_hotplug_modeset_says_each_display_is_shown() {
    let mut v = a_then_b(ModePolicy::default());
    logs::capture();
    v.card().state().listed.push(B);
    v.poll().unwrap();
    flip(&mut v);
    assert_eq!(modesets_shown(), [A_SHOWN, B_SHOWN]);
}

// A legacy modeset sends no flip event, so the first frame's flip after
// it says the mode is shown.
#[test]
fn a_legacy_modeset_says_so_at_the_first_frames_flip() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().atomic = false;
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    logs::capture();
    v.wake_up().unwrap();
    assert!(modesets_shown().is_empty());
    drawn(&mut v, A, 0x11);
    drawn(&mut v, A, 0x22);
    assert_eq!(modesets_shown(), [A_SHOWN]);
}

// A modeset's flip that the bound takes as done may not be on screen, so
// it logs no line, and the event that comes late logs none either; the
// next modeset logs its own.
#[test]
fn a_modeset_flip_taken_as_done_does_not_say_it_is_shown() {
    let mut v = one();
    logs::capture();
    v.wake_up().unwrap();
    v.flip_expired(100);
    assert!(modesets_shown().is_empty(), "the flip's event was lost");
    flip(&mut v);
    drawn(&mut v, A, 0x11);
    assert!(modesets_shown().is_empty(), "the late event");
    v.sleep();
    v.wake_up().unwrap();
    flip(&mut v);
    assert_eq!(modesets_shown(), [A_SHOWN]);
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

// ADR-1401 D2, the buffer token.

/// A card with one display at 640x480 keeping `buffers` buffers, awake.
fn kept(buffers: Buffers) -> Drm2d<FakeCard> {
    let mut v = one();
    v.buffers = buffers;
    awake(v)
}

/// The age of each token lent, frame by frame, each frame drawn, swapped
/// and flipped.
fn ages(v: &mut Drm2d<FakeCard>, frames: usize) -> Vec<u32> {
    (0..frames)
        .map(|_| {
            let mut token = v.lend(DisplayId(A)).expect("a token");
            let age = token.age();
            token.draw(|c| c.pixels[0] = 1).unwrap();
            v.swap(token, &[DOT]).unwrap();
            flip(v);
            age
        })
        .collect()
}

// A display keeps as many buffers as it is given, and each token's age
// counts the display's own frames since its buffer was drawn: with one
// buffer always the frame before, with two the frame before that, with
// three the frame before that again; 0 until each buffer holds a frame.
#[test]
fn a_tokens_age_counts_the_frames_since_its_buffer_was_drawn() {
    for (buffers, want) in [
        (Buffers::One, vec![0, 1, 1, 1, 1]),
        (Buffers::Two, vec![0, 0, 2, 2, 2]),
        (Buffers::Three, vec![0, 0, 0, 3, 3]),
    ] {
        let mut v = kept(buffers);
        assert_eq!(v.outputs[0].buffers.len(), buffers.count());
        assert_eq!(ages(&mut v, 5), want, "{buffers:?}");
    }
}

// The buffers take turns: each frame swapped goes to the next buffer, and
// a modeset puts the first on screen.
#[test]
fn three_buffers_take_turns() {
    let mut v = kept(Buffers::Three);
    let fbs: Vec<u32> = v.outputs[0].buffers.iter().map(|b| b.fb).collect();
    let mut shown = Vec::new();
    for _ in 0..4 {
        frame(&mut v, &[DOT]).unwrap();
        let c = v.card().last_commit().unwrap();
        shown.push(c.get(primary(&v, A), "FB_ID").unwrap());
        flip(&mut v);
    }
    let want: Vec<u64> = [1, 2, 0, 1].iter().map(|&i| u64::from(fbs[i])).collect();
    assert_eq!(shown, want);
}

// Two displays of one card count their own frames, so their ages differ
// when one draws more often (the C reference kept a text layer, and its
// ages, for each display).
#[test]
fn two_displays_of_one_card_keep_their_ages_apart() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    for _ in 0..3 {
        fill(&mut v, A, 1, &[DOT]).unwrap();
        flip(&mut v);
    }
    fill(&mut v, B, 1, &[DOT]).unwrap();
    flip(&mut v);
    assert_eq!((age(&mut v, A), age(&mut v, B)), (2, 0));
    fill(&mut v, B, 1, &[DOT]).unwrap();
    flip(&mut v);
    assert_eq!((age(&mut v, A), age(&mut v, B)), (2, 2));
}

// While its token is out a display lends no other and is not ready for a
// frame, so a display has one frame on its way at a time.
#[test]
fn a_display_lends_no_second_token_while_one_is_out() {
    for buffers in [Buffers::One, Buffers::Two, Buffers::Three] {
        let mut v = kept(buffers);
        let token = v.lend(DisplayId(A)).unwrap();
        assert!(v.lend(DisplayId(A)).is_none(), "{buffers:?}");
        assert!(!v.ready_for_frame(DisplayId(A)));
        v.give_back(token);
        assert!(v.ready_for_frame(DisplayId(A)));
        assert!(lends(&mut v, A));
    }
}

// With two buffers the frame after a swap waits for its flip; with three
// the third buffer is free while the second flips, and with one vtium draws
// into the buffer shown, so neither waits. A frame kept from a busy swap
// waits for the flip whatever the buffers.
#[test]
fn one_and_three_buffers_lend_while_a_flip_is_pending() {
    for (buffers, ready) in [
        (Buffers::One, true),
        (Buffers::Two, false),
        (Buffers::Three, true),
    ] {
        let mut v = kept(buffers);
        frame(&mut v, &[DOT]).unwrap();
        assert!(v.is_flipping(DisplayId(A)));
        assert_eq!(v.ready_for_frame(DisplayId(A)), ready, "{buffers:?}");
        assert_eq!(lends(&mut v, A), ready, "{buffers:?}");
        if ready {
            let busy = frame(&mut v, &[DOT]);
            assert!(matches!(busy, Err(SwapError::Busy)), "{buffers:?}");
            assert!(!v.ready_for_frame(DisplayId(A)), "the kept frame waits");
            flip(&mut v);
            assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Frame);
        }
    }
}

// A cursor commit while a token is out sends the cursor plane alone: the
// frame kept from a failed swap goes with the token's own swap, so the
// buffer the render thread draws into is never put on screen under it.
#[test]
fn a_commit_while_a_token_is_out_keeps_the_kept_frame_for_it() {
    let mut v = kept(Buffers::Two);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    assert!(matches!(frame(&mut v, &[DOT]), Err(SwapError::Failed(_))));
    let token = v.lend(DisplayId(A)).unwrap();
    assert_eq!(token.age(), 1, "the buffer holds the frame that failed");
    assert!(!v.need_commit(DisplayId(A)));
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Nothing);
    assert_eq!(v.swap(token, &[]).unwrap(), Swapped::Frame);
}

// A modeset that gives the display new buffers while its token is out
// keeps the token's buffer allocated and mapped: the render thread still
// draws into it, the modeset does not wait for it, and it is freed when the
// token comes back, which is too late. The next frame is drawn whole, into
// a new buffer.
#[test]
fn a_buffer_replaced_while_its_token_is_out_stays_until_the_token_returns() {
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
    let mut v = awake(Drm2d::with(card, policy).unwrap());
    let mut token = v.lend(DisplayId(A)).unwrap();
    let old = v.outputs[0].buffers[token.buffer()].handle;
    // The wake-up's test commit fails, so it is tried again in the
    // display's default mode, which needs new buffers.
    v.sleep();
    flip(&mut v);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    v.wake_up().unwrap();
    flip(&mut v);
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)), "a new mode");
    assert!(
        v.outputs[0].buffers.iter().all(|b| b.handle != old),
        "new buffers"
    );
    assert!(
        v.card().state().dumbs.contains_key(&old),
        "the token's buffer stays allocated"
    );
    assert!(!v.ready_for_frame(DisplayId(A)), "the token is still out");
    token.draw(|c| c.pixels.fill(7)).expect("still mapped");
    let from = v.card().state().buffer_calls.len();
    assert!(matches!(v.swap(token, &[DOT]), Err(SwapError::Dropped)));
    assert!(buffer_calls(&v, from).contains(&BufferCall::DestroyDumb(old)));
    assert!(!v.card().state().dumbs.contains_key(&old));
    assert_eq!(age(&mut v, A), 0);
    assert_eq!(token_size(&mut v, A), (640, 480));
}

// A token that comes back after its display's VT was released, or after
// its display was powered down, is dropped: the frame is not shown and
// every buffer is age 0, so the next frame is drawn whole.
#[test]
fn a_token_back_after_a_vt_release_or_a_blank_is_dropped() {
    for release in [true, false] {
        let mut v = awake(one());
        for _ in 0..3 {
            frame(&mut v, &[DOT]).unwrap();
            flip(&mut v);
        }
        let token = v.lend(DisplayId(A)).unwrap();
        assert_eq!(token.age(), 2);
        if release {
            v.sleep();
            flip(&mut v);
            v.wake_up().unwrap();
            flip(&mut v);
        } else {
            v.set_dpms(DisplayId(A), Dpms::Off).unwrap();
            v.set_dpms(DisplayId(A), Dpms::On).unwrap();
        }
        let commits = v.card().state().commits.len();
        assert!(matches!(v.swap(token, &[DOT]), Err(SwapError::Dropped)));
        assert_eq!(v.card().state().commits.len(), commits, "nothing shown");
        assert_eq!(age(&mut v, A), 0, "release: {release}");
        frame(&mut v, &[DOT]).unwrap();
        flip(&mut v);
        assert_eq!(age(&mut v, A), 0, "the other buffer, release: {release}");
    }
}

// A token that comes back to a display that has gone is dropped, and its
// buffer is freed then.
#[test]
fn a_token_back_to_a_display_gone_is_dropped() {
    let mut v = awake(one());
    let token = v.lend(DisplayId(A)).unwrap();
    let held = v.outputs[0].buffers[token.buffer()].handle;
    v.card().state().connectors.get_mut(&A).unwrap().connected = false;
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Removed(DisplayId(A))]);
    assert!(v.card().state().dumbs.contains_key(&held));
    assert!(matches!(v.swap(token, &[DOT]), Err(SwapError::Dropped)));
    assert!(v.card().state().dumbs.is_empty());
}

// A token the owner drops instead of giving back frees the display to
// lend again, with every buffer at age 0, as for one that came back late.
#[test]
fn a_token_dropped_unreturned_leaves_every_buffer_age_0() {
    let mut v = awake(one());
    for _ in 0..3 {
        frame(&mut v, &[DOT]).unwrap();
        flip(&mut v);
    }
    let token = v.lend(DisplayId(A)).unwrap();
    assert_eq!(token.age(), 2);
    assert!(!v.ready_for_frame(DisplayId(A)));
    drop(token);
    assert!(v.ready_for_frame(DisplayId(A)));
    assert_eq!(age(&mut v, A), 0);
}

// With one buffer the sleep's clear finds the buffer shown lent: the
// token's memory is cleared when it comes back, after the render thread
// is done with it.
#[test]
fn the_sleep_clears_a_lent_buffer_when_its_token_returns() {
    let mut v = kept(Buffers::One);
    let mut token = v.lend(DisplayId(A)).unwrap();
    v.sleep();
    token.draw(|c| c.pixels.fill(9)).unwrap();
    let map = Arc::clone(&v.outputs[0].buffers[0].map);
    assert!(matches!(v.swap(token, &[DOT]), Err(SwapError::Dropped)));
    assert!(lock(&map).iter().all(|&b| b == 0));
}

// The token's memory goes to the render thread and is drawn there.
#[test]
fn a_token_is_drawn_into_on_another_thread() {
    let mut v = awake(one());
    let mut token = v.lend(DisplayId(A)).unwrap();
    let index = token.buffer();
    let token = std::thread::spawn(move || {
        token.draw(|c| c.pixels[..4].fill(0x42)).unwrap();
        token
    })
    .join()
    .unwrap();
    v.swap(token, &[DOT]).unwrap();
    assert_eq!(lock(&v.outputs[0].buffers[index].map)[..4], [0x42; 4]);
}

// A display whose connector has no DPMS property stays lit through the
// blank, and nothing powers it on after it, so nothing else sets its ages
// to 0: a token lent before the blank and back after it is dropped, and
// the drop alone leaves every buffer at age 0.
#[test]
fn a_token_back_after_a_blank_without_dpms_leaves_every_buffer_age_0() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().no_dpms.push(A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    for _ in 0..3 {
        frame(&mut v, &[DOT]).unwrap();
        flip(&mut v);
    }
    let token = v.lend(DisplayId(A)).unwrap();
    assert_eq!(token.age(), 2);
    v.set_dpms(DisplayId(A), Dpms::Off).unwrap();
    v.set_dpms(DisplayId(A), Dpms::On).unwrap();
    assert!(matches!(v.swap(token, &[DOT]), Err(SwapError::Dropped)));
    assert_eq!(age(&mut v, A), 0);
}

// A retake, a wake-up with no release before it, sets every display up
// again: what each buffer holds counts as unknown, so a token lent before
// it comes back too late, and each buffer is drawn whole after it, as the
// C reference's text layer drew both buffers whole after a wake-up.
#[test]
fn a_token_back_after_a_retake_is_dropped() {
    let mut v = awake(one());
    for _ in 0..3 {
        frame(&mut v, &[DOT]).unwrap();
        flip(&mut v);
    }
    let token = v.lend(DisplayId(A)).unwrap();
    assert_eq!(token.age(), 2);
    v.wake_up().unwrap();
    flip(&mut v);
    assert!(matches!(v.swap(token, &[DOT]), Err(SwapError::Dropped)));
    for _ in 0..2 {
        assert_eq!(age(&mut v, A), 0);
        frame(&mut v, &[DOT]).unwrap();
        flip(&mut v);
    }
    assert_eq!(age(&mut v, A), 2);
}

// A hotplug's modeset that gives a display new buffers, with no wake-up
// around it, takes a new lease: a token lent for an old buffer comes back
// too late, and its buffer is freed then. Here the modeset with the
// configured mode fails its test commit, so every display falls back to
// its default mode, which needs new buffers for A.
#[test]
fn new_buffers_from_a_hotplug_drop_a_token_lent_before() {
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
    let mut v = a_then_b(policy);
    assert_eq!(v.size(DisplayId(A)), Some((320, 240)));
    let token = v.lend(DisplayId(A)).unwrap();
    let old = v.outputs[0].buffers[token.buffer()].handle;
    v.card().state().listed.push(B);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    v.poll().unwrap();
    flip(&mut v);
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)), "the default mode");
    assert!(v.outputs[0].buffers.iter().all(|b| b.handle != old));
    assert!(matches!(v.swap(token, &[DOT]), Err(SwapError::Dropped)));
    assert!(!v.card().state().dumbs.contains_key(&old));
    assert_eq!(age(&mut v, A), 0);
}

// vtium/vtium#208: a display online whose mode a modeset changes in place
// is told to redraw, and is not announced as added again: its owner reads
// the display's new size and refresh rate when it hears `Redraw`. Here the
// hotplug that brings B fails its test commit with A's configured mode,
// so every display falls back to its default mode and A, kept on its
// connector, goes from 320x240 to 640x480.
#[test]
fn a_mode_changed_in_place_asks_for_a_redraw_at_the_new_size() {
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
    let mut v = a_then_b(policy);
    events(&mut v);
    assert_eq!(v.size(DisplayId(A)), Some((320, 240)));
    v.card().state().listed.push(B);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    v.poll().unwrap();
    assert_eq!(
        events(&mut v),
        [Event::Redraw(DisplayId(A)), Event::Added(DisplayId(B))]
    );
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)), "the default mode");
    assert_eq!(v.refresh_mhz(DisplayId(A)), Some(60_000));
    flip(&mut v);
    assert_eq!(token_size(&mut v, A), (640, 480));
    assert_eq!(age(&mut v, A), 0, "drawn whole");
}

// vtium/vtium#216: a connector that stays connected while its list of
// modes changes, as when a monitor is swapped faster than its disconnect
// is seen, gets its mode chosen again from the new list. Here the new
// monitor prefers 800x600, so A goes from 640x480 to 800x600, is told to
// redraw and is not announced as added again. A hotplug that finds the
// same list sets no mode.
#[test]
fn a_connector_that_lists_new_modes_takes_its_new_preferred_mode() {
    let mut v = awake(one());
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    let commits = v.card().state().commits.len();
    v.poll().unwrap();
    assert_eq!(
        v.card().state().commits.len(),
        commits,
        "the same modes set nothing"
    );
    let swapped = vec![mode(800, 600, 60, true), mode(640, 480, 60, false)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A))]);
    let c = v.card().last_commit().unwrap();
    assert_eq!(
        *c.blob(100, "MODE_ID").unwrap(),
        mode_bytes(&mode(800, 600, 60, true).0)
    );
    assert_eq!(v.size(DisplayId(A)), Some((800, 600)));
    flip(&mut v);
    assert_eq!(token_size(&mut v, A), (800, 600));
    assert_eq!(age(&mut v, A), 0, "drawn whole");
}

/// A directory of its own for a test's sysfs, named for the test.
fn sysfs_dir(test: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("vtium-drm2d-{test}-{}", std::process::id()))
}

/// Writes connector `name`'s EDID under `dir`, as sysfs shows it: maker
/// AAA, product 1, serial string `serial`, `cm` across and down.
fn write_edid(dir: &std::path::Path, name: &str, serial: &[u8; 10], cm: (u8, u8)) {
    let conn = dir.join(format!("card1-{name}"));
    std::fs::create_dir_all(&conn).unwrap();
    let mut edid = vec![0u8; 128];
    edid[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    edid[8..12].copy_from_slice(&[0x04, 0x21, 0x01, 0x00]);
    (edid[21], edid[22]) = cm;
    edid[54..59].copy_from_slice(b"\0\0\0\xff\0");
    edid[59..69].copy_from_slice(serial);
    edid[69..72].copy_from_slice(b"\n  ");
    edid[127] = 0u8.wrapping_sub(edid.iter().fold(0u8, |s, &b| s.wrapping_add(b)));
    std::fs::write(conn.join("edid"), &edid).unwrap();
}

/// [`one`], its connector HDMI-A-1 showing monitor `serial`, 60 by 34 cm,
/// through the sysfs at `dir`; asleep.
fn one_showing(dir: &std::path::Path, serial: &[u8; 10]) -> Drm2d<FakeCard> {
    write_edid(dir, "HDMI-A-1", serial, (60, 34));
    let v = one();
    v.card().state().sysfs = Some(dir.to_path_buf());
    v
}

/// The serial string display `id`'s identity names, if any.
fn serial(v: &Drm2d<FakeCard>, id: u32) -> Option<String> {
    v.identity(DisplayId(id))
        .and_then(|i| i.edid.as_ref())
        .map(|e| e.to_string())
}

// vtium/vtium#216: a connector that shows another monitor than the one its
// display was bound to, as when a monitor is swapped faster than its
// disconnect is seen, is let go and bound again as the new monitor: the
// owner hears it removed and added, and the display takes the new
// monitor's identity, density and mode.
#[test]
fn a_connector_showing_another_monitor_is_bound_again() {
    let dir = sysfs_dir("another-monitor");
    let mut v = awake(one_showing(&dir, b"MONITOR001"));
    assert_eq!(serial(&v, A).as_deref(), Some("AAA:0x0001:MONITOR001"));
    write_edid(&dir, "HDMI-A-1", b"MONITOR002", (30, 17));
    let swapped = vec![mode(800, 600, 60, true)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.poll().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        events(&mut v),
        [Event::Removed(DisplayId(A)), Event::Added(DisplayId(A))]
    );
    assert_eq!(serial(&v, A).as_deref(), Some("AAA:0x0001:MONITOR002"));
    assert_eq!(v.size(DisplayId(A)), Some((800, 600)));
    // 800 pixels across 30 cm, where 640 across 60 cm gave 27
    assert_eq!(
        v.density(DisplayId(A)).map(|d| d.dpi().0.round()),
        Some(68.0)
    );
    flip(&mut v);
    assert_eq!(token_size(&mut v, A), (800, 600));
    assert_eq!(age(&mut v, A), 0);
}

// vtium/vtium#216: the same monitor keeps its display. With the same modes
// a hotplug sets nothing; with an EDID that cannot be read for a moment it
// sets nothing; with new modes, as after a DisplayPort link is trained
// again, its mode is chosen again in place and it is told to redraw.
#[test]
fn the_same_monitor_keeps_its_display() {
    let dir = sysfs_dir("same-monitor");
    let mut v = awake(one_showing(&dir, b"MONITOR001"));
    let commits = v.card().state().commits.len();
    v.poll().unwrap();
    std::fs::remove_file(dir.join("card1-HDMI-A-1").join("edid")).unwrap();
    v.poll().unwrap();
    assert_eq!(events(&mut v), []);
    assert_eq!(v.card().state().commits.len(), commits, "nothing set");
    write_edid(&dir, "HDMI-A-1", b"MONITOR001", (60, 34));
    let retrained = vec![mode(800, 600, 60, true), mode(640, 480, 60, false)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = retrained;
    v.poll().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A))]);
    assert_eq!(v.size(DisplayId(A)), Some((800, 600)));
    assert_eq!(serial(&v, A).as_deref(), Some("AAA:0x0001:MONITOR001"));
}

// vtium/vtium#216: a display bound while its EDID could not be read, whose
// EDID can be read later, is bound again once, so the head file's EDID
// rules can match it; the hotplug after that changes nothing.
#[test]
fn a_display_whose_edid_becomes_readable_is_bound_again_once() {
    let dir = sysfs_dir("edid-later");
    let v = one();
    v.card().state().sysfs = Some(dir.clone());
    let mut v = awake(v);
    assert_eq!(serial(&v, A), None);
    write_edid(&dir, "HDMI-A-1", b"MONITOR001", (60, 34));
    v.poll().unwrap();
    assert_eq!(
        events(&mut v),
        [Event::Removed(DisplayId(A)), Event::Added(DisplayId(A))]
    );
    assert_eq!(serial(&v, A).as_deref(), Some("AAA:0x0001:MONITOR001"));
    flip(&mut v);
    v.events().for_each(drop);
    v.poll().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(events(&mut v), []);
}

// vtium/vtium#216: a connector left dark that shows another monitor is
// asked of the dark rule again, as the new monitor: here the rule leaves
// only the first monitor dark, so the second is lit.
#[test]
fn a_dark_connector_showing_another_monitor_is_asked_again() {
    let dir = sysfs_dir("another-dark-monitor");
    let mut v = one_showing(&dir, b"MONITOR001");
    v.set_dark(Box::new(|d: &Identity| {
        d.edid
            .as_ref()
            .is_some_and(|e| e.to_string().ends_with("MONITOR001"))
    }));
    let mut v = awake(v);
    assert_eq!(v.dark().count(), 1);
    assert_eq!(v.displays().count(), 0);
    write_edid(&dir, "HDMI-A-1", b"MONITOR002", (60, 34));
    v.poll().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(events(&mut v), [Event::Added(DisplayId(A))]);
    assert_eq!(v.dark().count(), 0);
    assert_eq!(serial(&v, A).as_deref(), Some("AAA:0x0001:MONITOR002"));
}

/// The test-only commits the card has been sent.
fn tests_sent(v: &Drm2d<FakeCard>) -> usize {
    v.card()
        .state()
        .commits
        .iter()
        .filter(|c| c.test_only())
        .count()
}

/// A display at 640x480 whose connector then lists one mode of 800x600
/// alone, which the card refuses twice: the mode chosen, and the default.
fn refused_new_modes(mut v: Drm2d<FakeCard>) -> Drm2d<FakeCard> {
    let swapped = vec![mode(800, 600, 60, true)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.card().state().commit_errors.push_back(Errno::INVAL);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    v.poll().unwrap();
    v
}

// vtium/vtium#216: a new list of modes whose mode the card refuses, as
// chosen and as the default, leaves the display running at the mode it is
// set to, which its buffers fit, and holds the new mode back. Nothing is
// retried and nothing reports that the displays did not wake, so a key
// press tries nothing, and a hotplug with the same list tries nothing.
#[test]
fn a_new_mode_that_cannot_be_set_is_held_back() {
    let v = awake(one());
    let before = tests_sent(&v);
    let mut v = refused_new_modes(v);
    assert_eq!(
        tests_sent(&v) - before,
        3,
        "the mode chosen, the default, and the mode set"
    );
    assert_eq!(events(&mut v), []);
    assert!(v.retries.is_none() && !v.wake_failed, "nothing to retry");
    assert_eq!(
        v.size(DisplayId(A)),
        Some((640, 480)),
        "the mode it is set to"
    );
    flip(&mut v);
    assert_eq!(token_size(&mut v, A), (640, 480));
    let tried = tests_sent(&v);
    v.input().unwrap();
    v.poll().unwrap();
    assert_eq!(tests_sent(&v), tried, "a key and a hotplug try nothing");
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
}

// vtium/vtium#216: a mode held back gives way to the next list of modes
// the connector gives, and is not tried again at the wake-up after.
#[test]
fn a_held_back_mode_gives_way_to_a_new_list() {
    let mut v = refused_new_modes(awake(one()));
    flip(&mut v);
    v.events().for_each(drop);
    let swapped = vec![mode(1024, 768, 60, true), mode(800, 600, 60, false)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.poll().unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A))]);
    assert_eq!(v.size(DisplayId(A)), Some((1024, 768)));
    flip(&mut v);
    v.sleep();
    v.wake_up().unwrap();
    assert_eq!(v.size(DisplayId(A)), Some((1024, 768)), "nothing held back");
}

// vtium/vtium#216: a mode held back is tried again as the card wakes.
#[test]
fn a_held_back_mode_is_tried_again_at_a_wake_up() {
    let mut v = refused_new_modes(awake(one()));
    flip(&mut v);
    v.sleep();
    v.events().for_each(drop);
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A)), Event::Awake]);
    assert_eq!(v.size(DisplayId(A)), Some((800, 600)));
    flip(&mut v);
    assert_eq!(token_size(&mut v, A), (800, 600));
}

// vtium/vtium#216: a mode held back is tried again when the set of
// displays changes: here B is plugged in, and in the second part B is
// unplugged.
#[test]
fn a_held_back_mode_is_tried_again_when_the_displays_change() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    let mut v = refused_new_modes(awake(Drm2d::with(card, ModePolicy::default()).unwrap()));
    flip(&mut v);
    v.events().for_each(drop);
    v.card().state().listed.retain(|&c| c == A);
    v.poll().unwrap();
    assert_eq!(
        events(&mut v),
        [Event::Removed(DisplayId(B)), Event::Redraw(DisplayId(A))]
    );
    assert_eq!(v.size(DisplayId(A)), Some((800, 600)));

    let mut v = refused_new_modes(a_then_b(ModePolicy::default()));
    flip(&mut v);
    v.events().for_each(drop);
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    v.card().state().listed.push(B);
    v.poll().unwrap();
    assert_eq!(
        events(&mut v),
        [Event::Redraw(DisplayId(A)), Event::Added(DisplayId(B))]
    );
    assert_eq!(v.size(DisplayId(A)), Some((800, 600)));
}

/// A legacy card with one display at 640x480, whose connector then lists
/// one mode of 800x600 alone.
fn legacy_with_new_modes() -> Drm2d<FakeCard> {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().atomic = false;
    let v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    let swapped = vec![mode(800, 600, 60, true)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v
}

/// Whether display `id` lends a token its renderer can draw into.
fn drawable(v: &mut Drm2d<FakeCard>, id: u32) -> bool {
    let mut token = v.lend(DisplayId(id)).expect("a token");
    let drawn = token.draw(|_| ()).is_some();
    v.give_back(token);
    drawn
}

// vtium/vtium#216: a new mode that fails for a moment, as where one
// buffer cannot be made, leaves the display set at the mode it is set to,
// which its buffers fit, and starts the wake-up retries, whose next try
// sets the new mode.
#[test]
fn a_new_mode_that_fails_for_a_moment_is_tried_again() {
    let mut v = awake(one());
    let swapped = vec![mode(800, 600, 60, true)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.card().state().dumb_errors.push_back(Errno::NOMEM);
    v.poll().unwrap();
    assert_eq!(events(&mut v), []);
    assert_eq!(
        v.size(DisplayId(A)),
        Some((640, 480)),
        "the mode it is set to"
    );
    flip(&mut v);
    v.events().for_each(drop);
    assert!(drawable(&mut v, A));
    assert!(v.retries.is_some(), "the retries run");
    v.retry_wake();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A))]);
    assert_eq!(v.size(DisplayId(A)), Some((800, 600)));
    assert!(v.retries.is_none(), "nothing is left to retry");
    flip(&mut v);
    assert_eq!(token_size(&mut v, A), (800, 600));
}

// vtium/vtium#216: on a legacy card, a new mode whose buffers cannot be
// made, or whose CRTC is busy, leaves the display set at the mode it is
// set to, so the tokens it lends can be drawn into, and starts the wake-up
// retries, whose next try sets it.
#[test]
fn a_legacy_display_whose_new_mode_fails_for_a_moment_tries_it_again() {
    for fail in ["buffers", "crtc"] {
        let mut v = legacy_with_new_modes();
        if fail == "buffers" {
            v.card().state().dumb_errors.push_back(Errno::NOMEM);
        } else {
            v.card().state().set_crtc_errors.push_back(Errno::BUSY);
        }
        v.poll().unwrap();
        assert_eq!(events(&mut v), [], "{fail}");
        assert_eq!(v.size(DisplayId(A)), Some((640, 480)), "{fail}");
        assert_eq!(token_size(&mut v, A), (640, 480), "{fail}");
        assert!(drawable(&mut v, A), "{fail}");
        v.retry_wake();
        assert_eq!(v.size(DisplayId(A)), Some((800, 600)), "{fail}");
        assert_eq!(token_size(&mut v, A), (800, 600), "{fail}");
    }
}

// vtium/vtium#216: on a legacy card, a new mode the card refuses with
// EINVAL is held back as an atomic card's is: the display keeps the mode
// it is set to, nothing is retried, and a hotplug with the same list sets
// nothing.
#[test]
fn a_legacy_display_whose_new_mode_is_refused_holds_it_back() {
    let mut v = legacy_with_new_modes();
    v.card().state().set_crtc_errors.push_back(Errno::INVAL);
    v.poll().unwrap();
    assert_eq!(events(&mut v), []);
    assert!(v.retries.is_none(), "nothing to retry");
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    assert_eq!(token_size(&mut v, A), (640, 480));
    assert!(drawable(&mut v, A));
    let set = v.card().state().set_crtcs.len();
    v.poll().unwrap();
    assert_eq!(v.card().state().set_crtcs.len(), set, "nothing set");
}

/// Lists one mode of 800x600 alone on A's connector, whose buffers the
/// card can never make: every dumb buffer asked for fails with ENOMEM.
fn new_modes_that_never_fit(v: &Drm2d<FakeCard>) {
    let swapped = vec![mode(800, 600, 60, true)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.card().state().dumb_errors.extend([Errno::NOMEM; 1000]);
}

/// Runs the wake-up retries until they stop, completing the flips before
/// each, as the card does well within a retry's interval, and returns the
/// events they reported other than flips, and how many ran.
fn run_the_retries(v: &mut Drm2d<FakeCard>) -> (Vec<Event>, usize) {
    let mut seen = Vec::new();
    let mut tries = 0;
    while v.waits().1 {
        assert!(tries < crate::WAKE_RETRIES, "the retries are bounded");
        flip(v);
        v.retry_wake();
        tries += 1;
        seen.extend(
            events(v)
                .into_iter()
                .filter(|e| !matches!(e, Event::Flipped(_))),
        );
    }
    (seen, tries)
}

// vtium/vtium#216: a wake-up whose new mode keeps failing for another
// reason than the card refusing it, as where its buffers can never be
// made, wakes the card at the mode the display is set to, which needs no
// new buffers, and keeps DRM master. The retries try the new mode again
// without letting the card go, and once they run out it is held back: no
// wake-up is reported failed, and a key or a hotplug tries nothing. The
// next wake-up tries it again.
#[test]
fn a_wake_up_whose_new_mode_keeps_failing_wakes_at_the_mode_set() {
    let mut v = awake(one());
    v.sleep();
    flip(&mut v);
    v.events().for_each(drop);
    new_modes_that_never_fit(&v);
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A)), Event::Awake]);
    assert!(v.card().state().master, "DRM master is kept");
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    flip(&mut v);
    assert_eq!(token_size(&mut v, A), (640, 480));
    assert!(drawable(&mut v, A));
    assert_eq!(v.waits(), (false, true), "the new mode is tried again");
    logs::capture();
    let (seen, tries) = run_the_retries(&mut v);
    assert_eq!(seen, [], "no wake-up failed, and nothing to redraw");
    assert_eq!(tries, crate::WAKE_RETRIES);
    assert!(
        !logs::at_least(Level::Error)
            .iter()
            .any(|l| l.contains("cannot wake")),
        "nothing logs that the display did not wake"
    );
    assert!(v.card().state().master && v.awake);
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    let tried = tests_sent(&v);
    v.input().unwrap();
    v.poll().unwrap();
    assert_eq!(tests_sent(&v), tried, "a key and a hotplug try nothing");
    assert_eq!(v.waits(), (false, false));
    v.card().state().dumb_errors.clear();
    v.sleep();
    flip(&mut v);
    v.wake_up().unwrap();
    assert_eq!(
        v.size(DisplayId(A)),
        Some((800, 600)),
        "tried at the wake-up"
    );
}

// vtium/vtium#216: on a legacy card, a wake-up whose new mode keeps failing
// for another reason than the card refusing it sets the display at the
// mode it is set to, keeps DRM master, and asks for one redraw; the
// retries ask for none, and once they run out the mode is held back.
#[test]
fn a_legacy_wake_up_whose_new_mode_keeps_failing_wakes_at_the_mode_set() {
    for fail in ["buffers", "crtc"] {
        let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
        card.state().atomic = false;
        let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
        v.sleep();
        v.events().for_each(drop);
        let swapped = vec![mode(800, 600, 60, true)];
        v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
        if fail == "buffers" {
            v.card().state().dumb_errors.extend([Errno::NOMEM; 1000]);
        }
        // The CRTC refuses every set at the new mode as busy.
        v.card().state().busy_size = (fail == "crtc").then_some((800, 600));
        v.wake_up().unwrap();
        assert_eq!(
            events(&mut v),
            [Event::Redraw(DisplayId(A)), Event::Awake],
            "{fail}"
        );
        assert!(v.card().state().master, "{fail}: DRM master is kept");
        let last = v.card().state().set_crtcs.last().unwrap().3;
        assert_eq!(last.size(), (640, 480), "{fail}: set at the mode set");
        assert_eq!(token_size(&mut v, A), (640, 480), "{fail}");
        let (seen, tries) = run_the_retries(&mut v);
        assert_eq!(seen, [], "{fail}: no redraw, and no wake-up failed");
        assert_eq!(tries, crate::WAKE_RETRIES, "{fail}");
        assert!(v.card().state().master && v.awake, "{fail}");
        let set = v.card().state().set_crtcs.len();
        v.input().unwrap();
        v.poll().unwrap();
        assert_eq!(v.card().state().set_crtcs.len(), set, "{fail}: nothing set");
    }
}

// vtium/vtium#216: on a legacy card, a display whose CRTC cannot be set at
// a wake-up, at its new mode or at the mode it was set to, is not asked to
// redraw, so the retries of the wake-up ask for no redraw either.
#[test]
fn a_legacy_display_not_set_at_a_wake_up_is_not_redrawn() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().atomic = false;
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    v.sleep();
    v.events().for_each(drop);
    let swapped = vec![mode(800, 600, 60, true)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.card().state().set_crtc_errors.extend([Errno::BUSY; 1000]);
    let _ = v.wake_up();
    let (seen, _) = run_the_retries(&mut v);
    let mut all = events(&mut v);
    all.extend(seen);
    assert!(
        !all.contains(&Event::Redraw(DisplayId(A))),
        "no redraw for a display not set: {all:?}"
    );
}

// vtium/vtium#216: on a legacy card, a wake-up that cannot set a display's
// CRTC, at its new mode or at the mode it was set to, or with no new mode
// at the mode it is set to, wakes the card, which keeps DRM master, but the
// display counts as not set: the retries try it again, and once they run
// out they report WakeFailed, so the next key press tries again and sets
// it.
#[test]
fn a_legacy_display_not_set_at_a_wake_up_fails_the_wake_up() {
    for (new, size) in [(true, (800, 600)), (false, (640, 480))] {
        let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
        card.state().atomic = false;
        let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
        v.sleep();
        v.events().for_each(drop);
        if new {
            let swapped = vec![mode(800, 600, 60, true)];
            v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
        }
        v.card().state().set_crtc_errors.extend([Errno::BUSY; 1000]);
        v.wake_up().unwrap();
        assert_eq!(events(&mut v), [Event::Awake], "new mode: {new}");
        assert!(v.waits().1, "new mode: {new}: the display is tried again");
        let (seen, tries) = run_the_retries(&mut v);
        assert_eq!(seen, [Event::WakeFailed], "new mode: {new}");
        assert_eq!(tries, crate::WAKE_RETRIES, "new mode: {new}");
        assert!(v.wake_failed, "new mode: {new}");
        assert!(
            v.card().state().master && v.awake,
            "new mode: {new}: the card stays awake"
        );
        v.card().state().set_crtc_errors.clear();
        v.input().unwrap();
        assert_eq!(
            events(&mut v),
            [Event::Redraw(DisplayId(A))],
            "new mode: {new}"
        );
        let last = v.card().state().set_crtcs.last().unwrap().3;
        assert_eq!(last.size(), size, "new mode: {new}: the key press sets it");
        assert!(!v.waits().1 && !v.wake_failed, "new mode: {new}");
    }
}

// vtium/vtium#216: a wake-up retried while another process held DRM
// master, which wakes the card at the mode the display is set to because
// its new mode cannot be set, reports the card awake once: the retries
// that go on trying the new mode report nothing.
#[test]
fn a_retried_wake_up_at_the_mode_set_reports_awake_once() {
    let mut v = awake(one());
    v.sleep();
    flip(&mut v);
    v.events().for_each(drop);
    new_modes_that_never_fit(&v);
    v.card().state().master_elsewhere = true;
    assert!(v.wake_up().is_err());
    v.card().state().master_elsewhere = false;
    v.retry_wake();
    assert_eq!(events(&mut v), [Event::Redraw(DisplayId(A)), Event::Awake]);
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    let (seen, _) = run_the_retries(&mut v);
    assert_eq!(seen, [], "the card is reported awake once");
    assert!(v.card().state().master && v.awake);
}

// vtium/vtium#216: a mode owed after a failure of another kind, which the
// card then refuses when the retries try it again, is not tried again by
// the retries. Where the card refuses its default mode too, it is held
// back; where the card takes the default mode, which is here the mode the
// display is set to, nothing is owed.
#[test]
fn a_mode_owed_that_the_card_refuses_is_not_retried() {
    let mut v = awake(one());
    let swapped = vec![mode(800, 600, 60, true)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.card().state().dumb_errors.push_back(Errno::NOMEM);
    v.poll().unwrap();
    flip(&mut v);
    v.card().state().commit_errors.extend([Errno::INVAL; 2]);
    v.retry_wake();
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)), "held back");
    assert!(!v.waits().1, "nothing is retried");
    let tried = tests_sent(&v);
    flip(&mut v);
    v.poll().unwrap();
    assert_eq!(tests_sent(&v), tried, "a hotplug tries nothing");

    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    let policy = ModePolicy {
        use_original: false,
        wanted: Some(Box::new(|_: &str| {
            Some(Wanted {
                width: 800,
                height: 600,
                rate: None,
            })
        })),
    };
    let mut v = awake(Drm2d::with(card, policy).unwrap());
    let swapped = vec![mode(640, 480, 60, true), mode(800, 600, 60, false)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = swapped;
    v.card().state().dumb_errors.push_back(Errno::NOMEM);
    v.poll().unwrap();
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    flip(&mut v);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    v.retry_wake();
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)), "the default mode");
    assert!(!v.waits().1, "nothing is retried");
}

// vtium/vtium#216: a mode owed gives way to the next list of modes the
// connector gives, as a mode held back does: here the new list no longer
// has the mode owed, and the retries set nothing the monitor does not
// list.
#[test]
fn a_mode_owed_gives_way_to_a_new_list() {
    let mut v = awake(one());
    new_modes_that_never_fit(&v);
    v.poll().unwrap();
    flip(&mut v);
    v.card().state().dumb_errors.clear();
    let relisted = vec![mode(640, 480, 60, true), mode(1024, 768, 60, false)];
    v.card().state().connectors.get_mut(&A).unwrap().modes = relisted;
    v.poll().unwrap();
    flip(&mut v);
    v.retry_wake();
    flip(&mut v);
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    assert!(!v.waits().1, "nothing is left to retry");
}

// vtium/vtium#216: a hotplug that finds a new mode which keeps failing for
// another reason than the card refusing it leaves the display running at
// the mode it is set to. The retries try the new mode again without
// drawing the other displays whole at each try, and once they run out the
// mode is held back: no wake-up is reported failed, and a key or a hotplug
// starts nothing.
#[test]
fn a_hotplug_whose_new_mode_keeps_failing_holds_it_back_after_the_retries() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    for _ in 0..2 {
        fill(&mut v, B, 1, &[DOT]).unwrap();
        flip(&mut v);
    }
    v.events().for_each(drop);
    let aged = age(&mut v, B);
    assert_ne!(aged, 0, "B's buffers hold frames");
    new_modes_that_never_fit(&v);
    v.poll().unwrap();
    assert_eq!(events(&mut v), []);
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    flip(&mut v);
    assert!(drawable(&mut v, A));
    assert!(v.waits().1, "the new mode is tried again");
    v.retry_wake();
    flip(&mut v);
    assert_eq!(age(&mut v, B), aged, "a retry leaves B's buffers known");
    logs::capture();
    let (seen, _) = run_the_retries(&mut v);
    assert_eq!(seen, [], "no wake-up failed");
    assert!(
        !logs::at_least(Level::Error)
            .iter()
            .any(|l| l.contains("cannot wake")),
        "nothing logs that the display did not wake"
    );
    assert!(!v.wake_failed);
    flip(&mut v);
    assert_eq!(age(&mut v, B), aged);
    assert_eq!(v.size(DisplayId(A)), Some((640, 480)));
    let tried = tests_sent(&v);
    v.input().unwrap();
    v.poll().unwrap();
    assert_eq!(tests_sent(&v), tried, "a key and a hotplug try nothing");
    assert_eq!(v.waits(), (false, false), "and start no retries");
}

// vtium/vtium#216: a mode changed in place at the same size, with only a
// new refresh rate, keeps the display's buffers but is told to redraw, so
// its owner reads the new rate, and the display is drawn whole. Here A is
// configured at 640x480 at 30 Hz, and the hotplug that brings B fails its
// test commit, so every display falls back to its default mode and A goes
// to 640x480 at 60 Hz.
#[test]
fn a_refresh_rate_changed_in_place_asks_for_a_redraw() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true), mode(640, 480, 30, false)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c == A);
    let policy = ModePolicy {
        use_original: false,
        wanted: Some(Box::new(|_: &str| {
            Some(Wanted {
                width: 640,
                height: 480,
                rate: Some(30),
            })
        })),
    };
    let mut v = awake(Drm2d::with(card, policy).unwrap());
    assert_eq!(v.refresh_mhz(DisplayId(A)), Some(30_000));
    let held = fbs(&v, A);
    v.card().state().listed.push(B);
    v.card().state().commit_errors.push_back(Errno::INVAL);
    v.poll().unwrap();
    assert_eq!(
        events(&mut v),
        [Event::Redraw(DisplayId(A)), Event::Added(DisplayId(B))]
    );
    assert_eq!(
        v.refresh_mhz(DisplayId(A)),
        Some(60_000),
        "the default mode"
    );
    assert_eq!(fbs(&v, A), held, "the buffers are kept");
    flip(&mut v);
    assert_eq!(token_size(&mut v, A), (640, 480));
    assert_eq!(age(&mut v, A), 0, "drawn whole");
}

// With one buffer the sleep's clear finds the buffer lent, and a token
// given back unused pays the clear it owes, as a late swap does.
#[test]
fn a_token_given_back_after_the_sleep_clears_its_buffer() {
    let mut v = kept(Buffers::One);
    let mut token = v.lend(DisplayId(A)).unwrap();
    v.sleep();
    token.draw(|c| c.pixels.fill(9)).unwrap();
    let map = Arc::clone(&v.outputs[0].buffers[0].map);
    v.give_back(token);
    assert!(lock(&map).iter().all(|&b| b == 0));
}

// A frame kept from a failed swap waits in the back buffer; a token for
// that buffer drawn over it and dropped as too late leaves the buffer
// holding neither frame, so the kept frame goes whole, with no clips.
#[test]
fn a_late_token_over_a_kept_frame_sends_it_whole() {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().no_dpms.push(A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    for _ in 0..2 {
        frame(&mut v, &[DOT]).unwrap();
        flip(&mut v);
    }
    v.card().state().commit_errors.push_back(Errno::INVAL);
    assert!(matches!(frame(&mut v, &[DOT]), Err(SwapError::Failed(_))));
    let mut token = v.lend(DisplayId(A)).unwrap();
    token.draw(|c| c.pixels.fill(7)).unwrap();
    v.set_dpms(DisplayId(A), Dpms::Off).unwrap();
    v.set_dpms(DisplayId(A), Dpms::On).unwrap();
    assert!(matches!(v.swap(token, &[DOT]), Err(SwapError::Dropped)));
    assert_eq!(v.commit(DisplayId(A)).unwrap(), Swapped::Frame);
    let c = v.card().last_commit().unwrap();
    assert_eq!(clips(&c, primary(&v, A)), None, "the whole screen");
}

// A token the owner drops after its display has gone frees the display's
// buffer at the card's next hotplug scan, with no display left to lend.
#[test]
fn a_buffer_retired_for_a_dropped_token_is_freed_at_the_next_scan() {
    let mut v = awake(one());
    let token = v.lend(DisplayId(A)).unwrap();
    v.card().state().connectors.get_mut(&A).unwrap().connected = false;
    v.poll().unwrap();
    assert!(!v.card().state().dumbs.is_empty(), "kept for the token");
    drop(token);
    v.poll().unwrap();
    assert!(v.card().state().dumbs.is_empty());
}
