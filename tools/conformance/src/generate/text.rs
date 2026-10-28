//! Text, attributes, cursor motion, erasing, clusters and widths, modes,
//! replies, and the control strings around them.

use super::{CSI, Case, DCS, ESC, Inputs, Made, OSC, ST, Stream, case, cup, sgr, sized};

pub(super) const CASES: &[Case] = &[
    case("sgr-basic", sgr_basic),
    case("sgr-extended-color", sgr_extended_color),
    case("sgr-underline-extended", sgr_underline_extended),
    case("sgr-color-forms-fg", |_| color_forms(38, "").into()),
    case("sgr-color-forms-bg", |_| color_forms(48, "").into()),
    // the underline color on a single underline
    case("sgr-color-forms-ul", |_| color_forms(58, "4;").into()),
    case("sgr-underline-styles", sgr_underline_styles),
    case("sgr-strike", sgr_strike),
    case("sgr-underline-color-reset", sgr_underline_color_reset),
    case("sgr-subparam-unknown", sgr_subparam_unknown),
    case("sgr-color-colon-out-of-range", sgr_color_colon_out_of_range),
    case(
        "sgr-color-semicolon-subparams",
        sgr_color_semicolon_subparams,
    ),
    case("sgr-58-semicolon-no-blink", sgr_58_semicolon_no_blink),
    // sub-parameters past the eighth are dropped; the parameter and the
    // rest of the sequence still apply
    case("sgr-subparam-overflow", |_| {
        [CSI, "38:2:0:10:20:30:0:0:0:99:99;1m", "bold", &sgr(&[0])]
            .concat()
            .into()
    }),
    case("csi-subparam-non-sgr", csi_subparam_non_sgr),
    case("colour-resolve", color_resolve),
    case("cursor-motion", cursor_motion),
    case("erase", erase),
    sized("erase-display-above", 20, 6, |_| {
        (fill_letters() + &cup(3, 10) + CSI + "1J").into()
    }),
    case("insert-delete", insert_delete),
    sized("scroll-region", 20, 8, scroll_region),
    sized("origin-mode", 20, 8, |_| {
        [
            CSI,
            "3;6r",
            CSI,
            "?6h",
            &cup(1, 1),
            "origin",
            CSI,
            "?6l",
            &cup(1, 1),
            "absolute",
        ]
        .concat()
        .into()
    }),
    sized("autowrap", 10, 4, |_| {
        [
            "0123456789wrapped\r\n",
            CSI,
            "?7l",
            "0123456789overwrite",
            CSI,
            "?7h",
            "\r\nx",
        ]
        .concat()
        .into()
    }),
    case("tabs", tabs),
    sized("wide-chars", 12, 4, |_| {
        "\u{6f22}\u{5b57}\u{30c6}\u{30b9}\u{30c8}\r\n\u{ff21}\u{ff22}\u{ff23}\r\n\
         0123456789A\u{6f22}\r\nab\u{d55c}\u{ad6d}\u{c5b4}"
            .to_owned()
            .into()
    }),
    case("combining", |_| {
        "e\u{301} a\u{303}\u{300} n\u{308}x o\u{332}\u{332} \u{301}lead"
            .to_owned()
            .into()
    }),
    case("emoji", emoji),
    // ZWJ family, flag, keycap and skin tone: one two-cell cluster each
    case("emoji-sequences", |_| {
        ["", &family(), FLAG_US, &keycap_1(), THUMBS_TONE, ""]
            .join("|")
            .into()
    }),
    // VS16 after a listed base makes the placed cell two wide, mid-line
    case("vs16-widen", |_| ["a", HEART, VS16, "b"].concat().into()),
    // VS16 widening the cell in the last column: the cluster moves to the
    // next line and the cell it left is cleared
    sized("vs16-margin", 10, 4, |_| {
        ["012345678", HEART, VS16, "x"].concat().into()
    }),
    // VS16 after a base with no emoji variation sequence joins and changes
    // nothing
    case("vs16-unlisted-base", |_| ["A", VS16, "b"].concat().into()),
    // VS15 never changes width: a narrow base stays one cell, a wide one two
    case("vs15-width", |_| {
        ["|", HEART, VS15, "|\u{231a}", VS15, "|"].concat().into()
    }),
    case("width-methods", width_methods),
    // DECRQM for mode 2027: 1 while set (the default), 2 once reset, 1 again
    case("mode-2027-decrqm", |_| {
        let q = [CSI, "?2027$p"].concat();
        [&q, CSI, "?2027l", &q, CSI, "?2027h", &q].concat().into()
    }),
    case("mode-2027-resets", mode_2027_resets),
    case("mode-2026", mode_2026),
    case("combining-after-move", combining_after_move),
    // clusters erased and printed again, so their table entries are freed
    // and reused
    case("cluster-reuse-erase", |_| {
        [
            "e\u{301}a\u{300}",
            CSI,
            "2K\r",
            "a\u{300}e\u{301}o\u{302}",
            CSI,
            "1;2H",
            CSI,
            "K",
            "e\u{301}",
        ]
        .concat()
        .into()
    }),
    // a prepended concatenation mark (U+0600) starts a cluster with width,
    // and the digit after it joins it, mid-line and at the start of a line
    case("prepend-width", |_| {
        "x\u{600}1|\r\n\u{600}1|".to_owned().into()
    }),
    // VS16 widening in insert mode moves the rest of the line right
    // instead of overwriting the cell after the cluster
    sized("vs16-insert-mode", 8, 2, |_| {
        ["abc", CSI, "1G", CSI, "4h", HEART, VS16, CSI, "4l"]
            .concat()
            .into()
    }),
    // insert mode on a one-column screen: a wide character has no room and
    // prints as an empty cell; a VS16 cluster stays narrow; neither shifts
    // the line
    sized("insert-wide-one-column", 1, 3, |_| {
        [CSI, "4h", "\u{4e2d}", "\r\n", HEART, VS16].concat().into()
    }),
    // a cluster of 33 codepoints keeps its first 32
    case("cluster-max", |_| {
        ["a", &"\u{300}".repeat(32), "b"].concat().into()
    }),
    sized("alt-screen", 20, 4, |_| {
        [
            "main-screen",
            CSI,
            "?1049h",
            CSI,
            "H",
            "alt-screen",
            CSI,
            "?1049l",
            "-back",
        ]
        .concat()
        .into()
    }),
    case("replies", |_| {
        ["q", CSI, ">c", CSI, "5n", &cup(3, 7), CSI, "6n"]
            .concat()
            .into()
    }),
    // DA1 on its own: whether it offers sixel (attribute 4) depends on the
    // build
    case("replies-da1", |_| ["q", CSI, "c"].concat().into()),
    case("dec-graphics", |_| {
        [ESC, "(0", "lqqqk\r\nx   x\r\nmqqqj", ESC, "(B", " ascii"]
            .concat()
            .into()
    }),
    case("osc-sequences", osc_sequences),
    // a 6x6 red square in sixel, with text on both sides
    case("dcs-sixel", |_| {
        [
            "before",
            DCS,
            "0;1;0q",
            "\"1;1;6;6",
            "#1;2;100;0;0#1~~~~~~-~~~~~~",
            ST,
            "after",
        ]
        .concat()
        .into()
    }),
    // kitty graphics: transmit-and-display a 1x1 RGBA pixel, with text
    // around it
    case("apc-kitty-graphics", |_| {
        [
            "before",
            super::APC,
            "Ga=T,f=32,s=1,v=1;/wAA/w==",
            ST,
            "after",
        ]
        .concat()
        .into()
    }),
    sized("scrollback", 20, 5, |_| {
        ((1..13).map(|n| format!("line {n}\r\n")).collect::<String>() + "last").into()
    }),
    sized("reset", 20, 4, |_| {
        [
            &sgr(&[1, 31]),
            "red",
            CSI,
            "?1049h",
            CSI,
            "3;4r",
            ESC,
            "c",
            "clean",
        ]
        .concat()
        .into()
    }),
    case("utf8-ill-formed", utf8_ill_formed),
];

