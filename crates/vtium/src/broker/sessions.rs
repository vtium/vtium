//! The sessions that vtium's root process starts for its drawing process.
//!
//! The drawing process asks for a console's session by its head's name
//! and its console's id. The request also carries what only the drawing
//! process knows: the console's grid and character cell, which the root
//! process sets on the pty before the session starts, and two hints for
//! the session's environment, the kind of display and whether the head
//! shows 24-bit color. The root process checks each of them against the
//! values it allows, makes the pty, starts the session's program on the
//! pty's slave, keeps a copy of the master, and passes the master to the
//! drawing process.
//!
//! The drawing process never names the program. The root process reads
//! the login command from its own reading of vtium.conf: the head's own
//! `login` line where the head has one, and the top level's otherwise.
//! The drawing process runs as an account without root, so a drawing
//! process that crafted output has taken over can open consoles within
//! the caps of vtium.conf, but it can never choose what runs as root.
//!
//! The root process reaps every session it starts and reports each exit
//! to the drawing process. When the drawing process closes a console, the
//! root process hangs the session up, and it sends `SIGWINCH` to a
//! session's foreground process group when the drawing process asks,
//! since the session's processes run as another account.
//!
//! A closed session's program can ignore `SIGHUP` and run on, because the
//! drawing process holds its own copy of the master, so the kernel does
//! not hang the pty up. A drawing process that opened and closed a console
//! in a loop could then start root sessions without end. So the root
//! process bounds the closed sessions that still run by the same caps as
//! the open ones: while a head has as many of them as `head-consoles-max`,
//! or every head together as many as `consoles-max`, it opens no more
//! consoles. Each closed session counts until its process exits and the
//! root process reaps it.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the pty, `login`, the checks of
//!   the grid and the two hints, the command from the broker's own reading
//!   of vtium.conf, reaping, exits and `SIGWINCH`.
//! - ADR-1400 D4, "The limits that remain": a drawer that crafted output
//!   has taken over opens and closes consoles only within the caps.
//! - ADR-1401 D10: the caps on consoles, and the ownership check that the
//!   root process's reading of vtium.conf passes through.
//! - ADR-1401 D13: the copy of each master kept through a restart.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::process::Child;

use rustix::process::{WaitId, WaitIdOptions, WaitIdStatus};
use vtium::child::{self, Display as SessionDisplay, Place};
use vtium::config::{Config, Loaded, keys};
use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::{Interest, LoopHandle, Mode, PostAction};
use vtium_platform::pty::{self, Erase, Size};

use super::protocol::{
    CELL_MAX, ConsoleRef, Display, ERRNO_MAX, Errno, Grid, Head, SIGNAL_MAX, Status,
};
use super::store::Keep;
use super::watch::Seen;

/// The most consoles one head and every head together may have open, as
/// vtium.conf's `head-consoles-max` and `consoles-max` set them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    pub head: usize,
    pub total: usize,
}

/// What the root process reads from vtium.conf, and from its own place,
/// for the sessions it starts.
pub struct Conf {
    /// The top level of vtium.conf.
    pub top: Config,
    /// The configuration of each head that has a `login` line of its own,
    /// by the head's name.
    pub own: BTreeMap<String, Config>,
    /// Every head the drawing process may name: the heads of vtium.conf
    /// and those of the built-in configuration, whose head the display
    /// guard lights when no head of the file runs on vtium's VT.
    pub known: BTreeSet<String>,
    pub caps: Caps,
    /// The seat, and the VT's number, which each session's environment
    /// names.
    pub seat: String,
    pub vtnr: Option<u32>,
    /// The root process's own environment, without systemd's variables,
    /// which a session inherits with `reset-env` off.
    pub env: Vec<(OsString, OsString)>,
}

impl Conf {
    /// Reads what the sessions need from `loaded`, the root process's
    /// reading of vtium.conf, for the VT numbered `vt`.
    pub fn read(loaded: &Loaded, vt: Option<u16>, env: Vec<(OsString, OsString)>) -> Conf {
        let top = loaded.config.clone();
        let rules = vtium::head::rules(&loaded.path, &loaded.file).rules;
        let path = vtium::config::path(&top);
        let mut own = BTreeMap::new();
        for head in rules.heads.iter().filter(|h| h.login.is_some()) {
            let settings: Vec<vtium_settings::Setting> = rules
                .settings(head)
                .into_iter()
                .map(|(_, s)| s.clone())
                .collect();
            let (config, _) = top.for_head(&path, &settings);
            own.insert(head.name.clone(), config);
        }
        let known = rules
            .heads
            .iter()
            .chain(vtium_settings::Config::builtin().heads.iter())
            .map(|h| h.name.clone())
            .collect();
        let count = |key, default: u32| {
            let n = top.number(key).unwrap_or(u64::from(default));
            usize::try_from(n).unwrap_or(usize::MAX)
        };
        let caps = Caps {
            head: count(
                &keys::HEAD_CONSOLES_MAX,
                vtium_settings::HEAD_CONSOLES_DEFAULT,
            ),
            total: count(&keys::CONSOLES_MAX, vtium_settings::CONSOLES_TOTAL_DEFAULT),
        };
        Conf {
            top,
            own,
            known,
            caps,
            seat: "seat0".to_owned(),
            vtnr: vt.map(u32::from),
            env: vtium::survive::session_env(env),
        }
    }

    /// The configuration a session of `head` runs by: the head's own where
    /// it has a `login` line, and the top level's otherwise, as vtium in
    /// one process chooses it.
    fn config(&self, head: &str) -> &Config {
        self.own.get(head).unwrap_or(&self.top)
    }
}

