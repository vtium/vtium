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
//! whether it reads a file, and how many bytes the request needs, which
//! the request to start it carries. The root process chooses the account
//! by the same rules as vtium in one process, from the owner of the
//! console's pty and the session's logind session, and starts the helper
//! from the program it opened once at its start.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the broker starts and stops the
//!   helpers, chooses each one's user, and passes each helper's socket.
//! - ADR-1400 D2: the helpers and their sandbox.
//! - ADR-1200 D15: the font helper runs as `nobody`.
//! - ADR-1101 D2: the image helper runs as the session's user.

use std::collections::BTreeMap;
use std::os::fd::{AsFd, OwnedFd};
use std::process::Stdio;

use rustix::net::{AddressFamily, SocketFlags, SocketType};
use rustix::process::{Pid, PidfdFlags, Signal, WaitId, WaitIdOptions};
use vtium::helper::Launcher;

use super::protocol::{Errno, Helper, Status};
use super::sessions::{Sessions, Wake, errno, status_of};

/// The most font helpers that run at once: the one the drawing process
/// uses, and one it starts while the last one goes.
pub const FONT_HELPERS_MAX: usize = 2;

/// The most image helpers that run at once for one console: the one
/// that serves it, and one it starts while the last one goes.
pub const IMAGE_HELPERS_MAX: usize = 2;

/// The most image helpers that run at once for every console together:
/// two for each of the eight consoles vtium.conf allows by default. Each
/// helper holds descriptors in the root process, so the cap keeps a
/// drawing process that has been taken over from using up the root
/// process's descriptors, whatever consoles it opens and closes.
pub const IMAGE_HELPERS_TOTAL_MAX: usize = 16;

/// A helper the root process started. The helper leads its own process
/// group, so its process id names the group too.
struct Running {
    pid: Pid,
    pidfd: OwnedFd,
    /// The console an image helper serves; `None` for a font helper.
    console: Option<super::protocol::ConsoleRef>,
}

/// What the root process starts image helpers from: the program it opened
/// once, the helpers' `TMPDIR`, the pre-login account it looked up, and
/// the image budget it read from vtium.conf.
#[cfg(feature = "images")]
pub struct Images {
    pub program: vtium_platform::image_helper::Program,
    pub tmpdir: Option<std::path::PathBuf>,
    prelogin: vtium_platform::image_helper::Prelogin,
    /// The most bytes one console's pictures may take, from vtium.conf's
    /// `image-storage-limit`. For `auto`, which depends on displays the
    /// root process does not know, it is the most that `auto` gives.
    pub budget: u64,
}

#[cfg(feature = "images")]
impl Images {
    /// The image helper `config` names, opened and checked, or why the
    /// root process starts none.
    pub fn configured(
        config: &vtium::config::Config,
        env: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> Result<Images, String> {
        let program =
            vtium_platform::image_helper::Program::open(&vtium::head::images::program(config))?
                .sandboxed(vtium::helper::sandbox_on(config));
        Ok(Images {
            program,
            tmpdir: vtium::head::images::tmpdir(env),
            prelogin: vtium_platform::image_helper::Prelogin::default(),
            budget: config
                .number(&vtium::config::keys::IMAGE_STORAGE_LIMIT)
                .unwrap_or(vtium_image::AUTO_CAP),
        })
    }

    /// The memory cap's bytes and the largest file that a helper starts
    /// with for a request that asks for `max_bytes` and `largest_file`:
    /// never more than the budget, and for the file, the budget and the
    /// frame allowance that animations take past it. The drawing process
    /// names the numbers, but a drawing process that has been taken over
    /// could name anything up to 4 GiB, so the root process holds them to
    /// its own reading of vtium.conf.
    ///
    /// References:
    /// - ADR-1102 D1: the frame allowance, a fixed multiple of the budget.
    fn caps(&self, max_bytes: u64, largest_file: u64) -> (u64, u64) {
        let frames = self
            .budget
            .saturating_mul(vtium_image::FRAME_ALLOWANCE_FACTOR.saturating_add(1));
        (max_bytes.min(self.budget), largest_file.min(frames))
    }
}

/// Without pictures, the root process starts no image helper.
#[cfg(not(feature = "images"))]
pub struct Images;

/// The helpers the root process runs, by the id the drawing process gave
/// each.
pub struct Helpers {
    /// The font helper's program, opened once, and the account it runs
    /// as; `None` when vtium.conf's font helper cannot start.
    font: Option<Launcher>,
    /// What image helpers start from; `None` when vtium.conf's image
    /// helper cannot start, or vtium is built without pictures.
    #[cfg_attr(
        not(feature = "images"),
        expect(dead_code, reason = "vtium without pictures starts no image helper")
    )]
    images: Option<Images>,
    wake: Wake,
    running: BTreeMap<u32, Running>,
}

