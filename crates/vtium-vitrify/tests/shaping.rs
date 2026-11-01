//! Clusters shaped by HarfRust in the parser's child (vtium/vtium#144):
//! an emoji ZWJ sequence and a flag each become one glyph two cells wide,
//! a decomposed letter draws as its precomposed glyph, a sequence the font
//! does not ligate is left out, the answer's clusters are checked, and a
//! built set draws them whole.

#[path = "common/parser.rs"]
mod parser;
#[path = "common/ttf.rs"]
mod ttf;

use std::path::Path;

use parser::Parser;
use ttf::{FAMILY, FLAG};
use vtium_vitrify::outline::{Outline, OutlineRequest, Placed, rasterise};
use vtium_vitrify::shaping::{Ask, Kind};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-vitrify");

fn key(cs: &[char]) -> Vec<u32> {
    cs.iter().map(|&c| u32::from(c)).collect()
}

fn ask(cs: &[char], kind: Kind) -> Ask {
    Ask { key: key(cs), kind }
}

/// `Test Shaping` at a 20-pixel cell, asked for `asks`.
fn request(asks: Vec<Ask>) -> OutlineRequest {
    OutlineRequest {
        font: ttf::test_shaping().build().into(),
        family: "Test Shaping".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: asks,
        baselines: Vec::new(),
    }
}

/// The glyph the strike's cluster map gives `cs`.
fn cluster<'a>(o: &'a Outline, cs: &[char]) -> Option<&'a Placed> {
    let s = &o.strikes[0];
    let k = key(cs);
    let (_, g) = s.clusters.iter().find(|(c, _)| *c == k)?;
    s.glyphs.get(*g as usize)
}

fn codepoint(o: &Outline, c: char) -> &Placed {
    let s = &o.strikes[0];
    let (_, g) = s
        .map
        .iter()
        .find(|(cp, _)| *cp == u32::from(c))
        .expect("mapped");
    &s.glyphs[*g as usize]
}

// The family emoji, five scalar values, is one color glyph two cells wide
// (the font's ccmp ligature), drawn as the font draws it
#[test]
fn a_zwj_family_sequence_becomes_one_two_cell_glyph() {
    let o = rasterise(&request(vec![ask(&FAMILY, Kind::Emoji)])).expect("rasterised");
    let family = cluster(&o, &FAMILY).expect("the family is shaped");
    assert!(family.wide && family.color);
    assert_eq!(family.pixels.len(), 2 * 12 * 20 * 4);
    // the red square under the half-transparent blue: red at an edge,
    // purple where they overlap
    let px = |x: usize, y: usize| &family.pixels[(y * 24 + x) * 4..(y * 24 + x) * 4 + 4];
    assert!(family.pixels.chunks(4).any(|p| p == [255, 0, 0, 255]));
    assert!(
        family
            .pixels
            .chunks(4)
            .any(|p| p[0] > 0 && p[2] > 0 && p[3] == 255)
    );
    assert_eq!(px(0, 0), [0, 0, 0, 0], "inside its cells, not filling them");
}

// A flag, two regional indicators, is one glyph two cells wide
#[test]
fn a_flag_becomes_one_glyph() {
    let o = rasterise(&request(vec![ask(&FLAG, Kind::Emoji)])).expect("rasterised");
    let flag = cluster(&o, &FLAG).expect("the flag is shaped");
    assert!(flag.wide && flag.color);
    // the blue layer alone, half transparent, premultiplied
    assert!(flag.pixels.chunks(4).any(|p| p == [0, 0, 128, 128]));
    assert!(flag.pixels.chunks(4).all(|p| p[0] == 0));
}

// A sequence the font draws as several glyphs, or with a glyph it lacks,
// is left out: the console draws it its own way
#[test]
fn a_sequence_the_font_does_not_ligate_is_left_out() {
    let man_girl = ['\u{1f468}', '\u{200d}', '\u{1f467}'];
    let us_x = ['\u{1f1fa}', '\u{1f1fd}'];
    // the font has no q
    let q_acute = ['q', '\u{301}'];
    let o = rasterise(&request(vec![
        ask(&man_girl, Kind::Emoji),
        ask(&us_x, Kind::Emoji),
        ask(&q_acute, Kind::Marks),
    ]))
    .expect("rasterised");
    assert!(o.strikes[0].clusters.is_empty());
}

