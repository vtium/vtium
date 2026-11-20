//! What the TUI's tests, its screen goldens and `make tui-demo` start
//! from. [`flow`] is the flow of ADR-1403 D7's figure: the machine of D4's
//! example, two displays and one keyboard, with one more input rule for a
//! keyboard that is unplugged, and a look no head uses yet. [`inputs`] is
//! what the TUI opens on around it. [`DRAFT`] and [`CHECKER`] are the
//! installed vtium.conf and the stand-in console of the goldens that open
//! a draft.

use std::path::PathBuf;

use super::effects_found::{Effects, From, Vtlook};
use super::flow::{Base, Device, Draft, Flow, Head, Kind, Look, Rule, RuleState};
use super::model::{DpiFrom, Font, FontState, Inputs, View};
use super::resources::Resource;
use crate::batch::State;

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
                isolated: false,
            },
            Head {
                name: "panel".into(),
                consoles: 2,
                look: "small".into(),
                keys: Vec::new(),
                isolated: false,
            },
        ],
        looks: vec![
            Look {
                name: "big".into(),
                frit: "monoid".into(),
                points: 14,
                keys: Vec::new(),
                vtlook: None,
                fonts: vec![("Monoid".into(), true)],
                problem: None,
            },
            Look {
                name: "small".into(),
                frit: "dejavu".into(),
                points: 12,
                keys: Vec::new(),
                vtlook: None,
                fonts: vec![("DejaVu Sans Mono".into(), true)],
                problem: None,
            },
            Look {
                name: "tiny".into(),
                frit: "minspec-16".into(),
                points: 0,
                keys: Vec::new(),
                vtlook: None,
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
        labels: Vec::new(),
    }
}

/// The sections of the fixture's vtium.conf: a look that draws the frit
/// proof, the head desk drawing it, and the head panel drawing the default
/// look (ADR-1403 D4).
pub const SECTIONS: &str = "version = 1\n[look proof]\nfont-name = proof\nvtlook = sepia\n[head desk]\nlook = proof\n[head panel]\n[rules]\ndisplay * -> desk\n";

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
        flow_colors: super::flowcolor::Palette::console(),
        colors_note: None,
        head: None,
        locale: String::new(),
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
        effects: effects(),
        font_files: font_files(),
    }
}

/// The font files the fixture's Fonts tab lists, in the scan's order:
/// DejaVu Sans Mono, active; a Terminus strike, not active; Noto Color
/// Emoji in the administrator's folder, which fails the trust rule and
/// repeats the package's file of the same bytes; vtium's own Cullet Mono,
/// active; and a font in the person's own folder, which vtium cannot use.
/// None is described until [`described`] answers for it.
pub fn font_files() -> super::fonts_found::Fonts {
    super::fonts_found::Fonts::new(found())
}

/// The font in the person's own folder that the fixture lists.
pub const OWN_FONT: &str = "/home/user/.local/share/fonts/Mine-Regular.ttf";

/// Where installing [`OWN_FONT`] as a system font puts it.
pub const OWN_FONT_INSTALLED: &str = "/usr/local/share/fonts/Mine-Regular.ttf";

/// The command a stand-in desk shows before it installs [`OWN_FONT`].
pub const OWN_FONT_COMMAND: &str =
    "sudo vtium-config apply /home/user/.config/vtium/drafts/2026-11-19-1";

/// What a stand-in desk answers [`super::desk::Op::SystemFont`] with: the
/// install of [`OWN_FONT`], new to the system's folder. It refuses every
/// other file, as the real desk refuses a font a package installed.
pub fn system_font_op(path: &std::path::Path) -> Result<super::desk::Done, String> {
    if path != std::path::Path::new(OWN_FONT) {
        return Err(format!(
            "{}: a package installed this font in /usr/share/fonts, so it is a system font already.",
            path.display()
        ));
    }
    Ok(super::desk::Done::SystemFont(super::desk::FontReady {
        source: OWN_FONT.to_owned(),
        target: OWN_FONT_INSTALLED.to_owned(),
        placement: crate::apply::Placement::New,
        facts: None,
        command: OWN_FONT_COMMAND.to_owned(),
    }))
}

