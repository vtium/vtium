//! The palette the `palette` key names (vtium/vtium#94): one of
//! vtium-term's named palettes, or `custom`, whose sixteen colours and two
//! defaults the `palette-<colour>` keys set (`src/config.c:1305-1324,
//! 1427-1448` at kmscon b1f7a7c5).

use vtium_term::{Palette, Rgb};

use super::Config;
use super::keys::{self, Key};

/// The sixteen colours' keys, in palette order.
const PALETTE_KEYS: [&Key; 16] = [
    &keys::PALETTE_BLACK,
    &keys::PALETTE_RED,
    &keys::PALETTE_GREEN,
    &keys::PALETTE_YELLOW,
    &keys::PALETTE_BLUE,
    &keys::PALETTE_MAGENTA,
    &keys::PALETTE_CYAN,
    &keys::PALETTE_LIGHT_GREY,
    &keys::PALETTE_DARK_GREY,
    &keys::PALETTE_LIGHT_RED,
    &keys::PALETTE_LIGHT_GREEN,
    &keys::PALETTE_LIGHT_YELLOW,
    &keys::PALETTE_LIGHT_BLUE,
    &keys::PALETTE_LIGHT_MAGENTA,
    &keys::PALETTE_LIGHT_CYAN,
    &keys::PALETTE_WHITE,
];

impl Config {
    /// The palette `palette` names. A name the key does not take is
    /// reported as it is read and leaves the value below it, so `legacy`
    /// is drawn only when no source names a palette.
    pub fn palette(&self) -> Palette {
        match self.text(&keys::PALETTE) {
            Some("custom") => {
                let rgb = |k: &Key| {
                    let [r, g, b] = self.rgb(k).unwrap_or_default();
                    Rgb::new(r, g, b)
                };
                Palette::custom(
                    PALETTE_KEYS.map(rgb),
                    rgb(&keys::PALETTE_FOREGROUND),
                    rgb(&keys::PALETTE_BACKGROUND),
                )
            }
            name => name.and_then(Palette::named).unwrap_or_default(),
        }
    }
}
