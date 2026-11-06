//! A valid configuration wires at least one complete route (ADR-1403 D3):
//! a display present sent to a head that a keyboard present reaches, or by
//! the rules alone where no display or no keyboard is present or the
//! caller reads no devices.

use vtium_settings::{
    Class, Config, Edid, Identity, Judged, Missing, Route, complete_route, parse_sections,
};

fn flow(text: &str) -> Config {
    parse_sections(text)
        .expect("the sections are valid")
        .expect("the file holds a flow")
        .config
}

fn display(connector: &str) -> Identity {
    Identity {
        connector: Some(connector.to_owned()),
        edid: Some(Edid {
            maker: "AAA".to_owned(),
            product: 1,
            serial: "LARGE00001".to_owned(),
        }),
        ..Identity::display()
    }
}

fn keyboard() -> Identity {
    Identity {
        path: Some("pci-0000:15:00.0-usb-0:3.3:1.0".to_owned()),
        ..Identity::input().with_class(Class::Keyboard)
    }
}

/// The route's verdict and its message, as `vtium check-config` prints it.
fn said(route: &Route) -> (Judged, Option<String>) {
    (
        route.judged,
        route.missing.as_ref().map(ToString::to_string),
    )
}

#[test]
fn the_shipped_default_wires_a_route_by_the_devices_and_by_the_rules_alone() {
    let config = Config::builtin();
    let devices = [display("DP-8"), keyboard()];
    assert_eq!(
        said(&complete_route(&config, Some(&devices))),
        (Judged::Devices, None)
    );
    assert_eq!(
        said(&complete_route(&config, None)),
        (Judged::RulesAlone, None)
    );
    assert_eq!(
        said(&complete_route(&config, Some(&[]))),
        (Judged::NoDisplay, None)
    );
    assert_eq!(
        said(&complete_route(&config, Some(&[display("DP-8")]))),
        (Judged::NoKeyboard, None)
    );
}

#[test]
fn a_display_sent_to_off_names_the_rule_it_falls_to() {
    let config = flow(
        "version = 1\n[head desk]\n[rules]\ninput * -> desk\ndisplay class:keyboard -> desk\ndisplay * -> off\n",
    );
    let devices = [display("DP-8"), keyboard()];
    let route = complete_route(&config, Some(&devices));
    assert!(
        matches!(
            route.missing,
            Some(Missing::NoDisplay { rule: Some(3), .. })
        ),
        "{:?}",
        route.missing
    );
    assert_eq!(
        said(&route).1.as_deref(),
        Some(
            "No connected display reaches a head: rule 3 sends display \"DP-8\" to `off`. Add a rule above it that sends the display to a head."
        )
    );
    let none = flow("version = 1\n[head desk]\n[rules]\ninput * -> desk\n");
    assert_eq!(
        said(&complete_route(&none, Some(&devices))).1.as_deref(),
        Some(
            "No connected display reaches a head: no rule matches display \"DP-8\", so it stays off. Add a display rule that sends it to a head."
        )
    );
}

#[test]
fn a_head_no_keyboard_reaches_is_named_with_its_display() {
    let config = flow("version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> ignore\n");
    let devices = [display("DP-8"), keyboard()];
    assert_eq!(
        said(&complete_route(&config, Some(&devices))),
        (
            Judged::Devices,
            Some("No keyboard reaches head \"desk\", which shows on \"DP-8\". Add an input rule that sends a keyboard to \"desk\" or to `roaming`.".to_owned())
        )
    );
    // a keyboard sent to the head itself completes the route
    let direct = flow(
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput path:pci-0000:15:00.0-usb-0:3.3 -> desk\ninput * -> ignore\n",
    );
    assert!(complete_route(&direct, Some(&devices)).complete());
}

#[test]
fn roaming_keyboards_do_not_reach_an_isolated_head() {
    let config = flow(
        "version = 1\n[head desk]\nisolated = yes\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n",
    );
    let devices = [display("DP-8"), keyboard()];
    let message = "Head \"desk\" is isolated, so roaming keyboards do not reach it. Add an input rule that sends a keyboard to \"desk\", or write `isolated = no` in its section.";
    assert_eq!(
        said(&complete_route(&config, Some(&devices))),
        (Judged::Devices, Some(message.to_owned()))
    );
    assert_eq!(
        said(&complete_route(&config, None)),
        (Judged::RulesAlone, Some(message.to_owned()))
    );
}

#[test]
fn a_keyboard_no_rule_matches_roams_under_open_and_not_under_closed() {
    let devices = [display("DP-8"), keyboard()];
    let open = flow("version = 1\n[head desk]\n[rules]\ndisplay * -> desk\n");
    assert!(complete_route(&open, Some(&devices)).complete());
    assert!(complete_route(&open, None).complete());
    let closed = flow(
        "version = 1\npolicy = closed\n[head desk]\nisolated = no\n[rules]\ndisplay * -> desk\n",
    );
    assert!(!complete_route(&closed, Some(&devices)).complete());
    assert!(!complete_route(&closed, None).complete());
}

#[test]
fn a_token_is_no_keyboard_present() {
    let config = Config::builtin();
    let mut token = keyboard();
    token
        .properties
        .insert("VTIUM_ROLE".to_owned(), "token".to_owned());
    let devices = [display("DP-8"), token];
    assert_eq!(
        complete_route(&config, Some(&devices)).judged,
        Judged::NoKeyboard
    );
}

#[test]
fn by_the_rules_alone_a_keyboard_rule_must_be_class_keyboard_a_star_or_one_device() {
    let no_display = flow("version = 1\n[head desk]\n[rules]\ninput * -> roaming\n");
    assert_eq!(
        said(&complete_route(&no_display, None)).1.as_deref(),
        Some(
            "No display rule sends a display to a head. Add a rule `display MATCH -> TARGET` whose target is a head."
        )
    );
    let by_id = flow(
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput id:046d:c31c -> desk\ninput * -> ignore\n",
    );
    assert_eq!(
        said(&complete_route(&by_id, None)).1.as_deref(),
        Some(
            "No input rule sends a keyboard to head \"desk\", which gets its displays from rule 1. Add an input rule that sends a keyboard to \"desk\" or to `roaming`."
        )
    );
    for rule in [
        "input class:keyboard -> desk",
        "input * -> desk",
        "input path:pci-0000:15:00.0-usb-0:3.3 -> desk",
        "input name:Logitech* -> roaming",
    ] {
        let text = format!(
            "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\n{rule}\ninput * -> ignore\n"
        );
        assert!(complete_route(&flow(&text), None).complete(), "{rule}");
    }
    // a display rule no display reaches sends nothing
    let shadowed = flow(
        "version = 1\n[head desk]\n[rules]\ndisplay * -> off\ndisplay connector:card0/DP-1 -> desk\ninput * -> roaming\n",
    );
    assert_eq!(
        complete_route(&shadowed, None).missing,
        Some(Missing::NoDisplayRule)
    );
}

#[test]
fn the_judgement_says_why_it_read_the_rules_alone() {
    let texts: Vec<String> = [Judged::NoDisplay, Judged::NoKeyboard, Judged::RulesAlone]
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        texts,
        [
            "No display is connected, so vtium checked the routes by the rules alone.",
            "No keyboard is connected, so vtium checked the routes by the rules alone.",
            "vtium checked the routes by the rules alone, without the connected devices.",
        ]
    );
}
