#![expect(
    clippy::arithmetic_side_effects,
    clippy::disallowed_methods,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test's sizes are small, its clock starts at the host's, and a test fails by panicking"
)]
//! The clock against the real image store: each test feeds a terminal the
//! kitty commands a program sends, and passes the times in, counting turns.

use std::cell::Cell;

use vtium_term::images::{Kitty, Switches};
use vtium_term::{ImageString, Terminal};

use super::*;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// A terminal of 10 by 4 cells of 8 by 16 pixels that offers kitty
/// graphics.
fn term() -> Terminal {
    let mut t = Terminal::new(10, 4, 0);
    t.screen_mut().set_image_limit(1 << 26);
    t.screen_mut().set_image_cell(8, 16).unwrap();
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
    t
}

fn b64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `w` by `h` pixels of `color`, in base64.
fn fill(w: usize, h: usize, color: [u8; 4]) -> String {
    b64(&color.repeat(w * h))
}

/// Feeds the APC strings `cmds`, each `keys;payload`, with replies off.
fn feed(t: &mut Terminal, cmds: &[String]) {
    for c in cmds {
        let (keys, data) = c.split_once(';').unwrap_or((c, ""));
        let s = format!("\x1b_G{keys},q=2;{data}\x1b\\");
        let mut rest = s.as_bytes();
        while !rest.is_empty() {
            let n = t.input(rest);
            rest = &rest[n..];
        }
    }
}

/// Image 1, 2 by 2 pixels of red, put at the cursor, with frames 2 and 3,
/// green and blue, whose gaps are `gaps[1]` and `gaps[2]`, and the root
/// frame's gap `gaps[0]` unless it is 0, its default.
fn three_frames(t: &mut Terminal, gaps: [i32; 3]) -> ImageKey {
    let mut cmds = vec![format!("a=T,f=32,s=2,v=2,i=1;{}", fill(2, 2, RED))];
    for (color, gap) in [(GREEN, gaps[1]), (BLUE, gaps[2])] {
        cmds.push(format!(
            "a=f,i=1,f=32,s=2,v=2,z={gap};{}",
            fill(2, 2, color)
        ));
    }
    if gaps[0] != 0 {
        cmds.push(format!("a=a,i=1,r=1,z={}", gaps[0]));
    }
    feed(t, &cmds);
    t.screen().placements()[0].image
}

fn ms(t0: Instant, n: u64) -> Instant {
    t0 + Duration::from_millis(n)
}

/// The clock, its first turn at `t0` and the terminal's placements shown.
fn clock(t: &Terminal, t0: Instant) -> AnimationClock {
    let mut c = AnimationClock::default();
    c.turn(t.screen().images(), t0, true);
    c.show(&mut t.screen().placements(), t.screen().images());
    c
}

/// Turns the clock at each of `at`, ms after `t0`, and returns the frame
/// it shows after each and whether that turn asked for a draw.
fn turns(
    c: &mut AnimationClock,
    t: &Terminal,
    key: ImageKey,
    t0: Instant,
    at: &[u64],
) -> Vec<(u32, bool)> {
    at.iter()
        .map(|&n| {
            let drew = c.turn(t.screen().images(), ms(t0, n), true);
            (c.frame(key).unwrap(), drew)
        })
        .collect()
}

fn due(c: &AnimationClock, t: &Terminal) -> Option<Instant> {
    c.due(t.screen().images())
}

// D3, D4: a looping animation shows each frame for its gap and goes back
// to the first after the last; a turn before a frame's gap has passed
// changes nothing and draws nothing, and the timer is armed for the next
// change.
// covers: image.kitty-animation
#[test]
fn a_looping_animation_shows_each_frame_for_its_gap() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [30, 40, 50]);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    let mut c = clock(&t, t0);
    assert_eq!(c.frame(key), Some(1));
    assert_eq!(due(&c, &t), Some(ms(t0, 30)));
    let got = turns(&mut c, &t, key, t0, &[29, 30, 69, 70, 119, 120, 150]);
    assert_eq!(
        got,
        [
            (1, false),
            (2, true),
            (2, false),
            (3, true),
            (3, false),
            (1, true),
            (2, true)
        ]
    );
    assert_eq!(due(&c, &t), Some(ms(t0, 190)), "frame 2 shows for 40 ms");
}