impl Helpers {
    pub fn new(font: Option<Launcher>, images: Option<Images>, wake: Wake) -> Helpers {
        Helpers {
            font,
            images,
            wake,
            running: BTreeMap::new(),
        }
    }

    /// Starts helper `id` of kind `kind`, and gives the drawing process's
    /// end of its socket, or why it started none.
    ///
    /// - `EEXIST`: a helper with that id runs already.
    /// - `ENOENT`: an image helper's console has no session open.
    /// - `EACCES`: no account may run the image helper for this request,
    ///   such as a file asked for before anyone has logged in.
    /// - `EAGAIN`: [`FONT_HELPERS_MAX`] font helpers, [`IMAGE_HELPERS_MAX`]
    ///   image helpers of one console, or [`IMAGE_HELPERS_TOTAL_MAX`] image
    ///   helpers in all, run already.
    /// - `ENOEXEC`: vtium.conf's font or image helper cannot start.
    /// - any other error: the helper did not start.
    pub fn start(&mut self, id: u32, kind: &Helper, sessions: &Sessions) -> Result<OwnedFd, Errno> {
        if self.running.contains_key(&id) {
            return Err(errno(libc::EEXIST));
        }
        match kind {
            Helper::Image {
                console,
                max_bytes,
                largest_file,
                file,
            } => {
                let Some(master) = sessions.master(console) else {
                    return Err(errno(libc::ENOENT));
                };
                let images = self.running.values().filter(|r| r.console.is_some());
                let serving = images
                    .clone()
                    .filter(|r| r.console.as_ref() == Some(console))
                    .count();
                if serving >= IMAGE_HELPERS_MAX || images.count() >= IMAGE_HELPERS_TOTAL_MAX {
                    return Err(errno(libc::EAGAIN));
                }
                let child = sessions.child(console);
                let (started, pid, pidfd) =
                    self.start_image(master, child, *max_bytes, *largest_file, *file)?;
                (self.wake)(pidfd.as_fd());
                self.running.insert(
                    id,
                    Running {
                        pid,
                        pidfd,
                        console: Some(console.clone()),
                    },
                );
                Ok(started)
            }
            Helper::Font => self.start_font(id),
        }
    }

    /// Starts an image helper for one request of `session`'s console, as
    /// the account the rules choose, and gives the drawing process's end
    /// of its socket, its process id and its pidfd.
    ///
    /// References:
    /// - ADR-1101 D2: the account, from the pty's owner and the session's
    ///   logind session, and the refusal of a file before login.
    #[cfg(feature = "images")]
    fn start_image(
        &mut self,
        master: std::os::fd::BorrowedFd<'_>,
        child: Option<u32>,
        max_bytes: u64,
        largest_file: u64,
        file: bool,
    ) -> Result<(OwnedFd, Pid, OwnedFd), Errno> {
        use vtium_platform::image_helper::{Session, System, resolve, start};
        let images = self.images.as_mut().ok_or(errno(libc::ENOEXEC))?;
        let (max_bytes, largest_file) = images.caps(max_bytes, largest_file);
        if max_bytes == 0 {
            // `image-storage-limit=0` turns pictures off
            return Err(errno(libc::EACCES));
        }
        let session = Session {
            child,
            pty_owner: vtium_platform::pty::slave_owner(master).map_err(|e| e.to_string()),
        };
        let (account, disagree) = resolve(&System, &mut images.prelogin, &session, file);
        if let Some((owner, logind)) = disagree {
            log::warn!(
                target: "broker",
                "the pty's owner, uid {owner}, is not the user of the session's logind session, uid {logind}; vtium starts the image helper as uid {logind}"
            );
        }
        let account = account.map_err(|why| {
            log::debug!(target: "broker", "vtium's root process starts no image helper: {why:?}");
            errno(libc::EACCES)
        })?;
        let started = start(
            &images.program,
            &account,
            max_bytes,
            largest_file,
            images.tmpdir.as_deref(),
        )
        .map_err(|why| {
            log::warn!(target: "broker", "{why}");
            errno(libc::EIO)
        })?;
        Ok((started.socket, started.pid, started.pidfd))
    }

    #[cfg(not(feature = "images"))]
    fn start_image(
        &mut self,
        _: std::os::fd::BorrowedFd<'_>,
        _: Option<u32>,
        _: u64,
        _: u64,
        _: bool,
    ) -> Result<(OwnedFd, Pid, OwnedFd), Errno> {
        Err(errno(libc::ENOEXEC))
    }

