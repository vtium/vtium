//! vtium's built-in font, "vtium built-in": the font vtium draws its own
//! messages in, and the font it draws with when every other font is
//! missing or unusable. It is compiled into the binary as three strikes,
//! 8×8, 8×16 and 16×32, each cut by `build.rs` from its source with the
//! list in `data/builtin-chars.txt` and trusted by construction. The font
//! holds printable ASCII, Latin-1, the ellipsis, the four arrows, box
//! drawing and blocks, and the focus mark's circles, and nothing else.
//!
//! A cell under 16 pixels high draws the 8×8 strike, one from 16 to 31 the
//! 8×16 strike, and one from 32 the 16×32 strike, at the largest whole
//! multiple of the strike that fits the cell ([`face`]).
//!
//! References:
//! - ADR-1204 D1: the built-in font, its list, its strikes, which strike
//!   draws which cell, and its name.

use std::sync::{Arc, OnceLock};

use super::cut::{STRIKES, Strike};
use super::face::SetFace;
use super::read::Set;
use crate::face::Cell;

pub use super::cut::{NAME, SET_NAME};

include!(concat!(env!("OUT_DIR"), "/builtin_chars.rs"));

/// Whether the built-in font's list holds `c`. A message whose every
/// character it holds reads in the built-in font on every head.
pub fn holds(c: char) -> bool {
    super::cut::listed(LIST, u32::from(c))
}

/// Whether the built-in font's list holds every character of `text`.
pub fn holds_all(text: &str) -> bool {
    text.chars().all(holds)
}

/// The strike's bytes, as the build packed them, smallest first.
const BYTES: [&[u8]; 3] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/builtin-8x8.vtglass")),
    include_bytes!(concat!(env!("OUT_DIR"), "/builtin-8x16.vtglass")),
    include_bytes!(concat!(env!("OUT_DIR"), "/builtin-16x32.vtglass")),
];

/// The strike numbered `n`, smallest first, read the first time it is
/// asked for. `None` for a number past the last strike, or if the build
/// packed a strike its own reader refuses, which the tests hold against.
pub fn strike(n: usize) -> Option<&'static Arc<Set>> {
    static SLOTS: [OnceLock<Option<Arc<Set>>>; 3] = [const { OnceLock::new() }; 3];
    let bytes = *BYTES.get(n)?;
    SLOTS
        .get(n)?
        .get_or_init(|| Set::from_static(bytes).ok().map(Arc::new))
        .as_ref()
}

/// The strike that draws in `cell`, by its number, and the whole multiple
/// it draws at: by the cell's height, the 8×8 strike under 16 pixels, the
/// 8×16 strike from 16 to 31 and the 16×32 strike from 32. A strike more
/// than 2 pixels wider than the cell gives way to the next smaller one, as
/// a built-in glyph may reach 1 pixel into the cell on each side. The
/// multiple is the largest that fits the cell in both directions, and at
/// least 1.
pub fn choose(cell: Cell) -> (usize, u32) {
    let by_height = match cell.height {
        0..16 => 0,
        16..32 => 1,
        _ => 2,
    };
    let fits = |s: &Strike| u32::from(s.width) <= cell.width.saturating_add(2);
    let (n, strike) = STRIKES
        .iter()
        .enumerate()
        .filter(|(n, _)| *n <= by_height)
        .rev()
        .find(|(n, s)| *n == 0 || fits(s))
        .unwrap_or((0, &STRIKES[0]));
    let across = cell.width.checked_div(u32::from(strike.width));
    let down = cell.height.checked_div(u32::from(strike.height));
    let scale = across.zip(down).map_or(1, |(a, d)| a.min(d).max(1));
    (n, scale)
}