// A letter and its combining mark draw as the precomposed letter, which
// HarfRust composes to
#[test]
fn a_decomposed_letter_draws_as_its_precomposed_glyph() {
    let o = rasterise(&request(vec![ask(&['e', '\u{301}'], Kind::Marks)])).expect("rasterised");
    let composed = cluster(&o, &['e', '\u{301}']).expect("shaped");
    assert!(!composed.wide && !composed.color);
    assert_eq!(composed.pixels, codepoint(&o, '\u{e9}').pixels);
}

// The child shapes what the library does
#[test]
fn the_parsers_child_shapes() {
    let parser = Parser::with_program(Path::new(BIN), &["--parse"]);
    let req = request(vec![ask(&FAMILY, Kind::Emoji), ask(&FLAG, Kind::Emoji)]);
    let o = parser.rasterise(&req).expect("rasterised");
    assert_eq!(o, rasterise(&req).expect("rasterised"));
    assert_eq!(o.strikes[0].clusters.len(), 2);
}

#[test]
fn a_request_with_clusters_reads_back() {
    let r = request(vec![
        ask(&FAMILY, Kind::Emoji),
        ask(&['e', '\u{301}'], Kind::Marks),
    ]);
    assert_eq!(OutlineRequest::decode(&r.encode()), Ok(r.clone()));
    // a key of no scalar value, one past CLUSTER_SCALARS_MAX, and one that
    // is not a character
    for bad in [vec![], vec![0x41; 17], vec![0xd800]] {
        let r = request(vec![Ask {
            key: bad,
            kind: Kind::Emoji,
        }]);
        assert!(OutlineRequest::decode(&r.encode()).is_err());
    }
}

fn refused(bytes: &[u8], req: &OutlineRequest, why: &str) {
    let e = Outline::decode(bytes, req).expect_err("refused");
    assert!(e.contains(why), "{e}, not {why}");
}

// Every cluster field the parent reads is checked against the request
// before it is trusted, each count before anything is allocated for it
#[test]
fn the_answers_clusters_are_checked() {
    let req = request(vec![ask(&FAMILY, Kind::Emoji), ask(&FLAG, Kind::Emoji)]);
    let o = rasterise(&req).expect("rasterised");
    assert_eq!(Outline::decode(&o.encode(), &req), Ok(o.clone()));
    // a cluster the request did not ask for
    let mut bad = o.clone();
    bad.strikes[0].clusters[0].0 = key(&['\u{1f468}', '\u{200d}', '\u{1f466}']);
    refused(&bad.encode(), &req, "did not ask for");
    // a glyph the strike lacks
    let mut bad = o.clone();
    bad.strikes[0].clusters[0].1 = 999;
    refused(&bad.encode(), &req, "past the count");
    // out of order, or twice
    let mut bad = o.clone();
    bad.strikes[0].clusters.reverse();
    refused(&bad.encode(), &req, "not sorted");
    let mut bad = o.clone();
    let first = bad.strikes[0].clusters[0].clone();
    bad.strikes[0].clusters[1] = first;
    refused(&bad.encode(), &req, "not sorted");
    // more clusters than were asked for
    let mut bad = o.clone();
    let first = bad.strikes[0].clusters[0].clone();
    bad.strikes[0].clusters.push(first);
    refused(&bad.encode(), &req, "more clusters than");
    // a key longer than CLUSTER_SCALARS_MAX
    let mut bad = o.clone();
    bad.strikes[0].clusters[0].0 = vec![0x41; 17];
    refused(&bad.encode(), &req, "CLUSTER_SCALARS_MAX");
}

const ZWJ: char = '\u{200d}';
const BEH: char = '\u{628}';
const LAM: char = '\u{644}';
const ALEF: char = '\u{627}';

// Beh's initial, medial and final forms, keyed by ZWJ on the side it
// joins (ArabicShaping.txt's join-causing U+200D), each one cell and each
// its own glyph; the isolated form is the codepoint's own. Lam-alef is one
// ligature two cells wide, isolated and final
#[test]
fn an_arabic_letters_four_forms_are_shaped() {
    let forms = [vec![BEH, ZWJ], vec![ZWJ, BEH, ZWJ], vec![ZWJ, BEH]];
    let lam_alef = [vec![LAM, ALEF], vec![ZWJ, LAM, ALEF]];
    let asks = forms
        .iter()
        .chain(&lam_alef)
        .map(|k| ask(k, Kind::Joining))
        .collect();
    let o = rasterise(&request(asks)).expect("rasterised");
    let isolated = codepoint(&o, BEH);
    let mut drawn = vec![isolated.pixels.clone()];
    for k in &forms {
        let g = cluster(&o, k).unwrap_or_else(|| panic!("{k:?} shaped"));
        assert!(!g.wide && !g.color, "{k:?}");
        drawn.push(g.pixels.clone());
    }
    drawn.sort();
    drawn.dedup();
    assert_eq!(drawn.len(), 4, "four forms, each drawn apart");
    let iso = cluster(&o, &lam_alef[0]).expect("lam-alef");
    let fina = cluster(&o, &lam_alef[1]).expect("final lam-alef");
    assert!(iso.wide && fina.wide);
    assert_ne!(iso.pixels, fina.pixels);
}

