//! Images on the screen: each case judges one row of the screen-event
//! table for both kinds, a sixel and a kitty image, each 24x32 pixels, 3x2
//! cells of 8x16. The sixels are real sixel strings, shown with the cursor
//! saved and restored around them, so they land where a test placement
//! would and leave the cursor where it was. The kitty images, and the one
//! sixel that reaches past the screen's bottom, which no sixel string can
//! place, come from the replay's test commands.

use super::sixel::{BLUE, Pic, six, six_at};
use super::{CSI, Case, ESC, Inputs, Made, cell, cup, sized};

/// The cell size, a scrollback of `sb` lines when not 0, and image 2, a
/// green 24x32 kitty image.
fn setup(sb: u32) -> String {
    let mut out = cell(8, 16);
    if sb > 0 {
        out += &tt!("sb-max", n = sb);
    }
    out + &tt!("image", n = 2, w = 24, h = 32, kitty = 1, fill = "ff00ff00")
}

/// A test placement of image `img` at `col`, `row`.
fn put(img: u32, col: u32, row: u32) -> String {
    tt!("place", img = img, col = col, row = row)
}

const CYAN: Pic = Pic {
    rgb: [0, 100, 100],
    ..BLUE
};

/// A sixel at columns 2-4 and a kitty image at 7-9, rows 1-2, then `op`
/// with the cursor at row 1, column 3.
fn erase_case(op: &str) -> Made {
    [&setup(0), &six(2, 1), &put(2, 7, 1), &cup(2, 4), CSI, op]
        .concat()
        .into()
}

/// Images 3 and 4, kitty images 8x32.
fn region_images() -> String {
    [
        setup(0),
        tt!("image", n = 3, w = 8, h = 32, kitty = 1, fill = "ffffffff"),
        tt!("image", n = 4, w = 8, h = 32, kitty = 1, fill = "ff00ffff"),
    ]
    .concat()
}

