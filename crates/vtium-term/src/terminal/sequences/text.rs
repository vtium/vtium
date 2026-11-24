//! The entries of the `text` rows: REP, the character sets, the C1
//! controls, line sizes, sized text, and the font OSC strings. Most rows of
//! the family are about fonts and characters, and name no sequence.

use super::{Departure, Entry, Form, Other, Param, Record, TARGET_SET};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "text.rep",
        forms: &[Form {
            form: "CSI Ps b",
            name: "REP: repeat the last character",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "How many times to repeat the character.",
            default: Some("1"),
        }],
        does: &[
            "vtium prints the last character it printed Ps more times, even when controls came between, so `y z CR LF CSI 3 b` prints `zzz` on the next row. When a single shift mapped that character, REP repeats the mapped character.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "text.charsets",
        forms: &[
            Form {
                form: "ESC ( C",
                name: "SCS: designate a set into G0",
            },
            Form {
                form: "ESC ) C",
                name: "SCS: designate a set into G1",
            },
            Form {
                form: "ESC * C",
                name: "SCS: designate a set into G2",
            },
            Form {
                form: "ESC + C",
                name: "SCS: designate a set into G3",
            },
            Form {
                form: "SI",
                name: "LS0: G0 into GL",
            },
            Form {
                form: "SO",
                name: "LS1: G1 into GL",
            },
            Form {
                form: "ESC n",
                name: "LS2: G2 into GL",
            },
            Form {
                form: "ESC o",
                name: "LS3: G3 into GL",
            },
            Form {
                form: "ESC ~",
                name: "LS1R: G1 into GR",
            },
            Form {
                form: "ESC }",
                name: "LS2R: G2 into GR",
            },
            Form {
                form: "ESC |",
                name: "LS3R: G3 into GR",
            },
            Form {
                form: "ESC N",
                name: "SS2: G2 for the next character",
            },
            Form {
                form: "ESC O",
                name: "SS3: G3 for the next character",
            },
        ],
        params: &[Param {
            name: "C",
            meaning: "The set: `B` for ASCII, `0` for DEC special graphics, and `<` for DEC supplemental graphics, which vtium designates only in 8-bit input and takes as ASCII otherwise. The national replacement sets' finals `A`, `4`, `C`, `5`, `R`, `Q`, `K`, `Y`, `E`, `6`, `Z`, `H`, `7` and `=` designate ASCII. vtium ignores any other final, such as the national sets' `f`, `9`, `%2`, `%6`, `%=`, `\"4` and `\"?`.",
            default: None,
        }],
        does: &[
            "vtium keeps four character sets, G0 to G3. GL maps the ASCII graphic characters, and GR maps the bytes 0xA1 to 0xFE of 8-bit input. At the start G0 and G1 hold ASCII, and G2 and G3 hold ASCII, or DEC supplemental graphics in 8-bit input. GL holds G0 and GR holds G2.",
            "DEC special graphics draws lines and boxes in place of letters, which programs such as ncurses use. A character decoded from UTF-8 prints as itself, whatever G1 to G3 hold. A single shift applies to the next printed character only. DECSC saves the sets and a waiting single shift.",
        ],
        departure: Some(Departure {
            says: "The finals `A`, `4`, `C`, `5`, `R`, `Q`, `K`, `Y`, `E`, `6`, `Z`, `H`, `7` and `=` of the national replacement sets designate ASCII, since vtium does not implement national replacement sets. vtium ignores the other national sets' finals.",
            readme: "crates/vtium-term/README.md",
            section: "Character sets",
        }),
        record: TARGET_SET,
    },
    Entry {
        id: "text.c1",
        forms: &[
            Form {
                form: "0x80 to 0x9F",
                name: "the C1 controls as single bytes",
            },
            Form {
                form: "ESC SP F",
                name: "S7C1T: send 7-bit controls",
            },
            Form {
                form: "ESC SP G",
                name: "S8C1T: send 8-bit controls",
            },
        ],
        params: &[],
        does: &[
            "In 8-bit input vtium acts on the C1 controls sent as single bytes, such as 0x9B for CSI. In UTF-8 input it ignores U+0080 to U+009F, so a program that strips ESC from untrusted text stops every control. Each C1 control also works in its 7-bit form, ESC and a second byte.",
            "vtium accepts S7C1T and S8C1T, and its replies stay 7-bit either way. A program that reads UTF-8 would take an 8-bit control for ill-formed text.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "text.kitty-sizing",
        forms: &[Form {
            form: "OSC 66 ; metadata ; Pt ST",
            name: "write text at a larger size",
        }],
        params: &[
            Param {
                name: "metadata",
                meaning: "`key=value` pairs separated by `:`. `s` is the scale, 1 to 7, and `w` the width in cells, 0 to 7. `n` over `d` is a fraction of the scale, each 0 to 15. `v` and `h` align the text vertically and horizontally, each 0 to 2.",
                default: Some("empty, which writes the text at its normal size"),
            },
            Param {
                name: "Pt",
                meaning: "The text, at most 4096 bytes of UTF-8.",
                default: None,
            },
        ],
        does: &[
            "vtium stores the block of cells that one sequence writes as one unit, so an edit that would move, clear or overwrite only part of it erases it whole. vtium ignores a key it does not know. A known key with a value out of its range discards the whole sequence, text included.",
            "The renderer draws each run at its scale, fraction and alignment across its block, from the font's glyphs at the normal size scaled up, and a cursor on a run covers the whole block. OSC 66 has no reply.",
        ],
        departure: None,
        record: Record {
            adr: 1308,
            section: "D2",
        },
    },
    Entry {
        id: "text.line-size",
        forms: &[
            Form {
                form: "ESC # 3",
                name: "DECDHL: top half of a double-height line",
            },
            Form {
                form: "ESC # 4",
                name: "DECDHL: bottom half of a double-height line",
            },
            Form {
                form: "ESC # 5",
                name: "DECSWL: single-width line",
            },
            Form {
                form: "ESC # 6",
                name: "DECDWL: double-width line",
            },
        ],
        params: &[],
        does: &[
            "vtium sets the size of the cursor's line, and draws the line at that size. A double-size line holds half the screen's columns, and the characters right of its center are lost. The cursor never stands right of the line's last column, and printing wraps after it.",
            "ED returns each line it erases whole to single size, and IL, DL and the scrolls move a line with its size. While DECLRMM is set, every line is single size and vtium ignores these sequences. The pointer's columns on a double-size line are still the screen's.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "text.font-change",
        forms: &[
            Form {
                form: "OSC 50 ; Pt ST",
                name: "switch to the font set Pt",
            },
            Form {
                form: "OSC 50 ; ? ST",
                name: "ask which font set is in use",
            },
        ],
        params: &[Param {
            name: "Pt",
            meaning: "The name of a font set or a built-in font (1 to 64 characters of `A-Z a-z 0-9 . _ + -`, not starting with `.`), the absolute path of a font that root has installed and trusts (at most 4095 bytes), or the font family a head's configuration names.",
            default: None,
        }],
        does: &[
            "vtium switches to the font set that Pt names. For a font's path, vtium first checks that root has installed the font and trusts its folder, and then builds a font set of that one font. When the new font's character cell has another size, vtium resizes the grid to fit it. The query gets `OSC 50 ; NAME`, ended as it was asked, where NAME is the set in use, the font's path, or the family's name. While the `allow-font-ops` key is off, vtium ignores both forms.",
        ],
        departure: Some(Departure {
            says: "vtium takes three kinds of name: one of its font sets, the path of a font that root has installed and trusts, and the family a head's configuration names. It ignores anything else, such as an X font name or the path of any other font, and it also ignores a Pt that starts with `#`, the form that picks from a font menu.",
            readme: "crates/vtium-term/README.md",
            section: "OSC strings and the Linux console's sequences",
        }),
        record: Record {
            adr: 1200,
            section: "D5",
        },
    },
    Entry {
        id: "text.contour-fonts",
        forms: &[Form {
            form: "OSC 60 ; Pt ST",
            name: "contour's font settings",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "The font settings, in the form contour defines.",
            default: None,
        }],
        does: &[
            "vtium takes OSC 60 as XTQALLOWED, the meaning in xterm's control sequences, and ignores it. So a string meant to change the fonts changes nothing and gets no reply.",
        ],
        departure: None,
        record: TARGET_SET,
    },
];

pub(super) const OTHER: &[Other] = &[
    Other {
        id: "text.grapheme",
        what: "how vtium measures characters",
    },
    Other {
        id: "text.vs-width",
        what: "how variation selectors set a character's width",
    },
    Other {
        id: "text.bidi",
        what: "how vtium draws right-to-left text, which the modes on the Modes page control",
    },
    Other {
        id: "text.reflow",
        what: "how vtium rewraps lines when the screen's width changes",
    },
    Other {
        id: "text.sprites",
        what: "how vtium draws box drawing and block characters",
    },
    Other {
        id: "text.psf",
        what: "a font format",
    },
    Other {
        id: "text.outline",
        what: "a font format",
    },
    Other {
        id: "text.emoji",
        what: "a font feature",
    },
    Other {
        id: "text.overhang",
        what: "how vtium draws a glyph wider than its cell",
    },
    Other {
        id: "text.ligatures",
        what: "a font feature",
    },
    Other {
        id: "text.font-merge",
        what: "a font feature",
    },
    Other {
        id: "text.vitrify",
        what: "a way of building fonts that vtium refuses",
    },
];
