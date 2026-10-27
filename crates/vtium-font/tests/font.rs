//! [`Font`]'s drawing of a cluster into its cells, the decoration lines and
//! the built-in unifont, from the C reference's font.c
//! (`kmscon_font_render`, `fit_glyph`, `kmscon_font_get_deco`) and
//! font_unifont.c.

use vtium_font::{
    CELL_SIDE_MAX, Cell, Decorations, Face, Font, Format, Glyph, Psf, Style, Width, sprite,
};

fn plain() -> Style {
    Style::default()
}

/// A 4-glyph 8x16 PSF2 font without a table, glyph i's rows all i + 1.
fn small_psf() -> Psf {
    let mut b = vec![0x72, 0xb5, 0x4a, 0x86];
    for v in [0u32, 32, 0, 4, 16, 16, 8] {
        b.extend(v.to_le_bytes());
    }
    for i in 1..=4u8 {
        b.extend([i; 16]);
    }
    Psf::parse(&b).expect("loads")
}

#[test]
fn an_empty_cluster_draws_nothing() {
    assert!(
        Font::new(small_psf())
            .glyph(&[], Width::Narrow, plain())
            .is_none()
    );
}

// font.c:345-357: a sprite fills the cells, its lines the underline's
// thickness
#[test]
fn a_sprite_character_fills_its_cells() {
    let mut psf = small_psf();
    psf.set_height(32);
    let font = Font::new(psf);
    let deco = font.decorations();
    assert_eq!(deco.underline_thickness, 2);
    for c in ['│', '█', '⣿', '\u{1fb00}', '\u{1cd00}'] {
        assert!(font.has_glyph(c), "{c}");
        for (width, w) in [(Width::Narrow, 16), (Width::Wide, 32)] {
            let g = font.glyph(&[c], width, plain()).expect("a sprite");
            let want = sprite::draw(c, w, 32, 2).map(|s| s.with_cells(width));
            assert_eq!(Some(g), want, "{c}");
        }
    }
    // a cluster of more than one character is the face's
    let g = font.glyph(&['│', '\u{301}'], Width::Narrow, plain());
    assert_ne!(g, sprite::draw('│', 16, 32, 2));
}

// font.c:267-308: a glyph narrower than its cells is centred, a wider one
// cut evenly from both sides
#[test]
fn a_glyph_is_boxed_to_its_cells() {
    let font = Font::new(small_psf());
    let narrow = font.face().render(1, plain()).expect("glyph 1");
    let wide = font.glyph(&['\u{1}'], Width::Wide, plain()).expect("drawn");
    assert_eq!((wide.width(), wide.height()), (16, 16));
    for (y, row) in wide.rows().enumerate() {
        assert_eq!(&row[4..12], narrow.rows().nth(y).expect("a row"));
        assert!(row[..4].iter().chain(&row[12..]).all(|&p| p == 0));
    }
}

#[test]
fn a_box_of_faces_is_a_face() {
    let font: Font<Box<dyn Face>> = Font::new(Box::new(small_psf()));
    assert_eq!(font.cell().width, 8);
    assert!(font.glyph(&['\u{2}'], Width::Narrow, plain()).is_some());
}

// font.c:370-408
#[test]
fn decorations_for_a_face_without_metrics() {
    for (h, ul, thick, strike) in [
        (0, 0, 1, 0),
        (1, 0, 1, 0),
        (3, 2, 1, 1),
        (16, 15, 1, 8),
        (19, 18, 1, 9),
        (32, 30, 2, 16),
        (64, 60, 4, 32),
    ] {
        assert_eq!(
            Decorations::for_height(h),
            Decorations {
                underline: ul,
                underline_thickness: thick,
                strike,
                strike_thickness: thick,
            },
            "height {h}"
        );
    }
}

// Both formats are bounded alike: a side of at least 1, a height of at
// most CELL_SIDE_MAX and a width of at most two cells of it.
#[test]
fn glyph_bounds() {
    for new in [Glyph::mask, Glyph::rgba] {
        assert!(new(0, 1).is_none());
        assert!(new(1, 0).is_none());
        assert!(new(1, CELL_SIDE_MAX + 1).is_none());
        assert!(new(2 * CELL_SIDE_MAX + 1, 1).is_none());
        let g = new(2 * CELL_SIDE_MAX, CELL_SIDE_MAX).expect("at the bound");
        let bpp = g.format().bytes_per_pixel();
        assert_eq!(
            g.pixels().len(),
            (2 * CELL_SIDE_MAX * CELL_SIDE_MAX * bpp) as usize
        );
        assert_eq!(g.pixel(2 * CELL_SIDE_MAX, 0), None);
        assert_eq!(g.pixel(0, CELL_SIDE_MAX), None);
    }
    assert_eq!(Glyph::mask(1, 1).map(|g| g.pixels().len()), Some(1));
    assert_eq!(Glyph::rgba(1, 1).map(|g| g.pixels().len()), Some(4));
}

#[test]
fn rgba_must_be_premultiplied_and_the_right_length() {
    let px = vec![0x10, 0x20, 0x30, 0x80];
    assert!(Glyph::from_pixels(1, 1, Format::Rgba, px).is_some());
    // a colour byte greater than its alpha is not premultiplied
    assert!(Glyph::from_pixels(1, 1, Format::Rgba, vec![0x90, 0, 0, 0x80]).is_none());
    assert!(Glyph::from_pixels(1, 1, Format::Rgba, vec![0; 3]).is_none());
    assert!(Glyph::from_pixels(1, 1, Format::Mask, vec![0; 4]).is_none());
    assert!(Glyph::from_pixels(1, 1, Format::Mask, vec![0x7f]).is_some());
}

