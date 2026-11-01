//! A `path:` token against the ports and hubs below it, and a
//! `connector-path:` token against a hub port's `PATH` (ADR-1401 D4).

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

/// Whether `connector-path:PATTERN` takes a display whose connector's
/// `PATH` is `path`.
fn takes_path(pattern: &str, path: Option<&str>) -> bool {
    let token = Token::parse(&format!("connector-path:{pattern}"));
    assert_eq!(token, Ok(Token::ConnectorPath(pattern.to_owned())));
    let mut display = Identity::display();
    display.connector_path = path.map(str::to_owned);
    Token::ConnectorPath(pattern.to_owned()).matches(&display)
}

// ADR-1401 D4: `connector-path:` matches the whole `PATH`, exactly and
// with case, and a display with no `PATH` matches no `connector-path:`
// covers: platform.heads.rules
#[test]
fn connector_path_matches_the_whole_path_exactly_and_with_case() {
    assert!(takes_path("mst:213-1", Some("mst:213-1")));
    for path in [
        Some("mst:213-1-1"),
        Some("mst:213-10"),
        Some("mst:213"),
        Some("MST:213-1"),
        Some("xmst:213-1"),
        None,
    ] {
        assert!(!takes_path("mst:213-1", path), "{path:?}");
    }
}

// ADR-1401 D4: `connector-path:` takes the globs `connector:` and `card:`
// take, `*` for any run of characters and `?` for one
// covers: platform.heads.rules
#[test]
fn connector_path_takes_globs_as_connector_and_card_do() {
    for path in ["mst:213-1", "mst:213-2", "mst:213-2-1"] {
        assert!(takes_path("mst:213-*", Some(path)), "{path}");
    }
    assert!(!takes_path("mst:213-*", Some("mst:214-1")));
    assert!(!takes_path("mst:213-*", None));
    assert!(takes_path("mst:213-?", Some("mst:213-2")));
    assert!(!takes_path("mst:213-?", Some("mst:213-2-1")));
    assert!(takes_path("mst:*-1", Some("mst:99-1")));
    assert!(!takes_path("MST:*", Some("mst:213-1")), "a glob keeps case");
    // the same globbing as `card:` gives
    let mut display = Identity::display();
    display.card = Some("mst:213-2".to_owned());
    for pattern in ["mst:213-*", "mst:213-?", "mst:*-2", "MST:*"] {
        assert_eq!(
            takes_path(pattern, Some("mst:213-2")),
            Token::Card(pattern.to_owned()).matches(&display),
            "{pattern}"
        );
    }
}
