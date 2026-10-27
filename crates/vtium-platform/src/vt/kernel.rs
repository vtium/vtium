//! The kernel's VT: `VT_PROCESS` switching on SIGUSR1 and SIGUSR2, the
//! display in graphics mode and the keyboard off, and the guard that puts
//! them back.

use std::io;
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use calloop::generic::Generic;
use calloop::{Interest, Mode, PostAction};
use xkbcommon::xkb::keysyms;

use super::{Console, DisplayMode, KbMode, Reldisp, Switch, Switching, Tty, Vt, VtEvent};
use crate::event_loop::{Loop, Signal};
use crate::input::{KeyEvent, Mods};

/// Whether the VT is ours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Inactive,
    Active,
    /// The kernel asked for the VT, and the owner has not answered yet.
    Releasing,
}

/// What the guard has set and puts back when dropped.
#[derive(Clone, Copy, Debug, Default)]
struct Set {
    graphics: bool,
    process: bool,
    keyboard: bool,
}

/// The kernel's VT, held from [`KernelVt::new`] until dropped.
///
/// Dropping it restores the tty's line settings and the keyboard mode it
/// found, `VT_AUTO` and `KD_TEXT`, each only where it changed it, so an
/// open that fails half way puts back what it set.
#[derive(Debug)]
#[expect(private_bounds, reason = "the console is a Tty outside the crate")]
pub struct KernelVt<C: Console = Tty> {
    console: C,
    num: u16,
    /// The VT on screen at the open, which [`Vt::leave`] goes back to.
    saved: u16,
    /// The keyboard mode found at the open, restored when dropped.
    kb_mode: KbMode,
    /// The tty's line settings found at the open, when the open made it
    /// silent; restored when dropped. `None` when it could not, and then
    /// nothing silences the tty later.
    termios: Option<C::Termios>,
    set: Set,
    state: State,
    /// A re-take while a release waits, reported if the owner refuses it.
    retaken: bool,
    retakes: Retakes,
    hup: bool,
    events: Vec<VtEvent>,
}

impl KernelVt<Tty> {
    /// Takes VT `num` through `tty`, as [`Tty::open`] opened it, in the
    /// loop. The release and acquire signals go into the loop's signalfd
    /// first, which blocks them in this thread before `VT_PROCESS` asks the
    /// kernel to send them, as the C reference registered them before the
    /// open; otherwise a switch at that moment would end the process with
    /// the VT taken. Call it before any thread starts ([`Loop::on_signal`]),
    /// and store the VT where `access` finds it before the loop dispatches
    /// again: a signal that comes in between waits in the signalfd. The
    /// tty is watched: input on it takes the VT again ([`VtEvent::Retaken`])
    /// and a hangup is reported. So is tty0's `active`, the VT the kernel
    /// shows ([`KernelVt::shown_changed`]); without it, vtium runs on with a
    /// warning.
    pub fn new<D, A>(lp: &mut Loop<D>, tty: Tty, num: u16, access: A) -> io::Result<KernelVt<Tty>>
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut KernelVt<Tty>> + Clone + 'static,
    {
        KernelVt::start(lp, tty, num, access)
    }
}

