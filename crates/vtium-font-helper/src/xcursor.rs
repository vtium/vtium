//! The reader of a desktop cursor theme's files, the Xcursor format, which
//! the font helper runs in its sandbox for the console's pointer.
//!
//! A cursor file holds one cursor at several nominal sizes, the number a
//! desktop's cursor size setting names, and a busy cursor holds several
//! frames of each size. The file is a header, a table of contents and
//! chunks. The header is the magic `Xcur`, the header's length, a file
//! version and the number of table entries; the table starts at the
//! header's length, and each entry gives a chunk's type, its subtype and
//! its absolute position in the file. An image chunk has a 36-byte header:
//! its length, its type `0xfffd0002`, its nominal size as its subtype,
//! version 1, its width and height, its hotspot, and the delay to the next
//! frame, then width × height pixels. Every number is a 32-bit unsigned
//! integer stored least significant byte first, and each pixel is ARGB
//! with its colors premultiplied by its alpha.
//!
//! The reader keeps the first image of each nominal size, in the table's
//! order, so a busy cursor stands still on its first frame. It skips an
//! image of nominal size 0, which names no size, so the file's other sizes
//! still serve. It drops a size
//! whose picture is wider or taller than [`POINTER_SIDE_MAX`], since no
//! cursor plane takes it, and keeps the other sizes from the smallest up
//! while the pixels kept stay within [`POINTER_THEME_PIXELS_MAX`]. With a
//! size named, as `pointer-size` names one, it keeps only the size nearest
//! it, the smaller of two equally near. It ignores the file's version and
//! every chunk that is not an image. Each check below fails the whole
//! file, and a failed file counts as missing, so the console tries the
//! shape's next file name.
//!
//! References:
//! - ADR-1309 D6: what the reader reads, its checks and their limits, the first frame only, and the sizes it keeps.
//! - Xcursor(3), libXcursor's man page: the file's layout.

use vtium_render::{POINTER_SIDE_MAX, POINTER_THEME_PIXELS_MAX, XCURSOR_FILE_MAX, XCURSOR_TOC_MAX};

use crate::serve::PointerPicture;

/// The magic a cursor file starts with.
const MAGIC: &[u8; 4] = b"Xcur";

/// The fields of the file's header that the reader takes, so the shortest
/// header it accepts: magic, header length, version and entry count.
const HEADER_MIN: u32 = 16;

/// The bytes of one table entry: type, subtype and position.
const ENTRY_BYTES: usize = 12;

/// The type of an image chunk.
pub const IMAGE_TYPE: u32 = 0xfffd_0002;

/// The bytes of an image chunk's header.
pub const IMAGE_HEADER: u32 = 36;

/// The version every image chunk carries.
const IMAGE_VERSION: u32 = 1;

/// The 32-bit number at byte `at` of `bytes`, least significant byte
/// first, or `None` past the end.
fn word(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let b: [u8; 4] = bytes.get(at..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(b))
}

