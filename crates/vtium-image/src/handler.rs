//! The handler trait the protocol front ends implement, and the context
//! the terminal gives them (ADR-1100, D6).
//!
//! A handler turns one protocol's control strings into store and screen
//! changes. vtium-term's router calls it as the string arrives: `start`
//! with the introducer, `feed` with the data, then `finish` when the
//! string ended with its terminator or `cancel` when it was cancelled. A
//! handler that asked the helper for something (D7) gets the answer in
//! `decoded`. Every method gets the [`Context`] vtium-term implements; a
//! handler never reaches the screen's lines itself, so every scroll it
//! causes passes through the screen and the store's events.

use vtium_parse::{Chunk, Params};

use crate::decode::{Reply, Request};
use crate::image::{ImageKey, NewImage, Place, PlacementKey, Refusal};
use crate::store::Store;

/// A control string's introducer.
#[derive(Clone, Copy, Debug)]
pub enum Intro<'a> {
    /// A DCS string's parameters: sixel's `P1 ; P2 ; P3`.
    Dcs(&'a Params),
    /// An APC or OSC string the router matched to the handler.
    String,
}

/// The kinds of reply a handler writes, a closed set: vtium-term writes
/// each under one `Query`, so every image reply is in the reply rule's
/// generated list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyKind {
    /// A kitty graphics reply, `ESC _ G ... ESC \`, which carries only
    /// numbers it re-encodes and fixed messages.
    KittyGraphics,
}

/// Which protocols are on (D10, D11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Switches {
    pub sixel: bool,
    pub kitty: bool,
    pub iterm2: bool,
}

/// The limits D11's settings give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// `max-graphic-size`: the largest picture in pixels, each side.
    pub graphic: (u32, u32),
    /// `num-color-registers`.
    pub registers: u32,
    /// `private-color-registers`: mode 1070's state at reset.
    pub private_registers: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            graphic: (10_000, 10_000),
            registers: 1024,
            private_registers: true,
        }
    }
}

/// What a handler reaches, which vtium-term implements over one terminal.
pub trait Context {
    /// The store, to read.
    fn store(&self) -> &Store;
    /// Stores an image in the buffer in use.
    fn add_image(&mut self, image: NewImage) -> Result<ImageKey, Refusal>;
    /// Places an image of the buffer in use on its screen.
    fn place(&mut self, image: ImageKey, place: Place) -> Result<PlacementKey, Refusal>;
    /// Whether the host has an image helper to answer requests. Without
    /// one, a handler answers what only the helper reads as its protocol
    /// says (D7, "Without a helper").
    fn helper(&self) -> bool;
    /// Asks the image helper; the terminal takes no more input until the
    /// reply is handed back to [`Handler::decoded`].
    fn request(&mut self, request: Request);
    /// The cell size in pixels new placements are made in.
    fn cell_size(&self) -> (u32, u32);
    /// The grid's columns and rows.
    fn grid(&self) -> (usize, usize);
    /// The cursor's column and row.
    fn cursor(&self) -> (usize, usize);
    /// Moves the cursor, within the screen.
    fn move_cursor(&mut self, col: usize, row: usize);
    /// The index the screen does at the bottom margin, which a sixel uses
    /// to scroll.
    fn index(&mut self);
    /// Writes a reply to the program.
    fn reply(&mut self, kind: ReplyKind, bytes: &[u8]);
    fn switches(&self) -> Switches;
    fn limits(&self) -> Limits;
    /// Adds what the handler did to the input call's work (D5).
    fn add_work(&mut self, work: u64);
}

/// One protocol's front end.
pub trait Handler: core::fmt::Debug {
    /// A string begins.
    fn start(&mut self, ctx: &mut dyn Context, intro: Intro<'_>);
    /// Its data, in the pieces the parser hands out as they arrive; an OSC
    /// string comes whole, in one call.
    fn feed(&mut self, ctx: &mut dyn Context, data: Chunk<'_>);
    /// The string ended with its terminator: place the picture, or make a
    /// request, and reply.
    fn finish(&mut self, ctx: &mut dyn Context);
    /// The string was cancelled: drop what it holds and release what it
    /// reserved, placing and answering nothing.
    fn cancel(&mut self, ctx: &mut dyn Context);
    /// The helper's answer to the handler's request.
    fn decoded(&mut self, ctx: &mut dyn Context, reply: Reply);
    /// A copy of the handler, for a terminal that is cloned.
    fn clone_box(&self) -> Box<dyn Handler>;
}

impl Clone for Box<dyn Handler> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
