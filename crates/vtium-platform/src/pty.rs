//! The pty and its child: opening, the child's session, the window size,
//! the write queue to the child, and the child's exit.
//!
//! [`Pty::spawn`] opens a pty and runs a [`Command`] on it as the leader of
//! a new session with the pty as its controlling terminal.
//! [`Pty::insert`] puts it in the event loop: the master, edge-triggered,
//! feeds the child's output to a [`Consumer`], and a pidfd reports the
//! child's exit. Dropping the pty closes it, with what was queued for its
//! child, so a pty opened after it starts with an empty queue.

use std::io;
use std::marker::PhantomData;
use std::os::fd::OwnedFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::rc::Rc;

use calloop::generic::Generic;
use calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::process::{Pid, PidfdFlags, ioctl_tiocsctty, pidfd_open, setsid};
// rustix publishes its runtime module under a name that changes between
// releases, as it keeps that API unstable; Cargo.toml pins rustix to the
// release whose name this is.
use rustix::pty::{OpenptFlags, grantpt, ioctl_tiocgptpeer, openpt, unlockpt};
use rustix::runtime_448b8ad740e2a26f::{
    How, KERNEL_SIG_DFL, KERNEL_SIGRTMAX, KernelSigSet, KernelSigaction, Signal, kernel_sigaction,
    kernel_sigprocmask,
};
use rustix::termios::{
    OptionalActions, SpecialCodeIndex, Winsize, tcgetattr, tcsetattr, tcsetwinsize,
};

use crate::channel::{Channel, Endpoint, Next};
use crate::queue::Kind;

pub use crate::channel::{Consumer, Replies};

/// The terminal's size, in cells and in pixels per cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}

impl Size {
    /// The kernel's window size. A pixel size past 16 bits is 65535.
    fn winsize(self) -> Winsize {
        let px = |cells: u16, cell: u32| {
            u16::try_from(u32::from(cells).saturating_mul(cell)).unwrap_or(u16::MAX)
        };
        Winsize {
            ws_row: self.rows,
            ws_col: self.cols,
            ws_xpixel: px(self.cols, self.cell_width),
            ws_ypixel: px(self.rows, self.cell_height),
        }
    }
}

/// What the erase key sends, set in the pty's termios as `VERASE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Erase {
    /// BS, 0x08.
    Backspace,
    /// DEL, 0x7f.
    Delete,
}

/// The pty master.
struct Master(Rc<OwnedFd>);

impl Endpoint for Master {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Ok(rustix::io::read(&*self.0, buf)?)
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(rustix::io::write(&*self.0, bytes)?)
    }
}

/// What ties a pty to the loop it was inserted in.
trait Link {
    /// Does what a [`Next`] asks.
    fn kick(&self, next: Next);
    /// Leaves a source in the loop that reaps `child` when it exits.
    fn reap(&self, child: Child, pidfd: Rc<OwnedFd>);
}

/// A pty and its child.
pub struct Pty {
    channel: Channel<Master>,
    master: Rc<OwnedFd>,
    /// `None` once reaped.
    child: Option<Child>,
    /// How the child exited, until the consumer is told.
    exit: Option<ExitStatus>,
    pidfd: Rc<OwnedFd>,
    size: Size,
    link: Option<Box<dyn Link>>,
}

impl Pty {
    /// Opens a pty of `size` and runs `command` on it.
    ///
    /// The child leads a new session with the pty's slave as its
    /// controlling terminal and its stdin, stdout and stderr, with every
    /// signal at its default disposition and none blocked. A vtium started
    /// in the background ignores SIGINT and SIGQUIT, the Rust runtime
    /// ignores SIGPIPE, and the loop blocks the signals it reads from its
    /// signalfd; an exec keeps ignored signals and the mask, so the
    /// session would inherit all three. The C reference emptied the mask
    /// and reset the signals below SIGSYS (pty.c:313-320); vtium resets
    /// every signal (see the crate README's Departures).
    ///
    /// The child holds no other descriptor of vtium's: every descriptor
    /// vtium inherited is marked close-on-exec before the fork, as the C
    /// reference's child did not (see the crate README's Departures).
    pub fn spawn(mut command: Command, size: Size, erase: Erase) -> io::Result<Pty> {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)?;
        grantpt(&master)?;
        unlockpt(&master)?;
        // Reached from the master, not by its name, which could be replaced.
        let slave = ioctl_tiocgptpeer(
            &master,
            OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
        )?;
        let mut termios = tcgetattr(&slave)?;
        termios.special_codes[SpecialCodeIndex::VERASE] = match erase {
            Erase::Backspace => 0x08,
            Erase::Delete => 0x7f,
        };
        tcsetattr(&slave, OptionalActions::Now, &termios)?;
        tcsetwinsize(&master, size.winsize())?;
        fcntl_setfl(&master, fcntl_getfl(&master)? | OFlags::NONBLOCK)?;

