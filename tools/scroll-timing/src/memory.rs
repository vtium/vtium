//! The memory each way needs, for one, two and three display buffers.
//!
//! Every way keeps the display's buffers at the screen's size, apart from
//! the pan, whose buffers are as tall as its ring and twins. The copy adds
//! its ring in system memory. The redraw and the move add nothing.

use crate::Geometry;
use crate::ways::{Copy, Ring, Way};

/// A way's memory, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    /// The display's buffers together, as the way makes them.
    pub display: u64,
    /// Memory the way keeps outside the display's buffers: the copy's ring.
    pub system: u64,
    /// What the way adds to today's buffers at the screen's size.
    pub added: u64,
}

/// The memory `way` needs on `g` with `buffers` display buffers whose lines
/// are `pitch` bytes apart, the pan's ring holding `spare` rows beyond the
/// grid's.
pub fn needs(g: Geometry, way: Way, buffers: u32, pitch: u64, spare: u32) -> Memory {
    let screen = u64::from(buffers) * pitch * u64::from(g.height);
    let (display, system) = match way {
        Way::Redraw | Way::Move => (screen, 0),
        Way::Copy => (screen, Copy::memory(g)),
        Way::Pan => (
            u64::from(buffers) * pitch * u64::from(Ring::new(g, spare).buffer_height(g)),
            0,
        ),
    };
    Memory {
        display,
        system,
        added: display + system - screen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: Geometry = Geometry {
        width: 1920,
        height: 1080,
        cell_width: 8,
        cell_height: 16,
    };

    #[test]
    fn the_figures_at_1920x1080_in_an_8x16_cell() {
        let pitch = 1920 * 4;
        let m = |display, system, added| Memory {
            display,
            system,
            added,
        };
        // Two buffers of 1920 × 1080 × 4 bytes.
        assert_eq!(needs(HD, Way::Redraw, 2, pitch, 0), m(16_588_800, 0, 0));
        assert_eq!(needs(HD, Way::Move, 3, pitch, 0), m(24_883_200, 0, 0));
        // The ring: 67 rows of 16 lines of 7,680 bytes.
        assert_eq!(
            needs(HD, Way::Copy, 1, pitch, 0),
            m(8_294_400, 8_232_960, 8_232_960)
        );
        // No spare rows: 67 ring rows and 68 twins, 2,160 lines a buffer.
        assert_eq!(
            needs(HD, Way::Pan, 1, pitch, 0),
            m(16_588_800, 0, 8_294_400)
        );
        // A screen of spare rows: 134 ring rows and 68 twins, 3,232 lines.
        let tall = 3 * 7_680 * 3_232;
        assert_eq!(
            needs(HD, Way::Pan, 3, pitch, 67),
            m(tall, 0, tall - 24_883_200)
        );
    }
}