// A form the font draws as the letter's own glyph, a letter it does not
// join, is left out: the codepoint draws it
#[test]
fn a_form_the_font_does_not_join_is_left_out() {
    // the font has no final form for e, which does not join
    let o = rasterise(&request(vec![
        ask(&[ZWJ, 'e'], Kind::Joining),
        ask(&[LAM, BEH], Kind::Joining),
    ]))
    .expect("rasterised");
    assert!(o.strikes[0].clusters.is_empty());
}

/// Test Mono with a circumflex (U+0302), a bar across the whole cell on
/// top of a narrow stem at its right end, beside `A`'s ink, 250 units tall from 900 above the baseline, and a
/// dot below (U+0323) 150 tall, 350 under it, each of no advance: marks
/// HarfRust places on `A`, whose ink is 0 to 700 units.
fn marked() -> OutlineRequest {
    let mut f = ttf::test_mono();
    let rect = |x0, y0, x1, y1| vec![(x0, y0), (x0, y1), (x1, y1), (x1, y0)];
    f.glyphs.push(ttf::Glyph {
        advance: 0,
        contours: vec![rect(-600, 1050, 0, 1150), rect(-100, 900, 0, 1050)],
    });
    f.glyphs.push(ttf::Glyph::rect(0, -400, -500, -200, -350));
    f.cmap.push((0x302, 8));
    f.cmap.push((0x323, 9));
    f.cmap.sort_unstable();
    OutlineRequest {
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        cover: vec![(0x41, 0x41)],
        clusters: vec![
            ask(&['A', '\u{302}'], Kind::Marks),
            ask(&['A', '\u{323}'], Kind::Marks),
        ],
        baselines: Vec::new(),
    }
}

// ADR-1302 D3 (the maintainer, 2026-11-01): a mark whose ink passes the
// row moves toward the baseline, no closer to its letter than the
// letter's ink, and squeezes toward it where it still cannot fit; the base
// letter never moves. In the 12x20 cell, baseline 16, A's ink fills rows
// 2 to 15: the circumflex, placed above A by HarfRust and cut away whole
// at the row's top without the rule, is squeezed into rows 0 to 2, and
// the dot below ends in the cell's bottom rows
// covers: text.overhang
#[test]
fn a_mark_that_passes_the_row_moves_in_and_squeezes() {
    let o = rasterise(&marked()).expect("rasterised");
    let s = &o.strikes[0];
    let a = codepoint(&o, 'A');
    let rows = |p: &Placed, y: usize| p.pixels[y * 12..(y + 1) * 12].to_vec();
    let inked = |p: &Placed, y: usize| rows(p, y).iter().any(|&v| v > 0);
    let above = cluster(&o, &['A', '\u{302}']).expect("A and the circumflex");
    let below = cluster(&o, &['A', '\u{323}']).expect("A and the dot below");
    assert_eq!(s.baseline, 16);
    assert!(
        inked(above, 0) && inked(above, 1),
        "the circumflex in the top rows"
    );
    // squeezed whole, its bar is on top; cut, only its stem would show; and
    // moved past A's top, its stem would ink A's rows beside A
    let wide = rows(above, 0).iter().filter(|&&v| v > 0).count();
    assert!(
        wide >= 10,
        "the bar across the top row: {:?}",
        rows(above, 0)
    );
    assert!(!inked(a, 0) && !inked(a, 1), "A alone leaves them clear");
    assert!(inked(below, 19), "the dot below ends in the bottom row");
    for y in 3..16 {
        assert_eq!(
            rows(above, y),
            rows(a, y),
            "A's row {y} under the circumflex"
        );
        assert_eq!(rows(below, y), rows(a, y), "A's row {y} over the dot");
    }
}
