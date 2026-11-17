//! The `COLORTERM` a session's program is told, through the whole start of
//! a session: vtium reads each display's depth as it sets the display up,
//! and starts the program after that. The program is told
//! `COLORTERM=truecolor` when every display of its head shows color or
//! gray, and no `COLORTERM` when one of them is black and white.
//!
//! References:
//! - ADR-1301 D10: the rule, the displays that count, and the inherited
//!   value.
#![expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium_platform::display::DisplayId;
use vtium_platform::event_loop::Loop;
use vtium_platform::vt::FakeVt;

use super::App;
use super::tests::{Still, no_fonts, shown};

/// The script each session runs: it prints the `COLORTERM` it was given.
const SCRIPT: &str = "echo \"ct:${COLORTERM:-none}\"; read a";

/// The line the session printed, once it has, under `options`, with
/// vtium's own environment `env`, on one display that the device reports
/// as color.
fn told(options: &[&str], env: &[(&str, &str)]) -> Option<String> {
    started(options, env).2
}

/// The seat under `options` with vtium's own environment `env`, on one
/// display that the device reports as color, and the line the session
/// printed, once it has.
fn started(options: &[&str], env: &[(&str, &str)]) -> (Loop<App>, App, Option<String>) {
    let args: Vec<OsString> = ["--font-size=16"]
        .iter()
        .chain(options)
        .chain(&["--login", "--", "/bin/sh", "-c", SCRIPT])
        .map(OsString::from)
        .collect();
    let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
    let mut lp: Loop<App> = Loop::new().unwrap();
    let env = env
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect();
    let mut app = App::new(
        loaded.config,
        Box::new(FakeVt::new()),
        lp.handle(),
        Rc::new(Instant::now),
        env,
    );
    app.set_font_sources(no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    app.start();
    shown(&mut app);
    let line = |app: &App| {
        app.screen_text()
            .iter()
            .find_map(|row| row.find("ct:").map(|at| row[at..].trim_end().to_owned()))
    };
    for _ in 0..500 {
        if let Some(line) = line(&app) {
            return (lp, app, Some(line));
        }
        lp.dispatch(Some(Duration::from_millis(10)), &mut app)
            .unwrap();
        app.turn(Instant::now());
    }
    (lp, app, None)
}

// Each value of `display-depth` gives the program the `COLORTERM` that
// the display can show. At `auto` the device decides, and this one reports
// color.
//
// References:
// - ADR-1301 D10: the rule, and the depth of a display.
#[test]
fn colorterm_follows_display_depth() {
    let path = [("PATH", "/usr/bin:/bin")];
    for (options, want) in [
        (&[][..], "ct:truecolor"),
        (&["--display-depth=auto"][..], "ct:truecolor"),
        (&["--display-depth=color"][..], "ct:truecolor"),
        (&["--display-depth=gray4"][..], "ct:truecolor"),
        (&["--display-depth=gray16"][..], "ct:truecolor"),
        (&["--display-depth=1"][..], "ct:none"),
    ] {
        assert_eq!(told(options, &path).as_deref(), Some(want), "{options:?}");
    }
}

// With `reset-env` off, a session inherits vtium's own environment. On a
// black-and-white display vtium removes the `COLORTERM` it inherited, and
// on a color display it sets `truecolor` over what it inherited.
//
// References:
// - ADR-1301 D10: an inherited value.
#[test]
fn an_inherited_colorterm_never_says_more_than_the_display_shows() {
    let env = [("PATH", "/usr/bin:/bin"), ("COLORTERM", "truecolor")];
    assert_eq!(
        told(&["--no-reset-env", "--display-depth=1"], &env).as_deref(),
        Some("ct:none")
    );
    let env = [("PATH", "/usr/bin:/bin"), ("COLORTERM", "24bit")];
    assert_eq!(
        told(&["--no-reset-env"], &env).as_deref(),
        Some("ct:truecolor")
    );
}

// A black-and-white display that a device announces while a program told
// `truecolor` runs is named in the log, once vtium has set the display up
// on the program's head.
//
// References:
// - ADR-1301 D10: a display that joins or leaves later.
#[test]
fn a_black_and_white_display_that_joins_the_session_is_logged() {
    let (_lp, mut app, line) = started(&[], &[("PATH", "/usr/bin:/bin")]);
    assert_eq!(line.as_deref(), Some("ct:truecolor"));
    let named = |app: &App| app.heads.first().unwrap().black_and_white_named();
    assert!(named(&app).is_empty());
    let mut still = Still::new(&[(1, "B")]);
    still.levels = Some(2);
    app.add_video(PathBuf::from("/dev/mem1"), Box::new(still));
    shown(&mut app);
    let key = (PathBuf::from("/dev/mem1"), DisplayId(1));
    assert!(
        app.heads.first().unwrap().views.contains_key(&key),
        "set up"
    );
    assert_eq!(named(&app).into_iter().collect::<Vec<_>>(), [key]);
}
