//! The handler trait the protocol front ends implement, and the context
//! the terminal gives them (ADR-1100, D6).
//!
//! A handler turns one protocol's control strings into store and screen
//! changes. vtium-term's router calls it as the string arrives: `start`
//! with the introducer, `feed` with the data, then `finish` when the
//! string ended with its terminator or `cancel` when it was cancelled. A
//! handler that asked the helper for something (D7) gets the answer in
//! `decoded`, and RIS reaches it in `reset`. Every method gets the
//! [`Context`] vtium-term implements; a handler never reaches the screen's
//! lines itself, so every scroll it causes passes through the screen and
//! the store's events, and it changes the store only through an [`Edit`].

use vtium_parse::{Chunk, Params};

use crate::decode::{Reply, Request};
use crate::image::{ImageKey, NewImage, Place, PlacementKey, Refusal};
use crate::rules::Protocol;
use crate::store::{Edit, Reservation, Store};

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

/// The scroll region's four margins, each included: its first and last
/// rows, as DECSTBM set them, and its first and last columns, as DECSLRM
/// set them, which are the screen's first and last columns while DECLRMM
/// is reset (ADR-1100, D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Margins {
    pub top: usize,
    pub bottom: usize,
    pub left: usize,
    pub right: usize,
}

impl Margins {
    /// Whether column `x` lies between the left and right margins, where a
    /// line feed at the bottom margin scrolls the region.
    pub fn holds_column(&self, x: usize) -> bool {
        x >= self.left && x <= self.right
    }

    /// The column CUF stops at from column `x` on a screen `cols` wide: the
    /// right margin, or the screen's last column when `x` is past the
    /// right margin.
    pub fn right_stop(&self, x: usize, cols: usize) -> usize {
        if x > self.right {
            cols.saturating_sub(1)
        } else {
            self.right
        }
    }

    /// The column CR goes to from column `x`: the left margin, or the
    /// screen's first column when `x` is left of the left margin.
    pub fn line_start(&self, x: usize) -> usize {
        if x >= self.left { self.left } else { 0 }
    }
}

/// The sixel state a handler reads, which vtium-term holds (D1): the
/// three sixel modes in its mode table, and the color register count in
/// force.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SixelState {
    /// Mode 80, DECSDM, is set: sixel scrolling is off, so a picture sits
    /// at the screen's top left, cut to the screen, and the cursor stays
    /// where it is. Reset, the picture's top left is the cursor's cell.
    pub display: bool,
    /// Mode 1070 is set: each picture gets its own color registers, where
    /// reset the pictures share one set. Its state at reset is
    /// [`Limits::private_registers`].
    pub private_registers: bool,
    /// Mode 8452 is set: after a picture the cursor goes to the column
    /// after its right edge, where reset it goes to the picture's left
    /// column.
    pub cursor_right: bool,
    /// How many color registers a new picture gets: the count XTSMGRAPHICS
    /// set, or [`Limits::registers`] when it set none.
    pub registers: u32,
}

/// The terminal's colors, which a palette-index picture (kitty's `f=8` and
/// `f=16`) takes when it arrives (ADR-1100, D15). Each color is R, G, B.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Colors {
    /// The 256-color table that SGR 38;5 reads.
    pub table: [[u8; 3]; 256],
    /// The default foreground.
    pub fg: [u8; 3],
    /// The default background.
    pub bg: [u8; 3],
    /// The selection's background, which OSC 17 sets, or `None` while
    /// none is set and vtium draws the selection inverted.
    pub selection_bg: Option<[u8; 3]>,
    /// The selection's foreground, which OSC 19 sets, or `None` while none
    /// is set and the selection takes its text color from the cell.
    pub selection_fg: Option<[u8; 3]>,
    /// The cursor's color, which OSC 12 sets, or `None` while none is set
    /// and vtium draws the cursor inverted.
    pub cursor: Option<[u8; 3]>,
}

impl Colors {
    /// The color palette index `index` names, as kitty's specification
    /// gives it: 0 to 247 the table, 248 the default background, 249 the
    /// default foreground, 250 and 251 the selection's background and
    /// foreground, 252 the cursor's color, 253 the color of the text under
    /// the cursor, 254 the color of a link under the pointer, and 255 the
    /// default foreground. A color vtium takes from the cell, because it
    /// is not set or because vtium has none of its own, is the default
    /// foreground for 250 and 252 and the default background for 251 and
    /// 253, as the specification asks. vtium has no link color, so 254 is
    /// the default foreground.
    pub fn index(&self, index: u8) -> [u8; 3] {
        match index {
            0..=247 => self
                .table
                .get(usize::from(index))
                .copied()
                .unwrap_or(self.fg),
            248 | 253 => self.bg,
            250 => self.selection_bg.unwrap_or(self.fg),
            251 => self.selection_fg.unwrap_or(self.bg),
            252 => self.cursor.unwrap_or(self.fg),
            249 | 254 | 255 => self.fg,
        }
    }

