//! The helpers that vtium's root process starts and stops for its drawing
//! process.
//!
//! A helper runs as another account than the drawing process: the font
//! helper as `nobody`, and a console's image helper as the user the
//! console's session belongs to. So the drawing process can neither start
//! one as its account nor signal one. It asks the root process, which
//! chooses the account itself, starts the helper, passes the drawing
//! process its end of the helper's socket, stops the helper when the
//! drawing process asks, and reports each helper's exit. The drawing
//! process never names a user id.
//!
//! The font helper reads its requests from its standard input and writes
//! its answers to its standard output. The root process gives it one end
//! of a stream socket as both, and passes the other end to the drawing
//! process, which reads and writes the helper through that one socket.
//!
//! The image helper's account and its two caps depend on each request:
//! whether it reads a file, and how many bytes the request needs. The
//! protocol's request to start a helper carries none of them yet, so the
//! root process refuses to start an image helper with `ENOSYS` until it
//! does.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the broker starts and stops the
//!   helpers, chooses each one's user, and passes each helper's socket.
//! - ADR-1400 D2: the helpers and their sandbox.
//! - ADR-1200 D15: the font helper runs as `nobody`.
//! - ADR-1101 D2: the image helper runs as the session's user.

use std::collections::BTreeMap;
use std::os::fd::{AsFd, OwnedFd};
use std::process::{Child, Stdio};

use rustix::net::{AddressFamily, SocketFlags, SocketType};
use rustix::process::{Pid, PidfdFlags, Signal, WaitId, WaitIdOptions};
use vtium::helper::Launcher;

use super::protocol::{Errno, Helper, Status};
use super::sessions::{Sessions, Wake, errno, status_of};

/// The most font helpers that run at once: the one the drawing process
/// uses, and one it starts while the last one goes.
pub const FONT_HELPERS_MAX: usize = 2;

/// A helper the root process started.
struct Running {
    child: Child,
    pidfd: OwnedFd,
}

/// The helpers the root process runs, by the id the drawing process gave
/// each.
pub struct Helpers {
    /// The font helper's program, opened once, and the account it runs
    /// as; `None` when vtium.conf's font helper cannot start.
    font: Option<Launcher>,
    wake: Wake,
    running: BTreeMap<u32, Running>,
}

impl Helpers {
    pub fn new(font: Option<Launcher>, wake: Wake) -> Helpers {
        Helpers {
            font,
            wake,
            running: BTreeMap::new(),
        }
    }

    /// Starts helper `id` of kind `kind`, and gives the drawing process's
    /// end of its socket, or why it started none.
    ///
    /// - `EEXIST`: a helper with that id runs already.
    /// - `ENOENT`: an image helper's console has no session open.
    /// - `ENOSYS`: an image helper, which the root process does not start
    ///   yet.
    /// - `EAGAIN`: [`FONT_HELPERS_MAX`] font helpers run already.
    /// - `ENOEXEC`: vtium.conf's font helper cannot start.
    /// - any other error: the helper did not start.
    pub fn start(&mut self, id: u32, kind: &Helper, sessions: &Sessions) -> Result<OwnedFd, Errno> {
        if self.running.contains_key(&id) {
            return Err(errno(libc::EEXIST));
        }
        match kind {
            Helper::Image(console) => {
                if sessions.master(console).is_none() {
                    return Err(errno(libc::ENOENT));
                }
                Err(errno(libc::ENOSYS))
            }
            Helper::Font => self.start_font(id),
        }
    }

