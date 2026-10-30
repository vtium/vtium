//! The seat's devices as a head file's rules see them, listed read-only
//! from sysfs and udev's database for `vtium check-config` (ADR-1401 D7).
//!
//! [`list`] opens nothing under `/dev` and asks no device anything: it
//! reads the files the kernel shows in sysfs and the entries udev keeps in
//! its database, so it runs unprivileged and changes nothing. An input
//! device's identity is the monitor's ([`InputIdentity`]): udev's classes
//! where its database has them, else udev's rule applied to the bitmaps
//! sysfs shows, with the properties the kernel's `uevent` gives and those
//! udev recorded. A card's `ID_PATH` is its database entry's, and it is
//! primary by the monitor's rule. A display is a connector whose sysfs
//! `status` is `connected`, named as the kernel names it, with the monitor
//! its sysfs `edid` names; whether it was lit at start takes opening the
//! card, so a listed display was never lit at start.

#[cfg(test)]
mod tests;

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::bounds::{UDEV_PROPERTIES_MAX, UDEV_PROPERTY_MAX};
use crate::display::{self, Edid};
use crate::udev::{self, InputIdentity, Uevent};

/// Bytes read of one udev database entry or `uevent` file: as many
/// properties as an identity keeps, each as long as one may be. A longer
/// entry loses its lines past it, never a part of one.
const ENTRY_MAX: usize = UDEV_PROPERTIES_MAX.saturating_mul(UDEV_PROPERTY_MAX);

/// Where the listing reads: sysfs, and udev's database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roots {
    pub sys: PathBuf,
    pub udev_data: PathBuf,
}

impl Roots {
    /// The machine's own: `/sys` and `/run/udev/data`.
    pub fn system() -> Roots {
        Roots {
            sys: PathBuf::from("/sys"),
            udev_data: PathBuf::from("/run/udev/data"),
        }
    }
}

/// A DRM card and its connected displays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    /// Its node, as `/dev/dri/card1`: a name, never opened.
    pub node: PathBuf,
    /// udev's `ID_PATH` of the card.
    pub id_path: Option<String>,
    /// The card the system booted on.
    pub primary: bool,
    /// Its connected displays, in the kernel's order of connectors.
    pub displays: Vec<display::Identity>,
}

/// The seat's devices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub inputs: Vec<InputIdentity>,
    pub cards: Vec<Card>,
}

/// The devices of `seat` under `roots`, each kept as the monitor keeps it:
/// an event node with a parent input device, and a card, by the seat its
/// `ID_SEAT` names, `seat0` when unset. A directory that cannot be read
/// lists nothing.
pub fn list(roots: &Roots, seat: &str) -> Listing {
    Listing {
        inputs: inputs(roots, seat),
        cards: cards(roots, seat),
    }
}

fn inputs(roots: &Roots, seat: &str) -> Vec<InputIdentity> {
    let class = roots.sys.join("class/input");
    numbered(&class, "event")
        .into_iter()
        .filter_map(|name| {
            let node = class.join(&name);
            let parent = parent_on(&roots.sys, &node, "input")?;
            let parent_name = parent.file_name()?.to_str()?;
            let parent_db = database(roots, &format!("+input:{parent_name}"));
            if value(&parent_db, "ID_SEAT").unwrap_or("seat0") != seat {
                return None;
            }
            let mut all = kernel(roots, &node, "input");
            all.extend(entry(&node).map_or_else(Vec::new, |e| database(roots, &e)));
            let properties = udev::properties(
                |name| value(&all, name).map(str::to_owned),
                all.iter().cloned(),
            );
            let attributes = udev::attributes(|name| attribute(&parent.join(name)));
            Some(InputIdentity::of(&Uevent {
                properties,
                attributes,
                ..Uevent::default()
            }))
        })
        .collect()
}

fn cards(roots: &Roots, seat: &str) -> Vec<Card> {
    let class = roots.sys.join("class/drm");
    numbered(&class, "card")
        .into_iter()
        .filter_map(|name| {
            let dir = class.join(&name);
            let db = database(roots, &entry(&dir)?);
            if value(&db, "ID_SEAT").unwrap_or("seat0") != seat {
                return None;
            }
            let id_path = value(&db, "ID_PATH").map(str::to_owned);
            let pci = parent_on(&roots.sys, &dir, "pci");
            let primary = udev::primary(&Uevent {
                pci_boot_vga: pci.map(|p| attribute(&p.join("boot_vga")).as_deref() == Some("1")),
                usb_parent: parent_on(&roots.sys, &dir, "usb").is_some(),
                platform_parent: parent_on(&roots.sys, &dir, "platform").is_some(),
                ..Uevent::default()
            });
            let uevent = lines(&dir.join("uevent"));
            let node = value(&pairs(&uevent, ""), "DEVNAME").map_or_else(
                || Path::new("/dev/dri").join(&name),
                |n| Path::new("/dev").join(n),
            );
            let displays = displays(&dir, &name, id_path.as_deref());
            Some(Card {
                node,
                id_path,
                primary,
                displays,
            })
        })
        .collect()
}

