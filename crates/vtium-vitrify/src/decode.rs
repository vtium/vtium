//! The decoders the helper runs (ADR-1200 D6, D14): a compression found by
//! its magic, as kbd finds it, the PSF font inside or the font a CPI file's
//! `cp-size=` takes, and the Unicode map a font without its own table
//! takes. Everything here reads bytes someone else supplied, and runs only
//! in the helper or the parser's child process.
//!
//! gzip is decoded through `miniz_oxide`'s inflate, bzip2 through `bzip2`
//! on its Rust port of libbzip2, xz through `lzma-rust2` and zstd through
//! `ruzstd`, each checking its stream's own check value.

use std::io::Read;

use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZFlush, MZStatus};
use vtium_font::{GZIP_BYTES_MAX, PSF_BYTES_MAX, Psf, Style};

use crate::glyphs::Glyphs;

/// How a file is compressed, from its first two bytes, as kbd's
/// `kbdfile.c` tells them apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Compression {
    Plain,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

impl Compression {
    pub fn of(bytes: &[u8]) -> Compression {
        match bytes {
            [0x1f, 0x8b, ..] => Compression::Gzip,
            [0x42, 0x5a, ..] => Compression::Bzip2,
            [0xfd, 0x37, ..] => Compression::Xz,
            [0x28, 0xb5, ..] => Compression::Zstd,
            _ => Compression::Plain,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Compression::Plain => "plain",
            Compression::Gzip => "gzip",
            Compression::Bzip2 => "bzip2",
            Compression::Xz => "xz",
            Compression::Zstd => "zstd",
        }
    }
}

/// The most bytes a compressed file holds before it is decompressed, in
/// every compression: vtium-font's bound for gzip, which a crafted file of
/// tiny deflate blocks makes slow past it.
pub const COMPRESSED_BYTES_MAX: usize = GZIP_BYTES_MAX as usize;

/// `bytes` decompressed, at most `PSF_BYTES_MAX` of them, and the
/// compression it was in.
pub fn decompress(bytes: &[u8]) -> Result<(Vec<u8>, Compression), String> {
    let limit = PSF_BYTES_MAX as usize;
    let kind = Compression::of(bytes);
    if kind == Compression::Plain {
        if bytes.len() > limit {
            return Err(format!("larger than {PSF_BYTES_MAX} bytes"));
        }
        return Ok((bytes.to_vec(), kind));
    }
    if bytes.len() > COMPRESSED_BYTES_MAX {
        return Err(format!(
            "{}: larger than {COMPRESSED_BYTES_MAX} bytes before it is decompressed",
            kind.name()
        ));
    }
    let out = match kind {
        Compression::Gzip => gunzip(bytes, limit),
        Compression::Bzip2 => read_bounded(bzip2::read::MultiBzDecoder::new(bytes), limit),
        Compression::Xz => read_bounded(lzma_rust2::XzReader::new(bytes, true), limit),
        Compression::Zstd => unzstd(bytes, limit),
        Compression::Plain => Ok(bytes.to_vec()),
    };
    out.map(|b| (b, kind))
        .map_err(|e| format!("{}: {e}", kind.name()))
}

/// A zstd stream's frames decoded and joined, each frame's checksum
/// compared with the bytes it decoded to when the frame carries one, as
/// `zstd -d` does; `ruzstd` computes the checksum and leaves the
/// comparison to its caller.
fn unzstd(mut input: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    while !input.is_empty() {
        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(&mut input).map_err(|e| e.to_string())?;
        let frame = read_bounded(&mut decoder, limit.saturating_sub(out.len()))?;
        let frame_decoder = decoder.into_frame_decoder();
        if !frame_decoder.is_finished() {
            return Err("a frame cut short".to_owned());
        }
        if let Some(want) = frame_decoder.get_checksum_from_data() {
            if frame_decoder.get_calculated_checksum() != Some(want) {
                return Err("a frame's checksum does not match its data".to_owned());
            }
        }
        out.extend_from_slice(&frame);
    }
    Ok(out)
}

/// What `reader` decodes, refused past `limit` bytes.
fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    reader
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|e| format!("corrupt or cut short: {e}"))?;
    if out.len() > limit {
        return Err(format!("decompresses past {limit} bytes"));
    }
    Ok(out)
}

const FHCRC: u8 = 1 << 1;
const FEXTRA: u8 = 1 << 2;
const FNAME: u8 = 1 << 3;
const FCOMMENT: u8 = 1 << 4;