// D4: `v=3` plays the frames twice, looping once, and stops on the last
// frame shown, arming no timer; `s=1` sets the loops played back, so `s=3`
// plays them twice again.
// covers: image.kitty-animation
#[test]
fn the_loops_run_out_and_the_animation_stops_on_its_last_frame() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [10, 10, 10]);
    feed(&mut t, &["a=a,i=1,s=3,v=3".into()]);
    let mut c = clock(&t, t0);
    let got = turns(&mut c, &t, key, t0, &[10, 20, 30, 40, 50, 60, 70]);
    let frames: Vec<u32> = got.iter().map(|g| g.0).collect();
    assert_eq!(frames, [2, 3, 1, 2, 3, 3, 3]);
    assert_eq!(due(&c, &t), None, "spent, it arms no timer");
    feed(&mut t, &["a=a,i=1,s=1".into()]);
    c.turn(t.screen().images(), ms(t0, 90), true);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    c.turn(t.screen().images(), ms(t0, 100), true);
    // From frame 3 it plays to the end, which is no loop, then twice more.
    let got = turns(&mut c, &t, key, t0, &[110, 120, 130, 140, 150, 160, 170]);
    let frames: Vec<u32> = got.iter().map(|g| g.0).collect();
    assert_eq!(frames, [1, 2, 3, 1, 2, 3, 3], "stopping set the loops back");
}

// D4, D3: an animation whose loops run out stops on its last frame shown,
// so a later `s=3` runs it again from there with the loops set back, and
// a later `c=1` shows frame 1, stopped.
// covers: image.kitty-animation
#[test]
fn an_animation_whose_loops_ran_out_is_stopped() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [10, 10, 10]);
    feed(&mut t, &["a=a,i=1,s=3,v=2".into()]);
    let mut c = clock(&t, t0);
    let frames = |got: Vec<(u32, bool)>| got.iter().map(|g| g.0).collect::<Vec<u32>>();
    let got = turns(&mut c, &t, key, t0, &[10, 20, 30, 40, 50]);
    assert_eq!(frames(got), [2, 3, 3, 3, 3], "v=2 plays the frames once");
    assert_eq!(due(&c, &t), None);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    c.turn(t.screen().images(), ms(t0, 100), true);
    let got = turns(&mut c, &t, key, t0, &[110, 120, 130, 140]);
    assert_eq!(
        frames(got),
        [1, 2, 3, 3],
        "s=3 plays them once more from frame 3"
    );
    feed(&mut t, &["a=a,i=1,c=1".into()]);
    let got = turns(&mut c, &t, key, t0, &[150, 160, 170, 500]);
    assert_eq!(frames(got), [1, 1, 1, 1], "c=1 shows frame 1, stopped");
    assert_eq!(due(&c, &t), None);
}

// D4: a gapless frame never shows while the animation plays; the root
// frame is gapless until a program sets its gap, so frames 2 and 3 take
// turns; an animation whose every frame is gapless shows its current frame
// and arms no timer.
// covers: image.kitty-animation
#[test]
fn a_gapless_frame_is_passed_over() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [30, -1, 50]);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    let mut c = clock(&t, t0);
    let got = turns(&mut c, &t, key, t0, &[30, 80, 110]);
    assert_eq!(got, [(3, true), (1, true), (3, true)]);

    let mut t = term();
    let key = three_frames(&mut t, [0, 40, 40]);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    let mut c = clock(&t, t0);
    let got = turns(&mut c, &t, key, t0, &[0, 40, 80, 120]);
    let frames: Vec<u32> = got.iter().map(|g| g.0).collect();
    assert_eq!(frames, [2, 3, 2, 3], "the root frame's gap is 0 by default");

    let mut t = term();
    let key = three_frames(&mut t, [-1, -1, -1]);
    feed(&mut t, &["a=a,i=1,c=2".into(), "a=a,i=1,s=3".into()]);
    let mut c = clock(&t, t0);
    assert_eq!(
        turns(&mut c, &t, key, t0, &[0, 500]),
        [(2, false), (2, false)]
    );
    assert_eq!(due(&c, &t), None);
}

