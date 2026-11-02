//! The decode boundary: what a handler asks the image helper for, and
//! what comes back (ADR-1100 D7, ADR-1101 D7).
//!
//! The request is in mechanical terms, so the helper sees no protocol's
//! keys or strings: what to open, where to start, the fewest bytes the
//! picture needs and the most that may be read, whether to inflate or
//! decode, a description of raw pixels the helper converts, or a discard
//! that removes a refused temporary file or shared-memory object and reads
//! nothing. Every picture comes back in one format, premultiplied R, G, B,
//! A bytes, whether the helper decoded it or converted described pixels,
//! so the console has one path for every reply. The handler turns the
//! outcome into its protocol's reply. The console checks a reply's sizes
//! against its request, [`Reply::checked`], before it uses anything, and
//! reads its pixels with [`convert::from_reply`], which clamps them.
//!
//! [`wire`] encodes both packets for the socket, and both ends link it, so
//! they cannot read a field differently. [`convert`] is the one conversion
//! into the store's format.
//!
//! The submodules also hold the decoders that run in the console, which
//! take whatever a program writes: sixel, base64, and raw RGB and RGBA.
//! The protocol handlers reach them inside the crate.

pub(crate) mod base64;
pub mod convert;
pub(crate) mod raw;
pub(crate) mod sixel;
pub mod wire;

pub use convert::{Alpha, Layout};

/// What a request reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Medium {
    /// Bytes the program sent in the string, after base64.
    Direct(Vec<u8>),
    /// A file the program named, by its path as received.
    File(Vec<u8>),
    /// A temporary file the program named, which the helper removes.
    TempFile(Vec<u8>),
    /// A shared-memory object the program named, which the helper removes.
    SharedMemory(Vec<u8>),
}

impl Medium {
    /// Kitty's medium letter: `d`, `f`, `t` or `s`.
    pub fn letter(&self) -> char {
        match self {
            Self::Direct(_) => 'd',
            Self::File(_) => 'f',
            Self::TempFile(_) => 't',
            Self::SharedMemory(_) => 's',
        }
    }

    /// The path or name a named medium gives, as received; empty for
    /// direct bytes.
    pub fn name(&self) -> &[u8] {
        match self {
            Self::Direct(_) => &[],
            Self::File(n) | Self::TempFile(n) | Self::SharedMemory(n) => n,
        }
    }
}

/// A request to the image helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub medium: Medium,
    /// Where a read starts, for a named medium.
    pub offset: u64,
    /// The most bytes to read, 0 meaning to the end.
    pub max: u64,
    /// The fewest bytes the image needs after inflating; 0 for a discard.
    pub min: u64,
    /// Inflate zlib (kitty's `o=z`).
    pub inflate: bool,
    /// Decode PNG (kitty's `f=100`).
    pub png: bool,
    /// The inflated bytes must number exactly `min`.
    pub exact: bool,
    /// Remove the temporary file or shared-memory object and read nothing.
    pub discard: bool,
    /// Decode a PNG, JPEG or GIF file, recognized by its first bytes, as
    /// iTerm2's pictures ask (ADR-1103).
    pub any_image: bool,
    /// The raw pixels the helper reads or inflates, which it converts into
    /// the store's format; `None` for PNG, any image or a discard.
    pub pixels: Option<Description>,
    /// The largest picture the reply may hold, each side, from
    /// `max-graphic-size`, and its most bytes, from the budget.
    pub largest: (u32, u32),
    pub largest_bytes: u64,
}

/// Raw pixels as a request describes them, built by the handler from its
/// protocol's keys (ADR-1100 D7): kitty's `f=24` and `f=32` read from a
/// file or shared memory, or sent inline with zlib.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Description {
    pub layout: Layout,
    pub alpha: Alpha,
    pub width: u32,
    pub height: u32,
    /// The bytes are zlib-compressed; the request's `inflate` says the
    /// same.
    pub compressed: bool,
}

impl Description {
    /// The bytes the pixels take: width × height × the layout's bytes, or
    /// `None` past `u64`.
    pub fn bytes(&self) -> Option<u64> {
        u64::from(self.width)
            .checked_mul(u64::from(self.height))?
            .checked_mul(u64::try_from(self.layout.bytes()).ok()?)
    }
}