#[expect(private_bounds, reason = "the console is a Tty outside the crate")]
impl<C: Console + 'static> KernelVt<C> {
    /// The signals, then the open, then the tty in the loop.
    pub(crate) fn start<D, A>(lp: &mut Loop<D>, console: C, num: u16, access: A) -> io::Result<Self>
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut KernelVt<C>> + Clone + 'static,
    {
        let acquire = access.clone();
        lp.on_signal(Signal::SIGUSR1, move |data| {
            if let Some(vt) = acquire(data) {
                vt.acquire_signal();
            }
        })?;
        let release = access.clone();
        lp.on_signal(Signal::SIGUSR2, move |data| {
            if let Some(vt) = release(data) {
                vt.release_signal();
            }
        })?;
        let vt = KernelVt::open(console, num)?;
        match vt.console.watch_shown() {
            None => {}
            Some(Err(e)) => warn_unwatched(&e),
            Some(Ok(fd)) => {
                let shown = access.clone();
                // Edge-triggered: sysfs reports the attribute readable at
                // every poll, and wakes its pollers on each notification.
                let inserted = lp.handle().insert_source(
                    Generic::new(fd, Interest::READ, Mode::Edge),
                    move |_, fd, data: &mut D| {
                        let Some(vt) = shown(data) else {
                            return Ok(PostAction::Remove);
                        };
                        match super::shown::read(fd.as_fd()) {
                            Ok(n) => vt.shown_changed(n),
                            Err(e) => log::warn!(target: "vt", "cannot read the VT shown: {e}"),
                        }
                        Ok(PostAction::Continue)
                    },
                );
                if let Err(e) = inserted {
                    warn_unwatched(&io::Error::from(e.error));
                }
            }
        }
        if let Some(fd) = vt.console.watch() {
            lp.handle()
                .insert_source(
                    Generic::new(fd?, Interest::READ, Mode::Level),
                    move |readiness, _, data: &mut D| {
                        let Some(vt) = access(data) else {
                            return Ok(PostAction::Remove);
                        };
                        #[expect(
                            clippy::disallowed_methods,
                            reason = "the re-take's log reads the clock, held in vtium-platform"
                        )]
                        let now = Instant::now();
                        Ok(if vt.tty_ready(readiness.readable, readiness.error, now) {
                            PostAction::Continue
                        } else {
                            PostAction::Remove
                        })
                    },
                )
                .map_err(|e| io::Error::from(e.error))?;
        }
        Ok(vt)
    }
}

#[expect(private_bounds, reason = "the console is a Tty outside the crate")]
impl<C: Console> KernelVt<C> {
    /// Takes the VT: graphics mode, `VT_PROCESS` switching, the keyboard
    /// mode saved and set to `K_RAW`, then `K_OFF`, so keys reach only
    /// evdev (vt_real.c:261-361). A VT on screen now is reported
    /// [`VtEvent::Acquired`] at the first drain.
    pub(crate) fn open(console: C, num: u16) -> io::Result<KernelVt<C>> {
        let mut vt = KernelVt {
            console,
            num,
            saved: 0,
            kb_mode: KbMode::Unicode,
            termios: None,
            set: Set::default(),
            state: State::Inactive,
            retaken: false,
            retakes: Retakes::default(),
            hup: false,
            events: Vec::new(),
        };
        vt.saved = vt.console.active().inspect_err(|e| {
            log::warn!(target: "vt", "cannot find the VT on screen: {e}");
        })?;
        vt.console
            .set_display_mode(DisplayMode::Graphics)
            .inspect_err(
                |e| log::error!(target: "vt", "cannot put VT {num} in graphics mode: {e}"),
            )?;
        vt.set.graphics = true;
        vt.console
            .set_switching(Switching::Process)
            .inspect_err(|e| log::error!(target: "vt", "cannot take VT {num}'s switching: {e}"))?;
        vt.set.process = true;
        let kb_mode = vt.console.keyboard_mode().inspect_err(|e| {
            log::error!(target: "vt", "cannot read VT {num}'s keyboard mode: {e}");
        })?;
        log::debug!(target: "vt", "VT {num}'s keyboard mode was {kb_mode:?}");
        vt.kb_mode = match kb_mode {
            KbMode::Off | KbMode::Raw | KbMode::MediumRaw => {
                log::warn!(target: "vt", "VT {num}'s keyboard mode was {kb_mode:?}, restoring K_UNICODE");
                KbMode::Unicode
            }
            other => other,
        };
        vt.keyboard_off()?;
        vt.silence();
        if vt.saved == num {
            vt.state = State::Active;
            vt.events.push(VtEvent::Acquired);
        }
        Ok(vt)
    }

    /// `K_RAW`, then `K_OFF`, whose failure leaves `K_RAW` with a warning.
    fn keyboard_off(&mut self) -> io::Result<()> {
        let num = self.num;
        self.console.set_keyboard_mode(KbMode::Raw).inspect_err(
            |e| log::error!(target: "vt", "cannot set VT {num}'s keyboard to K_RAW: {e}"),
        )?;
        self.set.keyboard = true;
        if let Err(e) = self.console.set_keyboard_mode(KbMode::Off) {
            log::warn!(target: "vt", "cannot set VT {num}'s keyboard to K_OFF: {e}");
        }
        Ok(())
    }