/// A gzip file's members (RFC 1952) decompressed and joined, each one's
/// CRC-32 and length checked, refused past `limit` bytes.
pub fn gunzip(mut input: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut buf = vec![0u8; 32 * 1024];
    while !input.is_empty() {
        input = gzip_header(input)?;
        let start = out.len();
        state.reset(DataFormat::Raw);
        loop {
            let r = inflate(&mut state, input, &mut buf, MZFlush::None);
            input = input.get(r.bytes_consumed..).unwrap_or_default();
            let written = buf.get(..r.bytes_written).unwrap_or_default();
            if out.len().saturating_add(written.len()) > limit {
                return Err(format!("decompresses past {limit} bytes"));
            }
            out.extend_from_slice(written);
            match r.status {
                Ok(MZStatus::StreamEnd) => break,
                Ok(_) if r.bytes_consumed > 0 || r.bytes_written > 0 => {}
                _ => return Err("gzip data corrupt or cut short".to_owned()),
            }
        }
        let [c0, c1, c2, c3, l0, l1, l2, l3, rest @ ..] = input else {
            return Err("gzip trailer cut short".to_owned());
        };
        let member = out.get(start..).unwrap_or_default();
        if crc32(member) != u32::from_le_bytes([*c0, *c1, *c2, *c3]) {
            return Err("gzip CRC-32 mismatch".to_owned());
        }
        // ISIZE is the member's length modulo 2^32
        if (member.len() & 0xffff_ffff) as u32 != u32::from_le_bytes([*l0, *l1, *l2, *l3]) {
            return Err("gzip length mismatch".to_owned());
        }
        input = rest;
    }
    Ok(out)
}

/// `input` past a gzip member's header (RFC 1952 2.3).
fn gzip_header(input: &[u8]) -> Result<&[u8], String> {
    let [0x1f, 0x8b, 8, flg, _, _, _, _, _, _, rest @ ..] = input else {
        return Err("not a gzip member, or a method other than deflate".to_owned());
    };
    if flg & 0xe0 != 0 {
        return Err("gzip flags unknown".to_owned());
    }
    let mut rest = rest;
    if flg & FEXTRA != 0 {
        let [x0, x1, more @ ..] = rest else {
            return Err("gzip extra field cut short".to_owned());
        };
        let xlen = usize::from(u16::from_le_bytes([*x0, *x1]));
        rest = more.get(xlen..).ok_or("gzip extra field cut short")?;
    }
    for flag in [FNAME, FCOMMENT] {
        if flg & flag != 0 {
            let nul = rest
                .iter()
                .position(|&b| b == 0)
                .ok_or("gzip name or comment unterminated")?;
            rest = rest.get(nul.saturating_add(1)..).unwrap_or_default();
        }
    }
    if flg & FHCRC != 0 {
        rest = rest.get(2..).ok_or("gzip header CRC cut short")?;
    }
    Ok(rest)
}

/// CRC-32 (ISO 3309), as RFC 1952 8 computes it, a bit at a time.
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

/// The most entries a unimap holds: kbd counts them in 16 bits.
pub const UNIMAP_ENTRIES_MAX: usize = 0xffff;
/// The most glyph positions a unimap names: kbd's 512.
const UNIMAP_GLYPHS: u32 = 512;

