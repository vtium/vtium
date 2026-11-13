//! A kitty graphics command's control keys, the refusals made before any
//! data is read, and the reply the handler writes (ADR-1100, D6).

use core::fmt::Write as _;

use crate::bounds::{BOX_CELLS_MAX, IMAGE_OVERHEAD, IMAGE_SIDE_MAX};
use crate::handler::{Cause, Context, ReplyKind};
use crate::image::ImageKey;
use crate::store::Store;

/// What a command is answered when it fails. Where kitty 0.50.0 was seen
/// to answer a refusal, the text is kitty's, its numbers re-encoded
/// (ADR-1100, D15); the others keep vtium's own fixed text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    /// No image of that id or number, where kitty's text was not seen.
    NotFound,
    /// A put names an image that does not exist.
    PutNotFound { id: u32, number: u32 },
    /// A frame, a composition or a control names an image that does not
    /// exist.
    AnimNotFound { id: u32, number: u32 },
    /// A composition's destination frame does not exist.
    NoDestFrame { frame: u32, id: u32 },
    /// A new frame's base frame `c` does not exist.
    NoBaseFrame { frame: u32 },
    /// Any failure to read a file, temporary file or shared-memory object:
    /// the spec's one reply for all of them.
    BadFile,
    /// The budget refused.
    NoSpace,
    /// A relative placement's parent image does not exist.
    NoParentImage { parent: u32 },
    /// A relative placement's parent image exists, but not the placement.
    NoParentPlacement { parent: u32, placement: u32 },
    /// A relative placement names itself as its parent.
    SelfParent,
    /// A relative placement would follow itself through other placements.
    Cycle,
    /// A chain of relative placements would pass `RELATIVE_DEPTH_MAX`.
    TooDeep,
    /// A virtual placement, `U=1`, names a parent.
    VirtualParent,
    /// Both `i` and `I`.
    BothIds,
    /// A format other than 8, 16, 24, 32 and 100.
    UnknownFormat { format: u32 },
    /// A raw picture past `max-graphic-size`, `max` the side it passes.
    TooLarge { max: u32 },
    /// Raw data short of the pixels its width and height call for: the
    /// bytes that came and the bytes needed (ADR-1100, D15).
    NoData { got: u64, want: u64 },
    /// A raw upload or frame without `s` or `v` (ADR-1100, D15; ADR-1102,
    /// D2).
    ZeroSize,
    /// A frame on a picture that arrived as palette indices, or a frame in
    /// `f=8` or `f=16` (ADR-1100, D15).
    IndexedFrame,
    /// A composition on a picture that arrived as palette indices.
    IndexedComposition,
    /// A zlib payload that inflated to fewer bytes than needed.
    InflateSize,
    /// A composition within one frame whose rectangles overlap.
    ComposeOverlap,
    /// A composition whose destination rectangle leaves the image.
    ComposeDestOut,
    /// Anything else the handler cannot do.
    Invalid,
}

