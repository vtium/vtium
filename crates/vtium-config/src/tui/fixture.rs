//! The flow of ADR-1403 D7's figure, which the tests and the screen
//! goldens draw: the machine of D4's example, two displays and one
//! keyboard, with one more input rule for a keyboard that is unplugged, and
//! a look no head uses yet.

use super::flow::{Base, Device, Draft, Flow, Head, Kind, Look, Rule, RuleState};

fn rule(kind: Kind, line: usize, selector: &str, target: &str, state: RuleState) -> Rule {
    Rule {
        kind,
        line,
        selector: selector.into(),
        target: target.into(),
        state,
    }
}

fn device(name: &str, detail: &str, keys: &[&str], found: bool, rule: usize) -> Device {
    Device {
        name: name.into(),
        detail: detail.into(),
        keys: keys.iter().map(|k| (*k).to_owned()).collect(),
        found,
        rule: Some(rule),
    }
}

/// The figure's flow.
pub fn flow() -> Flow {
    use Kind::{Display, Input};
    use RuleState::{Matches, MatchesNothing};
    Flow {
        base: Base {
            set: [
                ("policy", "open"),
                ("font-size", "12"),
                ("xkb-layout", "us"),
                ("mouse", ""),
            ]
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .to_vec(),
            defaults: 101,
        },
        displays: vec![
            device(
                "DP-8",
                "LARGE00001",
                &["edid:AAA:0x0001:LARGE00001", "connector:card1/DP-8"],
                true,
                0,
            ),
            device(
                "HDMI-A-2",
                "SMALL00001",
                &["edid:BBB:0x0002:SMALL00001", "connector:card1/HDMI-A-2"],
                true,
                1,
            ),
        ],
        inputs: vec![
            device("046d:c31c:0", "", &[], false, 3),
            device(
                "keyboard",
                "usb-0:3.3",
                &[
                    "path:pci-0000:15:00.0-usb-0:3.3",
                    "id:046d:c31c",
                    "class:keyboard",
                ],
                true,
                4,
            ),
            device(
                "Power Button",
                "class key",
                &[
                    "path:LNXPWRBN/button/input0",
                    "name:Power Button",
                    "class:key",
                ],
                true,
                5,
            ),
        ],
        rules: vec![
            rule(Display, 19, "edid:AAA:0x0001:LARGE00001", "desk", Matches),
            rule(Display, 20, "edid:BBB:0x0002:SMALL00001", "panel", Matches),
            rule(Display, 21, "*", "desk", MatchesNothing),
            rule(Input, 22, "serial:046d:c31c:0", "desk", MatchesNothing),
            rule(Input, 23, "class:keyboard", "roaming", Matches),
            rule(Input, 24, "*", "ignore", Matches),
        ],
        heads: vec![
            Head {
                name: "desk".into(),
                consoles: 1,
                look: "big".into(),
                keys: Vec::new(),
            },
            Head {
                name: "panel".into(),
                consoles: 2,
                look: "small".into(),
                keys: Vec::new(),
            },
        ],
        looks: vec![
            Look {
                name: "big".into(),
                frit: "monoid".into(),
                points: 14,
                keys: Vec::new(),
                fonts: vec![("Monoid".into(), true)],
                problem: None,
            },
            Look {
                name: "small".into(),
                frit: "dejavu".into(),
                points: 12,
                keys: Vec::new(),
                fonts: vec![("DejaVu Sans Mono".into(), true)],
                problem: None,
            },
            Look {
                name: "tiny".into(),
                frit: "minspec-16".into(),
                points: 0,
                keys: Vec::new(),
                fonts: Vec::new(),
                problem: None,
            },
        ],
        active: Some("desk".into()),
        policy: "open".into(),
        draft: Some(Draft {
            name: "workstation".into(),
            changes: 1,
            problems: 0,
        }),
        read: true,
    }
}
