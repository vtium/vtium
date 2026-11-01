//! Outline fonts rasterised by swash (ADR-1200 D13, D14), on a TrueType
//! font written here: the cell an outline font sets, each glyph placed in
//! it as R8 coverage or premultiplied RGBA, one or two cells wide, the
//! answer's check, and the same through the parser's child.

#[path = "common/parser.rs"]
mod parser;
#[path = "common/ttf.rs"]
mod ttf;

use std::path::Path;

use parser::Parser;
use vtium_vitrify::outline::{
    Outline, OutlineRequest, baseline, cell_width, decorations, rasterise,
};
use vtium_vitrify::resample::{self, METHOD};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-vitrify");

fn request(cells: &[(u32, u32)], cover: &[(u32, u32)]) -> OutlineRequest {
    OutlineRequest {
        font: ttf::test_mono().build().into(),
        family: "Test Mono".to_owned(),
        cells: cells.to_vec(),
        cover: cover.to_vec(),
        clusters: Vec::new(),
        baselines: Vec::new(),
    }
}

fn glyph(o: &Outline, strike: usize, c: u32) -> &vtium_vitrify::outline::Placed {
    let s = &o.strikes[strike];
    let (_, g) = s.map.iter().find(|(cp, _)| *cp == c).expect("mapped");
    &s.glyphs[*g as usize]
}

// The font sets the cell from W, its ascent and descent span the height
// less half a pixel, and the baseline sits the ascent below the top (the
// C reference's `font_freetype.c` flow)
#[test]
fn the_first_outline_font_sets_the_cell() {
    let o = rasterise(&request(&[(0, 10), (0, 20)], &[])).expect("rasterised");
    assert_eq!(o.family, "Test Mono");
    assert_eq!(o.format, "outline+color");
    assert_eq!(o.license, "Free for testing, no warranty.");
    let cells: Vec<_> = o
        .strikes
        .iter()
        .map(|s| (s.width, s.height, s.baseline))
        .collect();
    // 9.5 and 19.5 pixels to the em: W advances 5.7 and 11.7, the ascent
    // 7.6 and 15.6
    assert_eq!(cells, [(6, 10, 8), (12, 20, 16)]);
    let mapped: Vec<u32> = o.strikes[0].map.iter().map(|&(c, _)| c).collect();
    assert_eq!(mapped, [0x20, 0x41, 0x57, 0x4e00, 0xe0a0, 0x1f600]);
    // A and U+E0A0 share their glyph
    let s = &o.strikes[0];
    let at = |c| s.map.iter().find(|m| m.0 == c).map(|m| m.1);
    assert_eq!(at(0x41), at(0xe0a0));
}

// ADR-1200 D17: an outline font's own cell is as wide as the W rule gives
// at the height asked, the widest of the faces given, as the C reference
// took the larger of its regular and bold faces' cells. Test Mono's W
// advances 600 of 1000 units and the bold one's 700: at 16 pixels, 15.5
// to the em, 9.3 inked over 10 pixels and 10.85 over 11
#[test]
fn an_outline_cell_is_as_wide_as_the_widest_faces_w() {
    let regular = ttf::test_mono().build();
    let mut bold = ttf::test_mono();
    bold.glyphs[3] = ttf::Glyph::rect(700, 0, 0, 700, 700);
    let bold = bold.build();
    let (r, b) = ((&regular[..], "Test Mono"), (&bold[..], "Test Mono"));
    assert_eq!(cell_width(&[r], 16), Ok(10));
    assert_eq!(cell_width(&[b], 16), Ok(11));
    assert_eq!(cell_width(&[r, b], 16), Ok(11));
    assert_eq!(cell_width(&[b, r], 16), Ok(11));
    // the strike rasterise draws at a width of 0 takes the same width
    let o = rasterise(&request(&[(0, 16)], &[])).expect("rasterised");
    assert_eq!(o.strikes[0].width, 10);
    // a face that cannot be read fails the answer, and so does no face or
    // a height of 0
    let other = cell_width(&[r, (&regular[..], "Other Sans")], 16).expect_err("another family");
    assert!(other.contains("Other Sans"), "{other}");
    assert!(cell_width(&[], 16).is_err());
    assert!(cell_width(&[r], 0).is_err());
}

