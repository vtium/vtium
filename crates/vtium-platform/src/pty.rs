//! The pty and its child: opening, the child's session, the window size,
//! the write queue to the child, and the child's exit.
//!
//! [`Pty::spawn`] opens a pty and runs a [`Command`] on it as the leader of
//! a new session with the pty as its controlling terminal.
//! [`Pty::insert`] puts it in the event loop: the master, edge-triggered,
//! feeds the child's output to a [`Consumer`], and a pidfd reports the
//! child's exit. Dropping the pty closes it, with what was queued for its
//! child, so a pty opened after it starts with an empty queue.
//!
//! A vtium that starts again after a crash or an upgrade takes back the
//! masters of the sessions that outlived it (ADR-1401 D13): [`Pty::adopt`]
//! makes a pty of a master whose session still runs, and
//! [`Pty::release`] lets a master go at vtium's exit without hanging its
//! session up.

use std::io;
use std::marker::PhantomData;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::rc::Rc;

use calloop::generic::Generic;
use calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::io::Errno;
use rustix::process::{
    Pid, PidfdFlags, WaitId, WaitIdOptions, ioctl_tiocsctty, kill_process_group, pidfd_open,
    pidfd_send_signal, setsid, waitid,
};
// rustix publishes its runtime module under a name that changes between
// releases, as it keeps that API unstable; Cargo.toml pins rustix to the
// release whose name this is.
use rustix::pty::{OpenptFlags, grantpt, ioctl_tiocgptpeer, openpt, unlockpt};
use rustix::runtime_448b8ad740e2a26f::{
    How, KERNEL_SIG_DFL, KERNEL_SIGRTMAX, KernelSigSet, KernelSigaction, Signal, kernel_sigaction,
    kernel_sigprocmask,
};
use rustix::termios::{
    OptionalActions, SpecialCodeIndex, Winsize, tcgetattr, tcgetpgrp, tcgetsid, tcgetwinsize,
    tcsetattr, tcsetwinsize,
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
    /// Takes the pty out of the loop and leaves a source there that reaps
    /// `child` when it exits.
    fn reap(self: Box<Self>, child: Child, pidfd: Rc<OwnedFd>);
}

/// A pty and its child.
pub struct Pty {
    channel: Channel<Master>,
    master: Rc<OwnedFd>,
    /// `None` once reaped, and for an adopted pty.
    child: Option<Child>,
    /// For a pty [`Pty::adopt`] made, the process id of the process whose
    /// exit ends the session, until it exits.
    adopted: Option<u32>,
    /// How the child exited, until the consumer is told.
    exit: Option<ExitStatus>,
    /// The child's pidfd, or the adopted process's.
    pidfd: Rc<OwnedFd>,
    size: Size,
    link: Option<Box<dyn Link>>,
    /// Whether dropping the pty hangs its session up; [`Pty::release`]
    /// turns it off.
    hang_up: bool,
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
    /// session would inherit all three. vtium resets every signal from 1
    /// to the kernel's last, SIGKILL and SIGSTOP aside.
    ///
    /// The child holds no other descriptor of vtium's: every descriptor
    /// vtium inherited is marked close-on-exec before the fork (see the
    /// crate README's pty rules). It is
    /// called from vtium's one thread only: a descriptor another thread
    /// opened between the marking and the fork would reach the child.
    pub fn spawn(command: Command, size: Size, erase: Erase) -> io::Result<Pty> {
        Pty::start(command, size, erase, None::<fn(u32)>)
    }

    /// [`Pty::spawn`], with the child held between its fork and its exec
    /// until `before_exec` has run with the child's process id. vtium uses
    /// it to move the child into a systemd scope of its own before the
    /// child runs `login` (ADR-1400 D3, its note on each login's own
    /// session).
    ///
    /// The child tells its process id through a pipe and then waits on a
    /// second pipe. `Command::spawn` returns only once the child has run
    /// its program, so another thread forks the child, while this thread
    /// reads the id, runs `before_exec` and then lets the child go on. When
    /// `before_exec` unwinds, the child is not let go: it ends without
    /// running its program, and the spawn fails.
    pub fn spawn_gated(
        command: Command,
        size: Size,
        erase: Erase,
        before_exec: impl FnOnce(u32),
    ) -> io::Result<Pty> {
        Pty::start(command, size, erase, Some(before_exec))
    }

