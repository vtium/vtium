//! A set's cluster map, `CLUS` (vtium/vtium#144): clusters written and
//! read back and drawn whole, looked up by their exact scalar values, a
//! set without clusters written as before, a reader that does not know the
//! section skipping it, and every refusal of a malformed one.

mod common;

use std::sync::Arc;

use common::{assemble, sections, with_section};
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetError, SetFace, StrikeBuilder};
use vtium_font::{Cell, Face, Format, Style, Width};

const CELL: Cell = Cell {
    width: 4,
    height: 4,
};

/// The family emoji, man ZWJ woman ZWJ girl.
const FAMILY: [char; 5] = [
    '\u{1f468}',
    '\u{200d}',
    '\u{1f469}',
    '\u{200d}',
    '\u{1f467}',
];
/// The flag of the United States, two regional indicators.
const FLAG: [char; 2] = ['\u{1f1fa}', '\u{1f1f8}'];
/// Beh's medial form, keyed as ZWJ beh ZWJ.
const MEDIAL: [char; 3] = ['\u{200d}', '\u{628}', '\u{200d}'];

fn rgba(px: [u8; 4], cells: u32) -> Vec<u8> {
    px.repeat((CELL.width * cells * CELL.height) as usize)
}

/// A set at one 4x4 cell: an R8 strike mapping `A` and beh, with beh's
/// medial form as a cluster, and an RGBA strike mapping one emoji, with
/// the family and the flag as two-cell clusters.
fn clustered() -> Vec<u8> {
    let mut r8 = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
    let a = r8.glyph(Width::Narrow, vec![10; 16]);
    let beh = r8.glyph(Width::Narrow, vec![20; 16]);
    let medial = r8.glyph(Width::Narrow, vec![30; 16]);
    r8.map('A', a);
    r8.map('\u{628}', beh);
    r8.cluster(&MEDIAL, medial);
    let mut color = StrikeBuilder::new(PixelFormat::Rgba, CELL, Style::default());
    let smile = color.glyph(Width::Wide, rgba([1, 2, 3, 255], 2));
    let family = color.glyph(Width::Wide, rgba([40, 50, 60, 255], 2));
    let flag = color.glyph(Width::Wide, rgba([70, 0, 0, 128], 2));
    color.map('\u{1f600}', smile);
    color.cluster(&FAMILY, family);
    // keyed again without its variation selector, the same glyph
    color.cluster(&FLAG, flag);
    color.cluster(&[FLAG[0], FLAG[1], '\u{fe0f}'], flag);
    SetBuilder::new("clustered")
        .strike(r8)
        .strike(color)
        .write()
        .expect("the set writes")
}

fn face(bytes: Vec<u8>) -> SetFace {
    let set = Set::from_bytes(bytes).expect("the set reads");
    SetFace::new(Arc::new(set), 4).expect("a face")
}

fn refused(bytes: Vec<u8>) -> SetError {
    Set::from_bytes(bytes).expect_err("the set is refused")
}

fn malformed(bytes: Vec<u8>) {
    let e = refused(bytes);
    assert!(matches!(e, SetError::Malformed(_)), "refused for {e}");
}