    fn start_font(&mut self, id: u32) -> Result<OwnedFd, Errno> {
        if self.running.len() >= FONT_HELPERS_MAX {
            return Err(errno(libc::EAGAIN));
        }
        let launcher = self.font.as_ref().ok_or(errno(libc::ENOEXEC))?;
        let failed = |what: &str, e: &std::io::Error| {
            log::warn!(target: "broker", "vtium's root process cannot start the font helper: {what}: {e}");
            errno(e.raw_os_error().unwrap_or(libc::EIO))
        };
        let (ours, theirs) = rustix::net::socketpair(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::CLOEXEC,
            None,
        )
        .map_err(|e| failed("its socket", &e.into()))?;
        let input = theirs.try_clone().map_err(|e| failed("its socket", &e))?;
        vtium_platform::cloexec_above_stdio().map_err(|e| failed("its descriptors", &e))?;
        let mut child = launcher
            .process()
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(theirs))
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                log::warn!(target: "broker", "{}", launcher.failed(&e));
                errno(e.raw_os_error().unwrap_or(libc::EIO))
            })?;
        // the child is not reaped before the check, so its pid is its own
        if let Err(why) = launcher.dropped(child.id()) {
            log::warn!(target: "broker", "{why}");
            let _ = child.kill();
            let _ = child.wait();
            return Err(errno(libc::EPERM));
        }
        let pidfd = match i32::try_from(child.id())
            .ok()
            .and_then(Pid::from_raw)
            .map(|pid| rustix::process::pidfd_open(pid, PidfdFlags::empty()))
        {
            Some(Ok(pidfd)) => pidfd,
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(errno(libc::EIO));
            }
        };
        (self.wake)(pidfd.as_fd());
        self.running.insert(id, Running { child, pidfd });
        Ok(ours)
    }

    /// Stops helper `id`, when it runs: its whole process group gets
    /// `SIGKILL`. Its exit is reported when the root process reaps it.
    pub fn stop(&mut self, id: u32) {
        if let Some(running) = self.running.get(&id) {
            // The helper leads its own process group; its pidfd still
            // names it, since nothing has reaped it.
            let _ = rustix::process::pidfd_send_signal(&running.pidfd, Signal::KILL);
            if let Some(pid) = i32::try_from(running.child.id())
                .ok()
                .and_then(Pid::from_raw)
            {
                let _ = rustix::process::kill_process_group(pid, Signal::KILL);
            }
        }
    }

    /// Reaps each helper that has exited, and gives its exit.
    pub fn reap(&mut self) -> Vec<(u32, Status)> {
        let done: Vec<(u32, Status)> = self
            .running
            .iter()
            .filter_map(|(&id, r)| ended(r).map(|status| (id, status)))
            .collect();
        for (id, _) in &done {
            self.running.remove(id);
        }
        done
    }
}

