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
use crate::display::Wanted;
use crate::display::fake::{Commit, FakeCard, logs};
use crate::display::mode::mode;

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

/// Draws a frame and swaps it.
fn frame(v: &mut Drm2d<FakeCard>, damage: &[Rect]) -> Result<(), SwapError> {
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
    frame(&mut v, &[]).unwrap();
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
    // An empty list is the whole frame: no clips.
    frame(&mut v, &[]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(clips(&c, plane), None);
    // Every damage blob was destroyed after its commit; the mode blob stays.
    assert_eq!(v.card().state().blobs.len(), 1);
}

#[test]
fn buffer_ages_follow_the_flips() {
    let mut v = awake(one());
    frame(&mut v, &[]).unwrap();
    flip(&mut v);
    assert_eq!(v.canvas(DisplayId(A)).unwrap().age, 0);
    frame(&mut v, &[]).unwrap();
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
    frame(&mut v, &[]).unwrap();
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
    assert!(matches!(frame(&mut v, &[]), Err(SwapError::Busy)));
    assert!(logs::at_least(Level::Warn).is_empty());
    frame(&mut v, &[]).unwrap();
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
    frame(&mut v, &[]).unwrap();
}

#[test]
fn a_power_cycle_restores_dpms() {
    let mut v = awake(one());
    v.set_dpms(DisplayId(A), Dpms::Standby).unwrap();
    v.card().state().dpms.insert(A, 0);
    v.sleep();
    flip(&mut v);
    v.wake_up().unwrap();
    assert_eq!(v.card().state().dpms[&A], 1);
}

// The cursor commits (aaronsb/kmscon#119).
#[test]
fn the_cursor_goes_with_the_next_commit() {
    let mut v = awake(one());
    frame(&mut v, &[]).unwrap();
    flip(&mut v);
    let cp = cursor_plane(&v, A);
    let image: Vec<u32> = (0..4).collect();
    v.setup_cursor(DisplayId(A), &image, 2, 2, (1, 2)).unwrap();
    assert!(!v.need_commit(DisplayId(A)));
    v.set_cursor_offset(DisplayId(A), 5, 5);
    v.show_cursor(DisplayId(A), 10, 20).unwrap();
    assert!(v.need_commit(DisplayId(A)));
    v.swap(DisplayId(A), &[rect(0, 0, 1, 1)]).unwrap();
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
    v.swap(DisplayId(A), &[rect(0, 0, 1, 1)]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(cp, "CRTC_X"), Some(signed(5 - 3 - 1)));
    flip(&mut v);
    v.hide_cursor(DisplayId(A));
    assert!(v.need_commit(DisplayId(A)));
    v.swap(DisplayId(A), &[rect(0, 0, 1, 1)]).unwrap();
    let c = v.card().last_commit().unwrap();
    assert_eq!(c.get(cp, "FB_ID"), Some(0));
    assert_eq!(c.get(cp, "CRTC_ID"), Some(0));
}

#[test]
fn a_failed_commit_keeps_the_cursor_waiting() {
    let mut v = awake(one());
    v.setup_cursor(DisplayId(A), &[0; 4], 2, 2, (0, 0)).unwrap();
    v.show_cursor(DisplayId(A), 1, 1).unwrap();
    v.card().state().commit_errors.push_back(Errno::INVAL);
    assert!(v.swap(DisplayId(A), &[]).is_err());
    assert!(v.need_commit(DisplayId(A)));
    v.swap(DisplayId(A), &[]).unwrap();
    assert!(!v.need_commit(DisplayId(A)));
}

#[test]
fn the_cursor_image_is_cut_to_the_plane() {
    let mut v = awake(one());
    let image = vec![0xff00_ff00u32; 100 * 100];
    v.setup_cursor(DisplayId(A), &image, 100, 100, (0, 0))
        .unwrap();
    let c = v.outputs[0].cursor.as_ref().unwrap();
    assert_eq!((c.width, c.height), (64, 64));
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
    frame(&mut v, &[]).unwrap();
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
    assert_eq!(
        events(&mut v),
        [Event::Redraw(DisplayId(A)), Event::Added(DisplayId(B))]
    );
}

#[test]
fn a_modeset_goes_ahead_when_the_flip_never_completes() {
    let card = FakeCard::new(&[
        (A, vec![mode(640, 480, 60, true)]),
        (B, vec![mode(800, 600, 60, true)]),
    ]);
    card.state().listed.retain(|&c| c == A);
    let mut v = awake(Drm2d::with(card, ModePolicy::default()).unwrap());
    frame(&mut v, &[]).unwrap();
    v.card().state().listed.push(B);
    v.poll().unwrap();
    // The flip's event is lost: the card forgets it.
    v.card().state().pending.clear();
    v.flip_wait_expired();
    assert_eq!(v.waits(), (false, false));
    assert!(v.card().last_commit().unwrap().mentions(B));
}

// The VT release waits on the flip as the modeset does.
#[test]
fn a_sleep_waits_for_the_pending_flip() {
    let mut v = awake(one());
    frame(&mut v, &[]).unwrap();
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
    frame(&mut v, &[]).unwrap();
    v.sleep();
    v.flip_wait_expired();
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
    v.swap(DisplayId(A), &[]).unwrap();
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

// A monitor's power cycle: the connector goes, and another comes, as an
// MST connector is replaced (aaronsb/kmscon#237).
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
    assert!(!v.card().state().master, "no display left: master dropped");
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

#[test]
fn a_connector_without_modes_is_no_display() {
    let card = FakeCard::new(&[(A, vec![])]);
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    v.wake_up().unwrap();
    assert_eq!(events(&mut v), [Event::Awake]);
    assert_eq!(v.displays().count(), 0);
    assert!(!v.card().state().master);
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
    frame(&mut app.0, &[]).unwrap();
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
