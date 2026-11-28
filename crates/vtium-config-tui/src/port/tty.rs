//! The real port: a terminal in raw mode on the alternate screen, with the
//! cursor hidden and mouse reporting on, and with SIGTERM, SIGINT,
//! SIGQUIT, SIGHUP and SIGWINCH held and read in the loop. Raw mode comes
//! from rustix's `make_raw`, which clears IXON and ISIG with the other
//! flags `cfmakeraw(3)` clears, so Ctrl+S and Ctrl+Z arrive as keys rather
//! than stopping the output or the process.
//!
//! Dropping the port puts the screen, the cursor, mouse reporting and the
//! terminal's mode back. A panic puts them back from the panic hook, since
//! a release build aborts on a panic without dropping anything.
//!
//! References:
//! - ADR-1206 D2, the notes of 2026-11-27: raw mode clears XON/XOFF and
//!   the signal keys, so Ctrl+S and Ctrl+Z reach the TUI.
//! - ADR-1403 D11, "The mouse": reporting on while the TUI runs, off when
//!   it steps aside and when it leaves.
//! - ADR-1201 D7: the raw-mode guard and the signals held in the loop.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use rustix::termios::{OptionalActions, QueueSelector, Termios, tcflush, tcgetattr, tcsetattr};
use vtium_platform::event_loop::{Loop, Signal};

use super::wake::{Wake, Woken, now, wait_on, watch_input};
use super::{ENTER, LEAVE, Port};
use crate::app::Size;

/// The terminal the TUI runs on.
pub struct Tty {
    input: OwnedFd,
    output: OwnedFd,
    raw: Termios,
    saved: Termios,
    events: Loop<Woken>,
    deadline: Option<Instant>,
    /// What came while the loop asked whether input was pending, which
    /// the next wait hands over first.
    held: Vec<Wake>,
    /// The port's entry in [`RESTORE`].
    id: u64,
}

/// What the panic hook and a drop put back: the terminal's mode as it was.
struct Restore {
    id: u64,
    saved: Termios,
    tty: OwnedFd,
}

/// What each open port puts back. A test process can hold several ports
/// at once, each on a pty of its own.
static RESTORE: Mutex<Vec<Restore>> = Mutex::new(Vec::new());

/// The id of the next port.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// The panic hook, set once.
static HOOK: Once = Once::new();

/// Puts back the terminal of the port `id`, or of every port where `id`
/// is `None`, once: mouse reporting off, the cursor shown, the normal
/// screen, and the mode it had.
fn restore(id: Option<u64>) {
    let taken: Vec<Restore> = match RESTORE.lock() {
        Ok(mut all) => {
            let (taken, kept) = std::mem::take(&mut *all)
                .into_iter()
                .partition(|r| id.is_none_or(|id| r.id == id));
            *all = kept;
            taken
        }
        Err(_) => Vec::new(),
    };
    for r in taken {
        let _ = rustix::io::write(&r.tty, LEAVE);
        let _ = tcflush(&r.tty, QueueSelector::IFlush);
        let _ = tcsetattr(&r.tty, OptionalActions::Now, &r.saved);
    }
}

impl Tty {
    /// Takes the terminal of standard input and output.
    pub fn open() -> io::Result<Tty> {
        let input = rustix::io::fcntl_dupfd_cloexec(std::io::stdin().as_fd(), 0)?;
        let output = rustix::io::fcntl_dupfd_cloexec(std::io::stdout().as_fd(), 0)?;
        Tty::on(input, output)
    }

    /// Takes the terminal that `input` reads and `output` writes: raw
    /// mode, the signals held, the alternate screen, and mouse reporting
    /// on.
    pub fn on(input: OwnedFd, output: OwnedFd) -> io::Result<Tty> {
        let saved = tcgetattr(&input)?;
        let mut raw = saved.clone();
        raw.make_raw();
        let mut events = Loop::new()?;
        for (signal, n) in [
            (Signal::SIGTERM, 15),
            (Signal::SIGINT, 2),
            (Signal::SIGQUIT, 3),
            (Signal::SIGHUP, 1),
        ] {
            events.on_signal(signal, move |w: &mut Woken| w.wakes.push(Wake::Signal(n)))?;
        }
        events.on_signal(Signal::SIGWINCH, |w: &mut Woken| w.wakes.push(Wake::Resize))?;
        // held so that the TUI can take the terminal's foreground back
        // after a step aside without being stopped
        events.on_signal(Signal::SIGTTOU, |_: &mut Woken| {})?;
        watch_input(&events, rustix::io::fcntl_dupfd_cloexec(&input, 0)?)?;
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let back = Restore {
            id,
            saved: saved.clone(),
            tty: rustix::io::fcntl_dupfd_cloexec(&output, 0)?,
        };
        tcsetattr(&input, OptionalActions::Now, &raw)?;
        if let Ok(mut all) = RESTORE.lock() {
            all.push(back);
        }
        HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                restore(None);
                previous(info);
            }));
        });
        let mut tty = Tty {
            input,
            output,
            raw,
            saved,
            events,
            deadline: None,
            held: Vec::new(),
            id,
        };
        tty.write(ENTER)?;
        Ok(tty)
    }
}

impl Port for Tty {
    fn size(&mut self) -> Size {
        let Ok(w) = rustix::termios::tcgetwinsize(&self.output) else {
            return Size::grid(80, 24);
        };
        let (cols, rows) = if w.ws_col > 0 && w.ws_row > 0 {
            (w.ws_col, w.ws_row)
        } else {
            (80, 24)
        };
        let cell = (
            u32::from(w.ws_xpixel).checked_div(u32::from(w.ws_col)),
            u32::from(w.ws_ypixel).checked_div(u32::from(w.ws_row)),
        );
        Size {
            cols,
            rows,
            cell_px: match cell {
                (Some(x), Some(y)) if x > 0 && y > 0 => Some((x, y)),
                _ => None,
            },
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut rest = bytes;
        while !rest.is_empty() {
            match rustix::io::write(&self.output, rest) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => rest = rest.get(n..).unwrap_or_default(),
                Err(rustix::io::Errno::INTR) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn arm(&mut self, after: Duration) {
        self.deadline = now().checked_add(after);
    }

    fn wait(&mut self) -> io::Result<Vec<Wake>> {
        if !self.held.is_empty() {
            return Ok(std::mem::take(&mut self.held));
        }
        let mut woken = Woken::default();
        wait_on(&mut self.events, &mut self.deadline, &mut woken)?;
        Ok(woken.wakes)
    }

    fn pending(&mut self, within: Duration) -> io::Result<bool> {
        if !self.held.is_empty() {
            return Ok(true);
        }
        let mut woken = Woken::default();
        self.events.dispatch(Some(within), &mut woken)?;
        self.held = woken.wakes;
        Ok(!self.held.is_empty())
    }

    fn step_aside(&mut self, aside: &mut dyn FnMut() -> io::Result<()>) -> io::Result<()> {
        self.write(LEAVE)?;
        tcsetattr(&self.input, OptionalActions::Now, &self.saved)?;
        let done = aside();
        tcsetattr(&self.input, OptionalActions::Now, &self.raw)?;
        self.write(ENTER)?;
        done
    }
}

impl Drop for Tty {
    /// Puts the terminal back.
    fn drop(&mut self) {
        restore(Some(self.id));
    }
}
