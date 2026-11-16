//! The entries of the `image` rows: the strings that carry pictures, sixel,
//! kitty's graphics protocol and iTerm2's inline images, and the graphics
//! strings that vtium ignores.

use super::{Departure, Entry, Form, Other, Param, Record, TARGET_SET};

/// The minspec build's rule for every picture string, said once for each
/// protocol's page.
const MINSPEC: &str = "vtium shows pictures only in a build with the `images` feature, and the minspec build, the default, ignores the string. A build with pictures offers a protocol only while its switch is on, `image-storage-limit` is above 0, and every display of the head can draw pictures.";

/// kitty's command string, which every kitty row's forms share.
const KEYS: Param = Param {
    name: "keys",
    meaning: "The command's keys, `key=value` pairs separated by commas, such as `a=T,f=100,i=1`. The key `a` names the action.",
    default: None,
};

/// kitty's payload, after the keys.
const PAYLOAD: Param = Param {
    name: "Pt",
    meaning: "The payload, in base64: the picture's data, or the name of a file or of shared memory to read it from.",
    default: None,
};

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "image.sixel",
        forms: &[Form {
            form: "DCS P1 ; P2 ; P3 q Pt ST",
            name: "a sixel picture",
        }],
        params: &[
            Param {
                name: "P1",
                meaning: "The pixel aspect ratio. vtium reads it and draws each dot as one square pixel.",
                default: Some("0"),
            },
            Param {
                name: "P2",
                meaning: "1 leaves the pixels that no dot sets transparent. Any other value fills them with color register 0.",
                default: Some("0"),
            },
            Param {
                name: "P3",
                meaning: "The horizontal grid size, which vtium ignores.",
                default: None,
            },
            Param {
                name: "Pt",
                meaning: "The picture: sixel characters, color register commands, repeats and raster attributes.",
                default: None,
            },
        ],
        does: &[
            MINSPEC,
            "vtium decodes the picture as it arrives and places it at the cursor. The cursor ends on the picture's last row, or after its right edge with mode 8452. With mode 80 set, the picture sits at the screen's top left and the cursor stays where it is. Mode 1070 gives each picture its own color registers.",
            "vtium reserves a picture's memory under the console's image budget before it decodes the pixels. A picture that cannot fit is refused.",
        ],
        departure: Some(Departure {
            says: "vtium draws each sixel dot as one square pixel, where DEC's VT340 manual draws a dot twice as tall as it is wide.",
            readme: "crates/vtium-image/README.md",
            section: "How sixel works",
        }),
        record: Record {
            adr: 1100,
            section: "D3",
        },
    },
    Entry {
        id: "image.kitty",
        forms: &[Form {
            form: "APC G keys ; Pt ST",
            name: "a kitty graphics command",
        }],
        params: &[KEYS, PAYLOAD],
        does: &[
            MINSPEC,
            "vtium takes every action of the protocol: transmit, transmit and show, put, delete, query, and the animation actions. A program may send a picture in chunks, and vtium reserves the whole picture's memory at the first chunk. vtium decodes PNG and zlib data, and opens files and shared memory, only in its sandboxed image helper.",
            "A command that carries an image id or number gets an answer, `OK` or an error, unless the key `q` quiets it. The answer holds only numbers and fixed text that vtium writes itself.",
        ],
        departure: None,
        record: Record {
            adr: 1100,
            section: "D14",
        },
    },
    Entry {
        id: "image.kitty-animation",
        forms: &[
            Form {
                form: "APC G a=f,keys ; Pt ST",
                name: "send or edit a frame",
            },
            Form {
                form: "APC G a=a,keys ST",
                name: "control the animation",
            },
            Form {
                form: "APC G a=c,keys ST",
                name: "compose one frame onto another",
            },
        ],
        params: &[KEYS, PAYLOAD],
        does: &[
            "vtium keeps the frames as the program sent them and composes each one before it draws it. The frames count against a frame allowance, five times the console's image budget, and never evict a still picture. vtium plays the frames with their gaps and loops, and the key `s` starts and stops the animation.",
        ],
        departure: None,
        record: Record {
            adr: 1102,
            section: "D1",
        },
    },
    Entry {
        id: "image.kitty-relative",
        forms: &[Form {
            form: "APC G a=p,P=Ps,Q=Ps,H=Ps,V=Ps,keys ST",
            name: "place a picture relative to another",
        }],
        params: &[
            Param {
                name: "P",
                meaning: "The parent's image id.",
                default: None,
            },
            Param {
                name: "Q",
                meaning: "The parent's placement id. Without it, vtium takes the parent image's placement made last that has no placement id.",
                default: None,
            },
            Param {
                name: "H, V",
                meaning: "How many cells the placement sits to the right of and below its parent.",
                default: Some("0"),
            },
        ],
        does: &[
            "vtium places the picture relative to its parent, so the two move together. A put whose parent does not exist answers ENOPARENT. A chain that loops answers ECYCLE, and a chain that is too deep answers ETOODEEP.",
        ],
        departure: None,
        record: Record {
            adr: 1100,
            section: "D13",
        },
    },
    Entry {
        id: "image.kitty-placeholders",
        forms: &[Form {
            form: "APC G a=p,U=1,keys ST",
            name: "make a virtual placement",
        }],
        params: &[KEYS],
        does: &[
            "A virtual placement draws nothing on its own. A program shows it by writing kitty's placeholder character, U+10EEEE, into the text, with marks that name the picture's row and column. vtium draws the part of the picture under each run of placeholder cells. So the picture moves with the text, as inside a multiplexer's pane.",
        ],
        departure: None,
        record: Record {
            adr: 1100,
            section: "D13",
        },
    },
    Entry {
        id: "image.kitty-feature-query",
        forms: &[Form {
            form: "APC G a=q,e=Pt,keys ST",
            name: "ask which keys an action takes",
        }],
        params: &[Param {
            name: "e",
            meaning: "The action the program asks about, such as `t` or `p`.",
            default: None,
        }],
        does: &[
            "vtium answers each of the protocol's eight actions. The answer lists each key of the query that a real command of that action takes, with its value. vtium writes every value in the answer itself, and `e` on any other action is refused.",
        ],
        departure: None,
        record: Record {
            adr: 1100,
            section: "D14",
        },
    },
    Entry {
        id: "image.iterm2",
        forms: &[
            Form {
                form: "OSC 1337 ; File=keys : Pt ST",
                name: "a picture in one string",
            },
            Form {
                form: "OSC 1337 ; MultipartFile=keys ST",
                name: "start a picture sent in parts",
            },
            Form {
                form: "OSC 1337 ; FilePart=Pt ST",
                name: "a part of the picture",
            },
            Form {
                form: "OSC 1337 ; FileEnd ST",
                name: "end the picture sent in parts",
            },
        ],
        params: &[
            Param {
                name: "keys",
                meaning: "`key=value` pairs separated by `;`. vtium shows only a picture with `inline=1`. `width` and `height` size it, and `preserveAspectRatio=0` lets it stretch.",
                default: None,
            },
            Param {
                name: "Pt",
                meaning: "The file, in base64.",
                default: None,
            },
        ],
        does: &[
            MINSPEC,
            "vtium's image helper decodes the file as PNG, JPEG or GIF, and vtium places the picture at the cursor. The text scrolls when the picture needs room, and the cursor ends on the picture's last row. An animated GIF plays its frames.",
        ],
        departure: None,
        record: Record {
            adr: 1103,
            section: "D1",
        },
    },
    Entry {
        id: "image.regis",
        forms: &[Form {
            form: "DCS p Pt ST",
            name: "a ReGIS drawing",
        }],
        params: &[],
        does: &["vtium reads the string and does nothing with it."],
        departure: None,
        record: TARGET_SET,
    },
    Entry {
        id: "image.decdld",
        forms: &[Form {
            form: "DCS Pm { Pt ST",
            name: "DECDLD: load a soft font",
        }],
        params: &[],
        does: &[
            "vtium reads the string and does nothing with it. Its fonts come from outline and bitmap font files.",
        ],
        departure: None,
        record: TARGET_SET,
    },
];

pub(super) const OTHER: &[Other] = &[
    Other {
        id: "image.pipeline",
        what: "how vtium stores and places pictures",
    },
    Other {
        id: "image.overlap",
        what: "how one picture covers another",
    },
    Other {
        id: "image.compositor",
        what: "how the CPU renderer draws pictures",
    },
    Other {
        id: "image.helper",
        what: "the sandboxed program that decodes pictures",
    },
    Other {
        id: "image.login",
        what: "a picture behind the login prompt, chosen in vtium.conf",
    },
];
