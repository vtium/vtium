//! The palettes a host chooses with the `palette` command, as INTERFACE.md's
//! `palettes` block lists them: colours 0 to 15 and the default foreground
//! and background. Colours 16 to 255 are xterm's cube and grey ramp in
//! every palette.

use vtium_term::{Palette, Rgb};

/// A palette's name, its sixteen colours and its defaults, each `0xRRGGBB`.
#[derive(Clone, Copy, Debug)]
pub struct Named {
    pub name: &'static str,
    colors: [u32; 16],
    fg: u32,
    bg: u32,
}

/// The palette the reference host starts with.
pub const DEFAULT: &str = "legacy";

#[rustfmt::skip]
pub const PALETTES: [Named; 9] = [
    named("legacy", [0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5, 0x7f7f7f, 0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff], 0xe5e5e5, 0x000000),
    named("vga", [0x000000, 0xaa0000, 0x00aa00, 0xaa5500, 0x0000aa, 0xaa00aa, 0x00aaaa, 0xaaaaaa, 0x555555, 0xff5555, 0x55ff55, 0xffff55, 0x5555ff, 0xff55ff, 0x55ffff, 0xffffff], 0xaaaaaa, 0x000000),
    named("nord", [0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0, 0x4c566a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4], 0xd8dee9, 0x2e3440),
    named("solarized", [0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5, 0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3], 0xeee8d5, 0x073642),
    named("solarized-black", [0x000000, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5, 0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3], 0xeee8d5, 0x000000),
    named("solarized-white", [0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5, 0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3], 0x073642, 0xeee8d5),
    named("soft-black", [0x3f3f3f, 0x705050, 0x60b48a, 0xdfaf8f, 0x9ab8d7, 0xdc8cc3, 0x8cd0d3, 0xffffff, 0x709080, 0xdca3a3, 0x72d5a3, 0xf0dfaf, 0x94bff3, 0xec93d3, 0x93e0e3, 0xdcdccc], 0xdcdccc, 0x2c2c2c),
    named("base16-dark", [0x000000, 0xab4642, 0xa1b56c, 0xf7ca88, 0x7cafc2, 0xba8baf, 0x86c1b9, 0xaaaaaa, 0x555555, 0xab4642, 0xa1b56c, 0xf7ca88, 0x7cafc2, 0xba8baf, 0x86c1b9, 0xffffff], 0xd8d8d8, 0x181818),
    named("base16-light", [0x000000, 0xab4642, 0xa1b56c, 0xf7ca88, 0x7cafc2, 0xba8baf, 0x86c1b9, 0xaaaaaa, 0x555555, 0xab4642, 0xa1b56c, 0xf7ca88, 0x7cafc2, 0xba8baf, 0x86c1b9, 0xffffff], 0x181818, 0xd8d8d8),
];

const fn named(name: &'static str, colors: [u32; 16], fg: u32, bg: u32) -> Named {
    Named {
        name,
        colors,
        fg,
        bg,
    }
}

fn rgb(c: u32) -> Rgb {
    let [_, r, g, b] = c.to_be_bytes();
    Rgb::new(r, g, b)
}

impl Named {
    /// The palette by name.
    pub fn find(name: &str) -> Option<&'static Self> {
        PALETTES.iter().find(|p| p.name == name)
    }

    /// The palette for vtium-term: these sixteen colours over the default
    /// palette's cube and grey ramp.
    pub fn palette(&self) -> Palette {
        let mut palette = Palette::default();
        for (i, c) in (0u8..).zip(self.colors) {
            palette.set_color(i, rgb(c));
        }
        palette.set_fg(rgb(self.fg));
        palette.set_bg(rgb(self.bg));
        palette
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_is_vtium_terms_default_palette() {
        let legacy = Named::find(DEFAULT).map(Named::palette);
        assert_eq!(legacy, Some(Palette::default()));
    }

    #[test]
    fn the_cube_and_ramp_are_every_palettes() {
        let nord = Named::find("nord").map(Named::palette);
        let p = nord
            .as_ref()
            .map(|p| (p.color(16), p.color(231), p.color(232), p.fg()));
        assert_eq!(
            p,
            Some((
                Rgb::new(0, 0, 0),
                Rgb::new(255, 255, 255),
                Rgb::new(8, 8, 8),
                Rgb::new(0xd8, 0xde, 0xe9)
            ))
        );
    }
}
