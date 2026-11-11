//! The seat through libseat against the stand-in: the seat and its devices
//! retried from the loop's timers while the loop serves signals and timers,
//! the enable and the disable, the switch keys, and what a drop puts back.

#![expect(clippy::panic, reason = "a test fails by panicking")]

use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use calloop::timer::{TimeoutAction, Timer};
use rustix::io::Errno;
use xkbcommon::xkb::keysyms;

use std::cell::RefCell;
use std::os::unix::fs::MetadataExt;
use std::rc::Rc;
use std::time::Instant;

use super::stand_in::{Client, StandIn, StandInSeat};
use super::{Chosen, LibseatVt, SeatInput, Told, Tries};
use crate::event_loop::{Loop, Signal};
use crate::input::fake::{self, FakeDevice, Node, Nodes};
use crate::input::{DeviceId, Input, InputConfig, InputEvent, Keymap, RawEvent};
use crate::input::{KeyEvent, Mods};
use crate::ioctl::EV_KEY;
use crate::vt::fake::{Call, FakeConsole};
use crate::vt::{DisplayMode, KbMode, Switch, Vt, VtEvent};

type Seated = LibseatVt<StandIn, FakeConsole>;

/// The loop's data: the VT, and what the test's own sources saw.
struct Data {
    vt: Option<Seated>,
    server: StandIn,
    /// The opens of the seat asked when the test's timer fired.
    timer_at: Option<usize>,
    /// The opens of the seat asked when SIGTERM was served.
    sigterm_at: Option<usize>,
    /// The tries at the device when the test's timer fired.
    device_timer_at: Option<usize>,
    /// The keyboards and pointers, opened through the seat.
    input: Option<Input<Nodes>>,
}

fn access(data: &mut Data) -> Option<&mut Seated> {
    data.vt.as_mut()
}

const CARD: &str = "/dev/dri/card0";

/// Tries every 5 ms: the seat's and a device's tries as many as a test
/// names.
fn tries(seat: usize, device: usize) -> Tries {
    Tries {
        every: Duration::from_millis(5),
        seat,
        device,
    }
}

fn ok<T>(r: io::Result<T>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e}"))
}

/// The seat started in a new loop on `tty`; the stand-in's answers are set
/// beforehand.
fn start(server: &StandIn, tty: Option<(FakeConsole, u16)>, tries: Tries) -> (Loop<Data>, Data) {
    let mut lp: Loop<Data> = ok(Loop::new(), "loop");
    let chosen = ok(
        LibseatVt::start_with(&mut lp, server.clone(), tty, tries, access),
        "start",
    );
    let Chosen::Seat(vt) = chosen else {
        panic!("no seat chosen");
    };
    let data = Data {
        vt: Some(vt),
        server: server.clone(),
        timer_at: None,
        sigterm_at: None,
        device_timer_at: None,
        input: None,
    };
    (lp, data)
}

fn vt(data: &mut Data) -> &mut Seated {
    data.vt.as_mut().unwrap_or_else(|| panic!("no VT"))
}

/// Dispatches the loop until `done` holds of the data and the VT's events
/// so far, at most 100 times and each for at most a second, so a seat that
/// never opens fails the test instead of hanging it. Returns the events.
fn run_until(
    lp: &mut Loop<Data>,
    data: &mut Data,
    done: impl Fn(&mut Data, &[VtEvent]) -> bool,
) -> Vec<VtEvent> {
    let mut events = Vec::new();
    for _ in 0..100 {
        events.extend(vt(data).events());
        if done(data, &events) {
            return events;
        }
        ok(lp.dispatch(Some(Duration::from_secs(1)), data), "dispatch");
    }
    panic!("not done after 100 turns; events {events:?}");
}

/// SIGTERM to this thread alone, where the loop blocks it.
fn sigterm() {
    #[expect(unsafe_code, reason = "libc's pthread_kill takes a raw thread handle")]
    // SAFETY: pthread_self is this live thread; pthread_kill sends it a
    // signal the loop's signalfd holds blocked in this thread.
    let sent = unsafe { libc::pthread_kill(libc::pthread_self(), libc::SIGTERM) };
    assert_eq!(sent, 0, "pthread_kill");
}

fn opens(server: &StandIn) -> usize {
    server.0.borrow().opens
}

fn device_opens(server: &StandIn, path: &str) -> usize {
    server
        .0
        .borrow()
        .device_opens
        .get(Path::new(path))
        .copied()
        .unwrap_or(0)
}