pub(super) const CASES: &[Case] = &[
    // text written over a sixel removes the cells it covers; a kitty
    // placement is untouched
    sized("img-print", 12, 4, |_| {
        [
            &setup(0),
            &six(1, 1),
            &put(2, 5, 1),
            &cup(2, 3),
            "xy",
            &cup(2, 7),
            "z",
        ]
        .concat()
        .into()
    }),
    // an opaque sixel removes the parts of older sixels under it; a
    // transparent one leaves them; neither touches a kitty placement
    sized("img-sixel-over", 12, 4, |_| {
        [
            setup(0),
            six(1, 1),
            put(2, 6, 1),
            six_at(2, 2, 16, 16, CYAN),
            six_at(
                1,
                1,
                8,
                16,
                Pic {
                    p2: 1,
                    holes: &[(0, 0), (7, 15)],
                    ..BLUE
                },
            ),
            six_at(
                6,
                1,
                16,
                16,
                Pic {
                    rgb: [100, 100, 0],
                    ..BLUE
                },
            ),
        ]
        .concat()
        .into()
    }),
    sized("img-el0", 12, 4, |_| erase_case("K")),
    sized("img-el1", 12, 4, |_| erase_case("1K")),
    sized("img-el2", 12, 4, |_| erase_case("2K")),
    sized("img-ech", 12, 4, |_| erase_case("X")),
    sized("img-ed0", 12, 4, |_| erase_case("J")),
    sized("img-ed1", 12, 4, |_| erase_case("1J")),
    // the parser sets no protected cells, so selective erase erases what
    // ED and EL do
    sized("img-decsed", 12, 4, |_| erase_case("?J")),
    sized("img-decsel", 12, 4, |_| erase_case("?2K")),
    sized("img-ich", 12, 4, |_| erase_case("@")),
    sized("img-dch", 12, 4, |_| erase_case("P")),
    sized("img-ed2", 12, 4, ed2),
    // ED 3 drops the scrollback, and the placements anchored on it, even
    // those reaching onto the screen
    sized("img-ed3", 12, 4, |_| {
        [
            &setup(10),
            &tt!("image", n = 3, w = 8, h = 16, kitty = 1, fill = "ffffffff"),
            &six(0, 0),
            &put(2, 4, 0),
            &cup(4, 1),
            "\n",
            &put(3, 5, 1),
            &six_at(0, 2, 8, 16, CYAN),
            CSI,
            "3J",
        ]
        .concat()
        .into()
    }),
    // LF at the bottom, IND and SU scroll the whole screen: placements move
    // with their lines into scrollback, whole, and the view brings them back
    sized("img-scroll-up", 12, 4, |_| {
        [
            &setup(10),
            &six(0, 1),
            &put(2, 4, 1),
            &cup(4, 1),
            "\n",
            ESC,
            "D",
            CSI,
            "2S",
            &tt!("sb-up", n = 3),
        ]
        .concat()
        .into()
    }),
    // placements reaching past the screen's bottom move up with their
    // lines: the bottom of the screen is no region edge
    sized("img-scroll-up-bottom", 20, 5, |_| {
        [
            &cell(8, 16),
            &tt!("image", n = 1, w = 24, h = 64, kitty = 1, fill = "ff00ff00"),
            &tt!("image", n = 2, w = 24, h = 64, fill = "ff0000ff"),
            &put(1, 0, 3),
            &put(2, 4, 3),
            &cup(5, 1),
            "\n\n\n",
        ]
        .concat()
        .into()
    }),
    // SD and RI at the top: placements move down with their lines; the part
    // pushed past the bottom is clipped
    sized("img-scroll-down", 12, 5, |_| {
        [
            &setup(0),
            &six(0, 1),
            &put(2, 4, 1),
            CSI,
            "T",
            &cup(1, 1),
            ESC,
            "M",
            CSI,
            "T",
        ]
        .concat()
        .into()
    }),
    // SD with placements reaching from scrollback onto the screen: the
    // sixel splits at the screen's top, its screen part moving down; the
    // kitty placement, not wholly on the screen, stays
    sized("img-scroll-down-sb", 12, 4, |_| {
        [
            &setup(10),
            &six(0, 0),
            &put(2, 4, 0),
            &cup(4, 1),
            "\n",
            CSI,
            "T",
            &tt!("sb-up", n = 1),
        ]
        .concat()
        .into()
    }),
    // two lines scroll in the region of rows 2-5; a sixel splits at the
    // region's edges, the part inside moving and clipped where it leaves; a
    // kitty placement wholly inside moves and is clipped, one across an
    // edge stays
    sized("img-region", 16, 8, |_| {
        [
            &region_images(),
            &six(0, 1),
            &six(3, 3),
            &six(6, 5),
            &put(2, 9, 3),
            &put(3, 13, 5),
            &put(4, 14, 1),
            CSI,
            "3;6r",
            &cup(6, 1),
            "\n\n",
        ]
        .concat()
        .into()
    }),
    // IL scrolls rows 3-5 down from the cursor's line
    sized("img-il", 14, 6, |_| {
        [
            &region_images(),
            &six(0, 2),
            &six(3, 4),
            &put(2, 6, 4),
            &tt!("image", n = 5, w = 24, h = 32, kitty = 1, fill = "ffff00ff"),
            &put(5, 9, 2),
            &cup(4, 1),
            CSI,
            "L",
        ]
        .concat()
        .into()
    }),
    // DL scrolls rows 3-5 up from the cursor's line
    sized("img-dl", 14, 6, |_| {
        [
            &region_images(),
            &six(0, 2),
            &six(3, 4),
            &put(2, 6, 4),
            &tt!("image", n = 5, w = 24, h = 32, kitty = 1, fill = "ffff00ff"),
            &put(5, 9, 2),
            &put(3, 12, 3),
            &cup(4, 1),
            CSI,
            "M",
        ]
        .concat()
        .into()
    }),
    // scrollback past its size discards its oldest line: a placement on it
    // loses that row and is anchored on the next line
    sized("img-discard-sb", 12, 3, |_| {
        [
            &cell(8, 16),
            &tt!("sb-max", n = 2),
            &tt!("image", n = 2, w = 24, h = 48, kitty = 1, fill = "ff00ff00"),
            &six_at(0, 0, 24, 48, BLUE),
            &put(2, 4, 0),
            &cup(3, 1),
            "\n\n\n\n",
            &tt!("sb-up", n = 2),
        ]
        .concat()
        .into()
    }),
    // the alternate screen has no scrollback: its top line is discarded
    sized("img-discard-alt", 12, 4, |_| {
        [
            CSI,
            "?1049h",
            &setup(0),
            &six(0, 0),
            &put(2, 4, 0),
            &cup(4, 1),
            "\n",
        ]
        .concat()
        .into()
    }),
    // lines the height change brings back from scrollback carry their
    // placements; a narrower screen keeps them, drawn clipped
    sized("img-resize", 12, 4, |_| {
        [
            &setup(10),
            &six(0, 0),
            &put(2, 6, 0),
            &tt!("image", n = 3, w = 24, h = 32, kitty = 1, fill = "ffffffff"),
            &put(3, 9, 2),
            &cup(4, 1),
            "\n\n",
            &tt!("resize", c = 10, r = 6),
        ]
        .concat()
        .into()
    }),
    // lines the height change sends to scrollback carry theirs
    sized("img-resize-shrink", 12, 6, |_| {
        [
            setup(10),
            six(0, 3),
            put(2, 4, 3),
            cup(6, 1),
            tt!("resize", c = 12, r = 3),
        ]
        .concat()
        .into()
    }),
    // the alternate screen's store is emptied on entry and on leaving, and
    // the main screen's placements show again
    sized("img-alt-1049", 12, 4, |_| {
        [
            &setup(0),
            &six(0, 0),
            &put(2, 4, 0),
            CSI,
            "?1049h",
            &setup(0),
            &six(0, 1),
            &put(2, 4, 2),
            CSI,
            "?1049l",
        ]
        .concat()
        .into()
    }),
    // 47 keeps the alternate screen's text when left, but not its images:
    // entered again, it shows only the sixel placed then
    sized("img-alt-47", 12, 4, |_| {
        [
            &setup(0),
            &six(0, 0),
            CSI,
            "?47h",
            "alt",
            &setup(0),
            &put(2, 4, 1),
            CSI,
            "?47l",
            CSI,
            "?47h",
            &setup(0),
            &six(0, 2),
        ]
        .concat()
        .into()
    }),
    sized("img-alt-1047", 12, 4, |_| {
        [
            &setup(0),
            &put(2, 4, 0),
            CSI,
            "?1047h",
            &setup(0),
            &six(0, 1),
            CSI,
            "?1047l",
        ]
        .concat()
        .into()
    }),
    // on the alternate screen only its own placements show
    sized("img-alt-shown", 12, 4, |_| {
        [
            &setup(0),
            &six(0, 0),
            CSI,
            "?1049h",
            &setup(0),
            &put(2, 4, 1),
        ]
        .concat()
        .into()
    }),
    // RIS empties both stores; a sixel placed after it shows alone
    sized("img-ris", 12, 4, |_| {
        [
            &setup(10),
            &six(0, 0),
            &cup(4, 1),
            "\n",
            &put(2, 4, 2),
            CSI,
            "?1049h",
            &setup(0),
            &put(2, 4, 1),
            ESC,
            "c",
            &setup(0),
            &six(0, 1),
        ]
        .concat()
        .into()
    }),
    // a soft reset leaves text, so it leaves pictures
    sized("img-decstr", 12, 4, |_| {
        [&setup(0), "text", &six(0, 1), &put(2, 4, 1), CSI, "!p"]
            .concat()
            .into()
    }),
];

/// ED 2 removes every placement on the screen's lines: the kitty
/// placements whole, the sixels where they lie on the screen. A kitty
/// image wholly in scrollback stays; the view is scrolled back to show it.
fn ed2(_: &Inputs) -> Made {
    [
        &setup(10),
        &tt!("image", n = 3, w = 8, h = 16, kitty = 1, fill = "ffffffff"),
        &put(3, 0, 0),
        &cup(4, 1),
        "\n",
        &six(2, 0),
        &put(2, 7, 0),
        &six_at(0, 2, 8, 16, CYAN),
        &put(3, 0, 3),
        "\n",
        CSI,
        "2J",
        &tt!("sb-up", n = 2),
    ]
    .concat()
    .into()
}
