//! Where each device lands: the first-match walk, each match key, and the
//! guards over a set of devices (ADR-1401 D3, D4, D9).

use vtium_config::{
    By, Class, Config, Disposition, Edid, Identity, Lockout, Place, Placed, UsbSerial, glob, parse,
    rule_for,
};

fn config(text: &str) -> Config {
    match parse(text) {
        Ok(p) => p.config,
        Err(e) => panic!("{e:?}\n{text}"),
    }
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

#[test]
fn the_shipped_rules_send_keyboards_keys_and_pointers_to_the_focus_and_close_the_rest() {
    let builtin = Config::builtin();
    let want = |name: &str| match name {
        "joystick" | "tablet" | "lid switch" | "accelerometer" | "no class" => Place::Ignore,
        _ => Place::Focus,
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
            place: Place::Focus,
            by: By::NoMatch
        }
    );
    let dp = monitor("pci-0000:03:00.0", "DP-8");
    assert_eq!(
        c.walk(&dp),
        Placed {
            place: Place::Dark,
            by: By::NoMatch
        }
    );
    let hdmi = monitor("pci-0000:00:02.0", "HDMI-A-2");
    assert_eq!(c.walk(&hdmi).place, Place::Head(1));
}

#[test]
fn a_rule_matches_only_its_own_kind() {
    let c = config("version = 1\n[head a]\n[rules]\ninput * -> a\n");
    assert_eq!(c.walk(&Identity::display()).place, Place::Dark);
    assert_eq!(c.walk(&Identity::input()).place, Place::Head(0));
}

#[test]
fn a_rules_tokens_must_all_hold() {
    let c = config(
        "version = 1\n[head a]\n[rules]\ninput path:pci-0000:15:00.0-usb-0:3.4.1 serial:aaaa:0002:NUMPAD0001 -> a\n",
    );
    let mut unit = at_path(keyboard(), "pci-0000:15:00.0-usb-0:3.4.1:1.0");
    assert_eq!(c.walk(&unit).place, Place::Focus, "no serial");
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
        Place::Focus,
        "the unit in another port"
    );
}

#[test]
fn path_takes_every_interface_of_its_device_and_nothing_beside_it() {
    let c = config("version = 1\n[head a]\n[rules]\ninput path:pci-0000:15:00.0-usb-0:3.3 -> a\n");
    for (path, want) in [
        ("pci-0000:15:00.0-usb-0:3.3", Place::Head(0)),
        ("pci-0000:15:00.0-usb-0:3.3:1.0", Place::Head(0)),
        ("pci-0000:15:00.0-usb-0:3.3:1.2", Place::Head(0)),
        ("pci-0000:15:00.0-usb-0:3.30:1.0", Place::Focus),
        ("pci-0000:15:00.0-usb-0:3.3.1:1.0", Place::Focus),
        ("pci-0000:15:00.0-usb-0:3", Place::Focus),
    ] {
        assert_eq!(c.walk(&at_path(keyboard(), path)).place, want, "{path}");
    }
    assert_eq!(
        c.walk(&keyboard()).place,
        Place::Focus,
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
        Place::Focus,
        "a unique id matches as written"
    );
    let mut wrong = keyboard();
    wrong.properties.insert("VTIUM_HEAD".into(), "desk".into());
    assert_eq!(c.walk(&wrong).place, Place::Focus);
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
        Place::Dark
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

#[test]
fn the_display_guard_lights_the_boot_display_in_the_first_head() {
    let devices = desk_and_panel();
    let dark = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay * -> dark\n";
    let p = config(dark).place(&devices, None);
    assert!(p.never_dark);
    assert_eq!(
        p.placed[0],
        Placed {
            place: Place::Head(0),
            by: By::NeverDark
        }
    );
    assert_eq!(p.placed[1].place, Place::Dark);

    let mut lit = devices.clone();
    lit[1].lit_at_start = true;
    let p = config(dark).place(&lit, None);
    assert_eq!(
        p.placed[1].place,
        Place::Head(0),
        "the connector lit at start wins"
    );
    assert_eq!(p.placed[0].place, Place::Dark);

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

    let off = format!("{dark}[guards]\nnever-dark = off\n");
    let p = config(&off).place(&devices, None);
    assert!(!p.never_dark);
    assert_eq!(
        (p.placed[0].place, p.placed[1].place),
        (Place::Dark, Place::Dark)
    );

    let p = config(
        "version = 1\n[head a]\n[rules]\ndisplay connector:*/HDMI-* -> a\ndisplay * -> dark\n",
    )
    .place(&devices, None);
    assert!(
        !p.never_dark,
        "a display the rules light leaves the guard idle"
    );
    assert_eq!(
        (p.placed[0].place, p.placed[1].place),
        (Place::Dark, Place::Head(0))
    );
}

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
    let p = config("version = 1\n[head a]\n[rules]\ndisplay * -> dark\n").place(&devices, None);
    assert!(p.never_dark, "a connected display always lights");
    let lit: Vec<Place> = p.placed.iter().map(|p| p.place).collect();
    assert_eq!(
        lit,
        [Place::Dark, Place::Head(0), Place::Dark],
        "the first card's first connector"
    );
}