// The W rule measures W's ink on the unhinted outline, as the C
// reference's FreeType light hinting left a TrueType glyph's width: a
// subset of DejaVu Sans Mono keeping its hinting instructions
// (`tests/fixtures/README.md`), whose W full hinting snaps to 8 columns
// at 16 pixels where its outline covers 9
#[test]
fn the_w_rule_measures_ink_unhinted_on_a_hinted_font() {
    let font = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DejaVuSansMono-W-M-0.ttf"),
    )
    .expect("the fixture");
    let face = (&font[..], "DejaVu Sans Mono");
    assert_eq!(cell_width(&[face], 16), Ok(9));
    assert_eq!(cell_width(&[face], 22), Ok(12));
}

// A face with no W takes M's width, then 0's, then half the height. At 16
// pixels, 15.5 to the em: an M advancing 700 units of 1000 covers 11
// columns, a 0 advancing 400 covers 7 (6.2 pixels), and half of 16 is 8
#[test]
fn a_face_with_no_w_takes_m_then_0_then_half_the_height() {
    let face = |cmap: &[(u32, u16)]| {
        let mut f = ttf::test_mono();
        f.glyphs.push(ttf::Glyph::rect(700, 0, 0, 700, 700));
        f.glyphs.push(ttf::Glyph::rect(400, 0, 0, 400, 700));
        f.cmap = cmap.to_vec();
        f.build()
    };
    let (m, zero) = ((0x4d, 8), (0x30, 9));
    let both = face(&[(0x20, 1), zero, m]);
    let only_zero = face(&[(0x20, 1), zero]);
    let neither = face(&[(0x20, 1), (0x41, 2)]);
    let width = |font: &[u8]| cell_width(&[(font, "Test Mono")], 16);
    assert_eq!(width(&both), Ok(11));
    assert_eq!(width(&only_zero), Ok(7));
    assert_eq!(width(&neither), Ok(8));
}

// The W rule on DejaVu Sans Mono as the machine installs it: 9 wide at 16
// pixels and 12 at 22, the cells the C reference's FreeType backend gave
// it. It reads a font outside vtium's tree, so it runs only when asked:
// `cargo test -p vtium-vitrify --test outline -- --ignored`
#[test]
#[ignore = "reads DejaVu Sans Mono from the machine's fonts"]
fn dejavu_sans_mono_takes_the_w_rules_cell() {
    let read = |file: &str| {
        ["/usr/share/fonts/TTF", "/usr/share/fonts/truetype/dejavu"]
            .into_iter()
            .map(|dir| Path::new(dir).join(file))
            .find(|p| p.is_file())
            .map(|p| std::fs::read(p).expect("read"))
            .expect("DejaVu Sans Mono installed")
    };
    let (regular, bold) = (read("DejaVuSansMono.ttf"), read("DejaVuSansMono-Bold.ttf"));
    let face = (&regular[..], "DejaVu Sans Mono");
    assert_eq!(cell_width(&[face], 16), Ok(9));
    assert_eq!(cell_width(&[face], 22), Ok(12));
    // the bold face's W is the regular one's, so it takes the same cell
    let both = [face, (&bold[..], "DejaVu Sans Mono")];
    assert_eq!(cell_width(&both, 16), Ok(9));
    assert_eq!(cell_width(&both, 22), Ok(12));
}

#[test]
fn an_outline_glyph_is_r8_on_the_baseline() {
    let o = rasterise(&request(&[(0, 20)], &[])).expect("rasterised");
    let a = glyph(&o, 0, 0x41);
    assert!(!a.wide && !a.color);
    assert_eq!(a.pixels.len(), 12 * 20);
    let row = |y: usize| &a.pixels[y * 12..(y + 1) * 12];
    // the bar spans 700 units up from the baseline, row 16: rows 3 to 15
    assert!(row(0).iter().all(|&p| p == 0), "above the bar");
    assert!(row(16).iter().all(|&p| p == 0), "below the baseline");
    assert_eq!(row(10)[5], 255, "inside the bar");
    assert_eq!(row(10)[11], 0, "right of the bar");
    // coverage between 0 and 255 where an edge cuts a pixel
    assert!(a.pixels.iter().any(|&p| p > 0 && p < 255));
    let space = glyph(&o, 0, 0x20);
    assert!(space.pixels.iter().all(|&p| p == 0));
}

#[test]
fn a_glyph_advancing_past_one_and_a_half_cells_is_wide() {
    let o = rasterise(&request(&[(0, 20)], &[])).expect("rasterised");
    let wide = glyph(&o, 0, 0x4e00);
    assert!(wide.wide && !wide.color);
    assert_eq!(wide.pixels.len(), 24 * 20);
    assert!(!glyph(&o, 0, 0x57).wide);
}