    fn start_font(&mut self, id: u32) -> Result<OwnedFd, Errno> {
        if self
            .running
            .values()
            .filter(|r| r.console.is_none())
            .count()
            >= FONT_HELPERS_MAX
        {
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
        let pid = i32::try_from(child.id()).ok().and_then(Pid::from_raw);
        let pidfd =
            match pid.map(|pid| (pid, rustix::process::pidfd_open(pid, PidfdFlags::empty()))) {
                Some((pid, Ok(pidfd))) => (pid, pidfd),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(errno(libc::EIO));
                }
            };
        let (pid, pidfd) = pidfd;
        // The child is reaped through its pidfd, so its handle goes.
        drop(child);
        (self.wake)(pidfd.as_fd());
        self.running.insert(
            id,
            Running {
                pid,
                pidfd,
                console: None,
            },
        );
        Ok(ours)
    }

    /// Stops helper `id`, when it runs: its whole process group gets
    /// `SIGKILL`. Its exit is reported when the root process reaps it.
    pub fn stop(&mut self, id: u32) {
        if let Some(running) = self.running.get(&id) {
            // The helper leads its own process group; its pidfd still
            // names it, since nothing has reaped it.
            let _ = rustix::process::pidfd_send_signal(&running.pidfd, Signal::KILL);
            let _ = rustix::process::kill_process_group(running.pid, Signal::KILL);
        }
    }

