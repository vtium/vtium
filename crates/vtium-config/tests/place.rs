//! Where each device lands: the first-match walk, each match key, and the
//! guards over a set of devices, under each policy (ADR-1401 D3, D4, D5,
//! D7, D9).

use vtium_config::{
    By, Class, Config, Disposition, Edid, Identity, Lockout, Place, Placed, Policy, UsbSerial,
    glob, parse, reading, rule_for,
};

fn config(text: &str) -> Config {
    match parse(text) {
        Ok(p) => p.config,
        Err(e) => panic!("{e:?}\n{text}"),
    }
}

/// What survives of `text`, a file with at least one error.
fn survivors(text: &str) -> Config {
    let r = reading(text.as_bytes());
    assert!(!r.errors.is_empty(), "{text}");
    r.parsed.config
}

/// `text` with `policy = closed` among its top-level keys.
fn closed(text: &str) -> String {
    format!("policy = closed\n{text}")
}

fn keyboard() -> Identity {
    Identity::input()
        .with_class(Class::Keyboard)
        .with_class(Class::Key)
}

fn at_path(mut d: Identity, path: &str) -> Identity {
    d.path = Some(path.to_owned());
    d
}

fn monitor(card: &str, connector: &str) -> Identity {
    let mut d = Identity::display();
    d.card = Some(card.to_owned());
    d.connector = Some(connector.to_owned());
    d
}

/// The Evidence table's inputs, classed as udev tags them.
fn evidence() -> Vec<(&'static str, Identity)> {
    let mut avrcp = Identity::input().with_class(Class::Key);
    avrcp.name = Some("Headphones (AVRCP)".to_owned());
    vec![
        ("keyboard A", keyboard()),
        ("numpad with letters", keyboard()),
        ("OTP token", keyboard()),
        (
            "letterless numpad",
            Identity::input().with_class(Class::Key),
        ),
        ("mouse", Identity::input().with_class(Class::Mouse)),
        ("touchpad", Identity::input().with_class(Class::Touchpad)),
        (
            "receiver's consumer controls",
            Identity::input().with_class(Class::Key),
        ),
        ("AVRCP node", avrcp),
        ("joystick", Identity::input().with_class(Class::Joystick)),
        ("tablet", Identity::input().with_class(Class::Tablet)),
        ("lid switch", Identity::input().with_class(Class::Switch)),
        (
            "accelerometer",
            Identity::input().with_class(Class::Accelerometer),
        ),
        ("no class", Identity::input()),
    ]
}

// covers: platform.heads.rules
#[test]
fn the_shipped_rules_roam_keyboards_keys_and_pointers_and_close_the_rest() {
    let builtin = Config::builtin();
    let want = |name: &str| match name {
        "joystick" | "tablet" | "lid switch" | "accelerometer" | "no class" => Place::Ignore,
        _ => Place::Roaming,
    };
    for (name, device) in evidence() {
        assert_eq!(builtin.walk(&device).place, want(name), "{name}");
    }
    let shown = builtin.walk(&monitor("pci-0000:03:00.0", "DP-8"));
    assert_eq!(
        shown,
        Placed {
            place: Place::Head(0),
            by: By::Rule(Config::builtin().rules[0].line)
        }
    );
}

// covers: platform.heads.rules
#[test]
fn the_first_matching_rule_places_a_device_and_no_match_has_a_default() {
    let c = config(
        "version = 1\n[head a]\n[head b]\n[rules]\ninput class:mouse -> b\ninput class:keyboard -> a\ninput class:keyboard -> b\ndisplay connector:*/HDMI-* -> b\n",
    );
    let both = keyboard().with_class(Class::Mouse);
    assert_eq!(
        c.walk(&both),
        Placed {
            place: Place::Head(1),
            by: By::Rule(5)
        }
    );
    assert_eq!(
        c.walk(&keyboard()),
        Placed {
            place: Place::Head(0),
            by: By::Rule(6)
        }
    );
    let joystick = Identity::input().with_class(Class::Joystick);
    assert_eq!(
        c.walk(&joystick),
        Placed {
            place: Place::Roaming,
            by: By::NoMatch
        }
    );
    let dp = monitor("pci-0000:03:00.0", "DP-8");
    assert_eq!(
        c.walk(&dp),
        Placed {
            place: Place::Off,
            by: By::NoMatch
        }
    );
    let hdmi = monitor("pci-0000:00:02.0", "HDMI-A-2");
    assert_eq!(c.walk(&hdmi).place, Place::Head(1));
}

#[test]
fn a_rule_matches_only_its_own_kind() {
    let c = config("version = 1\n[head a]\n[rules]\ninput * -> a\n");
    assert_eq!(c.walk(&Identity::display()).place, Place::Off);
    assert_eq!(c.walk(&Identity::input()).place, Place::Head(0));
}

