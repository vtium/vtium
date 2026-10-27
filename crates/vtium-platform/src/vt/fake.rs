//! The stand-in VT: records every call the VT makes and keeps the modes
//! they set, as the kernel would, and fails the call a test names.

use std::cell::RefCell;
use std::io;
use std::os::fd::OwnedFd;
use std::rc::Rc;

use rustix::process::Signal;
use rustix::runtime_448b8ad740e2a26f::{How, kernel_sigprocmask};

use super::{Console, DisplayMode, KbMode, Reldisp, Switching};

/// The stand-in's line settings: echo, and canonical input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lines {
    pub(crate) echo: bool,
    pub(crate) canonical: bool,
}

/// A console's line settings before anything changes them.
pub(crate) const COOKED: Lines = Lines {
    echo: true,
    canonical: true,
};

/// One call on the VT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Call {
    Active,
    SetDisplay(DisplayMode),
    Switching,
    SetSwitching(Switching),
    KeyboardMode,
    SetKeyboard(KbMode),
    Reldisp(Reldisp),
    Activate(u16),
    Flush,
    Termios,
    SetTermios(Lines),
    Bell,
}

#[derive(Debug)]
pub(crate) struct Kernel {
    /// The stand-in's VT, whose modes it keeps.
    pub(crate) num: u16,
    /// The VT on screen.
    pub(crate) on_screen: u16,
    /// Every other VT's switching and display mode.
    pub(crate) others: (Switching, DisplayMode),
    /// The VT a `VT_ACTIVATE` asked for, which the kernel's worker
    /// switches to later (`want_console`, vt.c:3171).
    pub(crate) wanted: Option<u16>,
    pub(crate) display: DisplayMode,
    pub(crate) switching: Switching,
    /// Whether `VT_GETMODE` shows vtium's own `VT_PROCESS`, with SIGUSR2
    /// to release and SIGUSR1 to acquire. False stands for another
    /// process's `VT_PROCESS`, or a mode the headers do not name; vtium's
    /// `VT_SETMODE` makes it true.
    pub(crate) signals_ours: bool,
    pub(crate) keyboard: KbMode,
    pub(crate) lines: Lines,
    pub(crate) calls: Vec<Call>,
    /// A call that fails with EIO each time it is made.
    pub(crate) fail: Option<Call>,
    /// Every call fails, as on a hung-up tty.
    pub(crate) hung_up: bool,
    /// A switch away the kernel waits on: any answer but a refusal
    /// completes it, as `vt_reldisp` takes every nonzero argument. With
    /// none, or without `VT_PROCESS`, every answer to a release is EINVAL;
    /// `VT_SETMODE` cancels it (vt_ioctl.c:559-566, 780).
    pub(crate) pending_switch: Option<u16>,
    /// Whether SIGUSR1 and SIGUSR2 were blocked in the calling thread when
    /// `VT_PROCESS` was set, so that neither could end the process.
    pub(crate) signals_blocked_at_process: Option<bool>,
}

/// The stand-in, shared between the VT and the test.
#[derive(Clone, Debug)]
pub(crate) struct FakeConsole(pub(crate) Rc<RefCell<Kernel>>);

impl FakeConsole {
    /// VT `num`, its console in text mode with `keyboard` and switched by
    /// the kernel, while VT `on_screen` is shown; every other VT is in
    /// text mode and switched by the kernel.
    pub(crate) fn new(num: u16, on_screen: u16, keyboard: KbMode) -> FakeConsole {
        FakeConsole(Rc::new(RefCell::new(Kernel {
            num,
            on_screen,
            others: (Switching::Auto, DisplayMode::Text),
            wanted: None,
            display: DisplayMode::Text,
            switching: Switching::Auto,
            signals_ours: true,
            keyboard,
            lines: COOKED,
            calls: Vec::new(),
            fail: None,
            hung_up: false,
            pending_switch: None,
            signals_blocked_at_process: None,
        })))
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.0.borrow().calls.clone()
    }

    pub(crate) fn clear(&self) {
        self.0.borrow_mut().calls.clear();
    }

    pub(crate) fn fail(&self, call: Option<Call>) {
        self.0.borrow_mut().fail = call;
    }

    /// The modes the VT is left in: display, switching, keyboard.
    pub(crate) fn modes(&self) -> (DisplayMode, Switching, KbMode) {
        let k = self.0.borrow();
        (k.display, k.switching, k.keyboard)
    }

    pub(crate) fn show(&self, num: u16) {
        self.0.borrow_mut().on_screen = num;
    }

