//! Vitrified sets (ADR-1200 D2, D10, D13): a set written and read back,
//! the same inputs giving the same bytes, deflated and raw chunks drawing
//! alike, the four PSF fixtures drawn through their sets to the C
//! reference's digests, and every refusal of the format's reader.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::{assemble, sections, with_section};
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetError, SetFace, StrikeBuilder};
use vtium_font::{
    Cell, Coverage, Decorations, Face, Font, Glyph, Psf, Sha256, Source, SpritePolicy, Style,
    Width, sha256_hex, sprite::Range,
};

const BOLD: Style = Style {
    bold: true,
    italic: false,
};

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// A PSF font's glyph `i` as 1-bit rows, padded to a byte, first pixel
/// high: the strike's stored form, from the `Psf` draw at its own size.
fn psf_bits(psf: &Psf, i: u32) -> Vec<u8> {
    let g = psf.render(i, Style::default()).expect("a glyph");
    let row = g.width().div_ceil(8) as usize;
    let mut out = Vec::new();
    for line in g.rows() {
        let mut bytes = vec![0u8; row];
        for (x, &px) in line.iter().enumerate() {
            if px != 0 {
                bytes[x / 8] |= 0x80 >> (x % 8);
            }
        }
        out.extend(bytes);
    }
    out
}

/// `psf` vitrified as `vtium-vitrify` will: one 1-bit strike at its own
/// size, every codepoint it draws mapped, glyphs shared as the font shares
/// them, and the policy the font gets by default.
fn vitrify_psf(name: &str, psf: &Psf) -> Vec<u8> {
    let (w, h) = psf.glyph_size();
    let cell = Cell {
        width: w,
        height: h,
    };
    let mut strike = StrikeBuilder::new(PixelFormat::Bits, cell, Style::default());
    let mut ids = BTreeMap::new();
    for c in (0..=0x10_ffffu32).filter_map(char::from_u32) {
        if !psf.has_glyph(c) {
            continue;
        }
        let i = psf.glyph_index(c).expect("an index");
        let id = *ids
            .entry(i)
            .or_insert_with(|| strike.glyph(Width::Narrow, psf_bits(psf, i)));
        strike.map(c, id);
    }
    let policy = SpritePolicy::default_for(&Coverage::new(|r| psf.coverage(r)));
    SetBuilder::new(name)
        .meta("source", &format!("file:{name}"))
        .policy(policy)
        .strike(strike)
        .write()
        .expect("the set writes")
}

/// A small set of two cell sizes: a 1-bit 4x6 regular strike with a bold
/// one, and an R8 6x8 strike with wide glyphs, `compress` asking for its
/// chunks to be deflated.
fn small_set(compress: bool) -> Vec<u8> {
    let cell = Cell {
        width: 4,
        height: 6,
    };
    let mut regular = StrikeBuilder::new(PixelFormat::Bits, cell, Style::default())
        .baseline(5)
        .decorations(Decorations {
            underline: 5,
            underline_thickness: 1,
            strike: 3,
            strike_thickness: 1,
        });
    let a = regular.glyph(Width::Narrow, vec![0x60, 0x90, 0xf0, 0x90, 0x90, 0x00]);
    let b = regular.glyph(Width::Narrow, vec![0xe0, 0x90, 0xe0, 0x90, 0xe0, 0x00]);
    let unused = regular.glyph(Width::Narrow, vec![0xff; 6]);
    let _ = unused;
    let wide = regular.glyph(Width::Wide, vec![0xff, 0x81, 0x81, 0x81, 0xff, 0x00]);
    regular.map('A', a);
    regular.map('B', b);
    regular.map('\u{391}', a); // Greek Alpha shares A's glyph
    regular.map('\u{4e00}', wide);
    regular.map('\u{2500}', b);
    let mut bold = StrikeBuilder::new(PixelFormat::Bits, cell, BOLD);
    let ab = bold.glyph(Width::Narrow, vec![0x70, 0xd8, 0xf8, 0xd8, 0xd8, 0x00]);
    bold.map('A', ab);

    let big = Cell {
        width: 6,
        height: 8,
    };
    let mut r8 = StrikeBuilder::new(PixelFormat::R8, big, Style::default()).compress(compress);
    for (i, c) in ('a'..='z').enumerate() {
        let px: Vec<u8> = (0..48).map(|p| ((p * 7 + i * 13) % 256) as u8).collect();
        let g = r8.glyph(Width::Narrow, px);
        r8.map(c, g);
    }
    let w: Vec<u8> = (0..96).map(|p| (p % 3 * 100) as u8).collect();
    let g = r8.glyph(Width::Wide, w);
    r8.map('\u{4e01}', g);
    SetBuilder::new("small-1.0")
        .meta("build-key", &"0".repeat(64))
        .meta("recipe", "source=file:/a.psf")
        .meta("recipe", "size=8")
        .meta("license", "OFL-1.1")
        .policy(SpritePolicy::SPRITES.with(Range::Braille, Source::Auto))
        .strike(r8)
        .strike(regular)
        .strike(bold)
        .write()
        .expect("the set writes")
}