impl Error {
    /// Writes the reply's message, the code and the text after it.
    pub(super) fn write_to(self, out: &mut String) {
        let _ = match self {
            Self::NotFound => write!(out, "ENOENT:image not found"),
            Self::PutNotFound { id, number } => write!(
                out,
                "ENOENT:Put command refers to non-existent image with id: {id} and number: {number}"
            ),
            Self::AnimNotFound { id, number } => write!(
                out,
                "ENOENT:Animation command refers to non-existent image with id: {id} and number: {number}"
            ),
            // kitty's text ends with a line feed
            Self::NoDestFrame { frame, id } => writeln!(
                out,
                "ENOENT:No destination frame number {frame} exists in image id: {id}"
            ),
            Self::NoBaseFrame { frame } => {
                write!(out, "EINVAL:No frame with number: {frame} found")
            }
            Self::BadFile => write!(out, "EBADF:Failed to read image file"),
            Self::NoSpace => write!(out, "ENOSPC:image storage is full"),
            Self::NoParentImage { parent } => write!(
                out,
                "ENOPARENT:Put command refers to a parent image with id: {parent} that does not exist"
            ),
            Self::NoParentPlacement { parent, placement } => write!(
                out,
                "ENOPARENT:Put command refers to a parent image placement with id: {parent} and placement id: {placement} that does not exist"
            ),
            Self::SelfParent => {
                write!(out, "EINVAL:Put command refers to itself as its own parent")
            }
            Self::Cycle => write!(out, "ECYCLE:relative placement would make a cycle"),
            Self::TooDeep => write!(out, "ETOODEEP:Too many levels of parent references"),
            Self::VirtualParent => write!(
                out,
                "EINVAL:Put command creating a virtual placement cannot refer to a parent"
            ),
            Self::BothIds => write!(
                out,
                "EINVAL:Must not specify both image id and image number"
            ),
            Self::UnknownFormat { format } => write!(out, "EINVAL:Unknown image format: {format}"),
            Self::TooLarge { max } => write!(
                out,
                "EINVAL:Image too large, width or height greater than {max}"
            ),
            Self::NoData { got, want } => {
                write!(out, "ENODATA:Insufficient image data: {got} < {want}")
            }
            Self::ZeroSize => write!(out, "EINVAL:Zero width/height not allowed"),
            Self::IndexedFrame => write!(
                out,
                "EINVAL:Animation is not supported for indexed color images"
            ),
            Self::IndexedComposition => write!(
                out,
                "EINVAL:Composition is not supported for indexed color images"
            ),
            Self::InflateSize => write!(
                out,
                "EINVAL:Image data size post inflation does not match expected size"
            ),
            Self::ComposeOverlap => write!(
                out,
                "EINVAL:The source and destination rectangles overlap and the src and destination frames are the same"
            ),
            Self::ComposeDestOut => {
                write!(out, "EINVAL:The destination rectangle is out of bounds")
            }
            Self::Invalid => write!(out, "EINVAL:invalid argument"),
        };
    }
}

/// What a command is answered: OK, or its error.
pub(super) type Answer = Result<(), Error>;

/// The bit of the set of keys seen for key letter `key`.
pub(super) const fn bit(key: u8) -> u64 {
    match key {
        b'a'..=b'z' => 1u64.wrapping_shl(key.wrapping_sub(b'a') as u32),
        b'A'..=b'Z' => 1u64.wrapping_shl(key.wrapping_sub(b'A').wrapping_add(26) as u32),
        _ => 0,
    }
}

/// One command's control keys, with the spec's defaults for those not
/// given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Command {
    /// The bit of each key given.
    pub(super) seen: u64,
    /// A key could not be read: its value is not a number of its kind, a
    /// letter key holds more than one letter, or a value is outside what
    /// the spec allows.
    pub(super) bad: bool,
    /// `a`: the action.
    pub(super) action: u8,
    /// `t`: the medium.
    pub(super) medium: u8,
    /// `o`: the compression, 0 for none.
    pub(super) compression: u8,
    /// `d`: what a delete chooses.
    pub(super) delete: u8,
    /// `e`: the action a feature query asks about, or 0 when `e` is not
    /// one letter (ADR-1100, D14).
    pub(super) ask: u8,
    /// `q`: which replies are left out.
    pub(super) quiet: u32,
    /// `f`: the format: 8 or 16 palette indices, 24 RGB, 32 RGBA or 100
    /// PNG.
    pub(super) format: u32,
    /// `s`, `v`: the picture's width and height in pixels.
    pub(super) width: u32,
    pub(super) height: u32,
    /// `S`, `O`: the bytes to read from a named medium and where to start.
    pub(super) size: u32,
    pub(super) offset: u32,
    /// `i`, `I`: the image id and the image number.
    pub(super) id: u32,
    pub(super) number: u32,
    /// `p`: the placement id.
    pub(super) placement: u32,
    /// `m`: more chunks follow.
    pub(super) more: u32,
    /// `N`: the usage hints; bit 0 marks the image transient.
    pub(super) hints: u32,
    /// `x`, `y`, `w`, `h`: the source rectangle, or for a delete its
    /// coordinates and id range.
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) w: u32,
    pub(super) h: u32,
    /// `X`, `Y`: the picture's offset into its first cell.
    pub(super) cell_x: u32,
    pub(super) cell_y: u32,
    /// `c`, `r`: the box in cells.
    pub(super) cols: u32,
    pub(super) rows: u32,
    /// `C`: 1 leaves the cursor where it is.
    pub(super) cursor: u32,
    /// `U`: 1 asks for a virtual placement, for Unicode placeholders.
    pub(super) virtual_placement: u32,
    /// `P`, `Q`: a relative placement's parent image and placement.
    pub(super) parent: u32,
    pub(super) parent_placement: u32,
    /// `z`: the depth.
    pub(super) z: i32,
    /// `H`, `V`: a relative placement's offset from its parent, in cells.
    pub(super) offset_cols: i32,
    pub(super) offset_rows: i32,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            seen: 0,
            bad: false,
            action: b't',
            medium: b'd',
            compression: 0,
            delete: b'a',
            ask: 0,
            quiet: 0,
            format: 32,
            width: 0,
            height: 0,
            size: 0,
            offset: 0,
            id: 0,
            number: 0,
            placement: 0,
            more: 0,
            hints: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            cell_x: 0,
            cell_y: 0,
            cols: 0,
            rows: 0,
            cursor: 0,
            virtual_placement: 0,
            parent: 0,
            parent_placement: 0,
            z: 0,
            offset_cols: 0,
            offset_rows: 0,
        }
    }
}

