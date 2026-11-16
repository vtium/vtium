//! The entries of the `sgr` rows: SGR, the sequence that sets the pen's
//! attributes and colors, and the SGR stack.

use super::{Entry, Form, Other, Param, TARGET_SET};

/// The parameter list that every SGR form takes.
const PM: Param = Param {
    name: "Pm",
    meaning: "One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline.",
    default: Some("0, which resets every attribute"),
};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "sgr.bold",
        forms: &[
            Form {
                form: "CSI 0 m",
                name: "reset every attribute and both colors",
            },
            Form {
                form: "CSI 1 m",
                name: "bold",
            },
            Form {
                form: "CSI 22 m",
                name: "normal intensity: neither bold nor dim",
            },
        ],
        params: &[PM],
        does: &[
            "SGR 0, and SGR with no parameter, return the pen to its defaults: no attribute set, the default foreground and the default background. SGR 1 draws the text that follows in the font's bold face, or in a bold that vtium makes from the regular face when the font has none. SGR 22 turns off both bold and dim.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.dim",
        forms: &[Form {
            form: "CSI 2 m",
            name: "dim, also called faint",
        }],
        params: &[PM],
        does: &[
            "vtium draws dim text in a color halfway between the text's color and its background. SGR 22 turns dim off together with bold.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.italic",
        forms: &[
            Form {
                form: "CSI 3 m",
                name: "italic",
            },
            Form {
                form: "CSI 23 m",
                name: "not italic",
            },
        ],
        params: &[PM],
        does: &[
            "vtium draws italic text in the font's italic face. A font without one, such as a bitmap font, draws the regular face slanted.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.underline",
        forms: &[
            Form {
                form: "CSI 4 m",
                name: "single underline",
            },
            Form {
                form: "CSI 24 m",
                name: "no underline",
            },
        ],
        params: &[PM],
        does: &[
            "vtium draws a single line under the text that follows SGR 4, in the text's color unless SGR 58 sets an underline color. SGR 24 removes every underline, whatever its style.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.underline-style",
        forms: &[
            Form {
                form: "CSI 4 : 0 m",
                name: "no underline",
            },
            Form {
                form: "CSI 4 : 1 m",
                name: "single underline",
            },
            Form {
                form: "CSI 4 : 2 m",
                name: "double underline",
            },
            Form {
                form: "CSI 4 : 3 m",
                name: "curly underline",
            },
            Form {
                form: "CSI 4 : 4 m",
                name: "dotted underline",
            },
            Form {
                form: "CSI 4 : 5 m",
                name: "dashed underline",
            },
        ],
        params: &[],
        does: &[
            "The colon form of SGR 4 chooses the underline's style. vtium draws each of the five styles, and a cell keeps one style at a time, so a new style replaces the last one.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.double-underline",
        forms: &[Form {
            form: "CSI 21 m",
            name: "double underline",
        }],
        params: &[PM],
        does: &[
            "vtium draws two lines under the text, as `CSI 4 : 2 m` does. SGR 24 removes them.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.underline-color",
        forms: &[
            Form {
                form: "CSI 58 : 5 : Ps m",
                name: "underline color from the 256-color palette",
            },
            Form {
                form: "CSI 58 : 2 : : Pr : Pg : Pb m",
                name: "underline color as red, green and blue",
            },
            Form {
                form: "CSI 59 m",
                name: "underline in the text's color",
            },
        ],
        params: &[
            Param {
                name: "Ps",
                meaning: "An index into the 256-color palette, from 0 to 255.",
                default: None,
            },
            Param {
                name: "Pr, Pg, Pb",
                meaning: "The red, green and blue values, each from 0 to 255.",
                default: None,
            },
        ],
        does: &[
            "vtium draws every underline style in the color SGR 58 sets, and SGR 59 returns the underline to the text's color. vtium also reads the semicolon forms, `CSI 58 ; 5 ; Ps m` and `CSI 58 ; 2 ; Pr ; Pg ; Pb m`.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.blink",
        forms: &[
            Form {
                form: "CSI 5 m",
                name: "blink",
            },
            Form {
                form: "CSI 25 m",
                name: "no blink",
            },
        ],
        params: &[PM],
        does: &[
            "vtium blinks the text that follows SGR 5. The `blink-text` key sets how the text looks in the blink's off phase, and `blink-decorations` sets whether its underline and strike blink with it. SGR 25 stops both SGR 5's blink and SGR 6's rapid blink.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.rapid-blink",
        forms: &[Form {
            form: "CSI 6 m",
            name: "rapid blink",
        }],
        params: &[PM],
        does: &[
            "vtium blinks the text that follows SGR 6 at 150 blinks a minute, faster than SGR 5's blink. SGR 25 stops it. A cell that carries both blinks blinks at the rapid rate.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.inverse",
        forms: &[
            Form {
                form: "CSI 7 m",
                name: "inverse",
            },
            Form {
                form: "CSI 27 m",
                name: "not inverse",
            },
        ],
        params: &[PM],
        does: &["vtium draws inverse text with its foreground and background colors swapped."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.conceal",
        forms: &[
            Form {
                form: "CSI 8 m",
                name: "conceal",
            },
            Form {
                form: "CSI 28 m",
                name: "reveal",
            },
        ],
        params: &[PM],
        does: &[
            "vtium draws a concealed cell as its background alone. The cell keeps its text, so a selection over it still copies the text.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.strike",
        forms: &[
            Form {
                form: "CSI 9 m",
                name: "strike through",
            },
            Form {
                form: "CSI 29 m",
                name: "no strike",
            },
        ],
        params: &[PM],
        does: &["vtium draws a line through the middle of the text that follows SGR 9."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.palette",
        forms: &[
            Form {
                form: "CSI 30 m to CSI 37 m",
                name: "foreground from palette colors 0 to 7",
            },
            Form {
                form: "CSI 39 m",
                name: "default foreground",
            },
            Form {
                form: "CSI 40 m to CSI 47 m",
                name: "background from palette colors 0 to 7",
            },
            Form {
                form: "CSI 49 m",
                name: "default background",
            },
            Form {
                form: "CSI 90 m to CSI 97 m",
                name: "foreground from the bright palette colors 8 to 15",
            },
            Form {
                form: "CSI 100 m to CSI 107 m",
                name: "background from the bright palette colors 8 to 15",
            },
        ],
        params: &[PM],
        does: &[
            "vtium keeps the palette color's number in the cell and looks the color up when it draws the cell. So a program that changes a palette color with OSC 4 recolors the text already on the screen. The `palette` key sets the colors that the palette starts with.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.extended-color",
        forms: &[
            Form {
                form: "CSI 38 : 5 : Ps m",
                name: "foreground from the 256-color palette",
            },
            Form {
                form: "CSI 48 : 5 : Ps m",
                name: "background from the 256-color palette",
            },
            Form {
                form: "CSI 38 : 2 : Pi : Pr : Pg : Pb m",
                name: "foreground as red, green and blue",
            },
            Form {
                form: "CSI 48 : 2 : Pi : Pr : Pg : Pb m",
                name: "background as red, green and blue",
            },
        ],
        params: &[
            Param {
                name: "Ps",
                meaning: "An index into the 256-color palette, from 0 to 255.",
                default: None,
            },
            Param {
                name: "Pi",
                meaning: "The color space's id, which T.416 places before the color. vtium reads it and does not use it, and it may be left empty.",
                default: None,
            },
            Param {
                name: "Pr, Pg, Pb",
                meaning: "The red, green and blue values, each from 0 to 255.",
                default: None,
            },
        ],
        does: &[
            "vtium reads both the colon forms above and the semicolon forms that most programs send, `CSI 38 ; 5 ; Ps m` and `CSI 38 ; 2 ; Pr ; Pg ; Pb m`. In the semicolon forms the color space's id is left out. A direct color is kept as it is, and a palette index is looked up when the cell is drawn.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.rgba",
        forms: &[
            Form {
                form: "CSI 38 : 6 : Pr : Pg : Pb : Pa m",
                name: "foreground with an alpha value",
            },
            Form {
                form: "CSI 48 : 6 : Pr : Pg : Pb : Pa m",
                name: "background with an alpha value",
            },
            Form {
                form: "CSI 58 : 6 : Pr : Pg : Pb : Pa m",
                name: "underline color with an alpha value",
            },
        ],
        params: &[],
        does: &[
            "vtium reads these forms and leaves the colors as they were. The rest of the SGR sequence still applies.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.overline",
        forms: &[
            Form {
                form: "CSI 53 m",
                name: "overline",
            },
            Form {
                form: "CSI 55 m",
                name: "no overline",
            },
        ],
        params: &[PM],
        does: &["vtium draws a line along the top of each cell of overlined text."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.script",
        forms: &[
            Form {
                form: "CSI 73 m",
                name: "superscript",
            },
            Form {
                form: "CSI 74 m",
                name: "subscript",
            },
            Form {
                form: "CSI 75 m",
                name: "neither superscript nor subscript",
            },
        ],
        params: &[PM],
        does: &[
            "vtium draws superscript and subscript text smaller, at two thirds of the normal size, raised to the top of the line or lowered to its bottom. Setting one of the two replaces the other.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.bold-faint-reset",
        forms: &[
            Form {
                form: "CSI 221 m",
                name: "turn off bold alone",
            },
            Form {
                form: "CSI 222 m",
                name: "turn off dim alone",
            },
        ],
        params: &[PM],
        does: &[
            "vtium reads these codes and changes nothing. SGR 22 turns off bold and dim together.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.ecma-other",
        forms: &[
            Form {
                form: "CSI 10 m to CSI 20 m",
                name: "primary and alternative fonts, and Fraktur",
            },
            Form {
                form: "CSI 26 m",
                name: "proportional spacing",
            },
            Form {
                form: "CSI 50 m to CSI 52 m",
                name: "no proportional spacing, framed and encircled",
            },
            Form {
                form: "CSI 54 m",
                name: "neither framed nor encircled",
            },
            Form {
                form: "CSI 60 m to CSI 65 m",
                name: "ideogram underlines, overlines and stress marks",
            },
        ],
        params: &[PM],
        does: &[
            "vtium reads these codes and changes nothing. The other codes of the same SGR sequence still apply.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "sgr.stack",
        forms: &[
            Form {
                form: "CSI Pm # {",
                name: "XTPUSHSGR: push the pen",
            },
            Form {
                form: "CSI Pm # p",
                name: "XTPUSHSGR, the other form",
            },
            Form {
                form: "CSI # }",
                name: "XTPOPSGR: pop the pen",
            },
            Form {
                form: "CSI # q",
                name: "XTPOPSGR, the other form",
            },
        ],
        params: &[Param {
            name: "Pm",
            meaning: "The parts of the pen to save, each named by its SGR code: 1 bold, 2 dim, 3 italic, 4 underline, 5 blink, 7 inverse, 8 conceal, 9 strike, 21 double underline, 30 the foreground and 31 the background.",
            default: Some("every part of the pen that SGR sets"),
        }],
        does: &[
            "XTPUSHSGR saves the pen on a stack, and XTPOPSGR restores the parts that the push named. The underline's style and color count as one part, which 4 and 21 both name. The overline and the superscript and subscript are not in the list, so a pop restores them only when the push saved every part.",
            "The stack holds a fixed number of pens. A push past that number saves nothing, and the pop that pairs with that push restores nothing. A pop of an empty stack does nothing.",
        ],
        departure: None,
        record: TARGET_SET,
    },
];

pub(super) const OTHER: &[Other] = &[
    Other {
        id: "sgr.blink.style",
        what: "a configuration key, blink-text",
    },
    Other {
        id: "sgr.blink.decorations",
        what: "a configuration key, blink-decorations",
    },
    Other {
        id: "sgr.draw-time-color",
        what: "how vtium stores colors",
    },
];