/// A kbd unimap (`.uni`, `.sfm`), as kbd's `loadunimap.c` reads one: each
/// line a glyph and its codepoints, `0xNN U+XXXX U+YYYY`, or a range of
/// glyphs `0xNN-0xMM` followed by `idem`, one codepoint or a codepoint
/// range of the same length; `#` starts a comment. Returns each
/// codepoint's glyph, a later line's taking a codepoint from an earlier
/// one's, as the kernel's table does.
pub fn unimap(bytes: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "a unimap that is not UTF-8".to_owned())?;
    let mut map = std::collections::BTreeMap::new();
    let mut count = 0usize;
    for (line, n) in text.lines().zip(1usize..) {
        let err = |why: &str| format!("unimap line {n}: {why}");
        let body = line.split('#').next().unwrap_or_default().trim();
        if body.is_empty() {
            continue;
        }
        let (glyphs, rest) = glyph_range(body).ok_or_else(|| err("no glyph number"))?;
        if glyphs.1 >= UNIMAP_GLYPHS || glyphs.0 > glyphs.1 {
            return Err(err("a glyph number past 0x1ff, or a range that runs back"));
        }
        let mut pairs = Vec::new();
        let words: Vec<&str> = rest.split_whitespace().collect();
        if glyphs.0 != glyphs.1 {
            let span = glyphs.1 - glyphs.0;
            match words.as_slice() {
                ["idem"] => pairs.extend((glyphs.0..=glyphs.1).map(|g| (g, g))),
                [one] => match one.split_once('-') {
                    Some((a, b)) => {
                        let (a, b) = (codepoint(a), codepoint(b));
                        let (Some(a), Some(b)) = (a, b) else {
                            return Err(err("a codepoint that is not U+XXXX"));
                        };
                        if b.checked_sub(a) != Some(span) {
                            return Err(err("a codepoint range of another length"));
                        }
                        pairs.extend((0..=span).map(|i| (glyphs.0 + i, a + i)));
                    }
                    None => {
                        let c = codepoint(one).ok_or_else(|| err("not U+XXXX"))?;
                        pairs.extend((glyphs.0..=glyphs.1).map(|g| (g, c)));
                    }
                },
                _ => return Err(err("a range takes idem, U+XXXX or U+XXXX-U+YYYY")),
            }
        } else {
            for w in words {
                let c = codepoint(w).ok_or_else(|| err("not U+XXXX"))?;
                pairs.push((glyphs.0, c));
            }
        }
        count = count.saturating_add(pairs.len());
        if count > UNIMAP_ENTRIES_MAX {
            return Err(err("more entries than a unimap holds"));
        }
        for (g, c) in pairs {
            map.insert(c, g);
        }
    }
    Ok(map.into_iter().collect())
}

/// `0xNN` or `0xNN-0xMM` (or decimal, or octal with a leading 0, as
/// `strtol` reads them) at the start of `s`, and what follows.
fn glyph_range(s: &str) -> Option<((u32, u32), &str)> {
    let (first, rest) = number(s)?;
    let rest = rest.trim_start();
    if let Some(after) = rest.strip_prefix('-') {
        let (last, rest) = number(after.trim_start())?;
        Some(((first, last), rest))
    } else {
        Some(((first, first), rest))
    }
}

fn number(s: &str) -> Option<(u32, &str)> {
    let (radix, digits) = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, hex)
    } else if s.starts_with('0') && s.len() > 1 {
        (8, s.get(1..)?)
    } else {
        (10, s)
    };
    let end = digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len());
    let (n, rest) = digits.split_at(end);
    if n.is_empty() {
        // a lone 0
        return (s.starts_with('0') && radix == 8).then_some((0, digits));
    }
    Some((u32::from_str_radix(n, radix).ok()?, rest))
}

/// `U+XXXX`: four hex digits, as kbd reads a unimap's codepoints.
fn codepoint(s: &str) -> Option<u32> {
    let hex = s.strip_prefix("U+")?;
    (hex.len() == 4 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
        .filter(|&c| char::from_u32(c).is_some())
}

/// One font of a CPI file: its height, its width and its glyphs, rows of
/// `width.div_ceil(8)` bytes.
struct CpiFont<'a> {
    height: u32,
    width: u32,
    count: u32,
    glyphs: &'a [u8],
}

/// The bytes a CPI file's codepage entry takes before its fonts: its
/// 28-byte entry header, and its info header's version, font count and
/// size.
const CPI_HEADER: usize = 0x22;
/// The most fonts one codepage holds.
const CPI_FONTS_MAX: usize = 8;
/// The most glyphs a CPI font holds.
const CPI_GLYPHS_MAX: u32 = 512;