/// A decimal number of up to 32 bits, all digits.
pub(super) fn number(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for &d in digits {
        if !d.is_ascii_digit() {
            return None;
        }
        value = value
            .checked_mul(10)?
            .checked_add(u32::from(d.wrapping_sub(b'0')))?;
    }
    Some(value)
}

/// A signed decimal number of 32 bits.
pub(super) fn signed(text: &[u8]) -> Option<i32> {
    match text.split_first() {
        Some((b'-', digits)) => i32::try_from(i64::from(number(digits)?).checked_neg()?).ok(),
        _ => i32::try_from(number(text)?).ok(),
    }
}

impl Command {
    /// Reads the comma-separated `KEY=VALUE` pairs of `keys`. An empty pair,
    /// as a leading, trailing or doubled comma makes, is skipped; a key the
    /// spec does not define is ignored; a value that cannot be read marks
    /// the command bad while the other keys are still read, so a refusal
    /// can name the image.
    pub(super) fn parse(keys: &[u8]) -> Self {
        let mut cmd = Self::default();
        for item in keys.split(|&b| b == b',') {
            let (key, value) = match item {
                [] => continue,
                [key, b'=', value @ ..] if key.is_ascii_alphabetic() && !value.is_empty() => {
                    (*key, value)
                }
                _ => {
                    cmd.bad = true;
                    continue;
                }
            };
            cmd.seen |= bit(key);
            if !cmd.set(key, value) {
                cmd.bad = true;
            }
        }
        let known = |set: &[u8], c: u8| set.contains(&c);
        if !known(b"tTqpdfac", cmd.action)
            || !known(b"dfts", cmd.medium)
            || (cmd.seen & bit(b'o') != 0 && cmd.compression != b'z')
            || !matches!(cmd.format, 8 | 16 | 24 | 32 | 100)
            || !known(b"aAiInNcCfFpPqQrRxXyYzZ", cmd.delete)
        {
            cmd.bad = true;
        }
        cmd
    }

    /// Sets key `key` from `value`; false when the value cannot be read.
    fn set(&mut self, key: u8, value: &[u8]) -> bool {
        let letter = match value {
            [c] => Some(*c),
            _ => None,
        };
        let slot = match key {
            b'a' => return letter.map(|c| self.action = c).is_some(),
            b't' => return letter.map(|c| self.medium = c).is_some(),
            b'o' => return letter.map(|c| self.compression = c).is_some(),
            b'd' => return letter.map(|c| self.delete = c).is_some(),
            // any value makes a feature query; one that names no action
            // is answered as an action vtium does not know
            b'e' => {
                self.ask = letter.unwrap_or(0);
                return true;
            }
            b'z' => return signed(value).map(|v| self.z = v).is_some(),
            b'H' => return signed(value).map(|v| self.offset_cols = v).is_some(),
            b'V' => return signed(value).map(|v| self.offset_rows = v).is_some(),
            b'q' => &mut self.quiet,
            b'f' => &mut self.format,
            b's' => &mut self.width,
            b'v' => &mut self.height,
            b'S' => &mut self.size,
            b'O' => &mut self.offset,
            b'i' => &mut self.id,
            b'I' => &mut self.number,
            b'p' => &mut self.placement,
            b'm' => &mut self.more,
            b'N' => &mut self.hints,
            b'x' => &mut self.x,
            b'y' => &mut self.y,
            b'w' => &mut self.w,
            b'h' => &mut self.h,
            b'X' => &mut self.cell_x,
            b'Y' => &mut self.cell_y,
            b'c' => &mut self.cols,
            b'r' => &mut self.rows,
            b'C' => &mut self.cursor,
            b'U' => &mut self.virtual_placement,
            b'P' => &mut self.parent,
            b'Q' => &mut self.parent_placement,
            // a key the spec does not define is ignored
            _ => return true,
        };
        number(value).map(|v| *slot = v).is_some()
    }

