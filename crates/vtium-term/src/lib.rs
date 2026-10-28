#![forbid(unsafe_code)]
// Determinism under hostile input (docs/architecture.md, "Determinism"):
// nothing a program sends can panic the terminal state.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! Terminal state.
//!
//! Applies parsed actions to the screen: cells and lines, scrollback,
//! wrapping, grapheme clusters and widths, the cursor, character sets,
//! attributes (SGR), the mode registry, and the replies a program can ask
//! for (device attributes, mode reports, identification, capability
//! queries). Produces frames: what changed since a display last drew.
//!
//! # The cell, line and frame model
//!
//! ADR-1001 records the model.
//!
//! - A [`Cell`] is 8 bytes: its content and width, and the indices of its
//!   [`Style`] and its hyperlink in its line's tables.
//! - A [`Line`] owns those tables, an arena of its grapheme clusters
//!   bounded by its width, a [`LineId`]
//!   that image placements and prompt marks anchor to, and its
//!   [`LineFlags`]: the reflow flag, the shell-integration marks and the
//!   line size.
//! - [`Damage`] keeps change ages by screen position, so cells and the
//!   scrollback carry none.
//! - A [`Frame`] is what a renderer reads: the visible lines with colors
//!   resolved through the [`Palette`] at draw time, each cell marked
//!   changed or not for the reading display, and the [`Cursor`].
//!
//! # The screen
//!
//! A [`Screen`] holds the main and alternate screens' lines, the cursor,
//! the margins and tab stops, the scrollback and its view, and the
//! selection, and carries printing, cursor movement, erasing, inserting,
//! deleting and scrolling. Text is cut into grapheme clusters by UAX #29,
//! with widths from the Unicode Character Database at
//! [`UNICODE_VERSION`]. A [`Terminal`] feeds the bytes a program writes
//! through `vtium-parse` and applies their actions to its screen.
//!
//! # Bounds
//!
//! Each bound is defined once in `src/bounds.rs` and tabulated in the
//! crate's README: [`COLUMNS_MAX`], [`ROWS_MAX`], [`CLUSTER_MAX`],
//! [`CLUSTER_BYTES_PER_CELL`] and the scrollback
//! caps [`SB_SIZE_CAP_SMALL`], [`SB_SIZE_CAP_MEDIUM`] and
//! [`SB_SIZE_CAP_LARGE`], which [`sb_size`] applies by the machine's
//! memory unless the configuration sets a cap, at most [`SB_SIZE_MAX`];
//! and the replies' [`REPLIES_PENDING_MAX`], [`VERSION_MAX`], [`TERMCAP_ENTRIES_MAX`],
//! [`TERMCAP_NAME_MAX`], [`TERMCAP_VALUE_MAX`], [`XTGETTCAP_NAMES`] and
//! [`XTGETTCAP_REPLY_MAX`]; OSC 52's [`CLIPBOARD_MAX`]; OSC 8's link
//! store's [`LINKS_BYTES_MAX`]; and the stacks' [`SGR_STACK_MAX`] and
//! [`COLOR_STACK_MAX`].

mod bounds;
mod cell;
mod damage;
mod frame;
mod line;
mod links;
mod palette;
mod screen;
mod scrollback;
mod style;
mod termcap;
mod terminal;
mod unicode;

pub use bounds::{
    CLIPBOARD_MAX, CLUSTER_BYTES_PER_CELL, CLUSTER_MAX, COLOR_STACK_MAX, COLUMNS_MAX,
    LINKS_BYTES_MAX, REPLIES_PENDING_MAX, ROWS_MAX, SB_SIZE_CAP_LARGE, SB_SIZE_CAP_MEDIUM,
    SB_SIZE_CAP_SMALL, SB_SIZE_MAX, SGR_STACK_MAX, TERMCAP_ENTRIES_MAX, TERMCAP_NAME_MAX,
    TERMCAP_VALUE_MAX, VERSION_MAX, XTGETTCAP_NAMES, XTGETTCAP_REPLY_MAX,
};
pub use cell::{Cell, ClusterOffset, Content, LinkIndex, StyleIndex, Width};
pub use damage::{Age, Damage};
pub use frame::{Cursor, CursorShape, Frame, FrameCell, Overlay, dim};
pub use line::{Glyph, Line, LineFlags, LineId, LineSize, LinkId};
pub use palette::{PALETTE_NAMES, Palette, Special};
pub use screen::{Erase, Screen, WidthMethod};
pub use scrollback::{SB_SIZE_DEFAULT, sb_size, sb_size_cap};
pub use style::{Attrs, Color, Rgb, Script, Style, UnderlineStyle};
pub use termcap::{Termcap, TermcapError};
pub use terminal::{
    BlankRequest, BlankTimeout, CursorStyle, Mods, MouseButton, MouseEvent, MouseTracking,
    Terminal, VersionError,
};
pub use unicode::UNICODE_VERSION;
