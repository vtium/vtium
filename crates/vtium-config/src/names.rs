//! A font file's family names and style, read from its name table alone,
//! so that `font:FAMILY` finds an installed outline font by the family the
//! font stores, whatever the file is called (ADR-1500 D8). Only the file's
//! table directory and its `name` table are read, each with a bound, and
//! read-fonts, the table reader under swash, parses the table. The search
//! among installed fonts and the rasteriser's choice of a face both decide
//! with [`faces`] and [`matches`], so a face the search finds is a face the
//! rasteriser takes. The fuzz target `names` feeds [`faces`] any bytes
//! (`fuzz/README.md`).

use std::fs::File;
use std::os::unix::fs::FileExt;

use skrifa::raw::tables::name::Name;
use skrifa::raw::types::NameId;
use skrifa::raw::{FontData, FontRead};

/// The most faces of a collection that are read.
pub const FACES_MAX: u32 = 256;
/// The most tables a face's directory may list; a face that lists more is
/// passed over. Real fonts list fewer than 40.
pub const TABLES_MAX: usize = 64;
/// The most bytes a `name` table may take; a face whose table is larger
/// is passed over.
pub const NAME_BYTES_MAX: u32 = 1 << 20;

/// One face of a font file, as its name table gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    /// The face's place in its collection, 0 for a font that is no
    /// collection: the index swash's `FontDataRef::get` takes.
    pub index: u32,
    /// Every family the face names, once each: its typographic families
    /// (name ID 16), then its legacy families (name ID 1), each in every
    /// language the table holds, English first.
    pub families: Vec<String>,
    /// Whether its style is Regular: its typographic subfamily (name ID
    /// 17) where it has one, else its subfamily (name ID 2).
    pub regular: bool,
}

/// Where a font's bytes are read from, a piece at a time: an open font
/// file, or the bytes themselves, as the tests and the fuzz target hold
/// them.
pub trait Source {
    /// Fills `buf` from `offset`, or fails when the source ends first.
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()>;
}

impl Source for File {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
        FileExt::read_exact_at(self, buf, offset)
    }
}

impl Source for [u8] {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
        let piece = usize::try_from(offset)
            .ok()
            .and_then(|start| self.get(start..start.checked_add(buf.len())?))
            .ok_or(std::io::ErrorKind::UnexpectedEof)?;
        buf.copy_from_slice(piece);
        Ok(())
    }
}

fn read_at<S: Source + ?Sized>(src: &S, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0; len];
    src.read_exact_at(&mut buf, offset)
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

/// Whether `face` is of `family`: one of its families is `family` with
/// case, blanks, `-` and `_` ignored, as fontconfig compares families.
pub fn matches(face: &Face, family: &str) -> bool {
    let key = crate::outline::family_key(family);
    face.families
        .iter()
        .any(|f| crate::outline::family_key(f) == key)
}

/// The face of `family` among `faces` that the rasteriser draws from: the
/// first whose subfamily is Regular, as the search prefers a file for its
/// Regular face, else the first of the family. In a collection whose
/// first face is Thin or Bold, this draws its Regular face.
pub fn choose<'a>(faces: &'a [Face], family: &str) -> Option<&'a Face> {
    faces
        .iter()
        .find(|f| f.regular && matches(f, family))
        .or_else(|| faces.iter().find(|f| matches(f, family)))
}