#[test]
fn a_rules_tokens_must_all_hold() {
    let c = config(
        "version = 1\n[head a]\n[rules]\ninput path:pci-0000:15:00.0-usb-0:3.4.1 serial:aaaa:0002:NUMPAD0001 -> a\n",
    );
    let mut unit = at_path(keyboard(), "pci-0000:15:00.0-usb-0:3.4.1:1.0");
    assert_eq!(c.walk(&unit).place, Place::Roaming, "no serial");
    unit.serial = Some(UsbSerial {
        vendor: 0xaaaa,
        product: 2,
        serial: "NUMPAD0001".into(),
    });
    assert_eq!(c.walk(&unit).place, Place::Head(0));
    let moved = UsbSerial {
        ..unit.serial.clone().expect("set")
    };
    let mut elsewhere = at_path(keyboard(), "pci-0000:15:00.0-usb-0:3.2:1.0");
    elsewhere.serial = Some(moved);
    assert_eq!(
        c.walk(&elsewhere).place,
        Place::Roaming,
        "the unit in another port"
    );
}

#[test]
fn path_takes_its_devices_interfaces_and_the_devices_on_a_hub_below_it_and_nothing_beside_it() {
    let c = config("version = 1\n[head a]\n[rules]\ninput path:pci-0000:15:00.0-usb-0:3.3 -> a\n");
    for (path, want) in [
        ("pci-0000:15:00.0-usb-0:3.3", Place::Head(0)),
        ("pci-0000:15:00.0-usb-0:3.3:1.0", Place::Head(0)),
        ("pci-0000:15:00.0-usb-0:3.3:1.2", Place::Head(0)),
        ("pci-0000:15:00.0-usb-0:3.30:1.0", Place::Roaming),
        ("pci-0000:15:00.0-usb-0:3.3.1:1.0", Place::Head(0)),
        ("pci-0000:15:00.0-usb-0:3", Place::Roaming),
    ] {
        assert_eq!(c.walk(&at_path(keyboard(), path)).place, want, "{path}");
    }
    assert_eq!(
        c.walk(&keyboard()).place,
        Place::Roaming,
        "no ID_PATH, as with no udev daemon"
    );
}

#[test]
fn each_input_key_reads_its_own_fact() {
    let c = config(
        "version = 1\n[head a]\n[rules]\n\
         input serial:AAAA:0X0002:SN:WITH:COLONS -> a\n\
         input bluetooth:aa:bb:cc:dd:ee:ff -> a\n\
         input id:046d:c52b -> a\n\
         input name:Headphones?(AVRCP) -> a\n\
         input udev:VTIUM_HEAD=pan* -> a\n",
    );
    let mut serial = keyboard();
    serial.serial = Some(UsbSerial {
        vendor: 0xaaaa,
        product: 2,
        serial: "SN:WITH:COLONS".into(),
    });
    let mut bt = keyboard();
    bt.unique = Some("aa:bb:cc:dd:ee:ff".into());
    let mut id = keyboard();
    id.id = Some((0x046d, 0xc52b));
    let mut name = Identity::input();
    name.name = Some("Headphones (AVRCP)".into());
    let mut udev = keyboard();
    udev.properties.insert("VTIUM_HEAD".into(), "panel".into());
    for (what, d, line) in [
        ("serial", serial, 4),
        ("bluetooth", bt, 5),
        ("id", id, 6),
        ("name", name, 7),
        ("udev", udev, 8),
    ] {
        assert_eq!(
            c.walk(&d),
            Placed {
                place: Place::Head(0),
                by: By::Rule(line)
            },
            "{what}"
        );
    }
    let mut other_bt = keyboard();
    other_bt.unique = Some("AA:BB:CC:DD:EE:FF".into());
    assert_eq!(
        c.walk(&other_bt).place,
        Place::Roaming,
        "a unique id matches as written"
    );
    let mut wrong = keyboard();
    wrong.properties.insert("VTIUM_HEAD".into(), "desk".into());
    assert_eq!(c.walk(&wrong).place, Place::Roaming);
}

#[test]
fn each_display_key_reads_its_own_fact() {
    let c = config(
        "version = 1\n[head a]\n[rules]\n\
         display edid:BBB:0x0002:SMALL:00:01 -> a\n\
         display connector-path:mst:213-1 -> a\n\
         display connector:pci-0000:03:00.0/DP-* -> a\n\
         display card:pci-0000:00:* -> a\n",
    );
    let mut edid = Identity::display();
    edid.edid = Some(Edid {
        maker: "BBB".into(),
        product: 2,
        serial: "SMALL:00:01".into(),
    });
    let mut mst = Identity::display();
    mst.connector_path = Some("mst:213-1".into());
    let port = monitor("pci-0000:03:00.0", "DP-8");
    let card = monitor("pci-0000:00:02.0", "HDMI-A-2");
    for (what, d, line) in [
        ("edid", edid, 4),
        ("connector-path", mst, 5),
        ("connector", port, 6),
        ("card", card, 7),
    ] {
        assert_eq!(
            c.walk(&d),
            Placed {
                place: Place::Head(0),
                by: By::Rule(line)
            },
            "{what}"
        );
    }
    assert_eq!(
        c.walk(&monitor("pci-0000:03:00.0", "HDMI-A-1")).place,
        Place::Off
    );
}

