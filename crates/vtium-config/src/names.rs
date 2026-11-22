//! A font file's family names and style, read from its name table alone,
//! so that `font:FAMILY` finds an installed outline font by the family the
//! font stores, whatever the file is called (ADR-1500 D8). Only the file's
//! table directory and its `name` table are read, each with a bound, and
//! read-fonts, the table reader under swash, parses the table. The search
//! among installed fonts and the rasteriser's choice of a face both decide
//! with [`faces`] and [`matches`](fn@matches), so a face the search finds is a face the
//! rasteriser takes. The fuzz target `names` feeds [`faces`] any bytes
//! (`fuzz/README.md`).

use std::collections::BTreeSet;
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
    /// The version the face states (name ID 5), in English where the
    /// table holds it, which `version=` is matched against.
    pub version: Option<String>,
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
        if let Some((families, regular, version)) = names {
            out.push(Face {
                index,
                families: families.clone(),
                regular: *regular,
                version: version.clone(),
            });
        }
    }
    Ok(out)
}

/// The faces of the TrueType or OpenType font, or collection, `font`, as
/// swash reads them, once [`sound`] passes the font. Every path of
/// vtium-config that hands a font to swash comes through here.
pub fn swash_fonts(font: &[u8]) -> Result<swash::FontDataRef<'_>, String> {
    sound(font)?;
    swash::FontDataRef::new(font).ok_or_else(|| NOT_A_FONT.to_owned())
}

const NOT_A_FONT: &str = "not a TrueType or OpenType font";

/// Checks the structures swash reads in `font`, and refuses a font where
/// one would make swash, or the outline reader under it, panic. swash
/// 0.2.10 checks a range by subtracting the range's start from the end of
/// its data before it compares them, so a start past the end overflows,
/// and some of its sums, counts and negations overflow on values that a
/// damaged font holds. In a debug build each overflow panics. vtium-config
/// checks each face swash would read:
///
/// - each table its table directory lists lies inside the file;
/// - its `name` table holds the records its count gives, and each
///   record's string lies inside the table;
/// - the character map subtable that swash reads, when swash reads it as
///   format 12, lies inside the file, maps no character past U+10FFFF, and
///   maps no character to a glyph number past the largest one a 32-bit
///   number holds, which swash's sum would overflow;
/// - a face gives swash at least one glyph width: a face with `head` and
///   `maxp` tables holds a number of widths that is not 0 in its `hhea`
///   table, where it has one, and a face with a character map or a `hmtx`
///   table has `head` and `maxp` tables;
/// - the descender of its `hhea` and `vhea` tables is not -32768, which
///   swash cannot negate, and neither is the typographic descender of its
///   `OS/2` table when the face asks for its typographic metrics;
/// - its `HVAR` table, which varies the glyph widths, places its item
///   variation store at an offset from the file's start that 32 bits
///   hold, its advance width map holds at least one entry, and each of
///   the store's item variation data gives no more 16-bit deltas than
///   regions;
/// - no glyph of its `glyf` table has no contours yet gives a point flag,
///   which read-fonts, the reader skrifa draws outlines with, stores past
///   the end of the glyph's flags.
///
/// swash finds each table with a binary search over the table directory,
/// and picks one subtable of a character map, and the check finds the
/// table and the subtable that swash finds. swash reads a format 4
/// subtable only after a check that cannot overflow, so a format 4
/// subtable is left to swash. swash's reader of color bitmap strikes is
/// not checked.
///
/// The check walks each distinct `name` table, character map and `HVAR`
/// table once, however many faces or records point at it, and of a
/// character map only the subtable swash reads. It reads at most
/// [`FACES_MAX`] faces of a collection, as [`faces`] does, and refuses a
/// collection that lists more, because swash would read the faces past
/// that number unchecked.
pub fn sound(font: &[u8]) -> Result<(), String> {
    check(font).map(|_| ())
}

/// What one run of [`sound`] has walked, so that it walks each structure
/// once, and how many structures it walked, which the tests count.
#[derive(Default)]
struct Checked {
    /// The `name` tables walked, by their start and end.
    names: BTreeSet<(usize, usize)>,
    /// The `cmap` tables walked, by their start.
    cmaps: BTreeSet<usize>,
    /// The format 12 subtables walked, by their start.
    subtables: BTreeSet<usize>,
    /// The `HVAR` tables walked, by their start.
    hvars: BTreeSet<usize>,
    /// The `glyf` tables walked, by the place of their `loca` table, its
    /// format, and their own place.
    glyfs: BTreeSet<GlyfKey>,
    /// How many `name` tables had their records walked.
    names_walked: usize,
    /// How many groups of format 12 subtables the check walked.
    groups_walked: usize,
}

/// [`sound`], giving what the check walked.
fn check(font: &[u8]) -> Result<Checked, String> {
    let is_font = |at: usize| {
        font.get(at..at.saturating_add(4))
            .is_some_and(|t| matches!(t, [0, 1, 0, 0] | b"OTTO" | b"true"))
    };
    let offsets: BTreeSet<usize> = if font.starts_with(b"ttcf") {
        let count = u32_at(font, 8).ok_or(NOT_A_FONT)?;
        if count > FACES_MAX {
            return Err(format!(
                "the font collection lists {count} faces, more than the {FACES_MAX} that vtium-config reads"
            ));
        }
        // swash reads each face's offset only where the file holds it
        (0..count as usize)
            .map_while(|i| u32_at(font, 12 + i * 4))
            .map(|o| o as usize)
            .filter(|&o| is_font(o))
            .collect()
    } else if is_font(0) {
        BTreeSet::from([0])
    } else {
        return Err(NOT_A_FONT.to_owned());
    };
    let mut checked = Checked::default();
    for at in offsets {
        sound_face(font, at, &mut checked)?;
    }
    sound_glyphs(font, &mut checked)?;
    Ok(checked)
}

