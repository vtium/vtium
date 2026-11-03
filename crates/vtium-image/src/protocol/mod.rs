//! The protocol handlers vtium-term's router sends strings to (ADR-1100,
//! D1, D6): [`sixel`] takes DCS `q` strings, [`kitty`] takes APC `G`
//! strings, and [`iterm2`] takes iTerm2's OSC 1337 picture strings
//! (ADR-1103).

pub mod iterm2;
pub mod kitty;
pub mod sixel;

use vtium_parse::Chunk;

use crate::handler::Context;
use crate::image::{ImageKey, Place};

/// The work reading `data` costs: one for each character it holds (D5).
pub(crate) fn chunk_work(data: Chunk<'_>) -> u64 {
    match data {
        Chunk::Bytes(bytes) => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        Chunk::Char(_) => 1,
    }
}

/// Places `image` at the cursor as a sixel is placed with sixel scrolling
/// on, and moves the cursor after it (ADR-1100, D3). `drawn` is the size
/// in pixels the picture draws at, or `None` for its own size; `tall` is the
/// rows its box covers.
///
/// The picture's top left is the cursor's cell, a pending wrap counting as
/// the last column. When the picture would pass the bottom margin, the
/// scroll region scrolls up first, as far as the region allows, so a
/// picture that fits the region moves whole; then it is placed, and the
/// cursor goes down a row fewer than the picture's rows, as that many line
/// feeds do, so an index covers what the region could not. The cursor ends
/// on the row holding the picture's last pixel row: at the picture's left
/// column, or with `cursor_right` in the column after its right edge, at
/// most the last column. A placement the store refuses drops the image.
/// Returns whether the picture was placed.
pub(crate) fn place_scrolling(
    ctx: &mut dyn Context,
    image: ImageKey,
    drawn: Option<(u32, u32)>,
    tall: usize,
    cursor_right: bool,
) -> bool {
    let (cols, _) = ctx.grid();
    let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let (cx, mut y) = ctx.cursor();
    // a pending wrap leaves the cursor past the last column
    let x = cx.min(cols.saturating_sub(1));
    let down = tall.saturating_sub(1);
    // the region scrolls first, as far as it can, so a picture that fits
    // it moves whole; index covers the rest
    let m = ctx.margins();
    if (m.top..=m.bottom).contains(&y) {
        let room = m.bottom.saturating_sub(y);
        if down > room {
            let s = down.saturating_sub(room).min(y.saturating_sub(m.top));
            ctx.scroll_up(s);
            y = y.saturating_sub(s);
        }
    }
    let at = Place {
        col: to_u32(x),
        row: to_u32(y),
        ..Place::default()
    };
    let placed = ctx.edit(|e| {
        let key = match drawn {
            Some(size) => e.place_drawn(image, at, size),
            None => e.place(image, at),
        };
        key.map(|k| e.extent(k))
    });
    let Some(Ok(Some((box_cols, _)))) = placed else {
        ctx.drop_image(image);
        return false;
    };
    ctx.move_cursor(x, y);
    ctx.index(down);
    let (_, row) = ctx.cursor();
    let box_cols = usize::try_from(box_cols).unwrap_or(usize::MAX);
    let col = if !cursor_right {
        x
    } else if box_cols < cols.saturating_sub(x) {
        x.saturating_add(box_cols)
    } else {
        cols.saturating_sub(1)
    };
    ctx.move_cursor(col, row);
    true
}
