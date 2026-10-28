//! PNG, as the specimens are committed: 8-bit RGB, not interlaced. The
//! writer picks each row's filter by the smallest sum of its bytes as
//! signed values, the usual heuristic, and deflates at the strongest level,
//! so the same image gives the same bytes on every host. The reader takes
//! any filter and refuses any other form, a chunk whose CRC is wrong, and
//! an image past `SIDE_MAX`.

use miniz_oxide::deflate::compress_to_vec_zlib;
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::draw::Image;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// The widest or tallest image read: the largest profile's collage has
/// room to spare under it.
pub const SIDE_MAX: u32 = 16384;

/// The CRC-32 of PNG's chunks (ISO 3309, as PNG's annex D gives it).
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for part in parts {
        for &byte in *part {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    0xedb8_8320 ^ (crc >> 1)
                } else {
                    crc >> 1
                };
            }
        }
    }
    !crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    let len = u32::try_from(data.len()).expect("a chunk under 4 GiB");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Filter type `kind` of a row: `raw` and the row above, `up`.
fn filter(kind: u8, raw: &[u8], up: &[u8], out: &mut Vec<u8>) {
    out.push(kind);
    for i in 0..raw.len() {
        let a = if i >= 3 { raw[i - 3] } else { 0 };
        let c = if i >= 3 { up[i - 3] } else { 0 };
        let pred = match kind {
            0 => 0,
            1 => a,
            2 => up[i],
            3 => ((u16::from(a) + u16::from(up[i])) / 2) as u8,
            _ => paeth(a, up[i], c),
        };
        out.push(raw[i].wrapping_sub(pred));
    }
}