#[test]
fn a_glob_takes_runs_and_single_characters() {
    for (pattern, text, want) in [
        ("*", "", true),
        ("*", "anything", true),
        ("DP-*", "DP-8", true),
        ("DP-*", "eDP-1", false),
        ("*DP-?", "eDP-1", true),
        ("a*b*c", "aXbYbZc", true),
        ("a*b*c", "aXbYbZ", false),
        ("?", "é", true),
        ("??", "é", false),
        ("a**", "a", true),
        ("", "", true),
        ("", "a", false),
        ("*a*a*a*a*a*b", &"a".repeat(64), false),
    ] {
        assert_eq!(glob(pattern, text), want, "{pattern} on {text}");
    }
}

/// Two displays and two keyboards, for the guards.
fn desk_and_panel() -> Vec<Identity> {
    let mut large = monitor("pci-0000:03:00.0", "DP-8");
    large.on_boot_card = true;
    large.order = 1;
    let mut small = monitor("pci-0000:00:02.0", "HDMI-A-2");
    small.order = 0;
    vec![
        large,
        small,
        at_path(keyboard(), "pci-0000:15:00.0-usb-0:3.3:1.0"),
        at_path(keyboard(), "pci-0000:15:00.0-usb-0:3.2:1.0"),
    ]
}

// covers: platform.heads.guards
#[test]
fn under_open_the_display_guard_lights_the_boot_display_in_the_first_head() {
    let devices = desk_and_panel();
    let dark = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay * -> off\n";
    let p = config(dark).place(&devices, None);
    assert!(p.never_dark);
    assert_eq!(
        p.placed[0],
        Placed {
            place: Place::Head(0),
            by: By::NeverDark
        }
    );
    assert_eq!(p.placed[1].place, Place::Off);

    let mut lit = devices.clone();
    lit[1].lit_at_start = true;
    let p = config(dark).place(&lit, None);
    assert_eq!(
        p.placed[1].place,
        Place::Head(0),
        "the connector lit at start wins"
    );
    assert_eq!(p.placed[0].place, Place::Off);

    let mut boot = devices.clone();
    boot[1].on_boot_card = true;
    let p = config(dark).place(&boot, None);
    assert_eq!(
        p.placed[1].place,
        Place::Head(0),
        "the kernel's first connector on the boot card"
    );

    let p = config("version = 1\n").place(&devices, None);
    assert_eq!(
        p.placed[0].place,
        Place::DefaultHead,
        "a file with no head lights the default head"
    );

    let p = config(
        "version = 1\n[head a]\n[rules]\ndisplay connector:*/HDMI-* -> a\ndisplay * -> off\n",
    )
    .place(&devices, None);
    assert!(
        !p.never_dark,
        "a display the rules light leaves the guard idle"
    );
    assert_eq!(
        (p.placed[0].place, p.placed[1].place),
        (Place::Off, Place::Head(0))
    );
}

// covers: platform.heads.guards
#[test]
fn with_no_display_lit_at_start_and_none_on_the_boot_card_the_first_connected_one_lights() {
    let mut later = monitor("pci-0000:05:00.0", "DP-1");
    later.card_order = 2;
    later.order = 0;
    let mut first = monitor("pci-0000:00:02.0", "HDMI-A-2");
    first.card_order = 1;
    first.order = 3;
    let mut second = monitor("pci-0000:00:02.0", "DP-2");
    second.card_order = 1;
    second.order = 5;
    let devices = [later, first, second];
    let p = config("version = 1\n[head a]\n[rules]\ndisplay * -> off\n").place(&devices, None);
    assert!(p.never_dark, "a connected display always lights");
    let lit: Vec<Place> = p.placed.iter().map(|p| p.place).collect();
    assert_eq!(
        lit,
        [Place::Off, Place::Head(0), Place::Off],
        "the first card's first connector"
    );
}

// covers: platform.heads.guards
#[test]
fn under_open_the_anti_lockout_guard_makes_every_keyboard_roam_while_none_reaches_a_lit_head() {
    let mut devices = desk_and_panel();
    let mouse = at_path(
        Identity::input().with_class(Class::Mouse),
        "pci-0000:15:00.0-usb-0:3.4.4:1.0",
    );
    devices.push(mouse);
    let stranded = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> off\ninput class:keyboard -> panel\ninput * -> desk\n";
    let p = config(stranded).place(&devices, None);
    assert!(p.anti_lockout);
    assert_eq!(
        p.placed[2],
        Placed {
            place: Place::Roaming,
            by: By::AntiLockout
        }
    );
    assert_eq!(
        p.placed[3],
        Placed {
            place: Place::Roaming,
            by: By::AntiLockout
        }
    );
    assert_eq!(
        p.placed[4].place,
        Place::Head(0),
        "a pointer stays where its rule put it"
    );

    let p = config(&closed(stranded)).place(&devices, None);
    assert!(!p.anti_lockout, "off under closed");
    assert_eq!(
        (p.placed[2].place, p.placed[3].place),
        (Place::Head(1), Place::Head(1))
    );

    let ignored =
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> ignore\n";
    let p = config(ignored).place(&devices, None);
    assert!(
        p.anti_lockout,
        "every keyboard ignored strands the lit head"
    );
    assert_eq!(p.placed[2].place, Place::Roaming);

    let one_reaches = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> off\ninput path:pci-0000:15:00.0-usb-0:3.3 -> desk\ninput class:keyboard -> panel\n";
    let p = config(one_reaches).place(&devices, None);
    assert!(
        !p.anti_lockout,
        "one keyboard reaching a lit head leaves the guard idle"
    );
    assert_eq!(p.placed[3].place, Place::Head(1));
}

