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
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::rc::Rc;

use calloop::generic::Generic;
use calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::io::Errno;
use rustix::process::{
    Pid, PidfdFlags, WaitId, WaitIdOptions, ioctl_tiocsctty, pidfd_open, pidfd_send_signal, setsid,
    waitid,
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
    OptionalActions, SpecialCodeIndex, Winsize, tcgetattr, tcgetsid, tcsetattr, tcsetwinsize,
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
    /// reference's child did not (see the crate README's Departures). It is
    /// called from vtium's one thread only: a descriptor another thread
    /// opened between the marking and the fork would reach the child.
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
        let mut child = command.spawn()?;
        // The parent's copies of the slave go with the command, so the
        // master sees a hangup once the child's session closes it.
        drop(command);
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
            exit: None,
            pidfd: Rc::new(pidfd),
            size,
            link: None,
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

    /// The child's process id, or `None` once it was reaped.
    pub fn child_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
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
    use std::time::Duration;

    use calloop::EventLoop;
    use rustix::process::{WaitOptions, kill_process, waitpid};

    thread_local! {
        /// The step at which the next [`fail_at`] on this thread fails.
        static FAIL: Cell<Option<Step>> = const { Cell::new(None) };
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
