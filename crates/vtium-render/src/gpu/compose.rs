//! Slot 2, composition into the upright frame (ADR-1303 D3).
//!
//! The upright frame is the whole display, held in tiles that each fit the
//! GPU's largest texture and viewport, one in the common case. Each frame
//! the composition draws what changed into every tile it meets, with the
//! tile's offset taken off: the border's parts the frame redraws, cleared
//! to the border's color; each cell the frame redraws, as one quad through
//! the cell shader, which does ADR-1300 D6's arithmetic; the pointer's
//! arrow over every pixel the frame drew; and last the console's notices,
//! uploaded over what the frame drew beneath them.
//!
//! A cell that pictures reach is drawn in the layers of ADR-1100 D4, as the
//! CPU path composes it: its background as a cell with nothing on it; then
//! each picture over the cells it reaches, through slot 1's textures, in
//! the frame's order, which is the layers' order; the cells' text layers,
//! premultiplied over the pictures beneath, before the first picture above
//! the text; the selection's tint; and vtium's mark, an underline cursor's
//! line. Last of all, after every cell, the bar of a bar cursor is drawn
//! as a quad of one color, over its cell's letter and pictures.
//!
//! No draw reads the target it draws into, and the GPU's own blending
//! turns its float result into a byte as the driver chooses. So each draw
//! that goes over pixels already drawn, the arrow and, in a cell pictures
//! reach, a picture that is not opaque, a text layer, vtium's mark and the
//! selection's tint, first copies the pixels beneath it out of the tile
//! into the copy, a target that lasts from frame to frame, and then reads
//! them from there and blends with the CPU path's arithmetic. The copy
//! grows when a frame draws over more than it holds, and is given back
//! once no frame has read it for `GPU_IDLE_FRAMES` frames.
//!
//! The renderer decides what changed, as it does on the CPU path: the
//! cells, the cursor, the border and the pointer, through the same front
//! end, and hands each row's cell jobs to [`GpuBack`], which turns them
//! into quads and the border into clears.

#[cfg(doc)]
use super::beside;
use super::gl::{Input, InputKind, Quad, Size};
use super::ink::ReceivingJob;
#[cfg(feature = "images")]
use super::pictures;
#[cfg(feature = "images")]
use super::pictures::Shown;
use crate::arrow::Arrow;
#[cfg(doc)]
use crate::deco::Line;
use crate::notice::Placed;
use crate::reach::Bound;
#[cfg(doc)]
use crate::reach::Reach;
use crate::skip;
use crate::target::Rect;

mod back;
mod composition;

pub(crate) use back::GpuBack;
use back::{COL, ROW};
pub(crate) use composition::Composition;
#[cfg(feature = "images")]
pub(crate) use composition::{Beneath, target_size};

const QUAD_VERT: &str = include_str!("glsl/quad.vert");
pub(crate) const CELL_VERT: &str = include_str!("glsl/cell.vert");
/// The first piece of the cell and picture shaders, which leave it out so
/// that [`OVER`] can follow it: GLSL takes nothing before `#version`.
const ES3: &str = "#version 300 es\n";
/// The piece that makes the cell or picture shader draw over the pixels
/// beneath it, which it reads from a copy of them, with the CPU path's
/// over_px in whole numbers: a cell's text layer or vtium's mark, or a
/// picture that is not opaque.
pub(crate) const OVER: &str = "#define OVER\n";
pub(crate) const CELL_FRAG: &str = include_str!("glsl/cell.frag");
/// The pieces that tell the cell shader how far a glyph's ink may reach
/// past its own cells to each side, and above and below (ADR-1306 D12):
/// the bound's two counts, each at most [`Reach::MAX`], as compile-time
/// constants of the shader, which bound its loops over a cell's
/// neighbours.
const REACH_SIDE: [&str; 4] = [
    "#define REACH_SIDE 0\n",
    "#define REACH_SIDE 1\n",
    "#define REACH_SIDE 2\n",
    "#define REACH_SIDE 3\n",
];
const REACH_ROWS: [&str; 4] = [
    "#define REACH_ROWS 0\n",
    "#define REACH_ROWS 1\n",
    "#define REACH_ROWS 2\n",
    "#define REACH_ROWS 3\n",
];

/// The piece that builds into the cell shader the ink a cell receives
/// from its neighbours' glyphs (ADR-1306 D9), for the cells flagged
/// [`RECEIVES`] and for the text layers over pictures. A cell that
/// receives no ink is drawn without it.
pub(crate) const INK: &str = "#define INK\n";

