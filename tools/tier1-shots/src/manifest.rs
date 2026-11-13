//! Each picture's name and file, and `manifest.json`, which lists them.
//!
//! The pictures sit in three directories of the output: `corpus/NAME.png`
//! and `vtium/NAME.png` for the replay cases, and `scenarios/NAME-NN.png`
//! for the screens a scenario dumps, NN its dump's number from 01. The
//! manifest lists every picture in the order the tool draws them: the
//! corpus's cases in `cases.list`'s order, then vtium's, then the
//! scenarios by name with each one's dumps in order. tools/conformance's
//! README describes each field.

use crate::judge::Line;

/// The set a picture belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// A case of the copied corpus, `tools/conformance/corpus/`.
    Corpus,
    /// One of vtium's own cases, `tools/conformance/vtium/`.
    Vtium,
    /// A screen a behaviour scenario dumps, `tools/conformance/scenarios/`.
    Scenarios,
}

impl Group {
    /// The group's name, which is also its directory in the output and in
    /// tier 1's.
    pub fn name(self) -> &'static str {
        match self {
            Group::Corpus => "corpus",
            Group::Vtium => "vtium",
            Group::Scenarios => "scenarios",
        }
    }
}

/// The dump a scenario's picture shows: its number from 1, how many the
/// scenario has, and the command that asks for it with its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dump {
    pub number: usize,
    pub of: usize,
    pub command: String,
    pub line: usize,
}

/// One golden that judges a picture, and the lines where the output
/// differs from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Judged {
    /// `screen`, `frame` or `display` for a replay case, `transcript` for a
    /// scenario.
    pub view: String,
    /// The golden's path from the repository's root.
    pub path: String,
    pub differs: Vec<Line>,
}

/// The live path of a picture: the command that plays its case or
/// scenario in the terminal it runs in, and whether the bytes it writes
/// for this screen, read back through a terminal, draw this picture.
/// When they draw another, the tool writes that drawing beside the
/// picture, at [`Shot::live_file`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Live {
    pub command: String,
    pub matches: bool,
}

/// One picture and what judged it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shot {
    pub group: Group,
    pub name: String,
    pub dump: Option<Dump>,
    pub width: u32,
    pub height: u32,
    /// The reason the provisional list gives, when it names the case or
    /// the scenario: its golden then judges nothing in `conformance run`.
    pub provisional: Option<String>,
    pub goldens: Vec<Judged>,
    /// Why the replay or the drawing stopped, when it did: the picture
    /// is then missing, or shows the terminal where the replay stopped.
    pub error: Option<String>,
    pub live: Live,
}

impl Shot {
    /// The picture's path in the output.
    pub fn file(&self) -> String {
        file_name(self.group, &self.name, self.dump.as_ref().map(|d| d.number))
    }

    /// The path in the output of the live path's drawing of the same
    /// screen, which the tool writes when it differs from the picture.
    pub fn live_file(&self) -> String {
        let file = self.file();
        format!("{}.live.png", file.strip_suffix(".png").unwrap_or(&file))
    }

    /// Whether the output equals every golden that judges the picture.
    pub fn matches(&self) -> bool {
        self.error.is_none() && self.goldens.iter().all(|g| g.differs.is_empty())
    }
}

/// The path in the output of the picture of `name` in `group`, and of its
/// dump `number` for a scenario.
pub fn file_name(group: Group, name: &str, dump: Option<usize>) -> String {
    match dump {
        Some(n) => format!("{}/{name}-{n:02}.png", group.name()),
        None => format!("{}/{name}.png", group.name()),
    }
}

/// `text` as a JSON string.
fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn optional(text: Option<&str>) -> String {
    text.map_or_else(|| "null".to_owned(), string)
}

