//! [`Unifont`]: the built-in GNU Unifont, 8x16 and 16x16 glyphs for every
//! codepoint it covers, the font vtium draws with when no font file is set.
//!
//! The glyphs come from `data/unifont.hex` (GNU Unifont 17.0.03, the
//! unifont-all build, under the SIL Open Font License 1.1 in
//! `data/OFL-1.1.txt`). `build.rs` packs them into a vitrified set, the
//! built-in set ADR-1200 D7 names `unifont`: one 1-bit strike of 8x16
//! cells, its glyph chunks of at most 16 KiB each deflated. A chunk is
//! inflated the first time a glyph is drawn from it and kept 1-bit for the
//! life of the process, each glyph expanded as it is drawn, so the binary
//! carries the compressed font and memory holds only the chunks in use. The set is
//! trusted by construction, so its chunks are not hashed again. The C
//! reference inflated the whole font, 3.7 MB, when it loaded
//! (font_unifont.c:203-245).
//!
//! The drawing follows the C reference's `font_unifont.c`, whose notice,
//! under the MIT licence whose text is in COPYING, is:
//!
//! Copyright (c) 2012 Ted Kotz <ted@kotz.us>
//! Copyright (c) 2012-2013 David Herrmann <dh.herrmann@googlemail.com>

use std::sync::OnceLock;

use crate::bounds::CELL_SIDE_MAX;
use crate::face::{Cell, Decorations, Face, Style};
use crate::glyph::Glyph;
use crate::policy::{RangeCoverage, SpritePolicy};
use crate::set::Set;
use crate::sprite::Range;

static PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/unifont.glass"));

/// The built-in set's one cell.
const CELL: Cell = Cell {
    width: 8,
    height: 16,
};

/// The built-in unifont set, read once; `None` only if the build packed a
/// set its own reader refuses, which `tests::the_builtin_set_reads` holds
/// against.
pub(crate) fn set() -> Option<&'static Set> {
    static SET: OnceLock<Option<Set>> = OnceLock::new();
    SET.get_or_init(|| Set::from_static(PACKED).ok()).as_ref()
}

/// The built-in unifont, drawn at a whole multiple of 8x16.
#[derive(Clone, Copy, Debug)]
pub struct Unifont {
    scale: u32,
}

impl Default for Unifont {
    fn default() -> Unifont {
        Unifont { scale: 1 }
    }
}

impl Unifont {
    /// The font scaled by the largest whole factor of 16 that fits
    /// `height`, at least 1, as far as keeps a wide glyph's cell inside
    /// [`CELL_SIDE_MAX`].
    pub fn new(height: u32) -> Unifont {
        let mut font = Unifont::default();
        font.set_height(height);
        font
    }

    /// Rescales the font as [`Unifont::new`] does.
    pub fn set_height(&mut self, height: u32) {
        self.scale = (height / 16).clamp(1, CELL_SIDE_MAX / 16);
    }

    /// The factor glyphs are drawn at.
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// The glyph for `c` at its own width, 1 cell or 2, bold widening each
    /// byte of a row by a pixel to the right; `None` if unifont lacks it.
    pub fn render(&self, c: char, style: Style) -> Option<Glyph> {
        self.render_at(c, style, self.scale)
    }

    /// The glyph for `c` drawn at `scale`.
    fn render_at(&self, c: char, style: Style, scale: u32) -> Option<Glyph> {
        set()?.draw(CELL, c, style, scale)
    }
}

impl Face for Unifont {
    fn cell(&self) -> Cell {
        Cell {
            width: self.scale.saturating_mul(8),
            height: self.scale.saturating_mul(16),
        }
    }

    fn size_step(&self) -> u32 {
        16
    }

    fn has_glyph(&self, c: char) -> bool {
        set().is_some_and(|s| s.has(CELL, c))
    }

    /// The set's own count, from its `POLY`.
    fn coverage(&self, range: Range) -> RangeCoverage {
        set()
            .and_then(|s| s.coverage(CELL, range))
            .unwrap_or_else(|| RangeCoverage::count(range, |c| self.has_glyph(c)))
    }

    /// The sprites, though unifont covers every range: the C reference
    /// drew them over its unifont, and the goldens hold that look. The
    /// built-in set carries this policy.
    fn sprite_policy(&self) -> SpritePolicy {
        set().map_or(SpritePolicy::SPRITES, Set::policy)
    }

    /// The set's lines, which are those of a 16-pixel face with no metrics
    /// of its own, scaled.
    fn decorations(&self) -> Decorations {
        Decorations::for_height(self.cell().height)
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.render(*cluster.first()?, style)
    }

    fn scale(&self) -> u32 {
        self.scale
    }

    fn draw_unscaled(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.render_at(*cluster.first()?, style, 1)
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::face::Width;

    // the built-in set reads back through the set reader, whole: its
    // provenance, one strike, its policy and every chunk
    #[test]
    fn the_builtin_set_reads() {
        let set = set().expect("the built-in unifont set");
        assert_eq!(set.name(), "unifont");
        assert_eq!(set.meta_values("license").collect::<Vec<_>>(), ["OFL-1.1"]);
        let strikes: Vec<_> = set.strikes().collect();
        assert_eq!(strikes.len(), 1);
        assert_eq!(strikes[0].cell, CELL);
        assert_eq!(set.policy(), SpritePolicy::SPRITES);
        set.check_chunks().expect("every chunk decodes");
    }

    // the last codepoint unifont holds draws, and one past it does not
    #[test]
    fn the_last_glyph_reads_back() {
        let u = Unifont::new(16);
        let last = (0..=0x10ffffu32)
            .rev()
            .filter_map(char::from_u32)
            .find(|&c| u.has_glyph(c))
            .expect("a glyph");
        assert!(u32::from(last) >> 16 == 15, "plane 15's last glyph");
        assert!(u.render(last, Style::default()).is_some());
        assert!(u.render('A', Style::default()).is_some());
        assert!(!u.has_glyph('\u{10ffff}'));
        let wide = u.render('\u{4e00}', Style::default()).expect("a CJK glyph");
        assert_eq!((wide.width(), wide.cells()), (16, Width::Wide));
    }
}
