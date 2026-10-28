//! Sixel: the decoder, placement and the cursor, the modes and the
//! replies, through real sixel strings. Each case asks for the cursor
//! after each picture, so its replies record where the cursor went.

use super::{CSI, Case, DCS, DSR, ESC, Inputs, Made, ST, cell, cup, sized};

/// A picture of one color: register 1 in RGB percent, the background
/// select P2 (1 leaves unset pixels transparent), the aspect P1, and the
/// pixels, `(x, y)`, left unset.
#[derive(Clone, Copy)]
pub(super) struct Pic {
    pub rgb: [u8; 3],
    pub p1: u8,
    pub p2: u8,
    pub holes: &'static [(u32, u32)],
}

/// A blue picture with every pixel set.
pub(super) const BLUE: Pic = Pic {
    rgb: [0, 0, 100],
    p1: 0,
    p2: 0,
    holes: &[],
};

/// A sixel string for a `w` x `h` picture, drawn band by band.
pub(super) fn sixel(w: u32, h: u32, pic: Pic) -> String {
    let [r, g, b] = pic.rgb;
    let mut out = format!(
        "{DCS}{};{};0q\"1;1;{w};{h}#1;2;{r};{g};{b}#1",
        pic.p1, pic.p2
    );
    for y in (0..h).step_by(6) {
        let rows = (h - y).min(6);
        for x in 0..w {
            let mut bits = (1_u32 << rows) - 1;
            for &(hx, hy) in pic.holes {
                if hx == x && (y..y + rows).contains(&hy) {
                    bits &= !(1 << (hy - y));
                }
            }
            out.push(char::from_u32(63 + bits).unwrap_or('?'));
        }
        out.push('-');
    }
    out + ST
}

/// A `w` x `h` sixel at `col`, `row` from 0, with the cursor kept, as a
/// test placement.
pub(super) fn six_at(col: u32, row: u32, w: u32, h: u32, pic: Pic) -> String {
    [
        ESC,
        "7",
        &cup(row + 1, col + 1),
        &sixel(w, h, pic),
        ESC,
        "8",
    ]
    .concat()
}

/// A blue 24x32 sixel at `col`, `row`, with the cursor kept.
pub(super) fn six(col: u32, row: u32) -> String {
    six_at(col, row, 24, 32, BLUE)
}

const RED: Pic = Pic {
    rgb: [100, 0, 0],
    ..BLUE
};