/// What a stand-in desk's install of [`OWN_FONT`] gives: the font in the
/// system's folder, found by a scan afterwards beside the fixture's fonts.
pub fn font_installed() -> super::desk::Done {
    let mut found = found();
    let new = crate::discover::Found {
        path: PathBuf::from(OWN_FONT_INSTALLED),
        folder: crate::discover::Folder {
            path: PathBuf::from("/usr/local/share/fonts"),
            place: crate::discover::Place::System,
        },
        trust: Ok(()),
        state: State::Inactive,
        duplicate: None,
    };
    found.insert(3.min(found.len()), new);
    super::desk::Done::FontInstalled {
        target: PathBuf::from(OWN_FONT_INSTALLED),
        said: format!("wrote {OWN_FONT_INSTALLED}"),
        found,
    }
}

/// The scan's answer behind [`font_files`].
fn found() -> Vec<crate::discover::Found> {
    use crate::discover::{Duplicate, Folder, Found, Place};
    let found = |path: &str, place, state, trust: Result<(), String>| Found {
        path: PathBuf::from(path),
        folder: Folder {
            path: PathBuf::from("/"),
            place,
        },
        trust,
        state,
        duplicate: None,
    };
    let mut emoji = found(
        "/usr/local/share/fonts/NotoColorEmoji.ttf",
        Place::System,
        State::Inactive,
        Err("/usr/local/share/fonts/NotoColorEmoji.ttf: the file belongs to user 1000, and only root may own it.".to_owned()),
    );
    emoji.duplicate = Some(Duplicate {
        of: PathBuf::from("/usr/share/fonts/noto/NotoColorEmoji.ttf"),
        same_bytes: true,
    });
    vec![
        found(
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            Place::System,
            State::Active,
            Ok(()),
        ),
        found(
            "/usr/share/fonts/misc/ter-u16n.otb",
            Place::System,
            State::Inactive,
            Ok(()),
        ),
        emoji,
        found(
            "/usr/share/vtium/fonts/cullet-mono.otf",
            Place::Vtium,
            State::Active,
            Ok(()),
        ),
        found(OWN_FONT, Place::User, State::Inactive, Ok(())),
    ]
}

/// What a stand-in font helper says of the fixture's font files: each
/// face as the real helper would describe it.
pub fn described(path: &std::path::Path) -> crate::discover::Described {
    use crate::describe::{Description, Face, Format, Group, Strike};
    use crate::discover::Described;
    let face =
        |family: &str, outline, monospaced, scripts: &[&'static str], groups: &[Group]| Face {
            family: family.to_owned(),
            style: "Regular".to_owned(),
            version: Some("2.37".to_owned()),
            outline,
            monospaced,
            scripts: scripts.to_vec(),
            groups: groups.to_vec(),
            strikes: Vec::new(),
            codepoints: 3_000,
            coverage: vec![(0x20, 0x7e)],
            coverage_cut: false,
        };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let faces = match name {
        "DejaVuSansMono.ttf" => vec![face(
            "DejaVu Sans Mono",
            true,
            true,
            &["Latin", "Greek", "Cyrillic"],
            &[Group::BoxDrawing, Group::Powerline],
        )],
        "ter-u16n.otb" => vec![Face {
            strikes: vec![Strike {
                width: 8,
                height: 16,
            }],
            ..face(
                "Terminus",
                false,
                true,
                &["Latin", "Cyrillic"],
                &[Group::BoxDrawing],
            )
        }],
        "NotoColorEmoji.ttf" => vec![face(
            "Noto Color Emoji",
            true,
            false,
            &[],
            &[Group::ColorEmoji],
        )],
        "cullet-mono.otf" => vec![face(
            "Cullet Mono",
            true,
            true,
            &["Latin", "Greek", "Cyrillic"],
            &[Group::BoxDrawing, Group::NerdFonts],
        )],
        "Mine-Regular.ttf" => vec![face("Mine", true, false, &["Latin"], &[])],
        _ => return Described::NotAFont("there is no such file".to_owned()),
    };
    Described::Font(Description {
        format: Format::OpenType,
        faces,
    })
}