/// The connected displays of the card `name` whose sysfs directory is
/// `dir`, by connector id, which is the kernel's order of connectors.
fn displays(dir: &Path, name: &str, id_path: Option<&str>) -> Vec<display::Identity> {
    let prefix = format!("{name}-");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut connected: Vec<(u32, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|d| {
            let connector = d
                .strip_prefix(&prefix)
                .filter(|c| !c.is_empty())?
                .to_owned();
            (attribute(&dir.join(&d).join("status")).as_deref() == Some("connected")).then(|| {
                let id = attribute(&dir.join(&d).join("connector_id"))
                    .and_then(|id| id.parse().ok())
                    .unwrap_or(u32::MAX);
                (id, connector)
            })
        })
        .collect();
    connected.sort();
    connected
        .into_iter()
        .map(|(_, connector)| display::Identity {
            edid: Edid::sysfs(dir, &connector).map(|e| e.id),
            connector,
            card: id_path.map(str::to_owned),
            lit_at_start: false,
        })
        .collect()
}

/// The entries of `dir` named `prefix` and a number, as `event3` or
/// `card1`, in the order of their numbers.
fn numbered(dir: &Path, prefix: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(u64, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|name| {
            let number = name.strip_prefix(prefix)?;
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            Some((number.parse().ok()?, name))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, name)| name).collect()
}

/// The nearest device above `dir` on `subsystem`, as libudev finds a
/// parent with a subsystem: each directory above it in sysfs's device
/// tree, by its `subsystem` link.
fn parent_on(sys: &Path, dir: &Path, subsystem: &str) -> Option<PathBuf> {
    let devices = sys.join("devices").canonicalize().ok()?;
    let mut at = dir.canonicalize().ok()?;
    while at.pop() && at.starts_with(&devices) && at != devices {
        let on = std::fs::read_link(at.join("subsystem")).ok();
        if on.as_deref().and_then(Path::file_name) == Some(subsystem.as_ref()) {
            return Some(at);
        }
    }
    None
}

/// udev's database entry of the device at `dir`, by its device number:
/// `c13:64` for `/sys/class/input/event0`.
fn entry(dir: &Path) -> Option<String> {
    Some(format!("c{}", attribute(&dir.join("dev"))?))
}

/// The properties the kernel gives the device at `dir` on `subsystem`, as
/// libudev reads them: its `DEVPATH` and `SUBSYSTEM`, and its `uevent`'s.
fn kernel(roots: &Roots, dir: &Path, subsystem: &str) -> Vec<(String, String)> {
    let devpath = roots
        .sys
        .canonicalize()
        .ok()
        .zip(dir.canonicalize().ok())
        .and_then(|(sys, dir)| Some(format!("/{}", dir.strip_prefix(sys).ok()?.display())));
    let mut out: Vec<(String, String)> = devpath
        .map(|p| ("DEVPATH".to_owned(), p))
        .into_iter()
        .collect();
    out.push(("SUBSYSTEM".to_owned(), subsystem.to_owned()));
    out.extend(pairs(&lines(&dir.join("uevent")), ""));
    out
}

/// The `E:` properties of udev's database entry `name`, in its order; none
/// where udev recorded none, as without a udev daemon.
fn database(roots: &Roots, name: &str) -> Vec<(String, String)> {
    pairs(&lines(&roots.udev_data.join(name)), "E:")
}

/// Each `NAME=VALUE` of `lines` that starts with `prefix`, without it.
fn pairs(lines: &[String], prefix: &str) -> Vec<(String, String)> {
    lines
        .iter()
        .filter_map(|l| {
            let (name, value) = l.strip_prefix(prefix)?.split_once('=')?;
            Some((name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// The value of `name` among `pairs`, the first given.
fn value<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// The whole lines of the file at `path`, at most [`ENTRY_MAX`] bytes of
/// it; none where it cannot be read.
fn lines(path: &Path) -> Vec<String> {
    let Some(mut bytes) = bytes(path, ENTRY_MAX) else {
        return Vec::new();
    };
    if bytes.len() > ENTRY_MAX {
        let end = bytes.iter().take(ENTRY_MAX).rposition(|&b| b == b'\n');
        bytes.truncate(end.unwrap_or(0));
    }
    String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// A sysfs attribute, without the newline that ends it: at most
/// `UDEV_PROPERTY_MAX` bytes and one more, so [`udev::attributes`] sees a
/// longer one as past its bound and leaves it out.
fn attribute(path: &Path) -> Option<String> {
    let bytes = bytes(path, UDEV_PROPERTY_MAX)?;
    let text = String::from_utf8_lossy(&bytes);
    Some(text.strip_suffix('\n').unwrap_or(&text).to_owned())
}

/// At most `max` bytes and one more of the file at `path`.
fn bytes(path: &Path, max: usize) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    let mut out = Vec::new();
    file.take(limit).read_to_end(&mut out).ok()?;
    Some(out)
}