const HEART: &str = "\u{2764}";
const VS15: &str = "\u{fe0e}";
const VS16: &str = "\u{fe0f}";
const ZWJ: &str = "\u{200d}";
const FLAG_US: &str = "\u{1f1fa}\u{1f1f8}";
const THUMBS_TONE: &str = "\u{1f44d}\u{1f3fd}";

fn family() -> String {
    ["\u{1f468}", ZWJ, "\u{1f469}", ZWJ, "\u{1f467}"].concat()
}

fn keycap_1() -> String {
    ["1", VS16, "\u{20e3}"].concat()
}

fn sgr_basic(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    let mut out = String::from("plain ");
    for (p, label) in [
        (1, "bold"),
        (2, "dim"),
        (3, "italic"),
        (4, "underline"),
        (5, "blink"),
        (7, "inverse"),
        (8, "conceal"),
        (9, "strike"),
    ] {
        out += &[&sgr(&[p]), label, &off, " "].concat();
    }
    for (range, label) in [(30..38, "fg"), (40..48, "bg"), (90..98, "hi")] {
        out += "\r\n";
        for c in range {
            out += &format!("{}{label}{c}{off} ", sgr(&[c]));
        }
    }
    out += &[
        "\r\n",
        &sgr(&[1, 4, 31, 42]),
        "combined",
        &sgr(&[22]),
        "nobold",
        &sgr(&[24]),
        "noul",
        &sgr(&[39, 49]),
        "default",
        &off,
    ]
    .concat();
    out.into()
}

