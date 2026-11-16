//! The entries of the `ctl` rows: the control functions that move the
//! cursor, edit the screen, set margins and tab stops, reset the terminal,
//! and the Linux console's private sequences.

use super::{Departure, Entry, Form, Other, Param, Record, TARGET_SET};

/// The count of the cursor movement and editing functions that take one.
const COUNT: Param = Param {
    name: "Ps",
    meaning: "In CUU, CUD, CUF, CUB, CNL, CPL, HPR, VPR, ECH, ICH, DCH, IL, DL, SU and SD, how many rows, columns, characters or lines the function acts on. A count of 0 counts as 1.",
    default: Some("1"),
};

/// ED's and EL's choice of what to erase.
const ERASE_PART: Param = Param {
    name: "Ps",
    meaning: "In ED and EL, which part to erase: 0 from the cursor to the end, 1 from the start to the cursor, and 2 all of it. ED 3 has a page of its own, ctl.ed3.",
    default: Some("0"),
};

/// The column of CHA and HPA, and the row of VPA.
const POSITION: Param = Param {
    name: "Ps",
    meaning: "In CHA and HPA, the column, and in VPA, the row, counted from 1. A value of 0 counts as 1.",
    default: Some("1"),
};

/// The section of ADR-1004 that decides SCP and SPD.
const BIDI_PARAGRAPHS: Record = Record {
    adr: 1004,
    section: "D2",
};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "ctl.edit",
        forms: &[
            Form {
                form: "CSI Ps A",
                name: "CUU: cursor up",
            },
            Form {
                form: "CSI Ps B",
                name: "CUD: cursor down",
            },
            Form {
                form: "CSI Ps C",
                name: "CUF: cursor forward",
            },
            Form {
                form: "CSI Ps D",
                name: "CUB: cursor back",
            },
            Form {
                form: "CSI Ps E",
                name: "CNL: cursor to the start of a following line",
            },
            Form {
                form: "CSI Ps F",
                name: "CPL: cursor to the start of a preceding line",
            },
            Form {
                form: "CSI Ps G",
                name: "CHA: cursor to a column",
            },
            Form {
                form: "CSI Ps `",
                name: "HPA: cursor to a column, as CHA does",
            },
            Form {
                form: "CSI Ps a",
                name: "HPR: cursor forward, as CUF does",
            },
            Form {
                form: "CSI Ps d",
                name: "VPA: cursor to a row",
            },
            Form {
                form: "CSI Ps e",
                name: "VPR: cursor down, as CUD does",
            },
            Form {
                form: "CSI Ps ; Ps H",
                name: "CUP: cursor to a row and column",
            },
            Form {
                form: "CSI Ps ; Ps f",
                name: "HVP: cursor to a row and column, as CUP does",
            },
            Form {
                form: "CSI Ps J",
                name: "ED: erase in display",
            },
            Form {
                form: "CSI Ps K",
                name: "EL: erase in line",
            },
            Form {
                form: "CSI Ps X",
                name: "ECH: erase characters",
            },
            Form {
                form: "CSI Ps @",
                name: "ICH: insert blank characters",
            },
            Form {
                form: "CSI Ps P",
                name: "DCH: delete characters",
            },
            Form {
                form: "CSI Ps L",
                name: "IL: insert lines",
            },
            Form {
                form: "CSI Ps M",
                name: "DL: delete lines",
            },
            Form {
                form: "CSI Ps S",
                name: "SU: scroll up",
            },
            Form {
                form: "CSI Ps T",
                name: "SD: scroll down",
            },
            Form {
                form: "ESC D",
                name: "IND: index, down one row",
            },
            Form {
                form: "ESC M",
                name: "RI: reverse index, up one row",
            },
            Form {
                form: "ESC E",
                name: "NEL: next line",
            },
        ],
        params: &[
            COUNT,
            ERASE_PART,
            POSITION,
            Param {
                name: "Ps ; Ps",
                meaning: "In CUP and HVP, the row and then the column, both counted from 1. An omitted or 0 row or column is 1.",
                default: Some("1 ; 1, the top left corner"),
            },
        ],
        does: &[
            "These functions move the cursor, erase text, and insert or delete characters and lines. vtium acts on each only when it has no private marker such as `?` and no intermediate byte, since those make a different function.",
            "ED 0 erases from the cursor to the end of the screen, ED 1 from the start of the screen to the cursor, and ED 2 the whole screen. EL 0, 1 and 2 erase the same parts of the cursor's line. An erased cell is empty and takes the pen's background color.",
            "IL, DL, SU, SD, IND, RI and NEL move lines only inside the scroll region that DECSTBM sets. When text scrolls up on the main screen, the line that leaves the region's top enters the scrollback. While left and right margins narrower than the screen are set, only the cells between them move, and nothing enters the scrollback. The 8-bit controls 0x84, 0x8D and 0x85 work as IND, RI and NEL.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.ed3",
        forms: &[Form {
            form: "CSI 3 J",
            name: "ED 3: erase the scrollback",
        }],
        params: &[],
        does: &[
            "vtium empties the scrollback and leaves the screen as it is. The `clear` command sends ED 3 after it erases the screen, so the history goes too.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.tabs",
        forms: &[
            Form {
                form: "ESC H",
                name: "HTS: set a tab stop at the cursor's column",
            },
            Form {
                form: "CSI Ps g",
                name: "TBC: clear tab stops",
            },
            Form {
                form: "CSI Ps I",
                name: "CHT: forward to a following tab stop",
            },
            Form {
                form: "CSI Ps Z",
                name: "CBT: back to a preceding tab stop",
            },
        ],
        params: &[Param {
            name: "Ps",
            meaning: "For TBC, 0 clears the stop at the cursor's column and 3 clears every stop, and vtium ignores other values. For CHT and CBT, how many stops to move, where 0 counts as 1.",
            default: Some("0 for TBC, and 1 for CHT and CBT"),
        }],
        does: &[
            "vtium starts with a tab stop every 8 columns. HT, CHT and CBT move the cursor to the next or previous stop, and HT stops at the right margin when no stop is left. The 8-bit control 0x88 works as HTS.",
            "DECSTR keeps the tab stops, and RIS returns them to a stop every 8 columns.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.decst8c",
        forms: &[Form {
            form: "CSI ? 5 W",
            name: "DECST8C: set a tab stop every 8 columns",
        }],
        params: &[],
        does: &[
            "vtium reads the sequence and does nothing with it. RIS sets a tab stop every 8 columns.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.decstbm",
        forms: &[Form {
            form: "CSI Ps ; Ps r",
            name: "DECSTBM: set the top and bottom margins",
        }],
        params: &[Param {
            name: "Ps ; Ps",
            meaning: "The top and bottom rows of the scroll region, counted from 1.",
            default: Some("the whole screen, from the first row to the last"),
        }],
        does: &[
            "DECSTBM sets the scroll region that scrolling, line feeds, and inserting and deleting lines keep within. It then moves the cursor home, which in origin mode is the region's top left corner.",
            "vtium ignores a region whose top is not above its bottom, and keeps the region and the cursor as they were. A bottom margin past the screen's last row stops at that row.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.decslrm",
        forms: &[
            Form {
                form: "CSI Ps ; Ps s",
                name: "DECSLRM: set the left and right margins, while mode 69 is set",
            },
            Form {
                form: "CSI s",
                name: "SCOSC: save the cursor, while mode 69 is reset",
            },
            Form {
                form: "CSI u",
                name: "SCORC: restore the cursor",
            },
        ],
        params: &[Param {
            name: "Ps ; Ps",
            meaning: "The left and right columns of the margins, counted from 1.",
            default: Some("the whole width, from the first column to the last"),
        }],
        does: &[
            "DECSLRM and SCOSC share `CSI s`. While DECLRMM, mode 69, is set, `CSI s` sets the left and right margins. While it is reset, `CSI s` with no parameter saves the cursor as DECSC does, and `CSI u` restores it as DECRC does.",
            "Printing, cursor movement, editing, scrolling and origin mode keep inside the left and right margins. DECSLRM moves the cursor home, and DECRQSS reports the margins. Resetting mode 69 returns the margins to the whole width.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.decic",
        forms: &[
            Form {
                form: "CSI Ps ' }",
                name: "DECIC: insert columns",
            },
            Form {
                form: "CSI Ps ' ~",
                name: "DECDC: delete columns",
            },
            Form {
                form: "ESC 6",
                name: "DECBI: back index",
            },
            Form {
                form: "ESC 9",
                name: "DECFI: forward index",
            },
        ],
        params: &[Param {
            name: "Ps",
            meaning: "How many columns DECIC inserts or DECDC deletes. A count of 0 counts as 1.",
            default: Some("1"),
        }],
        does: &[
            "DECIC and DECDC insert and delete columns at the cursor's column. They act on each row from the top margin to the bottom margin, up to the right margin, and do nothing while the cursor is outside those margins.",
            "DECBI and DECFI move the cursor one column left or right. At the left or right margin, they move the columns between the margins instead.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.decsc",
        forms: &[
            Form {
                form: "ESC 7",
                name: "DECSC: save the cursor",
            },
            Form {
                form: "ESC 8",
                name: "DECRC: restore the cursor",
            },
        ],
        params: &[],
        does: &[
            "DECSC saves the cursor's position and a pending wrap, the pen, autowrap, origin mode and the character sets. The main screen and the alternate screen each keep a saved cursor of their own.",
            "DECRC restores what DECSC saved for the screen in use. With nothing saved, it moves the cursor home and returns the pen and those modes to their defaults.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.decstr",
        forms: &[
            Form {
                form: "CSI ! p",
                name: "DECSTR: soft terminal reset",
            },
            Form {
                form: "ESC c",
                name: "RIS: reset to initial state",
            },
        ],
        params: &[],
        does: &[
            "DECSTR returns the modes, the pen, the cursor's style, the character sets, the saved cursors, the top and bottom margins, and the 256 palette colors to their initial state. The mouse modes and mode 1004 are among the modes it resets. It keeps the text, the cursor's position, the tab stops, and the left and right margins.",
            "RIS does all of that, then erases the screen and the scrollback and moves the cursor home. It also sets a tab stop every 8 columns, returns every color a program set, and empties the SGR and color stacks.",
        ],
        departure: Some(Departure {
            says: "DECSTR turns autowrap on, where the VT510 manual's DECSTR table turns it off (\"No autowrap\"). Terminfo entries that say the terminal wraps send DECSTR to reset it, so autowrap has to stay on.",
            readme: "crates/vtium-term/README.md",
            section: "Resets",
        }),
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.deckpam",
        forms: &[
            Form {
                form: "ESC =",
                name: "DECKPAM: keypad in application mode",
            },
            Form {
                form: "ESC >",
                name: "DECKPNM: keypad in numeric mode",
            },
        ],
        params: &[],
        does: &[
            "DECKPAM makes the keypad's keys send their application sequences, and DECKPNM makes them send their digits and symbols. The two set and reset DECNKM, mode 66, so DECRQM reports the keypad's state through that mode.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.decscl",
        forms: &[Form {
            form: "CSI Pl ; Pc \" p",
            name: "DECSCL: set the conformance level",
        }],
        params: &[
            Param {
                name: "Pl",
                meaning: "The level: 61 for a VT100, and 62 to 64 for a VT200 to a VT400.",
                default: None,
            },
            Param {
                name: "Pc",
                meaning: "Whether replies use 7-bit or 8-bit controls. vtium reads it and keeps its replies 7-bit.",
                default: None,
            },
        ],
        does: &[
            "vtium first does a soft reset, as DECSTR does. Level 61 then selects 7-bit input, and levels 62 to 64 select 8-bit input with DEC supplemental graphics in G2 and G3. Any other level leaves the input as UTF-8.",
            "DECRQSS reports the level that DECSCL set, and 62 until a program sets one.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.vt52",
        forms: &[Form {
            form: "CSI ? 2 l",
            name: "DECANM reset: enter VT52 mode",
        }],
        params: &[],
        does: &[
            "vtium stays in ANSI mode. Resetting DECANM does nothing, and vtium does not read VT52's escape sequences. DECRQM reports DECANM as permanently set.",
        ],
        departure: None,
        record: Record {
            adr: 1000,
            section: "Declined for now",
        },
    },
    Entry {
        id: "ctl.decaln",
        forms: &[Form {
            form: "ESC # 8",
            name: "DECALN: fill the screen with E",
        }],
        params: &[],
        does: &[
            "vtium fills every cell with an E in the default style, so a person can check the screen's alignment. It turns origin mode off, returns all four margins to the whole screen, and moves the cursor home.",
            "The pen keeps its colors and its protection from DECSCA, and loses its other attributes.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.rect",
        forms: &[
            Form {
                form: "CSI Pt ; Pl ; Pb ; Pr ; Pp ; Pt ; Pl ; Pp $ v",
                name: "DECCRA: copy a rectangle",
            },
            Form {
                form: "CSI Pc ; Pt ; Pl ; Pb ; Pr $ x",
                name: "DECFRA: fill a rectangle with a character",
            },
            Form {
                form: "CSI Pt ; Pl ; Pb ; Pr $ z",
                name: "DECERA: erase a rectangle",
            },
            Form {
                form: "CSI Pt ; Pl ; Pb ; Pr $ {",
                name: "DECSERA: erase the unprotected cells of a rectangle",
            },
            Form {
                form: "CSI Pt ; Pl ; Pb ; Pr ; Pm $ r",
                name: "DECCARA: change the attributes of a rectangle",
            },
            Form {
                form: "CSI Pt ; Pl ; Pb ; Pr ; Pm $ t",
                name: "DECRARA: reverse the attributes of a rectangle",
            },
            Form {
                form: "CSI Ps * x",
                name: "DECSACE: choose what DECCARA and DECRARA act on",
            },
        ],
        params: &[
            Param {
                name: "Pt ; Pl ; Pb ; Pr",
                meaning: "The rectangle's top row, left column, bottom row and right column, counted from 1. In origin mode they count from the margins. DECCRA's second Pt and Pl give the top left corner it copies to.",
                default: Some("the whole screen"),
            },
            Param {
                name: "Pp",
                meaning: "A page number. vtium has one page, so DECCRA's page numbers change nothing.",
                default: Some("1"),
            },
            Param {
                name: "Pc",
                meaning: "The code of the character that DECFRA fills with, 32 to 126 or 160 to 255, through the character sets in use. Past 255, vtium also takes any printable character one cell wide. It ignores DECFRA for any other code, and for 0 or a code left out.",
                default: None,
            },
            Param {
                name: "Pm",
                meaning: "For DECCARA, SGR codes, which it applies as SGR applies them to the pen, so 0 turns every attribute off. For DECRARA, the attributes to reverse: 1 bold, 4 underline, 5 blink, 7 inverse and 8 conceal, or 0 for the first four. With no Pm, both act as for 0.",
                default: Some("0"),
            },
            Param {
                name: "Ps",
                meaning: "For DECSACE, 2 makes DECCARA and DECRARA act on the rectangle, and any other value on the stream of text from the first corner to the last.",
                default: Some("0, the stream"),
            },
        ],
        does: &[
            "vtium clips each rectangle to the screen, or in origin mode to the region inside all four margins. It ignores a rectangle whose top is below its bottom or whose left is right of its right.",
            "DECERA and DECSERA erase as ED does, to empty cells in the pen's background color, and DECSERA leaves the cells that DECSCA protects. When a rectangle's edge cuts a wide character, the whole character goes.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.protect",
        forms: &[
            Form {
                form: "CSI Ps \" q",
                name: "DECSCA: protect the characters printed next",
            },
            Form {
                form: "CSI ? Ps J",
                name: "DECSED: erase the unprotected cells of the display",
            },
            Form {
                form: "CSI ? Ps K",
                name: "DECSEL: erase the unprotected cells of the line",
            },
        ],
        params: &[Param {
            name: "Ps",
            meaning: "For DECSCA, 1 protects the characters printed next, 0 and 2 end the protection, and vtium ignores other values. For DECSED and DECSEL, which part to erase, as for ED and EL.",
            default: Some("0"),
        }],
        does: &[
            "DECSED, DECSEL and DECSERA leave the cells that DECSCA protected, while ED, EL and DECERA erase them like any other cell. SGR 0 keeps the protection, and DECSTR and RIS end it.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.kitty-clear",
        forms: &[Form {
            form: "CSI 22 J",
            name: "move the screen's text into the scrollback, then clear the screen",
        }],
        params: &[],
        does: &["vtium reads the sequence and does nothing with it."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.kitty-unscroll",
        forms: &[Form {
            form: "CSI Ps + T",
            name: "unscroll: bring lines back from the scrollback",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "How many lines to bring back, as SD counts the lines it scrolls.",
            default: Some("1"),
        }],
        does: &["vtium reads the sequence and does nothing with it."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.xtsave",
        forms: &[
            Form {
                form: "CSI ? Pm s",
                name: "XTSAVE: save DEC private modes",
            },
            Form {
                form: "CSI ? Pm r",
                name: "XTRESTORE: restore DEC private modes",
            },
        ],
        params: &[Param {
            name: "Pm",
            meaning: "The numbers of the DEC private modes to save or restore.",
            default: Some("every mode that is free of side effects"),
        }],
        does: &[
            "XTSAVE saves the state of each mode it lists, and XTRESTORE sets each mode back to its saved state. A mode that nothing was saved for stays as it is.",
            "With an empty list, the two act on every supported DEC private mode except DECOM, the alternate screen modes 47, 1047 and 1049, mode 1048 and mode 2026. So the empty list never moves the cursor or switches screens. DECSTR keeps what XTSAVE saved, and RIS clears it.",
            "[Modes](../modes.md) says which modes share a saved state.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.status-line",
        forms: &[
            Form {
                form: "CSI Ps $ ~",
                name: "DECSSDT: select the status line's type",
            },
            Form {
                form: "CSI Ps $ }",
                name: "DECSASD: select the active display",
            },
        ],
        params: &[Param {
            name: "Ps",
            meaning: "For DECSSDT, the status line's type: 0 none, 1 the terminal's indicator line, and 2 a line the program writes. For DECSASD, where text goes: 0 the main display, and 1 the status line.",
            default: Some("0, as the VT510 manual gives it for both"),
        }],
        does: &[
            "vtium reads these sequences and does nothing with them, since it draws no status line of its own.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.page-size",
        forms: &[
            Form {
                form: "CSI Ps $ |",
                name: "DECSCPP: set columns per page",
            },
            Form {
                form: "CSI Ps t",
                name: "DECSLPP: set lines per page, for Ps of 24 or more",
            },
            Form {
                form: "CSI Ps * |",
                name: "DECSNLS: set lines per screen",
            },
        ],
        params: &[Param {
            name: "Ps",
            meaning: "For DECSCPP, the columns per page: 80 or 132, where 0 means 80. For DECSLPP, the lines per page, 24 or more. A smaller value is an XTWINOPS operation, on the page window.manipulate. For DECSNLS, the lines on the screen.",
            default: Some("80 for DECSCPP"),
        }],
        does: &[
            "vtium reads these sequences and does nothing with them. The screen's size comes from the display and the font.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.decll",
        forms: &[Form {
            form: "CSI Ps q",
            name: "DECLL: load the keyboard's LEDs",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "0 turns every light off. 1, 2 and 3 light Num Lock, Caps Lock and Scroll Lock, and 21, 22 and 23 turn them off.",
            default: Some("0"),
        }],
        does: &[
            "vtium reads the sequence and does nothing with it. The keyboard's lights stay as the user set them.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.bell-volume",
        forms: &[
            Form {
                form: "CSI Ps SP t",
                name: "DECSWBV: set the warning bell's volume",
            },
            Form {
                form: "CSI Ps SP u",
                name: "DECSMBV: set the margin bell's volume",
            },
        ],
        params: &[Param {
            name: "Ps",
            meaning: "The volume. For DECSWBV, 0 and 1 are off, 2 to 4 low, and 5 to 8 high. For DECSMBV, 1 is off, 2 to 4 low, and 0 and 5 to 8 high.",
            default: None,
        }],
        does: &[
            "vtium reads these sequences and does nothing with them. vtium rings the VT bell, whose volume the kernel and the hardware set.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.bel",
        forms: &[Form {
            form: "BEL",
            name: "ring the bell (0x07)",
        }],
        params: &[],
        does: &[
            "While mode 1042 is set, vtium rings the VT bell for a BEL, when its bell setting is on. Mode 1042 starts set, so a program's BEL rings without the program asking. A BEL from any console rings the bell, whether that console is shown or hidden.",
            "While mode 1042 is reset, vtium ignores BEL.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.sync-update",
        forms: &[
            Form {
                form: "DCS = 1 s ST",
                name: "begin a synchronized update",
            },
            Form {
                form: "DCS = 2 s ST",
                name: "end a synchronized update",
            },
        ],
        params: &[],
        does: &[
            "The two strings set and reset mode 2026. While an update is open, vtium holds its frames, so the display keeps showing the last whole picture until the program ends the update. vtium also ends an update that stays open longer than `sync-timeout`, 200 ms by default. A `sync-timeout` of 0 turns mode 2026 off.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.linux-blank",
        forms: &[Form {
            form: "CSI 9 ; Ps ]",
            name: "set the screen blank's timeout",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "The timeout in minutes, at most 60. A value of 0 turns blanking off.",
            default: Some("0"),
        }],
        does: &[
            "vtium blanks its displays after that many idle minutes, and the sequence also wakes the displays. vtium acts on it only while the `program-blank` key is on. RIS returns the timeout to the configured one.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.linux-unblank",
        forms: &[Form {
            form: "CSI 13 ]",
            name: "wake the screen",
        }],
        params: &[],
        does: &[
            "vtium wakes its displays, as a key press does, and starts the blank's countdown again. It acts on the sequence only while the `program-blank` key is on.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.linux-powerdown",
        forms: &[Form {
            form: "CSI 14 ; Ps ]",
            name: "set the VESA powerdown interval",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "The minutes of idle time before the display powers down.",
            default: None,
        }],
        does: &[
            "vtium reads the sequence and does nothing with it. vtium has one blanking step, which `CSI 9 ; Ps ]` times.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.linux-blink-interval",
        forms: &[Form {
            form: "CSI 16 ; Ps ]",
            name: "set the cursor's blink interval",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "How long each phase of the blink lasts, in milliseconds, from 50 to 65535. Any other value returns the interval to the default.",
            default: Some("the default interval, 500 ms"),
        }],
        does: &[
            "vtium blinks the cursor of the console that receives the sequence with phases of Ps milliseconds. RIS returns the interval to the default, while DECSTR and the alternate screen keep it.",
            "Blinking text keeps the `blink` key's 500 ms. The interval applies while the cursor blinks, so a steady cursor stays steady.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "ctl.scp",
        forms: &[Form {
            form: "CSI Ps ; Ps SP k",
            name: "SCP: select the character path",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "1 sets the cursor's paragraph left to right, and 2 right to left. 0 returns it to the direction that vtium.conf's `text-direction` sets. vtium ignores other values and the second parameter.",
            default: Some("0"),
        }],
        does: &[
            "vtium sets the direction of the paragraph that holds the cursor, and draws the paragraph in that order. Only a build with the `bidi` feature has SCP, and a build without it ignores the sequence.",
        ],
        departure: None,
        record: BIDI_PARAGRAPHS,
    },
    Entry {
        id: "ctl.spd",
        forms: &[Form {
            form: "CSI Ps ; Ps SP S",
            name: "SPD: select the presentation directions",
        }],
        params: &[Param {
            name: "Ps",
            meaning: "0 sets the cursor's paragraph left to right, as SCP 1 does, and 3 right to left, as SCP 2 does. vtium ignores other values and the second parameter.",
            default: Some("0"),
        }],
        does: &[
            "vtium takes SPD 0 and 3 as other names for SCP 1 and 2. Only a build with the `bidi` feature has SPD, and a build without it ignores the sequence.",
        ],
        departure: None,
        record: BIDI_PARAGRAPHS,
    },
];

pub(super) const OTHER: &[Other] = &[Other {
    id: "ctl.bce",
    what: "a rule for the color of the cells that the erase functions empty",
}];
