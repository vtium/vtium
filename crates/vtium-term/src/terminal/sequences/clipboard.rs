//! The entries of the `clipboard`, `osc` and `dcs` rows: the clipboard,
//! links, prompt marks, and the other OSC and DCS strings that programs
//! send.

use super::{Departure, Entry, Form, Other, Param, REFUSALS, REPLIES, Record, TARGET_SET};

/// The README section that records how vtium reads these OSC strings.
const OSC_README: &str = "crates/vtium-term/README.md";
/// That README's heading.
const OSC_SECTION: &str = "OSC strings and the Linux console's sequences";

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "osc.52.write",
        forms: &[Form {
            form: "OSC 52 ; Pc ; Pd ST",
            name: "set the clipboard",
        }],
        params: &[
            Param {
                name: "Pc",
                meaning: "The selections to set. `c`, `p`, `s` and an empty list all set vtium's one selection. `q` and the cut buffers `0` to `7` set nothing.",
                default: Some("an empty list"),
            },
            Param {
                name: "Pd",
                meaning: "The text to copy, encoded in base64.",
                default: None,
            },
        ],
        does: &[
            "vtium decodes the text and hands it to the console's clipboard, where a paste finds it. The text is UTF-8, and each ill-formed sequence in it becomes U+FFFD. A string that is too long, or that the parser cut short, sets nothing, and the selection keeps what it held.",
        ],
        departure: Some(Departure {
            says: "Data that is not base64 sets nothing, where ctlseqs clears the selection.",
            readme: OSC_README,
            section: OSC_SECTION,
        }),
        record: REPLIES,
    },
    Entry {
        id: "osc.52.read",
        forms: &[Form {
            form: "OSC 52 ; Pc ; ? ST",
            name: "ask for the clipboard's text",
        }],
        params: &[],
        does: &[
            "vtium sends no reply. The reply would hand the clipboard's text to whatever program reads the terminal's input, which may not be the program the user copied for.",
        ],
        departure: None,
        record: REFUSALS,
    },
    Entry {
        id: "osc.5522",
        forms: &[Form {
            form: "OSC 5522 ; Pt ST",
            name: "kitty's clipboard with several types",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "kitty's metadata for the clipboard request, then `;` and the payload.",
            default: None,
        }],
        does: &[
            "vtium reads the string and does nothing with it, and a read gets no reply. Its reads would hand clipboard data to the program.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "osc.8",
        forms: &[
            Form {
                form: "OSC 8 ; params ; URI ST",
                name: "start a link",
            },
            Form {
                form: "OSC 8 ; ; ST",
                name: "end the link",
            },
        ],
        params: &[
            Param {
                name: "params",
                meaning: "`key=value` pairs separated by `:`. The key `id` joins cells printed apart into one link.",
                default: None,
            },
            Param {
                name: "URI",
                meaning: "The link's address, from 1 to 2,083 bytes of printable ASCII with a scheme such as `https:`.",
                default: None,
            },
        ],
        does: &[
            "vtium links each cell printed while the link is open, and keeps each distinct link once. It never opens a link. It underlines a link while the pointer is over it, copies it with Ctrl+click, and shows its address while Ctrl is held.",
            "A URI past its bound, without a scheme, or with a byte outside printable ASCII closes the open link. RIS and a switch of screens end the open link, and an erased cell takes no link.",
        ],
        departure: Some(Departure {
            says: "vtium refuses a space in a URI, which the specification allows, so that the address vtium shows is the one a copy carries.",
            readme: OSC_README,
            section: OSC_SECTION,
        }),
        record: Record {
            adr: 1002,
            section: "3. What a link does",
        },
    },
    Entry {
        id: "osc.133",
        forms: &[
            Form {
                form: "OSC 133 ; A ST",
                name: "a prompt starts",
            },
            Form {
                form: "OSC 133 ; B ST",
                name: "the user's input starts",
            },
            Form {
                form: "OSC 133 ; C ST",
                name: "the command's output starts",
            },
            Form {
                form: "OSC 133 ; D ; Ps ST",
                name: "the command ends, with its exit status",
            },
            Form {
                form: "OSC 133 ; I ST",
                name: "the user's input starts, as B",
            },
            Form {
                form: "OSC 133 ; L ST",
                name: "move to a fresh line",
            },
            Form {
                form: "OSC 133 ; N ST",
                name: "a prompt starts, as A",
            },
            Form {
                form: "OSC 133 ; P ST",
                name: "a part of the prompt, which vtium ignores",
            },
        ],
        params: &[],
        does: &[
            "vtium marks the line where each part starts, so the user can jump from one prompt to the next. A and N first move to a fresh line when the cursor is not in the first column, and L only does that.",
            "P, D and the options change nothing, and vtium does not keep D's exit status. ED 2, ED 3 and RIS clear the marks of the lines they erase.",
        ],
        departure: None,
        record: Record {
            adr: 1002,
            section: "5. Prompt marks",
        },
    },
    Entry {
        id: "osc.7",
        forms: &[Form {
            form: "OSC 7 ; URI ST",
            name: "report the working directory",
        }],
        params: &[],
        does: &["vtium reads the string and does nothing with it."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "osc.notify",
        forms: &[
            Form {
                form: "OSC 9 ; Pt ST",
                name: "show a notification",
            },
            Form {
                form: "OSC 777 ; notify ; Pt ST",
                name: "show a notification with a title",
            },
            Form {
                form: "OSC 99 ; Pt ST",
                name: "kitty's notifications",
            },
        ],
        params: &[Param {
            name: "Pt",
            meaning: "For OSC 9, the notification's text. For OSC 777, the title and the body, separated by `;`. For OSC 99, kitty's metadata, then `;` and the payload.",
            default: None,
        }],
        does: &[
            "vtium reads these strings and does nothing with them. OSC 99's queries get no reply.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "osc.9.4",
        forms: &[Form {
            form: "OSC 9 ; 4 ; Ps ; Ps ST",
            name: "report progress",
        }],
        params: &[Param {
            name: "Ps ; Ps",
            meaning: "The state, then the progress from 0 to 100. The state is 0 to remove the progress, 1 to set it, 2 for an error, 3 for progress of unknown length, and 4 for paused.",
            default: None,
        }],
        does: &["vtium reads the string and does nothing with it."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "osc.1337",
        forms: &[Form {
            form: "OSC 1337 ; Pt ST",
            name: "iTerm2's other strings, such as SetUserVar",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "A command's name, `=` and its argument, such as `SetUserVar=name=value`, where the value is in base64.",
            default: None,
        }],
        does: &[
            "vtium reads these strings and does nothing with them. The strings that carry pictures, `File=`, `MultipartFile=`, `FilePart=` and `FileEnd`, have a page of their own, image.iterm2.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "osc.46",
        forms: &[Form {
            form: "OSC 46 ; Pt ST",
            name: "change the log file",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "The name of the file xterm logs to.",
            default: None,
        }],
        does: &[
            "vtium does nothing with this string. It would write a file on the host at a program's request.",
        ],
        departure: None,
        record: REFUSALS,
    },
    Entry {
        id: "osc.app-specific",
        forms: &[
            Form {
                form: "OSC 51 ; Pt ST",
                name: "reserved for Emacs",
            },
            Form {
                form: "OSC 176 ; Pt ST",
                name: "set an application id",
            },
            Form {
                form: "OSC 555 ST",
                name: "flash the screen",
            },
            Form {
                form: "OSC 3008 ; Pt ST",
                name: "report a context",
            },
            Form {
                form: "OSC 7501 ; Pt ST",
                name: "report a program's status",
            },
        ],
        params: &[Param {
            name: "Pt",
            meaning: "The string's text, in the form that the program or terminal that defines the number gives it.",
            default: None,
        }],
        does: &["vtium reads these strings and does nothing with them."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "osc.5113",
        forms: &[Form {
            form: "OSC 5113 ; Pt ST",
            name: "kitty's file transfer",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "kitty's metadata for the transfer, as `key=value` pairs, and the data.",
            default: None,
        }],
        does: &[
            "vtium does nothing with this string. It would read and write files on the host at the request of whatever wrote to the terminal.",
        ],
        departure: None,
        record: REFUSALS,
    },
    Entry {
        id: "osc.72",
        forms: &[Form {
            form: "OSC 72 ; Pt ST",
            name: "kitty's drag and drop",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "kitty's metadata for the drag and drop, then `;` and the payload.",
            default: None,
        }],
        does: &["vtium reads the string and does nothing with it."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "dcs.kitty-cmd",
        forms: &[Form {
            form: "DCS @kitty-cmd Pt ST",
            name: "kitty's remote control",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "A JSON object that names the command and its arguments.",
            default: None,
        }],
        does: &[
            "vtium does nothing with this string, so program output never drives the terminal.",
        ],
        departure: None,
        record: REFUSALS,
    },
    Entry {
        id: "dcs.tmux",
        forms: &[Form {
            form: "DCS 1000 p Pt ST",
            name: "tmux's control mode",
        }],
        params: &[Param {
            name: "Pt",
            meaning: "tmux's control mode output, which starts with `%begin`.",
            default: None,
        }],
        does: &[
            "vtium reads the string and does nothing with it. tmux runs on the console as a plain program.",
        ],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "osc.kmscon-display",
        forms: &[
            Form {
                form: "OSC setBackground ST",
                name: "hand the display to a graphical program",
            },
            Form {
                form: "OSC setForeground ST",
                name: "take the display back",
            },
        ],
        params: &[],
        does: &[
            "A program writes each string with no number after OSC, and BEL may end it as ST does. vtium takes both while the key `allow-display-ops` is on, which it is by default. Only a console that a head shows may send setBackground. Neither string gets a reply.",
            "On the kernel's VT, setBackground gives up every display card the sending head draws on. vtium draws again after setForeground from the same console, when that console's session ends, or when vtium's VT is shown again. Under a seat manager, vtium ignores setBackground and logs that it did.",
        ],
        departure: None,
        record: REPLIES,
    },
];

pub(super) const OTHER: &[Other] = &[
    Other {
        id: "clipboard.selection",
        what: "selecting and copying text with the pointer",
    },
    Other {
        id: "clipboard.paste",
        what: "pasting with the middle button and Shift+Insert",
    },
    Other {
        id: "clipboard.copy-tool",
        what: "a program vtium installs",
    },
];
