//! Hooks for the tests of crates that use vtium-render, hidden from its
//! documentation. They are not part of vtium-render's interface, and vtium
//! never calls them.
//!
//! A test calls them to draw at a reach bound wider than the one vtium
//! draws with (ADR-1306 D12), with glyphs whose ink reaches that far. A
//! font cannot draw such a glyph while the font crate's bound,
//! `vtium_font::REACH_CELLS`, is one cell, so the test gives the glyph's
//! ink box itself, and the renderer stores it as it stores any glyph at its
//! ink box. The shipped bound does not change.

use vtium_font::{Face, Variant, Width as FontWidth};

use super::Renderer;
use crate::cache::pool_of;
use crate::reach::{Bound, InkBox, Reach};
use crate::stored::Stored;
use crate::target::Orientation;

/// A narrow glyph's ink as a test gives it: a coverage mask `width` by
/// `height` pixels, one byte a pixel, whose top left lies `left` pixels
/// right of and `top` pixels below the top left of the glyph's own cell.
#[derive(Clone, Copy, Debug)]
pub struct TestInk<'a> {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

impl<F: Face> Renderer<F> {
    /// Draws with ink reaching at most `side` cells to each side of a
    /// glyph's own and `rows` rows above and below it, each at most 3. On
    /// the GPU path the cell shader's bounds and the boxes' slots follow.
    /// A test calls this before the renderer's first frame.
    #[doc(hidden)]
    pub fn set_reach_bound_for_test(&mut self, side: u8, rows: u8) {
        self.bound = Bound {
            side: side.min(Reach::MAX),
            rows: rows.min(Reach::MAX),
        };
    }

    /// Keeps `ink` as the glyph of `c`, narrow and in the regular style,
    /// in the CPU path's cache, which then never asks the font for it.
    #[doc(hidden)]
    pub fn keep_glyph_for_test(&mut self, c: char, ink: TestInk<'_>) {
        let stored = self.stored_for_test(ink, self.config.orientation);
        let pool = pool_of(false, false);
        self.compose.cache.keep(
            (&[c], FontWidth::Narrow, Variant::default()),
            pool,
            stored,
            false,
        );
    }

    /// Puts `ink` as the glyph of `c`, narrow and in the regular style,
    /// among the GPU path's glyphs in `pipeline`, which this renderer's
    /// first frame on it made. Says whether the glyph took a box, as a
    /// glyph whose ink passes its cell takes one when the boxes have room.
    #[doc(hidden)]
    #[cfg(feature = "gpu")]
    pub fn put_glyph_for_test<G: crate::gpu::Gl>(
        &self,
        gl: &mut G,
        pipeline: &mut crate::gpu::Pipeline,
        c: char,
        ink: TestInk<'_>,
    ) -> Result<bool, crate::gpu::Error> {
        // the GPU path draws upright and turns the frame as it presents it
        let stored = self.stored_for_test(ink, Orientation::Normal);
        let text = [c];
        let place =
            pipeline
                .glyphs()?
                .put(gl, (&text, FontWidth::Narrow, Variant::default()), stored)?;
        Ok(place.is_some_and(|p| p.boxed.is_some()))
    }

    /// `ink` stored for one cell of this renderer's, turned for `o`, as a
    /// cache stores a glyph from the font at its ink box.
    fn stored_for_test(&self, ink: TestInk<'_>, o: Orientation) -> Stored {
        let b = InkBox {
            rgba: false,
            left: ink.left,
            top: ink.top,
            width: ink.width,
            height: ink.height,
            pixels: ink.pixels,
        };
        let cell = |v: i64| u32::try_from(v).unwrap_or(1);
        Stored::boxed(
            b,
            (cell(self.layout.cw), cell(self.layout.ch), 1),
            1,
            o,
            self.config.storage.bits,
        )
    }
}
