//! Superscript and subscript glyphs from a smaller whole multiple of a
//! bitmap face's own strike (ADR-1202 D3): a face drawn at k times its
//! size takes its script glyph from itself at the largest whole multiple j
//! from one half to three quarters of k, each of the font's pixels a block
//! of j by j screen pixels, placed whole against the text band (D1).

#![cfg(feature = "unifont")]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]

use vtium_font::{Face, Font, Glyph, Psf, Script, Style, Unifont, Variant, Width};

const SUPER: Script = Script::Super;
const SUB: Script = Script::Sub;

const BOLD: Style = Style {
    bold: true,
    italic: false,
};

/// unifont at `k` times its 8x16 size.
fn unifont(k: u32) -> Font<Unifont> {
    Font::new(Unifont::new(16 * k))
}

fn script_glyph<F: Face>(font: &Font<F>, c: char, script: Script, style: Style) -> Glyph {
    font.drawn_variant(&[c], Width::Narrow, Variant { style, script })
        .unwrap()
        .glyph
}

/// Whether `g` holds ink at `x`, `y`, false outside it.
fn ink(g: &Glyph, x: i64, y: i64) -> bool {
    match (u32::try_from(x), u32::try_from(y)) {
        (Ok(x), Ok(y)) => g.pixel(x, y).is_some_and(|p| p[0] >= 128),
        _ => false,
    }
}

/// unifont's own glyph for `c` at 1x in `style`, every pixel on or off.
fn own(c: char, style: Style) -> Glyph {
    Unifont::new(16).render(c, style).unwrap()
}

/// Asserts that `got`, a cell `got.width()` by `got.height()`, holds
/// `small` with each of its pixels a block `j` across, its top left corner
/// at `at`, and nothing else.
fn holds_blocks(got: &Glyph, small: &Glyph, j: u32, at: (u32, u32), what: &str) {
    assert!(
        got.pixels().iter().all(|&p| p == 0 || p == 255),
        "{what}: on or off"
    );
    for y in 0..got.height() {
        for x in 0..got.width() {
            let (sx, sy) = (
                i64::from(x) - i64::from(at.0),
                i64::from(y) - i64::from(at.1),
            );
            let want = sx >= 0
                && sy >= 0
                && ink(small, sx / i64::from(j), sy / i64::from(j))
                && sx / i64::from(j) < i64::from(small.width())
                && sy / i64::from(j) < i64::from(small.height());
            assert_eq!(
                ink(got, i64::from(x), i64::from(y)),
                want,
                "{what} at {x},{y}"
            );
        }
    }
}

// D3: unifont at 2x in a 16x32 cell takes its script glyph from unifont at
// its own 8x16, every pixel of the font one pixel of the screen, against
// the cell's top or bottom and centred across
// covers: sgr.script
#[test]
fn unifont_at_twice_its_size_takes_its_scripts_at_its_own_size() {
    let font = unifont(2);
    for c in ['e', 'a', 'B', '8', '2', 'x', 'g', 'É'] {
        let sup = script_glyph(&font, c, SUPER, Style::default());
        let sub = script_glyph(&font, c, SUB, Style::default());
        assert_eq!((sup.width(), sup.height()), (16, 32));
        holds_blocks(
            &sup,
            &own(c, Style::default()),
            1,
            (4, 0),
            &format!("{c} super"),
        );
        holds_blocks(
            &sub,
            &own(c, Style::default()),
            1,
            (4, 16),
            &format!("{c} sub"),
        );
    }
}

// D3: at 3x, 24x48, the script glyph is unifont at 2x, each pixel a 2x2
// block, never resampled
// covers: sgr.script
#[test]
fn unifont_at_three_times_takes_its_scripts_at_twice_in_whole_blocks() {
    let font = unifont(3);
    for c in ['e', 'a', 'B', '8', '2', 'x', 'g', 'É'] {
        let sup = script_glyph(&font, c, SUPER, Style::default());
        let sub = script_glyph(&font, c, SUB, Style::default());
        assert_eq!((sup.width(), sup.height()), (24, 48));
        holds_blocks(
            &sup,
            &own(c, Style::default()),
            2,
            (4, 0),
            &format!("{c} super"),
        );
        holds_blocks(
            &sub,
            &own(c, Style::default()),
            2,
            (4, 16),
            &format!("{c} sub"),
        );
    }
}