pub(super) const CASES: &[Case] = &[
    sized("six-cursor-xterm", 40, 12, cursor_xterm),
    // the cursor ends on the row of the picture's last pixel row, at its
    // left column: 40 pixels from row 1 reach row 3, and one pixel stays on
    // its row; a new line then starts under the picture
    sized("six-cursor", 12, 6, |_| {
        [
            &cell(8, 16),
            &cup(2, 3),
            &sixel(16, 40, BLUE),
            DSR,
            "\r\nab",
            &cup(1, 9),
            &sixel(8, 1, BLUE),
            DSR,
        ]
        .concat()
        .into()
    }),
    // mode 8452: the cursor ends right of the picture on its last row; at
    // the right edge, in the last column; reset, it ends below again
    sized("six-cursor-8452", 12, 4, |_| {
        [
            &cell(8, 16),
            CSI,
            "?8452h",
            &cup(1, 1),
            &sixel(16, 20, BLUE),
            DSR,
            "x",
            &cup(4, 9),
            &sixel(40, 16, BLUE),
            DSR,
            CSI,
            "?8452l",
            &cup(1, 6),
            &sixel(8, 16, BLUE),
            DSR,
        ]
        .concat()
        .into()
    }),
    // a picture passing the bottom scrolls the screen as index would, its
    // top going into scrollback with its line; the view shows it back
    sized("six-scroll-bottom", 12, 4, |_| {
        [
            &cell(8, 16),
            &tt!("sb-max", n = 10),
            "line0\r\nline1",
            &cup(3, 2),
            &sixel(16, 48, BLUE),
            DSR,
            &tt!("sb-up", n = 1),
        ]
        .concat()
        .into()
    }),
    sized("six-scroll-region", 12, 6, scroll_region),
    // DECSDM set (mode 80, xterm's sense): the picture at the top left, cut
    // to the screen, the cursor unmoved; reset, at the cursor again
    sized("six-decsdm", 12, 4, |_| {
        [
            &cell(8, 16),
            "text",
            CSI,
            "?80h",
            &cup(3, 5),
            &sixel(120, 80, BLUE),
            DSR,
            CSI,
            "?80l",
            &cup(3, 5),
            &sixel(8, 16, RED),
            DSR,
        ]
        .concat()
        .into()
    }),
    // RGB and HLS registers (HLS hue 0 is blue), repeats, graphics carriage
    // return and next line, color taken when drawn, register 0 behind
    // unset pixels (P2 0)
    sized("six-colours", 12, 4, |_| {
        [
            &cell(8, 16),
            DCS,
            "0;0;0q",
            "#1;2;100;0;0#2;1;0;50;100#3;1;120;50;100#4;1;240;50;100",
            "#1!8~#2!8~$#3!4N#1;2;0;0;0!4N-",
            "#4~?~?~?~?",
            ST,
            DSR,
        ]
        .concat()
        .into()
    }),
    // raster attributes size the picture past its data: P2 0 fills the
    // rest with register 0, P2 1 leaves it transparent; raster attributes
    // after data are ignored; Pan;Pad and P1 do not scale a dot
    sized("six-raster", 12, 4, |_| {
        [
            &cell(8, 16),
            DCS,
            "0;0;0q\"1;1;16;20#1~~~~",
            ST,
            DSR,
            &cup(1, 4),
            DCS,
            "0;1;0q\"1;1;16;20#1~~~~\"1;1;40;40",
            ST,
            DSR,
            &cup(1, 7),
            DCS,
            "2;1;0q\"2;1;8;12#1!8~-!8~",
            ST,
            DSR,
        ]
        .concat()
        .into()
    }),
    sized("six-registers", 12, 4, registers),
    // RIS empties the shared registers and sets mode 1070 again: after it,
    // both pictures are black, the second shared again
    sized("six-registers-ris", 12, 4, |_| {
        [
            &cell(8, 16),
            CSI,
            "?1070l",
            DCS,
            "q#20;2;100;0;0#20!8~",
            ST,
            ESC,
            "c",
            &cell(8, 16),
            DCS,
            "q#20!8~",
            ST,
            CSI,
            "?1070l",
            &cup(1, 2),
            DCS,
            "q#20!8~",
            ST,
        ]
        .concat()
        .into()
    }),
    // max-graphic-size bounds each dimension; pixels past it are dropped
    sized("six-max-size", 12, 4, |_| {
        [
            &cell(8, 16),
            &tt!("graphic-max", w = 16, h = 10),
            &sixel(40, 40, BLUE),
            DSR,
            CSI,
            "?2;1S",
        ]
        .concat()
        .into()
    }),
    sized("six-cancel", 12, 4, cancel),
    // text written over a sixel removes the cells it covers; a transparent
    // sixel over text leaves the text, which shows through it
    sized("six-text", 12, 4, |_| {
        [
            &cell(8, 16),
            &sixel(32, 16, BLUE),
            &cup(1, 2),
            "ab",
            &cup(3, 1),
            "under",
            &cup(3, 1),
            &sixel(
                24,
                16,
                Pic {
                    p2: 1,
                    holes: &[(0, 0)],
                    ..BLUE
                },
            ),
            DSR,
        ]
        .concat()
        .into()
    }),
    // a picture scrolls into scrollback with its lines, and back into view
    // with them
    sized("six-scrollback", 12, 4, |_| {
        [
            &cell(8, 16),
            &tt!("sb-max", n = 10),
            "above",
            &cup(2, 1),
            &sixel(16, 32, BLUE),
            "\r\nbelow\n\n\n\n",
            &tt!("sb-up", n = 4),
        ]
        .concat()
        .into()
    }),
    // clear (ED 2, then ED 3) removes a picture on the screen and one in
    // scrollback
    sized("six-clear", 12, 4, |_| {
        [
            &cell(8, 16),
            &tt!("sb-max", n = 10),
            &sixel(16, 32, BLUE),
            "\r\n\n\n\n",
            &sixel(16, 16, BLUE),
            CSI,
            "H",
            CSI,
            "2J",
            CSI,
            "3J",
            DSR,
            &tt!("sb-up", n = 4),
        ]
        .concat()
        .into()
    }),
    sized("six-replies", 12, 4, replies),
    // with no budget, sixel is off: the string is ignored, DA1 has no
    // attribute 4 and XTSMGRAPHICS reports no graphics
    sized("six-off", 12, 4, |_| {
        [
            &cell(8, 16),
            &tt!("limit", b = 0),
            &sixel(16, 16, BLUE),
            DSR,
            CSI,
            "c",
            CSI,
            "?1;1S",
            CSI,
            "?2;1S",
        ]
        .concat()
        .into()
    }),
    sized("six-img2sixel-palette16", 12, 4, |i| {
        [
            &cell(8, 16),
            "img2sixel:\r\n",
            &img2sixel(i, "palette16"),
            DSR,
        ]
        .concat()
        .into()
    }),
    sized("six-img2sixel-gradient", 12, 6, |i| {
        [&cell(8, 16), &cup(2, 2), &img2sixel(i, "gradient"), DSR]
            .concat()
            .into()
    }),
    sized("six-img2sixel-highcolor", 12, 4, |i| {
        [&cell(8, 16), &img2sixel(i, "highcolor"), DSR]
            .concat()
            .into()
    }),
];

