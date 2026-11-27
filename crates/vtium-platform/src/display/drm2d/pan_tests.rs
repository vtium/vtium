//! drm2d's buffers taller than the screen against the stand-in card: the
//! window each frame commits as the plane's source rectangle, the test of
//! the pan at setup and its fallback, the card's largest framebuffer, the
//! buffers' ages across pans, and the window's reset at a sleep, a wake-up
//! and a hotplug. The last test runs in a build without `scroll-pan`, and
//! checks that the display makes buffers the screen's size there.
//!
//! References:
//! - ADR-1401 D2, "A buffer taller than the screen": what these tests hold.

#[cfg(feature = "scroll-pan")]
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod pan {

    use super::super::*;
    use crate::display::fake::{Commit, FakeCard};
    use crate::display::mode::mode;
    use crate::display::{Rect, SwapError, Swapped};

    const A: u32 = 41;

    /// A card with one display at 640x480, awake, its modeset's flip done.
    fn awake_one() -> Drm2d<FakeCard> {
        let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
        let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
        v.wake_up().unwrap();
        flip(&mut v);
        v.events().for_each(drop);
        v
    }

    fn flip(v: &mut Drm2d<FakeCard>) {
        v.card().complete_flips();
        v.dispatch().unwrap();
    }

    fn primary(v: &Drm2d<FakeCard>) -> u32 {
        v.outputs[0].planes.as_ref().unwrap().primary
    }

    /// The size of the display's buffers.
    fn buffer_size(v: &Drm2d<FakeCard>) -> (u32, u32) {
        v.outputs[0].buffers[0].size
    }

    /// Lends a token, moves its window to `window`, draws into it and swaps it
    /// with `damage`.
    fn pan(v: &mut Drm2d<FakeCard>, window: u32, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let mut token = v.lend(DisplayId(A)).expect("a token");
        assert!(
            token.set_window(window),
            "window {window} lies in the buffer"
        );
        token.draw(|c| c.pixels[0] = 1).expect("its memory");
        v.swap(token, damage)
    }

    /// Where the last commit put the primary plane's source rectangle, in
    /// pixels: `SRC_X`, `SRC_Y`, `SRC_W` and `SRC_H`.
    fn source(v: &Drm2d<FakeCard>) -> [u64; 4] {
        let c = v.card().last_commit().unwrap();
        ["SRC_X", "SRC_Y", "SRC_W", "SRC_H"].map(|n| c.get(primary(v), n).unwrap() >> 16)
    }

    /// The test-only commits since commit `from` that move the primary
    /// plane's source rectangle off the buffer's corner: the tests of a pan.
    fn pan_tests_since(v: &Drm2d<FakeCard>, from: usize) -> Vec<Commit> {
        let plane = primary(v);
        let moved = |c: &Commit| {
            ["SRC_X", "SRC_Y"]
                .iter()
                .any(|n| c.get(plane, n).is_some_and(|at| at != 0))
        };
        let s = v.card().state();
        s.commits[from..]
            .iter()
            .filter(|c| c.test_only() && moved(c))
            .cloned()
            .collect()
    }

    const DOWN: Tall = Tall {
        ring: 480,
        turned: false,
    };

    const ACROSS: Tall = Tall {
        ring: 640,
        turned: true,
    };

    /// The warnings that say a display cannot pan.
    fn pan_warnings() -> Vec<String> {
        crate::display::fake::logs::at_least(log::Level::Warn)
            .into_iter()
            .filter(|l| l.contains("cannot scroll by moving its window"))
            .collect()
    }

    // Each frame's commit carries the window its token was given as the
    // primary plane's SRC_Y, over a buffer the screen's height plus the ring.
    // The source rectangle stays the screen's size, and the damage is in the
    // buffer's coordinates, past the screen's bottom included.
    #[test]
    fn the_window_reaches_src_y() {
        let mut v = awake_one();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        assert_eq!(
            v.events().collect::<Vec<_>>(),
            [Event::Redraw(DisplayId(A))]
        );
        flip(&mut v);
        assert_eq!(buffer_size(&v), (640, 960));
        assert!(v.pans(DisplayId(A)));
        let mut token = v.lend(DisplayId(A)).unwrap();
        assert_eq!((token.size(), token.screen()), ((640, 960), (640, 480)));
        assert_eq!((token.window(), token.last_window()), (0, 480));
        assert!(!token.set_window(481), "past the buffer's end");
        v.give_back(token);
        // The modeset's frame goes whole, so the next one carries clips.
        pan(&mut v, 0, &[DOT]).unwrap();
        flip(&mut v);
        let low = Rect {
            x: 0,
            y: 900,
            width: 8,
            height: 8,
        };
        assert!(matches!(pan(&mut v, 96, &[low]), Ok(Swapped::Frame)));
        assert_eq!(source(&v), [0, 96, 640, 480]);
        let c = v.card().last_commit().unwrap();
        let blob = c.blob(primary(&v), "FB_DAMAGE_CLIPS").unwrap();
        assert_eq!(blob.len(), 16, "one clip");
        assert_eq!(i32::from_ne_bytes(blob[4..8].try_into().unwrap()), 900);
        flip(&mut v);
        let token = v.lend(DisplayId(A)).unwrap();
        assert_eq!(token.window(), 96, "the next token starts where it was");
        v.give_back(token);
    }

    // A token lent by a display with tall buffers cannot be told it is tall
    // again with a smaller screen, which would let its window run past the
    // buffer's end.
    #[test]
    fn a_lent_tall_token_cannot_be_made_taller() {
        let mut v = awake_one();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        let token = v.lend(DisplayId(A)).unwrap();
        let mut token = token.tall((640, 0), false);
        assert_eq!((token.screen(), token.last_window()), ((640, 480), 480));
        assert!(!token.set_window(960));
        v.give_back(token);
    }

    // The swap bounds the window by the display's own ring, whatever the
    // token says: a window past the ring's end is shown at the ring's end, so
    // the source rectangle never reads past the buffer.
    #[test]
    fn the_swap_keeps_the_window_inside_the_buffer() {
        let mut v = awake_one();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        let token = v.lend(DisplayId(A)).unwrap();
        let token = token.showing((640, 0), 960, false);
        assert_eq!(token.window(), 960);
        assert!(matches!(v.swap(token, &[DOT]), Ok(Swapped::Frame)));
        assert_eq!(source(&v), [0, 480, 640, 480]);
    }

    // On a display turned on its side the buffer is wider than the screen, and
    // the window moves across it through SRC_X.
    #[test]
    fn the_window_reaches_src_x_on_a_turned_display() {
        let mut v = awake_one();
        v.set_tall(DisplayId(A), Some(ACROSS)).unwrap();
        flip(&mut v);
        assert_eq!(buffer_size(&v), (1280, 480));
        let token = v.lend(DisplayId(A)).unwrap();
        assert!(token.turned());
        assert_eq!((token.screen(), token.last_window()), ((640, 480), 640));
        v.give_back(token);
        pan(&mut v, 200, &[]).unwrap();
        assert_eq!(source(&v), [200, 0, 640, 480]);
    }

    // The modeset that makes tall buffers tests the window at its furthest
    // start first, with a test-only commit, and then commits the window at the
    // buffer's start. A modeset that keeps those buffers tests nothing again.
    #[test]
    fn the_pan_is_tested_at_setup_and_only_once() {
        let mut v = awake_one();
        let from = v.card().state().commits.len();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        let plane = primary(&v);
        let tested: Vec<u64> = pan_tests_since(&v, from)
            .iter()
            .filter_map(|c| c.get(plane, "SRC_Y"))
            .collect();
        assert_eq!(tested, [480 << 16]);
        assert_eq!(source(&v), [0, 0, 640, 480]);
        flip(&mut v);
        let from = v.card().state().commits.len();
        v.sleep();
        flip(&mut v);
        v.wake_up().unwrap();
        assert_eq!(pan_tests_since(&v, from).len(), 0);
    }

    // A card that refuses the pan's test commit leaves the display with
    // buffers the screen's size: its tokens report the screen's size for both,
    // and their window cannot move. The refusal is logged once, and a later
    // modeset neither tests it again nor logs it again.
    #[test]
    fn a_refused_test_commit_falls_back_and_logs_once() {
        let mut v = awake_one();
        v.card().state().src_offset_refused = true;
        crate::display::fake::logs::capture();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        assert_eq!(buffer_size(&v), (640, 480));
        assert!(!v.pans(DisplayId(A)));
        let mut token = v.lend(DisplayId(A)).unwrap();
        assert_eq!((token.size(), token.screen()), ((640, 480), (640, 480)));
        assert!(!token.set_window(1));
        v.give_back(token);
        assert_eq!(pan_warnings().len(), 1, "{:?}", pan_warnings());
        let from = v.card().state().commits.len();
        v.sleep();
        flip(&mut v);
        v.wake_up().unwrap();
        flip(&mut v);
        assert_eq!(pan_tests_since(&v, from).len(), 0, "no pan tested again");
        assert_eq!(pan_warnings().len(), 1);
        assert_eq!(buffer_size(&v), (640, 480));
    }

    /// The framebuffer the screen of `v`'s display shows now.
    fn shown_fb(v: &Drm2d<FakeCard>) -> u32 {
        let o = &v.outputs[0];
        o.buffers[o.front()].fb
    }

    // A card whose planes show no framebuffer taller than the screen refuses
    // the modeset with the tall buffers whatever the window, so the test of
    // the pan cannot blame the window. The display gives the tall buffers up,
    // logs that once, and sets its mode with buffers the screen's size. A
    // later wake-up neither tries them again nor logs again.
    #[test]
    fn a_card_that_refuses_tall_framebuffers_falls_back_and_logs_once() {
        let mut v = awake_one();
        v.card().state().scanout_most = (16_384, 480);
        crate::display::fake::logs::capture();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        assert_eq!(buffer_size(&v), (640, 480));
        assert!(!v.pans(DisplayId(A)));
        assert_eq!(pan_warnings().len(), 1, "{:?}", pan_warnings());
        v.sleep();
        flip(&mut v);
        v.wake_up().unwrap();
        flip(&mut v);
        assert_eq!(buffer_size(&v), (640, 480));
        assert!(v.card().shows(v.outputs[0].crtc, shown_fb(&v)));
        assert_eq!(pan_warnings().len(), 1, "{:?}", pan_warnings());
    }

    // Tall buffers asked for during a sleep are made at the wake-up. On a card
    // whose planes show no framebuffer taller than the screen, the wake-up
    // still lights the display, with buffers the screen's size, and logs once.
    #[test]
    fn a_wake_up_on_a_card_that_refuses_tall_framebuffers_lights_the_screen() {
        let mut v = awake_one();
        v.card().state().scanout_most = (16_384, 480);
        v.sleep();
        flip(&mut v);
        crate::display::fake::logs::capture();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        v.wake_up().unwrap();
        flip(&mut v);
        assert_eq!(buffer_size(&v), (640, 480));
        assert!(!v.pans(DisplayId(A)));
        assert!(v.card().shows(v.outputs[0].crtc, shown_fb(&v)));
        assert_eq!(pan_warnings().len(), 1, "{:?}", pan_warnings());
    }

    // Where the modeset fails with buffers the screen's size too, the tall
    // buffers are not to blame: the wake-up fails as it would without them,
    // nothing is logged about the pan, and once the card takes the modeset the
    // display makes its tall buffers and pans.
    #[test]
    fn a_modeset_that_fails_without_tall_buffers_too_keeps_the_ask() {
        let mut v = awake_one();
        v.sleep();
        flip(&mut v);
        v.card().state().scanout_most = (16_384, 479);
        crate::display::fake::logs::capture();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        assert!(v.wake_up().is_err());
        assert_eq!(pan_warnings(), Vec::<String>::new());
        v.card().state().scanout_most = (0, 0);
        v.sleep();
        v.wake_up().unwrap();
        flip(&mut v);
        assert_eq!(buffer_size(&v), (640, 960));
        assert!(v.pans(DisplayId(A)));
    }

    // A display moved to another CRTC shows its buffers through another
    // primary plane, so the next modeset tests the pan again there, and a
    // refusal on the old plane does not hold on the new one. Here A, lit on
    // CRTC 100 with its pan refused, moves to 101 when B is plugged in, and
    // the hotplug's modeset tests the pan on 101's primary plane, which takes
    // it.
    #[test]
    fn a_display_moved_to_another_crtc_tests_its_pan_again() {
        const B: u32 = 42;
        let card = FakeCard::new(&[
            (A, vec![mode(640, 480, 60, true)]),
            (B, vec![mode(640, 480, 60, true)]),
        ]);
        {
            let mut s = card.state();
            s.encoders.get_mut(&200).unwrap().possible_crtcs = 0b11;
            s.encoders.get_mut(&201).unwrap().possible_crtcs = 0b01;
            s.listed.retain(|&c| c == A);
            s.src_offset_refused = true;
        }
        let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
        v.wake_up().unwrap();
        flip(&mut v);
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        assert!(!v.pans(DisplayId(A)), "the pan is refused on CRTC 100");
        let before = primary(&v);
        {
            let mut s = v.card().state();
            s.src_offset_refused = false;
            s.listed.push(B);
        }
        let from = v.card().state().commits.len();
        v.poll().unwrap();
        flip(&mut v);
        assert_eq!(v.outputs[0].crtc, 101);
        assert_ne!(primary(&v), before);
        let tested = pan_tests_since(&v, from);
        assert_eq!(tested.len(), 1, "one test of the pan on the new plane");
        assert!(v.pans(DisplayId(A)));
        assert_eq!(buffer_size(&v), (640, 960));
    }

    // A display moved to another CRTC tests a pan proven on its old plane
    // again on the new one.
    #[test]
    fn a_display_moved_to_another_crtc_tests_a_proven_pan_again() {
        const B: u32 = 42;
        let card = FakeCard::new(&[
            (A, vec![mode(640, 480, 60, true)]),
            (B, vec![mode(640, 480, 60, true)]),
        ]);
        {
            let mut s = card.state();
            s.encoders.get_mut(&200).unwrap().possible_crtcs = 0b11;
            s.encoders.get_mut(&201).unwrap().possible_crtcs = 0b01;
            s.listed.retain(|&c| c == A);
        }
        let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
        v.wake_up().unwrap();
        flip(&mut v);
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        assert!(v.pans(DisplayId(A)));
        v.card().state().listed.push(B);
        let from = v.card().state().commits.len();
        v.poll().unwrap();
        flip(&mut v);
        assert_eq!(v.outputs[0].crtc, 101);
        let plane = primary(&v);
        let tested: Vec<u64> = pan_tests_since(&v, from)
            .iter()
            .filter_map(|c| c.get(plane, "SRC_Y"))
            .collect();
        assert_eq!(tested, [480 << 16], "the pan is tested on the new plane");
        assert!(v.pans(DisplayId(A)));
    }

    // Tall buffers past the card's largest framebuffer are refused before any
    // is made, down and across alike: the display keeps the buffers it has,
    // makes no dumb buffer, and logs why once.
    #[test]
    fn a_buffer_past_the_largest_framebuffer_is_refused_before_allocation() {
        for (tall, most) in [(DOWN, (16_384, 959)), (ACROSS, (1279, 16_384))] {
            let mut v = awake_one();
            v.card().state().max_fb = most;
            // The display reads the largest framebuffer at its next scan.
            v.sleep();
            flip(&mut v);
            v.wake_up().unwrap();
            flip(&mut v);
            crate::display::fake::logs::capture();
            let from = v.card().state().buffer_calls.len();
            v.set_tall(DisplayId(A), Some(tall)).unwrap();
            flip(&mut v);
            let made = v.card().state().buffer_calls[from..]
                .iter()
                .filter(|c| matches!(c, crate::display::fake::BufferCall::CreateDumb(_)))
                .count();
            assert_eq!(made, 0, "{tall:?}");
            assert_eq!(buffer_size(&v), (640, 480));
            assert!(!v.pans(DisplayId(A)));
            assert_eq!(pan_warnings().len(), 1, "{:?}", pan_warnings());
        }
    }

    // A pan changes which part of the buffer is shown and draws nothing, so
    // each token's age counts frames as it does without one. A frame that only
    // moves the window, with no damage, is still presented.
    #[test]
    fn ages_hold_across_pans() {
        let mut v = awake_one();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        let mut ages = Vec::new();
        for window in [0, 24, 48, 72, 96] {
            let token = v.lend(DisplayId(A)).unwrap();
            ages.push(token.age());
            v.give_back(token);
            let damage: &[Rect] = if window == 48 { &[] } else { &[DOT] };
            assert!(
                matches!(pan(&mut v, window, damage), Ok(Swapped::Frame)),
                "{window}"
            );
            assert_eq!(source(&v)[1], u64::from(window));
            flip(&mut v);
        }
        assert_eq!(ages, [0, 0, 2, 2, 2]);
    }

    const DOT: Rect = Rect {
        x: 0,
        y: 0,
        width: 8,
        height: 8,
    };

    /// Moves the window to 96 and shows it, so the display holds a window away
    /// from the buffer's start.
    fn moved() -> Drm2d<FakeCard> {
        let mut v = awake_one();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        for _ in 0..2 {
            pan(&mut v, 96, &[DOT]).unwrap();
            flip(&mut v);
        }
        v.events().for_each(drop);
        v
    }

    /// The window and age of the token the display lends now.
    fn window_and_age(v: &mut Drm2d<FakeCard>) -> (u32, u32) {
        let token = v.lend(DisplayId(A)).unwrap();
        let got = (token.window(), token.age());
        v.give_back(token);
        got
    }

    // A sleep and the wake-up after it put the window back at the buffer's
    // start: the wake-up's modeset shows the start, and the next token starts
    // there with every buffer at age 0, so the owner draws it whole.
    #[test]
    fn a_sleep_and_a_wake_up_reset_the_window() {
        let mut v = moved();
        assert_eq!(window_and_age(&mut v), (96, 2));
        v.sleep();
        flip(&mut v);
        v.wake_up().unwrap();
        assert_eq!(source(&v), [0, 0, 640, 480]);
        assert!(v.events().any(|e| e == Event::Redraw(DisplayId(A))));
        flip(&mut v);
        assert_eq!(window_and_age(&mut v), (0, 0));
    }

    // A hotplug's modeset keeps a display's buffers when its mode does not
    // change, and still puts its window back at the start: the next token
    // starts there at age 0, and the owner is told to redraw.
    #[test]
    fn a_hotplug_resets_the_window() {
        const B: u32 = 42;
        let card = FakeCard::new(&[
            (A, vec![mode(640, 480, 60, true)]),
            (B, vec![mode(800, 600, 60, true)]),
        ]);
        card.state().listed.retain(|&c| c == A);
        let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
        v.wake_up().unwrap();
        flip(&mut v);
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        for _ in 0..2 {
            pan(&mut v, 96, &[DOT]).unwrap();
            flip(&mut v);
        }
        v.events().for_each(drop);
        let held: Vec<u32> = v.outputs[0].buffers.iter().map(|b| b.fb).collect();
        v.card().state().listed.push(B);
        v.poll().unwrap();
        let a = v.outputs.iter().find(|o| o.id == DisplayId(A)).unwrap();
        assert_eq!(a.buffers.iter().map(|b| b.fb).collect::<Vec<_>>(), held);
        let c = v.card().last_commit().unwrap();
        let plane = a.planes.as_ref().unwrap().primary;
        assert_eq!(c.get(plane, "SRC_Y"), Some(0));
        let events: Vec<Event> = v.events().collect();
        assert!(events.contains(&Event::Redraw(DisplayId(A))), "{events:?}");
        flip(&mut v);
        assert_eq!(window_and_age(&mut v), (0, 0));
    }

    // The legacy calls cannot move the source rectangle without a modeset, so a
    // card driven through them keeps buffers the screen's size when asked for
    // tall ones, and says so once.
    #[test]
    fn the_legacy_calls_never_pan() {
        let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
        card.state().atomic = false;
        let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
        v.wake_up().unwrap();
        flip(&mut v);
        crate::display::fake::logs::capture();
        v.set_tall(DisplayId(A), Some(DOWN)).unwrap();
        flip(&mut v);
        assert_eq!(buffer_size(&v), (640, 480));
        assert!(!v.pans(DisplayId(A)));
        let token = v.lend(DisplayId(A)).unwrap();
        assert_eq!(token.last_window(), 0);
        v.give_back(token);
        assert_eq!(pan_warnings().len(), 1, "{:?}", pan_warnings());
    }

    // Asking for buffers the screen's size again gives them back, and the
    // commits put the source rectangle at the buffer's corner.
    #[test]
    fn asking_for_no_tall_buffer_gives_screen_sized_ones_again() {
        let mut v = moved();
        v.set_tall(DisplayId(A), None).unwrap();
        flip(&mut v);
        assert_eq!(buffer_size(&v), (640, 480));
        assert!(!v.pans(DisplayId(A)));
        let token = v.lend(DisplayId(A)).unwrap();
        assert_eq!((token.size(), token.window()), ((640, 480), 0));
        v.give_back(token);
    }
}