/// Where a `loca` table and a `glyf` table lie in a file, each as its
/// start and length, and whether the `loca` table is of the long format.
type GlyfKey = ((usize, usize), bool, (usize, usize));

/// Whether each simple glyph that read-fonts can read from `font` gives
/// point flags only when it has points. read-fonts 0.41, which skrifa reads
/// outlines with for swash and for the rasteriser, stores a glyph's first
/// point flag before it compares the number of flags stored with the
/// glyph's number of points. So a glyph of no contours, which has no
/// points, whose data still holds a flag without the repeat bit (0x08)
/// stores that flag past the end of its flags, and read-fonts panics, in a
/// release build too. This walks each face as read-fonts reads the file,
/// so it reaches the faces skrifa reads, and each glyph number up to the
/// largest that a 16-bit glyph number holds, as a character map or a
/// composite glyph can name.
fn sound_glyphs(font: &[u8], checked: &mut Checked) -> Result<(), String> {
    use skrifa::raw::types::{GlyphId, Tag};
    use skrifa::raw::{FileRef, TableProvider, tables::glyf::Glyph};
    let Ok(file) = FileRef::new(font) else {
        return Ok(());
    };
    let place = |data: FontData<'_>| {
        let bytes = data.as_bytes();
        (
            bytes.as_ptr() as usize - font.as_ptr() as usize,
            bytes.len(),
        )
    };
    for face in file.fonts().take(FACES_MAX as usize).flatten() {
        let (Ok(head), Ok(glyf)) = (face.head(), face.glyf()) else {
            continue;
        };
        let long = head.index_to_loc_format() == 1;
        let (Ok(loca), Some(loca_data), Some(glyf_data)) = (
            face.loca(Some(long)),
            face.table_data(Tag::new(b"loca")),
            face.table_data(Tag::new(b"glyf")),
        ) else {
            continue;
        };
        if !checked
            .glyfs
            .insert((place(loca_data), long, place(glyf_data)))
        {
            continue;
        }
        for id in 0..=u32::from(u16::MAX) {
            if loca.get_raw(id as usize + 1).is_none() {
                break;
            }
            if let Ok(Some(Glyph::Simple(glyph))) = loca.get_glyf(GlyphId::new(id), &glyf)
                && glyph.number_of_contours() == 0
                && glyph
                    .glyph_data()
                    .first()
                    .is_some_and(|&flag| flag & 0x08 == 0)
            {
                return Err(format!(
                    "the font is damaged: its glyph {id} has no outline points but gives flags for some"
                ));
            }
        }
    }
    Ok(())
}

/// [`sound`] for the face whose table directory starts at `at`.
fn sound_face(font: &[u8], at: usize, checked: &mut Checked) -> Result<(), String> {
    let damaged = |what: &str| format!("the font is damaged: {what}");
    let tables = usize::from(u16_at(font, at.saturating_add(4)).unwrap_or(0));
    for i in 0..tables {
        let record = at.saturating_add(12).saturating_add(i * 16);
        let (Some(tag), Some(start), Some(len)) = (
            font.get(record..record.saturating_add(4)),
            u32_at(font, record.saturating_add(8)),
            u32_at(font, record.saturating_add(12)),
        ) else {
            return Err(damaged("its table directory runs past the end of the file"));
        };
        let (start, len) = (start as usize, len as usize);
        if start.checked_add(len).is_none_or(|end| end > font.len()) {
            let name = match std::str::from_utf8(tag) {
                Ok(t) if t.bytes().all(|b| b.is_ascii_graphic() || b == b' ') => {
                    format!("the {} table", t.trim_end())
                }
                _ => "a table".to_owned(),
            };
            return Err(damaged(&format!(
                "its table directory places {name} past the end of the file"
            )));
        }
    }
    // every table the directory lists lies inside the file, so each range
    // found below does too
    let table = |tag: &[u8; 4]| swash_table(font, at, tag);
    if let Some(range) = table(b"name")
        && checked.names.insert(range)
    {
        checked.names_walked += 1;
        sound_name(font.get(range.0..range.1).unwrap_or_default())
            .map_err(|()| damaged("its name table points past its own end"))?;
    }
    if let Some((start, _)) = table(b"cmap")
        && checked.cmaps.insert(start)
        && let Some(subtable) = swash_subtable(font, start)
        && checked.subtables.insert(subtable)
    {
        sound_subtable(font, subtable, checked).map_err(damaged)?;
    }
    if table(b"head").is_some() && table(b"maxp").is_some() {
        // a face with no hhea table gives swash one width
        if let Some((start, end)) = table(b"hhea")
            && u16_at(font.get(start..end).unwrap_or_default(), 34).unwrap_or(0) == 0
        {
            return Err(damaged("its hhea table gives no glyph widths"));
        }
    } else if table(b"cmap").is_some() || table(b"hmtx").is_some() {
        // swash stops reading the metrics without a head or maxp table,
        // and its number of widths stays 0
        return Err(damaged(
            "it lacks the head or maxp table that swash reads its glyph widths after",
        ));
    }
    // swash negates each descender, a 16-bit number, and the smallest one
    // has no negative; a field that the table does not hold reads as 0
    let field = |tag: &[u8; 4], at: usize| {
        table(tag).and_then(|(start, end)| u16_at(font.get(start..end)?, at))
    };
    for tag in [b"hhea", b"vhea"] {
        if field(tag, 6) == Some(0x8000) {
            let tag = String::from_utf8_lossy(tag);
            return Err(damaged(&format!(
                "its {tag} table gives a descender of -32768"
            )));
        }
    }
    // swash reads the typographic descender only for a face whose
    // fsSelection field sets bit 7, which asks for the typographic metrics
    if field(b"OS/2", 62).is_some_and(|flags| flags & 0x80 != 0)
        && field(b"OS/2", 70) == Some(0x8000)
    {
        return Err(damaged(
            "its OS/2 table gives a typographic descender of -32768",
        ));
    }
    // swash reads no HVAR table at the file's start
    if let Some((start, _)) = table(b"HVAR")
        && start != 0
        && checked.hvars.insert(start)
    {
        sound_hvar(font, start).map_err(damaged)?;
    }
    Ok(())
}