// D3: an animation no command runs, or one stopped, keeps its frame and
// arms no timer however many turns pass.
// covers: image.kitty-animation
#[test]
fn a_stopped_animation_arms_no_timer() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [30, 40, 50]);
    let mut c = clock(&t, t0);
    assert_eq!(due(&c, &t), None, "never run");
    let got = turns(&mut c, &t, key, t0, &[0, 30, 1000]);
    assert_eq!(got, [(1, false); 3]);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    c.turn(t.screen().images(), ms(t0, 1000), true);
    assert_eq!(due(&c, &t), Some(ms(t0, 1030)), "run, its timer is armed");
    feed(&mut t, &["a=a,i=1,s=1".into()]);
    c.turn(t.screen().images(), ms(t0, 1010), true);
    assert_eq!(due(&c, &t), None, "stopped");
}

// D3: nothing animates, so the clock arms no timer: no image, or a still
// picture alone.
// covers: image.kitty-animation
#[test]
fn with_nothing_animating_the_clock_never_wakes_the_loop() {
    let t0 = Instant::now();
    let mut t = term();
    let mut c = clock(&t, t0);
    assert_eq!(due(&c, &t), None, "no picture");
    feed(
        &mut t,
        &[format!("a=T,f=32,s=2,v=2,i=1;{}", fill(2, 2, RED))],
    );
    c.show(&mut t.screen().placements(), t.screen().images());
    for n in [0, 10, 1000] {
        assert!(!c.turn(t.screen().images(), ms(t0, n), true));
        assert_eq!(due(&c, &t), None, "a still picture, at {n} ms");
    }
}

// D3: while its console is hidden, or the VT is another's, the clock arms
// no timer and moves no frame; shown again, it works out from the time
// where the animation has got to and shows that frame in one step, with
// its timer armed for the next change and none due at once.
// covers: image.kitty-animation
#[test]
fn hidden_the_clock_arms_no_timer_and_shown_again_it_skips_to_the_time() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [30, 40, 50]);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    let mut c = clock(&t, t0);
    for n in [0, 30, 5000, 10_000] {
        assert!(!c.turn(t.screen().images(), ms(t0, n), false));
        assert_eq!(due(&c, &t), None, "hidden at {n} ms");
        assert_eq!(c.frame(key), Some(1));
    }
    // 10,045 ms is 83 loops of 120 ms and 85 ms more: frame 3, from 10,030.
    assert!(c.turn(t.screen().images(), ms(t0, 10_045), true));
    assert_eq!(c.frame(key), Some(3));
    assert_eq!(due(&c, &t), Some(ms(t0, 10_080)));
    assert!(!c.turn(t.screen().images(), ms(t0, 10_046), true));
}

// D3's table: `c` shows the store's frame from that moment and keeps
// running; `s=1` keeps the frame on show; `s=3` while stopped runs from the
// frame on show; `s=2` while running keeps its place; `d=f` shows the root
// frame, stopped.
// covers: image.kitty-animation
#[test]
fn the_clock_answers_each_command_by_its_stamps() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [30, 40, 50]);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    let mut c = clock(&t, t0);
    feed(&mut t, &["a=a,i=1,c=3".into()]);
    assert_eq!(turns(&mut c, &t, key, t0, &[10]), [(3, true)]);
    assert_eq!(due(&c, &t), Some(ms(t0, 60)), "frame 3 from 10 ms");
    assert_eq!(turns(&mut c, &t, key, t0, &[60]), [(1, true)]);
    feed(&mut t, &["a=a,i=1,s=1".into()]);
    assert_eq!(turns(&mut c, &t, key, t0, &[70, 500]), [(1, false); 2]);
    assert_eq!(t.screen().images().animation(key).unwrap().current, 3);
    feed(&mut t, &["a=a,i=1,s=3".into()]);
    assert_eq!(
        turns(&mut c, &t, key, t0, &[500, 530]),
        [(1, false), (2, true)]
    );
    feed(&mut t, &["a=a,i=1,s=2".into()]);
    assert_eq!(
        turns(&mut c, &t, key, t0, &[540, 570, 620]),
        [(2, false), (3, true), (3, false)]
    );
    assert_eq!(due(&c, &t), None, "loading, it waits on its last frame");
    feed(&mut t, &["a=d,d=f,i=1".into()]);
    assert_eq!(turns(&mut c, &t, key, t0, &[630]), [(1, true)]);
    assert_eq!(due(&c, &t), None);
}