fn read(bytes: Vec<u8>) -> Set {
    Set::from_bytes(bytes).expect("the set reads")
}

fn refused(bytes: Vec<u8>) -> SetError {
    Set::from_bytes(bytes).expect_err("the set is refused")
}

fn mask(w: u32, h: u32, px: &[u8]) -> Glyph {
    Glyph::from_pixels(w, h, vtium_font::Format::Mask, px.to_vec()).expect("a mask")
}

/// 1-bit rows as a mask of 0 and 255.
fn expanded(w: u32, rows: &[u8]) -> Vec<u8> {
    rows.iter()
        .flat_map(|&b| (0..w).map(move |x| if b & (0x80 >> x) != 0 { 0xff } else { 0 }))
        .collect()
}

// A set written and read back holds every field (ADR-1200 D10)
#[test]
fn a_set_reads_back_every_field() {
    let set = read(small_set(true));
    assert_eq!(set.name(), "small-1.0");
    assert_eq!(set.version(), (1, 0));
    let meta: Vec<_> = set.meta().collect();
    assert_eq!(
        meta,
        [
            ("name", "small-1.0"),
            ("format", "1.0"),
            (
                "build-key",
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
            ("recipe", "source=file:/a.psf"),
            ("recipe", "size=8"),
            ("license", "OFL-1.1"),
        ]
    );
    assert_eq!(
        set.meta_values("recipe").collect::<Vec<_>>(),
        ["source=file:/a.psf", "size=8"]
    );
    assert_eq!(
        set.policy(),
        SpritePolicy::SPRITES.with(Range::Braille, Source::Auto)
    );
    let strikes: Vec<_> = set.strikes().collect();
    // sorted by cell height, width and style
    assert_eq!(strikes.len(), 3);
    let (r, b, big) = (strikes[0], strikes[1], strikes[2]);
    assert_eq!(
        (r.cell.width, r.cell.height, r.style),
        (4, 6, Style::default())
    );
    assert_eq!((b.cell.width, b.cell.height, b.style), (4, 6, BOLD));
    assert_eq!((big.cell.width, big.cell.height), (6, 8));
    assert_eq!(r.format, PixelFormat::Bits);
    assert_eq!(big.format, PixelFormat::R8);
    assert_eq!(r.baseline, 5);
    assert_eq!(big.baseline, 8);
    assert_eq!(
        r.decorations,
        Decorations {
            underline: 5,
            underline_thickness: 1,
            strike: 3,
            strike_thickness: 1
        }
    );
    // the unused glyph is dropped, the shared one kept once
    assert_eq!(r.glyph_count, 3);
    assert_eq!(b.glyph_count, 1);
    assert_eq!(big.glyph_count, 27);
    assert_eq!(
        set.cells(),
        [
            Cell {
                width: 4,
                height: 6
            },
            Cell {
                width: 6,
                height: 8
            }
        ]
    );

    let set = Arc::new(set);
    let face = SetFace::new(set.clone(), 6).expect("a face");
    assert_eq!(
        face.cell(),
        Cell {
            width: 4,
            height: 6
        }
    );
    assert!(face.has_glyph('A') && face.has_glyph('\u{391}') && face.has_glyph('\u{4e00}'));
    assert!(!face.has_glyph('C') && !face.has_glyph('a'));
    let a = face.draw(&['A'], Style::default()).expect("A");
    assert_eq!(
        a,
        mask(4, 6, &expanded(4, &[0x60, 0x90, 0xf0, 0x90, 0x90, 0x00]))
    );
    assert_eq!(face.draw(&['\u{391}'], Style::default()), Some(a));
    // the bold strike draws bold A; bold B is widened from the regular
    let bold_a = face.draw(&['A'], BOLD).expect("bold A");
    assert_eq!(
        bold_a,
        mask(4, 6, &expanded(4, &[0x70, 0xd8, 0xf8, 0xd8, 0xd8, 0x00]))
    );
    let bold_b = face.draw(&['B'], BOLD).expect("bold B");
    assert_eq!(
        bold_b,
        mask(4, 6, &expanded(4, &[0xf0, 0xd8, 0xf0, 0xd8, 0xf0, 0x00]))
    );
    // a wide glyph spans two cells, its rows a byte across
    let wide = face.draw(&['\u{4e00}'], Style::default()).expect("wide");
    assert_eq!(wide.cells(), Width::Wide);
    assert_eq!(
        wide,
        mask(8, 6, &expanded(8, &[0xff, 0x81, 0x81, 0x81, 0xff, 0x00])).with_cells(Width::Wide)
    );
    // the coverage comes from POLY, and is the map's
    assert_eq!(face.coverage(Range::BoxDrawing).covered, 1);
    assert_eq!(face.coverage(Range::BoxDrawing).total, 128);
    assert_eq!(face.sprite_policy(), set.policy());

    let r8 = SetFace::new(set.clone(), 8).expect("a face");
    assert_eq!(
        r8.cell(),
        Cell {
            width: 6,
            height: 8
        }
    );
    let g = r8.draw(&['c'], Style::default()).expect("c");
    let want: Vec<u8> = (0..48).map(|p| ((p * 7 + 2 * 13) % 256) as u8).collect();
    assert_eq!(g, mask(6, 8, &want));
    let w: Vec<u8> = (0..96).map(|p| (p % 3 * 100) as u8).collect();
    assert_eq!(
        r8.draw(&['\u{4e01}'], Style::default()),
        Some(mask(12, 8, &w).with_cells(Width::Wide))
    );
}

// The same inputs give the same bytes, whatever order the glyphs were
// mapped in, and a raw set's bytes are pinned (ADR-1200 D2)
#[test]
fn the_same_inputs_give_the_same_bytes() {
    assert_eq!(small_set(true), small_set(true));
    assert_eq!(small_set(false), small_set(false));
    let cell = Cell {
        width: 8,
        height: 2,
    };
    let build = |order: &[char]| {
        let mut s = StrikeBuilder::new(PixelFormat::Bits, cell, Style::default());
        for &c in order {
            let g = s.glyph(Width::Narrow, vec![c as u8, !(c as u8)]);
            s.map(c, g);
        }
        SetBuilder::new("pin").strike(s).write().expect("writes")
    };
    let one = build(&['x', 'y', 'z', 'a']);
    assert_eq!(one, build(&['a', 'z', 'y', 'x']));
    // the format's bytes, pinned: a change to the layout moves this digest
    assert_eq!(
        sha256_hex(&Sha256::digest(&one)),
        "3b3cbaf7772b822001e9c99a5f4eb84c2e228f3af7b9ed0b254c30c1e5d9c714"
    );
}

// compress=yes and compress=no draw every glyph identically, and the
// deflated set is smaller (ADR-1200 D1, D10, D13)
#[test]
fn deflated_and_raw_chunks_draw_alike() {
    let packed = small_set(true);
    let raw = small_set(false);
    assert!(
        packed.len() < raw.len(),
        "{} >= {}",
        packed.len(),
        raw.len()
    );
    let (packed, raw) = (Arc::new(read(packed)), Arc::new(read(raw)));
    packed.check_chunks().expect("checks");
    raw.check_chunks().expect("checks");
    for height in [6, 8, 16] {
        let (p, r) = (
            SetFace::new(packed.clone(), height).expect("a face"),
            SetFace::new(raw.clone(), height).expect("a face"),
        );
        for c in (0..0x5000u32).filter_map(char::from_u32) {
            for style in [Style::default(), BOLD] {
                assert_eq!(p.draw(&[c], style), r.draw(&[c], style), "{c:?}");
            }
        }
    }
}

// Only the chunks drawn from are held (ADR-1200 D3)
#[test]
fn only_chunks_drawn_from_are_held() {
    let cell = Cell {
        width: 16,
        height: 16,
    };
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default());
    // 256 bytes a glyph: 64 to a chunk
    for c in (0x4e00..0x4e00 + 640u32).filter_map(char::from_u32) {
        let g = s.glyph(Width::Narrow, vec![(u32::from(c) % 251) as u8; 256]);
        s.map(c, g);
    }
    let set = Arc::new(read(
        SetBuilder::new("many").strike(s).write().expect("writes"),
    ));
    let face = SetFace::new(set.clone(), 16).expect("a face");
    assert_eq!(set.loaded_chunks(), 0);
    face.draw(&['\u{4e00}'], Style::default()).expect("draws");
    face.draw(&['\u{4e3f}'], Style::default()).expect("draws");
    assert_eq!(set.loaded_chunks(), 1);
    face.draw(&['\u{4e40}'], Style::default()).expect("draws");
    assert_eq!(set.loaded_chunks(), 2);
}

