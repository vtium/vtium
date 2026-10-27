//! The kernel's VT against the stand-in: what the VT is left in on every
//! exit, the release and acquire signals, the switch keys and the fake VT.

#![expect(clippy::panic, reason = "a test fails by panicking")]

use std::io;
use std::time::{Duration, Instant};

use xkbcommon::xkb::keysyms;

use super::fake::{COOKED, Call, FakeConsole, Lines};
use super::kernel::Retakes;
use super::{
    Console, DisplayMode, FakeVt, KbMode, KernelVt, Reldisp, Switch, Switching, Vt, VtEvent,
};
use crate::event_loop::Loop;
use crate::input::{KeyEvent, Mods};

/// The modes a VT is in before vtium takes it: text, switched by the
/// kernel, with `keyboard`.
fn untouched(keyboard: KbMode) -> (DisplayMode, Switching, KbMode) {
    (DisplayMode::Text, Switching::Auto, keyboard)
}

fn taken() -> (DisplayMode, Switching, KbMode) {
    (DisplayMode::Graphics, Switching::Process, KbMode::Off)
}

/// The tty's line settings while vtium holds it.
const SILENT: Lines = Lines {
    echo: false,
    canonical: false,
};

fn lines(console: &FakeConsole) -> Lines {
    console.0.borrow().lines
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

/// The kernel asks for the VT to switch to VT `to`: a pending switch, and
/// SIGUSR2.
fn asked_away(console: &FakeConsole, vt: &mut KernelVt<FakeConsole>, to: u16) {
    console.0.borrow_mut().pending_switch = Some(to);
    vt.release_signal();
}

#[expect(
    clippy::disallowed_methods,
    reason = "a re-take's time only chooses its log level"
)]
fn now() -> Instant {
    Instant::now()
}

fn ctrl_alt() -> Mods {
    Mods::CTRL.union(Mods::ALT)
}

/// VT 3, shown, its keyboard in K_UNICODE.
fn shown() -> (FakeConsole, KernelVt<FakeConsole>) {
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    let vt = KernelVt::open(console.clone(), 3).unwrap_or_else(|e| panic!("open: {e}"));
    console.clear();
    (console, vt)
}

#[test]
fn opening_takes_the_vt_and_dropping_puts_it_back() {
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    let mut vt = KernelVt::open(console.clone(), 3).unwrap_or_else(|e| panic!("open: {e}"));
    assert_eq!(
        console.calls(),
        [
            Call::Active,
            Call::SetDisplay(DisplayMode::Graphics),
            Call::SetSwitching(Switching::Process),
            Call::KeyboardMode,
            Call::SetKeyboard(KbMode::Raw),
            Call::SetKeyboard(KbMode::Off),
            Call::Termios,
            Call::SetTermios(SILENT),
        ]
    );
    assert_eq!(console.modes(), taken());
    assert_eq!(lines(&console), SILENT);
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
    assert!(vt.is_active());
    assert_eq!(vt.number(), Some(3));
    console.clear();
    drop(vt);
    assert_eq!(
        console.calls(),
        [
            Call::SetTermios(COOKED),
            Call::SetKeyboard(KbMode::Unicode),
            Call::SetSwitching(Switching::Auto),
            Call::SetDisplay(DisplayMode::Text),
        ]
    );
    assert_eq!(console.modes(), untouched(KbMode::Unicode));
    assert_eq!(lines(&console), COOKED);
}

/// The seat's setup after the VT opened, failing at `fail`: the VT goes
/// out of scope with the error, as `?` returns it.
fn seat_setup(console: FakeConsole, fail: bool) -> io::Result<KernelVt<FakeConsole>> {
    let vt = KernelVt::open(console, 3)?;
    if fail {
        return Err(io::Error::other("the seat's configuration failed"));
    }
    Ok(vt)
}

// covers: platform.vt-switch
#[test]
fn a_setup_failure_after_the_vt_opens_restores_it() {
    for keyboard in [KbMode::Unicode, KbMode::Xlate] {
        let console = FakeConsole::new(3, 1, keyboard);
        assert!(seat_setup(console.clone(), true).is_err());
        assert_eq!(console.modes(), untouched(keyboard), "{keyboard:?}");
    }
    let console = FakeConsole::new(3, 1, KbMode::Unicode);
    let vt = seat_setup(console.clone(), false);
    assert_eq!(console.modes(), taken());
    drop(vt);
    assert_eq!(console.modes(), untouched(KbMode::Unicode));
}

/// The loop's data in [`the_switch_signals_are_blocked_before_vt_process`].
struct Seat {
    vt: Option<KernelVt<FakeConsole>>,
}

