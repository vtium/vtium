//! The font helper's process, and the threads that write to it and read
//! from it.

use std::io::Write;
use std::os::fd::OwnedFd;
use std::process::{Child, ExitStatus, Stdio};
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

/// The helper's process: its pidfd, which ends it, the writer's queue and
/// the reader's events. The reaper thread owns the child.
pub(super) struct Running {
    pidfd: Option<OwnedFd>,
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
        let (stdin, stdout) = (child.stdin.take(), child.stdout.take());
        // the reaper starts first, so the child is reaped whatever fails
        // after
        let (exited, status) = mpsc::channel();
        let (give, given) = mpsc::channel::<Child>();
        if let Err(e) = thread("reaper", move || {
            if let Ok(child) = given.recv() {
                let _ = exited.send(reap(child));
            }
        }) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "vtium cannot start the font helper's reaper thread: {e}"
            ));
        }
        let _ = give.send(child);
        // a thread that cannot start ends the child, which its reaper reaps
        let end = |what: &str, e: std::io::Error| {
            if let Some(fd) = &pidfd {
                let _ = pidfd_send_signal(fd, Signal::KILL);
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
        if let Some(mut stdin) = stdin {
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
            if let Some(mut stdout) = stdout {
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
            let status = status.recv().ok().flatten();
            let _ = tx.send(Event::Closed(status));
            wake();
        })
        .map_err(|e| end("reader", e))?;
        Ok(Running {
            pidfd,
            requests,
            events,
        })
    }

    /// Gives the helper up for the boot: ends it, drops what was asked
    /// and logs `why` once.
    pub(super) fn fail(&mut self, why: &str) {
        self.stop();
        self.lose_pointer(why);
        self.search = None;
        self.suspect = None;
        self.deferred.clear();
        self.asks.clear();
        for c in &mut self.cells {
            if c.fill == Fill::Active {
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
        if let Some(fd) = self.running.take().and_then(|r| r.pidfd) {
            let _ = pidfd_send_signal(&fd, Signal::KILL);
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.stop();
    }
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