/// What the fixture's Effects tab lists: No filter; the presets amber and
/// green; the administrator's sepia, which names the color map sepia.cube,
/// and green, which does not read and hides the preset; the color maps
/// sepia.cube, active, warm.cube, not active, loose.cube, which fails the
/// trust rule, and mine.cube in the person's own folder; and no shader.
pub fn effects() -> Effects {
    let vtlook =
        |name: &str, from, hidden, read: Result<Vec<String>, String>, files: &[&str]| Vtlook {
            name: name.to_owned(),
            file: match from {
                From::BuiltIn => None,
                From::Preset => Some(PathBuf::from(format!(
                    "/usr/share/vtium/looks/{name}.vtlook"
                ))),
                From::Administrator => {
                    Some(PathBuf::from(format!("/etc/vtium/looks/{name}.vtlook")))
                }
            },
            from,
            hidden,
            read,
            files: files.iter().map(PathBuf::from).collect(),
        };
    let tint = |color: &str| Ok(vec![format!("tint: color {color}, amount 1, floor 0")]);
    let map = |name: &str, state, trust: Result<(), String>, own| Resource {
        path: PathBuf::from(if own {
            format!("/home/user/.config/vtium/looks/{name}")
        } else {
            format!("/etc/vtium/looks/{name}")
        }),
        kind: "color map",
        state,
        trust,
        own,
        hidden: false,
    };
    Effects {
        vtlooks: vec![
            vtlook("default", From::BuiltIn, false, Ok(Vec::new()), &[]),
            vtlook("amber", From::Preset, false, tint("255,176,0"), &[]),
            vtlook("green", From::Preset, true, tint("51,255,102"), &[]),
            vtlook(
                "green",
                From::Administrator,
                false,
                Err("Line 2: there is no color filter named glow.".to_owned()),
                &[],
            ),
            vtlook(
                "sepia",
                From::Administrator,
                false,
                Ok(vec!["cube: file sepia.cube".to_owned()]),
                &["/etc/vtium/looks/sepia.cube"],
            ),
        ],
        maps: vec![
            map("sepia.cube", State::Active, Ok(()), false),
            map("warm.cube", State::Inactive, Ok(()), false),
            map(
                "loose.cube",
                State::Inactive,
                Err("/etc/vtium/looks/loose.cube: the file belongs to user 1000, and only root may own it.".to_owned()),
                false,
            ),
            map("mine.cube", State::Inactive, Ok(()), true),
        ],
        shaders: Vec::new(),
        note: None,
    }
}

/// What a stand-in desk answers [`super::desk::Op::Resource`] with, on
/// `states`, the resources that are active or that the draft changes: the
/// draft's change as the real desk keeps it, an activation of a resource
/// the draft deactivates taking the deactivation back. It refuses what
/// the real batch refuses: an activation of a file the system does not
/// hold that fails the trust rule, as the fixture's [`effects`] and
/// [`font_files`] mark it,
/// and a deactivation of a resource that is not active. Its messages name
/// the kind of resource as the real desk's do.
pub fn resource_op(
    states: &mut Vec<(PathBuf, State)>,
    path: &std::path::Path,
    change: crate::batch::Change,
) -> Result<super::desk::Done, String> {
    use crate::batch::Change;
    let kind = crate::batch::kind(path);
    let now = states
        .iter()
        .find(|(p, _)| p == path)
        .map_or(State::Inactive, |(_, s)| *s);
    let said = match (change, now) {
        (Change::Activate, State::Activates) => Some(super::words::already_activates(kind)),
        (Change::Activate, State::Active | State::Always) => {
            Some(super::words::already_active(kind))
        }
        (Change::Deactivate, State::Inactive | State::Deactivates) => {
            return Err(format!(
                "{} is not active, so there is nothing to deactivate.",
                path.display()
            ));
        }
        (Change::Activate, State::Inactive) => {
            let trust = effects()
                .resource(path)
                .map(|r| r.trust.clone())
                .or_else(|| font_files().file(path).map(|f| f.resource.trust.clone()));
            if let Some(Err(why)) = trust {
                return Err(why);
            }
            None
        }
        _ => None,
    };
    if said.is_none() {
        let after = match (change, now) {
            (Change::Activate, State::Deactivates) => Some(State::Active),
            (Change::Activate, _) => Some(State::Activates),
            (Change::Deactivate, State::Active) => Some(State::Deactivates),
            (Change::Deactivate, _) => None,
        };
        states.retain(|(p, _)| p != path);
        if let Some(after) = after {
            states.push((path.to_path_buf(), after));
        }
    }
    let changes = states
        .iter()
        .filter(|(_, s)| matches!(s, State::Activates | State::Deactivates))
        .count();
    Ok(super::desk::Done::Resources {
        states: states.clone(),
        said: said.unwrap_or_else(|| super::words::changed(change, kind)),
        changes,
    })
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
