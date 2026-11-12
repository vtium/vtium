//! A device as the head file's rules see it: vtium-platform's identities
//! of a display and of an input, read from udev, sysfs, the device and the
//! KMS connector, mapped into vtium-settings's [`Identity`] (ADR-1401 D4).
//!
//! The start walks each device through the rules with what these give, and
//! `vtium check-config` reads the devices present through the same mapping,
//! so a rule matches the same device in both.

use vtium_platform::display::{self, Serial};
use vtium_platform::input::Class as Udev;
use vtium_platform::udev::InputIdentity;
use vtium_settings::{Class, Classes, Edid, Identity, UsbSerial};

/// What the display guard reads of a display's card beside the display's
/// own identity (D9).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    /// udev's `ID_PATH` of the card, which `card:` and `connector:` match.
    pub id_path: Option<String>,
    /// The card the system booted on (`boot_vga`).
    pub primary: bool,
    /// The card's place in the kernel's order of cards: its node's number,
    /// `card1`'s 1, which is the card's minor number.
    pub order: u32,
}

impl Card {
    /// The card at `node`, with udev's `ID_PATH` and the boot card's mark.
    pub fn at(node: &std::path::Path, id_path: Option<&str>, primary: bool) -> Card {
        let name = node.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let digits = name.trim_start_matches(|c: char| !c.is_ascii_digit());
        Card {
            id_path: id_path.map(str::to_owned),
            primary,
            order: digits.parse().unwrap_or(0),
        }
    }
}

/// The display `identity` names, on `card`, the `order`-th of its card's
/// connectors in the kernel's order.
pub fn display(identity: &display::Identity, card: &Card, order: u32) -> Identity {
    Identity {
        edid: identity
            .edid
            .as_ref()
            .map(|e| edid(e.maker(), e.product(), e.serial())),
        connector: Some(identity.connector.clone()),
        connector_path: identity.connector_path.clone(),
        card: identity.card.clone().or_else(|| card.id_path.clone()),
        lit_at_start: identity.lit_at_start,
        on_boot_card: card.primary,
        card_order: card.order,
        order,
        ..Identity::display()
    }
}

/// A display its device names only by `connector`, as a framebuffer's is:
/// no EDID, and not lit at start as far as vtium can tell.
pub fn unnamed_display(connector: &str, card: &Card, order: u32) -> Identity {
    Identity {
        connector: Some(connector.to_owned()),
        card: card.id_path.clone(),
        on_boot_card: card.primary,
        card_order: card.order,
        order,
        ..Identity::display()
    }
}

/// Why vtium cannot write a rule that names the display `id`, for the log
/// line of a display that no rule claims and for which
/// `vtium_settings::rule_for` finds no key to write. Each part is judged
/// by `rule_for` itself, given that part of the display alone, so the
/// reason and the rule line never disagree. `udev_daemon` says whether a
/// udev daemon runs to give a card its path.
pub fn why_unnamed(id: &Identity, udev_daemon: bool) -> String {
    let writes = |part: Identity| vtium_settings::rule_for(&part, "x").is_some();
    let mut reasons = Vec::new();
    match &id.edid {
        None => reasons.push("its monitor reports no EDID"),
        Some(e) if e.serial.is_empty() => {
            reasons.push("its monitor's EDID carries no serial number");
        }
        Some(_) => {
            let edid = Identity {
                edid: id.edid.clone(),
                ..Identity::display()
            };
            if !writes(edid) {
                reasons.push(
                    "its monitor's EDID names a maker or serial number that a rule cannot hold",
                );
            }
        }
    }
    let hub = Identity {
        connector_path: id.connector_path.clone(),
        ..Identity::display()
    };
    if id.connector_path.is_some() && !writes(hub) {
        reasons.push("its DisplayPort hub's path has a character that a rule cannot hold");
    }
    let port = Identity {
        card: id.card.clone(),
        connector: id.connector.clone(),
        ..Identity::display()
    };
    match &id.card {
        None if udev_daemon => reasons.push("udev gave its card no path (ID_PATH)"),
        None => reasons.push("no udev daemon runs to give its card a path (ID_PATH)"),
        Some(_) if !writes(port) => {
            reasons.push(
                "its card's path or its connector's name has a character that a rule cannot hold",
            );
        }
        Some(_) => {}
    }
    let because = match reasons.as_slice() {
        [] => String::from("it reports nothing a rule can match"),
        [one] => (*one).to_owned(),
        [first @ .., last] => format!("{}, and {last}", first.join(", ")),
    };
    format!("vtium cannot write a rule that names this display, because {because}.")
}