// Every PSF fixture drawn through its vitrified set is the Psf draw, glyph
// for glyph at 1, 2 and 3 times its height, plain and bold, and its
// coverage and policy are the font's: 1-bit strikes expand at load to the
// mask the Psf draw gives (ADR-1200 D10, D13)
#[test]
fn psf_fixtures_draw_alike_through_their_sets() {
    for name in [
        "psf-test.psf",
        "ter-v16n.psf.gz",
        "ter-v22b.psf.gz",
        "ter-v32n.psf.gz",
    ] {
        // a build without gzip refuses the gzipped fixtures
        if name.ends_with(".gz") && !cfg!(feature = "gzip") {
            continue;
        }
        let psf = Psf::load(fixture(name)).expect("the fixture loads");
        let set = Arc::new(read(vitrify_psf("fixture", &psf)));
        let h = psf.glyph_size().1;
        for scale in 1..=3 {
            let mut p = psf.clone();
            p.set_height(h * scale);
            let s = SetFace::new(set.clone(), h * scale).expect("a face");
            assert_eq!(s.cell(), p.cell(), "{name}");
            assert_eq!(s.scale(), p.scale(), "{name}");
            for range in Range::ALL {
                assert_eq!(s.coverage(range), p.coverage(range), "{name} {range}");
            }
            assert_eq!(s.sprite_policy(), p.sprite_policy(), "{name}");
            let mut drawn = 0;
            for c in (0..=0x10_ffffu32).filter_map(char::from_u32) {
                assert_eq!(s.has_glyph(c), p.has_glyph(c), "{name} {c:?}");
                if !p.has_glyph(c) {
                    continue;
                }
                for style in [Style::default(), BOLD] {
                    assert_eq!(s.draw(&[c], style), p.draw(&[c], style), "{name} {c:?}");
                }
                assert_eq!(
                    s.draw_unscaled(&[c], BOLD),
                    p.draw_unscaled(&[c], BOLD),
                    "{name} {c:?}"
                );
                drawn += 1;
            }
            assert!(drawn >= 256, "{name}: {drawn} glyphs");
        }
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
    }
    h
}

