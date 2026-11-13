//! The kitty graphics protocol's handler: APC `G` strings, read as they
//! stream in (ADR-1100, D6).
//!
//! A string is `G`, then control keys, `KEY=VALUE` pairs separated by
//! commas, then `;` and a base64 payload. The handler reads the keys
//! first, at most [`KEYS_MAX`] bytes of them, and decides from them what
//! the string is before a byte of the payload arrives:
//!
//! - **A direct transmission** (`a=t`, `a=T`, `a=f`, or no `a`, sent
//!   directly) or a query (`a=q`) decodes its payload into a transfer
//!   ([`upload`]) as the pieces arrive. Raw pixels are converted at once,
//!   and a PNG or zlib payload is kept for the image helper under the
//!   budget. When the string ends, the handler stores the picture or the
//!   frame, or asks the helper, and answers.
//! - **A transmission from a file, a temporary file or shared memory**
//!   decodes its payload into the object's name, which the helper opens.
//! - **A put, a delete, an animation control or a composition** acts
//!   when the string ends, and carries no data.
//! - **A feature query**, a query with `e` ([`feature`]), is answered when
//!   the string ends. `e` on any other action is refused.
//!
//! # Uploads in chunks
//!
//! A program may split a picture's base64 into chunks, each a command of
//! its own. The first carries the keys and `m=1`, and the last carries
//! `m=0` or no `m`. Each buffer, the main screen and the alternate screen,
//! holds at most one open upload. A command reaches the upload of the
//! buffer in use when it arrives, so a switch of screens leaves each
//! upload as it was. While an upload is open:
//!
//! - every direct transmission is its next chunk, whatever keys it
//!   carries: the handler takes its data, its `m`, and a `q` other than 0,
//!   and ignores the rest;
//! - a delete, and a transmission from a file, a temporary file or shared
//!   memory, end it with no answer, and then act on their own;
//! - a put, a composition, an animation control, a query and a feature
//!   query act on their own and leave it open, as does any output that is
//!   not a kitty graphics command, a sixel and an iTerm2 picture among it;
//! - RIS ends it, and so does the end of the session, with no answer.
//!
//! The first chunk reserves the upload's whole cost from the budget beside
//! the image of the same id, which stays until the upload is accepted, so
//! a put during the upload finds the old image. A first chunk refused by
//! its keys, or by the budget, is answered at once and opens nothing. An
//! upload's data is capped at its reservation, and a chunk that passes the
//! cap fails the upload with EINVAL at once. Otherwise the upload gets one
//! answer, at its last chunk, built from the first chunk's keys and the
//! data of every chunk, and `q=2` silences it.
//!
//! References: ADR-1100 D2, D5, D6, D7, D14, D15, D16; ADR-1102 D1, D2.

use vtium_parse::Chunk;

use crate::bounds::IMAGE_OVERHEAD;
use crate::decode::base64::{Base64, Sink};
use crate::decode::convert::{Alpha, Layout};
use crate::decode::raw::{Format, Raw};
use crate::decode::{Description, Medium, Payload, Reply, Request};
use crate::handler::{Cause, Context, Handler, Intro};
use crate::image::{ImageKey, NewImage, Place, Refusal};
use crate::protocol::chunk_work;
use crate::rules::Protocol;
use crate::store::kitty::{Delete, Select};
use crate::store::{Compose, Control, Link, NewFrame, Placing, Reservation, Store, layer_cost};

mod command;
mod feature;
#[cfg(test)]
mod tests;
mod upload;

use command::{Answer, Command, Error, charge, graphic, reply};
use upload::{Data, Transfer, TransferSink};

/// The most bytes of control keys a command may carry; the handler
/// ignores a string whose keys pass it. A command that gives every key
/// kitty's spec names fits in far fewer bytes.
pub const KEYS_MAX: usize = 1024;

/// The most bytes a file, temporary-file or shared-memory name may decode
/// to, the same bound as the image helper's [`crate::decode::wire::NAME_MAX`].
pub const NAME_MAX: usize = 4095;

/// The first size of the buffer a PNG or zlib payload is kept in; it
/// doubles as it fills.
const HOST_FIRST: u64 = 4096;

/// The note for a transmission's bytes past the pixels its keys call for.
const PAST: &str = "a kitty upload's data past the pixels s and v call for, ignored";

/// The note for a payload or a file that only the image helper reads,
/// while no helper runs.
const NO_HELPER: &str = "no image helper reads a kitty picture's file, PNG or zlib payload";

/// The buffers an upload can be open in: the main screen, then the
/// alternate screen.
const BUFFERS: usize = 2;

/// Where the string is in its reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    /// No string is being read.
    #[default]
    Idle,
    /// The control keys, up to the `;`.
    Keys,
    /// The payload, which goes where the string's [`Role`] says.
    Payload,
    /// The keys passed [`KEYS_MAX`]: the whole string is ignored.
    Ignored,
}

/// What the string being read is, decided from its keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Role {
    /// It reads no payload: a put, a delete, an animation control, a
    /// composition, a feature query, a query without `i`, or a command its
    /// keys refuse, which is answered when the string ends.
    #[default]
    Plain,
    /// Its payload is data for the transfer the handler holds: a whole
    /// command, the first chunk of an upload, or the next chunk of the
    /// open upload of the buffer in use.
    Direct,
    /// Its payload is the name of a file, a temporary file or a
    /// shared-memory object.
    Named,
}

/// A request out to the helper.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    cmd: Command,
    /// The handler's own refusal of a temporary file or shared-memory
    /// object, which the helper removes and reads nothing of.
    discard: Option<Error>,
    /// The reservation of a buffered payload, given back at the answer.
    held: Option<Reservation>,
}

/// A name's base64 as the decoder's sink.
struct NameSink<'a> {
    name: &'a mut Vec<u8>,
    error: &'a mut Option<Error>,
}

