//! [`Face`], what a glyph source provides, and [`Font`], which draws a
//! cluster into its cells from a face and the sprites.

use crate::glyph::Glyph;
use crate::hex;
use crate::policy::{Coverage, Missing, RangeCoverage, Source, SpritePolicy};
use crate::sprite::{self, Range};

/// How a glyph is drawn. Bitmap faces draw bold by widening each row by a
/// pixel and draw italic upright, as the C reference's did.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
}

/// How many cells a glyph spans.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Width {
    #[default]
    Narrow,
    Wide,
}

impl Width {
    /// 1 or 2.
    pub fn cells(self) -> u32 {
        match self {
            Width::Narrow => 1,
            Width::Wide => 2,
        }
    }
}

/// A cell's size in pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub width: u32,
    pub height: u32,
}

/// Where underline and strike-through go in a cell, in pixel rows from its
/// top, each inside the cell and at least a row thick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Decorations {
    pub underline: u32,
    pub underline_thickness: u32,
    pub strike: u32,
    pub strike_thickness: u32,
}

impl Decorations {
    /// The lines a cell `height` tall gets from a face with no metrics of
    /// its own: a sixteenth of the cell thick, between 1 and a quarter of
    /// the cell, the underline on the bottom rows and the strike from the
    /// middle. The C reference's `kmscon_font_get_deco` (font.c:370-408).
    pub fn for_height(height: u32) -> Decorations {
        let h = height.max(1);
        let thick = (h / 16).clamp(1, (h / 4).max(1));
        let bottom = h.saturating_sub(thick);
        Decorations {
            underline: bottom,
            underline_thickness: thick,
            strike: (h / 2).min(bottom),
            strike_thickness: thick,
        }
    }
}

/// A glyph source: a font file, the built-in unifont, or later an outline
/// font.
pub trait Face {
    /// The cell this face draws into.
    fn cell(&self) -> Cell;

    /// How far one zoom step moves the height asked for: the font's own
    /// height for a bitmap font, which scales by whole factors.
    fn size_step(&self) -> u32;

    /// Whether the face holds a glyph for `c`.
    fn has_glyph(&self, c: char) -> bool;

    /// How many of `range`'s characters the face holds, from
    /// [`Face::has_glyph`]. A face that reads a character map may count
    /// it faster.
    fn coverage(&self, range: Range) -> RangeCoverage {
        RangeCoverage::count(range, |c| self.has_glyph(c))
    }

    /// The policy a [`Font`] of this face draws with until it is given
    /// one: [`SpritePolicy::default_for`] the face's coverage.
    fn sprite_policy(&self) -> SpritePolicy {
        SpritePolicy::default_for(&Coverage::new(|range| self.coverage(range)))
    }

    /// The glyph for `cluster` at the face's own width for it, marked with
    /// the cells that width spans, or `None`
    /// when the face lacks it. A face that cannot shape draws the first
    /// character.
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph>;

    /// The glyph drawn when the face lacks the character, U+25A1 and `?`.
    fn last_resort(&self, _style: Style) -> Option<Glyph> {
        None
    }

    /// Where underline and strike-through go.
    fn decorations(&self) -> Decorations {
        Decorations::for_height(self.cell().height)
    }

    /// The whole factor a bitmap face scales its glyphs by, each pixel a
    /// square; 1 for a face that draws at its own size.
    fn scale(&self) -> u32 {
        1
    }

    /// [`Face::draw`] at scale 1, for a renderer that stores a bitmap
    /// glyph small and scales it while drawing (ADR-1300 D2). `None` from
    /// a face that cannot.
    fn draw_unscaled(&self, _cluster: &[char], _style: Style) -> Option<Glyph> {
        None
    }

    /// [`Face::last_resort`] at scale 1.
    fn last_resort_unscaled(&self, _style: Style) -> Option<Glyph> {
        None
    }
}

impl<F: Face + ?Sized> Face for Box<F> {
    fn cell(&self) -> Cell {
        (**self).cell()
    }
    fn size_step(&self) -> u32 {
        (**self).size_step()
    }
    fn has_glyph(&self, c: char) -> bool {
        (**self).has_glyph(c)
    }
    fn coverage(&self, range: Range) -> RangeCoverage {
        (**self).coverage(range)
    }
    fn sprite_policy(&self) -> SpritePolicy {
        (**self).sprite_policy()
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        (**self).draw(cluster, style)
    }
    fn last_resort(&self, style: Style) -> Option<Glyph> {
        (**self).last_resort(style)
    }
    fn decorations(&self) -> Decorations {
        (**self).decorations()
    }
    fn scale(&self) -> u32 {
        (**self).scale()
    }
    fn draw_unscaled(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        (**self).draw_unscaled(cluster, style)
    }
    fn last_resort_unscaled(&self, style: Style) -> Option<Glyph> {
        (**self).last_resort_unscaled(style)
    }
}

/// What the renderer draws text with: a face, and the sprites its
/// [`SpritePolicy`] draws over it for the characters that fill their cell.
#[derive(Clone, Debug)]
pub struct Font<F> {
    face: F,
    policy: SpritePolicy,
    missing: Missing,
}

/// Drawn for a character the face lacks under [`Missing::Replacement`].
pub const REPLACEMENT: char = '\u{25a1}';

impl<F: Face> Font<F> {
    /// `face`, drawn with its own [`Face::sprite_policy`] and
    /// [`Missing::Replacement`] for a character it lacks.
    pub fn new(face: F) -> Font<F> {
        Font {
            policy: face.sprite_policy(),
            missing: Missing::default(),
            face,
        }
    }