/// The built-in font's face for `cell`: the strike [`choose`] picks, at
/// its multiple. Its own cell is that multiple of the strike's, which the
/// caller centres in `cell` where the two differ.
pub fn face(cell: Cell) -> Option<SetFace> {
    let (n, scale) = choose(cell);
    let height = u32::from(STRIKES.get(n)?.height).saturating_mul(scale);
    SetFace::new(Arc::clone(strike(n)?), height)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::super::cut;
    use super::*;
    use crate::face::Face;

    fn cell(width: u32, height: u32) -> Cell {
        Cell { width, height }
    }

    /// The characters ADR-1204 D1 lists, and the four arrows that a
    /// notice names a chord's arrow key with, from the list file the build
    /// reads.
    #[test]
    fn the_list_holds_d1s_ranges() {
        let d1 = [
            (0x20, 0x7E),
            (0xA0, 0xFF),
            (0x2026, 0x2026),
            (0x2190, 0x2193),
            (0x2500, 0x259F),
            (0x25CB, 0x25CB),
            (0x25CF, 0x25CF),
        ];
        assert_eq!(LIST, d1);
        assert!(holds('A') && holds('\u{b7}') && holds('\u{2026}') && holds('\u{25cf}'));
        assert!(!holds('\u{1f}') && !holds('\u{7f}') && !holds('\u{2194}') && !holds('中'));
        assert!(holds_all("● desk · console 2 of 3") && !holds_all("Tastatur ⇧"));
    }

    /// Each strike holds every character of the list that its source
    /// holds, the same glyph, and no character beyond the list. Terminus
    /// lacks ten characters of the list, which vtium's sprites draw.
    #[test]
    fn each_strike_holds_every_listed_character_its_source_holds() {
        let root = env!("CARGO_MANIFEST_DIR");
        let list = std::fs::read_to_string(format!("{root}/data/builtin-chars.txt")).unwrap();
        let ranges = cut::list(&list).unwrap();
        let lacking: Vec<u32> = (0x2504..=0x2507)
            .chain(0x254C..=0x254F)
            .chain([0x2594, 0x2595])
            .collect();
        for (n, s) in STRIKES.iter().enumerate() {
            let text = std::fs::read_to_string(format!("{root}/{}", s.path)).unwrap();
            let source = cut::read(s, &text).unwrap();
            let set = strike(n).expect("a strike the reader takes");
            let own = cell(s.width.into(), s.height.into());
            assert_eq!(set.cells(), [own], "{}", s.file);
            let face = SetFace::new(Arc::clone(set), own.height).unwrap();
            let mut missing = Vec::new();
            for &(a, b) in &ranges {
                for cp in a..=b {
                    let c = char::from_u32(cp).unwrap();
                    match source.glyphs.get(&cp) {
                        Some(bits) => {
                            assert!(face.has_glyph(c), "{} lacks U+{cp:04X}", s.file);
                            let g = face.draw(&[c], Default::default()).unwrap();
                            let rows = usize::from(s.width.div_ceil(8));
                            for y in 0..u32::from(s.height) {
                                for x in 0..u32::from(s.width) {
                                    let byte = bits[y as usize * rows + x as usize / 8];
                                    let on = byte & (0x80 >> (x % 8)) != 0;
                                    assert_eq!(
                                        g.pixel(x, y).is_some_and(|p| p[0] != 0),
                                        on,
                                        "{} U+{cp:04X}",
                                        s.file
                                    );
                                }
                            }
                        }
                        None => missing.push(cp),
                    }
                }
            }
            let expected: &[u32] = if n == 0 { &[] } else { &lacking };
            assert_eq!(missing, expected, "{} lacks", s.file);
            for cp in source
                .glyphs
                .keys()
                .filter(|cp| !cut::listed(&ranges, **cp))
            {
                let c = char::from_u32(*cp).unwrap();
                assert!(!face.has_glyph(c), "{} holds U+{cp:04X}", s.file);
            }
        }
    }

    /// Each strike draws the letter F with the rows written out here, read
    /// by eye from its source file, so a reader that turned or shifted
    /// every glyph would fail this test even though the strike would then
    /// still match what the same reader makes of the source. F tells
    /// left from right and top from bottom.
    #[test]
    fn each_strike_draws_f_as_its_source_does() {
        let unscii_8x8 = [
            ".######.", ".##.....", ".##.....", ".#####..", ".##.....", ".##.....", ".##.....",
            "........",
        ];
        let terminus_8x16 = [
            "........", "........", ".######.", ".#......", ".#......", ".#......", ".####...",
            ".#......", ".#......", ".#......", ".#......", ".#......", "........", "........",
            "........", "........",
        ];
        let blank = "................";
        let bar = "..############..";
        let arm = "..#########.....";
        let stem = "..##............";
        let mut terminus_16x32 = vec![blank; 6];
        terminus_16x32.extend([bar, bar]);
        terminus_16x32.extend([stem; 7]);
        terminus_16x32.extend([arm, arm]);
        terminus_16x32.extend([stem; 9]);
        terminus_16x32.extend([blank; 6]);
        let expected: [&[&str]; 3] = [&unscii_8x8, &terminus_8x16, &terminus_16x32];
        for (n, rows) in expected.into_iter().enumerate() {
            let s = &STRIKES[n];
            let face = SetFace::new(Arc::clone(strike(n).unwrap()), s.height.into()).unwrap();
            let g = face.draw(&['F'], Default::default()).unwrap();
            let drawn: Vec<String> = (0..u32::from(s.height))
                .map(|y| {
                    (0..u32::from(s.width))
                        .map(|x| match g.pixel(x, y).is_some_and(|p| p[0] != 0) {
                            true => '#',
                            false => '.',
                        })
                        .collect()
                })
                .collect();
            assert_eq!(drawn, rows, "{}", s.file);
        }
    }

    /// Each strike records the SHA-256 of the source file it was cut
    /// from, and that digest is the one `data/README.md` states. A source
    /// file replaced or edited, for example regenerated from another
    /// Terminus version, changes the digest and fails this test.
    #[test]
    fn each_strike_is_cut_from_its_pinned_source() {
        let pinned = [
            "03094f7fbab7085cf6a6b624cee61e47e71ce5d0c2f308c2f4436afdc17f776c",
            "5197662b22bf9f3e68d4af9f969a7fefa3edae40dd82ae969a147381130fb4ae",
            "5ad01972d58adfee75077e2c65ba5947e260043031664cb8ad8f98b5473b23a4",
        ];
        for (n, sha) in pinned.into_iter().enumerate() {
            let set = strike(n).unwrap();
            let file = STRIKES[n].file;
            assert_eq!(set.meta_values("source-sha256").next(), Some(sha), "{file}");
        }
    }

    /// Each strike's provenance names the font as a set name spells
    /// "vtium built-in", never Terminus, and its source.
    #[test]
    fn each_strike_is_named_vtium_built_in() {
        for (n, s) in STRIKES.iter().enumerate() {
            let set = strike(n).unwrap();
            assert_eq!(set.meta_values("name").next(), Some(SET_NAME), "{}", s.file);
            assert_eq!(
                set.meta_values("source").next(),
                Some(s.source),
                "{}",
                s.file
            );
        }
    }

    /// A cell under 16 pixels draws the 8×8 strike, one from 16 to 31 the
    /// 8×16 and one from 32 the 16×32, at the largest whole multiple that
    /// fits; a strike much wider than the cell gives way to a smaller one.
    #[test]
    fn the_cells_height_chooses_the_strike() {
        assert_eq!(choose(cell(8, 8)), (0, 1));
        assert_eq!(choose(cell(6, 9)), (0, 1));
        assert_eq!(choose(cell(8, 15)), (0, 1));
        assert_eq!(choose(cell(8, 16)), (1, 1));
        assert_eq!(choose(cell(12, 24)), (1, 1));
        assert_eq!(choose(cell(15, 31)), (1, 1));
        assert_eq!(choose(cell(16, 32)), (2, 1));
        assert_eq!(choose(cell(24, 48)), (2, 1));
        assert_eq!(choose(cell(32, 64)), (2, 2));
        assert_eq!(choose(cell(48, 100)), (2, 3));
        // 16 columns in a cell 12 wide would squeeze: the 8×16 draws
        assert_eq!(choose(cell(12, 32)), (1, 1));
        assert_eq!(choose(cell(0, 0)), (0, 1));
        let f = face(cell(32, 64)).unwrap();
        assert_eq!(f.cell(), cell(32, 64));
        assert_eq!(face(cell(12, 24)).unwrap().cell(), cell(8, 16));
    }
}
