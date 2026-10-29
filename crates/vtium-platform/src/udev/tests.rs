//! The monitor against recorded udev streams: a scan at start, then the
//! hotplug events after it.

#![expect(clippy::panic, reason = "a test fails by panicking")]

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use super::{
    Action, Device, DeviceEvent, DeviceFlags, DeviceKind, InputIdentity, Monitor, Probe, Udev,
    Uevent, attributes, properties,
};
use crate::bounds::{UDEV_PROPERTIES_MAX, UDEV_PROPERTY_MAX};
use crate::input::{Class, Classes};

/// Answers for the cards and framebuffers by node; any other node fails,
/// as an open would.
#[derive(Default)]
struct Answers {
    drivers: BTreeMap<PathBuf, String>,
    fb_ids: BTreeMap<PathBuf, String>,
    asked: Vec<PathBuf>,
}

impl Probe for Answers {
    fn drm_driver(&mut self, node: &Path) -> io::Result<String> {
        self.asked.push(node.to_owned());
        self.drivers
            .get(node)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::PermissionDenied))
    }

    fn fb_id(&mut self, node: &Path) -> io::Result<String> {
        self.asked.push(node.to_owned());
        self.fb_ids
            .get(node)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::PermissionDenied))
    }
}

fn device(subsystem: &str, sysname: &str, devnode: Option<&str>) -> Uevent {
    Uevent {
        subsystem: Some(subsystem.to_owned()),
        sysname: sysname.to_owned(),
        syspath: format!("/sys/class/{subsystem}/{sysname}"),
        devnode: devnode.map(PathBuf::from),
        ..Uevent::default()
    }
}

/// The boot GPU: a PCI card with `boot_vga`.
fn card0() -> Uevent {
    Uevent {
        id_path: Some(String::from("pci-0000:03:00.0")),
        pci_boot_vga: Some(true),
        ..device("drm", "card0", Some("/dev/dri/card0"))
    }
}

/// A USB display on DisplayLink's driver.
fn card1() -> Uevent {
    Uevent {
        id_path: Some(String::from("pci-0000:00:14.0-usb-0:2:1.0")),
        pci_boot_vga: Some(false),
        usb_parent: true,
        ..device("drm", "card1", Some("/dev/dri/card1"))
    }
}

fn keyboard() -> Uevent {
    Uevent {
        input_parent: true,
        ..device("input", "event3", Some("/dev/input/event3"))
    }
}

/// The devices a machine with one PCI card, its connector, its render
/// node, a framebuffer the card backs, a keyboard and a mouse lists, with
/// the nodes the monitor skips among them.
fn machine() -> Vec<Uevent> {
    vec![
        card0(),
        device("drm", "card0-HDMI-A-1", None),
        device("drm", "renderD128", Some("/dev/dri/renderD128")),
        device("graphics", "fb0", Some("/dev/fb0")),
        device("graphics", "fbcon", None),
        Uevent {
            input_parent: true,
            ..device("input", "input3", None)
        },
        keyboard(),
        Uevent {
            input_parent: true,
            ..device("input", "mouse0", Some("/dev/input/mouse0"))
        },
        Uevent {
            input_parent: true,
            ..device("input", "event4", Some("/dev/input/event4"))
        },
    ]
}

fn answers() -> Answers {
    let mut a = Answers::default();
    a.drivers
        .insert(PathBuf::from("/dev/dri/card0"), String::from("amdgpu"));
    a.drivers
        .insert(PathBuf::from("/dev/dri/card1"), String::from("udl"));
    a.fb_ids
        .insert(PathBuf::from("/dev/fb0"), String::from("amdgpudrmfb"));
    a
}

fn added(kind: DeviceKind, node: &str, flags: DeviceFlags, id_path: Option<&str>) -> DeviceEvent {
    DeviceEvent::Added(Device {
        kind,
        node: PathBuf::from(node),
        flags,
        id_path: id_path.map(String::from),
        input: (kind == DeviceKind::Input).then(Box::default),
    })
}