// D4: in loading mode an animation waits on its last frame, and a frame
// that arrives later shows from that turn, once the last frame's gap has
// passed.
// covers: image.kitty-animation
#[test]
fn a_loading_animation_goes_on_to_a_frame_that_arrives() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [30, 40, 50]);
    feed(&mut t, &["a=a,i=1,s=2".into()]);
    let mut c = clock(&t, t0);
    let got = turns(&mut c, &t, key, t0, &[30, 70, 120, 400]);
    assert_eq!(got, [(2, true), (3, true), (3, false), (3, false)]);
    feed(
        &mut t,
        &[format!("a=f,i=1,f=32,s=2,v=2,z=40;{}", fill(2, 2, WHITE))],
    );
    assert_eq!(turns(&mut c, &t, key, t0, &[410]), [(4, true)]);
    assert_eq!(due(&c, &t), None, "it waits on frame 4");
}

/// The store, counting the frames whose pixels it is asked for.
struct Counting<'a> {
    store: &'a Store,
    asked: Cell<usize>,
}

impl Frames for Counting<'_> {
    fn animation(&self, image: ImageKey) -> Option<Animation> {
        self.store.animation(image)
    }
    fn gap(&self, image: ImageKey, frame: u32) -> Option<i32> {
        self.store.frame_gap(image, frame)
    }
    fn pixels(&self, image: ImageKey, frame: u32) -> Option<FramePixels> {
        self.asked.set(self.asked.get() + 1);
        self.store.frame_pixels(image, frame)
    }
}

// D3: the snapshot shows the frame the clock chose, its pixels, number and
// pixel revision, on every placement of the image, asking the store for
// them once however many placements show it, and not at all while the
// store's entries already show that frame.
// covers: image.kitty-animation
#[test]
fn the_snapshot_shows_the_clocks_frame_asked_once_per_image() {
    let t0 = Instant::now();
    let mut t = term();
    let key = three_frames(&mut t, [30, 40, 50]);
    feed(&mut t, &["a=p,i=1,p=2".into(), "a=a,i=1,s=3".into()]);
    let mut c = clock(&t, t0);
    let counting = Counting {
        store: t.screen().images(),
        asked: Cell::new(0),
    };
    let mut shown = t.screen().placements();
    assert_eq!(shown.len(), 2, "two placements of image 1");
    c.show(&mut shown, &counting);
    assert_eq!(counting.asked.get(), 0, "the entries show frame 1");
    assert!(c.turn(t.screen().images(), ms(t0, 30), true));
    let mut shown = t.screen().placements();
    assert!(
        shown.iter().all(|p| p.frame == 1),
        "the store shows frame 1"
    );
    c.show(&mut shown, &counting);
    assert_eq!(counting.asked.get(), 1, "one ask for two placements");
    let want = t.screen().images().frame_pixels(key, 2).unwrap();
    for p in &shown {
        assert_eq!((p.frame, p.pixel_revision), (2, want.revision));
        assert_eq!(p.pixels.as_slice(), want.pixels.as_slice());
    }
}

// ADR-1102 D1: the store's composed cache keeps a frame it is asked for
// again, so a snapshot of an animation a program made current at a change
// frame, while the clock plays others, composes that frame once rather
// than at every snapshot.
// covers: image.kitty-animation
#[test]
fn the_stores_current_frame_is_composed_once_while_the_clock_plays_others() {
    let t0 = Instant::now();
    let mut t = term();
    // 200 by 200 pixels, past FRAME_WHOLE_MAX, with frames that each
    // change 10 by 10 pixels of the root, which stay change frames.
    let mut cmds = vec![format!("a=T,f=32,s=200,v=200,i=1;{}", fill(200, 200, RED))];
    for color in [GREEN, BLUE, WHITE, GREEN, BLUE] {
        cmds.push(format!(
            "a=f,i=1,c=1,f=32,s=10,v=10,z=10;{}",
            fill(10, 10, color)
        ));
    }
    cmds.extend([
        "a=a,i=1,r=1,z=10".into(),
        "a=a,i=1,c=2".into(),
        "a=a,i=1,s=3".into(),
    ]);
    feed(&mut t, &cmds);
    let mut c = clock(&t, t0);
    // The first composition, held so that no later one takes its memory.
    let first = t.screen().placements()[0].pixels.clone();
    let current = |t: &Terminal| t.screen().placements()[0].pixels.as_slice().as_ptr();
    for n in 1..=4 {
        assert!(c.turn(t.screen().images(), ms(t0, 10 * n), true));
        c.show(&mut t.screen().placements(), t.screen().images());
        assert_eq!(
            current(&t),
            first.as_slice().as_ptr(),
            "frame 2 composed again at snapshot {n}"
        );
    }
}