#[test]
fn the_switch_signals_are_blocked_before_vt_process() {
    let mut lp = Loop::<Seat>::new().unwrap_or_else(|e| panic!("the loop: {e}"));
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    let vt = KernelVt::start(&mut lp, console.clone(), 3, |s: &mut Seat| s.vt.as_mut())
        .unwrap_or_else(|e| panic!("start: {e}"));
    // A switch the moment VT_PROCESS is set sends a signal the loop holds
    // until it reads it, never one with its default action.
    assert_eq!(console.0.borrow().signals_blocked_at_process, Some(true));
    let mut seat = Seat { vt: Some(vt) };
    assert!(
        lp.dispatch(Some(std::time::Duration::ZERO), &mut seat)
            .is_ok()
    );
}

#[test]
fn an_open_that_fails_part_way_puts_back_what_it_set() {
    let cases = [
        (Call::Active, vec![]),
        (Call::SetDisplay(DisplayMode::Graphics), vec![]),
        (
            Call::SetSwitching(Switching::Process),
            vec![Call::SetDisplay(DisplayMode::Text)],
        ),
        (
            Call::KeyboardMode,
            vec![
                Call::SetSwitching(Switching::Auto),
                Call::SetDisplay(DisplayMode::Text),
            ],
        ),
        (
            Call::SetKeyboard(KbMode::Raw),
            vec![
                Call::SetSwitching(Switching::Auto),
                Call::SetDisplay(DisplayMode::Text),
            ],
        ),
    ];
    for (fail, undo) in cases {
        let console = FakeConsole::new(3, 3, KbMode::Unicode);
        console.fail(Some(fail));
        assert!(KernelVt::open(console.clone(), 3).is_err(), "{fail:?}");
        let calls = console.calls();
        let at = calls.iter().position(|&c| c == fail).unwrap_or(calls.len());
        assert_eq!(calls.get(at + 1..).unwrap_or_default(), undo, "{fail:?}");
        console.fail(None);
        assert_eq!(console.modes(), untouched(KbMode::Unicode), "{fail:?}");
    }
}

/// Whether `signal` is ignored in this process.
fn ignored(signal: rustix::process::Signal) -> bool {
    use rustix::runtime_448b8ad740e2a26f::{kernel_sig_ign, kernel_sigaction};
    #[expect(unsafe_code, reason = "reading a signal's disposition")]
    // SAFETY: with no new action, rt_sigaction only reads the disposition
    // and changes nothing.
    let action = unsafe { kernel_sigaction(signal, None) };
    // SIG_IGN is the handler value 1, compared as an address.
    let address = |h: rustix::runtime_448b8ad740e2a26f::KernelSighandler| h.map(|f| f as usize);
    action.is_ok_and(|a| address(a.sa_handler_kernel) == address(kernel_sig_ign()))
}

#[test]
fn the_switch_signals_are_ignored_once_the_vt_is_put_back() {
    let (_, vt) = shown();
    drop(vt);
    // The loop's signalfd unblocks them when it drops, after the VT: a
    // release still pending then is discarded, not acted on by default.
    assert!(ignored(rustix::process::Signal::USR1));
    assert!(ignored(rustix::process::Signal::USR2));
}

#[test]
fn a_keyboard_left_off_or_raw_is_restored_as_unicode() {
    for left in [KbMode::Off, KbMode::Raw, KbMode::MediumRaw] {
        let console = FakeConsole::new(2, 2, left);
        drop(KernelVt::open(console.clone(), 2));
        assert_eq!(console.modes(), untouched(KbMode::Unicode), "{left:?}");
    }
}

#[test]
fn input_on_the_tty_takes_the_vt_again() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    // The VT as something other than vtium might leave it, its line
    // settings neither the open's nor vtium's.
    {
        let mut k = console.0.borrow_mut();
        k.display = DisplayMode::Text;
        k.switching = Switching::Auto;
        k.keyboard = KbMode::Unicode;
        k.lines = Lines {
            echo: true,
            canonical: false,
        };
    }
    assert!(vt.tty_ready(true, false, now()));
    assert_eq!(console.modes(), taken());
    assert_eq!(lines(&console), SILENT);
    assert_eq!(console.calls().last(), Some(&Call::Flush));
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Retaken]);
    // The settings found at the open are what a drop puts back.
    drop(vt);
    assert_eq!(console.modes(), untouched(KbMode::Unicode));
    assert_eq!(lines(&console), COOKED);
}

