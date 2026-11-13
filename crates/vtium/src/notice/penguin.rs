//! The panic screen's drawing (ADR-1401 D7): the maintainer's drawing of
//! Tux, the Linux penguin, panicked inside a computer monitor. The Tux
//! character is Larry Ewing's, drawn with The GIMP, and COPYING credits
//! both.
//!
//! A build without image support draws the drawing in 7-bit ASCII, a
//! character to a cell: [`FULL`], 32 columns by 20 rows, or [`SMALL`], the
//! monitor cropped from it, 21 columns by 10 rows. A build with image
//! support draws the maintainer's 80x80 sprite in their place. vtium's
//! root process decodes no compressed image, so the sprite is compiled in
//! as a binary PPM, [`PPM`]: a short header, then its RGB samples,
//! uncompressed. When the notice draws the sprite, it matches each
//! pixel's color to its entry in [`PALETTE`], six colors.
//! `tests/panic_sprite.rs` checks that the PPM is 80x80, that every pixel
//! is one of the palette's colors, and that the palette has no entry the
//! sprite does not use.

use vtium_term::Rgb;

/// The full drawing: 20 lines of at most 32 characters.
pub const FULL: &str = include_str!("../../art/panic-penguin-aaron.txt");

/// The small drawing, for a screen where the full one and the text do not
/// both fit: 10 lines of 21 characters.
pub const SMALL: &str = include_str!("../../art/panic-penguin-aaron-small.txt");

/// The sprite's width and height in pixels.
pub const SPRITE_SIZE: (u32, u32) = (80, 80);

/// The sprite's colors, the six its pixels match, in this order: black, white,
/// gray, beak orange, mouth dark red and tongue pink.
pub static PALETTE: [Rgb; 6] = [
    Rgb::new(0, 0, 0),
    Rgb::new(255, 255, 255),
    Rgb::new(128, 128, 128),
    Rgb::new(247, 168, 0),
    Rgb::new(40, 4, 4),
    Rgb::new(240, 120, 150),
];

/// The sprite as a binary PPM (P6): [`PPM_HEADER`], then its pixels row
/// by row, three bytes each, red, green and blue.
pub static PPM: &[u8] = include_bytes!("../../art/panic-penguin-80x80-oO.ppm");

/// The header [`PPM`] begins with: P6, 80 by 80 pixels, samples up to 255.
pub const PPM_HEADER: &[u8] = b"P6\n80 80\n255\n";

/// The sprite's RGB samples, [`PPM`] past its header: `None` for a PPM
/// that does not begin with [`PPM_HEADER`].
pub fn samples() -> Option<&'static [u8]> {
    PPM.strip_prefix(PPM_HEADER)
}

/// The drawing this build shows on the panic screen: the ASCII drawing,
/// and in a build with image support the sprite in its place.
pub fn drawing() -> vtium_render::Drawing {
    let drawing = vtium_render::Drawing::text(FULL, SMALL);
    #[cfg(feature = "images")]
    let drawing = match samples() {
        Some(samples) => drawing.with_sprite(vtium_render::Sprite::new(
            SPRITE_SIZE.0,
            SPRITE_SIZE.1,
            &PALETTE,
            samples,
        )),
        None => drawing,
    };
    drawing
}

#[cfg(test)]
#[cfg_attr(
    feature = "images",
    expect(clippy::expect_used, reason = "a test fails by panicking")
)]
mod tests {
    use super::*;

    /// The lines of `art`, each as its characters.
    fn lines(art: &str) -> Vec<Vec<char>> {
        art.lines().map(|l| l.chars().collect()).collect()
    }

    // the maintainer's decision: plain 7-bit ASCII, which shows in any
    // font, code page 437 included, the full drawing 32 columns by 20
    // rows and the small one 21 by 10
    #[test]
    fn the_drawings_are_7_bit_ascii_of_their_sizes() {
        for (art, cols, rows) in [(FULL, 32, 20), (SMALL, 21, 10)] {
            assert!(
                art.bytes()
                    .all(|b| b == b'\n' || (b' '..=b'~').contains(&b)),
                "a printable 7-bit ASCII character or a newline"
            );
            let lines = lines(art);
            assert_eq!(lines.len(), rows);
            assert_eq!(lines.iter().map(Vec::len).max(), Some(cols));
        }
    }

    // the small drawing is the monitor cropped from the full one
    #[test]
    fn the_small_drawing_is_the_monitor_cropped_from_the_full_one() {
        let (full, small) = (lines(FULL), lines(SMALL));
        let found = (0..full.len()).find_map(|top| {
            (0..32)
                .find(|&left| {
                    small.iter().enumerate().all(|(r, line)| {
                        full.get(top + r)
                            .and_then(|f| f.get(left..left + line.len()))
                            .is_some_and(|f| f == line.as_slice())
                    })
                })
                .map(|left| (top, left))
        });
        assert_eq!(found, Some((2, 6)));
    }

    // the PPM begins with the header the drawing strips, and holds 80 by
    // 80 pixels of three samples each past it
    #[test]
    fn the_sprite_is_an_80_by_80_ppm() {
        assert_eq!(samples().map(<[u8]>::len), Some(80 * 80 * 3));
    }

    // ADR-1401 D7: the panic screen carries the drawing this build shows,
    // the ASCII drawing, and in a build with image support the sprite in
    // its place
    #[test]
    fn the_panic_screen_carries_this_builds_drawing() {
        let report =
            crate::notice::panic(std::path::Path::new("/etc/vtium/vtium.conf"), Vec::new(), 0);
        let text = vtium_render::Drawing::text(FULL, SMALL);
        #[cfg(feature = "images")]
        let want = {
            let sprite =
                vtium_render::Sprite::new(80, 80, &PALETTE, samples().expect("the samples"));
            assert!(sprite.is_whole());
            assert_eq!(sprite.get(0, 0), Some(Rgb::new(0, 0, 0)));
            text.with_sprite(sprite)
        };
        #[cfg(not(feature = "images"))]
        let want = text;
        assert_eq!(report.drawing, Some(want));
    }
}