        let ctty = slave.try_clone()?;
        command
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        #[expect(
            unsafe_code,
            reason = "the signal dispositions and mask, a new session and its controlling terminal can only be set between fork and exec"
        )]
        // SAFETY: the hook runs in the forked child before exec, so it
        // makes only async-signal-safe calls: raw syscalls through rustix,
        // rt_sigaction, rt_sigprocmask, setsid and the TIOCSCTTY ioctl,
        // which allocate nothing and take no lock. The child has one
        // thread, and nothing in it needs a handler or a blocked signal
        // between here and exec, so rt_sigaction may set SIG_DFL on every
        // signal, those libc reserves for its threads included, and
        // rt_sigprocmask may set the empty mask. Each number passed to
        // rt_sigaction is a signal from 1 to the kernel's last; SIGKILL
        // and SIGSTOP, whose dispositions cannot change, are skipped.
        // The dispositions go first, so a signal that arrives once the
        // mask is empty takes its default action, as it would after exec.
        unsafe {
            command.pre_exec(move || {
                let default = KernelSigaction {
                    sa_handler_kernel: KERNEL_SIG_DFL,
                    ..KernelSigaction::default()
                };
                for raw in 1..=KERNEL_SIGRTMAX {
                    if raw == Signal::KILL.as_raw() || raw == Signal::STOP.as_raw() {
                        continue;
                    }
                    kernel_sigaction(Signal::from_raw_unchecked(raw), Some(default.clone()))?;
                }
                kernel_sigprocmask(How::SETMASK, Some(&KernelSigSet::empty()))?;
                setsid()?;
                ioctl_tiocsctty(&ctty)?;
                Ok(())
            });
        }
        // Here and not in the hook: listing the descriptors where
        // close_range cannot mark them allocates.
        crate::fds::cloexec_above_stdio()?;
        let child = command.spawn()?;
        // The parent's copies of the slave go with the command, so the
        // master sees a hangup once the child's session closes it.
        drop(command);
        let pidfd = pidfd_open(Pid::from_child(&child), PidfdFlags::empty())?;

        let master = Rc::new(master);
        Ok(Pty {
            channel: Channel::new(Master(Rc::clone(&master))),
            master,
            child: Some(child),
            exit: None,
            pidfd: Rc::new(pidfd),
            size,
            link: None,
        })
    }

    /// Puts the pty in the loop behind `handle`. `access` finds the pty
    /// and its consumer in the loop's data, or `None` once the host has
    /// closed it.
    pub fn insert<D, C, A>(&mut self, handle: &LoopHandle<'static, D>, access: A) -> io::Result<()>
    where
        D: 'static,
        C: Consumer + 'static,
        A: Fn(&mut D) -> Option<(&mut Pty, &mut C)> + Clone + 'static,
    {
        let output = handle
            .insert_source(
                Generic::new(Rc::clone(&self.master), Interest::BOTH, Mode::Edge),
                {
                    let access = access.clone();
                    move |readiness, _, data: &mut D| {
                        let Some((pty, consumer)) = access(data) else {
                            return Ok(PostAction::Remove);
                        };
                        let readable = readiness.readable || readiness.error;
                        let next = pty.channel.ready(readable, readiness.writable, consumer);
                        pty.settle(next, consumer);
                        Ok(PostAction::Continue)
                    }
                },
            )
            .map_err(|e| io::Error::from(e.error))?;
        let exit = handle
            .insert_source(
                Generic::new(Rc::clone(&self.pidfd), Interest::READ, Mode::Level),
                {
                    let access = access.clone();
                    move |_, _, data: &mut D| {
                        let Some((pty, consumer)) = access(data) else {
                            return Ok(PostAction::Remove);
                        };
                        let Some(child) = pty.child.as_mut() else {
                            return Ok(PostAction::Remove);
                        };
                        match child.try_wait()? {
                            Some(status) => {
                                pty.child = None;
                                pty.exit = Some(status);
                                let next = pty.channel.orphan();
                                pty.settle(next, consumer);
                                Ok(PostAction::Remove)
                            }
                            None => Ok(PostAction::Continue),
                        }
                    }
                },
            )
            .map_err(|e| io::Error::from(e.error))?;
        self.link = Some(Box::new(LoopLink {
            handle: handle.clone(),
            output,
            exit,
            access,
            consumer: PhantomData::<fn(&mut C)>,
        }));
        Ok(())
    }

    fn kick(&self, next: Next) {
        if let Some(link) = &self.link {
            link.kick(next);
        }
    }

    /// Does what `next` asks, and once the child has exited and the output
    /// it left has been fed, tells the consumer.
    fn settle<C: Consumer + ?Sized>(&mut self, next: Next, consumer: &mut C) {
        let (next, told) = self.channel.settle(next, consumer);
        self.kick(next);
        if told {
            if let Some(status) = self.exit.take() {
                consumer.exited(status);
            }
        }
    }

    /// Writes what the user sent: keys, pastes and button reports. They
    /// are queued whole while the child does not read.
    pub fn write(&mut self, bytes: &[u8]) {
        self.channel.send(Kind::User, bytes);
    }

    /// Writes one pointer motion report. While the child leaves
    /// [`crate::MOTION_COALESCE_AT`] bytes unread, it replaces the
    /// motion reports queued after the last other write.
    pub fn write_motion(&mut self, report: &[u8]) {
        self.channel.send(Kind::Motion, report);
    }

    /// Writes a reply made outside [`Consumer::output`], such as the image
    /// host's. It counts against the reply bounds as the consumer's do.
    pub fn reply(&mut self, bytes: &[u8]) {
        self.channel.send(Kind::Reply, bytes);
    }

    /// Bytes queued for the child.
    pub fn queued(&self) -> usize {
        self.channel.queued()
    }

    /// Reply bytes queued for the child.
    pub fn queued_replies(&self) -> usize {
        self.channel.queued_replies()
    }

    /// Stops reading the child's output until [`Pty::resume`].
    pub fn pause(&mut self) {
        self.channel.pause();
    }

    pub fn resume(&mut self) {
        let next = self.channel.resume();
        self.kick(next);
    }

    /// Sets the window size, which signals SIGWINCH to the child's
    /// foreground process group. The same size again does nothing.
    pub fn resize(&mut self, size: Size) -> io::Result<()> {
        if size == self.size {
            return Ok(());
        }
        tcsetwinsize(&*self.master, size.winsize())?;
        self.size = size;
        Ok(())
    }

    /// The child's process id, or `None` once it was reaped.
    pub fn child_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }
}

