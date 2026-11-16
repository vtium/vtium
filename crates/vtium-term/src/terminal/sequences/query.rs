//! The entries of the `query` rows: the queries a program sends to learn
//! what the terminal is and what state it is in, and the replies vtium
//! sends back.

use super::{Entry, Form, Other, Param, REPLIES, Record, TARGET_SET};

/// The parameter of a query that vtium answers only for 0.
const ZERO: Param = Param {
    name: "Ps",
    meaning: "Must be 0. vtium answers no other value.",
    default: Some("0"),
};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "query.da1",
        forms: &[Form {
            form: "CSI Ps c",
            name: "DA1: primary device attributes",
        }],
        params: &[ZERO],
        does: &[
            "vtium answers `CSI ? 62 ; 22 c`: a VT220-level terminal (62) with ANSI color (22). While vtium offers sixel images, it adds attribute 4 and answers `CSI ? 62 ; 4 ; 22 c`.",
            "DECID and the 8-bit control 0x9A get the same answer.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "query.da2",
        forms: &[Form {
            form: "CSI > Ps c",
            name: "DA2: secondary device attributes",
        }],
        params: &[ZERO],
        does: &[
            "vtium answers `CSI > 1 ; 1 ; 0 c`: a VT220 (1) with firmware version 1 and no ROM cartridge (0).",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "query.da3",
        forms: &[Form {
            form: "CSI = Ps c",
            name: "DA3: tertiary device attributes",
        }],
        params: &[ZERO],
        does: &[
            "vtium answers with DECRPTUI, the report of the terminal's unit id. The id is always zeros, so the reply is `DCS ! | 00000000 ST`.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "query.decid",
        forms: &[Form {
            form: "ESC Z",
            name: "DECID: identify the terminal",
        }],
        params: &[],
        does: &[
            "vtium answers DECID as it answers DA1: `CSI ? 62 ; 22 c`, or `CSI ? 62 ; 4 ; 22 c` while it offers sixel images.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "query.dsr",
        forms: &[
            Form {
                form: "CSI 5 n",
                name: "DSR: device status report",
            },
            Form {
                form: "CSI 6 n",
                name: "CPR: report the cursor's position",
            },
            Form {
                form: "CSI ? 6 n",
                name: "DECXCPR: report the cursor's position and page",
            },
        ],
        params: &[],
        does: &[
            "vtium answers `CSI 5 n` with `CSI 0 n`, which means the terminal works. It answers `CSI 6 n` with `CSI Pr ; Pc R`, the cursor's row and column counted from 1. DECXCPR adds the page, which is always 1: `CSI ? Pr ; Pc ; 1 R`.",
            "In origin mode, the row counts from the top of the scrolling region and the column from the left margin. While a wrap is pending, the cursor stands on the last column, and vtium reports that column.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "query.dsr-other",
        forms: &[
            Form {
                form: "CSI ? 15 n",
                name: "printer status",
            },
            Form {
                form: "CSI ? 25 n",
                name: "user-defined keys status",
            },
            Form {
                form: "CSI ? 26 n",
                name: "keyboard status",
            },
            Form {
                form: "CSI ? 53 n",
                name: "locator status",
            },
            Form {
                form: "CSI ? 55 n",
                name: "locator status",
            },
            Form {
                form: "CSI ? 56 n",
                name: "locator type",
            },
            Form {
                form: "CSI ? 62 n",
                name: "macro space report",
            },
            Form {
                form: "CSI ? 63 n",
                name: "memory checksum report",
            },
            Form {
                form: "CSI ? 75 n",
                name: "data integrity report",
            },
            Form {
                form: "CSI ? 85 n",
                name: "multiple-session status",
            },
        ],
        params: &[],
        does: &["vtium reads these queries and sends no reply."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "query.decrqm",
        forms: &[
            Form {
                form: "CSI Pa $ p",
                name: "DECRQM: report an ANSI mode",
            },
            Form {
                form: "CSI ? Pd $ p",
                name: "DECRQM: report a DEC private mode",
            },
        ],
        params: &[
            Param {
                name: "Pa",
                meaning: "The number of an ANSI mode, as SM and RM take it.",
                default: None,
            },
            Param {
                name: "Pd",
                meaning: "The number of a DEC private mode, as DECSET and DECRST take it.",
                default: None,
            },
        ],
        does: &[
            "vtium answers `CSI Pa ; Ps $ y`, or `CSI ? Pd ; Ps $ y` for a DEC private mode. `Ps` is 1 when the mode is set and 2 when it is reset. It is 3 for a mode that is always set and 4 for one that is always reset, such as a mode vtium ignores or refuses. It is 0 for a number vtium does not recognize.",
            "vtium answers from its mode table, which [Modes](../modes.md) lists.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "query.decrqss",
        forms: &[Form {
            form: "DCS $ q Pt ST",
            name: "DECRQSS: request a setting",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "The final bytes of the control function whose setting the program asks for, such as `m` for SGR or `r` for DECSTBM.",
            default: None,
        }],
        does: &[
            "vtium answers `DCS 1 $ r Pt ST`, where `Pt` is the sequence that sets the current value. It answers for SGR (`m`), DECSTBM (`r`), DECSLRM (`s`), DECSCUSR (`SP q`), DECSCL (`\" p`), DECSCA (`\" q`) and DECSACE (`* x`).",
            "Any other request gets `DCS 0 $ r ST`, and the reply never repeats the request's bytes. For DECSCL, vtium reports 7-bit controls, because its replies stay 7-bit.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "query.xtversion",
        forms: &[Form {
            form: "CSI > Ps q",
            name: "XTVERSION: report the terminal's name and version",
        }],
        params: &[ZERO],
        does: &[
            "vtium answers `DCS > | vtium(VERSION) ST`, where VERSION is vtium's version, as in `vtium(0.1.0)`.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "query.xtgettcap",
        forms: &[Form {
            form: "DCS + q Pt ST",
            name: "XTGETTCAP: request terminfo capabilities",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "The names of the capabilities, each written in hex and separated by `;`.",
            default: None,
        }],
        does: &[
            "vtium looks each name up in the terminfo entry that `TERM` names, and answers `DCS 1 + r Pt ST`. In the reply, `Pt` holds `name=value` for each capability, both in uppercase hex, or the name alone for a boolean capability.",
            "vtium stops at the first name it does not know. If it knows none of the names, it answers `DCS 0 + r ST`. Each name in the reply comes from vtium's table, never from the request.",
            "vtium answers a bounded number of names in one request, and keeps the reply under a bounded length.",
        ],
        departure: None,
        record: Record {
            adr: 1000,
            section: "The terminfo entry and TERM",
        },
    },
    Entry {
        id: "query.xtsettcap",
        forms: &[
            Form {
                form: "DCS + p Pt ST",
                name: "XTSETTCAP: set a terminfo capability",
            },
            Form {
                form: "DCS + Q Pt ST",
                name: "XTGETXRES: request X resources",
            },
        ],
        params: &[],
        does: &["vtium reads these strings, changes nothing and sends no reply."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "query.xtqallowed",
        forms: &[
            Form {
                form: "OSC 60 ST",
                name: "XTQALLOWED: list the allowed features",
            },
            Form {
                form: "OSC 61 ; Pt ST",
                name: "XTQDISALLOWED: list the disallowed features",
            },
            Form {
                form: "OSC 62 ST",
                name: "XTQALLOWABLE: list the features that may be allowed",
            },
        ],
        params: &[],
        does: &["vtium reads these strings and sends no reply."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "query.xtsmgraphics",
        forms: &[Form {
            form: "CSI ? Pi ; Pa ; Pv S",
            name: "XTSMGRAPHICS: set or read a graphics attribute",
        }],
        params: &[
            Param {
                name: "Pi",
                meaning: "The item: 1 for the number of sixel color registers, 2 for the largest sixel image in pixels.",
                default: None,
            },
            Param {
                name: "Pa",
                meaning: "The action: 1 reads the value in force, 2 resets it, 3 sets it to `Pv`, and 4 reads the largest value allowed.",
                default: None,
            },
            Param {
                name: "Pv",
                meaning: "The value that action 3 sets.",
                default: None,
            },
        ],
        does: &[
            "vtium answers `CSI ? Pi ; Ps ; Pv S`, with the value or values after the status `Ps`. The status is 0 for success, 1 for an item vtium does not know, 2 for an action it does not know, and 3 for a value it cannot take.",
            "A program can set the number of color registers from 2 up to the `num-color-registers` key. The new number takes effect at the next image. The image size reads `max-graphic-size`, and a program cannot reset or set it.",
            "While vtium does not offer sixel, and in a build without images, every item answers status 1.",
        ],
        departure: None,
        record: Record {
            adr: 1100,
            section: "D10",
        },
    },
    Entry {
        id: "query.xtreportsgr",
        forms: &[
            Form {
                form: "CSI Pt ; Pl ; Pb ; Pr # |",
                name: "XTREPORTSGR: report the SGR of an area",
            },
            Form {
                form: "CSI Ps # y",
                name: "XTCHECKSUM: choose how DECRQCRA computes its checksum",
            },
            Form {
                form: "CSI Pi ; Pg ; Pt ; Pl ; Pb ; Pr * y",
                name: "DECRQCRA: report the checksum of an area",
            },
        ],
        params: &[],
        does: &["vtium reads these sequences and sends no reply."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "query.decrqpsr",
        forms: &[
            Form {
                form: "CSI Ps $ w",
                name: "DECRQPSR: report the cursor (DECCIR) or the tab stops (DECTABSR)",
            },
            Form {
                form: "DCS Ps $ t Pt ST",
                name: "DECRSPS: restore the cursor or the tab stops",
            },
        ],
        params: &[],
        does: &["vtium reads these sequences, changes nothing and sends no reply."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "query.decreqtparm",
        forms: &[
            Form {
                form: "CSI Ps x",
                name: "DECREQTPARM: report the serial line's parameters",
            },
            Form {
                form: "CSI \" v",
                name: "DECRQDE: report the window's extent",
            },
            Form {
                form: "CSI & u",
                name: "DECRQUPSS: report the user-preferred character set",
            },
            Form {
                form: "CSI Ps $ u",
                name: "DECRQTSR: report the terminal's state",
            },
        ],
        params: &[],
        does: &["vtium reads these sequences and sends no reply."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "query.locator",
        forms: &[
            Form {
                form: "CSI Ps ; Pu ' z",
                name: "DECELR: enable locator reports",
            },
            Form {
                form: "CSI Pm ' {",
                name: "DECSLE: select locator events",
            },
            Form {
                form: "CSI Pt ; Pl ; Pb ; Pr ' w",
                name: "DECEFR: enable a filter rectangle",
            },
            Form {
                form: "CSI Ps ' |",
                name: "DECRQLP: request the locator's position",
            },
        ],
        params: &[],
        does: &[
            "vtium reads these sequences, changes nothing and sends no reply. A program that wants the pointer's position turns on mouse reporting through the mouse modes, which [Modes](../modes.md) lists.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "query.enq",
        forms: &[Form {
            form: "ENQ",
            name: "enquiry: ask for the answerback message",
        }],
        params: &[],
        does: &["vtium's answerback message is empty, so it sends nothing in reply to ENQ."],
        departure: None,
        record: REPLIES,
    },
];

pub(super) const OTHER: &[Other] = &[];
