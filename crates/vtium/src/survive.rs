//! Sessions that survive a vtium restart (ADR-1401 D13).
//!
//! When vtium starts a session, it gives a copy of the session's pty
//! master to a keeper that outlives vtium: systemd's file-descriptor store
//! where the unit enables one ([`notify`]), or else, when vtium runs as
//! root, a small holder process of its own ([`hold`]). While any copy of a
//! master stays open, the kernel does not hang up the pty's session, so
//! when vtium crashes or is restarted, each session's program keeps
//! running. A vtium that starts again takes the masters back from the
//! keeper, puts each on the console it ran on, and sends it a resize so
//! that its program redraws. Only the screen contents and the scrollback
//! are lost.
//!
//! Each master is kept under a name that holds its head's name and its
//! console's id ([`Name`]), so the restarted vtium knows where each
//! session goes. vtium tells the keeper to drop a master when its session
//! ends or its console closes.

pub mod hold;
pub mod notify;

use std::ffi::OsString;
use std::io;
use std::os::fd::{BorrowedFd, OwnedFd};
use std::path::Path;

/// The name a kept master is stored under: its head and its console's id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Name {
    pub head: String,
    pub console: u64,
}

/// What every name of a kept master starts with.
const PREFIX: &str = "pty.";

/// The longest name systemd takes for a stored descriptor (`FDNAME_MAX`).
pub const NAME_MAX: usize = 255;

impl Name {
    pub fn new(head: &str, console: u64) -> Name {
        Name {
            head: head.to_owned(),
            console,
        }
    }

    /// `pty.<console>.<head>`. A head's name holds only `A-Z a-z 0-9 . _
    /// + -` (vtium-settings' name rule), so the name holds no `:` and no
    /// control character, as systemd asks of a stored descriptor's name.
    /// The head's name comes last because it may hold a `.`.
    pub fn encode(&self) -> String {
        format!("{PREFIX}{}.{}", self.console, self.head)
    }

    /// The name [`Name::encode`] wrote, or `None` for any other text, such
    /// as `stored`, the name systemd gives a descriptor stored without one.
    pub fn parse(text: &str) -> Option<Name> {
        let rest = text.strip_prefix(PREFIX)?;
        let (id, head) = rest.split_once('.')?;
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || id.starts_with('0') {
            return None;
        }
        let good_head = !head.is_empty()
            && head
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'));
        if !good_head || text.len() > NAME_MAX {
            return None;
        }
        Some(Name {
            head: head.to_owned(),
            console: id.parse().ok()?,
        })
    }
}

/// Whether systemd takes `name` as a stored descriptor's name: at most
/// [`NAME_MAX`] bytes of printable ASCII, with no `:`, which separates the
/// names in `LISTEN_FDNAMES` (sd_pid_notify_with_fds(3)).
pub fn storable(name: &str) -> bool {
    name.len() <= NAME_MAX && name.bytes().all(|b| (0x20..0x7f).contains(&b) && b != b':')
}

/// The variables systemd sets for vtium's own use, which no session
/// inherits: the descriptors passed at a restart, the notification
/// socket and the size of the file-descriptor store.
const SYSTEMD_ONLY: [&str; 5] = [
    "LISTEN_FDS",
    "LISTEN_PID",
    "LISTEN_FDNAMES",
    "NOTIFY_SOCKET",
    "FDSTORE",
];

/// vtium's environment without the variables that are systemd's word to
/// vtium alone, as the sessions inherit it with `reset-env` off.
pub fn session_env(env: Vec<(OsString, OsString)>) -> Vec<(OsString, OsString)> {
    env.into_iter()
        .filter(|(k, _)| !SYSTEMD_ONLY.iter().any(|v| k == v))
        .collect()
}