/// The fonts of a CPI file as kbd ships them (`.cp`, the form `setfont`
/// reads): one screen codepage's entry header, device type 1, its info
/// header with the count of fonts at 0x1e, then each font, a header of its
/// height, width, two aspect bytes and 16-bit glyph count, and its glyphs,
/// the last ending the file. `None` for any other file.
fn cpi_fonts(bytes: &[u8]) -> Option<Vec<CpiFont<'_>>> {
    let u16_at = |at: usize| {
        let b = bytes.get(at..at.checked_add(2)?)?;
        Some(u16::from_le_bytes([*b.first()?, *b.get(1)?]))
    };
    if u16_at(6)? != 1 {
        return None;
    }
    let count = usize::from(u16_at(0x1e)?);
    if !(1..=CPI_FONTS_MAX).contains(&count) {
        return None;
    }
    let mut fonts = Vec::with_capacity(count);
    let mut at = CPI_HEADER;
    for _ in 0..count {
        let (height, width) = (u32::from(*bytes.get(at)?), u32::from(*bytes.get(at + 1)?));
        let glyphs = u32::from(u16_at(at + 4)?);
        if height == 0 || !(1..=32).contains(&width) || !(1..=CPI_GLYPHS_MAX).contains(&glyphs) {
            return None;
        }
        let size = (width.div_ceil(8) * height * glyphs) as usize;
        let start = at.checked_add(6)?;
        let end = start.checked_add(size)?;
        fonts.push(CpiFont {
            height,
            width,
            count: glyphs,
            glyphs: bytes.get(start..end)?,
        });
        at = end;
    }
    (at == bytes.len()).then_some(fonts)
}

/// The font of `height` pixels in a CPI file, as `cp-size=` takes it
/// (ADR-1200 D1), as a PSF2 font with no table: decompressed, its
/// codepage's fonts read, and the one of that height taken.
pub fn cpi(bytes: &[u8], height: u32) -> Result<Vec<u8>, String> {
    let (plain, _) = decompress(bytes)?;
    let fonts = cpi_fonts(&plain).ok_or("cp-size= is for a CPI font, and this is none")?;
    let font = fonts.iter().find(|f| f.height == height).ok_or_else(|| {
        let held: Vec<String> = fonts.iter().map(|f| f.height.to_string()).collect();
        format!(
            "cp-size={height}: the CPI font holds {} pixels high, none {height}",
            held.join(", ")
        )
    })?;
    let mut out = Vec::with_capacity(32 + font.glyphs.len());
    for v in [
        0x864a_b572u32,
        0,
        32,
        0,
        font.count,
        font.width.div_ceil(8) * font.height,
        font.height,
        font.width,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(font.glyphs);
    Ok(out)
}

/// A font file's bytes parsed into the glyph form: decompressed, read as
/// PSF1 or PSF2, and mapped by `unimap` when the frit gives one, else by
/// the font's own table, else by `def_uni`, kbd's default map. A font with
/// none of them is refused, naming `unimap=`, and a CPI font, which holds
/// several, is refused naming `cp-size=`.
pub fn font(bytes: &[u8], unimap: Option<&[u8]>, def_uni: Option<&[u8]>) -> Result<Glyphs, String> {
    let (plain, compression) = decompress(bytes)?;
    let kind = match plain.as_slice() {
        [0x36, 0x04, ..] => "psf1",
        [0x72, 0xb5, 0x4a, 0x86, ..] => "psf2",
        [b'S', b'T', b'A', b'R', b'T', b'F', b'O', b'N', b'T', ..] => {
            return Err("a BDF font, which a bdf: source names".to_owned());
        }
        _ if cpi_fonts(&plain).is_some() => {
            return Err(
                "a CPI font holds a font of each size: give the source a cp-size= of 8, 14 or 16"
                    .to_owned(),
            );
        }
        _ => {
            return Err("not a PSF or CPI font (raw fonts are not read yet)".to_owned());
        }
    };
    let psf = Psf::parse(&plain).map_err(|e| e.to_string())?;
    let count = psf.glyph_count();
    let (width, height) = psf.glyph_size();
    let row = width.div_ceil(8) as usize;
    let mut bits = Vec::with_capacity(
        row.saturating_mul(height as usize)
            .saturating_mul(count as usize),
    );
    for g in 0..count {
        let glyph = psf
            .render(g, Style::default())
            .ok_or_else(|| format!("glyph {g} cannot be drawn"))?;
        for y in 0..height {
            let mut packed = vec![0u8; row];
            for x in 0..width {
                if glyph
                    .pixel(x, y)
                    .and_then(|p| p.first())
                    .is_some_and(|&a| a >= 0x80)
                {
                    if let Some(byte) = packed.get_mut((x / 8) as usize) {
                        *byte |= 0x80 >> (x % 8);
                    }
                }
            }
            bits.extend_from_slice(&packed);
        }
    }
    let unicode = |bytes: &[u8]| -> Result<Vec<(u32, u32)>, String> {
        let (plain, _) = decompress(bytes)?;
        Ok(self::unimap(&plain)?
            .into_iter()
            .filter(|&(_, g)| g < count)
            .collect())
    };
    let (map, table) = match (unimap, psf.has_table(), def_uni) {
        (Some(u), _, _) => (unicode(u)?, "unimap"),
        (None, true, _) => (
            (0..=0x10_ffff_u32)
                .filter_map(|c| Some((c, psf.glyph_index(char::from_u32(c)?)?)))
                .collect(),
            "table",
        ),
        (None, false, Some(d)) => (unicode(d)?, "def.uni"),
        (None, false, None) => {
            return Err(
                "the font has no Unicode table and kbd's def.uni was not found: give the source a unimap="
                    .to_owned(),
            );
        }
    };
    let format = match compression {
        Compression::Plain => format!("{kind}+{table}"),
        c => format!("{kind}+{}+{table}", c.name()),
    };
    Ok(Glyphs {
        width,
        height,
        count,
        bits,
        map,
        format,
    })
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test fails by panicking"
)]
pub(crate) mod tests {
    use std::io::{Read, Write};