    /// At the open: raw mode with no echo on the tty, the line settings
    /// found kept to restore. A failure leaves the tty as it is, with a
    /// warning, and nothing silences it later.
    fn silence(&mut self) {
        let num = self.num;
        let found = match self.console.termios() {
            Ok(found) => found,
            Err(e) => {
                log::warn!(target: "vt", "cannot read VT {num}'s line settings: {e}");
                return;
            }
        };
        match self.console.set_termios(&C::silent(&found)) {
            Ok(()) => self.termios = Some(found),
            Err(e) => log::warn!(target: "vt", "cannot silence VT {num}'s tty: {e}"),
        }
    }

    /// The tty silent again, when the open made it so.
    fn silence_again(&self) -> io::Result<()> {
        match &self.termios {
            Some(found) => self.console.set_termios(&C::silent(found)),
            None => Ok(()),
        }
    }

    /// Every mode vtium set, each tried on its own so one refusal leaves
    /// the rest set; returns the first refusal. `VT_PROCESS` is set only
    /// where it is not vtium's own ([`KernelVt::take_switching`]).
    fn retake(&mut self) -> io::Result<()> {
        let num = self.num;
        let logged = |what: &str, r: io::Result<()>| {
            r.inspect_err(|e| log::error!(target: "vt", "cannot take VT {num}'s {what} again: {e}"))
        };
        let graphics = logged(
            "graphics mode",
            self.console.set_display_mode(DisplayMode::Graphics),
        );
        self.set.graphics |= graphics.is_ok();
        let switching = logged("switching", self.take_switching());
        // keyboard_off logs its own failures.
        let keyboard = self.keyboard_off();
        let silent = logged("silent tty", self.silence_again());
        graphics.and(switching).and(keyboard).and(silent)
    }

    /// vtium's `VT_PROCESS` on a VT found in any other mode; a mode that
    /// cannot be read is left as it is. vtium's own `VT_PROCESS` is left
    /// as it is: `VT_SETMODE` would cancel a switch the kernel waits on
    /// (vt_ioctl.c:780), and the kernel waits on vtium only there
    /// (559-566).
    fn take_switching(&mut self) -> io::Result<()> {
        if self.console.switching_is_ours()? {
            return Ok(());
        }
        log::warn!(target: "vt", "VT {}'s switching was not vtium's, taking it", self.num);
        self.console.set_switching(Switching::Process)?;
        self.set.process = true;
        Ok(())
    }

    /// Whether VT `num` is on screen; `None` when the kernel cannot say.
    fn on_screen(&self) -> Option<bool> {
        match self.console.active() {
            Ok(active) => Some(active == self.num),
            Err(e) => {
                log::warn!(target: "vt", "cannot find the VT on screen: {e}");
                None
            }
        }
    }

    /// SIGUSR1: the VT was switched to (vt_real.c:109-139).
    pub(crate) fn acquire_signal(&mut self) {
        if self.on_screen() != Some(true) {
            return;
        }
        // No VT_ACKACQ: the kernel needs none for an acquire, and while a
        // switch away is pending, which a release signal read after this one
        // reports, it takes any answer but a refusal as allowing it.
        if self.state == State::Releasing {
            log::debug!(target: "vt", "VT {}'s acquire while its release waits, ignored", self.num);
            return;
        }
        log::debug!(target: "vt", "entering VT {} on its signal", self.num);
        if self.state == State::Active {
            log::debug!(target: "vt", "VT {} is ours already", self.num);
        } else {
            self.state = State::Active;
            self.events.push(VtEvent::Acquired);
        }
    }

    /// SIGUSR2: the VT is asked for (vt_real.c:141-178). The answer waits
    /// for the owner, which acknowledges once the displays are asleep.
    pub(crate) fn release_signal(&mut self) {
        if self.on_screen() != Some(true) {
            return;
        }
        log::debug!(target: "vt", "leaving VT {} on its signal", self.num);
        match self.state {
            State::Active => {
                self.state = State::Releasing;
                self.events.push(VtEvent::Release);
            }
            State::Releasing => {}
            State::Inactive => {
                log::warn!(target: "vt", "leaving VT {} though it is not ours", self.num);
                self.answer(Reldisp::Allow);
            }
        }
    }