#[test]
fn a_retake_sets_every_mode_it_can() {
    let (console, mut vt) = shown();
    {
        let mut k = console.0.borrow_mut();
        k.switching = Switching::Auto;
        k.keyboard = KbMode::Unicode;
        k.lines = COOKED;
    }
    console.fail(Some(Call::SetDisplay(DisplayMode::Graphics)));
    assert!(vt.tty_ready(true, false, now()));
    // KDSETMODE refused; the rest is set anyway.
    assert_eq!(console.0.borrow().switching, Switching::Process);
    assert_eq!(console.0.borrow().keyboard, KbMode::Off);
    assert_eq!(lines(&console), SILENT);
}

#[test]
fn a_retake_is_reported_only_while_the_vt_is_ours() {
    // While a release waits, it is held until the owner answers.
    for refuse in [true, false] {
        let (console, mut vt) = shown();
        vt.events().for_each(drop);
        asked_away(&console, &mut vt, 5);
        vt.events().for_each(drop);
        assert!(vt.tty_ready(true, false, now()));
        assert_eq!(vt.events().count(), 0, "refuse: {refuse}");
        if refuse {
            assert!(vt.refuse_release().is_ok());
            assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Retaken]);
            assert!(vt.is_active());
        } else {
            assert!(vt.acknowledge_release().is_ok());
            assert_eq!(vt.events().count(), 0);
            assert_eq!(console.0.borrow().on_screen, 5);
        }
    }
    // Released, nothing is reported: the next Acquired sets everything up.
    let (console, mut vt) = shown();
    asked_away(&console, &mut vt, 5);
    assert!(vt.acknowledge_release().is_ok());
    vt.events().for_each(drop);
    assert!(vt.tty_ready(true, false, now()));
    assert_eq!(vt.events().count(), 0);
    assert!(console.calls().contains(&Call::Flush));
    assert!(!vt.is_active());
}

/// Another process resets the VT while a release waits, through the
/// stand-in's calls: `VT_AUTO`, whose `VT_SETMODE` ends the kernel's wait
/// (vt_ioctl.c:780), text mode, and the keyboard and line settings a
/// console has.
fn reset_while_released(console: &FakeConsole) {
    let reset = [
        console.set_switching(Switching::Auto),
        console.set_display_mode(DisplayMode::Text),
        console.set_keyboard_mode(KbMode::Unicode),
        console.set_termios(&COOKED),
    ];
    assert!(reset.iter().all(Result::is_ok));
}

/// The kernel shows VT 3 again, and signals the acquire only in
/// `VT_PROCESS` (vt_ioctl.c:1164-1170).
fn switched_back(console: &FakeConsole, vt: &mut KernelVt<FakeConsole>) {
    console.show(3);
    if console.0.borrow().switching == Switching::Process {
        vt.acquire_signal();
    }
}

// covers: platform.vt-switch
#[test]
fn a_release_the_kernel_no_longer_waits_on_keeps_the_vt() {
    // With or without the tty's input after the reset.
    for input in [true, false] {
        let (console, mut vt) = shown();
        vt.events().for_each(drop);
        asked_away(&console, &mut vt, 5);
        vt.events().for_each(drop);
        reset_while_released(&console);
        if input {
            assert!(vt.tty_ready(true, false, now()));
        }
        // The owner acknowledges once the displays are asleep.
        assert!(vt.acknowledge_release().is_ok(), "input: {input}");
        assert_eq!(console.0.borrow().on_screen, 3);
        assert!(vt.is_active());
        assert_eq!(console.modes(), taken(), "input: {input}");
        assert_eq!(lines(&console), SILENT, "input: {input}");
        assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
        // The switch keys work again.
        console.clear();
        assert!(vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
        assert!(console.calls().contains(&Call::Activate(2)));
    }
}

#[test]
fn a_release_completed_after_a_reset_signals_the_way_back() {
    // The reset leaves VT_AUTO, so the switch to VT 5 completes with no
    // answer; the VT is taken again in the background.
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    asked_away(&console, &mut vt, 5);
    reset_while_released(&console);
    console.show(5);
    vt.events().for_each(drop);
    assert!(vt.acknowledge_release().is_ok());
    assert!(!vt.is_active());
    assert_eq!(vt.events().count(), 0);
    assert_eq!(console.modes(), taken());
    assert_eq!(lines(&console), SILENT);
    // The switch back signals the acquire, so the displays wake.
    switched_back(&console, &mut vt);
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
    assert!(vt.is_active());
}

#[test]
fn a_refusal_the_kernel_no_longer_waits_on_takes_the_vt_again() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    asked_away(&console, &mut vt, 5);
    vt.events().for_each(drop);
    reset_while_released(&console);
    assert!(vt.refuse_release().is_ok());
    assert!(vt.is_active());
    assert_eq!(console.modes(), taken());
    assert_eq!(lines(&console), SILENT);
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Retaken]);
}

