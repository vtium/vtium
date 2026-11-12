//! Clusters shaped by HarfRust in the parser's child (vtium/vtium#144):
//! an emoji ZWJ sequence and a flag each become one glyph two cells wide,
//! a decomposed letter draws as its precomposed glyph, a sequence the font
//! does not ligate is left out, the answer's clusters are checked, and a
//! built set draws them whole.

#[path = "common/parser.rs"]
mod parser;
use vtium_testfont::ttf;

use std::path::Path;

use parser::Parser;
use ttf::{FAMILY, FLAG};
use vtium_config::frit::Hinting;
use vtium_config::outline::{Outline, OutlineRequest, Placed, rasterise};
use vtium_config::shaping::{Ask, Kind};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

fn key(cs: &[char]) -> Vec<u32> {
    cs.iter().map(|&c| u32::from(c)).collect()
}

fn ask(cs: &[char], kind: Kind) -> Ask {
    Ask { key: key(cs), kind }
}

/// `Test Shaping` at a 20-pixel cell, asked for `asks`.
fn request(asks: Vec<Ask>) -> OutlineRequest {
    OutlineRequest {
        fit: false,
        synth: Default::default(),
        script: Default::default(),
        font: ttf::test_shaping().build().into(),
        family: "Test Shaping".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: asks,
        baselines: Vec::new(),
        sizes: Vec::new(),
        reaches: Vec::new(),
        hinting: Hinting::Light,
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
    refused(
        &bad.encode(),
        &req,
        &format!("more than {} characters", vtium_font::CLUSTER_SCALARS_MAX),
    );
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

/// `Test Shaping` without its lam-alef ligature: lam and alef shape to
/// their own joined forms, two glyphs, as Noto Sans Arabic draws
/// lam-alef, asked for `asks`.
fn two_glyph_lam_alef(asks: Vec<Ask>) -> OutlineRequest {
    let mut f = ttf::test_shaping();
    f.gsub.retain(|(tag, _)| tag != b"rlig");
    OutlineRequest {
        font: f.build().into(),
        ..request(asks)
    }
}

/// The columns of the glyph `g`, `width` pixels wide, that hold ink.
fn inked_columns(g: &Placed, width: usize) -> Vec<usize> {
    (0..width)
        .filter(|&x| g.pixels.chunks(width).any(|row| row[x] > 0))
        .collect()
}

// ADR-1004 D6: the console asks for lam and alef as the two-cell ligature
// the helper shapes. A font may draw lam-alef as two glyphs, lam's and
// alef's joined forms, where another draws one ligature glyph; Noto Sans
// Arabic does. The helper keeps that drawing, isolated and final, two
// cells wide, where it refused it for coming from two characters
#[test]
fn a_lam_alef_the_font_draws_as_two_glyphs_is_kept() {
    let keys = [vec![LAM, ALEF], vec![ZWJ, LAM, ALEF]];
    let asks = keys.iter().map(|k| ask(k, Kind::Joining)).collect();
    let o = rasterise(&two_glyph_lam_alef(asks)).expect("rasterised");
    for k in &keys {
        let g = cluster(&o, k).unwrap_or_else(|| panic!("{k:?} kept"));
        assert!(g.wide, "{k:?} two cells wide");
    }
    // lam and a letter that is no alef stay two letters, which the console
    // never asks for together
    let o =
        rasterise(&two_glyph_lam_alef(vec![ask(&[LAM, BEH], Kind::Joining)])).expect("rasterised");
    assert!(o.strikes[0].clusters.is_empty());
}

// ADR-1302 D2 and D11: lam-alef is right to left, and its lam joins the
// letter read before it, which stands to its right. The pen rule, the pen
// on the cells' left edge, is mirrored for it, so its advance ends at the
// right edge of its two cells and the join meets. The ligature glyph is a
// cell wide, 600 units, so all its ink lies in the right cell. Drawn as
// two glyphs, lam's medial form, a bar across its whole advance, and
// alef's final form, its advance is the two cells, and the lam's bar
// reaches their right edge
#[test]
fn a_lam_alef_stands_against_the_right_edge_of_its_two_cells() {
    let k = [ZWJ, LAM, ALEF];
    let o = rasterise(&request(vec![ask(&k, Kind::Joining)])).expect("rasterised");
    let w = o.strikes[0].width as usize;
    let inked = inked_columns(cluster(&o, &k).expect("final lam-alef"), 2 * w);
    let (first, last) = (inked[0], inked[inked.len() - 1]);
    assert!(
        first >= w - 1 && last < 2 * w,
        "the ligature in the right cell of {w}: columns {first} to {last}"
    );
    let o = rasterise(&two_glyph_lam_alef(vec![ask(&k, Kind::Joining)])).expect("rasterised");
    let inked = inked_columns(cluster(&o, &k).expect("final lam-alef"), 2 * w);
    assert_eq!(
        inked.last(),
        Some(&(2 * w - 1)),
        "the lam's bar meets the right edge"
    );
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

// ADR-1302 D3 (the maintainer, 2026-11-01), with ADR-1306 D2's edge at the
// reach: lam with shadda and fatha in DejaVu Sans Mono (`tests/fixtures/`),
// drawn through the rasteriser at the 9x16 cell the console gives the
// font, hinted lightly by default, with a reach of no rows above or below,
// so the reach's edges are the row's. The subset has no printable ASCII,
// so the baseline is the ascent rounded up, row 13. The lam alone has
// blank rows above it and its stem starts at row 3; its marks would pass
// the reach's top, so the whole stack is scaled to fit: the marks reach
// the top row, the lam's stem starts lower than it does alone, a blank row
// parts the marks from the stem, and the lam keeps its bottom row.
// `src/shaping/shape.rs` pins the same stack's edges at full hinting
// covers: text.overhang
#[test]
fn lam_shadda_fatha_is_scaled_to_fit_the_row_it_is_drawn_in() {
    let font = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/DejaVuSansMono-lam-shadda-fatha.ttf"),
    )
    .expect("the fixture");
    let stack = ['\u{644}', '\u{651}', '\u{64e}'];
    let o = rasterise(&OutlineRequest {
        fit: false,
        font: font.into(),
        cells: vec![(9, 16)],
        clusters: vec![ask(&stack, Kind::Marks)],
        reaches: vec![(0, 0)],
        ..OutlineRequest::default()
    })
    .expect("rasterised");
    assert_eq!(o.strikes[0].baseline, 13);
    let inked = |p: &Placed, y: usize, cols: std::ops::Range<usize>| {
        p.pixels[y * 9..(y + 1) * 9][cols].iter().any(|&v| v > 0)
    };
    let lam = codepoint(&o, '\u{644}');
    let g = cluster(&o, &stack).expect("lam, shadda and fatha");
    let rows = |p: &Placed, cols: std::ops::Range<usize>| -> Vec<usize> {
        (0..16).filter(|&y| inked(p, y, cols.clone())).collect()
    };
    // the lam's stem lies in columns 6 to 8
    let (alone, drawn) = (rows(lam, 0..9), rows(g, 0..9));
    assert_eq!(alone.first(), Some(&3), "blank rows above the lam");
    assert_eq!(drawn.first(), Some(&0), "the marks reach the top row");
    assert_eq!(drawn.last(), alone.last(), "the lam keeps its bottom row");
    let stem = rows(g, 6..9);
    let gap = (1..16).find(|y| !stem.contains(y)).expect("a blank row");
    assert!(stem.contains(&(gap - 1)), "the marks above the gap");
    let top = (gap..16).find(|y| stem.contains(y)).expect("the stem");
    assert!(top > 3, "the lam's stem starts lower than alone, at {top}");
}

/// Test Mono with letters and marks of no advance, each a bar, which
/// HarfRust places on their letters: `x` (0 to 400 units) and a dot above
/// (U+0307) 380 units tall; `A` (0 to 700) and a dot below (U+0323)
/// from 100 to 150 units under the baseline; an alef (U+0627) reaching 800, near the row's top, and a damma
/// (U+064F) 200 tall; and a jeem (U+062C) from 200 below the baseline,
/// near the row's bottom, to 400, and a kasratan (U+064D) 150 tall; and
/// a mark above (U+0308) 1500 units tall, which HarfRust stacks over x so
/// that it reaches more than a row above the row's top. The cell's glyphs'
/// ink may reach `reach` pixels above and below it, or a row of the cell
/// with none.
fn marked(keys: &[&[char]], height: u32, reach: Option<u32>) -> OutlineRequest {
    let mut f = ttf::test_mono();
    let mut add = |cp: u32, g: ttf::Glyph| {
        f.glyphs.push(g);
        let i = u16::try_from(f.glyphs.len() - 1).expect("a glyph id");
        f.cmap.push((cp, i));
    };
    add(0x78, ttf::Glyph::rect(600, 100, 0, 500, 400));
    add(0x307, ttf::Glyph::rect(0, -350, 0, -250, 380));
    add(0x323, ttf::Glyph::rect(0, -400, -150, -200, -100));
    add(0x627, ttf::Glyph::rect(600, 250, 0, 350, 800));
    add(0x64f, ttf::Glyph::rect(0, -400, 0, -200, 200));
    add(0x62c, ttf::Glyph::rect(600, 100, -200, 500, 400));
    add(0x64d, ttf::Glyph::rect(0, -400, 0, -200, 150));
    add(0x308, ttf::Glyph::rect(0, -350, 0, -250, 1500));
    f.cmap.sort_unstable();
    OutlineRequest {
        fit: false,
        synth: Default::default(),
        script: Default::default(),
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, height)],
        cover: Vec::new(),
        clusters: keys.iter().map(|k| ask(k, Kind::Marks)).collect(),
        baselines: Vec::new(),
        sizes: Vec::new(),
        reaches: reach.map(|r| vec![(r, r)]).unwrap_or_default(),
        hinting: Hinting::Light,
    }
}

