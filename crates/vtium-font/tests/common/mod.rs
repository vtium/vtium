//! Builders for crafted PSF fonts and gzip files, shared by the tests and
//! the fuzz target.
#![allow(dead_code)]

/// PSF1's header: magic, mode, height.
pub fn psf1(mode: u8, height: u8) -> Vec<u8> {
    vec![0x36, 0x04, mode, height]
}

/// PSF2's header with every field given.
pub fn psf2_header(
    header: u32,
    flags: u32,
    glyphs: u32,
    glyph_bytes: u32,
    height: u32,
    width: u32,
) -> Vec<u8> {
    let mut b = vec![0x72, 0xb5, 0x4a, 0x86];
    for v in [0, header, flags, glyphs, glyph_bytes, height, width] {
        b.extend(v.to_le_bytes());
    }
    b
}

/// PSF2's 32-byte header.
pub fn psf2(flags: u32, glyphs: u32, glyph_bytes: u32, height: u32, width: u32) -> Vec<u8> {
    psf2_header(32, flags, glyphs, glyph_bytes, height, width)
}

/// A 512-glyph 8x16 PSF2 font with a unicode table, whose bytes follow.
pub fn psf2_with_table() -> Vec<u8> {
    let mut b = psf2(1, 512, 16, 16, 8);
    b.resize(b.len() + 512 * 16, 0xaa);
    b
}

/// A 512-glyph 8x16 PSF1 font with a unicode table, whose units follow.
pub fn psf1_with_table() -> Vec<u8> {
    let mut b = psf1(0x03, 16);
    b.resize(b.len() + 512 * 16, 0xaa);
    b
}

/// `units` as PSF1 table bytes.
pub fn units(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|u| u.to_le_bytes()).collect()
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                0xedb8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// One gzip member holding `data`, with no optional header fields.
pub fn gzip(data: &[u8]) -> Vec<u8> {
    gzip_with(data, 0, &[])
}

/// One gzip member with header flags `flg` and the optional fields in
/// `fields`, already laid out in RFC 1952's order.
pub fn gzip_with(data: &[u8], flg: u8, fields: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, flg, 0, 0, 0, 0, 0, 3];
    out.extend(fields);
    out.extend(miniz_oxide::deflate::compress_to_vec(data, 6));
    out.extend(crc32(data).to_le_bytes());
    out.extend((data.len() as u32).to_le_bytes());
    out
}

/// A gzip file's members decompressed, for the kbd sweep; `None` if it is
/// not one this helper reads.
pub fn gunzip(mut bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let [0x1f, 0x8b, 8, flg, _, _, _, _, _, _, rest @ ..] = bytes else {
            return None;
        };
        let mut rest = rest;
        if flg & 4 != 0 {
            let n = usize::from(u16::from_le_bytes([*rest.first()?, *rest.get(1)?]));
            rest = rest.get(2 + n..)?;
        }
        for f in [8, 16] {
            if flg & f != 0 {
                rest = rest.get(rest.iter().position(|&b| b == 0)? + 1..)?;
            }
        }
        if flg & 2 != 0 {
            rest = rest.get(2..)?;
        }
        let mut state =
            miniz_oxide::inflate::stream::InflateState::new_boxed(miniz_oxide::DataFormat::Raw);
        let mut buf = vec![0; 1 << 16];
        loop {
            let r = miniz_oxide::inflate::stream::inflate(
                &mut state,
                rest,
                &mut buf,
                miniz_oxide::MZFlush::None,
            );
            out.extend(&buf[..r.bytes_written]);
            rest = &rest[r.bytes_consumed..];
            match r.status {
                Ok(miniz_oxide::MZStatus::StreamEnd) => break,
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        bytes = rest.get(8..)?;
    }
    Some(out)
}

/// A section's tag and bytes.
pub type Section = ([u8; 4], Vec<u8>);

/// A vitrified set's sections, in order, as the format lays them out:
/// the tests take a set apart, change a section and put it back together
/// to craft the refusals.
pub fn sections(set: &[u8]) -> (u16, u16, Vec<Section>) {
    let u16_at = |i: usize| u16::from_le_bytes([set[i], set[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes(set[i..i + 4].try_into().unwrap());
    let u64_at = |i: usize| u64::from_le_bytes(set[i..i + 8].try_into().unwrap());
    let count = u32_at(12) as usize;
    let mut out = Vec::new();
    for s in 0..count {
        let at = 16 + 20 * s;
        let tag: [u8; 4] = set[at..at + 4].try_into().unwrap();
        let (offset, len) = (u64_at(at + 4) as usize, u64_at(at + 12) as usize);
        out.push((tag, set[offset..offset + len].to_vec()));
    }
    (u16_at(8), u16_at(10), out)
}

/// A set put back together from its versions and sections, each section
/// placed after the header in the order given.
pub fn assemble(major: u16, minor: u16, sections: &[Section]) -> Vec<u8> {
    let mut out = b"VTGLASS\0".to_vec();
    out.extend(major.to_le_bytes());
    out.extend(minor.to_le_bytes());
    out.extend((sections.len() as u32).to_le_bytes());
    let mut at = (16 + 20 * sections.len()) as u64;
    for (tag, bytes) in sections {
        out.extend(tag);
        out.extend(at.to_le_bytes());
        out.extend((bytes.len() as u64).to_le_bytes());
        at += bytes.len() as u64;
    }
    for (_, bytes) in sections {
        out.extend(bytes);
    }
    out
}

/// `set` with section `tag`'s bytes changed by `f`.
pub fn with_section(set: &[u8], tag: &[u8; 4], f: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let (major, minor, mut secs) = sections(set);
    let s = secs
        .iter_mut()
        .find(|(t, _)| t == tag)
        .expect("the section");
    f(&mut s.1);
    assemble(major, minor, &secs)
}