#[test]
fn a_refusal_after_the_switch_completed_releases_the_vt() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    asked_away(&console, &mut vt, 5);
    reset_while_released(&console);
    console.show(5);
    vt.events().for_each(drop);
    assert!(vt.refuse_release().is_ok());
    assert!(!vt.is_active());
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Release]);
    assert_eq!(console.modes(), taken());
    assert_eq!(lines(&console), SILENT);
    // The owner sleeps and acknowledges; the VT stays released.
    assert!(vt.acknowledge_release().is_ok());
    assert!(!vt.is_active());
    assert_eq!(vt.events().count(), 0);
    switched_back(&console, &mut vt);
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
}

#[test]
fn a_switch_back_during_the_sleep_a_refusal_asked_for_waits_for_its_acknowledgement() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    asked_away(&console, &mut vt, 5);
    reset_while_released(&console);
    console.show(5);
    assert!(vt.refuse_release().is_ok());
    vt.events().for_each(drop);
    // Switched back while the displays go to sleep: nothing is acquired
    // before the owner acknowledges.
    switched_back(&console, &mut vt);
    assert_eq!(vt.events().count(), 0);
    assert!(!vt.is_active());
    assert!(vt.acknowledge_release().is_ok());
    assert!(vt.is_active());
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
}

#[test]
fn an_acquired_release_reports_no_error() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    asked_away(&console, &mut vt, 5);
    reset_while_released(&console);
    vt.events().for_each(drop);
    console.fail(Some(Call::SetDisplay(DisplayMode::Graphics)));
    assert!(vt.acknowledge_release().is_ok());
    assert!(vt.is_active());
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
    assert_eq!(console.0.borrow().switching, Switching::Process);
}

#[test]
fn a_switch_key_works_when_another_process_left_vt_auto() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    // VT_AUTO in KD_GRAPHICS: the kernel drops any VT_ACTIVATE.
    assert!(console.set_switching(Switching::Auto).is_ok());
    assert!(console.activate(2).is_ok());
    console.run_worker();
    assert_eq!(console.0.borrow().on_screen, 3);
    assert_eq!(console.0.borrow().pending_switch, None);
    assert!(vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
    assert_eq!(console.0.borrow().switching, Switching::Process);
    console.run_worker();
    assert_eq!(console.0.borrow().pending_switch, Some(2));
    // The release it asks for is vtium's to answer.
    vt.release_signal();
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Release]);
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(console.0.borrow().on_screen, 2);
}

#[test]
fn a_vt_shown_with_no_acquire_is_acquired_on_its_ttys_input() {
    // Released, then another process sets VT_AUTO, and the VT is shown
    // with no acquire signal.
    let (console, mut vt) = shown();
    asked_away(&console, &mut vt, 5);
    assert!(vt.acknowledge_release().is_ok());
    console.0.borrow_mut().switching = Switching::Auto;
    switched_back(&console, &mut vt);
    vt.events().for_each(drop);
    assert!(!vt.is_active());
    assert!(vt.tty_ready(true, false, now()));
    assert!(vt.is_active());
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
    assert_eq!(console.modes(), taken());
    // An acquire signal read after it only logs.
    vt.acquire_signal();
    assert_eq!(vt.events().count(), 0);
    assert!(vt.is_active());
}

#[test]
fn an_acknowledged_release_drops_the_retake_it_held() {
    let (console, mut vt) = shown();
    asked_away(&console, &mut vt, 5);
    assert!(vt.tty_ready(true, false, now()));
    assert!(vt.acknowledge_release().is_ok());
    switched_back(&console, &mut vt);
    asked_away(&console, &mut vt, 5);
    vt.events().for_each(drop);
    assert!(vt.refuse_release().is_ok());
    assert_eq!(vt.events().count(), 0);
}

#[test]
fn a_retake_after_a_quiet_spell_warns_again() {
    let quiet = Duration::from_millis(u64::try_from(crate::RETAKE_QUIET_MS).unwrap_or(u64::MAX));
    let second = Duration::from_secs(1);
    let start = now();
    let mut retakes = Retakes::default();
    assert!(retakes.note(start), "the first warns");
    // A steady stream warns once, however long it lasts.
    let mut at = start;
    for _ in 0..30 {
        at += second;
        assert!(!retakes.note(at));
    }
    at += quiet;
    assert!(retakes.note(at), "the first after a quiet spell warns");
    assert!(!retakes.note(at + second));
}