// covers: text.emoji
#[test]
fn a_color_glyph_is_premultiplied_rgba_two_cells_wide() {
    let o = rasterise(&request(&[(0, 20)], &[])).expect("rasterised");
    let e = glyph(&o, 0, 0x1f600);
    assert!(e.wide && e.color);
    assert_eq!(e.pixels.len(), 24 * 20 * 4);
    let px = |x: usize, y: usize| {
        let at = (y * 24 + x) * 4;
        <[u8; 4]>::try_from(&e.pixels[at..at + 4]).expect("a pixel")
    };
    for p in e.pixels.chunks_exact(4) {
        assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "{p:?}");
    }
    // the red square's corner, then the blue square over it at half alpha
    let [r, g, b, a] = px(4, 6);
    assert_eq!((r, g, b, a), (255, 0, 0, 255));
    let [r, g, b, a] = px(12, 10);
    assert_eq!((g, a), (0, 255));
    assert!(
        (120..=136).contains(&r) && (120..=136).contains(&b),
        "{r} {b}"
    );
    assert_eq!(px(0, 0), [0; 4], "outside both");
}

/// Test Mono with `L` (U+004C), a bar from 150 units left of its pen to
/// 450 right of it, advancing 600, whose ink passes its cell's left edge,
/// and `R` (U+0052), a bar from 0 to `right` units.
fn overhanging(right: i16) -> OutlineRequest {
    let mut font = ttf::test_mono();
    font.glyphs.push(ttf::Glyph::rect(600, -150, 0, 450, 700));
    font.glyphs.push(ttf::Glyph::rect(600, 0, 0, right, 700));
    font.cmap.push((0x4c, 8));
    font.cmap.push((0x52, 9));
    font.cmap.sort_unstable();
    OutlineRequest {
        font: font.build().into(),
        ..request(&[(0, 20)], &[])
    }
}

// ADR-1302 D1, D6: ink past a glyph's cell is kept in its ink box, up to
// one cell past it, and the glyph's own pixels stay cut to its cell. At
// 19.5 pixels to the em, L spans -2.9 to 8.8 pixels: its box starts 3
// columns left of its 12-pixel cell and ends at the cell's right edge
// covers: text.overhang
#[test]
fn ink_past_a_cell_is_kept_in_the_glyphs_ink_box() {
    let o = rasterise(&overhanging(600)).expect("rasterised");
    let l = glyph(&o, 0, 0x4c);
    assert_eq!(l.pixels.len(), 12 * 20);
    let ink = l.ink.as_ref().expect("L's box");
    assert_eq!((ink.left, ink.width), (-3, 15));
    let at = |y: usize, x: usize| ink.pixels[y * 15 + x];
    // the box's columns 3 to 14 are the cell's own pixels
    for y in 0..20 {
        assert_eq!(
            &ink.pixels[y * 15 + 3..y * 15 + 15],
            &l.pixels[y * 12..y * 12 + 12]
        );
    }
    assert_eq!(at(10, 1), 255, "ink two columns left of the cell");
    assert!(
        at(10, 0) > 0 && at(10, 0) < 255,
        "the bar's edge, 0.9 of a pixel in"
    );
    // A, W and the space keep their ink in their cells
    for c in [0x41, 0x57, 0x20] {
        assert!(glyph(&o, 0, c).ink.is_none(), "{c:#x}");
    }
}

// ink past one cell beyond the glyph's is cut at the reach, and nothing
// moves a glyph to keep its ink in (ADR-1302 D1, D2): R, a bar 2000 units
// wide, 39 pixels, stays at its pen and is cut one cell past its own
#[test]
fn ink_past_the_reach_is_cut() {
    let o = rasterise(&overhanging(2000)).expect("rasterised");
    let r = glyph(&o, 0, 0x52);
    let ink = r.ink.as_ref().expect("R's box");
    assert_eq!((ink.left, ink.width), (0, 24), "to one cell past its own");
    assert!(
        ink.pixels[..24].iter().all(|&p| p == 0),
        "nothing above the bar"
    );
    assert!(
        ink.pixels[10 * 24..11 * 24].iter().all(|&p| p == 255),
        "the bar fills the reach"
    );
}