    /// The color of every index, in index order.
    pub fn resolved(&self) -> [[u8; 3]; 256] {
        core::array::from_fn(|i| self.index(u8::try_from(i).unwrap_or(u8::MAX)))
    }
}

/// Who can act on a note's reason, which sets how loudly vtium logs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// What the program sent breaks its protocol, such as an iTerm2
    /// string without `inline=1`. Only the program's author can act on it,
    /// so vtium logs it at debug level.
    Program,
    /// The console refused a picture the program sent as its protocol
    /// asks: a setting, one of vtium's bounds, the image budget, memory or
    /// the image helper. The picture never draws, and the operator can act
    /// on it, so vtium logs it at info level, which a default build shows.
    Console,
}

/// What a handler reaches, which vtium-term implements over one terminal.
///
/// A handler changes the store only through
/// [`edit_store`](Self::edit_store), which it usually calls through
/// `dyn Context`'s `edit` method and the shorthand methods beside it. It
/// reaches the screen only through the cursor, the index and the scroll
/// here, so every scroll it causes passes through the screen and the
/// store's events.
pub trait Context {
    /// The store, to read.
    fn store(&self) -> &Store;
    /// Runs `f` once over an [`Edit`] of the store, which binds both
    /// buffers' screen rows, then settles the store and marks the cells
    /// under every box the edit changed, as a screen event does.
    fn edit_store(&mut self, f: &mut dyn FnMut(&mut Edit<'_>));
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
    /// The cursor's column and row. The column equals the grid's width
    /// while a wrap is pending.
    fn cursor(&self) -> (usize, usize);
    /// Moves the cursor, within the screen.
    fn move_cursor(&mut self, col: usize, row: usize);
    /// The scroll region's margins.
    fn margins(&self) -> Margins;
    /// Moves the cursor `n` rows down as `n` line feeds do, scrolling the
    /// region up at the bottom margin by what is left.
    fn index(&mut self, n: usize);
    /// Scrolls the region up `n` rows, as SU does, leaving the cursor
    /// where it is.
    fn scroll_up(&mut self, n: usize);
    /// The sixel modes and the register count.
    fn sixel(&self) -> SixelState;
    /// Writes a reply to the program.
    fn reply(&mut self, kind: ReplyKind, bytes: &[u8]);
    fn switches(&self) -> Switches;
    fn limits(&self) -> Limits;
    /// Adds what the handler did to the input call's work (D5).
    fn add_work(&mut self, work: u64);
    /// The terminal's colors as they are now, which a palette-index
    /// picture takes when it arrives (ADR-1100, D15): the palette the
    /// frames draw through, with the colors a program has set. The default
    /// gives none, and a handler then refuses palette-index pictures.
    fn colors(&self) -> Option<Colors> {
        None
    }
    /// Says why a handler dropped or ignored what a program sent, for
    /// vtium's log, at the level its [`Cause`] gives (ADR-1103, D1). `why`
    /// is a fixed phrase, so that each repeat of a reason reads the same:
    /// vtium-term counts each protocol's reasons and hands the host a note
    /// the first time a reason comes and each time its count reaches a
    /// power of ten. The default keeps nothing.
    fn note(&mut self, cause: Cause, why: &str) {
        let _ = (cause, why);
    }
}

/// The store edits a handler makes, each through one
/// [`edit_store`](Context::edit_store).
impl dyn Context + '_ {
    /// Runs `f` over an [`Edit`] of the store and returns what it returned,
    /// or `None` when the context did not run it. vtium-term's context
    /// always runs it, so there the result is never `None`.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Edit<'_>) -> R) -> Option<R> {
        let mut f = Some(f);
        let mut out = None;
        self.edit_store(&mut |edit| {
            if let Some(f) = f.take() {
                out = Some(f(edit));
            }
        });
        out
    }

    /// [`Edit::reserve`].
    pub fn reserve(
        &mut self,
        protocol: Protocol,
        id: u32,
        bytes: u64,
    ) -> Result<Reservation, Refusal> {
        self.edit(|e| e.reserve(protocol, id, bytes))
            .unwrap_or(Err(Refusal::NoSpace))
    }