#[test]
fn input_while_a_release_waits_keeps_the_switch_away() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    asked_away(&console, &mut vt, 5);
    // Input on the tty before the displays are asleep: the VT is taken
    // again, but the pending switch stays the kernel's to finish.
    assert!(vt.tty_ready(true, false, now()));
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(console.0.borrow().on_screen, 5);
    assert!(!vt.is_active());
}

#[test]
fn a_restore_after_a_retake_puts_back_what_the_open_found() {
    let console = FakeConsole::new(3, 3, KbMode::Xlate);
    let mut vt = KernelVt::open(console.clone(), 3).unwrap_or_else(|e| panic!("open: {e}"));
    let restore = vt.restorer().unwrap_or_else(|e| panic!("restorer: {e}"));
    console.0.borrow_mut().keyboard = KbMode::Unicode;
    console.0.borrow_mut().lines = Lines {
        echo: true,
        canonical: false,
    };
    assert!(vt.tty_ready(true, false, now()));
    std::mem::forget(vt);
    restore.run();
    assert_eq!(console.modes(), untouched(KbMode::Xlate));
    assert_eq!(lines(&console), COOKED);
}

#[test]
fn a_tty_that_cannot_be_silenced_still_opens() {
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    console.fail(Some(Call::SetTermios(SILENT)));
    let vt = KernelVt::open(console.clone(), 3);
    assert!(vt.is_ok());
    assert_eq!(lines(&console), COOKED);
    console.fail(None);
    let Ok(mut vt) = vt else { return };
    let restore = vt.restorer().unwrap_or_else(|e| panic!("restorer: {e}"));
    // A re-take leaves the tty alone too.
    console.0.borrow_mut().lines = Lines {
        echo: true,
        canonical: false,
    };
    assert!(vt.tty_ready(true, false, now()));
    assert_eq!(
        lines(&console),
        Lines {
            echo: true,
            canonical: false,
        }
    );
    console.clear();
    drop(vt);
    restore.run();
    // Nothing was changed, so nothing is put back, by the drop or the
    // Restore.
    assert!(
        !console
            .calls()
            .iter()
            .any(|c| matches!(c, Call::SetTermios(_)))
    );
}

#[test]
fn a_refused_k_off_leaves_k_raw_and_the_vt_opens() {
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    console.fail(Some(Call::SetKeyboard(KbMode::Off)));
    let vt = KernelVt::open(console.clone(), 3);
    assert!(vt.is_ok());
    assert_eq!(
        console.modes(),
        (DisplayMode::Graphics, Switching::Process, KbMode::Raw)
    );
    drop(vt);
    assert_eq!(console.modes(), untouched(KbMode::Unicode));
}

#[test]
fn a_vt_not_shown_waits_for_its_acquire() {
    let console = FakeConsole::new(3, 1, KbMode::Unicode);
    let mut vt = KernelVt::open(console.clone(), 3).unwrap_or_else(|e| panic!("open: {e}"));
    assert_eq!(vt.events().count(), 0);
    assert!(!vt.is_active());
    assert_eq!(vt.wait().ok(), Some(Switch::Pending));
    console.clear();
    // An acquire meant for another VT changes nothing.
    vt.acquire_signal();
    assert_eq!(console.calls(), [Call::Active]);
    assert_eq!(vt.events().count(), 0);
    console.show(3);
    console.clear();
    vt.acquire_signal();
    assert_eq!(console.calls(), [Call::Active], "no VT_ACKACQ");
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
    assert!(vt.is_active());
    assert_eq!(vt.wait().ok(), Some(Switch::Done));
}

#[test]
fn the_release_is_answered_only_when_the_owner_answers() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    asked_away(&console, &mut vt, 5);
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Release]);
    assert_eq!(console.calls(), [Call::Active], "no answer yet");
    // The displays are still going to sleep: a second signal waits too.
    asked_away(&console, &mut vt, 5);
    assert_eq!(vt.events().count(), 0);
    assert!(!vt.is_active());
    console.clear();
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(console.calls(), [Call::Reldisp(Reldisp::Allow)]);
    assert!(!vt.is_active());
    // Nothing is pending now.
    console.clear();
    assert!(vt.acknowledge_release().is_ok());
    assert!(console.calls().is_empty());
}

