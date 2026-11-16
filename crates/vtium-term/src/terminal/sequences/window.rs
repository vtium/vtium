//! The entries of the `window` rows: XTWINOPS, the title and icon strings,
//! and the pointer's shape.

use super::{Entry, Form, Other, Param, REFUSALS, REPLIES, TARGET_SET};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "window.manipulate",
        forms: &[
            Form {
                form: "CSI 1 t",
                name: "XTWINOPS: de-iconify the window",
            },
            Form {
                form: "CSI 2 t",
                name: "XTWINOPS: iconify the window",
            },
            Form {
                form: "CSI 3 ; Px ; Py t",
                name: "XTWINOPS: move the window",
            },
            Form {
                form: "CSI 4 ; Ph ; Pw t",
                name: "XTWINOPS: resize the window in pixels",
            },
            Form {
                form: "CSI 5 t",
                name: "XTWINOPS: raise the window",
            },
            Form {
                form: "CSI 6 t",
                name: "XTWINOPS: lower the window",
            },
            Form {
                form: "CSI 7 t",
                name: "XTWINOPS: refresh the window",
            },
            Form {
                form: "CSI 9 ; Ps t",
                name: "XTWINOPS: maximize or restore the window",
            },
            Form {
                form: "CSI 10 ; Ps t",
                name: "XTWINOPS: full screen",
            },
            Form {
                form: "CSI Ps t",
                name: "DECSLPP: lines per screen, for Ps of 24 or more",
            },
        ],
        params: &[],
        does: &[
            "vtium fills the whole screen and has no window, so these sequences have nothing to act on. vtium reads them and does nothing.",
            "A program that wants a grid of a given size in characters sends `CSI 8 ; Ph ; Pw t`, which vtium supports when the configuration allows it.",
        ],
        departure: None,
        record: REFUSALS,
    },
    Entry {
        id: "window.resize",
        forms: &[Form {
            form: "CSI 8 ; Ph ; Pw t",
            name: "XTWINOPS: resize the text area in characters",
        }],
        params: &[
            Param {
                name: "Ph",
                meaning: "The number of rows. An omitted value keeps the rows there are, and 0 takes as many as the display holds.",
                default: None,
            },
            Param {
                name: "Pw",
                meaning: "The number of columns. An omitted value keeps the columns there are, and 0 takes as many as the display holds.",
                default: None,
            },
        ],
        does: &[
            "vtium changes the grid only when the `allow-window-ops` key is on, and the key is off by default. Otherwise it reads the sequence and changes nothing.",
            "vtium meets the request by changing the font size. It draws at the largest zoom step whose grid holds the rows and columns asked for, on a grid of exactly that size. When the displays cannot hold a side even at the smallest step, that side takes what they hold.",
            "vtium resizes the pty before it answers a size report that follows in the same write, such as `CSI 18 t`. The grid lasts until the next zoom, or until the session that asked for it ends.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "window.size-report",
        forms: &[
            Form {
                form: "CSI 14 t",
                name: "XTWINOPS: report the text area's size in pixels",
            },
            Form {
                form: "CSI 16 t",
                name: "XTWINOPS: report the cell's size in pixels",
            },
            Form {
                form: "CSI 18 t",
                name: "XTWINOPS: report the text area's size in characters",
            },
            Form {
                form: "CSI 19 t",
                name: "XTWINOPS: report the screen's size in characters",
            },
        ],
        params: &[],
        does: &[
            "vtium answers `CSI 14 t` with `CSI 4 ; height ; width t` and `CSI 16 t` with `CSI 6 ; height ; width t`, both in pixels. It answers `CSI 18 t` with `CSI 8 ; rows ; columns t`.",
            "The screen is the display the grid fills, so `CSI 19 t` gets the same numbers as `CSI 18 t`, in `CSI 9 ; rows ; columns t`. A report in pixels answers only when vtium knows the cell's size.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "window.screen-pixels",
        forms: &[Form {
            form: "CSI 15 t",
            name: "XTWINOPS: report the screen's size in pixels",
        }],
        params: &[],
        does: &[
            "vtium answers `CSI 5 ; height ; width t` with the real size, in pixels, of the display the grid fills. It answers only when it knows that size.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "window.state-report",
        forms: &[
            Form {
                form: "CSI 11 t",
                name: "XTWINOPS: report whether the window is iconified",
            },
            Form {
                form: "CSI 13 t",
                name: "XTWINOPS: report the window's position",
            },
            Form {
                form: "CSI 14 ; 2 t",
                name: "XTWINOPS: report the window's size in pixels",
            },
        ],
        params: &[],
        does: &["vtium has no window, so it reads these queries and sends no reply."],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "window.title-report",
        forms: &[Form {
            form: "CSI 21 t",
            name: "XTWINOPS: report the window's title",
        }],
        params: &[],
        does: &[
            "vtium keeps no title, so it answers with an empty one: `OSC l ST`. The reply is the same whatever title a program sent before.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "window.icon-report",
        forms: &[Form {
            form: "CSI 20 t",
            name: "XTWINOPS: report the icon's label",
        }],
        params: &[],
        does: &["vtium has no icon, so it reads the query and sends no reply."],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "window.title-stack",
        forms: &[
            Form {
                form: "CSI 22 ; Ps t",
                name: "XTWINOPS: push the title on a stack",
            },
            Form {
                form: "CSI 23 ; Ps t",
                name: "XTWINOPS: pop the title from the stack",
            },
        ],
        params: &[],
        does: &["vtium keeps no title, so it reads these sequences and does nothing."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "window.xtsmtitle",
        forms: &[
            Form {
                form: "CSI > Pm t",
                name: "XTSMTITLE: set the title's encoding features",
            },
            Form {
                form: "CSI > Pm T",
                name: "XTRMTITLE: reset the title's encoding features",
            },
            Form {
                form: "CSI # S",
                name: "XTTITLEPOS: report the title stack's position",
            },
        ],
        params: &[],
        does: &[
            "vtium keeps no title, so it reads these sequences, does nothing and sends no reply.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "window.title",
        forms: &[
            Form {
                form: "OSC 0 ; Pt ST",
                name: "set the icon name and the title",
            },
            Form {
                form: "OSC 1 ; Pt ST",
                name: "set the icon name",
            },
            Form {
                form: "OSC 2 ; Pt ST",
                name: "set the title",
            },
        ],
        params: &[Param {
            name: "Pt",
            meaning: "The text of the icon name or the title.",
            default: None,
        }],
        does: &[
            "vtium fills the screen and shows no title bar, so it reads these strings and keeps nothing. `CSI 21 t` therefore always reports an empty title.",
        ],
        departure: None,
        record: REPLIES,
    },
    Entry {
        id: "window.xprop",
        forms: &[
            Form {
                form: "OSC 3 ; Pt ST",
                name: "set an X property on the window",
            },
            Form {
                form: "OSC I Pt ST",
                name: "set the icon from a file, in Sun's form",
            },
            Form {
                form: "OSC l Pt ST",
                name: "set the title, in Sun's form",
            },
            Form {
                form: "OSC L Pt ST",
                name: "set the icon name, in Sun's form",
            },
        ],
        params: &[],
        does: &["vtium has no X window, so it reads these strings and does nothing."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "window.pointer-shape",
        forms: &[
            Form {
                form: "OSC 22 ; Pt ST",
                name: "set the pointer's shape",
            },
            Form {
                form: "OSC 22 ; = Pt ST",
                name: "set the pointer's shape",
            },
            Form {
                form: "OSC 22 ; > Pt ST",
                name: "push shapes on the pointer's stack",
            },
            Form {
                form: "OSC 22 ; < ST",
                name: "pop a shape from the pointer's stack",
            },
            Form {
                form: "OSC 22 ; ? Pt ST",
                name: "ask which shapes vtium has",
            },
        ],
        params: &[Param {
            name: "Pt",
            meaning: "One or more shape names, separated by `,`.",
            default: None,
        }],
        does: &[
            "vtium draws the pointer in the shape a program asks for while the pointer is over its text. It has these shapes: default, text, pointer, crosshair, wait, progress, not-allowed, move, ew-resize and ns-resize. The X cursor names left_ptr, xterm, hand1, hand2, watch, fleur, sb_h_double_arrow and sb_v_double_arrow name the same shapes. A name vtium does not know gives the default shape.",
            "A set replaces the shape on top of the stack, and takes the first name in the list that vtium knows. A push adds each name in turn, skips empty names, and keeps at most 16 entries. The main and alternate screens each keep a stack of their own, and RIS empties both.",
            "A query answers `OSC 22 ; Pt ST` with one answer for each name: 1 when vtium has the shape and 0 when it does not. `__current__` answers the shape on top of the stack, or 0 when the stack is empty. `__default__` and `__grabbed__` answer `default`. The reply ends with BEL when the query did.",
            "The `pointer-theme` key can replace vtium's own pictures of the shapes with those of an SVG file or a cursor theme.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "window.xtsmpointer",
        forms: &[Form {
            form: "CSI > Ps p",
            name: "XTSMPOINTER: hide the pointer while typing",
        }],
        params: &[],
        does: &[
            "vtium reads the sequence and does nothing. It follows its own rule for hiding the pointer while you type.",
        ],
        departure: None,
        record: TARGET_SET,
    },
];

pub(super) const OTHER: &[Other] = &[];