fn put32(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put16(bytes: &mut [u8], at: usize, v: u16) {
    bytes[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

/// Where record `i` of `CLUS` starts, after its 20-byte head.
fn record(i: usize) -> usize {
    20 + 20 * i
}

// A cluster written and read back draws its glyph whole, one or two
// cells wide, in its strike's pixel format
#[test]
fn a_cluster_reads_back_and_draws() {
    let face = face(clustered());
    let family = face.cluster(&FAMILY, Style::default()).expect("the family");
    assert_eq!(family.format(), Format::Rgba);
    assert_eq!((family.width(), family.height()), (8, 4));
    assert_eq!(&family.pixels()[..4], &[40, 50, 60, 255]);
    let flag = face.cluster(&FLAG, Style::default()).expect("the flag");
    assert_eq!(&flag.pixels()[..4], &[70, 0, 0, 128]);
    let again = face
        .cluster(&[FLAG[0], FLAG[1], '\u{fe0f}'], Style::default())
        .expect("the flag with its selector");
    assert_eq!(again.pixels(), flag.pixels());
    let medial = face.cluster(&MEDIAL, Style::default()).expect("medial");
    assert_eq!(medial.format(), Format::Mask);
    assert_eq!((medial.width(), medial.height()), (4, 4));
    assert!(medial.pixels().iter().all(|&p| p == 30));
    // the codepoints still draw from CMAP
    let beh = face.draw(&['\u{628}'], Style::default()).expect("beh");
    assert!(beh.pixels().iter().all(|&p| p == 20));
}

// A lookup is by the exact scalar values: a prefix, a longer cluster, a
// cluster past CLUSTER_SCALARS_MAX and the empty cluster find nothing
#[test]
fn a_lookup_takes_the_exact_cluster() {
    let face = face(clustered());
    assert!(face.cluster(&FAMILY[..3], Style::default()).is_none());
    let mut longer = FAMILY.to_vec();
    longer.push('\u{200d}');
    assert!(face.cluster(&longer, Style::default()).is_none());
    assert!(face.cluster(&['A'], Style::default()).is_none());
    assert!(face.cluster(&[], Style::default()).is_none());
    assert!(face.cluster(&['A'; 17], Style::default()).is_none());
    // bold draws from the regular strike, a mask widened
    let bold = Style {
        bold: true,
        italic: false,
    };
    assert!(face.cluster(&MEDIAL, bold).is_some());
}

// A set with no cluster has no CLUS, so its bytes are as before
#[test]
fn a_set_without_clusters_has_no_clus() {
    let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
    let a = s.glyph(Width::Narrow, vec![10; 16]);
    s.map('A', a);
    let bytes = SetBuilder::new("plain").strike(s).write().expect("writes");
    let (_, _, secs) = sections(&bytes);
    let tags: Vec<&[u8; 4]> = secs.iter().map(|(t, _)| t).collect();
    assert_eq!(tags, [b"META", b"STRK", b"CMAP", b"POLY", b"GLYF"]);
    let (major, minor, with) = sections(&clustered());
    assert_eq!((major, minor), (2, 0), "CLUS leaves the format at 2.0");
    assert_eq!(with.last().map(|(t, _)| t), Some(b"CLUS"));
}

// A reader that does not know CLUS skips it, as a 2.0 reader skips any
// section a later minor version adds: the section renamed to a tag this
// reader does not know reads, draws its codepoints alike and maps no
// cluster
#[test]
fn a_reader_that_does_not_know_clus_skips_it() {
    let (major, minor, mut secs) = sections(&clustered());
    for (tag, _) in &mut secs {
        if tag == b"CLUS" {
            *tag = *b"CLUZ";
        }
    }
    let old = face(assemble(major, minor, &secs));
    let new = face(clustered());
    for c in ['A', '\u{628}', '\u{1f600}'] {
        assert_eq!(
            old.draw(&[c], Style::default()),
            new.draw(&[c], Style::default())
        );
    }
    assert!(old.cluster(&FAMILY, Style::default()).is_none());
    assert_eq!(old.set().check_chunks().ok(), Some(()));
}

// Every chunk CLUS holds is checked as GLYF's are; the writer deflates
// the chunk with the `deflate` feature
#[cfg(feature = "deflate")]
#[test]
fn a_cluster_chunk_that_fails_to_inflate_is_refused() {
    let bad = with_section(&clustered(), b"CLUS", |c| {
        let last = c.len() - 1;
        c[last] ^= 0xff;
    });
    let face = face(bad);
    assert!(face.draw(&['A'], Style::default()).is_some());
    assert!(face.set().check_chunks().is_err());
    // the R8 strike's cluster chunk is CLUS's first; the RGBA one's second
    assert!(face.cluster(&MEDIAL, Style::default()).is_some());
    assert!(face.cluster(&FAMILY, Style::default()).is_none());
    assert!(matches!(face.set().fault(), Some(SetError::ChunkLength(_))));
}

// A malformed CLUS is refused before any glyph is drawn
#[test]
fn a_malformed_cluster_map_is_refused() {
    let set = clustered();
    // records: 0 beh's medial (R8 strike 0), then the RGBA strike's: the
    // flag, the flag with FE0F, the family, sorted by key
    let clus = |f: &dyn Fn(&mut Vec<u8>)| with_section(&set, b"CLUS", |c| f(c));
    // an unknown flag
    malformed(clus(&|c| put32(c, record(0) + 16, 2)));
    // a strike the set lacks
    malformed(clus(&|c| put16(c, record(0), 9)));
    // out of order: the family moved to the R8 strike, after the RGBA
    // strike's clusters
    malformed(clus(&|c| put16(c, record(3), 0)));
    // a key repeated: the flag's key as its FE0F form's
    malformed(clus(&|c| {
        let key = c[record(1) + 4..record(1) + 8].to_vec();
        c[record(2) + 4..record(2) + 8].copy_from_slice(&key);
        put16(c, record(2), 1);
        put16(c, record(2) + 2, 2);
    }));
    // a key past the pool
    malformed(clus(&|c| put32(c, record(3) + 4, 1 << 20)));
    // a glyph past its chunk's end, and a chunk the section lacks
    malformed(clus(&|c| put32(c, record(0) + 12, 16 * 1024)));
    malformed(clus(&|c| put32(c, record(0) + 8, 7)));
    // a scalar of the pool that is not a character: the first key's first
    malformed(clus(&|c| put32(c, record(4), 0xd800)));
    // a pool longer than its clusters' keys can be
    malformed(clus(&|c| put32(c, 8, 4 * 16 + 1)));
    // a chunk table whose chunks do not fill the section
    malformed(clus(&|c| c.push(0)));
    // a key of no scalars, and one past CLUSTER_SCALARS_MAX
    for len in [0, 17] {
        let e = refused(clus(&|c| put16(c, record(0) + 2, len)));
        assert!(
            matches!(e, SetError::Bound("CLUSTER_SCALARS_MAX")),
            "refused for {e}"
        );
    }
    // more clusters than SET_CLUSTERS_MAX
    let e = refused(clus(&|c| put32(c, 0, (1 << 16) + 1)));
    assert!(
        matches!(e, SetError::Bound("SET_CLUSTERS_MAX")),
        "refused for {e}"
    );
    // a record smaller than its fields, and cut short
    malformed(clus(&|c| put32(c, 4, 16)));
    let e = refused(clus(&|c| c.truncate(record(2))));
    assert!(
        matches!(e, SetError::Truncated | SetError::Malformed(_)),
        "refused for {e}"
    );
}

// The writer keeps the bounds the reader holds it to
#[test]
fn the_writer_refuses_a_cluster_the_reader_would() {
    let write = |key: &[char], glyph_ok: bool| {
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        let g = s.glyph(Width::Narrow, vec![0; 16]);
        s.cluster(key, if glyph_ok { g } else { g + 1 });
        SetBuilder::new("x").strike(s).write()
    };
    assert!(write(&MEDIAL, true).is_ok());
    assert!(write(&[], true).is_err(), "an empty key");
    assert!(write(&['a'; 17], true).is_err(), "a key past its bound");
    assert!(write(&MEDIAL, false).is_err(), "a glyph the strike lacks");
    let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
    let g = s.glyph(Width::Narrow, vec![0; 16]);
    s.cluster(&MEDIAL, g);
    s.cluster(&MEDIAL, g);
    assert!(
        SetBuilder::new("x").strike(s).write().is_err(),
        "a key twice"
    );
}

// The same clusters give the same bytes, whatever order they are given in
#[test]
fn the_same_clusters_give_the_same_bytes() {
    let one = |order: &[usize]| {
        let mut s = StrikeBuilder::new(PixelFormat::Rgba, CELL, Style::default());
        let g = s.glyph(Width::Wide, rgba([9, 9, 9, 255], 2));
        let keys: [&[char]; 3] = [&FAMILY, &FLAG, &['\u{1f600}', '\u{fe0f}']];
        for &i in order {
            s.cluster(keys[i], g);
        }
        SetBuilder::new("x").strike(s).write().expect("writes")
    };
    assert_eq!(one(&[0, 1, 2]), one(&[2, 0, 1]));
}

// The clusters a set maps are listed with their strike's cell and style,
// as the console checks a frame against what it asked for
#[test]
fn a_sets_clusters_are_listed_with_their_cells() {
    let set = Set::from_bytes(clustered()).expect("the set reads");
    let listed: Vec<(Cell, Style, Vec<char>)> = set.clusters().collect();
    let regular = Style::default();
    let flagged = vec![FLAG[0], FLAG[1], '\u{fe0f}'];
    assert_eq!(
        listed,
        [
            (CELL, regular, MEDIAL.to_vec()),
            (CELL, regular, FLAG.to_vec()),
            (CELL, regular, flagged),
            (CELL, regular, FAMILY.to_vec()),
        ]
    );
    let plain = {
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        let a = s.glyph(Width::Narrow, vec![10; 16]);
        s.map('A', a);
        Set::from_bytes(SetBuilder::new("plain").strike(s).write().expect("writes")).expect("reads")
    };
    assert_eq!(plain.clusters().count(), 0);
}
