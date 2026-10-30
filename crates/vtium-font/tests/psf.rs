//! The PSF loader against crafted fonts, each case plain and, with the
//! `gzip` feature, gzipped: the C reference's `test_font_psf.c` cases
//! (aaronsb/kmscon#229), the unicode-table sequence fix (aaronsb/kmscon#231)
//! and vtium's departures. Every font the loader accepts has each of its
//! glyphs drawn. Then every PSF font in kbd's console font directory, when
//! the machine has one, must load, and every other file there must be
//! refused.

mod common;

use common::*;
use vtium_font::{Face, Font, PSF_BYTES_MAX, PSF_ENTRIES_MAX, Psf, PsfError, Style, Width};

/// `bytes` parsed, and its every glyph and a sweep of lookups drawn, as
/// test_font_psf.c's exercise() does.
fn parse(bytes: &[u8]) -> Result<Psf, PsfError> {
    let psf = Psf::parse(bytes)?;
    for i in 0..psf.glyph_count() {
        assert!(psf.render(i, Style::default()).is_some(), "glyph {i}");
    }
    let font = Font::new(psf.clone());
    for c in (0..0x300).filter_map(char::from_u32).chain(['\u{10ffff}']) {
        let g = font.glyph(&[c], Width::Narrow, Style::default());
        assert!(g.is_some(), "U+{:04X}", u32::from(c));
    }
    Ok(psf)
}

/// `bytes` plain and gzipped give `want`.
fn expect(what: &str, bytes: &[u8], loads: bool) {
    let got = parse(bytes);
    assert_eq!(got.is_ok(), loads, "{what}: {:?}", got.err());
    if cfg!(feature = "gzip") {
        let got = parse(&gzip(bytes));
        assert_eq!(got.is_ok(), loads, "{what} (gz): {:?}", got.err());
    }
}

fn with(mut b: Vec<u8>, byte: u8, n: usize) -> Vec<u8> {
    b.resize(b.len() + n, byte);
    b
}

// test_font_psf.c test_header()
// covers: text.psf
#[test]
fn header_bounds() {
    // 16 * 0x10000001 is 16 in 32 bits
    expect(
        "glyph count times glyph size wraps",
        &with(psf2(0, 0x1000_0001, 16, 16, 8), 0xaa, 64 * 16),
        false,
    );
    expect(
        "16M glyphs",
        &with(psf2(0, 0x0100_0000, 16, 16, 8), 0xaa, 16),
        false,
    );
    expect(
        "65536 glyphs",
        &with(psf2(0, 0x10000, 1, 1, 8), 0xaa, 0x10000),
        false,
    );
    expect(
        "65535 glyphs",
        &with(psf2(0, 0xffff, 1, 1, 8), 0xaa, 0xffff),
        true,
    );
    // a row of 32 pixels is 4 bytes, so a 2-row glyph is 8
    expect(
        "glyph size short of width and height",
        &with(psf2(0, 256, 2, 2, 32), 0xaa, 256 * 2),
        false,
    );
    expect(
        "glyph size past width and height",
        &with(psf2(0, 256, 32, 2, 8), 0xaa, 256 * 32),
        false,
    );
    expect(
        "33 pixels wide",
        &with(psf2(0, 256, 2 * 5, 2, 33), 0xaa, 256 * 2 * 5),
        false,
    );
    expect(
        "32 pixels wide",
        &with(psf2(0, 256, 2 * 4, 2, 32), 0xaa, 256 * 2 * 4),
        true,
    );
    expect("0 rows", &psf2(0, 256, 0, 0, 8), false);
    expect("0 pixels wide", &psf2(0, 256, 0, 16, 0), false);
    expect("0 glyphs", &psf2(0, 0, 16, 16, 8), false);
    expect("psf1 of 0 rows", &psf1(0, 0), false);
    expect(
        "257 rows",
        &with(psf2(0, 4, 257, 257, 8), 0xaa, 4 * 257),
        false,
    );
    expect(
        "32x256",
        &with(psf2(0, 4, 256 * 4, 256, 32), 0xaa, 4 * 256 * 4),
        true,
    );
    expect(
        "16-byte header",
        &with(psf2_header(16, 0, 256, 16, 16, 8), 0xaa, 16 + 256 * 16),
        false,
    );
    expect(
        "header size wraps with the glyphs",
        &with(psf2_header(0xffff_fff0, 0, 256, 16, 16, 8), 0xaa, 256 * 16),
        false,
    );
    // inside the byte bound, but past the end of the file
    expect(
        "15 MiB header",
        &with(psf2_header(15 << 20, 0, 256, 16, 16, 8), 0xaa, 256 * 16),
        false,
    );
}

