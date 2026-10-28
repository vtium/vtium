//! Printed text: wide characters, combining marks and clusters, emoji
//! sequences and variation selectors, the DEC graphics set, and UTF-8 as
//! Unicode defines it.

use super::{CSI, Case, ESC, Inputs, Made, Stream, case, sized};

pub(super) const CASES: &[Case] = &[
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
    case("dec-graphics", |_| {
        [ESC, "(0", "lqqqk\r\nx   x\r\nmqqqj", ESC, "(B", " ascii"]
            .concat()
            .into()
    }),
    case("utf8-ill-formed", utf8_ill_formed),
];

pub(super) const HEART: &str = "\u{2764}";

const VS15: &str = "\u{fe0e}";

pub(super) const VS16: &str = "\u{fe0f}";

const ZWJ: &str = "\u{200d}";

pub(super) const FLAG_US: &str = "\u{1f1fa}\u{1f1f8}";

pub(super) const THUMBS_TONE: &str = "\u{1f44d}\u{1f3fd}";

pub(super) fn family() -> String {
    ["\u{1f468}", ZWJ, "\u{1f469}", ZWJ, "\u{1f467}"].concat()
}

fn keycap_1() -> String {
    ["1", VS16, "\u{20e3}"].concat()
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