// covers: platform.vt-switch
#[test]
fn the_scan_reports_the_seats_cards_framebuffers_and_event_nodes() {
    let mut monitor = Monitor::new("seat0");
    let mut probe = answers();
    for dev in machine() {
        monitor.add(&dev, &mut probe);
    }
    assert_eq!(
        monitor.events().collect::<Vec<_>>(),
        [
            added(
                DeviceKind::Drm,
                "/dev/dri/card0",
                DeviceFlags {
                    primary: true,
                    ..DeviceFlags::default()
                },
                Some("pci-0000:03:00.0"),
            ),
            added(
                DeviceKind::Fbdev,
                "/dev/fb0",
                DeviceFlags {
                    drm_backed: true,
                    ..DeviceFlags::default()
                },
                None,
            ),
            added(
                DeviceKind::Input,
                "/dev/input/event3",
                DeviceFlags::default(),
                None
            ),
            added(
                DeviceKind::Input,
                "/dev/input/event4",
                DeviceFlags::default(),
                None
            ),
        ]
    );
    // Only the card and the framebuffer were opened to read their flags.
    assert_eq!(
        probe.asked,
        [PathBuf::from("/dev/dri/card0"), PathBuf::from("/dev/fb0")]
    );
    // A second scan, as a monitor event racing the scan, adds nothing.
    for dev in machine() {
        monitor.add(&dev, &mut probe);
    }
    assert_eq!(monitor.events().count(), 0);
}

#[test]
fn devices_of_another_seat_are_skipped_unopened() {
    let mut monitor = Monitor::new("seat0");
    let mut probe = answers();
    let mut other = card1();
    other.seat = Some(String::from("seat1"));
    monitor.add(&other, &mut probe);
    let mut kbd = keyboard();
    kbd.seat = Some(String::from("seat1"));
    monitor.add(&kbd, &mut probe);
    assert_eq!(monitor.events().count(), 0);
    assert!(probe.asked.is_empty());
    // The other seat sees them.
    let mut seat1 = Monitor::new("seat1");
    seat1.add(&other, &mut probe);
    seat1.add(&kbd, &mut probe);
    assert_eq!(seat1.events().count(), 2);
}

#[test]
fn a_usb_display_is_auxiliary_and_a_platform_controller_primary() {
    let mut monitor = Monitor::new("seat0");
    let mut probe = answers();
    monitor.add(&card1(), &mut probe);
    let soc = Uevent {
        platform_parent: true,
        ..device("drm", "card2", Some("/dev/dri/card2"))
    };
    monitor.add(&soc, &mut probe);
    let soc_usb = Uevent {
        platform_parent: true,
        usb_parent: true,
        ..device("drm", "card3", Some("/dev/dri/card3"))
    };
    monitor.add(&soc_usb, &mut probe);
    let flags: Vec<DeviceFlags> = monitor
        .events()
        .filter_map(|e| match e {
            DeviceEvent::Added(d) => Some(d.flags),
            _ => None,
        })
        .collect();
    assert_eq!(
        flags,
        [
            DeviceFlags {
                aux: true,
                ..DeviceFlags::default()
            },
            DeviceFlags {
                primary: true,
                ..DeviceFlags::default()
            },
            DeviceFlags::default(),
        ]
    );
}