    /// `a=t`, `a=T`, `a=q` and `a=f` carry data.
    pub(super) fn transmits(&self) -> bool {
        matches!(self.action, b't' | b'T' | b'q' | b'f')
    }

    /// The command carries `e`, which makes a query a feature query and
    /// any other action a refusal (ADR-1100, D14).
    pub(super) fn asks(&self) -> bool {
        self.seen & bit(b'e') != 0
    }

    /// A direct transmission: `a=t`, `a=T` or `a=f`, or no `a`, with no
    /// `t` or `t=d` (ADR-1100, D16).
    pub(super) fn direct(&self) -> bool {
        matches!(self.action, b't' | b'T' | b'f') && self.medium == b'd'
    }

    /// An `a=q` without `i`, with or without `I`: it reads nothing and is
    /// answered under no `q`.
    pub(super) fn dropped(&self) -> bool {
        self.action == b'q' && self.id == 0
    }

    /// A direct transmission of a raw format: the bytes a pixel.
    pub(super) fn bpp(&self) -> u8 {
        match self.format {
            8 => 1,
            16 => 2,
            24 => 3,
            _ => 4,
        }
    }

    /// The pixels are palette indices, `f=8` or `f=16` (ADR-1100, D15).
    pub(super) fn indexed(&self) -> bool {
        matches!(self.format, 8 | 16)
    }

    /// Whether the helper decodes this transmission's data.
    pub(super) fn for_helper(&self) -> bool {
        self.format == 100 || self.compression == b'z'
    }

    /// A put makes a relative placement: it names a parent image with `P`
    /// (D13). An id of 0 names no image anywhere in the protocol, so `P=0`
    /// is no parent, and `Q`, `H` and `V` without a parent do nothing.
    pub(super) fn relative(&self) -> bool {
        self.parent != 0
    }

    /// A put makes a virtual placement, for Unicode placeholders: `U=1`.
    pub(super) fn virtual_placement(&self) -> bool {
        self.virtual_placement == 1
    }

    /// What is refused before any data is read: a key that cannot be read;
    /// `i` with `I`, as the spec asks; a `U` other than 0 or 1, and `U=1` with a parent, since a virtual
    /// placement is never a relative one (D13); a raw upload without `s` and
    /// `v`, answered as kitty answers it (D15), or past `max-graphic-size`; palette indices in a frame, from a named medium
    /// or with `o=z` (ADR-1100, D15); and a placement whose pixel offset
    /// lies outside its cell or whose box passes [`BOX_CELLS_MAX`].
    pub(super) fn refusal(&self, ctx: &dyn Context) -> Option<Error> {
        if !matches!(self.format, 8 | 16 | 24 | 32 | 100) {
            return Some(Error::UnknownFormat {
                format: self.format,
            });
        }
        if self.id != 0 && self.number != 0 {
            return Some(Error::BothIds);
        }
        if self.virtual_placement() && self.relative() {
            return Some(Error::VirtualParent);
        }
        if self.bad
            || self.virtual_placement > 1
            || (self.transmits()
                && self.action != b'f'
                && self.indexed()
                && (self.medium != b'd' || self.compression == b'z'))
        {
            return Some(Error::Invalid);
        }
        let (max_w, max_h) = graphic(ctx);
        let raw = self.transmits() && self.format != 100;
        // a frame's size is checked once its image is found (frame_target)
        if raw && self.action != b'f' && (self.width == 0 || self.height == 0) {
            return Some(Error::ZeroSize);
        }
        if raw && self.width > max_w {
            return Some(Error::TooLarge { max: max_w });
        }
        if raw && self.height > max_h {
            return Some(Error::TooLarge { max: max_h });
        }
        let (cw, ch) = ctx.cell_size();
        if matches!(self.action, b'T' | b'p')
            && (self.cell_x >= cw
                || self.cell_y >= ch
                || u64::from(self.cols) > BOX_CELLS_MAX
                || u64::from(self.rows) > BOX_CELLS_MAX)
        {
            return Some(Error::Invalid);
        }
        None
    }

