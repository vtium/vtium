//! The virtual terminal: who holds the screen and the keyboard.
//!
//! A seat has one [`Vt`] backend, chosen at start as the C reference chose
//! it (vt.c:84-110): libseat when configured and connected (vtium/vtium#34),
//! otherwise the kernel's VT ([`KernelVt`]), otherwise a fake VT that is
//! the seat's only session ([`FakeVt`]). The backend reports through
//! [`VtEvent`]s the owner drains once a turn, as the displays do:
//!
//! - [`VtEvent::Acquired`]: the VT is ours. The owner wakes the input
//!   devices and the displays; the wake-up counts as user activity.
//! - [`VtEvent::Release`]: the kernel asks for the VT. The owner puts the
//!   input devices and the displays to sleep, and acknowledges the release
//!   with [`Vt::acknowledge_release`] once every display reports
//!   [`crate::display::Event::Asleep`], never before; or refuses it with
//!   [`Vt::refuse_release`]. A release the kernel no longer waits on, as
//!   after another process set the VT's switching meanwhile, is taken
//!   again: on a VT still shown the acknowledgement reports `Acquired` and
//!   a refusal `Retaken`; on one no longer shown a refusal reports
//!   `Release`.
//! - [`VtEvent::Hangup`]: the VT is gone. The owner exits.
//! - [`VtEvent::Retaken`]: vtium took the VT again while it is ours. The
//!   owner sets the displays up again and draws whole frames. A re-take
//!   while a release waits is reported if the owner refuses the release;
//!   one while the VT is not ours is not reported.
//!
//! [`KernelVt`] owns the VT's state and restores its tty's line settings,
//! the keyboard mode, `KD_TEXT` and `VT_AUTO` when dropped, so every return
//! after the VT is opened restores them, a failed setup's included
//! (aaronsb/kmscon#253).
//! A [`Restore`] does the same from a panic hook, where `panic = "abort"`
//! runs no `Drop`.

mod fake_vt;
pub mod font;
mod kernel;
mod shown;
mod tty;

#[cfg(test)]
mod fake;
#[cfg(test)]
mod tests;

pub use fake_vt::FakeVt;
pub use kernel::{KernelVt, Restore};
pub use tty::Tty;

use std::io;
use std::os::fd::{BorrowedFd, OwnedFd};
use std::path::Path;

use rustix::fs::{Mode, OFlags};

use crate::input::KeyEvent;

/// What a VT backend tells its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtEvent {
    /// The VT is ours: wake the input devices and the displays.
    Acquired,
    /// The VT is asked for: put the input devices and the displays to
    /// sleep, then acknowledge or refuse.
    Release,
    /// The VT hung up.
    Hangup,
    /// vtium took the VT again while it is ours: set the displays up again
    /// and draw whole frames.
    Retaken,
}

/// Whether a switch asked for is done or waits for the kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    /// Nothing to wait for: the VT is already where it was asked to be.
    Done,
    /// Asked; a [`VtEvent`] tells when it happens.
    Pending,
}

/// A seat's VT backend: the kernel's VT, a fake VT, or libseat
/// (vtium/vtium#34).
pub trait Vt {
    /// What happened since the last call.
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent>;

    /// Lets the VT go after a [`VtEvent::Release`], once the displays are
    /// asleep.
    fn acknowledge_release(&mut self) -> io::Result<()>;

    /// Keeps the VT after a [`VtEvent::Release`].
    fn refuse_release(&mut self) -> io::Result<()>;

    /// Asks for this VT to be shown.
    fn activate(&mut self) -> io::Result<Switch>;

    /// Asks to switch back to the VT that was shown when this one was
    /// opened, as at exit. While it is pending, the loop keeps serving the
    /// release signal.
    fn leave(&mut self) -> io::Result<Switch>;

    /// Whether this VT is shown, without asking for it: `Pending` until a
    /// [`VtEvent::Acquired`] or [`VtEvent::Hangup`] ends the wait.
    fn wait(&mut self) -> io::Result<Switch>;

    /// Whether the VT is ours.
    fn is_active(&self) -> bool;

    /// Takes the VT's own keys, which switch VTs. Returns whether the key
    /// was taken, so no one later in the key order sees it.
    fn key(&mut self, key: &KeyEvent) -> bool;

    /// Whether `key` is this VT's own switch key, pressed while the VT is
    /// ours and shown: a key [`Vt::key`] takes and does nothing with,
    /// which a head's rescue reads (ADR-1401 D9). Asking switches nothing.
    /// A backend with no such key has none.
    fn is_own_key(&self, key: &KeyEvent) -> bool {
        let _ = key;
        false
    }

    /// Puts the VT's modes back after a program handed the display back
    /// (vtium/vtium#36).
    fn restore(&mut self) -> io::Result<()>;

