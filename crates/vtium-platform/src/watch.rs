//! A watch on udev's events for displays and input devices, for a program
//! that only needs to know that a device came, went or changed, such as
//! vtium-config's Flow view (ADR-1403 D7). It reads each event's action,
//! subsystem and name from the netlink socket and opens no device node,
//! unlike [`crate::udev::Udev`], which opens a card to learn its driver.
//! With no udev daemon running no event arrives.

use std::ffi::OsStr;
use std::io;
use std::os::fd::{AsFd, BorrowedFd};

/// What one udev event says happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Came(Device),
    Went(Device),
    /// The device changed, as a card does when a display is plugged into
    /// one of its connectors.
    Changed(Device),
}

/// A device as an event names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// `drm` or `input`.
    pub subsystem: String,
    /// The kernel's name for it, such as `card1` or `event4`.
    pub name: String,
}

/// The change an event reports, from its action, subsystem and name;
/// `None` for an action that is none of `add`, `remove` and `change`.
pub fn change(action: Option<&str>, subsystem: Option<&str>, name: &str) -> Option<Change> {
    let device = Device {
        subsystem: subsystem.unwrap_or_default().to_owned(),
        name: name.to_owned(),
    };
    match action? {
        "add" => Some(Change::Came(device)),
        "remove" => Some(Change::Went(device)),
        "change" => Some(Change::Changed(device)),
        _ => None,
    }
}

/// The watch: udev's monitor socket for the `drm` and `input` subsystems.
pub struct Watch {
    socket: udev::MonitorSocket,
}

impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watch").finish_non_exhaustive()
    }
}

impl Watch {
    /// Listens to udev's events for displays and input devices, without
    /// blocking.
    pub fn new() -> io::Result<Watch> {
        let socket = udev::MonitorBuilder::new()?
            .match_subsystem("drm")?
            .match_subsystem("input")?
            .listen()?;
        let flags = rustix::fs::fcntl_getfl(&socket)?;
        rustix::fs::fcntl_setfl(&socket, flags | rustix::fs::OFlags::NONBLOCK)?;
        Ok(Watch { socket })
    }

    /// The changes waiting on the socket, each read from the event alone.
    pub fn changes(&mut self) -> Vec<Change> {
        let text = |s: Option<&OsStr>| s.and_then(OsStr::to_str).map(str::to_owned);
        self.socket
            .iter()
            .filter_map(|event| {
                change(
                    text(event.action()).as_deref(),
                    text(event.subsystem()).as_deref(),
                    &event.sysname().to_string_lossy(),
                )
            })
            .collect()
    }
}

impl AsFd for Watch {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.socket.as_fd()
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn an_event_s_action_says_whether_a_device_came_went_or_changed() {
        let card = Device {
            subsystem: "drm".into(),
            name: "card1".into(),
        };
        assert_eq!(
            change(Some("add"), Some("drm"), "card1"),
            Some(Change::Came(card.clone()))
        );
        assert_eq!(
            change(Some("remove"), Some("drm"), "card1"),
            Some(Change::Went(card.clone()))
        );
        assert_eq!(
            change(Some("change"), Some("drm"), "card1"),
            Some(Change::Changed(card))
        );
        assert_eq!(change(Some("bind"), Some("drm"), "card1"), None);
        assert_eq!(change(None, Some("input"), "event4"), None);
    }

    /// The devices this process holds open: each descriptor's target
    /// under `/dev`, leaving out the null device and the terminals, which
    /// other tests of this crate open while this one runs.
    fn devices_open() -> Vec<std::path::PathBuf> {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
            .filter(|t| t.starts_with("/dev/"))
            .filter(|t| {
                !(t.starts_with("/dev/pts") || *t == std::path::Path::new("/dev/null"))
                    && !t.starts_with("/dev/ptmx")
            })
            .collect()
    }

    /// Reading the watch opens no device, whatever the events. Where
    /// netlink is refused, as in some sandboxes, there is nothing to
    /// watch and nothing to check.
    #[test]
    fn reading_the_watch_opens_no_device() {
        let before = devices_open();
        let Ok(mut watch) = Watch::new() else {
            return;
        };
        for _ in 0..3 {
            let _ = watch.changes();
        }
        let after: Vec<_> = devices_open()
            .into_iter()
            .filter(|d| !before.contains(d))
            .collect();
        assert!(after.is_empty(), "{after:?}");
    }
}
