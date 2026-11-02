//! The kitty graphics handler, which takes APC `G` strings (ADR-1100, D6).
//!
//! It follows the kitty graphics protocol's specification
//! (sw.kovidgoyal.net/kitty/graphics-protocol, read 2026-11-02), and
//! kitty's own behaviour where the specification is silent (D14).
//!
//! A string is a command: control keys up to the first `;`, at most
//! [`KEYS_MAX`] bytes of them, then a base64 payload. A longer key section
//! makes the whole string ignored. The payload is decoded as it streams in,
//! never collected whole: raw `f=24` and `f=32` pixels go straight into the
//! pending image, whose bytes the store reserves when the first chunk
//! names its size, and a file or shared-memory name goes into a buffer of
//! at most [`NAME_MAX`] bytes.
//!
//! - **Chunks.** A direct transmission (`t=d`) may come in chunks: `m=1` on
//!   every chunk but the last, and later chunks carry only `m` and `q`.
//!   Any other graphics command ends an upload in chunks, and gives back
//!   what it reserved. A transmission that fails part way swallows its
//!   remaining chunks and answers once, at its last. A chunk's `q` of 1 or
//!   2 holds from then on.
//! - **The helper.** PNG (`f=100`) and zlib (`o=z`) payloads sent directly
//!   are kept, each growth of the buffer reserved first, and handed to the
//!   image helper whole; files, temporary files and shared memory (`t=f`,
//!   `t=t`, `t=s`) go to it by name (D7). A `t=t` or `t=s` command the
//!   handler refuses still goes to the helper as a discard, which removes
//!   the object, since the spec gives its removal to the terminal. Without
//!   a helper, every named medium answers EBADF and PNG and zlib answer
//!   EINVAL, as D7's "Without a helper" says.
//! - **Replies.** Only a command that carries `i` or `I` is answered; `q=1`
//!   leaves out OK and `q=2` every reply. An `a=q` without `i` reads nothing
//!   and is answered under no `q`, as kitty 0.49.2 behaved when the C
//!   reference ran it as a black box (kmscon's ADR 107). A delete answers only
//!   an error.
//! - **The cursor.** After a placement the cursor goes down one row fewer
//!   than the box has rows, then to the column after the box; when that
//!   reaches the right edge it goes down once more, to column 0. `C=1`
//!   leaves it where it is.
//!
//! Relative placements (`P`, `Q`, `H`, `V`), Unicode placeholders (`U`) and
//! animation (`a=f`, `a=a`, `a=c`) are answered EINVAL until their own work
//! builds them (D13, and kitty animation's record); `Command::later` is the one place that
//! refuses them. A delete of animation frames (`d=f`, `d=F`) deletes
//! nothing.

use vtium_parse::Chunk;

use crate::bounds::IMAGE_OVERHEAD;
use crate::decode::base64::{Base64, Sink};
use crate::decode::convert::{self, Alpha, Layout};
use crate::decode::raw::Raw;
use crate::decode::{Description, Medium, Payload, Reply, Request};
use crate::handler::{Context, Handler, Intro};
use crate::image::{ImageKey, NewImage, Place, Refusal};
use crate::protocol::chunk_work;
use crate::rules::Protocol;
use crate::store::Reservation;
use crate::store::kitty::{Delete, Select};

mod command;
#[cfg(test)]
mod tests;

use command::{Answer, Command, Error, charge, graphic, reply};

/// The most bytes of control keys a command may carry, as the C reference
/// bounded them (its `KITTY_KEYS_MAX`).
pub const KEYS_MAX: usize = 1024;

/// The most bytes a file, temporary-file or shared-memory name may decode
/// to, as the C reference bounded it (its `KITTY_NAME_MAX`).
pub const NAME_MAX: usize = 4095;

/// The first size of the buffer a PNG or zlib payload is kept in; it
/// doubles as it fills.
const HOST_FIRST: u64 = 4096;

