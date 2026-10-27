//! The per-font choice between a face's glyphs and the sprites for each
//! range vtium draws as sprites (vtium/vtium#85), with stand-in faces that
//! cover a range fully, partly and not at all. The C reference always drew
//! the sprites (font.c:345-357); `Source::Sprites` is that behaviour.

use vtium_font::sprite::{self, Range};
use vtium_font::{Cell, Coverage, Face, Font, Glyph, Missing, RangeCoverage, Source, SpritePolicy};
use vtium_font::{Style, Width};

const CELL: Cell = Cell {
    width: 8,
    height: 16,
};

/// The face's own glyphs are solid at this coverage, which no sprite
/// draws, so a test can tell the face's glyph from a sprite.
const FACE_INK: u8 = 0x11;
/// The face's U+25A1, drawn for a character it lacks.
const BOX_INK: u8 = 0x22;

/// A stand-in face holding the characters `holds` accepts and U+25A1,
/// each a solid glyph the size of the cell, drawn at its `scale`.
struct Stub {
    holds: fn(char) -> bool,
    scale: u32,
}

impl Stub {
    fn new(holds: fn(char) -> bool) -> Stub {
        Stub { holds, scale: 1 }
    }

    fn solid(&self, c: char, scale: u32) -> Option<Glyph> {
        let ink = match c {
            vtium_font::REPLACEMENT => BOX_INK,
            c if (self.holds)(c) => FACE_INK,
            _ => return None,
        };
        let (w, h) = (
            CELL.width / self.scale * scale,
            CELL.height / self.scale * scale,
        );
        let n = usize::try_from(w * h).expect("small");
        Glyph::from_pixels(w, h, vtium_font::Format::Mask, vec![ink; n])
    }
}

impl Face for Stub {
    fn cell(&self) -> Cell {
        CELL
    }
    fn size_step(&self) -> u32 {
        CELL.height
    }
    fn has_glyph(&self, c: char) -> bool {
        c == vtium_font::REPLACEMENT || (self.holds)(c)
    }
    fn draw(&self, cluster: &[char], _style: Style) -> Option<Glyph> {
        self.solid(*cluster.first()?, self.scale)
    }
    fn scale(&self) -> u32 {
        self.scale
    }
    fn draw_unscaled(&self, cluster: &[char], _style: Style) -> Option<Glyph> {
        self.solid(*cluster.first()?, 1)
    }
}

/// Every character of every range.
fn full(c: char) -> bool {
    Range::of(c).is_some()
}

/// Every other character of each range, and none of Braille.
fn partial(c: char) -> bool {
    Range::of(c).is_some_and(|r| r != Range::Braille) && u32::from(c) % 2 == 0
}

fn none(_: char) -> bool {
    false
}

fn plain() -> Style {
    Style::default()
}

fn draw<F: Face>(font: &Font<F>, c: char) -> Glyph {
    font.glyph(&[c], Width::Narrow, plain()).expect("drawn")
}

fn sprite(c: char) -> Glyph {
    let thick = vtium_font::Decorations::for_height(CELL.height).underline_thickness;
    sprite::draw(c, CELL.width, CELL.height, thick).expect("a sprite")
}

fn ink(g: &Glyph) -> Option<u8> {
    let first = *g.pixels().first()?;
    g.pixels().iter().all(|&p| p == first).then_some(first)
}

const SIZES: [(Range, u32); 6] = [
    (Range::BoxDrawing, 128),
    (Range::BlockElements, 32),
    (Range::Braille, 256),
    (Range::Sextants, 60),
    (Range::Octants, 230),
    (Range::LegacyComputing, 168),
];

