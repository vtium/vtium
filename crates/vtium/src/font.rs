//! Font selection with its fallback.
//!
//! A font is looked up at the size asked for. When none is in use yet, a
//! failure tries [`FALLBACK_SIZE`] once, and a second failure is the
//! error; with a font in use, as at a zoom, a failure is the error and
//! that font stays. The C reference retried at 16 for as long as the
//! lookup failed (`src/terminal.c:921-939` at kmscon b1f7a7c5,
//! aaronsb/kmscon#63).

use vtium_font::sprite::Range;
use vtium_font::{Missing, Source, SpritePolicy};

use crate::config::Config;
use crate::config::keys::{
    GLYPH_SOURCE, GLYPH_SOURCE_BLOCK, GLYPH_SOURCE_BOX, GLYPH_SOURCE_BRAILLE, GLYPH_SOURCE_LEGACY,
    GLYPH_SOURCE_OCTANTS, GLYPH_SOURCE_SEXTANTS, Key, MISSING,
};

/// Each sprite range and the key that sets its source alone.
pub const RANGE_KEYS: [(Range, &Key); 6] = [
    (Range::BoxDrawing, &GLYPH_SOURCE_BOX),
    (Range::BlockElements, &GLYPH_SOURCE_BLOCK),
    (Range::Braille, &GLYPH_SOURCE_BRAILLE),
    (Range::Sextants, &GLYPH_SOURCE_SEXTANTS),
    (Range::Octants, &GLYPH_SOURCE_OCTANTS),
    (Range::LegacyComputing, &GLYPH_SOURCE_LEGACY),
];

fn source(text: &str) -> Option<Source> {
    match text {
        "font" => Some(Source::Font),
        "sprites" => Some(Source::Sprites),
        "auto" => Some(Source::Auto),
        _ => None,
    }
}

/// A font's sprite policy from the configuration: `glyph-source` for all
/// six ranges, else the face's own `default`, then each range's own key
/// over it (vtium-font's README, "#21 builds a font's policy").
pub fn policy(config: &Config, default: SpritePolicy) -> SpritePolicy {
    let base = config
        .text(&GLYPH_SOURCE)
        .and_then(source)
        .map_or(default, SpritePolicy::all);
    RANGE_KEYS.iter().fold(base, |p, &(range, key)| {
        match config.text(key).and_then(source) {
            Some(s) => p.with(range, s),
            None => p,
        }
    })
}

/// What a character the font lacks draws as, from `missing`.
pub fn missing(config: &Config) -> Missing {
    match config.text(&MISSING) {
        Some("hex") => Missing::Hex,
        _ => Missing::Replacement,
    }
}

/// The size tried once when the first lookup fails.
pub const FALLBACK_SIZE: u32 = 16;

/// Looks a font up at `size` with `find`, which gives the font at a size
/// or why there is none; `in_use` says whether a font is in use. Returns
/// the font and the size it was found at.
pub fn select<F, E>(
    size: u32,
    in_use: bool,
    mut find: impl FnMut(u32) -> Result<F, E>,
) -> Result<(F, u32), E> {
    match find(size) {
        Ok(font) => Ok((font, size)),
        Err(e) if in_use => Err(e),
        Err(_) => find(FALLBACK_SIZE).map(|f| (f, FALLBACK_SIZE)),
    }
}

/// The height `font-size=0` picks for a display `width` by `height`: at
/// most 120 columns, taking a cell as half as wide as it is high, and at
/// least 40 rows, and never under 16 (`src/terminal.c:899-918`).
pub fn auto_size(width: u32, height: u32) -> u32 {
    if width == 0 || height == 0 {
        return FALLBACK_SIZE;
    }
    let by_width = width.saturating_mul(2) / 120;
    by_width.min(height / 40).max(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(args: &[&str]) -> Config {
        let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        crate::config::load(&args, None, &mut |_: &std::path::Path| Ok(None)).config
    }

    #[test]
    fn the_policy_is_the_faces_unless_a_key_sets_it() {
        let named = SpritePolicy::all(Source::Font).with(Range::Octants, Source::Sprites);
        assert_eq!(policy(&config(&[]), named), named);
        assert_eq!(
            policy(&config(&["--glyph-source=sprites"]), named),
            SpritePolicy::SPRITES
        );
        let one = policy(&config(&["--glyph-source-braille=auto"]), named);
        assert_eq!(one, named.with(Range::Braille, Source::Auto));
        // A range's own key wins over the key for all six.
        let both = policy(
            &config(&["--glyph-source=font", "--glyph-source-legacy=sprites"]),
            SpritePolicy::SPRITES,
        );
        assert_eq!(
            both,
            SpritePolicy::all(Source::Font).with(Range::LegacyComputing, Source::Sprites)
        );
    }

    #[test]
    fn missing_is_box_or_hex() {
        assert_eq!(missing(&config(&[])), Missing::Replacement);
        assert_eq!(missing(&config(&["--missing=hex"])), Missing::Hex);
        assert_eq!(missing(&config(&["--missing=box"])), Missing::Replacement);
    }

    #[test]
    fn the_automatic_size() {
        assert_eq!(auto_size(1366, 768), 19);
        assert_eq!(auto_size(1280, 800), 20);
        assert_eq!(auto_size(3840, 2160), 54);
        assert_eq!(auto_size(640, 480), 16);
        assert_eq!(auto_size(0, 768), 16);
    }

    #[test]
    fn the_size_asked_for() {
        assert_eq!(select(24, false, |s| Ok::<_, ()>(s * 2)), Ok((48, 24)));
    }

    #[test]
    fn with_no_font_in_use_a_failure_falls_back_once() {
        let mut asked = Vec::new();
        let found = select(99, false, |s| {
            asked.push(s);
            if s == FALLBACK_SIZE {
                Ok("font")
            } else {
                Err("no 99")
            }
        });
        assert_eq!(found, Ok(("font", FALLBACK_SIZE)));
        assert_eq!(asked, [99, FALLBACK_SIZE]);
    }

    #[test]
    fn a_lookup_that_always_fails_is_an_error_after_the_fallback() {
        let mut asked = Vec::new();
        let found = select(99, false, |s| {
            asked.push(s);
            Err::<(), _>(s)
        });
        assert_eq!(found, Err(FALLBACK_SIZE));
        assert_eq!(asked, [99, FALLBACK_SIZE]);
    }

    #[test]
    fn with_a_font_in_use_a_failure_is_the_error() {
        let mut asked = Vec::new();
        let found = select(99, true, |s| {
            asked.push(s);
            Err::<(), _>(s)
        });
        assert_eq!(found, Err(99));
        assert_eq!(asked, [99]);
    }
}
