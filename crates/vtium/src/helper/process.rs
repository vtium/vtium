//! The font helper's process, and the threads that write to it and read
//! from it.

use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::process::{Child, ExitStatus, Stdio};
#[cfg(feature = "split")]
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Instant;

use rustix::process::{
    Pid, PidfdFlags, Signal, WaitId, WaitIdOptions, kill_process_group, pidfd_open,
    pidfd_send_signal, waitid,
};

use super::cells::Fill;
use super::search::leave_out_request;
use super::wire::Event;
use super::{Helper, after, frame};

/// vtium's root process, which starts and stops the font helper's
/// processes for the drawing process under the privilege split.
#[cfg(feature = "split")]
pub trait Root {
    /// Starts a font helper, and gives this process's end of the stream
    /// socket that is the helper's standard input and output.
    fn start_font(&self) -> std::io::Result<Launched>;
    /// Stops the helper started under `id`.
    fn stop(&self, id: u32);
}

/// A font helper that vtium's root process started.
#[cfg(feature = "split")]
pub struct Launched {
    /// The id the root process knows the helper by.
    pub id: u32,
    /// This process's end of the helper's stream socket.
    pub socket: OwnedFd,
    /// How the helper exited, once the root process has reaped it.
    pub exit: mpsc::Receiver<ExitStatus>,
}

/// The helper's process: what ends it, the writer's queue and the
/// reader's events. When this process started the helper, the reaper
/// thread owns the child and the pidfd ends it; when vtium's root process
/// started it, the root process ends it by its id.
pub(super) struct Running {
    pidfd: Option<OwnedFd>,
    #[cfg(feature = "split")]
    started_by_root: Option<(Rc<dyn Root>, u32)>,
    pub(super) requests: mpsc::Sender<Vec<u8>>,
    pub(super) events: mpsc::Receiver<Event>,
}

impl Helper {
    /// Sends one request's `bytes` as [`Helper::post`] does. During a
    /// search it sends nothing: the ask or fill waits, and the helper
    /// started after the search is sent every waiting ask and fill.
    pub(super) fn submit(&mut self, bytes: Vec<u8>, now: Instant) {
        if self.search.is_none() {
            self.post(bytes, now);
        }
    }

    /// Sends one request's `bytes`, starting the helper first when it is
    /// not running, and sets when its answer is due. A helper started
    /// outside a search is first asked the cell queries that waited for
    /// it. A helper given up takes nothing, so a helper that cannot start
    /// while vtium sends its waiting asks again is started once, not once
    /// for each ask.
    pub(super) fn post(&mut self, bytes: Vec<u8>, now: Instant) {
        if self.failed.is_some() {
            return;
        }
        if self.running.is_none() {
            match self.start() {
                Ok(r) => self.running = Some(r),
                Err(why) => return self.fail(&why),
            }
            if self.search.is_none() {
                self.send_deferred(now);
            }
        }
        if let Some(r) = &self.running {
            // a send fails only once the writer is gone, which the reader
            // reports as the helper closing
            let _ = r.requests.send(bytes);
        }
        if let Some(suspect) = &mut self.suspect {
            suspect.later = true;
        }
        if self.owes() {
            self.due.get_or_insert(after(now, self.answer));
        }
    }

