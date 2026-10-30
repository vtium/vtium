//! The seat's devices as a head file's rules see them, listed read-only
//! from sysfs and udev's database for `vtium check-config` (ADR-1401 D7).
//!
//! [`list`] opens nothing under `/dev` and asks no device anything: it
//! reads the files the kernel shows in sysfs and the entries udev keeps in
//! its database, so it runs unprivileged and changes nothing. An input
//! device's identity is the monitor's ([`InputIdentity`]): udev's classes
//! where its database has them, else udev's rule applied to the bitmaps
//! sysfs shows, with the properties the kernel's `uevent` gives and those
//! udev recorded, as libudev builds them ([`properties`]). A card's
//! `ID_PATH` is its database entry's, and it is primary by the monitor's
//! rule. A display is a connector whose sysfs `status` is `connected` and
//! whose `modes` lists a mode, named as the kernel names it, with the
//! monitor its sysfs `edid` names; whether it was lit at start takes
//! opening the card, so a listed display was never lit at start. A
//! framebuffer no DRM driver backs is listed by the monitor's rule, read
//! from the id its sysfs `name` shows.

#[cfg(test)]
mod tests;

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::bounds::{UDEV_PROPERTIES_MAX, UDEV_PROPERTY_MAX};
use crate::display::{self, Edid};
use crate::udev::{self, InputIdentity, Uevent, ordered};

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

/// A framebuffer no DRM driver backs, the one display it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Framebuffer {
    /// Its node, as `/dev/fb0`: a name, never opened.
    pub node: PathBuf,
    /// The framebuffer the firmware set up (`VESA VGA`).
    pub primary: bool,
}

/// The seat's devices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub inputs: Vec<InputIdentity>,
    pub cards: Vec<Card>,
    pub framebuffers: Vec<Framebuffer>,
}

