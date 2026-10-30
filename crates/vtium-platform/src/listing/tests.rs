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

/// A connector: its name, status, modes, connector id and EDID.
type Connector<'a> = (&'a str, &'a str, &'a str, u32, Option<Vec<u8>>);

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

    /// udev's database entry `name`, its text as udev wrote it.
    fn udev_raw(&self, name: &str, text: &str) {
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
    /// `connectors` as `(name, status, modes, connector id, EDID)`.
    fn card(&self, parent: &str, card: &str, minor: u32, connectors: &[Connector]) {
        let dir = format!("devices/{parent}/drm/{card}");
        self.file(&format!("{dir}/dev"), format!("226:{minor}\n"));
        self.file(
            &format!("{dir}/uevent"),
            format!("MAJOR=226\nMINOR={minor}\nDEVNAME=dri/{card}\nDEVTYPE=drm_minor\n"),
        );
        self.link(&format!("{dir}/device"), "../..");
        for (name, status, modes, id, edid) in connectors {
            let conn = format!("{dir}/{card}-{name}");
            self.file(&format!("{conn}/status"), format!("{status}\n"));
            self.file(&format!("{conn}/connector_id"), format!("{id}\n"));
            self.file(&format!("{conn}/edid"), edid.clone().unwrap_or_default());
            self.file(&format!("{conn}/modes"), modes);
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

/// A machine of one GPU with four connectors, two of them lit, a keyboard udev has not
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
            ("DP-1", "connected", "1920x1080\n", 95, None),
            (
                "DP-2",
                "connected",
                "2560x1440\n1920x1080\n",
                90,
                Some(edid(7)),
            ),
            ("DP-3", "connected", "", 97, None),
            ("HDMI-A-1", "disconnected", "", 99, None),
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
            ("DEVNAME".to_owned(), "/dev/input/event3".to_owned()),
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
        "the connected connectors with modes, by connector id; no render node"
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

/// A Logitech receiver's keys on the operator's machine, as udev recorded
/// them under systemd 262: its database entry, and the properties
/// `udevadm info --query=property --name=/dev/input/event3` printed. udevadm
/// joins `DEVLINKS`, `TAGS` and `CURRENT_TAGS` in a hash order that
/// changes from run to run; here they are sorted. The unit
/// has no serial of its own: `ID_SERIAL` is its model's.
const RECEIVER_ENTRY: &str = "S:input/by-id/usb-Logitech_USB_Receiver-event-if01
S:input/by-path/pci-0000:13:00.0-usb-0:1:1.1-event
S:input/by-path/pci-0000:13:00.0-usbv2-0:1:1.1-event
I:11014048
E:ID_INPUT=1
E:ID_INPUT_KEY=1
E:ID_BUS=usb
E:ID_MODEL=USB_Receiver
E:ID_MODEL_ENC=USB\\x20Receiver
E:ID_MODEL_ID=c548
E:ID_SERIAL=Logitech_USB_Receiver
E:ID_VENDOR=Logitech
E:ID_VENDOR_ENC=Logitech
E:ID_VENDOR_ID=046d
E:ID_REVISION=0505
E:ID_TYPE=hid
E:ID_USB_MODEL=USB_Receiver
E:ID_USB_MODEL_ENC=USB\\x20Receiver
E:ID_USB_MODEL_ID=c548
E:ID_USB_SERIAL=Logitech_USB_Receiver
E:ID_USB_VENDOR=Logitech
E:ID_USB_VENDOR_ENC=Logitech
E:ID_USB_VENDOR_ID=046d
E:ID_USB_REVISION=0505
E:ID_USB_TYPE=hid
E:ID_USB_INTERFACES=:030101:030102:030000:
E:ID_USB_INTERFACE_NUM=01
E:ID_USB_DRIVER=usbhid
E:ID_PATH_WITH_USB_REVISION=pci-0000:13:00.0-usbv2-0:1:1.1
E:ID_PATH=pci-0000:13:00.0-usb-0:1:1.1
E:ID_PATH_TAG=pci-0000_13_00_0-usb-0_1_1_1
E:ID_INTEGRATION=external
E:ID_FOR_SEAT=input-pci-0000_13_00_0-usb-0_1_1_1
E:LIBINPUT_DEVICE_GROUP=3/46d/c548:usb-0000:13:00.0-1
G:seat
G:uaccess
G:Logitech_HID_20
G:power-switch
Q:seat
Q:uaccess
Q:Logitech_HID_20
Q:power-switch
V:1
";

const RECEIVER_PROPERTIES: [(&str, &str); 39] = [
    (
        "DEVPATH",
        "/devices/pci0000:00/0000:00:02.1/0000:05:00.0/0000:06:08.0/0000:0b:00.0/0000:0c:0c.0/0000:13:00.0/usb6/6-1/6-1:1.1/0003:046D:C548.0002/input/input3/event3",
    ),
    ("DEVNAME", "/dev/input/event3"),
    ("MAJOR", "13"),
    ("MINOR", "67"),
    ("SUBSYSTEM", "input"),
    ("USEC_INITIALIZED", "11014048"),
    ("ID_INPUT", "1"),
    ("ID_INPUT_KEY", "1"),
    ("ID_BUS", "usb"),
    ("ID_MODEL", "USB_Receiver"),
    ("ID_MODEL_ENC", "USB\\x20Receiver"),
    ("ID_MODEL_ID", "c548"),
    ("ID_SERIAL", "Logitech_USB_Receiver"),
    ("ID_VENDOR", "Logitech"),
    ("ID_VENDOR_ENC", "Logitech"),
    ("ID_VENDOR_ID", "046d"),
    ("ID_REVISION", "0505"),
    ("ID_TYPE", "hid"),
    ("ID_USB_MODEL", "USB_Receiver"),
    ("ID_USB_MODEL_ENC", "USB\\x20Receiver"),
    ("ID_USB_MODEL_ID", "c548"),
    ("ID_USB_SERIAL", "Logitech_USB_Receiver"),
    ("ID_USB_VENDOR", "Logitech"),
    ("ID_USB_VENDOR_ENC", "Logitech"),
    ("ID_USB_VENDOR_ID", "046d"),
    ("ID_USB_REVISION", "0505"),
    ("ID_USB_TYPE", "hid"),
    ("ID_USB_INTERFACES", ":030101:030102:030000:"),
    ("ID_USB_INTERFACE_NUM", "01"),
    ("ID_USB_DRIVER", "usbhid"),
    (
        "ID_PATH_WITH_USB_REVISION",
        "pci-0000:13:00.0-usbv2-0:1:1.1",
    ),
    ("ID_PATH", "pci-0000:13:00.0-usb-0:1:1.1"),
    ("ID_PATH_TAG", "pci-0000_13_00_0-usb-0_1_1_1"),
    ("ID_INTEGRATION", "external"),
    ("ID_FOR_SEAT", "input-pci-0000_13_00_0-usb-0_1_1_1"),
    ("LIBINPUT_DEVICE_GROUP", "3/46d/c548:usb-0000:13:00.0-1"),
    (
        "DEVLINKS",
        "/dev/input/by-id/usb-Logitech_USB_Receiver-event-if01 /dev/input/by-path/pci-0000:13:00.0-usb-0:1:1.1-event /dev/input/by-path/pci-0000:13:00.0-usbv2-0:1:1.1-event",
    ),
    ("TAGS", ":Logitech_HID_20:power-switch:seat:uaccess:"),
    (
        "CURRENT_TAGS",
        ":Logitech_HID_20:power-switch:seat:uaccess:",
    ),
];

// ADR-1401 D4: an input's properties are libudev's, as `udevadm info`
// prints them, so a `udev:` rule matches in check-config what it matches
// at start: DEVNAME under /dev, DEVLINKS, TAGS and USEC_INITIALIZED
#[test]
fn an_inputs_properties_are_the_ones_udevadm_prints() {
    let t = Tree::new("receiver");
    let parent = "pci0000:00/0000:00:02.1/0000:05:00.0/0000:06:08.0/0000:0b:00.0/0000:0c:0c.0/0000:13:00.0/usb6/6-1/6-1:1.1/0003:046D:C548.0002";
    t.input(
        parent,
        "input3",
        "event3",
        67,
        &[("name", "Logitech USB Receiver")],
    );
    t.udev_raw("c13:67", RECEIVER_ENTRY);
    let listing = list(&t.roots(), "seat0");
    let expected: BTreeMap<String, String> = RECEIVER_PROPERTIES
        .iter()
        .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
        .collect();
    assert_eq!(listing.inputs.len(), 1);
    assert_eq!(listing.inputs[0].properties, expected);
}

// ADR-1401 D3, determinism: a `udev:` glob on a multi-valued property
// places a device the same way whatever order udev recorded the links in
#[test]
fn the_links_and_tags_are_sorted_whatever_order_the_entry_has_them() {
    let one = "S:input/by-path/b\nS:input/by-id/a\nG:seat\nG:power-switch\nQ:uaccess\nQ:seat\n";
    let two = "S:input/by-id/a\nS:input/by-path/b\nG:power-switch\nG:seat\nQ:seat\nQ:uaccess\n";
    let props = |name: &str, entry: &str| {
        let t = Tree::new(name);
        t.input(USB, "input1", "event0", 64, &[("name", "K")]);
        t.udev_raw("c13:64", entry);
        list(&t.roots(), "seat0").inputs[0].properties.clone()
    };
    let (a, b) = (props("order-one", one), props("order-two", two));
    assert_eq!(a, b);
    assert_eq!(
        a.get("DEVLINKS").map(String::as_str),
        Some("/dev/input/by-id/a /dev/input/by-path/b")
    );
    assert_eq!(
        a.get("TAGS").map(String::as_str),
        Some(":power-switch:seat:")
    );
    assert_eq!(
        a.get("CURRENT_TAGS").map(String::as_str),
        Some(":seat:uaccess:")
    );
}

// a database property replaces the kernel's of the same name, as in
// sd-device
#[test]
fn a_database_property_replaces_the_kernels() {
    let t = Tree::new("replace");
    t.input(USB, "input1", "event0", 64, &[("name", "K")]);
    t.udev("c13:64", &["MINOR=99", "ID_INPUT=1"]);
    let listing = list(&t.roots(), "seat0");
    let p = &listing.inputs[0].properties;
    assert_eq!(p.get("MINOR").map(String::as_str), Some("99"));
}

// two connectors of one connector id sort by name in natural order
#[test]
fn connectors_of_one_id_sort_by_name_in_natural_order() {
    let t = Tree::new("natural");
    t.bus_device(GPU, "pci");
    t.card(
        GPU,
        "card0",
        0,
        &[
            ("DP-10", "connected", "640x480\n", 0, None),
            ("DP-2", "connected", "640x480\n", 0, None),
            ("DP-1", "connected", "640x480\n", 1, None),
        ],
    );
    let names: Vec<String> = list(&t.roots(), "seat0").cards[0]
        .displays
        .iter()
        .map(|d| d.connector.clone())
        .collect();
    assert_eq!(names, ["DP-2", "DP-10", "DP-1"]);
}

// a framebuffer no DRM driver backs is listed by the monitor's rule, read
// from its sysfs name, without opening /dev/fbN
#[test]
fn a_framebuffer_no_drm_driver_backs_is_listed_by_its_sysfs_name() {
    let t = Tree::new("fb");
    for (fb, id) in [
        ("fb0", "VESA VGA"),
        ("fb1", "amdgpudrmfb"),
        ("fb2", "udlfb"),
        ("fb3", "nouveaufb"),
    ] {
        let minor = fb.trim_start_matches("fb");
        let dir = format!("devices/platform/{fb}/graphics/{fb}");
        t.file(&format!("{dir}/dev"), format!("29:{minor}\n"));
        t.file(
            &format!("{dir}/uevent"),
            format!("MAJOR=29\nMINOR={minor}\nDEVNAME={fb}\n"),
        );
        t.file(&format!("{dir}/name"), format!("{id}\n"));
        t.link(&format!("class/graphics/{fb}"), &format!("../../{dir}"));
    }
    let dir = "devices/platform/fb4/graphics/fb4";
    t.file(&format!("{dir}/dev"), "29:4\n");
    t.link("class/graphics/fb4", &format!("../../{dir}"));
    t.udev("c29:2", &["ID_SEAT=seat1"]);
    let framebuffers: Vec<(PathBuf, bool)> = list(&t.roots(), "seat0")
        .framebuffers
        .into_iter()
        .map(|f| (f.node, f.primary))
        .collect();
    assert_eq!(
        framebuffers,
        [(PathBuf::from("/dev/fb0"), true)],
        "DRM-backed ones, another seat's and one with no name are left out"
    );
}