    fn answer(&self, answer: Reldisp) {
        if let Err(e) = self.console.reldisp(answer) {
            log::warn!(target: "vt", "cannot answer VT {}'s release: {e}", self.num);
        }
    }

    /// After `VT_RELDISP` failed, the kernel no longer waits on the
    /// release, as after another process set the VT's switching while it
    /// waited (vt_ioctl.c:559-566, 780). Returns whether the VT is still
    /// shown, or the error when the kernel cannot say. The caller takes the
    /// VT again either way: `VT_SETMODE` on a VT not shown cancels nothing,
    /// and the next switch to it signals the acquire.
    fn release_gone(&self, e: io::Error) -> io::Result<bool> {
        log::warn!(target: "vt", "cannot answer VT {}'s release: {e}", self.num);
        let shown = self.on_screen().ok_or(e)?;
        log::warn!(target: "vt", "taking VT {} again, {}", self.num, if shown { "still shown" } else { "in the background" });
        Ok(shown)
    }

    /// The tty is readable or failed. vtium keeps its tty silent, so
    /// anything arriving on it means the VT is no longer as vtium set it:
    /// the VT is taken again and the input dropped, since the keys come
    /// from evdev. While the VT is ours the re-take is reported as
    /// [`VtEvent::Retaken`]; one while a release waits is reported if the
    /// owner refuses the release, and one while the VT is not ours is not
    /// reported, since the next [`VtEvent::Acquired`] sets everything up. A
    /// VT found shown while not ours is acquired.
    /// The re-takes are logged as [`Retakes`] says. A hangup, or a flush
    /// that fails as on a hung-up tty, is reported once. Returns whether to
    /// keep watching it.
    pub(crate) fn tty_ready(&mut self, readable: bool, error: bool, now: Instant) -> bool {
        if readable && !error {
            if self.retakes.note(now) {
                log::warn!(target: "vt", "input on VT {}'s tty, taking the VT again", self.num);
            } else {
                log::debug!(target: "vt", "taking VT {} again, {} times since the warning", self.num, self.retakes.count);
            }
            // retake logs each refusal.
            let _ = self.retake();
            if self.console.flush_input().is_ok() {
                match self.state {
                    State::Active => self.events.push(VtEvent::Retaken),
                    State::Releasing => self.retaken = true,
                    // Shown with no acquire signal, as when another process
                    // set VT_AUTO while the VT was not ours.
                    State::Inactive if self.on_screen() == Some(true) => {
                        self.state = State::Active;
                        self.events.push(VtEvent::Acquired);
                    }
                    State::Inactive => {}
                }
                return true;
            }
        }
        if !readable && !error {
            return true;
        }
        log::debug!(target: "vt", "hangup on VT {}", self.num);
        self.hup = true;
        self.events.push(VtEvent::Hangup);
        false
    }

    /// tty0's `active` named VT `shown` after a switch. The kernel
    /// notifies before it signals the acquire (vt_ioctl.c:1140, 1170) and
    /// the loop reads the two in either order; whichever comes second
    /// changes nothing. This VT shown while not ours, as when another
    /// process set `VT_AUTO` while it was not ours, is taken again and
    /// reported as [`VtEvent::Acquired`]. Another VT shown while this one
    /// is ours, a switch the kernel made with no release, is reported as
    /// [`VtEvent::Release`]; its acknowledgement then decides as for any
    /// release the kernel no longer waits on. While a release waits, the
    /// answer decides.
    pub(crate) fn shown_changed(&mut self, shown: u16) {
        if self.hup {
            return;
        }
        let num = self.num;
        match self.state {
            State::Inactive if shown == num => {
                log::debug!(target: "vt", "VT {num} shown, entering it");
                self.state = State::Active;
                self.events.push(VtEvent::Acquired);
                // retake logs each refusal.
                let _ = self.retake();
            }
            State::Active if shown != num => {
                log::warn!(target: "vt", "VT {shown} shown with no release of VT {num}, leaving it");
                self.state = State::Releasing;
                self.retaken = false;
                self.events.push(VtEvent::Release);
            }
            State::Inactive | State::Active | State::Releasing => {}
        }
    }