// D3's table through the face: unifont at each scale from 2x to 6x draws
// its superscript H as itself at the table's multiple, centred
// covers: sgr.script
#[test]
fn each_scale_takes_the_tables_multiple() {
    for (k, j) in [(2, 1), (3, 2), (4, 3), (5, 3), (6, 4)] {
        let font = unifont(k);
        let h = script_glyph(&font, 'H', SUPER, Style::default());
        let at = ((8 * k - 8 * j) / 2, 0);
        holds_blocks(&h, &own('H', Style::default()), j, at, &format!("{k}x"));
    }
}

// D7: bold at a smaller multiple widens by one of the font's own pixels,
// j screen pixels, so the glyph stays the font's blocks
// covers: sgr.script
#[test]
fn bold_at_a_smaller_multiple_widens_by_one_of_the_fonts_pixels() {
    let font = unifont(3);
    for c in ['l', 'H', 'm'] {
        let bold = script_glyph(&font, c, SUPER, BOLD);
        holds_blocks(&bold, &own(c, BOLD), 2, (4, 0), &format!("{c} bold"));
    }
}

// D3 at 1x: no smaller multiple lies in the range, so the glyph is scaled
// to two thirds by D4, as script-unifont shows
#[test]
fn unifont_at_its_own_size_scales_its_scripts() {
    let font = unifont(1);
    let g = script_glyph(&font, 'H', SUPER, Style::default());
    // unifont's H fills rows 2 to 13 of its 16; two thirds of the cell is
    // the top 11
    let rows: Vec<i64> = (0..16).filter(|&y| (0..8).any(|x| ink(&g, x, y))).collect();
    assert!(!rows.is_empty() && rows.iter().all(|&y| y < 11), "{rows:?}");
}

// ADR-1300 D2: at 3x the script glyph is stored at unifont's own size with
// scale 2, and drawn with each pixel a 2x2 square it is the glyph drawn at
// its full size; at 4x, 3 does not divide the 32-pixel cell, so the glyph
// comes at its drawn size
// covers: sgr.script
#[test]
fn a_smaller_multiples_glyph_is_stored_at_its_own_size() {
    let font = unifont(3);
    for script in [SUPER, SUB] {
        let v = Variant {
            style: Style::default(),
            script,
        };
        let stored = font
            .glyph_unscaled_variant(&['g'], Width::Narrow, v)
            .unwrap();
        let drawn = font.drawn_variant(&['g'], Width::Narrow, v).unwrap();
        assert_eq!(stored.scale, 2, "{script:?}");
        assert_eq!((stored.glyph.width(), stored.glyph.height()), (12, 24));
        holds_blocks(
            &drawn.glyph,
            &stored.glyph,
            2,
            (0, 0),
            &format!("{script:?}"),
        );
    }
    let four = unifont(4);
    let v = Variant {
        style: Style::default(),
        script: SUPER,
    };
    let stored = four
        .glyph_unscaled_variant(&['g'], Width::Narrow, v)
        .unwrap();
    assert_eq!(stored.scale, 1);
    assert_eq!(
        stored,
        four.drawn_variant(&['g'], Width::Narrow, v).unwrap()
    );
}

// D3 for a PSF face: an 8x8 font drawn at 3x takes its scripts at 2x
#[test]
fn a_psf_font_at_a_multiple_takes_a_smaller_one() {
    // an 8x8 raw font of 256 glyphs, each glyph's rows its index
    let bytes: Vec<u8> = (0..=255u8).flat_map(|g| [g; 8]).collect();
    let mut psf = Psf::parse(&bytes).unwrap();
    psf.set_height(24);
    let font = Font::new(psf.clone());
    let c = char::from(0x5a_u8);
    let sup = script_glyph(&font, c, SUPER, Style::default());
    let small = {
        let mut one = psf.clone();
        one.set_height(8);
        one.draw(&[c], Style::default()).unwrap()
    };
    holds_blocks(&sup, &small, 2, (4, 0), "psf");
}
