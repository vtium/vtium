//! The iTerm2 compatibility handler, which takes iTerm2's inline pictures,
//! OSC `1337;File=` and its transfer in parts (ADR-1103).
//!
//! vtium-term's router hands the handler each string whole, with the
//! `1337;` cut off, in one [`feed`](Handler::feed) between a
//! [`start`](Handler::start) and a [`finish`](Handler::finish). The
//! handler takes four strings:
//!
//! - `File=KEYS:BASE64` is a picture in one string. Its keys are read as
//!   [`keys`] describes; a string with no `:`, or without `inline=1`, is
//!   dropped.
//! - `MultipartFile=KEYS` starts a *transfer in parts*: a picture too large
//!   for one string, sent as a run of `FilePart=BASE64` strings, each a
//!   piece of the base64 file, and ended by `FileEnd`. A part need not hold
//!   a multiple of 4 base64 characters: the decoder carries up to 3 left
//!   over into the next part. Output between the parts, text included,
//!   leaves the transfer as it is. A `FilePart=` or `FileEnd` with no
//!   transfer in progress is ignored.
//!
//! The handler decodes the base64 into a buffer reserved under the
//! console's image budget as it grows. Once a picture's file is whole, the
//! handler sends it to the image helper as inline bytes with the any-image
//! flag (ADR-1101, D7), and the helper decodes it as PNG, JPEG or GIF by
//! its first bytes. The decoded picture is stored at its own size under the
//! sixel tag, so every sixel rule applies to it, and placed at the cursor
//! as a sixel is with sixel scrolling on ([`place_scrolling`]), sized by
//! its keys. Mode 80 (DECSDM) and mode 8452 are sixel modes and do not
//! apply: an iTerm2 picture always scrolls, and the cursor ends at the
//! picture's left column.
//!
//! The handler drops a transfer in progress, giving its reservation back,
//! on a new `MultipartFile=` or `File=`, which then goes on as a string of
//! its own; on RIS; when its file would pass [`ITERM2_FILE_MAX`] or its
//! parts [`ITERM2_PARTS_MAX`]; when a part's base64 does not decode; when
//! the budget refuses the buffer's growth; and when it is cancelled, which
//! vtium-term does when the iTerm2 switch goes off and when it drops an
//! OSC 1337 string, one past the OSC bound or one a control cut short.
//!
//! iTerm2's protocol answers nothing, so the handler writes no reply: a
//! picture that fails at any step is dropped, and the context notes why.

mod keys;
#[cfg(test)]
mod tests;

use vtium_parse::Chunk;

use self::keys::{Keys, drawn};
use crate::bounds::{IMAGE_OVERHEAD, IMAGE_SIDE_MAX, ITERM2_FILE_MAX, ITERM2_PARTS_MAX};
use crate::decode::base64::{Base64, Sink};
use crate::decode::{Medium, Payload, Reply, Request};
use crate::handler::{Context, Handler, Intro};
use crate::image::NewImage;
use crate::protocol::{chunk_work, place_scrolling};
use crate::rules::Protocol;
use crate::store::Reservation;

/// The first bytes a file's buffer reserves; it doubles from there.
const BUFFER_FIRST: u64 = 4096;

/// The iTerm2 protocol's front end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Iterm2 {
    /// A picture whose file the string just fed made whole, for
    /// [`finish`](Handler::finish) to send to the helper.
    ready: Option<(Keys, File)>,
    /// The transfer in parts in progress.
    transfer: Option<Transfer>,
    /// The keys of the picture whose request is out, and the reservation
    /// its file holds until the answer comes.
    pending: Option<(Keys, Option<Reservation>)>,
}

/// A file's bytes as the base64 decodes, and the reservation that holds
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct File {
    data: Vec<u8>,
    /// The bytes reserved, which the buffer grows within.
    cap: u64,
    reservation: Option<Reservation>,
}

impl File {
    fn release(self, ctx: &mut dyn Context) {
        if let Some(r) = self.reservation {
            ctx.unreserve(r);
        }
    }
}

/// A transfer in parts.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Transfer {
    keys: Keys,
    base64: Base64,
    file: File,
    parts: u64,
}

/// The base64 decoder's sink: a file growing under the budget and
/// [`ITERM2_FILE_MAX`], which fails once and takes nothing more.
struct Fill<'a> {
    ctx: &'a mut dyn Context,
    file: &'a mut File,
    failed: Option<&'static str>,
}