    /// A [`Restore`] for a panic hook, holding its own handle on the VT.
    pub fn restorer(&self) -> io::Result<Restore<C>> {
        Ok(Restore {
            console: self.console.try_clone()?,
            num: self.num,
            kb_mode: self.kb_mode,
            termios: self.termios.clone(),
            state: AtomicU8::new(RESTORE_IDLE),
        })
    }

    fn hung_up(&self) -> io::Result<()> {
        if self.hup {
            Err(io::Error::from(rustix::io::Errno::PIPE))
        } else {
            Ok(())
        }
    }
}

impl<C: Console> Vt for KernelVt<C> {
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
        self.events.drain(..)
    }

    /// A release the kernel no longer waits on is taken again: a VT still
    /// shown is reported as [`VtEvent::Acquired`], since the owner put
    /// everything to sleep, and one no longer shown stays released. Fails
    /// only when the kernel can say neither.
    fn acknowledge_release(&mut self) -> io::Result<()> {
        if self.state != State::Releasing {
            return Ok(());
        }
        self.state = State::Inactive;
        self.retaken = false;
        let Err(e) = self.console.reldisp(Reldisp::Allow) else {
            return Ok(());
        };
        if self.release_gone(e)? {
            self.state = State::Active;
            self.events.push(VtEvent::Acquired);
        }
        // retake logs each refusal.
        let _ = self.retake();
        Ok(())
    }

    /// A re-take while the release waited is reported as
    /// [`VtEvent::Retaken`]. A release the kernel no longer waits on is
    /// taken again: a VT still shown is reported as [`VtEvent::Retaken`],
    /// and one no longer shown as [`VtEvent::Release`], whose
    /// acknowledgement then decides as for any release gone, so a switch
    /// back while the owner sleeps is acquired only then. Fails only when
    /// the kernel can say neither.
    fn refuse_release(&mut self) -> io::Result<()> {
        if self.state != State::Releasing {
            return Ok(());
        }
        self.state = State::Active;
        log::debug!(target: "vt", "not leaving VT {}", self.num);
        let retaken = std::mem::take(&mut self.retaken);
        let Err(e) = self.console.reldisp(Reldisp::Refuse) else {
            if retaken {
                self.events.push(VtEvent::Retaken);
            }
            return Ok(());
        };
        let shown = self.release_gone(e)?;
        // While Active, so VT_PROCESS is set too; retake logs each refusal.
        let _ = self.retake();
        if shown {
            self.events.push(VtEvent::Retaken);
        } else {
            self.state = State::Releasing;
            self.events.push(VtEvent::Release);
        }
        Ok(())
    }

    fn activate(&mut self) -> io::Result<Switch> {
        self.hung_up()?;
        if self.on_screen() == Some(true) {
            return Ok(Switch::Done);
        }
        if self.state == State::Active {
            log::warn!(target: "vt", "entering VT {} though it is ours", self.num);
        }
        self.console.activate(self.num).inspect_err(|e| {
            log::warn!(target: "vt", "cannot enter VT {}: {e}", self.num);
        })?;
        log::debug!(target: "vt", "entering VT {} on demand", self.num);
        Ok(Switch::Pending)
    }

    fn leave(&mut self) -> io::Result<Switch> {
        self.hung_up()?;
        let active = self.console.active()?;
        if active != self.num || active == self.saved {
            return Ok(Switch::Done);
        }
        if self.state == State::Inactive {
            log::warn!(target: "vt", "leaving VT {} though it is not ours", self.num);
        }
        self.console.activate(self.saved).inspect_err(|e| {
            log::warn!(target: "vt", "cannot leave VT {} for VT {}: {e}", self.num, self.saved);
        })?;
        log::debug!(target: "vt", "leaving VT {} on demand for VT {}", self.num, self.saved);
        Ok(Switch::Pending)
    }

    fn wait(&mut self) -> io::Result<Switch> {
        self.hung_up()?;
        if self.console.active()? == self.num {
            return Ok(Switch::Done);
        }
        log::info!(target: "vt", "waiting for VT {} to be shown", self.num);
        Ok(Switch::Pending)
    }

    fn is_active(&self) -> bool {
        self.state == State::Active
    }

    /// Ctrl+Alt+F1 to F12 and the XF86Switch_VT keys ask the kernel for
    /// that VT (vt_real.c:484-530); this VT's own key is taken and does
    /// nothing. Unless `VT_GETMODE` shows vtium's own `VT_PROCESS`, the
    /// key sets it first: the kernel drops a switch from a VT in `VT_AUTO`
    /// and `KD_GRAPHICS`, as another process can leave it (vt.c:3153-3172),
    /// and sends another process's `VT_PROCESS` the release (vt_ioctl.c:
    /// 1223-1240). No release of vtium's can be waiting then: the kernel
    /// waits on one only in vtium's `VT_PROCESS` (559-566), and the
    /// `VT_SETMODE` that ended it cleared the wait (780). vtium's own
    /// `VT_PROCESS` is left as it is: a release not yet read may be
    /// waiting, and the kernel's worker asks for the new switch only later
    /// (vt.c:3118-3126). Another process using vtium's signals cannot be
    /// told from vtium.
    fn key(&mut self, key: &KeyEvent) -> bool {
        let Some(target) = switch_target(key) else {
            return false;
        };
        if self.state != State::Active || self.hup || self.on_screen() != Some(true) {
            return false;
        }
        if target != self.num {
            log::debug!(target: "vt", "leaving VT {} for VT {target} on a key", self.num);
            if let Err(e) = self.take_switching() {
                log::warn!(target: "vt", "cannot take VT {}'s switching: {e}", self.num);
            }
            if let Err(e) = self.console.activate(target) {
                log::warn!(target: "vt", "cannot leave VT {} for VT {target}: {e}", self.num);
            }
        }
        true
    }

    /// Graphics mode, `VT_PROCESS`, the keyboard off and the tty silent
    /// again (vt_real.c:227-259), as a re-take sets them: each tried on its
    /// own, and `VT_PROCESS` left as it is where it is vtium's own.
    fn restore(&mut self) -> io::Result<()> {
        self.retake()
    }

    fn bell(&mut self) {
        if let Err(e) = self.console.bell() {
            log::warn!(target: "vt", "cannot ring VT {}'s bell: {e}", self.num);
        }
    }

    fn number(&self) -> Option<u16> {
        Some(self.num)
    }
}