/// The start and end of the table `tag` of the face whose table directory
/// starts at `at`, found as swash 0.2.10's `table_range` finds it: by a
/// binary search over the directory's records, which a font sorts by tag.
fn swash_table(font: &[u8], at: usize, tag: &[u8; 4]) -> Option<(usize, usize)> {
    let want = u32::from_be_bytes(*tag);
    let (mut low, mut high) = (0, usize::from(u16_at(font, at.checked_add(4)?)?));
    while low < high {
        let i = (low + high) / 2;
        let record = at.checked_add(12)?.checked_add(i * 16)?;
        match want.cmp(&u32_at(font, record)?) {
            std::cmp::Ordering::Less => high = i,
            std::cmp::Ordering::Greater => low = i + 1,
            std::cmp::Ordering::Equal => {
                let start = u32_at(font, record + 8)?;
                let end = start.checked_add(u32_at(font, record + 12)?)?;
                return Some((start as usize, end as usize));
            }
        }
    }
    None
}

/// Whether the `name` table `table` holds each record its count gives, and
/// each record's string inside the table.
fn sound_name(table: &[u8]) -> Result<(), ()> {
    // swash reads no record from a table too short for its count, nor from
    // a table whose count is 0
    let Some(count) = u16_at(table, 2).filter(|&n| n != 0) else {
        return Ok(());
    };
    let storage = usize::from(u16_at(table, 4).ok_or(())?);
    for i in 0..usize::from(count) {
        let record = 6 + i * 12;
        let (Some(len), Some(offset)) = (u16_at(table, record + 8), u16_at(table, record + 10))
        else {
            return Err(());
        };
        let start = storage + usize::from(offset);
        if start + usize::from(len) > table.len() {
            return Err(());
        }
    }
    Ok(())
}

/// The start of the character map subtable that swash 0.2.10's
/// `cmap::subtable` picks from the `cmap` table at `cmap`, when swash reads
/// it as format 12. swash picks the first subtable for Windows' symbol
/// encoding, or else the last Unicode subtable of format 12, or else the
/// first Unicode subtable. It considers only subtables whose format's low
/// byte is 4 or 12, and maps characters by that byte alone. It picks none
/// when a record, or the format a record points at, lies past the end of
/// the file, or when the subtable's offset from the file's start passes 32
/// bits. vtium-config reaches character maps through swash only by this
/// pick, and a format 4 subtable is left to swash, whose reads of one
/// cannot overflow.
fn swash_subtable(font: &[u8], cmap: usize) -> Option<usize> {
    let cmap = u32::try_from(cmap).ok().filter(|&c| c != 0)?;
    let data = font.get(cmap as usize..)?;
    let mut best = None;
    for i in 0..usize::from(u16_at(data, 2)?) {
        let record = 4 + i * 8;
        let platform = u16_at(data, record)?;
        let encoding = u16_at(data, record + 2)?;
        let offset = u32_at(data, record + 4)?;
        let format = u16_at(data, offset as usize)? as u8;
        if format != 4 && format != 12 {
            continue;
        }
        let at = (cmap.checked_add(offset)? as usize, format);
        if (platform, encoding) == (3, 0) {
            best = Some(at);
            break;
        }
        let unicode = matches!((platform, encoding), (0, _) | (3, 1) | (3, 10));
        if unicode && (format == 12 || best.is_none()) {
            best = Some(at);
        }
    }
    best.filter(|&(_, format)| format == 12).map(|(at, _)| at)
}

/// Whether the format 12 subtable at `at` is sound, as [`sound`] says.
/// swash reads the subtable against the end of the file, not of the
/// `cmap` table, and so does this.
fn sound_subtable(font: &[u8], at: usize, checked: &mut Checked) -> Result<(), &'static str> {
    let groups = u32_at(font, at.saturating_add(12)).map(|n| n as usize);
    let end = groups
        .and_then(|n| n.checked_mul(12))
        .and_then(|n| n.checked_add(16))
        .and_then(|n| n.checked_add(at));
    if end.is_none_or(|end| end > font.len()) {
        return Err("its character map runs past the end of the file");
    }
    for g in 0..groups.unwrap_or(0) {
        checked.groups_walked += 1;
        let group = at + 16 + g * 12;
        let (Some(first), Some(last), Some(glyph)) = (
            u32_at(font, group),
            u32_at(font, group + 4),
            u32_at(font, group + 8),
        ) else {
            return Err("its character map runs past the end of the file");
        };
        if first > last {
            continue;
        }
        if last > 0x10_ffff {
            return Err("its character map maps a character past U+10FFFF");
        }
        if glyph.checked_add(last).is_none() {
            return Err(
                "its character map maps a character to a glyph past the largest glyph number",
            );
        }
    }
    Ok(())
}