    fn start(
        mut command: Command,
        size: Size,
        erase: Erase,
        before_exec: Option<impl FnOnce(u32)>,
    ) -> io::Result<Pty> {
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
        // makes only async-signal-safe calls: default_signals's, setsid
        // and the TIOCSCTTY ioctl, raw syscalls through rustix which
        // allocate nothing and take no lock. The child has one thread,
        // as default_signals requires.
        unsafe {
            command.pre_exec(move || {
                default_signals()?;
                setsid()?;
                ioctl_tiocsctty(&ctty)?;
                Ok(())
            });
        }
        // Here and not in the hook: listing the descriptors where
        // close_range cannot mark them allocates.
        crate::fds::cloexec_before_spawn()?;
        // The parent's copies of the slave go with the command, so the
        // master sees a hangup once the child's session closes it.
        let mut child = match before_exec {
            None => {
                let spawned = command.spawn();
                drop(command);
                spawned?
            }
            Some(before_exec) => spawn_held(command, before_exec)?,
        };
        let opened = fail_at(Step::Pidfd)
            .and_then(|()| Ok(pidfd_open(Pid::from_child(&child), PidfdFlags::empty())?));
        let pidfd = match opened {
            Ok(pidfd) => pidfd,
            Err(e) => {
                // Nothing else will wait for this child, so spawn kills it
                // and waits for it here; a child nobody waits for stays a
                // zombie until vtium exits. SIGKILL ends the child whatever
                // it does with other signals, so the wait is short.
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };

        let master = Rc::new(master);
        Ok(Pty {
            channel: Channel::new(Master(Rc::clone(&master))),
            master,
            child: Some(child),
            adopted: None,
            exit: None,
            pidfd: Rc::new(pidfd),
            size,
            link: None,
            hang_up: true,
        })
    }

    /// Makes a pty of `master`, the master of a session that outlived the
    /// vtium that started it (ADR-1401 D13), at the size the kernel holds
    /// for it.
    ///
    /// vtium cannot wait for a process it did not start, so the pty
    /// watches, through a pidfd, the process that leads the pty's session
    /// now: the program vtium started, or the process that program handed
    /// the pty to, as `login` hands it to the user's shell. The leader is
    /// found as [`Pty`]'s drop finds it, by the session id the master
    /// names, used only when the master still names that session once the
    /// pidfd is open. With no session on the pty, because its program
    /// ended while no vtium ran, it fails with [`io::ErrorKind::NotFound`]
    /// and drops the master. The session's exit, when it comes, is told
    /// with status 0, since nothing can read the real one.
    pub fn adopt(master: OwnedFd) -> io::Result<Pty> {
        let ended = || io::Error::new(io::ErrorKind::NotFound, "the session on the pty has ended");
        let sid = tcgetsid(&master).map_err(|_| ended())?;
        let pidfd = pidfd_open(sid, PidfdFlags::empty()).map_err(|_| ended())?;
        if tcgetsid(&master).ok() != Some(sid) {
            return Err(ended());
        }
        let leader = u32::try_from(sid.as_raw_nonzero().get()).map_err(|_| ended())?;
        let held = tcgetwinsize(&master)?;
        let cell = |px: u16, cells: u16| u32::from(px).checked_div(u32::from(cells)).unwrap_or(0);
        let size = Size {
            cols: held.ws_col,
            rows: held.ws_row,
            cell_width: cell(held.ws_xpixel, held.ws_col),
            cell_height: cell(held.ws_ypixel, held.ws_row),
        };
        fcntl_setfl(&master, fcntl_getfl(&master)? | OFlags::NONBLOCK)?;
        let master = Rc::new(master);
        Ok(Pty {
            channel: Channel::new(Master(Rc::clone(&master))),
            master,
            child: None,
            adopted: Some(leader),
            exit: None,
            pidfd: Rc::new(pidfd),
            size,
            link: None,
            hang_up: true,
        })
    }

    /// Puts the pty in the loop behind `handle`. `access` finds the pty
    /// and its consumer in the loop's data, or `None` once the host has
    /// closed it.
    ///
    /// When it fails, it leaves nothing of the pty in the loop, and the pty
    /// is as it was before the call: the host may insert it again, and
    /// dropping it kills and reaps its child.
    pub fn insert<D, C, A>(&mut self, handle: &LoopHandle<'static, D>, access: A) -> io::Result<()>
    where
        D: 'static,
        C: Consumer + 'static,
        A: Fn(&mut D) -> Option<(&mut Pty, &mut C)> + Clone + 'static,
    {
        let output = fail_at(Step::OutputSource).and_then(|()| {
            handle
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
                .map_err(|e| io::Error::from(e.error))
        })?;
        let exit = fail_at(Step::ExitSource).and_then(|()| {
            handle
                .insert_source(
                    Generic::new(Rc::clone(&self.pidfd), Interest::READ, Mode::Level),
                    {
                        let access = access.clone();
                        move |_, _, data: &mut D| {
                            let Some((pty, consumer)) = access(data) else {
                                return Ok(PostAction::Remove);
                            };
                            let status = match pty.child.as_mut() {
                                Some(child) => match child.try_wait()? {
                                    Some(status) => {
                                        pty.child = None;
                                        status
                                    }
                                    None => return Ok(PostAction::Continue),
                                },
                                // An adopted process's pidfd turns readable
                                // when it exits; vtium can neither reap it
                                // nor read its status.
                                None if pty.adopted.take().is_some() => ExitStatus::from_raw(0),
                                None => return Ok(PostAction::Remove),
                            };
                            pty.exit = Some(status);
                            let next = pty.channel.orphan();
                            pty.settle(next, consumer);
                            Ok(PostAction::Remove)
                        }
                    },
                )
                .map_err(|e| io::Error::from(e.error))
        });
        let exit = match exit {
            Ok(exit) => exit,
            Err(e) => {
                handle.remove(output);
                return Err(e);
            }
        };
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
        if told && let Some(status) = self.exit.take() {
            consumer.exited(status);
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

    /// Sets the window size as [`Pty::resize`] does, and when the kernel
    /// already holds that size, so that setting it would send no SIGWINCH,
    /// sends SIGWINCH to the pty's foreground process group itself, so
    /// that its program redraws all the same. A restarted vtium calls it
    /// on each pty it adopted, whose screen it starts empty (ADR-1401
    /// D13).
    pub fn redraw(&mut self, size: Size) -> io::Result<()> {
        let held = tcgetwinsize(&*self.master)?;
        let wanted = size.winsize();
        self.size = size;
        let same = (held.ws_row, held.ws_col, held.ws_xpixel, held.ws_ypixel)
            == (
                wanted.ws_row,
                wanted.ws_col,
                wanted.ws_xpixel,
                wanted.ws_ypixel,
            );
        if !same {
            tcsetwinsize(&*self.master, wanted)?;
            return Ok(());
        }
        kill_process_group(tcgetpgrp(&*self.master)?, Signal::WINCH)?;
        Ok(())
    }

    /// The child's process id, or `None` once it was reaped; for an
    /// adopted pty, the process id of the process it watches, until that
    /// process exits.
    pub fn child_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id).or(self.adopted)
    }

    /// Whether [`Pty::adopt`] made the pty, so its session's exit status
    /// is not known.
    pub fn adopted(&self) -> bool {
        self.adopted.is_some()
    }

    /// The master, which the host lends to whatever keeps a copy of it
    /// through a restart (ADR-1401 D13).
    pub fn master(&self) -> BorrowedFd<'_> {
        self.master.as_fd()
    }

    /// Lets the pty go without hanging its session up: it leaves its loop
    /// and closes this copy of the master, sends no SIGHUP, and neither
    /// kills nor waits for the child. vtium calls it at its exit for each
    /// pty whose master something outside vtium still holds, so that the
    /// session runs on for the next vtium (ADR-1401 D13). A child that
    /// vtium started and that still runs is left to whatever adopts it
    /// once vtium has exited, PID 1 or a subreaper.
    pub fn release(mut self) {
        self.hang_up = false;
        self.child = None;
    }

    /// The uid that owns the pty's slave now, as `login` sets it when a
    /// user logs in. The slave is reopened from the master, as
    /// [`Pty::spawn`] opens it, so no path name in `/dev/pts` is trusted;
    /// it is opened without becoming a controlling terminal, read with
    /// `fstat`, and closed. The image helper's identity reads it before
    /// each request (ADR-1101 D2).
    pub fn slave_owner(&self) -> io::Result<u32> {
        let slave = ioctl_tiocgptpeer(
            &*self.master,
            OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
        )?;
        Ok(rustix::fs::fstat(&slave)?.st_uid)
    }
}

/// Sets every signal's disposition to its default and empties the mask,
/// as a child starts in [`Pty::spawn`] and the suspend shell
/// ([`crate::power`]). The dispositions go first, so a signal that arrives
/// once the mask is empty takes its default action, as it would after
/// exec.
///
/// # Safety
///
/// Called only in a forked child before exec, with one thread, where
/// nothing needs a handler or a blocked signal. It makes only raw
/// rt_sigaction and rt_sigprocmask syscalls, which allocate nothing and
/// take no lock.
#[expect(
    unsafe_code,
    reason = "setting SIG_DFL on every signal is sound only between fork and exec"
)]
pub(crate) unsafe fn default_signals() -> io::Result<()> {
    let default = KernelSigaction {
        sa_handler_kernel: KERNEL_SIG_DFL,
        ..KernelSigaction::default()
    };
    for raw in 1..=KERNEL_SIGRTMAX {
        if raw == Signal::KILL.as_raw() || raw == Signal::STOP.as_raw() {
            continue;
        }
        // SAFETY: raw runs from 1 to the kernel's last signal, SIGKILL
        // and SIGSTOP, whose dispositions cannot change, skipped; SIG_DFL
        // on those libc reserves for its threads is sound in a child
        // with one thread, as the caller promises.
        unsafe {
            kernel_sigaction(Signal::from_raw_unchecked(raw), Some(default.clone()))?;
        }
    }
    // SAFETY: the empty mask, in a child that needs no blocked signal
    // before exec, as the caller promises.
    unsafe {
        kernel_sigprocmask(How::SETMASK, Some(&KernelSigSet::empty()))?;
    }
    Ok(())
}