// test_font_psf.c test_header_size(): a 64-byte PSF2 header, glyph i all of
// whose bytes are i starting at 64 + 16i
// covers: text.psf
#[test]
fn glyphs_start_where_the_header_says() {
    let mut b = with(psf2_header(64, 0, 256, 16, 16, 8), 0x55, 32);
    for i in 0..=255u8 {
        b = with(b, i, 16);
    }
    let psf = parse(&b).expect("loads");
    for i in 0..256u32 {
        let g = psf.render(i, Style::default()).expect("a glyph");
        for (y, row) in g.rows().enumerate() {
            for (x, &px) in row.iter().enumerate() {
                let on = i >> (7 - x) & 1 != 0;
                assert_eq!(px, if on { 0xff } else { 0 }, "glyph {i} at {x},{y}");
            }
        }
    }
}

// setfont(8): a raw font is 256*N bytes, one byte a row, 8 pixels across,
// 0 < N <= 32. The C reference read no raw font.
// covers: text.psf
#[test]
fn a_raw_font_is_256_glyphs_of_one_byte_rows() {
    // glyph i is N rows, row y holding i + y
    for height in [1usize, 8, 14, 16, 32] {
        let mut b = Vec::new();
        for i in 0..=255u8 {
            for y in 0..height {
                b.push(i.wrapping_add(y as u8));
            }
        }
        let psf = parse(&b).expect("loads");
        assert_eq!(psf.glyph_count(), 256);
        assert_eq!(psf.glyph_size(), (8, height as u32));
        // a raw font holds no unicode table
        assert!(!psf.has_table());
        for i in [0u32, 1, 0x41, 0x80, 255] {
            let g = psf.render(i, Style::default()).expect("a glyph");
            for (y, row) in g.rows().enumerate() {
                let byte = (i as u8).wrapping_add(y as u8);
                for (x, &px) in row.iter().enumerate() {
                    let on = byte >> (7 - x) & 1 != 0;
                    assert_eq!(
                        px,
                        if on { 0xff } else { 0 },
                        "{height}: glyph {i} at {x},{y}"
                    );
                }
            }
        }
        if cfg!(feature = "gzip") {
            assert_eq!(
                parse(&gzip(&b)).expect("gz").glyph_size(),
                (8, height as u32)
            );
        }
    }
}

// a size that is not 256 times a height of 1 to 32 is no raw font
// covers: text.psf
#[test]
fn a_file_of_no_raw_size_is_refused() {
    for len in [1usize, 255, 257, 256 * 33, 256 * 32 + 1, 256 * 64, 4095] {
        let b = vec![0xaa; len];
        let e = Psf::parse(&b).expect_err("not a font");
        assert!(matches!(e, PsfError::NotPsf), "{len}: {e}");
    }
    // gzipped, its size past the bound
    if cfg!(feature = "gzip") {
        let e = Psf::parse(&gzip(&vec![0xaa; 256 * 33])).expect_err("not a font");
        assert!(matches!(e, PsfError::NotPsf), "{e}");
    }
    // an empty file has no header to read
    assert!(matches!(Psf::parse(&[]), Err(PsfError::Truncated)));
    // the edges either side of the bound
    assert!(Psf::parse(&vec![0xaa; 256 * 32]).is_ok());
    assert!(Psf::parse(&vec![0xaa; 256]).is_ok());
}