/// How `running` ended, once it has, reaped through its pidfd.
fn ended(running: &Running) -> Option<Status> {
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG;
    loop {
        match rustix::process::waitid(WaitId::PidFd(running.pidfd.as_fd()), options) {
            Ok(Some(status)) => return Some(status_of(&status)),
            Ok(None) => return None,
            Err(rustix::io::Errno::INTR) => {}
            // reaped already: nothing more to learn
            Err(_) => return Some(Status::Code(0)),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    use vtium_broker_protocol::Got;

    use super::super::outbox::Outbox;
    use super::super::power::PowerKeys;
    use super::super::protocol::{Display, Link, Order, Request};
    use super::super::sessions::tests::{
        GRID, WIDE, conf, config, console, wait_for_exits, watching,
    };
    use super::super::store::Nowhere;
    use super::super::{Jobs, Processes, Serve};

    /// A font helper that runs until something stops it.
    fn sleeper() -> Launcher {
        Launcher::open(vtium::helper::Command {
            program: "/bin/sleep".into(),
            args: vec!["1000".into()],
            account: None,
        })
        .unwrap()
    }

    /// The next order the drawer reads.
    fn next(drawer: &Link) -> (Order, Option<OwnedFd>) {
        match drawer.next_order().unwrap() {
            Got::Message(Ok(got)) => got,
            other => panic!("no order: {other:?}"),
        }
    }

    #[test]
    fn a_helpers_stop_and_a_sessions_exit_reach_the_drawer_as_messages() {
        let (sessions_watched, session_wake) = watching();
        let (helpers_watched, helper_wake) = watching();
        let mut jobs = Jobs {
            processes: Processes::new(
                Sessions::new(
                    conf("exit 3", "exit 0", WIDE),
                    Box::new(Nowhere),
                    session_wake,
                ),
                Helpers::new(Some(sleeper()), helper_wake),
                PowerKeys::new(&config(&[]), Box::new(vtium::power::System), None),
            ),
        };
        let (link, theirs) = Link::pair().unwrap();
        let drawer = Link::passed(theirs).unwrap();
        let mut out = Outbox::new(&link);

        let start = Request::StartHelper {
            helper: 5,
            kind: Helper::Font,
        };
        jobs.serve(start, &mut out).unwrap();
        let (order, socket) = next(&drawer);
        assert_eq!(
            order,
            Order::Helper {
                helper: 5,
                started: Ok(())
            }
        );
        assert!(socket.is_some(), "the helper's socket did not come");
        // A second helper under the same id is refused.
        jobs.serve(
            Request::StartHelper {
                helper: 5,
                kind: Helper::Font,
            },
            &mut out,
        )
        .unwrap();
        assert_eq!(
            next(&drawer).0,
            Order::Helper {
                helper: 5,
                started: Err(errno(libc::EEXIST))
            }
        );

        let open = Request::OpenSession {
            console: console("left", 2),
            grid: GRID,
            display: Display::Kms,
            truecolor: true,
        };
        jobs.serve(open, &mut out).unwrap();
        let (order, master) = next(&drawer);
        assert!(
            matches!(order, Order::Pty { opened: Ok(()), .. }),
            "{order:?}"
        );
        assert!(master.is_some());

        jobs.serve(Request::StopHelper { helper: 5 }, &mut out)
            .unwrap();
        wait_for_exits(&helpers_watched);
        wait_for_exits(&sessions_watched);
        jobs.tick(&mut out);
        assert_eq!(
            next(&drawer).0,
            Order::SessionExit {
                console: console("left", 2),
                status: Status::Code(3)
            }
        );
        assert_eq!(
            next(&drawer).0,
            Order::HelperExit {
                helper: 5,
                status: Status::Signal(9)
            }
        );
        jobs.tick(&mut out);
        assert!(
            matches!(drawer.next_order().unwrap(), Got::Nothing),
            "an exit came twice"
        );
    }

    #[test]
    fn an_image_helper_needs_an_open_session() {
        let (_, wake) = watching();
        let mut helpers = Helpers::new(None, wake);
        let (_, session_wake) = watching();
        let sessions = Sessions::new(
            conf("sleep 1000", "exit 0", WIDE),
            Box::new(Nowhere),
            session_wake,
        );
        let image = Helper::Image(console("left", 1));
        assert_eq!(
            helpers.start(1, &image, &sessions).err(),
            Some(errno(libc::ENOENT))
        );
        assert_eq!(
            helpers.start(2, &Helper::Font, &sessions).err(),
            Some(errno(libc::ENOEXEC)),
            "a font helper with no program started"
        );
    }

    #[test]
    fn at_most_two_font_helpers_run() {
        let (watched, wake) = watching();
        let mut helpers = Helpers::new(Some(sleeper()), wake);
        let (_, session_wake) = watching();
        let sessions = Sessions::new(
            conf("exit 0", "exit 0", WIDE),
            Box::new(Nowhere),
            session_wake,
        );
        assert!(helpers.start(1, &Helper::Font, &sessions).is_ok());
        assert!(helpers.start(2, &Helper::Font, &sessions).is_ok());
        assert_eq!(
            helpers.start(3, &Helper::Font, &sessions).err(),
            Some(errno(libc::EAGAIN))
        );
        for id in [1, 2] {
            helpers.stop(id);
        }
        wait_for_exits(&watched);
        let mut ended: Vec<u32> = helpers.reap().into_iter().map(|(id, _)| id).collect();
        ended.sort_unstable();
        assert_eq!(ended, [1, 2]);
    }

    #[test]
    fn of_the_blanks_and_wakes_unsent_the_last_holds() {
        let (_, session_wake) = watching();
        let (_, helper_wake) = watching();
        let mut jobs = Jobs {
            processes: Processes::new(
                Sessions::new(
                    conf("exit 0", "exit 0", WIDE),
                    Box::new(Nowhere),
                    session_wake,
                ),
                Helpers::new(None, helper_wake),
                PowerKeys::new(&config(&[]), Box::new(vtium::power::System), None),
            ),
        };
        let (link, theirs) = Link::pair().unwrap();
        let drawer = Link::passed(theirs).unwrap();
        let mut out = Outbox::new(&link);
        let signalled = std::rc::Rc::clone(&jobs.processes.signalled);
        signalled
            .borrow_mut()
            .extend([vtium::blank::Order::Wake, vtium::blank::Order::Blank]);
        jobs.tick(&mut out);
        signalled.borrow_mut().push(vtium::blank::Order::Wake);
        jobs.tick(&mut out);
        assert_eq!(next(&drawer).0, Order::Blank);
        assert_eq!(next(&drawer).0, Order::Wake);
        assert!(matches!(drawer.next_order().unwrap(), Got::Nothing));
        // With the drawer's socket and the queue full, one order waits.
        while link.order(&Order::Back, None).is_ok() {}
        while !out.full() {
            out.order(Order::Back, None).unwrap();
        }
        for _ in 0..100 {
            signalled.borrow_mut().push(vtium::blank::Order::Blank);
            jobs.tick(&mut out);
        }
        assert_eq!(jobs.processes.unasked, [Order::Blank]);
    }
}
