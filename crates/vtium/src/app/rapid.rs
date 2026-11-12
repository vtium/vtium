//! The rapid timer in the loop: SGR 6's blink runs while a shown cell
//! carries it, at its own phase, and stops on what stops SGR 5's blink
//! (crates/vtium/src/blink.rs). The tests count the loop's turns at given
//! times, and never wait on the clock.
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

/// The rapid timer's next firing and whether its phase draws the text.
fn rapid(app: &App) -> (Option<Instant>, bool) {
    let blink = &app.term().unwrap().blink;
    (blink.rapid_due(), blink.rapid_shown())
}

/// Whether the shown head's next frame draws SGR 6's text in its off
/// phase.
fn hidden_in_frame(app: &App) -> bool {
    app.heads.first().unwrap().draw_state().rapid_hidden
}

// ECMA-48 §8.3.117: SGR 6 blinks at 150 a minute or more. The rapid timer
// flips every 200 ms, 150 blinks a minute, and the frame draws its phase.
// covers: sgr.rapid-blink
#[test]
fn rapid_blink_flips_every_200_ms_while_a_cell_carries_it() {
    let t = Instant::now();
    let ms = |n: u64| t + Duration::from_millis(n);
    let (_lp, mut app) = seat_at(&[], t);
    assert_eq!(rapid(&app), (None, true), "no SGR 6 cell, no timer");
    output(&mut app, b"\x1b[6mfast\x1b[0m \x1b[5mslow\x1b[0m", t);
    assert_eq!(rapid(&app), (Some(ms(200)), true));
    assert!(app.next_wake(t) <= Some(Duration::from_millis(200)));
    // a turn before the time changes nothing
    app.turn(ms(199));
    assert_eq!(rapid(&app), (Some(ms(200)), true));
    let mut flips = 0;
    let mut shown = true;
    for n in (200..=1000).step_by(200) {
        app.turn(ms(n));
        let (due, now_shown) = rapid(&app);
        assert_eq!(due, Some(ms(n + 200)), "armed again at {n} ms");
        assert_eq!(hidden_in_frame(&app), !now_shown, "the frame at {n} ms");
        flips += u32::from(now_shown != shown);
        shown = now_shown;
    }
    // five phases in a second: 2.5 blinks, 150 a minute
    assert_eq!(flips, 5);
    // SGR 5's text keeps its own 500 ms phase
    let blink = &app.term().unwrap().blink;
    assert!(
        !blink.text_shown(),
        "the text timer flipped once, at 500 ms"
    );
}

// The rapid timer stops when no cell carries SGR 6, its phase shown, and
// starts again when one does.
// covers: sgr.rapid-blink
#[test]
fn the_rapid_timer_stops_when_no_cell_carries_sgr_6() {
    let t = Instant::now();
    let ms = |n: u64| t + Duration::from_millis(n);
    let (_lp, mut app) = seat_at(&[], t);
    output(&mut app, b"\x1b[6mfast\x1b[0m", t);
    app.turn(ms(200));
    assert_eq!(rapid(&app), (Some(ms(400)), false));
    // the line is erased
    output(&mut app, b"\r\x1b[2K", ms(250));
    assert_eq!(rapid(&app), (None, true));
    assert!(!hidden_in_frame(&app));
    for n in [400, 600, 800] {
        app.turn(ms(n));
        assert_eq!(rapid(&app), (None, true), "stopped at {n} ms");
    }
    // only the 500 ms timers wake the loop now
    assert!(
        app.next_wake(ms(800))
            .is_some_and(|d| d > Duration::from_millis(200))
    );
    output(&mut app, b"\x1b[6magain", ms(900));
    assert_eq!(rapid(&app), (Some(ms(1100)), true));
}

// What stops SGR 5's blink stops SGR 6's: `blink` off, `blink-text=off`,
// and a VT that is another's.
// covers: sgr.rapid-blink
#[test]
fn what_stops_the_text_blink_stops_the_rapid_blink() {
    let t = Instant::now();
    let ms = |n: u64| t + Duration::from_millis(n);
    for options in [["--no-blink"], ["--blink-text=off"]] {
        let (_lp, mut app) = seat_at(&options, t);
        output(&mut app, b"\x1b[6mfast", t);
        for n in [0, 200, 400] {
            app.turn(ms(n));
            assert_eq!(rapid(&app), (None, true), "{options:?} at {n} ms");
        }
    }
    let (_lp, mut app) = seat_at(&[], t);
    output(&mut app, b"\x1b[6mfast", t);
    app.turn(ms(200));
    assert_eq!(rapid(&app), (Some(ms(400)), false));
    // the VT goes, and the display reports itself asleep
    app.vt.leave().unwrap();
    app.turn(ms(250));
    let still = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
    let still = still.as_any().downcast_mut::<Still>().unwrap();
    still.events.push(Event::Asleep);
    app.turn(ms(250));
    assert!(!app.awake, "the VT is let go");
    assert_eq!(rapid(&app), (None, true));
    app.turn(ms(400));
    assert_eq!(rapid(&app), (None, true), "no blink while away");
    // back on the VT, the timer starts again from the time it comes back
    app.vt.activate().unwrap();
    app.turn(ms(1000));
    assert_eq!(rapid(&app), (Some(ms(1200)), true));
}