// covers: platform.heads.guards
#[test]
fn a_device_marked_a_token_or_a_numpad_does_not_count_as_a_keyboard() {
    let mut devices = desk_and_panel();
    devices.truncate(3);
    let rules = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay * -> desk\ninput path:pci-0000:15:00.0-usb-0:3.3 -> desk\n";
    for role in ["token", "numpad"] {
        devices[2]
            .properties
            .insert("VTIUM_ROLE".into(), role.into());
        assert!(!devices[2].counts_as_keyboard());
        let p = config(rules).place(&devices, None);
        assert!(
            p.anti_lockout,
            "{role}: the head's only keyboard is marked, so none counts"
        );
        assert_eq!(p.placed[2].place, Place::Roaming);
    }
    devices[2]
        .properties
        .insert("VTIUM_ROLE".into(), "keyboard".into());
    assert!(!config(rules).place(&devices, None).anti_lockout);
}

// covers: platform.heads.rescue-key
#[test]
fn the_rescue_key_takes_a_keyboard_under_open_unless_its_head_is_isolated() {
    let kiosk = "version = 1\n[head desk]\n[head kiosk]\nisolated = yes\n";
    let c = config(kiosk);
    let placed = |place| Placed {
        place,
        by: By::NoMatch,
    };
    assert!(c.rescue(&placed(Place::Head(0))));
    assert!(!c.rescue(&placed(Place::Head(1))));
    assert!(c.rescue(&placed(Place::Ignore)));
    assert!(c.rescue(&placed(Place::DefaultHead)));
    assert!(
        !c.rescue(&placed(Place::Roaming)),
        "a keyboard already roaming stays"
    );
    assert!(!c.rescue(&placed(Place::Head(2))), "no such head");

    // under closed the rescue key is off for every head, one that writes
    // `isolated = no` included
    let off = config(&closed("version = 1\n[head desk]\nisolated = no\n"));
    for place in [Place::Head(0), Place::Ignore, Place::DefaultHead] {
        assert!(!off.rescue(&placed(place)), "{place:?}");
    }

    // what survives of an open file ignores `isolated`, so the rescue key
    // reaches every head's keyboards
    let partly = survivors(&format!("{kiosk}[rules]\ninput * -> nosuch\n"));
    assert_eq!(partly.policy, Policy::Open);
    assert!(partly.rescue(&placed(Place::Head(1))));
    let sticky = survivors(&closed(&format!("{kiosk}[rules]\ninput * -> nosuch\n")));
    assert!(
        !sticky.rescue(&placed(Place::Head(0))),
        "closed stays closed"
    );
}

// covers: platform.heads.rescue-key
#[test]
fn an_ignored_keyboard_is_opened_for_the_rescue_key_alone_under_open() {
    let c = Config::builtin();
    let ignored = Placed {
        place: Place::Ignore,
        by: By::NoMatch,
    };
    assert_eq!(
        c.disposition(&ignored, &keyboard()),
        Disposition::RescueOnly
    );
    let mouse = Identity::input().with_class(Class::Mouse);
    assert_eq!(c.disposition(&ignored, &mouse), Disposition::Closed);
    let roaming = Placed {
        place: Place::Roaming,
        by: By::NoMatch,
    };
    assert_eq!(c.disposition(&roaming, &mouse), Disposition::Deliver);
    let off = config(&closed("version = 1\n"));
    assert_eq!(
        off.disposition(&ignored, &keyboard()),
        Disposition::Closed,
        "under closed vtium opens no ignored device"
    );
    assert_eq!(off.disposition(&roaming, &mouse), Disposition::Deliver);
}

#[test]
fn layer_one_warns_of_pointer_only_heads_ignored_keyboards_and_single_keyboards() {
    let mut devices = desk_and_panel();
    devices.push(at_path(
        Identity::input().with_class(Class::Mouse),
        "pci-0000:15:00.0-usb-0:3.4.4:1.0",
    ));
    let c = config(
        "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> panel\ninput class:mouse -> desk\n",
    );
    assert_eq!(c.lockout(&devices), [Lockout::PointersOnly { head: 0 }]);

    let c = config(
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput path:pci-0000:15:00.0-usb-0:3.3 -> desk\n",
    );
    assert_eq!(
        c.lockout(&devices),
        [Lockout::OneKeyboard { head: 0, device: 2 }]
    );

    let c = config(
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> ignore\n",
    );
    assert_eq!(c.lockout(&devices), [Lockout::EveryKeyboardIgnored]);

    assert!(Config::builtin().lockout(&devices).is_empty());
}

