//! The store of pty masters, which vtium's root process keeps under the
//! privilege split.
//!
//! Each session's program outlives a restart of vtium while a copy of its
//! pty master stays open somewhere outside vtium: in systemd's
//! file-descriptor store, or in vtium's own holder process where systemd
//! has no store. Only the unit's main process may talk to systemd's store,
//! and under the split that process is the root process, so the root
//! process keeps the copies. It starts the keeper before it starts the
//! drawing process, keeps a copy of each master it makes, and tells the
//! keeper to drop a copy when the session ends or its console closes.
//!
//! A master kept by an earlier vtium waits here until the drawing process
//! opens that console's session again. The root process then hands the
//! drawing process the kept master in place of a new login. At the
//! drawing process's start, the root process tells it which consoles have
//! a master waiting, so it opens each of them as its head starts. The
//! root process hangs up at once each master whose head vtium.conf no
//! longer names, and each one kept under a name vtium does not write, as
//! vtium in one process does.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the broker keeps the pty
//!   masters through a restart, as the unit's main process.
//! - ADR-1401 D13: the store, the holder, the names of kept masters, and
//!   that vtium's own exit never hangs up a kept session.

use std::collections::BTreeSet;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use vtium::survive::{Keeper, Kept, Name, describe, storable};
use vtium_platform::pty;

/// Where the root process keeps a copy of each master, and the masters an
/// earlier vtium left there.
pub trait Keep {
    /// Gives the keeper a copy of the master of `head`'s console `console`.
    fn keep(&mut self, head: &str, console: u64, master: BorrowedFd<'_>);
    /// Tells the keeper to drop its copy of that console's master.
    fn forget(&mut self, head: &str, console: u64);
    /// Takes the master an earlier vtium kept for that console, if any.
    fn take(&mut self, head: &str, console: u64) -> Option<OwnedFd>;
    /// Whether the keeper holds a copy of that console's master, so the
    /// session outlives vtium.
    fn holds(&self, head: &str, console: u64) -> bool;
    /// The consoles whose masters an earlier vtium kept, of the heads in
    /// `known`, in order. Each other master the keeper gave back is hung
    /// up: one whose head is not in `known`, and one kept under a name
    /// vtium does not write.
    fn waiting(&mut self, known: &BTreeSet<String>) -> Vec<(String, u64)>;
}

/// Keeps nothing, for the tests that need no keeper.
#[cfg(test)]
pub struct Nowhere;

#[cfg(test)]
impl Keep for Nowhere {
    fn keep(&mut self, _: &str, _: u64, _: BorrowedFd<'_>) {}
    fn forget(&mut self, _: &str, _: u64) {}
    fn take(&mut self, _: &str, _: u64) -> Option<OwnedFd> {
        None
    }
    fn holds(&self, _: &str, _: u64) -> bool {
        false
    }
    fn waiting(&mut self, _: &BTreeSet<String>) -> Vec<(String, u64)> {
        Vec::new()
    }
}

/// The keeper [`vtium::survive::start`] found, and the masters it gave
/// back.
pub struct Store {
    keeper: Keeper,
    kept: Kept,
    /// Set once the keeper has failed, after which nothing more is kept.
    gave_up: bool,
    /// The names under which the keeper gave back a second master, which
    /// the root process hung up. The keeper drops every master of such a
    /// name when the first is taken back, and is given that one again.
    rekeep: Vec<String>,
}

impl Store {
    pub fn new(keeper: Keeper, kept: Vec<(String, OwnedFd)>) -> Store {
        Store {
            keeper,
            kept: Kept::new(kept),
            gave_up: false,
            rekeep: Vec::new(),
        }
    }

    /// Tells the keeper to drop every master kept under `name`.
    fn forget_name(&mut self, name: &str) {
        if self.gave_up || !storable(name) {
            return;
        }
        if let Err(e) = self.keeper.forget(name) {
            self.give_up(&e);
        }
    }