// The C reference's digests of its kmscon_font_render over the PSF
// fixtures (tests/reference/psf.tsv) hold through vitrify: each page drawn
// through a Font of the fixture's set (ADR-1200 D10)
#[test]
fn psf_digests_hold_through_vitrify() {
    let path = format!("{}/tests/reference/psf.tsv", env!("CARGO_MANIFEST_DIR"));
    let rows = std::fs::read_to_string(path).expect("psf.tsv");
    let mut sets = BTreeMap::new();
    let mut checked = 0;
    for row in rows.lines() {
        let f: Vec<&str> = row.split('\t').collect();
        let &["render", name, height, page, cells, bold, want] = f.as_slice() else {
            continue;
        };
        if name.ends_with(".gz") && !cfg!(feature = "gzip") {
            continue;
        }
        let set = sets
            .entry(name.to_owned())
            .or_insert_with(|| {
                let psf = Psf::load(fixture(name)).expect("loads");
                Arc::new(read(vitrify_psf("fixture", &psf)))
            })
            .clone();
        let font = Font::new(SetFace::new(set, height.parse().unwrap()).expect("a face"));
        let width = if cells == "2" {
            Width::Wide
        } else {
            Width::Narrow
        };
        let style = Style {
            bold: bold == "1",
            italic: false,
        };
        let page = u32::from_str_radix(page, 16).unwrap();
        let mut d = FNV_OFFSET;
        let mut skipped_page = false;
        for cp in page << 8..(page + 1) << 8 {
            // the characters vtium draws as sprites beyond the C reference
            if matches!(cp, 0x1cea0..=0x1ceaf | 0x1fb3c..=0x1fb92 | 0x1fb94..=0x1fbaf | 0x1fbbd..=0x1fbbf | 0x1fbce..=0x1fbef)
            {
                continue;
            }
            let Some(c) = char::from_u32(cp) else {
                skipped_page = true;
                break;
            };
            let g = font.glyph(&[c], width, style).expect("draws");
            d = fnv(d, &g.width().to_le_bytes());
            d = fnv(d, &g.height().to_le_bytes());
            d = fnv(d, g.pixels());
        }
        if skipped_page {
            continue;
        }
        assert_eq!(format!("{d:016x}"), want, "{row}");
        checked += 1;
    }
    assert!(checked > 50, "{checked} rows");
}

