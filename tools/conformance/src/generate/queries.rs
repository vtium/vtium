//! Queries and their replies, and the control strings around text: OSC,
//! a sixel DCS and a kitty graphics APC.

use super::{CSI, Case, DCS, Inputs, Made, OSC, ST, case, cup};

pub(super) const CASES: &[Case] = &[
    case("replies", |_| {
        ["q", CSI, ">c", CSI, "5n", &cup(3, 7), CSI, "6n"]
            .concat()
            .into()
    }),
    // DA1 on its own: whether it offers sixel (attribute 4) depends on the
    // build
    case("replies-da1", |_| ["q", CSI, "c"].concat().into()),
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
];

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