    /// Rings the console's bell.
    fn bell(&mut self);

    /// The VT's number, or `None` without a kernel VT.
    fn number(&self) -> Option<u16>;

    /// The seat's name.
    fn seat(&self) -> &str {
        "seat0"
    }

    /// Opens a device of the seat: read-write and non-blocking, directly,
    /// or through the seat manager.
    fn open_device(&mut self, path: &Path) -> io::Result<OwnedFd> {
        let fd = rustix::fs::open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )?;
        Ok(fd)
    }

    /// Closes a device [`Vt::open_device`] opened.
    fn close_device(&mut self, fd: OwnedFd) {
        drop(fd);
    }
}

/// A keyboard mode, `KDGKBMODE`'s and `KDSKBMODE`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KbMode {
    Raw,
    Xlate,
    MediumRaw,
    Unicode,
    Off,
    /// A value the headers do not name, kept to restore it as it was.
    Other(u32),
}

impl KbMode {
    pub(crate) fn from_raw(raw: u32) -> KbMode {
        use crate::ioctl::{K_MEDIUMRAW, K_OFF, K_RAW, K_UNICODE, K_XLATE};
        match raw {
            K_RAW => KbMode::Raw,
            K_XLATE => KbMode::Xlate,
            K_MEDIUMRAW => KbMode::MediumRaw,
            K_UNICODE => KbMode::Unicode,
            K_OFF => KbMode::Off,
            other => KbMode::Other(other),
        }
    }

    pub(crate) fn raw(self) -> u32 {
        use crate::ioctl::{K_MEDIUMRAW, K_OFF, K_RAW, K_UNICODE, K_XLATE};
        match self {
            KbMode::Raw => K_RAW,
            KbMode::Xlate => K_XLATE,
            KbMode::MediumRaw => K_MEDIUMRAW,
            KbMode::Unicode => K_UNICODE,
            KbMode::Off => K_OFF,
            KbMode::Other(raw) => raw,
        }
    }
}

/// The VT's display mode, `KDSETMODE`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayMode {
    Text,
    Graphics,
}

/// Who switches the VT, `VT_SETMODE`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Switching {
    /// The kernel switches on its own.
    Auto,
    /// The kernel signals the release and the acquire and waits for the
    /// process to acknowledge them: SIGUSR2 to release, SIGUSR1 to acquire,
    /// as the C reference set them (vt_real.c:239-240).
    Process,
}

/// The answer to a release, `VT_RELDISP`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reldisp {
    /// Keep the VT.
    Refuse,
    /// Let it go.
    Allow,
}

/// The kernel's calls on one VT, so the tests can record them.
pub(crate) trait Console: Sized {
    /// The tty's line settings.
    type Termios: Clone + std::fmt::Debug;
    /// The number of the VT on screen, `VT_GETSTATE`.
    fn active(&self) -> io::Result<u16>;
    fn set_display_mode(&self, mode: DisplayMode) -> io::Result<()>;
    /// Whether `VT_GETMODE` shows vtium's own `VT_PROCESS`: SIGUSR2 to
    /// release and SIGUSR1 to acquire.
    fn switching_is_ours(&self) -> io::Result<bool>;
    fn set_switching(&self, mode: Switching) -> io::Result<()>;
    fn keyboard_mode(&self) -> io::Result<KbMode>;
    fn set_keyboard_mode(&self, mode: KbMode) -> io::Result<()>;
    fn reldisp(&self, answer: Reldisp) -> io::Result<()>;
    /// Asks for VT `num` to be shown, `VT_ACTIVATE`.
    fn activate(&self, num: u16) -> io::Result<()>;
    /// Drops what the tty read: the keys come from evdev.
    fn flush_input(&self) -> io::Result<()>;
    fn termios(&self) -> io::Result<Self::Termios>;
    fn set_termios(&self, termios: &Self::Termios) -> io::Result<()>;
    /// `termios` in raw mode, echoing nothing.
    fn silent(termios: &Self::Termios) -> Self::Termios;
    fn bell(&self) -> io::Result<()>;
    /// A second handle on the same VT.
    fn try_clone(&self) -> io::Result<Self>;
    /// A descriptor for the loop to watch; `None` for one without.
    fn watch(&self) -> Option<io::Result<OwnedFd>>;
    /// tty0's `active`, which names the VT shown and notifies on every
    /// switch; `None` for a console without it.
    fn watch_shown(&self) -> Option<io::Result<OwnedFd>>;
    /// The VT `watched`, the descriptor [`Console::watch_shown`] gave,
    /// names.
    fn shown(&self, watched: BorrowedFd<'_>) -> io::Result<u16>;
}
