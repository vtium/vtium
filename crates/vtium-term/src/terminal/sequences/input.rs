//! The entries of the `input` rows: the sequences with which a program
//! chooses how vtium reports keys and the pointer.

use super::{Entry, Form, Other, Param, REFUSALS, Record, TARGET_SET};

/// The kitty keyboard protocol's flags, which several of its forms take.
const FLAGS: Param = Param {
    name: "flags",
    meaning: "The sum of the flags a program asks for: 1 disambiguates escape codes, 2 reports repeats and releases, 4 reports the shifted key and the base layout key, 8 reports every key as an escape code, and 16 reports the text a key makes. vtium keeps these five bits and drops any other.",
    default: Some("0"),
};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "input.modify-other-keys",
        forms: &[
            Form {
                form: "CSI > 4 ; Pv m",
                name: "XTMODKEYS: set the modifyOtherKeys level",
            },
            Form {
                form: "CSI > 4 n",
                name: "XTMODKEYS: turn modifyOtherKeys off",
            },
            Form {
                form: "CSI ? 4 m",
                name: "XTQMODKEYS: ask for the modifyOtherKeys level",
            },
        ],
        params: &[Param {
            name: "Pv",
            meaning: "The level: 0 turns modifyOtherKeys off, 1 reports the keys whose modifiers would otherwise be lost, and 2 reports every key pressed with a modifier.",
            default: Some("0"),
        }],
        does: &[
            "At level 1 or 2, vtium sends a key pressed with Shift, Alt or Ctrl as `CSI 27 ; Pm ; Pc ~`, where `Pm` gives the modifiers and `Pc` the key's code. The key table in ctlseqs decides which keys take that form at each level. `CSI > m` with no parameters also returns the level to 0.",
            "XTQMODKEYS is answered `CSI > 4 ; Pv m` with the level in force. vtium ignores the other resources that XTMODKEYS can name. RIS and DECSTR return the level to 0.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "input.format-other-keys",
        forms: &[
            Form {
                form: "CSI > Pp ; Pv f",
                name: "XTFMTKEYS: choose the format of modified keys",
            },
            Form {
                form: "CSI ? Pp f",
                name: "XTQFMTKEYS: ask for that format",
            },
        ],
        params: &[],
        does: &["vtium reads both sequences and does nothing with them. XTQFMTKEYS gets no reply."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "input.kitty-keyboard",
        forms: &[
            Form {
                form: "CSI > flags u",
                name: "push a set of flags",
            },
            Form {
                form: "CSI < Ps u",
                name: "pop sets of flags",
            },
            Form {
                form: "CSI = flags ; Pm u",
                name: "change the flags in force",
            },
            Form {
                form: "CSI ? u",
                name: "ask for the flags in force",
            },
            Form {
                form: "CSI Pk ; Pm u",
                name: "a key, as vtium reports it",
            },
        ],
        params: &[
            FLAGS,
            Param {
                name: "Ps",
                meaning: "How many sets to pop. 0 pops nothing.",
                default: Some("1"),
            },
            Param {
                name: "Pm",
                meaning: "In `CSI = flags ; Pm u`, how to apply the flags: 1 sets exactly these flags, 2 sets their bits, and 3 clears their bits. Any other value changes nothing. In a key report, `Pm` is 1 plus the modifiers held.",
                default: Some("1"),
            },
            Param {
                name: "Pk",
                meaning: "In a key report, the key's code: the Unicode code point of the unshifted key, or a number from the specification's table of functional keys.",
                default: None,
            },
        ],
        does: &[
            "vtium keeps one stack of flags for the main screen and one for the alternate screen, and switching screens leaves each stack as it is. A stack holds the set in force and seven saved sets. A push onto a full stack drops the oldest saved set, and a pop of more sets than are saved puts 0 in force.",
            "While flag 1 or flag 8 is in force, the protocol decides the bytes of every key. Without them, each key follows the specification's legacy encoding. `CSI ? u` is answered `CSI ? flags u` with the flags in force on the screen shown. RIS and DECSTR empty both stacks.",
        ],
        departure: None,
        record: Record {
            adr: 1005,
            section: "D3",
        },
    },
    Entry {
        id: "input.xtshiftescape",
        forms: &[Form {
            form: "CSI > Ps s",
            name: "XTSHIFTESCAPE: let Shift reach mouse reports",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "1 lets a pointer event with Shift held reach the program's mouse reports, and 0 keeps Shift for the user's own selection.",
            default: Some("0"),
        }],
        does: &[
            "With XTSHIFTESCAPE 0, a click or a drag with Shift held selects and pastes text even while a program tracks the mouse. With XTSHIFTESCAPE 1, vtium reports that event to the program instead. DECSTR keeps the setting, and RIS returns it to 0.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "input.xthimouse",
        forms: &[Form {
            form: "CSI Ps ; Ps ; Ps ; Ps ; Ps T",
            name: "XTHIMOUSE: the program's answer to hilite mouse tracking",
        }],
        params: &[],
        does: &[
            "vtium does nothing with this sequence. It refuses hilite mouse tracking, mode 1001, so there is no question for a program to answer. The five parameters keep the sequence apart from SD, which takes one.",
        ],
        departure: None,
        record: REFUSALS,
    },
    Entry {
        id: "input.decudk",
        forms: &[Form {
            form: "DCS Ps ; Ps | Pt ST",
            name: "DECUDK: define user-defined keys",
        }],
        params: &[],
        does: &[
            "vtium does nothing with this string, and every key keeps sending what it sent before. A key that program output could define would later type that output into the shell when the user presses the key.",
        ],
        departure: None,
        record: REFUSALS,
    },
];

pub(super) const OTHER: &[Other] = &[
    Other {
        id: "input.legacy",
        what: "how vtium encodes keys",
    },
    Other {
        id: "input.fixterms",
        what: "a key encoding that the kitty keyboard protocol replaces",
    },
    Other {
        id: "input.shift-override",
        what: "how Shift and the pointer select text",
    },
    Other {
        id: "input.kitty-pointer-leave",
        what: "a mouse report that vtium does not send",
    },
    Other {
        id: "input.paste-end-marker",
        what: "a filter that vtium applies to pasted text",
    },
    Other {
        id: "input.ime",
        what: "an input method, which vtium does not have yet",
    },
    Other {
        id: "input.braille",
        what: "Braille displays, which vtium does not support yet",
    },
];
