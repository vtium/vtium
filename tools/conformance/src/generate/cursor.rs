//! Cursor motion, erasing, inserting and deleting, scroll regions, origin
//! mode, autowrap, tabs and the scrollback.

use super::{CSI, Case, ESC, Inputs, Made, case, cup, sized};

pub(super) const CASES: &[Case] = &[
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
    sized("scrollback", 20, 5, |_| {
        ((1..13).map(|n| format!("line {n}\r\n")).collect::<String>() + "last").into()
    }),
];

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
