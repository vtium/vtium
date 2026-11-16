//! Just enough of the OpenType file format to check a built face and to
//! write it again with tables replaced: the table directory, the `name`
//! table's strings and the characters the `cmap` table maps.
//!
//! Every read is bounds-checked and returns an error instead of
//! panicking, so a truncated or damaged file is reported, not crashed on.

use std::collections::BTreeSet;

/// A font file's bytes and its table directory.
pub struct Font<'a> {
    version: u32,
    /// Each table's tag and bytes, in the order the table directory lists
    /// them.
    tables: Vec<([u8; 4], &'a [u8])>,
    /// The tables' tags in the order their bytes lie in the file.
    file_order: Vec<[u8; 4]>,
}

/// One record of the `name` table, decoded.
#[derive(Clone, Debug)]
pub struct Name {
    pub platform: u16,
    pub name_id: u16,
    pub text: String,
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    match bytes.get(at..at.saturating_add(2)) {
        Some(&[a, b]) => Ok(u16::from_be_bytes([a, b])),
        _ => Err(format!("the font ends before offset {at}")),
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    match bytes.get(at..at.saturating_add(4)) {
        Some(&[a, b, c, d]) => Ok(u32::from_be_bytes([a, b, c, d])),
        _ => Err(format!("the font ends before offset {at}")),
    }
}

fn slice(bytes: &[u8], at: usize, len: usize) -> Result<&[u8], String> {
    bytes
        .get(at..at.saturating_add(len))
        .ok_or_else(|| format!("{len} bytes at offset {at} lie past the end of the data"))
}

fn usize_of(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

impl<'a> Font<'a> {
    /// Reads the table directory of a TrueType or OpenType file.
    pub fn parse(bytes: &'a [u8]) -> Result<Font<'a>, String> {
        let version = u32_at(bytes, 0)?;
        if version != 0x0001_0000 && version != u32::from_be_bytes(*b"OTTO") {
            return Err(format!(
                "not a TrueType or OpenType file (version {version:#010x})"
            ));
        }
        let count = usize::from(u16_at(bytes, 4)?);
        let mut tables = Vec::with_capacity(count);
        let mut offsets = Vec::with_capacity(count);
        for i in 0..count {
            let record = 12usize.saturating_add(i.saturating_mul(16));
            let tag = slice(bytes, record, 4)?;
            let tag = [tag[0], tag[1], tag[2], tag[3]];
            let offset = usize_of(u32_at(bytes, record.saturating_add(8))?);
            let length = usize_of(u32_at(bytes, record.saturating_add(12))?);
            tables.push((tag, slice(bytes, offset, length)?));
            offsets.push((offset, tag));
        }
        offsets.sort();
        Ok(Font {
            version,
            tables,
            file_order: offsets.into_iter().map(|(_, tag)| tag).collect(),
        })
    }

    /// Writes the font again, with the tables that `replace` names holding
    /// the given bytes in place of their own. Every other table keeps its
    /// bytes, and the tables' data keeps its order in the file.
    pub fn rewrite(&self, replace: &[([u8; 4], &[u8])]) -> Vec<u8> {
        let tables: Vec<([u8; 4], &[u8])> = self
            .file_order
            .iter()
            .filter_map(|tag| {
                let own = self.table(tag)?;
                let new = replace.iter().find(|(t, _)| t == tag).map(|(_, b)| *b);
                Some((*tag, new.unwrap_or(own)))
            })
            .collect();
        assemble(self.version, &tables)
    }

    /// The bytes of the table tagged `tag`.
    pub fn table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        self.tables.iter().find(|(t, _)| t == tag).map(|(_, b)| *b)
    }

    /// Every table but the one tagged `except`, with its tag.
    pub fn tables_except(&self, except: &[u8; 4]) -> impl Iterator<Item = ([u8; 4], &'a [u8])> {
        self.tables
            .iter()
            .filter(move |(t, _)| t != except)
            .copied()
    }

    /// The `name` table's records. Strings of the Unicode and Windows
    /// platforms are UTF-16BE; strings of the Macintosh platform are read
    /// one byte to a character, which is exact for ASCII.
    pub fn names(&self) -> Result<Vec<Name>, String> {
        let table = self.table(b"name").ok_or("the font has no name table")?;
        let count = usize::from(u16_at(table, 2)?);
        let strings = usize::from(u16_at(table, 4)?);
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let record = 6usize.saturating_add(i.saturating_mul(12));
            let platform = u16_at(table, record)?;
            let name_id = u16_at(table, record.saturating_add(6))?;
            let length = usize::from(u16_at(table, record.saturating_add(8))?);
            let offset = usize::from(u16_at(table, record.saturating_add(10))?);
            let raw = slice(table, strings.saturating_add(offset), length)?;
            let text = if platform == 1 {
                raw.iter().map(|&b| char::from(b)).collect()
            } else {
                let units: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&pair| u16::from_be_bytes(pair))
                    .collect();
                String::from_utf16_lossy(&units)
            };
            names.push(Name {
                platform,
                name_id,
                text,
            });
        }
        Ok(names)
    }