/// Whether the `HVAR` table at `base` is sound, as [`sound`] says. This
/// follows swash 0.2.10's `metric_delta` and `item_delta` for an advance
/// width: a field that the file does not hold ends swash's reading, and
/// ends the check there too.
fn sound_hvar(font: &[u8], base: usize) -> Result<(), &'static str> {
    let Some(store) = u32_at(font, base + 4).filter(|&s| s != 0) else {
        return Ok(());
    };
    // swash adds the two offsets as 32-bit numbers
    let Some(store) = u32::try_from(base).ok().and_then(|b| store.checked_add(b)) else {
        return Err("its HVAR table points past the largest offset a font can hold");
    };
    let store = store as usize;
    // swash takes the last entry of the advance width map for a glyph past
    // its count, by subtracting one from the count
    if let Some(map) = u32_at(font, base + 8).filter(|&m| m != 0) {
        let map = base.saturating_add(map as usize);
        if u16_at(font, map).is_some() && u16_at(font, map.saturating_add(2)) == Some(0) {
            return Err("its HVAR table holds an advance width map with no entry");
        }
    }
    // swash subtracts an item variation data's count of 16-bit deltas from
    // its count of regions
    let Some(datas) = u16_at(font, store.saturating_add(6)) else {
        return Ok(());
    };
    let Some(regions) = u32_at(font, store.saturating_add(2)) else {
        return Ok(());
    };
    let regions = store.saturating_add(regions as usize);
    if u16_at(font, regions).is_none() || u16_at(font, regions.saturating_add(2)).is_none() {
        return Ok(());
    }
    for outer in 0..usize::from(datas) {
        let Some(data) = u32_at(font, store.saturating_add(8 + outer * 4)) else {
            break;
        };
        let data = store.saturating_add(data as usize);
        if let (Some(count), Some(short)) = (
            u16_at(font, data.saturating_add(4)),
            u16_at(font, data.saturating_add(2)),
        ) && short > count
        {
            return Err("its HVAR table gives more 16-bit deltas than regions");
        }
    }
    Ok(())
}