/// The EDID's maker, product code and serial as `edid:` matches them: the
/// serial string, else the serial number as vtium-platform writes it,
/// `0x` and eight hex digits, else none, which no `edid:` token matches.
fn edid(maker: String, product: u16, serial: &Serial) -> Edid {
    Edid {
        maker,
        product,
        serial: match serial {
            Serial::Text(text) => text.clone(),
            Serial::Number(n) => format!("0x{n:08X}"),
            Serial::None => String::new(),
        },
    }
}

/// The input device `identity` describes.
pub fn input(identity: &InputIdentity) -> Identity {
    let hex = |v: &Option<String>| v.as_deref().and_then(|v| u16::from_str_radix(v, 16).ok());
    let serial = match (
        hex(&identity.vendor_id),
        hex(&identity.model_id),
        identity.serial_short.as_ref(),
    ) {
        (Some(vendor), Some(product), Some(serial)) if !serial.is_empty() => Some(UsbSerial {
            vendor,
            product,
            serial: serial.clone(),
        }),
        _ => None,
    };
    Identity {
        classes: identity
            .classes
            .iter()
            .fold(Classes::default(), |all, c| all.with(class(c))),
        path: identity.id_path.clone(),
        serial,
        unique: identity.uniq.clone(),
        id: identity.id,
        name: identity.name.clone(),
        properties: identity.properties.clone(),
        ..Identity::input()
    }
}