/// Row `y` of a glyph one 12-pixel cell wide.
fn row(p: &Placed, y: usize) -> &[u8] {
    &p.pixels[y * 12..(y + 1) * 12]
}

// ADR-1302 D3, with ADR-1306 D2's edge at the reach: a mark that passes the
// reach and can move in without its ink touching its letter's moves toward
// the baseline until it fits, its letter untouched. With a reach of no
// rows, in the 12x20 cell, baseline 16, x's ink fills rows 8 to 15, and
// the dot above, placed from 9 to 16.4 pixels up, ends a row over the top;
// moved in, it fills rows 0 to 7
// covers: text.overhang
#[test]
fn a_mark_that_passes_the_reach_moves_in() {
    let o = rasterise(&marked(&[&['x', '\u{307}']], 20, Some(0))).expect("rasterised");
    assert_eq!(o.strikes[0].baseline, 16);
    let x = codepoint(&o, 'x');
    let g = cluster(&o, &['x', '\u{307}']).expect("x and its dot");
    assert!(
        row(g, 0)[5] > 0 && row(g, 7)[5] > 0,
        "the dot from row 0 to row 7"
    );
    for y in 8..20 {
        assert_eq!(row(g, y), row(x, y), "x's row {y}");
    }
}

// ADR-1302 D3 (the maintainer, 2026-11-01), with ADR-1306 D2's edge at the
// reach, here a reach of no rows: where the marks cannot move in without
// landing more of their ink on the letter's, or cannot fit at all, the
// whole stack, letter and marks, is scaled vertically just enough to fit
// the reach, keeping the font's arrangement. Here the letter
// reaches the edge its marks pass, so a move in would land the marks on
// it (`src/shaping/shape.rs` pins a letter with blank rows above it,
// lam with shadda and fatha, which is scaled too). The alef alone fills rows 0 to 15 of its stem's
// columns, 5 and 6; with its damma, columns 4 to 7, the damma takes the top
// rows and the alef still ends on the baseline. The jeem alone fills rows
// 8 to 19 of columns 2 to 9; with its kasratan, columns 4 to 7, it keeps
// its top row and the kasratan takes the bottom rows
// covers: text.overhang
#[test]
fn a_stack_that_cannot_move_in_is_scaled_to_fit() {
    let o = rasterise(&marked(
        &[&['\u{627}', '\u{64f}'], &['\u{62c}', '\u{64d}']],
        20,
        Some(0),
    ))
    .expect("rasterised");
    let alef = codepoint(&o, '\u{627}');
    let g = cluster(&o, &['\u{627}', '\u{64f}']).expect("alef and damma");
    assert_eq!(row(alef, 0)[4], 0, "the alef alone has no ink in column 4");
    assert!(row(g, 0)[4] > 0, "the damma in the top row");
    assert!(
        row(g, 15)[5] > 0 && row(g, 16)[5] == 0,
        "the alef ends on the baseline"
    );
    let gap = (1..15).any(|y| row(g, y)[4] == 0 && row(g, y)[5] > 0);
    assert!(
        gap,
        "the alef's stem below the damma, as the font arranged them"
    );
    let jeem = codepoint(&o, '\u{62c}');
    let g = cluster(&o, &['\u{62c}', '\u{64d}']).expect("jeem and kasratan");
    assert!(
        row(jeem, 19)[2] > 0,
        "the jeem alone reaches the bottom row"
    );
    let top = |p: &Placed| (0..20).find(|&y| row(p, y)[2] > 0);
    assert_eq!(top(g), top(jeem), "the jeem keeps its top row");
    assert_eq!(row(g, 19)[2], 0, "the jeem lifted off the bottom row");
    assert!(row(g, 19)[5] > 0, "the kasratan in the bottom row");
}