/// Where the string is in its reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    /// No string, or one whose keys passed [`KEYS_MAX`].
    #[default]
    Ignore,
    Keys,
    Payload,
}

/// Where a string's payload goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Target {
    #[default]
    None,
    /// The pending direct upload.
    Upload,
    /// A file, temporary-file or shared-memory name.
    Name,
}

/// What the pending direct upload does with its data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Mode {
    /// No upload pending.
    #[default]
    None,
    /// Refused or failed: its chunks are swallowed, and its last answers
    /// this.
    Skip(Answer),
    /// `f=24` or `f=32` into the pending image, whose bytes are reserved.
    Raw { raw: Raw, reservation: Reservation },
    /// `a=q`: the bytes counted, not kept.
    Count { len: u64, want: u64 },
    /// PNG or zlib, kept for the helper; `cap` is the buffer's reserved
    /// size.
    Host {
        data: Vec<u8>,
        cap: u64,
        reservation: Option<Reservation>,
    },
}

/// The pending direct upload: its first chunk's keys and its decoder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Upload {
    mode: Mode,
    cmd: Command,
    base64: Base64,
}

/// The request out to the helper, until its reply.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    cmd: Command,
    /// A discard answers the handler's own refusal, whatever the helper
    /// says.
    discard: Option<Error>,
    /// The bytes a directly sent payload held in the console, given back
    /// when the reply comes.
    held: Option<Reservation>,
}

/// The kitty graphics protocol's front end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kitty {
    phase: Phase,
    keys: Vec<u8>,
    cmd: Command,
    target: Target,
    name: Vec<u8>,
    /// The name failed: too long or holding NUL (EBADF), or not base64
    /// (EINVAL).
    name_error: Option<Error>,
    name_base64: Base64,
    upload: Upload,
    pending: Option<Pending>,
}

/// The pending upload as the base64 decoder's sink.
struct UploadSink<'a, 'c> {
    mode: &'a mut Mode,
    ctx: &'a mut (dyn Context + 'c),
}

impl UploadSink<'_, '_> {
    /// The upload fails with `error`; the rest of its data is ignored.
    fn fail(&mut self, error: Error) {
        fail(self.mode, self.ctx, error);
    }
}

/// Gives back what an upload holds, and leaves no upload pending.
fn abort(mode: &mut Mode, ctx: &mut dyn Context) {
    match core::mem::take(mode) {
        Mode::Raw { reservation, .. }
        | Mode::Host {
            reservation: Some(reservation),
            ..
        } => ctx.unreserve(reservation),
        _ => {}
    }
}

/// The upload fails with `error`, once: a failed upload stays failed.
fn fail(mode: &mut Mode, ctx: &mut dyn Context, error: Error) {
    if matches!(mode, Mode::Skip(_)) {
        return;
    }
    abort(mode, ctx);
    *mode = Mode::Skip(Err(error));
}

impl Sink for UploadSink<'_, '_> {
    fn open(&self) -> bool {
        matches!(
            self.mode,
            Mode::Raw { .. } | Mode::Count { .. } | Mode::Host { .. }
        )
    }

    fn bytes(&mut self, data: &[u8]) {
        let n = u64::try_from(data.len()).unwrap_or(u64::MAX);
        match self.mode {
            Mode::Raw { raw, .. } => raw.push(data),
            Mode::Count { len, want } => *len = len.saturating_add(n).min(*want),
            Mode::Host {
                data: buf,
                cap,
                reservation,
            } => {
                // a later string without a helper buffers nothing more
                if !self.ctx.helper() {
                    self.fail(Error::Invalid);
                    return;
                }
                let len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
                let need = len.saturating_add(n);
                if need > *cap {
                    let mut grown = if *cap == 0 { HOST_FIRST } else { *cap };
                    while grown < need {
                        grown = grown.saturating_mul(2);
                    }
                    let more = grown.saturating_sub(*cap);
                    let ok = match reservation {
                        Some(r) => self.ctx.grow(r, more).is_ok(),
                        None => match self.ctx.reserve(Protocol::KITTY, 0, more) {
                            Ok(r) => {
                                *reservation = Some(r);
                                true
                            }
                            Err(_) => false,
                        },
                    };
                    if !ok {
                        self.fail(Error::NoSpace);
                        return;
                    }
                    self.ctx.add_work(more);
                    *cap = grown;
                    buf.reserve_exact(usize::try_from(more).unwrap_or(0));
                }
                buf.extend_from_slice(data);
            }
            Mode::None | Mode::Skip(_) => {}
        }
    }