// test_font_psf.c test_short()
// covers: text.psf
#[test]
fn files_shorter_than_their_header_say() {
    expect(
        "psf1 shorter than its glyphs",
        &with(psf1(0x01, 16), 0xaa, 100),
        false,
    );
    expect(
        "psf2 shorter than its glyphs",
        &with(psf2(0, 256, 16, 16, 8), 0xaa, 256 * 16 - 1),
        false,
    );
    expect(
        "psf2 header cut short",
        &psf2(0, 256, 16, 16, 8)[..20],
        false,
    );
    expect("psf1 header cut short", &[0x36, 0x04, 0x00], false);
    expect("one byte", &[0x36], false);
    expect("empty file", &[], false);
    assert!(matches!(
        Psf::parse(&[0x72, 0xb5]),
        Err(PsfError::Truncated)
    ));
    assert!(matches!(Psf::parse(b"not a font"), Err(PsfError::NotPsf)));
}

// test_font_psf.c test_small(): a missing character draws from glyph 0 in
// a font too small to hold '?' (63)
// covers: text.psf
#[test]
fn a_font_without_question_mark_draws_glyph_0() {
    let mut b = psf2(0, 1, 16, 16, 8);
    b.extend([0x81; 16]);
    expect("one glyph", &b, true);
    let psf = Psf::parse(&b).expect("loads");
    let glyph0 = psf.render(0, Style::default());
    let font = Font::new(psf);
    assert!(!font.face().has_glyph('Z'));
    assert_eq!(font.glyph(&['Z'], Width::Narrow, Style::default()), glyph0);
}

// test_font_psf.c test_table()
// covers: text.psf
#[test]
fn table_bounds() {
    let cap = PSF_ENTRIES_MAX as usize;
    expect(
        "table at the entry cap",
        &with(psf2_with_table(), b'A', cap),
        true,
    );
    let past = with(psf2_with_table(), b'A', cap + 1);
    expect("table past the entry cap", &past, false);
    assert!(matches!(Psf::parse(&past), Err(PsfError::TableTooLarge)));
    // terminators only: no entries, so only the byte bound ends it
    let big = with(psf2_with_table(), 0xff, PSF_BYTES_MAX as usize);
    expect("file past the byte cap", &big, false);
    assert!(matches!(Psf::parse(&big), Err(PsfError::TooLarge)));
    expect(
        "psf1 table past the entry cap",
        &with(with(psf1(0x02, 16), 0xaa, 256 * 16), b'A', 2 * (cap + 1)),
        false,
    );
}

// test_font_psf.c test_split(): the C reference read the table 1024 bytes
// at a time; vtium reads it whole, and keeps the cases
// covers: text.psf
#[test]
fn utf8_at_the_table_edges() {
    let mut b = with(with(psf2_with_table(), b'B', 1022), 0xff, 1);
    b.extend([0xc3, 0xa9, 0xff]);
    let psf = parse(&b).expect("loads");
    assert_eq!(psf.glyph_index('é'), Some(1));
    assert_eq!(psf.glyph_index('B'), Some(0));
    // the lead byte of a 4-byte sequence, then the end of the file
    let mut b = with(with(psf2_with_table(), b'B', 1022), 0xff, 1);
    b.push(0xf0);
    expect("UTF-8 cut off by the end", &b, true);
}

// aaronsb/kmscon#231: a start-of-sequence mark opens the glyph's sequences,
// which run to its terminator. The C reference took the mark for a
// terminator, so every later glyph was mapped one off per sequence.
// covers: text.psf
#[test]
fn a_sequence_does_not_shift_later_glyphs() {
    // glyph 0: 'A', then the sequence 'B' U+0301; glyph 1: 'D'; glyph 2: 'E'
    let mut b = psf1_with_table();
    b.extend(units(&[
        0x41, 0xfffe, 0x42, 0x301, 0xffff, 0x44, 0xffff, 0x45, 0xffff,
    ]));
    let psf = parse(&b).expect("psf1 loads");
    assert_eq!(psf.glyph_index('A'), Some(0));
    assert_eq!(psf.glyph_index('D'), Some(1));
    assert_eq!(psf.glyph_index('E'), Some(2));
    assert_eq!(
        psf.glyph_index('B'),
        None,
        "a sequence member is no mapping"
    );
    assert_eq!(psf.glyph_index('\u{301}'), None);

    let mut b = psf2_with_table();
    b.extend(b"A\xfeB\xcc\x81\xfeC\xffD\xffE\xff");
    let psf = parse(&b).expect("psf2 loads");
    assert_eq!(psf.glyph_index('A'), Some(0));
    assert_eq!(psf.glyph_index('D'), Some(1));
    assert_eq!(psf.glyph_index('E'), Some(2));
    assert_eq!(psf.glyph_index('B'), None);
    assert_eq!(psf.glyph_index('C'), None);
}