/// Whether `grid` and the hints hold values the root process allows: a
/// grid of 1 to 65535 columns and rows, a character cell of 1 to
/// [`CELL_MAX`] pixels each way, and one of the two kinds of display. The
/// protocol's reader refuses other values already. The root process
/// checks them again here, so a session starts only from values it has
/// checked itself, whatever reached it.
pub fn allowed(grid: &Grid, display: Display) -> bool {
    let cell = 1..=CELL_MAX;
    grid.cols >= 1
        && grid.rows >= 1
        && cell.contains(&grid.cell_width)
        && cell.contains(&grid.cell_height)
        && matches!(display, Display::Kms | Display::Fbdev)
}

/// The error number `raw` as the protocol carries it, or `EIO` when it is
/// outside the protocol's range.
pub(super) fn errno(raw: i32) -> Errno {
    Errno(
        u16::try_from(raw)
            .ok()
            .filter(|n| (1..=ERRNO_MAX).contains(n))
            .unwrap_or(5),
    )
}

/// The error number of `e`, or `EIO` when it has none.
fn io_errno(e: &io::Error) -> Errno {
    errno(e.raw_os_error().unwrap_or(libc::EIO))
}

/// A session the root process runs.
struct Running {
    /// The root process's own copy of the pty's master.
    master: OwnedFd,
    /// The program the root process started, `None` for a session an
    /// earlier vtium started, which the root process cannot wait for.
    child: Option<Child>,
    /// The program's pidfd, or for a kept session, the pidfd of the
    /// process that led the pty's session when it was taken back.
    pidfd: OwnedFd,
}

/// A session whose console closed, kept only until its process exits so
/// the root process can reap it, and counted toward the caps until then.
/// Its master is gone: the root process drops its copy as it hangs the
/// session up, so once the drawing process lets its copy go too, the
/// kernel hangs the pty up, and a program that ignores `SIGHUP` still
/// sees its terminal end.
struct Closing {
    head: Head,
    child: Option<Child>,
    pidfd: OwnedFd,
}

