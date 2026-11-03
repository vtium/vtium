//! The sixel handler, which takes DCS `q` strings (ADR-1100, D3 and D6).
//!
//! A string's data streams into the [decoder](crate::decode::sixel) as it
//! arrives, which reserves the picture's memory from the console's budget
//! as it grows. When the string ends with its terminator, the picture is
//! stored with that reservation and placed:
//!
//! - **With mode 80 (DECSDM) reset,** sixel scrolling is on: the
//!   picture's top left is the cursor's cell, the scroll region scrolls
//!   for a picture that would pass the bottom margin, and the cursor ends
//!   on the row holding the picture's last pixel row, as
//!   [`place_scrolling`] describes, in the column after the picture's
//!   right edge with mode 8452 set.
//! - **With mode 80 set,** the picture sits at the screen's top left, cut
//!   to the screen, and the cursor stays where it is.
//!
//! Color registers are the picture's own with mode 1070 set, each picture
//! starting from the defaults, or shared between pictures with it reset,
//! kept from one picture to the next until RIS or a change of the register
//! count. A string that is cancelled, or that ends while sixel is not
//! offered, stores nothing and gives back what it reserved.
//!
//! Sixel answers nothing, so a picture that ends with no size, or that the
//! budget or the store refuses, is dropped without a word to the program,
//! and the handler notes why for vtium's log ([`Context::note`]), as the C
//! reference logged a sixel picture it did not store or place at debug
//! level (libtsm `eeaf293`, `src/tsm/tsm-vte-image.c:161-171`).

use vtium_parse::Chunk;

use crate::bounds::IMAGE_SIDE_MAX;
use crate::decode::Reply;
use crate::decode::sixel::{Budget, Decoder, Empty, Palette, Picture};
use crate::handler::{Context, Handler, Intro};
use crate::image::{ImageKey, NewImage, Place};
use crate::protocol::{chunk_work, place_scrolling};
use crate::rules::Protocol;
use crate::store::Reservation;

/// `max-graphic-size` when the host gives none, as the C reference's
/// `TSM_SIXEL_MAX_SIZE_DEFAULT`.
const GRAPHIC_DEFAULT: u32 = 10_000;

/// The sixel protocol's front end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sixel {
    /// The picture being decoded.
    picture: Option<Decoder>,
    /// Whether that picture draws with the shared registers, which it
    /// gives back when it is done.
    shared_in_use: bool,
    /// The registers pictures share while mode 1070 is reset; made at the
    /// first picture that needs them.
    shared: Option<Palette>,
    /// The bytes of the budget the picture in progress holds.
    held: Option<Reservation>,
}

/// The console's budget, as the decoder asks it: one reservation that
/// grows as the picture does. A shrink keeps the reservation as it is,
/// since the store takes back what a picture does not need when it stores
/// it, and a shrink to nothing gives it all back.
struct Hold<'a> {
    ctx: &'a mut dyn Context,
    held: &'a mut Option<Reservation>,
}

impl Budget for Hold<'_> {
    fn resize(&mut self, _old: u64, new: u64) -> bool {
        if new == 0 {
            if let Some(r) = self.held.take() {
                self.ctx.unreserve(r);
            }
            return true;
        }
        let have = self.held.as_ref().map_or(0, Reservation::bytes);
        if new <= have {
            return true;
        }
        let more = new.saturating_sub(have);
        match self.held {
            Some(r) => self.ctx.grow(r, more).is_ok(),
            None => match self.ctx.reserve(Protocol::SIXEL, 0, more) {
                Ok(r) => {
                    *self.held = Some(r);
                    true
                }
                Err(_) => false,
            },
        }
    }
}

impl Sixel {
    /// Ends the picture in progress, giving its registers back to the
    /// shared set when they were the shared ones, and returns it.
    fn take_picture(&mut self) -> Option<Decoder> {
        let mut picture = self.picture.take()?;
        if core::mem::take(&mut self.shared_in_use) {
            self.shared = Some(picture.take_palette());
        }
        Some(picture)
    }

    /// Drops the picture in progress and what it reserved.
    fn drop_picture(&mut self, ctx: &mut dyn Context) {
        if let Some(mut picture) = self.take_picture() {
            let mut hold = Hold {
                ctx: &mut *ctx,
                held: &mut self.held,
            };
            picture.abort(&mut hold);
        }
        if let Some(r) = self.held.take() {
            ctx.unreserve(r);
        }
    }
}