    /// Hangs up `master`, kept under `name` by an earlier vtium, because
    /// of `why`, and with `forget` tells the keeper to drop it first, as
    /// vtium in one process hangs up a kept session it has no place for.
    fn hang_up(&mut self, name: &str, master: OwnedFd, why: &str, forget: bool) {
        if forget {
            self.forget_name(name);
        }
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
        pty::hang_up(master.as_fd(), None);
    }

    /// Gives the keeper up after `e`, as vtium in one process does: one
    /// warning, a request to drop what the keeper holds, and nothing kept
    /// after it.
    fn give_up(&mut self, e: &io::Error) {
        log::warn!(
            target: "broker",
            "vtium's root process can no longer keep the sessions' ptys, so the sessions end if vtium restarts: {e}"
        );
        self.keeper = Keeper::Off;
        self.gave_up = true;
    }
}

impl Keep for Store {
    fn keep(&mut self, head: &str, console: u64, master: BorrowedFd<'_>) {
        let name = Name::new(head, console).encode();
        if self.gave_up || !storable(&name) {
            return;
        }
        if let Err(e) = self.keeper.keep(&name, master) {
            self.give_up(&e);
        }
    }

    fn forget(&mut self, head: &str, console: u64) {
        let name = Name::new(head, console).encode();
        if self.gave_up || !storable(&name) {
            return;
        }
        if let Err(e) = self.keeper.forget(&name) {
            self.give_up(&e);
        }
    }

    fn holds(&self, head: &str, console: u64) -> bool {
        !self.gave_up && self.keeper.keeps() && storable(&Name::new(head, console).encode())
    }

    fn take(&mut self, head: &str, console: u64) -> Option<OwnedFd> {
        let master = self.kept.take(head, console)?;
        let name = Name::new(head, console).encode();
        if self.rekeep.contains(&name) {
            // Drops the second master too; the session's open keeps this
            // one again.
            self.rekeep.retain(|n| *n != name);
            self.forget_name(&name);
        }
        Some(master)
    }