/// A colour face: every character a 4x2 RGBA glyph whose pixel x, y is
/// (x, y, 0, 0x80) premultiplied.
struct Colour;

impl Colour {
    fn glyph() -> Glyph {
        let px = (0..2u8)
            .flat_map(|y| (0..4u8).flat_map(move |x| [x, y, 0, 0x80]))
            .collect();
        Glyph::from_pixels(4, 2, Format::Rgba, px).expect("premultiplied")
    }
}

impl Face for Colour {
    fn cell(&self) -> Cell {
        Cell {
            width: 4,
            height: 2,
        }
    }
    fn size_step(&self) -> u32 {
        2
    }
    fn has_glyph(&self, _: char) -> bool {
        true
    }
    fn draw(&self, _: &[char], _: Style) -> Option<Glyph> {
        Some(Colour::glyph())
    }
}

// A colour glyph keeps its pixels and alpha through Font, boxed into its
// cells as a mask would be, with the margins transparent.
#[test]
fn a_colour_glyph_keeps_its_pixels_and_alpha() {
    let font = Font::new(Colour);
    let narrow = font.glyph(&['x'], Width::Narrow, plain()).expect("drawn");
    assert_eq!(narrow, Colour::glyph());
    assert_eq!(
        (narrow.format(), narrow.cells()),
        (Format::Rgba, Width::Narrow)
    );
    assert_eq!(narrow.pixel(3, 1), Some(&[3, 1, 0, 0x80][..]));

    let wide = font.glyph(&['x'], Width::Wide, plain()).expect("drawn");
    assert_eq!((wide.width(), wide.height()), (8, 2));
    assert_eq!((wide.format(), wide.cells()), (Format::Rgba, Width::Wide));
    for y in 0..2u8 {
        for x in 0..8u8 {
            let want: &[u8] = if (2..6).contains(&x) {
                &[x - 2, y, 0, 0x80]
            } else {
                &[0, 0, 0, 0]
            };
            assert_eq!(
                wide.pixel(u32::from(x), u32::from(y)),
                Some(want),
                "{x},{y}"
            );
        }
    }
    // a sprite character from a colour face is still a mask sprite
    let block = font
        .glyph(&['█'], Width::Narrow, plain())
        .expect("a sprite");
    assert_eq!(block.format(), Format::Mask);
}

// A bitmap face's glyph and a sprite stay masks, one byte a pixel, at
// either width.
#[test]
fn a_mask_stays_a_mask() {
    let font = Font::new(small_psf());
    for c in ['\u{1}', '█'] {
        for width in [Width::Narrow, Width::Wide] {
            let g = font.glyph(&[c], width, plain()).expect("drawn");
            assert_eq!((g.format(), g.cells()), (Format::Mask, width));
            assert_eq!(g.pixels().len(), (g.width() * g.height()) as usize);
        }
    }
}

#[cfg(feature = "unifont")]
mod unifont {
    use super::*;
    use vtium_font::{REPLACEMENT, Unifont};

    // font_unifont.c:229-235
    #[test]
    fn scales_by_whole_multiples_of_16() {
        for (height, cell) in [(0, (8, 16)), (16, (8, 16)), (31, (8, 16)), (32, (16, 32))] {
            let f = Unifont::new(height);
            assert_eq!(
                (f.cell().width, f.cell().height),
                cell,
                "asked for {height}"
            );
        }
        let huge = Unifont::new(u32::MAX);
        assert_eq!(huge.cell().height, CELL_SIDE_MAX);
        let g = huge.render('中', plain()).expect("a wide glyph");
        assert_eq!((g.width(), g.height()), (CELL_SIDE_MAX, CELL_SIDE_MAX));
        assert_eq!(huge.size_step(), 16);
    }

    #[test]
    fn glyphs_carry_their_own_width() {
        let f = Unifont::new(16);
        let a = f.render('A', plain()).expect("A");
        assert_eq!(
            (a.width(), a.cells(), a.format()),
            (8, Width::Narrow, Format::Mask)
        );
        let zhong = f.render('中', plain()).expect("中");
        assert_eq!((zhong.width(), zhong.cells()), (16, Width::Wide));
        // across every plane unifont-all covers
        for c in [
            '\u{1f600}',
            '\u{20000}',
            '\u{30000}',
            '\u{e0041}',
            '\u{f0000}',
        ] {
            assert!(f.has_glyph(c), "U+{:04X}", u32::from(c));
            assert!(f.render(c, plain()).is_some(), "U+{:04X}", u32::from(c));
        }
    }

    // font.c:358-364: a character unifont lacks draws U+25A1
    #[test]
    fn a_missing_character_draws_the_replacement() {
        let font = Font::new(Unifont::new(16));
        let missing = '\u{40000}';
        assert!(!font.has_glyph(missing));
        assert!(font.face().render(missing, plain()).is_none());
        assert_eq!(
            font.glyph(&[missing], Width::Narrow, plain()),
            font.glyph(&[REPLACEMENT], Width::Narrow, plain())
        );
    }

    // font.c:267-308: a wide glyph in one cell keeps its middle
    #[test]
    fn a_wide_glyph_in_one_cell_keeps_its_middle() {
        let font = Font::new(Unifont::new(16));
        let wide = font.face().render('中', plain()).expect("drawn");
        let cut = font.glyph(&['中'], Width::Narrow, plain()).expect("drawn");
        assert_eq!(cut.width(), 8);
        for (a, b) in cut.rows().zip(wide.rows()) {
            assert_eq!(a, &b[4..12]);
        }
    }

    #[test]
    fn a_cluster_draws_its_first_character() {
        let font = Font::new(Unifont::new(16));
        assert_eq!(
            font.glyph(&['e', '\u{301}'], Width::Narrow, plain()),
            font.glyph(&['e'], Width::Narrow, plain())
        );
    }
}
