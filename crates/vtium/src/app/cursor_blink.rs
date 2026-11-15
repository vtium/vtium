//! The cursor timer in the loop: the cursor blinks at the interval a
//! program set with the Linux console's `CSI 16 ; n ]`, without a key. The
//! phase that is running when the sequence arrives keeps its end, or ends
//! within the new interval when that is shorter. The tests count the loop's
//! turns at given times, and never wait on the clock.
#![expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vtium_platform::display::Event;
use vtium_platform::event_loop::Loop;

use super::App;
use super::tests::{Still, no_fonts, seat_with, settle, shown};

/// The seat under `options` on one display, shown and settled at `t`.
fn seat_at(options: &[&str], t: Instant) -> (Loop<App>, App) {
    let options: Vec<&str> = ["--font-size=16"].iter().chain(options).copied().collect();
    let (mut lp, mut app) = seat_with(&options, None, no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    shown(&mut app);
    settle(&mut lp, &mut app, t);
    (lp, app)
}

/// `bytes` written to the shown console, as the pty's output is, and a
/// turn at `t`.
fn output(app: &mut App, bytes: &[u8], t: Instant) {
    let term = app.term_mut().unwrap();
    assert_eq!(term.terminal.input(bytes), bytes.len());
    term.changed = true;
    app.turn(t);
}

/// When the cursor timer next fires, and the length of its phases.
fn cursor(app: &App) -> Option<(Instant, Duration)> {
    app.heads.first().unwrap().cursor_due
}

// console_codes(4): `CSI 16 ; n ]` sets the cursor blink interval in
// milliseconds. The loop's cursor timer ends the running 500 ms phase
// within 250 ms of the turn the sequence arrives in, and flips the
// cursor's phase at that pace. When the program sets no interval, the
// running phase keeps its end and the phases after it last 500 ms again.
// The text timer keeps its 500 ms throughout.
// covers: ctl.linux-blink-interval
#[test]
fn the_cursor_blinks_at_the_interval_a_program_set_without_a_key() {
    let t = Instant::now();
    let ms = |n: u64| t + Duration::from_millis(n);
    let ms_long = Duration::from_millis;
    let (_lp, mut app) = seat_at(&[], t);
    let text_due = app.heads.first().unwrap().text_due;
    assert_eq!(cursor(&app).map(|(_, every)| every), Some(ms_long(500)));
    output(&mut app, b"\x1b[16;250]", ms(100));
    assert_eq!(cursor(&app), Some((ms(350), ms_long(250))));
    assert!(app.term().unwrap().blink.cursor_shown());
    app.turn(ms(350));
    assert!(!app.term().unwrap().blink.cursor_shown(), "the off phase");
    assert_eq!(cursor(&app), Some((ms(600), ms_long(250))));
    app.turn(ms(600));
    assert!(app.term().unwrap().blink.cursor_shown());
    assert_eq!(cursor(&app), Some((ms(850), ms_long(250))));
    // the text timer stays on its own 500 ms steps
    let step = |due: Option<Instant>| due.map(|d| d.duration_since(t).as_millis() % 500);
    assert_eq!(step(app.heads.first().unwrap().text_due), step(text_due));
    output(&mut app, b"\x1b[16]", ms(700));
    assert_eq!(cursor(&app), Some((ms(850), ms_long(500))));
    app.turn(ms(850));
    assert!(!app.term().unwrap().blink.cursor_shown(), "the off phase");
    assert_eq!(cursor(&app), Some((ms(1350), ms_long(500))));
}

// A console shown again keeps the interval its program set: the cursor
// timer is armed at that interval from the turn the VT comes back, with no
// key. The first phase lasts the whole interval, whether it is shorter or
// longer than the `blink` key's 500 ms.
// covers: ctl.linux-blink-interval
#[test]
fn the_cursor_takes_the_interval_again_when_the_vt_comes_back() {
    for every in [250, 1000] {
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        let (_lp, mut app) = seat_at(&[], t);
        output(&mut app, format!("\x1b[16;{every}]").as_bytes(), ms(100));
        // the VT goes, and the display reports itself asleep
        app.vt.leave().unwrap();
        app.turn(ms(150));
        let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
        let still = still.as_any().downcast_mut::<Still>().unwrap();
        still.events.push(Event::Asleep);
        app.turn(ms(150));
        assert!(!app.awake, "the VT is let go");
        assert_eq!(cursor(&app), None, "no blink while away");
        app.vt.activate().unwrap();
        app.turn(ms(2000));
        assert!(app.awake);
        assert_eq!(
            cursor(&app),
            Some((ms(2000 + every), Duration::from_millis(every))),
            "{every} ms"
        );
    }
}

// With `blink` off the cursor does not blink, whatever a program sets.
// covers: ctl.linux-blink-interval
#[test]
fn with_blink_off_the_interval_blinks_nothing() {
    let t = Instant::now();
    let ms = |n: u64| t + Duration::from_millis(n);
    let (_lp, mut app) = seat_at(&["--no-blink"], t);
    output(&mut app, b"\x1b[16;250]", ms(100));
    for n in [350, 600, 850] {
        app.turn(ms(n));
        assert_eq!(cursor(&app), None, "at {n} ms");
        assert!(app.term().unwrap().blink.cursor_shown(), "at {n} ms");
    }
}

// A new interval never restarts the phase that is running. A program that
// sets 250 ms and 260 ms in turn every 200 ms still sees the cursor blink,
// because each phase ends when it was due. A longer interval set during a
// phase leaves that phase's end where it was, and a shorter one ends the
// phase within the new interval.
// covers: ctl.linux-blink-interval
#[test]
fn a_new_interval_never_stretches_the_running_phase() {
    let t = Instant::now();
    let ms = |n: u64| t + Duration::from_millis(n);
    let (_lp, mut app) = seat_at(&[], t);
    output(&mut app, b"\x1b[16;250]", ms(100));
    let mut flips = 0;
    let mut shown = app.term().unwrap().blink.cursor_shown();
    for (n, every) in (1..=20).zip([b"260", b"250"].iter().cycle()) {
        let mut bytes = b"\x1b[16;".to_vec();
        bytes.extend_from_slice(*every);
        bytes.push(b']');
        output(&mut app, &bytes, ms(100 + 200 * n));
        let now = app.term().unwrap().blink.cursor_shown();
        flips += u32::from(now != shown);
        shown = now;
    }
    assert!(flips >= 10, "the cursor flipped {flips} times in 4 s");

    // a longer interval keeps the running phase's end
    let t = Instant::now();
    let ms = |n: u64| t + Duration::from_millis(n);
    let (_lp, mut app) = seat_at(&[], t);
    output(&mut app, b"\x1b[16;250]", ms(100));
    assert_eq!(cursor(&app), Some((ms(350), Duration::from_millis(250))));
    output(&mut app, b"\x1b[16;65535]", ms(200));
    assert_eq!(cursor(&app), Some((ms(350), Duration::from_millis(65535))));
    app.turn(ms(350));
    assert!(!app.term().unwrap().blink.cursor_shown(), "the off phase");
    // a shorter one ends the off phase within its own 100 ms
    output(&mut app, b"\x1b[16;100]", ms(400));
    assert_eq!(cursor(&app), Some((ms(500), Duration::from_millis(100))));
    app.turn(ms(500));
    assert!(app.term().unwrap().blink.cursor_shown());
}
