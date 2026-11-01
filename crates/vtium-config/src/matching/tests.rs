//! A `path:` token against the ports and hubs below it (ADR-1401 D4).

use super::*;

/// Whether `path:PORT` takes an input whose `ID_PATH` is `path`.
fn takes(port: &str, path: &str) -> bool {
    assert_eq!(
        Token::parse(&format!("path:{port}")),
        Ok(Token::Path(port.to_owned()))
    );
    let mut device = Identity::input();
    device.path = Some(path.to_owned());
    Token::Path(port.to_owned()).matches(&device)
}

const PORT: &str = "pci-0000:15:00.0-usb-0:3.3";

// covers: platform.heads.rules
#[test]
fn path_takes_its_port_every_interface_below_it_and_every_device_on_a_hub_in_it() {
    for path in [
        "pci-0000:15:00.0-usb-0:3.3",
        "pci-0000:15:00.0-usb-0:3.3:1.0",
        "pci-0000:15:00.0-usb-0:3.3:1.2",
        "pci-0000:15:00.0-usb-0:3.3.2",
        "pci-0000:15:00.0-usb-0:3.3.2:1.0",
        "pci-0000:15:00.0-usb-0:3.3.4.1:1.0",
    ] {
        assert!(takes(PORT, path), "{path}");
    }
}

// covers: platform.heads.rules
#[test]
fn path_never_takes_a_port_whose_number_only_begins_with_its_own() {
    for path in [
        "pci-0000:15:00.0-usb-0:3.30:1.0",
        "pci-0000:15:00.0-usb-0:3.31.2:1.0",
        "pci-0000:15:00.0-usb-0:3.3-1:1.0",
        "pci-0000:15:00.0-usb-0:3",
        "pci-0000:15:00.0-usb-0:3.2.3:1.0",
    ] {
        assert!(!takes(PORT, path), "{path}");
    }
    let mut no_path = Identity::input();
    no_path.path = None;
    assert!(
        !Token::Path(PORT.to_owned()).matches(&no_path),
        "no ID_PATH, as with no udev daemon"
    );
}

// covers: platform.heads.rules
#[test]
fn a_port_on_a_hub_takes_only_that_hub_port() {
    let port = "pci-0000:15:00.0-usb-0:3.3.2";
    assert!(takes(port, "pci-0000:15:00.0-usb-0:3.3.2:1.0"));
    assert!(takes(port, "pci-0000:15:00.0-usb-0:3.3.2.1:1.0"));
    assert!(!takes(port, "pci-0000:15:00.0-usb-0:3.3.1:1.0"));
    assert!(!takes(port, "pci-0000:15:00.0-usb-0:3.3:1.0"));
}