impl Drop for Pty {
    /// Sends SIGHUP to the leader of the pty's session, then removes the
    /// pty from its loop and closes the master, which hangs up the session.
    /// A child still running is reaped by the loop when it exits. A pty
    /// that is in no loop, because it was never inserted or its insert
    /// failed, kills its child with SIGKILL and waits for it, as does a pty
    /// whose loop cannot take the source that would reap the child: nothing
    /// else would wait for the child, and a child nobody waits for stays a
    /// zombie until vtium exits.
    ///
    /// The signal goes first because the kernel's hangup ends the session's
    /// input before it signals the session's leader: closing the master
    /// wakes a reader of the pty with EIO, and only then sends SIGHUP to
    /// the leader. A leader that reads the pty can see the EIO and exit in
    /// between, so the kernel's SIGHUP finds no process and the session is
    /// never told it was hung up. Sent before the master closes, SIGHUP is
    /// pending in the leader before its input ends.
    fn drop(&mut self) {
        if !self.hang_up {
            return;
        }
        self.signal_leader();
        let link = self.link.take();
        let Some(child) = self.child.take() else {
            return;
        };
        match link {
            Some(link) => link.reap(child, Rc::clone(&self.pidfd)),
            None => kill_and_reap(self.pidfd.as_fd()),
        }
    }
}