impl Sink for NameSink<'_> {
    fn open(&self) -> bool {
        self.error.is_none()
    }

    fn bytes(&mut self, data: &[u8]) {
        if data.contains(&0) || self.name.len().saturating_add(data.len()) > NAME_MAX {
            *self.error = Some(Error::BadFile);
            return;
        }
        self.name.extend_from_slice(data);
    }

    fn error(&mut self) {
        *self.error = Some(Error::Invalid);
    }
}

/// The kitty graphics protocol's front end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kitty {
    phase: Phase,
    keys: Vec<u8>,
    /// The string's keys, once read.
    cmd: Command,
    role: Role,
    /// The refusal the keys alone decide, answered when the string ends.
    refusal: Option<Error>,
    /// The base64 decoder of the string's payload; each string, each
    /// chunk among them, decodes on its own.
    base64: Base64,
    /// The transfer the string's data goes to.
    transfer: Option<Transfer>,
    /// The string is the next chunk of the open upload of this buffer.
    chunk_of: Option<usize>,
    /// A named medium's name, decoded.
    name: Vec<u8>,
    /// The name failed: too long or holding NUL (EBADF), or not base64
    /// (EINVAL).
    name_error: Option<Error>,
    /// The open upload in chunks of each buffer, main then alternate.
    open: [Option<Transfer>; BUFFERS],
    /// The request out to the helper.
    pending: Option<Pending>,
}

/// The index of the buffer in use, as [`Kitty::open`] holds them.
fn buffer_in_use(ctx: &dyn Context) -> usize {
    usize::from(ctx.store().in_alternate())
}

impl Kitty {
    /// Takes control-key bytes, up to the `;`.
    fn keys(&mut self, ctx: &mut dyn Context, bytes: &[u8]) {
        let (keys, payload) = match bytes.iter().position(|&b| b == b';') {
            Some(at) => (
                bytes.get(..at).unwrap_or_default(),
                bytes.get(at.saturating_add(1)..),
            ),
            None => (bytes, None),
        };
        if self.keys.len().saturating_add(keys.len()) > KEYS_MAX {
            self.phase = Phase::Ignored;
            self.keys.clear();
            return;
        }
        self.keys.extend_from_slice(keys);
        if let Some(payload) = payload {
            self.begin(ctx);
            self.phase = Phase::Payload;
            self.payload(ctx, payload);
        }
    }

    /// The keys are read: decides what the string is, and readies where
    /// its payload goes.
    fn begin(&mut self, ctx: &mut dyn Context) {
        let cmd = Command::parse(&self.keys);
        self.cmd = cmd;
        self.role = Role::Plain;
        // a feature query reads no data; e on another action is refused
        if cmd.asks() {
            return;
        }
        let b = buffer_in_use(ctx);
        if cmd.direct()
            && let Some(mut open) = self.open.get_mut(b).and_then(Option::take)
        {
            // the next chunk of the open upload, whatever its other keys
            open.chunk_quiet(cmd.quiet);
            self.transfer = Some(open);
            self.chunk_of = Some(b);
            self.role = Role::Direct;
            return;
        }
        match cmd.action {
            b't' | b'T' | b'q' | b'f' if cmd.medium == b'd' => {
                // a query while an upload is open is answered whole, its m
                // ignored
                let open = self.open.get(b).is_some_and(Option::is_some);
                let chunked = cmd.more != 0 && !(cmd.action == b'q' && open);
                self.transmit(ctx, cmd, chunked);
            }
            b't' | b'T' | b'q' | b'f' => {
                self.role = Role::Named;
                self.name.clear();
                self.name_error = None;
            }
            _ => {}
        }
    }

    /// A direct transmission or query begins: refused by its keys, or a
    /// transfer readied with its reservation. `chunked` opens an upload.
    fn transmit(&mut self, ctx: &mut dyn Context, mut cmd: Command, chunked: bool) {
        if cmd.dropped() {
            return;
        }
        let refusal = refused(ctx, &cmd).or_else(|| {
            (cmd.action == b'f')
                .then(|| frame_target(ctx, &mut cmd).err())
                .flatten()
        });
        if let Some(e) = refusal {
            self.refusal = Some(e);
            return;
        }
        match ready(ctx, &cmd, chunked) {
            Ok((data, reservation, cost)) => {
                self.cmd = cmd;
                self.transfer = Some(Transfer::new(cmd, data, reservation, chunked, cost));
                self.role = Role::Direct;
            }
            Err(e) => self.refusal = Some(e),
        }
    }

    /// Takes payload bytes.
    fn payload(&mut self, ctx: &mut dyn Context, bytes: &[u8]) {
        match self.role {
            Role::Plain => {}
            Role::Direct => {
                if let Some(transfer) = self.transfer.as_mut() {
                    let mut sink = TransferSink { transfer, ctx };
                    self.base64.feed(bytes, &mut sink);
                }
            }
            Role::Named => {
                let mut sink = NameSink {
                    name: &mut self.name,
                    error: &mut self.name_error,
                };
                self.base64.feed(bytes, &mut sink);
            }
        }
    }

    /// The string ended with its terminator: acts on it and answers.
    fn end(&mut self, ctx: &mut dyn Context) {
        match self.phase {
            Phase::Idle | Phase::Ignored => return,
            Phase::Keys => self.begin(ctx),
            Phase::Payload => {}
        }
        let cmd = self.cmd;
        match self.role {
            Role::Direct => self.end_direct(ctx),
            Role::Named => {
                let mut sink = NameSink {
                    name: &mut self.name,
                    error: &mut self.name_error,
                };
                self.base64.flush(&mut sink);
                // a transmission from a named medium ends the open upload,
                // and a query leaves it
                if cmd.action != b'q' {
                    self.end_upload(ctx);
                }
                self.media_finish(ctx);
            }
            Role::Plain => {
                if cmd.asks() {
                    if cmd.action == b'q' {
                        feature::answer(ctx, &cmd, &self.keys);
                    } else {
                        // the specification allows e only in a query
                        reply(ctx, &cmd, cmd.id, Err(Error::Invalid));
                    }
                } else if let Some(e) = self.refusal {
                    reply(ctx, &cmd, cmd.id, Err(e));
                } else {
                    match cmd.action {
                        b'p' => self.put(ctx),
                        b'd' => {
                            // the specification: "the partial upload must be
                            // aborted"
                            self.end_upload(ctx);
                            self.delete(ctx);
                        }
                        b'a' | b'c' => self.animate(ctx),
                        // a query without i reads nothing and is never
                        // answered
                        b'q' => {}
                        _ => reply(ctx, &cmd, cmd.id, Err(Error::Invalid)),
                    }
                }
            }
        }
    }

