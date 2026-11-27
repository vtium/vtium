//! Three ways to scroll the CPU path's display, and the redraw that vtium
//! does today, as code that draws into any buffer of XRGB8888 pixels. The binary runs them on a KMS device; the tests run them on
//! buffers in memory and check every picture against a redraw.
//!
//! The view scrolls back through a scrollback with no end, and new rows
//! come in at the top. The view's place is a pixel line of that scrollback,
//! where text row `r` covers lines `r` times the cell's height and the
//! cell's height after it. A scroll by whole text rows moves the place by
//! a cell's height each step; a smooth scroll moves it by a few pixel
//! lines, so the view can stand inside a row and show the first and last
//! rows of the grid in part. Each row is a test pattern of character cells
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

/// The unit a scroll's speed is given in: whole text rows a frame, or
/// pixel lines a frame, as a smooth scroll moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Rows,
    Pixels,
}

impl Unit {
    /// The name the results file's `speed_unit` column uses.
    pub fn name(self) -> &'static str {
        match self {
            Unit::Rows => "rows",
            Unit::Pixels => "pixels",
        }
    }

    /// The pixel lines a speed of `speed` in this unit moves the view by
    /// each frame, in cells of `g`.
    pub fn lines(self, speed: u32, g: Geometry) -> i64 {
        match self {
            Unit::Rows => i64::from(speed) * i64::from(g.cell_height),
            Unit::Pixels => i64::from(speed),
        }
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
    /// Text rows drawn from the pattern, wherever they were drawn. A row
    /// drawn in part, only the lines of it that came into view, counts as
    /// one.
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