/// The manifest of `shots`, `manifest.json`: the cell size the pictures
/// are drawn in, and each picture in order. The same shots give the same
/// bytes.
pub fn json(shots: &[Shot]) -> String {
    let mut out = String::from("{\n  \"cell\": [8, 16],\n  \"font\": \"unifont\",\n");
    out.push_str("  \"pictures\": [");
    for (i, s) in shots.iter().enumerate() {
        out.push_str(if i == 0 { "\n" } else { ",\n" });
        out.push_str("    {\n");
        out.push_str(&format!("      \"file\": {},\n", string(&s.file())));
        out.push_str(&format!("      \"group\": {},\n", string(s.group.name())));
        out.push_str(&format!("      \"name\": {},\n", string(&s.name)));
        match &s.dump {
            Some(d) => out.push_str(&format!(
                "      \"dump\": {{\"number\": {}, \"of\": {}, \"command\": {}, \"line\": {}}},\n",
                d.number,
                d.of,
                string(&d.command),
                d.line
            )),
            None => out.push_str("      \"dump\": null,\n"),
        }
        out.push_str(&format!("      \"width\": {},\n", s.width));
        out.push_str(&format!("      \"height\": {},\n", s.height));
        out.push_str(&format!(
            "      \"provisional\": {},\n",
            optional(s.provisional.as_deref())
        ));
        out.push_str(&format!("      \"matches\": {},\n", s.matches()));
        out.push_str(&format!(
            "      \"error\": {},\n",
            optional(s.error.as_deref())
        ));
        out.push_str(&format!(
            "      \"live\": {{\"command\": {}, \"matches\": {}, \"file\": {}}},\n",
            string(&s.live.command),
            s.live.matches,
            if s.live.matches {
                "null".to_owned()
            } else {
                string(&s.live_file())
            }
        ));
        out.push_str("      \"goldens\": [");
        for (k, g) in s.goldens.iter().enumerate() {
            out.push_str(if k == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "        {{\"view\": {}, \"path\": {}, \"matches\": {}, \"differs\": [",
                string(&g.view),
                string(&g.path),
                g.differs.is_empty()
            ));
            for (n, l) in g.differs.iter().enumerate() {
                out.push_str(if n == 0 { "\n" } else { ",\n" });
                out.push_str(&format!(
                    "          {{\"side\": {}, \"line\": {}, \"text\": {}}}",
                    string(l.side.name()),
                    l.number,
                    string(&l.text)
                ));
            }
            out.push_str(if g.differs.is_empty() {
                "]}"
            } else {
                "\n        ]}"
            });
        }
        out.push_str(if s.goldens.is_empty() {
            "]\n"
        } else {
            "\n      ]\n"
        });
        out.push_str("    }");
    }
    out.push_str(if shots.is_empty() {
        "]\n}\n"
    } else {
        "\n  ]\n}\n"
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::Side;

    #[test]
    fn a_case_is_named_for_its_set_and_a_dump_for_its_scenario_and_number() {
        assert_eq!(
            file_name(Group::Corpus, "sgr-basic", None),
            "corpus/sgr-basic.png"
        );
        assert_eq!(
            file_name(Group::Vtium, "kit-anim", None),
            "vtium/kit-anim.png"
        );
        assert_eq!(
            file_name(Group::Scenarios, "key-function", Some(3)),
            "scenarios/key-function-03.png"
        );
        assert_eq!(
            file_name(Group::Scenarios, "key-function", Some(12)),
            "scenarios/key-function-12.png"
        );
    }

    fn shots() -> Vec<Shot> {
        vec![
            Shot {
                group: Group::Vtium,
                name: "kit-anim".to_owned(),
                dump: None,
                width: 96,
                height: 64,
                provisional: Some("#32: \"frames\"; not reviewed yet".to_owned()),
                goldens: vec![
                    Judged {
                        view: "screen".to_owned(),
                        path: "tools/conformance/vtium/golden/kit-anim.txt".to_owned(),
                        differs: Vec::new(),
                    },
                    Judged {
                        view: "frame".to_owned(),
                        path: "tools/conformance/vtium/golden-frame/kit-anim.txt".to_owned(),
                        differs: vec![
                            Line {
                                side: Side::Golden,
                                number: 7,
                                text: "anim 1 changed 0".to_owned(),
                            },
                            Line {
                                side: Side::Output,
                                number: 7,
                                text: "anim 1 changed 1".to_owned(),
                            },
                        ],
                    },
                ],
                error: None,
                live: Live {
                    command: "make tier1-play CASE=kit-anim".to_owned(),
                    matches: true,
                },
            },
            Shot {
                group: Group::Scenarios,
                name: "key-function".to_owned(),
                dump: Some(Dump {
                    number: 2,
                    of: 3,
                    command: "dump screen".to_owned(),
                    line: 9,
                }),
                width: 80,
                height: 32,
                provisional: None,
                goldens: vec![Judged {
                    view: "transcript".to_owned(),
                    path: "tools/conformance/scenarios/key-function.transcript".to_owned(),
                    differs: Vec::new(),
                }],
                error: None,
                live: Live {
                    command: "make tier1-play SCENARIO=key-function".to_owned(),
                    matches: false,
                },
            },
        ]
    }

    #[test]
    fn the_manifest_names_each_picture_its_size_and_its_goldens() {
        let want = r##"{
  "cell": [8, 16],
  "font": "unifont",
  "pictures": [
    {
      "file": "vtium/kit-anim.png",
      "group": "vtium",
      "name": "kit-anim",
      "dump": null,
      "width": 96,
      "height": 64,
      "provisional": "#32: \"frames\"; not reviewed yet",
      "matches": false,
      "error": null,
      "live": {"command": "make tier1-play CASE=kit-anim", "matches": true, "file": null},
      "goldens": [
        {"view": "screen", "path": "tools/conformance/vtium/golden/kit-anim.txt", "matches": true, "differs": []},
        {"view": "frame", "path": "tools/conformance/vtium/golden-frame/kit-anim.txt", "matches": false, "differs": [
          {"side": "golden", "line": 7, "text": "anim 1 changed 0"},
          {"side": "output", "line": 7, "text": "anim 1 changed 1"}
        ]}
      ]
    },
    {
      "file": "scenarios/key-function-02.png",
      "group": "scenarios",
      "name": "key-function",
      "dump": {"number": 2, "of": 3, "command": "dump screen", "line": 9},
      "width": 80,
      "height": 32,
      "provisional": null,
      "matches": true,
      "error": null,
      "live": {"command": "make tier1-play SCENARIO=key-function", "matches": false, "file": "scenarios/key-function-02.live.png"},
      "goldens": [
        {"view": "transcript", "path": "tools/conformance/scenarios/key-function.transcript", "matches": true, "differs": []}
      ]
    }
  ]
}
"##;
        assert_eq!(json(&shots()), want);
    }

    #[test]
    fn a_shot_with_an_error_does_not_match() {
        let mut s = shots().remove(1);
        assert!(s.matches());
        s.error = Some("the replay exits 3".to_owned());
        assert!(!s.matches());
    }

    #[test]
    fn control_characters_are_escaped() {
        assert_eq!(string("a\u{1b}[1m\"\\\n"), r#""a\u001b[1m\"\\\n""#);
    }
}