#[test]
fn a_switch_in_and_straight_out_waits_for_the_displays() {
    // Both signals pending in one turn: the switch to VT 3 done, and the
    // switch on to VT 5 waiting on vtium's answer. The signalfd gives
    // SIGUSR1 first.
    let console = FakeConsole::new(3, 1, KbMode::Unicode);
    let mut vt = KernelVt::open(console.clone(), 3).unwrap_or_else(|e| panic!("open: {e}"));
    console.show(3);
    console.0.borrow_mut().pending_switch = Some(5);
    console.clear();
    vt.acquire_signal();
    vt.release_signal();
    assert_eq!(
        vt.events().collect::<Vec<_>>(),
        [VtEvent::Acquired, VtEvent::Release]
    );
    assert!(
        !console
            .calls()
            .iter()
            .any(|c| matches!(c, Call::Reldisp(_))),
        "nothing answered before the displays sleep"
    );
    assert_eq!(console.0.borrow().on_screen, 3);
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(console.0.borrow().on_screen, 5);
    assert!(!vt.is_active());
}

#[test]
fn an_acquire_while_the_release_waits_answers_nothing() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    asked_away(&console, &mut vt, 5);
    vt.events().for_each(drop);
    console.clear();
    // VT_ACKACQ now would let the switch finish before the displays sleep.
    vt.acquire_signal();
    assert_eq!(console.calls(), [Call::Active]);
    assert_eq!(vt.events().count(), 0);
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(
        console.calls(),
        [Call::Active, Call::Reldisp(Reldisp::Allow)]
    );
}

#[test]
fn a_refused_release_keeps_the_vt() {
    let (console, mut vt) = shown();
    asked_away(&console, &mut vt, 5);
    console.clear();
    assert!(vt.refuse_release().is_ok());
    assert_eq!(console.calls(), [Call::Reldisp(Reldisp::Refuse)]);
    assert!(vt.is_active());
}

#[test]
fn a_release_while_not_ours_is_allowed_at_once() {
    let console = FakeConsole::new(4, 3, KbMode::Unicode);
    let mut vt = KernelVt::open(console.clone(), 4).unwrap_or_else(|e| panic!("open: {e}"));
    console.show(4);
    console.clear();
    asked_away(&console, &mut vt, 5);
    assert_eq!(
        console.calls(),
        [Call::Active, Call::Reldisp(Reldisp::Allow)]
    );
    assert_eq!(vt.events().count(), 0);
}