    /// This font with each range drawn from the source `policy` gives it.
    pub fn with_policy(self, policy: SpritePolicy) -> Font<F> {
        Font { policy, ..self }
    }

    /// This font with a character the face lacks drawn as `missing`.
    pub fn with_missing(self, missing: Missing) -> Font<F> {
        Font { missing, ..self }
    }

    pub fn policy(&self) -> SpritePolicy {
        self.policy
    }

    pub fn missing(&self) -> Missing {
        self.missing
    }

    /// How much of each sprite range the face holds.
    pub fn coverage(&self) -> Coverage {
        Coverage::new(|range| self.face.coverage(range))
    }

    pub fn face(&self) -> &F {
        &self.face
    }

    pub fn face_mut(&mut self) -> &mut F {
        &mut self.face
    }

    pub fn cell(&self) -> Cell {
        self.face.cell()
    }

    pub fn decorations(&self) -> Decorations {
        self.face.decorations()
    }

    /// Whether `c` draws as itself: a sprite always does.
    pub fn has_glyph(&self, c: char) -> bool {
        self.draws_sprite(c) || self.face.has_glyph(c)
    }

    /// Whether `c` draws as a sprite under the policy.
    fn draws_sprite(&self, c: char) -> bool {
        Range::of(c).is_some_and(|range| match self.policy.source(range) {
            Source::Sprites => true,
            Source::Font => false,
            Source::Auto => !self.face.has_glyph(c),
        })
    }

    /// The character `cluster` draws as a sprite: a single character the
    /// policy draws as one, or the first of a longer cluster when the
    /// policy draws it as one and the face lacks it. The marks are
    /// dropped, as the C reference's bitmap faces dropped them; it drew
    /// U+25A1 for such a cluster (font.c:345-366).
    fn sprite_base(&self, cluster: &[char]) -> Option<char> {
        match *cluster {
            [c] => self.draws_sprite(c).then_some(c),
            [c, ..] => (self.draws_sprite(c) && !self.face.has_glyph(c)).then_some(c),
            [] => None,
        }
    }

    /// The glyph for `cluster`, exactly `width` cells wide and a cell high.
    /// A character the policy draws as a sprite fills the cell, alone or
    /// as the base of a cluster the face lacks. A cluster whose first
    /// character the face lacks otherwise draws as the font's [`Missing`]
    /// glyph, whose last fallback is U+25A1, else `?`, else the face's last
    /// resort; a glyph of another width is centred and cut to the cells.
    /// `None` only for an empty cluster or a face that can draw nothing.
    /// The C reference's `kmscon_font_render` (font.c:310-368).
    pub fn glyph(&self, cluster: &[char], width: Width, style: Style) -> Option<Glyph> {
        let &first = cluster.first()?;
        let cell = self.face.cell();
        let w = cell.width.saturating_mul(width.cells());
        if let Some(c) = self.sprite_base(cluster) {
            let thick = self.face.decorations().underline_thickness;
            if let Some(s) = sprite::draw(c, w, cell.height, thick) {
                return Some(s.with_cells(width));
            }
        }
        let glyph = self
            .face
            .has_glyph(first)
            .then(|| self.face.draw(cluster, style))
            .flatten()
            .or_else(|| self.missing_glyph(first, width))
            .or_else(|| self.face.draw(&[REPLACEMENT], style))
            .or_else(|| self.face.draw(&['?'], style))
            .or_else(|| self.face.last_resort(style))?;
        glyph.fit(w, width)
    }

    /// The glyph [`Missing`] draws for `c` across `width`, or `None` for
    /// U+25A1's chain.
    fn missing_glyph(&self, c: char, width: Width) -> Option<Glyph> {
        match self.missing {
            Missing::Replacement => None,
            Missing::Hex => hex::draw(c, self.face.cell(), width),
        }
    }

    /// [`Font::glyph`] for a renderer that stores bitmap glyphs small: the
    /// glyph at the face's scale 1 with that scale, which drawn with each
    /// pixel a square `scale` across is exactly [`Font::glyph`]'s. A
    /// sprite, a face that draws at its own size, and a glyph whose box
    /// to its cells does not scale (centred in an odd number of spare
    /// pixels) come at their drawn size, with scale 1.
    pub fn glyph_unscaled(
        &self,
        cluster: &[char],
        width: Width,
        style: Style,
    ) -> Option<(Glyph, u32)> {
        let drawn = || self.glyph(cluster, width, style).map(|g| (g, 1));
        let scale = self.face.scale();
        let &first = cluster.first()?;
        if scale <= 1 || self.sprite_base(cluster).is_some() {
            return drawn();
        }
        let w = self
            .face
            .cell()
            .width
            .checked_div(scale)?
            .saturating_mul(width.cells());
        let own = self
            .face
            .has_glyph(first)
            .then(|| self.face.draw_unscaled(cluster, style))
            .flatten();
        if own.is_none() && self.missing == Missing::Hex {
            return drawn();
        }
        let glyph = own
            .or_else(|| self.face.draw_unscaled(&[REPLACEMENT], style))
            .or_else(|| self.face.draw_unscaled(&['?'], style))
            .or_else(|| self.face.last_resort_unscaled(style));
        match glyph {
            Some(g) if w.abs_diff(g.width()) % 2 == 0 => Some((g.fit(w, width)?, scale)),
            _ => drawn(),
        }
    }
}