#[test]
fn the_ranges_are_exactly_the_sprites() {
    let mut counted = 0u32;
    for (range, size) in SIZES {
        let chars: Vec<char> = range.chars().collect();
        assert_eq!(
            chars.len(),
            usize::try_from(size).expect("small"),
            "{range}"
        );
        assert!(
            chars.iter().all(|&c| Range::of(c) == Some(range)),
            "{range}"
        );
        counted += size;
    }
    let sprites = (0..=0x10ffff)
        .filter_map(char::from_u32)
        .filter(|&c| {
            assert_eq!(
                sprite::has(c),
                Range::of(c).is_some(),
                "U+{:04X}",
                u32::from(c)
            );
            sprite::has(c)
        })
        .count();
    assert_eq!(u32::try_from(sprites).expect("small"), counted);
    assert_eq!(
        Range::ALL.map(|r| r.to_string()),
        [
            "box drawing",
            "block elements",
            "Braille",
            "sextants",
            "octants",
            "Legacy Computing",
        ]
    );
}

#[test]
fn coverage_counts_each_range() {
    for (holds, name) in [
        (full as fn(char) -> bool, "full"),
        (partial, "partial"),
        (none, "none"),
    ] {
        let coverage = Font::new(Stub::new(holds)).coverage();
        for (range, size) in SIZES {
            let want = range.chars().filter(|&c| holds(c)).count();
            let got = coverage.get(range);
            assert_eq!(got.total, size, "{name} {range}");
            assert_eq!(
                got.covered,
                u32::try_from(want).expect("small"),
                "{name} {range}"
            );
            assert_eq!(
                got.is_complete(),
                want == range.chars().count(),
                "{name} {range}"
            );
            assert_eq!(got.is_empty(), want == 0, "{name} {range}");
        }
    }
    // the stand-ins cover what their names say
    let half = Font::new(Stub::new(partial))
        .coverage()
        .get(Range::BoxDrawing);
    assert_eq!(
        half,
        RangeCoverage {
            covered: 64,
            total: 128
        }
    );
}

// the warning on a partly covered range (vtium/vtium#85)
#[test]
fn a_partly_covered_range_is_reported() {
    let partial_ranges = |c: Coverage| c.partial().map(|(r, _)| r).collect::<Vec<_>>();
    assert!(partial_ranges(Font::new(Stub::new(full)).coverage()).is_empty());
    assert!(partial_ranges(Font::new(Stub::new(none)).coverage()).is_empty());
    let all_but_braille: Vec<Range> = Range::ALL
        .into_iter()
        .filter(|&r| r != Range::Braille)
        .collect();
    assert_eq!(
        partial_ranges(Font::new(Stub::new(partial)).coverage()),
        all_but_braille
    );
    let (range, n) = Font::new(Stub::new(partial))
        .coverage()
        .partial()
        .next()
        .expect("box drawing");
    assert_eq!(
        format!("{range}: the font covers {} of {}", n.covered, n.total),
        "box drawing: the font covers 64 of 128"
    );
}

#[cfg(feature = "unifont")]
#[test]
fn unifont_covers_every_range() {
    let coverage = Font::new(vtium_font::Unifont::new(16)).coverage();
    for (range, size) in SIZES {
        assert_eq!(
            coverage.get(range),
            RangeCoverage {
                covered: size,
                total: size
            },
            "{range}"
        );
    }
}

// font.c:345-357: the C reference drew every one of these as a sprite,
// whatever the font held
#[test]
fn sprites_draw_the_sprite_whatever_the_face_holds() {
    for holds in [full as fn(char) -> bool, partial, none] {
        let font = Font::new(Stub::new(holds)).with_policy(SpritePolicy::SPRITES);
        for range in Range::ALL {
            for c in range.chars() {
                assert_eq!(draw(&font, c), sprite(c), "U+{:04X}", u32::from(c));
                assert!(font.has_glyph(c));
            }
        }
    }
}

#[test]
fn font_draws_the_face_and_its_replacement() {
    for holds in [full as fn(char) -> bool, partial, none] {
        let font = Font::new(Stub::new(holds)).with_policy(SpritePolicy::all(Source::Font));
        for range in Range::ALL {
            for c in range.chars() {
                let want = if holds(c) { FACE_INK } else { BOX_INK };
                assert_eq!(ink(&draw(&font, c)), Some(want), "U+{:04X}", u32::from(c));
                assert_eq!(font.has_glyph(c), holds(c));
            }
        }
    }
}