fn key(mods: Mods, sym: u32) -> KeyEvent {
    KeyEvent {
        keycode: 0,
        ascii: 0,
        mods,
        keysyms: vec![sym],
        codepoints: vec![None],
        repeat: false,
    }
}

/// The gate of vtium/vtium#34: seatd listens, and the seat is held by
/// another client for three opens, which libseat answers `ENOSYS`, and then
/// opens the seat. Meanwhile a timer of the test's own fires and SIGTERM is
/// served, each while the seat is still pending, and then the seat opens
/// and is enabled.
// covers: platform.libseat
#[test]
fn a_busy_seat_is_retried_from_the_loop_which_serves_a_timer_and_sigterm_meanwhile() {
    let server = StandIn::new();
    server.0.borrow_mut().seatd = true;
    server.0.borrow_mut().held = 3;
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    ok(
        lp.on_signal(Signal::SIGTERM, |d: &mut Data| {
            d.sigterm_at = Some(opens(&d.server));
        }),
        "SIGTERM",
    );
    ok(
        lp.handle()
            .insert_source(
                Timer::from_duration(Duration::from_millis(1)),
                |_, _, d: &mut Data| {
                    d.timer_at = Some(opens(&d.server));
                    TimeoutAction::Drop
                },
            )
            .map_err(|e| io::Error::from(e.error)),
        "timer",
    );
    sigterm();
    let events = run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    // A SIGTERM still pending is served here, so that a seat opened before
    // the loop ran fails the assertions below instead of ending the test
    // process when the loop's drop unblocks the signal.
    ok(lp.dispatch(Some(Duration::ZERO), &mut data), "dispatch");
    assert_eq!(events, [VtEvent::Acquired]);
    assert_eq!(opens(&server), 4, "three busy answers, then the open");
    let pending = |at: Option<usize>| at.is_some_and(|n| n < 4);
    assert!(
        pending(data.timer_at),
        "the timer fired at {:?} opens",
        data.timer_at
    );
    assert!(
        pending(data.sigterm_at),
        "SIGTERM was served at {:?} opens",
        data.sigterm_at
    );
    assert_eq!(vt(&mut data).seat(), "seat0");
}

/// A device the seat manager answers `ENODEV` and then `EBUSY` for waits,
/// with the loop serving a timer meanwhile, and is named settled once it
/// opens; the owner's second ask then gets it.
// covers: platform.libseat
#[test]
fn a_device_the_seat_manager_is_not_ready_for_is_retried_from_the_loop() {
    let server = StandIn::new();
    server
        .0
        .borrow_mut()
        .device_answers
        .insert(PathBuf::from(CARD), [Errno::NODEV, Errno::BUSY].into());
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    let first = vt(&mut data).open_device(Path::new(CARD));
    assert_eq!(
        first.err().map(|e| e.kind()),
        Some(io::ErrorKind::WouldBlock)
    );
    assert_eq!(device_opens(&server, CARD), 1, "tried once at the ask");
    ok(
        lp.handle()
            .insert_source(
                Timer::from_duration(Duration::from_millis(1)),
                |_, _, d: &mut Data| {
                    d.device_timer_at = Some(device_opens(&d.server, CARD));
                    TimeoutAction::Drop
                },
            )
            .map_err(|e| io::Error::from(e.error)),
        "timer",
    );
    run_until(&mut lp, &mut data, |d, _| {
        d.vt.as_ref().is_some_and(|v| !v.settled.is_empty())
    });
    assert!(
        data.device_timer_at.is_some_and(|n| n < 3),
        "the timer fired at {:?} tries",
        data.device_timer_at
    );
    assert_eq!(vt(&mut data).settled(), [PathBuf::from(CARD)]);
    assert_eq!(device_opens(&server, CARD), 3);
    let fd: OwnedFd = ok(vt(&mut data).open_device(Path::new(CARD)), "second ask");
    drop(fd);
    assert_eq!(server.0.borrow().devices.len(), 1);
    vt(&mut data).close_device(Path::new(CARD));
    assert!(
        server.0.borrow().devices.is_empty(),
        "closed through the seat manager"
    );
}

/// A device asked for before the seat is enabled is not asked of the seat
/// manager, which would refuse it, until the enable.
#[test]
fn a_device_asked_before_the_enable_waits_for_it() {
    let server = StandIn::new();
    server.0.borrow_mut().enables_at_open = false;
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    ok(lp.dispatch(Some(Duration::ZERO), &mut data), "dispatch");
    let asked = vt(&mut data).open_device(Path::new(CARD));
    assert_eq!(
        asked.err().map(|e| e.kind()),
        Some(io::ErrorKind::WouldBlock)
    );
    assert_eq!(device_opens(&server, CARD), 0);
    server.enable();
    let events = run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    assert_eq!(events, [VtEvent::Acquired]);
    assert_eq!(vt(&mut data).settled(), [PathBuf::from(CARD)]);
    assert_eq!(device_opens(&server, CARD), 1);
    assert!(vt(&mut data).open_device(Path::new(CARD)).is_ok());
}