    fn error(&mut self) {
        self.fail(Error::Invalid);
    }
}

/// A name as the base64 decoder's sink.
struct NameSink<'a> {
    name: &'a mut Vec<u8>,
    error: &'a mut Option<Error>,
}

impl Sink for NameSink<'_> {
    fn open(&self) -> bool {
        self.error.is_none()
    }

    fn bytes(&mut self, data: &[u8]) {
        for &b in data {
            if b == 0 || self.name.len() >= NAME_MAX {
                *self.error = Some(Error::BadFile);
                return;
            }
            self.name.push(b);
        }
    }

    fn error(&mut self) {
        self.error.get_or_insert(Error::Invalid);
    }
}

impl Kitty {
    /// Decodes payload bytes into the string's target.
    fn payload(&mut self, ctx: &mut dyn Context, data: &[u8]) {
        match self.target {
            Target::Upload => {
                let Upload { mode, base64, .. } = &mut self.upload;
                base64.feed(data, &mut UploadSink { mode, ctx });
            }
            Target::Name => self.name_base64.feed(
                data,
                &mut NameSink {
                    name: &mut self.name,
                    error: &mut self.name_error,
                },
            ),
            Target::None => {}
        }
    }

    /// The keys are read: where the payload goes.
    fn command_start(&mut self, ctx: &mut dyn Context) {
        self.cmd = Command::parse(&self.keys);
        self.target = Target::None;
        let cmd = self.cmd;
        if self.upload.mode != Mode::None && cmd.continues(&self.upload.cmd) {
            if cmd.bad {
                fail(&mut self.upload.mode, ctx, Error::Invalid);
            }
            // a chunk's q of 1 or 2 holds from then on, and q=0 keeps the
            // first chunk's; the spec is silent, and Ghostty keeps it so
            if cmd.quiet != 0 {
                self.upload.cmd.quiet = cmd.quiet;
            }
            self.target = Target::Upload;
            return;
        }
        // any other graphics command ends the pending upload: the spec asks
        // it of a delete, and vtium, as the C reference did, asks it of
        // every command that does not continue the upload
        abort(&mut self.upload.mode, ctx);
        if !cmd.transmits() {
            return;
        }
        if cmd.medium == b'd' {
            self.upload_start(ctx);
            self.target = Target::Upload;
        } else {
            self.name.clear();
            self.name_error = None;
            self.name_base64 = Base64::new();
            self.target = Target::Name;
        }
    }

    /// The first chunk of a direct transmission. Raw data has its pixels'
    /// bytes reserved before they are allocated; `a=q` only counts what
    /// arrives.
    fn upload_start(&mut self, ctx: &mut dyn Context) {
        let cmd = self.cmd;
        self.upload = Upload {
            mode: Mode::Skip(Ok(())),
            cmd,
            base64: Base64::new(),
        };
        if cmd.dropped() {
            return;
        }
        if let Some(e) = cmd.refusal(ctx) {
            self.upload.mode = Mode::Skip(Err(e));
            return;
        }
        if cmd.for_helper() {
            self.upload.mode = if ctx.helper() {
                Mode::Host {
                    data: Vec::new(),
                    cap: 0,
                    reservation: None,
                }
            } else {
                Mode::Skip(Err(Error::Invalid))
            };
            return;
        }
        let pixels = cmd.pixels();
        let want = pixels.saturating_mul(u64::from(cmd.bpp()));
        if cmd.action == b'q' {
            self.upload.mode = Mode::Count { len: 0, want };
            return;
        }
        let Some((count, (buffer, reservation))) = usize::try_from(pixels)
            .ok()
            .and_then(|count| Some((count, buffer(ctx, cmd.id, count, charge(pixels))?)))
        else {
            self.upload.mode = Mode::Skip(Err(Error::NoSpace));
            return;
        };
        self.upload.mode = Mode::Raw {
            raw: Raw::new(cmd.bpp(), count, buffer),
            reservation,
        };
    }