// Departure: a PSF2 terminator ends the glyph's entry even inside a UTF-8
// sequence; the C reference read it as the sequence's continuation byte.
// covers: text.psf
#[test]
fn a_terminator_ends_a_broken_utf8_sequence() {
    let mut b = psf2_with_table();
    // glyph 0: a 3-byte lead and its terminator; glyph 1: 'X'; glyph 2: a
    // stray continuation byte, then 'Y'
    b.extend(b"\xe0\xffX\xff\x80Y\xff");
    let psf = parse(&b).expect("loads");
    assert_eq!(psf.glyph_index('X'), Some(1));
    assert_eq!(psf.glyph_index('Y'), Some(2));
    // an overlong encoding of 'Z' maps nothing
    let mut b = psf2_with_table();
    b.extend(b"\xc1\x9a\xff");
    assert_eq!(parse(&b).expect("loads").glyph_index('Z'), None);
}

// Departure: where the table lists a codepoint for several glyphs, the last
// listed wins, as the Linux console loads it; the C reference's binary
// search found any of them.
// covers: text.psf
#[test]
fn a_codepoint_listed_twice_maps_to_the_last() {
    let mut b = psf2_with_table();
    b.extend(b"Q\xffQ\xffR\xffQ\xff");
    assert_eq!(parse(&b).expect("loads").glyph_index('Q'), Some(3));
    let mut b = psf1_with_table();
    b.extend(units(&[0x51, 0xffff, 0x51, 0xffff]));
    assert_eq!(parse(&b).expect("loads").glyph_index('Q'), Some(1));
}

// Departure: entries past the last glyph are ignored; the C reference's
// 16-bit glyph counter wrapped after 65536 terminators and mapped later
// entries onto the first glyphs.
// covers: text.psf
#[test]
fn entries_past_the_last_glyph_map_nothing() {
    let mut b = with(psf2(1, 2, 16, 16, 8), 0xaa, 2 * 16);
    b.extend(b"a\xffb\xffc\xff");
    let psf = parse(&b).expect("loads");
    assert_eq!(psf.glyph_index('b'), Some(1));
    assert_eq!(psf.glyph_index('c'), None);

    let mut b = with(psf1(0x02, 16), 0xaa, 256 * 16);
    let mut table = vec![0xffff; 0x10000];
    table.push(u16::from(b'w'));
    b.extend(units(&table));
    assert_eq!(parse(&b).expect("loads").glyph_index('w'), None);
}

// Departure: with a table, the fallback '?' is looked up as a character;
// the C reference drew glyph 63 whatever the table said.
// covers: text.psf
#[test]
fn the_fallback_question_mark_is_looked_up() {
    let mut b = psf2(1, 2, 16, 16, 8);
    b.extend([0x00; 16]);
    b.extend([0xff; 16]);
    b.extend(b"a\xff?\xff");
    let psf = parse(&b).expect("loads");
    let question = psf.render(1, Style::default());
    let font = Font::new(psf);
    assert_eq!(
        font.glyph(&['z'], Width::Narrow, Style::default()),
        question
    );
}

#[test]
fn a_font_without_a_table_maps_codepoints_to_indices() {
    let b = with(psf2(0, 256, 16, 16, 8), 0x0f, 256 * 16);
    let psf = parse(&b).expect("loads");
    assert!(!psf.has_table());
    assert_eq!(psf.glyph_index('A'), Some(65));
    assert_eq!(psf.glyph_index('\u{100}'), None);
    // with no U+25A1, a missing character draws glyph 63, '?'
    let font = Font::new(psf.clone());
    let want = psf.render(63, Style::default());
    assert_eq!(
        font.glyph(&['\u{100}'], Width::Narrow, Style::default()),
        want
    );
}