/// The devices of `seat` under `roots`, each kept as the monitor keeps it:
/// an event node with a parent input device, a card, and a framebuffer no
/// DRM driver backs, by the seat its `ID_SEAT` names, `seat0` when unset.
/// A directory that cannot be read lists nothing.
pub fn list(roots: &Roots, seat: &str) -> Listing {
    Listing {
        inputs: inputs(roots, seat),
        cards: cards(roots, seat),
        framebuffers: framebuffers(roots, seat),
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
            let all = properties(roots, &node, "input");
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
            let node = node(&dir, &name);
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

fn framebuffers(roots: &Roots, seat: &str) -> Vec<Framebuffer> {
    let class = roots.sys.join("class/graphics");
    numbered(&class, "fb")
        .into_iter()
        .filter_map(|name| {
            let dir = class.join(&name);
            let db = database(roots, &entry(&dir)?);
            if value(&db, "ID_SEAT").unwrap_or("seat0") != seat {
                return None;
            }
            let flags = udev::fb_id_flags(attribute(&dir.join("name")).as_deref());
            (!flags.drm_backed).then(|| Framebuffer {
                node: node(&dir, &name),
                primary: flags.primary,
            })
        })
        .collect()
}

/// The node of the device `name` at `dir`: its `uevent`'s `DEVNAME` under
/// `/dev`, else `/dev/dri/<name>` for a card and `/dev/<name>` for a
/// framebuffer.
fn node(dir: &Path, name: &str) -> PathBuf {
    let uevent = pairs(&lines(&dir.join("uevent")), "");
    match value(&uevent, "DEVNAME") {
        Some(devname) => Path::new("/dev").join(devname),
        None if name.starts_with("card") => Path::new("/dev/dri").join(name),
        None => Path::new("/dev").join(name),
    }
}

/// The displays of the card `name` whose sysfs directory is `dir`: each
/// connector whose `status` is `connected` and whose `modes` lists one, as
/// the start needs a mode to light it, by connector id, the kernel's order
/// of connectors, then by name in natural order (`DP-2` before `DP-10`).
fn displays(dir: &Path, name: &str, id_path: Option<&str>) -> Vec<display::Identity> {
    let prefix = format!("{name}-");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let connected = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|d| {
            let connector = d
                .strip_prefix(&prefix)
                .filter(|c| !c.is_empty())?
                .to_owned();
            let at = dir.join(&d);
            let lit = attribute(&at.join("status")).as_deref() == Some("connected")
                && bytes(&at.join("modes"), 0).is_some_and(|b| !b.is_empty());
            lit.then(|| {
                let id = attribute(&at.join("connector_id"))
                    .and_then(|id| id.parse().ok())
                    .unwrap_or(u64::MAX);
                (key(id, &connector), connector)
            })
        })
        .collect();
    sorted(connected)
        .into_iter()
        .map(|connector| display::Identity {
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
    let found = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| {
            name.strip_prefix(prefix)
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|name| (key(0, &name), name))
        .collect();
    sorted(found)
}

/// How every list sorts: by a number, then by a name in natural order, its
/// text and then the number that ends it.
type Key = (u64, String, u64);

/// The key of `name`, after `first`.
fn key(first: u64, name: &str) -> Key {
    let text = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let number = name
        .get(text.len()..)
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::MAX);
    (first, text.to_owned(), number)
}

/// The names of `found` in the order of their keys.
fn sorted(mut found: Vec<(Key, String)>) -> Vec<String> {
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

/// The properties of the device at `dir` on `subsystem`, as sd-device,
/// behind libudev, gives them to the start: its `DEVPATH`; its `uevent`'s,
/// with `DEVNAME` under `/dev`; its `SUBSYSTEM` and `DRIVER`; and from its
/// udev database entry, `USEC_INITIALIZED` (`I:`), its properties (`E:`),
/// each replacing one of the same name, `DEVLINKS` (`S:`, each under
/// `/dev`, joined by spaces), and `TAGS` (`G:`) and `CURRENT_TAGS` (`Q:`),
/// as `:a:b:`. sd-device joins the links and tags in a hash order that
/// changes from process to process; these are sorted, as the start's are
/// ([`udev::ordered`]).
fn properties(roots: &Roots, dir: &Path, subsystem: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let devpath = roots
        .sys
        .canonicalize()
        .ok()
        .zip(dir.canonicalize().ok())
        .and_then(|(sys, dir)| Some(format!("/{}", dir.strip_prefix(sys).ok()?.display())));
    if let Some(devpath) = devpath {
        set(&mut out, "DEVPATH", devpath);
    }
    for (name, value) in pairs(&lines(&dir.join("uevent")), "") {
        let value = match name.as_str() {
            "DEVNAME" if !value.starts_with('/') => format!("/dev/{value}"),
            _ => value,
        };
        set(&mut out, &name, value);
    }
    set(&mut out, "SUBSYSTEM", subsystem.to_owned());
    let driver = std::fs::read_link(dir.join("driver")).ok();
    if let Some(driver) = driver.as_deref().and_then(Path::file_name) {
        set(&mut out, "DRIVER", driver.to_string_lossy().into_owned());
    }
    let entry = entry(dir).map_or_else(Vec::new, |e| lines(&roots.udev_data.join(e)));
    let (mut links, mut tags, mut current) = (Vec::new(), Vec::new(), Vec::new());
    for line in &entry {
        match line.split_at_checked(2) {
            Some(("I:", usec)) => set(&mut out, "USEC_INITIALIZED", usec.to_owned()),
            Some(("E:", property)) => {
                if let Some((name, value)) = property.split_once('=') {
                    set(&mut out, name, value.to_owned());
                }
            }
            Some(("S:", link)) => links.push(format!("/dev/{link}")),
            Some(("G:", tag)) => tags.push(tag),
            Some(("Q:", tag)) => current.push(tag),
            _ => {}
        }
    }
    if !links.is_empty() {
        set(&mut out, "DEVLINKS", ordered("DEVLINKS", links.join(" ")));
    }
    for (name, tags) in [("TAGS", tags), ("CURRENT_TAGS", current)] {
        if !tags.is_empty() {
            set(
                &mut out,
                name,
                ordered(name, format!(":{}:", tags.join(":"))),
            );
        }
    }
    out
}

/// Sets `name` to `value` among `pairs`, in its place where it is there.
fn set(pairs: &mut Vec<(String, String)>, name: &str, value: String) {
    match pairs.iter_mut().find(|(n, _)| n == name) {
        Some((_, v)) => *v = value,
        None => pairs.push((name.to_owned(), value)),
    }
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