impl Pty {
    /// Sends SIGHUP to the process the kernel's hangup signals: the leader
    /// of the session whose controlling terminal the pty is. That is the
    /// child vtium started until the child hands the pty on, as login does:
    /// its first process gives the pty up and forks, and the fork starts a
    /// new session on the pty, so the shell leads the session and the
    /// child is outside it. The leader is found by its session id and
    /// reached through a pidfd, which is used only when the pty still names
    /// that session after the pidfd is open: the kernel clears the pty's
    /// session when its leader exits, so a process that took the leader's
    /// id since is never signalled. With no session on the pty, a child
    /// still running gets the signal.
    fn signal_leader(&self) {
        let leader = tcgetsid(&*self.master).ok().and_then(|sid| {
            let pidfd = pidfd_open(sid, PidfdFlags::empty()).ok()?;
            (tcgetsid(&*self.master).ok() == Some(sid)).then_some(pidfd)
        });
        match leader {
            Some(pidfd) => {
                let _ = pidfd_send_signal(&pidfd, Signal::HUP);
            }
            None if self.child.is_some() => {
                let _ = pidfd_send_signal(&*self.pidfd, Signal::HUP);
            }
            None => {}
        }
    }
}

/// Kills the child behind `pidfd` with SIGKILL and waits for it to exit,
/// for a child no loop will reap. SIGKILL ends the child whatever it does
/// with other signals, so the wait is short.
fn kill_and_reap(pidfd: BorrowedFd<'_>) {
    let _ = pidfd_send_signal(pidfd, Signal::KILL);
    while let Err(Errno::INTR) = waitid(WaitId::PidFd(pidfd), WaitIdOptions::EXITED) {}
}