/// What the helper sends back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    /// A picture in the store's format: premultiplied R, G, B, A bytes,
    /// rows top to bottom, exactly width × height × 4 of them. The helper
    /// decoded it or converted described pixels into it; the console
    /// still clamps each color to its alpha as it reads it
    /// ([`convert::from_reply`]).
    Rgba {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
    /// A discard's answer, which carries nothing.
    Discarded,
}

/// One outcome of a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Ok(Payload),
    Unreadable,
    TooShort,
    Corrupt,
    TooLarge,
    OverBudget,
    /// The helper did not answer in time, or failed.
    Failed,
}

impl Reply {
    /// The reply, or [`Reply::Failed`] when its sizes do not fit its
    /// request: a picture past the largest the request allows, of another
    /// size than the pixels the request described, or whose data is not
    /// its width times its height times 4 bytes; a picture a discard did
    /// not ask for; or a discard's answer to a request that was no
    /// discard. A discard is answered [`Payload::Discarded`].
    pub fn checked(self, request: &Request) -> Self {
        let fits = match &self {
            Self::Ok(payload) if request.discard => *payload == Payload::Discarded,
            Self::Ok(Payload::Rgba {
                width,
                height,
                data,
            }) => {
                let len = u64::from(*width)
                    .checked_mul(u64::from(*height))
                    .and_then(|n| n.checked_mul(4));
                let size = match request.pixels {
                    Some(d) => (d.width, d.height) == (*width, *height),
                    None => request.png || request.any_image,
                };
                size && *width > 0
                    && *height > 0
                    && *width <= request.largest.0
                    && *height <= request.largest.1
                    && len.is_some_and(|l| l <= request.largest_bytes)
                    && u64::try_from(data.len()).ok() == len
            }
            Self::Ok(Payload::Discarded) => false,
            _ => true,
        };
        if fits { self } else { Self::Failed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(png: bool) -> Request {
        Request {
            medium: Medium::File(b"/x".to_vec()),
            offset: 0,
            max: 8,
            min: 4,
            inflate: false,
            png,
            exact: false,
            discard: false,
            any_image: false,
            pixels: None,
            largest: (2, 2),
            largest_bytes: 16,
        }
    }

    fn rgba(width: u32, height: u32, n: usize) -> Reply {
        Reply::Ok(Payload::Rgba {
            width,
            height,
            data: vec![0; n],
        })
    }

    // covers: image.pipeline
    #[test]
    fn a_reply_whose_sizes_do_not_fit_its_request_fails() {
        assert_eq!(rgba(2, 2, 16).checked(&request(true)), rgba(2, 2, 16));
        assert_eq!(rgba(2, 2, 15).checked(&request(true)), Reply::Failed);
        assert_eq!(rgba(3, 1, 12).checked(&request(true)), Reply::Failed);
        assert_eq!(rgba(0, 1, 0).checked(&request(true)), Reply::Failed);
        // a picture for a request that asked for no picture
        assert_eq!(rgba(2, 2, 16).checked(&request(false)), Reply::Failed);
        let any = Request {
            any_image: true,
            ..request(false)
        };
        assert_eq!(rgba(1, 2, 8).checked(&any), rgba(1, 2, 8));
        assert_eq!(Reply::Corrupt.checked(&request(false)), Reply::Corrupt);
        // a discard's answer to a request that was no discard
        let discarded = Reply::Ok(Payload::Discarded);
        assert_eq!(discarded.clone().checked(&request(true)), Reply::Failed);
        let discard = Request {
            discard: true,
            min: 0,
            ..request(false)
        };
        assert_eq!(discarded.clone().checked(&discard), discarded);
        assert_eq!(rgba(1, 1, 4).checked(&discard), Reply::Failed);
        assert_eq!(Reply::Unreadable.checked(&discard), Reply::Unreadable);
    }

    #[test]
    fn a_reply_to_described_pixels_has_the_size_they_describe() {
        let described = Request {
            pixels: Some(Description {
                layout: Layout::Rgb,
                alpha: Alpha::Straight,
                width: 2,
                height: 1,
                compressed: false,
            }),
            min: 6,
            ..request(false)
        };
        assert_eq!(rgba(2, 1, 8).checked(&described), rgba(2, 1, 8));
        assert_eq!(rgba(1, 2, 8).checked(&described), Reply::Failed);
        assert_eq!(rgba(1, 1, 4).checked(&described), Reply::Failed);
    }
}