fn index(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// An image the table names whose header passes, before its pixels are
/// read.
struct Candidate {
    nominal: u32,
    width: u32,
    height: u32,
    hot: (u32, u32),
    /// Where its pixels start.
    pixels: usize,
}

/// The pictures of the cursor file `bytes` that the reader keeps, each a
/// [`PointerPicture`] whose size is its nominal size, smallest first; with
/// `size`, only the one nearest it. The error says why the file fails, in
/// words that finish the sentence "because ...". A file whose every size
/// is too large keeps nothing, and that is an error too, since it gives
/// the shape no picture.
pub fn read(bytes: &[u8], size: Option<u32>) -> Result<Vec<PointerPicture>, String> {
    if bytes.len() > XCURSOR_FILE_MAX {
        return Err(format!(
            "it is longer than {} MiB",
            XCURSOR_FILE_MAX / (1024 * 1024)
        ));
    }
    if bytes.get(..4) != Some(&MAGIC[..]) {
        return Err("it is not a cursor file: it does not start with Xcur".to_owned());
    }
    let cut = || "it ends inside its header".to_owned();
    let header = word(bytes, 4).ok_or_else(cut)?;
    let entries = word(bytes, 12).ok_or_else(cut)?;
    if header < HEADER_MIN {
        return Err(format!(
            "its header is {header} bytes long, shorter than the {HEADER_MIN} bytes of its fields"
        ));
    }
    if index(entries) > XCURSOR_TOC_MAX {
        return Err(format!(
            "its table of contents has {entries} entries, more than {XCURSOR_TOC_MAX}"
        ));
    }
    let table = index(header);
    let table_end = index(entries)
        .checked_mul(ENTRY_BYTES)
        .and_then(|n| n.checked_add(table))
        .filter(|&end| end <= bytes.len())
        .ok_or("its table of contents runs past the end of the file")?;
    let mut candidates: Vec<Candidate> = Vec::new();
    // the nominal sizes whose first image has been read, kept or dropped
    let mut seen: Vec<u32> = Vec::new();
    for entry in (table..table_end).step_by(ENTRY_BYTES) {
        let field = |i: usize| {
            entry
                .checked_add(i.saturating_mul(4))
                .and_then(|at| word(bytes, at))
        };
        let (Some(kind), Some(subtype), Some(position)) = (field(0), field(1), field(2)) else {
            return Err("its table of contents runs past the end of the file".to_owned());
        };
        if kind != IMAGE_TYPE || subtype == 0 || seen.contains(&subtype) {
            continue;
        }
        seen.push(subtype);
        if let Some(c) = image(bytes, subtype, index(position))? {
            candidates.push(c);
        }
    }
    candidates.sort_by_key(|c| c.nominal);
    let chosen: Vec<&Candidate> = match size {
        Some(want) => candidates
            .iter()
            .min_by_key(|c| (c.nominal.abs_diff(want), c.nominal))
            .into_iter()
            .collect(),
        None => {
            let mut total = 0_usize;
            candidates
                .iter()
                .take_while(|c| {
                    total = total.saturating_add(index(c.width).saturating_mul(index(c.height)));
                    total <= POINTER_THEME_PIXELS_MAX
                })
                .collect()
        }
    };
    if chosen.is_empty() {
        return Err(format!(
            "it has no picture of at most {POINTER_SIDE_MAX} pixels a side"
        ));
    }
    chosen.into_iter().map(|c| pixels(bytes, c)).collect()
}

/// The image chunk at `at`, which the table names with the nominal size
/// `subtype`: its header checked, and `None` when it is too large to keep.
fn image(bytes: &[u8], subtype: u32, at: usize) -> Result<Option<Candidate>, String> {
    let field = |i: usize| {
        at.checked_add(i.saturating_mul(4))
            .and_then(|at| word(bytes, at))
            .ok_or("an image's header runs past the end of the file")
    };
    let (length, kind, sub, version) = (field(0)?, field(1)?, field(2)?, field(3)?);
    let (width, height, xhot, yhot) = (field(4)?, field(5)?, field(6)?, field(7)?);
    field(8)?;
    if length != IMAGE_HEADER {
        return Err(format!(
            "an image's header is {length} bytes long, not {IMAGE_HEADER}"
        ));
    }
    if kind != IMAGE_TYPE {
        return Err("a chunk the table calls an image is not one".to_owned());
    }
    if sub != subtype {
        return Err(format!(
            "an image of size {sub} sits where the table names size {subtype}"
        ));
    }
    if version != IMAGE_VERSION {
        return Err(format!("an image has version {version}, not 1"));
    }
    if width == 0 || height == 0 {
        return Err("an image has no pixels".to_owned());
    }
    if xhot >= width || yhot >= height {
        return Err(format!(
            "an image's hotspot {xhot},{yhot} lies outside its {width} × {height} pixels"
        ));
    }
    if width > POINTER_SIDE_MAX || height > POINTER_SIDE_MAX {
        return Ok(None);
    }
    let pixels = at.saturating_add(index(IMAGE_HEADER));
    let end = index(width)
        .saturating_mul(index(height))
        .saturating_mul(4)
        .checked_add(pixels);
    if end.is_none_or(|end| end > bytes.len()) {
        return Err("an image's pixels run past the end of the file".to_owned());
    }
    Ok(Some(Candidate {
        nominal: subtype,
        width,
        height,
        hot: (xhot, yhot),
        pixels,
    }))
}

/// The candidate's picture: its ARGB pixels as premultiplied RGBA, each
/// color lowered to its alpha where it exceeds it.
fn pixels(bytes: &[u8], c: &Candidate) -> Result<PointerPicture, String> {
    let n = index(c.width).saturating_mul(index(c.height));
    let mut rgba = Vec::with_capacity(n.saturating_mul(4));
    for i in 0..n {
        let at = i.saturating_mul(4).saturating_add(c.pixels);
        let p = word(bytes, at).ok_or("an image's pixels run past the end of the file")?;
        let [b, g, r, a] = p.to_le_bytes();
        rgba.extend_from_slice(&[r.min(a), g.min(a), b.min(a), a]);
    }
    Ok(PointerPicture {
        size: c.nominal,
        width: c.width,
        height: c.height,
        hot: c.hot,
        rgba,
    })
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    /// One image of a cursor file being built.
    #[derive(Clone)]
    struct Img {
        nominal: u32,
        width: u32,
        height: u32,
        hot: (u32, u32),
        /// The ARGB value every pixel takes.
        argb: u32,
    }

    fn img(nominal: u32, side: u32) -> Img {
        Img {
            nominal,
            width: side,
            height: side,
            hot: (0, 0),
            argb: 0xff00_00ff,
        }
    }

    fn put(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    /// A cursor file holding `images` in order, after one comment chunk.
    fn file(images: &[Img]) -> Vec<u8> {
        file_with(images, 0)
    }

    /// [`file`] with `junk` more table entries, each of a chunk type of no
    /// interest, between the comment's and the images'.
    fn file_with(images: &[Img], junk: usize) -> Vec<u8> {
        let header = 16_u32;
        let entries = u32::try_from(images.len() + 1 + junk).unwrap();
        let mut at = header + entries * 12;
        let mut out = MAGIC.to_vec();
        put(&mut out, header);
        put(&mut out, 0x1_0000);
        put(&mut out, entries);
        // the comment chunk: 20 bytes of header and a 4-byte string
        put(&mut out, 0xfffe_0001);
        put(&mut out, 1);
        put(&mut out, at);
        at += 24;
        for _ in 0..junk {
            put(&mut out, 0x1234);
            put(&mut out, 0);
            put(&mut out, 0);
        }
        for i in images {
            put(&mut out, IMAGE_TYPE);
            put(&mut out, i.nominal);
            put(&mut out, at);
            at += IMAGE_HEADER + i.width * i.height * 4;
        }
        for v in [20, 0xfffe_0001, 1, 1, 4] {
            put(&mut out, v);
        }
        out.extend_from_slice(b"note");
        for i in images {
            for v in [
                IMAGE_HEADER,
                IMAGE_TYPE,
                i.nominal,
                1,
                i.width,
                i.height,
                i.hot.0,
                i.hot.1,
                16,
            ] {
                put(&mut out, v);
            }
            for _ in 0..i.width * i.height {
                put(&mut out, i.argb);
            }
        }
        out
    }

    fn sizes(pictures: &[PointerPicture]) -> Vec<u32> {
        pictures.iter().map(|p| p.size).collect()
    }

    // A file of three sizes reads whole, smallest first, its pixels as
    // RGBA, and the comment chunk is ignored.
    #[test]
    fn a_file_keeps_each_size_smallest_first() {
        let read = read(&file(&[img(32, 32), img(24, 24), img(48, 48)]), None).unwrap();
        assert_eq!(sizes(&read), [24, 32, 48]);
        let p = read.first().unwrap();
        assert_eq!((p.width, p.height, p.hot), (24, 24, (0, 0)));
        assert_eq!(p.rgba.get(..4), Some(&[0, 0, 0xff, 0xff][..]));
        assert_eq!(p.rgba.len(), 24 * 24 * 4);
    }

    // The kept sizes stop at the pixel budget: four pictures of 64 × 64
    // fill its 16,384 pixels exactly, and a fifth size, or one pixel more,
    // passes it.
    #[test]
    fn the_kept_sizes_stop_at_the_pixel_budget() {
        let four = [img(16, 64), img(17, 64), img(18, 64), img(19, 64)];
        assert_eq!(sizes(&read(&file(&four), None).unwrap()), [16, 17, 18, 19]);
        let mut five = four.to_vec();
        five.push(img(20, 64));
        assert_eq!(sizes(&read(&file(&five), None).unwrap()), [16, 17, 18, 19]);
        // one pixel past the budget leaves out the size that passes it
        let mut over = four.to_vec();
        over.insert(0, img(8, 1));
        assert_eq!(sizes(&read(&file(&over), None).unwrap()), [8, 16, 17, 18]);
    }

    // With a size named, only the nominal size nearest it, and of two
    // equally near, the smaller.
    #[test]
    fn a_named_size_keeps_the_nearest_and_the_smaller_on_a_tie() {
        let f = file(&[img(24, 24), img(30, 30), img(36, 36)]);
        assert_eq!(sizes(&read(&f, Some(32)).unwrap()), [30]);
        assert_eq!(sizes(&read(&f, Some(33)).unwrap()), [30]);
        assert_eq!(sizes(&read(&f, Some(34)).unwrap()), [36]);
        assert_eq!(sizes(&read(&f, Some(8)).unwrap()), [24]);
        assert_eq!(sizes(&read(&f, Some(64)).unwrap()), [36]);
    }

    // A picture past 64 a side drops that size alone; at 64 it stays.
    #[test]
    fn a_picture_past_64_a_side_drops_that_size_alone() {
        let mut wide = img(72, 64);
        wide.width = 65;
        let read = read(&file(&[img(24, 24), wide.clone(), img(64, 64)]), None).unwrap();
        assert_eq!(sizes(&read), [24, 64]);
        assert!(super::read(&file(&[wide]), None).is_err());
    }

    // An image of nominal size 0 is skipped and the other sizes serve; a
    // file of size 0 alone has no picture.
    #[test]
    fn an_image_of_size_0_is_skipped() {
        let read = read(&file(&[img(0, 16), img(24, 24)]), None).unwrap();
        assert_eq!(sizes(&read), [24]);
        assert!(super::read(&file(&[img(0, 16)]), None).is_err());
    }

    // Two images of one size keep the first, a busy cursor's first frame.
    #[test]
    fn two_images_of_one_size_keep_the_first() {
        let mut second = img(24, 24);
        second.argb = 0xff00_ff00;
        let read = read(&file(&[img(24, 24), second]), None).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(
            read.first().unwrap().rgba.get(..4),
            Some(&[0, 0, 0xff, 0xff][..])
        );
    }

    // A color above its alpha is lowered to the alpha.
    #[test]
    fn a_color_above_its_alpha_is_lowered_to_it() {
        let mut i = img(24, 24);
        i.argb = 0x80ff_40ff;
        let read = read(&file(&[i]), None).unwrap();
        assert_eq!(
            read.first().unwrap().rgba.get(..4),
            Some(&[0x80, 0x40, 0x80, 0x80][..])
        );
    }

    // A hotspot on the picture's last pixel passes, and one past it fails.
    #[test]
    fn the_hotspot_must_lie_inside_the_picture() {
        let mut i = img(24, 24);
        i.hot = (23, 23);
        assert_eq!(
            read(&file(&[i.clone()]), None)
                .unwrap()
                .first()
                .unwrap()
                .hot,
            (23, 23)
        );
        for hot in [(24, 0), (0, 24)] {
            i.hot = hot;
            assert!(
                read(&file(&[i.clone()]), None)
                    .unwrap_err()
                    .contains("hotspot")
            );
        }
    }

    // The file's size passes at its limit, by its length alone, and fails
    // one byte past it.
    #[test]
    fn the_file_size_limit() {
        let mut at = file(&[img(24, 24)]);
        at.resize(XCURSOR_FILE_MAX, 0);
        assert!(read(&at, None).is_ok());
        at.push(0);
        assert!(read(&at, None).unwrap_err().contains("16 MiB"));
    }

    // The magic, and the header's length at 16 and one below.
    #[test]
    fn the_magic_and_the_header_length() {
        let mut f = file(&[img(24, 24)]);
        assert!(read(&f, None).is_ok());
        if let Some(b) = f.get_mut(0) {
            *b = b'x';
        }
        assert!(read(&f, None).unwrap_err().contains("Xcur"));
        let mut short = file(&[img(24, 24)]);
        if let Some(b) = short.get_mut(4) {
            *b = 15;
        }
        assert!(read(&short, None).unwrap_err().contains("header"));
    }

    // The table holds at most 4,096 entries, and it lies inside the file.
    #[test]
    fn the_table_of_contents_limits() {
        // the comment, the image and 4,094 entries of no interest
        assert!(read(&file_with(&[img(24, 24)], 4094), None).is_ok());
        let why = read(&file_with(&[img(24, 24)], 4095), None).unwrap_err();
        assert!(why.contains("4097 entries"), "{why}");
        // a table that runs past the end
        let mut f = file(&[img(24, 24)]);
        if let Some(b) = f.get_mut(12) {
            *b = 200;
        }
        assert!(read(&f, None).unwrap_err().contains("table"));
    }

    // A file cut short inside the table, an image's header or its pixels
    // fails each time.
    #[test]
    fn a_file_cut_short_fails() {
        let f = file(&[img(24, 24)]);
        let image = index(word(&f, 16 + 12 + 8).unwrap());
        for cut in [20, 16 + 12 + 6, image + 10, image + 36 + 100, f.len() - 1] {
            assert!(read(f.get(..cut).unwrap(), None).is_err(), "cut at {cut}");
        }
    }

    // Each field of an image's header is checked: its length, its type,
    // its subtype against the table's, its version, and its sides.
    #[test]
    fn each_image_header_field_is_checked() {
        let f = file(&[img(24, 24)]);
        let image = index(word(&f, 16 + 12 + 8).unwrap());
        for (field, value, words) in [
            (0, 40, "36"),
            (1, 0xfffd_0003, "not one"),
            (2, 25, "size 25"),
            (3, 2, "version"),
            (4, 0, "no pixels"),
            (5, 0, "no pixels"),
        ] {
            let mut bad = f.clone();
            let at = image + field * 4;
            if let Some(b) = bad.get_mut(at..at + 4) {
                b.copy_from_slice(&u32::to_le_bytes(value));
            }
            let why = read(&bad, None).unwrap_err();
            assert!(why.contains(words), "field {field}: {why}");
        }
    }

    // A chunk that is not an image is skipped, however it lies.
    #[test]
    fn a_chunk_that_is_not_an_image_is_skipped() {
        let mut f = file(&[img(24, 24)]);
        // the comment's position, past the end
        if let Some(b) = f.get_mut(16 + 8..16 + 12) {
            b.copy_from_slice(&u32::MAX.to_le_bytes());
        }
        assert!(read(&f, None).is_ok());
    }
}