// a letter and a mark that fit the row are drawn as the shaper placed
// them: in a 32-pixel cell, baseline 26, A fills rows 4 to 25 and its dot
// below lies well inside the 6 rows under the baseline
#[test]
fn a_letter_and_mark_that_fit_are_untouched() {
    let o = rasterise(&marked(&[&['A', '\u{323}']], 32, None)).expect("rasterised");
    assert_eq!(o.strikes[0].baseline, 26);
    let w = o.strikes[0].width as usize;
    let rows = |p: &Placed, y: usize| p.pixels[y * w..(y + 1) * w].to_vec();
    let a = codepoint(&o, 'A');
    let g = cluster(&o, &['A', '\u{323}']).expect("A and the dot below");
    for y in 0..26 {
        assert_eq!(rows(g, y), rows(a, y), "A's row {y}");
    }
    let dot: Vec<usize> = (26..32)
        .filter(|&y| rows(g, y).iter().any(|&v| v > 0))
        .collect();
    assert!(
        !dot.is_empty() && !dot.contains(&31),
        "the dot inside the row: {dot:?}"
    );
}

// A cluster whose pen sum is negative is placed as one with no advance
// (through rasterise): its pen goes half the cell in, since a negative
// advance counts as none. Test Mono with an x, a bar from 100 to 500 units
// across and 400 tall, and a dot above, at 20 pixels: as the font gives it,
// x advances 600 units, 11.7 pixels, which rounds to the 12-pixel cell, so
// its pen is on the cell's left edge. With a GPOS adjustment of -1600 units
// to x's advance the cluster's pen sum is -1000 units, and its pen is 6
// columns in, where an unclamped advance of -19.5 pixels would put it 16
// columns in, past the cell
#[test]
fn a_cluster_with_a_negative_advance_is_placed_as_one_with_none() {
    let font = |by: Option<i16>| {
        let mut f = ttf::test_mono();
        f.glyphs.push(ttf::Glyph::rect(600, 100, 0, 500, 400));
        let x = u16::try_from(f.glyphs.len() - 1).expect("a glyph id");
        f.glyphs.push(ttf::Glyph::rect(0, -350, 500, -250, 600));
        f.cmap.push((0x78, x));
        f.cmap.push((0x307, x + 1));
        f.cmap.sort_unstable();
        if let Some(by) = by {
            f.gpos = vec![(*b"kern", ttf::Lookup::Advance(vec![(x, by)]))];
        }
        f.build()
    };
    let key = ['x', '\u{307}'];
    let drawn = |by: Option<i16>| {
        let o = rasterise(&OutlineRequest {
            fit: false,
            synth: Default::default(),
            script: Default::default(),
            font: font(by).into(),
            family: "Test Mono".to_owned(),
            cells: vec![(12, 20)],
            cover: Vec::new(),
            clusters: vec![ask(&key, Kind::Marks)],
            baselines: Vec::new(),
            sizes: Vec::new(),
            reaches: Vec::new(),
            hinting: Hinting::Light,
        })
        .expect("rasterised");
        let g = cluster(&o, &key).expect("x and its dot").clone();
        // the columns x's ink covers in row 12, between its top and the
        // baseline
        (0..12)
            .filter(|&c| g.pixels[12 * 12 + c] > 0)
            .collect::<Vec<usize>>()
    };
    let at_edge = drawn(None);
    let negative = drawn(Some(-1600));
    assert!(!at_edge.is_empty(), "x's ink as the font places it");
    let moved: Vec<usize> = at_edge.iter().map(|c| c + 6).filter(|&c| c < 12).collect();
    assert_eq!(negative, moved, "x's ink 6 columns in, cut at the cell");
}