#[test]
fn a_framebuffer_is_drm_backed_unless_its_id_says_otherwise() {
    let cases = [
        ("amdgpudrmfb", true, false, false),
        ("nouveaufb", true, false, false),
        ("psbfb", true, false, false),
        ("efifb", false, false, false),
        ("udlfb", false, true, false),
        ("VESA VGA", false, false, true),
    ];
    for (id, drm_backed, aux, primary) in cases {
        let mut monitor = Monitor::new("seat0");
        let mut probe = Answers::default();
        probe
            .fb_ids
            .insert(PathBuf::from("/dev/fb0"), String::from(id));
        monitor.add(&device("graphics", "fb0", Some("/dev/fb0")), &mut probe);
        let flags = monitor.events().find_map(|e| match e {
            DeviceEvent::Added(d) => Some(d.flags),
            _ => None,
        });
        assert_eq!(
            flags,
            Some(DeviceFlags {
                primary,
                aux,
                drm_backed
            }),
            "{id}"
        );
    }
    // A framebuffer that cannot be read may be a card's: it is DRM-backed.
    let mut monitor = Monitor::new("seat0");
    monitor.add(
        &device("graphics", "fb1", Some("/dev/fb1")),
        &mut Answers::default(),
    );
    let flags = monitor.events().find_map(|e| match e {
        DeviceEvent::Added(d) => Some(d.flags),
        _ => None,
    });
    assert_eq!(flags.map(|f| f.drm_backed), Some(true));
}

#[test]
fn hotplug_events_add_remove_and_poll_the_seats_devices() {
    let mut monitor = Monitor::new("seat0");
    let mut probe = answers();
    for dev in machine() {
        monitor.add(&dev, &mut probe);
    }
    monitor.events().for_each(drop);
    let hotplug = Uevent {
        hotplug: true,
        ..card0()
    };
    let stream = [
        (Action::Change, hotplug.clone()),
        // A change without HOTPLUG asks nothing.
        (Action::Change, card0()),
        (Action::Add, card1()),
        (Action::Remove, keyboard()),
        // A remove of a node never added is ignored.
        (
            Action::Remove,
            device("input", "event9", Some("/dev/input/event9")),
        ),
        (Action::Other, card1()),
        (Action::Add, keyboard()),
    ];
    for (action, dev) in &stream {
        monitor.handle(*action, dev, &mut probe);
    }
    assert_eq!(
        monitor.events().collect::<Vec<_>>(),
        [
            DeviceEvent::Hotplug {
                kind: DeviceKind::Drm,
                node: PathBuf::from("/dev/dri/card0"),
            },
            added(
                DeviceKind::Drm,
                "/dev/dri/card1",
                DeviceFlags {
                    aux: true,
                    ..DeviceFlags::default()
                },
                Some("pci-0000:00:14.0-usb-0:2:1.0"),
            ),
            DeviceEvent::Removed {
                kind: DeviceKind::Input,
                node: PathBuf::from("/dev/input/event3"),
            },
            added(
                DeviceKind::Input,
                "/dev/input/event3",
                DeviceFlags::default(),
                None
            ),
        ]
    );
}

#[test]
fn a_device_that_changes_seat_leaves_or_joins() {
    let mut monitor = Monitor::new("seat0");
    let mut probe = answers();
    monitor.handle(Action::Add, &keyboard(), &mut probe);
    monitor.events().for_each(drop);
    let mut moved = keyboard();
    moved.seat = Some(String::from("seat1"));
    monitor.handle(Action::Change, &moved, &mut probe);
    assert_eq!(
        monitor.events().collect::<Vec<_>>(),
        [DeviceEvent::Removed {
            kind: DeviceKind::Input,
            node: PathBuf::from("/dev/input/event3"),
        }]
    );
    // A change on an unknown device that is now this seat's adds it.
    monitor.handle(Action::Change, &keyboard(), &mut probe);
    assert_eq!(
        monitor.events().collect::<Vec<_>>(),
        [added(
            DeviceKind::Input,
            "/dev/input/event3",
            DeviceFlags::default(),
            None
        )]
    );
}

#[test]
fn a_change_keeps_an_event_node_on_its_parents_seat() {
    let mut monitor = Monitor::new("seat1");
    let mut kbd = keyboard();
    kbd.seat = Some(String::from("seat1"));
    monitor.handle(Action::Add, &kbd, &mut Answers::default());
    assert_eq!(monitor.events().count(), 1);
    // The C reference read the node's own ID_SEAT at a change, unset on an
    // event node, and removed and added the keyboard again.
    monitor.handle(Action::Change, &kbd, &mut Answers::default());
    assert_eq!(monitor.events().count(), 0);
}