    /// A direct transmission's string ended: an upload stays open for its
    /// next chunk while the chunk says `m=1`, and otherwise the transfer
    /// ends and is answered.
    fn end_direct(&mut self, ctx: &mut dyn Context) {
        let Some(mut transfer) = self.transfer.take() else {
            return;
        };
        let mut sink = TransferSink {
            transfer: &mut transfer,
            ctx,
        };
        self.base64.end_unpadded(&mut sink);
        if transfer.chunked && !transfer.ended && self.cmd.more != 0 {
            let b = self.chunk_of.unwrap_or_else(|| buffer_in_use(ctx));
            if let Some(slot) = self.open.get_mut(b) {
                *slot = Some(transfer);
            }
            return;
        }
        self.complete(ctx, transfer);
    }

    /// Ends the open upload of the buffer in use, dropping its data with
    /// no answer.
    fn end_upload(&mut self, ctx: &mut dyn Context) {
        let b = buffer_in_use(ctx);
        if let Some(open) = self.open.get_mut(b).and_then(Option::take) {
            open.drop_quietly(ctx);
        }
    }

    /// A transfer ended: stores its picture or frame, answers a query, or
    /// hands a PNG or zlib payload to the helper.
    fn complete(&mut self, ctx: &mut dyn Context, mut transfer: Transfer) {
        let cmd = transfer.cmd;
        if let Some(e) = transfer.failed {
            transfer.drop_quietly(ctx);
            return reply(ctx, &cmd, cmd.id, Err(e));
        }
        let reservation = transfer.reservation.take();
        match transfer.data {
            Data::Raw(raw) => {
                if raw.ignored() {
                    ctx.note(Cause::Program, PAST);
                }
                if !raw.complete() {
                    if let Some(r) = reservation {
                        ctx.unreserve(r);
                    }
                    let (got, want) = (raw.received(), raw.wanted());
                    return reply(ctx, &cmd, cmd.id, Err(Error::NoData { got, want }));
                }
                let Some(reservation) = reservation else {
                    return reply(ctx, &cmd, cmd.id, Err(Error::NoSpace));
                };
                let (pixels, opaque) = raw.into_pixels();
                let picture = (cmd.width, cmd.height, pixels, opaque);
                if cmd.action == b'f' {
                    frame(ctx, &cmd, picture, reservation);
                } else {
                    store(ctx, &cmd, picture, reservation);
                }
            }
            Data::Count { want } => {
                if transfer.taken < want {
                    let got = transfer.taken;
                    return reply(ctx, &cmd, cmd.id, Err(Error::NoData { got, want }));
                }
                if transfer.taken > want {
                    ctx.note(Cause::Program, PAST);
                }
                reply(ctx, &cmd, cmd.id, Ok(()));
            }
            Data::Bytes { data, .. } => {
                self.request(ctx, &cmd, Medium::Direct(data), None, reservation);
            }
            Data::Dropped => {
                if let Some(r) = reservation {
                    ctx.unreserve(r);
                }
            }
        }
    }

    /// Drops the string being read, giving back what its transfer holds.
    /// A chunk's transfer is its upload's, so the upload ends with it.
    fn drop_string(&mut self, ctx: &mut dyn Context) {
        if let Some(transfer) = self.transfer.take() {
            transfer.drop_quietly(ctx);
        }
        self.clear_string();
    }

    /// Forgets the string just read.
    fn clear_string(&mut self) {
        self.phase = Phase::Idle;
        self.keys.clear();
        self.role = Role::Plain;
        self.refusal = None;
        self.base64 = Base64::new();
        self.chunk_of = None;
        self.name.clear();
        self.name_error = None;
    }

    /// Forgets the request out to the helper, giving back the payload's
    /// reservation, with no answer.
    fn drop_request(&mut self, ctx: &mut dyn Context) {
        if let Some(held) = self.pending.take().and_then(|p| p.held) {
            ctx.unreserve(held);
        }
    }

    /// Drops every open upload, giving back what each holds, with no
    /// answer.
    fn drop_uploads(&mut self, ctx: &mut dyn Context) {
        for slot in &mut self.open {
            if let Some(open) = slot.take() {
                open.drop_quietly(ctx);
            }
        }
    }

    /// A transmission from a file, temporary file or shared memory, which
    /// the helper reads. An empty name, a longer one or one holding NUL
    /// is EBADF, and a name that is not base64 is EINVAL.
    fn media_finish(&mut self, ctx: &mut dyn Context) {
        let mut cmd = self.cmd;
        if cmd.dropped() {
            return;
        }
        let mut name_error = self.name_error;
        if name_error.is_none() && self.name.is_empty() {
            name_error = Some(Error::BadFile);
        }
        let helper = ctx.helper();
        let name = core::mem::take(&mut self.name);
        let medium = match cmd.medium {
            b'f' => Medium::File(name),
            b't' => Medium::TempFile(name),
            _ => Medium::SharedMemory(name),
        };
        let refusal = refused(ctx, &cmd).or_else(|| {
            (cmd.action == b'f')
                .then(|| frame_target(ctx, &mut cmd).err())
                .flatten()
        });
        if let Some(e) = refusal {
            let removable = matches!(cmd.medium, b't' | b's');
            if helper && name_error.is_none() && removable {
                self.request(ctx, &cmd, medium, Some(e), None);
            } else {
                reply(ctx, &cmd, cmd.id, Err(e));
            }
            return;
        }
        match name_error {
            Some(e) => reply(ctx, &cmd, cmd.id, Err(e)),
            None if !helper => {
                ctx.note(Cause::Console, NO_HELPER);
                reply(ctx, &cmd, cmd.id, Err(Error::BadFile));
            }
            None => self.request(ctx, &cmd, medium, None, None),
        }
    }