// A build without `scroll-pan` makes every buffer the screen's size, and
// each frame's commit shows it from the buffer's corner.
#[cfg(not(feature = "scroll-pan"))]
#[test]
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test fails by panicking"
)]
fn a_build_without_scroll_pan_makes_screen_sized_buffers() {
    use super::*;
    use crate::display::Rect;
    use crate::display::fake::FakeCard;
    use crate::display::mode::mode;
    let card = FakeCard::new(&[(41, vec![mode(640, 480, 60, true)])]);
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    v.wake_up().unwrap();
    v.card().complete_flips();
    v.dispatch().unwrap();
    assert!(v.outputs[0].buffers.iter().all(|b| b.size == (640, 480)));
    let mut token = v.lend(DisplayId(41)).unwrap();
    assert_eq!((token.size(), token.screen()), ((640, 480), (640, 480)));
    token
        .draw(|c| assert_eq!((c.height, c.screen), (480, (640, 480))))
        .unwrap();
    let dot = Rect {
        x: 0,
        y: 0,
        width: 8,
        height: 8,
    };
    v.swap(token, &[dot]).unwrap();
    let c = v.card().last_commit().unwrap();
    let plane = v.outputs[0].planes.as_ref().unwrap().primary;
    let src = ["SRC_X", "SRC_Y", "SRC_W", "SRC_H"].map(|n| c.get(plane, n).unwrap() >> 16);
    assert_eq!(src, [0, 0, 640, 480]);
}