/// Spawns `command` held between its fork and its exec until
/// `before_exec` has run with its process id (see [`Pty::spawn_gated`]).
/// The command is spawned, and dropped, on a thread of its own, since
/// `Command::spawn` returns only once the child has run its program.
fn spawn_held(mut command: Command, before_exec: impl FnOnce(u32)) -> io::Result<Child> {
    let (told, tell) = io::pipe()?;
    let (wait, go) = io::pipe()?;
    let (tell, wait) = (OwnedFd::from(tell), OwnedFd::from(wait));
    // The child's copy of the end that lets it go, which it closes, or the
    // end of `wait` would never come when this end closes.
    let go_raw = go.as_raw_fd();
    #[expect(
        unsafe_code,
        reason = "the child can only wait for its scope between fork and exec"
    )]
    // SAFETY: the hook runs in the forked child before exec and makes only
    // raw syscalls: close, getpid, write and read, which allocate nothing
    // and take no lock. `go_raw` is the child's own copy of `go`, which
    // nothing else in the child uses.
    unsafe {
        command.pre_exec(move || {
            libc::close(go_raw);
            hold(&tell, &wait)
        });
    }
    std::thread::scope(|threads| {
        // The command, with the parent's copies of the pipes' child ends,
        // goes when the thread ends, so a child that fails before it tells
        // its id leaves `told` at its end.
        // A thread that cannot start fails the spawn with its error, as a
        // fork that cannot start does, before any child exists.
        let forking = forker().spawn_scoped(threads, move || command.spawn())?;
        if let Some(pid) = read_pid(&told) {
            before_exec(pid);
            let mut sent = rustix::io::write(&go, b"g");
            while sent == Err(Errno::INTR) {
                sent = rustix::io::write(&go, b"g");
            }
        }
        // Closing `go` without a byte, as an unwinding `before_exec` does,
        // ends the child before its exec.
        drop(go);
        forking
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("the thread that starts the session ended")))
    })
}

/// The thread that forks a held child: std's defaults, or in a test, a
/// stack the test sets.
fn forker() -> std::thread::Builder {
    let builder = std::thread::Builder::new().name("vtium-spawn".to_owned());
    #[cfg(test)]
    let builder = match tests::STACK.get() {
        Some(bytes) => builder.stack_size(bytes),
        None => builder,
    };
    builder
}

