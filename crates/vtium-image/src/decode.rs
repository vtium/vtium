//! The decode boundary: what a handler asks the image helper for, and
//! what comes back (ADR-1100, D7).
//!
//! The request is in mechanical terms, so the helper reads no part of any
//! protocol's specification: what to open, where to start, the fewest
//! bytes the image needs and the most that may be read, whether to
//! inflate or decode, or a discard that removes a refused temporary file
//! or shared-memory object and reads nothing. The handler turns the
//! outcome into its protocol's reply. The console checks a reply's sizes
//! against its request, [`Reply::checked`], before it uses anything.

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
    /// The largest picture the reply may hold, each side, from
    /// `max-graphic-size`, and its most bytes, from the budget.
    pub largest: (u32, u32),
    pub largest_bytes: u64,
}

/// What the helper sends back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    /// A decoded picture: R, G, B, A bytes, rows top to bottom.
    Rgba {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
    /// The bytes read or inflated.
    Bytes(Vec<u8>),
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
    /// request: a picture past the largest the request allows or whose
    /// data is not its width times its height times 4 bytes, bytes past
    /// the most the request reads or fewer than it needs, or a payload a
    /// discard did not ask for. A discard is answered OK with no bytes.
    pub fn checked(self, request: &Request) -> Self {
        let fits = match &self {
            Self::Ok(payload) if request.discard => *payload == Payload::Bytes(Vec::new()),
            Self::Ok(Payload::Rgba {
                width,
                height,
                data,
            }) => {
                let len = u64::from(*width)
                    .checked_mul(u64::from(*height))
                    .and_then(|n| n.checked_mul(4));
                request.png
                    && *width > 0
                    && *height > 0
                    && *width <= request.largest.0
                    && *height <= request.largest.1
                    && len.is_some_and(|l| l <= request.largest_bytes)
                    && u64::try_from(data.len()).ok() == len
            }
            Self::Ok(Payload::Bytes(bytes)) => {
                let n = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                !request.png
                    && n >= request.min
                    && (request.max == 0 || request.inflate || n <= request.max)
                    && (!request.exact || n == request.min)
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
            largest: (2, 2),
            largest_bytes: 16,
        }
    }

    // covers: image.pipeline
    #[test]
    fn a_reply_whose_sizes_do_not_fit_its_request_fails() {
        let rgba = |w, h, n| {
            Reply::Ok(Payload::Rgba {
                width: w,
                height: h,
                data: vec![0; n],
            })
        };
        assert_eq!(rgba(2, 2, 16).checked(&request(true)), rgba(2, 2, 16));
        assert_eq!(rgba(2, 2, 15).checked(&request(true)), Reply::Failed);
        assert_eq!(rgba(3, 1, 12).checked(&request(true)), Reply::Failed);
        assert_eq!(rgba(2, 2, 16).checked(&request(false)), Reply::Failed);
        let bytes = |n| Reply::Ok(Payload::Bytes(vec![1; n]));
        assert_eq!(bytes(4).checked(&request(false)), bytes(4));
        assert_eq!(bytes(3).checked(&request(false)), Reply::Failed);
        assert_eq!(bytes(9).checked(&request(false)), Reply::Failed);
        assert_eq!(Reply::Corrupt.checked(&request(false)), Reply::Corrupt);
        let discard = Request {
            discard: true,
            min: 0,
            ..request(false)
        };
        assert_eq!(bytes(0).checked(&discard), bytes(0));
        assert_eq!(bytes(1).checked(&discard), Reply::Failed);
        assert_eq!(Reply::Unreadable.checked(&discard), Reply::Unreadable);
    }
}