    fn start(&self) -> Result<Running, String> {
        #[cfg(feature = "split")]
        let started = match &self.root {
            Some(root) => self.start_by_root(root)?,
            None => self.start_here()?,
        };
        #[cfg(not(feature = "split"))]
        let started = self.start_here()?;
        let Started {
            input,
            output,
            status,
            pidfd,
            #[cfg(feature = "split")]
            started_by_root,
        } = started;
        // a thread that cannot start ends the child, which its reaper, or
        // the root process, reaps
        let end = |what: &str, e: std::io::Error| {
            if let Some(fd) = &pidfd {
                let _ = pidfd_send_signal(fd, Signal::KILL);
            }
            #[cfg(feature = "split")]
            if let Some((root, id)) = &started_by_root {
                root.stop(*id);
            }
            format!("vtium cannot start the font helper's {what} thread: {e}")
        };
        let (requests, to_write) = mpsc::channel::<Vec<u8>>();
        // the active fonts go first, before any request reads a frit, and
        // then the sources the frit leaves out
        if let Some(active) = &self.launcher.active {
            let _ = requests.send(active.to_vec());
        }
        let leave = self.leave_out();
        if leave != 0 {
            let _ = requests.send(leave_out_request(leave));
        }
        if let Some(mut stdin) = input {
            thread("writer", move || {
                for bytes in to_write {
                    if stdin
                        .write_all(&bytes)
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| end("writer", e))?;
        }
        let (tx, events) = mpsc::channel();
        let wake = Arc::clone(&self.wake);
        thread("reader", move || {
            if let Some(mut stdout) = output {
                while let Some(e) = frame(&mut stdout) {
                    let malformed = matches!(e, Event::Malformed(_));
                    if tx.send(e).is_err() {
                        break;
                    }
                    wake();
                    if malformed {
                        break;
                    }
                }
            }
            // its end is told once it has exited too, so a helper that
            // closes its output a moment before it exits is waited for,
            // never ended
            let status = status.recv().ok();
            let _ = tx.send(Event::Closed(status));
            wake();
        })
        .map_err(|e| end("reader", e))?;
        Ok(Running {
            pidfd,
            #[cfg(feature = "split")]
            started_by_root,
            requests,
            events,
        })
    }

    /// Starts the helper's process here, as the account vtium runs as or
    /// the one the launcher drops to, with a reaper thread that owns it.
    fn start_here(&self) -> Result<Started, String> {
        // a descriptor the console holds, the VT's and the display's among
        // them, would otherwise reach the helper
        vtium_platform::cloexec_above_stdio().map_err(|e| {
            format!("vtium cannot keep its own descriptors from the font helper: {e}")
        })?;
        let mut child = self
            .launcher
            .process()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| self.launcher.failed(&e))?;
        // the child is not reaped before the check, so its pid is its own
        if let Err(why) = self.launcher.dropped(child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(why);
        }
        // a pidfd names this child alone, even once the reaper has reaped it
        let pidfd = i32::try_from(child.id())
            .ok()
            .and_then(Pid::from_raw)
            .and_then(|pid| pidfd_open(pid, PidfdFlags::empty()).ok());
        let input = child.stdin.take().map(|i| File::from(OwnedFd::from(i)));
        let output = child.stdout.take().map(|o| File::from(OwnedFd::from(o)));
        // the reaper starts first, so the child is reaped whatever fails
        // after
        let (exited, status) = mpsc::channel();
        let (give, given) = mpsc::channel::<Child>();
        if let Err(e) = thread("reaper", move || {
            if let Ok(child) = given.recv()
                && let Some(status) = reap(child)
            {
                let _ = exited.send(status);
            }
        }) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "vtium cannot start the font helper's reaper thread: {e}"
            ));
        }
        let _ = give.send(child);
        Ok(Started {
            input,
            output,
            status,
            pidfd,
            #[cfg(feature = "split")]
            started_by_root: None,
        })
    }

    /// Asks vtium's root process to start the helper's process. The root
    /// process chooses the helper's account, checks that the helper
    /// dropped to it, reaps the helper and reports its exit, which
    /// reaches the reader as the helper's status.
    ///
    /// References:
    /// - ADR-1400 D4, "What the broker does": the broker starts the font
    ///   helper and passes the drawer its socket.
    /// - ADR-1200 D15: the font helper runs as `nobody`.
    #[cfg(feature = "split")]
    fn start_by_root(&self, root: &Rc<dyn Root>) -> Result<Started, String> {
        // the helper's input is a second copy of its socket
        let (Launched { id, socket, exit }, input) = root
            .start_font()
            .and_then(|launched| {
                let input = launched.socket.try_clone();
                if input.is_err() {
                    root.stop(launched.id);
                }
                Ok((launched, input?))
            })
            .map_err(|e| format!("vtium's root process cannot start the font helper: {e}"))?;
        Ok(Started {
            input: Some(File::from(input)),
            output: Some(File::from(socket)),
            status: exit,
            pidfd: None,
            started_by_root: Some((Rc::clone(root), id)),
        })
    }

    /// Gives the helper up for the boot: ends it, drops what was asked,
    /// stops each fill under way or held, and logs `why` once. A held fill
    /// stops too, so the console's turns no longer look for it to start.
    pub(super) fn fail(&mut self, why: &str) {
        self.stop();
        self.lose_pointer(why);
        self.search = None;
        self.suspect = None;
        self.deferred.clear();
        self.asks.clear();
        for c in &mut self.cells {
            if matches!(c.fill, Fill::Active | Fill::Held) {
                c.fill = Fill::Stopped;
            }
        }
        self.due = None;
        if self.failed.is_none() {
            if !self.pointers_only {
                log::warn!(
                    "the font helper could not draw the font ({why}), so vtium uses its built-in font, one character at a time with no combined characters"
                );
            }
            self.failed = Some(why.to_owned());
        }
    }

    /// Ends the helper's process, if it runs; its reaper ends what it
    /// started and reaps it, and nothing it sent is taken.
    pub(super) fn stop(&mut self) {
        self.stopped.clear();
        self.late.clear();
        let Some(running) = self.running.take() else {
            return;
        };
        if let Some(fd) = &running.pidfd {
            let _ = pidfd_send_signal(fd, Signal::KILL);
        }
        #[cfg(feature = "split")]
        if let Some((root, id)) = &running.started_by_root {
            root.stop(*id);
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A helper's process as it started: the ends of its standard input and
/// output, where its exit status comes from, and what ends it.
struct Started {
    input: Option<File>,
    output: Option<File>,
    status: mpsc::Receiver<ExitStatus>,
    pidfd: Option<OwnedFd>,
    #[cfg(feature = "split")]
    started_by_root: Option<(Rc<dyn Root>, u32)>,
}

/// Starts one of a helper's threads, `name` its role.
fn thread(name: &str, f: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name(format!("vtium-helper-{name}"))
        .spawn(f)
        .map(drop)
}

/// Waits for the helper `child` to exit, ends each process it started in
/// its process group, and reaps it. It returns how the helper exited. Until it is reaped its
/// pid, and with it the group's id, names no other process, so the
/// group's end reaches only what the helper started.
fn reap(mut child: Child) -> Option<ExitStatus> {
    if let Some(pid) = i32::try_from(child.id()).ok().and_then(Pid::from_raw) {
        loop {
            match waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT,
            ) {
                Err(rustix::io::Errno::INTR) => {}
                Ok(_) => {
                    let _ = kill_process_group(pid, Signal::KILL);
                    break;
                }
                Err(_) => break,
            }
        }
    }
    child.wait().ok()
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use std::sync::Arc;

    use vtium_font::Cell;
    use vtium_font::spacing::SpacedCell;

    use super::super::{Command, FritText, Helper, Launcher, stand_in_shell};

    /// The tests' fixed instant.
    #[expect(clippy::disallowed_methods, reason = "the tests' fixed instant")]
    fn t0() -> std::time::Instant {
        std::time::Instant::now()
    }

    // A helper that vtium gives up holds no fill. The console held this
    // fill while it waited for the frit's own cell, and it stops with the
    // helper as a fill under way does. Otherwise the console's every turn
    // would look for a held fill to start, and the given-up helper would
    // never start it.
    #[test]
    fn a_failed_helper_holds_no_fill() {
        let command = Command {
            program: stand_in_shell().to_path_buf(),
            args: vec!["-c".into(), "exec cat >/dev/null".into()],
            account: None,
        };
        let mut helper = Helper::new(
            Launcher::open(command).unwrap(),
            Arc::new(|| {}),
            t0(),
            FritText {
                name: "t".to_owned(),
                text: "source=builtin:unscii-8\n".to_owned(),
            },
            None,
            crate::deferred::Shared::default(),
            64 << 20,
        );
        let at = SpacedCell::plain(Cell {
            width: 8,
            height: 16,
        });
        helper.hold_fill(at, &['a']);
        assert!(helper.holds_fills(), "the fill is held");
        helper.fail("the helper failed");
        assert!(!helper.holds_fills(), "a failed helper holds no fill");
    }
}
