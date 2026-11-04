//! A font file's family names and style, read from its name table alone,
//! so that `font:FAMILY` finds an installed outline font by the family the
//! font stores, whatever the file is called (ADR-1500 D8). Only the file's
//! table directory and its `name` table are read, each with a bound, and
//! read-fonts, the table reader under swash, parses the table.

use std::fs::File;
use std::os::unix::fs::FileExt;

use skrifa::raw::tables::name::Name;
use skrifa::raw::types::NameId;
use skrifa::raw::{FontData, FontRead};

/// The most faces of a collection that are read.
pub const FACES_MAX: u32 = 256;
/// The most bytes a `name` table may take; a face whose table is larger
/// is passed over.
pub const NAME_BYTES_MAX: u32 = 1 << 20;

/// One face of a font file, as its name table gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    /// Every family the face names: its typographic families (name ID 16)
    /// and its legacy families (name ID 1), in every language the table
    /// holds.
    pub families: Vec<String>,
    /// Whether its style is Regular: its typographic subfamily (name ID
    /// 17) where it has one, else its subfamily (name ID 2).
    pub regular: bool,
}

fn read_at(file: &File, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0; len];
    file.read_exact_at(&mut buf, offset)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// The faces of the TrueType or OpenType font, or collection, in `file`:
/// one for a font, each of up to [`FACES_MAX`] for a collection. A face
/// with no `name` table, or one past [`NAME_BYTES_MAX`] or malformed, is
/// left out.
pub fn faces(file: &File) -> Result<Vec<Face>, String> {
    let head = read_at(file, 0, 12)?;
    let offsets: Vec<u32> = if head.starts_with(b"ttcf") {
        let count = u32_at(&head, 8).unwrap_or(0).min(FACES_MAX);
        let table = read_at(file, 12, count as usize * 4)?;
        table
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_be_bytes(*c))
            .collect()
    } else {
        vec![0]
    };
    let mut out = Vec::new();
    for offset in offsets {
        if let Some(face) = face(file, u64::from(offset))? {
            out.push(face);
        }
    }
    Ok(out)
}

