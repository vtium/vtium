//! Palettes: the 256 colors and the default foreground and background a
//! frame resolves cells through, and the palettes the configuration's
//! `palette` key names (vtium/vtium#94).
//!
//! A palette sets the sixteen ANSI colors and the two defaults; colors
//! 16 to 255 are xterm's 6x6x6 cube (levels 0, 95, 135, 175, 215, 255)
//! and grey ramp (8 to 238 in steps of 10) in every palette. The named
//! tables are the C reference's (`src/tsm/tsm-vte.c:254-451` at libtsm
//! eeaf2931), and `tests/palettes.rs` checks each against that source.
//!
//! A palette also holds the colors OSC 12 to 19 name: the cursor's, the
//! pointer's and the selection's, each unset until a program or the host
//! sets it, and drawn by its fallback while unset.
//!
//! A terminal holds the palette it was configured with and a copy the
//! program changes with OSC 4, 10 to 19 and 21. Frames draw through the
//! copy and the color queries answer from it; OSC 104 to 119 return its
//! colors to the configured palette's (vtium/vtium#38).

use crate::style::{Color, Rgb};

/// The 256-color palette, the default colors, and the colors of the
/// cursor, the pointer and the selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    colors: [Rgb; 256],
    fg: Rgb,
    bg: Rgb,
    special: [Option<Rgb>; Special::ALL.len()],
}

/// The colors past the defaults that xterm's OSC 12 to 19 name. Each is
/// unset in every named palette, and an unset one draws as its fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    /// OSC 12. Unset, a block cursor draws the cell under it inverted.
    Cursor,
    /// OSC 13, the pointer's foreground. Unset, the default foreground.
    PointerFg,
    /// OSC 14, the pointer's background. Unset, the default background.
    PointerBg,
    /// OSC 17, the selection's background. Unset, the selection draws its
    /// cells inverted.
    SelectionBg,
    /// OSC 19, the selection's foreground. Unset, the cell's foreground
    /// over a set selection background, else inverted.
    SelectionFg,
}

impl Special {
    pub const ALL: [Special; 5] = [
        Special::Cursor,
        Special::PointerFg,
        Special::PointerBg,
        Special::SelectionBg,
        Special::SelectionFg,
    ];

    fn slot(self) -> usize {
        self as usize
    }
}

/// A named palette: its sixteen colors and its two defaults.
struct Named {
    name: &'static str,
    colors: [Rgb; 16],
    fg: Rgb,
    bg: Rgb,
}

/// The names [`Palette::named`] takes, `legacy` first. The `palette` key
/// takes these and `custom`.
pub const PALETTE_NAMES: [&str; 9] = [
    NAMED[0].name,
    NAMED[1].name,
    NAMED[2].name,
    NAMED[3].name,
    NAMED[4].name,
    NAMED[5].name,
    NAMED[6].name,
    NAMED[7].name,
    NAMED[8].name,
];