/// A device still refused after its tries settles with the seat manager's
/// error, which the owner's second ask gets.
#[test]
fn a_device_refused_past_its_tries_settles_with_its_error() {
    let server = StandIn::new();
    server
        .0
        .borrow_mut()
        .device_answers
        .insert(PathBuf::from(CARD), [Errno::BUSY; 3].into());
    let (mut lp, mut data) = start(&server, None, tries(50, 3));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    assert!(vt(&mut data).open_device(Path::new(CARD)).is_err());
    run_until(&mut lp, &mut data, |d, _| {
        d.vt.as_ref().is_some_and(|v| !v.settled.is_empty())
    });
    assert_eq!(device_opens(&server, CARD), 3);
    let e = vt(&mut data).open_device(Path::new(CARD)).err();
    assert_eq!(
        e.and_then(|e| e.raw_os_error()),
        Some(Errno::BUSY.raw_os_error())
    );
}

/// A seat busy past its tries is given up: a hangup, which ends vtium.
#[test]
fn a_seat_busy_past_its_tries_hangs_up() {
    let server = StandIn::new();
    server.0.borrow_mut().seatd = true;
    server.0.borrow_mut().held = 3;
    let (mut lp, mut data) = start(&server, None, tries(3, 20));
    let events = run_until(&mut lp, &mut data, |_, events| !events.is_empty());
    assert_eq!(events, [VtEvent::Hangup]);
    assert_eq!(opens(&server), 3);
    assert!(vt(&mut data).wait().is_err());
}

/// With `LIBSEAT_BACKEND` naming seatd, a seat held by another client is
/// answered `EPIPE`, and is retried as well.
#[test]
fn a_held_seat_answered_epipe_is_retried() {
    let server = StandIn::new();
    server.0.borrow_mut().seatd = true;
    server.0.borrow_mut().backend_named = true;
    server.0.borrow_mut().held = 2;
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    let events = run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    assert_eq!(events, [VtEvent::Acquired]);
    assert_eq!(opens(&server), 3);
}

/// No seat manager answering hands the VT back for the kernel's backend.
// covers: platform.libseat
#[test]
fn no_seat_manager_hands_the_vt_back() {
    let server = StandIn::new();
    server.0.borrow_mut().open_answers = [Errno::NOSYS].into();
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    let mut lp: Loop<Data> = ok(Loop::new(), "loop");
    let chosen = ok(
        LibseatVt::start_with(
            &mut lp,
            server.clone(),
            Some((console.clone(), 3)),
            tries(50, 20),
            access,
        ),
        "start",
    );
    assert!(matches!(chosen, Chosen::Kernel(Some((_, 3)))));
    assert_eq!(opens(&server), 1);
    assert_eq!(console.modes().0, DisplayMode::Text, "the VT is untouched");
}

/// With seatd listening, a VT not shown is waited for before the seat
/// opens, since seatd gives a new client the VT shown; asking to show it
/// asks the kernel. Once shown, the seat opens, and its enable puts the VT
/// in graphics mode with the keyboard off.
#[test]
fn with_seatd_a_vt_not_shown_is_shown_before_the_seat_opens() {
    let server = StandIn::new();
    server.0.borrow_mut().seatd = true;
    let console = FakeConsole::new(3, 1, KbMode::Unicode);
    let (mut lp, mut data) = start(&server, Some((console.clone(), 3)), tries(50, 20));
    ok(
        lp.dispatch(Some(Duration::from_millis(20)), &mut data),
        "dispatch",
    );
    assert_eq!(opens(&server), 0, "no open while VT 1 is shown");
    assert_eq!(ok(vt(&mut data).activate(), "activate"), Switch::Pending);
    assert!(console.calls().contains(&Call::Activate(3)));
    console.run_worker();
    let events = run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    assert_eq!(events, [VtEvent::Acquired]);
    assert_eq!(opens(&server), 1);
    assert_eq!(console.modes().0, DisplayMode::Graphics);
    assert_eq!(console.modes().2, KbMode::Off);
}