    /// The kernel's worker switches to the VT a `VT_ACTIVATE` asked for, as
    /// `change_console` does from the VT on screen: under `VT_PROCESS` it
    /// waits for that VT's answer, under `VT_AUTO` it completes, except in
    /// `KD_GRAPHICS` (vt.c:3118-3126; vt_ioctl.c:1223-1240, 1259-1263).
    /// Only the stand-in's own VT keeps a switch waiting, and a switch to
    /// it is left for the test to signal.
    pub(crate) fn run_worker(&self) {
        let mut k = self.0.borrow_mut();
        let Some(want) = k.wanted.take() else {
            return;
        };
        if want == k.on_screen {
            return;
        }
        let own = k.on_screen == k.num;
        match shown_modes(&k) {
            (Switching::Process, _) if own => k.pending_switch = Some(want),
            (Switching::Process, _) | (Switching::Auto, DisplayMode::Graphics) => {}
            (Switching::Auto, DisplayMode::Text) => k.on_screen = want,
        }
    }

    fn call(&self, call: Call) -> io::Result<()> {
        let mut k = self.0.borrow_mut();
        k.calls.push(call);
        if k.hung_up || k.fail == Some(call) {
            return Err(io::Error::from(rustix::io::Errno::IO));
        }
        Ok(())
    }
}

impl Console for FakeConsole {
    type Termios = Lines;

    fn active(&self) -> io::Result<u16> {
        self.call(Call::Active)?;
        Ok(self.0.borrow().on_screen)
    }

    fn set_display_mode(&self, mode: DisplayMode) -> io::Result<()> {
        self.call(Call::SetDisplay(mode))?;
        self.0.borrow_mut().display = mode;
        Ok(())
    }

    fn switching_is_ours(&self) -> io::Result<bool> {
        self.call(Call::Switching)?;
        let k = self.0.borrow();
        Ok(k.switching == Switching::Process && k.signals_ours)
    }

    fn set_switching(&self, mode: Switching) -> io::Result<()> {
        if mode == Switching::Process {
            self.0.borrow_mut().signals_blocked_at_process = Some(signals_blocked());
        }
        self.call(Call::SetSwitching(mode))?;
        let mut k = self.0.borrow_mut();
        k.switching = mode;
        k.signals_ours = true;
        k.pending_switch = None;
        Ok(())
    }

    fn keyboard_mode(&self) -> io::Result<KbMode> {
        self.call(Call::KeyboardMode)?;
        Ok(self.0.borrow().keyboard)
    }

    fn set_keyboard_mode(&self, mode: KbMode) -> io::Result<()> {
        self.call(Call::SetKeyboard(mode))?;
        self.0.borrow_mut().keyboard = mode;
        Ok(())
    }

    fn reldisp(&self, answer: Reldisp) -> io::Result<()> {
        self.call(Call::Reldisp(answer))?;
        let mut k = self.0.borrow_mut();
        if k.switching != Switching::Process {
            return Err(io::Error::from(rustix::io::Errno::INVAL));
        }
        let Some(target) = k.pending_switch.take() else {
            return Err(io::Error::from(rustix::io::Errno::INVAL));
        };
        if answer != Reldisp::Refuse {
            k.on_screen = target;
        }
        Ok(())
    }

    /// `set_console`: dropped while the VT on screen is in `VT_AUTO` and
    /// `KD_GRAPHICS`, otherwise left for the kernel's worker
    /// ([`FakeConsole::run_worker`]), with no answer either way
    /// (vt.c:3153-3172).
    fn activate(&self, num: u16) -> io::Result<()> {
        self.call(Call::Activate(num))?;
        let mut k = self.0.borrow_mut();
        if shown_modes(&k) != (Switching::Auto, DisplayMode::Graphics) {
            k.wanted = Some(num);
        }
        Ok(())
    }

    fn flush_input(&self) -> io::Result<()> {
        self.call(Call::Flush)
    }

    fn termios(&self) -> io::Result<Lines> {
        self.call(Call::Termios)?;
        Ok(self.0.borrow().lines)
    }

    fn set_termios(&self, lines: &Lines) -> io::Result<()> {
        self.call(Call::SetTermios(*lines))?;
        self.0.borrow_mut().lines = *lines;
        Ok(())
    }

    fn silent(_: &Lines) -> Lines {
        Lines {
            echo: false,
            canonical: false,
        }
    }

    fn bell(&self) -> io::Result<()> {
        self.call(Call::Bell)
    }

    fn try_clone(&self) -> io::Result<FakeConsole> {
        Ok(self.clone())
    }

    fn watch(&self) -> Option<io::Result<OwnedFd>> {
        None
    }
}

/// The switching and display mode of the VT on screen.
fn shown_modes(k: &Kernel) -> (Switching, DisplayMode) {
    if k.on_screen == k.num {
        (k.switching, k.display)
    } else {
        k.others
    }
}

/// Whether the calling thread blocks both switch signals.
fn signals_blocked() -> bool {
    #[expect(unsafe_code, reason = "reading the thread's signal mask")]
    // SAFETY: with no set, rt_sigprocmask only reads the mask and changes
    // nothing, so no signal libc reserves is touched.
    let mask = unsafe { kernel_sigprocmask(How::BLOCK, None) };
    mask.is_ok_and(|m| m.contains(Signal::USR1) && m.contains(Signal::USR2))
}