fn sgr_extended_color(_: &Inputs) -> Made {
    let mut out = String::new();
    for n in [16, 88, 196, 231, 232, 255] {
        out += &format!(
            "{}f{n}{}b{n}{} ",
            sgr(&[38, 5, n]),
            sgr(&[48, 5, n]),
            sgr(&[0])
        );
    }
    out += &[
        "\r\n",
        &sgr(&[38, 2, 255, 128, 0]),
        "orange",
        &sgr(&[48, 2, 0, 0, 128]),
        "onnavy",
        &sgr(&[0]),
    ]
    .concat();
    out.into()
}

/// Colon sub-parameters: styled and colored underlines (SGR 4:x, 58, 59).
fn sgr_underline_extended(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        CSI,
        "4:3m",
        "curly",
        &off,
        " ",
        CSI,
        "4:2m",
        "double",
        &off,
        " ",
        CSI,
        "1;4:3m",
        "boldcurly",
        &off,
        " ",
        CSI,
        "4;58:2::255:0:0m",
        "redul",
        CSI,
        "59m",
        "nocolor",
        &off,
        " ",
        CSI,
        "4;58;5;196m",
        "ul196",
        &off,
        " ",
        CSI,
        "4:0m",
        "ulnone",
        &off,
    ]
    .concat()
    .into()
}

/// Each color form SGR 38, 48 and 58 accept, one labelled run each, with
/// a distinct color per form, so a form read wrong shows as a wrong value.
fn color_forms(selector: u32, prefix: &str) -> String {
    let forms = [
        // colon palette index, 16-255: resolved to RGB
        ("c5", ":5:196"),
        // colon palette index below 16: stays a code
        ("c5lo", ":5:9"),
        // semicolon palette index
        ("s5", ";5;33"),
        // colon RGB without a color-space slot
        ("c2", ":2:10:20:30"),
        // colon RGB, color space empty (T.416)
        ("c2e", ":2::40:50:60"),
        // colon RGB with a color space
        ("c2cs", ":2:0:70:80:90"),
        // trailing T.416 fields ignored
        ("c2t", ":2:0:100:110:120:7:8:9"),
        // semicolon RGB takes exactly 3; the 1 is bold
        ("s2", ";2;130;140;150;1"),
    ];
    forms
        .iter()
        .map(|(label, form)| format!("{CSI}{prefix}{selector}{form}m{label}{} ", sgr(&[0])))
        .collect()
}

/// 4:0-4:5, an unknown style (single), plain 4, 21 double, 24 none, and a
/// style replaced by plain 4.
fn sgr_underline_styles(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    let mut out: String = [0, 1, 2, 3, 4, 5, 9]
        .iter()
        .map(|n| format!("{CSI}4:{n}ms{n}{off} "))
        .collect();
    out += &[
        &sgr(&[4]),
        "plain",
        &off,
        " ",
        &sgr(&[21]),
        "dbl21",
        &off,
        " ",
        CSI,
        "4:3m",
        "curly",
        &sgr(&[24]),
        "off24",
        " ",
        CSI,
        "4:3m",
        "curly",
        &sgr(&[4]),
        "single",
        &off,
    ]
    .concat();
    out.into()
}