// D9: the strike of the height asked for, else one whose multiple it is,
// else the tallest under it scaled, else the smallest
#[test]
fn a_face_picks_its_strike_by_height() {
    let set = Arc::new(read(small_set(true)));
    let pick = |h| {
        let f = SetFace::new(set.clone(), h).expect("a face");
        (f.size_step(), f.scale())
    };
    assert_eq!(pick(6), (6, 1));
    assert_eq!(pick(8), (8, 1));
    assert_eq!(pick(12), (6, 2));
    assert_eq!(pick(16), (8, 2));
    assert_eq!(pick(24), (8, 3));
    assert_eq!(pick(30), (6, 5));
    assert_eq!(pick(7), (6, 1));
    assert_eq!(pick(17), (8, 2));
    // below the smallest strike, it stops there
    assert_eq!(pick(4), (6, 1));
    assert_eq!(pick(0), (6, 1));
}

/// A set's chunk index entry `i`: offset, stored, decoded, flags.
fn chunk_entry(set: &[u8], i: usize) -> (usize, usize, usize, u32) {
    let (_, _, secs) = sections(set);
    let glyf_at = {
        let u64_at = |i: usize| u64::from_le_bytes(set[i..i + 8].try_into().unwrap());
        let n = secs.len();
        (0..n)
            .map(|s| 16 + 20 * s)
            .find(|&at| &set[at..at + 4] == b"GLYF")
            .map(|at| u64_at(at + 4) as usize)
            .expect("GLYF")
    };
    let e = glyf_at + 8 + 52 * i;
    let u32_at = |i: usize| u32::from_le_bytes(set[i..i + 4].try_into().unwrap());
    let offset = u64::from_le_bytes(set[e..e + 8].try_into().unwrap()) as usize;
    (
        glyf_at + offset,
        u32_at(e + 8) as usize,
        u32_at(e + 12) as usize,
        u32_at(e + 16),
    )
}