    /// Every character that a Unicode subtable of the `cmap` table maps to
    /// a glyph other than glyph 0, the missing-glyph box. Reads subtables
    /// of format 4 (the Basic Multilingual Plane) and 12 (all of Unicode).
    pub fn characters(&self) -> Result<BTreeSet<u32>, String> {
        let table = self.table(b"cmap").ok_or("the font has no cmap table")?;
        let count = usize::from(u16_at(table, 2)?);
        let mut chars = BTreeSet::new();
        for i in 0..count {
            let record = 4usize.saturating_add(i.saturating_mul(8));
            let platform = u16_at(table, record)?;
            let encoding = u16_at(table, record.saturating_add(2))?;
            let unicode = platform == 0 || (platform == 3 && (encoding == 1 || encoding == 10));
            if !unicode {
                continue;
            }
            let at = usize_of(u32_at(table, record.saturating_add(4))?);
            let sub = table
                .get(at..)
                .ok_or("a cmap subtable lies past the table")?;
            match u16_at(sub, 0)? {
                4 => format4(sub, &mut chars)?,
                12 => format12(sub, &mut chars)?,
                _ => {}
            }
        }
        Ok(chars)
    }
}

/// The checksum of a table: the sum of its bytes read as big-endian 32-bit
/// numbers, with the last number padded with zeros.
fn checksum(data: &[u8]) -> u32 {
    let (words, rest) = data.as_chunks::<4>();
    let mut last = [0u8; 4];
    last[..rest.len()].copy_from_slice(rest);
    words
        .iter()
        .chain(std::iter::once(&last))
        .fold(0u32, |sum, w| sum.wrapping_add(u32::from_be_bytes(*w)))
}