/// Test Mono with an Arabic alef (U+0627), a bar 760 units tall, and a
/// damma (U+064F), a mark of no advance 300 units tall, which HarfRust
/// places just above the alef, past the row's top. The cell's glyphs' ink
/// may reach `reach` pixels above and below it, or a row of the cell with
/// none.
fn alef_damma(reach: Option<u32>) -> OutlineRequest {
    let mut f = ttf::test_mono();
    f.glyphs.push(ttf::Glyph::rect(600, 250, 0, 350, 760));
    f.glyphs.push(ttf::Glyph::rect(0, -400, 0, -200, 300));
    f.cmap.push((0x627, 8));
    f.cmap.push((0x64f, 9));
    f.cmap.sort_unstable();
    OutlineRequest {
        fit: false,
        synth: Default::default(),
        script: Default::default(),
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: vec![ask(&['\u{627}', '\u{64f}'], Kind::Marks)],
        baselines: Vec::new(),
        sizes: Vec::new(),
        reaches: reach.map(|r| vec![(r, r)]).unwrap_or_default(),
        hinting: Hinting::Light,
    }
}

// ADR-1302 D3 for right-to-left text: HarfRust gives the cluster in
// visual order, the damma before its alef, and the damma is a mark by its
// character. At 20 pixels the alef's ink reaches 14.8 pixels above the
// baseline, row 1, and the damma lies wholly above the row's top, so the
// row's top row is clear unless the mark rule brings the damma in. With a
// reach of no rows, the reach's edge is the row's, and the rule brings it
// in; with the reach of a row, the damma stays where the font puts it,
// above the row, in the glyph's ink box (ADR-1306 D2)
// covers: text.overhang
#[test]
fn a_right_to_left_mark_is_fitted_to_the_reach() {
    let inked = |p: &Placed, y: usize| p.pixels[y * 12..(y + 1) * 12].iter().any(|&v| v > 0);
    let o = rasterise(&alef_damma(Some(0))).expect("rasterised");
    let alef = codepoint(&o, '\u{627}');
    let g = cluster(&o, &['\u{627}', '\u{64f}']).expect("alef and damma");
    assert!(!inked(alef, 0), "the alef alone leaves the top row clear");
    assert!(inked(g, 0), "the damma brought into the top row");
    assert!(g.ink.is_none(), "no ink past the row");
    let o = rasterise(&alef_damma(None)).expect("rasterised");
    let g = cluster(&o, &['\u{627}', '\u{64f}']).expect("alef and damma");
    assert!(!inked(g, 0), "the damma left above the row");
    let ink = g.ink.as_ref().expect("the damma in the box");
    assert!(ink.top < 0 && ink.top >= -20, "{}", ink.top);
    let box_row = |y: usize| &ink.pixels[y * ink.width as usize..(y + 1) * ink.width as usize];
    let above: Vec<usize> = (0..(-ink.top) as usize)
        .filter(|&y| box_row(y).iter().any(|&v| v > 0))
        .collect();
    assert!(!above.is_empty(), "the damma's ink above the row");
}

// ADR-1306 D2: inside the reach a mark draws where HarfRust places it. In
// the 12x20 cell, baseline 16, the dot above x, from 9 to 16.4 pixels up,
// passes the row's top by 0.4 of a pixel, a row of the cell's reach, so it
// is not moved: its top row lies in the box, the row above the cell, and
// x's rows are its own
// covers: text.overhang
#[test]
fn a_mark_inside_the_reach_keeps_its_place() {
    let o = rasterise(&marked(&[&['x', '\u{307}']], 20, None)).expect("rasterised");
    let x = codepoint(&o, 'x');
    let g = cluster(&o, &['x', '\u{307}']).expect("x and its dot");
    let ink = g.ink.as_ref().expect("the dot's top row past the row");
    assert_eq!((ink.left, ink.top, ink.width, ink.height), (0, -1, 12, 21));
    assert!(ink.pixels[5] > 0, "the dot's top row, above the cell");
    for y in 8..20 {
        assert_eq!(row(g, y), row(x, y), "x's row {y}");
    }
}

