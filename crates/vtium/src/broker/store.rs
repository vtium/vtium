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
//! drawing process the kept master in place of a new login.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the broker keeps the pty
//!   masters through a restart, as the unit's main process.
//! - ADR-1401 D13: the store, the holder, the names of kept masters, and
//!   that vtium's own exit never hangs up a kept session.

use std::io;
use std::os::fd::{BorrowedFd, OwnedFd};

use vtium::survive::{Keeper, Kept, Name, storable};

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
}

/// The keeper [`vtium::survive::start`] found, and the masters it gave
/// back.
pub struct Store {
    keeper: Keeper,
    kept: Kept,
    /// Set once the keeper has failed, after which nothing more is kept.
    gave_up: bool,
}

impl Store {
    pub fn new(keeper: Keeper, kept: Vec<(String, OwnedFd)>) -> Store {
        Store {
            keeper,
            kept: Kept::new(kept),
            gave_up: false,
        }
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
        // The line vtium in one process logs for the same event, so the
        // log reads the same whether vtium splits or not.
        log::info!(
            "head {head}: console {console} takes back the session that outlived the last vtium"
        );
        Some(master)
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

    #[test]
    fn nowhere_keeps_nothing() {
        let (a, _b) = rustix::pipe::pipe().unwrap();
        let mut keep = Nowhere;
        keep.keep("left", 1, a.as_fd());
        assert!(keep.take("left", 1).is_none());
    }
}