    /// The upload's last chunk has arrived.
    fn upload_finish(&mut self, ctx: &mut dyn Context) {
        let cmd = self.upload.cmd;
        match core::mem::take(&mut self.upload.mode) {
            Mode::None => {}
            Mode::Skip(answer) => reply(ctx, &cmd, cmd.id, answer),
            Mode::Count { len, want } => {
                let answer = if len < want {
                    Err(Error::Invalid)
                } else {
                    Ok(())
                };
                reply(ctx, &cmd, cmd.id, answer);
            }
            Mode::Host {
                data, reservation, ..
            } => {
                if ctx.helper() {
                    self.request(ctx, &cmd, Medium::Direct(data), None, reservation);
                } else {
                    if let Some(r) = reservation {
                        ctx.unreserve(r);
                    }
                    reply(ctx, &cmd, cmd.id, Err(Error::Invalid));
                }
            }
            Mode::Raw { raw, reservation } => {
                // raw data must hold every pixel the keys call for
                if !raw.complete() {
                    ctx.unreserve(reservation);
                    reply(ctx, &cmd, cmd.id, Err(Error::Invalid));
                    return;
                }
                ctx.add_work(cmd.pixels());
                let (pixels, opaque) = raw.into_pixels();
                let picture = (cmd.width, cmd.height, pixels, opaque);
                store(ctx, &cmd, picture, reservation);
            }
        }
    }