/// Without seatd, the seat opens at once on a VT not shown, as logind
/// takes the session from vtium's login session.
#[test]
fn without_seatd_the_seat_opens_at_once() {
    let server = StandIn::new();
    server.0.borrow_mut().enables_at_open = false;
    let console = FakeConsole::new(3, 1, KbMode::Unicode);
    let (_lp, mut data) = start(&server, Some((console, 3)), tries(50, 20));
    assert_eq!(opens(&server), 1);
    assert_eq!(ok(vt(&mut data).wait(), "wait"), Switch::Pending);
}

/// A disable is a release the owner acknowledges once its displays sleep,
/// and only then is the seat manager told; the next enable acquires again.
// covers: platform.libseat
#[test]
fn a_disable_waits_for_the_owner_before_the_seat_manager_is_told() {
    let server = StandIn::new();
    let console = FakeConsole::new(3, 3, KbMode::Xlate);
    let (mut lp, mut data) = start(&server, Some((console.clone(), 3)), tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    server.disable();
    let events = run_until(&mut lp, &mut data, |d, _| !vt(d).is_active());
    assert_eq!(events, [VtEvent::Release]);
    assert_eq!(
        server.0.borrow().acknowledged,
        0,
        "the owner has not answered"
    );
    ok(vt(&mut data).acknowledge_release(), "acknowledge");
    assert_eq!(server.0.borrow().acknowledged, 1);
    assert_eq!(server.0.borrow().client, Client::Disabled);
    assert_eq!(console.modes().0, DisplayMode::Text);
    assert_eq!(console.modes().2, KbMode::Xlate, "the keyboard mode found");
    server.enable();
    let events = run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    assert_eq!(events, [VtEvent::Acquired]);
}

/// An enable that comes while a release waits, as logind's backend can
/// send, is reported after the owner's acknowledgement, never before: the
/// owner sees its release finish before the next acquire.
#[test]
fn an_enable_while_a_release_waits_follows_the_acknowledgement() {
    let server = StandIn::new();
    server.0.borrow_mut().logind = true;
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    server.tell(Told::Disable);
    server.tell(Told::Enable);
    ok(
        lp.dispatch(Some(Duration::from_millis(20)), &mut data),
        "dispatch",
    );
    let events: Vec<VtEvent> = vt(&mut data).events().collect();
    assert_eq!(events, [VtEvent::Release]);
    assert!(!vt(&mut data).is_active());
    ok(vt(&mut data).acknowledge_release(), "acknowledge");
    let events: Vec<VtEvent> = vt(&mut data).events().collect();
    assert_eq!(events, [VtEvent::Acquired]);
    assert!(vt(&mut data).is_active());
    assert_eq!(server.0.borrow().acknowledged, 1);
}

/// Ctrl+Alt+F2 asks the seat manager for session 2; the VT's own key is
/// taken and asks nothing; neither acts while the seat is not vtium's.
// covers: platform.libseat
#[test]
fn the_switch_keys_ask_the_seat_manager() {
    let server = StandIn::new();
    server.0.borrow_mut().enables_at_open = false;
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    let (mut lp, mut data) = start(&server, Some((console, 3)), tries(50, 20));
    let ctrl_alt = Mods::CTRL.union(Mods::ALT);
    assert!(
        !vt(&mut data).key(&key(ctrl_alt, keysyms::KEY_F2)),
        "not vtium's yet"
    );
    server.enable();
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    assert!(vt(&mut data).key(&key(ctrl_alt, keysyms::KEY_F2)));
    assert!(vt(&mut data).key(&key(ctrl_alt, keysyms::KEY_F3)));
    assert!(vt(&mut data).is_own_key(&key(ctrl_alt, keysyms::KEY_F3)));
    assert!(vt(&mut data).key(&key(Mods::empty(), keysyms::KEY_XF86Switch_VT_5)));
    assert!(!vt(&mut data).key(&key(Mods::CTRL, keysyms::KEY_F2)));
    assert_eq!(server.0.borrow().switches, [2, 5]);
}

/// Dropping the VT closes its devices through the seat manager, puts the
/// VT back in text mode with the keyboard mode found, and closes the seat.
#[test]
fn dropping_closes_the_devices_the_vt_and_the_seat() {
    let server = StandIn::new();
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    let (mut lp, mut data) = start(&server, Some((console.clone(), 3)), tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    let _fd = ok(vt(&mut data).open_device(Path::new(CARD)), "open");
    assert_eq!(server.0.borrow().devices.len(), 1);
    data.vt = None;
    assert!(server.0.borrow().devices.is_empty());
    assert_eq!(server.0.borrow().client, Client::Closed);
    assert_eq!(console.modes().0, DisplayMode::Text);
    assert_eq!(console.modes().2, KbMode::Unicode);
}

/// The seat manager's connection failing is a hangup, and the devices
/// still pending fail with it.
#[test]
fn a_failed_connection_hangs_up() {
    let server = StandIn::new();
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    server
        .0
        .borrow_mut()
        .device_answers
        .insert(PathBuf::from(CARD), [Errno::BUSY].into());
    assert!(vt(&mut data).open_device(Path::new(CARD)).is_err());
    server.0.borrow_mut().broken = true;
    server.disable();
    let events = run_until(&mut lp, &mut data, |_, events| !events.is_empty());
    assert_eq!(events, [VtEvent::Hangup]);
    assert_eq!(vt(&mut data).settled(), [PathBuf::from(CARD)]);
}

/// A device opened by a retry and not yet taken by the owner is closed
/// through the seat manager when the owner lets it go, and when the VT
/// drops.
#[test]
fn a_device_opened_by_a_retry_and_never_taken_is_closed_through_the_seat() {
    for drop_vt in [false, true] {
        let server = StandIn::new();
        server
            .0
            .borrow_mut()
            .device_answers
            .insert(PathBuf::from(CARD), [Errno::BUSY].into());
        let (mut lp, mut data) = start(&server, None, tries(50, 20));
        run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
        assert!(vt(&mut data).open_device(Path::new(CARD)).is_err());
        run_until(&mut lp, &mut data, |d, _| {
            d.vt.as_ref().is_some_and(|v| !v.settled.is_empty())
        });
        assert_eq!(server.0.borrow().devices.len(), 1, "opened by the retry");
        if drop_vt {
            data.vt = None;
        } else {
            vt(&mut data).close_device(Path::new(CARD));
        }
        assert!(
            server.0.borrow().devices.is_empty(),
            "closed through the seat manager (drop: {drop_vt})"
        );
    }
}

/// A retry that meets a disable seatd has begun and vtium has not yet
/// read is refused `EPERM`; the device waits for the next enable instead
/// of failing.
#[test]
fn a_retry_refused_by_a_disable_in_flight_waits_for_the_next_enable() {
    let server = StandIn::new();
    server
        .0
        .borrow_mut()
        .device_answers
        .insert(PathBuf::from(CARD), [Errno::BUSY].into());
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    assert!(vt(&mut data).open_device(Path::new(CARD)).is_err());
    server.disable();
    // The retry's timer fires before the loop reads the disable.
    vt(&mut data).tick();
    assert_eq!(
        device_opens(&server, CARD),
        2,
        "tried while the disable is in flight"
    );
    assert!(vt(&mut data).settled().is_empty(), "not failed");
    let events = run_until(&mut lp, &mut data, |_, e| !e.is_empty());
    assert_eq!(events, [VtEvent::Release]);
    ok(vt(&mut data).acknowledge_release(), "acknowledge");
    server.enable();
    run_until(&mut lp, &mut data, |d, _| {
        d.vt.as_ref().is_some_and(|v| !v.settled.is_empty())
    });
    assert_eq!(vt(&mut data).settled(), [PathBuf::from(CARD)]);
    assert!(vt(&mut data).open_device(Path::new(CARD)).is_ok());
}

/// One device the seat manager answers `EBUSY` for once, asked on a seat
/// vtium holds: it waits for a retry.
fn retried_device(server: &StandIn) -> (Loop<Data>, Data) {
    server
        .0
        .borrow_mut()
        .device_answers
        .insert(PathBuf::from(CARD), [Errno::BUSY].into());
    let (mut lp, mut data) = start(server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    assert!(vt(&mut data).open_device(Path::new(CARD)).is_err());
    (lp, data)
}

/// Dispatches the loop for 30 ms, six of the tests' retry intervals.
fn idle(lp: &mut Loop<Data>, data: &mut Data) {
    for _ in 0..3 {
        ok(
            lp.dispatch(Some(Duration::from_millis(10)), data),
            "dispatch",
        );
    }
}

/// A device the owner lets go while its open is pending is tried no more.
#[test]
fn a_device_let_go_while_pending_is_tried_no_more() {
    let server = StandIn::new();
    let (mut lp, mut data) = retried_device(&server);
    vt(&mut data).close_device(Path::new(CARD));
    idle(&mut lp, &mut data);
    assert_eq!(device_opens(&server, CARD), 1);
    assert!(vt(&mut data).settled().is_empty());
    assert!(server.0.borrow().devices.is_empty());
}

/// A disable puts a device being retried back to wait for the enable, so
/// no retry timer runs while the seat is another's.
#[test]
fn a_disable_stops_the_retries_until_the_enable() {
    let server = StandIn::new();
    let (mut lp, mut data) = retried_device(&server);
    server.disable();
    let events = run_until(&mut lp, &mut data, |_, e| !e.is_empty());
    assert_eq!(events, [VtEvent::Release]);
    ok(vt(&mut data).acknowledge_release(), "acknowledge");
    idle(&mut lp, &mut data);
    assert!(!vt(&mut data).armed, "no timer while the seat is another's");
    assert_eq!(device_opens(&server, CARD), 1);
}

/// A second disable while a release waits cancels the enable owed, as the
/// session is gone again: the acknowledgement acquires nothing.
#[test]
fn a_second_disable_cancels_the_enable_owed() {
    let server = StandIn::new();
    server.0.borrow_mut().logind = true;
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    server.tell(Told::Disable);
    server.tell(Told::Enable);
    server.tell(Told::Disable);
    let events = run_until(&mut lp, &mut data, |_, e| !e.is_empty());
    assert_eq!(events, [VtEvent::Release]);
    ok(vt(&mut data).acknowledge_release(), "acknowledge");
    let events: Vec<VtEvent> = vt(&mut data).events().collect();
    assert!(events.is_empty(), "{events:?}");
    assert!(!vt(&mut data).is_active());
}

/// Nothing asks the seat manager for a device while the seat is not
/// vtium's, whatever runs: the timer's tick or the descriptor's dispatch.
#[test]
fn no_device_is_asked_while_the_seat_is_not_vtiums() {
    let server = StandIn::new();
    server.0.borrow_mut().enables_at_open = false;
    let (_lp, mut data) = start(&server, None, tries(50, 20));
    assert!(vt(&mut data).open_device(Path::new(CARD)).is_err());
    vt(&mut data).tick();
    assert!(vt(&mut data).readable());
    assert_eq!(device_opens(&server, CARD), 0);
}

/// An enable libseat delivers inside the open is acted on at once, with
/// no readable descriptor to wait for.
#[test]
fn an_enable_delivered_inside_the_open_acquires_at_once() {
    let server = StandIn::new();
    let (_lp, mut data) = start(&server, None, tries(50, 20));
    assert!(vt(&mut data).is_active());
    let events: Vec<VtEvent> = vt(&mut data).events().collect();
    assert_eq!(events, [VtEvent::Acquired]);
}

/// The acknowledgement that reports an enable owed also asks for the
/// devices that waited for it.
#[test]
fn the_acknowledgement_that_acquires_asks_for_the_waiting_devices() {
    let server = StandIn::new();
    server.0.borrow_mut().logind = true;
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    server.tell(Told::Disable);
    run_until(&mut lp, &mut data, |_, e| !e.is_empty());
    assert!(vt(&mut data).open_device(Path::new(CARD)).is_err());
    server.tell(Told::Enable);
    ok(
        lp.dispatch(Some(Duration::from_millis(10)), &mut data),
        "dispatch",
    );
    ok(vt(&mut data).acknowledge_release(), "acknowledge");
    let events: Vec<VtEvent> = vt(&mut data).events().collect();
    assert_eq!(events, [VtEvent::Acquired]);
    assert_eq!(vt(&mut data).settled(), [PathBuf::from(CARD)]);
}

const KEYBOARD: &str = "/dev/input/event3";
const KEY_A: u16 = 30;

/// The stand-in keyboards and pointers at `nodes`, opened through the
/// seat. Each device is made only from a descriptor of the open file that
/// the seat manager opened for its node, and only once vtium has made the
/// descriptor non-blocking, since vtium reads a device until it has no
/// more events.
fn through_seat(data: &mut Data, nodes: &Nodes) -> SeatInput<StandInSeat, FakeDevice> {
    let server = data.server.clone();
    let nodes = nodes.clone();
    vt(data).input_with(Box::new(move |path: &Path, fd: OwnedFd| {
        let inode = std::fs::File::from(fd.try_clone()?).metadata()?.ino();
        if server.0.borrow().files.get(path) != Some(&inode) {
            return Err(io::Error::other(format!(
                "{} is not the open file the seat manager gave",
                path.display()
            )));
        }
        let flags = rustix::fs::fcntl_getfl(&fd)?;
        if !flags.contains(rustix::fs::OFlags::NONBLOCK) {
            return Err(io::Error::other("the seat manager's descriptor blocks"));
        }
        nodes.device(path)
    }))
}

fn keymap() -> Keymap {
    Keymap::fallback().unwrap_or_else(|| panic!("the built-in keymap compiles"))
}

/// The input with a stand-in keyboard at [`KEYBOARD`], which the input
/// opens directly unless `seat` opens it.
fn keyboard_input(data: &mut Data, seat: bool) -> (Input<Nodes>, Rc<RefCell<Node>>) {
    let nodes = Nodes::default();
    let node = nodes.add(KEYBOARD, fake::keyboard());
    let mut input = Input::new(nodes.clone(), keymap(), InputConfig::default());
    if seat {
        input.open_with(through_seat(data, &nodes));
    }
    (input, node)
}

/// A press and release of A, read from the keyboard.
fn type_a(input: &mut Input<Nodes>, node: &Rc<RefCell<Node>>, id: DeviceId) -> Vec<InputEvent> {
    for value in [1, 0] {
        node.borrow_mut().queue.push_back(RawEvent {
            kind: EV_KEY,
            code: KEY_A,
            value,
        });
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "the input reads its keys at a time, here the test's"
    )]
    let now = Instant::now();
    input.dispatch(id, now);
    input.events().collect()
}

fn seat_devices(server: &StandIn) -> Vec<PathBuf> {
    server.0.borrow().devices.values().cloned().collect()
}

/// A keyboard opened through the seat delivers its keys through the same
/// path as one opened directly, from the descriptor the seat manager gave.
/// A sleep closes it through the seat manager, and the next wake-up opens
/// it through the seat manager again.
// covers: platform.libseat
#[test]
fn a_keyboard_opened_through_the_seat_types_as_one_opened_directly() {
    let server = StandIn::new();
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());

    let (mut direct, direct_node) = keyboard_input(&mut data, false);
    direct.wake_up();
    let direct_id = direct
        .add(Path::new(KEYBOARD))
        .unwrap_or_else(|| panic!("the keyboard opens directly"));
    let expected = type_a(&mut direct, &direct_node, direct_id);
    assert!(
        expected.iter().any(|e| matches!(e, InputEvent::Key(_))),
        "a direct open types: {expected:?}"
    );
    assert_eq!(device_opens(&server, KEYBOARD), 0);

    let (mut input, node) = keyboard_input(&mut data, true);
    input.wake_up();
    let id = input
        .add(Path::new(KEYBOARD))
        .unwrap_or_else(|| panic!("the keyboard opens through the seat"));
    assert_eq!(device_opens(&server, KEYBOARD), 1);
    assert_eq!(seat_devices(&server), [PathBuf::from(KEYBOARD)]);
    assert_eq!(type_a(&mut input, &node, id), expected);

    input.sleep();
    assert!(seat_devices(&server).is_empty(), "closed through the seat");
    assert!(!node.borrow().open);
    input.wake_up();
    assert_eq!(device_opens(&server, KEYBOARD), 2);
    assert_eq!(seat_devices(&server), [PathBuf::from(KEYBOARD)]);
    assert_eq!(type_a(&mut input, &node, id), expected);
}

/// A keyboard the seat manager answers `ENODEV` and then `EBUSY` for waits,
/// with the loop serving a timer of the test's own meanwhile, and opens
/// from the input's retry timer.
// covers: platform.libseat
#[test]
fn a_keyboard_the_seat_manager_is_not_ready_for_is_retried_from_the_loop() {
    let server = StandIn::new();
    server
        .0
        .borrow_mut()
        .device_answers
        .insert(PathBuf::from(KEYBOARD), [Errno::NODEV, Errno::BUSY].into());
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    let (mut input, node) = keyboard_input(&mut data, true);
    input.insert(&lp.handle(), |d: &mut Data| d.input.as_mut());
    input.wake_up();
    let id = input
        .add(Path::new(KEYBOARD))
        .unwrap_or_else(|| panic!("the keyboard is kept while it waits"));
    assert!(input.waiting(id));
    assert_eq!(device_opens(&server, KEYBOARD), 1, "tried once at the add");
    data.input = Some(input);
    ok(
        lp.handle()
            .insert_source(
                Timer::from_duration(Duration::from_millis(1)),
                |_, _, d: &mut Data| {
                    d.device_timer_at = Some(device_opens(&d.server, KEYBOARD));
                    TimeoutAction::Drop
                },
            )
            .map_err(|e| io::Error::from(e.error)),
        "timer",
    );
    run_until(&mut lp, &mut data, |d, _| {
        d.input.as_ref().is_some_and(|i| !i.waiting(id))
    });
    assert!(
        data.device_timer_at.is_some_and(|n| n < 3),
        "the timer fired at {:?} tries",
        data.device_timer_at
    );
    assert_eq!(device_opens(&server, KEYBOARD), 3);
    assert_eq!(seat_devices(&server), [PathBuf::from(KEYBOARD)]);
    let input = data.input.as_mut().unwrap_or_else(|| panic!("no input"));
    assert!(
        type_a(input, &node, id)
            .iter()
            .any(|e| matches!(e, InputEvent::Key(_)))
    );
}

/// A keyboard the seat manager keeps refusing is dropped after its tries,
/// as one that cannot be opened directly is.
#[test]
fn a_keyboard_refused_past_its_tries_is_dropped() {
    let server = StandIn::new();
    server
        .0
        .borrow_mut()
        .device_answers
        .insert(PathBuf::from(KEYBOARD), [Errno::NODEV; 10].into());
    let (mut lp, mut data) = start(&server, None, tries(50, 3));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    let (mut input, _node) = keyboard_input(&mut data, true);
    input.insert(&lp.handle(), |d: &mut Data| d.input.as_mut());
    input.wake_up();
    let id = input.add(Path::new(KEYBOARD));
    assert!(id.is_some());
    data.input = Some(input);
    run_until(&mut lp, &mut data, |d, _| {
        d.input
            .as_ref()
            .is_some_and(|i| i.find(Path::new(KEYBOARD)).is_none())
    });
    assert_eq!(device_opens(&server, KEYBOARD), 3);
    assert!(seat_devices(&server).is_empty());
}

/// A keyboard the seat manager refuses `EPERM`, while it takes the seat
/// away, waits for the next wake-up, which follows the next enable.
#[test]
fn a_keyboard_refused_while_the_seat_is_taken_away_waits_for_the_wake_up() {
    let server = StandIn::new();
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    let (mut input, node) = keyboard_input(&mut data, true);
    input.wake_up();
    // The seat manager begins the disable, which the loop has not read.
    server.disable();
    let id = input
        .add(Path::new(KEYBOARD))
        .unwrap_or_else(|| panic!("the keyboard is kept while it waits"));
    assert!(input.waiting(id));
    assert_eq!(device_opens(&server, KEYBOARD), 1);
    let events = run_until(&mut lp, &mut data, |_, e| e.contains(&VtEvent::Release));
    assert_eq!(events, [VtEvent::Release]);
    input.sleep();
    ok(vt(&mut data).acknowledge_release(), "acknowledge");
    assert_eq!(
        device_opens(&server, KEYBOARD),
        1,
        "not tried while released"
    );
    server.enable();
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    input.wake_up();
    assert_eq!(device_opens(&server, KEYBOARD), 2);
    assert!(!input.waiting(id));
    assert!(node.borrow().open);
}

/// A keyboard asked for while the seat is not vtium's is not asked of the
/// seat manager, which would refuse it.
#[test]
fn no_keyboard_is_asked_while_the_seat_is_not_vtiums() {
    let server = StandIn::new();
    server.0.borrow_mut().enables_at_open = false;
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    ok(lp.dispatch(Some(Duration::ZERO), &mut data), "dispatch");
    let (mut input, _node) = keyboard_input(&mut data, true);
    input.wake_up();
    let id = input
        .add(Path::new(KEYBOARD))
        .unwrap_or_else(|| panic!("the keyboard is kept while it waits"));
    assert!(input.waiting(id));
    assert_eq!(device_opens(&server, KEYBOARD), 0);
}

/// Dropping the VT closes the keyboards opened through the seat, through
/// the seat manager; the input's later close closes only its own
/// descriptor.
// covers: platform.libseat
#[test]
fn dropping_the_vt_closes_the_keyboards_through_the_seat() {
    let server = StandIn::new();
    let (mut lp, mut data) = start(&server, None, tries(50, 20));
    run_until(&mut lp, &mut data, |d, _| vt(d).is_active());
    let (mut input, node) = keyboard_input(&mut data, true);
    input.wake_up();
    let id = input.add(Path::new(KEYBOARD));
    assert!(id.is_some());
    assert_eq!(seat_devices(&server), [PathBuf::from(KEYBOARD)]);
    data.vt = None;
    assert!(seat_devices(&server).is_empty());
    assert_eq!(server.0.borrow().client, Client::Closed);
    input.sleep();
    assert!(!node.borrow().open);
}
