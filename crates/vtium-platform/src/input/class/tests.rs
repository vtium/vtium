//! The classes from udev's tags against the classes from the bitmaps.

#![expect(clippy::panic, reason = "a test fails by panicking")]

use super::{Bitmaps, Class, Classes, parse_bitmap};
use crate::ioctl::{KeyBits, bit_is_set};

/// A device as sysfs and `udevadm info` showed it: `capabilities/ev`,
/// `/key`, `/rel`, `/abs`, `properties`, and the `ID_INPUT_*` tags udev
/// set, each without its prefix.
struct Recorded {
    role: &'static str,
    ev: &'static str,
    key: &'static str,
    rel: &'static str,
    abs: &'static str,
    props: &'static str,
    tags: &'static str,
}

const fn dev(role: &'static str, [ev, key, rel, abs, props, tags]: [&'static str; 6]) -> Recorded {
    Recorded {
        role,
        ev,
        key,
        rel,
        abs,
        props,
        tags,
    }
}

/// Every event node of the operator's machine, read on 2026-10-29 from
/// sysfs and `udevadm info` without privilege, under systemd 262's udev:
/// ADR-1401's Evidence devices and the rest of the machine.
const MACHINE: &[Recorded] = &[
    dev(
        "keyboard A",
        [
            "12001f",
            "3f00733fff 0 0 483ffff17aff32d bfd4444600000000 1 130ff38b17d007 ffff7bfad941dfff ffbeffdfffefffff fffffffffffffffe",
            "1040",
            "100000000",
            "0",
            "KEY KEYBOARD",
        ],
    ),
    dev(
        "keyboard A's pointer, interface 1.0",
        ["17", "1f0000 0 0 0 0", "1943", "0", "0", "MOUSE"],
    ),
    dev(
        "the numpad with letters",
        [
            "12001f",
            "733eff 0 0 483ffff17aff32d bfd4444600000000 1 130ff38b17c007 ffff7bfad9415fff febeffdfffefffff fffffffffffffffe",
            "1040",
            "100000000",
            "0",
            "KEY KEYBOARD",
        ],
    ),
    dev(
        "the numpad's pointer",
        ["17", "1f0000 0 0 0 0", "1943", "0", "0", "MOUSE"],
    ),
    dev(
        "the OTP token",
        [
            "120013",
            "e080ffdf01cfffff fffffffffffffffe",
            "0",
            "0",
            "0",
            "KEY KEYBOARD",
        ],
    ),
    dev(
        "keyboard B, interface 1.0",
        [
            "120013",
            "10000 0 0 0 101007b02001007 ff9f207ac14057ff ffbeffdfffefffff fffffffffffffffe",
            "0",
            "0",
            "0",
            "KEY KEYBOARD",
        ],
    ),
    dev(
        "keyboard B, interface 1.1, without Left Control",
        [
            "10001f",
            "3f00733fff 0 0 483ffff17aff32d bfd4444600000000 1 130ff38b17d007 ffe77bfad941dfff 9fbeffcdffefffff febffbffdffffffe",
            "1040",
            "10100000000",
            "0",
            "KEY",
        ],
    ),
    dev(
        "keyboard B, interface 1.2",
        ["17", "1f0000 0 0 0 0", "1943", "0", "0", "MOUSE"],
    ),
    dev(
        "the mouse",
        ["17", "ffff0000 0 0 0 0", "1943", "0", "0", "MOUSE"],
    ),
    dev(
        "the receiver's keyboard",
        [
            "120013",
            "1000000000007 ff9f207ac14057ff febeffdfffefffff fffffffffffffffe",
            "0",
            "0",
            "0",
            "KEY KEYBOARD",
        ],
    ),
    dev(
        "the receiver's mouse",
        ["17", "ffff0000 0 0 0 0", "1943", "0", "0", "MOUSE"],
    ),
    dev(
        "the receiver's consumer controls",
        [
            "1f",
            "3f00733fff 0 0 483ffff17aff32d bfd4444600000000 1 130ff38b17d000 677bfad9415fed 19ed68000004400 10000002",
            "1040",
            "100000000",
            "0",
            "KEY",
        ],
    ),
    dev(
        "the receiver's system controls",
        [
            "13",
            "8000 0 0 0 0 0 0 c000 10000000000000 0",
            "0",
            "0",
            "0",
            "KEY",
        ],
    ),
    dev(
        "the receiver's button node",
        ["13", "1 0 0 0 0", "0", "0", "0", ""],
    ),
    dev(
        "the receiver's axis node",
        ["9", "0", "0", "10000000000", "0", ""],
    ),
    dev(
        "the receiver's touchpad",
        [
            "1b",
            "e520 10000 0 0 0 0",
            "0",
            "6e0800001000003",
            "5",
            "TOUCHPAD",
        ],
    ),
    dev(
        "the microphone's keys",
        [
            "13",
            "10000 7800000000 e000000000000 0",
            "0",
            "0",
            "0",
            "KEY",
        ],
    ),
    dev(
        "the headphones' AVRCP node",
        [
            "3",
            "2fc800 145200000000 0 10300 49e800000c00 e16800000000f f810000010000ffc",
            "0",
            "0",
            "0",
            "KEY",
        ],
    ),
    dev(
        "the power button",
        ["3", "8000 10000000000000 0", "0", "0", "0", "KEY"],
    ),
    dev(
        "the video bus",
        ["3", "3e000b00000000 0 0 0", "0", "0", "0", "KEY"],
    ),
    dev("the PC speaker", ["40001", "0", "0", "0", "0", ""]),
    dev("an HDMI jack", ["21", "0", "0", "0", "0", "SWITCH"]),
];