#[test]
fn psf1_mode_bits() {
    // 512 glyphs, no table
    let psf = parse(&with(psf1(0x01, 8), 0, 512 * 8)).expect("loads");
    assert_eq!((psf.glyph_count(), psf.has_table()), (512, false));
    // the sequence flag alone carries a table, as in the C reference
    let mut b = with(psf1(0x04, 8), 0, 256 * 8);
    b.extend(units(&[0x263a, 0xffff]));
    let psf = parse(&b).expect("loads");
    assert_eq!(psf.glyph_index('☺'), Some(0));
    // an odd byte at the table's end is dropped
    let mut b = with(psf1(0x02, 8), 0, 256 * 8);
    b.extend(units(&[0x41, 0xffff]));
    b.push(0x42);
    assert_eq!(parse(&b).expect("loads").glyph_index('A'), Some(0));
}

#[test]
fn scaling() {
    let mut psf = parse(&with(psf2(0, 256, 16, 16, 8), 0x80, 256 * 16)).expect("loads");
    for (height, scale) in [(0, 1), (15, 1), (16, 1), (31, 1), (32, 2), (40, 2), (48, 3)] {
        psf.set_height(height);
        assert_eq!(psf.scale(), scale, "asked for {height}");
    }
    psf.set_height(u32::MAX);
    assert_eq!(psf.scale(), 128, "the cell stays inside CELL_SIDE_MAX");
    let g = psf.render(0, Style::default()).expect("a glyph");
    assert_eq!((g.width(), g.height()), (1024, 2048));
    assert_eq!(psf.size_step(), 16);
}

#[test]
fn bold_widens_each_row_by_a_pixel() {
    let mut b = psf2(0, 1, 4, 2, 12);
    b.extend([0b1000_0000, 0b0001_0000, 0, 0]);
    let psf = parse(&b).expect("loads");
    let bold = psf
        .render(
            0,
            Style {
                bold: true,
                italic: false,
            },
        )
        .expect("a glyph");
    let row: Vec<u8> = bold.rows().next().expect("a row").to_vec();
    let on: Vec<usize> = (0..12).filter(|&x| row[x] != 0).collect();
    assert_eq!(on, [0, 1, 11]);
}

#[cfg(feature = "gzip")]
mod gz {
    use super::*;
    use vtium_font::GZIP_BYTES_MAX;

    fn font() -> Vec<u8> {
        with(psf2_with_table(), b'A', 0x10000)
    }

    // test_font_psf.c test_gz_corrupt()
    // covers: text.psf
    #[test]
    fn a_bad_checksum_is_refused() {
        let mut gz = gzip(&font());
        let at = gz.len() - 8;
        gz[at] ^= 0xff;
        assert!(matches!(Psf::parse(&gz), Err(PsfError::Gzip(_))));
    }

    #[test]
    fn a_bad_length_is_refused() {
        let mut gz = gzip(&font());
        let at = gz.len() - 4;
        gz[at] ^= 1;
        assert!(matches!(Psf::parse(&gz), Err(PsfError::Gzip(_))));
    }

    // covers: text.psf
    #[test]
    fn malformed_framing_is_refused() {
        let gz = gzip(&font());
        for cut in [3, 10, 20, gz.len() / 2, gz.len() - 1] {
            assert!(Psf::parse(&gz[..cut]).is_err(), "cut at {cut}");
        }
        let mut bad = gz.clone();
        bad[2] = 7;
        assert!(Psf::parse(&bad).is_err(), "method 7");
        let mut bad = gz.clone();
        bad[3] = 0x20;
        assert!(Psf::parse(&bad).is_err(), "a reserved flag");
        let mut bad = gz.clone();
        bad.extend(b"trailing");
        assert!(Psf::parse(&bad).is_err(), "bytes after the member");
        let mut bad = gz.clone();
        bad[12] ^= 0x55;
        assert!(Psf::parse(&bad).is_err(), "corrupt deflate data");
        // FNAME with no terminating NUL
        let bad = [0x1f, 0x8b, 8, 8, 0, 0, 0, 0, 0, 3, b'x', b'y'];
        assert!(Psf::parse(&bad).is_err(), "unterminated name");
    }

