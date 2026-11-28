//! Where a preview goes: the `Rect` a tab gives inside the viewer the
//! frame hands it, as a [`Place`]. A picture is shown from the place's top
//! left and never reaches past it, and [`Covered`] names the cells under
//! it, which the view marks so that no frame writes over the picture. The
//! console's own preview lines are laid out inside the same place
//! ([`super::console::placed`]).
//!
//! A place holds the Rect's four sides rather than ratatui's `Rect`
//! itself, because this crate draws nothing: the tab that draws makes the
//! place from its Rect with [`Place::new`].
//!
//! References:
//! - ADR-1206 D4, "What the frame replaces in layout.rs": a picture is placed inside the viewer's Rect.
//! - ADR-1206 D4, "ratatui draws what changed": the cells under a kitty graphics picture are marked to skip.

use super::ASSUMED_CELL;

/// The place a tab gives its preview: a Rect of the terminal's cells,
/// counted from 0 at the screen's top left, as ratatui counts them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Place {
    /// The place of the Rect at `x`, `y` that is `width` by `height`
    /// cells, as ratatui's `Rect::new` takes them.
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Place {
        Place {
            x,
            y,
            width,
            height,
        }
    }

    /// The cell a picture is shown from, row and column counted from 1,
    /// as the cursor position that the picture is written at counts them.
    pub fn at(&self) -> (usize, usize) {
        (
            usize::from(self.y).saturating_add(1),
            usize::from(self.x).saturating_add(1),
        )
    }

    /// The cells the place holds, columns by rows.
    pub fn area(&self) -> (usize, usize) {
        (usize::from(self.width), usize::from(self.height))
    }

    /// The most pixels a picture takes in the place, at this terminal's
    /// character cell `cell_px`, else the cell vtium-config assumes, and
    /// at most [`crate::specimen::PICTURE_MAX`].
    pub fn max(&self, cell_px: Option<(u32, u32)>) -> (u32, u32) {
        let (cw, ch) = cell_px.unwrap_or(ASSUMED_CELL);
        let (cols, rows) = self.area();
        (
            u32::try_from(cols)
                .unwrap_or(0)
                .saturating_mul(cw)
                .min(crate::specimen::PICTURE_MAX.0),
            u32::try_from(rows)
                .unwrap_or(0)
                .saturating_mul(ch)
                .min(crate::specimen::PICTURE_MAX.1),
        )
    }

    /// The cells a picture of `pixels` covers from the place's top left,
    /// at this terminal's character cell `cell_px`, else the cell
    /// vtium-config assumes, as the picture was sized by, and at most the
    /// place.
    pub fn covered(&self, (width, height): (u32, u32), cell_px: Option<(u32, u32)>) -> Covered {
        let (cw, ch) = cell_px.unwrap_or(ASSUMED_CELL);
        let cells = |pixels: u32, cell: u32| {
            usize::try_from(pixels.div_ceil(cell.max(1))).unwrap_or(usize::MAX)
        };
        let (cols, rows) = self.area();
        Covered {
            row: usize::from(self.y),
            col: usize::from(self.x),
            cols: cells(width, cw).min(cols),
            rows: cells(height, ch).min(rows),
        }
    }
}

/// The cells under a picture on the screen: the picture's first row and
/// column, counted from 0, and how many columns and rows it takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Covered {
    pub row: usize,
    pub col: usize,
    pub cols: usize,
    pub rows: usize,
}