#[test]
fn an_event_node_needs_its_input_parent() {
    let mut monitor = Monitor::new("seat0");
    let orphan = device("input", "event5", Some("/dev/input/event5"));
    monitor.add(&orphan, &mut Answers::default());
    assert_eq!(monitor.events().count(), 0);
}

/// libudev on the machine running the tests: the scan reads sysfs and
/// udev's database and opens no device, as the stand-in answers for the
/// cards and framebuffers.
#[test]
fn the_scan_reads_this_machines_devices_through_libudev() {
    let mut udev = Udev::new("seat0").unwrap_or_else(|e| panic!("the udev monitor: {e}"));
    let mut probe = Answers::default();
    udev.scan_with(&mut probe)
        .unwrap_or_else(|e| panic!("the scan: {e}"));
    let events: Vec<DeviceEvent> = udev.events().collect();
    for event in &events {
        let DeviceEvent::Added(d) = event else {
            panic!("a scan only adds: {event:?}");
        };
        let name = d.node.to_string_lossy();
        let prefix = match d.kind {
            DeviceKind::Drm => "/dev/dri/card",
            DeviceKind::Fbdev => "/dev/fb",
            DeviceKind::Input => "/dev/input/event",
        };
        assert!(name.starts_with(prefix), "{name} as {:?}", d.kind);
    }
    // The cards and framebuffers found were each asked about once.
    let cards = events
        .iter()
        .filter(|e| matches!(e, DeviceEvent::Added(d) if d.kind != DeviceKind::Input))
        .count();
    assert_eq!(probe.asked.len(), cards);
    // Nothing is waiting on the monitor socket that the loop would miss.
    udev.receive();
}

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// The numpad of ADR-1401's Evidence, its keyboard interface, as sysfs
/// and udev describe it; its serial stands in for the real one.
fn numpad(tagged: bool) -> Uevent {
    let mut properties = map(&[
        ("ID_INPUT", "1"),
        ("ID_INPUT_KEY", "1"),
        ("ID_INPUT_KEYBOARD", "1"),
        ("ID_PATH", "pci-0000:15:00.0-usb-0:3.4.1:1.1"),
        ("ID_VENDOR_ID", "2dc8"),
        ("ID_MODEL_ID", "5204"),
        ("ID_SERIAL_SHORT", "0000000001"),
        ("VTIUM_ROLE", "numpad"),
    ]);
    if !tagged {
        properties.clear();
    }
    Uevent {
        input_parent: true,
        properties,
        attributes: map(&[
            ("name", "8BitDo Retro 18 Numpad Keyboard"),
            ("uniq", "0000000001"),
            ("id/vendor", "2dc8"),
            ("id/product", "5204"),
            ("capabilities/ev", "12001f"),
            (
                "capabilities/key",
                "733eff 0 0 483ffff17aff32d bfd4444600000000 1 130ff38b17c007 ffff7bfad9415fff febeffdfffefffff fffffffffffffffe",
            ),
            ("capabilities/rel", "1040"),
            ("capabilities/abs", "100000000"),
            ("properties", "0"),
        ]),
        ..device("input", "event24", Some("/dev/input/event24"))
    }
}

fn identity(dev: &Uevent) -> InputIdentity {
    let mut monitor = Monitor::new("seat0");
    monitor.add(dev, &mut answers());
    match monitor.events().next() {
        Some(DeviceEvent::Added(Device {
            input: Some(input), ..
        })) => *input,
        other => panic!("not an input device: {other:?}"),
    }
}

fn keyboard_classes() -> Classes {
    Classes::empty().with(Class::Keyboard).with(Class::Key)
}