// ADR-1306 D2: a stack that would pass the reach is still fitted to it,
// to the reach's edge and not to the row's. In the 12x20 cell, baseline
// 16, the tall mark over x reaches 40 pixels above the baseline, 4 past
// the reach's top, 36 up, and a row over x's top; moved down it would land
// on x, so the stack is scaled to fit the reach, anchored at x's bottom:
// its top lies in the reach's top row, a row above the cell, and x still
// ends on the baseline
// covers: text.overhang
#[test]
fn a_stack_past_the_reach_is_fitted_to_the_reach() {
    let o = rasterise(&marked(&[&['x', '\u{308}']], 20, None)).expect("rasterised");
    let x = codepoint(&o, 'x');
    let g = cluster(&o, &['x', '\u{308}']).expect("x and its mark");
    let ink = g.ink.as_ref().expect("the mark above the row");
    assert_eq!((ink.left, ink.width), (0, 12));
    assert_eq!(ink.top, -20, "fitted to the reach's top, a row up");
    let box_row = |y: usize| &ink.pixels[y * 12..(y + 1) * 12];
    assert!(box_row(0)[5] > 0, "the mark in the reach's top row");
    assert!(
        row(x, 15)[2] > 0 && row(g, 15)[2] > 0,
        "x ends on the baseline"
    );
    assert!(row(g, 16).iter().all(|&p| p == 0), "nothing below x");
}

// a beh keyed with U+200D after it, as the console asks for it, is judged a
// joining form: drawn one cell wide in its initial form, a bar from 300 to
// 600 units, stretched to its 12-pixel cell from column 6 to its right edge
#[test]
fn a_letter_with_a_joiner_draws_one_cell_wide_and_stretched() {
    let key = [BEH, ZWJ];
    let asked = vtium_config::shaping::ask(&key.map(u32::from));
    let o = rasterise(&request(vec![asked])).expect("rasterised");
    let init = cluster(&o, &key).expect("the initial beh");
    assert!(!init.wide, "one cell");
    let row = &init.pixels[10 * 12..11 * 12];
    assert!(row[5] == 0 && row[6] > 200 && row[11] == 255, "{row:?}");
}

// A beh with U+FE0F before its joiner, as a program may write after a
// letter, is still a joining form: U+FE0F is a mark, of General_Category
// Mn, so the cluster's one letter is the beh, which joins, and the
// variation selector does not make it an emoji sequence two cells wide.
// It draws one cell wide, as the beh's initial form keyed alone does
#[test]
fn a_letter_with_vs16_and_a_joiner_is_a_joining_form_one_cell_wide() {
    let key = [BEH, '\u{fe0f}', ZWJ];
    let asked = vtium_config::shaping::ask(&key.map(u32::from));
    assert_eq!(asked.kind, Kind::Joining);
    let bare = vtium_config::shaping::ask(&[BEH, ZWJ].map(u32::from));
    let o = rasterise(&request(vec![asked, bare])).expect("rasterised");
    let g = cluster(&o, &key).expect("the beh with U+FE0F and a joiner");
    assert!(!g.wide && !g.color, "one cell, a mask");
    let init = cluster(&o, &[BEH, ZWJ]).expect("the initial beh");
    assert_eq!(g.pixels, init.pixels, "drawn as the initial beh");
}

// A beh with a fatha (U+064E), keyed with U+200D on either side, is a
// joining form: the fatha, whose Joining_Type is T, does not make the
// cluster an emoji sequence, whose one-glyph rule would drop it. Each order
// draws one cell wide, the beh in its joined form with the fatha above it:
// the bar from 0 to 500 units fills rows 6 to 15 of the 20-pixel cell, and
// the fatha, a bar from 600 to 700 units, ink in rows 2 to 4, where the beh
// alone has none
#[test]
fn a_vocalised_letter_with_a_joiner_draws_as_a_joining_form() {
    const FATHA: char = '\u{64e}';
    let mut font = ttf::test_shaping();
    font.glyphs.push(ttf::Glyph::rect(0, -400, 600, -200, 700));
    let fatha = u16::try_from(font.glyphs.len() - 1).expect("a glyph id");
    font.cmap.push((u32::from(FATHA), fatha));
    font.cmap.sort_unstable();
    let keys = [[BEH, FATHA, ZWJ], [ZWJ, BEH, FATHA]];
    let bare = [[BEH, ZWJ], [ZWJ, BEH]];
    let asks: Vec<Ask> = keys
        .iter()
        .map(|k| k.as_slice())
        .chain(bare.iter().map(|k| k.as_slice()))
        .map(|k| vtium_config::shaping::ask(&key(k)))
        .collect();
    let o = rasterise(&OutlineRequest {
        fit: false,
        font: font.build().into(),
        ..request(asks)
    })
    .expect("rasterised");
    let inked = |p: &Placed, y: usize| p.pixels[y * 12..(y + 1) * 12].iter().any(|&v| v > 0);
    for (k, b) in keys.iter().zip(&bare) {
        let g = cluster(&o, k).unwrap_or_else(|| panic!("{k:?} shaped"));
        let joined = cluster(&o, b).unwrap_or_else(|| panic!("{b:?} shaped"));
        assert!(!g.wide, "{k:?} one cell");
        assert!((2..5).any(|y| inked(g, y)), "{k:?}: the fatha");
        assert!(!(2..5).any(|y| inked(joined, y)), "{b:?}: no fatha");
        for y in 6..16 {
            assert_eq!(
                &g.pixels[y * 12..(y + 1) * 12],
                &joined.pixels[y * 12..(y + 1) * 12],
                "{k:?}: the joined beh's row {y}"
            );
        }
    }
}