// A chunk whose bytes do not match its hash draws nothing, and is the
// set's fault; the others still draw (ADR-1200 D3, D10)
#[test]
fn a_chunk_that_fails_its_hash_is_refused() {
    let mut bytes = small_set(false);
    // the last chunk is the R8 strike's; the first, the 1-bit one's
    let (at, _, _, _) = chunk_entry(&bytes, 0);
    bytes[at] ^= 1;
    let set = Arc::new(read(bytes));
    assert!(matches!(set.check_chunks(), Err(SetError::ChunkHash(0))));
    let face = SetFace::new(set.clone(), 6).expect("a face");
    assert_eq!(face.draw(&['A'], Style::default()), None);
    assert!(matches!(set.fault(), Some(SetError::ChunkHash(0))));
    let r8 = SetFace::new(set.clone(), 8).expect("a face");
    assert!(r8.draw(&['a'], Style::default()).is_some());
}

// A deflated chunk that decodes to any length but its index's is refused
// (ADR-1200 D3, D10)
#[test]
fn a_deflated_chunk_of_another_length_is_refused() {
    let cell = Cell {
        width: 16,
        height: 16,
    };
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default());
    let g = s.glyph(Width::Narrow, vec![0; 256]);
    s.map('x', g);
    let good = SetBuilder::new("short").strike(s).write().expect("writes");
    let (at, stored, decoded, flags) = chunk_entry(&good, 0);
    assert_eq!((decoded, flags), (256, 1), "a deflated chunk");
    for len in [255, 257] {
        let other = miniz_oxide::deflate::compress_to_vec(&vec![0u8; len], 9);
        assert_eq!(other.len(), stored, "a stream of the same stored length");
        let mut bytes = good.clone();
        bytes[at..at + stored].copy_from_slice(&other);
        // the index's hash is the new bytes', so only the length is wrong
        let (_, _, secs) = sections(&bytes);
        let glyf = &secs.iter().find(|(t, _)| t == b"GLYF").unwrap().1;
        let hash_at = bytes.len() - glyf.len() + 8 + 20;
        bytes[hash_at..hash_at + 32].copy_from_slice(&Sha256::digest(&other));
        let set = read(bytes);
        assert!(
            matches!(set.check_chunks(), Err(SetError::ChunkLength(0))),
            "{len}"
        );
    }
}