// ADR-1302 D2: the pen starts at the cell's left edge, moved by half of
// what the cells exceed the advance rounded, rounded down. At 19.5 pixels
// to the em: U+4E00 advances 19.5, rounded 20, in its two 12-pixel cells,
// so its pen is 2 pixels in and its ink, from 0.98 pixels past the pen,
// starts in column 2; C advances 750 units, 14.6 rounded 15, in one cell,
// so its pen moves half of -3, rounded down, 2 pixels left, and its ink
// runs from 2 left of the cell to 12.6
// covers: text.overhang
#[test]
fn a_glyph_is_placed_by_its_rounded_advance() {
    let mut font = ttf::test_mono();
    font.glyphs.push(ttf::Glyph::rect(750, 0, 0, 750, 700));
    font.cmap.push((0x43, 8));
    font.cmap.sort_unstable();
    let o = rasterise(&OutlineRequest {
        font: font.build().into(),
        ..request(&[(0, 20)], &[])
    })
    .expect("rasterised");
    let column = |g: &vtium_vitrify::outline::Placed, w: usize, x: usize| {
        (0..20).map(|y| g.pixels[y * w + x]).max().unwrap_or(0)
    };
    let wide = glyph(&o, 0, 0x4e00);
    assert_eq!(column(wide, 24, 1), 0, "left of the pen's ink");
    assert!(column(wide, 24, 2) > 0, "the ink's first column");
    assert!(wide.ink.is_none(), "its ink lies inside its cells");
    let c = glyph(&o, 0, 0x43);
    let ink = c.ink.as_ref().expect("C's box");
    assert_eq!(
        (ink.left, ink.width),
        (-2, 15),
        "from 2 left of the cell to 12.6"
    );
    // W: an advance of 11.7, rounded 12, in a 12-pixel cell keeps the pen
    // on the cell's left edge, as the C reference drew it
    assert!(column(glyph(&o, 0, 0x57), 12, 0) > 0);
}

// ADR-1302 D6: a glyph whose box would pass the 16 KiB chunk keeps its
// pixels cut to its cell and carries no box. At 128 pixels high, a 77-pixel
// cell, R's box would be the whole reach, 231 by 128 pixels, where L's,
// from 19.1 pixels left of its cell, 97 by 128, fits
#[test]
fn a_box_past_the_chunk_bound_leaves_the_glyph_cut_to_its_cell() {
    let o = rasterise(&OutlineRequest {
        cells: vec![(0, 128)],
        ..overhanging(2000)
    })
    .expect("rasterised");
    let s = &o.strikes[0];
    assert_eq!(s.width, 77);
    let r = glyph(&o, 0, 0x52);
    assert!(r.ink.is_none());
    assert_eq!(r.pixels.len(), 77 * 128);
    let l = glyph(&o, 0, 0x4c);
    assert_eq!(l.ink.as_ref().map(|i| (i.left, i.width)), Some((-20, 97)));
}

// the parent checks each ink box in the child's answer: R8 only, inside
// the glyph's reach and past its cells, and no larger than a chunk
#[test]
fn an_answer_with_a_box_past_the_reach_is_refused() {
    let req = overhanging(600);
    let o = rasterise(&req).expect("rasterised");
    assert_eq!(Outline::decode(&o.encode(), &req), Ok(o.clone()));
    let l = o.strikes[0].map.iter().find(|m| m.0 == 0x4c).expect("L").1 as usize;
    let mut bad = o.clone();
    let ink = bad.strikes[0].glyphs[l].ink.as_mut().expect("a box");
    ink.left = -13;
    ink.width = 25;
    ink.pixels = vec![0; 25 * 20];
    assert!(
        Outline::decode(&bad.encode(), &req).is_err(),
        "a box past the reach"
    );
    let mut inside = o.clone();
    let ink = inside.strikes[0].glyphs[l].ink.as_mut().expect("a box");
    (ink.left, ink.width, ink.pixels) = (0, 12, vec![0; 12 * 20]);
    assert!(
        Outline::decode(&inside.encode(), &req).is_err(),
        "a box of the cell alone"
    );
    let mut short = o.clone();
    short.strikes[0].glyphs[l]
        .ink
        .as_mut()
        .expect("a box")
        .pixels
        .pop();
    assert!(
        Outline::decode(&short.encode(), &req).is_err(),
        "a box cut short"
    );
}

/// Test Mono with an underscore from 200 to 100 units below the baseline
/// and `a` from it to 500 above.
fn underscored() -> Vec<u8> {
    let mut font = ttf::test_mono();
    font.glyphs.push(ttf::Glyph::rect(600, 0, -200, 600, -100));
    font.glyphs.push(ttf::Glyph::rect(600, 100, 0, 500, 500));
    font.cmap.push((0x5f, 8));
    font.cmap.push((0x61, 9));
    font.cmap.sort_unstable();
    font.build()
}