impl<C: Console> Drop for KernelVt<C> {
    /// The tty's line settings, the keyboard mode, `VT_AUTO` and `KD_TEXT`,
    /// each where it was set (vt_real.c:363-400); a hung-up VT fails them
    /// quietly. Then SIGUSR1 and SIGUSR2 are ignored: the loop's signalfd
    /// unblocks them when the loop drops, which comes after the VT's, and a
    /// release still pending would otherwise end the process by default.
    fn drop(&mut self) {
        log::debug!(target: "vt", "closing VT {}", self.num);
        let quiet = self.hup;
        let saved = Saved {
            kb_mode: self.kb_mode,
            termios: self.termios.as_ref(),
        };
        put_back(&self.console, self.num, &saved, self.set, quiet);
        for signal in [rustix::process::Signal::USR1, rustix::process::Signal::USR2] {
            if let Err(e) = ignore(signal) {
                log::warn!(target: "vt", "cannot ignore {signal:?}: {e}");
            }
        }
    }
}

/// Without tty0's `active`, a switch the kernel makes with no signal goes
/// unseen until the next signal, key or tty input.
fn warn_unwatched(e: &io::Error) {
    log::warn!(target: "vt", "cannot watch the VT shown, running without it: {e}");
}

/// Sets `signal` to be ignored, which also discards one pending.
fn ignore(signal: rustix::process::Signal) -> io::Result<()> {
    use rustix::runtime_448b8ad740e2a26f::{kernel_sig_ign, kernel_sigaction};
    #[expect(unsafe_code, reason = "setting a signal's disposition")]
    // SAFETY: rt_sigaction reads the current action, and the same action
    // is set back with SIG_IGN as its handler: no handler function runs,
    // and SIGUSR1 and SIGUSR2 are neither reserved by libc nor relied on
    // elsewhere in the process once the VT is put back.
    unsafe {
        let mut action = kernel_sigaction(signal, None)?;
        action.sa_handler_kernel = kernel_sig_ign();
        kernel_sigaction(signal, Some(action))?;
    }
    Ok(())
}

/// The re-takes, for the log: the first is a warning, and so is the first
/// after [`crate::RETAKE_QUIET_MS`] with none; the others are counted at
/// debug level.
#[derive(Debug, Default)]
pub(super) struct Retakes {
    last: Option<Instant>,
    /// Re-takes since the last warning, that one included.
    count: u64,
}