    use super::*;

    /// A PSF2 font of `count` glyphs `width` by `height`, glyph `g`'s rows
    /// each `g`, with a table mapping `table`'s codepoints to glyphs.
    pub(crate) fn psf2(
        width: u32,
        height: u32,
        count: u32,
        table: Option<&[(u32, char)]>,
    ) -> Vec<u8> {
        let row = width.div_ceil(8);
        let mut f = Vec::new();
        for v in [
            0x864a_b572u32,
            0,
            32,
            u32::from(table.is_some()),
            count,
            row * height,
            height,
            width,
        ] {
            f.extend_from_slice(&v.to_le_bytes());
        }
        for g in 0..count {
            for _ in 0..height * row {
                f.push(g as u8);
            }
        }
        if let Some(table) = table {
            for g in 0..count {
                for &(tg, c) in table {
                    if tg == g {
                        let mut b = [0; 4];
                        f.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                    }
                }
                f.push(0xff);
            }
        }
        f
    }

    /// `bytes` in a gzip member, stored with deflate's fixed level 6.
    pub(crate) fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
        out.extend(miniz_oxide::deflate::compress_to_vec(bytes, 6));
        out.extend_from_slice(&crc32(bytes).to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out
    }

    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    /// `bytes` in a bzip2 stream.
    pub(crate) fn bzip2(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        bzip2::read::BzEncoder::new(bytes, bzip2::Compression::best())
            .read_to_end(&mut out)
            .unwrap();
        out
    }