    /// Which of vtium's bounds refuses this command, where one does: the
    /// picture's size past `max-graphic-size`, or its box past
    /// `BOX_CELLS_MAX`. [`refusal`](Self::refusal) answers EINVAL for
    /// both; this names the bound for the log, since the operator can act
    /// on a setting where only the program's author can act on a bad key.
    pub(super) fn bound(&self, ctx: &dyn Context) -> Option<&'static str> {
        let (max_w, max_h) = graphic(ctx);
        if self.transmits() && self.format != 100 && (self.width > max_w || self.height > max_h) {
            return Some("a kitty picture past max-graphic-size");
        }
        if matches!(self.action, b'T' | b'p')
            && (u64::from(self.cols) > BOX_CELLS_MAX || u64::from(self.rows) > BOX_CELLS_MAX)
        {
            return Some("a kitty picture's box passes BOX_CELLS_MAX");
        }
        None
    }

    /// The pixels the raw picture holds.
    pub(super) fn pixels(&self) -> u64 {
        u64::from(self.width).saturating_mul(u64::from(self.height))
    }
}

/// `max-graphic-size`, bounded by the store's largest side.
pub(super) fn graphic(ctx: &dyn Context) -> (u32, u32) {
    let side = u32::try_from(IMAGE_SIDE_MAX).unwrap_or(u32::MAX);
    let (w, h) = ctx.limits().graphic;
    (w.min(side), h.min(side))
}

/// The bytes the store charges an image of `pixels` pixels.
pub(super) fn charge(pixels: u64) -> u64 {
    pixels.saturating_mul(4).saturating_add(IMAGE_OVERHEAD)
}

/// The frame a frame command's reply names with `r`, as kitty 0.50.0's
/// replies do (ADR-1102, D2): the frame an edit names, or else the frame a
/// new frame made, the image's last, or on a refusal the number it would
/// have had. A refusal for palette indices names only the `r` the command
/// gave, and a reply to anything but a frame names none.
fn frame_named(store: &Store, cmd: &Command, key: ImageKey, answer: Answer) -> Option<u32> {
    if cmd.action != b'f' {
        return None;
    }
    if cmd.rows != 0 || answer == Err(Error::IndexedFrame) {
        return (cmd.rows != 0).then_some(cmd.rows);
    }
    let frames = store.animation(key).map_or(1, |a| a.frames);
    Some(if answer.is_ok() {
        frames
    } else {
        frames.saturating_add(1)
    })
}

/// Answers `cmd` with `answer` for the image of id `id`, 0 when there is
/// none to name: `ESC _ G [i=ID][,I=N][,p=P][,r=R];MESSAGE ESC \`, each key
/// only when it is set, as kitty 0.50.0 writes it: `I` always after a
/// comma, and an animation command names `i` only when the command gave it
/// (ADR-1100, D15).
pub(super) fn reply(ctx: &mut dyn Context, cmd: &Command, id: u32, answer: Answer) {
    // noted whether or not the program hears it
    if answer == Err(Error::NoSpace) {
        ctx.note(Cause::Console, "the budget refuses a kitty picture");
    }
    if (cmd.id == 0 && cmd.number == 0) || cmd.dropped() {
        return;
    }
    let key = if matches!(cmd.action, b'f' | b'a' | b'c') {
        ctx.find_image(cmd.id, cmd.number)
    } else {
        None
    };
    // a frame to an image that does not exist names only the r it gave
    let frame = match key {
        Some(key) => frame_named(ctx.store(), cmd, key, answer),
        None => (cmd.action == b'f' && cmd.rows != 0).then_some(cmd.rows),
    };
    let quiet = match answer {
        Ok(()) => cmd.quiet >= 1,
        Err(_) => cmd.quiet >= 2,
    };
    if quiet {
        return;
    }
    let mut text = String::from("\x1b_G");
    if id != 0 {
        let _ = write!(text, "i={id}");
    }
    // kitty writes I after a comma even with no i before it
    if cmd.number != 0 {
        let _ = write!(text, ",I={}", cmd.number);
    }
    if cmd.placement != 0 {
        let _ = write!(text, ",p={}", cmd.placement);
    }
    if let Some(r) = frame {
        let _ = write!(text, ",r={r}");
    }
    text.push(';');
    match answer {
        Ok(()) => text.push_str("OK"),
        Err(e) => e.write_to(&mut text),
    }
    text.push_str("\x1b\\");
    ctx.reply(ReplyKind::KittyGraphics, text.as_bytes());
}
