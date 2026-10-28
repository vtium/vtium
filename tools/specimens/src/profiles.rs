//! The display profiles: for each, the display, the font vtium draws on it,
//! the grid that gives, and how the card's blocks are arrayed on it.

use crate::card::{BLOCK_COLS, BLOCK_ROWS, BLOCKS};

/// What a profile draws its text with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyphs {
    /// A set built into vtium (ADR-1200 D7), drawn at `height` pixels.
    Builtin { set: &'static str, height: u32 },
    /// A font no merged code draws yet: its description, and the issues
    /// it waits on.
    Pending {
        font: &'static str,
        issues: &'static [u32],
    },
}

/// One display profile.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    /// Its directory under `docs/specimens/`.
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    /// The display's depth, as the card's header names it.
    pub depth: &'static str,
    /// The display class it falls in, by the display-capabilities record's
    /// table (vtium/vtium#136).
    pub class: &'static str,
    pub glyphs: Glyphs,
    /// What it is, one line, for the tables.
    pub summary: &'static str,
    /// Why this font and size is the best choice on this display.
    pub why: &'static str,
    /// The vtium.conf lines for the profile, comments included.
    pub conf: &'static str,
}

/// How a profile's grid holds the card: the blocks a page holds across and
/// down, where the first sits, and the pages the card takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub cols: usize,
    pub rows: usize,
    pub across: usize,
    pub down: usize,
    /// The first block's top row and left column: the blocks are centered.
    pub top: usize,
    pub left: usize,
    pub pages: usize,
}

impl Layout {
    /// The layout of a grid `cols` by `rows`; `None` for a grid smaller
    /// than one block.
    pub fn new(cols: usize, rows: usize) -> Option<Layout> {
        let across = cols / BLOCK_COLS;
        let down = rows / BLOCK_ROWS;
        let per_page = across * down;
        if per_page == 0 {
            return None;
        }
        Some(Layout {
            cols,
            rows,
            across,
            down,
            top: (rows - down * BLOCK_ROWS) / 2,
            left: (cols - across * BLOCK_COLS) / 2,
            pages: BLOCKS.div_ceil(per_page),
        })
    }

    /// The blocks page `page` holds, each with its top row and left column.
    pub fn blocks(&self, page: usize) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
        let per_page = self.across * self.down;
        (page * per_page..((page + 1) * per_page).min(BLOCKS)).map(move |b| {
            let i = b % per_page;
            (
                b,
                self.top + (i / self.across) * BLOCK_ROWS,
                self.left + (i % self.across) * BLOCK_COLS,
            )
        })
    }
}

const UNSCII_WHY: &str = "ADR-1200 D7 draws the built-in Unscii-8 for cells under 16 pixels high, and #118's rule has `font-size=0` take a set's smallest height when 16-pixel cells would leave fewer than 80x25 of them, so vtium draws this display in 8x8 cells with no font keys at all. It is the most text the display holds at a cell a person can still read, and it costs no disk: D11 builds nothing beyond the built-ins for an English machine at this size. Unscii-8 holds no CJK or Hangul, nor the emoji and private-use symbols, so those draw as the missing glyph, U+25A1, until #141's per-character fallback draws them from unifont; in a zh, ja or ko locale D7 draws unifont at 16 pixels instead.";

const UNIFONT_WHY: &str = "16-pixel cells leave at least 80x25 here, so #118's rule keeps `font-size=0`'s 16 pixels, and ADR-1200 D7 draws the built-in unifont from 16 pixels: 8x16 cells for Latin, Greek and Cyrillic and 16x16 for CJK and Hangul, which it covers whole, with nothing built. The size the display wants is under 24 pixels, the smallest outline size D11 builds, so a default machine holds no outline set for it. Emoji draw as unifont's monochrome glyphs where it has one, and Nerd Font symbols as the missing glyph until #23 and #64 draw them from an outline font.";

