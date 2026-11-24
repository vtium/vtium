//! The packets between vtium and the image helper (ADR-1101 D7), encoded
//! and decoded here alone, since both ends link this module.
//!
//! The socket is a Unix socket pair of the sequenced-packet kind, so the
//! kernel keeps each packet whole. Each packet begins with its kind and
//! its whole length, as the font helper's frames do, then fixed fields,
//! all little-endian, then a variable part:
//!
//! - The helper's first packet is a [`Hello`]: its wire version, the
//!   landlock version the kernel offers, whether its seccomp filter is in
//!   force, and why its seccomp filter and its workers' landlock domains
//!   are not, as a [`NoSeccomp`] and a [`NoLandlock`] at 20 and 24, each 0
//!   when the part is in force.
//! - A request is ADR-1100 D7's [`Request`], field for field, followed by
//!   the name of a named medium. Inline bytes travel beside it in a sealed
//!   memory file, never in the packet, so a decoded request's
//!   [`Medium::Direct`] is empty.
//! - A reply is a [`ReplyHeader`]: the outcome, the size of a decoded
//!   picture, the bytes ignored, whether its memory file is a pooled one,
//!   each frame's delay and the loop count of an animated GIF, and a reason
//!   for vtium's log. The pixels travel in the memory file.
//!
//! Request layout, after the kind and the length:
//!
//! | Offset | Field |
//! |---|---|
//! | 8 | version, `u32` |
//! | 12 | medium letter, `u8` |
//! | 13 | flags, `u8`: [`INFLATE`], [`PNG`], [`EXACT`], [`DISCARD`], [`ANY_IMAGE`], [`FRAMES`] |
//! | 14 | the pixel description's layout, `u8`: 0 for none, 1 for a palette index, 2 for an index and its alpha, 3 for RGB, 4 for RGBA |
//! | 15 | its alpha, `u8`: 0 straight, 1 premultiplied |
//! | 16 | the request's id, `u64` |
//! | 24 | offset, `u64` |
//! | 32 | the most bytes to read, `u64`, 0 meaning to the end |
//! | 40 | the fewest bytes the picture needs, `u64` |
//! | 48 | `max_bytes`, `u64` |
//! | 56 | the largest width and height, `u32` each |
//! | 64 | the described width and height, `u32` each |
//! | 72 | the name's length, `u32` |
//! | 76 | `max_frames`, `u32`, 0 without [`FRAMES`] |
//! | 80 | `max_frame_bytes`, `u64`, 0 without [`FRAMES`] |
//! | 88 | the login picture's fit, `u8`: 0 for none, 1 `cover`, 2 `contain`, 3 `stretch`, 4 `none`, 5 `banner`; its position, `u8`: 0 unset, then 1 to 9 for `top-left`, `top`, `top-right`, `left`, `center`, `right`, `bottom-left`, `bottom` and `bottom-right`; its scale, `u8`, 1 to 100; a reserved byte |
//! | 92 | the banner's rows, `u32`, 0 for its own size |
//! | 96 | the display's width and height, the cell's, and the grid's columns and rows, `u32` each |
//! | 120 | the name |
//!
//! A described picture's zlib compression travels as the [`INFLATE`] flag.
//!
//! Reply layout: version at 8, status at 12, the id at 16, width and height
//! at 24 and 28, the bytes ignored at 32, the pooled flag at 36 and three
//! reserved bytes, the reason's length at 40, the frame count at 44, the
//! loop count at 48, whether the GIF gives one at 50 and a reserved byte,
//! then each frame's delay, 2 bytes a frame, then the reason. A reply with
//! a picture counts one frame or more, and any other reply none.

use super::{Alpha, Description, FrameLimits, Layout, Medium, Request};
use crate::login::{Fit, Fitting, Form, Geometry, Position};

/// The wire's version, which both ends of one build share.
pub const VERSION: u32 = 5;

/// The helper's first packet.
pub const KIND_HELLO: u32 = 1;
/// A request from vtium.
pub const KIND_REQUEST: u32 = 2;
/// A reply from the helper.
pub const KIND_REPLY: u32 = 3;

/// Inflate zlib (kitty's `o=z`).
pub const INFLATE: u8 = 0x01;
/// Decode PNG.
pub const PNG: u8 = 0x02;
/// The inflated bytes must number exactly the fewest the picture needs.
pub const EXACT: u8 = 0x04;
/// Remove a temporary file or shared-memory object and read nothing.
pub const DISCARD: u8 = 0x08;
/// Decode a PNG, JPEG or GIF file by its first bytes (ADR-1103).
pub const ANY_IMAGE: u8 = 0x10;
/// Return every frame of an animated GIF (ADR-1101 D7), only beside
/// [`ANY_IMAGE`].
pub const FRAMES: u8 = 0x20;
const FLAGS: u8 = INFLATE | PNG | EXACT | DISCARD | ANY_IMAGE | FRAMES;

/// The longest name of a file or shared-memory object, in bytes: the
/// kernel's `PATH_MAX` less its terminating NUL.
pub const NAME_MAX: usize = 4095;
/// The longest reason a reply carries, in bytes.
pub const REASON_MAX: usize = 255;