#[test]
fn a_rule_matching_no_device_present_is_idle() {
    let c = config(
        "version = 1\n[head a]\n[rules]\ndisplay connector:*/DP-* -> a\ndisplay connector:*/VGA-* -> a\ninput class:joystick -> a\ninput class:keyboard -> roaming\n",
    );
    assert_eq!(c.idle_rules(&desk_and_panel()), [5, 6]);
}

#[test]
fn a_dark_displays_log_line_carries_a_rule_ready_to_paste() {
    let mut d = monitor("pci-0000:03:00.0", "DP-8");
    assert_eq!(
        rule_for(&d, "desk").as_deref(),
        Some("display connector:pci-0000:03:00.0/DP-8 -> desk")
    );
    // a PATH holding `*` or `?` would paste as a glob, so the rule names
    // the connector instead
    for glob in ["mst:213-*", "mst:213-?"] {
        d.connector_path = Some(glob.into());
        assert_eq!(
            rule_for(&d, "desk").as_deref(),
            Some("display connector:pci-0000:03:00.0/DP-8 -> desk"),
            "{glob}"
        );
    }
    d.connector_path = Some("mst:213-1".into());
    assert_eq!(
        rule_for(&d, "desk").as_deref(),
        Some("display connector-path:mst:213-1 -> desk")
    );
    d.edid = Some(Edid {
        maker: "AAA".into(),
        product: 1,
        serial: "LARGE00001".into(),
    });
    let line = rule_for(&d, "desk").expect("a rule");
    assert_eq!(line, "display edid:AAA:0x0001:LARGE00001 -> desk");
    let pasted = format!("version = 1\n[head desk]\n[rules]\n{line}\n");
    assert_eq!(
        config(&pasted).walk(&d).place,
        Place::Head(0),
        "the line places the display"
    );

    d.edid = Some(Edid {
        maker: "AAA".into(),
        product: 1,
        serial: "two words".into(),
    });
    assert_eq!(
        rule_for(&d, "desk").as_deref(),
        Some("display connector-path:mst:213-1 -> desk"),
        "a serial a token cannot hold is passed over"
    );
    assert_eq!(rule_for(&Identity::display(), "desk"), None);
    assert_eq!(rule_for(&keyboard(), "desk"), None);
}

// covers: platform.heads.guards, platform.heads.focus
#[test]
fn the_anti_lockout_guard_moves_the_active_head_off_a_head_whose_display_is_gone() {
    // the panel's display is gone; desk is lit; every keyboard roams,
    // and the active head is panel: no keyboard reaches a lit head
    let mut devices = desk_and_panel();
    devices.remove(1);
    let c = config(
        "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> panel\ninput class:keyboard -> roaming\n",
    );
    let p = c.place(&devices, Some(Place::Head(1)));
    assert!(
        p.anti_lockout,
        "the active head unlit strands every keyboard"
    );
    assert_eq!(
        p.focus,
        Some(Place::Head(0)),
        "the active head moves to the boot display's head"
    );
    assert_eq!(
        (p.placed[1].place, p.placed[2].place),
        (Place::Roaming, Place::Roaming)
    );

    // the first lit head in the file's order, when the boot display is in
    // none: desk's display is not the boot card's
    let mut off_boot = devices.clone();
    off_boot[0].on_boot_card = false;
    let three = config(
        "version = 1\n[head side]\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ninput class:keyboard -> roaming\n",
    );
    let p = three.place(&off_boot, Some(Place::Head(2)));
    assert_eq!(p.focus, Some(Place::Head(1)));

    let off = config(&closed(
        "version = 1\n[head desk]\nisolated = no\n[head panel]\nisolated = no\n[rules]\ndisplay connector:*/DP-* -> desk\ninput class:keyboard -> roaming\n",
    ));
    let p = off.place(&devices, Some(Place::Head(1)));
    assert!(!p.anti_lockout);
    assert_eq!(
        p.focus,
        Some(Place::Head(1)),
        "under closed the guard is off and the active head stays"
    );
}

// covers: platform.heads.focus
#[test]
fn a_keyboard_on_a_lit_head_leaves_the_guard_idle_and_the_active_head_where_it_is() {
    let devices = desk_and_panel();
    // keyboard A types into desk, which is lit; keyboard B roams, and the
    // active head may be panel, whose display is dark
    let c = config(
        "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> off\ninput path:pci-0000:15:00.0-usb-0:3.3 -> desk\ninput class:keyboard -> roaming\n",
    );
    for focus in [Some(Place::Head(0)), Some(Place::Head(1))] {
        let p = c.place(&devices, focus);
        assert!(!p.anti_lockout, "{focus:?}");
        assert_eq!(p.focus, focus, "the active head stays put, lit or not");
        assert_eq!(p.placed[2].place, Place::Head(0));
        assert_eq!(p.placed[3].place, Place::Roaming);
    }
}

