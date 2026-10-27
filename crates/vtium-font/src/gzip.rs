//! gzip (RFC 1952) decompression for PSF fonts, as kbd ships them: each
//! member's header, its deflate data through `miniz_oxide`, and its trailer's
//! CRC-32 and length checked.

use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZFlush, MZStatus};

/// Why a gzip file was refused.
pub(crate) enum GzipError {
    /// It decompresses past the limit.
    TooLarge,
    /// It is not a well-formed gzip file.
    Corrupt(&'static str),
}

const FHCRC: u8 = 1 << 1;
const FEXTRA: u8 = 1 << 2;
const FNAME: u8 = 1 << 3;
const FCOMMENT: u8 = 1 << 4;
const RESERVED: u8 = 0xe0;

/// `input`'s members decompressed and joined, refused past `limit` bytes.
/// Anything after the last member that is not a member is refused too.
pub(crate) fn gunzip(mut input: &[u8], limit: usize) -> Result<Vec<u8>, GzipError> {
    let mut out = Vec::new();
    loop {
        input = skip_header(input)?;
        let start = out.len();
        input = inflate_member(input, &mut out, limit)?;
        let [c0, c1, c2, c3, l0, l1, l2, l3, rest @ ..] = input else {
            return Err(GzipError::Corrupt("gzip trailer cut short"));
        };
        let member = out.get(start..).unwrap_or_default();
        if crc32(member) != u32::from_le_bytes([*c0, *c1, *c2, *c3]) {
            return Err(GzipError::Corrupt("gzip CRC-32 mismatch"));
        }
        // ISIZE is the length modulo 2^32
        #[expect(
            clippy::cast_possible_truncation,
            reason = "ISIZE is the member's length modulo 2^32"
        )]
        let len = member.len() as u32;
        if len != u32::from_le_bytes([*l0, *l1, *l2, *l3]) {
            return Err(GzipError::Corrupt("gzip length mismatch"));
        }
        input = rest;
        if input.is_empty() {
            return Ok(out);
        }
    }
}

/// `input` past a member's header (RFC 1952 2.3).
fn skip_header(input: &[u8]) -> Result<&[u8], GzipError> {
    let [0x1f, 0x8b, cm, flg, _, _, _, _, _, _, rest @ ..] = input else {
        return Err(GzipError::Corrupt("not a gzip member"));
    };
    if *cm != 8 || flg & RESERVED != 0 {
        return Err(GzipError::Corrupt("gzip method or flags unknown"));
    }
    let mut rest = rest;
    if flg & FEXTRA != 0 {
        let [x0, x1, more @ ..] = rest else {
            return Err(GzipError::Corrupt("gzip extra field cut short"));
        };
        let xlen = usize::from(u16::from_le_bytes([*x0, *x1]));
        rest = more
            .get(xlen..)
            .ok_or(GzipError::Corrupt("gzip extra field cut short"))?;
    }
    for flag in [FNAME, FCOMMENT] {
        if flg & flag != 0 {
            let nul = rest
                .iter()
                .position(|&b| b == 0)
                .ok_or(GzipError::Corrupt("gzip name or comment unterminated"))?;
            rest = rest.get(nul.saturating_add(1)..).unwrap_or_default();
        }
    }
    if flg & FHCRC != 0 {
        rest = rest
            .get(2..)
            .ok_or(GzipError::Corrupt("gzip header CRC cut short"))?;
    }
    Ok(rest)
}

/// Inflates the deflate stream at the start of `input` onto `out`, and
/// returns what follows it.
fn inflate_member<'a>(
    mut input: &'a [u8],
    out: &mut Vec<u8>,
    limit: usize,
) -> Result<&'a [u8], GzipError> {
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        let r = inflate(&mut state, input, &mut buf, MZFlush::None);
        input = input.get(r.bytes_consumed..).unwrap_or_default();
        let written = buf.get(..r.bytes_written).unwrap_or_default();
        if out.len().saturating_add(written.len()) > limit {
            return Err(GzipError::TooLarge);
        }
        out.extend_from_slice(written);
        match r.status {
            Ok(MZStatus::StreamEnd) => return Ok(input),
            Ok(_) if r.bytes_consumed > 0 || r.bytes_written > 0 => {}
            _ => return Err(GzipError::Corrupt("gzip deflate data corrupt or cut short")),
        }
    }
}

/// CRC-32 (ISO 3309, the polynomial 0xEDB88320 reflected), as RFC 1952 8
/// computes it, a bit at a time.
fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, &b| {
        (0..8).fold(crc ^ u32::from(b), |c, _| {
            if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 1952's check value for "123456789", as zlib's crc32 gives it
    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}