impl Sink for Fill<'_> {
    fn open(&self) -> bool {
        self.failed.is_none()
    }

    fn bytes(&mut self, data: &[u8]) {
        let len = u64::try_from(self.file.data.len()).unwrap_or(u64::MAX);
        let need = len.saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
        if need > ITERM2_FILE_MAX {
            self.failed = Some("the file passes ITERM2_FILE_MAX");
            return;
        }
        if need > self.file.cap {
            let mut grown = self.file.cap.max(BUFFER_FIRST);
            while grown < need {
                grown = grown.saturating_mul(2);
            }
            let grown = grown.min(ITERM2_FILE_MAX);
            let more = grown.saturating_sub(self.file.cap);
            let ok = match &mut self.file.reservation {
                Some(r) => self.ctx.grow(r, more).is_ok(),
                None => match self.ctx.reserve(Protocol::SIXEL, 0, more) {
                    Ok(r) => {
                        self.file.reservation = Some(r);
                        true
                    }
                    Err(_) => false,
                },
            };
            if !ok {
                self.failed = Some("the budget refuses the file's buffer");
                return;
            }
            self.file.cap = grown;
            let room = usize::try_from(more).unwrap_or(0);
            if self.file.data.try_reserve_exact(room).is_err() {
                self.failed = Some("memory refuses the file's buffer");
                return;
            }
        }
        self.file.data.extend_from_slice(data);
    }

    fn error(&mut self) {
        self.failed = Some("base64 that does not decode");
    }
}

/// Decodes `text` into `file` through `base64`, ending the base64 too when
/// `last`; why it failed, if it did.
fn fill(
    ctx: &mut dyn Context,
    base64: &mut Base64,
    file: &mut File,
    text: &[u8],
    last: bool,
) -> Result<(), &'static str> {
    let mut sink = Fill {
        ctx,
        file,
        failed: None,
    };
    base64.feed(text, &mut sink);
    if last {
        base64.flush(&mut sink);
    }
    sink.failed.map_or(Ok(()), Err)
}

/// The keys of `text`, up to its first `:`, and what follows the `:`.
fn split_keys(text: &[u8]) -> (&[u8], Option<&[u8]>) {
    match text.iter().position(|&b| b == b':') {
        Some(at) => (
            text.get(..at).unwrap_or_default(),
            text.get(at.saturating_add(1)..),
        ),
        None => (text, None),
    }
}

/// `max-graphic-size`, bounded by the store's largest side.
fn graphic(ctx: &dyn Context) -> (u32, u32) {
    let side = u32::try_from(IMAGE_SIDE_MAX).unwrap_or(u32::MAX);
    let (w, h) = ctx.limits().graphic;
    (w.min(side), h.min(side))
}

impl Iterm2 {
    /// Drops the transfer in progress and gives its reservation back.
    fn drop_transfer(&mut self, ctx: &mut dyn Context) {
        if let Some(t) = self.transfer.take() {
            t.file.release(ctx);
        }
    }

    /// Drops everything the handler holds: the picture ready, the transfer
    /// and the request out, whose answer is then ignored.
    fn drop_all(&mut self, ctx: &mut dyn Context) {
        if let Some((_, file)) = self.ready.take() {
            file.release(ctx);
        }
        self.drop_transfer(ctx);
        if let Some((_, Some(r))) = self.pending.take() {
            ctx.unreserve(r);
        }
    }

    /// `File=KEYS:BASE64`.
    fn file(&mut self, ctx: &mut dyn Context, text: &[u8]) {
        self.drop_transfer(ctx);
        let (keys, base64) = split_keys(text);
        let Some(base64) = base64 else {
            return ctx.note("an iTerm2 File= string with no ':'");
        };
        let keys = Keys::parse(keys);
        if !keys.inline {
            return ctx.note("an iTerm2 picture without inline=1");
        }
        let mut file = File::default();
        match fill(ctx, &mut Base64::new(), &mut file, base64, true) {
            Ok(()) => self.ready = Some((keys, file)),
            Err(why) => {
                file.release(ctx);
                ctx.note(why);
            }
        }
    }

    /// `MultipartFile=KEYS`.
    fn multipart(&mut self, ctx: &mut dyn Context, text: &[u8]) {
        self.drop_transfer(ctx);
        let keys = Keys::parse(split_keys(text).0);
        if !keys.inline {
            return ctx.note("an iTerm2 picture without inline=1");
        }
        self.transfer = Some(Transfer {
            keys,
            base64: Base64::new(),
            file: File::default(),
            parts: 0,
        });
    }

    /// `FilePart=BASE64`, or with `last` the `FileEnd` that ends the
    /// transfer.
    fn part(&mut self, ctx: &mut dyn Context, text: &[u8], last: bool) {
        let Some(t) = self.transfer.as_mut() else {
            return;
        };
        if !last {
            t.parts = t.parts.saturating_add(1);
            if t.parts > ITERM2_PARTS_MAX {
                self.drop_transfer(ctx);
                return ctx.note("an iTerm2 transfer passes ITERM2_PARTS_MAX");
            }
        }
        if let Err(why) = fill(ctx, &mut t.base64, &mut t.file, text, last) {
            self.drop_transfer(ctx);
            return ctx.note(why);
        }
        if last && let Some(t) = self.transfer.take() {
            self.ready = Some((t.keys, t.file));
        }
    }

