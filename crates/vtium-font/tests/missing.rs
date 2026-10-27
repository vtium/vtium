//! `Missing::Hex`: a character the face lacks drawn as its codepoint in hex
//! (vtium/vtium#85), as the operator approved it. The C reference drew
//! U+25A1, `?` or glyph 0 (font.c:327-339), which `Missing::Replacement`
//! keeps.

use vtium_font::sprite::Range;
use vtium_font::{Cell, Face, Font, Glyph, Missing, Source, SpritePolicy, Style, Width};

/// A face holding only `A`, solid, and U+25A1, half covered, in a `cell`.
struct Stub {
    cell: Cell,
    scale: u32,
}

impl Stub {
    fn new(width: u32, height: u32) -> Stub {
        Stub {
            cell: Cell { width, height },
            scale: 1,
        }
    }
}

impl Face for Stub {
    fn cell(&self) -> Cell {
        self.cell
    }
    fn size_step(&self) -> u32 {
        self.cell.height
    }
    fn has_glyph(&self, c: char) -> bool {
        matches!(c, 'A' | vtium_font::REPLACEMENT)
    }
    fn draw(&self, cluster: &[char], _style: Style) -> Option<Glyph> {
        let ink = match cluster.first()? {
            'A' => 0xff,
            &vtium_font::REPLACEMENT => 0x80,
            _ => return None,
        };
        let (w, h) = (self.cell.width, self.cell.height);
        let n = usize::try_from(w * h).expect("small");
        Glyph::from_pixels(w, h, vtium_font::Format::Mask, vec![ink; n])
    }
    fn scale(&self) -> u32 {
        self.scale
    }
}

fn hex(font: &Font<Stub>, c: char, width: Width) -> Glyph {
    font.glyph(&[c], width, Style::default()).expect("drawn")
}

/// A glyph as rows of `#` for full coverage and `.` for none.
fn picture(g: &Glyph) -> Vec<String> {
    g.rows()
        .map(|row| {
            row.iter()
                .map(|&p| match p {
                    0xff => '#',
                    0 => '.',
                    _ => '?',
                })
                .collect()
        })
        .collect()
}

// the mock-up the operator approved: the cell in the text colour, the
// digits cut out in a 3x5 font, two rows of two
#[test]
fn u_2603_in_a_narrow_cell_is_the_approved_mock_up() {
    let font = Font::new(Stub::new(8, 16)).with_missing(Missing::Hex);
    let g = hex(&font, '\u{2603}', Width::Narrow);
    assert_eq!((g.width(), g.height(), g.cells()), (8, 16, Width::Narrow));
    assert_eq!(
        picture(&g),
        [
            "########", "########", "#...#...", "###.#.##", "#...#...", "#.###.#.", "#...#...",
            "########", "########", "#...#...", "#.#.###.", "#.#.#...", "#.#.###.", "#...#...",
            "########", "########",
        ]
    );
}

// above U+FFFF: six digits, three rows of two in a 3x4 font, lower-case b
// and d apart from 8 and 0
#[test]
fn a_codepoint_past_the_bmp_takes_six_smaller_digits() {
    let font = Font::new(Stub::new(8, 16)).with_missing(Missing::Hex);
    assert_eq!(
        picture(&hex(&font, '\u{1f600}', Width::Narrow)),
        [
            "########", "#...##.#", "#.#.#..#", "#.#.##.#", "#...#...", "########", "#...#...",
            "#.###.##", "#..##...", "#.###...", "########", "#...#...", "#.#.#.#.", "#.#.#.#.",
            "#...#...", "########",
        ]
    );
    // U+BD0B0: b, d, 0 and B differ
    let g = picture(&hex(&font, '\u{bd0b0}', Width::Narrow));
    let digit = |i: usize| -> Vec<String> {
        let (x, y) = (1 + 4 * (i % 2), 1 + 5 * (i / 2));
        g[y..y + 4]
            .iter()
            .map(|r| r[x..x + 3].to_string())
            .collect()
    };
    let shapes = [digit(1), digit(2), digit(3), digit(4)];
    assert_eq!(shapes[0], [".##", "...", ".#.", "..."], "b");
    assert_eq!(shapes[1], ["##.", "...", ".#.", "..."], "d");
    assert_eq!(shapes[2], ["...", ".#.", ".#.", "..."], "0");
    assert_eq!(shapes[3], shapes[0], "b again");
}

// a wide cell: a box outline with 3x5 digits inside, as unifont draws its
// own missing glyphs
#[test]
fn a_wide_cell_draws_the_digits_in_a_box() {
    let font = Font::new(Stub::new(8, 16)).with_missing(Missing::Hex);
    let g = hex(&font, '\u{2603}', Width::Wide);
    assert_eq!((g.width(), g.height(), g.cells()), (16, 16, Width::Wide));
    assert_eq!(
        picture(&g),
        [
            "################",
            "#..............#",
            "#...###..###...#",
            "#.....#..#.....#",
            "#...###..###...#",
            "#...#....#.#...#",
            "#...###..###...#",
            "#..............#",
            "#..............#",
            "#...###..###...#",
            "#...#.#....#...#",
            "#...#.#..###...#",
            "#...#.#....#...#",
            "#...###..###...#",
            "#..............#",
            "################",
        ]
    );
    assert_eq!(
        picture(&hex(&font, '\u{1f600}', Width::Wide)),
        [
            "################",
            "#..............#",
            "#..###..#..###.#",
            "#..#.#.##..#...#",
            "#..#.#..#..###.#",
            "#..#.#..#..#...#",
            "#..###.###.#...#",
            "#..............#",
            "#..............#",
            "#..###.###.###.#",
            "#..#...#.#.#.#.#",
            "#..###.#.#.#.#.#",
            "#..#.#.#.#.#.#.#",
            "#..###.###.###.#",
            "#..............#",
            "################",
        ]
    );
}