    /// [`Edit::grow`].
    pub fn grow(&mut self, reservation: &mut Reservation, more: u64) -> Result<(), Refusal> {
        self.edit(|e| e.grow(reservation, more))
            .unwrap_or(Err(Refusal::NoSpace))
    }

    /// [`Edit::unreserve`].
    pub fn unreserve(&mut self, reservation: Reservation) {
        self.edit(|e| e.unreserve(reservation));
    }

    /// [`Edit::add_image`].
    pub fn add_image(
        &mut self,
        image: NewImage,
        reservation: Option<Reservation>,
    ) -> Result<ImageKey, Refusal> {
        self.edit(|e| e.add_image(image, reservation))
            .unwrap_or(Err(Refusal::NoSpace))
    }

    /// [`Edit::drop_image`].
    pub fn drop_image(&mut self, image: ImageKey) {
        self.edit(|e| e.drop_image(image));
    }

    /// [`Edit::find_image`].
    pub fn find_image(&mut self, id: u32, number: u32) -> Option<ImageKey> {
        self.edit(|e| e.find_image(id, number)).flatten()
    }

    /// [`Edit::touch`].
    pub fn touch(&mut self, image: ImageKey) {
        self.edit(|e| e.touch(image));
    }

    /// [`Edit::free_id`].
    pub fn free_id(&mut self) -> u32 {
        self.edit(|e| e.free_id()).unwrap_or(1)
    }

    /// [`Edit::place`].
    pub fn place(&mut self, image: ImageKey, place: Place) -> Result<PlacementKey, Refusal> {
        self.edit(|e| e.place(image, place))
            .unwrap_or(Err(Refusal::NoSpace))
    }
}

/// One protocol's front end.
pub trait Handler: core::fmt::Debug {
    /// A string begins.
    fn start(&mut self, ctx: &mut dyn Context, intro: Intro<'_>);
    /// Its data, in the pieces the parser hands out as they arrive; an OSC
    /// string comes whole, in one call.
    fn feed(&mut self, ctx: &mut dyn Context, data: Chunk<'_>);
    /// A run of a DCS string's data, of which the handler may take only the
    /// first bytes, as many as do `work_left` of work or the byte that
    /// passes it (D5), and returns how many it took, at least one of a
    /// run that is not empty. The input call stops there, once the work
    /// reaches its bound, and the rest of the run comes in the next call.
    /// The default takes the whole run through [`feed`](Self::feed), so a
    /// handler that decodes little for each byte need not implement it.
    fn feed_some(&mut self, ctx: &mut dyn Context, run: &[u8], work_left: u64) -> usize {
        let _ = work_left;
        self.feed(ctx, Chunk::Bytes(run));
        run.len()
    }
    /// The string ended with its terminator: place the picture, or make a
    /// request, and reply.
    fn finish(&mut self, ctx: &mut dyn Context);
    /// The string was cancelled: drop what it holds and release what it
    /// reserved, placing and answering nothing.
    fn cancel(&mut self, ctx: &mut dyn Context);
    /// The helper's answer to the handler's request.
    fn decoded(&mut self, ctx: &mut dyn Context, reply: Reply);
    /// RIS: the handler returns to its state at start, giving back what it
    /// reserved. It runs before the store empties both buffers. A string
    /// it was being fed has already been cancelled.
    fn reset(&mut self, ctx: &mut dyn Context) {
        let _ = ctx;
    }
    /// The protocol is no longer offered, or the session ended: the
    /// handler drops what it holds open between its strings, such as a
    /// kitty upload in chunks, gives back what it reserved, and answers
    /// nothing (ADR-1100, D6 and D16).
    fn withdraw(&mut self, ctx: &mut dyn Context) {
        let _ = ctx;
    }
    /// A copy of the handler, for a terminal that is cloned.
    fn clone_box(&self) -> Box<dyn Handler>;
    /// The handler as `Any`, so [`same`](Self::same) can compare it.
    fn as_any(&self) -> &dyn core::any::Any;
    /// Whether `other` is a handler of the same type in the same state, so
    /// two terminals compare by what their handlers hold, a string half
    /// fed among it. An implementation reads
    /// `other.as_any().downcast_ref::<Self>() == Some(self)`.
    fn same(&self, other: &dyn Handler) -> bool;
}

impl Clone for Box<dyn Handler> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