    #[test]
    fn optional_header_fields_and_members() {
        let f = font();
        // FEXTRA, FNAME, FCOMMENT and FHCRC, in RFC 1952's order
        let mut fields = vec![3, 0, b'a', b'b', b'c'];
        fields.extend(b"name\0comment\0");
        fields.extend([0x12, 0x34]);
        let gz = gzip_with(&f, 2 | 4 | 8 | 16, &fields);
        assert_eq!(Psf::parse(&gz).expect("loads").glyph_index('A'), Some(0));
        // two members, the font cut between them
        let (a, b) = f.split_at(1000);
        let mut gz = gzip(a);
        gz.extend(gzip(b));
        assert_eq!(Psf::parse(&gz).expect("loads").glyph_index('A'), Some(0));
    }

    /// `n` gzip members of nothing, 20 bytes each.
    fn empty_members(n: usize) -> Vec<u8> {
        let member = gzip(&[]);
        assert_eq!(member.len(), 20);
        member.repeat(n)
    }

    // A file of many small members costs one decompressor, not one each:
    // at the gzip bound, 52,428 empty members inflate to nothing, refused
    // as no font, in bounded time. Before vtium reused the decompressor, a
    // 16 MiB file of them took 1.32 s to refuse in a release build.
    // covers: text.psf
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test bounds the time a refusal takes; no state depends on it"
    )]
    fn many_empty_members_are_refused_in_bounded_time() {
        let at = empty_members(GZIP_BYTES_MAX as usize / 20);
        let t = std::time::Instant::now();
        assert!(matches!(Psf::parse(&at), Err(PsfError::Truncated)));
        assert!(
            t.elapsed().as_secs() < 10,
            "{:?} in a debug build",
            t.elapsed()
        );
    }

    // The inflater builds a Huffman table for every deflate block, and a
    // block can be 10 bits, so the compressed size bounds the work: a
    // gzipped file past GZIP_BYTES_MAX is refused before it is inflated.
    // covers: text.psf
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test bounds the time a refusal takes; no state depends on it"
    )]
    fn a_gzip_file_past_its_bound_is_refused_unread() {
        let past = empty_members(PSF_BYTES_MAX as usize / 20);
        let t = std::time::Instant::now();
        assert!(matches!(Psf::parse(&past), Err(PsfError::TooLarge)));
        assert!(t.elapsed().as_millis() < 100, "{:?}", t.elapsed());
        let mut over = empty_members(GZIP_BYTES_MAX as usize / 20);
        over.resize(GZIP_BYTES_MAX as usize + 1, 0);
        assert!(matches!(Psf::parse(&over), Err(PsfError::TooLarge)));
    }

    // covers: text.psf
    #[test]
    fn a_file_inflating_past_the_byte_bound_is_refused() {
        let bomb = gzip(&vec![0u8; PSF_BYTES_MAX as usize + 1]);
        assert!(bomb.len() < 64 * 1024);
        assert!(matches!(Psf::parse(&bomb), Err(PsfError::TooLarge)));
        let mut font = with(psf2_with_table(), 0xff, 1);
        font.resize(PSF_BYTES_MAX as usize, 0xff);
        assert!(Psf::parse(&gzip(&font)).is_ok(), "at the bound");
    }
}

