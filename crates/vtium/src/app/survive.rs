//! The seat's side of the sessions that survive a vtium restart (ADR-1401
//! D13): where each console's pty master is kept, and how a vtium that
//! starts again puts each kept session back on the console it ran on.
//!
//! The keeper ([`crate::survive`]) holds a copy of each running session's
//! master under the name of its head and console. At the end of each turn
//! the seat tells it which masters run now, so a session whose program
//! started again is kept under its new pty and one that ended is dropped.
//! A starting vtium is given the masters an earlier vtium left. When a
//! head starts, each console whose name has a kept master takes that pty
//! back in place of starting a new session, and a console the head would
//! not open at its start opens again for it, within the caps. A master
//! whose head is gone from vtium.conf, or for which the caps leave no
//! room, is hung up. A kept master whose session ended while no vtium ran
//! counts as that console's program exiting at the start. At its exit,
//! vtium lets go of every session the keeper holds without hanging it up.

use std::os::fd::{BorrowedFd, OwnedFd};
use std::rc::Rc;
use std::time::Duration;

use vtium_platform::pty::Pty;

use super::App;
use crate::head::session::AfterExit;
use crate::respawn::Respawn;
use crate::survive::{Keeper, Kept, Ledger, Name, describe};

/// What the seat keeps through a restart.
#[derive(Default)]
pub(crate) struct Survival {
    keeper: Keeper,
    /// The masters an earlier vtium left, until their consoles start.
    kept: Kept,
    ledger: Ledger,
    /// vtium let go of the sessions the keeper holds, as it exits, so it
    /// tells the keeper nothing more.
    released: bool,
    /// The id the next pty the seat opens or takes back gets.
    next_pty_id: u64,
    /// The names under which the keeper held a second master, which vtium
    /// hung up: the keeper drops every master of such a name when the
    /// first is taken back, and is given it again at the turn's end.
    rekeep: Vec<String>,
}

/// What became of a console's kept master at its start.
enum Taken {
    /// Its session runs on in the console.
    Adopted,
    /// Its session ended while no vtium ran.
    Ended,
    /// No master was kept for the console.
    None,
}

impl App {
    /// Keeps each session's master with `keeper` from now on, and gives the
    /// seat `kept`, the masters an earlier vtium left with each name it
    /// kept them under. Each goes back to its console as its head starts;
    /// one whose head is not among the seat's heads is hung up at once.
    pub fn set_survival(&mut self, keeper: Keeper, kept: Vec<(String, OwnedFd)>) {
        let mut kept = Kept::new(kept);
        let heads: Vec<String> = self
            .heads
            .iter()
            .filter(|head| !head.notice_only)
            .map(|head| head.name().to_owned())
            .collect();
        let apart = kept.without_place(|head| heads.iter().any(|h| h == head));
        self.survival = Survival {
            keeper,
            kept,
            next_pty_id: self.survival.next_pty_id,
            ..Survival::default()
        };
        for (name, master) in apart {
            match Name::parse(&name) {
                // A second master under a name whose first has a place:
                // dropping the name would drop the first too.
                Some(n) if heads.contains(&n.head) => {
                    self.survival.rekeep.push(name.clone());
                    self.hang_up_kept(
                        &name,
                        master,
                        "another session was kept for the same console",
                        false,
                    );
                }
                Some(_) => self.hang_up_kept(&name, master, "its head is not in vtium.conf", true),
                None => self.hang_up_kept(&name, master, "", true),
            }
        }
    }

    /// The id of a pty the seat opens or takes back now, which no other
    /// pty of this run has.
    pub(super) fn next_pty_id(&mut self) -> u64 {
        let id = self.survival.next_pty_id;
        self.survival.next_pty_id = id.saturating_add(1);
        id
    }

    /// Whether the sessions outlive this vtium.
    pub fn keeps_sessions(&self) -> bool {
        self.survival.keeper.keeps()
    }