    /// Hands `cmd` to the helper. A discard removes the object and reads
    /// nothing, and is answered `discard`, the handler's own refusal.
    fn request(
        &mut self,
        ctx: &mut dyn Context,
        cmd: &Command,
        medium: Medium,
        discard: Option<Error>,
        held: Option<Reservation>,
    ) {
        let named = !matches!(medium, Medium::Direct(_));
        let (max_w, max_h) = graphic(ctx);
        let mut request = Request {
            medium,
            offset: 0,
            max: 0,
            min: 0,
            inflate: false,
            png: false,
            exact: false,
            discard: discard.is_some(),
            any_image: false,
            frames: None,
            pixels: None,
            largest: (max_w, max_h),
            largest_bytes: ctx.store().limit().saturating_sub(IMAGE_OVERHEAD),
        };
        if discard.is_none() {
            if named {
                request.offset = u64::from(cmd.offset);
                request.max = u64::from(cmd.size);
            }
            request.inflate = cmd.compression == b'z';
            if cmd.format == 100 {
                request.png = true;
                // direct PNG with zlib holds the inflated length to a
                // non-zero S exactly, as the spec words its rule
                if !named && request.inflate && cmd.size != 0 {
                    request.exact = true;
                    request.min = u64::from(cmd.size);
                }
            } else {
                // the helper converts the raw pixels it reads or inflates
                // into the store's format, from this description
                request.min = cmd.pixels().saturating_mul(u64::from(cmd.bpp()));
                request.pixels = Some(Description {
                    layout: if cmd.bpp() == 3 {
                        Layout::Rgb
                    } else {
                        Layout::Rgba
                    },
                    alpha: Alpha::Straight,
                    width: cmd.width,
                    height: cmd.height,
                    compressed: request.inflate,
                });
            }
        }
        self.pending = Some(Pending {
            cmd: *cmd,
            discard,
            held,
        });
        ctx.request(request);
    }

    /// `a=p`: the image `i` names, or the newest of number `I`, placed at
    /// the cursor.
    fn put(&mut self, ctx: &mut dyn Context) {
        let cmd = self.cmd;
        let mut id = cmd.id;
        let answer = match refused(ctx, &cmd) {
            Some(e) => Err(e),
            None if cmd.id == 0 && cmd.number == 0 => Err(Error::Invalid),
            None => match ctx.find_image(cmd.id, cmd.number) {
                Some(key) => {
                    id = ctx.store().kitty_id(key).unwrap_or(cmd.id);
                    place(ctx, &cmd, key, id)
                }
                None => Err(Error::PutNotFound {
                    id: cmd.id,
                    number: cmd.number,
                }),
            },
        };
        reply(ctx, &cmd, id, answer);
    }

    /// `a=d`: a delete, answered only when its keys are refused.
    fn delete(&mut self, ctx: &mut dyn Context) {
        let cmd = self.cmd;
        // the spec: "Specifying both i and I keys in any command is an
        // error", answered with the text a put gets
        if cmd.id != 0 && cmd.number != 0 {
            reply(ctx, &cmd, cmd.id, Err(Error::BothIds));
            return;
        }
        if cmd.bad {
            reply(ctx, &cmd, cmd.id, Err(Error::Invalid));
            return;
        }
        let at = |x: u32| x.checked_sub(1);
        let (cols, _) = ctx.grid();
        let (cx, cy) = ctx.cursor();
        let cursor_col = u32::try_from(cx.min(cols.saturating_sub(1))).unwrap_or(u32::MAX);
        let cursor_row = u32::try_from(cy).unwrap_or(u32::MAX);
        let select = match cmd.delete.to_ascii_lowercase() {
            b'a' => Select::All,
            b'i' => Select::Id {
                id: cmd.id,
                placement: cmd.placement,
            },
            b'n' => Select::Number {
                number: cmd.number,
                placement: cmd.placement,
            },
            b'c' => Select::Cell {
                col: cursor_col,
                row: cursor_row,
            },
            b'p' => match (at(cmd.x), at(cmd.y)) {
                (Some(col), Some(row)) => Select::Cell { col, row },
                _ => Select::Nothing,
            },
            b'q' => match (at(cmd.x), at(cmd.y)) {
                (Some(col), Some(row)) => Select::CellZ { col, row, z: cmd.z },
                _ => Select::Nothing,
            },
            b'r' => Select::Range {
                from: cmd.x,
                to: cmd.y,
            },
            b'x' => at(cmd.x).map_or(Select::Nothing, Select::Column),
            b'y' => at(cmd.y).map_or(Select::Nothing, Select::Row),
            b'z' => Select::Z(cmd.z),
            // animation frames: every frame but the root of the image named;
            // the spec asks every animation command for i or I, so with
            // neither it deletes nothing (ADR-1102, D5)
            b'f' => {
                if let Some(key) = ctx.find_image(cmd.id, cmd.number) {
                    ctx.touch(key);
                    ctx.edit(|e| e.delete_frames(key));
                }
                return;
            }
            _ => return,
        };
        let free = cmd.delete.is_ascii_uppercase();
        ctx.edit(|e| e.kitty_delete(Delete { select, free }));
    }