// covers: platform.heads.focus
#[test]
fn with_no_active_head_yet_it_starts_on_the_boot_displays_head() {
    let devices = desk_and_panel();
    let p = Config::builtin().place(&devices, None);
    assert_eq!(p.focus, Some(Place::Head(0)));
    assert!(
        !p.anti_lockout,
        "a boot with keyboards and a lit head needs no guard"
    );
    let c = config(
        "version = 1\n[head side]\n[head desk]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> side\n",
    );
    assert_eq!(
        c.place(&devices, None).focus,
        Some(Place::Head(1)),
        "the boot display's head over the file's first"
    );
}

// covers: platform.heads.policy
#[test]
fn an_input_no_rule_matches_roams_under_open_and_is_ignored_under_closed() {
    let rules = "version = 1\n[head desk]\n[rules]\ndisplay connector:*/HDMI-* -> desk\ninput class:mouse -> desk\n";
    let joystick = Identity::input().with_class(Class::Joystick);
    let dp = monitor("pci-0000:03:00.0", "DP-8");
    for (text, want) in [
        (rules.to_owned(), Place::Roaming),
        (format!("policy = open\n{rules}"), Place::Roaming),
        (closed(rules), Place::Ignore),
    ] {
        let c = config(&text);
        for device in [keyboard(), joystick.clone()] {
            assert_eq!(
                c.walk(&device),
                Placed {
                    place: want,
                    by: By::NoMatch
                },
                "{text}"
            );
        }
        assert_eq!(
            c.walk(&dp),
            Placed {
                place: Place::Off,
                by: By::NoMatch
            },
            "a display no rule matches is off under either: {text}"
        );
        let mouse = Identity::input().with_class(Class::Mouse);
        assert_eq!(c.walk(&mouse).place, Place::Head(0), "a rule still places");
    }
}

// covers: platform.heads.policy, platform.heads.isolated
#[test]
fn an_unset_isolated_is_no_under_open_and_yes_under_closed_and_a_written_one_wins() {
    let heads = "version = 1\n[head a]\n[head b]\nisolated = yes\n[head c]\nisolated = no\n";
    let open = config(heads);
    assert_eq!(
        (open.isolated(0), open.isolated(1), open.isolated(2)),
        (false, true, false)
    );
    let shut = config(&closed(heads));
    assert_eq!(
        (shut.isolated(0), shut.isolated(1), shut.isolated(2)),
        (true, true, false)
    );
    assert!(!open.isolated(3) && !shut.isolated(3), "no such head");
}

// covers: platform.heads.isolated, platform.heads.focus
#[test]
fn the_active_head_starts_only_on_a_head_that_is_not_isolated() {
    // the large display, the boot display, is desk's, and the small one
    // panel's; both are lit
    let devices = desk_and_panel();
    let rules = "[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> panel\n";
    let file = |desk: &str, panel: &str| {
        format!("version = 1\n[head desk]\n{desk}[head panel]\n{panel}{rules}")
    };
    let yes = "isolated = yes\n";
    let no = "isolated = no\n";
    let start = |text: &str| config(text).place(&devices, None).focus;
    assert_eq!(
        start(&file("", "")),
        Some(Place::Head(0)),
        "the boot display's head"
    );
    assert_eq!(
        start(&file(yes, "")),
        Some(Place::Head(1)),
        "the boot display's head is isolated, so the next lit head"
    );
    assert_eq!(start(&file(yes, yes)), None, "every head isolated: none");
    assert_eq!(
        start(&closed(&file("", ""))),
        None,
        "closed isolates unset heads"
    );
    assert_eq!(
        start(&closed(&file("", no))),
        Some(Place::Head(1)),
        "a closed file's head that writes isolated = no"
    );
    // an active head given that can no longer be one starts again
    let c = config(&file("", yes));
    assert_eq!(
        c.place(&devices, Some(Place::Head(1))).focus,
        Some(Place::Head(0))
    );
    assert_eq!(
        c.place(&devices, Some(Place::Head(0))).focus,
        Some(Place::Head(0))
    );
}

// covers: platform.heads.guards, platform.heads.isolated
#[test]
fn the_anti_lockout_guard_makes_only_a_head_that_is_not_isolated_active() {
    // every keyboard is claimed by panel, which has no display; desk holds
    // the boot display, and side the other one
    let mut devices = desk_and_panel();
    devices[1].connector = Some("DP-1".to_owned());
    let file = |desk: &str| {
        format!(
            "version = 1\n[head side]\n[head desk]\n{desk}[head panel]\n[rules]\ndisplay connector:*/DP-8 -> desk\ndisplay * -> side\ninput class:keyboard -> panel\n"
        )
    };
    let p = config(&file("")).place(&devices, Some(Place::Head(2)));
    assert!(p.anti_lockout);
    assert_eq!(p.focus, Some(Place::Head(1)), "the boot display's head");
    let p = config(&file("isolated = yes\n")).place(&devices, Some(Place::Head(2)));
    assert!(p.anti_lockout);
    assert_eq!(
        p.focus,
        Some(Place::Head(0)),
        "desk is isolated, so the first lit head in the file that is not"
    );
    assert_eq!(
        (p.placed[2].place, p.placed[3].place),
        (Place::Roaming, Place::Roaming)
    );
}