// A mark the shaper leaves at the pen, with no advance and no offset, as
// HarfRust leaves an acute after a circumflex in DejaVu Sans Mono, is
// placed by the helper itself: centred over its base's ink, a row above
// all the ink since the base.
// Here x's ink fills rows 8 to 15 of columns 1 to 9; the font's empty
// GPOS leaves the circumflex and the acute, zero-width bars drawn right of
// the pen, where HarfRust put them, which is in the next cell
#[test]
fn a_mark_left_at_the_pen_is_centred_over_its_base() {
    let mut f = ttf::Font {
        empty_gpos: true,
        ..ttf::test_mono()
    };
    f.glyphs.push(ttf::Glyph::rect(600, 100, 0, 500, 400));
    f.glyphs.push(ttf::Glyph::rect(0, 0, 0, 200, 50));
    f.glyphs.push(ttf::Glyph::rect(0, 50, 0, 150, 100));
    f.cmap.extend([(0x78, 8), (0x302, 9), (0x301, 10)]);
    f.cmap.sort_unstable();
    let key = ['x', '\u{302}', '\u{301}'];
    let o = rasterise(&OutlineRequest {
        fit: false,
        synth: Default::default(),
        script: Default::default(),
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: vec![ask(&key, Kind::Marks)],
        baselines: Vec::new(),
        sizes: Vec::new(),
        reaches: Vec::new(),
        hinting: Hinting::Light,
    })
    .expect("rasterised");
    let g = cluster(&o, &key).expect("x and its marks");
    assert!(!g.wide && g.ink.is_none(), "nothing past the cell");
    let cols = |y: usize| -> Vec<usize> { (0..12).filter(|&x| g.pixels[y * 12 + x] > 0).collect() };
    let rows: Vec<usize> = (0..8).filter(|&y| !cols(y).is_empty()).collect();
    assert!(rows.len() >= 2, "both marks above x: {rows:?}");
    assert!(cols(7).is_empty(), "a row clear between x and its marks");
    for &y in &rows {
        let c = cols(y);
        let (first, last) = (c[0], c[c.len() - 1]);
        assert!(first >= 1 && last <= 9, "row {y} over x's ink: {c:?}");
        assert!((first + last).abs_diff(10) <= 1, "row {y} centred: {c:?}");
    }
}

// ADR-1302 D2: a stretched cluster is stretched as one, its marks' offsets
// included, so a mark stays over its letter. At 20 pixels, a 12-pixel cell,
// a beh advancing 400 units, 7.8 pixels, stretches by 12 / 7.8; HarfRust
// centres its fatha, 200 units wide, over it with an offset. Stretched with
// its offset, the fatha covers columns 3 to 9 of row 7, centred over the
// stretched beh; with its offset left unstretched, columns 2 to 8. A seen
// advancing 1,000 units, 19.5 pixels, takes the one cell the terminal gives
// it (ADR-1200 D21) and is squeezed to fill it, as ADR-1302 D2 squeezes a
// letter wider than its cell
#[test]
fn a_stretched_cluster_keeps_its_marks_over_its_letter() {
    let mut f = ttf::test_mono();
    f.glyphs.push(ttf::Glyph::rect(400, 0, 0, 400, 300));
    f.glyphs.push(ttf::Glyph::rect(0, 0, 400, 200, 500));
    f.glyphs.push(ttf::Glyph::rect(1000, 0, 0, 1000, 300));
    f.cmap.extend([(0x628, 8), (0x64e, 9), (0x633, 10)]);
    f.cmap.sort_unstable();
    let key = ['\u{628}', '\u{64e}'];
    let o = rasterise(&OutlineRequest {
        fit: false,
        synth: Default::default(),
        script: Default::default(),
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: vec![ask(&key, Kind::Marks)],
        baselines: Vec::new(),
        sizes: Vec::new(),
        reaches: Vec::new(),
        hinting: Hinting::Light,
    })
    .expect("rasterised");
    assert_eq!(o.strikes[0].width, 12);
    let g = cluster(&o, &key).expect("beh and fatha");
    let row: Vec<usize> = (0..12).filter(|&x| g.pixels[7 * 12 + x] > 0).collect();
    assert_eq!(
        row,
        (3..10).collect::<Vec<_>>(),
        "the fatha centred over the beh"
    );
    let seen = codepoint(&o, '\u{633}');
    assert!(!seen.wide, "one cell");
    assert!(
        seen.pixels[12 * 12..13 * 12].iter().all(|&p| p == 255),
        "the bar fills its cell"
    );
}

/// The columns of row `y` that `p`, a glyph `w` pixels wide, inks.
fn inked(p: &Placed, w: usize, y: usize) -> Vec<usize> {
    (0..w).filter(|&x| p.pixels[y * w + x] > 0).collect()
}

/// `Test Mono` with an empty GPOS, so that HarfRust positions no mark
/// itself and leaves each at the pen, and with `glyphs` added, each mapped
/// from its codepoint.
fn unpositioned(glyphs: &[(u32, ttf::Glyph)]) -> ttf::Font {
    let mut f = ttf::Font {
        empty_gpos: true,
        ..ttf::test_mono()
    };
    for (cp, g) in glyphs {
        let id = u16::try_from(f.glyphs.len()).expect("a glyph id");
        f.glyphs.push(g.clone());
        f.cmap.push((*cp, id));
    }
    f.cmap.sort_unstable();
    f
}

