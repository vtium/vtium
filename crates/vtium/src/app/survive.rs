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
use crate::head::console::Console;
use crate::head::session::AfterExit;
use crate::respawn::Respawn;
use crate::survive::{Keeper, Kept, Ledger, Name};

/// What the seat keeps through a restart.
#[derive(Default)]
pub(crate) struct Survival {
    keeper: Keeper,
    /// The masters an earlier vtium left, until their consoles start.
    kept: Kept,
    ledger: Ledger,
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
            ledger: Ledger::default(),
        };
        for (name, master) in apart {
            self.hang_up_kept(&name, master, "it names no head of this vtium.conf");
        }
    }

    /// Whether the sessions outlive this vtium.
    pub fn keeps_sessions(&self) -> bool {
        self.survival.keeper.keeps()
    }

    /// Hangs up the kept session `name` for the reason `why`, which is
    /// logged: the keeper drops its copy, and the session's leader gets
    /// SIGHUP before the master closes, as a console that closes does.
    fn hang_up_kept(&mut self, name: &str, master: OwnedFd, why: &str) {
        log::warn!("vtium hangs up the session it kept as {name}, because {why}");
        if let Err(e) = self.survival.keeper.forget(name) {
            log::warn!("cannot drop the kept copy of {name}: {e}");
        }
        drop(Pty::adopt(master));
    }

    /// Opens the consoles of head `h` that have a kept master and that the
    /// head does not open at its start, within the caps (ADR-1401 D10),
    /// and hangs up each kept session the caps leave no room for.
    pub(super) fn open_kept(&mut self, h: usize) {
        let Some(name) = self.heads.get(h).map(|head| head.name().to_owned()) else {
            return;
        };
        let ids = self.survival.kept.consoles(&name);
        if ids.is_empty() {
            return;
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
            return;
        };
        let left = head.open_kept(&ids, room, || {
            crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock))
        });
        for id in left {
            if let Some(master) = self.survival.kept.take(&name, id) {
                let key = Name::new(&name, id).encode();
                self.hang_up_kept(&key, master, "the console caps leave no room for it");
            }
        }
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
        let mut pty = match Pty::adopt(master) {
            Ok(pty) => pty,
            Err(e) => {
                log::info!(
                    "head {head_name}: console {id}: its session ended while vtium was away ({e})"
                );
                if let Err(e) = self.survival.keeper.forget(&name) {
                    log::warn!("cannot drop the kept copy of {name}: {e}");
                }
                return Taken::Ended;
            }
        };
        if let Some(size) = self.heads.get(h).and_then(|head| head.console_pty_size(id))
            && let Err(e) = pty.redraw(size)
        {
            log::warn!("head {head_name}: console {id}: cannot size the recovered session: {e}");
        }
        let watched = pty.insert(&self.handle, move |app: &mut App| {
            let Console { pty, term, .. } = app.heads.get_mut(h)?.console_mut(id)?;
            pty.as_mut().map(|p| (p, term))
        });
        if let Err(e) = watched {
            log::warn!("head {head_name}: console {id}: cannot watch the recovered session: {e}");
            if let Err(e) = self.survival.keeper.forget(&name) {
                log::warn!("cannot drop the kept copy of {name}: {e}");
            }
            return Taken::None;
        }
        let number = pty.number().ok();
        if let Some(n) = number {
            self.survival.ledger.held(name, n);
        }
        let Some(console) = self.heads.get_mut(h).and_then(|head| head.console_mut(id)) else {
            return Taken::None;
        };
        console.pty = Some(pty);
        console.number = number;
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
        if !survival.keeper.keeps() {
            return;
        }
        let live: Vec<(String, u32, BorrowedFd<'_>)> = heads
            .iter()
            .filter(|head| !head.notice_only)
            .flat_map(|head| {
                head.consoles().filter_map(|c| {
                    let pty = c.pty.as_ref()?;
                    Some((
                        Name::new(head.name(), c.id).encode(),
                        c.number?,
                        pty.master(),
                    ))
                })
            })
            .collect();
        survival.ledger.sync(&mut survival.keeper, live);
    }

    /// Lets go of every session the keeper holds, as vtium exits, without
    /// hanging it up, so that the next vtium takes it back. A session the
    /// keeper does not hold stays in its console, and dropping the seat
    /// hangs it up.
    pub fn release_sessions(&mut self) {
        if !self.survival.keeper.keeps() {
            return;
        }
        self.sync_kept();
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
            vec![("PATH".into(), "/usr/bin:/bin".into())],
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
}