/// The keeper of a vtium starting on the VT at `tty` with the environment
/// `env`, and the masters an earlier vtium left with it.
///
/// Where systemd passes `NOTIFY_SOCKET` and a store of a nonzero size
/// (`FDSTORE`), the keeper is the store, and the masters are those systemd
/// passed, which `take` takes from descriptor 3 up. Otherwise, when vtium
/// runs as root on a VT, `run_dir`'s directory for that VT holds the
/// holder's socket, and the keeper is the holder found there or the one
/// `exe` starts. It is called before vtium blocks any signal, since the
/// holder it starts inherits the signal mask. Any other vtium, such as one
/// a user runs by hand, keeps nothing, and its sessions end with it.
pub fn start(
    env: &[(OsString, OsString)],
    tty: Option<&Path>,
    run_dir: &Path,
    exe: &Path,
    take: impl FnOnce(usize) -> io::Result<Vec<OwnedFd>>,
) -> (Keeper, Vec<(String, OwnedFd)>) {
    // What systemd passed is vtium's to take whether or not it keeps more.
    let passed = notify::passed(env, std::process::id(), take);
    if let Some(store) = notify::Store::from_env(env) {
        log::info!(
            "vtium keeps each session's pty in systemd's file-descriptor store, so the sessions outlive a restart"
        );
        return (Keeper::Store(store), passed);
    }
    let vt = tty.and_then(crate::getty::vt_number);
    let Some(vt) = vt.filter(|_| rustix::process::geteuid().is_root()) else {
        return (Keeper::Off, passed);
    };
    match hold::start(&run_dir.join(format!("tty{vt}")), exe) {
        Ok((client, mut kept)) => {
            log::info!(
                "vtium keeps each session's pty in its holder, vtium hold, so the sessions outlive a restart"
            );
            kept.extend(passed);
            (Keeper::Hold(client), kept)
        }
        Err(e) => {
            log::warn!("vtium cannot start its holder, so the sessions end if vtium restarts: {e}");
            (Keeper::Off, passed)
        }
    }
}

/// The masters a starting vtium took back from its keeper, waiting for
/// their consoles. They are at most as many as the consoles vtium opens,
/// so a list serves.
#[derive(Default)]
pub struct Kept {
    masters: Vec<(Name, OwnedFd)>,
    /// Masters whose name is not one vtium writes, which have no place.
    unnamed: Vec<(String, OwnedFd)>,
}

impl Kept {
    /// The masters `named` holds. A name vtium does not write, or a second
    /// master under one name, has no place, and [`Kept::without_place`]
    /// returns it.
    pub fn new(named: Vec<(String, OwnedFd)>) -> Kept {
        let mut kept = Kept::default();
        for (text, fd) in named {
            match Name::parse(&text) {
                Some(name) if !kept.masters.iter().any(|(n, _)| *n == name) => {
                    kept.masters.push((name, fd));
                }
                _ => kept.unnamed.push((text, fd)),
            }
        }
        kept
    }

    /// Takes the master of head `head`'s console `console`.
    pub fn take(&mut self, head: &str, console: u64) -> Option<OwnedFd> {
        let at = self
            .masters
            .iter()
            .position(|(n, _)| n.head == head && n.console == console)?;
        Some(self.masters.swap_remove(at).1)
    }

    /// The ids of head `head`'s consoles with a master waiting, in order.
    pub fn consoles(&self, head: &str) -> Vec<u64> {
        let mut ids: Vec<u64> = Vec::new();
        for (name, _) in self.masters.iter().filter(|(n, _)| n.head == head) {
            let at = ids.binary_search(&name.console).unwrap_or_else(|at| at);
            ids.insert(at, name.console);
        }
        ids
    }

    pub fn is_empty(&self) -> bool {
        self.masters.is_empty() && self.unnamed.is_empty()
    }

