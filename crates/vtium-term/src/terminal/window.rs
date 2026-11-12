//! XTWINOPS 8, `CSI 8 ; height ; width t`: a program asks for a text area
//! of `height` rows and `width` columns (ctlseqs, "Window manipulation
//! (XTWINOPS)"). The terminal keeps the request until the host takes it
//! with [`Terminal::take_grid_request`](crate::Terminal::take_grid_request).
//! The host makes the new grid and resizes the pty, then resizes the
//! terminal, so a size report that follows the request answers the new
//! size.
//!
//! - An omitted parameter keeps that side as it is, and a parameter of 0
//!   takes the display's length on that side, as ctlseqs says.
//! - The input call returns after the request, as after OSC 50, so the
//!   host acts on it before the next byte is parsed. A report such as
//!   `CSI 18 t` that follows in the same write then answers the size the
//!   host made.
//! - Until the host turns window operations on, the request is ignored,
//!   as xterm ignores it under `allowWindowOps: false`.

use vtium_parse::Params;

use super::State;

/// One side of the text area that XTWINOPS 8 asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extent {
    /// The parameter was omitted: the side keeps its length.
    Keep,
    /// The parameter was 0: the side takes the length of the display.
    Display,
    /// That many character cells, 1 or more.
    Cells(usize),
}

impl Extent {
    fn of(param: Option<u32>) -> Extent {
        match param {
            None => Extent::Keep,
            Some(0) => Extent::Display,
            Some(n) => Extent::Cells(usize::try_from(n).unwrap_or(usize::MAX)),
        }
    }
}

/// The text area a program asked for with XTWINOPS 8, in rows and columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridRequest {
    pub rows: Extent,
    pub cols: Extent,
}

impl State {
    /// XTWINOPS 8 from its parameters: the request waits for the host
    /// while window operations are on.
    pub(super) fn request_grid(&mut self, params: &Params) {
        if !self.host.window_ops {
            return;
        }
        self.grid_request = Some(GridRequest {
            rows: Extent::of(params.get(1)),
            cols: Extent::of(params.get(2)),
        });
        // The input call returns here, so the host resizes before the next
        // byte, and a size report that follows answers the new size.
        self.pause = true;
    }
}
