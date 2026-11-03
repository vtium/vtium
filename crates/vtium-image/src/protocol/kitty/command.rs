//! A kitty graphics command's control keys, the refusals made before any
//! data is read, and the reply the handler writes (ADR-1100, D6).

use core::fmt::Write as _;

use crate::bounds::{BOX_CELLS_MAX, IMAGE_OVERHEAD, IMAGE_SIDE_MAX};
use crate::handler::{Context, ReplyKind};

/// What a command is answered when it fails: each a fixed message of the
/// closed set of kitty replies (D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    /// No image of that id or number.
    NotFound,
    /// Any failure to read a file, temporary file or shared-memory object:
    /// the spec's one reply for all of them.
    BadFile,
    /// The budget refused.
    NoSpace,
    /// A relative placement's parent does not exist.
    NoParent,
    /// A relative placement would follow itself.
    Cycle,
    /// A chain of relative placements would pass `RELATIVE_DEPTH_MAX`.
    TooDeep,
    /// Anything else the handler cannot do.
    Invalid,
}

impl Error {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::NotFound => "ENOENT:image not found",
            Self::BadFile => "EBADF:Failed to read image file",
            Self::NoSpace => "ENOSPC:image storage is full",
            Self::NoParent => "ENOPARENT:parent placement not found",
            Self::Cycle => "ECYCLE:relative placement would make a cycle",
            Self::TooDeep => "ETOODEEP:chain of relative placements is too deep",
            Self::Invalid => "EINVAL:invalid argument",
        }
    }
}

/// What a command is answered: OK, or an error.
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
    /// `q`: which replies are left out.
    pub(super) quiet: u32,
    /// `f`: the format, 24, 32 or 100.
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
            || !matches!(cmd.format, 24 | 32 | 100)
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

    /// An `a=q` without `i`, with or without `I`: it reads nothing and is
    /// answered under no `q`.
    pub(super) fn dropped(&self) -> bool {
        self.action == b'q' && self.id == 0
    }

    /// A direct transmission of a raw format: the bytes a pixel.
    pub(super) fn bpp(&self) -> u8 {
        if self.format == 24 { 3 } else { 4 }
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
    /// placement is never a relative one (D13); a raw format without `s` and
    /// `v`, which a frame's data alone may leave out, or past
    /// `max-graphic-size`; and a placement whose pixel offset lies outside
    /// its cell or whose box passes [`BOX_CELLS_MAX`].
    pub(super) fn refusal(&self, ctx: &dyn Context) -> Option<Error> {
        if self.bad
            || (self.id != 0 && self.number != 0)
            || self.virtual_placement > 1
            || (self.virtual_placement() && self.relative())
        {
            return Some(Error::Invalid);
        }
        let (max_w, max_h) = graphic(ctx);
        let sized = self.action == b'f' || (self.width != 0 && self.height != 0);
        if self.transmits()
            && self.format != 100
            && (!sized || self.width > max_w || self.height > max_h)
        {
            return Some(Error::Invalid);
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

    /// Whether this command continues the upload in chunks begun by
    /// `first`: it carries only `m` and `q`, and `a=f` too when the upload
    /// began with `a=f`, as the spec asks of animation frames.
    pub(super) fn continues(&self, first: &Self) -> bool {
        let mut keys = bit(b'm') | bit(b'q');
        if first.action == b'f' && self.action == b'f' {
            keys |= bit(b'a');
        }
        self.seen & !keys == 0
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

/// Answers `cmd` with `answer` for the image of id `id`, 0 when there is
/// none to name: `ESC _ G i=ID[,I=N][,p=P];MESSAGE ESC \`, each key only
/// when it is set. An animation command, `a=f`, `a=a` or `a=c`, names an
/// image that already exists, so every answer to it, a refusal as well as
/// an OK, names that image's id; `id` names it only when no image is found.
pub(super) fn reply(ctx: &mut dyn Context, cmd: &Command, id: u32, answer: Answer) {
    if (cmd.id == 0 && cmd.number == 0) || cmd.dropped() {
        return;
    }
    let id = if matches!(cmd.action, b'f' | b'a' | b'c') {
        ctx.find_image(cmd.id, cmd.number)
            .and_then(|key| ctx.store().kitty_id(key))
            .unwrap_or(id)
    } else {
        id
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
    if cmd.number != 0 {
        let comma = if id != 0 { "," } else { "" };
        let _ = write!(text, "{comma}I={}", cmd.number);
    }
    if cmd.placement != 0 {
        let _ = write!(text, ",p={}", cmd.placement);
    }
    let message = match answer {
        Ok(()) => "OK",
        Err(e) => e.message(),
    };
    let _ = write!(text, ";{message}\x1b\\");
    ctx.reply(ReplyKind::KittyGraphics, text.as_bytes());
}