#[test]
fn the_switch_keys_ask_for_their_vt() {
    let (console, mut vt) = shown();
    assert!(vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
    assert!(vt.key(&key(Mods::empty(), keysyms::KEY_XF86Switch_VT_5)));
    assert!(vt.key(&key(ctrl_alt(), keysyms::KEY_F12)));
    let asked: Vec<Call> = console
        .calls()
        .into_iter()
        .filter(|c| matches!(c, Call::Activate(_)))
        .collect();
    assert_eq!(
        asked,
        [Call::Activate(2), Call::Activate(5), Call::Activate(12)]
    );
    console.clear();
    // This VT's own key is taken and asks for nothing.
    assert!(vt.key(&key(ctrl_alt(), keysyms::KEY_F3)));
    // F2 alone, or with Ctrl only, is the terminal's.
    assert!(!vt.key(&key(Mods::empty(), keysyms::KEY_F2)));
    assert!(!vt.key(&key(Mods::CTRL, keysyms::KEY_F2)));
    assert!(!vt.key(&key(ctrl_alt(), keysyms::KEY_F13)));
    assert!(
        !console
            .calls()
            .iter()
            .any(|c| matches!(c, Call::Activate(_)))
    );
}

#[test]
fn the_switch_keys_wait_while_the_vt_is_not_ours() {
    let (console, mut vt) = shown();
    asked_away(&console, &mut vt, 5);
    // While the release waits, a key reads and sets nothing, so the switch
    // the kernel waits on stays.
    console.clear();
    assert!(!vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
    assert!(console.calls().is_empty());
    assert_eq!(console.0.borrow().pending_switch, Some(5));
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(console.0.borrow().on_screen, 5);
    console.show(2);
    console.clear();
    assert!(!vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
    assert!(console.calls().is_empty());
}

#[test]
fn a_switch_key_takes_a_vt_process_that_is_not_vtiums() {
    // VT_PROCESS with another process's signals, or a mode the headers do
    // not name: the switch would wait on that process, or be dropped.
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    console.0.borrow_mut().signals_ours = false;
    assert!(vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
    assert!(console.0.borrow().signals_ours);
    assert_eq!(console.0.borrow().switching, Switching::Process);
    console.run_worker();
    vt.release_signal();
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Release]);
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(console.0.borrow().on_screen, 2);
}

#[test]
fn a_switch_key_whose_switching_cannot_be_read_still_asks() {
    let (console, mut vt) = shown();
    console.fail(Some(Call::Switching));
    console.clear();
    assert!(vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
    assert_eq!(
        console.calls(),
        [Call::Active, Call::Switching, Call::Activate(2)]
    );
}

#[test]
fn the_stand_in_switches_as_the_kernel_does() {
    // Dropped at VT_ACTIVATE from VT_AUTO and graphics mode: a VT_SETMODE
    // after it leaves the worker nothing to do.
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    console.0.borrow_mut().display = DisplayMode::Graphics;
    assert!(console.activate(2).is_ok());
    assert!(console.set_switching(Switching::Process).is_ok());
    console.run_worker();
    assert_eq!(console.0.borrow().on_screen, 3);
    assert_eq!(console.0.borrow().pending_switch, None);
    // Asked from text mode, then put in graphics mode before the worker
    // runs: change_console drops it.
    let console = FakeConsole::new(3, 3, KbMode::Unicode);
    assert!(console.activate(2).is_ok());
    assert!(console.set_display_mode(DisplayMode::Graphics).is_ok());
    console.run_worker();
    assert_eq!(console.0.borrow().on_screen, 3);
    // From another VT in VT_PROCESS, the switch waits on that VT's owner,
    // and nothing waits on the stand-in's VT.
    let console = FakeConsole::new(3, 5, KbMode::Unicode);
    assert!(console.set_switching(Switching::Process).is_ok());
    console.0.borrow_mut().others = (Switching::Process, DisplayMode::Text);
    assert!(console.activate(3).is_ok());
    console.run_worker();
    assert_eq!(console.0.borrow().on_screen, 5);
    assert_eq!(console.0.borrow().pending_switch, None);
    // From the stand-in's own VT in VT_PROCESS, its switch waits.
    console.show(3);
    assert!(console.activate(5).is_ok());
    console.run_worker();
    assert_eq!(console.0.borrow().pending_switch, Some(5));
}

#[test]
fn a_switch_key_leaves_a_release_not_yet_read_to_its_answer() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    // The kernel asks for VT 5; its SIGUSR2 is not read yet.
    console.0.borrow_mut().pending_switch = Some(5);
    assert!(vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
    // The kernel's worker has not run: the release is read and answered
    // first, and switches to VT 5.
    vt.release_signal();
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Release]);
    assert_eq!(console.0.borrow().on_screen, 5);
    assert!(!vt.is_active());
    // Then the worker switches on to the key's VT.
    console.run_worker();
    assert_eq!(console.0.borrow().on_screen, 2);
}

#[test]
fn leaving_goes_back_to_the_vt_shown_at_the_open() {
    let console = FakeConsole::new(3, 7, KbMode::Unicode);
    let mut vt = KernelVt::open(console.clone(), 3).unwrap_or_else(|e| panic!("open: {e}"));
    // Not shown: nothing to leave.
    assert_eq!(vt.leave().ok(), Some(Switch::Done));
    console.show(3);
    vt.acquire_signal();
    console.clear();
    assert_eq!(vt.leave().ok(), Some(Switch::Pending));
    assert_eq!(console.calls(), [Call::Active, Call::Activate(7)]);
    // Started on the VT it came from, there is nowhere to go back to.
    let (_, mut own) = shown();
    assert_eq!(own.leave().ok(), Some(Switch::Done));
}

#[test]
fn activating_switches_as_the_vt_shown_allows() {
    // From a VT in VT_AUTO and text mode, the switch completes and the
    // acquire is signalled.
    let console = FakeConsole::new(3, 5, KbMode::Unicode);
    let mut vt = KernelVt::open(console.clone(), 3).unwrap_or_else(|e| panic!("open: {e}"));
    assert_eq!(vt.activate().ok(), Some(Switch::Pending));
    console.run_worker();
    assert_eq!(console.0.borrow().on_screen, 3);
    assert_eq!(console.0.borrow().pending_switch, None);
    vt.acquire_signal();
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
    // From one in VT_AUTO and graphics mode, the kernel drops it.
    let console = FakeConsole::new(3, 5, KbMode::Unicode);
    console.0.borrow_mut().others = (Switching::Auto, DisplayMode::Graphics);
    let mut vt = KernelVt::open(console.clone(), 3).unwrap_or_else(|e| panic!("open: {e}"));
    assert_eq!(vt.activate().ok(), Some(Switch::Pending));
    console.run_worker();
    assert_eq!(console.0.borrow().on_screen, 5);
}

#[test]
fn activating_asks_only_when_another_vt_is_shown() {
    let (console, mut vt) = shown();
    assert_eq!(vt.activate().ok(), Some(Switch::Done));
    console.show(1);
    console.clear();
    assert_eq!(vt.activate().ok(), Some(Switch::Pending));
    assert_eq!(console.calls(), [Call::Active, Call::Activate(3)]);
}

#[test]
fn a_hangup_is_reported_once_and_ends_the_switching() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    assert!(
        vt.tty_ready(true, false, now()),
        "readable input is dropped"
    );
    assert_eq!(console.calls().last(), Some(&Call::Flush));
    assert!(!vt.tty_ready(true, true, now()));
    assert_eq!(
        vt.events().collect::<Vec<_>>(),
        [VtEvent::Retaken, VtEvent::Hangup]
    );
    for result in [vt.activate(), vt.leave(), vt.wait()] {
        assert_eq!(
            result.err().and_then(|e| e.raw_os_error()),
            Some(rustix::io::Errno::PIPE.raw_os_error())
        );
    }
    assert!(!vt.key(&key(ctrl_alt(), keysyms::KEY_F2)));
    // A hung-up VT is still put back as far as the kernel lets it.
    console.0.borrow_mut().hung_up = true;
    console.clear();
    drop(vt);
    assert_eq!(
        console.calls(),
        [
            Call::SetTermios(COOKED),
            Call::SetKeyboard(KbMode::Unicode),
            Call::SetSwitching(Switching::Auto),
            Call::SetDisplay(DisplayMode::Text),
        ]
    );
}

