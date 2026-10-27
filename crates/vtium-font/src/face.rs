//! [`Face`], what a glyph source provides, and [`Font`], which draws a
//! cluster into its cells from a face and the sprites.

use crate::bitmap::Bitmap;
use crate::sprite;

/// How a glyph is drawn. Bitmap faces draw bold by widening each row by a
/// pixel and draw italic upright, as the C reference's do.
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

    /// The glyph for `cluster` at the face's own width for it, or `None`
    /// when the face lacks it. A face that cannot shape draws the first
    /// character.
    fn draw(&self, cluster: &[char], style: Style) -> Option<Bitmap>;

    /// The glyph drawn when the face lacks the character, U+25A1 and `?`.
    fn last_resort(&self, _style: Style) -> Option<Bitmap> {
        None
    }

    /// Where underline and strike-through go.
    fn decorations(&self) -> Decorations {
        Decorations::for_height(self.cell().height)
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
    fn draw(&self, cluster: &[char], style: Style) -> Option<Bitmap> {
        (**self).draw(cluster, style)
    }
    fn last_resort(&self, style: Style) -> Option<Bitmap> {
        (**self).last_resort(style)
    }
    fn decorations(&self) -> Decorations {
        (**self).decorations()
    }
}

/// What the renderer draws text with: a face, with the sprites drawn over
/// it for the characters that must fill their cell.
#[derive(Clone, Debug)]
pub struct Font<F> {
    face: F,
}

/// Drawn for a character the face lacks.
pub const REPLACEMENT: char = '\u{25a1}';

impl<F: Face> Font<F> {
    pub fn new(face: F) -> Font<F> {
        Font { face }
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
        sprite::has(c) || self.face.has_glyph(c)
    }

    /// The glyph for `cluster`, exactly `width` cells wide and a cell high.
    /// A single box drawing, block, Braille, sextant or octant character is
    /// a sprite that fills the cell. A cluster whose first character the
    /// face lacks draws as U+25A1, else `?`, else the face's last resort; a
    /// glyph of another width is centred and cut to the cells. `None` only
    /// for an empty cluster or a face that can draw nothing. The C
    /// reference's `kmscon_font_render` (font.c:310-368).
    pub fn glyph(&self, cluster: &[char], width: Width, style: Style) -> Option<Bitmap> {
        let &first = cluster.first()?;
        let cell = self.face.cell();
        let w = cell.width.saturating_mul(width.cells());
        if let [c] = *cluster {
            let thick = self.face.decorations().underline_thickness;
            if let Some(s) = sprite::draw(c, w, cell.height, thick) {
                return Some(s);
            }
        }
        let glyph = self
            .face
            .has_glyph(first)
            .then(|| self.face.draw(cluster, style))
            .flatten()
            .or_else(|| self.face.draw(&[REPLACEMENT], style))
            .or_else(|| self.face.draw(&['?'], style))
            .or_else(|| self.face.last_resort(style))?;
        glyph.fit(w)
    }
}