#[test]
fn auto_draws_the_face_and_a_sprite_where_it_has_none() {
    for holds in [full as fn(char) -> bool, partial, none] {
        let font = Font::new(Stub::new(holds)).with_policy(SpritePolicy::all(Source::Auto));
        for range in Range::ALL {
            for c in range.chars() {
                let g = draw(&font, c);
                if holds(c) {
                    assert_eq!(ink(&g), Some(FACE_INK), "U+{:04X}", u32::from(c));
                } else {
                    assert_eq!(g, sprite(c), "U+{:04X}", u32::from(c));
                }
                assert!(font.has_glyph(c));
            }
        }
    }
}

#[test]
fn each_range_takes_its_own_source() {
    let policy = SpritePolicy::SPRITES
        .with(Range::BoxDrawing, Source::Font)
        .with(Range::LegacyComputing, Source::Auto);
    assert_eq!(policy.source(Range::BoxDrawing), Source::Font);
    assert_eq!(policy.source(Range::Braille), Source::Sprites);
    let font = Font::new(Stub::new(partial)).with_policy(policy);
    // box drawing: the face's, else its U+25A1
    assert_eq!(ink(&draw(&font, '─')), Some(FACE_INK));
    assert_eq!(ink(&draw(&font, '━')), Some(BOX_INK));
    // block elements: the sprite though the face has one
    assert_eq!(draw(&font, '█'), sprite('█'));
    // Legacy Computing: the face's, else the sprite
    assert_eq!(ink(&draw(&font, '\u{1fb3c}')), Some(FACE_INK));
    assert_eq!(draw(&font, '\u{1fb3d}'), sprite('\u{1fb3d}'));
}

#[test]
fn a_wide_cell_fits_the_face_glyph_or_the_wide_sprite() {
    let font = Font::new(Stub::new(partial)).with_policy(SpritePolicy::all(Source::Auto));
    let face = font.glyph(&['─'], Width::Wide, plain()).expect("drawn");
    assert_eq!((face.width(), face.cells()), (16, Width::Wide));
    let thick = font.decorations().underline_thickness;
    let wide = font.glyph(&['━'], Width::Wide, plain());
    assert_eq!(
        wide,
        sprite::draw('━', 16, 16, thick).map(|s| s.with_cells(Width::Wide))
    );
}

#[test]
fn a_cluster_is_the_faces_under_every_policy() {
    for source in [Source::Font, Source::Sprites, Source::Auto] {
        let font = Font::new(Stub::new(none)).with_policy(SpritePolicy::all(source));
        assert_eq!(ink(&draw_cluster(&font)), Some(BOX_INK), "{source:?}");
    }
}

fn draw_cluster<F: Face>(font: &Font<F>) -> Glyph {
    font.glyph(&['─', '\u{301}'], Width::Narrow, plain())
        .expect("drawn")
}

// ADR-1300 D2: a face's glyph is handed over small; a sprite at its size
#[test]
fn the_unscaled_glyph_follows_the_policy() {
    let stub = || Stub {
        holds: partial,
        scale: 2,
    };
    let font = Font::new(stub()).with_policy(SpritePolicy::all(Source::Auto));
    let (g, scale) = font
        .glyph_unscaled(&['─'], Width::Narrow, plain())
        .expect("drawn");
    assert_eq!(
        (g.width(), g.height(), scale, ink(&g)),
        (4, 8, 2, Some(FACE_INK))
    );
    let (g, scale) = font
        .glyph_unscaled(&['━'], Width::Narrow, plain())
        .expect("drawn");
    assert_eq!((g, scale), (sprite('━'), 1));
    let font = Font::new(stub()).with_policy(SpritePolicy::SPRITES);
    let (g, scale) = font
        .glyph_unscaled(&['─'], Width::Narrow, plain())
        .expect("drawn");
    assert_eq!((g, scale), (sprite('─'), 1));
    let font = Font::new(stub()).with_policy(SpritePolicy::all(Source::Font));
    let (g, scale) = font
        .glyph_unscaled(&['━'], Width::Narrow, plain())
        .expect("drawn");
    assert_eq!((g.width(), scale, ink(&g)), (4, 2, Some(BOX_INK)));
}