/// `image` as a PNG file.
pub fn encode(image: &Image) -> Vec<u8> {
    let stride = image.width as usize * 3;
    let zero = vec![0u8; stride];
    let mut data = Vec::with_capacity((stride + 1) * image.height as usize);
    let mut trial = Vec::with_capacity(stride + 1);
    for (y, raw) in image.rgb.chunks(stride).enumerate() {
        let up = if y == 0 {
            &zero[..]
        } else {
            &image.rgb[(y - 1) * stride..y * stride]
        };
        let mut best: Option<(u64, Vec<u8>)> = None;
        for kind in 0..5 {
            trial.clear();
            filter(kind, raw, up, &mut trial);
            let cost = trial[1..]
                .iter()
                .map(|&b| u64::from((b as i8).unsigned_abs()))
                .sum();
            if best.as_ref().is_none_or(|(c, _)| cost < *c) {
                best = Some((cost, trial.clone()));
            }
        }
        if let Some((_, row)) = best {
            data.extend_from_slice(&row);
        }
    }
    let mut out = SIGNATURE.to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&image.width.to_be_bytes());
    ihdr.extend_from_slice(&image.height.to_be_bytes());
    // 8 bits, RGB, deflate, adaptive filtering, not interlaced
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &compress_to_vec_zlib(&data, 10));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The image in a PNG file `bytes`, or why it is refused.
pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    let mut rest = bytes.strip_prefix(&SIGNATURE[..]).ok_or("not a PNG file")?;
    let mut header: Option<(u32, u32)> = None;
    let mut idat = Vec::new();
    loop {
        if rest.len() < 12 {
            return Err("a chunk is cut short".to_owned());
        }
        let len = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        let kind = &rest[4..8];
        if rest.len() < 12 + len {
            return Err("a chunk is cut short".to_owned());
        }
        let data = &rest[8..8 + len];
        let crc =
            u32::from_be_bytes([rest[8 + len], rest[9 + len], rest[10 + len], rest[11 + len]]);
        if crc != crc32(&[kind, data]) {
            return Err(format!(
                "chunk {} fails its CRC",
                String::from_utf8_lossy(kind)
            ));
        }
        rest = &rest[12 + len..];
        match kind {
            b"IHDR" => {
                let [
                    w0,
                    w1,
                    w2,
                    w3,
                    h0,
                    h1,
                    h2,
                    h3,
                    depth,
                    color,
                    comp,
                    filt,
                    lace,
                ] = data[..]
                else {
                    return Err("IHDR is not 13 bytes".to_owned());
                };
                let (w, h) = (
                    u32::from_be_bytes([w0, w1, w2, w3]),
                    u32::from_be_bytes([h0, h1, h2, h3]),
                );
                if [depth, color, comp, filt, lace] != [8, 2, 0, 0, 0] {
                    return Err("not 8-bit RGB without interlacing".to_owned());
                }
                if w == 0 || h == 0 || w > SIDE_MAX || h > SIDE_MAX {
                    return Err(format!("a side of {w}x{h} pixels"));
                }
                header = Some((w, h));
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ if kind[0].is_ascii_uppercase() => {
                return Err(format!(
                    "the critical chunk {} is not read",
                    String::from_utf8_lossy(kind)
                ));
            }
            _ => {}
        }
    }
    let (width, height) = header.ok_or("no IHDR")?;
    let stride = width as usize * 3;
    let want = (stride + 1) * height as usize;
    let data = decompress_to_vec_zlib_with_limit(&idat, want)
        .map_err(|e| format!("IDAT does not inflate: {e:?}"))?;
    if data.len() != want {
        return Err(format!("IDAT holds {} bytes, not {want}", data.len()));
    }
    let mut rgb = vec![0u8; stride * height as usize];
    for (y, row) in data.chunks(stride + 1).enumerate() {
        let (done, cur) = rgb.split_at_mut(y * stride);
        let up = if y == 0 {
            None
        } else {
            Some(&done[(y - 1) * stride..])
        };
        let cur = &mut cur[..stride];
        for i in 0..stride {
            let a = if i >= 3 { cur[i - 3] } else { 0 };
            let b = up.map_or(0, |u| u[i]);
            let c = if i >= 3 {
                up.map_or(0, |u| u[i - 3])
            } else {
                0
            };
            let pred = match row[0] {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => paeth(a, b, c),
                f => return Err(format!("row {y} has filter type {f}")),
            };
            cur[i] = row[1 + i].wrapping_add(pred);
        }
    }
    Ok(Image { width, height, rgb })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Image {
        let (width, height) = (7, 5);
        let rgb = (0..width * height * 3)
            .map(|i| (i * 37 % 251) as u8)
            .collect();
        Image { width, height, rgb }
    }

    #[test]
    fn an_image_reads_back_as_written() {
        let img = sample();
        assert_eq!(decode(&encode(&img)).unwrap(), img);
    }

    #[test]
    fn the_crc_is_png_s() {
        // the CRC of an empty IEND chunk, as every PNG file ends
        assert_eq!(crc32(&[b"IEND"]), 0xae42_6082);
    }

    #[test]
    fn every_filter_reads_back() {
        let img = sample();
        let stride = img.width as usize * 3;
        for kind in 0..5 {
            let mut data = Vec::new();
            let zero = vec![0u8; stride];
            for (y, raw) in img.rgb.chunks(stride).enumerate() {
                let up = if y == 0 {
                    &zero[..]
                } else {
                    &img.rgb[(y - 1) * stride..y * stride]
                };
                filter(kind, raw, up, &mut data);
            }
            let mut file = SIGNATURE.to_vec();
            let mut ihdr = img.width.to_be_bytes().to_vec();
            ihdr.extend_from_slice(&img.height.to_be_bytes());
            ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
            chunk(&mut file, b"IHDR", &ihdr);
            chunk(&mut file, b"IDAT", &compress_to_vec_zlib(&data, 1));
            chunk(&mut file, b"IEND", &[]);
            assert_eq!(decode(&file).unwrap(), img, "filter {kind}");
        }
    }

    #[test]
    fn a_damaged_file_is_refused() {
        let good = encode(&sample());
        let mut bad = good.clone();
        let last = bad.len() - 20;
        bad[last] ^= 1;
        assert!(decode(&bad).unwrap_err().contains("CRC"));
        assert!(decode(&good[..good.len() - 4]).is_err());
        assert!(decode(b"GIF89a").is_err());
        let mut gray = good.clone();
        gray[8 + 8 + 9] = 0;
        assert!(decode(&gray).is_err());
    }
}
