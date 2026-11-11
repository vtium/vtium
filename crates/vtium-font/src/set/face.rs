//! [`SetFace`]: one cell size of a [`Set`], drawn as a [`Face`].

use std::sync::Arc;

use super::read::Set;
use crate::bounds::CELL_SIDE_MAX;
use crate::face::{Cell, Decorations, Face, Style};
use crate::glyph::Glyph;
use crate::policy::{RangeCoverage, SpritePolicy};
use crate::script::lines::at_multiple;
use crate::script::{self, Origin, Script, Variant};
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

    /// The glyph the set draws for `cluster` whole, at this face's cell and
    /// scale: a cluster its `CLUS` maps, such as an emoji sequence or a
    /// letter's joining form; `None` for one it does not map.
    pub fn cluster(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.set.cluster(self.cell, cluster, style, self.scale)
    }

    /// The glyph the set holds for `cluster` in `variant`, at this face's
    /// cell and scale, from a strike of the variant's script alone
    /// (ADR-1202 D11): a codepoint's from the strike's map, and a longer
    /// cluster's from its cluster map. A superscript or subscript glyph
    /// comes as its source made it, small and placed in the whole cell;
    /// `None` when the set holds none, so a set of normal glyphs gives no
    /// script glyph.
    pub fn draw_variant(&self, cluster: &[char], variant: Variant) -> Option<Glyph> {
        match cluster {
            [c] => self.set.draw_variant(self.cell, *c, variant, self.scale),
            _ => self
                .set
                .cluster_variant(self.cell, cluster, variant, self.scale),
        }
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
            .map(|d| at_multiple(d, self.cell.height, self.scale))
            .unwrap_or_else(|| Decorations::for_height(self.cell().height))
    }

    /// The lines the set's strike of `script` at the face's cell reports,
    /// at the face's scale, as the glyph helper's script strikes report
    /// them (ADR-1202 D8 step 3); else the lines vtium-font's script rule
    /// makes from the set's own strike ([`script::lines::of_face`]).
    fn script_decorations(&self, script: Script) -> Decorations {
        match self.set.script_decorations(self.cell, script) {
            Some(d) => at_multiple(d, self.cell.height, self.scale),
            None => script::lines::of_face(self, script),
        }
    }

    /// The lines of the set's regular strike at the face's cell, at its
    /// own size.
    fn strike_decorations(&self) -> Decorations {
        self.set
            .decorations(self.cell)
            .unwrap_or_else(|| Decorations::for_height(self.cell.height))
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

    /// The set's strikes at the face's cell, at the face's scale. Only a
    /// glyph from a 1-bit strike takes a smaller multiple of it
    /// ([`Face::origin`] says which).
    fn strike(&self) -> Option<(Cell, u32)> {
        Some((self.cell, self.scale))
    }

    fn draw_multiple(&self, cluster: &[char], style: Style, multiple: u32) -> Option<Glyph> {
        self.set
            .draw(self.cell, *cluster.first()?, style, multiple.max(1))
    }

    /// The strike's ([`Set`]'s origin): a 1-bit strike's glyph a bitmap
    /// whose widening and shear the small glyph takes, any other smooth.
    fn origin(&self, cluster: &[char], style: Style) -> Origin {
        self.set
            .origin(self.cell, cluster.first().copied(), style, self.scale)
            .unwrap_or_default()
    }
}