    /// Takes the masters of the heads `stays` keeps no place for, and those
    /// whose name is not vtium's, each with the name it was kept under.
    pub fn without_place(&mut self, stays: impl Fn(&str) -> bool) -> Vec<(String, OwnedFd)> {
        let mut out: Vec<(String, OwnedFd)> = std::mem::take(&mut self.unnamed);
        let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.masters)
            .into_iter()
            .partition(|(n, _)| stays(&n.head));
        self.masters = keep;
        out.extend(gone.into_iter().map(|(n, fd)| (n.encode(), fd)));
        out
    }
}

/// Where vtium keeps a copy of each session's pty master.
#[derive(Default)]
pub enum Keeper {
    /// Nowhere: a vtium run by hand as an ordinary user, or one whose
    /// keeper failed. Its sessions end with it.
    #[default]
    Off,
    /// systemd's file-descriptor store.
    Store(notify::Store),
    /// vtium's holder process.
    Hold(hold::Client),
}

impl Keeper {
    /// Whether the sessions outlive vtium.
    pub fn keeps(&self) -> bool {
        !matches!(self, Keeper::Off)
    }

    /// Gives the keeper a copy of `master` under `name`.
    pub fn keep(&mut self, name: &str, master: BorrowedFd<'_>) -> io::Result<()> {
        match self {
            Keeper::Off => Ok(()),
            Keeper::Store(store) => store.keep(name, master),
            Keeper::Hold(client) => client.keep(name, master),
        }
    }

    /// Tells the keeper to close its copy of the master kept under `name`.
    pub fn forget(&mut self, name: &str) -> io::Result<()> {
        match self {
            Keeper::Off => Ok(()),
            Keeper::Store(store) => store.forget(name),
            Keeper::Hold(client) => client.forget(name),
        }
    }
}

/// Which master the keeper holds under each name, so vtium tells it only
/// what changed. The seat gives each pty it opens or takes back an id it
/// never gives again in a run, and the ledger records each name's id.
#[derive(Default)]
pub struct Ledger {
    held: Vec<(String, u64)>,
}

impl Ledger {
    /// What vtium tells `keeper` so that it holds exactly `live`: each
    /// name with the id of its pty and the pty's master. A master under a
    /// name whose id changed replaces the one kept, and a name no longer
    /// live is forgotten. A failure is logged, and the session it concerns
    /// does not survive a restart.
    pub fn sync<'a>(
        &mut self,
        keeper: &mut Keeper,
        live: impl IntoIterator<Item = (String, u64, BorrowedFd<'a>)>,
    ) {
        if !keeper.keeps() {
            return;
        }
        let mut seen: Vec<String> = Vec::new();
        for (name, id, master) in live {
            let had = self.held.iter().position(|(n, _)| *n == name);
            if let Some(at) = had {
                if self.held.get(at).is_some_and(|&(_, held)| held == id) {
                    seen.push(name);
                    continue;
                }
                self.held.swap_remove(at);
                if let Err(e) = keeper.forget(&name) {
                    log::warn!("cannot drop the kept copy of {name}: {e}");
                }
            }
            match keeper.keep(&name, master) {
                Ok(()) => self.held.push((name.clone(), id)),
                Err(e) => {
                    log::warn!("cannot keep {name}, so its session ends if vtium restarts: {e}");
                }
            }
            seen.push(name);
        }
        let (stay, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|(n, _)| seen.contains(n));
        self.held = stay;
        for (name, _) in gone {
            if let Err(e) = keeper.forget(&name) {
                log::warn!("cannot drop the kept copy of {name}: {e}");
            }
        }
    }

    /// Records that the keeper holds `name`'s master, whose pty has the id
    /// `id`, as it does for a master vtium took back at its start.
    pub fn held(&mut self, name: String, id: u64) {
        self.held.retain(|(n, _)| *n != name);
        self.held.push((name, id));
    }

    /// Whether the keeper holds a master under `name`.
    pub fn holds(&self, name: &str) -> bool {
        self.held.iter().any(|(n, _)| n == name)
    }
}

#[cfg(test)]
mod tests;