    /// A transmission from a file, temporary file or shared memory, which
    /// the helper reads. An empty name, a longer one or one holding NUL
    /// answers EBADF with no request, as an unreadable file does.
    fn media_finish(&mut self, ctx: &mut dyn Context) {
        let cmd = self.cmd;
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
        if let Some(e) = cmd.refusal(ctx) {
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
            None if !helper => reply(ctx, &cmd, cmd.id, Err(Error::BadFile)),
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
        let answer = match cmd.refusal(ctx) {
            Some(e) => Err(e),
            None if cmd.id == 0 && cmd.number == 0 => Err(Error::Invalid),
            None => match ctx.find_image(cmd.id, cmd.number) {
                Some(key) => {
                    id = ctx.store().kitty_id(key).unwrap_or(cmd.id);
                    place(ctx, &cmd, key, id)
                }
                None => Err(Error::NotFound),
            },
        };
        reply(ctx, &cmd, id, answer);
    }

    /// `a=d`: a delete, answered only when its keys are refused.
    fn delete(&mut self, ctx: &mut dyn Context) {
        let cmd = self.cmd;
        if cmd.bad || (cmd.id != 0 && cmd.number != 0) {
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
            // animation frames: nothing to delete until animation exists
            _ => return,
        };
        let free = cmd.delete.is_ascii_uppercase();
        ctx.edit(|e| e.kitty_delete(Delete { select, free }));
    }

    /// The string ended with its terminator.
    fn command_finish(&mut self, ctx: &mut dyn Context) {
        match self.target {
            Target::Upload => {
                let Upload { mode, base64, .. } = &mut self.upload;
                base64.flush(&mut UploadSink { mode, ctx });
                if self.cmd.more == 0 {
                    self.upload_finish(ctx);
                }
            }
            Target::Name => {
                self.name_base64.flush(&mut NameSink {
                    name: &mut self.name,
                    error: &mut self.name_error,
                });
                self.media_finish(ctx);
            }
            Target::None => match self.cmd.action {
                b'p' => self.put(ctx),
                b'd' => self.delete(ctx),
                _ => {
                    let cmd = self.cmd;
                    reply(ctx, &cmd, cmd.id, Err(Error::Invalid));
                }
            },
        }
        self.target = Target::None;
    }

    /// Gives back everything the handler holds: the pending upload's
    /// reservation, and the bytes a request out holds.
    fn release(&mut self, ctx: &mut dyn Context) {
        abort(&mut self.upload.mode, ctx);
        if let Some(Pending {
            held: Some(held), ..
        }) = self.pending.take()
        {
            ctx.unreserve(held);
        }
    }
}

/// Stores `cmd`'s transmitted picture, its width, height, pixels and
/// whether every pixel is opaque, whose bytes `reservation` holds, places
/// it for `a=T`, and answers.
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
        Ok(key) => key,
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
/// and moves the cursor after it unless `C=1`.
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
    let placed = ctx
        .edit(|e| {
            let p = e.place(key, at)?;
            e.extent(p).ok_or(Refusal::NotFound)
        })
        .unwrap_or(Err(Refusal::NoSpace));
    let (box_cols, box_rows) = match placed {
        Ok(extent) => extent,
        Err(Refusal::NoSpace) => return Err(Error::NoSpace),
        Err(_) => return Err(Error::Invalid),
    };
    if cmd.cursor == 1 {
        return Ok(());
    }
    // down one row fewer than the box's rows, then the column after the
    // box, or down once more to column 0 when that reaches the right edge;
    // bounded by the distance to the bottom margin and one screen, so a
    // box of 10,000 rows costs at most a screen of scrolling
    let box_cols = usize::try_from(box_cols).unwrap_or(usize::MAX);
    let target = if box_cols < cols.saturating_sub(x) {
        x.saturating_add(box_cols)
    } else {
        cols
    };
    let wraps = target >= cols;
    let down = usize::try_from(box_rows)
        .unwrap_or(usize::MAX)
        .saturating_sub(1)
        .saturating_add(usize::from(wraps));
    let margins = ctx.margins();
    let room = if cy >= margins.top && cy <= margins.bottom {
        margins.bottom.saturating_sub(cy)
    } else {
        0
    };
    ctx.index(down.min(room.saturating_add(rows)));
    let (_, row) = ctx.cursor();
    ctx.move_cursor(if wraps { 0 } else { target }, row);
    Ok(())
}

/// The kitty answer to a helper's outcome other than OK. Too few bytes
/// read from a named medium is a failure to read, EBADF, as kitty answers
/// it; too few inflated, or sent directly, is EINVAL.
fn outcome(reply: &Reply, named: bool, inflate: bool) -> Error {
    match reply {
        Reply::TooShort if named && !inflate => Error::BadFile,
        Reply::TooShort | Reply::Corrupt | Reply::TooLarge => Error::Invalid,
        Reply::OverBudget => Error::NoSpace,
        Reply::Unreadable | Reply::Failed | Reply::Ok(_) => Error::BadFile,
    }
}

impl Handler for Kitty {
    fn start(&mut self, _: &mut dyn Context, _: Intro<'_>) {
        self.phase = Phase::Keys;
        self.keys.clear();
        self.target = Target::None;
    }

    fn feed(&mut self, ctx: &mut dyn Context, data: Chunk<'_>) {
        ctx.add_work(chunk_work(data));
        let wide = [0x80u8];
        let mut bytes: &[u8] = match data {
            Chunk::Bytes(b) => b,
            // a character from U+00A0 up is neither a key nor base64
            Chunk::Char(_) => &wide,
        };
        if self.phase == Phase::Keys {
            let end = bytes.iter().position(|&b| b == b';');
            let keys = end.map_or(bytes, |e| bytes.get(..e).unwrap_or_default());
            if self.keys.len().saturating_add(keys.len()) > KEYS_MAX {
                // a longer key section: the whole string is ignored
                self.phase = Phase::Ignore;
                return;
            }
            self.keys.extend_from_slice(keys);
            let Some(end) = end else {
                return;
            };
            self.command_start(ctx);
            self.phase = Phase::Payload;
            bytes = bytes.get(end.saturating_add(1)..).unwrap_or_default();
        }
        if self.phase == Phase::Payload && !bytes.is_empty() {
            self.payload(ctx, bytes);
        }
    }