/// The named palettes, in the C reference's order.
const NAMED: [Named; 9] = [
    Named {
        name: "legacy",
        colors: [
            Rgb::new(0, 0, 0),
            Rgb::new(205, 0, 0),
            Rgb::new(0, 205, 0),
            Rgb::new(205, 205, 0),
            Rgb::new(0, 0, 238),
            Rgb::new(205, 0, 205),
            Rgb::new(0, 205, 205),
            Rgb::new(229, 229, 229),
            Rgb::new(127, 127, 127),
            Rgb::new(255, 0, 0),
            Rgb::new(0, 255, 0),
            Rgb::new(255, 255, 0),
            Rgb::new(92, 92, 255),
            Rgb::new(255, 0, 255),
            Rgb::new(0, 255, 255),
            Rgb::new(255, 255, 255),
        ],
        fg: Rgb::new(229, 229, 229),
        bg: Rgb::new(0, 0, 0),
    },
    Named {
        name: "vga",
        colors: [
            Rgb::new(0, 0, 0),
            Rgb::new(170, 0, 0),
            Rgb::new(0, 170, 0),
            Rgb::new(170, 85, 0),
            Rgb::new(0, 0, 170),
            Rgb::new(170, 0, 170),
            Rgb::new(0, 170, 170),
            Rgb::new(170, 170, 170),
            Rgb::new(85, 85, 85),
            Rgb::new(255, 85, 85),
            Rgb::new(85, 255, 85),
            Rgb::new(255, 255, 85),
            Rgb::new(85, 85, 255),
            Rgb::new(255, 85, 255),
            Rgb::new(85, 255, 255),
            Rgb::new(255, 255, 255),
        ],
        fg: Rgb::new(170, 170, 170),
        bg: Rgb::new(0, 0, 0),
    },
    Named {
        name: "nord",
        colors: [
            Rgb::new(59, 66, 82),
            Rgb::new(191, 97, 106),
            Rgb::new(163, 190, 140),
            Rgb::new(235, 203, 139),
            Rgb::new(129, 161, 193),
            Rgb::new(180, 142, 173),
            Rgb::new(136, 192, 208),
            Rgb::new(229, 233, 240),
            Rgb::new(76, 86, 106),
            Rgb::new(191, 97, 106),
            Rgb::new(163, 190, 140),
            Rgb::new(235, 203, 139),
            Rgb::new(129, 161, 193),
            Rgb::new(180, 142, 173),
            Rgb::new(143, 188, 187),
            Rgb::new(236, 239, 244),
        ],
        fg: Rgb::new(216, 222, 233),
        bg: Rgb::new(46, 52, 64),
    },
    Named {
        name: "solarized",
        colors: [
            Rgb::new(7, 54, 66),
            Rgb::new(220, 50, 47),
            Rgb::new(133, 153, 0),
            Rgb::new(181, 137, 0),
            Rgb::new(38, 139, 210),
            Rgb::new(211, 54, 130),
            Rgb::new(42, 161, 152),
            Rgb::new(238, 232, 213),
            Rgb::new(0, 43, 54),
            Rgb::new(203, 75, 22),
            Rgb::new(88, 110, 117),
            Rgb::new(101, 123, 131),
            Rgb::new(131, 148, 150),
            Rgb::new(108, 113, 196),
            Rgb::new(147, 161, 161),
            Rgb::new(253, 246, 227),
        ],
        fg: Rgb::new(238, 232, 213),
        bg: Rgb::new(7, 54, 66),
    },
    Named {
        name: "solarized-black",
        colors: [
            Rgb::new(0, 0, 0),
            Rgb::new(220, 50, 47),
            Rgb::new(133, 153, 0),
            Rgb::new(181, 137, 0),
            Rgb::new(38, 139, 210),
            Rgb::new(211, 54, 130),
            Rgb::new(42, 161, 152),
            Rgb::new(238, 232, 213),
            Rgb::new(0, 43, 54),
            Rgb::new(203, 75, 22),
            Rgb::new(88, 110, 117),
            Rgb::new(101, 123, 131),
            Rgb::new(131, 148, 150),
            Rgb::new(108, 113, 196),
            Rgb::new(147, 161, 161),
            Rgb::new(253, 246, 227),
        ],
        fg: Rgb::new(238, 232, 213),
        bg: Rgb::new(0, 0, 0),
    },
    Named {
        name: "solarized-white",
        colors: [
            Rgb::new(7, 54, 66),
            Rgb::new(220, 50, 47),
            Rgb::new(133, 153, 0),
            Rgb::new(181, 137, 0),
            Rgb::new(38, 139, 210),
            Rgb::new(211, 54, 130),
            Rgb::new(42, 161, 152),
            Rgb::new(238, 232, 213),
            Rgb::new(0, 43, 54),
            Rgb::new(203, 75, 22),
            Rgb::new(88, 110, 117),
            Rgb::new(101, 123, 131),
            Rgb::new(131, 148, 150),
            Rgb::new(108, 113, 196),
            Rgb::new(147, 161, 161),
            Rgb::new(253, 246, 227),
        ],
        fg: Rgb::new(7, 54, 66),
        bg: Rgb::new(238, 232, 213),
    },
    Named {
        name: "soft-black",
        colors: [
            Rgb::new(0x3f, 0x3f, 0x3f),
            Rgb::new(0x70, 0x50, 0x50),
            Rgb::new(0x60, 0xb4, 0x8a),
            Rgb::new(0xdf, 0xaf, 0x8f),
            Rgb::new(0x9a, 0xb8, 0xd7),
            Rgb::new(0xdc, 0x8c, 0xc3),
            Rgb::new(0x8c, 0xd0, 0xd3),
            Rgb::new(0xff, 0xff, 0xff),
            Rgb::new(0x70, 0x90, 0x80),
            Rgb::new(0xdc, 0xa3, 0xa3),
            Rgb::new(0x72, 0xd5, 0xa3),
            Rgb::new(0xf0, 0xdf, 0xaf),
            Rgb::new(0x94, 0xbf, 0xf3),
            Rgb::new(0xec, 0x93, 0xd3),
            Rgb::new(0x93, 0xe0, 0xe3),
            Rgb::new(0xdc, 0xdc, 0xcc),
        ],
        fg: Rgb::new(0xdc, 0xdc, 0xcc),
        bg: Rgb::new(0x2c, 0x2c, 0x2c),
    },
    Named {
        name: "base16-dark",
        colors: [
            Rgb::new(0x00, 0x00, 0x00),
            Rgb::new(0xab, 0x46, 0x42),
            Rgb::new(0xa1, 0xb5, 0x6c),
            Rgb::new(0xf7, 0xca, 0x88),
            Rgb::new(0x7c, 0xaf, 0xc2),
            Rgb::new(0xba, 0x8b, 0xaf),
            Rgb::new(0x86, 0xc1, 0xb9),
            Rgb::new(0xaa, 0xaa, 0xaa),
            Rgb::new(0x55, 0x55, 0x55),
            Rgb::new(0xab, 0x46, 0x42),
            Rgb::new(0xa1, 0xb5, 0x6c),
            Rgb::new(0xf7, 0xca, 0x88),
            Rgb::new(0x7c, 0xaf, 0xc2),
            Rgb::new(0xba, 0x8b, 0xaf),
            Rgb::new(0x86, 0xc1, 0xb9),
            Rgb::new(0xff, 0xff, 0xff),
        ],
        fg: Rgb::new(0xd8, 0xd8, 0xd8),
        bg: Rgb::new(0x18, 0x18, 0x18),
    },
    Named {
        name: "base16-light",
        colors: [
            Rgb::new(0x00, 0x00, 0x00),
            Rgb::new(0xab, 0x46, 0x42),
            Rgb::new(0xa1, 0xb5, 0x6c),
            Rgb::new(0xf7, 0xca, 0x88),
            Rgb::new(0x7c, 0xaf, 0xc2),
            Rgb::new(0xba, 0x8b, 0xaf),
            Rgb::new(0x86, 0xc1, 0xb9),
            Rgb::new(0xaa, 0xaa, 0xaa),
            Rgb::new(0x55, 0x55, 0x55),
            Rgb::new(0xab, 0x46, 0x42),
            Rgb::new(0xa1, 0xb5, 0x6c),
            Rgb::new(0xf7, 0xca, 0x88),
            Rgb::new(0x7c, 0xaf, 0xc2),
            Rgb::new(0xba, 0x8b, 0xaf),
            Rgb::new(0x86, 0xc1, 0xb9),
            Rgb::new(0xff, 0xff, 0xff),
        ],
        fg: Rgb::new(0x18, 0x18, 0x18),
        bg: Rgb::new(0xd8, 0xd8, 0xd8),
    },
];