#[test]
fn the_anti_lockout_guard_sends_every_keyboard_to_the_focus_while_none_reaches_a_lit_head() {
    let mut devices = desk_and_panel();
    let mouse = at_path(
        Identity::input().with_class(Class::Mouse),
        "pci-0000:15:00.0-usb-0:3.4.4:1.0",
    );
    devices.push(mouse);
    let stranded = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> dark\ninput class:keyboard -> panel\ninput * -> desk\n";
    let p = config(stranded).place(&devices, None);
    assert!(p.anti_lockout);
    assert_eq!(
        p.placed[2],
        Placed {
            place: Place::Focus,
            by: By::AntiLockout
        }
    );
    assert_eq!(
        p.placed[3],
        Placed {
            place: Place::Focus,
            by: By::AntiLockout
        }
    );
    assert_eq!(
        p.placed[4].place,
        Place::Head(0),
        "a pointer stays where its rule put it"
    );

    let off = format!("{stranded}[guards]\nanti-lockout = off\n");
    let p = config(&off).place(&devices, None);
    assert!(!p.anti_lockout);
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
    assert_eq!(p.placed[2].place, Place::Focus);

    let one_reaches = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> dark\ninput path:pci-0000:15:00.0-usb-0:3.3 -> desk\ninput class:keyboard -> panel\n";
    let p = config(one_reaches).place(&devices, None);
    assert!(
        !p.anti_lockout,
        "one keyboard reaching a lit head leaves the guard idle"
    );
    assert_eq!(p.placed[3].place, Place::Head(1));
}

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
        assert_eq!(p.placed[2].place, Place::Focus);
    }
    devices[2]
        .properties
        .insert("VTIUM_ROLE".into(), "keyboard".into());
    assert!(!config(rules).place(&devices, None).anti_lockout);
}

#[test]
fn the_rescue_key_takes_a_keyboard_unless_its_head_or_the_guard_says_off() {
    let c = config("version = 1\n[head desk]\n[head kiosk]\nrescue = off\n");
    let placed = |place| Placed {
        place,
        by: By::NoMatch,
    };
    assert!(c.rescue(&placed(Place::Head(0))));
    assert!(!c.rescue(&placed(Place::Head(1))));
    assert!(c.rescue(&placed(Place::Ignore)));
    assert!(c.rescue(&placed(Place::DefaultHead)));
    assert!(
        !c.rescue(&placed(Place::Focus)),
        "a keyboard already following the focus stays"
    );
    let off = config("version = 1\n[head desk]\n[guards]\nrescue-key = off\n");
    assert!(!off.rescue(&placed(Place::Head(0))));
    assert!(!off.rescue(&placed(Place::Ignore)));
}