/// What wakes the root process's loop when a session's or a helper's
/// process exits: it watches a copy of the process's pidfd.
pub type Wake = Box<dyn FnMut(BorrowedFd<'_>)>;

/// Watches a copy of `pidfd` in the loop behind `handle`, so the loop
/// wakes once the process exits, and the turn that follows reaps it. The
/// watch ends with that wake: a pidfd stays readable after the exit.
pub fn wake_at_exit(handle: &LoopHandle<'static, Seen>, pidfd: BorrowedFd<'_>) {
    let watched = pidfd.try_clone_to_owned().and_then(|fd| {
        handle
            .insert_source(Generic::new(fd, Interest::READ, Mode::Level), |_, _, _| {
                Ok(PostAction::Remove)
            })
            .map(drop)
            .map_err(|e| io::Error::other(e.error))
    });
    if let Err(e) = watched {
        log::warn!(
            target: "broker",
            "vtium's root process cannot watch a process it started, so it learns of the exit only at its next event: {e}"
        );
    }
}

/// The sessions the root process runs, by their console.
pub struct Sessions {
    conf: Conf,
    store: Box<dyn Keep>,
    wake: Wake,
    open: BTreeMap<ConsoleRef, Running>,
    /// Sessions whose console closed, until their process exits. Their
    /// exits are not reported, since the drawing process has let the
    /// console go.
    closing: Vec<Closing>,
}

impl Sessions {
    pub fn new(conf: Conf, store: Box<dyn Keep>, wake: Wake) -> Sessions {
        Sessions {
            conf,
            store,
            wake,
            open: BTreeMap::new(),
            closing: Vec::new(),
        }
    }

    /// Opens `console`'s session at `grid`, with the two hints, and gives
    /// the root process's answer: a copy of the pty's master, or why it
    /// made none.
    ///
    /// - `EINVAL`: the grid or a hint is outside the allowed values.
    /// - `ENOENT`: vtium.conf names no such head.
    /// - `EEXIST`: the console's session is open already.
    /// - `EMFILE`: the head, or every head together, has as many consoles
    ///   as vtium.conf's caps allow, or as many closed sessions whose
    ///   processes still run.
    /// - any other error: the pty or the program did not start.
    pub fn open(
        &mut self,
        console: &ConsoleRef,
        grid: &Grid,
        display: Display,
        truecolor: bool,
    ) -> Result<OwnedFd, Errno> {
        if !allowed(grid, display) {
            return Err(errno(libc::EINVAL));
        }
        let head = console.head.as_str();
        if !self.conf.known.contains(head) {
            return Err(errno(libc::ENOENT));
        }
        if self.open.contains_key(console) {
            return Err(errno(libc::EEXIST));
        }
        let caps = self.conf.caps;
        let of_head = self.open.keys().filter(|c| c.head == console.head).count();
        if of_head >= caps.head || self.open.len() >= caps.total {
            return Err(errno(libc::EMFILE));
        }
        // Closed sessions whose processes have exited no longer count.
        self.reap_closing();
        let closing = self.closing.iter().filter(|c| c.head == console.head);
        if closing.count() >= caps.head || self.closing.len() >= caps.total {
            return Err(errno(libc::EMFILE));
        }
        let size = Size {
            cols: grid.cols,
            rows: grid.rows,
            cell_width: grid.cell_width,
            cell_height: grid.cell_height,
        };
        let running = match self.store.take(head, console.console) {
            Some(master) => match adopt(master, size, console) {
                Some(running) => {
                    // The line vtium in one process logs for the same
                    // event, so the log reads the same whether vtium
                    // splits or not.
                    log::info!(
                        "head {head}: console {} takes back the session that outlived the last vtium",
                        console.console
                    );
                    Ok(running)
                }
                // The session ended while no vtium ran, so the console
                // starts a new one, as vtium in one process restarts it.
                None => {
                    log::info!(
                        "head {head}: console {}: its session ended while vtium was away",
                        console.console
                    );
                    self.store.forget(head, console.console);
                    self.start(console, size, display, truecolor)
                }
            },
            None => self.start(console, size, display, truecolor),
        }
        .map_err(|e| {
            log::warn!(
                target: "broker",
                "head {head}: console {}: vtium's root process cannot start the session: {e}",
                console.console
            );
            io_errno(&e)
        })?;
        let theirs = running.master.try_clone().map_err(|e| io_errno(&e))?;
        self.store
            .keep(head, console.console, running.master.as_fd());
        (self.wake)(running.pidfd.as_fd());
        self.open.insert(console.clone(), running);
        Ok(theirs)
    }

    /// Starts a new session's program on a new pty.
    fn start(
        &self,
        console: &ConsoleRef,
        size: Size,
        display: Display,
        truecolor: bool,
    ) -> io::Result<Running> {
        let head = console.head.as_str();
        let config = self.conf.config(head);
        let place = Place {
            seat: Some(&self.conf.seat),
            vtnr: self.conf.vtnr,
            display: match display {
                Display::Kms => SessionDisplay::Kms,
                Display::Fbdev => SessionDisplay::Fbdev,
            },
            console: Some((head, console.console)),
            logind_seat: false,
            truecolor,
        };
        let spec = child::child(config, &place, self.conf.env.clone());
        let command = spec
            .command()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no command to run"))?;
        let erase = if self.conf.top.switch(&keys::BACKSPACE_DELETE) {
            Erase::Delete
        } else {
            Erase::Backspace
        };
        let started = pty::open_session(command, size, erase)?;
        Ok(Running {
            master: started.master,
            child: Some(started.child),
            pidfd: started.pidfd,
        })
    }

    /// The consoles whose sessions an earlier vtium kept, which the root
    /// process names to the drawing process at its start, so that each
    /// opens again as its head starts. The root process hangs up each
    /// other session the keeper gave back: one of a head that vtium.conf
    /// no longer names, and one kept under a name vtium does not write.
    ///
    /// References:
    /// - ADR-1401 D13: a starting vtium takes back each kept session, and
    ///   hangs up one whose head is gone from vtium.conf.
    pub fn kept(&mut self) -> Vec<ConsoleRef> {
        let known: BTreeSet<String> = self
            .conf
            .known
            .iter()
            .filter(|head| Head::new(head).is_some())
            .cloned()
            .collect();
        self.store
            .waiting(&known)
            .into_iter()
            .filter_map(|(head, console)| {
                Some(ConsoleRef {
                    head: Head::new(&head)?,
                    console,
                })
            })
            .collect()
    }

    /// Hangs `console`'s session up, when it is open, and drops the root
    /// process's copy of its master. Its process is reaped when it exits,
    /// and its exit is not reported. A session an earlier vtium kept for
    /// the console, which no console has taken back, is hung up too: the
    /// drawing process closes it so when it has no place for it.
    pub fn close(&mut self, console: &ConsoleRef) {
        let head = console.head.as_str();
        let Some(running) = self.open.remove(console) else {
            if let Some(master) = self.store.take(head, console.console) {
                self.store.forget(head, console.console);
                pty::hang_up(master.as_fd(), None);
            }
            return;
        };
        self.store.forget(console.head.as_str(), console.console);
        let Running {
            master,
            child,
            pidfd,
        } = running;
        pty::hang_up(master.as_fd(), child.is_some().then(|| pidfd.as_fd()));
        drop(master);
        self.closing.push(Closing {
            head: console.head.clone(),
            child,
            pidfd,
        });
    }

    /// Ends the sessions at the drawing process's end, as vtium in one
    /// process does at its exit: each session the keeper holds runs on for
    /// the next vtium, which takes it back, and every other session is hung
    /// up. vtium cannot tell a restart from a stop, so systemd decides what
    /// becomes of the kept sessions: a restart keeps its store, and a stop
    /// closes it.
    ///
    /// References:
    /// - ADR-1401 D13, item 9: vtium never hangs up a session that the
    ///   store or the holder keeps when it exits.
    pub fn end(&mut self) {
        let unkept: Vec<ConsoleRef> = self
            .open
            .keys()
            .filter(|c| !self.store.holds(c.head.as_str(), c.console))
            .cloned()
            .collect();
        for console in &unkept {
            self.close(console);
        }
    }

    /// Whether vtium.conf, or the built-in configuration, names `head`.
    pub fn knows(&self, head: &str) -> bool {
        self.conf.known.contains(head)
    }

    /// Sends `SIGWINCH` to the foreground process group of `console`'s
    /// session, when it is open.
    pub fn winch(&mut self, console: &ConsoleRef) {
        if let Some(running) = self.open.get(console)
            && let Err(e) = pty::winch(running.master.as_fd())
        {
            log::debug!(
                target: "broker",
                "head {}: console {}: vtium's root process cannot send SIGWINCH: {e}",
                console.head.as_str(),
                console.console
            );
        }
    }

    /// The root process's copy of `console`'s pty master, while the
    /// session is open. The image helper's account is read from the
    /// pty's slave through it.
    pub fn master(&self, console: &ConsoleRef) -> Option<BorrowedFd<'_>> {
        self.open.get(console).map(|r| r.master.as_fd())
    }

    /// The process id of the program the root process started for
    /// `console`'s session, while it runs; `None` for a kept session.
    pub fn child(&self, console: &ConsoleRef) -> Option<u32> {
        self.open
            .get(console)
            .and_then(|r| r.child.as_ref())
            .map(Child::id)
    }

    /// Reaps each session whose process has exited, and gives the exits of
    /// those whose console is still open, in the consoles' order. The
    /// keeper drops each such session's master.
    pub fn reap(&mut self) -> Vec<(ConsoleRef, Status)> {
        self.reap_closing();
        let done: Vec<(ConsoleRef, Status)> = self
            .open
            .iter()
            .filter_map(|(console, r)| {
                ended(r.child.as_ref(), r.pidfd.as_fd()).map(|status| (console.clone(), status))
            })
            .collect();
        for (console, _) in &done {
            self.open.remove(console);
            self.store.forget(console.head.as_str(), console.console);
        }
        done
    }

    /// Reaps each closed session whose process has exited.
    fn reap_closing(&mut self) {
        self.closing
            .retain(|c| ended(c.child.as_ref(), c.pidfd.as_fd()).is_none());
    }
}