    /// Stops every image helper of `console`, whose session has closed or
    /// ended. A console's helpers run as its session's user, so they go
    /// with the session, as they do when the console's host goes in vtium
    /// in one process.
    pub fn stop_console(&mut self, console: &super::protocol::ConsoleRef) {
        let ids: Vec<u32> = self
            .running
            .iter()
            .filter(|(_, r)| r.console.as_ref() == Some(console))
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            self.stop(id);
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
                Helpers::new(Some(sleeper()), None, helper_wake),
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
        let mut helpers = Helpers::new(None, None, wake);
        let (_, session_wake) = watching();
        let sessions = Sessions::new(
            conf("sleep 1000", "exit 0", WIDE),
            Box::new(Nowhere),
            session_wake,
        );
        let image = Helper::Image {
            console: console("left", 1),
            max_bytes: 4096,
            largest_file: 4096,
            file: false,
        };
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
        let mut helpers = Helpers::new(Some(sleeper()), None, wake);
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
                Helpers::new(None, None, helper_wake),
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

    /// Image helpers that run `/bin/sleep`, so they live until something
    /// stops them, under an image budget of `budget` bytes.
    #[cfg(feature = "images")]
    fn sleeping_images(budget: u64) -> Images {
        let program = vtium_platform::image_helper::Program::open_with(
            "/bin/sleep".as_ref(),
            vec!["1000".into()],
        )
        .unwrap();
        Images {
            program,
            tmpdir: None,
            prelogin: vtium_platform::image_helper::Prelogin::default(),
            budget,
        }
    }

    /// An image helper of `console`, for a request of a MiB.
    #[cfg(feature = "images")]
    fn image_of(console: super::super::protocol::ConsoleRef) -> Helper {
        Helper::Image {
            console,
            max_bytes: 1 << 20,
            largest_file: 1 << 20,
            file: false,
        }
    }

    /// The process jobs with sessions that `top` runs and image helpers
    /// that sleep, and the pidfds of the sessions and of the helpers.
    #[cfg(feature = "images")]
    fn image_jobs(
        top: &str,
    ) -> (
        Jobs,
        super::super::sessions::tests::Watched,
        super::super::sessions::tests::Watched,
    ) {
        let (sessions_watched, session_wake) = watching();
        let (helpers_watched, helper_wake) = watching();
        let jobs = Jobs {
            processes: Processes::new(
                Sessions::new(conf(top, "exit 0", WIDE), Box::new(Nowhere), session_wake),
                Helpers::new(None, Some(sleeping_images(1 << 24)), helper_wake),
                PowerKeys::new(&config(&[]), Box::new(vtium::power::System), None),
            ),
        };
        (jobs, sessions_watched, helpers_watched)
    }

    /// Opens `console`'s session and starts image helpers `ids` for it,
    /// each answered with its socket.
    #[cfg(feature = "images")]
    fn open_with_helpers(
        jobs: &mut Jobs,
        out: &mut Outbox<'_>,
        drawer: &Link,
        console: &super::super::protocol::ConsoleRef,
        ids: &[u32],
    ) -> OwnedFd {
        let open = Request::OpenSession {
            console: console.clone(),
            grid: GRID,
            display: Display::Kms,
            truecolor: true,
        };
        jobs.serve(open, out).unwrap();
        let (order, master) = next(drawer);
        assert!(
            matches!(order, Order::Pty { opened: Ok(()), .. }),
            "{order:?}"
        );
        for &helper in ids {
            let start = Request::StartHelper {
                helper,
                kind: image_of(console.clone()),
            };
            jobs.serve(start, out).unwrap();
            assert_eq!(
                next(drawer).0,
                Order::Helper {
                    helper,
                    started: Ok(())
                }
            );
        }
        master.unwrap()
    }

    /// The ids of the helpers whose exits the drawer reads next, sorted.
    #[cfg(feature = "images")]
    fn helper_exits(drawer: &Link) -> Vec<u32> {
        let mut ids = Vec::new();
        loop {
            match drawer.next_order().unwrap() {
                Got::Message(Ok((Order::HelperExit { helper, .. }, _))) => ids.push(helper),
                Got::Message(Ok((Order::SessionExit { .. }, _))) => {}
                Got::Nothing => break,
                other => panic!("{other:?}"),
            }
        }
        ids.sort_unstable();
        ids
    }

    #[cfg(feature = "images")]
    #[test]
    fn a_closed_consoles_image_helpers_are_stopped() {
        let (mut jobs, _, helpers_watched) = image_jobs("sleep 1000");
        let (link, theirs) = Link::pair().unwrap();
        let drawer = Link::passed(theirs).unwrap();
        let mut out = Outbox::new(&link);
        let left = console("left", 1);
        let master = open_with_helpers(&mut jobs, &mut out, &drawer, &left, &[1, 2]);
        drop(master);
        jobs.serve(Request::CloseSession { console: left }, &mut out)
            .unwrap();
        wait_for_exits(&helpers_watched);
        jobs.tick(&mut out);
        assert_eq!(helper_exits(&drawer), [1, 2]);
        assert!(
            jobs.processes.helpers.running.is_empty(),
            "a closed console's helper still runs"
        );
    }

    #[cfg(feature = "images")]
    #[test]
    fn an_ended_sessions_image_helpers_are_stopped() {
        let (mut jobs, sessions_watched, helpers_watched) = image_jobs("read x; exit 4");
        let (link, theirs) = Link::pair().unwrap();
        let drawer = Link::passed(theirs).unwrap();
        let mut out = Outbox::new(&link);
        let left = console("left", 1);
        let master = open_with_helpers(&mut jobs, &mut out, &drawer, &left, &[1, 2]);
        rustix::io::write(&master, b"go\n").unwrap();
        wait_for_exits(&sessions_watched);
        // the turn that reaps the session stops its helpers
        jobs.tick(&mut out);
        assert_eq!(
            next(&drawer).0,
            Order::SessionExit {
                console: left,
                status: Status::Code(4)
            }
        );
        wait_for_exits(&helpers_watched);
        jobs.tick(&mut out);
        assert_eq!(helper_exits(&drawer), [1, 2]);
        assert!(jobs.processes.helpers.running.is_empty());
    }

    #[cfg(feature = "images")]
    #[test]
    fn image_helpers_are_capped_in_total() {
        let (_, session_wake) = watching();
        let mut sessions = Sessions::new(
            conf("sleep 1000", "exit 0", WIDE),
            Box::new(Nowhere),
            session_wake,
        );
        let (watched, wake) = watching();
        let mut helpers = Helpers::new(None, Some(sleeping_images(1 << 24)), wake);
        let consoles = IMAGE_HELPERS_TOTAL_MAX.div_ceil(IMAGE_HELPERS_MAX) + 1;
        let mut masters = Vec::new();
        for id in 1..=u64::try_from(consoles).unwrap() {
            masters.push(
                sessions
                    .open(&console("left", id), &GRID, Display::Kms, true)
                    .unwrap(),
            );
        }
        let mut started = 0;
        let mut next_id = 1;
        'all: for id in 1..=u64::try_from(consoles).unwrap() {
            for _ in 0..IMAGE_HELPERS_MAX {
                if started == IMAGE_HELPERS_TOTAL_MAX {
                    break 'all;
                }
                helpers
                    .start(next_id, &image_of(console("left", id)), &sessions)
                    .unwrap();
                started += 1;
                next_id += 1;
            }
        }
        // the last console has room of its own, but every console's
        // helpers together are at the cap
        let last = u64::try_from(consoles).unwrap();
        assert_eq!(
            helpers
                .start(next_id, &image_of(console("left", last)), &sessions)
                .err(),
            Some(errno(libc::EAGAIN))
        );
        for id in 1..next_id {
            helpers.stop(id);
        }
        wait_for_exits(&watched);
        helpers.reap();
        for id in 1..=last {
            sessions.close(&console("left", id));
        }
    }