    /// `bytes` in an xz stream with a CRC-64 check.
    pub(crate) fn xz(bytes: &[u8]) -> Vec<u8> {
        let mut w =
            lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::default()).unwrap();
        w.write_all(bytes).unwrap();
        w.finish().unwrap()
    }

    /// `bytes` in a zstd frame with its checksum.
    pub(crate) fn zstd(bytes: &[u8]) -> Vec<u8> {
        ruzstd::encoding::compress_to_vec(bytes, ruzstd::encoding::CompressionLevel::Fastest)
    }

    #[test]
    fn each_compression_is_told_by_its_magic_and_decoded() {
        let plain = psf2(8, 2, 2, None);
        assert_eq!(decompress(&plain).expect("plain").1, Compression::Plain);
        for (packed, kind) in [
            (gzip(&plain), Compression::Gzip),
            (bzip2(&plain), Compression::Bzip2),
            (xz(&plain), Compression::Xz),
            (zstd(&plain), Compression::Zstd),
        ] {
            let (back, c) = decompress(&packed).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert_eq!((back.as_slice(), c), (plain.as_slice(), kind));
        }
    }

    #[test]
    fn a_corrupt_or_cut_stream_is_refused_in_each_compression() {
        let plain = psf2(8, 16, 64, None);
        for (name, packed) in [
            ("bzip2", bzip2(&plain)),
            ("xz", xz(&plain)),
            ("zstd", zstd(&plain)),
        ] {
            let e = decompress(&packed[..packed.len() - 4]).expect_err(name);
            assert!(e.starts_with(name), "{name}: {e}");
            // a flipped byte in the checked data or its check value
            let mut bad = packed.clone();
            let at = bad.len() - 6;
            bad[at] ^= 0x55;
            assert!(decompress(&bad).is_err(), "{name}: a flipped byte decoded");
        }
    }

    #[test]
    fn a_zstd_frame_whose_checksum_differs_is_refused() {
        let plain = psf2(8, 16, 64, None);
        let mut bad = zstd(&plain);
        let last = bad.len() - 1;
        bad[last] ^= 1;
        let e = decompress(&bad).expect_err("checksum");
        assert!(e.contains("checksum"), "{e}");
        // two frames back to back decode as one stream, as zstd -d reads them
        let mut two = zstd(&plain[..100]);
        two.extend(zstd(&plain[100..]));
        assert_eq!(decompress(&two).expect("two frames").0, plain);
    }

    #[test]
    fn each_compression_is_refused_past_its_bounds() {
        let big = vec![0u8; PSF_BYTES_MAX as usize + 1];
        for (name, packed) in [
            ("gzip", gzip(&big)),
            ("bzip2", bzip2(&big)),
            ("xz", xz(&big)),
            ("zstd", zstd(&big)),
        ] {
            assert!(
                packed.len() <= COMPRESSED_BYTES_MAX,
                "{name}: the fixture is small"
            );
            let e = decompress(&packed).expect_err(name);
            assert!(e.contains("past"), "{name}: {e}");
        }
        let mut long = bzip2(b"x");
        long.resize(COMPRESSED_BYTES_MAX + 1, 0);
        let e = decompress(&long).expect_err("long");
        assert!(e.contains("before it is decompressed"), "{e}");
    }

    #[test]
    fn a_corrupt_gzip_is_refused() {
        let mut g = gzip(b"hello");
        let crc = g.len() - 8;
        g[crc] ^= 1;
        assert!(decompress(&g).expect_err("crc").contains("CRC-32"));
        let g = gzip(b"hello");
        assert!(decompress(&g[..g.len() - 3]).is_err());
    }

    #[test]
    fn a_unimap_reads_as_kbd_reads_it() {
        let text = b"# comment\n\
            0x41 U+0041 U+00C0 # trailing\n\
            0x20-0x22 idem\n\
            0x30-0x31 U+2500\n\
            0x60-0x62 U+0391-U+0393\n\
            0101 U+0042\n\
            0x41 U+0042\n";
        let map = unimap(text).expect("parses");
        let get = |c: u32| map.iter().find(|&&(m, _)| m == c).map(|&(_, g)| g);
        assert_eq!(get(0x41), Some(0x41));
        assert_eq!(get(0xc0), Some(0x41));
        assert_eq!(get(0x21), Some(0x21));
        // a range to one codepoint leaves it with the range's last glyph
        assert_eq!(get(0x2500), Some(0x31));
        assert_eq!(get(0x392), Some(0x61));
        // octal 0101 is 0x41; the later line takes U+0042
        assert_eq!(get(0x42), Some(0x41));
        assert!(map.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn a_malformed_unimap_is_refused_by_line() {
        for (text, line) in [
            (&b"0x41 U+41\n"[..], 1),
            (b"\n0x200 U+0041\n", 2),
            (b"0x40-0x41 U+0041-U+0043\n", 1),
            (b"0x41-0x40 idem\n", 1),
            (b"U+0041\n", 1),
            (b"0x40-0x41 junk\n", 1),
        ] {
            let e = unimap(text).expect_err("refused");
            assert!(e.starts_with(&format!("unimap line {line}:")), "{e}");
        }
    }

    #[test]
    fn a_font_with_a_table_keeps_it() {
        let f = psf2(8, 3, 4, Some(&[(1, 'A'), (2, 'B'), (2, 'b')]));
        let g = font(&gzip(&f), None, Some(b"0x00 U+0041\n")).expect("font");
        assert_eq!((g.width, g.height, g.count), (8, 3, 4));
        assert_eq!(g.map, [(0x41, 1), (0x42, 2), (0x62, 2)]);
        assert_eq!(g.glyph(2), Some(&[2u8, 2, 2][..]));
        assert_eq!(g.format, "psf2+gzip+table");
    }

    #[test]
    fn a_font_without_a_table_takes_the_unimap_then_def_uni() {
        let f = psf2(8, 1, 2, None);
        let g = font(
            &f,
            Some(b"0x01 U+0041\n0x05 U+0042\n"),
            Some(b"0x00 U+0041\n"),
        )
        .unwrap();
        // glyph 5 is past the font, so U+0042 maps nothing
        assert_eq!(
            (g.map.as_slice(), g.format.as_str()),
            (&[(0x41, 1)][..], "psf2+unimap")
        );
        let g = font(&f, None, Some(b"0x00 U+0041\n")).unwrap();
        assert_eq!(
            (g.map.as_slice(), g.format.as_str()),
            (&[(0x41, 0)][..], "psf2+def.uni")
        );
        let e = font(&f, None, None).expect_err("no map");
        assert!(e.contains("unimap="), "{e}");
    }

    #[test]
    fn a_wide_glyph_packs_its_rows() {
        let f = psf2(12, 1, 1, Some(&[(0, 'x')]));
        let g = font(&f, None, None).unwrap();
        // each row of glyph 0 is all zeros; draw a font whose glyph is ones
        assert_eq!(g.glyph(0), Some(&[0u8, 0][..]));
        let mut f = psf2(12, 1, 1, Some(&[(0, 'x')]));
        f[32] = 0xff;
        f[33] = 0xf0;
        let g = font(&f, None, None).unwrap();
        assert_eq!(g.glyph(0), Some(&[0xffu8, 0xf0][..]));
    }

    #[test]
    fn not_a_psf_is_refused() {
        let e = font(b"STARTFONT 2.1\n", None, None).expect_err("bdf");
        assert!(e.contains("a bdf: source"), "{e}");
        let e = font(b"\0\0\0\0", None, None).expect_err("raw");
        assert!(e.contains("not a PSF or CPI font"), "{e}");
    }

    /// A CPI file as kbd ships one: a screen codepage's 28-byte entry, its
    /// info header, and a font for each of `heights`, 8 wide with 256
    /// glyphs, glyph `g`'s rows each `g` plus the height.
    pub(crate) fn cpi_file(heights: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8; 0x22];
        f[0] = 0x1c;
        f[6] = 1;
        f[8..16].copy_from_slice(b"EGA     ");
        f[0x10..0x12].copy_from_slice(&437u16.to_le_bytes());
        f[0x1c] = 1;
        f[0x1e] = heights.len() as u8;
        for &h in heights {
            f.extend_from_slice(&[h, 8, 0, 0, 0, 1]);
            for g in 0..256u32 {
                f.extend(std::iter::repeat_n(
                    (g as u8).wrapping_add(h),
                    usize::from(h),
                ));
            }
        }
        f
    }

    // ADR-1200 D1: cp-size= takes the CPI font of that height
    #[test]
    fn cp_size_takes_the_cpi_font_of_its_height() {
        let file = cpi_file(&[16, 14, 8]);
        assert_eq!(file.len(), 9780, "the size of kbd's three-font .cp files");
        for (h, packed) in [(14, gzip(&file)), (8, file.clone()), (16, zstd(&file))] {
            let psf = cpi(&packed, h).unwrap_or_else(|e| panic!("{h}: {e}"));
            let g = font(&psf, None, Some(b"0x41 U+0041\n")).expect("parses");
            assert_eq!(
                (g.width, g.height, g.count, g.format.as_str()),
                (8, h, 256, "psf2+def.uni")
            );
            let a = (0x41 + h) as u8;
            assert_eq!(g.glyph(0x41), Some(&vec![a; h as usize][..]), "{h}");
        }
        let e = cpi(&cpi_file(&[16, 8]), 14).expect_err("no 14");
        assert_eq!(
            e,
            "cp-size=14: the CPI font holds 16, 8 pixels high, none 14"
        );
        let e = cpi(&psf2(8, 16, 256, None), 16).expect_err("a PSF");
        assert!(e.contains("cp-size= is for a CPI font"), "{e}");
        // a CPI font with no cp-size= is refused, naming it
        let e = font(&file, None, Some(b"0x41 U+0041\n")).expect_err("no cp-size=");
        assert!(e.contains("cp-size="), "{e}");
    }

    #[test]
    fn a_cpi_file_cut_short_or_longer_is_none() {
        let file = cpi_file(&[16, 14, 8]);
        for bad in [
            file[..file.len() - 1].to_vec(),
            [file.as_slice(), &[0]].concat(),
            file[..0x21].to_vec(),
        ] {
            assert!(cpi(&bad, 16).expect_err("refused").contains("is none"));
        }
        let mut printer = file.clone();
        printer[6] = 2;
        assert!(cpi(&printer, 16).is_err(), "a printer's codepage");
        let mut many = file;
        many[0x1e] = 9;
        assert!(cpi(&many, 16).is_err(), "more fonts than CPI_FONTS_MAX");
    }
}