    /// Hangs up the kept session `name` for the reason `why`, which is
    /// logged while its session still runs: with `forget`, the keeper
    /// drops its copy, and the session's leader gets SIGHUP before the
    /// master closes, as a console that closes does.
    fn hang_up_kept(&mut self, name: &str, master: OwnedFd, why: &str, forget: bool) {
        if forget {
            let Survival { keeper, ledger, .. } = &mut self.survival;
            ledger.forget(keeper, name);
        }
        let Ok(pty) = Pty::adopt(master) else {
            return;
        };
        if Name::parse(name).is_some() {
            log::warn!(
                "{}: vtium hangs up its session, kept from the last vtium, because {why}",
                describe(name)
            );
        } else {
            log::warn!(
                "vtium hangs up a session kept from the last vtium under a name it does not know"
            );
        }
        drop(pty);
    }

    /// Opens on head `h` exactly the consoles it has kept masters for, the
    /// lowest shown, within the caps (ADR-1401 D10), and hangs up each
    /// kept session the caps leave no room for. Returns whether the head
    /// had kept masters, in which case it opens no other console at its
    /// start.
    pub(super) fn open_kept(&mut self, h: usize) -> bool {
        let Some(name) = self.heads.get(h).map(|head| head.name().to_owned()) else {
            return false;
        };
        let ids = self.survival.kept.consoles(&name);
        if ids.is_empty() {
            return false;
        }
        // The consoles the other heads have, or will open at their start.
        let others: usize = self
            .heads
            .iter()
            .enumerate()
            .filter(|&(i, head)| i != h && !head.notice_only)
            .map(|(_, head)| {
                if head.started() {
                    head.console_count()
                } else {
                    head.wanted
                }
            })
            .fold(0, usize::saturating_add);
        let room = self.caps.head.min(self.caps.total.saturating_sub(others));
        let App {
            heads,
            config,
            scrollback,
            scrollback_bytes,
            clock,
            ..
        } = self;
        let Some(head) = heads.get_mut(h) else {
            return false;
        };
        let left = head.open_kept(&ids, room, || {
            crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock))
        });
        for id in left {
            if let Some(master) = self.survival.kept.take(&name, id) {
                let key = Name::new(&name, id).encode();
                self.hang_up_kept(&key, master, "the console caps leave no room for it", true);
            }
        }
        true
    }

    /// Starts the session of head `h`'s console `id` at `since`: the one
    /// kept for it, when an earlier vtium left one that still runs, and
    /// otherwise a new one, as [`App::spawn`] starts it.
    pub(super) fn start_console(&mut self, h: usize, id: u64, since: Duration) {
        match self.adopt_kept(h, id, since) {
            Taken::Adopted => {}
            Taken::Ended => {
                let oneshot = self.oneshot;
                let after = self
                    .heads
                    .get_mut(h)
                    .and_then(|head| head.ended_away(id, since, oneshot));
                if matches!(after, Some(AfterExit::Restart)) {
                    self.spawn(h, id);
                }
                if let Some(exit) = crate::head::session::exit_due(&self.heads) {
                    self.request_exit(exit);
                }
            }
            Taken::None => self.spawn(h, id),
        }
    }

    /// Puts the master kept for head `h`'s console `id` back in the
    /// console: the pty is adopted, set to the console's size with a
    /// redraw, and watched by the loop.
    fn adopt_kept(&mut self, h: usize, id: u64, since: Duration) -> Taken {
        let Some(head_name) = self.heads.get(h).map(|head| head.name().to_owned()) else {
            return Taken::None;
        };
        let Some(master) = self.survival.kept.take(&head_name, id) else {
            return Taken::None;
        };
        let name = Name::new(&head_name, id).encode();
        let rekeep = self.survival.rekeep.contains(&name);
        if rekeep {
            self.survival.rekeep.retain(|n| *n != name);
        }
        let mut pty = match Pty::adopt(master) {
            Ok(pty) => pty,
            Err(_) => {
                log::info!(
                    "head {head_name}: console {id}: its session ended while vtium was away"
                );
                let Survival { keeper, ledger, .. } = &mut self.survival;
                ledger.forget(keeper, &name);
                return Taken::Ended;
            }
        };
        if let Some(size) = self.heads.get(h).and_then(|head| head.console_pty_size(id))
            && let Err(e) = pty.redraw(size)
        {
            log::warn!("head {head_name}: console {id}: cannot size the recovered session: {e}");
        }
        if let Err(e) = self.watch(h, id, &mut pty) {
            log::warn!("head {head_name}: console {id}: cannot watch the recovered session: {e}");
            let Survival { keeper, ledger, .. } = &mut self.survival;
            ledger.forget(keeper, &name);
            return Taken::None;
        }
        let pty_id = self.next_pty_id();
        let Survival { keeper, ledger, .. } = &mut self.survival;
        if rekeep {
            // Drops the second master too; the turn's end keeps this one
            // again.
            ledger.forget(keeper, &name);
        } else {
            ledger.held(name, pty_id);
        }
        let Some(console) = self.heads.get_mut(h).and_then(|head| head.console_mut(id)) else {
            return Taken::None;
        };
        console.pty = Some(pty);
        console.pty_id = Some(pty_id);
        console.respawn = Some(Respawn::recovered(since));
        log::info!(
            "head {head_name}: console {id} takes back the session that outlived the last vtium"
        );
        Taken::Adopted
    }

    /// Tells the keeper which masters run now: each console's with a
    /// session, under its head's name and its id.
    pub(super) fn sync_kept(&mut self) {
        let App {
            heads, survival, ..
        } = self;
        if !survival.keeper.keeps() || survival.released {
            return;
        }
        let live: Vec<(String, u64, BorrowedFd<'_>)> = heads
            .iter()
            .filter(|head| !head.notice_only)
            .flat_map(|head| {
                head.consoles().filter_map(|c| {
                    let pty = c.pty.as_ref()?;
                    Some((
                        Name::new(head.name(), c.id).encode(),
                        c.pty_id?,
                        pty.master(),
                    ))
                })
            })
            .collect();
        survival.ledger.sync(&mut survival.keeper, live);
    }

    /// Lets go of every session the keeper holds, as vtium exits, without
    /// hanging it up, so that the next vtium takes it back, and tells the
    /// keeper nothing from then on. A session the keeper does not hold
    /// stays in its console, and dropping the seat hangs it up.
    pub fn release_sessions(&mut self) {
        if !self.survival.keeper.keeps() || self.survival.released {
            return;
        }
        self.sync_kept();
        self.survival.released = true;
        let App {
            heads, survival, ..
        } = self;
        for head in heads.iter_mut() {
            let name = head.name().to_owned();
            for console in head.consoles_mut() {
                if survival
                    .ledger
                    .holds(&Name::new(&name, console.id).encode())
                    && let Some(pty) = console.pty.take()
                {
                    pty.release();
                }
            }
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test's set-up fails loudly")]
#[expect(clippy::indexing_slicing, reason = "a test indexes the heads it built")]
#[expect(
    clippy::disallowed_methods,
    reason = "the loop's clock in a test, as the seat's tests read it"
)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::Instant;

    use rustix::event::{PollFd, PollFlags, poll};
    use rustix::process::{Pid, PidfdFlags, WaitOptions, pidfd_open, waitpid};
    use vtium_platform::event_loop::Loop;
    use vtium_platform::pty::{Erase, Size};
    use vtium_platform::vt::FakeVt;

    use super::*;
    use crate::app::tests::{Still, no_fonts, shown};
    use crate::survive::hold;

    /// A session an earlier vtium started and let go of at its exit: its
    /// leader's process id and a copy of its master, which the keeper
    /// held. It starts at 10x5, a size no seat here draws.
    fn left_running(script: &str) -> (u32, OwnedFd) {
        let mut sh = Command::new("/bin/sh");
        sh.args(["-c", script]);
        let size = Size {
            cols: 10,
            rows: 5,
            cell_width: 8,
            cell_height: 16,
        };
        let pty = Pty::spawn(sh, size, Erase::Delete).unwrap();
        let pid = pty.child_id().unwrap();
        let master = pty.master().try_clone_to_owned().unwrap();
        pty.release();
        (pid, master)
    }

    fn pid(raw: u32) -> Pid {
        Pid::from_raw(i32::try_from(raw).unwrap()).unwrap()
    }

    /// Whether the process `raw`, a child of the test, exits within 10 s;
    /// it is reaped when it does.
    fn exits(raw: u32) -> bool {
        let pidfd = pidfd_open(pid(raw), PidfdFlags::empty()).unwrap();
        let mut fds = [PollFd::new(&pidfd, PollFlags::IN)];
        let timeout = rustix::time::Timespec {
            tv_sec: 10,
            tv_nsec: 0,
        };
        let done = poll(&mut fds, Some(&timeout)).unwrap() == 1;
        if done {
            let _ = waitpid(Some(pid(raw)), WaitOptions::empty());
        }
        done
    }

    /// Whether the process `raw` still runs.
    fn runs(raw: u32) -> bool {
        let Ok(pidfd) = pidfd_open(pid(raw), PidfdFlags::empty()) else {
            return false;
        };
        let mut fds = [PollFd::new(&pidfd, PollFlags::IN)];
        poll(&mut fds, Some(&rustix::time::Timespec::default())).unwrap() == 0
    }

    /// Ends the session `raw` leads, and reaps it.
    fn end(raw: u32) {
        let _ = rustix::process::kill_process_group(pid(raw), rustix::process::Signal::KILL);
        let _ = waitpid(Some(pid(raw)), WaitOptions::empty());
    }

    /// A seat whose new sessions run `script`, with one display, set up
    /// with `set` before its start, shown, after its first turn.
    fn seat(options: &[&str], script: &str, set: impl FnOnce(&mut App)) -> (Loop<App>, App) {
        let env = vec![("PATH".into(), "/usr/bin:/bin".into())];
        seat_with_env(options, script, env, set)
    }

    /// [`seat`], vtium's environment `env`.
    fn seat_with_env(
        options: &[&str],
        script: &str,
        env: Vec<(OsString, OsString)>,
        set: impl FnOnce(&mut App),
    ) -> (Loop<App>, App) {
        let args: Vec<OsString> = ["--font-size=16"]
            .iter()
            .chain(options)
            .chain(&["--login", "--", "/bin/sh", "-c", script])
            .map(OsString::from)
            .collect();
        let loaded = crate::config::load(&args, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().unwrap();
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            Rc::new(Instant::now),
            env,
        );
        app.set_font_sources(no_fonts());
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A")])),
        );
        set(&mut app);
        app.start();
        shown(&mut app);
        (lp, app)
    }

    /// Turns the loop until `done`, at most 500 turns of up to 10 ms.
    fn until(lp: &mut Loop<App>, app: &mut App, done: impl Fn(&App) -> bool) -> bool {
        for _ in 0..500 {
            if done(app) {
                return true;
            }
            lp.dispatch(Some(Duration::from_millis(10)), app).unwrap();
            app.turn(Instant::now());
        }
        done(app)
    }

    fn shows(app: &App, text: &str) -> bool {
        app.screen_text().iter().any(|row| row.contains(text))
    }

    const FRESH: &str = "echo fresh; while read a; do echo new:$a; done";

    // ADR-1401 D13: the session an earlier vtium left goes back to its
    // console, set to the console's size, and answers as it did; no new
    // session starts there.
    #[test]
    fn a_kept_session_goes_back_to_its_console_at_its_size() {
        let (leader, master) = left_running("while read a; do echo got:$a; done");
        let held = master.try_clone().unwrap();
        let (mut lp, mut app) = seat(&[], FRESH, |app| {
            app.set_survival(Keeper::Off, vec![("pty.1.console".into(), master)]);
        });
        let pty = app.heads()[0].pty().unwrap();
        assert!(pty.adopted(), "the console took the kept pty back");
        assert_eq!(pty.child_id(), Some(leader));
        let size = app.heads()[0].console_pty_size(1).unwrap();
        let now = rustix::termios::tcgetwinsize(&held).unwrap();
        assert_eq!(
            (now.ws_col, now.ws_row),
            (size.cols, size.rows),
            "the kept pty has the console's size"
        );
        if let Some(pty) = app.heads.get_mut(0).and_then(|h| h.console.pty.as_mut()) {
            pty.write(b"on\r");
        }
        assert!(until(&mut lp, &mut app, |a| shows(a, "got:on")));
        assert!(!shows(&app, "fresh"), "no new session started");
        drop(app);
        end(leader);
    }

    // A kept session whose program ended while no vtium ran counts as an
    // exit at the start: under respawn its console starts a new session,
    // and under oneshot the console ends.
    #[test]
    fn a_kept_session_that_ended_meanwhile_counts_as_an_exit() {
        for (options, fresh) in [(&[][..], true), (&["--oneshot"][..], false)] {
            let (leader, master) = left_running("exit 0");
            assert!(exits(leader), "the earlier session ends");
            let (mut lp, mut app) = seat(options, FRESH, |app| {
                app.set_survival(Keeper::Off, vec![("pty.1.console".into(), master)]);
            });
            if fresh {
                assert!(
                    until(&mut lp, &mut app, |a| shows(a, "fresh")),
                    "a new session"
                );
                assert!(!app.heads()[0].pty().unwrap().adopted());
            } else {
                assert!(app.heads()[0].pty().is_none(), "the console ended");
                assert!(app.heads()[0].ended());
            }
        }
    }

    // A kept session whose head is gone from vtium.conf has no console,
    // so vtium hangs it up.
    #[test]
    fn a_kept_session_with_no_head_is_hung_up() {
        let (leader, master) = left_running("while read a; do :; done");
        let (_lp, _app) = seat(&[], FRESH, |app| {
            app.set_survival(Keeper::Off, vec![("pty.1.gone".into(), master)]);
        });
        assert!(exits(leader), "the session got SIGHUP");
    }

    // At vtium's exit, a session its keeper holds runs on, and the keeper
    // still holds its master for the next vtium.
    #[test]
    fn a_held_session_outlives_the_vtium_that_started_it() {
        let dir = std::env::temp_dir().join(format!("vtium-app-survive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = hold::socket_path(&dir);
        let listener = rustix::net::socket_with(
            rustix::net::AddressFamily::UNIX,
            rustix::net::SocketType::SEQPACKET,
            rustix::net::SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        rustix::net::bind(&listener, &rustix::net::SocketAddrUnix::new(&path).unwrap()).unwrap();
        rustix::net::listen(&listener, 4).unwrap();
        let holder = std::thread::spawn(move || hold::serve(listener));

        let client = hold::Client::connect(&path).unwrap();
        let (mut lp, mut app) = seat(&[], FRESH, |app| {
            app.set_survival(Keeper::Hold(client), Vec::new());
        });
        assert!(until(&mut lp, &mut app, |a| shows(a, "fresh")));
        let leader = app.heads()[0].pty().and_then(Pty::child_id).unwrap();
        // vtium ends as SIGTERM ends it, through its turns, then lets go of
        // the seat as main does.
        app.request_exit(crate::app::Exit::Signal);
        assert!(until(&mut lp, &mut app, |a| a.finished().is_some()));
        app.release_sessions();
        drop(app);
        drop(lp);
        assert!(runs(leader), "the session runs on after vtium's exit");

        let next = hold::Client::connect(&path).unwrap();
        let mut kept = Kept::new(next.take().unwrap());
        assert!(
            kept.take("console", 1).is_some(),
            "the holder holds console 1"
        );
        next.forget(&Name::new("console", 1).encode()).unwrap();
        drop(next);
        assert!(holder.join().unwrap().is_ok());
        end(leader);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A holder serving on a socket in a directory of its own, and that
    /// directory.
    fn holder(name: &str) -> (PathBuf, std::thread::JoinHandle<std::io::Result<()>>) {
        let dir = std::env::temp_dir().join(format!("vtium-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = hold::socket_path(&dir);
        let listener = rustix::net::socket_with(
            rustix::net::AddressFamily::UNIX,
            rustix::net::SocketType::SEQPACKET,
            rustix::net::SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        rustix::net::bind(&listener, &rustix::net::SocketAddrUnix::new(&path).unwrap()).unwrap();
        rustix::net::listen(&listener, 4).unwrap();
        (path, std::thread::spawn(move || hold::serve(listener)))
    }

    // The maintainer's answer of 2026-11-06 (ADR-1401 D13): a head with
    // kept sessions opens exactly those consoles and no fresh one, and
    // shows the lowest. Consoles 2 to 5 under head-consoles-max 4 all come
    // back, and nothing is hung up.
    #[test]
    fn a_head_with_kept_sessions_opens_exactly_those_consoles() {
        let kept: Vec<(u64, u32, OwnedFd)> = (2..=5)
            .map(|id| {
                let (leader, master) = left_running("while read a; do :; done");
                (id, leader, master)
            })
            .collect();
        let leaders: Vec<(u64, u32)> = kept.iter().map(|&(id, leader, _)| (id, leader)).collect();
        let named = kept
            .into_iter()
            .map(|(id, _, master)| (Name::new("console", id).encode(), master))
            .collect();
        let (_lp, app) = seat(&["--head-consoles-max=4"], FRESH, |app| {
            app.set_survival(Keeper::Off, named);
        });
        let head = &app.heads()[0];
        let ids: Vec<u64> = head.consoles().map(|c| c.id).collect();
        assert_eq!(ids, [2, 3, 4, 5], "the kept consoles, the lowest shown");
        for (id, leader) in &leaders {
            let pty = head
                .consoles()
                .find(|c| c.id == *id)
                .and_then(|c| c.pty.as_ref());
            assert_eq!(pty.and_then(Pty::child_id), Some(*leader), "console {id}");
            assert!(runs(*leader), "console {id}'s session runs");
        }
        drop(app);
        for (_, leader) in leaders {
            end(leader);
        }
    }

    // Two masters kept under one console's name: the second is hung up,
    // and the keeper ends up holding the first, which the console took
    // back, and only it.
    #[test]
    fn a_second_master_for_one_console_is_hung_up_and_the_first_kept() {
        let (first, master) = left_running("while read a; do :; done");
        let (second, again) = left_running("while read a; do :; done");
        let held = master.try_clone().unwrap();
        let (path, holder) = holder("duplicate");
        let client = hold::Client::connect(&path).unwrap();
        let name = Name::new("console", 1).encode();
        let (mut lp, mut app) = seat(&[], FRESH, |app| {
            app.set_survival(
                Keeper::Hold(client),
                vec![(name.clone(), master), (name.clone(), again)],
            );
        });
        assert!(exits(second), "the second session is hung up");
        assert_eq!(app.heads()[0].pty().and_then(Pty::child_id), Some(first));
        app.request_exit(crate::app::Exit::Signal);
        assert!(until(&mut lp, &mut app, |a| a.finished().is_some()));
        app.release_sessions();
        drop(app);
        drop(lp);
        let next = hold::Client::connect(&path).unwrap();
        let kept = next.take().unwrap();
        let pts = |fd| rustix::pty::ptsname(fd, Vec::new()).unwrap();
        let names: Vec<&str> = kept.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, [name.as_str()], "one master kept");
        assert_eq!(pts(&kept[0].1), pts(&held), "the first session's");
        next.forget(&name).unwrap();
        drop(next);
        assert!(holder.join().unwrap().is_ok());
        end(first);
    }

    // No session inherits what systemd says to vtium alone, through the
    // seat as main builds it, with reset-env off.
    #[test]
    fn no_session_inherits_systemds_variables() {
        let env = [
            ("PATH", "/usr/bin:/bin"),
            ("HOME", "/home-of-the-test"),
            ("NOTIFY_SOCKET", "/run/systemd/notify"),
            ("INVOCATION_ID", "1234"),
            ("MEMORY_PRESSURE_WATCH", "/sys/fs/cgroup/memory.pressure"),
        ]
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect();
        let script = "echo \"N=${NOTIFY_SOCKET:-none} I=${INVOCATION_ID:-none} M=${MEMORY_PRESSURE_WATCH:-none} H=$HOME\"; read a";
        let (mut lp, mut app) = seat_with_env(&["--no-reset-env"], script, env, |_| {});
        assert!(until(&mut lp, &mut app, |a| shows(
            a,
            "H=/home-of-the-test"
        )));
        assert!(shows(&app, "N=none I=none M=none H=/home-of-the-test"));
    }
}