    /// A process's limit named `name` in `/proc/<pid>/limits`, soft.
    #[cfg(feature = "images")]
    fn limit(pid: Pid, name: &str) -> u64 {
        let limits =
            std::fs::read_to_string(format!("/proc/{}/limits", pid.as_raw_nonzero())).unwrap();
        let line = limits.lines().find(|l| l.starts_with(name)).unwrap();
        line[name.len()..]
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    #[cfg(feature = "images")]
    #[test]
    fn a_request_above_the_budget_starts_a_helper_at_the_budget() {
        let (_, session_wake) = watching();
        let mut sessions = Sessions::new(
            conf("sleep 1000", "exit 0", WIDE),
            Box::new(Nowhere),
            session_wake,
        );
        let _master = sessions
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        let budget = 1 << 20;
        let (watched, wake) = watching();
        let mut helpers = Helpers::new(None, Some(sleeping_images(budget)), wake);
        let greedy = Helper::Image {
            console: console("left", 1),
            max_bytes: super::super::protocol::IMAGE_BYTES_MAX,
            largest_file: super::super::protocol::IMAGE_BYTES_MAX,
            file: false,
        };
        helpers.start(1, &greedy, &sessions).unwrap();
        let pid = helpers.running[&1].pid;
        assert_eq!(
            limit(pid, "Max address space"),
            vtium_platform::image_helper::memory_cap(budget),
            "the helper's memory cap follows the drawer's number"
        );
        assert_eq!(
            limit(pid, "Max file size"),
            budget * (1 + vtium_image::FRAME_ALLOWANCE_FACTOR),
            "the helper's largest file follows the drawer's number"
        );
        helpers.stop(1);
        wait_for_exits(&watched);
        helpers.reap();
        sessions.close(&console("left", 1));
    }

    /// An image helper starts as the account the rules choose: for a test
    /// that runs without root, the test's own user, who owns the
    /// session's pty. A third helper for one console waits.
    #[cfg(feature = "images")]
    #[test]
    fn an_image_helper_starts_as_the_sessions_user() {
        let (_, session_wake) = watching();
        let mut sessions = Sessions::new(
            conf("sleep 1000", "exit 0", WIDE),
            Box::new(Nowhere),
            session_wake,
        );
        let _master = sessions
            .open(&console("left", 1), &GRID, Display::Kms, true)
            .unwrap();
        let program = vtium_platform::image_helper::Program::open_with(
            "/bin/sleep".as_ref(),
            vec!["1000".into()],
        )
        .unwrap();
        let images = Images {
            program,
            tmpdir: None,
            prelogin: vtium_platform::image_helper::Prelogin::default(),
            budget: 1 << 24,
        };
        let (watched, wake) = watching();
        let mut helpers = Helpers::new(None, Some(images), wake);
        let image = |id: u64| Helper::Image {
            console: console("left", id),
            max_bytes: 1 << 20,
            largest_file: 1 << 20,
            file: false,
        };
        assert_eq!(
            helpers.start(1, &image(2), &sessions).err(),
            Some(errno(libc::ENOENT))
        );
        let socket = helpers.start(1, &image(1), &sessions).unwrap();
        assert_eq!(
            rustix::net::sockopt::socket_type(&socket).unwrap(),
            SocketType::SEQPACKET
        );
        let pid = helpers.running[&1].pid;
        let status =
            std::fs::read_to_string(format!("/proc/{}/status", pid.as_raw_nonzero())).unwrap();
        let me = rustix::process::getuid().as_raw();
        let uids: Vec<u32> = status
            .lines()
            .find_map(|l| l.strip_prefix("Uid:"))
            .unwrap()
            .split_whitespace()
            .map(|u| u.parse().unwrap())
            .collect();
        assert_eq!(uids, [me; 4], "the helper runs as another account");
        assert!(helpers.start(2, &image(1), &sessions).is_ok());
        assert_eq!(
            helpers.start(3, &image(1), &sessions).err(),
            Some(errno(libc::EAGAIN))
        );
        for id in [1, 2] {
            helpers.stop(id);
        }
        wait_for_exits(&watched);
        let mut ended = helpers.reap();
        ended.sort_by_key(|(id, _)| *id);
        assert_eq!(ended, [(1, Status::Signal(9)), (2, Status::Signal(9))]);
        sessions.close(&console("left", 1));
    }
}