/// 9 on, 29 off, strike beside other attributes, SGR 0 clearing it.
fn sgr_strike(_: &Inputs) -> Made {
    [
        &sgr(&[9]),
        "struck",
        &sgr(&[29]),
        "clear",
        " ",
        &sgr(&[1, 9, 31]),
        "boldred",
        &sgr(&[22]),
        "red",
        &sgr(&[0]),
        "reset",
    ]
    .concat()
    .into()
}

/// 59 clears the color and keeps the style; 24 clears the style and keeps
/// the color; SGR 0 clears both.
fn sgr_underline_color_reset(_: &Inputs) -> Made {
    [
        CSI,
        "4:3;58:5:196m",
        "curlyred",
        CSI,
        "59m",
        "curly",
        " ",
        CSI,
        "4:2;58:5:2m",
        "dblgreen",
        &sgr(&[24]),
        "colouronly",
        &sgr(&[4]),
        "single",
        " ",
        CSI,
        "4:3;58;2;1;2;3m",
        "set",
        &sgr(&[0]),
        "reset",
    ]
    .concat()
    .into()
}

/// A parameter with sub-parameters SGR does not know is skipped on its
/// own; the rest of the sequence still applies.
fn sgr_subparam_unknown(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        // italic with a sub-parameter
        CSI,
        "1;3:1;4m",
        "boldul",
        &off,
        " ",
        // underline with two
        CSI,
        "4:3:1;1m",
        "bold",
        &off,
        " ",
        // an unknown color form
        CSI,
        "38:9:1;1m",
        "bold",
        &off,
        " ",
        // the colon palette form missing its index
        CSI,
        "38:5;3m",
        "italic",
        &off,
        " ",
        CSI,
        "7:1;9m",
        "strike",
        &off,
    ]
    .concat()
    .into()
}

/// In the colon forms an index or component above 255 rejects the color
/// alone; 255 itself is accepted.
fn sgr_color_colon_out_of_range(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        CSI,
        "48:2:256:300:65535;1m",
        "bold",
        &off,
        " ",
        CSI,
        "38:5:256;3m",
        "italic",
        &off,
        " ",
        CSI,
        "4;58:2::0:0:256m",
        "ul",
        &off,
        " ",
        CSI,
        "38:2:255:255:255;48:5:255m",
        "max",
        &off,
    ]
    .concat()
    .into()
}

/// A semicolon color form whose arguments carry sub-parameters is not that
/// form: nothing is consumed, and the arguments are read as SGR on their
/// own.
fn sgr_color_semicolon_subparams(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        // 5:1 skipped, 9 strike
        CSI,
        "38;5:1;9m",
        "strike",
        &off,
        " ",
        // 2 dim, 1 bold, 3:4 skipped
        CSI,
        "48;2;1;2;3:4m",
        "dimbold",
        &off,
    ]
    .concat()
    .into()
}

/// 58 consumes its own 5 and index; a later 5 is still blink.
fn sgr_58_semicolon_no_blink(_: &Inputs) -> Made {
    [
        &sgr(&[4, 58, 5, 196]),
        "ul196",
        &sgr(&[0]),
        " ",
        &sgr(&[4, 58, 2, 1, 2, 3, 5]),
        "blink",
        &sgr(&[0]),
    ]
    .concat()
    .into()
}

/// Only SGR reads sub-parameters: any other sequence carrying them is
/// ignored whole.
fn csi_subparam_non_sgr(_: &Inputs) -> Made {
    [&cup(2, 3), CSI, "5:1H", "here", CSI, "1:2C", "next"]
        .concat()
        .into()
}