impl Retakes {
    /// Counts a re-take at `now`; returns whether it is logged as a warning.
    pub(super) fn note(&mut self, now: Instant) -> bool {
        let quiet =
            Duration::from_millis(u64::try_from(crate::RETAKE_QUIET_MS).unwrap_or(u64::MAX));
        let warn = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= quiet);
        self.last = Some(now);
        self.count = if warn {
            1
        } else {
            self.count.saturating_add(1)
        };
        warn
    }
}

/// What the VT was found in.
struct Saved<'a, T> {
    kb_mode: KbMode,
    /// `None` when the tty was never made silent.
    termios: Option<&'a T>,
}

/// What a drop puts back: the tty's line settings, then the C reference's
/// order.
fn put_back<C: Console>(
    console: &C,
    num: u16,
    saved: &Saved<'_, C::Termios>,
    set: Set,
    quiet: bool,
) {
    let report = |what: &str, e: io::Error| {
        if !quiet {
            log::warn!(target: "vt", "cannot reset VT {num}'s {what}: {e}");
        }
    };
    if let Some(termios) = saved.termios {
        if let Err(e) = console.set_termios(termios) {
            report("line settings", e);
        }
    }
    if set.keyboard {
        if let Err(e) = console.set_keyboard_mode(saved.kb_mode) {
            report("keyboard mode", e);
        }
    }
    if set.process {
        if let Err(e) = console.set_switching(Switching::Auto) {
            report("switching to VT_AUTO", e);
        }
    }
    if set.graphics {
        if let Err(e) = console.set_display_mode(DisplayMode::Text) {
            report("display to text mode", e);
        }
    }
}

/// The restore a [`KernelVt`]'s drop does, for a panic hook: the release
/// and minspec profiles abort on a panic, and an abort runs no `Drop`. The
/// binary calls [`Restore::run`] from its hook.
#[derive(Debug)]
#[expect(private_bounds, reason = "the console is a Tty outside the crate")]
pub struct Restore<C: Console = Tty> {
    console: C,
    num: u16,
    kb_mode: KbMode,
    termios: Option<C::Termios>,
    /// Idle, running or done: the first run restores, and a run in
    /// another thread waits for it to finish, since its hook's return
    /// aborts the process.
    state: AtomicU8,
}

const RESTORE_IDLE: u8 = 0;
const RESTORE_RUNNING: u8 = 1;
const RESTORE_DONE: u8 = 2;

#[expect(private_bounds, reason = "the console is a Tty outside the crate")]
impl<C: Console> Restore<C> {
    /// Puts back the tty's line settings, the keyboard mode, `VT_AUTO` and
    /// `KD_TEXT`, the first time it is called. A call in another thread
    /// while the first runs returns once it has finished; a later call
    /// returns at once.
    pub fn run(&self) {
        let claimed = self.state.compare_exchange(
            RESTORE_IDLE,
            RESTORE_RUNNING,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        if claimed.is_err() {
            while self.state.load(Ordering::SeqCst) != RESTORE_DONE {
                std::thread::yield_now();
            }
            return;
        }
        let all = Set {
            graphics: true,
            process: true,
            keyboard: true,
        };
        let saved = Saved {
            kb_mode: self.kb_mode,
            termios: self.termios.as_ref(),
        };
        put_back(&self.console, self.num, &saved, all, false);
        self.state.store(RESTORE_DONE, Ordering::SeqCst);
    }
}

/// The VT a switch key names: Ctrl+Alt+F1 to F12, or XF86Switch_VT_1 to
/// 12.
fn switch_target(key: &KeyEvent) -> Option<u16> {
    let sym = *key.keysyms.first()?;
    let ctrl_alt = key.mods.contains(Mods::CTRL) && key.mods.contains(Mods::ALT);
    let base = if ctrl_alt && (keysyms::KEY_F1..=keysyms::KEY_F12).contains(&sym) {
        keysyms::KEY_F1
    } else if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&sym) {
        keysyms::KEY_XF86Switch_VT_1
    } else {
        return None;
    };
    u16::try_from(sym.checked_sub(base)?.checked_add(1)?).ok()
}