/// A picture's place at the cursor, and the cursor after it. Returns
/// whether the picture was placed; the store drops one it refuses.
fn place(ctx: &mut dyn Context, image: ImageKey, width: u32, height: u32) -> bool {
    let state = ctx.sixel();
    let (cols, rows) = ctx.grid();
    let (cw, ch) = ctx.cell_size();
    let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    if state.display {
        // DECSDM: the screen's top left, cut to the screen
        let at = Place {
            src_width: width.min(to_u32(cols).saturating_mul(cw)),
            src_height: height.min(to_u32(rows).saturating_mul(ch)),
            ..Place::default()
        };
        if ctx.place(image, at).is_err() {
            ctx.drop_image(image);
            return false;
        }
        return true;
    }
    let tall = usize::try_from(height.div_ceil(ch.max(1))).unwrap_or(usize::MAX);
    place_scrolling(ctx, image, None, tall, state.cursor_right)
}

impl Handler for Sixel {
    fn start(&mut self, ctx: &mut dyn Context, intro: Intro<'_>) {
        self.drop_picture(ctx);
        // P1, the pixel aspect, is read and not applied (D3); P2 1 leaves
        // the pixels no dot sets clear; P3, the grid size, is ignored, as
        // xterm ignores it
        let transparent = match intro {
            Intro::Dcs(params) => params.get(1) == Some(1),
            Intro::String => false,
        };
        let state = ctx.sixel();
        let palette = if state.private_registers {
            Palette::new(state.registers)
        } else {
            // the terminal resets the handler, dropping them, whenever the
            // count in force or the host's settings change
            let shared = self.shared.take();
            self.shared_in_use = true;
            shared.unwrap_or_else(|| Palette::new(state.registers))
        };
        let side = |s: u32| {
            if s == 0 {
                GRAPHIC_DEFAULT
            } else {
                s.min(u32::try_from(IMAGE_SIDE_MAX).unwrap_or(u32::MAX))
            }
        };
        let (w, h) = ctx.limits().graphic;
        self.picture = Some(Decoder::new((side(w), side(h)), palette, transparent));
    }

    fn feed(&mut self, ctx: &mut dyn Context, data: Chunk<'_>) {
        let Some(picture) = self.picture.as_mut() else {
            ctx.add_work(chunk_work(data));
            return;
        };
        let before = picture.work();
        match data {
            Chunk::Bytes(bytes) => {
                let mut hold = Hold {
                    ctx: &mut *ctx,
                    held: &mut self.held,
                };
                picture.feed(bytes, &mut hold);
            }
            // the data is ASCII; a character past it is ignored, as the
            // decoder ignores the bytes it has no use for
            Chunk::Char(_) => ctx.add_work(1),
        }
        ctx.add_work(picture.work().saturating_sub(before));
    }

    fn feed_some(&mut self, ctx: &mut dyn Context, run: &[u8], work_left: u64) -> usize {
        let Some(picture) = self.picture.as_mut() else {
            ctx.add_work(chunk_work(Chunk::Bytes(run)));
            return run.len();
        };
        let before = picture.work();
        let mut hold = Hold {
            ctx: &mut *ctx,
            held: &mut self.held,
        };
        let taken = picture.feed_some(run, &mut hold, before.saturating_add(work_left));
        ctx.add_work(picture.work().saturating_sub(before));
        taken
    }

    fn finish(&mut self, ctx: &mut dyn Context) {
        let Some(mut picture) = self.take_picture() else {
            return;
        };
        let before = picture.work();
        let done = {
            let mut hold = Hold {
                ctx: &mut *ctx,
                held: &mut self.held,
            };
            picture.finish(&mut hold)
        };
        ctx.add_work(picture.work().saturating_sub(before));
        let Picture {
            width,
            height,
            pixels,
            transparent,
        } = match done {
            Ok(picture) => picture,
            Err(empty) => {
                // the decoder gave back what it held
                if let Some(r) = self.held.take() {
                    ctx.unreserve(r);
                }
                match empty {
                    Empty::NoData => ctx.note("a sixel picture with no size"),
                    Empty::Refused => ctx.note("the budget refuses a sixel picture"),
                    Empty::Idle => {}
                }
                return;
            }
        };
        let image = NewImage {
            protocol: Protocol::SIXEL,
            width,
            height,
            pixels,
            id: 0,
            number: 0,
            transient: false,
            // register 0 filled every pixel no dot set, and registers are
            // opaque; with P2 1 the store reads the pixels to find out
            opaque: (!transparent).then_some(true),
        };
        let Ok(key) = ctx.add_image(image, self.held.take()) else {
            return ctx.note("the store refuses a sixel picture");
        };
        if !place(ctx, key, width, height) {
            ctx.note("the store refuses a sixel picture's place");
        }
    }

    fn cancel(&mut self, ctx: &mut dyn Context) {
        self.drop_picture(ctx);
    }

    fn decoded(&mut self, _: &mut dyn Context, _: Reply) {}

    /// RIS drops the shared registers, so the next picture starts from the
    /// defaults with the register count in force then.
    fn reset(&mut self, ctx: &mut dyn Context) {
        self.drop_picture(ctx);
        self.shared = None;
    }

    fn clone_box(&self) -> Box<dyn Handler> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn same(&self, other: &dyn Handler) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}