/// Takes back `master`, kept through a restart, for `console`, as vtium
/// in one process does: it watches the process that leads the pty's
/// session, sets the pty to the console's grid, and sends `SIGWINCH` when
/// the pty already has that size, so the program redraws. Gives `None`
/// when the session has ended.
///
/// A resize or a redraw that fails leaves the session with its console,
/// with a warning, as vtium in one process keeps it. The `SIGWINCH` fails,
/// for example, when the process group in the pty's foreground has just
/// ended. Were the take-back to fail there, the session would run on with
/// no console, and the next open of the console would start a second
/// session under the same name.
fn adopt(master: OwnedFd, size: Size, console: &ConsoleRef) -> Option<Running> {
    let pidfd = pty::leader(master.as_fd())?;
    let redrawn = match pty::size(master.as_fd()) {
        Ok(held) if held == size => pty::winch(master.as_fd()),
        _ => pty::set_size(master.as_fd(), size),
    };
    if let Err(e) = redrawn {
        log::warn!(
            "head {}: console {}: cannot size the recovered session: {e}",
            console.head.as_str(),
            console.console
        );
    }
    Some(Running {
        master,
        child: None,
        pidfd,
    })
}

/// How a session's process ended, once it has, reaped for a child of the
/// root process. `child` is `None` for a kept session, whose process is
/// not the root process's child, so its pidfd only says that it ended,
/// and its status is 0, as vtium in one process reports it.
fn ended(child: Option<&Child>, pidfd: BorrowedFd<'_>) -> Option<Status> {
    if child.is_none() {
        return exited(pidfd).then_some(Status::Code(0));
    }
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG;
    loop {
        match rustix::process::waitid(WaitId::PidFd(pidfd), options) {
            Ok(Some(status)) => return Some(status_of(&status)),
            Ok(None) => return None,
            Err(rustix::io::Errno::INTR) => {}
            // reaped already: nothing more to learn
            Err(_) => return Some(Status::Code(0)),
        }
    }
}

/// Whether the process behind `pidfd` has exited: its pidfd reads ready.
fn exited(pidfd: BorrowedFd<'_>) -> bool {
    let mut fds = [rustix::event::PollFd::new(
        &pidfd,
        rustix::event::PollFlags::IN,
    )];
    let zero = rustix::event::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    matches!(rustix::event::poll(&mut fds, Some(&zero)), Ok(n) if n > 0)
}

