#![forbid(unsafe_code)]
// Determinism under hostile input (docs/architecture.md, "Determinism"):
// nothing a program sends can panic the store.
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
//! Inline images: the one pipeline sixel and the kitty graphics protocol
//! feed (ADR-1100).
//!
//! - The [store](Store): each console's images and placements, main and
//!   alternate buffers, the budget over both and the eviction order, and
//!   what every screen event does to a placement. A placement is anchored
//!   to the line holding its top row, named by an opaque [`LineKey`], so
//!   this crate never depends on vtium-term.
//! - The [protocol tag](Protocol) and the [rules] function, the one place
//!   each protocol's rules live.
//! - The [handler trait](Handler) the protocol front ends implement, the
//!   [context](Context) vtium-term gives them, and the [store edit](Edit)
//!   through which a handler changes the store.
//! - The two [protocol] handlers, [`Sixel`] and [`Kitty`].
//! - The [decode request and reply types](decode), which the image helper
//!   answers.
//! - Its bounds, named constants from the C reference's values, and the
//!   `auto` budget's rule, [`auto_limit`].
//!
//! The crate is not part of the minspec build: vtium-term takes it behind
//! its `images` feature.

mod bounds;
pub mod decode;
mod handler;
mod image;
pub mod protocol;
mod rules;
mod store;

pub use bounds::{
    AUTO_CAP, AUTO_FLOOR, AUTO_FRAMES, BOX_CELLS_MAX, CELL_MAX, IMAGE_OVERHEAD, IMAGE_SIDE_MAX,
    IMAGES_PER_BUFFER, PLACEMENT_OVERHEAD, PLACEMENTS_PER_BUFFER, auto_limit,
};
pub use handler::{Context, Handler, Intro, Limits, Margins, ReplyKind, SixelState, Switches};
pub use image::{ImageKey, LineKey, NewImage, Pixels, Place, PlacementKey, Refusal};
pub use protocol::kitty::Kitty;
pub use protocol::sixel::Sixel;
pub use rules::{Layer, Protocol, RegionScroll, Rules, rules};
pub use store::{Edit, FramePlacement, Region, Reservation, Rows, Screens, Store};