// covers: platform.heads.guards
#[test]
fn with_no_keyboard_present_the_anti_lockout_guard_has_none_to_strand() {
    let mut devices = desk_and_panel();
    devices.truncate(2);
    devices.push(Identity::input().with_class(Class::Mouse));
    let c = config(
        "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> off\ninput * -> panel\n",
    );
    let p = c.place(&devices, Some(Place::Head(1)));
    assert!(!p.anti_lockout);
    assert_eq!(p.focus, Some(Place::Head(1)));
}

// covers: platform.heads.guards, platform.heads.rescue-key
#[test]
fn a_keyboard_the_rescue_key_moved_roams_and_counts_for_the_anti_lockout_guard() {
    // both keyboards present are claimed by panel, which has no display;
    // a third, rescued, roams
    let devices = {
        let mut d = desk_and_panel();
        d.remove(1);
        d
    };
    let rescued = [at_path(keyboard(), "pci-0000:15:00.0-usb-0:3.4.1:1.0")];
    let c = config(
        "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> panel\ninput class:keyboard -> panel\n",
    );
    let p = c.place_rescued(&devices, &rescued, Some(Place::Head(0)));
    assert!(!p.anti_lockout, "the rescued keyboard reaches desk, lit");
    assert_eq!(p.placed[1].place, Place::Head(1));
    let p = c.place_rescued(&devices, &rescued, Some(Place::Head(1)));
    assert!(p.anti_lockout, "the active head is unlit, so none reaches");
    assert_eq!(p.focus, Some(Place::Head(0)));
    // only rescued keyboards, the active head unlit: still one to strand
    let p = c.place_rescued(&devices[..1], &rescued, Some(Place::Head(1)));
    assert!(p.anti_lockout);
    assert_eq!(p.focus, Some(Place::Head(0)));
    assert_eq!(
        c.place(&devices, Some(Place::Head(0))),
        c.place_rescued(&devices, &[], Some(Place::Head(0)))
    );
}

// covers: platform.heads.guards
#[test]
fn under_closed_the_display_guard_lights_the_boot_display_for_a_notice_alone() {
    let devices = desk_and_panel();
    let dark = "version = 1\n[head desk]\nisolated = no\n[head panel]\n[rules]\ndisplay * -> off\ninput class:keyboard -> desk\n";
    let p = config(&closed(dark)).place(&devices, None);
    assert!(p.never_dark);
    assert_eq!(
        p.placed[0],
        Placed {
            place: Place::Notice,
            by: By::NeverDark
        },
        "never a head on a display its rules do not give it"
    );
    assert_eq!(p.placed[1].place, Place::Off);
    assert_eq!(p.focus, None, "no head is lit");
    assert!(!p.anti_lockout, "and the guard is off under closed");

    let p = config(&closed("version = 1\n[rules]\ndisplay * -> off\n")).place(&devices, None);
    assert!(
        !p.never_dark,
        "a closed file with no head is dark and silent"
    );
    assert_eq!(
        (p.placed[0].place, p.placed[1].place),
        (Place::Off, Place::Off)
    );

    // a closed file that failed has no head left, and shows its notice
    let failed = survivors(&closed("[head desk]\n[rules]\ndisplay * -> desk\n"));
    assert!(failed.heads.is_empty(), "no version drops the file");
    let p = failed.place(&devices, None);
    assert_eq!(p.placed[0].place, Place::Notice);
    assert_eq!(p.focus, None);
}