/// What a `name` table gives a face: its families, whether its style is
/// Regular, and its version.
type Names = (Vec<String>, bool, Option<String>);

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
    let version = strings(NameId::VERSION_STRING).into_iter().next();
    Some((families, regular, version))
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
    use crate::scratch::Scratch;

    /// The faces of `bytes`, read once as bytes and once as a file, which
    /// must agree. The file is in a directory of the test's own, named with
    /// the test process's id, so two test runs at once never write each
    /// other's file, and the directory is removed once the file is read.
    fn faces_of(bytes: &[u8], test: &str) -> Result<Vec<Face>, String> {
        let dir = Scratch::new(&format!("config-names-{test}"));
        let path = dir.join(test);
        std::fs::write(&path, bytes).unwrap();
        let from_file = faces(&File::open(&path).unwrap());
        drop(dir);
        let from_bytes = faces(bytes);
        assert_eq!(from_file.is_ok(), from_bytes.is_ok(), "{test}");
        assert_eq!(from_file.as_ref().ok(), from_bytes.as_ref().ok(), "{test}");
        from_bytes
    }

    fn face(index: u32, families: &[&str], regular: bool) -> Face {
        Face {
            version: None,
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

    /// A font of one face whose directory lists `tables`, each `(tag,
    /// bytes)` laid out after the directory in order; `cut` bytes are then
    /// taken off the end of the file.
    fn font_of(tables: &[(&[u8; 4], Vec<u8>)], cut: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(0x0001_0000u32.to_be_bytes());
        out.extend((tables.len() as u16).to_be_bytes());
        out.extend([0u8; 6]);
        let mut at = 12 + 16 * tables.len();
        for (tag, bytes) in tables {
            out.extend(*tag);
            out.extend(0u32.to_be_bytes());
            out.extend((at as u32).to_be_bytes());
            out.extend((bytes.len() as u32).to_be_bytes());
            at += bytes.len();
        }
        for (_, bytes) in tables {
            out.extend(bytes);
        }
        out.truncate(out.len() - cut);
        out
    }

    /// A `cmap` table of one Windows Unicode record whose format 12
    /// subtable holds `groups`, each `(first, last, glyph)`.
    fn cmap12(groups: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for v in [0u16, 1, 3, 10] {
            out.extend(v.to_be_bytes());
        }
        out.extend(12u32.to_be_bytes());
        out.extend(12u16.to_be_bytes());
        out.extend(0u16.to_be_bytes());
        out.extend((16 + 12 * groups.len() as u32).to_be_bytes());
        out.extend(0u32.to_be_bytes());
        out.extend((groups.len() as u32).to_be_bytes());
        for (first, last, glyph) in groups {
            for v in [first, last, glyph] {
                out.extend(v.to_be_bytes());
            }
        }
        out
    }

    /// A font of one face with the `cmap` table `cmap`, and the `head` and
    /// `maxp` tables, here all zeros, that a face with a character map
    /// needs for swash to give it glyph widths.
    fn with_widths(cmap: Vec<u8>) -> Vec<u8> {
        font_of(
            &[
                (b"cmap", cmap),
                (b"head", vec![0; 54]),
                (b"maxp", vec![0; 6]),
            ],
            0,
        )
    }

    /// Whether swash, asked for the font's coverage as the helper asks,
    /// panics on `font`. The test profile checks overflow, as a debug
    /// build does.
    fn swash_panics(font: &[u8]) -> bool {
        std::panic::catch_unwind(|| {
            if let Some(f) = swash::FontRef::from_index(font, 0) {
                f.charmap().enumerate(|_, _| {});
                let _ = f
                    .localized_strings()
                    .find_by_id(swash::StringId::Family, None);
            }
        })
        .is_err()
    }

    // Each font below makes swash 0.2.10 subtract past zero in a range
    // check, or overflow a sum, which panics in a debug build. The check
    // refuses each one with a message that names what is damaged, and
    // passes the sound fonts it is built from.
    #[test]
    fn the_check_refuses_each_structure_that_makes_swash_overflow() {
        let damaged = |what: &str| Err(format!("the font is damaged: {what}"));
        let name = sfnt(&[&[(1, "Mono")]]);
        assert_eq!(sound(&name), Ok(()));
        assert_eq!(sound(&with_widths(cmap12(&[(0x41, 0x5a, 1)]))), Ok(()));
        assert_eq!(sound(&vtium_testfont::ttf::test_mono().build()), Ok(()));

        // a name table whose records end past it
        let mut short = name.clone();
        short[31] = 9;
        assert!(swash_panics(&short));
        assert_eq!(
            sound(&short),
            damaged("its name table points past its own end")
        );
        // a string past the end of its name table
        let mut long = name.clone();
        long[44] = 0xf0;
        assert_eq!(
            sound(&long),
            damaged("its name table points past its own end")
        );

        // a format 12 subtable cut 8 bytes into its header
        let cut = font_of(&[(b"cmap", cmap12(&[]))], 8);
        assert!(swash_panics(&cut));
        assert_eq!(
            sound(&cut),
            damaged("its table directory places the cmap table past the end of the file")
        );
        let mut cut = cut;
        cut[24..28].copy_from_slice(&20u32.to_be_bytes());
        assert_eq!(
            sound(&cut),
            damaged("its character map runs past the end of the file")
        );
        // a glyph number the sum of the first glyph and a character overflows
        let wraps = font_of(&[(b"cmap", cmap12(&[(0x41, 0x42, u32::MAX - 1)]))], 0);
        assert!(swash_panics(&wraps));
        assert_eq!(
            sound(&wraps),
            damaged("its character map maps a character to a glyph past the largest glyph number")
        );
        let far = font_of(&[(b"cmap", cmap12(&[(0x41, 0x11_0000, 1)]))], 0);
        assert_eq!(
            sound(&far),
            damaged("its character map maps a character past U+10FFFF")
        );

        // a table directory listing a table past the end of the file
        let mut past = name.clone();
        past[20..24].copy_from_slice(&0xffff_0000u32.to_be_bytes());
        assert_eq!(
            sound(&past),
            damaged("its table directory places the name table past the end of the file")
        );
        assert_eq!(sound(b"not a font"), Err(NOT_A_FONT.to_owned()));
        // a collection is checked face by face
        let two = sfnt(&[&[(1, "A")], &[(1, "B")]]);
        assert_eq!(sound(&two), Ok(()));
        let mut two = two;
        let last = two.len() - 1;
        two[last - 4] = 0xff;
        assert!(sound(&two).is_err());
    }

    /// DejaVu Sans Mono cut to printable ASCII, a font that maps every
    /// character `describe` measures the widths of.
    const DEJAVU: &[u8] = include_bytes!("../tests/fixtures/DejaVuSansMono-ascii.ttf");

    /// `font` with its tables edited by `edit` and laid out again, each
    /// table on a four-byte boundary and the directory sorted by tag, as
    /// swash's binary search over the directory needs.
    fn retable(font: &[u8], edit: impl FnOnce(&mut Vec<([u8; 4], Vec<u8>)>)) -> Vec<u8> {
        let count = usize::from(u16_at(font, 4).unwrap());
        let mut tables: Vec<([u8; 4], Vec<u8>)> = (0..count)
            .map(|i| {
                let record = 12 + 16 * i;
                let start = u32_at(font, record + 8).unwrap() as usize;
                let len = u32_at(font, record + 12).unwrap() as usize;
                let tag = font[record..record + 4].try_into().unwrap();
                (tag, font[start..start + len].to_vec())
            })
            .collect();
        edit(&mut tables);
        tables.sort_by_key(|(tag, _)| *tag);
        let mut out = font[..4].to_vec();
        out.extend((tables.len() as u16).to_be_bytes());
        out.extend([0u8; 6]);
        let mut at = 12 + 16 * tables.len();
        let mut bodies = Vec::new();
        for (tag, bytes) in &tables {
            out.extend(tag);
            out.extend(0u32.to_be_bytes());
            out.extend((at as u32).to_be_bytes());
            out.extend((bytes.len() as u32).to_be_bytes());
            let mut body = bytes.clone();
            body.resize(bytes.len().next_multiple_of(4), 0);
            at += body.len();
            bodies.extend(body);
        }
        out.extend(bodies);
        out
    }

    /// The table `tag` of `tables`, to edit.
    fn table<'a>(tables: &'a mut [([u8; 4], Vec<u8>)], tag: &[u8; 4]) -> &'a mut Vec<u8> {
        &mut tables.iter_mut().find(|(t, _)| t == tag).unwrap().1
    }

    /// An `HVAR` table of version 1.0 whose item variation store starts
    /// `store` bytes into the table and whose advance width map starts
    /// `map` bytes in, followed by `rest`.
    fn hvar(store: u32, map: u32, rest: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 1, 0, 0];
        for v in [store, map, 0, 0] {
            out.extend(v.to_be_bytes());
        }
        out.extend(rest);
        out
    }

    /// Whether swash, asked for a glyph's advance width as `describe` and
    /// the rasteriser ask, panics on `font`.
    fn swash_width_panics(font: &[u8]) -> bool {
        std::panic::catch_unwind(|| {
            if let Some(f) = swash::FontRef::from_index(font, 0) {
                let _ = f.glyph_metrics(&[]).advance_width(36);
            }
        })
        .is_err()
    }

    /// Whether swash, asked for the font's metrics as `describe`, the
    /// rasteriser and every scaler ask, panics on `font`.
    fn swash_metrics_panics(font: &[u8]) -> bool {
        std::panic::catch_unwind(|| {
            if let Some(f) = swash::FontRef::from_index(font, 0) {
                let _ = f.metrics(&[]);
            }
        })
        .is_err()
    }

    /// Checks that swash panics on `font` when asked for a glyph's width,
    /// that the check refuses `font` as damaged in the way `what` says, and
    /// that `describe` refuses the font too, with no panic.
    fn refused(font: &[u8], what: &str) {
        refused_after(font, what, swash_width_panics);
    }

    /// [`refused`], with `panics` saying whether swash panics on `font`.
    fn refused_after(font: &[u8], what: &str, panics: fn(&[u8]) -> bool) {
        assert!(panics(font), "swash reads {what} with no panic");
        assert_eq!(sound(font), Err(format!("the font is damaged: {what}")));
        let described = std::panic::catch_unwind(|| crate::describe::describe(font, "f.ttf"));
        assert!(matches!(described, Ok(Err(_))), "{what}: {described:?}");
    }

    // swash takes the number of glyph widths in the `hmtx` table from the
    // `hhea` table, and an advance past that number from the last width.
    // So it subtracts one from the number, which overflows when the number
    // is 0. The number is 0 when the `hhea` table is too short to hold it,
    // and when the face lacks a `head` or `maxp` table, because swash stops
    // reading the metrics there.
    #[test]
    fn the_check_refuses_a_face_that_gives_swash_no_glyph_widths() {
        assert_eq!(sound(DEJAVU), Ok(()));
        assert!(!swash_width_panics(DEJAVU));
        let none = "its hhea table gives no glyph widths";
        refused(
            &retable(DEJAVU, |t| table(t, b"hhea")[34..36].fill(0)),
            none,
        );
        refused(&retable(DEJAVU, |t| table(t, b"hhea").truncate(30)), none);
        // swash subtracts whether or not the face has an hmtx table
        refused(
            &retable(DEJAVU, |t| {
                table(t, b"hhea")[34..36].fill(0);
                t.retain(|(tag, _)| tag != b"hmtx");
            }),
            none,
        );
        // and whether or not the face has a character map: the shaper asks
        // for the width of the glyph a tatweel maps to, glyph 0 here
        refused(
            &retable(DEJAVU, |t| {
                table(t, b"hhea")[34..36].fill(0);
                t.retain(|(tag, _)| tag != b"hmtx" && tag != b"cmap");
            }),
            none,
        );
        let lacks = "it lacks the head or maxp table that swash reads its glyph widths after";
        refused(
            &retable(DEJAVU, |t| t.retain(|(tag, _)| tag != b"maxp")),
            lacks,
        );
        refused(
            &retable(DEJAVU, |t| t.retain(|(tag, _)| tag != b"head")),
            lacks,
        );
        // a face with no hhea table gives swash one glyph width
        let no_hhea = retable(DEJAVU, |t| t.retain(|(tag, _)| tag != b"hhea"));
        assert!(!swash_width_panics(&no_hhea));
        assert_eq!(sound(&no_hhea), Ok(()));
    }

    // swash negates the descender of the `hhea` and `vhea` tables, and the
    // typographic descender of the `OS/2` table when the face asks for its
    // typographic metrics. Each is a 16-bit number, and negating the
    // smallest one, -32768, overflows.
    #[test]
    fn the_check_refuses_a_descender_that_swash_cannot_negate() {
        assert!(!swash_metrics_panics(DEJAVU));
        refused_after(
            &retable(DEJAVU, |t| {
                table(t, b"hhea")[6..8].copy_from_slice(&0x8000u16.to_be_bytes())
            }),
            "its hhea table gives a descender of -32768",
            swash_metrics_panics,
        );
        // a vhea table of version 1.0 with one vertical metric
        let mut vhea = vec![0u8; 36];
        vhea[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        vhea[6..8].copy_from_slice(&0x8000u16.to_be_bytes());
        vhea[34..36].copy_from_slice(&1u16.to_be_bytes());
        refused_after(
            &retable(DEJAVU, |t| t.push((*b"vhea", vhea))),
            "its vhea table gives a descender of -32768",
            swash_metrics_panics,
        );
        // fsSelection's bit 7 asks for the typographic metrics
        let typo = |use_typo: bool| {
            retable(DEJAVU, |t| {
                let os2 = table(t, b"OS/2");
                os2[63] = if use_typo {
                    os2[63] | 0x80
                } else {
                    os2[63] & !0x80
                };
                os2[70..72].copy_from_slice(&0x8000u16.to_be_bytes());
            })
        };
        refused_after(
            &typo(true),
            "its OS/2 table gives a typographic descender of -32768",
            swash_metrics_panics,
        );
        assert!(!swash_metrics_panics(&typo(false)));
        assert_eq!(sound(&typo(false)), Ok(()));
        // one more than the smallest descender negates
        let next = retable(DEJAVU, |t| {
            table(t, b"hhea")[6..8].copy_from_slice(&0x8001u16.to_be_bytes())
        });
        assert!(!swash_metrics_panics(&next));
        assert_eq!(sound(&next), Ok(()));
    }

    // swash adds the offset of an HVAR table's item variation store to the
    // table's own offset in 32 bits, subtracts one from the count of its
    // advance width map, and subtracts an item variation data's count of
    // 16-bit deltas from its count of regions. Each of those overflows on
    // the HVAR tables below.
    #[test]
    fn the_check_refuses_each_hvar_table_that_makes_swash_overflow() {
        let with = |hvar: Vec<u8>| retable(DEJAVU, |t| t.push((*b"HVAR", hvar)));
        // a store of one item variation data, with no regions, whose one
        // region index and `short` 16-bit deltas follow it
        let store = |short: u16| {
            let mut out = Vec::new();
            for v in [1u16, 0, 12, 1, 0, 16, 0, 0, 1, short, 1, 0] {
                out.extend(v.to_be_bytes());
            }
            out.extend([0u8; 4]);
            out
        };
        let sound_one = with(hvar(20, 0, &store(1)));
        assert!(!swash_width_panics(&sound_one));
        assert_eq!(sound(&sound_one), Ok(()));
        refused(
            &with(hvar(0xffff_fff0, 0, &[])),
            "its HVAR table points past the largest offset a font can hold",
        );
        // an advance width map of format 0 that holds no entry, before a
        // sound store
        let mut empty = vec![0u8; 4];
        empty.extend(store(1));
        refused(
            &with(hvar(24, 20, &empty)),
            "its HVAR table holds an advance width map with no entry",
        );
        refused(
            &with(hvar(20, 0, &store(2))),
            "its HVAR table gives more 16-bit deltas than regions",
        );
    }

    /// DejaVu with its last glyph, glyph 95, replaced by a glyph of no
    /// contours whose data after its header and its empty instructions is
    /// `rest`. Its `loca` table is of the short format, which halves each
    /// offset.
    fn last_glyph(rest: &[u8]) -> Vec<u8> {
        retable(DEJAVU, |t| {
            let glyf = table(t, b"glyf");
            let start = glyf.len();
            glyf.extend([0u8; 12]);
            glyf.extend(rest);
            let end = glyf.len();
            let loca = table(t, b"loca");
            let last = loca.len() - 4;
            loca[last..last + 2].copy_from_slice(&((start / 2) as u16).to_be_bytes());
            loca[last + 2..].copy_from_slice(&((end / 2) as u16).to_be_bytes());
        })
    }

    /// Whether swash, asked to draw glyph 95's outline as the rasteriser
    /// asks, panics on `font`.
    fn swash_draw_panics(font: &[u8]) -> bool {
        use swash::scale::{Render, ScaleContext, Source};
        std::panic::catch_unwind(|| {
            if let Some(f) = swash::FontRef::from_index(font, 0) {
                let mut context = ScaleContext::new();
                let mut scaler = context.builder(f).size(16.0).build();
                let _ = Render::new(&[Source::Outline]).render(&mut scaler, 95);
            }
        })
        .is_err()
    }

    // read-fonts, which swash and the rasteriser read outlines with,
    // writes a simple glyph's first point flag before it compares the
    // number of flags written with the glyph's number of points. A glyph
    // of no contours has no points, so a flag in its data without the
    // repeat bit (0x08) lands past the end of its flags, which panics in a
    // release build too.
    #[test]
    fn the_check_refuses_a_glyph_that_gives_flags_but_no_points() {
        assert!(!swash_draw_panics(DEJAVU));
        for sound_one in [last_glyph(&[]), last_glyph(&[0x08, 0])] {
            assert!(!swash_draw_panics(&sound_one));
            assert_eq!(sound(&sound_one), Ok(()));
        }
        refused_after(
            &last_glyph(&[0x01, 0]),
            "its glyph 95 has no outline points but gives flags for some",
            swash_draw_panics,
        );
    }

    /// A collection that lists `count` faces, over `dirs` copies of the
    /// one face's table directory in `font`, all of which list one copy of
    /// its tables. The faces take the copies in turn.
    fn collection(count: u32, dirs: u32, font: &[u8]) -> Vec<u8> {
        let tables = usize::from(u16_at(font, 4).unwrap());
        let dir = 12 + 16 * tables;
        let head = 12 + 4 * count;
        let mut out = b"ttcf".to_vec();
        out.extend(0x0001_0000u32.to_be_bytes());
        out.extend(count.to_be_bytes());
        for i in 0..count {
            out.extend((head + (i % dirs) * dir as u32).to_be_bytes());
        }
        // each table's offset is from the start of the file
        let moved = head + dirs * dir as u32 - dir as u32;
        let mut face = font[..dir].to_vec();
        for i in 0..tables {
            let at = 12 + 16 * i + 8;
            let offset = u32_at(&face, at).unwrap() + moved;
            face[at..at + 4].copy_from_slice(&offset.to_be_bytes());
        }
        for _ in 0..dirs {
            out.extend(&face);
        }
        out.extend(&font[dir..]);
        out
    }

    // A thousand encoding records that point at one format 12 subtable,
    // and a thousand faces that share one name table, cost the check what
    // one costs: it walks each distinct subtable and name table once.
    #[test]
    fn the_check_walks_each_shared_table_once() {
        let groups: Vec<(u32, u32, u32)> = (0..10_000).map(|i| (i * 2, i * 2, 1)).collect();
        let mut cmap = Vec::new();
        cmap.extend(0u16.to_be_bytes());
        cmap.extend(1000u16.to_be_bytes());
        let subtable = 4 + 8 * 1000u32;
        for _ in 0..1000 {
            for v in [3u16, 10] {
                cmap.extend(v.to_be_bytes());
            }
            cmap.extend(subtable.to_be_bytes());
        }
        cmap.extend(&cmap12(&groups)[12..]);
        assert_eq!(
            check(&with_widths(cmap)).map(|c| c.groups_walked),
            Ok(10_000)
        );

        let one = sfnt(&[&[(1, "Mono")]]);
        let many = collection(FACES_MAX, FACES_MAX, &one);
        assert_eq!(faces(many.as_slice()).map(|f| f.len()), Ok(256));
        assert_eq!(check(&many).map(|c| c.names_walked), Ok(1));
    }

    // A region of the 32-bit number 0x000C0000 repeated reads, from any
    // four-byte step, as a sound format 12 subtable of 786,432 groups.
    // Sixteen encoding records that each point four bytes further into it
    // list sixteen such subtables, which share their groups' bytes. swash
    // reads only one of them, and the check walks only that one.
    #[test]
    fn the_check_walks_only_the_subtable_that_swash_reads() {
        let records = 16u32;
        let mut cmap = vec![0u8, 0];
        cmap.extend((records as u16).to_be_bytes());
        let region = 4 + 8 * records;
        for i in 0..records {
            for v in [3u16, 10] {
                cmap.extend(v.to_be_bytes());
            }
            cmap.extend((region + 4 * i).to_be_bytes());
        }
        let groups = 0x000C_0000;
        for _ in 0..(16 + 12 * groups) / 4 + records as usize {
            cmap.extend(0x000C_0000u32.to_be_bytes());
        }
        let font = with_widths(cmap);
        assert!(!swash_panics(&font));
        assert_eq!(check(&font).map(|c| c.groups_walked), Ok(groups));
    }

    /// Whether swash, asked for the glyph of U+0042, panics on `font`.
    fn swash_map_panics(font: &[u8]) -> bool {
        std::panic::catch_unwind(|| {
            if let Some(f) = swash::FontRef::from_index(font, 0) {
                let _ = f.charmap().map(0x42u32);
            }
        })
        .is_err()
    }

    // swash picks one subtable of a character map: the first one for
    // Windows' symbol encoding, or else the last Unicode one of format 12,
    // or else the first Unicode one. It reads only the low byte of the
    // subtable's format when it picks, so it maps characters through a
    // subtable of format 0x010C as through one of format 12.
    #[test]
    fn the_check_reads_the_subtable_that_swash_picks_as_swash_reads_it() {
        let wraps = (0x41, 0x42, u32::MAX - 1);
        let wrong = "its character map maps a character to a glyph past the largest glyph number";
        // a character map of a sound Unicode subtable, then a damaged
        // subtable under the second record's platform and encoding
        let after = |platform: u16, encoding: u16| {
            let sound = &cmap12(&[(0x41, 0x5a, 1)])[12..];
            let mut two = Vec::new();
            for v in [0u16, 2, 3, 10] {
                two.extend(v.to_be_bytes());
            }
            two.extend(20u32.to_be_bytes());
            for v in [platform, encoding] {
                two.extend(v.to_be_bytes());
            }
            two.extend((20 + sound.len() as u32).to_be_bytes());
            two.extend(sound);
            two.extend(&cmap12(&[wraps])[12..]);
            with_widths(two)
        };
        // swash never picks the Macintosh's Roman encoding
        let mac = after(1, 0);
        assert!(!swash_panics(&mac) && !swash_map_panics(&mac));
        assert_eq!(sound(&mac), Ok(()));
        // and picks Windows' symbol encoding before any other
        let symbol = after(3, 0);
        assert!(swash_panics(&symbol));
        assert_eq!(sound(&symbol), Err(format!("the font is damaged: {wrong}")));
        // a subtable of format 0x010C, whose first glyph swash's mapping
        // adds to the distance from the group's first character
        let mut odd = cmap12(&[(0x41, 0x42, u32::MAX)]);
        odd[12..14].copy_from_slice(&0x010Cu16.to_be_bytes());
        let odd = with_widths(odd);
        assert!(swash_map_panics(&odd));
        assert_eq!(sound(&odd), Err(format!("the font is damaged: {wrong}")));
    }

    // swash reads every face that a collection lists, and the check reads
    // as many as `faces` and `describe` read: a collection that lists more
    // is refused, so no face swash reads goes unchecked.
    #[test]
    fn a_collection_of_more_faces_than_the_check_reads_is_refused() {
        let one = sfnt(&[&[(1, "Mono")]]);
        let at_most = collection(FACES_MAX, 1, &one);
        assert_eq!(sound(&at_most), Ok(()));
        let more = collection(FACES_MAX + 1, 1, &one);
        assert_eq!(
            sound(&more),
            Err(
                "the font collection lists 257 faces, more than the 256 that vtium-config reads"
                    .to_owned()
            )
        );
    }

    // swash reads a name table whose count is 0 as holding no records, so
    // a table of 4 bytes that gives no storage offset is sound.
    #[test]
    fn a_name_table_of_no_records_needs_no_storage_offset() {
        for table in [vec![0u8, 0, 0, 0], vec![0, 0, 0, 0, 0]] {
            let font = font_of(&[(b"name", table)], 0);
            assert_eq!(sound(&font), Ok(()));
        }
        let font = font_of(&[(b"name", vec![0, 0, 0, 1])], 0);
        assert!(sound(&font).is_err());
    }
}