// ADR-1302 D3: the baseline is the ascent below the cell's top, rounded
// up, raised by the whole pixels the lowest ink of printable ASCII passes
// the cell's bottom. At 9.5, 11.5 and 19.5 pixels to the em the ascent is
// 7.6, 9.2 and 15.6, rounded 8, 10 and 16, and the underscore's ink ends
// 1.9, 2.3 and 3.9 pixels below it, in 2, 3 and 4 rows: at 12 pixels that
// is row 13 of 12, so the baseline rises a row there and nowhere else
// covers: text.overhang
#[test]
fn the_baseline_rises_where_ascii_ink_passes_the_cell() {
    let font = underscored();
    let at = |h| baseline(&font, "Test Mono", h).expect("a baseline");
    assert_eq!([at(10), at(12), at(20)], [8, 9, 16]);
    let o = rasterise(&OutlineRequest {
        font: font.clone().into(),
        cells: vec![(0, 10), (0, 12), (0, 20)],
        cover: vec![(0x61, 0x61)],
        ..request(&[], &[])
    })
    .expect("rasterised");
    let bases: Vec<u32> = o.strikes.iter().map(|s| s.baseline).collect();
    assert_eq!(bases, [8, 9, 16], "whatever the request covers");
    // the underscore ends in the cell's bottom row, and a's ink stands on
    // the baseline
    let o = rasterise(&OutlineRequest {
        font: font.into(),
        cells: vec![(0, 12)],
        ..request(&[], &[])
    })
    .expect("rasterised");
    let s = &o.strikes[0];
    let row_inked = |c: u32, y: usize| {
        let g = glyph(&o, 0, c);
        g.pixels[y * s.width as usize..(y + 1) * s.width as usize]
            .iter()
            .any(|&p| p > 0)
    };
    assert!(row_inked(0x5f, 11), "the underscore's last row in the cell");
    assert!(
        row_inked(0x61, 8) && !row_inked(0x61, 9),
        "a ends on the baseline"
    );
}

// ADR-1302 D3 and the C reference's kmscon_font_freetype_get_deco: the
// underline's centre is FreeType's underline_position below the baseline,
// post's underlinePosition less half its underlineThickness. Test Mono at
// 20 pixels, 0.0195 of a pixel to the unit, baseline 16, with an underline
// at -100 units and 50 thick: a row thick, round(0.975); its centre 16 +
// (100 + 25) * 0.0195 = 18.44, its top row floor(18.44) = 18
// covers: text.overhang
#[test]
fn the_underline_is_centred_below_the_baseline_as_freetype_gives_it() {
    let font = ttf::Font {
        underline: Some((-100, 50)),
        ..ttf::test_mono()
    }
    .build();
    let d = decorations(&font, "Test Mono", 20, 16).expect("the lines");
    assert_eq!((d.underline, d.underline_thickness), (18, 1));
}

#[test]
fn a_later_source_draws_at_the_cell_it_is_given_and_only_what_it_covers() {
    let o =
        rasterise(&request(&[(8, 16)], &[(0x41, 0x41), (0x1f600, 0x1f64f)])).expect("rasterised");
    let s = &o.strikes[0];
    assert_eq!((s.width, s.height), (8, 16));
    let mapped: Vec<u32> = s.map.iter().map(|&(c, _)| c).collect();
    assert_eq!(mapped, [0x41, 0x1f600]);
    // the bar, 3.1 to 6.2 pixels across, stays inside the narrower cell
    assert_eq!(glyph(&o, 0, 0x41).pixels.len(), 8 * 16);
}

#[test]
fn a_font_of_another_family_or_none_at_all_is_refused() {
    let mut r = request(&[(0, 16)], &[]);
    r.family = "Other Sans".to_owned();
    let e = rasterise(&r).expect_err("another family");
    assert!(e.contains("Test Mono") && e.contains("Other Sans"), "{e}");
    r.family = "test-mono".to_owned();
    rasterise(&r).expect("case, blanks and hyphens ignored");
    r.font = b"not a font at all".to_vec().into();
    assert!(rasterise(&r).is_err());
}

