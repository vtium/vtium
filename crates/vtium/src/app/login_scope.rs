//! The tests of a session's own systemd scope on a seat that libseat's
//! logind backend opened (ADR-1400 D3, its note on each login's own
//! session), with the scope call stood in for.
#![expect(clippy::unwrap_used, reason = "a test's set-up fails loudly")]
#![expect(clippy::indexing_slicing, reason = "a test indexes the heads it built")]
#![expect(
    clippy::disallowed_methods,
    reason = "the loop's clock in a test, as the seat's tests read it"
)]

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium_platform::event_loop::Loop;
use vtium_platform::vt::FakeVt;

use super::*;
use crate::app::tests::{Still, no_fonts, shown};

/// What the stand-in for the scope call saw: the place and the child's
/// process id. vtium-platform's tests show that the child waits before its
/// exec until the call returns.
#[derive(Debug, PartialEq, Eq)]
struct Asked {
    place: String,
    pid: u32,
}

type Seen = Rc<RefCell<Vec<Asked>>>;

/// The scope call stood in for: it records what it was asked, and answers
/// `answer`. It refuses a place that would make no valid unit name, as
/// systemd refuses one (systemd.unit(5): letters, digits and `:_.\-`).
fn scope(seen: &Seen, answer: Result<(), &'static str>) -> LoginScope {
    let seen = Rc::clone(seen);
    Box::new(move |place: &str, pid: u32| {
        seen.borrow_mut().push(Asked {
            place: place.to_owned(),
            pid,
        });
        let unit = format!("vtium-login-{place}-{pid}.scope");
        if !unit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ":_.\\-".contains(c))
        {
            return Err(format!(
                "systemd answered org.freedesktop.DBus.Error.InvalidArgs: Invalid unit name {unit}"
            ));
        }
        answer.map(|()| unit).map_err(str::to_owned)
    })
}

/// vtium's environment under its unit on the libseat build, with its PAM
/// session's variables.
fn in_its_session() -> Vec<(OsString, OsString)> {
    [
        ("PATH", "/usr/bin:/bin"),
        ("XDG_SEAT", "seat0"),
        ("XDG_VTNR", "2"),
        ("XDG_SESSION_ID", "c1"),
        ("XDG_SESSION_CLASS", "greeter"),
    ]
    .iter()
    .map(|(k, v)| (OsString::from(k), OsString::from(v)))
    .collect()
}

/// What the session prints: the seat, VT and session variables it got,
/// then its own process id.
const SCRIPT: &str = "echo \"env:$XDG_SEAT:$XDG_VTNR:$XDG_SESSION_ID:$XDG_SESSION_CLASS:end\"; echo pid:$$; while read a; do :; done";

/// A seat on `vt` whose session runs [`SCRIPT`] with vtium's environment
/// kept, and whose scope call is `login_scope`.
fn seat(vt: FakeVt, login_scope: LoginScope) -> (Loop<App>, App) {
    let args: Vec<OsString> = [
        "--font-size=16",
        "--no-reset-env",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        SCRIPT,
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
    let lp: Loop<App> = Loop::new().unwrap();
    let mut app = App::new(
        loaded.config,
        Box::new(vt),
        lp.handle(),
        Rc::new(Instant::now),
        in_its_session(),
    );
    app.login_scope = login_scope;
    app.set_font_sources(no_fonts());
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    app.start();
    shown(&mut app);
    (lp, app)
}

/// The screen's row that starts with `prefix`, once the session printed
/// it, at most 500 turns of up to 10 ms.
fn row(lp: &mut Loop<App>, app: &mut App, prefix: &str) -> Option<String> {
    for _ in 0..500 {
        let found = app
            .screen_text()
            .into_iter()
            .find(|r| r.trim_start().starts_with(prefix));
        if let Some(found) = found {
            return Some(found.trim().to_owned());
        }
        lp.dispatch(Some(Duration::from_millis(10)), app).unwrap();
        app.turn(Instant::now());
    }
    None
}

/// The session's leader, ended and reaped.
fn end(app: &App) {
    if let Some(pid) = app.heads()[0].pty().and_then(|p| p.child_id()) {
        let pid = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap()).unwrap();
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        let _ = rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::empty());
    }
}

// On the logind seat, the session's child is moved into its scope by its
// process id, and it starts with no seat, no VT number and none of vtium's
// own session's variables.
#[test]
fn on_the_logind_seat_the_sessions_child_is_moved_into_its_scope() {
    let seen = Seen::default();
    let (mut lp, mut app) = seat(FakeVt::on_logind_seat(2), scope(&seen, Ok(())));
    let env = row(&mut lp, &mut app, "env:");
    let pid = row(&mut lp, &mut app, "pid:");
    let leader = app.heads()[0].pty().and_then(|p| p.child_id());
    end(&app);
    let leader = leader.unwrap();
    assert_eq!(
        *seen.borrow(),
        [Asked {
            place: "tty2".to_owned(),
            pid: leader,
        }],
        "one scope call, on the child's id"
    );
    assert_eq!(
        pid,
        Some(format!("pid:{leader}")),
        "the child that ran is the one moved"
    );
    assert_eq!(env.as_deref(), Some("env:::::end"));
}

// A failed scope call starts the session all the same, inside vtium's own
// logind session, still with no seat and none of that session's
// variables; the log says why.
#[test]
fn a_failed_scope_call_starts_the_session_all_the_same() {
    let seen = Seen::default();
    let (mut lp, mut app) = seat(
        FakeVt::on_logind_seat(2),
        scope(&seen, Err("systemd is not the init process")),
    );
    let env = row(&mut lp, &mut app, "env:");
    end(&app);
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(env.as_deref(), Some("env:::::end"));
    assert_eq!(
        no_scope("tty2", "systemd is not the init process"),
        "The session on tty2 gets no logind session of its own: vtium cannot move it into a systemd scope, because systemd is not the init process. The session runs inside vtium's own logind session instead."
    );
    assert_eq!(
        in_scope("tty2", "vtium-login-tty2-7.scope"),
        "The session on tty2 runs in the systemd scope vtium-login-tty2-7.scope."
    );
}

// Off the logind seat, as on the kernel's VT, the default build's seat and
// seatd's, nothing asks for a scope, and the session keeps the seat, the VT
// number and what vtium inherited.
#[test]
fn off_the_logind_seat_nothing_asks_for_a_scope() {
    let seen = Seen::default();
    let (mut lp, mut app) = seat(FakeVt::with_number(2), scope(&seen, Ok(())));
    let env = row(&mut lp, &mut app, "env:");
    end(&app);
    assert_eq!(*seen.borrow(), []);
    assert_eq!(env.as_deref(), Some("env:seat0:2:c1:greeter:end"));
}