    fn waiting(&mut self, known: &BTreeSet<String>) -> Vec<(String, u64)> {
        let apart = self.kept.without_place(|head| known.contains(head));
        let waiting: Vec<(String, u64)> = known
            .iter()
            .flat_map(|head| {
                self.kept
                    .consoles(head)
                    .into_iter()
                    .map(move |id| (head.clone(), id))
            })
            .collect();
        for (name, master) in apart {
            match Name::parse(&name) {
                // A second master under a name whose first waits: dropping
                // the name would drop the first too.
                Some(n) if waiting.iter().any(|(h, c)| *h == n.head && *c == n.console) => {
                    self.rekeep.push(name.clone());
                    self.hang_up(
                        &name,
                        master,
                        "another session was kept for the same console",
                        false,
                    );
                }
                Some(_) => self.hang_up(&name, master, "its head is not in vtium.conf", true),
                None => self.hang_up(&name, master, "", true),
            }
        }
        waiting
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use std::os::fd::AsFd;

    #[test]
    fn a_kept_master_waits_for_its_console() {
        let (a, _b) = rustix::pipe::pipe().unwrap();
        let mut store = Store::new(Keeper::Off, vec![(Name::new("left", 3).encode(), a)]);
        assert!(store.take("left", 2).is_none());
        assert!(store.take("right", 3).is_none());
        assert!(store.take("left", 3).is_some());
        assert!(
            store.take("left", 3).is_none(),
            "a kept master is handed out once"
        );
    }

    /// A keeper that is systemd's store behind a socket in the abstract
    /// namespace named `name`, and the socket systemd would read, when
    /// `listen` is set.
    fn systemd(name: &str, listen: bool) -> (Keeper, Option<OwnedFd>) {
        use rustix::net::{AddressFamily, SocketAddrUnix, SocketFlags, SocketType};
        let abstract_name = format!("vtium-store-test-{}-{name}", std::process::id());
        let path = format!("@{abstract_name}");
        let listener = listen.then(|| {
            let fd = rustix::net::socket_with(
                AddressFamily::UNIX,
                SocketType::DGRAM,
                SocketFlags::CLOEXEC,
                None,
            )
            .unwrap();
            let addr = SocketAddrUnix::new_abstract_name(abstract_name.as_bytes()).unwrap();
            rustix::net::bind(&fd, &addr).unwrap();
            fd
        });
        let store = vtium::survive::notify::Store::open(path.as_bytes(), 8).unwrap();
        (Keeper::Store(store), listener)
    }

    /// The store holds a session's master, so the session outlives vtium,
    /// only while a keeper keeps, the keeper has not failed, and systemd
    /// takes the master's name.
    #[test]
    fn the_store_holds_a_master_only_where_a_working_keeper_takes_its_name() {
        let off = Store::new(Keeper::Off, Vec::new());
        assert!(!off.holds("left", 1), "no keeper keeps");

        let (keeper, _listener) = systemd("on", true);
        let mut on = Store::new(keeper, Vec::new());
        let (a, _b) = rustix::pipe::pipe().unwrap();
        on.keep("left", 1, a.as_fd());
        assert!(on.holds("left", 1));
        let long = "h".repeat(300);
        assert!(
            !on.holds(&long, 1),
            "systemd takes no name of more than 255 bytes"
        );

        let (keeper, _) = systemd("gone", false);
        let mut failed = Store::new(keeper, Vec::new());
        assert!(failed.holds("left", 1), "the keeper has not failed yet");
        failed.keep("left", 1, a.as_fd());
        assert!(
            !failed.holds("left", 1),
            "a keeper that failed still holds the sessions"
        );
    }

    /// Of the masters an earlier vtium kept, those of a head vtium.conf
    /// names wait for their consoles, in order. The root process closes its
    /// copy of each other one at once: a head that is gone, a name vtium
    /// does not write, and a second master under one name.
    #[test]
    fn a_master_without_a_place_is_let_go_and_the_rest_wait() {
        let pipe = || rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC).unwrap();
        let (left2, left2_w) = pipe();
        let (left1, left1_w) = pipe();
        let (gone, gone_w) = pipe();
        let (stray, stray_w) = pipe();
        let (second, second_w) = pipe();
        let mut store = Store::new(
            Keeper::Off,
            vec![
                (Name::new("left", 2).encode(), left2_w),
                (Name::new("gone", 1).encode(), gone_w),
                ("stored".to_owned(), stray_w),
                (Name::new("left", 1).encode(), left1_w),
                (Name::new("left", 1).encode(), second_w),
            ],
        );
        let known = ["left", "right"].map(str::to_owned).into();
        assert_eq!(
            store.waiting(&known),
            [("left".to_owned(), 1), ("left".to_owned(), 2)]
        );
        // A pipe's reading end polls ready once its writing end is closed
        // everywhere. A child that another test forks holds a copy of each
        // descriptor until it execs, so the wait for a close is bounded at
        // 10 s rather than none.
        let closed = |fd: &OwnedFd, seconds: i64| {
            let mut fds = [rustix::event::PollFd::new(fd, rustix::event::PollFlags::IN)];
            let bound = rustix::event::Timespec {
                tv_sec: seconds,
                tv_nsec: 0,
            };
            rustix::event::poll(&mut fds, Some(&bound)).unwrap() == 1
        };
        assert!(closed(&gone, 10), "the gone head's master is still held");
        assert!(closed(&stray, 10), "the stray master is still held");
        assert!(closed(&second, 10), "the second master is still held");
        assert!(
            !closed(&left1, 0) && !closed(&left2, 0),
            "a waiting master was let go"
        );
        assert!(store.take("left", 1).is_some());
        assert!(store.take("left", 2).is_some());
    }

    #[test]
    fn nowhere_keeps_nothing() {
        let (a, _b) = rustix::pipe::pipe().unwrap();
        let mut keep = Nowhere;
        keep.keep("left", 1, a.as_fd());
        assert!(keep.take("left", 1).is_none());
    }
}