#[test]
fn a_flush_that_fails_is_a_hangup() {
    let (console, mut vt) = shown();
    vt.events().for_each(drop);
    console.fail(Some(Call::Flush));
    assert!(!vt.tty_ready(true, false, now()));
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Hangup]);
}

#[test]
fn the_restorer_puts_the_vt_back_without_the_drop() {
    let (console, vt) = shown();
    let restore = vt.restorer().unwrap_or_else(|e| panic!("restorer: {e}"));
    std::mem::forget(vt);
    assert_eq!(console.modes(), taken());
    restore.run();
    assert_eq!(console.modes(), untouched(KbMode::Unicode));
    // A second panic restores nothing again.
    console.clear();
    restore.run();
    assert!(console.calls().is_empty());
}

#[test]
fn restore_takes_the_vt_again() {
    let (console, mut vt) = shown();
    {
        let mut k = console.0.borrow_mut();
        k.display = DisplayMode::Text;
        k.switching = Switching::Auto;
        k.keyboard = KbMode::Unicode;
    }
    assert!(vt.restore().is_ok());
    assert_eq!(console.modes(), taken());
    drop(vt);
    assert_eq!(console.modes(), untouched(KbMode::Unicode));
}

#[test]
fn restore_sets_every_mode_it_can_and_keeps_a_switch_away() {
    let (console, mut vt) = shown();
    {
        let mut k = console.0.borrow_mut();
        k.display = DisplayMode::Text;
        k.switching = Switching::Auto;
        k.keyboard = KbMode::Unicode;
    }
    console.fail(Some(Call::SetDisplay(DisplayMode::Graphics)));
    assert!(vt.restore().is_err());
    assert_eq!(
        console.modes(),
        (DisplayMode::Text, Switching::Process, KbMode::Off)
    );
    console.fail(None);
    // While a release waits, VT_PROCESS stays as it is.
    asked_away(&console, &mut vt, 5);
    console.clear();
    assert!(vt.restore().is_ok());
    assert!(
        !console
            .calls()
            .iter()
            .any(|c| matches!(c, Call::SetSwitching(_)))
    );
    assert!(vt.acknowledge_release().is_ok());
    assert_eq!(console.0.borrow().on_screen, 5);
}

#[test]
fn the_bell_rings_the_console() {
    let (console, mut vt) = shown();
    vt.bell();
    assert_eq!(console.calls(), [Call::Bell]);
}

#[test]
fn the_fake_vt_is_taken_and_given_up_on_request_and_its_key() {
    let mut vt = FakeVt::new();
    assert!(!vt.is_active());
    assert_eq!(vt.number(), None);
    assert_eq!(vt.seat(), "seat0");
    assert_eq!(vt.wait().ok(), Some(Switch::Done));
    assert_eq!(vt.activate().ok(), Some(Switch::Pending));
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
    assert!(vt.is_active());
    let toggle = key(Mods::CTRL.union(Mods::LOGO), keysyms::KEY_F12);
    assert!(vt.key(&toggle));
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Release]);
    assert!(vt.acknowledge_release().is_ok());
    assert!(!vt.is_active());
    assert!(vt.key(&toggle));
    assert_eq!(vt.events().collect::<Vec<_>>(), [VtEvent::Acquired]);
    assert!(!vt.key(&key(ctrl_alt(), keysyms::KEY_F12)));
    assert_eq!(vt.leave().ok(), Some(Switch::Pending));
    assert!(vt.refuse_release().is_ok());
    assert!(vt.is_active());
}