/// The bytes of the kind and the length that begin every packet.
pub const HEADER: usize = 8;
/// The bytes of a hello.
pub const HELLO_BYTES: usize = 28;
/// The bytes of a request before its name.
pub const REQUEST_FIXED: usize = 120;
/// The bytes of a reply before its delays and its reason.
pub const REPLY_FIXED: usize = 52;
/// The most frames a reply counts: ADR-1102's `FRAMES_MAX`.
pub const REPLY_FRAMES_MAX: usize = 1024;
/// The most bytes any packet takes: a request with the longest name, or a
/// reply with the most frames and the longest reason.
pub const PACKET_MAX: usize = {
    let request = REQUEST_FIXED + NAME_MAX;
    let reply = REPLY_FIXED + 2 * REPLY_FRAMES_MAX + REASON_MAX;
    if request > reply { request } else { reply }
};

/// Why a packet is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Malformed(pub &'static str);

impl core::fmt::Display for Malformed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.0)
    }
}

/// The helper's first packet, once it has set itself up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hello {
    pub version: u32,
    /// The landlock version the kernel offers, 0 for none.
    pub landlock: u32,
    /// The helper's seccomp filter is in force.
    pub seccomp: bool,
    /// Why the helper's seccomp filter is not in force; `None` when it is.
    pub no_seccomp: Option<NoSeccomp>,
    /// Why the workers enter no landlock domain; `None` when they do.
    pub no_landlock: Option<NoLandlock>,
}

/// Why the image helper's seccomp filter is not in force, as its hello
/// numbers it.
///
/// References:
/// - ADR-1200 D15, "Nothing on screen, and the log says why": the five
///   reasons, which hold for both helpers.
/// - ADR-1101 D7: the hello carries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoSeccomp {
    /// `helper-sandbox=off` turns the filters off.
    Off = 1,
    /// vtium builds no filter for this architecture.
    NoTable = 2,
    /// The kernel is 3.8 or later and was built without seccomp: its
    /// status has no `Seccomp:` line.
    NotBuilt = 3,
    /// The kernel is 5.9 or later and was built with seccomp but without
    /// seccomp filters: its status has no `Seccomp_filters:` line.
    NoFilters = 4,
    /// The kernel is older than 3.8 and its status has no `Seccomp:` line,
    /// so it is too old for the filters or was built without seccomp.
    TooOld = 5,
}

/// Why the image helper's workers enter no landlock domain, as its hello
/// numbers it.
///
/// References:
/// - ADR-1200 D15, "Nothing on screen, and the log says why": the four
///   reasons a landlock domain is not in force.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoLandlock {
    /// `helper-sandbox=off` turns the domains off.
    Off = 1,
    /// The kernel is older than landlock, Linux 5.13.
    TooOld = 2,
    /// The kernel is 5.13 or later and was built without landlock, so
    /// landlock's version query answers `ENOSYS`.
    NotBuilt = 3,
    /// The kernel was built with landlock but did not turn it on at boot,
    /// so the version query answers `EOPNOTSUPP`.
    NotEnabled = 4,
}

impl NoSeccomp {
    fn from_wire(n: u32) -> Result<Option<Self>, Malformed> {
        Ok(Some(match n {
            0 => return Ok(None),
            1 => Self::Off,
            2 => Self::NoTable,
            3 => Self::NotBuilt,
            4 => Self::NoFilters,
            5 => Self::TooOld,
            _ => return Err(Malformed("a hello with an unknown seccomp reason")),
        }))
    }
}

impl NoLandlock {
    fn from_wire(n: u32) -> Result<Option<Self>, Malformed> {
        Ok(Some(match n {
            0 => return Ok(None),
            1 => Self::Off,
            2 => Self::TooOld,
            3 => Self::NotBuilt,
            4 => Self::NotEnabled,
            _ => return Err(Malformed("a hello with an unknown landlock reason")),
        }))
    }
}

/// A reply's outcome, as the wire numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok = 0,
    Unreadable = 1,
    TooShort = 2,
    Corrupt = 3,
    TooLarge = 4,
    OverBudget = 5,
}

impl Status {
    /// The status the wire numbers `n`.
    pub fn from_wire(n: u32) -> Option<Self> {
        Some(match n {
            0 => Self::Ok,
            1 => Self::Unreadable,
            2 => Self::TooShort,
            3 => Self::Corrupt,
            4 => Self::TooLarge,
            5 => Self::OverBudget,
            _ => return None,
        })
    }

    /// The reply of an outcome other than OK; `None` for OK, whose reply
    /// carries the pixels.
    pub fn failure(self) -> Option<super::Reply> {
        use super::Reply;
        Some(match self {
            Self::Ok => return None,
            Self::Unreadable => Reply::Unreadable,
            Self::TooShort => Reply::TooShort,
            Self::Corrupt => Reply::Corrupt,
            Self::TooLarge => Reply::TooLarge,
            Self::OverBudget => Reply::OverBudget,
        })
    }
}

/// A reply's fixed fields and reason; its memory file travels beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyHeader {
    pub status: Status,
    pub id: u64,
    /// A picture's size; 0 for any other reply.
    pub width: u32,
    pub height: u32,
    /// Bytes of the source the helper did not use.
    pub ignored: u32,
    /// The memory file is one of a stream's pooled files, sealed against
    /// shrinking and growing only, which the console reads with `pread`.
    pub pooled: bool,
    /// Each frame's delay in hundredths of a second, one for each frame the
    /// reply's picture holds: one for a still picture, more for an animated
    /// GIF, none for a reply without a picture.
    pub delays: Vec<u16>,
    /// An animated GIF's loop count, 0 meaning forever, or `None` when it
    /// gives none or the reply is no animation.
    pub loops: Option<u16>,
    /// At most [`REASON_MAX`] bytes for vtium's log.
    pub reason: Vec<u8>,
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn get32(p: &[u8], at: usize) -> u32 {
    p.get(at..at.saturating_add(4))
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map_or(0, u32::from_le_bytes)
}

