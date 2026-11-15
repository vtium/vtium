//! The cursor timer in the loop: the cursor blinks at the interval a
//! program set with the Linux console's `CSI 16 ; n ]`, from the phase that
//! starts when the sequence arrives, without a key. The tests count the
//! loop's turns at given times, and never wait on the clock.
#![expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

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
// milliseconds. The loop's cursor timer takes 250 ms from the turn the
// sequence arrives in, flips the cursor's phase at that pace, and returns
// to the `blink` key's 500 ms when the program sets no interval. The text
// timer keeps its 500 ms throughout.
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
    assert_eq!(cursor(&app), Some((ms(1200), ms_long(500))));
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