    /// `a=a` and `a=c`: an animation control or a composition of frames on
    /// the image `i` or `I` names. A composition is answered as a put is; a
    /// control only when its image does not exist, as kitty answers it
    /// (ADR-1102, D3).
    fn animate(&mut self, ctx: &mut dyn Context) {
        let cmd = self.cmd;
        let answer = match refused(ctx, &cmd) {
            Some(e) => Err(e),
            None => animated(ctx, &cmd).and_then(|key| {
                if cmd.action == b'a' {
                    let control = Control {
                        current: cmd.cols,
                        run: cmd.width,
                        loops: cmd.height,
                        frame: cmd.rows,
                        gap: cmd.z,
                    };
                    let _ = ctx.edit(|e| e.control_animation(key, control));
                    return Ok(());
                }
                composition_refusal(ctx.store(), &cmd, key).map_or(Ok(()), Err)?;
                let compose = Compose {
                    src: cmd.rows,
                    dst: cmd.cols,
                    src_x: cmd.cell_x,
                    src_y: cmd.cell_y,
                    dst_x: cmd.x,
                    dst_y: cmd.y,
                    width: cmd.w,
                    height: cmd.h,
                    replace: cmd.cursor == 1,
                };
                ctx.edit(|e| e.compose_frames(key, compose))
                    .unwrap_or(Err(Refusal::NoSpace))
                    .map_err(error)
            }),
        };
        if cmd.action == b'a' && !matches!(answer, Err(Error::AnimNotFound { .. })) {
            return;
        }
        reply(ctx, &cmd, cmd.id, answer);
    }
}

fn store(
    ctx: &mut dyn Context,
    cmd: &Command,
    (width, height, pixels, opaque): (u32, u32, Vec<u32>, bool),
    reservation: Reservation,
) {
    let mut id = cmd.id;
    // a number alone: the terminal picks an unused id (spec)
    if id == 0 && cmd.number != 0 {
        id = ctx.free_id();
    }
    let image = NewImage {
        protocol: Protocol::KITTY,
        width,
        height,
        pixels,
        id,
        number: cmd.number,
        transient: cmd.hints & 1 != 0,
        opaque: Some(opaque),
    };
    let key = match ctx.add_image(image, Some(reservation)) {
        Ok(key) => {
            if cmd.indexed() {
                ctx.edit(|e| e.set_indexed(key));
            }
            key
        }
        Err(Refusal::NoSpace) => return reply(ctx, cmd, cmd.id, Err(Error::NoSpace)),
        Err(_) => return reply(ctx, cmd, cmd.id, Err(Error::Invalid)),
    };
    let answer = if cmd.action == b'T' {
        place(ctx, cmd, key, id)
    } else {
        Ok(())
    };
    reply(ctx, cmd, id, answer);
}

/// The kitty answer to a store's refusal.
fn error(refusal: Refusal) -> Error {
    match refusal {
        Refusal::NotFound => Error::NotFound,
        Refusal::NoSpace => Error::NoSpace,
        Refusal::Invalid => Error::Invalid,
    }
}

/// The image an animation command names by `i` or `I`, whose use clock the
/// command moves (ADR-1102, D1), or ENOENT.
fn animated(ctx: &mut dyn Context, cmd: &Command) -> Result<ImageKey, Error> {
    let key = ctx
        .find_image(cmd.id, cmd.number)
        .ok_or(Error::AnimNotFound {
            id: cmd.id,
            number: cmd.number,
        })?;
    ctx.touch(key);
    Ok(key)
}

/// What refuses composition `cmd` on image `key` with kitty 0.50.0's
/// texts, checked before the store's own checks (ADR-1100, D15): a
/// picture that arrived as palette indices, a destination frame that does
/// not exist, a destination rectangle that leaves the image, and
/// overlapping rectangles within one frame.
fn composition_refusal(store: &Store, cmd: &Command, key: ImageKey) -> Option<Error> {
    if store.indexed(key) {
        return Some(Error::IndexedComposition);
    }
    let frames = store.animation(key).map_or(1, |a| a.frames);
    if cmd.cols == 0 || cmd.cols > frames {
        let id = store.kitty_id(key).unwrap_or(0);
        return Some(Error::NoDestFrame {
            frame: cmd.cols,
            id,
        });
    }
    let (iw, ih) = store.image_size(key)?;
    let w = if cmd.w == 0 { iw } else { cmd.w };
    let h = if cmd.h == 0 { ih } else { cmd.h };
    let fits = |x: u32, y: u32| {
        u64::from(x).saturating_add(u64::from(w)) <= u64::from(iw)
            && u64::from(y).saturating_add(u64::from(h)) <= u64::from(ih)
    };
    if !fits(cmd.x, cmd.y) {
        return Some(Error::ComposeDestOut);
    }
    let apart = |a: u32, b: u32, l: u32| {
        u64::from(a).saturating_add(u64::from(l)) <= u64::from(b)
            || u64::from(b).saturating_add(u64::from(l)) <= u64::from(a)
    };
    let overlaps = !apart(cmd.cell_x, cmd.x, w) && !apart(cmd.cell_y, cmd.y, h);
    (cmd.rows == cmd.cols && overlaps).then_some(Error::ComposeOverlap)
}

/// The image `a=f` sends a frame to, with its checks made before any data
/// is read (ADR-1102, D2): raw data's rectangle, which `s` and `v` give
/// ([`Command::refusal`] refuses a raw frame without them), must lie inside
/// the image; the frame `r` edits, or else the frame `c` starts from, must
/// exist; and a new frame must fit `FRAMES_MAX`. A PNG's size is known
/// only once it is decoded, so only its offset is checked here.
fn frame_target(ctx: &mut dyn Context, cmd: &mut Command) -> Result<ImageKey, Error> {
    let key = animated(ctx, cmd)?;
    // as kitty checks them: the image, palette indices, then the size
    if cmd.indexed() || ctx.store().indexed(key) {
        return Err(Error::IndexedFrame);
    }
    if cmd.format != 100 && (cmd.width == 0 || cmd.height == 0) {
        return Err(Error::ZeroSize);
    }
    let frames = ctx.store().animation(key).map_or(1, |a| a.frames);
    // an edit of a frame that does not exist makes a new frame, as kitty
    // does, and a new frame's base frame must exist
    if cmd.rows > frames {
        cmd.rows = 0;
    }
    if cmd.rows == 0 && cmd.cols > frames {
        return Err(Error::NoBaseFrame { frame: cmd.cols });
    }
    let size = ctx.store().image_size(key).ok_or(Error::Invalid)?;
    let rect = if cmd.format == 100 {
        (0, 0, 1, 1)
    } else {
        clip(size, (cmd.x, cmd.y, cmd.width, cmd.height)).unwrap_or((0, 0, 1, 1))
    };
    ctx.store()
        .check_frame(key, rect, cmd.cols, cmd.rows)
        .map_err(error)?;
    Ok(key)
}