fn get64(p: &[u8], at: usize) -> u64 {
    p.get(at..at.saturating_add(8))
        .and_then(|b| <[u8; 8]>::try_from(b).ok())
        .map_or(0, u64::from_le_bytes)
}

fn get8(p: &[u8], at: usize) -> u8 {
    p.get(at).copied().unwrap_or(0)
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The kind and length of `packet`, checked: its length field must be its
/// length, and it must hold at least `fixed` bytes.
fn header(packet: &[u8], kind: u32, fixed: usize) -> Result<(), Malformed> {
    if packet.len() < fixed {
        return Err(Malformed("a packet shorter than its fixed fields"));
    }
    if get32(packet, 0) != kind {
        return Err(Malformed("a packet of another kind"));
    }
    if usize::try_from(get32(packet, 4)).ok() != Some(packet.len()) {
        return Err(Malformed("a packet whose length field is not its length"));
    }
    if get32(packet, 8) != VERSION {
        return Err(Malformed("a packet of another version"));
    }
    Ok(())
}

/// The hello's bytes.
pub fn encode_hello(hello: &Hello) -> Vec<u8> {
    let mut out = Vec::with_capacity(HELLO_BYTES);
    put32(&mut out, KIND_HELLO);
    put32(&mut out, len32(HELLO_BYTES));
    put32(&mut out, hello.version);
    put32(&mut out, hello.landlock);
    put32(&mut out, u32::from(hello.seccomp));
    put32(&mut out, hello.no_seccomp.map_or(0, |n| n as u32));
    put32(&mut out, hello.no_landlock.map_or(0, |n| n as u32));
    out
}

/// The wire version a hello in `packet` names, whether or not it is this
/// build's, so a refusal can say which version the helper speaks; `None`
/// for a packet too short or of another kind.
pub fn hello_version(packet: &[u8]) -> Option<u32> {
    (packet.len() >= 12 && get32(packet, 0) == KIND_HELLO).then(|| get32(packet, 8))
}

/// The hello in `packet`, which must be exactly one.
pub fn decode_hello(packet: &[u8]) -> Result<Hello, Malformed> {
    header(packet, KIND_HELLO, HELLO_BYTES)?;
    if packet.len() != HELLO_BYTES {
        return Err(Malformed("a hello of the wrong length"));
    }
    let seccomp = match get32(packet, 16) {
        0 => false,
        1 => true,
        _ => return Err(Malformed("a hello with an unknown seccomp state")),
    };
    let no_seccomp = NoSeccomp::from_wire(get32(packet, 20))?;
    if seccomp == no_seccomp.is_some() {
        return Err(Malformed(
            "a hello whose seccomp filter is both in force and missing, or neither",
        ));
    }
    Ok(Hello {
        version: VERSION,
        landlock: get32(packet, 12),
        seccomp,
        no_seccomp,
        no_landlock: NoLandlock::from_wire(get32(packet, 24))?,
    })
}

/// The flags `request` sets.
fn flags(request: &Request) -> u8 {
    [
        (request.inflate, INFLATE),
        (request.png, PNG),
        (request.exact, EXACT),
        (request.discard, DISCARD),
        (request.any_image, ANY_IMAGE),
        (request.frames.is_some(), FRAMES),
    ]
    .into_iter()
    .filter_map(|(on, bit)| on.then_some(bit))
    .fold(0, |a, b| a | b)
}

/// The packet of request `id`, or why vtium may not send it: whatever
/// [`decode_request`] refuses, and a description whose compression is not
/// the request's `inflate`, which the wire carries once.
pub fn encode_request(id: u64, request: &Request) -> Result<Vec<u8>, Malformed> {
    if request
        .pixels
        .is_some_and(|d| d.compressed != request.inflate)
    {
        return Err(Malformed(
            "a description whose compression is not the request's",
        ));
    }
    let name = request.medium.name();
    let (layout, alpha, width, height) = match request.pixels {
        None => (0, 0, 0, 0),
        Some(d) => (
            u8::try_from(d.layout.bytes()).unwrap_or(0),
            match d.alpha {
                Alpha::Straight => 0,
                Alpha::Premultiplied => 1,
            },
            d.width,
            d.height,
        ),
    };
    let total = REQUEST_FIXED.saturating_add(name.len());
    let mut out = Vec::with_capacity(total);
    put32(&mut out, KIND_REQUEST);
    put32(&mut out, len32(total));
    put32(&mut out, VERSION);
    let letter = u8::try_from(u32::from(request.medium.letter())).unwrap_or(0);
    out.extend_from_slice(&[letter, flags(request), layout, alpha]);
    put64(&mut out, id);
    put64(&mut out, request.offset);
    put64(&mut out, request.max);
    put64(&mut out, request.min);
    put64(&mut out, request.largest_bytes);
    put32(&mut out, request.largest.0);
    put32(&mut out, request.largest.1);
    put32(&mut out, width);
    put32(&mut out, height);
    put32(&mut out, len32(name.len()));
    let limits = request
        .frames
        .map_or((0, 0), |l| (l.max_frames, l.max_frame_bytes));
    put32(&mut out, limits.0);
    put64(&mut out, limits.1);
    put_fitting(&mut out, request.fit.as_ref());
    out.extend_from_slice(name);
    decode_request(&out)?;
    Ok(out)
}

/// The id and request in `packet`, or why it is refused: version 2's
/// refusals (a length that is not the fixed fields and the name,
/// a version, flag or reserved value it does not know, a medium with the
/// wrong name, exact without inflate, a discard with another flag, of a file or inline bytes, or with
/// a bound, and more bytes needed than the budget), and those of the pixel
/// description: a layout or alpha it does not know, a description beside
/// PNG, any image or a discard, a described size of no pixels or whose
/// bytes are not the fewest the request needs, raw pixels without a
/// description (which also refuses a raw request that needs no bytes), inline raw pixels without zlib, which the console decodes
/// itself, and any image from anything but inline bytes. A name holding a
/// NUL byte is not refused here; the helper answers it UNREADABLE.
pub fn decode_request(packet: &[u8]) -> Result<(u64, Request), Malformed> {
    header(packet, KIND_REQUEST, REQUEST_FIXED)?;
    let flags = get8(packet, 13);
    if flags & !FLAGS != 0 {
        return Err(Malformed("a request with an unknown flag"));
    }
    let name_len = usize::try_from(get32(packet, 72)).unwrap_or(usize::MAX);
    if REQUEST_FIXED.checked_add(name_len) != Some(packet.len()) {
        return Err(Malformed(
            "a request whose length is not its fields and its name",
        ));
    }
    let name = packet.get(REQUEST_FIXED..).unwrap_or_default().to_vec();
    let medium = match get8(packet, 12) {
        b'd' if name.is_empty() => Medium::Direct(Vec::new()),
        b'f' | b't' | b's' if name.is_empty() || name.len() > NAME_MAX => {
            return Err(Malformed("a named medium with no name or a name too long"));
        }
        b'f' => Medium::File(name),
        b't' => Medium::TempFile(name),
        b's' => Medium::SharedMemory(name),
        _ => {
            return Err(Malformed(
                "a request with an unknown medium or a name for inline bytes",
            ));
        }
    };
    let on = |bit: u8| flags & bit != 0;
    let pixels = description(packet, on(INFLATE))?;
    let request = Request {
        medium,
        offset: get64(packet, 24),
        max: get64(packet, 32),
        min: get64(packet, 40),
        inflate: on(INFLATE),
        png: on(PNG),
        exact: on(EXACT),
        discard: on(DISCARD),
        any_image: on(ANY_IMAGE),
        frames: frame_limits(packet, on(FRAMES))?,
        pixels,
        largest: (get32(packet, 56), get32(packet, 60)),
        largest_bytes: get64(packet, 48),
        fit: fitting(packet)?,
    };
    valid(&request)?;
    Ok((get64(packet, 16), request))
}

/// The login picture's fitting, at offsets 88 to 120.
fn put_fitting(out: &mut Vec<u8>, fit: Option<&Fitting>) {
    let Some(f) = fit else {
        out.extend_from_slice(&[0; 32]);
        return;
    };
    let kind = match f.form.fit {
        Fit::Cover => 1,
        Fit::Contain => 2,
        Fit::Stretch => 3,
        Fit::Natural => 4,
        Fit::Banner => 5,
    };
    // 0 for no anchor, then the nine anchors from 1 in the key's order
    let position = f.form.position.map_or(0, |p| {
        Position::ALL
            .iter()
            .position(|&q| q == p)
            .and_then(|i| u8::try_from(i).ok())
            .map_or(0, |i| i.saturating_add(1))
    });
    out.extend_from_slice(&[kind, position, f.form.scale, 0]);
    put32(out, f.form.rows.unwrap_or(0));
    let g = &f.geometry;
    for v in [
        g.display.0,
        g.display.1,
        g.cell.0,
        g.cell.1,
        g.grid.0,
        g.grid.1,
    ] {
        put32(out, v);
    }
}

/// The login picture's fitting at offsets 88 to 120: `None` when its kind
/// is 0 and every byte is 0, and refused with an unknown kind or position,
/// a scale outside 1 to 100, or a reserved byte that is not 0.
fn fitting(packet: &[u8]) -> Result<Option<Fitting>, Malformed> {
    let fit = match get8(packet, 88) {
        0 => {
            let rest = packet.get(89..REQUEST_FIXED).unwrap_or_default();
            return if rest.iter().all(|&b| b == 0) {
                Ok(None)
            } else {
                Err(Malformed("a request with no fit but a fitting"))
            };
        }
        1 => Fit::Cover,
        2 => Fit::Contain,
        3 => Fit::Stretch,
        4 => Fit::Natural,
        5 => Fit::Banner,
        _ => return Err(Malformed("a request with an unknown fit")),
    };
    let position = match get8(packet, 89) {
        0 => None,
        n => Some(
            *Position::ALL
                .get(usize::from(n).saturating_sub(1))
                .ok_or(Malformed("a request with an unknown position"))?,
        ),
    };
    let scale = get8(packet, 90);
    if !(1..=crate::login::SCALE_FULL).contains(&scale) {
        return Err(Malformed("a request with a scale outside 1 to 100"));
    }
    if get8(packet, 91) != 0 {
        return Err(Malformed("a request with a reserved byte set"));
    }
    let rows = get32(packet, 92);
    let at = |o: usize| get32(packet, o);
    Ok(Some(Fitting {
        form: Form {
            fit,
            rows: (rows > 0).then_some(rows),
            position,
            scale,
        },
        geometry: Geometry {
            display: (at(96), at(100)),
            cell: (at(104), at(108)),
            grid: (at(112), at(116)),
        },
    }))
}

/// The frame bounds at offsets 76 and 80: present only with [`FRAMES`],
/// with `max_frames` from 1 to [`REPLY_FRAMES_MAX`], and both 0 without it.
fn frame_limits(packet: &[u8], on: bool) -> Result<Option<FrameLimits>, Malformed> {
    let (max_frames, max_frame_bytes) = (get32(packet, 76), get64(packet, 80));
    if !on {
        return if max_frames == 0 && max_frame_bytes == 0 {
            Ok(None)
        } else {
            Err(Malformed("frame bounds without the frames flag"))
        };
    }
    let count = usize::try_from(max_frames).unwrap_or(usize::MAX);
    if count == 0 || count > REPLY_FRAMES_MAX {
        return Err(Malformed("a request whose max_frames is out of bounds"));
    }
    Ok(Some(FrameLimits {
        max_frames,
        max_frame_bytes,
    }))
}

/// The pixel description at offsets 14, 15 and 64, if any.
fn description(packet: &[u8], inflate: bool) -> Result<Option<Description>, Malformed> {
    let (layout, alpha) = (get8(packet, 14), get8(packet, 15));
    let (width, height) = (get32(packet, 64), get32(packet, 68));
    let layout = match layout {
        0 if alpha == 0 && width == 0 && height == 0 => return Ok(None),
        1 => Layout::Index,
        2 => Layout::IndexAlpha,
        3 => Layout::Rgb,
        4 => Layout::Rgba,
        _ => return Err(Malformed("a request with an unknown pixel layout")),
    };
    let alpha = match alpha {
        0 => Alpha::Straight,
        1 => Alpha::Premultiplied,
        _ => return Err(Malformed("a request with an unknown alpha")),
    };
    Ok(Some(Description {
        layout,
        alpha,
        width,
        height,
        compressed: inflate,
    }))
}

/// The combinations a request may not make, whoever built it.
fn valid(r: &Request) -> Result<(), Malformed> {
    let direct = matches!(r.medium, Medium::Direct(_));
    if r.fit.is_some() && !(direct && r.any_image && r.frames.is_none()) {
        return Err(Malformed(
            "a login picture's fit beside anything but one inline picture",
        ));
    }
    if r.discard {
        let removable = matches!(r.medium, Medium::TempFile(_) | Medium::SharedMemory(_));
        let alone = !(r.inflate
            || r.png
            || r.exact
            || r.any_image
            || r.frames.is_some()
            || r.pixels.is_some());
        if !removable || !alone || r.offset != 0 || r.max != 0 || r.min != 0 {
            return Err(Malformed("a discard with anything but its name"));
        }
    } else if r.png || r.any_image {
        if r.pixels.is_some() || (r.png && r.any_image) {
            return Err(Malformed("a decode with a pixel description"));
        }
        if r.any_image && (!direct || r.inflate) {
            return Err(Malformed("any image from anything but inline bytes"));
        }
    } else {
        let Some(d) = r.pixels else {
            return Err(Malformed("raw pixels without a description"));
        };
        if d.width == 0 || d.height == 0 || d.bytes() != Some(r.min) {
            return Err(Malformed(
                "a description whose bytes are not those the picture needs",
            ));
        }
        if direct && !r.inflate {
            return Err(Malformed("inline raw pixels without zlib"));
        }
    }
    if r.exact && !r.inflate {
        return Err(Malformed("exact without inflate"));
    }
    if r.frames.is_some() && !r.any_image {
        return Err(Malformed("the frames flag without the any-image flag"));
    }
    if r.min > r.largest_bytes {
        return Err(Malformed("a request that needs more bytes than its budget"));
    }
    Ok(())
}

/// The reply's bytes; the reason is cut at [`REASON_MAX`], and the delays
/// at [`REPLY_FRAMES_MAX`].
pub fn encode_reply(reply: &ReplyHeader) -> Vec<u8> {
    let reason = reply.reason.get(..REASON_MAX).unwrap_or(&reply.reason);
    let delays = reply
        .delays
        .get(..REPLY_FRAMES_MAX)
        .unwrap_or(&reply.delays);
    let total = REPLY_FIXED
        .saturating_add(delays.len().saturating_mul(2))
        .saturating_add(reason.len());
    let mut out = Vec::with_capacity(total);
    put32(&mut out, KIND_REPLY);
    put32(&mut out, len32(total));
    put32(&mut out, VERSION);
    put32(&mut out, reply.status as u32);
    put64(&mut out, reply.id);
    put32(&mut out, reply.width);
    put32(&mut out, reply.height);
    put32(&mut out, reply.ignored);
    out.extend_from_slice(&[u8::from(reply.pooled), 0, 0, 0]);
    put32(&mut out, len32(reason.len()));
    put32(&mut out, len32(delays.len()));
    out.extend_from_slice(&reply.loops.unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&[u8::from(reply.loops.is_some()), 0]);
    for d in delays {
        out.extend_from_slice(&d.to_le_bytes());
    }
    out.extend_from_slice(reason);
    out
}

/// The reply in `packet`, or why it is refused: a length that is not its
/// fields, its delays and its reason, a reason past [`REASON_MAX`], more
/// frames than [`REPLY_FRAMES_MAX`], another version, an unknown status, a
/// pooled or loop flag other than 0 or 1, a loop count without its flag,
/// or a reserved byte set.
pub fn decode_reply(packet: &[u8]) -> Result<ReplyHeader, Malformed> {
    header(packet, KIND_REPLY, REPLY_FIXED)?;
    let reason_len = usize::try_from(get32(packet, 40)).unwrap_or(usize::MAX);
    let frames = usize::try_from(get32(packet, 44)).unwrap_or(usize::MAX);
    if frames > REPLY_FRAMES_MAX {
        return Err(Malformed("a reply with more frames than it may count"));
    }
    let delays_end = REPLY_FIXED.saturating_add(frames.saturating_mul(2));
    if reason_len > REASON_MAX || delays_end.checked_add(reason_len) != Some(packet.len()) {
        return Err(Malformed(
            "a reply whose length is not its fields, its delays and its reason",
        ));
    }
    let loops = match (
        get8(packet, 50),
        u16::from_le_bytes([get8(packet, 48), get8(packet, 49)]),
    ) {
        (0, 0) => None,
        (1, n) => Some(n),
        _ => return Err(Malformed("a reply with an unknown loop flag")),
    };
    if get8(packet, 51) != 0 {
        return Err(Malformed("a reply with a reserved byte set"));
    }
    let delays = packet
        .get(REPLY_FIXED..delays_end)
        .unwrap_or_default()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| u16::from_le_bytes(b))
        .collect();
    let status =
        Status::from_wire(get32(packet, 12)).ok_or(Malformed("a reply with an unknown status"))?;
    let pooled = match get8(packet, 36) {
        0 => false,
        1 => true,
        _ => return Err(Malformed("a reply with an unknown pooled flag")),
    };
    if packet.get(37..40) != Some(&[0, 0, 0]) {
        return Err(Malformed("a reply with a reserved byte set"));
    }
    Ok(ReplyHeader {
        status,
        id: get64(packet, 16),
        width: get32(packet, 24),
        height: get32(packet, 28),
        ignored: get32(packet, 32),
        pooled,
        delays,
        loops,
        reason: packet.get(delays_end..).unwrap_or_default().to_vec(),
    })
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn file(name: &[u8]) -> Request {
        Request {
            medium: Medium::File(name.to_vec()),
            offset: 3,
            max: 9,
            min: 12,
            inflate: false,
            png: false,
            exact: false,
            discard: false,
            any_image: false,
            frames: None,
            pixels: Some(Description {
                layout: Layout::Rgba,
                alpha: Alpha::Straight,
                width: 3,
                height: 1,
                compressed: false,
            }),
            largest: (10, 20),
            largest_bytes: 4096,
            fit: None,
        }
    }

    #[test]
    fn a_request_comes_back_field_for_field() {
        let r = file(b"/tmp/a");
        let packet = encode_request(7, &r).unwrap();
        assert_eq!(packet.len(), REQUEST_FIXED + 6);
        assert_eq!(decode_request(&packet).unwrap(), (7, r));
        let png = Request {
            medium: Medium::Direct(Vec::new()),
            offset: 0,
            max: 0,
            min: 40,
            inflate: true,
            png: true,
            exact: true,
            pixels: None,
            ..file(b"")
        };
        let packet = encode_request(u64::MAX, &png).unwrap();
        assert_eq!(decode_request(&packet).unwrap(), (u64::MAX, png));
        // kitty's palette formats carry their own layouts, f=8 one byte a
        // pixel and f=16 two
        for (layout, min, byte) in [(Layout::Index, 3, 1), (Layout::IndexAlpha, 6, 2)] {
            let mut r = file(b"/tmp/a");
            r.min = min;
            if let Some(d) = r.pixels.as_mut() {
                d.layout = layout;
            }
            let packet = encode_request(3, &r).unwrap();
            assert_eq!(packet.get(14), Some(&byte));
            assert_eq!(decode_request(&packet).unwrap(), (3, r));
        }
    }

    #[test]
    fn each_refusal_of_the_request_and_of_the_pixel_description_holds() {
        let refuse = |r: Request| encode_request(1, &r).unwrap_err();
        let discard = Request {
            medium: Medium::TempFile(b"/tmp/x".to_vec()),
            offset: 0,
            max: 0,
            min: 0,
            discard: true,
            pixels: None,
            ..file(b"")
        };
        assert!(encode_request(1, &discard).is_ok());
        refuse(Request {
            png: true,
            ..discard.clone()
        });
        refuse(Request {
            medium: Medium::File(b"/x".to_vec()),
            ..discard.clone()
        });
        refuse(Request {
            offset: 1,
            ..discard
        });
        refuse(file(b""));
        refuse(file(&[b'a'; NAME_MAX + 1]));
        refuse(Request {
            exact: true,
            ..file(b"/x")
        });
        refuse(Request {
            min: 11,
            ..file(b"/x")
        });
        refuse(Request {
            pixels: None,
            ..file(b"/x")
        });
        refuse(Request {
            png: true,
            ..file(b"/x")
        });
        refuse(Request {
            largest_bytes: 11,
            ..file(b"/x")
        });
        // inline raw pixels go to the helper only with zlib
        refuse(Request {
            medium: Medium::Direct(Vec::new()),
            offset: 0,
            max: 0,
            ..file(b"")
        });
        // a description whose compression is not the request's
        refuse(Request {
            inflate: true,
            ..file(b"/x")
        });
        refuse(Request {
            any_image: true,
            pixels: None,
            ..file(b"/x")
        });
    }

    // covers: image.iterm2
    #[test]
    fn a_request_for_frames_comes_back_and_its_bounds_are_held() {
        let gif = Request {
            medium: Medium::Direct(Vec::new()),
            offset: 0,
            max: 0,
            min: 0,
            any_image: true,
            frames: Some(FrameLimits {
                max_frames: 1024,
                max_frame_bytes: 1 << 40,
            }),
            pixels: None,
            ..file(b"")
        };
        let packet = encode_request(3, &gif).unwrap();
        assert_eq!(packet[13] & FRAMES, FRAMES);
        assert_eq!(decode_request(&packet).unwrap(), (3, gif.clone()));
        let refuse = |r: Request| encode_request(1, &r).unwrap_err();
        refuse(Request {
            any_image: false,
            png: true,
            ..gif.clone()
        });
        for max_frames in [0, 1025] {
            refuse(Request {
                frames: Some(FrameLimits {
                    max_frames,
                    max_frame_bytes: 0,
                }),
                ..gif.clone()
            });
        }
        // bounds without the flag
        let mut bad = encode_request(
            1,
            &Request {
                frames: None,
                ..gif
            },
        )
        .unwrap();
        bad[80] = 1;
        assert!(decode_request(&bad).is_err());
    }

    #[test]
    fn a_reply_without_a_loop_count_or_frames_comes_back() {
        let reply = ReplyHeader {
            status: Status::Corrupt,
            id: 1,
            width: 0,
            height: 0,
            ignored: 0,
            pooled: false,
            delays: Vec::new(),
            loops: None,
            reason: Vec::new(),
        };
        let packet = encode_reply(&reply);
        assert_eq!(packet.len(), REPLY_FIXED);
        assert_eq!(decode_reply(&packet).unwrap(), reply);
        // a loop count without its flag
        let mut bad = packet.clone();
        bad[48] = 3;
        assert!(decode_reply(&bad).is_err());
        // the most frames a reply may count fit a packet
        let most = ReplyHeader {
            delays: vec![1; REPLY_FRAMES_MAX + 5],
            reason: vec![b'x'; REASON_MAX],
            ..reply
        };
        let packet = encode_reply(&most);
        assert!(packet.len() <= PACKET_MAX);
        assert_eq!(
            decode_reply(&packet).unwrap().delays.len(),
            REPLY_FRAMES_MAX
        );
    }

    #[test]
    fn a_packet_that_is_short_long_or_unknown_is_refused() {
        let packet = encode_request(1, &file(b"/a")).unwrap();
        for cut in 0..packet.len() {
            assert!(decode_request(&packet[..cut]).is_err(), "cut at {cut}");
        }
        let mut long = packet.clone();
        long.push(0);
        assert!(decode_request(&long).is_err());
        for (at, value) in [
            (0, 9),
            (8, 1),
            (12, b'x'),
            (13, 0x80),
            (14, 2),
            (15, 2),
            (76, 1),
            (80, 1),
        ] {
            let mut bad = packet.clone();
            bad[at] = value;
            assert!(decode_request(&bad).is_err(), "byte {at} set to {value}");
        }
    }

    #[test]
    fn a_reply_and_a_hello_come_back_and_bad_ones_are_refused() {
        let reply = ReplyHeader {
            status: Status::Ok,
            id: 9,
            width: 2,
            height: 3,
            ignored: 4,
            pooled: true,
            delays: vec![7, 0, 65535],
            loops: Some(0),
            reason: b"why".to_vec(),
        };
        let packet = encode_reply(&reply);
        assert_eq!(decode_reply(&packet).unwrap(), reply);
        let long = ReplyHeader {
            reason: vec![b'x'; 300],
            ..reply
        };
        assert_eq!(
            decode_reply(&encode_reply(&long)).unwrap().reason.len(),
            REASON_MAX
        );
        for (at, value) in [
            (12, 6),
            (36, 2),
            (37, 1),
            (40, 2),
            (8, 0),
            (44, 4),
            (50, 2),
            (51, 1),
        ] {
            let mut bad = packet.clone();
            bad[at] = value;
            assert!(decode_reply(&bad).is_err(), "byte {at} set to {value}");
        }
        let hello = Hello {
            version: VERSION,
            landlock: 6,
            seccomp: true,
            no_seccomp: None,
            no_landlock: None,
        };
        assert_eq!(decode_hello(&encode_hello(&hello)).unwrap(), hello);
        let old = Hello {
            version: 1,
            ..hello
        };
        assert_eq!(hello_version(&encode_hello(&old)), Some(1));
        assert!(decode_hello(&encode_hello(&old)).is_err());
        assert_eq!(hello_version(&packet), None, "a reply is no hello");
        let mut bad = encode_hello(&hello);
        bad[16] = 2;
        assert!(decode_hello(&bad).is_err());
        assert!(decode_hello(&packet).is_err());
    }

    // The hello carries why the helper's seccomp filter and its workers'
    // landlock domains are not in force, and a hello of the wire before
    // the reasons, version 4 and 20 bytes long, is refused whole, with
    // its version still read so vtium can say which version the helper
    // speaks. A reason the wire does not number, and a filter both in
    // force and missing, are refused too.
    //
    // References:
    // - ADR-1101 D7: the hello and its reasons.
    // - ADR-1200 D15, "Nothing on screen, and the log says why".
    #[test]
    fn a_hello_carries_its_reasons_and_one_of_the_older_wire_is_refused() {
        let off = Hello {
            version: VERSION,
            landlock: 6,
            seccomp: false,
            no_seccomp: Some(NoSeccomp::Off),
            no_landlock: Some(NoLandlock::Off),
        };
        let bytes = encode_hello(&off);
        assert_eq!(bytes.len(), 28);
        assert_eq!(&bytes[20..], &[1, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(decode_hello(&bytes).unwrap(), off);
        for (no_seccomp, at) in [
            (NoSeccomp::NoTable, 2),
            (NoSeccomp::NotBuilt, 3),
            (NoSeccomp::NoFilters, 4),
            (NoSeccomp::TooOld, 5),
        ] {
            let h = Hello {
                no_seccomp: Some(no_seccomp),
                ..off
            };
            assert_eq!(encode_hello(&h)[20], at);
            assert_eq!(decode_hello(&encode_hello(&h)).unwrap(), h);
        }
        for (no_landlock, at) in [
            (NoLandlock::TooOld, 2),
            (NoLandlock::NotBuilt, 3),
            (NoLandlock::NotEnabled, 4),
        ] {
            let h = Hello {
                no_landlock: Some(no_landlock),
                ..off
            };
            assert_eq!(encode_hello(&h)[24], at);
            assert_eq!(decode_hello(&encode_hello(&h)).unwrap(), h);
        }
        // the hello of version 4: kind, length 20, version, landlock, seccomp
        let mut four = Vec::new();
        for word in [KIND_HELLO, 20, 4, 6, 1] {
            four.extend(word.to_le_bytes());
        }
        assert_eq!(hello_version(&four), Some(4));
        assert!(decode_hello(&four).is_err());
        // reasons the wire does not number, and a filter in force with a reason
        let mut bad = bytes.clone();
        bad[20] = 6;
        assert!(decode_hello(&bad).is_err());
        let mut bad = bytes.clone();
        bad[24] = 5;
        assert!(decode_hello(&bad).is_err());
        let mut bad = bytes.clone();
        bad[16] = 1;
        assert!(decode_hello(&bad).is_err());
        let mut bad = bytes;
        bad[20] = 0;
        assert!(decode_hello(&bad).is_err());
    }

    // The login picture's fitting travels with its request, and a fit
    // beside anything but one inline picture is refused.
    //
    // References:
    // - ADR-1100 D17, "Who reads the file and who decodes it": the helper
    //   fits the picture it decodes.
    #[test]
    fn a_login_pictures_fitting_round_trips_and_an_unknown_fit_is_refused() {
        use crate::login::{Fit, Fitting, Form, Geometry, Position};
        let fit = Fitting {
            form: Form {
                fit: Fit::Banner,
                rows: Some(3),
                position: Some(Position::Center),
                scale: 100,
            },
            geometry: Geometry {
                display: (1920, 1080),
                cell: (8, 16),
                grid: (240, 67),
            },
        };
        let request = Request {
            medium: Medium::Direct(Vec::new()),
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
            largest: (8192, 8192),
            largest_bytes: 256 << 20,
            fit: Some(fit),
        };
        let packet = encode_request(7, &request).unwrap();
        assert_eq!(decode_request(&packet), Ok((7, request.clone())));
        let mut bad = packet.clone();
        if let Some(b) = bad.get_mut(88) {
            *b = 6;
        }
        assert!(decode_request(&bad).is_err());
        // a fit beside a named file is refused
        let file = Request {
            medium: Medium::File(b"/a".to_vec()),
            ..request.clone()
        };
        assert!(encode_request(8, &file).is_err());
        // so is a fit beside a pixel description, a PNG's flag, zlib, an
        // exact length or a discard: the helper fits only a decoded picture
        let described = Request {
            pixels: Some(Description {
                layout: Layout::Rgba,
                alpha: Alpha::Straight,
                width: 1,
                height: 1,
                compressed: false,
            }),
            min: 4,
            ..request.clone()
        };
        let png = Request {
            png: true,
            ..request.clone()
        };
        let inflated = Request {
            inflate: true,
            ..request.clone()
        };
        let exact = Request {
            inflate: true,
            exact: true,
            ..request.clone()
        };
        let discard = Request {
            discard: true,
            ..request
        };
        for (what, r) in [
            ("pixels", described),
            ("png", png),
            ("inflate", inflated),
            ("exact", exact),
            ("discard", discard),
        ] {
            assert!(encode_request(9, &r).is_err(), "{what}");
        }
    }

    // The position byte carries the nine anchors and unset, and a
    // reserved byte carries the scale, so the request keeps its length. A
    // scale of 0 or past 100, or a tenth anchor, is malformed.
    //
    // References:
    // - ADR-1100 D17, "The keys": the nine anchors and the scale from 1 to
    //   100.
    #[test]
    fn each_anchor_and_scale_crosses_the_wire_and_a_bad_scale_is_refused() {
        use crate::login::{Fit, Fitting, Form, Geometry, Position};
        let request = |position, scale| Request {
            medium: Medium::Direct(Vec::new()),
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
            largest: (8192, 8192),
            largest_bytes: 256 << 20,
            fit: Some(Fitting {
                form: Form {
                    fit: Fit::Contain,
                    rows: None,
                    position,
                    scale,
                },
                geometry: Geometry {
                    display: (1920, 1080),
                    cell: (8, 16),
                    grid: (240, 67),
                },
            }),
        };
        let unset = encode_request(1, &request(None, 100)).unwrap();
        assert_eq!(unset.len(), REQUEST_FIXED);
        let anchors = Position::ALL.into_iter().map(Some);
        for position in std::iter::once(None).chain(anchors) {
            for scale in [1, 30, 99, 100] {
                let sent = request(position, scale);
                let packet = encode_request(2, &sent).unwrap();
                assert_eq!(packet.len(), REQUEST_FIXED);
                assert_eq!(
                    decode_request(&packet),
                    Ok((2, sent)),
                    "{position:?} {scale}"
                );
            }
        }
        for (at, byte) in [(90, 0), (90, 101), (90, 255), (89, 10), (91, 1)] {
            let mut bad = unset.clone();
            if let Some(b) = bad.get_mut(at) {
                *b = byte;
            }
            assert!(decode_request(&bad).is_err(), "byte {at} at {byte}");
        }
    }
}