    /// The decoded picture `payload` of `size` pixels, stored under the
    /// sixel tag and placed at the cursor as `keys` size it.
    fn store(ctx: &mut dyn Context, keys: &Keys, payload: &Payload, size: (u32, u32)) {
        // max-graphic-size may have been lowered since the request
        let (max_w, max_h) = graphic(ctx);
        if size.0 > max_w || size.1 > max_h {
            return ctx.note("an iTerm2 picture past max-graphic-size");
        }
        let count = u64::from(size.0).saturating_mul(u64::from(size.1));
        let charge = count.saturating_mul(4).saturating_add(IMAGE_OVERHEAD);
        // the budget refuses before a byte of the picture is copied
        let Ok(reservation) = ctx.reserve(Protocol::SIXEL, 0, charge) else {
            return ctx.note("the budget refuses an iTerm2 picture");
        };
        let Some((pixels, opaque)) = payload.copy_into(Vec::new()) else {
            ctx.unreserve(reservation);
            return ctx.note("an iTerm2 picture's reply file does not read whole");
        };
        ctx.add_work(count);
        let image = NewImage {
            protocol: Protocol::SIXEL,
            width: size.0,
            height: size.1,
            pixels,
            id: 0,
            number: 0,
            transient: false,
            opaque: Some(opaque),
        };
        let Ok(key) = ctx.add_image(image, Some(reservation)) else {
            return ctx.note("the store refuses an iTerm2 picture");
        };
        let cell = ctx.cell_size();
        let (dw, dh) = drawn(keys, size, cell, ctx.grid());
        let (Ok(draw_width), Ok(draw_height)) = (u32::try_from(dw), u32::try_from(dh)) else {
            ctx.drop_image(key);
            return ctx.note("an iTerm2 picture's box passes BOX_CELLS_MAX");
        };
        let tall = usize::try_from(dh.div_ceil(u64::from(cell.1.max(1)))).unwrap_or(usize::MAX);
        place_scrolling(ctx, key, Some((draw_width, draw_height)), tall, false);
    }
}

impl Handler for Iterm2 {
    fn start(&mut self, ctx: &mut dyn Context, _: Intro<'_>) {
        if let Some((_, file)) = self.ready.take() {
            file.release(ctx);
        }
    }

    fn feed(&mut self, ctx: &mut dyn Context, data: Chunk<'_>) {
        ctx.add_work(chunk_work(data));
        let Chunk::Bytes(text) = data else {
            return;
        };
        if let Some(rest) = text.strip_prefix(b"File=") {
            self.file(ctx, rest);
        } else if let Some(rest) = text.strip_prefix(b"MultipartFile=") {
            self.multipart(ctx, rest);
        } else if let Some(rest) = text.strip_prefix(b"FilePart=") {
            self.part(ctx, rest, false);
        } else if text == b"FileEnd" {
            self.part(ctx, &[], true);
        }
    }

    fn finish(&mut self, ctx: &mut dyn Context) {
        let Some((keys, file)) = self.ready.take() else {
            return;
        };
        if !ctx.helper() {
            file.release(ctx);
            return ctx.note("no image helper decodes an iTerm2 picture");
        }
        let File {
            data, reservation, ..
        } = file;
        let request = Request {
            medium: Medium::Direct(data),
            offset: 0,
            max: 0,
            min: 0,
            inflate: false,
            png: false,
            exact: false,
            discard: false,
            any_image: true,
            frames: None,
            pixels: None,
            largest: graphic(ctx),
            largest_bytes: ctx.store().limit().saturating_sub(IMAGE_OVERHEAD),
        };
        self.pending = Some((keys, reservation));
        ctx.request(request);
    }

    fn cancel(&mut self, ctx: &mut dyn Context) {
        self.drop_all(ctx);
    }

    fn decoded(&mut self, ctx: &mut dyn Context, reply: Reply) {
        let Some((keys, held)) = self.pending.take() else {
            return;
        };
        if let Some(r) = held {
            ctx.unreserve(r);
        }
        match reply {
            Reply::Ok(payload) => match payload.size() {
                Some(size) => Self::store(ctx, &keys, &payload, size),
                None => ctx.note("an iTerm2 picture answered with no picture"),
            },
            _ => ctx.note("the image helper does not decode an iTerm2 picture"),
        }
    }

    fn reset(&mut self, ctx: &mut dyn Context) {
        self.drop_all(ctx);
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