/// Every profile, smallest display first.
pub const PROFILES: &[Profile] = &[
    Profile {
        name: "320x240-1bit",
        width: 320,
        height: 240,
        depth: "1-bit",
        class: "1-bit",
        glyphs: Glyphs::Pending {
            font: "Unscii-8, 8x8",
            issues: &[135],
        },
        summary: "A 1-bit 320x240 panel, such as an SSD130x OLED or a memory LCD",
        why: "A 1-bit panel lights a pixel or does not, so a bitmap font drawn whole fits it exactly, and Unscii-8 at 8x8 gives the same 40x30 cells as the color profile. #135 maps each cell's colors to lit or unlit.",
        conf: "",
    },
    Profile {
        name: "320x240",
        width: 320,
        height: 240,
        depth: "24-bit color",
        class: "small color LCD",
        glyphs: Glyphs::Builtin {
            set: "unscii-8",
            height: 8,
        },
        summary: "minspec's display floor, a 320x240 SPI panel",
        why: UNSCII_WHY,
        conf: "# vtium's defaults: font-size=0 picks 8 pixels here (#118), which\n# draws the built-in Unscii-8 (ADR-1200 D7).\nfont-size=0\n",
    },
    Profile {
        name: "480x320",
        width: 480,
        height: 320,
        depth: "24-bit color",
        class: "small color LCD",
        glyphs: Glyphs::Builtin {
            set: "unscii-8",
            height: 8,
        },
        summary: "A 480x320 SPI panel, 3.5 inches",
        why: UNSCII_WHY,
        conf: "# vtium's defaults: font-size=0 picks 8 pixels here (#118), which\n# draws the built-in Unscii-8 (ADR-1200 D7).\nfont-size=0\n",
    },
    Profile {
        name: "640x480",
        width: 640,
        height: 480,
        depth: "24-bit color",
        class: "small color LCD",
        glyphs: Glyphs::Builtin {
            set: "unifont",
            height: 16,
        },
        summary: "VGA, the smallest display with 80 columns of 16-pixel text",
        why: UNIFONT_WHY,
        conf: "# vtium's defaults: font-size=0 picks 16 pixels here, which draws\n# the built-in unifont (ADR-1200 D7).\nfont-size=0\n",
    },
    Profile {
        name: "800x480",
        width: 800,
        height: 480,
        depth: "24-bit color",
        class: "small color LCD",
        glyphs: Glyphs::Builtin {
            set: "unifont",
            height: 16,
        },
        summary: "A 7-inch panel, as on the Raspberry Pi touch display",
        why: UNIFONT_WHY,
        conf: "# vtium's defaults: font-size=0 picks 16 pixels here, which draws\n# the built-in unifont (ADR-1200 D7).\nfont-size=0\n",
    },
    Profile {
        name: "1366x768",
        width: 1366,
        height: 768,
        depth: "24-bit color",
        class: "order unknown",
        glyphs: Glyphs::Builtin {
            set: "unifont",
            height: 16,
        },
        summary: "A common laptop panel",
        why: "`font-size=0` asks 19 pixels here, and a bitmap set draws at the largest whole multiple of its cell that fits (ADR-1200 D9), so the built-in unifont (D7) draws at 16 pixels, 170x48 cells, and the whole card fits one page. 19 pixels is under 24, the smallest outline size D11 builds, so a default machine holds no outline set for this display. Emoji draw as unifont's monochrome glyphs where it has one, and Nerd Font symbols as the missing glyph until #23 and #64 draw them from an outline font.",
        conf: "# vtium's defaults: font-size=0 asks 19 pixels here, and unifont\n# draws at 16, its largest whole multiple that fits (ADR-1200 D9).\nfont-size=0\n",
    },
    Profile {
        name: "1920x1080",
        width: 1920,
        height: 1080,
        depth: "24-bit color",
        class: "order unknown",
        glyphs: Glyphs::Pending {
            font: "an outline font at 24 pixels",
            issues: &[23, 126, 141],
        },
        summary: "A full HD monitor",
        why: "ADR-1200 D11 builds one outline size for the display the package hook sees, snapped down to 24, 32 or 48 pixels: 1920x1080 gives 24, a grid of 160x45, with unifont filling the outline font's gaps.",
        conf: "",
    },
    Profile {
        name: "2560x1440",
        width: 2560,
        height: 1440,
        depth: "24-bit color",
        class: "order unknown",
        glyphs: Glyphs::Pending {
            font: "an outline font at 32 pixels",
            issues: &[23, 126, 141],
        },
        summary: "A QHD monitor",
        why: "ADR-1200 D11 snaps the size this display wants down to 32 pixels, a grid of 160x45, with unifont filling the outline font's gaps.",
        conf: "",
    },
];

/// The profile named `name`.
pub fn profile(name: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blocks_are_centered_and_paged() {
        let l = Layout::new(40, 30).unwrap();
        assert_eq!((l.across, l.down, l.top, l.left, l.pages), (1, 1, 0, 0, 4));
        let l = Layout::new(60, 40).unwrap();
        assert_eq!((l.top, l.left, l.pages), (5, 10, 4));
        let l = Layout::new(80, 30).unwrap();
        assert_eq!(l.pages, 2);
        assert_eq!(l.blocks(1).collect::<Vec<_>>(), vec![(2, 0, 0), (3, 0, 40)]);
        let l = Layout::new(170, 48).unwrap();
        assert_eq!((l.top, l.left, l.pages), (9, 5, 1));
        assert_eq!(
            l.blocks(0).map(|b| b.2).collect::<Vec<_>>(),
            [5, 45, 85, 125]
        );
        assert_eq!(Layout::new(39, 30), None);
        assert_eq!(Layout::new(40, 29), None);
    }

    #[test]
    fn every_profile_has_a_unique_name_and_a_conf_when_built() {
        for (i, p) in PROFILES.iter().enumerate() {
            assert!(PROFILES[..i].iter().all(|q| q.name != p.name), "{}", p.name);
            let built = matches!(p.glyphs, Glyphs::Builtin { .. });
            assert_eq!(built, !p.conf.is_empty(), "{}", p.name);
        }
    }
}