/// The part of the rectangle `w` by `h` at `x`, `y` that lies inside an
/// image of `size`, or `None` when none of it does: a frame's data past the
/// image's edge is cut off, as kitty cuts it (ADR-1102, D2).
fn clip((iw, ih): (u32, u32), (x, y, w, h): (u32, u32, u32, u32)) -> Option<(u32, u32, u32, u32)> {
    if x >= iw || y >= ih || w == 0 || h == 0 {
        return None;
    }
    Some((
        x,
        y,
        w.min(iw.saturating_sub(x)),
        h.min(ih.saturating_sub(y)),
    ))
}

/// Stores `cmd`'s frame data, its width, height, pixels and whether every
/// pixel is opaque, whose bytes `reservation` holds of the frame
/// allowance, and answers.
fn frame(
    ctx: &mut dyn Context,
    cmd: &Command,
    (width, height, pixels, opaque): (u32, u32, Vec<u32>, bool),
    reservation: Reservation,
) {
    let size = ctx
        .find_image(cmd.id, cmd.number)
        .and_then(|key| ctx.store().image_size(key))
        .unwrap_or((0, 0));
    // data past the image's edge is cut off; data wholly outside it lays
    // one clear pixel, by blending, which changes nothing
    let (x, y, width, height, pixels, opaque, replace) =
        match clip(size, (cmd.x, cmd.y, width, height)) {
            Some(rect) if rect == (cmd.x, cmd.y, width, height) => {
                (cmd.x, cmd.y, width, height, pixels, opaque, cmd.cell_x == 1)
            }
            Some((x, y, w, h)) => {
                let cut: Vec<u32> = pixels
                    .chunks_exact(usize::try_from(width).unwrap_or(1).max(1))
                    .take(usize::try_from(h).unwrap_or(0))
                    .flat_map(|row| row.iter().take(usize::try_from(w).unwrap_or(0)))
                    .copied()
                    .collect();
                let opaque = cut.iter().all(|px| px >> 24 == 0xff);
                (x, y, w, h, cut, opaque, cmd.cell_x == 1)
            }
            None => (0, 0, 1, 1, vec![0], false, false),
        };
    let new = NewFrame {
        x,
        y,
        width,
        height,
        pixels,
        opaque,
        replace,
        base: cmd.cols,
        background: cmd.cell_y,
        edit: cmd.rows,
        gap: cmd.z,
    };
    let answer = ctx
        .edit(|e| e.add_frame(new, reservation))
        .unwrap_or(Err(Refusal::NoSpace))
        .map(|_| ())
        .map_err(error);
    reply(ctx, cmd, cmd.id, answer);
}

/// The helper's picture for `a=f`, `size` pixels: checked against the image
/// and its frames, reserved of the frame allowance before a byte is copied,
/// then stored.
fn decoded_frame(ctx: &mut dyn Context, cmd: &Command, payload: &Payload, size: (u32, u32)) {
    let Some(key) = ctx.find_image(cmd.id, cmd.number) else {
        let (id, number) = (cmd.id, cmd.number);
        return reply(ctx, cmd, cmd.id, Err(Error::AnimNotFound { id, number }));
    };
    let image = ctx.store().image_size(key).unwrap_or((0, 0));
    let rect = clip(image, (cmd.x, cmd.y, size.0, size.1)).unwrap_or((0, 0, 1, 1));
    if let Err(e) = ctx.store().check_frame(key, rect, cmd.cols, cmd.rows) {
        return reply(ctx, cmd, cmd.id, Err(error(e)));
    }
    let reserved = ctx
        .edit(|e| e.frame_reserve(key, layer_cost(size.0, size.1)))
        .and_then(Result::ok);
    let Some(reservation) = reserved else {
        return reply(ctx, cmd, cmd.id, Err(Error::NoSpace));
    };
    let Some((pixels, opaque)) = payload.copy_into(Vec::new()) else {
        ctx.unreserve(reservation);
        return reply(ctx, cmd, cmd.id, Err(Error::BadFile));
    };
    ctx.add_work(u64::from(size.0).saturating_mul(u64::from(size.1)));
    frame(ctx, cmd, (size.0, size.1, pixels, opaque), reservation);
}

/// An empty buffer for a picture of `count` pixels and the reservation of
/// its charge `bytes` for an image of kitty id `id`, or `None` when the
/// budget refuses: see [`Edit::kitty_buffer`](crate::Edit).
fn buffer(
    ctx: &mut dyn Context,
    id: u32,
    count: usize,
    bytes: u64,
) -> Option<(Vec<u32>, Reservation)> {
    ctx.edit(|e| e.kitty_buffer(id, count, bytes))
        .and_then(Result::ok)
}