/// `g` with each pixel a square `n` across.
fn scaled(g: &Glyph, n: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for row in g.rows() {
        let wide: Vec<u8> = row
            .iter()
            .flat_map(|&p| std::iter::repeat_n(p, n as usize))
            .collect();
        for _ in 0..n {
            out.extend(&wide);
        }
    }
    out
}

// a bitmap font scaled by a whole factor scales the glyph by it
#[test]
fn the_glyph_scales_by_whole_factors() {
    let small = Font::new(Stub::new(8, 16)).with_missing(Missing::Hex);
    for n in [2, 3] {
        let big = Font::new(Stub {
            cell: Cell {
                width: 8 * n,
                height: 16 * n,
            },
            scale: n,
        })
        .with_missing(Missing::Hex);
        for (c, width) in [('\u{2603}', Width::Narrow), ('\u{10fffd}', Width::Wide)] {
            let want = scaled(&hex(&small, c, width), n);
            assert_eq!(
                hex(&big, c, width).pixels(),
                &want[..],
                "{n} U+{:X}",
                u32::from(c)
            );
            // handed over at its drawn size, as a sprite is
            let (g, scale) = big
                .glyph_unscaled(&[c], width, Style::default())
                .expect("drawn");
            assert_eq!((g.pixels(), scale), (&want[..], 1));
        }
    }
}

// a cell between whole factors: the largest that fits, centred; a narrow
// cell's margin stays in the text colour, a wide cell's stays clear
#[test]
fn a_cell_between_factors_centres_the_largest_that_fits() {
    let exact = Font::new(Stub::new(8, 16)).with_missing(Missing::Hex);
    let odd = Font::new(Stub::new(11, 22)).with_missing(Missing::Hex);
    let (want, got) = (
        hex(&exact, '\u{2603}', Width::Narrow),
        hex(&odd, '\u{2603}', Width::Narrow),
    );
    assert_eq!((got.width(), got.height()), (11, 22));
    for y in 0..22 {
        for x in 0..11 {
            let inside = (1..9).contains(&x) && (3..19).contains(&y);
            let px = if inside {
                want.pixel(x - 1, y - 3).expect("inside")[0]
            } else {
                0xff
            };
            assert_eq!(got.pixel(x, y), Some(&[px][..]), "{x},{y}");
        }
    }
    let wide = hex(&odd, '\u{2603}', Width::Wide);
    let want = hex(&exact, '\u{2603}', Width::Wide);
    assert_eq!((wide.width(), wide.height()), (22, 22));
    for y in 0..22 {
        for x in 0..22 {
            let inside = (3..19).contains(&x) && (3..19).contains(&y);
            let px = if inside {
                want.pixel(x - 3, y - 3).expect("inside")[0]
            } else {
                0
            };
            assert_eq!(wide.pixel(x, y), Some(&[px][..]), "{x},{y}");
        }
    }
}

// a cell too small for the digits draws U+25A1 as `box` does
#[test]
fn a_cell_too_small_draws_the_replacement() {
    let font = Font::new(Stub::new(6, 12)).with_missing(Missing::Hex);
    assert!(
        hex(&font, '\u{2603}', Width::Narrow)
            .pixels()
            .iter()
            .all(|&p| p == 0x80)
    );
}

#[test]
fn only_a_missing_character_draws_its_codepoint() {
    let font = Font::new(Stub::new(8, 16)).with_missing(Missing::Hex);
    assert!(
        hex(&font, 'A', Width::Narrow)
            .pixels()
            .iter()
            .all(|&p| p == 0xff)
    );
    // the first character of a cluster names it
    let cluster = font
        .glyph(&['\u{2603}', '\u{301}'], Width::Narrow, Style::default())
        .expect("drawn");
    assert_eq!(cluster, hex(&font, '\u{2603}', Width::Narrow));
    // a sprite still draws as a sprite; under `font`, a range character
    // the face lacks draws its codepoint
    assert_eq!(font.policy(), SpritePolicy::SPRITES);
    let sprite = hex(&font, '─', Width::Narrow);
    assert_eq!(
        sprite,
        vtium_font::sprite::draw('─', 8, 16, 1).expect("a sprite")
    );
    let font = font.with_policy(SpritePolicy::SPRITES.with(Range::BoxDrawing, Source::Font));
    assert_eq!(picture(&hex(&font, '─', Width::Narrow))[2], "#...#...");
    // `box` is the default
    let plain = Font::new(Stub::new(8, 16));
    assert_eq!(plain.missing(), Missing::Replacement);
    assert!(
        hex(&plain, '\u{2603}', Width::Narrow)
            .pixels()
            .iter()
            .all(|&p| p == 0x80)
    );
}