/// Colors as a renderer resolves them: bold brightens a base color, dim
/// dims a default underline but not a set one, reverse video swaps the
/// default underline with the foreground but not a set one.
fn color_resolve(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        &sgr(&[1, 31]),
        "BR",
        &off,
        " ",
        &sgr(&[2, 4]),
        "DU",
        &off,
        " ",
        &sgr(&[2, 4, 58, 2, 10, 20, 30]),
        "DS",
        &off,
        " ",
        &sgr(&[7, 4]),
        "RU",
        &off,
        " ",
        &sgr(&[7, 4, 58, 5, 1]),
        "RS",
        &off,
        " ",
        &sgr(&[38, 5, 196]),
        "I",
        &off,
        " ",
        &sgr(&[38, 2, 1, 2, 3]),
        "T",
        &off,
        " ",
        &sgr(&[1, 38, 5, 1]),
        "B1",
        &off,
    ]
    .concat()
    .into()
}

fn cursor_motion(_: &Inputs) -> Made {
    [
        &cup(5, 10),
        "A",
        CSI,
        "2A",
        "B",
        CSI,
        "3B",
        "C",
        CSI,
        "4C",
        "D",
        CSI,
        "6D",
        "E",
        "\r",
        "F",
        CSI,
        "20G",
        "G",
        CSI,
        "15d",
        "H",
        ESC,
        "7",
        &cup(1, 1),
        "I",
        ESC,
        "8",
        "J",
        CSI,
        "s",
        &cup(2, 2),
        "K",
        CSI,
        "u",
        "L",
    ]
    .concat()
    .into()
}

/// Rows 1 to 6 filled with the first twenty letters.
fn fill_letters() -> String {
    (1..7).map(|r| cup(r, 1) + "abcdefghijklmnopqrst").collect()
}

fn erase(_: &Inputs) -> Made {
    [
        &fill_letters(),
        &cup(1, 10),
        CSI,
        "K",
        &cup(2, 10),
        CSI,
        "1K",
        &cup(3, 10),
        CSI,
        "2K",
        &cup(4, 5),
        CSI,
        "3X",
        &cup(5, 10),
        CSI,
        "J",
    ]
    .concat()
    .into()
}

fn insert_delete(_: &Inputs) -> Made {
    let fill: String = (1..9)
        .map(|r| format!("{}line{r:02}-abcdefghij", cup(r, 1)))
        .collect();
    [
        &fill,
        &cup(1, 5),
        CSI,
        "3@",
        &cup(2, 5),
        CSI,
        "3P",
        &cup(4, 1),
        CSI,
        "2L",
        &cup(7, 1),
        CSI,
        "1M",
    ]
    .concat()
    .into()
}

fn scroll_region(_: &Inputs) -> Made {
    let fill: String = (1..9).map(|r| format!("{}row{r}", cup(r, 1))).collect();
    [
        &fill,
        CSI,
        "3;6r",
        &cup(6, 1),
        "\n\nNEW",
        &cup(3, 1),
        ESC,
        "M",
        "TOP",
        CSI,
        "2S",
        CSI,
        "1T",
        CSI,
        "r",
        &cup(8, 1),
        "\nafter-reset",
    ]
    .concat()
    .into()
}

fn tabs(_: &Inputs) -> Made {
    [
        "a\tb\tc\r\n",
        CSI,
        "3g",
        &cup(2, 5),
        ESC,
        "H",
        &cup(2, 13),
        ESC,
        "H",
        "\r",
        "\tx\ty\tz",
    ]
    .concat()
    .into()
}

fn emoji(_: &Inputs) -> Made {
    [
        "smile \u{1f600} | vs16 ",
        HEART,
        VS16,
        " | text ",
        HEART,
        " | zwj ",
        &family(),
        " | flag ",
        FLAG_US,
        " | tone ",
        THUMBS_TONE,
        " | end",
    ]
    .concat()
    .into()
}

/// The same text under each method: mode 2027 set (unicode), then reset
/// (wcwidth), where every codepoint with width takes its own cells.
fn width_methods(_: &Inputs) -> Made {
    let text = [
        "e\u{301}|",
        THUMBS_TONE,
        "|",
        FLAG_US,
        "|",
        &family(),
        "|",
        HEART,
        VS16,
        "|",
    ]
    .concat();
    [CSI, "?2027h", &text, "\r\n", CSI, "?2027l", &text]
        .concat()
        .into()
}

