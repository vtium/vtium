//! The power keys through the seat's key order: a bound chord acts once a
//! press, through a stand-in for PID 1, logind and the kernel; an unbound
//! one, a repeat and a key while the VT is not ours act on nothing and
//! pass on as any key does.

use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use vtium::app::App;
use vtium::config;
use vtium::grab::Power;
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::FakeVt;

const DELETE: u32 = 0xffff;
const END: u32 = 0xff57;
const S: u32 = 0x73;

/// A stand-in for PID 1, logind and `reboot(2)`: it records each call,
/// and reads `/proc/sys/kernel/ctrl-alt-del` as 0.
#[derive(Clone, Default)]
struct Host(Rc<RefCell<Vec<&'static str>>>);

impl Power for Host {
    fn ctrl_alt_del(&mut self) -> io::Result<String> {
        Ok("0\n".to_owned())
    }
    fn interrupt_init(&mut self) -> io::Result<()> {
        self.0.borrow_mut().push("SIGINT to PID 1");
        Ok(())
    }
    fn power_off_init(&mut self) -> io::Result<()> {
        self.0.borrow_mut().push("SIGRTMIN+4 to PID 1");
        Ok(())
    }
    fn restart_kernel(&mut self) -> io::Result<()> {
        self.0.borrow_mut().push("sync, reboot(RB_AUTOBOOT)");
        Ok(())
    }
    fn suspend(&mut self) -> io::Result<()> {
        self.0.borrow_mut().push("logind Suspend");
        Ok(())
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the test runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

/// The seat on the fake VT, shown and awake, with no display, no child,
/// and its power keys acting on `host`.
fn seat(args: &[&str], host: &Host) -> (Loop<App>, App) {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let loaded = config::load(&args, None, &mut |_: &Path| Ok(None));
    assert_eq!(loaded.reports, [], "the command line");
    let lp: Loop<App> = Loop::new().expect("a loop");
    let mut app = App::new(
        loaded.config,
        Box::new(FakeVt::new()),
        lp.handle(),
        Rc::new(now),
        Vec::new(),
    );
    app.set_power(Box::new(host.clone()));
    app.vt.activate().expect("the fake VT");
    app.turn(now());
    (lp, app)
}

fn key(mods: Mods, keysym: u32, repeat: bool) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        mods,
        keysyms: vec![keysym],
        codepoints: vec![None],
        repeat,
    })
}

fn ctrl_alt() -> Mods {
    Mods::CTRL.union(Mods::ALT)
}

#[test]
fn a_bound_chord_acts_once_a_press() {
    let host = Host::default();
    let (_lp, mut app) = seat(
        &[
            "--grab-reboot=<Ctrl><Alt>Delete",
            "--grab-power-off=<Ctrl><Alt>End",
            "--grab-suspend=<Logo>s",
        ],
        &host,
    );
    app.input_event(key(ctrl_alt(), DELETE, false), now());
    // A held chord's repeats ask for nothing more.
    for _ in 0..10 {
        app.input_event(key(ctrl_alt(), DELETE, true), now());
    }
    app.input_event(key(ctrl_alt(), END, false), now());
    app.input_event(key(Mods::LOGO, S, false), now());
    // Other modifiers held: no match.
    app.input_event(key(ctrl_alt().union(Mods::SHIFT), DELETE, false), now());
    assert_eq!(
        *host.0.borrow(),
        ["SIGINT to PID 1", "SIGRTMIN+4 to PID 1", "logind Suspend"]
    );
}

#[test]
fn an_unbound_chord_does_nothing() {
    let host = Host::default();
    let (_lp, mut app) = seat(&[], &host);
    for (mods, keysym) in [(ctrl_alt(), DELETE), (ctrl_alt(), END), (Mods::LOGO, S)] {
        app.input_event(key(mods, keysym, false), now());
    }
    assert_eq!(*host.0.borrow(), [] as [&str; 0]);
}

#[test]
fn a_chord_while_the_vt_is_not_ours_does_nothing() {
    let host = Host::default();
    let (_lp, mut app) = seat(&["--grab-power-off=<Ctrl><Alt>End"], &host);
    // The VT is given up: with no device to wait for, the release is
    // acknowledged at once.
    app.vt.leave().expect("the fake VT");
    app.turn(now());
    assert!(!app.vt.is_active());
    app.input_event(key(ctrl_alt(), END, false), now());
    assert_eq!(*host.0.borrow(), [] as [&str; 0]);
}