/// Places image `key`, of kitty id `id`, at the cursor as `cmd`'s keys say,
/// and moves the cursor after it unless `C=1`; or makes a relative or a
/// virtual placement of it, which leaves the cursor where it is.
fn place(ctx: &mut dyn Context, cmd: &Command, key: ImageKey, id: u32) -> Answer {
    let (cols, rows) = ctx.grid();
    let (cx, cy) = ctx.cursor();
    // a pending wrap leaves the cursor past the last column
    let x = cx.min(cols.saturating_sub(1));
    let at = Place {
        col: u32::try_from(x).unwrap_or(u32::MAX),
        row: u32::try_from(cy).unwrap_or(u32::MAX),
        x: cmd.cell_x,
        y: cmd.cell_y,
        cols: cmd.cols,
        rows: cmd.rows,
        src_x: cmd.x,
        src_y: cmd.y,
        src_width: cmd.w,
        src_height: cmd.h,
        z: cmd.z,
        // a placement id on an image without an id is ignored (spec)
        id: if id != 0 { cmd.placement } else { 0 },
    };
    let link = if cmd.virtual_placement() {
        Link::Virtual
    } else if cmd.relative() {
        Link::Parent {
            image: cmd.parent,
            placement: cmd.parent_placement,
            cols: cmd.offset_cols,
            rows: cmd.offset_rows,
        }
    } else {
        Link::Cell
    };
    let placed = ctx
        .edit(|e| {
            let p = e.kitty_place(key, at, link)?;
            e.extent(p).ok_or(Placing::Refused(Refusal::NotFound))
        })
        .unwrap_or(Err(Placing::Refused(Refusal::NoSpace)));
    let (box_cols, box_rows) = match placed {
        Ok(extent) => extent,
        Err(Placing::Refused(Refusal::NoSpace)) => return Err(Error::NoSpace),
        Err(Placing::NoParent) => {
            let parent = cmd.parent;
            return Err(if ctx.find_image(parent, 0).is_none() {
                Error::NoParentImage { parent }
            } else {
                Error::NoParentPlacement {
                    parent,
                    placement: cmd.parent_placement,
                }
            });
        }
        Err(Placing::Cycle) if cmd.parent == id && cmd.parent_placement == cmd.placement => {
            return Err(Error::SelfParent);
        }
        Err(Placing::Cycle) => return Err(Error::Cycle),
        Err(Placing::TooDeep) => return Err(Error::TooDeep),
        Err(Placing::Refused(_)) => return Err(Error::Invalid),
    };
    if cmd.cursor == 1 || link != Link::Cell {
        return Ok(());
    }
    // down one row fewer than the box's rows, then the column after the
    // box; when that passes the column CUF stops at, down once more to the
    // column CR goes to (ADR-1100, D6); the index is bounded by the
    // distance to the bottom margin and one screen, so a box of 10,000
    // rows costs at most a screen of scrolling
    let margins = ctx.margins();
    let target = x.saturating_add(usize::try_from(box_cols).unwrap_or(usize::MAX));
    let wraps = target > margins.right_stop(x, cols);
    let down = usize::try_from(box_rows)
        .unwrap_or(usize::MAX)
        .saturating_sub(1)
        .saturating_add(usize::from(wraps));
    let room = if cy >= margins.top && cy <= margins.bottom {
        margins.bottom.saturating_sub(cy)
    } else {
        0
    };
    ctx.index(down.min(room.saturating_add(rows)));
    let (_, row) = ctx.cursor();
    ctx.move_cursor(if wraps { margins.line_start(x) } else { target }, row);
    Ok(())
}

/// [`Command::refusal`], noting for the log a refusal that one of vtium's
/// bounds makes ([`Command::bound`]), which the operator can act on.
fn refused(ctx: &mut dyn Context, cmd: &Command) -> Option<Error> {
    let e = cmd.refusal(ctx)?;
    if let Some(why) = cmd.bound(ctx) {
        ctx.note(Cause::Console, why);
    }
    Some(e)
}

/// The kitty answer to a helper's outcome other than OK (ADR-1100, D15):
/// too few bytes read from a named medium is a failure to read, EBADF, and
/// a zlib payload that inflates to too few bytes is EINVAL with kitty
/// 0.50.0's text on the inflated size. kitty's text for a payload that does
/// not inflate names zlib's error, which the image helper does not report,
/// so that keeps vtium's own EINVAL.
fn outcome(reply: &Reply, named: bool, inflate: bool) -> Error {
    match reply {
        Reply::TooShort if named && !inflate => Error::BadFile,
        Reply::TooShort if inflate => Error::InflateSize,
        Reply::TooShort | Reply::Corrupt | Reply::TooLarge => Error::Invalid,
        Reply::OverBudget => Error::NoSpace,
        Reply::Unreadable | Reply::Failed | Reply::Ok(_) => Error::BadFile,
    }
}

/// Where direct transmission `cmd`'s data goes, what the budget reserved
/// for it, and the cost an upload's cap is, or the refusal that the keys
/// and the budget decide before any data (ADR-1100, D5, D16). An upload in
/// chunks, `chunked`, of a picture reserves its whole cost beside the
/// image of the same id; a whole command counts that image toward its own
/// cost, as a replacement frees it.
fn ready(
    ctx: &mut dyn Context,
    cmd: &Command,
    chunked: bool,
) -> Result<(Data, Option<Reservation>, Option<u64>), Error> {
    let frame_key = |ctx: &mut dyn Context| {
        ctx.find_image(cmd.id, cmd.number)
            .ok_or(Error::AnimNotFound {
                id: cmd.id,
                number: cmd.number,
            })
    };
    if cmd.for_helper() {
        if !ctx.helper() {
            ctx.note(Cause::Console, NO_HELPER);
            return Err(Error::Invalid);
        }
        let reserved = match cmd.action {
            b'f' => {
                let key = frame_key(ctx)?;
                ctx.edit(|e| e.frame_reserve(key, HOST_FIRST))
                    .unwrap_or(Err(Refusal::NoSpace))
            }
            b'q' => ctx.reserve(Protocol::KITTY, 0, HOST_FIRST),
            _ if chunked => ctx
                .edit(|e| e.reserve_beside(cmd.id, HOST_FIRST))
                .unwrap_or(Err(Refusal::NoSpace)),
            _ => ctx.reserve(Protocol::KITTY, cmd.id, HOST_FIRST),
        };
        let reservation = reserved.map_err(error)?;
        let data = Data::Bytes {
            data: Vec::new(),
            room: HOST_FIRST,
        };
        return Ok((data, Some(reservation), None));
    }
    let count = cmd.pixels();
    if cmd.action == b'q' {
        if cmd.indexed() && ctx.colors().is_none() {
            return Err(Error::Invalid);
        }
        let want = count.saturating_mul(u64::from(cmd.bpp()));
        return Ok((Data::Count { want }, None, None));
    }
    // palette indices take the colors in force at the first chunk
    let colors = || {
        ctx.colors()
            .map(|c| Box::new(c.resolved()))
            .ok_or(Error::Invalid)
    };
    let format = match cmd.format {
        8 => Format::Index(colors()?),
        16 => Format::IndexAlpha(colors()?),
        24 => Format::Rgb,
        _ => Format::Rgba,
    };
    let len = usize::try_from(count).map_err(|_| Error::Invalid)?;
    if cmd.action == b'f' {
        let key = frame_key(ctx)?;
        let cost = layer_cost(cmd.width, cmd.height);
        let reservation = ctx
            .edit(|e| e.frame_reserve(key, cost))
            .unwrap_or(Err(Refusal::NoSpace))
            .map_err(error)?;
        let raw = Raw::new(format, len, Vec::new());
        return Ok((Data::Raw(raw), Some(reservation), Some(cost)));
    }
    let cost = charge(count);
    let (into, reservation) = if chunked {
        let reservation = ctx
            .edit(|e| e.reserve_beside(cmd.id, cost))
            .unwrap_or(Err(Refusal::NoSpace))
            .map_err(error)?;
        (Vec::new(), reservation)
    } else {
        buffer(ctx, cmd.id, len, cost).ok_or(Error::NoSpace)?
    };
    let raw = Raw::new(format, len, into);
    Ok((Data::Raw(raw), Some(reservation), Some(cost)))
}