// covers: platform.heads.rules
#[test]
fn what_survives_of_a_partly_broken_open_file_runs_then_the_shipped_rules_tail() {
    let text = "version = 1\n[head desk]\nisolated = yes\n[rules]\ndisplay connector:*/DP-* -> desk\ninput class:mouse -> desk\ninput class:keyboard -> nosuch\n";
    let c = survivors(text);
    assert_eq!(
        (c.heads.len(), c.rules.len()),
        (1, 2),
        "the bad rule alone dropped"
    );
    let mouse = Identity::input().with_class(Class::Mouse);
    let joystick = Identity::input().with_class(Class::Joystick);
    let pad = Identity::input().with_class(Class::Key);
    assert_eq!(
        c.walk(&mouse),
        Placed {
            place: Place::Head(0),
            by: By::Rule(6)
        },
        "a surviving rule places first"
    );
    for (device, want) in [
        (keyboard(), Place::Roaming),
        (pad, Place::Roaming),
        (joystick.clone(), Place::Ignore),
        (Identity::input(), Place::Ignore),
    ] {
        assert_eq!(
            c.walk(&device),
            Placed {
                place: want,
                by: By::Tail
            },
            "{device:?}"
        );
    }
    assert_eq!(
        c.walk(&monitor("pci-0000:00:02.0", "HDMI-A-2")),
        Placed {
            place: Place::Off,
            by: By::NoMatch
        },
        "the tail places inputs alone"
    );
    assert!(
        !c.isolated(0),
        "isolated is ignored, so the rescue has a way in"
    );
    assert_eq!(c.place(&desk_and_panel(), None).focus, Some(Place::Head(0)));

    // a valid open file has no tail: what no rule matches roams
    let valid = config(&text.replace(" -> nosuch", " -> desk"));
    assert_eq!(valid.walk(&joystick).by, By::NoMatch);
    assert_eq!(valid.walk(&joystick).place, Place::Roaming);

    // what survives of a closed file runs as written: no tail, isolation
    // as the file writes it
    let shut = survivors(&closed(text));
    assert_eq!(
        shut.walk(&keyboard()),
        Placed {
            place: Place::Ignore,
            by: By::NoMatch
        }
    );
    assert!(shut.isolated(0));
}

// covers: platform.heads.policy
#[test]
fn placement_runs_the_switches_and_the_unplaced_inputs_the_outcome_gives() {
    use vtium_config::{Read, Runs, SwitchKey, Unplaced, resolve};
    let heads = "version = 1\n[head a]\nisolated = yes\n[head b]\nisolated = no\n[head c]\n[rules]\ndisplay * -> c\ninput class:keyboard -> b\n";
    let bad = "input * -> nosuch\n";
    let joystick = Identity::input().with_class(Class::Joystick);
    for (text, runs) in [
        (heads.to_owned(), Runs::File),
        (closed(heads), Runs::File),
        (format!("{heads}{bad}"), Runs::Survivors),
        (closed(&format!("{heads}{bad}")), Runs::Survivors),
    ] {
        let r = reading(text.as_bytes());
        let outcome = resolve(&Read::Trusted(r.trusted()));
        assert_eq!(outcome.runs, runs, "{text}");
        let c = r.parsed.config;
        assert_eq!(c.policy, outcome.policy, "{text}");
        for (i, head) in c.heads.iter().enumerate() {
            let isolated = outcome.switch(SwitchKey::Isolated).of(head.isolated);
            assert_eq!(c.isolated(i), isolated, "{text} head {i}");
            let rescue = outcome.switch(SwitchKey::RescueKey).of(None) && !isolated;
            let claimed = Placed {
                place: Place::Head(i),
                by: By::NoMatch,
            };
            assert_eq!(c.rescue(&claimed), rescue, "{text} head {i}");
        }
        // the only keyboard is b's, and only c's display is lit
        let stranded = [monitor("pci-0000:03:00.0", "DP-8"), keyboard()];
        assert_eq!(
            c.place(&stranded, None).anti_lockout,
            outcome.switch(SwitchKey::AntiLockout).of(None),
            "{text}"
        );
        let (keys, other) = match outcome.unplaced {
            Unplaced::Roaming => (Place::Roaming, Place::Roaming),
            Unplaced::Shipped => (Place::Roaming, Place::Ignore),
            Unplaced::Ignore => (Place::Ignore, Place::Ignore),
        };
        let pad = Identity::input().with_class(Class::Key);
        assert_eq!(c.walk(&pad).place, keys, "{text}");
        assert_eq!(c.walk(&joystick).place, other, "{text}");
    }
}

// covers: platform.heads.guards, platform.heads.isolated
#[test]
fn with_no_lit_head_that_can_be_active_the_anti_lockout_guard_moves_nothing() {
    // desk is lit and isolated; panel, unlit, claims the keyboard, which
    // keeps typing into it (D8)
    let mut devices = desk_and_panel();
    devices.truncate(3);
    let c = config(
        "version = 1\n[head desk]\nisolated = yes\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> off\ninput class:keyboard -> panel\n",
    );
    for focus in [None, Some(Place::Head(1))] {
        let p = c.place(&devices, focus);
        assert!(!p.anti_lockout, "{focus:?}");
        assert_eq!(
            p.placed[2],
            Placed {
                place: Place::Head(1),
                by: By::Rule(8)
            }
        );
        assert_eq!(p.focus, focus, "panel can be the active head, unlit");
    }
}

// covers: platform.heads.policy
#[test]
fn the_default_head_is_never_the_active_head_under_closed() {
    let devices = desk_and_panel();
    let shut = config(&closed("version = 1\n"));
    let p = shut.place(&devices, Some(Place::DefaultHead));
    assert_eq!(p.focus, None);
    assert!(!shut.place(&devices, None).never_dark);
    let open = config("version = 1\n");
    assert_eq!(
        open.place(&devices, Some(Place::DefaultHead)).focus,
        Some(Place::DefaultHead)
    );
}
