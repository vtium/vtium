//! What the TUI's tests, its screen goldens and `make tui-demo` start
//! from. [`flow`] is the flow of ADR-1403 D7's figure: the machine of D4's
//! example, two displays and one keyboard, with one more input rule for a
//! keyboard that is unplugged, and a look no head uses yet. [`inputs`] is
//! what the TUI opens on around it. [`DRAFT`] and [`CHECKER`] are the
//! installed vtium.conf and the stand-in console of the goldens that open
//! a draft.

use std::path::PathBuf;

use super::flow::{Base, Device, Draft, Flow, Head, Kind, Look, Rule, RuleState};
use super::model::{DpiFrom, Font, FontState, Inputs, View};

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

/// The sections of the fixture's vtium.conf: a look that draws the frit
/// proof, the head desk drawing it, and the head panel drawing the default
/// look (ADR-1403 D4).
pub const SECTIONS: &str = "version = 1\n[look proof]\nfont-name = proof\n[head desk]\nlook = proof\n[head panel]\n[rules]\ndisplay * -> desk\n";

/// What the TUI opens on: the fonts of a machine with three frits, one that
/// does not read, its vtium.conf, and [`flow`] in the Flow view, with no
/// desk, so the Flow view changes nothing.
pub fn inputs() -> Inputs {
    let font = |name: &str, state| Font {
        name: name.to_owned(),
        state,
    };
    Inputs {
        fonts: vec![
            font(
                "broken",
                FontState::Refused("line 2: unknown key colour".into()),
            ),
            font("minspec-16", FontState::Reads),
            font("proof", FontState::Reads),
            font("tiny5", FontState::Reads),
            font("unifont", FontState::BuiltIn),
            font("unscii-8", FontState::BuiltIn),
        ],
        conf_path: PathBuf::from("/etc/vtium/vtium.conf"),
        conf: Some(format!("# mine\nfont-name=unifont\n{SECTIONS}")),
        dpi: (96, DpiFrom::Assumed),
        frits: PathBuf::from("/etc/vtium/fonts"),
        view: View::Fonts,
        display: Some((640, 480)),
        flow: flow(),
        color: true,
        desk: false,
        hotplug: false,
        installed: [
            ("broken", &b"source=builtin:unifont\ncolour=red\n"[..]),
            ("minspec-16", b"source=builtin:unifont\n"),
            ("proof", b"source=font:JetBrains Mono\n"),
            ("tiny5", b"source=bdf:/x/Tiny5Mono.bdf\n"),
            ("latin1", b"source=kbd:lat1-16 # \xe9\n"),
        ]
        .into_iter()
        .map(|(n, b)| (n.to_owned(), b.to_vec()))
        .collect(),
    }
}

/// The installed vtium.conf of the goldens that open a draft: two heads,
/// a look, and three rules, the second naming the keyboard at `pci-0:3.3`.
pub const DRAFT: &str = "font-size=12\nversion = 1\n[look big]\nfont-name = proof\n[head desk]\nlook = big\n[head panel]\n[rules]\ndisplay * -> desk\ninput path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming\n";

/// The stand-in console those goldens check a draft with: `check-config
/// --configdir DIR FILE` names each line of FILE that holds `BAD` and
/// fails, and passes any other file.
pub const CHECKER: &str = r#"#!/bin/sh
[ "$1" = check-config ] || exit 9
file="$4"
bad=$(grep -n BAD "$file" | cut -d: -f1)
if [ -n "$bad" ]; then
  for n in $bad; do
    echo "$file:$n: BAD is not a key vtium reads" >&2
  done
  exit 1
fi
echo "$file: checked"
"#;
