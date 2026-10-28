//! [`SetFace`]: one cell size of a [`Set`], drawn as a [`Face`].

use std::sync::Arc;

use super::read::Set;
use crate::bounds::CELL_SIDE_MAX;
use crate::face::{Cell, Decorations, Face, Style};
use crate::glyph::Glyph;
use crate::policy::{RangeCoverage, SpritePolicy};
use crate::sprite::Range;

/// A set drawn at one of its cell sizes, scaled by a whole factor.
#[derive(Clone, Debug)]
pub struct SetFace {
    set: Arc<Set>,
    cell: Cell,
    scale: u32,
}

/// The cell `height` asks for among `cells` and its scale (ADR-1200 D9):
/// the tallest cell whose whole multiple is `height`, else the tallest
/// under it scaled by the largest whole factor that fits, else the
/// smallest at its own size; the scale as far as keeps the cell inside
/// [`CELL_SIDE_MAX`].
pub(crate) fn pick(cells: &[Cell], height: u32) -> Option<(Cell, u32)> {
    let multiple = cells
        .iter()
        .filter(|c| height >= c.height && height.checked_rem(c.height) == Some(0))
        .max_by_key(|c| c.height);
    let under = || {
        cells
            .iter()
            .filter(|c| c.height < height)
            .max_by_key(|c| c.height)
    };
    let smallest = || cells.iter().min_by_key(|c| c.height);
    let cell = *multiple.or_else(under).or_else(smallest)?;
    let fit = CELL_SIDE_MAX
        .checked_div(cell.width.max(cell.height))
        .unwrap_or(1);
    let scale = height.checked_div(cell.height).unwrap_or(1);
    Some((cell, scale.clamp(1, fit.max(1))))
}

impl SetFace {
    /// `set` at the cell `height` asks for, `None` for a set with no
    /// strikes.
    pub fn new(set: Arc<Set>, height: u32) -> Option<SetFace> {
        let (cell, scale) = pick(&set.cells(), height)?;
        Some(SetFace { set, cell, scale })
    }

    /// Picks the cell and scale for `height` as [`SetFace::new`] does.
    pub fn set_height(&mut self, height: u32) {
        if let Some((cell, scale)) = pick(&self.set.cells(), height) {
            self.cell = cell;
            self.scale = scale;
        }
    }

    pub fn set(&self) -> &Arc<Set> {
        &self.set
    }
}

/// `d` scaled by `scale`, as the glyphs are: each row a band `scale`
/// rows tall.
pub(crate) fn scale_decorations(d: Decorations, scale: u32) -> Decorations {
    Decorations {
        underline: d.underline.saturating_mul(scale),
        underline_thickness: d.underline_thickness.saturating_mul(scale),
        strike: d.strike.saturating_mul(scale),
        strike_thickness: d.strike_thickness.saturating_mul(scale),
    }
}

impl Face for SetFace {
    fn cell(&self) -> Cell {
        Cell {
            width: self.cell.width.saturating_mul(self.scale),
            height: self.cell.height.saturating_mul(self.scale),
        }
    }

    fn size_step(&self) -> u32 {
        self.cell.height
    }

    fn has_glyph(&self, c: char) -> bool {
        self.set.has(self.cell, c)
    }

    /// The set's own count, from `POLY`.
    fn coverage(&self, range: Range) -> RangeCoverage {
        self.set
            .coverage(self.cell, range)
            .unwrap_or_else(|| RangeCoverage::count(range, |c| self.has_glyph(c)))
    }

    fn sprite_policy(&self) -> SpritePolicy {
        self.set.policy()
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.set
            .draw(self.cell, *cluster.first()?, style, self.scale)
    }

    fn last_resort(&self, style: Style) -> Option<Glyph> {
        self.set.last_resort(self.cell, style, self.scale)
    }

    fn decorations(&self) -> Decorations {
        self.set
            .decorations(self.cell)
            .map(|d| scale_decorations(d, self.scale))
            .unwrap_or_else(|| Decorations::for_height(self.cell().height))
    }

    fn scale(&self) -> u32 {
        self.scale
    }

    fn draw_unscaled(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.set.draw(self.cell, *cluster.first()?, style, 1)
    }

    fn last_resort_unscaled(&self, style: Style) -> Option<Glyph> {
        self.set.last_resort(self.cell, style, 1)
    }
}
