//! The entries of the `cursor` rows: the cursor's style and color.

use super::{Entry, Form, Other, Param, TARGET_SET};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "cursor.style",
        forms: &[Form {
            form: "CSI Ps SP q",
            name: "DECSCUSR: set the cursor style",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "The style: 0 and 1 a blinking block, 2 a steady block, 3 a blinking underline, 4 a steady underline, 5 a blinking bar and 6 a steady bar.",
            default: Some("0"),
        }],
        does: &[
            "A session starts with the cursor style that vtium's configuration gives: a block that blinks while the `blink` key is on. DECSCUSR changes the style as the VT510 manual decides, so `CSI 0 SP q` and `CSI SP q` select a blinking block. vtium ignores a value past 6 and keeps the style it has.",
            "DECSTR and RIS return the cursor to the configured style.",
            "DECSC does not save the cursor style, so DECRC leaves it as it is. DECRQSS reports the style that DECSCUSR set.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "cursor.color",
        forms: &[
            Form {
                form: "OSC 12 ; spec ST",
                name: "set the cursor's color",
            },
            Form {
                form: "OSC 12 ; ? ST",
                name: "ask for the cursor's color",
            },
            Form {
                form: "OSC 112 ST",
                name: "reset the cursor's color",
            },
        ],
        params: &[Param {
            name: "spec",
            meaning: "A color as X11 writes it: `rgb:r/g/b` or `#rgb` with one to four hex digits for each channel, or an X11 color name.",
            default: None,
        }],
        does: &[
            "vtium draws the cursor in the color OSC 12 sets. A cursor with no color of its own draws its cell inverted, and the query then answers the foreground. OSC 112 returns the cursor to its configured color.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "cursor.kitty-multiple",
        forms: &[Form {
            form: "CSI > Ps ; Pt SP q",
            name: "add or remove extra cursors",
        }],
        params: &[
            Param {
                name: "Ps",
                meaning: "The shape of the extra cursors: 0 none, which removes them, 1 a block, 2 a bar, 3 an underline, and 29 the main cursor's shape. kitty's specification gives 30, 40 and 100 other uses: the colors of the extra cursors, and a query.",
                default: None,
            },
            Param {
                name: "Pt",
                meaning: "Where the cursors go, as groups separated by `;`. Each group is a coordinate type and its coordinates, joined by `:`: 0 for the main cursor's cell, 2 for cells written as row and column pairs, and 4 for rectangles written as top, left, bottom and right.",
                default: None,
            },
        ],
        does: &[
            "vtium reads the sequence and does nothing with it. The screen keeps its one cursor.",
        ],
        departure: None,
        record: TARGET_SET,
    },
];

pub(super) const OTHER: &[Other] = &[];
