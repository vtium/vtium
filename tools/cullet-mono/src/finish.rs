//! The step that finishes each face after the patcher: it writes the
//! description that credits Terminus into the `name` table, and renames the
//! family of the bitmap strikes in FontForge's `BDF ` table.
//!
//! The patcher renames the font in the `name` table, but FontForge copies
//! each bitmap strike's BDF properties from the source font unchanged. So
//! the strikes keep the family "Terminus", as FAMILY_NAME and inside each
//! strike's XLFD name, such as `-xos4-Terminus-Medium-R-Normal--12-...`.
//! FreeType hands these properties to programs through
//! `FT_Get_BDF_Property`. This step gives the strikes the font's own family
//! name, "Cullet Mono", so that every name the font gives itself agrees.
//! It keeps the table and every other property, because the table also
//! tells programs each strike's ascent, descent, spacing and default
//! character. It writes every other table byte for byte as the patcher
//! wrote it.
//!
//! References:
//! - ADR-1204 D2, its note of 2026-11-15: the bare word "Terminus" is
//!   reserved as the font's name, and the attribution states the source.

use crate::recipe;
use crate::sfnt::Font;

/// Finishes the face in `bytes`, as the module's comment describes.
pub fn face(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let font = Font::parse(bytes)?;
    let name = font.table(b"name").ok_or("the font has no name table")?;
    let name = with_description(name, recipe::DESCRIPTION)?;
    let mut replace: Vec<([u8; 4], &[u8])> = vec![(*b"name", &name)];
    let bdf = match font.table(b"BDF ") {
        Some(table) => Some(rename_strikes(table, recipe::FAMILY)?),
        None => None,
    };
    if let Some(bdf) = &bdf {
        replace.push((*b"BDF ", bdf));
    }
    Ok(font.rewrite(&replace))
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    match bytes.get(at..at.saturating_add(2)) {
        Some(&[a, b]) => Ok(u16::from_be_bytes([a, b])),
        _ => Err(format!("a table ends before offset {at}")),
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    match bytes.get(at..at.saturating_add(4)) {
        Some(&[a, b, c, d]) => Ok(u32::from_be_bytes([a, b, c, d])),
        _ => Err(format!("a table ends before offset {at}")),
    }
}

/// One record of a format 0 `name` table, with its string's bytes.
struct Record {
    platform: u16,
    encoding: u16,
    language: u16,
    name_id: u16,
    raw: Vec<u8>,
}

/// The `name` table `table` with its description (name ID 10) set to
/// `text`. The description goes into each platform, encoding and language
/// that has a family name (name ID 1), and replaces any description there
/// was. The records are sorted the way the OpenType specification asks:
/// by platform, encoding, language and name ID.
fn with_description(table: &[u8], text: &str) -> Result<Vec<u8>, String> {
    if u16_at(table, 0)? != 0 {
        return Err("the name table is not of format 0".into());
    }
    let count = usize::from(u16_at(table, 2)?);
    let strings = usize::from(u16_at(table, 4)?);
    let mut records = Vec::with_capacity(count);
    for i in 0..count {
        let at = 6usize.saturating_add(i.saturating_mul(12));
        let field = |n: usize| u16_at(table, at.saturating_add(n.saturating_mul(2)));
        let length = usize::from(field(4)?);
        let offset = usize::from(field(5)?);
        let start = strings.saturating_add(offset);
        let raw = table
            .get(start..start.saturating_add(length))
            .ok_or("a name lies past the end of the name table")?;
        records.push(Record {
            platform: field(0)?,
            encoding: field(1)?,
            language: field(2)?,
            name_id: field(3)?,
            raw: raw.to_vec(),
        });
    }
    let mut families: Vec<(u16, u16, u16)> = records
        .iter()
        .filter(|r| r.name_id == 1)
        .map(|r| (r.platform, r.encoding, r.language))
        .collect();
    families.sort();
    families.dedup();
    records.retain(|r| r.name_id != 10);
    for (platform, encoding, language) in families {
        let raw = if platform == 1 {
            if !text.is_ascii() {
                return Err(
                    "the description is not ASCII, so a Macintosh name cannot hold it".into(),
                );
            }
            text.as_bytes().to_vec()
        } else {
            text.encode_utf16().flat_map(u16::to_be_bytes).collect()
        };
        records.push(Record {
            platform,
            encoding,
            language,
            name_id: 10,
            raw,
        });
    }
    records.sort_by_key(|r| (r.platform, r.encoding, r.language, r.name_id));
    let too_big = || "the name table grows past 64 KiB".to_owned();
    let count = u16::try_from(records.len()).map_err(|_| too_big())?;
    let strings =
        u16::try_from(records.len().saturating_mul(12).saturating_add(6)).map_err(|_| too_big())?;
    let mut head = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    for v in [0, count, strings] {
        head.extend(v.to_be_bytes());
    }
    for r in &records {
        let length = u16::try_from(r.raw.len()).map_err(|_| too_big())?;
        let offset = u16::try_from(data.len()).map_err(|_| too_big())?;
        for v in [
            r.platform, r.encoding, r.language, r.name_id, length, offset,
        ] {
            head.extend(v.to_be_bytes());
        }
        data.extend(&r.raw);
    }
    head.extend(data);
    Ok(head)
}

/// One property of a strike in a `BDF ` table: its name, its type, and its
/// value, a string or a number.
struct Property {
    name: Vec<u8>,
    kind: u16,
    value: Value,
}

enum Value {
    Text(Vec<u8>),
    Number(u32),
}

/// FontForge's `BDF ` table `table` with the family of each strike set to
/// `family`: the FAMILY_NAME property, and the family field of the FONT
/// property's XLFD name. Every other property keeps its value.
///
/// The table starts with its version (1), the number of strikes and the
/// offset of its string pool. Then each strike gives its size in pixels
/// and its number of properties, and the properties of all strikes follow
/// in strike order. Each property is the offset of its name in the pool,
/// its type, and its value. A type whose low four bits are 0 or 1 holds a
/// string, whose value is an offset in the pool; types 2 and 3 hold a
/// number. The pool holds strings ended by a zero byte.
fn rename_strikes(table: &[u8], family: &str) -> Result<Vec<u8>, String> {
    if u16_at(table, 0)? != 1 {
        return Err("the BDF table is not of version 1".into());
    }
    let strikes = usize::from(u16_at(table, 2)?);
    let pool_at = usize::try_from(u32_at(table, 4)?).unwrap_or(usize::MAX);
    let pool = table
        .get(pool_at..)
        .ok_or("the BDF table's strings lie past its end")?;
    let string = |offset: u32| -> Result<Vec<u8>, String> {
        let at = usize::try_from(offset).unwrap_or(usize::MAX);
        let rest = pool
            .get(at..)
            .ok_or("a BDF string lies past the table's end")?;
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or("a BDF string has no end")?;
        Ok(rest[..end].to_vec())
    };
    let mut heads = Vec::with_capacity(strikes);
    for i in 0..strikes {
        let at = 8usize.saturating_add(i.saturating_mul(4));
        heads.push((
            u16_at(table, at)?,
            usize::from(u16_at(table, at.saturating_add(2))?),
        ));
    }
    let mut at = 8usize.saturating_add(strikes.saturating_mul(4));
    let mut properties = Vec::new();
    for &(_, count) in &heads {
        for _ in 0..count {
            let name = string(u32_at(table, at)?)?;
            let kind = u16_at(table, at.saturating_add(4))?;
            let raw = u32_at(table, at.saturating_add(6))?;
            at = at.saturating_add(10);
            let mut value = if kind & 0x0F <= 1 {
                Value::Text(string(raw)?)
            } else {
                Value::Number(raw)
            };
            if let Value::Text(text) = &mut value {
                match name.as_slice() {
                    b"FAMILY_NAME" => *text = family.as_bytes().to_vec(),
                    b"FONT" => *text = xlfd_with_family(text, family)?,
                    _ => {}
                }
            }
            properties.push(Property { name, kind, value });
        }
    }
    if at != pool_at {
        return Err("the BDF table's properties do not end where its strings begin".into());
    }
    // the pool holds each string once, in the order the properties first
    // use it
    let mut pool: Vec<u8> = Vec::new();
    let mut offsets: Vec<(Vec<u8>, u32)> = Vec::new();
    let mut intern = |s: &[u8]| -> u32 {
        if let Some((_, o)) = offsets.iter().find(|(t, _)| t == s) {
            return *o;
        }
        let o = u32::try_from(pool.len()).unwrap_or(u32::MAX);
        pool.extend(s);
        pool.push(0);
        offsets.push((s.to_vec(), o));
        o
    };
    let mut out = Vec::new();
    out.extend(1u16.to_be_bytes());
    out.extend(u16_at(table, 2)?.to_be_bytes());
    out.extend(u32::try_from(pool_at).unwrap_or(u32::MAX).to_be_bytes());
    for (size, count) in heads {
        out.extend(size.to_be_bytes());
        out.extend(u16::try_from(count).unwrap_or(u16::MAX).to_be_bytes());
    }
    for p in properties {
        out.extend(intern(&p.name).to_be_bytes());
        out.extend(p.kind.to_be_bytes());
        let value = match &p.value {
            Value::Text(text) => intern(text),
            Value::Number(n) => *n,
        };
        out.extend(value.to_be_bytes());
    }
    out.extend(pool);
    Ok(out)
}

/// The XLFD name `xlfd` with its family field, the second of its fourteen
/// fields, set to `family`.
fn xlfd_with_family(xlfd: &[u8], family: &str) -> Result<Vec<u8>, String> {
    let fields: Vec<&[u8]> = xlfd.split(|&b| b == b'-').collect();
    if fields.len() != 15 || !fields[0].is_empty() {
        return Err(format!(
            "the BDF table's FONT property {:?} is not an XLFD name of fourteen fields",
            String::from_utf8_lossy(xlfd)
        ));
    }
    let mut out = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            out.push(b'-');
        }
        out.extend(if i == 2 { family.as_bytes() } else { field });
    }
    Ok(out)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::sfnt::tests::{font_with, name_table};

    /// A property for [`bdf_table`]: its name, its type, and its value, a
    /// string or a number.
    type TestProperty = (&'static str, u16, Result<String, u32>);

    /// A `BDF ` table of two strikes in the layout FontForge writes, with
    /// each string once in its pool.
    fn bdf_table(family: &str) -> Vec<u8> {
        let strikes: [(u16, Vec<TestProperty>); 2] = [
            (
                12,
                vec![
                    (
                        "FONT",
                        1,
                        Ok(format!(
                            "-xos4-{family}-Medium-R-Normal--12-120-72-72-C-60-ISO10646-1"
                        )),
                    ),
                    ("FAMILY_NAME", 16, Ok(family.to_owned())),
                    (
                        "COPYRIGHT",
                        16,
                        Ok("Copyright (C) 2020 Dimitar Toshkov Zhekov".into()),
                    ),
                    ("PIXEL_SIZE", 18, Err(12)),
                ],
            ),
            (
                32,
                vec![
                    (
                        "FONT",
                        1,
                        Ok(format!(
                            "-xos4-{family}-Medium-R-Normal--32-320-72-72-C-160-ISO10646-1"
                        )),
                    ),
                    ("FAMILY_NAME", 16, Ok(family.to_owned())),
                    ("DEFAULT_CHAR", 19, Err(65533)),
                ],
            ),
        ];
        let mut pool = Vec::new();
        let mut seen: Vec<(String, u32)> = Vec::new();
        let mut add = |s: &str| {
            if let Some((_, o)) = seen.iter().find(|(t, _)| t == s) {
                return *o;
            }
            let o = u32::try_from(pool.len()).unwrap();
            pool.extend(s.as_bytes());
            pool.push(0);
            seen.push((s.to_owned(), o));
            o
        };
        let mut props = Vec::new();
        for (_, list) in &strikes {
            for (name, kind, value) in list {
                props.extend(add(name).to_be_bytes());
                props.extend(kind.to_be_bytes());
                let v = match value {
                    Ok(s) => add(s),
                    Err(n) => *n,
                };
                props.extend(v.to_be_bytes());
            }
        }
        let mut t = Vec::new();
        t.extend(1u16.to_be_bytes());
        t.extend(2u16.to_be_bytes());
        t.extend(
            u32::try_from(8 + 4 * strikes.len() + props.len())
                .unwrap()
                .to_be_bytes(),
        );
        for (size, list) in &strikes {
            t.extend(size.to_be_bytes());
            t.extend(u16::try_from(list.len()).unwrap().to_be_bytes());
        }
        t.extend(props);
        t.extend(pool);
        t
    }

    #[test]
    fn the_strikes_take_the_font_s_family() {
        let renamed = rename_strikes(&bdf_table("Terminus"), "Cullet Mono").unwrap();
        assert_eq!(renamed, bdf_table("Cullet Mono"));
    }

    #[test]
    fn a_strike_name_that_is_no_xlfd_name_is_refused() {
        assert!(xlfd_with_family(b"Terminus-12", "Cullet Mono").is_err());
        assert_eq!(
            xlfd_with_family(
                b"-xos4-Terminus-Bold-R-Normal--12-120-72-72-C-60-ISO10646-1",
                "Cullet Mono"
            )
            .unwrap(),
            b"-xos4-Cullet Mono-Bold-R-Normal--12-120-72-72-C-60-ISO10646-1"
        );
    }

    #[test]
    fn the_description_goes_beside_each_family_name() {
        let before = name_table(&[
            (3, 0, recipe::COPYRIGHT),
            (3, 1, "Cullet Mono"),
            (3, 10, "an older description"),
            (1, 1, "Cullet Mono"),
            (1, 2, "Regular"),
        ]);
        let after = with_description(&before, "Derived from Terminus.").unwrap();
        let expected = name_table(&[
            (1, 1, "Cullet Mono"),
            (1, 2, "Regular"),
            (1, 10, "Derived from Terminus."),
            (3, 0, recipe::COPYRIGHT),
            (3, 1, "Cullet Mono"),
            (3, 10, "Derived from Terminus."),
        ]);
        assert_eq!(after, expected);
    }

    #[test]
    fn a_finished_face_keeps_every_other_table() {
        let names = [
            (3, 0, recipe::COPYRIGHT),
            (3, 1, recipe::FAMILY),
            (1, 1, recipe::FAMILY),
        ];
        let head = [7u8; 54];
        let before = font_with(
            &names,
            &[0x41],
            &[(*b"BDF ", &bdf_table("Terminus")), (*b"head", &head)],
        );
        let after = face(&before).unwrap();
        let (old, new) = (Font::parse(&before).unwrap(), Font::parse(&after).unwrap());
        for tag in [b"cmap", b"head"] {
            let (mut a, mut b) = (
                old.table(tag).unwrap().to_vec(),
                new.table(tag).unwrap().to_vec(),
            );
            if tag == b"head" {
                // checkSumAdjustment covers the whole file, so it moves
                a[8..12].fill(0);
                b[8..12].fill(0);
            }
            assert_eq!(a, b, "{}", String::from_utf8_lossy(tag));
        }
        assert_eq!(new.table(b"BDF ").unwrap(), bdf_table("Cullet Mono"));
        let described: Vec<_> = new
            .names()
            .unwrap()
            .into_iter()
            .filter(|n| n.name_id == 10)
            .map(|n| (n.platform, n.text))
            .collect();
        assert_eq!(
            described,
            [
                (1, recipe::DESCRIPTION.to_owned()),
                (3, recipe::DESCRIPTION.to_owned())
            ]
        );
        assert_eq!(
            face(&after).unwrap(),
            after,
            "finishing a finished face changes it"
        );
    }
}