/// In the child, between fork and exec: tells its process id on `tell`,
/// then waits on `wait` for the byte that lets it go on. An end of `wait`
/// with no byte ends the child before its program runs.
fn hold(tell: &OwnedFd, wait: &OwnedFd) -> io::Result<()> {
    let pid = rustix::process::getpid().as_raw_nonzero().get();
    let bytes = pid.to_ne_bytes();
    // a pipe takes a write this small whole (pipe(7), PIPE_BUF)
    loop {
        match rustix::io::write(tell, &bytes) {
            Err(Errno::INTR) => {}
            Ok(n) if n == bytes.len() => break,
            Ok(_) => return Err(Errno::IO.into()),
            Err(e) => return Err(e.into()),
        }
    }
    let mut byte = [0u8];
    loop {
        match rustix::io::read(wait, &mut byte) {
            Err(Errno::INTR) => {}
            Ok(1) => return Ok(()),
            Ok(_) => return Err(Errno::CANCELED.into()),
            Err(e) => return Err(e.into()),
        }
    }
}

/// The child's process id from `told`, or `None` when the child ended
/// before it told it.
fn read_pid(told: &io::PipeReader) -> Option<u32> {
    let mut bytes = [0u8; 4];
    let mut got = 0;
    while got < bytes.len() {
        match rustix::io::read(told, bytes.get_mut(got..)?) {
            Err(Errno::INTR) => {}
            Ok(0) | Err(_) => return None,
            Ok(n) => got = got.saturating_add(n),
        }
    }
    u32::try_from(i32::from_ne_bytes(bytes)).ok()
}

/// The steps after the fork that can fail, which a unit test makes fail to
/// check that the child is still reaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// [`Pty::spawn`]'s pidfd for the child.
    Pidfd,
    /// [`Pty::insert`]'s source for the master.
    OutputSource,
    /// [`Pty::insert`]'s source for the child's exit.
    ExitSource,
    /// The source a dropped pty leaves in its loop to reap its child.
    Reaper,
}

/// Fails at `step` when a unit test asks it to, and otherwise succeeds.
#[cfg(not(test))]
#[expect(clippy::unnecessary_wraps, reason = "the unit tests' version fails")]
#[inline]
fn fail_at(_: Step) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn fail_at(step: Step) -> io::Result<()> {
    tests::fail_at(step)
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

    fn reap(self: Box<Self>, mut child: Child, pidfd: Rc<OwnedFd>) {
        let handle = self.handle.clone();
        // The exit source watches the same pidfd, and the loop's poller
        // refuses a descriptor it already watches, so the pty's sources
        // leave the loop before the reaper joins it.
        drop(self);
        let inserted = fail_at(Step::Reaper).is_ok()
            && handle
                .insert_source(
                    Generic::new(Rc::clone(&pidfd), Interest::READ, Mode::Level),
                    move |_, _, _| match child.try_wait() {
                        Ok(None) => Ok(PostAction::Continue),
                        _ => Ok(PostAction::Remove),
                    },
                )
                .is_ok();
        if !inserted {
            kill_and_reap(pidfd.as_fd());
        }
    }
}