// ADR-1302 D3, a mark the shaper left at the pen, in right-to-left text.
// A mark follows its base in the order the text is written (Unicode's
// combining character sequence), and HarfRust returns a right-to-left
// cluster in visual order, the mark first, so the helper finds the base on
// either side of the mark. The font holds a Hebrew alef (U+05D0) and an
// Arabic alef (U+0627), each a bar from 100 to 500 units across and up to
// 500 units tall, a segol accent (U+0592, class 230, above) and a hamza
// below (U+0655, class 220, below), each a zero-width bar from 0 to 100
// units across and 50 units tall on the baseline. At 20 pixels, the
// baseline on row 16, the letters fill rows 6 to 15. Left at the pen, each
// mark would lie on the baseline at the letter's left edge, over row 15;
// placed, the accent is centred over the letter with a clear row between
// them, and the hamza centred directly under it
// covers: text.overhang
#[test]
fn an_unplaced_mark_in_right_to_left_text_is_placed_over_its_letter() {
    let f = unpositioned(&[
        (0x5d0, ttf::Glyph::rect(600, 100, 0, 500, 500)),
        (0x627, ttf::Glyph::rect(600, 100, 0, 500, 500)),
        (0x592, ttf::Glyph::rect(0, 0, 0, 100, 50)),
        (0x655, ttf::Glyph::rect(0, 0, 0, 100, 50)),
    ]);
    let (accent, hamza) = (['\u{5d0}', '\u{592}'], [ALEF, '\u{655}']);
    let o = rasterise(&OutlineRequest {
        fit: false,
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        clusters: [accent, hamza]
            .iter()
            .map(|k| vtium_config::shaping::ask(&key(k)))
            .collect(),
        ..OutlineRequest::default()
    })
    .expect("rasterised");
    let (w, baseline) = (o.strikes[0].width as usize, o.strikes[0].baseline);
    assert_eq!((w, baseline), (12, 16));
    for (k, above) in [(&accent, true), (&hamza, false)] {
        let g = cluster(&o, k).unwrap_or_else(|| panic!("{k:?} shaped"));
        // the letter's columns, in a row only it inks
        let letter = inked(g, w, 10);
        assert!(!letter.is_empty(), "{k:?}: the letter in row 10");
        let (top, bottom) = (6, 15);
        for y in top..=bottom {
            assert_eq!(
                inked(g, w, y),
                letter,
                "{k:?}: row {y} holds the letter alone"
            );
        }
        let outside: Vec<usize> = (0..20)
            .filter(|&y| (y < top || y > bottom) && !inked(g, w, y).is_empty())
            .collect();
        assert!(
            !outside.is_empty(),
            "{k:?}: the mark outside the letter's rows"
        );
        for &y in &outside {
            assert_eq!(y < top, above, "{k:?}: the mark's side, row {y}");
            let c = inked(g, w, y);
            let middle = letter[0] + letter[letter.len() - 1];
            assert!(
                (c[0] + c[c.len() - 1]).abs_diff(middle) <= 1,
                "{k:?}: row {y} centred over {letter:?}: {c:?}"
            );
        }
        if above {
            assert!(
                inked(g, w, top - 1).is_empty(),
                "a clear row under the accent"
            );
        } else {
            assert_eq!(
                outside[0],
                bottom + 1,
                "the hamza directly under the letter"
            );
        }
    }
}

// ADR-1302 D3: a letter the font draws as several glyphs is one base for
// an unplaced mark. The font has no glyph for d with dot below (U+1E0D),
// so HarfRust decomposes it into d and a dot below (U+0323), both glyphs
// numbered by the U+1E0D they come from, as JetBrains Mono's ccmp draws
// that letter as d and a dot. A circumflex (U+0302, class 230) after it is
// left at the pen. Its base is the d and the dot together: centred over
// their ink, a clear row above the d. Taken from the dot alone, the
// circumflex would sit a row above the dot, inside the d. The d is a bar
// from 100 to 500 units across and 500 tall, rows 6 to 15 at 20 pixels;
// the dot lies from 250 to 350 units across under the baseline; the
// circumflex is a zero-width bar 200 units wide and 50 tall
// covers: text.overhang
#[test]
fn a_letter_drawn_as_several_glyphs_is_one_base_for_an_unplaced_mark() {
    let f = unpositioned(&[
        (0x64, ttf::Glyph::rect(600, 100, 0, 500, 500)),
        (0x323, ttf::Glyph::rect(0, -350, -150, -250, -50)),
        (0x302, ttf::Glyph::rect(0, 0, 0, 200, 50)),
    ]);
    let k = ['\u{1e0d}', '\u{302}'];
    let o = rasterise(&OutlineRequest {
        fit: false,
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        clusters: vec![ask(&k, Kind::Marks)],
        ..OutlineRequest::default()
    })
    .expect("rasterised");
    let w = o.strikes[0].width as usize;
    let g = cluster(&o, &k).expect("d with dot below and a circumflex");
    let d = inked(g, w, 10);
    assert!(!d.is_empty(), "the d in row 10");
    for y in 6..=15 {
        assert_eq!(inked(g, w, y), d, "row {y} holds the d alone");
    }
    assert!(!inked(g, w, 17).is_empty(), "the dot under the baseline");
    let above: Vec<usize> = (0..6).filter(|&y| !inked(g, w, y).is_empty()).collect();
    assert!(!above.is_empty(), "the circumflex above the d");
    assert!(
        !above.contains(&5),
        "a clear row between the d and the circumflex"
    );
    for &y in &above {
        let c = inked(g, w, y);
        assert!(
            (c[0] + c[c.len() - 1]).abs_diff(d[0] + d[d.len() - 1]) <= 1,
            "row {y} centred over the d's columns {d:?}: {c:?}"
        );
    }
}