impl Recorded {
    fn bitmaps(&self) -> Bitmaps {
        Bitmaps::from_sysfs(|name| {
            Some(String::from(match name {
                "capabilities/ev" => self.ev,
                "capabilities/key" => self.key,
                "capabilities/rel" => self.rel,
                "capabilities/abs" => self.abs,
                "properties" => self.props,
                _ => return None,
            }))
        })
    }

    /// The device's udev properties: `ID_INPUT=1` and its tags.
    fn properties(&self) -> Vec<(String, &'static str)> {
        std::iter::once((String::from("ID_INPUT"), "1"))
            .chain(
                self.tags
                    .split_whitespace()
                    .map(|t| (format!("ID_INPUT_{t}"), "1")),
            )
            .collect()
    }

    fn tagged(&self) -> Classes {
        let properties = self.properties();
        Classes::from_properties(|name| properties.iter().find(|(n, _)| n == name).map(|&(_, v)| v))
            .unwrap_or_else(|| panic!("{}: udev tagged it", self.role))
    }
}

fn names(classes: Classes) -> Vec<&'static str> {
    classes.iter().map(Class::name).collect()
}

fn role(role: &str) -> &'static Recorded {
    MACHINE
        .iter()
        .find(|d| d.role == role)
        .unwrap_or_else(|| panic!("no device {role}"))
}

#[test]
fn the_evidence_devices_get_the_same_class_from_udev_tags_and_from_their_bits() {
    for d in MACHINE {
        assert_eq!(
            names(Classes::from_bitmaps(&d.bitmaps())),
            names(d.tagged()),
            "{}",
            d.role
        );
    }
    for keyboard in ["keyboard A", "the numpad with letters", "the OTP token"] {
        assert!(
            Classes::from_bitmaps(&role(keyboard).bitmaps()).contains(Class::Keyboard),
            "{keyboard}"
        );
    }
    for media in [
        "the receiver's consumer controls",
        "the headphones' AVRCP node",
    ] {
        assert_eq!(
            names(Classes::from_bitmaps(&role(media).bitmaps())),
            ["key"],
            "{media}"
        );
    }

    // A numpad without letters: its keypad, Num Lock, Backspace and Enter.
    let mut numpad = Bitmaps::default();
    parse_bitmap("3", &mut numpad.ev);
    let mut key = KeyBits::default();
    for code in [
        14, 28, 55, 69, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 96, 98,
    ] {
        super::set_bit(&mut key, code);
    }
    numpad.key = key;
    assert_eq!(names(Classes::from_bitmaps(&numpad)), ["key"]);
}

#[test]
fn a_hwdb_correction_changes_the_class_vtium_reads() {
    let token = role("the OTP token");
    let corrected = |name: &str| match name {
        "ID_INPUT" | "ID_INPUT_KEY" => Some("1"),
        "ID_INPUT_KEYBOARD" => Some("0"),
        _ => None,
    };
    assert_eq!(names(token.tagged()), ["keyboard", "key"]);
    assert_eq!(
        Classes::from_properties(corrected).map(names),
        Some(vec!["key"])
    );
}

#[test]
fn a_device_udev_never_tagged_has_no_classes_from_properties() {
    assert_eq!(Classes::from_properties(|_| None), None);
    assert_eq!(
        Classes::from_properties(|name| (name == "ID_INPUT_KEYBOARD").then_some("1")),
        None
    );
}

#[test]
fn a_sysfs_bitmap_reads_its_least_significant_word_last() {
    let mut key = KeyBits::default();
    parse_bitmap("1 0 8000", &mut key);
    assert!(bit_is_set(&key, 15));
    assert!(bit_is_set(&key, 128));
    assert!(!bit_is_set(&key, 0));
    let mut short = [0usize; 1];
    parse_bitmap("ff 0", &mut short);
    assert_eq!(short, [0]);
    parse_bitmap("zz 3", &mut short);
    assert_eq!(short, [3]);
}
