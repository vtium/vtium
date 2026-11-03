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
        font: ttf::test_shaping().build().into(),
        family: "Test Shaping".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: asks,
        baselines: Vec::new(),
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

// ADR-1302 D3 (the maintainer, 2026-11-01): lam with shadda and fatha in
// DejaVu Sans Mono (`tests/fixtures/`), drawn through the rasteriser at the
// 9x16 cell the console gives the font, hinted lightly by default. The
// subset has no printable ASCII, so the baseline is the ascent rounded up,
// row 13. The lam alone has blank rows above it and its stem starts at row
// 3; its marks would pass the row's top, so the whole stack is scaled to
// fit: the marks reach the top row, the lam's stem starts lower than it
// does alone, a blank row parts the marks from the stem, and the lam keeps
// its bottom row. `src/shaping/shape.rs` pins the same stack's edges at
// full hinting
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
        font: font.into(),
        cells: vec![(9, 16)],
        clusters: vec![ask(&stack, Kind::Marks)],
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
/// near the row's bottom, to 400, and a kasratan (U+064D) 150 tall.
fn marked(keys: &[&[char]], height: u32) -> OutlineRequest {
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
    f.cmap.sort_unstable();
    OutlineRequest {
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, height)],
        cover: Vec::new(),
        clusters: keys.iter().map(|k| ask(k, Kind::Marks)).collect(),
        baselines: Vec::new(),
        hinting: Hinting::Light,
    }
}

/// Row `y` of a glyph one 12-pixel cell wide.
fn row(p: &Placed, y: usize) -> &[u8] {
    &p.pixels[y * 12..(y + 1) * 12]
}

// ADR-1302 D3: a mark that passes the row and can move in without its ink
// touching its letter's moves toward the baseline until it fits, its
// letter untouched. In the 12x20 cell, baseline 16, x's ink fills rows 8
// to 15, and the dot above, placed from 9 to 16.4 pixels up, ends a row
// over the top; moved in, it fills rows 0 to 7
// covers: text.overhang
#[test]
fn a_mark_that_passes_the_row_moves_in() {
    let o = rasterise(&marked(&[&['x', '\u{307}']], 20)).expect("rasterised");
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

// ADR-1302 D3 (the maintainer, 2026-11-01): where the marks cannot move
// in without landing more of their ink on the letter's, or cannot fit at
// all, the whole stack, letter and marks, is scaled vertically just
// enough to fit the row, keeping the font's arrangement. Here the letter
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
    let o = rasterise(&marked(&[&['A', '\u{323}']], 32)).expect("rasterised");
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
// (review finding A9, through rasterise): its pen goes half the cell in,
// as the C reference's centre_dx clamped the advance at 0. Test Mono with
// an x, a bar from 100 to 500 units across and 400 tall, and a dot above,
// at 20 pixels: as the font gives it,
// x advances 600 units, 11.7 pixels, which rounds to the 12-pixel cell,
// so its pen is on the cell's left edge. With a GPOS adjustment of -1600
// units to x's advance the cluster's pen sum is -1000 units, and its pen
// is 6 columns in, where an unclamped advance of -19.5 pixels would put
// it 16 columns in, past the cell
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
            font: font(by).into(),
            family: "Test Mono".to_owned(),
            cells: vec![(12, 20)],
            cover: Vec::new(),
            clusters: vec![ask(&key, Kind::Marks)],
            baselines: Vec::new(),
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
/// places just above the alef, past the row's top.
fn alef_damma() -> OutlineRequest {
    let mut f = ttf::test_mono();
    f.glyphs.push(ttf::Glyph::rect(600, 250, 0, 350, 760));
    f.glyphs.push(ttf::Glyph::rect(0, -400, 0, -200, 300));
    f.cmap.push((0x627, 8));
    f.cmap.push((0x64f, 9));
    f.cmap.sort_unstable();
    OutlineRequest {
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: vec![ask(&['\u{627}', '\u{64f}'], Kind::Marks)],
        baselines: Vec::new(),
        hinting: Hinting::Light,
    }
}

// ADR-1302 D3 for right-to-left text: HarfRust gives the cluster in
// visual order, the damma before its alef, and the damma is a mark by its
// character. At 20 pixels the alef's ink reaches 14.8 pixels above the
// baseline, row 1, and the damma lies wholly above the row's top, so the
// row's top row is clear unless the mark rule brings the damma in
// covers: text.overhang
#[test]
fn a_right_to_left_mark_is_fitted_to_the_row() {
    let o = rasterise(&alef_damma()).expect("rasterised");
    let alef = codepoint(&o, '\u{627}');
    let g = cluster(&o, &['\u{627}', '\u{64f}']).expect("alef and damma");
    let inked = |p: &Placed, y: usize| p.pixels[y * 12..(y + 1) * 12].iter().any(|&v| v > 0);
    assert!(!inked(alef, 0), "the alef alone leaves the top row clear");
    assert!(inked(g, 0), "the damma brought into the top row");
}

// a beh keyed with U+200D after it, as the console asks for it, is judged
// a joining form (review finding B4): drawn one cell wide in its initial
// form, a bar from 300 to 600 units, stretched to its 12-pixel cell from
// column 6 to its right edge
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
// joining form (review finding 2): the fatha, whose Joining_Type is T,
// does not make the cluster an emoji sequence, whose one-glyph rule would
// drop it. Each order draws one cell wide, the beh in its joined form
// with the fatha above it: the bar from 0 to 500 units fills rows 6 to 15
// of the 20-pixel cell, and the fatha, a bar from 600 to 700 units, ink
// in rows 2 to 4, where the beh alone has none
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
// HarfRust leaves an acute after a circumflex in DejaVu Sans Mono (review
// finding B5), goes where the C reference put it (render_cluster):
// centred over its base's ink, a row above all the ink since the base.
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
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: vec![ask(&key, Kind::Marks)],
        baselines: Vec::new(),
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

// ADR-1302 D2: a stretched cluster is stretched as one, its marks'
// offsets included, so a mark stays over its letter (review finding B8).
// At 20 pixels, a 12-pixel cell, a beh advancing 400 units, 7.8 pixels,
// stretches by 12 / 7.8; HarfRust centres its fatha, 200 units wide, over
// it with an offset. Stretched with its offset, the fatha covers columns 3
// to 9 of row 7, centred over the stretched beh; with its offset left
// unstretched, columns 2 to 8. A seen advancing 1,000 units, 19.5 pixels,
// is two cells wide, and stretched to fill both
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
        font: f.build().into(),
        family: "Test Mono".to_owned(),
        cells: vec![(0, 20)],
        cover: Vec::new(),
        clusters: vec![ask(&key, Kind::Marks)],
        baselines: Vec::new(),
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
    assert!(seen.wide, "two cells");
    assert!(
        seen.pixels[12 * 24..13 * 24].iter().all(|&p| p == 255),
        "the bar fills both cells"
    );
}