#[test]
fn an_answer_reads_back_and_each_malformed_one_is_refused() {
    let req = request(&[(0, 10)], &[]);
    let o = rasterise(&req).expect("rasterised");
    let bytes = o.encode();
    assert_eq!(Outline::decode(&bytes, &req), Ok(o.clone()));
    let refused = |b: &[u8], r: &OutlineRequest, why: &str| {
        let e = Outline::decode(b, r).expect_err(why);
        assert!(e.contains(why), "{why}: {e}");
    };
    let mut extra = bytes.clone();
    extra.push(0);
    refused(&extra, &req, "after the last strike");
    refused(&bytes[..bytes.len() - 1], &req, "cut short");
    // asked at another cell
    refused(&bytes, &request(&[(0, 12)], &[]), "not asked for");
    refused(&bytes, &request(&[(0, 10), (0, 20)], &[]), "each cell");
    // a codepoint the request does not cover
    refused(
        &bytes,
        &request(&[(0, 10)], &[(0x41, 0x41)]),
        "does not cover",
    );
    // the color glyph's first pixel made brighter than its alpha
    let mut bad = o.clone();
    let e = bad.strikes[0]
        .glyphs
        .iter_mut()
        .find(|g| g.color)
        .expect("a color glyph");
    e.pixels[0..4].copy_from_slice(&[9, 0, 0, 8]);
    refused(&bad.encode(), &req, "not premultiplied");
    // more glyphs than the map names
    let mut bad = o.clone();
    let extra = bad.strikes[0].glyphs[0].clone();
    let codepoints = bad.strikes[0].map.len();
    bad.strikes[0].glyphs.resize(codepoints + 1, extra);
    refused(&bad.encode(), &req, "more glyphs than");
    // a map naming a glyph the strike lacks
    let mut bad = o.clone();
    bad.strikes[0].map[0].1 = 99;
    refused(&bad.encode(), &req, "past the count");
}

#[test]
fn the_request_reads_back() {
    let r = request(&[(0, 10), (8, 16)], &[(0x41, 0x5a)]);
    assert_eq!(OutlineRequest::decode(&r.encode()), Ok(r.clone()));
    let mut long = r.encode();
    long.push(0);
    assert!(OutlineRequest::decode(&long).is_err());
}

// The child rasterises what the library does, and a font that is no font
// fails its source with the child's message
#[test]
fn the_parsers_child_rasterises() {
    let parser = Parser::with_program(Path::new(BIN), &["--parse"]);
    let req = request(&[(0, 10)], &[]);
    let o = parser.rasterise(&req).expect("rasterised");
    assert_eq!(o, rasterise(&req).expect("rasterised"));
    let mut bad = req.clone();
    bad.font = vec![0; 64].into();
    let e = parser.rasterise(&bad).expect_err("not a font");
    assert!(e.contains("not a TrueType or OpenType font"), "{e}");
}

// A color bitmap is scaled down by vitrify itself, in premultiplied color,
// with the method this build chose (the operator's Lanczos and sharpen by
// default), from the font's largest strike
#[test]
fn a_color_bitmap_is_scaled_from_its_strike_by_the_chosen_method() {
    let req = OutlineRequest {
        font: ttf::test_emoji().build().into(),
        family: "Test Emoji".to_owned(),
        cells: vec![(0, 10)],
        cover: Vec::new(),
        clusters: Vec::new(),
        baselines: Vec::new(),
    };
    let o = rasterise(&req).expect("rasterised");
    let s = &o.strikes[0];
    // no W, M or 0: the cell is half as wide as it is tall
    assert_eq!((s.width, s.height, s.baseline), (5, 10, 8));
    let fire = glyph(&o, 0, 0x1f525);
    assert!(fire.wide && fire.color);
    // 36 pixels at 40 to the em, drawn at 9.5: 8.55, so 9 by 9, its top
    // 30 * 0.2375 rows above the baseline: row 1
    let native = resample::Image {
        width: 36,
        height: 36,
        pixels: (0..36 * 36)
            .flat_map(|i| {
                if i % 36 < 18 {
                    [255, 0, 0, 255]
                } else {
                    [0; 4]
                }
            })
            .collect(),
    };
    let small = resample::resize(&native, 9, 9, METHOD);
    let mut want = vec![0u8; 10 * 10 * 4];
    for y in 0..9 {
        let row = &small.pixels[y * 9 * 4..(y + 1) * 9 * 4];
        want[((y + 1) * 10) * 4..((y + 1) * 10 + 9) * 4].copy_from_slice(row);
    }
    assert_eq!(fire.pixels, want);
}