/// The cell shader's source for `bound`, after `#version`: for a text
/// layer or a mark, [`OVER`], for a cell that receives ink, [`INK`], and
/// the least alpha of a color glyph's ink, which the CPU path reads from
/// the same constant ([`skip::COLOR_INK`]).
fn cell_frag(over: bool, ink: bool, bound: Bound) -> [&'static str; 7] {
    let pick =
        |pieces: [&'static str; 4], n: u8| pieces.get(usize::from(n)).copied().unwrap_or(pieces[3]);
    [
        ES3,
        if over { OVER } else { "" },
        if ink { INK } else { "" },
        pick(REACH_SIDE, bound.side),
        pick(REACH_ROWS, bound.rows),
        skip::COLOR_INK_DEFINE,
        CELL_FRAG,
    ]
}
const ARROW_FRAG: &str = include_str!("glsl/arrow.frag");
#[cfg(feature = "images")]
const PICTURE_FRAG: &str = include_str!("glsl/picture.frag");
const UNDER_FRAG: &str = include_str!("glsl/under.frag");

pub(crate) const TARGET_SIZE: Input = Input {
    name: "target_size",
    kind: InputKind::Vec2,
};

const CELL_INPUTS: [Input; 6] = [
    TARGET_SIZE,
    Input {
        name: "masks",
        kind: InputKind::Texture,
    },
    Input {
        name: "colors",
        kind: InputKind::Texture,
    },
    Input {
        name: "sprites",
        kind: InputKind::Texture,
    },
    Input {
        name: "cell",
        kind: InputKind::IVec4,
    },
    Input {
        name: "besides",
        kind: InputKind::Texture,
    },
];

/// The inputs of the cell shader built with [`INK`]: the cell shader's,
/// the boxes, a box slot's size, the ink grid and the color boxes.
const INK_INPUTS: [Input; 10] = [
    CELL_INPUTS[0],
    CELL_INPUTS[1],
    CELL_INPUTS[2],
    CELL_INPUTS[3],
    CELL_INPUTS[4],
    CELL_INPUTS[5],
    Input {
        name: "boxes",
        kind: InputKind::Texture,
    },
    Input {
        name: "box_slot",
        kind: InputKind::IVec4,
    },
    Input {
        name: "ink",
        kind: InputKind::Texture,
    },
    Input {
        name: "color_boxes",
        kind: InputKind::Texture,
    },
];

/// A cell job's flags, in the top byte of its quad's second parameter, as
/// the cell shader reads them: it receives ink from its neighbours' glyphs;
/// a block cursor or the selection marks it; a display line runs down a
/// column of the upright frame, on a display turned a quarter; its
/// underline skips a color glyph's ink too ([`skip::skips_color`]). The
/// bits from [`GAP`] up hold the gap its underline keeps on each side of
/// its glyph's ink, in display pixels, as the CPU path's `near_ink` keeps
/// it, or 0 when the underline skips no ink ([`Line::skips_ink`]).
const RECEIVES: u32 = 1;
const MARKED: u32 = 2;
const DOWN: u32 = 4;
const COLOR: u32 = 8;
const GAP: u32 = 4;

/// The job a quad of the cell shader draws, in cells of `cell`, when it
/// receives ink. A quad that covers part of a cell counts that cell whole,
/// since the shader reads the line bits of the cell each pixel lies in.
pub(crate) fn receiving(q: &Quad, cell: Size) -> Option<ReceivingJob> {
    let flags = q.params[1] >> 24;
    if flags & RECEIVES == 0 {
        return None;
    }
    let n = |v: u32| usize::try_from(v).unwrap_or(0);
    let width = u32::try_from(q.dst[2]).unwrap_or(0);
    Some(ReceivingJob {
        col: n(q.src[2] & COL),
        row: n(q.src[3] & ROW),
        cells: n(if cell.width > 0 {
            width.div_ceil(cell.width)
        } else {
            0
        }),
        down: flags & DOWN != 0,
    })
}

/// What a shader that goes over the pixels beneath it takes besides its
/// own inputs: the copy of those pixels, and the pixel of the target its
/// first texel holds. The arrow's shader always takes them, the cell and
/// picture shaders when built with [`OVER`].
const BENEATH: [Input; 2] = [
    Input {
        name: "under",
        kind: InputKind::Texture,
    },
    Input {
        name: "under_at",
        kind: InputKind::IVec4,
    },
];

#[cfg(feature = "images")]
const LAYER_INPUTS: [Input; 12] = [
    INK_INPUTS[0],
    INK_INPUTS[1],
    INK_INPUTS[2],
    INK_INPUTS[3],
    INK_INPUTS[4],
    INK_INPUTS[5],
    INK_INPUTS[6],
    INK_INPUTS[7],
    INK_INPUTS[8],
    INK_INPUTS[9],
    BENEATH[0],
    BENEATH[1],
];

const ARROW_INPUTS: [Input; 4] = [
    TARGET_SIZE,
    Input {
        name: "arrow",
        kind: InputKind::Texture,
    },
    BENEATH[0],
    BENEATH[1],
];

#[cfg(feature = "images")]
const PICTURE_INPUTS: [Input; 4] = [
    TARGET_SIZE,
    Input {
        name: "picture",
        kind: InputKind::Texture,
    },
    Input {
        name: "scale",
        kind: InputKind::UVec4,
    },
    Input {
        name: "origin",
        kind: InputKind::IVec4,
    },
];

#[cfg(feature = "images")]
const PICTURE_OVER_INPUTS: [Input; 7] = [
    PICTURE_INPUTS[0],
    PICTURE_INPUTS[1],
    PICTURE_INPUTS[2],
    PICTURE_INPUTS[3],
    BENEATH[0],
    BENEATH[1],
    super::pictures::TABLES,
];

const UNDER_INPUTS: [Input; 2] = [
    TARGET_SIZE,
    Input {
        name: "under",
        kind: InputKind::Texture,
    },
];

/// The cells of a frame that sample one texture of glyphs: its index among
/// the glyphs' textures and whether it holds color glyphs, or `None` for
/// cells with no glyph; and whether they receive ink from their
/// neighbours' glyphs, which the cell shader built with [`INK`] draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cells {
    pub(crate) glyphs: Option<(usize, bool)>,
    pub(crate) ink: bool,
    pub(crate) quads: Vec<Quad>,
}

