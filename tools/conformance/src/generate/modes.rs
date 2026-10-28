//! Modes: the width method (2027), synchronized output (2026), the
//! alternate screen, and resets.

use super::text::{FLAG_US, HEART, THUMBS_TONE, VS16, family};
use super::{CSI, Case, ESC, Inputs, Made, case, sgr, sized};

pub(super) const CASES: &[Case] = &[
    case("width-methods", width_methods),
    // DECRQM for mode 2027: 1 while set (the default), 2 once reset, 1 again
    case("mode-2027-decrqm", |_| {
        let q = [CSI, "?2027$p"].concat();
        [&q, CSI, "?2027l", &q, CSI, "?2027h", &q].concat().into()
    }),
    case("mode-2027-resets", mode_2027_resets),
    case("mode-2026", mode_2026),
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
];

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
