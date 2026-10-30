//! The listing on stand-in sysfs trees and udev databases, laid out as the
//! kernel and udev lay out the machine's.

#![expect(clippy::unwrap_used, reason = "a test fails by panicking")]
#![expect(clippy::indexing_slicing, reason = "a test fails by panicking")]

use std::collections::BTreeMap;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::{Card, Roots, list};
use crate::bounds::UDEV_PROPERTY_MAX;
use crate::display::{Identity, Serial};
use crate::input::{Class, Classes};

/// A stand-in machine under a directory of its own.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Tree {
        let root =
            std::env::temp_dir().join(format!("vtium-listing-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("sys/class/input")).unwrap();
        std::fs::create_dir_all(root.join("sys/class/drm")).unwrap();
        std::fs::create_dir_all(root.join("udev")).unwrap();
        Tree { root }
    }

    fn roots(&self) -> Roots {
        Roots {
            sys: self.root.join("sys"),
            udev_data: self.root.join("udev"),
        }
    }

    /// Writes `text` at `path` under sysfs, making its directories.
    fn file(&self, path: &str, text: impl AsRef<[u8]>) {
        let path = self.root.join("sys").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Makes `path` under sysfs a link to `target`, as sysfs links are,
    /// relative to the link's directory.
    fn link(&self, path: &str, target: &str) {
        let path = self.root.join("sys").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(target, path).unwrap();
    }

    /// udev's database entry `name`, with `properties` as its `E:` lines.
    fn udev(&self, name: &str, properties: &[&str]) {
        let mut text = String::from("I:1000\n");
        for p in properties {
            text.push_str(&format!("E:{p}\n"));
        }
        text.push_str("G:seat\n");
        std::fs::write(self.root.join("udev").join(name), text).unwrap();
    }

    /// A bus device at `dir` under `sys/devices`, on `subsystem`.
    fn bus_device(&self, dir: &str, subsystem: &str) {
        let depth = dir.split('/').count().saturating_add(1);
        let up = "../".repeat(depth);
        self.link(
            &format!("devices/{dir}/subsystem"),
            &format!("{up}bus/{subsystem}"),
        );
    }

    /// An input device `input` at `parent` with its event node `event`
    /// (major 13, `minor`), its sysfs attributes `attributes` and its
    /// event node's `uevent`.
    fn input(
        &self,
        parent: &str,
        input: &str,
        event: &str,
        minor: u32,
        attributes: &[(&str, &str)],
    ) {
        let dir = format!("devices/{parent}/input/{input}");
        let depth = dir.split('/').count();
        let up = "../".repeat(depth);
        self.link(&format!("{dir}/subsystem"), &format!("{up}class/input"));
        for (name, value) in attributes {
            self.file(&format!("{dir}/{name}"), format!("{value}\n"));
        }
        let node = format!("{dir}/{event}");
        self.file(&format!("{node}/dev"), format!("13:{minor}\n"));
        self.file(
            &format!("{node}/uevent"),
            format!("MAJOR=13\nMINOR={minor}\nDEVNAME=input/{event}\n"),
        );
        self.link(&format!("{node}/device"), "..");
        self.link(
            &format!("{node}/subsystem"),
            &format!("{}class/input", "../".repeat(depth.saturating_add(1))),
        );
        self.link(&format!("class/input/{event}"), &format!("../../{node}"));
        self.link(&format!("class/input/{input}"), &format!("../../{dir}"));
    }

    /// The card `card` (major 226, `minor`) at `parent`, with each of
    /// `connectors` as `(name, status, connector id, EDID)`.
    fn card(
        &self,
        parent: &str,
        card: &str,
        minor: u32,
        connectors: &[(&str, &str, u32, Option<Vec<u8>>)],
    ) {
        let dir = format!("devices/{parent}/drm/{card}");
        self.file(&format!("{dir}/dev"), format!("226:{minor}\n"));
        self.file(
            &format!("{dir}/uevent"),
            format!("MAJOR=226\nMINOR={minor}\nDEVNAME=dri/{card}\nDEVTYPE=drm_minor\n"),
        );
        self.link(&format!("{dir}/device"), "../..");
        for (name, status, id, edid) in connectors {
            let conn = format!("{dir}/{card}-{name}");
            self.file(&format!("{conn}/status"), format!("{status}\n"));
            self.file(&format!("{conn}/connector_id"), format!("{id}\n"));
            self.file(&format!("{conn}/edid"), edid.clone().unwrap_or_default());
            self.link(
                &format!("class/drm/{card}-{name}"),
                &format!("../../{conn}"),
            );
        }
        self.link(&format!("class/drm/{card}"), &format!("../../{dir}"));
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

/// An EDID base block naming maker `AAA`, product 1 and serial number
/// `number`.
fn edid(number: u32) -> Vec<u8> {
    let mut b = vec![0u8; 128];
    b[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    b[8..10].copy_from_slice(&0x0421u16.to_be_bytes());
    b[10..12].copy_from_slice(&1u16.to_le_bytes());
    b[12..16].copy_from_slice(&number.to_le_bytes());
    b[21] = 60;
    b[22] = 34;
    let sum = b.iter().fold(0u8, |s, &x| s.wrapping_add(x));
    b[127] = 0u8.wrapping_sub(sum);
    b
}

const USB: &str = "pci0000:00/0000:00:14.0/usb1/1-3";
const GPU: &str = "pci0000:00/0000:03:00.0";

/// A machine of one GPU with three connectors, a keyboard udev has not
/// tagged, a mouse it has, and a keyboard on another seat.
fn machine(name: &str) -> Tree {
    let t = Tree::new(name);
    t.bus_device(GPU, "pci");
    t.file(&format!("devices/{GPU}/boot_vga"), "1\n");
    t.card(
        GPU,
        "card1",
        1,
        &[
            ("DP-1", "connected", 95, None),
            ("DP-2", "connected", 90, Some(edid(7))),
            ("HDMI-A-1", "disconnected", 99, None),
        ],
    );
    t.file(&format!("devices/{GPU}/drm/renderD128/dev"), "226:128\n");
    t.link(
        "class/drm/renderD128",
        &format!("../../devices/{GPU}/drm/renderD128"),
    );
    t.udev(
        "c226:1",
        &[
            "ID_PATH=pci-0000:03:00.0",
            "ID_FOR_SEAT=drm-pci-0000_03_00_0",
        ],
    );
    t.bus_device(USB, "usb");
    t.input(
        USB,
        "input4",
        "event3",
        67,
        &[
            ("name", "Keyboard B"),
            ("uniq", ""),
            ("id/vendor", "bbbb"),
            ("id/product", "0001"),
            ("capabilities/ev", "3"),
            ("capabilities/key", "fffffffe"),
            ("capabilities/rel", "0"),
            ("capabilities/abs", "0"),
            ("properties", "0"),
        ],
    );
    t.input(
        USB,
        "input5",
        "event4",
        68,
        &[
            ("name", "A mouse"),
            ("id/vendor", "046d"),
            ("id/product", "c52b"),
            ("capabilities/ev", "7"),
        ],
    );
    t.udev(
        "c13:68",
        &[
            "ID_INPUT=1",
            "ID_INPUT_MOUSE=1",
            "ID_PATH=pci-0000:00:14.0-usb-0:3:1.1",
            "ID_VENDOR_ID=046d",
            "ID_MODEL_ID=c52b",
            "ID_SERIAL_SHORT=M1",
            "VTIUM_HEAD=panel",
        ],
    );
    t.input(
        USB,
        "input6",
        "event5",
        69,
        &[
            ("name", "Seat one"),
            ("capabilities/ev", "3"),
            ("capabilities/key", "fffffffe"),
        ],
    );
    t.udev("+input:input6", &["ID_SEAT=seat1"]);
    t
}

// ADR-1401 D4, D7: inputs, cards and connected displays, read from sysfs
// and udev's database without opening a device
#[test]
fn the_listing_reads_the_seats_inputs_cards_and_displays_from_sysfs_and_udev() {
    let t = machine("seat");
    let listing = list(&t.roots(), "seat0");

    let names: Vec<Option<&str>> = listing.inputs.iter().map(|i| i.name.as_deref()).collect();
    assert_eq!(
        names,
        [Some("Keyboard B"), Some("A mouse")],
        "seat1's is left out"
    );

    let keyboard = &listing.inputs[0];
    assert!(!keyboard.tagged, "no database entry: the bitmaps' rule");
    assert_eq!(
        keyboard.classes,
        Classes::empty().with(Class::Keyboard).with(Class::Key)
    );
    assert_eq!(keyboard.id, Some((0xbbbb, 0x0001)));
    assert_eq!(keyboard.uniq, None, "an empty attribute is none");
    assert_eq!(keyboard.id_path, None);
    assert_eq!(
        keyboard.properties,
        BTreeMap::from([
            ("DEVNAME".to_owned(), "input/event3".to_owned()),
            (
                "DEVPATH".to_owned(),
                format!("/devices/{USB}/input/input4/event3")
            ),
            ("MAJOR".to_owned(), "13".to_owned()),
            ("MINOR".to_owned(), "67".to_owned()),
            ("SUBSYSTEM".to_owned(), "input".to_owned()),
        ]),
        "only the kernel's own: its path, its subsystem and its uevent's"
    );

    let mouse = &listing.inputs[1];
    assert!(mouse.tagged);
    assert_eq!(mouse.classes, Classes::empty().with(Class::Mouse));
    assert_eq!(
        mouse.id_path.as_deref(),
        Some("pci-0000:00:14.0-usb-0:3:1.1")
    );
    assert_eq!(
        (
            mouse.vendor_id.as_deref(),
            mouse.model_id.as_deref(),
            mouse.serial_short.as_deref()
        ),
        (Some("046d"), Some("c52b"), Some("M1"))
    );
    assert_eq!(
        mouse.properties.get("VTIUM_HEAD").map(String::as_str),
        Some("panel")
    );
    assert_eq!(
        mouse.properties.get("MINOR").map(String::as_str),
        Some("68")
    );

    let edid = crate::display::Edid::parse(&edid(7)).unwrap().id;
    assert_eq!(edid.serial(), &Serial::Number(7));
    let display = |connector: &str, edid| Identity {
        connector: connector.to_owned(),
        card: Some("pci-0000:03:00.0".to_owned()),
        edid,
        lit_at_start: false,
    };
    assert_eq!(
        listing.cards,
        [Card {
            node: PathBuf::from("/dev/dri/card1"),
            id_path: Some("pci-0000:03:00.0".to_owned()),
            primary: true,
            displays: vec![display("DP-2", Some(edid)), display("DP-1", None)],
        }],
        "the connected connectors, by connector id; no render node"
    );
}

// the same rule as the monitor's: a card is primary by its PCI parent's
// boot_vga, else when it sits on the platform bus and not on USB
#[test]
fn a_card_is_primary_by_boot_vga_else_on_the_platform_bus_off_usb() {
    let t = Tree::new("primary");
    t.bus_device("pci0000:00/0000:01:00.0", "pci");
    t.file("devices/pci0000:00/0000:01:00.0/boot_vga", "0\n");
    t.card("pci0000:00/0000:01:00.0", "card0", 0, &[]);
    t.bus_device("platform/display", "platform");
    t.card("platform/display", "card2", 2, &[]);
    t.bus_device("platform/usb/1-1", "usb");
    t.card("platform/usb/1-1", "card3", 3, &[]);
    let primary: Vec<(PathBuf, bool)> = list(&t.roots(), "seat0")
        .cards
        .into_iter()
        .map(|c| (c.node, c.primary))
        .collect();
    assert_eq!(
        primary,
        [
            (PathBuf::from("/dev/dri/card0"), false),
            (PathBuf::from("/dev/dri/card2"), true),
            (PathBuf::from("/dev/dri/card3"), false),
        ]
    );
}

// ADR-1401 D4: an attribute past UDEV_PROPERTY_MAX is left out, never cut
// short; a missing sysfs or database lists nothing
#[test]
fn an_attribute_past_its_bound_is_left_out_and_no_sysfs_lists_nothing() {
    let t = Tree::new("bound");
    let long = "n".repeat(UDEV_PROPERTY_MAX.saturating_add(1));
    t.input(USB, "input1", "event0", 64, &[("name", long.as_str())]);
    let listing = list(&t.roots(), "seat0");
    assert_eq!(listing.inputs.len(), 1);
    assert_eq!(listing.inputs[0].name, None);
    let none = Roots {
        sys: Path::new("/nonexistent/vtium/sys").to_owned(),
        udev_data: Path::new("/nonexistent/vtium/udev").to_owned(),
    };
    assert_eq!(list(&none, "seat0"), super::Listing::default());
}