impl Drop for Pty {
    /// Removes the pty from its loop and closes the master, which hangs up
    /// the child's session. A child still running is reaped by the loop
    /// when it exits.
    fn drop(&mut self) {
        let Some(link) = self.link.take() else {
            return;
        };
        if let Some(child) = self.child.take() {
            link.reap(child, Rc::clone(&self.pidfd));
        }
    }
}

struct LoopLink<D: 'static, C, A> {
    handle: LoopHandle<'static, D>,
    output: RegistrationToken,
    exit: RegistrationToken,
    access: A,
    consumer: PhantomData<fn(&mut C)>,
}

impl<D, C, A> Link for LoopLink<D, C, A>
where
    D: 'static,
    C: Consumer + 'static,
    A: Fn(&mut D) -> Option<(&mut Pty, &mut C)> + Clone + 'static,
{
    fn kick(&self, next: Next) {
        if next.rearm {
            // Within the output source's own callback calloop renews it
            // once the callback returns.
            let _ = self.handle.update(&self.output);
        }
        if next.feed {
            let access = self.access.clone();
            self.handle.insert_idle(move |data| {
                if let Some((pty, consumer)) = access(data) {
                    let next = pty.channel.feed_held(consumer);
                    pty.settle(next, consumer);
                }
            });
        }
    }

    fn reap(&self, mut child: Child, pidfd: Rc<OwnedFd>) {
        let _ = self.handle.insert_source(
            Generic::new(pidfd, Interest::READ, Mode::Level),
            move |_, _, _| match child.try_wait() {
                Ok(None) => Ok(PostAction::Continue),
                _ => Ok(PostAction::Remove),
            },
        );
    }
}

impl<D: 'static, C, A> Drop for LoopLink<D, C, A> {
    fn drop(&mut self) {
        self.handle.remove(self.output);
        self.handle.remove(self.exit);
    }
}