#[test]
fn an_event_node_carries_its_identity_from_udev_and_sysfs() {
    let id = identity(&numpad(true));
    assert_eq!(id.classes, keyboard_classes());
    assert!(id.tagged);
    assert_eq!(
        id.id_path.as_deref(),
        Some("pci-0000:15:00.0-usb-0:3.4.1:1.1")
    );
    assert_eq!(
        (
            id.vendor_id.as_deref(),
            id.model_id.as_deref(),
            id.serial_short.as_deref()
        ),
        (Some("2dc8"), Some("5204"), Some("0000000001"))
    );
    assert_eq!(id.uniq.as_deref(), Some("0000000001"));
    assert_eq!(id.id, Some((0x2dc8, 0x5204)));
    assert_eq!(id.name.as_deref(), Some("8BitDo Retro 18 Numpad Keyboard"));
    assert_eq!(
        id.properties.get("VTIUM_ROLE").map(String::as_str),
        Some("numpad")
    );
}

#[test]
fn without_a_udev_daemon_an_event_node_is_classed_from_its_bitmaps() {
    let id = identity(&numpad(false));
    assert_eq!(id.classes, keyboard_classes());
    assert!(!id.tagged);
    assert_eq!(id.id_path, None);
    assert_eq!(id.serial_short, None);
    assert!(id.properties.is_empty());
    // What sysfs shows still identifies it.
    assert_eq!(id.id, Some((0x2dc8, 0x5204)));
    assert_eq!(id.name.as_deref(), Some("8BitDo Retro 18 Numpad Keyboard"));
}

#[test]
fn a_hwdb_correction_reaches_an_event_nodes_classes() {
    let mut dev = numpad(true);
    dev.properties
        .insert(String::from("ID_INPUT_KEYBOARD"), String::from("0"));
    assert_eq!(identity(&dev).classes, Classes::empty().with(Class::Key));
}

#[test]
fn a_card_carries_no_input_identity() {
    let mut monitor = Monitor::new("seat0");
    monitor.add(&card0(), &mut answers());
    assert!(matches!(
        monitor.events().next(),
        Some(DeviceEvent::Added(Device { input: None, .. }))
    ));
}

#[test]
fn an_event_nodes_properties_are_bounded_and_its_named_ones_kept() {
    let many = (0..UDEV_PROPERTIES_MAX + 10).map(|i| (format!("P{i:04}"), String::from("1")));
    let long = std::iter::once((String::from("LONG"), "x".repeat(UDEV_PROPERTY_MAX)));
    let named = |name: &str| match name {
        "ID_INPUT" | "ID_INPUT_KEYBOARD" => Some(String::from("1")),
        "ID_PATH" => Some(String::from("pci-0000:15:00.0-usb-0:3.2:1.0")),
        _ => None,
    };
    let kept = properties(named, long.chain(many));
    assert_eq!(kept.len(), UDEV_PROPERTIES_MAX);
    for name in ["ID_INPUT", "ID_INPUT_KEYBOARD", "ID_PATH", "P0000"] {
        assert!(kept.contains_key(name), "{name}");
    }
    assert!(!kept.contains_key("LONG"));
    assert!(!kept.contains_key(&format!("P{:04}", UDEV_PROPERTIES_MAX)));
}

#[test]
fn an_input_devices_sysfs_attributes_are_bounded() {
    let read = |name: &str| match name {
        "name" => Some("n".repeat(UDEV_PROPERTY_MAX)),
        "uniq" => Some("u".repeat(UDEV_PROPERTY_MAX + 1)),
        "id/vendor" => Some(String::from("2dc8")),
        _ => None,
    };
    let kept = attributes(read);
    assert_eq!(
        kept.keys().map(String::as_str).collect::<Vec<_>>(),
        ["id/vendor", "name"]
    );
    assert_eq!(kept.get("name").map(String::len), Some(UDEV_PROPERTY_MAX));
}