/// The faces of the TrueType or OpenType font, or collection, in `src`:
/// one for a font, each of up to [`FACES_MAX`] for a collection, a face
/// that a collection lists twice read once. A face with no `name` table,
/// one listing more than [`TABLES_MAX`] tables, one whose `name` table
/// passes [`NAME_BYTES_MAX`], and one cut short or malformed is left out,
/// and the other faces of its collection are still read. Only a file too
/// short for its header, or for a collection's list of faces, is an error.
pub fn faces<S: Source + ?Sized>(src: &S) -> Result<Vec<Face>, String> {
    let head = read_at(src, 0, 12)?;
    let offsets: Vec<u32> = if head.starts_with(b"ttcf") {
        let count = u32_at(&head, 8).unwrap_or(0).min(FACES_MAX);
        let table = read_at(src, 12, count as usize * 4)?;
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
    let mut seen = std::collections::BTreeSet::new();
    // a collection's faces often share one `name` table: each distinct
    // table is read and parsed once
    let mut tables: std::collections::BTreeMap<(u32, u32), Option<Names>> =
        std::collections::BTreeMap::new();
    for (index, offset) in (0u32..).zip(offsets) {
        if !seen.insert(offset) {
            continue;
        }
        let Some(place) = name_table(src, u64::from(offset)) else {
            continue;
        };
        let names = tables.entry(place).or_insert_with(|| names_in(src, place));
        if let Some((families, regular)) = names {
            out.push(Face {
                index,
                families: families.clone(),
                regular: *regular,
            });
        }
    }
    Ok(out)
}

/// What a `name` table gives a face: its families, and whether its style
/// is Regular.
type Names = (Vec<String>, bool);

/// Where the `name` table of the face whose table directory starts at
/// `offset` lies, its offset and length, or `None` when the face lists more
/// than [`TABLES_MAX`] tables, has no `name` table, or is cut short.
fn name_table<S: Source + ?Sized>(src: &S, offset: u64) -> Option<(u32, u32)> {
    let dir = read_at(src, offset, 12).ok()?;
    let tables = usize::from(u16_at(&dir, 4).unwrap_or(0));
    if tables > TABLES_MAX {
        return None;
    }
    let records = read_at(src, offset + 12, tables * 16).ok()?;
    let (at, len) = records.as_chunks::<16>().0.iter().find_map(|r| {
        r.starts_with(b"name")
            .then(|| (u32_at(r, 8), u32_at(r, 12)))
    })?;
    Some((at?, len?))
}

/// The families and whether the style is Regular that the `name` table at
/// `place` gives, or `None` when it passes [`NAME_BYTES_MAX`], is cut
/// short or is malformed.
fn names_in<S: Source + ?Sized>(src: &S, (at, len): (u32, u32)) -> Option<Names> {
    if len > NAME_BYTES_MAX {
        return None;
    }
    let bytes = read_at(src, u64::from(at), len as usize).ok()?;
    let name = Name::read(FontData::new(&bytes)).ok()?;
    // each string under `id`, English first: Windows' US English, or the
    // Macintosh's English
    let strings = |id: NameId| -> Vec<String> {
        let mut found: Vec<(bool, String)> = name
            .name_record()
            .iter()
            .filter(|r| r.name_id() == id)
            .filter_map(|r| {
                let english = matches!((r.platform_id(), r.language_id()), (3, 0x0409) | (1, 0));
                let text: String = r.string(name.string_data()).ok()?.chars().collect();
                (!text.is_empty()).then_some((!english, text))
            })
            .collect();
        found.sort_by_key(|(other, _)| *other);
        found.into_iter().map(|(_, text)| text).collect()
    };
    let mut families: Vec<String> = Vec::new();
    for f in strings(NameId::TYPOGRAPHIC_FAMILY_NAME)
        .into_iter()
        .chain(strings(NameId::FAMILY_NAME))
    {
        if !families.contains(&f) {
            families.push(f);
        }
    }
    let mut style = strings(NameId::TYPOGRAPHIC_SUBFAMILY_NAME);
    if style.is_empty() {
        style = strings(NameId::SUBFAMILY_NAME);
    }
    let regular = style
        .iter()
        .any(|s| s.trim().eq_ignore_ascii_case("regular"));
    Some((families, regular))
}

/// A font file for the tests: one face, or with several a collection,
/// each face a `name` table alone holding `(name ID, string)` records in
/// UTF-16 for Windows, US English.
#[cfg(test)]
pub(crate) fn sfnt(faces: &[&[(u16, &str)]]) -> Vec<u8> {
    let faces: Vec<Vec<(u16, u16, &str)>> = faces
        .iter()
        .map(|records| records.iter().map(|&(id, s)| (id, 0x0409, s)).collect())
        .collect();
    let faces: Vec<&[(u16, u16, &str)]> = faces.iter().map(Vec::as_slice).collect();
    sfnt_in(&faces)
}

/// [`sfnt`], each record `(name ID, Windows language ID, string)`.
#[cfg(test)]
pub(crate) fn sfnt_in(faces: &[&[(u16, u16, &str)]]) -> Vec<u8> {
    fn name_table(records: &[(u16, u16, &str)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut out = Vec::new();
        out.extend(0u16.to_be_bytes());
        out.extend((records.len() as u16).to_be_bytes());
        out.extend((6 + 12 * records.len() as u16).to_be_bytes());
        for (id, lang, text) in records {
            let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
            for v in [
                3u16,
                1,
                *lang,
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
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// The faces of `bytes`, read once as bytes and once as a file, which
    /// must agree. The file is in a directory of the test's own, named with
    /// the test process's id, so two test runs at once never write each
    /// other's file, and the directory is removed once the file is read.
    fn faces_of(bytes: &[u8], test: &str) -> Result<Vec<Face>, String> {
        let dir =
            std::env::temp_dir().join(format!("vtium-config-names-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(test);
        std::fs::write(&path, bytes).unwrap();
        let from_file = faces(&File::open(&path).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
        let from_bytes = faces(bytes);
        assert_eq!(from_file.is_ok(), from_bytes.is_ok(), "{test}");
        assert_eq!(from_file.as_ref().ok(), from_bytes.as_ref().ok(), "{test}");
        from_bytes
    }

    fn face(index: u32, families: &[&str], regular: bool) -> Face {
        Face {
            index,
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
            [face(
                0,
                &["JetBrains Mono", "JetBrains Mono ExtraBold"],
                false
            )]
        );
        let regular: &[(u16, &str)] = &[(1, "JetBrains Mono"), (2, "Regular")];
        assert_eq!(
            faces_of(&sfnt(&[regular]), "regular").unwrap(),
            [face(0, &["JetBrains Mono"], true)]
        );
    }

    #[test]
    fn a_collection_gives_each_of_its_faces() {
        let a: &[(u16, &str)] = &[(1, "Alpha"), (2, "Bold")];
        let b: &[(u16, &str)] = &[(1, "Beta"), (2, "Regular")];
        assert_eq!(
            faces_of(&sfnt(&[a, b]), "collection").unwrap(),
            [face(0, &["Alpha"], false), face(1, &["Beta"], true)]
        );
    }

    // A family in another language counts as one in English, and English
    // comes first, so the name a face is reported by is its English one.
    #[test]
    fn a_family_in_another_language_matches_and_english_comes_first() {
        let font = sfnt_in(&[&[
            (1, 0x0804, "文鼎测试楷"),
            (1, 0x0409, "AR Test Kai"),
            (2, 0x0409, "Regular"),
        ]]);
        let got = faces_of(&font, "localized").unwrap();
        assert_eq!(got, [face(0, &["AR Test Kai", "文鼎测试楷"], true)]);
        assert!(matches(&got[0], "文鼎测试楷"));
        assert!(matches(&got[0], "ar-test kai"));
        assert!(!matches(&got[0], "AR Test"));
    }

    // ADR-1500 D8: the search takes a file for its Regular face, so the
    // rasteriser draws that face, though a heavier one comes first in the
    // collection, and falls back to the family's first face.
    #[test]
    fn the_regular_face_of_a_collection_is_chosen_before_one_listed_first() {
        let bold: &[(u16, &str)] = &[(1, "Fam"), (2, "Bold")];
        let regular: &[(u16, &str)] = &[(1, "Fam"), (2, "Regular")];
        let other: &[(u16, &str)] = &[(1, "Other"), (2, "Regular")];
        let both = faces_of(&sfnt(&[other, bold, regular]), "bold-first").unwrap();
        assert_eq!(choose(&both, "Fam").map(|f| f.index), Some(2));
        let no_regular = faces_of(&sfnt(&[other, bold]), "no-regular").unwrap();
        assert_eq!(choose(&no_regular, "Fam").map(|f| f.index), Some(1));
        assert_eq!(choose(&no_regular, "Absent"), None);
    }

    // Faces that share one `name` table, as a collection's often do, read
    // and parse it once, so a file's reads stay near one table's bound.
    #[test]
    fn faces_that_share_a_name_table_read_it_once() {
        struct Counted<'a>(&'a [u8], std::cell::RefCell<Vec<(u64, usize)>>);
        impl Source for Counted<'_> {
            fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
                self.1.borrow_mut().push((offset, buf.len()));
                self.0.read_exact_at(buf, offset)
            }
        }
        let a: &[(u16, &str)] = &[(1, "Fam"), (2, "Regular")];
        let mut font = sfnt(&[a, a, a]);
        let dir = |f: &[u8], i: usize| {
            u32::from_be_bytes(f[12 + 4 * i..16 + 4 * i].try_into().unwrap()) as usize
        };
        // the second and third faces' records name the first face's table
        let first = dir(&font, 0) + 12;
        let record: Vec<u8> = font[first + 8..first + 16].to_vec();
        for i in [1, 2] {
            let at = dir(&font, i) + 12;
            font[at + 8..at + 16].copy_from_slice(&record);
        }
        let table = u32::from_be_bytes(record[..4].try_into().unwrap());
        let counted = Counted(&font, Default::default());
        let got = faces(&counted).unwrap();
        assert_eq!(got.iter().map(|f| f.index).collect::<Vec<_>>(), [0, 1, 2]);
        let reads = counted
            .1
            .borrow()
            .iter()
            .filter(|(o, _)| *o == u64::from(table))
            .count();
        assert_eq!(reads, 1);
    }

    // Item by item, a collection's damage stays with its face: a face
    // listed twice is read once, a face cut short or listing too many
    // tables is left out, and the faces after it are still read with
    // their own indexes.
    #[test]
    fn a_collection_keeps_its_good_faces_and_reads_a_repeated_one_once() {
        let a: &[(u16, &str)] = &[(1, "Alpha")];
        let b: &[(u16, &str)] = &[(1, "Beta")];
        let c: &[(u16, &str)] = &[(1, "Gamma")];
        let mut font = sfnt(&[a, b, c]);
        let offset =
            |f: &[u8], i: usize| u32::from_be_bytes(f[12 + 4 * i..16 + 4 * i].try_into().unwrap());
        let (first, second) = (offset(&font, 0), offset(&font, 1) as usize);
        // the second face's directory lists 65 tables, its first still its
        // `name` table, and the file runs long enough to hold them all, so
        // only the cap leaves it out
        font[second + 4..second + 6].copy_from_slice(&65u16.to_be_bytes());
        font.extend([0u8; 2048]);
        assert_eq!(
            faces_of(&font, "too-many-tables").unwrap(),
            [face(0, &["Alpha"], false), face(2, &["Gamma"], false)]
        );
        // the third face's offset names the first face again
        font[20..24].copy_from_slice(&first.to_be_bytes());
        assert_eq!(
            faces_of(&font, "repeated").unwrap(),
            [face(0, &["Alpha"], false)]
        );
        // the second face points past the end of the file
        let mut cut = sfnt(&[a, b, c]);
        cut[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            faces_of(&cut, "past-the-end").unwrap(),
            [face(0, &["Alpha"], false), face(2, &["Gamma"], false)]
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