/// A wait's status as the protocol carries it.
pub(super) fn status_of(status: &WaitIdStatus) -> Status {
    match (status.exit_status(), status.terminating_signal()) {
        (Some(code), _) => Status::Code(u8::try_from(code & 0xff).unwrap_or(1)),
        (None, Some(signal)) => Status::Signal(
            u8::try_from(signal)
                .ok()
                .filter(|s| (1..=SIGNAL_MAX).contains(s))
                .unwrap_or(SIGNAL_MAX),
        ),
        (None, None) => Status::Code(1),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
pub(super) mod tests {
    //! The sessions against real ptys and programs that the tests choose
    //! through each stand-in vtium.conf's `login` line.

    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::super::outbox::Outbox;
    use super::super::protocol::{Head, Link, Order, Request};
    use super::super::store::Nowhere;
    use super::super::{Jobs, Processes, Serve, helpers::Helpers, power::PowerKeys};

    /// How many waits of 50 ms a test makes for a program's output before
    /// it fails, 10 s in all, so a broken broker fails the test instead of
    /// hanging it.
    const WAITS: u32 = 200;

    pub(in crate::broker) fn config(args: &[&str]) -> Config {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        vtium::config::load(&args, &mut |_: &std::path::Path| Ok(None)).config
    }

    /// A vtium.conf whose top level runs `top` as the login, whose head
    /// `own` runs `own_login`, and whose heads `left` and `own` are known.
    pub(in crate::broker) fn conf(top: &str, own_login: &str, caps: Caps) -> Conf {
        let mut own = BTreeMap::new();
        own.insert(
            "own".to_owned(),
            config(&["--login", "--", "/bin/sh", "-c", own_login]),
        );
        Conf {
            top: config(&["--login", "--", "/bin/sh", "-c", top]),
            own,
            known: ["left", "own"].map(str::to_owned).into(),
            caps,
            seat: "seat0".to_owned(),
            vtnr: Some(7),
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        }
    }

    pub(in crate::broker) const WIDE: Caps = Caps {
        head: 50,
        total: 50,
    };

    /// The pidfds the sessions asked the loop to watch.
    pub(in crate::broker) type Watched = Rc<RefCell<Vec<OwnedFd>>>;

    pub(in crate::broker) fn watching() -> (Watched, Wake) {
        let watched: Watched = Rc::default();
        let w = Rc::clone(&watched);
        let wake: Wake = Box::new(move |pidfd: BorrowedFd<'_>| {
            w.borrow_mut().push(pidfd.try_clone_to_owned().unwrap());
        });
        (watched, wake)
    }

    pub(in crate::broker) fn console(head: &str, id: u64) -> ConsoleRef {
        ConsoleRef {
            head: Head::new(head).unwrap(),
            console: id,
        }
    }

    pub(in crate::broker) const GRID: Grid = Grid {
        cols: 100,
        rows: 30,
        cell_width: 9,
        cell_height: 18,
    };

    /// What the program on `master` prints, until `want` appears.
    pub(in crate::broker) fn read_until(master: &OwnedFd, want: &str) -> String {
        let mut got = Vec::new();
        let mut waits = 0;
        while !String::from_utf8_lossy(&got).contains(want) {
            waits += 1;
            assert!(
                waits <= WAITS,
                "{want:?} never came; the program printed {:?}",
                String::from_utf8_lossy(&got)
            );
            let mut fds = [rustix::event::PollFd::new(
                master,
                rustix::event::PollFlags::IN,
            )];
            let tick = rustix::event::Timespec {
                tv_sec: 0,
                tv_nsec: 50_000_000,
            };
            let _ = rustix::event::poll(&mut fds, Some(&tick));
            let mut buf = [0u8; 4096];
            match rustix::io::read(master, &mut buf) {
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
                Err(e) => panic!("the pty failed: {e}; the program printed {got:?}"),
            }
        }
        String::from_utf8_lossy(&got).into_owned()
    }

    /// Waits until each watched process has exited.
    pub(in crate::broker) fn wait_for_exits(watched: &Watched) {
        for pidfd in watched.borrow().iter() {
            let mut fds = [rustix::event::PollFd::new(
                pidfd,
                rustix::event::PollFlags::IN,
            )];
            let bound = rustix::event::Timespec {
                tv_sec: 10,
                tv_nsec: 0,
            };
            let n = rustix::event::poll(&mut fds, Some(&bound)).unwrap();
            assert_eq!(n, 1, "a session's process never exited");
        }
    }

    fn sessions(conf: Conf) -> (Sessions, Watched) {
        let (watched, wake) = watching();
        (Sessions::new(conf, Box::new(Nowhere), wake), watched)
    }

    /// Each value outside the allowed ones, by the field it sets.
    fn refused_grids() -> Vec<(&'static str, Grid)> {
        vec![
            ("no columns", Grid { cols: 0, ..GRID }),
            ("no rows", Grid { rows: 0, ..GRID }),
            (
                "a cell 0 wide",
                Grid {
                    cell_width: 0,
                    ..GRID
                },
            ),
            (
                "a cell 0 tall",
                Grid {
                    cell_height: 0,
                    ..GRID
                },
            ),
            (
                "a cell too wide",
                Grid {
                    cell_width: CELL_MAX + 1,
                    ..GRID
                },
            ),
            (
                "a cell too tall",
                Grid {
                    cell_height: CELL_MAX + 1,
                    ..GRID
                },
            ),
        ]
    }

    #[test]
    fn a_session_outside_the_allowed_values_is_refused() {
        let (mut s, watched) = sessions(conf("echo ran; sleep 1000", "exit 0", WIDE));
        for (what, grid) in refused_grids() {
            assert_eq!(
                s.open(&console("left", 1), &grid, Display::Kms, true).err(),
                Some(errno(libc::EINVAL)),
                "{what}"
            );
        }
        assert!(
            watched.borrow().is_empty(),
            "a refused request started a program"
        );
        assert!(s.open.is_empty());
        // The largest values allowed, and the smallest, open.
        let largest = Grid {
            cols: u16::MAX,
            rows: u16::MAX,
            cell_width: CELL_MAX,
            cell_height: CELL_MAX,
        };
        let smallest = Grid {
            cols: 1,
            rows: 1,
            cell_width: 1,
            cell_height: 1,
        };
        assert!(
            s.open(&console("left", 1), &largest, Display::Fbdev, false)
                .is_ok()
        );
        assert!(
            s.open(&console("left", 2), &smallest, Display::Kms, true)
                .is_ok()
        );
        for id in [1, 2] {
            s.close(&console("left", id));
        }
    }

    #[test]
    fn a_request_outside_the_allowed_values_gets_einval_and_starts_nothing() {
        let (mut jobs, watched) = jobs(conf("echo ran; sleep 1000", "exit 0", WIDE));
        let (link, theirs) = Link::pair().unwrap();
        let mut out = Outbox::new(&link);
        let drawer_end = Link::passed(theirs).unwrap();
        for (_, grid) in refused_grids() {
            let request = Request::OpenSession {
                console: console("left", 1),
                grid,
                display: Display::Kms,
                truecolor: true,
            };
            jobs.serve(request, &mut out).unwrap();
            match drawer_end.next_order().unwrap() {
                vtium_broker_protocol::Got::Message(Ok((order, fd))) => {
                    assert_eq!(
                        order,
                        Order::Pty {
                            console: console("left", 1),
                            opened: Err(errno(libc::EINVAL)),
                        }
                    );
                    assert!(fd.is_none());
                }
                other => panic!("no answer: {other:?}"),
            }
        }
        assert!(
            watched.borrow().is_empty(),
            "a refused request started a program"
        );
    }

    /// The process jobs with `conf`'s sessions, no font helper and no
    /// power key, served through the broker's dispatch.
    pub(in crate::broker) fn jobs(conf: Conf) -> (Jobs, Watched) {
        let (watched, wake) = watching();
        let (_, helper_wake) = watching();
        let processes = Processes::new(
            Sessions::new(conf, Box::new(Nowhere), wake),
            Helpers::new(None, None, helper_wake),
            PowerKeys::new(&config(&[]), Box::new(vtium::power::System), None),
        );
        (Jobs { processes }, watched)
    }

    #[test]
    fn the_session_gets_its_grid_and_hints_and_the_brokers_command() {
        let show = "stty size; echo \"$TERM_SESSION_TYPE/${COLORTERM:-none}/$VTIUM_HEAD/$VTIUM_CONSOLE/$XDG_VTNR\"; sleep 1000";
        let (mut s, _) = sessions(conf(
            &format!("echo TOPCMD; {show}"),
            &format!("echo OWNCMD; {show}"),
            WIDE,
        ));
        let master = s
            .open(&console("left", 4), &GRID, Display::Fbdev, false)
            .unwrap();
        let out = read_until(&master, "/7");
        assert!(out.contains("TOPCMD") && !out.contains("OWNCMD"), "{out}");
        assert!(
            out.contains("30 100"),
            "the grid is not the pty's size: {out}"
        );
        assert!(out.contains("fb/none/left/4/7"), "{out}");
        let size = pty::size(master.as_fd()).unwrap();
        assert_eq!((size.cell_width, size.cell_height), (9, 18));

        let master = s
            .open(&console("own", 1), &GRID, Display::Kms, true)
            .unwrap();
        let out = read_until(&master, "/7");
        assert!(
            out.contains("OWNCMD") && !out.contains("TOPCMD"),
            "the head's own login did not run: {out}"
        );
        assert!(out.contains("kms/truecolor/own/1/7"), "{out}");
        for c in [console("left", 4), console("own", 1)] {
            s.close(&c);
        }
    }

    #[test]
    fn an_unknown_head_a_second_session_and_the_caps_are_refused() {
        let caps = Caps { head: 2, total: 3 };
        let (mut s, _) = sessions(conf("sleep 1000", "sleep 1000", caps));
        let open = |s: &mut Sessions, head, id| {
            s.open(&console(head, id), &GRID, Display::Kms, true)
                .map(drop)
                .map_err(|e| e.0)
        };
        let e = |raw: i32| u16::try_from(raw).unwrap();
        assert_eq!(open(&mut s, "right", 1), Err(e(libc::ENOENT)));
        assert_eq!(open(&mut s, "left", 1), Ok(()));
        assert_eq!(open(&mut s, "left", 1), Err(e(libc::EEXIST)));
        assert_eq!(open(&mut s, "left", 2), Ok(()));
        assert_eq!(
            open(&mut s, "left", 3),
            Err(e(libc::EMFILE)),
            "the head's cap"
        );
        assert_eq!(open(&mut s, "own", 1), Ok(()));
        assert_eq!(
            open(&mut s, "own", 2),
            Err(e(libc::EMFILE)),
            "every head's cap"
        );
        // A closed console makes room again.
        s.close(&console("left", 1));
        assert_eq!(open(&mut s, "own", 2), Ok(()));
        for (head, id) in [("left", 2), ("own", 1), ("own", 2)] {
            s.close(&console(head, id));
        }
    }

    /// A drawer that opens and closes a console in a loop starts no more
    /// root sessions than the caps allow. Each program here ignores
    /// `SIGHUP`, and the test keeps each master, as a drawer would, so no
    /// closed session ends by itself. Once the head has as many closed
    /// sessions still running as its cap, the next open is refused with
    /// `EMFILE`. When their masters close, the programs end, and the
    /// console opens again.
    #[test]
    fn opening_and_closing_a_console_in_a_loop_stays_within_the_caps() {
        let caps = Caps { head: 2, total: 3 };
        let (mut s, watched) =
            sessions(conf("trap '' HUP; echo ready; read x", "sleep 1000", caps));
        let left = console("left", 1);
        let mut masters = Vec::new();
        for _ in 0..caps.head {
            let master = s.open(&left, &GRID, Display::Kms, true).unwrap();
            read_until(&master, "ready");
            s.close(&left);
            masters.push(master);
        }
        assert_eq!(
            s.open(&left, &GRID, Display::Kms, true)
                .map(drop)
                .map_err(|e| e.0),
            Err(u16::try_from(libc::EMFILE).unwrap()),
            "a closed session that still runs counts toward the head's cap"
        );
        drop(masters);
        wait_for_exits(&watched);
        let master = s.open(&left, &GRID, Display::Kms, true).unwrap();
        drop(master);
        s.close(&left);
    }

    #[test]
    fn a_sessions_exit_is_reported_and_a_closed_ones_is_not() {
        let (mut s, watched) = sessions(conf("exit 7", "sleep 1000", WIDE));
        let _quick = s
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        let _slow = s
            .open(&console("own", 1), &GRID, Display::Kms, true)
            .unwrap();
        s.close(&console("own", 1));
        wait_for_exits(&watched);
        assert_eq!(s.reap(), [(console("left", 1), Status::Code(7))]);
        assert!(s.open.is_empty());
        assert!(
            s.closing.is_empty(),
            "the closed session's process was not reaped"
        );
        assert_eq!(s.reap(), []);
    }

    #[test]
    fn a_closed_session_that_ignores_sighup_ends_once_its_masters_close() {
        // The program ignores SIGHUP, as a shell with `trap '' HUP` does,
        // so only the pty's last master closing ends it: its `read` then
        // sees the hang-up.
        let (mut s, watched) = sessions(conf("trap '' HUP; echo ready; read x", "exit 0", WIDE));
        let master = s
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        read_until(&master, "ready");
        // The drawing process lets its copy go as it closes the console.
        drop(master);
        s.close(&console("left", 1));
        wait_for_exits(&watched);
        assert_eq!(s.reap(), [], "a closed session's exit was reported");
        assert!(
            s.closing.is_empty(),
            "the closed session's process was not reaped"
        );
    }

    #[test]
    fn winch_reaches_the_sessions_foreground_group() {
        let (mut s, _) = sessions(conf(
            "trap 'echo winched' WINCH; echo ready; while :; do sleep 0.05; done",
            "exit 0",
            WIDE,
        ));
        let master = s
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        read_until(&master, "ready");
        s.winch(&console("left", 1));
        read_until(&master, "winched");
        s.close(&console("left", 1));
    }

    /// A keeper that records what it is told, holds one master an earlier
    /// vtium kept, and says it holds the masters of the consoles in
    /// `holds`.
    #[derive(Default)]
    struct Recorded {
        told: Rc<RefCell<Vec<String>>>,
        kept: Option<(String, u64, OwnedFd)>,
        holds: Vec<(String, u64)>,
    }

    impl Keep for Recorded {
        fn keep(&mut self, head: &str, console: u64, _: BorrowedFd<'_>) {
            self.told
                .borrow_mut()
                .push(format!("keep {head} {console}"));
        }
        fn forget(&mut self, head: &str, console: u64) {
            self.told
                .borrow_mut()
                .push(format!("forget {head} {console}"));
        }
        fn holds(&self, head: &str, console: u64) -> bool {
            self.holds.iter().any(|(h, c)| h == head && *c == console)
        }
        fn take(&mut self, head: &str, console: u64) -> Option<OwnedFd> {
            let at = self.kept.as_ref()?;
            if at.0 != head || at.1 != console {
                return None;
            }
            self.kept.take().map(|(_, _, fd)| fd)
        }
        fn waiting(&mut self, known: &BTreeSet<String>) -> Vec<(String, u64)> {
            self.kept
                .iter()
                .filter(|(head, _, _)| known.contains(head))
                .map(|(head, id, _)| (head.clone(), *id))
                .collect()
        }
    }

    /// A session an earlier vtium started on a pty of its own, which
    /// prints `ready` and then sleeps with SIGHUP's default, so a hangup
    /// ends it.
    fn earlier_session() -> pty::Session {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "echo ready; exec sleep 1000"]);
        let small = Size {
            cols: 40,
            rows: 10,
            cell_width: 9,
            cell_height: 18,
        };
        let earlier = pty::open_session(command, small, Erase::Delete).unwrap();
        read_until(&earlier.master, "ready");
        earlier
    }

    /// The sessions of `conf` over a real store that an earlier vtium left
    /// `kept` in, with no keeper.
    fn sessions_kept(conf: Conf, kept: Vec<(String, OwnedFd)>) -> (Sessions, Watched) {
        let (watched, wake) = watching();
        let store = super::super::store::Store::new(vtium::survive::Keeper::Off, kept);
        (Sessions::new(conf, Box::new(store), wake), watched)
    }

    fn name(head: &str, id: u64) -> String {
        vtium::survive::Name::new(head, id).encode()
    }

    /// Whether the process behind `pidfd` exits within 10 s.
    fn exits(pidfd: &OwnedFd) -> bool {
        let mut fds = [rustix::event::PollFd::new(
            pidfd,
            rustix::event::PollFlags::IN,
        )];
        let bound = rustix::event::Timespec {
            tv_sec: 10,
            tv_nsec: 0,
        };
        rustix::event::poll(&mut fds, Some(&bound)).unwrap() == 1
    }

    /// Two sessions an earlier vtium kept on head `left` both come back:
    /// the root process names both consoles to the drawing process, and
    /// each console that opens gets its own kept session. The session of
    /// head `gone`, which vtium.conf no longer names, and the one kept
    /// under a name vtium does not write are hung up at once, before the
    /// drawing process hears of them.
    #[test]
    fn two_kept_sessions_come_back_and_one_of_a_removed_head_is_hung_up() {
        let one = earlier_session();
        let two = earlier_session();
        let gone = earlier_session();
        let stray = earlier_session();
        let (mut s, _) = sessions_kept(
            conf("echo NEW; sleep 1000", "exit 0", WIDE),
            vec![
                (name("left", 2), two.master.try_clone().unwrap()),
                (name("gone", 1), gone.master.try_clone().unwrap()),
                ("not-vtiums".to_owned(), stray.master.try_clone().unwrap()),
                (name("left", 1), one.master.try_clone().unwrap()),
            ],
        );
        assert_eq!(s.kept(), [console("left", 1), console("left", 2)]);
        assert!(exits(&gone.pidfd), "the removed head's session runs on");
        assert!(exits(&stray.pidfd), "the stray session runs on");
        for (id, earlier) in [(1, &one), (2, &two)] {
            let master = s
                .open(&console("left", id), &GRID, Display::Kms, true)
                .unwrap();
            assert_eq!(
                rustix::termios::tcgetsid(&master).unwrap(),
                rustix::termios::tcgetsid(&earlier.master).unwrap(),
                "console {id} did not get its kept session"
            );
            assert!(!exited(earlier.pidfd.as_fd()));
        }
        for id in [1, 2] {
            s.close(&console("left", id));
        }
        for earlier in [one, two, gone, stray] {
            let mut child = earlier.child;
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// A kept session whose console the drawing process closes before it
    /// opens it, because it has no place for it, is hung up, and no later
    /// open of that console gets it.
    #[test]
    fn a_kept_session_the_drawer_closes_unopened_is_hung_up() {
        let kept = earlier_session();
        let (mut s, watched) = sessions_kept(
            conf("echo NEW; sleep 1000", "exit 0", WIDE),
            vec![(name("left", 3), kept.master.try_clone().unwrap())],
        );
        assert_eq!(s.kept(), [console("left", 3)]);
        s.close(&console("left", 3));
        assert!(exits(&kept.pidfd), "the closed kept session runs on");
        let master = s
            .open(&console("left", 3), &GRID, Display::Kms, true)
            .unwrap();
        read_until(&master, "NEW");
        s.close(&console("left", 3));
        wait_for_exits(&watched);
        let mut child = kept.child;
        let _ = child.wait();
    }

    /// A kept session that ended while no vtium ran leaves its console a
    /// new session, as vtium in one process restarts it, rather than an
    /// error that would end vtium.
    #[test]
    fn a_kept_session_that_ended_while_vtium_was_away_starts_anew() {
        let kept = earlier_session();
        let mut child = kept.child;
        child.kill().unwrap();
        child.wait().unwrap();
        let (mut s, watched) = sessions_kept(
            conf("echo NEW; sleep 1000", "exit 0", WIDE),
            vec![(name("left", 1), kept.master.try_clone().unwrap())],
        );
        drop(kept.master);
        let master = s
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        read_until(&master, "NEW");
        s.close(&console("left", 1));
        wait_for_exits(&watched);
    }

    #[test]
    fn the_keeper_holds_each_master_while_its_session_runs() {
        let told: Rc<RefCell<Vec<String>>> = Rc::default();
        let store = Recorded {
            told: Rc::clone(&told),
            ..Recorded::default()
        };
        let (watched, wake) = watching();
        let mut s = Sessions::new(conf("exit 0", "sleep 1000", WIDE), Box::new(store), wake);
        let _a = s
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        let _b = s
            .open(&console("own", 2), &GRID, Display::Kms, true)
            .unwrap();
        s.close(&console("own", 2));
        wait_for_exits(&watched);
        s.reap();
        assert_eq!(
            *told.borrow(),
            ["keep left 1", "keep own 2", "forget own 2", "forget left 1"]
        );
    }

    #[test]
    fn a_kept_session_goes_back_to_its_console_and_redraws() {
        // The session an earlier vtium started, at another size.
        let mut command = std::process::Command::new("/bin/sh");
        command.args([
            "-c",
            "trap 'stty size' WINCH; echo ready; while :; do sleep 0.05; done",
        ]);
        let small = Size {
            cols: 40,
            rows: 10,
            cell_width: 9,
            cell_height: 18,
        };
        let earlier = pty::open_session(command, small, Erase::Delete).unwrap();
        read_until(&earlier.master, "ready");
        let store = Recorded {
            kept: Some(("left".to_owned(), 3, earlier.master.try_clone().unwrap())),
            ..Recorded::default()
        };
        let (_, wake) = watching();
        let mut s = Sessions::new(
            conf("echo NEW; sleep 1000", "exit 0", WIDE),
            Box::new(store),
            wake,
        );
        let master = s
            .open(&console("left", 3), &GRID, Display::Kms, true)
            .unwrap();
        // the kept session, now at the console's grid, redraws
        read_until(&master, "30 100");
        assert_eq!(
            rustix::termios::tcgetsid(&master).unwrap(),
            rustix::termios::tcgetsid(&earlier.master).unwrap(),
            "the console did not get the kept session"
        );
        s.close(&console("left", 3));
        let mut child = earlier.child;
        let _ = child.kill();
        let _ = child.wait();
    }

    /// A kept session whose redraw fails still goes back to its console,
    /// as vtium in one process keeps it, so it never runs on unseen
    /// beside a new session under the same name. The earlier session's
    /// program puts a process group of its own in the foreground, and
    /// that group's only process then exits, so the `SIGWINCH` that asks
    /// for the redraw finds nobody to signal. The earlier session already
    /// has the console's grid, which is when the root process sends that
    /// signal.
    #[test]
    fn a_kept_session_whose_redraw_fails_still_goes_back_to_its_console() {
        let mut command = std::process::Command::new("python3");
        command.args([
            "-c",
            "import os, signal\n\
             signal.signal(signal.SIGTTOU, signal.SIG_IGN)\n\
             pid = os.fork()\n\
             if pid == 0:\n    \
                 os.setpgid(0, 0)\n    \
                 os.tcsetpgrp(0, os.getpgrp())\n    \
                 os._exit(0)\n\
             os.waitpid(pid, 0)\n\
             print('ready', flush=True)\n\
             signal.pause()\n",
        ]);
        let size = Size {
            cols: GRID.cols,
            rows: GRID.rows,
            cell_width: GRID.cell_width,
            cell_height: GRID.cell_height,
        };
        let earlier = pty::open_session(command, size, Erase::Delete).unwrap();
        read_until(&earlier.master, "ready");
        assert!(
            pty::winch(earlier.master.as_fd()).is_err(),
            "the earlier session's foreground group still has a process"
        );
        let (mut s, _) = sessions_kept(
            conf("echo NEW; sleep 1000", "exit 0", WIDE),
            vec![(name("left", 1), earlier.master.try_clone().unwrap())],
        );
        let master = s
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        assert_eq!(
            rustix::termios::tcgetsid(&master).unwrap(),
            rustix::termios::tcgetsid(&earlier.master).unwrap(),
            "the console did not get the kept session"
        );
        s.close(&console("left", 1));
        let mut child = earlier.child;
        let _ = child.kill();
        let _ = child.wait();
    }

    /// At the drawing process's end, the session the keeper holds runs on
    /// and the one it does not hold is hung up. Its program, which takes
    /// SIGHUP's default, then exits.
    #[test]
    fn at_the_drawers_end_a_kept_session_runs_on_and_an_unkept_one_is_hung_up() {
        let store = Recorded {
            holds: vec![("left".to_owned(), 1)],
            ..Recorded::default()
        };
        let (watched, wake) = watching();
        let mut s = Sessions::new(
            conf(
                "echo ready; exec sleep 1000",
                "echo ready; exec sleep 1000",
                WIDE,
            ),
            Box::new(store),
            wake,
        );
        let kept = s
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        let unkept = s
            .open(&console("own", 2), &GRID, Display::Kms, true)
            .unwrap();
        read_until(&kept, "ready");
        read_until(&unkept, "ready");
        // The drawing process has exited, so its copies are gone, and the
        // test's copy of the kept master stands in for the keeper's.
        drop(unkept);
        let keepers = s
            .master(&console("left", 1))
            .unwrap()
            .try_clone_to_owned()
            .unwrap();
        s.end();
        let pidfds: Vec<OwnedFd> = watched
            .borrow()
            .iter()
            .map(|fd| fd.try_clone().unwrap())
            .collect();
        let [kept_pidfd, unkept_pidfd] = pidfds.as_slice() else {
            panic!("two sessions were watched: {pidfds:?}");
        };
        let one = Watched::default();
        one.borrow_mut().push(unkept_pidfd.try_clone().unwrap());
        wait_for_exits(&one);
        assert!(!exited(kept_pidfd.as_fd()), "the kept session was hung up");
        assert_eq!(
            s.child(&console("own", 2)),
            None,
            "the unkept session is still open"
        );
        let child = s
            .child(&console("left", 1))
            .expect("the kept session is still open");
        let _ = rustix::process::kill_process(
            rustix::process::Pid::from_raw(i32::try_from(child).unwrap()).unwrap(),
            rustix::process::Signal::KILL,
        );
        drop((kept, keepers));
    }
}