/// Writes a font file from `tables`, whose data goes into the file in the
/// order given, each table starting on a 4-byte boundary. The table
/// directory lists the tables sorted by tag, as the OpenType specification
/// requires, with each table's checksum. When there is a `head` table, its
/// `checkSumAdjustment` is set so that the whole file sums to 0xB1B0AFBA.
pub fn assemble(version: u32, tables: &[([u8; 4], &[u8])]) -> Vec<u8> {
    let count = tables.len();
    let count16 = u16::try_from(count).unwrap_or(u16::MAX);
    let mut power = 1u16;
    let mut selector = 0u16;
    while power.saturating_mul(2) <= count16 {
        power = power.saturating_mul(2);
        selector = selector.saturating_add(1);
    }
    let search_range = power.saturating_mul(16);
    let range_shift = count16.saturating_mul(16).saturating_sub(search_range);
    let mut out = Vec::new();
    out.extend(version.to_be_bytes());
    for v in [count16, search_range, selector, range_shift] {
        out.extend(v.to_be_bytes());
    }
    let directory = out.len();
    out.resize(directory.saturating_add(count.saturating_mul(16)), 0);
    let mut records = Vec::with_capacity(count);
    let mut head_at = None;
    for (tag, data) in tables {
        let offset = out.len();
        let mut data = data.to_vec();
        if tag == b"head" && data.len() >= 12 {
            data[8..12].fill(0);
            head_at = Some(offset);
        }
        records.push((*tag, checksum(&data), offset, data.len()));
        out.extend(&data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    records.sort_by_key(|r| r.0);
    for (i, (tag, sum, offset, len)) in records.into_iter().enumerate() {
        let at = directory.saturating_add(i.saturating_mul(16));
        let mut record = Vec::with_capacity(16);
        record.extend(tag);
        record.extend(sum.to_be_bytes());
        record.extend(u32::try_from(offset).unwrap_or(u32::MAX).to_be_bytes());
        record.extend(u32::try_from(len).unwrap_or(u32::MAX).to_be_bytes());
        out[at..at.saturating_add(16)].copy_from_slice(&record);
    }
    if let Some(at) = head_at {
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[at.saturating_add(8)..at.saturating_add(12)].copy_from_slice(&adjustment.to_be_bytes());
    }
    out
}

/// A format 4 subtable: segments of consecutive characters, each mapped by
/// a delta or through the glyph index array.
fn format4(sub: &[u8], chars: &mut BTreeSet<u32>) -> Result<(), String> {
    let segs = usize::from(u16_at(sub, 6)? / 2);
    let ends = 14usize;
    let starts = ends
        .saturating_add(segs.saturating_mul(2))
        .saturating_add(2);
    let deltas = starts.saturating_add(segs.saturating_mul(2));
    let ranges = deltas.saturating_add(segs.saturating_mul(2));
    for s in 0..segs {
        let two_s = s.saturating_mul(2);
        let end = u16_at(sub, ends.saturating_add(two_s))?;
        let start = u16_at(sub, starts.saturating_add(two_s))?;
        let delta = u16_at(sub, deltas.saturating_add(two_s))?;
        let range_at = ranges.saturating_add(two_s);
        let range = usize::from(u16_at(sub, range_at)?);
        for c in start..=end {
            if c == 0xFFFF {
                break;
            }
            let glyph = if range == 0 {
                c.wrapping_add(delta)
            } else {
                let index = range_at
                    .saturating_add(range)
                    .saturating_add(usize::from(c.wrapping_sub(start)).saturating_mul(2));
                match u16_at(sub, index)? {
                    0 => 0,
                    g => g.wrapping_add(delta),
                }
            };
            if glyph != 0 {
                chars.insert(u32::from(c));
            }
        }
    }
    Ok(())
}

/// A format 12 subtable: groups of consecutive characters mapped to
/// consecutive glyphs.
fn format12(sub: &[u8], chars: &mut BTreeSet<u32>) -> Result<(), String> {
    let groups = usize_of(u32_at(sub, 12)?);
    for g in 0..groups {
        let at = 16usize.saturating_add(g.saturating_mul(12));
        let start = u32_at(sub, at)?;
        let end = u32_at(sub, at.saturating_add(4))?;
        let first_glyph = u32_at(sub, at.saturating_add(8))?;
        if end < start || end > 0x10_FFFF {
            return Err(format!("a cmap group runs from {start:#x} to {end:#x}"));
        }
        for c in start..=end {
            if first_glyph != 0 || c != start {
                chars.insert(c);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
pub mod tests {
    use super::*;

    /// A font of two tables, written the way the format lays them out: a
    /// `name` table with the given (platform, name ID, text) records, and a
    /// `cmap` table with one format 4 subtable (Windows, Unicode BMP) and
    /// one format 12 subtable (Windows, full Unicode) mapping `chars`.
    pub fn font(names: &[(u16, u16, &str)], chars: &[u32]) -> Vec<u8> {
        font_with(names, chars, &[])
    }

    /// The font that [`font`] writes, with the tables in `extra` added.
    pub fn font_with(
        names: &[(u16, u16, &str)],
        chars: &[u32],
        extra: &[([u8; 4], &[u8])],
    ) -> Vec<u8> {
        let name = name_table(names);
        let cmap = cmap_table(chars);
        let mut tables: Vec<([u8; 4], &[u8])> = vec![(*b"cmap", &cmap), (*b"name", &name)];
        tables.extend_from_slice(extra);
        assemble(0x0001_0000, &tables)
    }

    pub fn name_table(names: &[(u16, u16, &str)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut records = Vec::new();
        for &(platform, id, text) in names {
            let raw: Vec<u8> = if platform == 1 {
                text.bytes().collect()
            } else {
                text.encode_utf16().flat_map(u16::to_be_bytes).collect()
            };
            for v in [platform, if platform == 1 { 0 } else { 1 }, 0, id] {
                records.extend(v.to_be_bytes());
            }
            records.extend(u16::try_from(raw.len()).unwrap().to_be_bytes());
            records.extend(u16::try_from(strings.len()).unwrap().to_be_bytes());
            strings.extend(raw);
        }
        let mut t = Vec::new();
        t.extend(0u16.to_be_bytes());
        t.extend(u16::try_from(names.len()).unwrap().to_be_bytes());
        t.extend(u16::try_from(6 + records.len()).unwrap().to_be_bytes());
        t.extend(records);
        t.extend(strings);
        t
    }

    fn cmap_table(chars: &[u32]) -> Vec<u8> {
        // format 4: one segment for each of the first thousand BMP
        // characters, mapped by delta to glyph 1, 2, ..., then the closing
        // 0xFFFF segment; a format 4 subtable holds at most 64 KiB, and the
        // format 12 subtable maps every character
        let bmp: Vec<u16> = chars
            .iter()
            .filter_map(|&c| u16::try_from(c).ok())
            .take(1000)
            .collect();
        let segs = bmp.len() + 1;
        let mut f4 = Vec::new();
        let mut ends = Vec::new();
        let mut starts = Vec::new();
        let mut deltas = Vec::new();
        for (i, &c) in bmp.iter().enumerate() {
            ends.push(c);
            starts.push(c);
            deltas.push(u16::try_from(i + 1).unwrap().wrapping_sub(c));
        }
        ends.push(0xFFFF);
        starts.push(0xFFFF);
        deltas.push(1);
        let length = 16 + 8 * segs;
        for v in [
            4u16,
            u16::try_from(length).unwrap(),
            0,
            u16::try_from(2 * segs).unwrap(),
            0,
            0,
            0,
        ] {
            f4.extend(v.to_be_bytes());
        }
        for v in &ends {
            f4.extend(v.to_be_bytes());
        }
        f4.extend(0u16.to_be_bytes());
        for v in starts.iter().chain(&deltas) {
            f4.extend(v.to_be_bytes());
        }
        for _ in 0..segs {
            f4.extend(0u16.to_be_bytes());
        }
        // format 12: one group per character
        let mut f12 = Vec::new();
        f12.extend(12u16.to_be_bytes());
        f12.extend(0u16.to_be_bytes());
        f12.extend(u32::try_from(16 + 12 * chars.len()).unwrap().to_be_bytes());
        f12.extend(0u32.to_be_bytes());
        f12.extend(u32::try_from(chars.len()).unwrap().to_be_bytes());
        for (i, &c) in chars.iter().enumerate() {
            f12.extend(c.to_be_bytes());
            f12.extend(c.to_be_bytes());
            f12.extend(u32::try_from(i + 1).unwrap().to_be_bytes());
        }
        let mut t = Vec::new();
        t.extend(0u16.to_be_bytes());
        t.extend(2u16.to_be_bytes());
        let first = 4 + 2 * 8;
        for (enc, at) in [(1u16, first), (10, first + f4.len())] {
            t.extend(3u16.to_be_bytes());
            t.extend(enc.to_be_bytes());
            t.extend(u32::try_from(at).unwrap().to_be_bytes());
        }
        t.extend(f4);
        t.extend(f12);
        t
    }

    #[test]
    fn names_decode_on_both_platforms() {
        let bytes = font(&[(1, 1, "Cullet Mono"), (3, 2, "Bold")], &[]);
        let names = Font::parse(&bytes).unwrap().names().unwrap();
        assert_eq!(names.len(), 2);
        assert_eq!(
            (names[0].platform, names[0].name_id, names[0].text.as_str()),
            (1, 1, "Cullet Mono")
        );
        assert_eq!(
            (names[1].platform, names[1].name_id, names[1].text.as_str()),
            (3, 2, "Bold")
        );
    }

    #[test]
    fn characters_come_from_both_subtables() {
        let bytes = font(&[], &[0x41, 0x42, 0xF0001]);
        let chars = Font::parse(&bytes).unwrap().characters().unwrap();
        assert_eq!(chars.into_iter().collect::<Vec<_>>(), [0x41, 0x42, 0xF0001]);
    }

    #[test]
    fn each_subtable_format_reads_on_its_own() {
        let bytes = font(&[], &[0x41, 0x42, 0xF0001]);
        let cmap = Font::parse(&bytes).unwrap().table(b"cmap").unwrap();
        let at = |record: usize| usize_of(u32_at(cmap, 4 + record * 8 + 4).unwrap());
        let mut bmp = BTreeSet::new();
        format4(&cmap[at(0)..], &mut bmp).unwrap();
        assert_eq!(bmp.into_iter().collect::<Vec<_>>(), [0x41, 0x42]);
        let mut all = BTreeSet::new();
        format12(&cmap[at(1)..], &mut all).unwrap();
        assert_eq!(all.into_iter().collect::<Vec<_>>(), [0x41, 0x42, 0xF0001]);
    }

    #[test]
    fn a_written_font_follows_the_format() {
        let head = [9u8; 54];
        let tables: [([u8; 4], &[u8]); 5] = [
            (*b"head", &head),
            (*b"cmap", b"cmap data"),
            (*b"BDF ", b"bdf"),
            (*b"name", b"name data!"),
            (*b"OS/2", b"os2"),
        ];
        let bytes = assemble(0x0001_0000, &tables);
        assert_eq!(
            checksum(&bytes),
            0xB1B0_AFBA,
            "the whole file sums to the magic number"
        );
        // five tables: searchRange 64, entrySelector 2, rangeShift 16
        assert_eq!(bytes[4..12], [0, 5, 0, 64, 0, 2, 0, 16]);
        let tags: Vec<&[u8]> = (0..5).map(|i| &bytes[12 + i * 16..16 + i * 16]).collect();
        assert_eq!(tags, [b"BDF ", b"OS/2", b"cmap", b"head", b"name"]);
        let font = Font::parse(&bytes).unwrap();
        for (i, (tag, data)) in tables.iter().enumerate().skip(1) {
            let at = 12 + 16 * tags.iter().position(|t| t == tag).unwrap();
            assert_eq!(font.table(tag).unwrap(), *data);
            assert_eq!(u32_at(&bytes, at + 4).unwrap(), checksum(data), "table {i}");
            assert_eq!(u32_at(&bytes, at + 8).unwrap() % 4, 0);
        }
        assert_eq!(font.file_order, tables.map(|t| t.0));
        assert_eq!(
            font.rewrite(&[]),
            bytes,
            "a font written again unchanged keeps its bytes"
        );
        let renamed = font.rewrite(&[(*b"name", b"other names")]);
        let again = Font::parse(&renamed).unwrap();
        assert_eq!(again.table(b"name").unwrap(), b"other names");
        assert_eq!(again.table(b"cmap").unwrap(), b"cmap data");
        assert_eq!(checksum(&renamed), 0xB1B0_AFBA);
    }

    #[test]
    fn a_truncated_font_is_an_error() {
        let bytes = font(&[(3, 1, "Cullet Mono")], &[0x41]);
        for len in [0, 3, 11, 20, bytes.len() - 1] {
            let cut = &bytes[..len];
            let ok = Font::parse(cut)
                .and_then(|f| f.names().map(|_| ()).and(f.characters().map(|_| ())));
            assert!(ok.is_err(), "a font cut to {len} bytes reads");
        }
    }
}