/// udev's class as `class:` names it.
fn class(c: Udev) -> Class {
    match c {
        Udev::Keyboard => Class::Keyboard,
        Udev::Key => Class::Key,
        Udev::Mouse => Class::Mouse,
        Udev::Touchpad => Class::Touchpad,
        Udev::Touchscreen => Class::Touchscreen,
        Udev::Trackball => Class::Trackball,
        Udev::PointingStick => Class::Pointingstick,
        Udev::Tablet => Class::Tablet,
        Udev::TabletPad => Class::TabletPad,
        Udev::Joystick => Class::Joystick,
        Udev::Accelerometer => Class::Accelerometer,
        Udev::Switch => Class::Switch,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use vtium_platform::input::Classes as UdevClasses;
    use vtium_settings::{Place, parse};

    use super::*;

    #[test]
    fn every_udev_class_maps_to_the_class_of_its_name() {
        for c in Udev::ALL {
            assert_eq!(class(c).name(), c.name());
        }
    }

    #[test]
    fn an_edid_serial_string_and_number_read_as_edid_matches_them() {
        let text = edid("AAA".into(), 1, &Serial::Text("LARGE00001".into()));
        let number = edid("BBB".into(), 2, &Serial::Number(0x1234));
        let none = edid("CCC".into(), 3, &Serial::None);
        let config = parse(
            "version = 1\n[head a]\n[head b]\n[head c]\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> a\ndisplay edid:BBB:0x0002:0x00001234 -> b\ndisplay * -> c\n",
        )
        .map(|p| p.config)
        .unwrap_or_default();
        let place = |edid: Edid| {
            config
                .walk(&Identity {
                    edid: Some(edid),
                    ..Identity::display()
                })
                .place
        };
        assert_eq!(place(text), Place::Head(0));
        assert_eq!(place(number), Place::Head(1));
        assert_eq!(place(none.clone()), Place::Head(2));
        assert_eq!(none.serial, "");
    }

    // A display whose card udev never gave an ID_PATH, as padnoir's
    // NVIDIA card when vtium listed it before udev set it up, says so, and
    // each part of a display is judged on its own, as rule_for judges it.
    #[test]
    fn a_display_no_rule_can_name_says_what_it_lacks() {
        let reason = |edid: Option<Edid>, card: Option<&str>, daemon: bool| {
            let id = Identity {
                edid,
                card: card.map(str::to_owned),
                connector: Some("DP-4".into()),
                ..Identity::display()
            };
            why_unnamed(&id, daemon)
        };
        let no_serial = || Some(edid("AAA".into(), 1, &Serial::None));
        let spaced = || Some(edid("AAA".into(), 1, &Serial::Text("AB 12".into())));
        let card = Some("pci-0000:01:00.0");
        // Neither part: the EDID is missing and udev gave the card no path.
        assert_eq!(
            reason(None, None, true),
            "vtium cannot write a rule that names this display, because its monitor reports no EDID, and udev gave its card no path (ID_PATH)."
        );
        // No udev daemon runs at all.
        assert_eq!(
            reason(None, None, false),
            "vtium cannot write a rule that names this display, because its monitor reports no EDID, and no udev daemon runs to give its card a path (ID_PATH)."
        );
        // An EDID without a serial number, and no card path.
        assert_eq!(
            reason(no_serial(), None, true),
            "vtium cannot write a rule that names this display, because its monitor's EDID carries no serial number, and udev gave its card no path (ID_PATH)."
        );
        // The opposite: no EDID, and a card path a rule could hold, so only
        // the EDID is named.
        assert_eq!(
            reason(None, card, true),
            "vtium cannot write a rule that names this display, because its monitor reports no EDID."
        );
        // A serial with a space, which rule_for refuses, and a card path.
        assert_eq!(
            reason(spaced(), card, true),
            "vtium cannot write a rule that names this display, because its monitor's EDID names a maker or serial number that a rule cannot hold."
        );
        // A card path a rule cannot hold.
        assert_eq!(
            reason(None, Some("pci 0000"), true),
            "vtium cannot write a rule that names this display, because its monitor reports no EDID, and its card's path or its connector's name has a character that a rule cannot hold."
        );
        // Each reason agrees with rule_for: with a writable card the line is
        // written, so no reason is asked for.
        let named = Identity {
            edid: spaced(),
            card: card.map(str::to_owned),
            connector: Some("DP-4".into()),
            ..Identity::display()
        };
        assert!(vtium_settings::rule_for(&named, "x").is_some());
    }

    #[test]
    fn a_display_carries_its_connector_its_cards_path_and_the_guards_facts() {
        let card = Card::at(Path::new("/dev/dri/card1"), Some("pci-0000:03:00.0"), true);
        assert_eq!(card.order, 1);
        let platform = display::Identity {
            connector: "DP-8".into(),
            card: None,
            edid: None,
            connector_path: None,
            lit_at_start: true,
        };
        let id = display(&platform, &card, 4);
        assert_eq!(id.connector.as_deref(), Some("DP-8"));
        assert_eq!(id.card.as_deref(), Some("pci-0000:03:00.0"));
        assert!(id.lit_at_start && id.on_boot_card);
        assert_eq!((id.card_order, id.order), (1, 4));
        let config = parse(
            "version = 1\n[head desk]\n[rules]\ndisplay connector:pci-0000:03:*/DP-* -> desk\n",
        )
        .map(|p| p.config)
        .unwrap_or_default();
        assert_eq!(config.walk(&id).place, Place::Head(0));
        let fb = unnamed_display("fb", &Card::at(Path::new("/dev/fb0"), None, false), 0);
        assert_eq!(config.walk(&fb).place, Place::Off);
    }

    // ADR-1401 D4: a DisplayPort multi-stream hub's port carries its
    // connector's `PATH`, which a `connector-path:` rule with no glob matches
    // whole and exactly, whatever name the driver gave the connector.
    #[test]
    fn a_hub_port_carries_its_connector_path_and_a_connector_path_rule_places_it() {
        let card = Card::at(Path::new("/dev/dri/card1"), Some("pci-0000:03:00.0"), true);
        let port = |connector: &str, path: Option<&str>| display::Identity {
            connector: connector.into(),
            card: None,
            edid: None,
            connector_path: path.map(str::to_owned),
            lit_at_start: false,
        };
        let config = parse(
            "version = 1\n[head desk]\n[head side]\n[rules]\ndisplay connector-path:mst:213-1 -> desk\ndisplay connector-path:mst:213-2 -> side\n",
        )
        .map(|p| p.config)
        .unwrap_or_default();
        let place = |connector: &str, path: Option<&str>| {
            let id = display(&port(connector, path), &card, 0);
            config.walk(&id).place
        };
        assert_eq!(
            display(&port("DP-8", Some("mst:213-1")), &card, 0)
                .connector_path
                .as_deref(),
            Some("mst:213-1")
        );
        // the port renamed after a replug keeps its head
        assert_eq!(place("DP-8", Some("mst:213-1")), Place::Head(0));
        assert_eq!(place("DP-11", Some("mst:213-1")), Place::Head(0));
        assert_eq!(place("DP-9", Some("mst:213-2")), Place::Head(1));
        // a port further down the chain, a prefix, or no path takes neither
        for other in [
            Some("mst:213-1-1"),
            Some("mst:213"),
            Some("MST:213-1"),
            None,
        ] {
            assert_eq!(place("DP-8", other), Place::Off, "{other:?}");
        }
    }

    #[test]
    fn an_input_carries_every_key_the_rules_read() {
        let udev = InputIdentity {
            classes: UdevClasses::empty().with(Udev::Keyboard).with(Udev::Key),
            tagged: true,
            id_path: Some("pci-0000:15:00.0-usb-0:3.4.1:1.0".into()),
            vendor_id: Some("aaaa".into()),
            model_id: Some("0002".into()),
            serial_short: Some("NUMPAD0001".into()),
            uniq: Some("aa:bb:cc:dd:ee:ff".into()),
            id: Some((0xaaaa, 0x0002)),
            name: Some("A numpad".into()),
            properties: BTreeMap::from([("VTIUM_ROLE".to_owned(), "numpad".to_owned())]),
        };
        let id = input(&udev);
        assert!(id.classes.has(Class::Keyboard) && id.classes.has(Class::Key));
        assert!(!id.classes.has(Class::Mouse));
        assert!(!id.counts_as_keyboard(), "VTIUM_ROLE=numpad");
        let config = parse(
            "version = 1\n[head a]\n[head b]\n[head c]\n[head d]\n[rules]\ninput path:pci-0000:15:00.0-usb-0:3.4.1 serial:aaaa:0002:NUMPAD0001 -> a\ninput * -> d\n",
        )
        .map(|p| p.config)
        .unwrap_or_default();
        assert_eq!(config.walk(&id).place, Place::Head(0));
        for rule in [
            "bluetooth:aa:bb:cc:dd:ee:ff",
            "id:aaaa:0002",
            "name:A?numpad",
            "udev:VTIUM_ROLE=num*",
            "class:key",
        ] {
            let config = parse(&format!(
                "version = 1\n[head a]\n[rules]\ninput {rule} -> a\n"
            ))
            .map(|p| p.config)
            .unwrap_or_default();
            assert_eq!(config.walk(&id).place, Place::Head(0), "{rule}");
        }
        // A unit without a serial matches no `serial:`.
        let bare = input(&InputIdentity {
            serial_short: None,
            ..udev
        });
        assert_eq!(bare.serial, None);
    }
}