    /// A run of an APC string's data: the key section whole, since it is at
    /// most 1 KiB, and of the payload only as many bytes as `work_left`
    /// allows, each byte one unit of work, at least one byte of a run that
    /// is not empty. The base64 decoder keeps its open group between runs,
    /// so the rest of the run decodes the same in the next input call.
    fn feed_some(&mut self, ctx: &mut dyn Context, run: &[u8], work_left: u64) -> usize {
        let mut taken = 0usize;
        if self.phase == Phase::Keys {
            let keys = run
                .iter()
                .position(|&b| b == b';')
                .map_or(run.len(), |e| e.saturating_add(1));
            self.feed(ctx, Chunk::Bytes(run.get(..keys).unwrap_or_default()));
            taken = keys;
        }
        let rest = run.get(taken..).unwrap_or_default();
        if rest.is_empty() {
            return taken.max(usize::from(!run.is_empty()));
        }
        if self.phase != Phase::Payload {
            // an ignored string reads nothing more
            ctx.add_work(chunk_work(Chunk::Bytes(rest)));
            return run.len();
        }
        let left = work_left.saturating_sub(u64::try_from(taken).unwrap_or(u64::MAX));
        let n = usize::try_from(left)
            .unwrap_or(usize::MAX)
            .clamp(usize::from(taken == 0), rest.len());
        if n > 0 {
            self.feed(ctx, Chunk::Bytes(rest.get(..n).unwrap_or_default()));
        }
        taken.saturating_add(n)
    }

    fn finish(&mut self, ctx: &mut dyn Context) {
        match core::mem::take(&mut self.phase) {
            Phase::Ignore => return,
            Phase::Keys => self.command_start(ctx),
            Phase::Payload => {}
        }
        self.command_finish(ctx);
    }

    fn cancel(&mut self, ctx: &mut dyn Context) {
        // a cancelled string ends the upload it belongs to, and any other;
        // a request out is dropped with no reply to the program
        self.phase = Phase::Ignore;
        self.target = Target::None;
        self.release(ctx);
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
        let (size, data) = match answer {
            Reply::Ok(Payload::Rgba {
                width,
                height,
                data,
            }) => ((width, height), data),
            other => {
                let e = outcome(&other, named, inflate);
                return reply(ctx, &cmd, cmd.id, Err(e));
            }
        };
        // max-graphic-size may have been lowered since the request
        let (max_w, max_h) = graphic(ctx);
        if size.0 > max_w || size.1 > max_h {
            return reply(ctx, &cmd, cmd.id, Err(Error::Invalid));
        }
        if cmd.action == b'q' {
            return reply(ctx, &cmd, cmd.id, Ok(()));
        }
        let count = u64::from(size.0).saturating_mul(u64::from(size.1));
        let len = usize::try_from(count).unwrap_or(usize::MAX);
        let Some((into, reservation)) = buffer(ctx, cmd.id, len, charge(count)) else {
            return reply(ctx, &cmd, cmd.id, Err(Error::NoSpace));
        };
        // the one copy into memory only the console holds clamps each
        // color to its alpha and finds the opacity, before the store sees
        // a pixel
        let (pixels, opaque) = convert::from_reply(&data, into);
        ctx.add_work(count);
        store(ctx, &cmd, (size.0, size.1, pixels, opaque), reservation);
    }

    fn reset(&mut self, ctx: &mut dyn Context) {
        self.release(ctx);
        *self = Self::default();
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