impl<D: 'static, C, A> Drop for LoopLink<D, C, A> {
    fn drop(&mut self) {
        self.handle.remove(self.output);
        self.handle.remove(self.exit);
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::BTreeSet;
    use std::path::Path;
    use std::time::Duration;

    use calloop::EventLoop;
    use rustix::process::{WaitOptions, kill_process, waitpid};

    thread_local! {
        /// The step at which the next [`fail_at`] on this thread fails.
        static FAIL: Cell<Option<Step>> = const { Cell::new(None) };
        /// The stack size of the thread that forks a held child, when the
        /// test sets one.
        pub(super) static STACK: Cell<Option<usize>> = const { Cell::new(None) };
    }

    /// Fails once, at the step the test set, with EMFILE, as a process
    /// out of descriptors would.
    pub(super) fn fail_at(step: Step) -> io::Result<()> {
        if FAIL.get() == Some(step) {
            FAIL.set(None);
            return Err(io::Error::from_raw_os_error(libc::EMFILE));
        }
        Ok(())
    }

    const SIZE: Size = Size {
        cols: 80,
        rows: 25,
        cell_width: 8,
        cell_height: 16,
    };

    /// A child that runs until something ends it.
    fn sleeper() -> Command {
        let mut command = Command::new("sleep");
        command.arg("1000");
        command
    }

    /// The children of this thread that nobody has waited for yet: those
    /// running and the zombies. Each test runs on a thread of its own, so
    /// another test's children are not listed.
    fn children() -> BTreeSet<i32> {
        std::fs::read_to_string("/proc/thread-self/children")
            .unwrap()
            .split_whitespace()
            .map(|pid| pid.parse().unwrap())
            .collect()
    }

    /// The children this thread started since `before` and has not waited
    /// for. The test kills and reaps each one before it asserts, so a
    /// failing test leaves no process behind.
    fn left_unreaped(before: &BTreeSet<i32>) -> Vec<i32> {
        let left: Vec<i32> = children().difference(before).copied().collect();
        for &raw in &left {
            let pid = Pid::from_raw(raw).unwrap();
            let _ = kill_process(pid, Signal::KILL);
            let _ = waitpid(Some(pid), WaitOptions::empty());
        }
        left
    }

    /// Makes the next `step` fail, runs `f`, and checks that `f` reached it.
    fn failing_at<T>(step: Step, f: impl FnOnce() -> T) -> T {
        FAIL.set(Some(step));
        let done = f();
        assert_eq!(FAIL.take(), None, "{step:?} was never reached");
        done
    }

    struct Ignore;

    impl Consumer for Ignore {
        fn output(&mut self, output: &[u8], _: &mut Replies<'_>) -> usize {
            output.len()
        }

        fn exited(&mut self, _: ExitStatus) {}
    }

    /// The loop's data holds no pty: the tests keep it.
    fn nothing(_: &mut ()) -> Option<(&mut Pty, &mut Ignore)> {
        None
    }

    // ADR-1101 D2: the image helper's identity reads the slave's owner,
    // which grantpt gives the account that opened the pty, here the
    // account the test runs as.
    #[test]
    fn the_slave_owner_is_the_account_the_pty_was_opened_as() {
        let before = children();
        let pty = Pty::spawn(sleeper(), SIZE, Erase::Delete).unwrap();
        let owner = pty.slave_owner();
        drop(pty);
        left_unreaped(&before);
        assert_eq!(owner.unwrap(), rustix::process::geteuid().as_raw());
    }

    /// Ends and reaps the child `pid`, which another thread forked.
    fn end(pid: u32) {
        let pid = Pid::from_raw(i32::try_from(pid).unwrap()).unwrap();
        let _ = kill_process(pid, Signal::KILL);
        let _ = waitpid(Some(pid), WaitOptions::empty());
    }

    // The child runs its program only once `before_exec` has returned:
    // while it runs, the child is still a copy of this test's program.
    #[test]
    fn a_held_child_runs_its_program_only_after_before_exec() {
        let ours = std::fs::read_link("/proc/self/exe").unwrap();
        let mut seen = None;
        let pty = Pty::spawn_gated(sleeper(), SIZE, Erase::Delete, |pid| {
            seen = Some((pid, std::fs::read_link(format!("/proc/{pid}/exe")).unwrap()));
        })
        .unwrap();
        let (pid, held) = seen.unwrap();
        let ran = std::fs::read_link(format!("/proc/{pid}/exe")).unwrap();
        let id = pty.child_id();
        pty.release();
        end(pid);
        assert_eq!(id, Some(pid), "before_exec is told the child's id");
        assert_eq!(held, ours, "the child waits before its exec");
        assert!(ran.ends_with("sleep"), "then it runs its program: {ran:?}");
    }

    // A thread that cannot start, here because its stack of 1 PiB is more
    // than the address space, fails the spawn with the thread's error before
    // any child exists, so the caller's "cannot start the session" follows.
    #[test]
    fn a_held_spawn_whose_thread_cannot_start_fails() {
        STACK.set(Some(1 << 50));
        let mut asked = false;
        let spawned = Pty::spawn_gated(sleeper(), SIZE, Erase::Delete, |_| asked = true);
        STACK.set(None);
        let e = spawned.err().map(|e| e.kind());
        assert!(e.is_some(), "the spawn fails");
        assert!(!asked, "no child was forked to ask about");
    }

    // A `before_exec` that unwinds never lets the child go: the child ends
    // before its exec, and the spawn fails with the panic.
    #[test]
    #[expect(clippy::panic, reason = "the scope call's failure, as an unwind")]
    fn a_held_child_whose_before_exec_unwinds_never_runs() {
        let mut pid = None;
        let spawned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Pty::spawn_gated(sleeper(), SIZE, Erase::Delete, |p| {
                pid = Some(p);
                panic!("the scope call failed");
            })
        }));
        assert!(spawned.is_err(), "the panic comes through");
        let pid = pid.unwrap();
        // the spawn waited for the child, which ended before its exec
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn a_spawn_that_fails_after_the_fork_leaves_no_child() {
        let before = children();
        let spawned = failing_at(Step::Pidfd, || {
            Pty::spawn(sleeper(), SIZE, Erase::Delete).err()
        });
        let left = left_unreaped(&before);
        assert!(spawned.is_some(), "the spawn fails");
        assert_eq!(left, [], "children left unreaped");
    }

    #[test]
    fn a_pty_whose_insert_failed_leaves_no_child_once_dropped() {
        for step in [Step::OutputSource, Step::ExitSource] {
            let lp: EventLoop<'static, ()> = EventLoop::try_new().unwrap();
            let before = children();
            let mut pty = Pty::spawn(sleeper(), SIZE, Erase::Delete).unwrap();
            let inserted = failing_at(step, || pty.insert(&lp.handle(), nothing));
            drop(pty);
            let left = left_unreaped(&before);
            assert!(inserted.is_err(), "the insert fails at {step:?}");
            assert_eq!(left, [], "children left unreaped after {step:?}");
        }
    }

    // The loop's poller refuses a descriptor it already watches, so a
    // second insert succeeds only if the failed one removed the master's
    // source.
    #[test]
    fn a_failed_insert_leaves_nothing_in_the_loop() {
        let lp: EventLoop<'static, ()> = EventLoop::try_new().unwrap();
        let before = children();
        let mut pty = Pty::spawn(sleeper(), SIZE, Erase::Delete).unwrap();
        let first = failing_at(Step::ExitSource, || pty.insert(&lp.handle(), nothing));
        let second = pty.insert(&lp.handle(), nothing);
        drop(pty);
        left_unreaped(&before);
        assert!(first.is_err(), "the first insert fails");
        assert!(second.is_ok(), "the second insert: {second:?}");
    }

    // The drop sends SIGHUP, which ends the sleeper, and the loop waits for
    // it: the drop itself neither kills nor waits for a child in a loop.
    #[test]
    fn a_pty_dropped_in_its_loop_leaves_its_child_to_the_loop() {
        let mut lp: EventLoop<'static, ()> = EventLoop::try_new().unwrap();
        let before = children();
        let mut pty = Pty::spawn(sleeper(), SIZE, Erase::Delete).unwrap();
        pty.insert(&lp.handle(), nothing).unwrap();
        drop(pty);
        let after_drop = children().difference(&before).count();
        // bounded so the test fails instead of hanging
        for _ in 0..100 {
            if children().is_subset(&before) {
                break;
            }
            lp.dispatch(Some(Duration::from_secs(1)), &mut ()).unwrap();
        }
        let left = left_unreaped(&before);
        assert_eq!(after_drop, 1, "the child is the loop's to reap");
        assert_eq!(left, [], "children the loop left unreaped");
    }

    #[test]
    fn a_pty_whose_reaper_cannot_join_the_loop_leaves_no_child() {
        let lp: EventLoop<'static, ()> = EventLoop::try_new().unwrap();
        let before = children();
        let mut pty = Pty::spawn(sleeper(), SIZE, Erase::Delete).unwrap();
        pty.insert(&lp.handle(), nothing).unwrap();
        failing_at(Step::Reaper, || drop(pty));
        let left = left_unreaped(&before);
        assert_eq!(left, [], "children left unreaped");
    }
}
