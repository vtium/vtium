//! The entries of the `color` rows: the OSC strings that set, query and
//! reset colors, the color stack, and the color scheme report.

use super::{Departure, Entry, Form, Other, Param, REPLIES, TARGET_SET};

/// A color specification, as the color OSC strings take it.
const SPEC: Param = Param {
    name: "spec",
    meaning: "A color as X11 writes it: `rgb:r/g/b` or `#rgb` with one to four hex digits for each channel, or an X11 color name such as `dark slate gray`. A `?` in its place asks for the color instead of setting it.",
    default: None,
};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "color.palette.query",
        forms: &[Form {
            form: "OSC 4 ; c ; ? ST",
            name: "ask for palette color c",
        }],
        params: &[Param {
            name: "c",
            meaning: "The palette color's index, from 0 to 255.",
            default: None,
        }],
        does: &[
            "vtium answers `OSC 4 ; c ; rgb:rrrr/gggg/bbbb`, with each byte of the color written twice, and ends the reply with BEL or ST as the query ended. One string can ask for several colors, and each gets a reply of its own.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.palette.set",
        forms: &[
            Form {
                form: "OSC 4 ; c ; spec ST",
                name: "set palette color c",
            },
            Form {
                form: "OSC 104 ; c ST",
                name: "reset palette color c",
            },
        ],
        params: &[
            Param {
                name: "c",
                meaning: "The palette color's index, from 0 to 255. OSC 4 takes several `c ; spec` pairs, and OSC 104 several indexes separated by `;`.",
                default: Some("every color, for OSC 104 with no index"),
            },
            SPEC,
        ],
        does: &[
            "OSC 4 sets each palette color it names, and text already on the screen in that color changes with it. vtium skips an index past 255 and a spec it cannot read, such as `rgbi:`. OSC 104 returns each color it lists to the palette that vtium.conf sets.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.dynamic.query",
        forms: &[
            Form {
                form: "OSC 10 ; ? ST",
                name: "ask for the default foreground",
            },
            Form {
                form: "OSC 11 ; ? ST",
                name: "ask for the default background",
            },
        ],
        params: &[],
        does: &[
            "vtium answers `OSC 10 ; rgb:rrrr/gggg/bbbb` or `OSC 11 ; rgb:rrrr/gggg/bbbb` with the color in use, ended as the query was. A program can read the background this way to tell a dark screen from a light one.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.dynamic.set",
        forms: &[
            Form {
                form: "OSC 10 ; spec ST",
                name: "set the default foreground",
            },
            Form {
                form: "OSC 11 ; spec ST",
                name: "set the default background",
            },
            Form {
                form: "OSC 12 ; spec ST",
                name: "set the cursor's color",
            },
            Form {
                form: "OSC 12 ; ? ST",
                name: "ask for the cursor's color",
            },
            Form {
                form: "OSC 110 ST",
                name: "reset the default foreground",
            },
            Form {
                form: "OSC 111 ST",
                name: "reset the default background",
            },
            Form {
                form: "OSC 112 ST",
                name: "reset the cursor's color",
            },
        ],
        params: &[SPEC],
        does: &[
            "vtium sets the color and redraws every cell in it. A string can list several specs, and each goes to the next number, so `OSC 10 ; red ; blue ST` sets the foreground and the background. A reset returns the color to the configured one.",
            "A cursor with no color of its own draws its cell inverted, and the query then answers the foreground.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.pointer",
        forms: &[
            Form {
                form: "OSC 13 ; spec ST",
                name: "set the pointer's foreground",
            },
            Form {
                form: "OSC 14 ; spec ST",
                name: "set the pointer's background",
            },
            Form {
                form: "OSC 113 ST",
                name: "reset the pointer's foreground",
            },
            Form {
                form: "OSC 114 ST",
                name: "reset the pointer's background",
            },
        ],
        params: &[SPEC],
        does: &[
            "vtium draws the mouse pointer in these two colors. A `?` in place of a spec asks for the color. While a color is unset, the query answers the default foreground for the pointer's foreground and the default background for its background.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.selection",
        forms: &[
            Form {
                form: "OSC 17 ; spec ST",
                name: "set the selection's background",
            },
            Form {
                form: "OSC 19 ; spec ST",
                name: "set the selection's foreground",
            },
            Form {
                form: "OSC 117 ST",
                name: "reset the selection's background",
            },
            Form {
                form: "OSC 119 ST",
                name: "reset the selection's foreground",
            },
        ],
        params: &[SPEC],
        does: &[
            "With no background set, vtium draws the selection inverted. With a background set, the selected text keeps its own color unless OSC 19 sets a foreground too.",
            "A `?` in place of a spec asks for the color. While a color is unset, the query answers the color vtium draws in: the foreground for the background, and the background for the foreground.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.tektronix",
        forms: &[
            Form {
                form: "OSC 15 ; spec ST",
                name: "set the Tektronix foreground",
            },
            Form {
                form: "OSC 16 ; spec ST",
                name: "set the Tektronix background",
            },
            Form {
                form: "OSC 18 ; spec ST",
                name: "set the Tektronix cursor's color",
            },
            Form {
                form: "OSC 115 ST",
                name: "reset the Tektronix foreground",
            },
            Form {
                form: "OSC 116 ST",
                name: "reset the Tektronix background",
            },
            Form {
                form: "OSC 118 ST",
                name: "reset the Tektronix cursor's color",
            },
        ],
        params: &[SPEC],
        does: &[
            "vtium reads these strings and does nothing with them, and a query gets no reply. In a list that starts at a lower number, such as `OSC 10`, vtium skips their places and sets the colors after them.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.special",
        forms: &[
            Form {
                form: "OSC 5 ; c ; spec ST",
                name: "set a special color",
            },
            Form {
                form: "OSC 6 ; c ; f ST",
                name: "turn a special color on or off",
            },
            Form {
                form: "OSC 105 ; c ST",
                name: "reset a special color",
            },
            Form {
                form: "OSC 106 ; c ; f ST",
                name: "turn a special color on or off",
            },
        ],
        params: &[
            Param {
                name: "c",
                meaning: "The special color: 0 bold, 1 underline, 2 blink, 3 reverse and 4 italic.",
                default: None,
            },
            Param {
                name: "f",
                meaning: "1 turns the color on, and 0 turns it off.",
                default: None,
            },
        ],
        does: &[
            "vtium reads these strings and does nothing with them. Bold, underlined, blinking and inverse text keep their own colors.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.kitty-protocol",
        forms: &[Form {
            form: "OSC 21 ; Pt ST",
            name: "set, ask for or reset colors by key",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "A list of keys separated by `;`. `key=spec` sets a color, `key=?` asks for it, `key=` unsets it, and a bare key resets it. The keys are `foreground`, `background`, `cursor`, `selection_background`, `selection_foreground`, and the palette indexes 0 to 255.",
            default: None,
        }],
        does: &[
            "vtium sets, unsets and resets each key in turn. It answers every query of the string in one reply, ended as the string was. `key=` unsets only the cursor and the selection colors, and vtium ignores it for the other keys. vtium reads a spec as OSC 4 reads it.",
        ],
        departure: Some(Departure {
            says: "vtium leaves out of its reply a key that it does not know, instead of answering it with `unknown=` and the key's name in base64. A reply from vtium never carries bytes of the request.",
            readme: "crates/vtium-term/README.md",
            section: "Colors",
        }),
        record: REPLIES,
    },
    Entry {
        id: "color.stack",
        forms: &[
            Form {
                form: "CSI Pm # P",
                name: "XTPUSHCOLORS: push the colors",
            },
            Form {
                form: "CSI Pm # Q",
                name: "XTPOPCOLORS: pop the colors",
            },
            Form {
                form: "CSI # R",
                name: "XTREPORTCOLORS: report the stack",
            },
            Form {
                form: "OSC 30001 ST",
                name: "push the colors",
            },
            Form {
                form: "OSC 30101 ST",
                name: "pop the colors",
            },
        ],
        params: &[Param {
            name: "Pm",
            meaning: "The entries of the stack to store into or restore from, numbered from 1 to 10. Several numbers act in turn, and 0 pushes or pops.",
            default: Some("0"),
        }],
        does: &[
            "A push stores every color in a stack entry: the 256 palette colors, the defaults, and the cursor's, pointer's and selection's colors. A pop restores them, and text on the screen changes with them. OSC 30001 and 30101 push and pop the same stack as XTPUSHCOLORS and XTPOPCOLORS.",
            "The stack holds 10 entries. A push with the stack full, and a pop of an empty stack, do nothing. XTREPORTCOLORS answers `CSI ? Pc ; Ps # Q`, with the current entry and the number of entries stored.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "color.scheme-report",
        forms: &[Form {
            form: "CSI ? 996 n",
            name: "ask whether the colors are dark or light",
        }],
        params: &[],
        does: &[
            "vtium answers `CSI ? 997 ; 1 n` when the default background is dark and `CSI ? 997 ; 2 n` when it is light. The background is dark when its luma, by ITU-R BT.601, is below half.",
            "While mode 2031 is set, vtium sends the same report by itself each time the default background crosses between dark and light. A change that keeps the background dark, or light, sends nothing.",
        ],
        departure: None,
        record: TARGET_SET,
    },
];

pub(super) const OTHER: &[Other] = &[
    Other {
        id: "color.names",
        what: "the color names that color strings accept",
    },
    Other {
        id: "color.palette.named",
        what: "configuration keys, palette and palette-<color>",
    },
    Other {
        id: "color.wide-gamut",
        what: "color forms that color strings do not accept",
    },
];
