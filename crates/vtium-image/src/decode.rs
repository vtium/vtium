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
//! against its request, [`Reply::checked`], before it uses anything. The
//! helper's pixels stay in its reply file, a [`ReplyFile`], until the
//! handler has reserved their bytes under the budget; then
//! [`Payload::copy_into`] reads them once into that buffer, clamping each
//! color to its alpha as it goes ([`convert::from_reply`]), so a picture
//! is copied into memory only the console holds exactly once (ADR-1101
//! D7).
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

use core::fmt;
use std::sync::Arc;

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

/// The bytes of a picture still in the image helper's reply file, which
/// vtium-platform implements over the memory file once it has checked its
/// seals and size (ADR-1101 D7).
pub trait ReplySource: fmt::Debug + Send + Sync {
    /// The bytes it holds.
    fn bytes(&self) -> u64;
    /// Hands each of its bytes to `sink` once, in order, in runs each a
    /// whole number of 4-byte pixels, allocating nothing the size of the
    /// picture; false when it cannot be read whole.
    fn read(&self, sink: &mut dyn FnMut(&[u8])) -> bool;
}

/// A reply's picture left in the helper's file, shared so a reply can be
/// cloned and compared: two are equal when they are the same file.
#[derive(Clone)]
pub struct ReplyFile(Arc<dyn ReplySource>);

impl ReplyFile {
    pub fn new(source: impl ReplySource + 'static) -> Self {
        Self(Arc::new(source))
    }

    /// The bytes the file holds.
    pub fn bytes(&self) -> u64 {
        self.0.bytes()
    }

    /// Hands the file's bytes to `sink` once, as [`ReplySource::read`]
    /// says; false when they cannot be read whole.
    pub fn read(&self, sink: &mut dyn FnMut(&[u8])) -> bool {
        self.0.read(sink)
    }
}

impl fmt::Debug for ReplyFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ReplyFile {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ReplyFile {}

/// What the helper sends back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    /// A picture in the store's format: premultiplied R, G, B, A bytes,
    /// rows top to bottom, exactly width × height × 4 of them, already in
    /// memory, as the replay interface's fixture host and the tests' stand-in
    /// helpers give it.
    Rgba {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
    /// The same picture left in the image helper's reply file, as the real
    /// helper's host gives it: [`copy_into`](Self::copy_into) reads it once.
    File {
        width: u32,
        height: u32,
        file: ReplyFile,
    },
    /// A discard's answer, which carries nothing.
    Discarded,
}

impl Payload {
    /// A picture's width and height; `None` for a discard's answer.
    pub fn size(&self) -> Option<(u32, u32)> {
        match self {
            Self::Rgba { width, height, .. } | Self::File { width, height, .. } => {
                Some((*width, *height))
            }
            Self::Discarded => None,
        }
    }

    /// The bytes a picture holds.
    fn bytes(&self) -> Option<u64> {
        match self {
            Self::Rgba { data, .. } => u64::try_from(data.len()).ok(),
            Self::File { file, .. } => Some(file.bytes()),
            Self::Discarded => None,
        }
    }

    /// The one copy of a picture into memory only the console holds: its
    /// pixels in the store's format, each color clamped to its alpha, in
    /// `out`, emptied first, which the caller reserved under the budget, and
    /// whether every pixel is opaque ([`convert::from_reply`]). `out` grows
    /// at most once, to the picture's size, and not at all when it already
    /// has room. `None` for a discard's answer, and for a file that cannot
    /// be read whole or hands over more or fewer bytes than its size.
    pub fn copy_into(&self, mut out: Vec<u32>) -> Option<(Vec<u32>, bool)> {
        let (width, height) = self.size()?;
        let pixels = usize::try_from(u64::from(width).checked_mul(u64::from(height))?).ok()?;
        match self {
            Self::Rgba { data, .. } => {
                let (out, opaque) = convert::from_reply(data, out);
                (out.len() == pixels).then_some((out, opaque))
            }
            Self::File { file, .. } => {
                out.clear();
                out.reserve_exact(pixels);
                let mut alphas = 0xffu8;
                let mut over = false;
                let whole = file.read(&mut |run| {
                    let room = pixels.saturating_sub(out.len()).saturating_mul(4);
                    let take = run.get(..room).unwrap_or(run);
                    over |= take.len() < run.len();
                    convert::extend_from_reply(take, &mut out, &mut alphas);
                });
                (whole && !over && out.len() == pixels).then_some((out, alphas == 0xff))
            }
            Self::Discarded => None,
        }
    }
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
            Self::Ok(Payload::Discarded) => false,
            Self::Ok(picture) => {
                let (width, height) = picture.size().unwrap_or_default();
                let len = u64::from(width)
                    .checked_mul(u64::from(height))
                    .and_then(|n| n.checked_mul(4));
                let size = match request.pixels {
                    Some(d) => (d.width, d.height) == (width, height),
                    None => request.png || request.any_image,
                };
                size && width > 0
                    && height > 0
                    && width <= request.largest.0
                    && height <= request.largest.1
                    && len.is_some_and(|l| l <= request.largest_bytes)
                    && picture.bytes() == len
            }
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

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod file_tests {
    use super::*;

    /// A reply file over `data`, handed over in runs of `run` bytes, that
    /// says it holds `bytes`.
    #[derive(Debug)]
    struct Fake {
        data: Vec<u8>,
        run: usize,
        bytes: u64,
    }

    impl ReplySource for Fake {
        fn bytes(&self) -> u64 {
            self.bytes
        }

        fn read(&self, sink: &mut dyn FnMut(&[u8])) -> bool {
            self.data.chunks(self.run).for_each(&mut *sink);
            true
        }
    }

    fn file(data: Vec<u8>, run: usize) -> Payload {
        let bytes = u64::try_from(data.len()).unwrap();
        Payload::File {
            width: 2,
            height: 1,
            file: ReplyFile::new(Fake { data, run, bytes }),
        }
    }

    #[test]
    fn a_file_is_copied_once_into_the_buffer_given_and_clamped() {
        let pixels = [0x40, 0x80, 0x10, 0x40, 1, 2, 3, 0xff];
        let buffer = Vec::with_capacity(2);
        let at = buffer.as_ptr();
        let (out, opaque) = file(pixels.to_vec(), 4).copy_into(buffer).unwrap();
        assert_eq!(out, [0x4040_4010, 0xff01_0203]);
        assert!(!opaque);
        assert_eq!(out.as_ptr(), at, "a buffer with room is filled where it is");
        // the same bytes in memory give the same pixels
        let rgba = Payload::Rgba {
            width: 2,
            height: 1,
            data: pixels.to_vec(),
        };
        assert_eq!(rgba.copy_into(Vec::new()).unwrap(), (out, false));
    }

    #[test]
    fn a_file_that_hands_over_more_or_fewer_bytes_than_its_picture_fails() {
        assert_eq!(file(vec![0xff; 4], 4).copy_into(Vec::new()), None);
        assert_eq!(file(vec![0xff; 12], 4).copy_into(Vec::new()), None);
        assert!(file(vec![0xff; 8], 8).copy_into(Vec::new()).unwrap().1);
        assert_eq!(Payload::Discarded.copy_into(Vec::new()), None);
    }
}
