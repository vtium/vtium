//! How a frame's pixels are made: the [`Back`] trait, which draws the
//! border and each row's cell jobs, and [`Cpu`], the CPU path's back end,
//! which writes the row scratch into a target.

use vtium_font::{Face, Font, Variant, Width as FontWidth};
use vtium_term::Rgb;

use crate::blend::pixel;
use crate::compose::{Area, Composer, int};
use crate::reach::Reach;
use crate::target::{Rect, Target};

use super::double;

/// How a frame's pixels are made: the border and each row's cell jobs.
/// The CPU path writes them into the display's buffer ([`Cpu`]); the GPU
/// path turns them into clears and quads for the upright frame.
pub(crate) trait Back {
    /// Fills the border inside the display area `r` with `bg`. `display`
    /// is the whole display and `grid` the grid's area on it, `None` for
    /// an empty grid: the border is what the grid leaves.
    fn border(&mut self, compose: &mut Composer, display: (Rect, Option<Area>), bg: Rgb, r: Rect);

    /// Draws the jobs of grid row `row`, which `compose` holds with their
    /// text and the ink each receives.
    fn row<F: Face>(
        &mut self,
        compose: &mut Composer,
        font: &Font<F>,
        strip: (usize, usize),
        row: usize,
    );

    /// Whether the glyphs it draws from hold one whose ink passes its
    /// cells: until they do, no cell's ink reaches another.
    fn reaching(&self, compose: &Composer) -> bool;

    /// How far piece `piece` of the glyph for `text` in `style`, `width`
    /// cells wide, reaches past its cells, from the glyphs it draws from.
    /// A doubled piece is one cell of a glyph on a line of double size.
    fn reach<F: Face>(
        &mut self,
        compose: &mut Composer,
        font: &Font<F>,
        key: (&[char], FontWidth, Variant),
        piece: double::Piece,
    ) -> Reach;

    /// Whether it draws the ink a cell receives from its neighbours' glyphs,
    /// which the renderer then gathers for each job.
    const RECEIVES: bool;
}

/// The CPU path's [`Back`]: the row scratch, written into a target.
pub(super) struct Cpu<'a, T: Target> {
    pub(super) target: &'a mut T,
    /// Set while the border is drawn, for a ring that writes the border's
    /// second copies with it.
    #[cfg(feature = "scroll-pan")]
    pub(super) border: Option<&'a core::cell::Cell<bool>>,
}

impl<T: Target> Back for Cpu<'_, T> {
    fn border(
        &mut self,
        compose: &mut Composer,
        (display, grid): (Rect, Option<Area>),
        bg: Rgb,
        r: Rect,
    ) {
        #[cfg(feature = "scroll-pan")]
        if let Some(flag) = self.border {
            flag.set(true);
        }
        let (width, height) = (display.width, display.height);
        let (x0, x1) = (r.x.min(width), r.x.saturating_add(r.width).min(width));
        let y1 = r.y.saturating_add(r.height).min(height);
        let line = compose.solid(
            usize::try_from(x1.saturating_sub(x0)).unwrap_or(0),
            pixel(bg),
        );
        // the grid's columns, as a part of the run from x0
        let at = |v: i64| usize::try_from(v.clamp(int(x0), int(x1)).saturating_sub(int(x0)));
        let g = grid.unwrap_or_default();
        let (gl, gr) = (at(g.x).unwrap_or(0), at(g.right()).unwrap_or(0));
        for y in r.y..y1 {
            let yi = int(y);
            if grid.is_none() || yi < g.y || yi >= g.bottom() {
                self.target.write(x0, y, line);
                continue;
            }
            if let Some(left) = line.get(..gl).filter(|l| !l.is_empty()) {
                self.target.write(x0, y, left);
            }
            if let Some(right) = line.get(gr..).filter(|l| !l.is_empty()) {
                let x = x0.saturating_add(u32::try_from(gr).unwrap_or(u32::MAX));
                self.target.write(x, y, right);
            }
        }
        #[cfg(feature = "scroll-pan")]
        if let Some(flag) = self.border {
            flag.set(false);
        }
    }

    fn row<F: Face>(
        &mut self,
        compose: &mut Composer,
        font: &Font<F>,
        strip: (usize, usize),
        _: usize,
    ) {
        compose.draw(font, strip, &mut *self.target);
    }

    fn reaching(&self, compose: &Composer) -> bool {
        compose.cache.reaching()
    }

    fn reach<F: Face>(
        &mut self,
        compose: &mut Composer,
        font: &Font<F>,
        (text, width, style): (&[char], FontWidth, Variant),
        piece: double::Piece,
    ) -> Reach {
        match piece.is_whole() {
            true => compose.cache.reach(font, text, width, style),
            false => compose
                .cache
                .get_piece(font, (text, width, style), piece)
                .map_or(Reach::NONE, crate::stored::Stored::reach),
        }
    }

    const RECEIVES: bool = true;
}
