//! The mock-up catalog's six example pages of the Flow tab, each as a
//! machine: its vtium.conf and the devices it has. The test laptop's
//! panel, eDP-1, and its keyboard stand in for the catalog's machine, and
//! the kiosk and the lab have their own displays and USB ports.
//!
//! References:
//! - The catalog's "Flow: paths, with roaming inputs", "Flow: vtium's
//!   built-in default", "Flow: two terminals sharing a display on separate
//!   VTs", "Flow: a VT clash", "Flow: a kiosk with an admin station" and
//!   "Flow: three stations on one machine".

use vtium_config_core::flowmodel::Present;
use vtium_config_core::ui::Shared;
use vtium_settings::{Class, Edid, Identity};

/// A display on `connector`, with the EDID serial `serial`.
fn monitor(connector: &str, serial: &str) -> Identity {
    let mut d = Identity::display();
    d.connector = Some(connector.to_owned());
    d.card = Some("pci-0000:00:02.0".to_owned());
    d.edid = Some(Edid {
        maker: "BOE".to_owned(),
        product: 0x0bca,
        serial: serial.to_owned(),
    });
    d
}

/// A laptop's panel: no EDID serial a rule names, matched by its
/// connector.
fn panel() -> Identity {
    let mut d = Identity::display();
    d.connector = Some("eDP-1".to_owned());
    d.card = Some("pci-0000:00:02.0".to_owned());
    d
}

/// An input of `class` named `name` on the port `path`.
fn input(class: Class, name: &str, path: &str, id: (u16, u16)) -> Identity {
    let mut d = Identity::input().with_class(class);
    d.name = Some(name.to_owned());
    d.path = Some(path.to_owned());
    d.id = Some(id);
    d
}

/// The laptop: its panel, HDMI-A-1 with nothing plugged in, its keyboard,
/// its touchpad, and the ThinkPad's extra buttons and power button.
fn laptop() -> Present {
    let mut extra = input(
        Class::Key,
        "ThinkPad Extra Buttons",
        "platform-thinkpad_acpi",
        (0x17aa, 0x5054),
    );
    extra.classes = extra.classes.with(Class::Keyboard);
    Present {
        devices: vec![
            panel(),
            input(
                Class::Keyboard,
                "AT Translated Set 2 keyboard",
                "platform-i8042-serio-0",
                (0x0001, 0x0001),
            ),
            input(
                Class::Touchpad,
                "SynPS/2 Synaptics TouchPad",
                "platform-i8042-serio-1",
                (0x0002, 0x0007),
            ),
            extra,
            input(
                Class::Key,
                "Power Button",
                "LNXPWRBN/button/input0",
                (0x0000, 0x0001),
            ),
        ],
        udev: true,
    }
}

/// The look the laptop's terminals draw with.
const LOOK: &str = "[look My-Look]\nfont-name = cullet-mono\nfont-size = 11\n";

/// The rules that send every pointer roaming, as the shipped default
/// writes them.
const POINTERS_ROAM: &str = "input class:mouse -> roaming\ninput class:touchpad -> roaming\ninput class:touchscreen -> roaming\ninput class:trackball -> roaming\ninput class:pointingstick -> roaming\n";

/// "Flow: paths, with roaming inputs": the panel shows console, which the
/// keyboard types into, and the pointers roam.
pub fn roaming() -> Shared {
    let text = format!(
        "version = 1\n{LOOK}[head console]\nlook = My-Look\n[rules]\ndisplay connector:pci-0000:00:02.0/eDP-1 -> console\ninput id:0001:0001 -> console\n{POINTERS_ROAM}"
    );
    super::machine("padnoir", &text, laptop())
}

/// "Flow: vtium's built-in default": the shipped example, with every
/// display showing console and every input roaming.
pub fn built_in() -> Shared {
    super::machine("padnoir", vtium_config_core::drafts::EXAMPLE, laptop())
}

/// "Flow: two terminals sharing a display on separate VTs": console on VT
/// 1 and work on VT 2, each with the panel and the keyboard.
pub fn shared_vts() -> Shared {
    let text = format!(
        "version = 1\n{LOOK}[head console]\nlook = My-Look\nvt = 1\n[head work]\nvt = 2\n[rules]\ndisplay connector:pci-0000:00:02.0/eDP-1 -> console\ndisplay connector:pci-0000:00:02.0/eDP-1 -> work\ninput id:0001:0001 -> console\ninput id:0001:0001 -> work\n{POINTERS_ROAM}"
    );
    super::machine("padnoir", &text, laptop())
}

/// The kiosk machine: the kiosk's display and the admin's, each with a
/// keyboard on a USB port of its own.
fn kiosk_machine() -> Present {
    Present {
        devices: vec![
            monitor("DP-1", "KIOSK00001"),
            monitor("HDMI-A-1", "ADMIN00001"),
            input(
                Class::Keyboard,
                "Kiosk keypad",
                "pci-0000:00:14.0-usb-0:3.1:1.0",
                (0x04d9, 0x1203),
            ),
            input(
                Class::Keyboard,
                "Admin keyboard",
                "pci-0000:00:14.0-usb-0:3.2:1.0",
                (0x046d, 0xc31c),
            ),
        ],
        udev: true,
    }
}

/// "Flow: a kiosk with an admin station": the policy is closed, the
/// kiosk's display and keyboard port go to kiosk, and the admin's to
/// admin.
pub fn kiosk() -> Shared {
    let text = "version = 1\npolicy = closed\n[head kiosk]\n[head admin]\n[rules]\ndisplay edid:BOE:0x0bca:KIOSK00001 -> kiosk\ndisplay edid:BOE:0x0bca:ADMIN00001 -> admin\ninput path:pci-0000:00:14.0-usb-0:3.1 -> kiosk\ninput path:pci-0000:00:14.0-usb-0:3.2 -> admin\n";
    super::machine("kiosk", text, kiosk_machine())
}

/// "Flow: three stations on one machine": three displays, each with its
/// own terminal and the inputs of one USB port.
pub fn stations() -> Shared {
    let mut devices = Vec::new();
    let mut text = String::from("version = 1\npolicy = closed\n");
    let seats = [
        ("DP-1", "MON0000001"),
        ("DP-2", "MON0000002"),
        ("HDMI-A-1", "MON0000003"),
    ];
    for k in 1..=seats.len() {
        text.push_str(&format!("[head station-{k}]\n"));
    }
    text.push_str("[rules]\n");
    for (k, (connector, serial)) in seats.iter().enumerate() {
        let n = k + 1;
        devices.push(monitor(connector, serial));
        text.push_str(&format!(
            "display edid:BOE:0x0bca:{serial} -> station-{n}\n"
        ));
    }
    for k in 1..=seats.len() {
        let port = format!("pci-0000:00:14.0-usb-0:3.{k}");
        devices.push(input(
            Class::Keyboard,
            &format!("Station {k} keyboard"),
            &format!("{port}:1.0"),
            (0x046d, 0xc310 + u16::try_from(k).unwrap_or(0)),
        ));
        text.push_str(&format!("input path:{port} -> station-{k}\n"));
    }
    super::machine(
        "lab",
        &text,
        Present {
            devices,
            udev: true,
        },
    )
}

/// The six examples, each with the name its snapshots take.
pub fn all() -> Vec<(&'static str, Shared)> {
    vec![
        ("paths-roaming", roaming()),
        ("built-in-default", built_in()),
        ("shared-vts", shared_vts()),
        ("vt-clash", shared_vts()),
        ("kiosk-admin", kiosk()),
        ("stations", stations()),
    ]
}
