//! zlib (kitty's `o=z`), inflated as a stream that stops at its bound
//! (ADR-1101 D6), through `miniz_oxide`.
//!
//! How much comes out follows the C reference's rules: for raw pixels,
//! once the bytes the picture needs are out the rest is ignored; where the
//! request asks for exactly that many bytes, one byte more is `CORRUPT`;
//! otherwise, for a PNG inside zlib, a byte past `max_bytes` is
//! `OVER_BUDGET`. A stream that does not inflate or ends early is
//! `CORRUPT`, and fewer bytes than the picture needs is `TOO_SHORT`.

use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};
use vtium_image::decode::wire::Status;

use crate::job::Failure;

/// Where inflating stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Raw pixels: stop once this many bytes are out and ignore the rest.
    Stop(u64),
    /// Exactly this many bytes; one more is `CORRUPT`.
    Exact(u64),
    /// At least this many, at most the budget; one past it is
    /// `OVER_BUDGET`.
    Budget { min: u64, max: u64 },
}

/// What came out, and how many bytes of the input went unused.
#[derive(Debug, PartialEq, Eq)]
pub struct Inflated {
    pub bytes: Vec<u8>,
    pub rest: u64,
}

fn corrupt(why: &str) -> Failure {
    Failure::new(Status::Corrupt, why)
}

fn over(why: String) -> Failure {
    Failure::new(Status::OverBudget, why)
}

/// `buf` grown to `size` bytes, or `OVER_BUDGET` when memory refuses.
fn grow(buf: &mut Vec<u8>, size: usize) -> Result<(), Failure> {
    buf.try_reserve_exact(size.saturating_sub(buf.len()))
        .map_err(|_| over(format!("cannot allocate {size} bytes to inflate into")))?;
    buf.resize(size, 0);
    Ok(())
}

/// `input` inflated toward `goal`.
pub fn inflate_bounded(input: &[u8], goal: Goal) -> Result<Inflated, Failure> {
    let (min, limit) = match goal {
        Goal::Stop(n) | Goal::Exact(n) => (n, n),
        Goal::Budget { min, max } => (min, max),
    };
    let limit = usize::try_from(limit).map_err(|_| over(format!("{limit} bytes to inflate")))?;
    // raw pixels need exactly their bytes, which are allocated at once;
    // a PNG takes the C reference's first guess, four times the input and
    // at least 4 KiB, and grows toward the budget
    let first = match goal {
        Goal::Stop(_) | Goal::Exact(_) => limit,
        Goal::Budget { .. } => input.len().saturating_mul(4).max(4096).min(limit),
    };
    let mut out = Vec::new();
    grow(&mut out, first)?;
    let mut state = InflateState::new_boxed(DataFormat::Zlib);
    let (mut used, mut total) = (0usize, 0usize);
    let ended = loop {
        if total == out.len() {
            if total == limit {
                break false;
            }
            let next = if out.len() > limit / 2 {
                limit
            } else {
                out.len().saturating_mul(2)
            };
            grow(&mut out, next)?;
        }
        let r = inflate(
            &mut state,
            input.get(used..).unwrap_or_default(),
            out.get_mut(total..).unwrap_or_default(),
            MZFlush::None,
        );
        used = used.saturating_add(r.bytes_consumed);
        total = total.saturating_add(r.bytes_written);
        match r.status {
            Ok(MZStatus::StreamEnd) => break true,
            Ok(MZStatus::Ok) if r.bytes_consumed > 0 || r.bytes_written > 0 => {}
            Err(MZError::Buf) if total == out.len() => {}
            Ok(MZStatus::Ok) | Err(MZError::Buf) => return Err(corrupt("truncated zlib data")),
            _ => return Err(corrupt("corrupt zlib data")),
        }
    };
    if !ended {
        used = past_limit(&mut state, input, used, goal)?;
    }
    out.truncate(total);
    if u64::try_from(total).unwrap_or(u64::MAX) < min {
        return Err(Failure::new(
            Status::TooShort,
            format!("zlib data inflates to {total} bytes, {min} needed"),
        ));
    }
    Ok(Inflated {
        bytes: out,
        rest: u64::try_from(input.len().saturating_sub(used)).unwrap_or(u64::MAX),
    })
}

/// The bytes of `input` used once the limit is out and the stream has not
/// ended: for [`Goal::Stop`], the stream's end and trailer if they follow
/// with no more output, and the rest is ignored; otherwise one byte more
/// fails, and the stream must end.
fn past_limit(
    state: &mut InflateState,
    input: &[u8],
    mut used: usize,
    goal: Goal,
) -> Result<usize, Failure> {
    let mut probe = [0u8; 1];
    loop {
        let r = inflate(
            state,
            input.get(used..).unwrap_or_default(),
            &mut probe,
            MZFlush::None,
        );
        if r.bytes_written > 0 {
            return match goal {
                Goal::Stop(_) => Ok(used),
                Goal::Exact(n) => Err(corrupt(&format!("zlib data inflates past {n} bytes"))),
                Goal::Budget { .. } => Err(over("zlib data inflates past the budget".to_owned())),
            };
        }
        used = used.saturating_add(r.bytes_consumed);
        match r.status {
            Ok(MZStatus::StreamEnd) => return Ok(used),
            Ok(MZStatus::Ok) if r.bytes_consumed > 0 => {}
            _ if matches!(goal, Goal::Stop(_)) => return Ok(used),
            Ok(MZStatus::Ok) | Err(MZError::Buf) => return Err(corrupt("truncated zlib data")),
            _ => return Err(corrupt("corrupt zlib data")),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn zlib(data: &[u8]) -> Vec<u8> {
        miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
    }

    #[test]
    fn raw_pixels_stop_at_what_they_need_and_ignore_the_rest() {
        let data: Vec<u8> = (0..100u8).collect();
        let z = zlib(&data);
        let got = inflate_bounded(&z, Goal::Stop(40)).unwrap();
        assert_eq!(got.bytes, &data[..40]);
        let got = inflate_bounded(&z, Goal::Stop(100)).unwrap();
        assert_eq!((got.bytes.as_slice(), got.rest), (&data[..], 0));
        let mut junk = z.clone();
        junk.extend_from_slice(b"junk");
        assert_eq!(inflate_bounded(&junk, Goal::Stop(100)).unwrap().rest, 4);
        let short = inflate_bounded(&z, Goal::Stop(101)).unwrap_err();
        assert_eq!(short.status, Status::TooShort);
    }

    #[test]
    fn exact_and_budget_fail_one_byte_past_and_a_broken_stream_is_corrupt() {
        let data = vec![7u8; 5000];
        let z = zlib(&data);
        assert_eq!(inflate_bounded(&z, Goal::Exact(5000)).unwrap().bytes, data);
        assert_eq!(
            inflate_bounded(&z, Goal::Exact(4999)).unwrap_err().status,
            Status::Corrupt
        );
        assert_eq!(
            inflate_bounded(&z, Goal::Exact(5001)).unwrap_err().status,
            Status::TooShort
        );
        let budget = |max| Goal::Budget { min: 0, max };
        assert_eq!(inflate_bounded(&z, budget(5000)).unwrap().bytes, data);
        assert_eq!(
            inflate_bounded(&z, budget(4999)).unwrap_err().status,
            Status::OverBudget
        );
        let cut = &z[..z.len() / 2];
        assert_eq!(
            inflate_bounded(cut, budget(1 << 20)).unwrap_err().status,
            Status::Corrupt
        );
        assert_eq!(
            inflate_bounded(b"not zlib at all", budget(1 << 20))
                .unwrap_err()
                .status,
            Status::Corrupt
        );
    }
}