#[test]
fn a_box_of_faces_forwards_its_coverage() {
    struct Counted;
    impl Face for Counted {
        fn cell(&self) -> Cell {
            CELL
        }
        fn size_step(&self) -> u32 {
            CELL.height
        }
        fn has_glyph(&self, _: char) -> bool {
            false
        }
        fn draw(&self, _: &[char], _: Style) -> Option<Glyph> {
            None
        }
        // a face that reads its own character map may count it faster
        fn coverage(&self, range: Range) -> RangeCoverage {
            RangeCoverage {
                covered: 7,
                total: u32::try_from(range.chars().count()).expect("small"),
            }
        }
    }
    let font: Font<Box<dyn Face>> = Font::new(Box::new(Counted));
    assert_eq!(font.coverage().get(Range::Braille).covered, 7);
}

/// The whole of box drawing, every other block element, and nothing else.
fn whole_boxes_half_blocks(c: char) -> bool {
    match Range::of(c) {
        Some(Range::BoxDrawing) => true,
        Some(Range::BlockElements) => u32::from(c) % 2 == 0,
        _ => false,
    }
}

// the operator's default (vtium/vtium#85): a range the font covers whole
// is the font's, any other the sprites', and no range mixes the two
#[test]
fn a_named_font_draws_only_the_ranges_it_covers_whole() {
    let font = Font::new(Stub::new(whole_boxes_half_blocks));
    assert_eq!(
        font.policy(),
        SpritePolicy::SPRITES.with(Range::BoxDrawing, Source::Font)
    );
    assert_eq!(ink(&draw(&font, '─')), Some(FACE_INK));
    // the face holds U+2588, but not the whole of its range
    assert!(whole_boxes_half_blocks('█'));
    assert_eq!(draw(&font, '█'), sprite('█'));
    assert_eq!(draw(&font, '▉'), sprite('▉'));
    let blocks: Vec<Range> = font.coverage().partial().map(|(r, _)| r).collect();
    assert_eq!(blocks, [Range::BlockElements]);

    assert_eq!(
        Font::new(Stub::new(full)).policy(),
        SpritePolicy::all(Source::Font)
    );
    assert_eq!(
        Font::new(Stub::new(partial)).policy(),
        SpritePolicy::SPRITES
    );
    assert_eq!(Font::new(Stub::new(none)).policy(), SpritePolicy::SPRITES);
    for holds in [
        full as fn(char) -> bool,
        partial,
        none,
        whole_boxes_half_blocks,
    ] {
        let policy = Font::new(Stub::new(holds)).policy();
        assert!(Range::ALL.iter().all(|&r| policy.source(r) != Source::Auto));
    }
}

// the built-in unifont keeps the C reference's sprites, though it covers
// every range
#[cfg(feature = "unifont")]
#[test]
fn unifont_draws_the_sprites_by_default() {
    let unifont = || vtium_font::Unifont::new(16);
    assert_eq!(Font::new(unifont()).policy(), SpritePolicy::SPRITES);
    let boxed: Font<Box<dyn Face>> = Font::new(Box::new(unifont()));
    assert_eq!(boxed.policy(), SpritePolicy::SPRITES);
    assert_eq!(draw(&Font::new(unifont()), '─'), sprite('─'));
}

#[test]
fn a_missing_character_draws_u_25a1_by_default() {
    assert_eq!(Missing::default(), Missing::Replacement);
    let font = Font::new(Stub::new(none)).with_policy(SpritePolicy::all(Source::Font));
    assert_eq!(font.missing(), Missing::Replacement);
    assert_eq!(ink(&draw(&font, '─')), Some(BOX_INK));
    assert_eq!(ink(&draw(&font, 'A')), Some(BOX_INK));
    let font = font.with_missing(Missing::Replacement);
    assert_eq!(ink(&draw(&font, 'A')), Some(BOX_INK));
}