/// The face whose table directory starts at `offset`, or `None` when it
/// has no readable `name` table.
fn face(file: &File, offset: u64) -> Result<Option<Face>, String> {
    let dir = read_at(file, offset, 12)?;
    let tables = usize::from(u16_at(&dir, 4).unwrap_or(0));
    let records = read_at(file, offset + 12, tables * 16)?;
    let Some((at, len)) = records.as_chunks::<16>().0.iter().find_map(|r| {
        r.starts_with(b"name")
            .then(|| (u32_at(r, 8), u32_at(r, 12)))
    }) else {
        return Ok(None);
    };
    let (Some(at), Some(len)) = (at, len) else {
        return Ok(None);
    };
    if len > NAME_BYTES_MAX {
        return Ok(None);
    }
    let Ok(bytes) = read_at(file, u64::from(at), len as usize) else {
        return Ok(None);
    };
    let Ok(name) = Name::read(FontData::new(&bytes)) else {
        return Ok(None);
    };
    let strings = |id: NameId| -> Vec<String> {
        name.name_record()
            .iter()
            .filter(|r| r.name_id() == id)
            .filter_map(|r| r.string(name.string_data()).ok())
            .map(|s| s.chars().collect::<String>())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let mut families = strings(NameId::TYPOGRAPHIC_FAMILY_NAME);
    families.extend(strings(NameId::FAMILY_NAME));
    families.dedup();
    let mut style = strings(NameId::TYPOGRAPHIC_SUBFAMILY_NAME);
    if style.is_empty() {
        style = strings(NameId::SUBFAMILY_NAME);
    }
    let regular = style
        .iter()
        .any(|s| s.trim().eq_ignore_ascii_case("regular"));
    Ok(Some(Face { families, regular }))
}

/// A font file for the tests: one face, or with several a collection,
/// each face a `name` table alone holding `(name ID, string)` records in
/// UTF-16 for Windows, English.
#[cfg(test)]
pub(crate) fn sfnt(faces: &[&[(u16, &str)]]) -> Vec<u8> {
    fn name_table(records: &[(u16, &str)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut out = Vec::new();
        out.extend(0u16.to_be_bytes());
        out.extend((records.len() as u16).to_be_bytes());
        out.extend((6 + 12 * records.len() as u16).to_be_bytes());
        for (id, text) in records {
            let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
            for v in [
                3u16,
                1,
                0x409,
                *id,
                utf16.len() as u16,
                strings.len() as u16,
            ] {
                out.extend(v.to_be_bytes());
            }
            strings.extend(utf16);
        }
        out.extend(strings);
        out
    }
    let collection = faces.len() > 1;
    let head = if collection { 12 + 4 * faces.len() } else { 0 };
    // each face: its 12-byte directory and one record, then its table
    let mut bodies = Vec::new();
    let mut at = head;
    for records in faces {
        let table = name_table(records);
        let mut body = Vec::new();
        body.extend(0x0001_0000u32.to_be_bytes());
        body.extend([0u8, 1, 0, 16, 0, 0, 0, 0]);
        body.extend(*b"name");
        body.extend(0u32.to_be_bytes());
        body.extend(((at + 28) as u32).to_be_bytes());
        body.extend((table.len() as u32).to_be_bytes());
        body.extend(table);
        at += body.len();
        bodies.push(body);
    }
    let mut out = Vec::new();
    if collection {
        out.extend(*b"ttcf");
        out.extend(0x0001_0000u32.to_be_bytes());
        out.extend((faces.len() as u32).to_be_bytes());
        let mut offset = head;
        for body in &bodies {
            out.extend((offset as u32).to_be_bytes());
            offset += body.len();
        }
    }
    out.extend(bodies.concat());
    out
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn faces_of(bytes: &[u8], test: &str) -> Result<Vec<Face>, String> {
        let dir =
            std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/tmp"))
                .join("vtium-config-names");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(test);
        std::fs::write(&path, bytes).unwrap();
        faces(&File::open(&path).unwrap())
    }

    fn face(families: &[&str], regular: bool) -> Face {
        Face {
            families: families.iter().map(|s| (*s).to_owned()).collect(),
            regular,
        }
    }

    // ADR-1500 D8: the typographic family and style come first, so a heavy
    // face of JetBrains Mono names the family JetBrains Mono and is not
    // Regular, though its legacy family and subfamily say otherwise.
    #[test]
    fn a_face_gives_its_typographic_and_legacy_families_and_its_style() {
        let extrabold: &[(u16, &str)] = &[
            (1, "JetBrains Mono ExtraBold"),
            (2, "Regular"),
            (16, "JetBrains Mono"),
            (17, "ExtraBold"),
        ];
        assert_eq!(
            faces_of(&sfnt(&[extrabold]), "extrabold").unwrap(),
            [face(&["JetBrains Mono", "JetBrains Mono ExtraBold"], false)]
        );
        let regular: &[(u16, &str)] = &[(1, "JetBrains Mono"), (2, "Regular")];
        assert_eq!(
            faces_of(&sfnt(&[regular]), "regular").unwrap(),
            [face(&["JetBrains Mono"], true)]
        );
    }

    #[test]
    fn a_collection_gives_each_of_its_faces() {
        let a: &[(u16, &str)] = &[(1, "Alpha"), (2, "Bold")];
        let b: &[(u16, &str)] = &[(1, "Beta"), (2, "Regular")];
        assert_eq!(
            faces_of(&sfnt(&[a, b]), "collection").unwrap(),
            [face(&["Alpha"], false), face(&["Beta"], true)]
        );
    }

    // A file that is not a font, or is cut short, gives an error or no
    // face, and never a panic.
    #[test]
    fn a_file_that_is_no_font_gives_no_face() {
        assert!(faces_of(b"x", "short").is_err());
        let font = sfnt(&[&[(1, "Alpha")]]);
        for cut in [12, 20, 28, 40, font.len() - 1] {
            let got = faces_of(font.get(..cut).unwrap(), &format!("cut-{cut}"));
            assert!(got.as_ref().map_or(true, Vec::is_empty), "{cut}: {got:?}");
        }
        assert_eq!(faces_of(&[0u8; 64], "zeros").unwrap(), []);
    }
}
