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
use vtium_vitrify::outline::{Outline, OutlineRequest, cell_width, rasterise};
use vtium_vitrify::resample::{self, METHOD};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-vitrify");

fn request(cells: &[(u32, u32)], cover: &[(u32, u32)]) -> OutlineRequest {
    OutlineRequest {
        font: ttf::test_mono().build().into(),
        family: "Test Mono".to_owned(),
        cells: cells.to_vec(),
        cover: cover.to_vec(),
        clusters: Vec::new(),
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