impl Default for Palette {
    /// `legacy`.
    fn default() -> Self {
        let [legacy, ..] = &NAMED;
        Self::custom(legacy.colors, legacy.fg, legacy.bg)
    }
}

impl Palette {
    /// A palette of `colors` and the defaults, with every [`Special`]
    /// color unset.
    pub fn new(colors: [Rgb; 256], fg: Rgb, bg: Rgb) -> Self {
        Self {
            colors,
            fg,
            bg,
            special: [None; Special::ALL.len()],
        }
    }

    /// The palette `name` names, one of [`PALETTE_NAMES`].
    pub fn named(name: &str) -> Option<Self> {
        NAMED
            .iter()
            .find(|n| n.name == name)
            .map(|n| Self::custom(n.colors, n.fg, n.bg))
    }

    /// The sixteen colors and the defaults given, over the cube and the
    /// grey ramp: `palette=custom`.
    pub fn custom(sixteen: [Rgb; 16], fg: Rgb, bg: Rgb) -> Self {
        const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let mut colors = [Rgb::default(); 256];
        for (i, c) in colors.iter_mut().enumerate() {
            *c = match i {
                0..=15 => sixteen.get(i).copied().unwrap_or_default(),
                16..=231 => {
                    let n = i.saturating_sub(16);
                    let level = |k: usize| LEVELS.get(k % 6).copied().unwrap_or(0);
                    Rgb::new(level(n / 36), level(n / 6), level(n))
                }
                _ => {
                    let grey =
                        u8::try_from(i.saturating_sub(232).saturating_mul(10).saturating_add(8))
                            .unwrap_or(0);
                    Rgb::new(grey, grey, grey)
                }
            };
        }
        Self::new(colors, fg, bg)
    }

    pub fn color(&self, index: u8) -> Rgb {
        self.colors
            .get(usize::from(index))
            .copied()
            .unwrap_or_default()
    }

    pub fn set_color(&mut self, index: u8, rgb: Rgb) {
        if let Some(c) = self.colors.get_mut(usize::from(index)) {
            *c = rgb;
        }
    }

    pub fn fg(&self) -> Rgb {
        self.fg
    }

    pub fn bg(&self) -> Rgb {
        self.bg
    }

    pub fn set_fg(&mut self, rgb: Rgb) {
        self.fg = rgb;
    }

    pub fn set_bg(&mut self, rgb: Rgb) {
        self.bg = rgb;
    }

    /// The color `which` names, `None` while unset.
    pub fn special(&self, which: Special) -> Option<Rgb> {
        self.special.get(which.slot()).copied().flatten()
    }

    /// Sets the color `which` names, or with `None` unsets it.
    pub fn set_special(&mut self, which: Special, rgb: Option<Rgb>) {
        if let Some(slot) = self.special.get_mut(which.slot()) {
            *slot = rgb;
        }
    }

    /// The color OSC 12 sets, `None` while the cursor draws inverted.
    pub fn cursor(&self) -> Option<Rgb> {
        self.special(Special::Cursor)
    }

    /// The pointer's foreground and background, the defaults while unset.
    pub fn pointer(&self) -> (Rgb, Rgb) {
        (
            self.special(Special::PointerFg).unwrap_or(self.fg),
            self.special(Special::PointerBg).unwrap_or(self.bg),
        )
    }

    /// `color` in RGB, `default` for the default color.
    pub(crate) fn resolve(&self, color: Color, default: Rgb) -> Rgb {
        match color {
            Color::Default => default,
            Color::Indexed(i) => self.color(i),
            Color::Rgb(rgb) => rgb,
        }
    }
}
