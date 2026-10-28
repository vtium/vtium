//! Placing images: size and source rectangle, bands, the cursor, and the
//! store under its limit and across ED 2, the alternate screen and RIS.

use super::{BLUE4, GREEN, RED, kitty, kt, rgba};
use crate::generate::sixel::{BLUE, six_at};
use crate::generate::{CSI, Case, DSR, ESC, Inputs, Made, cell, cup, sized};

pub(in crate::generate) const CASES: &[Case] = &[
    sized("kit-place-size", 24, 6, place_size),
    // z chooses the band: below cell backgrounds, below text, above it; at
    // one z the lower id draws first
    sized("kit-bands", 12, 4, |_| {
        [
            cell(8, 16),
            kt(3, 8, 8, RED, ",z=-1073741825,C=1"),
            kt(2, 8, 8, RED, ",z=-1,C=1"),
            kt(5, 8, 8, RED, ",z=4,C=1"),
            kt(9, 8, 8, GREEN, ",C=1"),
            kt(4, 8, 8, BLUE4, ",C=1"),
        ]
        .concat()
        .into()
    }),
    // C=1 leaves the cursor; otherwise it moves past the box: three rows
    // from the last row scroll the screen twice; a box reaching the right
    // edge sends the cursor to column 0 of the next row; inside a region
    // the scroll is the region's
    sized("kit-cursor", 12, 4, |_| {
        [
            &cell(8, 16),
            &kt(1, 8, 32, RED, ",C=1"),
            DSR,
            &cup(4, 1),
            &kt(2, 8, 48, RED, ""),
            DSR,
            &cup(1, 11),
            &kt(3, 16, 16, RED, ""),
            DSR,
            CSI,
            "2;3r",
            &cup(3, 5),
            &kt(4, 8, 32, RED, ""),
            DSR,
            CSI,
            "r",
        ]
        .concat()
        .into()
    }),
    sized("kit-evict", 12, 4, evict),
    sized("kit-evict-stamp", 12, 4, evict_stamp),
    // ED 2 removes kitty placements and keeps their images, so a=p shows
    // one again; the alternate screen's images go when it is left; RIS
    // empties both stores
    sized("kit-ed2", 12, 4, |_| {
        [
            &cell(8, 16),
            &kt(1, 8, 16, RED, ""),
            CSI,
            "2J",
            &kitty("a=p,i=1,C=1", b""),
            CSI,
            "?1049h",
            &kt(2, 8, 8, RED, ""),
            CSI,
            "?1049l",
            &kitty("a=p,i=2", b""),
            &cup(3, 1),
            &kt(3, 8, 8, RED, ""),
            ESC,
            "c",
            &kitty("a=p,i=3", b""),
            &kitty("a=p,i=1", b""),
        ]
        .concat()
        .into()
    }),
];

/// c and r together letterbox or pillarbox the picture; one alone keeps
/// the aspect; x, y, w, h choose the source rectangle, cut to the image;
/// X, Y offset it in its first cell, and an offset outside the cell is
/// refused.
fn place_size(_: &Inputs) -> Made {
    [
        &cell(8, 16),
        &kitty("a=t,f=32,s=32,v=16,i=1", &rgba(32, 16, RED)),
        &kitty("a=p,i=1,c=2,r=2,C=1", b""),
        &cup(1, 4),
        &kitty("a=p,i=1,c=3,C=1", b""),
        &cup(1, 8),
        &kitty("a=p,i=1,r=2,C=1", b""),
        &cup(1, 14),
        &kitty("a=p,i=1,x=8,y=4,w=16,h=100,C=1", b""),
        &cup(4, 1),
        &kitty("a=p,i=1,X=3,Y=5,C=1", b""),
        &cup(4, 8),
        &kitty("a=p,i=1,X=8,C=1", b""),
        &kitty("a=p,i=1,x=32", b""),
        DSR,
    ]
    .concat()
    .into()
}

/// Past the limit, images go unplaced first, then placed only in
/// scrollback, then kitty images placed on the screen, oldest transmission
/// first; a sixel on the screen is never evicted, so a new image that does
/// not fit beside it is refused with ENOSPC.
fn evict(_: &Inputs) -> Made {
    let cost = 8 * 16 * 4 + 512;
    let mut out = [
        cell(8, 16),
        tt!("sb-max", n = 10),
        tt!("limit", b = 4 * cost),
        kt(1, 8, 16, RED, ""),
        "\r\n\n\n\n".to_owned(),
        six_at(2, 0, 8, 16, BLUE),
        kt(2, 8, 16, RED, ",C=1"),
        cup(1, 7),
        kt(3, 8, 16, RED, ",C=1"),
    ]
    .concat();
    for i in 4..9 {
        out += &kitty(&format!("a=t,f=32,s=8,v=16,i={i}"), &rgba(8, 16, RED));
    }
    out += &kitty("a=t,f=32,s=8,v=32,i=9", &rgba(8, 32, RED));
    out += &kitty("a=t,f=32,s=8,v=96,i=10", &rgba(8, 96, RED));
    (out + &tt!("sb-up", n = 4)).into()
}

/// Kitty images on the screen are evicted oldest transmission first: an
/// old image placed again with a=p keeps its old stamp and goes before a
/// newer one placed earlier. The limit holds two images and image 1's
/// second placement.
fn evict_stamp(_: &Inputs) -> Made {
    let cost = 8 * 16 * 4 + 512;
    [
        cell(8, 16),
        tt!("limit", b = 2 * cost + 256),
        kt(1, 8, 16, RED, ",C=1"),
        cup(1, 4),
        kt(2, 8, 16, RED, ",C=1"),
        cup(3, 1),
        kitty("a=p,i=1,C=1", b""),
        cup(1, 8),
        kt(3, 8, 16, RED, ",C=1"),
        kitty("a=p,i=1", b""),
        kitty("a=p,i=2", b""),
    ]
    .concat()
    .into()
}