/// The subset of Noto Naskh Arabic in `tests/fixtures/`.
fn naskh() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/NotoNaskhArabic-beh-fatha.ttf"),
    )
    .expect("the fixture")
}

// A joining form the font draws as several glyphs from the letter's own
// character is kept: Noto Naskh Arabic draws beh's joined forms as a
// dotless body and a separate dot, both from the beh. Beh's initial,
// medial and final forms, and its initial form with a fatha, each draw one
// cell wide, each unlike the others and unlike the isolated beh, and the
// fatha adds ink above the initial form where the bare form has none
#[test]
fn a_joining_form_the_font_draws_as_several_glyphs_is_kept() {
    const FATHA: char = '\u{64e}';
    let forms: [&[char]; 4] = [
        &[BEH, ZWJ],
        &[ZWJ, BEH, ZWJ],
        &[ZWJ, BEH],
        &[BEH, FATHA, ZWJ],
    ];
    let o = rasterise(&OutlineRequest {
        fit: false,
        font: naskh().into(),
        cells: vec![(12, 24)],
        clusters: forms
            .iter()
            .map(|k| vtium_config::shaping::ask(&key(k)))
            .collect(),
        ..OutlineRequest::default()
    })
    .expect("rasterised");
    let mut drawn = vec![codepoint(&o, BEH).pixels.clone()];
    for k in forms {
        assert_eq!(vtium_config::shaping::kind(&key(k)), Kind::Joining, "{k:?}");
        let g = cluster(&o, k).unwrap_or_else(|| panic!("{k:?} kept"));
        assert!(!g.wide && !g.color, "{k:?}: one cell, a mask");
        drawn.push(g.pixels.clone());
    }
    let bare = cluster(&o, forms[0]).expect("the initial beh");
    let vocalised = cluster(&o, forms[3]).expect("the initial beh with a fatha");
    drawn.sort();
    drawn.dedup();
    assert_eq!(drawn.len(), 5, "each form drawn apart");
    let fatha =
        (0..24).any(|y| inked(bare, 12, y).is_empty() && !inked(vocalised, 12, y).is_empty());
    assert!(fatha, "the fatha's ink where the bare form has none");
}

// A joining cluster that the font draws as its letter alone draws is left
// out, though no scalar value maps to the glyph it shapes to: this font
// replaces beh's glyph under `ccmp` wherever beh stands and has no joined
// forms, so beh keyed with U+200D on either side shapes to the same glyph
// as beh without it, and the console draws beh by its codepoint instead
#[test]
fn a_joining_cluster_drawn_as_its_letter_alone_is_left_out() {
    let mut f = unpositioned(&[
        (0x628, ttf::Glyph::rect(600, 100, 0, 500, 300)),
        (0xe000, ttf::Glyph::rect(600, 100, 0, 500, 400)),
    ]);
    let id = |cp: u32| f.cmap.iter().find(|(c, _)| *c == cp).expect("mapped").1;
    let (beh, other) = (id(0x628), id(0xe000));
    f.gsub = vec![(*b"ccmp", ttf::Lookup::Single(vec![(beh, other)]))];
    let keys = [[BEH, ZWJ], [ZWJ, BEH]];
    let o = rasterise(&OutlineRequest {
        fit: false,
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        clusters: keys
            .iter()
            .map(|k| vtium_config::shaping::ask(&key(k)))
            .collect(),
        ..OutlineRequest::default()
    })
    .expect("rasterised");
    assert!(
        o.strikes[0].clusters.is_empty(),
        "{:x?}",
        o.strikes[0].clusters
    );
}

// A right-joining letter keyed with U+200D on the side it does not join is
// no joining form, though shaping changes its glyph: HarfRust composes
// alef (U+0627) and hamza above (U+0654) into alef with hamza above
// (U+0623), which the font holds. Alef joins only to the letter before it,
// so alef and hamza with U+200D after them draw as they do without the
// joiner, and are left out; with U+200D before them they take alef's final
// form, the font's `fina` substitution, and are kept
#[test]
fn a_letter_composed_with_its_mark_and_not_joined_is_left_out() {
    let mut f = unpositioned(&[
        (0x627, ttf::Glyph::rect(600, 250, 0, 350, 500)),
        (0x654, ttf::Glyph::rect(0, -350, 600, -250, 700)),
        (0x623, ttf::Glyph::rect(600, 250, 0, 350, 700)),
        (0xe000, ttf::Glyph::rect(600, 0, 0, 350, 700)),
    ]);
    let id = |cp: u32| f.cmap.iter().find(|(c, _)| *c == cp).expect("mapped").1;
    let (composed, fina) = (id(0x623), id(0xe000));
    f.gsub = vec![(*b"fina", ttf::Lookup::Single(vec![(composed, fina)]))];
    let (after, before) = ([ALEF, '\u{654}', ZWJ], [ZWJ, ALEF, '\u{654}']);
    let o = rasterise(&OutlineRequest {
        fit: false,
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        clusters: [after, before]
            .iter()
            .map(|k| vtium_config::shaping::ask(&key(k)))
            .collect(),
        ..OutlineRequest::default()
    })
    .expect("rasterised");
    assert!(
        cluster(&o, &after).is_none(),
        "alef and hamza, U+200D after them"
    );
    assert!(
        cluster(&o, &before).is_some(),
        "alef and hamza in alef's final form"
    );
}
