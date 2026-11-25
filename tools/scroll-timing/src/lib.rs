//! The three ways to scroll that ADR-1310 D6 measures, and the redraw that
//! vtium does today, as code that draws into any buffer of XRGB8888
//! pixels. The binary runs them on a KMS device; the tests run them on
//! buffers in memory and check every picture against a redraw.
//!
//! The view scrolls back through a scrollback with no end: the row at the
//! top of the screen has an id that falls by the speed each frame, and the
//! rows come in at the top. Each row is a test pattern of character cells
//! that stands in for text ([`pattern`]), so the figures measure moving
//! rows and not drawing glyphs.
//!
//! References:
//! - ADR-1310 D6, "The measurement": the three ways and what to record.

pub mod memory;
pub mod pattern;
pub mod stats;
pub mod ways;

/// A screen and the character cell its text is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
}

impl Geometry {
    /// The text rows that fit the screen: the grid's height in rows.
    pub fn rows(&self) -> u32 {
        self.height / self.cell_height
    }

    /// The character cells across a row.
    pub fn cols(&self) -> u32 {
        self.width / self.cell_width
    }

    /// The pixel rows below the grid, which the border fills.
    pub fn strip(&self) -> u32 {
        self.height - self.rows() * self.cell_height
    }

    /// The bytes of one line of pixels across the screen.
    pub fn line_bytes(&self) -> usize {
        self.width as usize * 4
    }

    /// The bytes one text row writes: a line of pixels across the screen
    /// for each pixel row of the cell.
    pub fn row_bytes(&self) -> u64 {
        self.line_bytes() as u64 * u64::from(self.cell_height)
    }
}

/// What one frame of a way did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Bytes written into the display's buffer.
    pub written: u64,
    /// Bytes read back from the display's buffer.
    pub read: u64,
    /// Bytes written into memory of the way's own, outside the display's
    /// buffer.
    pub system_written: u64,
    /// Text rows drawn from the pattern, wherever they were drawn.
    pub rows_drawn: u32,
}

impl std::ops::AddAssign for Counts {
    fn add_assign(&mut self, other: Counts) {
        self.written += other.written;
        self.read += other.read;
        self.system_written += other.system_written;
        self.rows_drawn += other.rows_drawn;
    }
}
