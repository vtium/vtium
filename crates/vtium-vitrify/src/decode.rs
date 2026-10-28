//! The decoders the parser runs (ADR-1200 D6, D14): a compression found by
//! its magic, as kbd finds it, the PSF font inside, and the Unicode map a
//! font without its own table takes. Everything here reads bytes someone
//! else supplied, and runs only in the parser's child process.
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
        Compression::Zstd => ruzstd::decoding::StreamingDecoder::new(bytes)
            .map_err(|e| e.to_string())
            .and_then(|d| read_bounded(d, limit)),
        Compression::Plain => Ok(bytes.to_vec()),
    };
    out.map(|b| (b, kind))
        .map_err(|e| format!("{}: {e}", kind.name()))
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

/// A font file's bytes parsed into the glyph form: decompressed, read as
/// PSF1 or PSF2, and mapped by `unimap` when the frit gives one, else by
/// the font's own table, else by `def_uni`, kbd's default map. A font with
/// none of them is refused, naming `unimap=`.
pub fn font(bytes: &[u8], unimap: Option<&[u8]>, def_uni: Option<&[u8]>) -> Result<Glyphs, String> {
    let (plain, compression) = decompress(bytes)?;
    let kind = match plain.as_slice() {
        [0x36, 0x04, ..] => "psf1",
        [0x72, 0xb5, 0x4a, 0x86, ..] => "psf2",
        _ => {
            return Err(
                "not a PSF font (CPI and raw fonts are read in a later slice of vtium/vtium#125)"
                    .to_owned(),
            );
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
        assert!(e.contains("not a PSF font"), "{e}");
    }
}