impl Handler for Kitty {
    fn start(&mut self, ctx: &mut dyn Context, _: Intro<'_>) {
        self.drop_string(ctx);
        self.phase = Phase::Keys;
    }

    fn feed(&mut self, ctx: &mut dyn Context, data: Chunk<'_>) {
        ctx.add_work(chunk_work(data));
        // a character past ASCII is outside both the keys and base64
        let bytes = match data {
            Chunk::Bytes(bytes) => bytes,
            Chunk::Char(_) => &[0xff],
        };
        match self.phase {
            Phase::Keys => self.keys(ctx, bytes),
            Phase::Payload => self.payload(ctx, bytes),
            Phase::Idle | Phase::Ignored => {}
        }
    }

    fn feed_some(&mut self, ctx: &mut dyn Context, run: &[u8], work_left: u64) -> usize {
        // one unit of work for each byte, and at least one byte a call
        let n = usize::try_from(work_left)
            .unwrap_or(usize::MAX)
            .clamp(1, run.len().max(1))
            .min(run.len());
        if let Some(taken) = run.get(..n)
            && !taken.is_empty()
        {
            self.feed(ctx, Chunk::Bytes(taken));
        }
        n
    }

    fn finish(&mut self, ctx: &mut dyn Context) {
        self.end(ctx);
        self.drop_string(ctx);
    }

    fn cancel(&mut self, ctx: &mut dyn Context) {
        // a chunk cut short ends its upload, whose data it can no longer
        // complete, with no answer
        self.drop_string(ctx);
        // a request the host abandons is answered by nobody
        self.drop_request(ctx);
    }

    fn decoded(&mut self, ctx: &mut dyn Context, answer: Reply) {
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        if let Some(held) = pending.held.take() {
            ctx.unreserve(held);
        }
        let cmd = pending.cmd;
        if let Some(refusal) = pending.discard {
            return reply(ctx, &cmd, cmd.id, Err(refusal));
        }
        let named = cmd.medium != b'd';
        let inflate = cmd.compression == b'z';
        // every picture comes back premultiplied, decoded or converted
        // from the pixels the request described (ADR-1101 D7)
        let (payload, size) = match answer {
            Reply::Ok(payload) => match payload.size() {
                Some(size) => (payload, size),
                None => return reply(ctx, &cmd, cmd.id, Err(Error::BadFile)),
            },
            other => {
                match other {
                    Reply::Failed => {
                        ctx.note(Cause::Console, "the image helper fails on a kitty picture");
                    }
                    Reply::TooLarge => ctx.note(
                        Cause::Console,
                        "the image helper finds a kitty picture past its bounds",
                    ),
                    _ => {}
                }
                let e = outcome(&other, named, inflate);
                return reply(ctx, &cmd, cmd.id, Err(e));
            }
        };
        // max-graphic-size may have been lowered since the request
        let (max_w, max_h) = graphic(ctx);
        if size.0 > max_w || size.1 > max_h {
            ctx.note(Cause::Console, "a kitty picture past max-graphic-size");
            return reply(ctx, &cmd, cmd.id, Err(Error::Invalid));
        }
        if cmd.action == b'q' {
            return reply(ctx, &cmd, cmd.id, Ok(()));
        }
        let count = u64::from(size.0).saturating_mul(u64::from(size.1));
        if cmd.action == b'f' {
            return decoded_frame(ctx, &cmd, &payload, size);
        }
        let len = usize::try_from(count).unwrap_or(usize::MAX);
        // the budget refuses before a byte of the picture is copied
        let Some((into, reservation)) = buffer(ctx, cmd.id, len, charge(count)) else {
            return reply(ctx, &cmd, cmd.id, Err(Error::NoSpace));
        };
        // the one copy into memory only the console holds, into the buffer
        // just reserved, clamps each color to its alpha and finds the
        // opacity, before the store sees a pixel
        let Some((pixels, opaque)) = payload.copy_into(into) else {
            ctx.unreserve(reservation);
            return reply(ctx, &cmd, cmd.id, Err(Error::BadFile));
        };
        ctx.add_work(count);
        store(ctx, &cmd, (size.0, size.1, pixels, opaque), reservation);
    }

    fn reset(&mut self, ctx: &mut dyn Context) {
        self.withdraw(ctx);
        self.drop_request(ctx);
        *self = Self::default();
    }

    fn withdraw(&mut self, ctx: &mut dyn Context) {
        self.drop_string(ctx);
        self.drop_uploads(ctx);
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