/// libsixel's `img2sixel` output for the picture `name`.
fn img2sixel(inputs: &Inputs, name: &str) -> String {
    inputs.latin1(&format!("sixel/{name}.six"))
}

/// The byte stream of a probe run in xterm 411 (-ti vt340, 6x13 cells,
/// 40x12) as a black box; its cursor reports were 4;3 4;3 2;3 2;3, then
/// 4;5 with mode 8452, 3;1 at the right edge, 11;1 at the bottom, and 3;4
/// with DECSDM set. vtium answers 12;1 at the bottom and 2;40 at the right
/// edge, where xterm differs.
fn cursor_xterm(_: &Inputs) -> Made {
    let clear = [CSI, "2J"].concat();
    let steps = [
        ([clear.as_str(), &cup(2, 3)].concat(), 12, 32),
        ([clear.as_str(), &cup(2, 3)].concat(), 12, 39),
        ([clear.as_str(), &cup(2, 3)].concat(), 12, 1),
        ([clear.as_str(), &cup(2, 3)].concat(), 12, 13),
        ([&clear, CSI, "?8452h", &cup(2, 3)].concat(), 12, 27),
        ([&clear, CSI, "?8452h", &cup(2, 39)].concat(), 24, 13),
        ([CSI, "?8452l", &clear, &cup(12, 1)].concat(), 12, 39),
        ([&clear, CSI, "?80h", &cup(3, 4)].concat(), 12, 39),
    ];
    let mut out = cell(6, 13);
    for (pre, w, h) in steps {
        out += &[&pre, &sixel(w, h, BLUE), DSR, CSI, "?80l", CSI, "?8452l"].concat();
    }
    out.into()
}

/// In the region of rows 2 to 5, a picture that fits is scrolled into the
/// region whole before it is placed. One taller than the region is
/// scrolled to its top, then by index: its top row leaves the region and
/// is cut, the part below the region stays, and the first picture scrolls
/// out of the region.
fn scroll_region(_: &Inputs) -> Made {
    [
        &cell(8, 16),
        "top",
        &cup(6, 1),
        "bottom",
        CSI,
        "2;5r",
        &cup(4, 1),
        &sixel(8, 48, BLUE),
        DSR,
        &cup(5, 4),
        &sixel(8, 80, RED),
        DSR,
    ]
    .concat()
    .into()
}

/// Mode 1070 set (the default): each picture starts from the default
/// registers, so the second is black; reset, definitions carry over, so
/// the fourth is red like the first and third.
fn registers(_: &Inputs) -> Made {
    let red = "#20;2;100;0;0";
    [
        &cell(8, 16),
        DCS,
        "q",
        red,
        "#20!8~",
        ST,
        &cup(1, 2),
        DCS,
        "q#20!8~",
        ST,
        CSI,
        "?1070l",
        &cup(1, 3),
        DCS,
        "q",
        red,
        "#20!8~",
        ST,
        &cup(1, 4),
        DCS,
        "q#20!8~",
        ST,
    ]
    .concat()
    .into()
}

/// CAN and SUB discard a picture; ESC ending the string shows it, and what
/// follows runs; an empty picture, and a DCS with an intermediate
/// (DECRQSS), make nothing.
fn cancel(_: &Inputs) -> Made {
    let whole = sixel(16, 16, BLUE);
    let pic = whole.strip_suffix(ST).unwrap_or(&whole);
    [
        &cell(8, 16),
        pic,
        "\x18",
        DSR,
        &cup(1, 4),
        pic,
        "\x1a",
        DSR,
        &cup(1, 7),
        pic,
        ESC,
        "[2;1H",
        DSR,
        DCS,
        "q",
        ST,
        DCS,
        "q#1",
        ST,
        DCS,
        "$q\"p",
        ST,
        DSR,
    ]
    .concat()
    .into()
}

/// DA1 with attribute 4; XTSMGRAPHICS for registers (read, set within
/// range, set out of range, reset, read the maximum, a bad action),
/// geometry (read, read the maximum, set refused) and ReGIS (none);
/// XTWINOPS 14, 16 and 18.
fn replies(_: &Inputs) -> Made {
    let mut out = cell(8, 16);
    for seq in [
        "c",
        "?1;1S",
        "?1;3;256S",
        "?1;3;1S",
        "?1;2S",
        "?1;4S",
        "?1;9S",
        "?2;1S",
        "?2;4S",
        "?2;3;100;100S",
        "?3;1S",
        "14t",
        "16t",
        "18t",
    ] {
        out += CSI;
        out += seq;
    }
    out.into()
}