/// RIS and DECSTR return mode 2027 to the configured method (unicode
/// here).
fn mode_2027_resets(_: &Inputs) -> Made {
    let q = [CSI, "?2027$p"].concat();
    [
        CSI,
        "?2027l",
        ESC,
        "c",
        &q,
        THUMBS_TONE,
        "\r\n",
        CSI,
        "?2027l",
        CSI,
        "!p",
        &q,
        THUMBS_TONE,
    ]
    .concat()
    .into()
}

/// Mode 2026, synchronized output, with the host presenting by it: DECRQM
/// replies 2, then 1 inside a frame, answered there and not at the
/// frame's end, and 2 after RM, RIS and DECSTR each end one. With the
/// host's setting off it replies 4 and SM does nothing.
fn mode_2026(_: &Inputs) -> Made {
    let q = [CSI, "?2026$p"].concat();
    let (bsu, esu) = ([CSI, "?2026h"].concat(), [CSI, "?2026l"].concat());
    [
        &q,
        &bsu,
        "frame",
        &q,
        &esu,
        &q,
        "\r\n",
        &bsu,
        "a",
        ESC,
        "c",
        &q,
        &bsu,
        "b",
        CSI,
        "!p",
        &q,
        &esu,
        &tt!("sync", on = 0),
        &q,
        &bsu,
        &q,
        &tt!("sync", on = 1),
        &q,
    ]
    .concat()
    .into()
}

/// After the cursor moves, a combining mark joins the cell before the
/// cursor; with nothing there it is dropped. SGR does not end a cluster.
fn combining_after_move(_: &Inputs) -> Made {
    [
        "ab", CSI, "D", "\u{301}", "\r\n", "x", CSI, "C", "\u{300}", "y", "\r\n", "e", CSI, "31m",
        "\u{301}", CSI, "0m", "z",
    ]
    .concat()
    .into()
}

fn osc_sequences(_: &Inputs) -> Made {
    [
        OSC,
        "0;window title",
        "\x07",
        "a",
        OSC,
        "2;other",
        ST,
        "b",
        OSC,
        "8;;https://example.com",
        ST,
        "link",
        OSC,
        "8;;",
        ST,
        "c",
    ]
    .concat()
    .into()
}

/// UTF-8 as Unicode Table 3-7 defines it, one labelled line per rule:
/// well-formed sequences at the table's bounds; each byte that starts
/// nothing, and each maximal subpart of an ill-formed sequence, one
/// U+FFFD (Unicode 3.9, Table 3-8 among them); the byte that ends one
/// read again, so E2 ESC [1m sets bold; overlongs, surrogates and values
/// past U+10FFFF ill-formed, so an overlong ESC (C0 9B, E0 80 9B, F0 80 80
/// 9B) neither inverts, asks DSR 6 nor resets; and C1 CSI through C2 9B
/// still a control. The stream is bytes, not text.
fn utf8_ill_formed(_: &Inputs) -> Made {
    let well_formed = [
        b"wf ".as_slice(),
        "\u{e9}\u{7ff}\u{800}\u{d7ff}\u{e000}\u{fffd}\u{10000}\u{10ffff}".as_bytes(),
    ]
    .concat();
    let lines: [&[u8]; 9] = [
        &well_formed,
        b"lone \x80|\xbf|\xc0|\xc1|\xf5|\xff|\x80\x80",
        b"trunc \xc3a\xe2\x82a\xf0\x9f\x98a\xe2\x82\xe2\x82\xac",
        b"3-8 \x61\xf1\x80\x80\xe1\x80\xc2\x62\x80\x63\x80\xbf\x64",
        b"keep \xe2\x1b[1mbold\x1b[0m \xe2A \xf0\x9fA \xe2\xc3\xa9",
        b"overlong \xc0\x9b[7mx \xe0\x80\x9b[6n \xf0\x80\x80\x9bc \xc1\xbf",
        b"surrogate \xed\xa0\x80 \xed\xbf\xbf \xed\x9f\xbf",
        b"above \xf4\x90\x80\x80 \xf5\x80\x80\x80 \xf4\x8f\xbf\xbf",
        b"c1 \xc2\x9b1mbold\xc2\x9b0m plain",
    ];
    Made {
        stream: Stream::Bytes(lines.join(b"\r\n".as_slice())),
        files: Default::default(),
    }
}