#[cfg(not(feature = "gzip"))]
#[test]
fn a_gzipped_font_needs_the_feature() {
    let gz = [0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    let err = Psf::parse(&gz).expect_err("refused");
    assert!(matches!(err, PsfError::GzipUnsupported));
    assert!(err.to_string().contains("gzip feature"));
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

// test_font_psf.c test_fixture(): psf-test.psf, glyph G's row R holding
// G + R
// covers: text.psf
#[test]
fn the_fixture_loads() {
    let check = |psf: Psf| {
        assert_eq!(psf.glyph_size(), (8, 16));
        assert_eq!(psf.glyph_count(), 256);
        let a = psf.render(u32::from(b'A'), Style::default()).expect("A");
        let row: Vec<u8> = a.rows().nth(3).expect("row 3").to_vec();
        let byte = (0..8).fold(0u8, |acc, x| acc << 1 | u8::from(row[x] != 0));
        assert_eq!(byte, b'A' + 3);
    };
    let psf = Psf::load(fixture("psf-test.psf")).expect("loads");
    // Debug shows the font's shape, never its glyph bytes
    assert_eq!(
        format!("{psf:?}"),
        "Psf { glyphs: 256, size: (8, 16), scale: 1, table: None }"
    );
    check(psf);
    let gz = Psf::load(fixture("psf-test.psf.gz"));
    if cfg!(feature = "gzip") {
        check(gz.expect("loads"));
    } else {
        assert!(matches!(gz, Err(PsfError::GzipUnsupported)));
    }
    assert!(matches!(
        Psf::load(fixture("no-such-font")),
        Err(PsfError::Io(_))
    ));
}

// The Terminus fixtures: PSF1 and PSF2 fonts with unicode tables, as kbd
// ships them
#[cfg(feature = "gzip")]
// covers: text.psf
#[test]
fn the_terminus_fixtures_load() {
    for (name, size, count) in [
        ("ter-v16n.psf.gz", (8, 16), 512),
        ("ter-v22b.psf.gz", (11, 22), 512),
        ("ter-v32n.psf.gz", (16, 32), 512),
    ] {
        let psf = Psf::load(fixture(name)).expect(name);
        assert_eq!(
            (psf.glyph_size(), psf.glyph_count()),
            (size, count),
            "{name}"
        );
        assert!(psf.has_table(), "{name}");
        for c in ['A', 'z', 'é', 'Ж', '─', '█'] {
            assert!(psf.has_glyph(c), "{name}: {c}");
        }
        // the font's own box drawing glyph is not what the font draws
        assert!(vtium_font::sprite::has('─'));
    }
}

/// The positive control: every PSF font kbd ships loads, and loads again
/// gunzipped when it ships gzipped; every other file there is refused.
// covers: text.psf
#[test]
fn every_kbd_console_font_loads() {
    // the files kbd ships that are no PSF and load as raw fonts by their
    // size alone; one a distribution adds is named here when it is seen
    const RAW_KBD_FONTS: [&str; 17] = [
        "alt-8x14.gz",
        "alt-8x16.gz",
        "alt-8x8.gz",
        "altc-8x16.gz",
        "arm8.fnt.gz",
        "cp857.08.gz",
        "cp857.14.gz",
        "cp857.16.gz",
        "cybercafe.fnt.gz",
        "koi8c-8x16.gz",
        "koi8r-8x14.gz",
        "koi8r-8x16.gz",
        "koi8r-8x8.gz",
        "Mik_8x16.gz",
        "README.Arabic",
        "t850b.fnt.gz",
        "t.fnt.gz",
    ];
    let dir = "/usr/share/kbd/consolefonts";
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("no {dir}: positive control skipped");
        return;
    };
    let (mut loaded, mut refused) = (0, 0);
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let raw = std::fs::read(&path).expect("read");
        let plain = if raw.starts_with(&[0x1f, 0x8b]) {
            gunzip(&raw).unwrap_or_default()
        } else {
            raw.clone()
        };
        let psf = plain.starts_with(&[0x36, 0x04]) || plain.starts_with(&[0x72, 0xb5, 0x4a, 0x86]);
        let got = Psf::load(&path);
        if !cfg!(feature = "gzip") && raw.starts_with(&[0x1f, 0x8b]) {
            assert!(got.is_err(), "{}", path.display());
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let raw_font = RAW_KBD_FONTS.contains(&name);
        assert_eq!(
            got.is_ok(),
            psf || raw_font,
            "{}: {:?}",
            path.display(),
            got.err()
        );
        if psf || raw_font {
            loaded += 1;
            assert!(Psf::parse(&plain).is_ok(), "{} gunzipped", path.display());
        } else {
            refused += 1;
        }
    }
    eprintln!("{dir}: {loaded} PSF fonts loaded, {refused} other files refused");
}