/// The pointer's arrow for a frame: its upright image and where its top
/// left lies in the upright frame.
#[derive(Clone, Copy, Debug)]
pub struct ArrowAt<'a> {
    pub image: &'a Arrow,
    pub x: i64,
    pub y: i64,
}

/// One frame's work for the composition, in the upright frame's pixels.
#[derive(Clone, Copy, Debug)]
pub struct Work<'a> {
    /// The border's color, `0x00RRGGBB`. A new one composes the whole frame.
    pub border: u32,
    /// The rectangles cleared to the border's color.
    pub clear: &'a [Rect],
    /// The rectangles that changed, which present draws into each buffer
    /// that lacks them.
    pub changed: &'a [Rect],
    /// The pointer's arrow, drawn over every pixel this frame draws.
    pub arrow: Option<ArrowAt<'a>>,
    /// The console's notices and focus mark, in the order they are drawn.
    pub overlays: &'a [Placed],
    pub(crate) cells: &'a [Cells],
    /// Uploads the frame made before the composition, which its check of
    /// uploads covers.
    pub(crate) uploads: usize,
    /// The frame's pictures, in draw order, and where each draws.
    #[cfg(feature = "images")]
    pub(crate) pictures: &'a [Shown],
    /// The text layers of the cells pictures reach.
    #[cfg(feature = "images")]
    pub(crate) text: &'a [Cells],
    /// The selection's tint over the cells pictures reach that are
    /// selected: each cell and the selection's color, `0x00RRGGBB`.
    #[cfg(feature = "images")]
    pub(crate) tints: &'a [(Rect, u32)],
    /// vtium's marks over the cells pictures reach.
    #[cfg(feature = "images")]
    pub(crate) marks: &'a [Cells],
    /// The bars of bar cursors, each a quad of one color drawn over
    /// everything else in its cell.
    pub(crate) bars: &'a [Cells],
}

impl<'a> Work<'a> {
    /// A frame that clears `clear` to `border` and changes `changed`, with
    /// no cell, arrow or notice.
    pub fn new(border: u32, clear: &'a [Rect], changed: &'a [Rect]) -> Work<'a> {
        Work {
            border,
            clear,
            changed,
            arrow: None,
            overlays: &[],
            cells: &[],
            uploads: 0,
            #[cfg(feature = "images")]
            pictures: &[],
            #[cfg(feature = "images")]
            text: &[],
            #[cfg(feature = "images")]
            tints: &[],
            #[cfg(feature = "images")]
            marks: &[],
            bars: &[],
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "a test fails by panicking"
)]
mod tests;