// An unknown major version is refused, naming the version; a later minor
// version's extra section is skipped
#[test]
fn an_unknown_major_version_is_refused() {
    let mut bytes = small_set(true);
    bytes[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert!(matches!(refused(bytes), SetError::UnknownMajor(2)));
    let (_, _, mut secs) = sections(&small_set(true));
    secs.insert(2, (*b"XTRA", vec![1, 2, 3]));
    let set = read(assemble(1, 7, &secs));
    assert_eq!(set.version(), (1, 7));
    set.check_chunks().expect("checks");
}

#[test]
fn not_a_set_is_refused() {
    assert!(matches!(
        refused(b"PSF2 or anything".to_vec()),
        SetError::NotSet
    ));
    assert!(matches!(
        refused(b"VTGLASS\0".to_vec()),
        SetError::Truncated
    ));
    let bytes = small_set(true);
    assert!(matches!(
        refused(bytes[..bytes.len() - 1].to_vec()),
        SetError::Truncated
    ));
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(matches!(refused(longer), SetError::Malformed(_)));
}

/// Little-endian u32 `v` written at `at` in `bytes`.
fn put32(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

// Each bound refuses the set (ADR-1200 D2: "Every count, size and offset
// is bounded")
#[test]
fn each_bound_refuses_the_set() {
    let set = small_set(true);
    let bound = |bytes: Vec<u8>| match refused(bytes) {
        SetError::Bound(name) => name,
        other => panic!("refused for {other}, not a bound"),
    };
    let mut b = set.clone();
    put32(&mut b, 12, 17);
    assert_eq!(bound(b), "SET_SECTIONS_MAX");
    assert_eq!(
        bound(with_section(&set, b"META", |m| m.extend(vec![b'x'; 1 << 20]))),
        "SET_META_BYTES_MAX"
    );
    assert_eq!(
        bound(with_section(&set, b"STRK", |s| put32(s, 0, 33))),
        "SET_STRIKES_MAX"
    );
    assert_eq!(
        bound(with_section(&set, b"STRK", |s| put32(s, 4, 257))),
        "SET_RECORD_BYTES_MAX"
    );
    // the first strike's glyph count, then its cell width
    assert_eq!(
        bound(with_section(&set, b"STRK", |s| put32(s, 8 + 20, 0x11_0001))),
        "SET_GLYPHS_MAX"
    );
    assert_eq!(
        bound(with_section(&set, b"STRK", |s| s[8 + 4..8 + 6]
            .copy_from_slice(&2049u16.to_le_bytes()))),
        "CELL_SIDE_MAX"
    );
    assert_eq!(
        bound(with_section(&set, b"CMAP", |s| put32(s, 8, (1 << 20) + 1))),
        "SET_RUNS_MAX"
    );
    assert_eq!(
        bound(with_section(&set, b"GLYF", |s| put32(s, 0, (1 << 18) + 1))),
        "SET_CHUNKS_MAX"
    );
    assert_eq!(
        bound(with_section(&set, b"GLYF", |s| put32(
            s,
            8 + 8,
            16 * 1024 + 1
        ))),
        "SET_CHUNK_BYTES"
    );
}

// The writer keeps the bounds the reader holds it to
#[test]
fn the_writer_refuses_what_the_reader_would() {
    let cell = Cell {
        width: 128,
        height: 129,
    };
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default());
    let g = s.glyph(Width::Narrow, vec![0; 128 * 129]);
    s.map('x', g);
    assert!(
        SetBuilder::new("big").strike(s).write().is_err(),
        "a glyph past a chunk"
    );
    let s = StrikeBuilder::new(
        PixelFormat::Bits,
        Cell {
            width: 8,
            height: 8,
        },
        Style::default(),
    );
    assert!(
        SetBuilder::new(".hidden")
            .strike(s.clone())
            .write()
            .is_err()
    );
    assert!(
        SetBuilder::new("x")
            .meta("name", "y")
            .strike(s.clone())
            .write()
            .is_err()
    );
    assert!(
        SetBuilder::new("x")
            .meta("k", "a\nb")
            .strike(s.clone())
            .write()
            .is_err()
    );
    let bold = StrikeBuilder::new(
        PixelFormat::Bits,
        Cell {
            width: 8,
            height: 8,
        },
        BOLD,
    );
    assert!(
        SetBuilder::new("x").strike(bold).write().is_err(),
        "a cell size with no regular strike"
    );
    let mut wrong = s.clone();
    let g = wrong.glyph(Width::Narrow, vec![0; 7]);
    wrong.map('x', g);
    assert!(
        SetBuilder::new("x").strike(wrong).write().is_err(),
        "a glyph of the wrong size"
    );
}

// A table that contradicts another is refused
#[test]
fn inconsistent_tables_are_refused() {
    let set = small_set(true);
    let malformed = |bytes: Vec<u8>| {
        assert!(
            matches!(refused(bytes), SetError::Malformed(_)),
            "not refused as malformed"
        )
    };
    // POLY's coverage of box drawing, first strike
    malformed(with_section(&set, b"POLY", |p| put32(p, 16, 2)));
    // a sprite source past Auto
    malformed(with_section(&set, b"POLY", |p| p[0] = 3));
    // the first strike's glyph count
    malformed(with_section(&set, b"STRK", |s| put32(s, 8 + 20, 2)));
    // two runs out of order: the first strike's first run starts past its
    // second
    malformed(with_section(&set, b"CMAP", |c| put32(c, 12, 0x10_0000)));
    // a run over the surrogates
    malformed(with_section(&set, b"CMAP", |c| put32(c, 12, 0xd7ff)));
    // META without a name
    malformed(with_section(&set, b"META", |m| m[0] = b'N'));
    // a reserved field
    malformed(with_section(&set, b"STRK", |s| s[8 + 2] = 1));
    // a subpixel strike is a format this build does not draw
    assert!(matches!(
        refused(with_section(&set, b"STRK", |s| s[8 + 1] = 3)),
        SetError::UnknownFormat(3)
    ));
}