#[test]
fn an_ignored_keyboard_is_opened_for_the_rescue_key_alone() {
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
    let focus = Placed {
        place: Place::Focus,
        by: By::NoMatch,
    };
    assert_eq!(c.disposition(&focus, &mouse), Disposition::Deliver);
    let off = config("version = 1\n[guards]\nrescue-key = off\n");
    assert_eq!(off.disposition(&ignored, &keyboard()), Disposition::Closed);
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
        "version = 1\n[head a]\n[rules]\ndisplay connector:*/DP-* -> a\ndisplay connector:*/VGA-* -> a\ninput class:joystick -> a\ninput class:keyboard -> focus\n",
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

#[test]
fn the_kernel_command_lines_word_runs_the_builtin_configuration() {
    use vtium_config::{HeadsWord, heads_word};
    assert_eq!(heads_word("quiet rw"), HeadsWord::File);
    assert_eq!(
        heads_word("quiet vtium.heads=builtin rw"),
        HeadsWord::Builtin
    );
    assert_eq!(
        heads_word("vtium.heads=builtin vtium.heads=file"),
        HeadsWord::Unknown
    );
    assert_eq!(
        heads_word("vtium.heads=file vtium.heads=builtin"),
        HeadsWord::Builtin
    );
    assert_eq!(
        heads_word("vtium.headsbuiltin xvtium.heads=builtin"),
        HeadsWord::File
    );
}

#[test]
fn the_anti_lockout_guard_moves_the_focus_off_a_head_whose_display_is_gone() {
    // the panel's display is gone; desk is lit; every keyboard follows the
    // focus, which rests on panel: no keyboard reaches a lit head
    let mut devices = desk_and_panel();
    devices.remove(1);
    let c = config(
        "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> panel\ninput class:keyboard -> focus\n",
    );
    let p = c.place(&devices, Some(Place::Head(1)));
    assert!(
        p.anti_lockout,
        "the focus on an unlit head strands every keyboard"
    );
    assert_eq!(
        p.focus,
        Some(Place::Head(0)),
        "the focus moves to the boot display's head"
    );
    assert_eq!(
        (p.placed[1].place, p.placed[2].place),
        (Place::Focus, Place::Focus)
    );

    // the first lit head in the file's order, when the boot display is in
    // none: desk's display is not the boot card's
    let mut off_boot = devices.clone();
    off_boot[0].on_boot_card = false;
    let three = config(
        "version = 1\n[head side]\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ninput class:keyboard -> focus\n[guards]\nnever-dark = off\n",
    );
    let p = three.place(&off_boot, Some(Place::Head(2)));
    assert_eq!(p.focus, Some(Place::Head(1)));

    let off = config(
        "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ninput class:keyboard -> focus\n[guards]\nanti-lockout = off\n",
    );
    let p = off.place(&devices, Some(Place::Head(1)));
    assert!(!p.anti_lockout);
    assert_eq!(
        p.focus,
        Some(Place::Head(1)),
        "with the guard off the focus stays"
    );
}

#[test]
fn a_keyboard_on_a_lit_head_leaves_the_guard_idle_and_the_focus_where_it_is() {
    let devices = desk_and_panel();
    // keyboard A types into desk, which is lit; keyboard B follows the
    // focus on panel, whose display is dark
    let c = config(
        "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay connector:*/DP-* -> desk\ndisplay * -> dark\ninput path:pci-0000:15:00.0-usb-0:3.3 -> desk\ninput class:keyboard -> focus\n",
    );
    for focus in [Some(Place::Head(0)), Some(Place::Head(1))] {
        let p = c.place(&devices, focus);
        assert!(!p.anti_lockout, "{focus:?}");
        assert_eq!(p.focus, focus, "the focus stays put, lit or not");
        assert_eq!(p.placed[2].place, Place::Head(0));
        assert_eq!(p.placed[3].place, Place::Focus);
    }
}

#[test]
fn with_no_focus_yet_it_starts_on_the_boot_displays_head() {
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
