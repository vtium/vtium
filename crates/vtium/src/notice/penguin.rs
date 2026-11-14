//! The panic screen's drawing (ADR-1401 D7): the maintainer's drawing of
//! Tux, the Linux penguin, panicked inside a computer monitor. The Tux
//! character is Larry Ewing's, drawn with The GIMP, and COPYING credits
//! both.
//!
//! A build without image support draws the drawing in 7-bit ASCII, a
//! character to a cell: [`FULL`], 32 columns by 20 rows, or [`SMALL`], the
//! monitor cropped from it, 21 columns by 10 rows. A build with image
//! support draws the maintainer's 80×80 sprite in their place: the whole
//! sprite in the full drawing's place, and the sprite cropped to its
//! monitor, [`SPRITE_MONITOR_AT`] and [`SPRITE_MONITOR_SIZE`], in the
//! small drawing's place. vtium's
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

/// The top left pixel of the sprite's monitor, which a build with image
/// support draws in the small drawing's place, as [`SMALL`] is the monitor
/// cropped from [`FULL`] (ADR-1401 D7). The monitor is its bezel and its
/// screen, x 15 to 64 and y 6 to 49 of the sprite, inclusive. It holds no
/// pixel of the arms, the sparks, the stand or the keyboard: the arms join
/// the bezel at its outer edge, outside the crop. Its four corners outside
/// the rounded bezel are the sprite's black background.
pub const SPRITE_MONITOR_AT: (u32, u32) = (15, 6);

/// The width and height in pixels of the sprite's monitor, from
/// [`SPRITE_MONITOR_AT`].
pub const SPRITE_MONITOR_SIZE: (u32, u32) = (50, 44);

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
/// and in a build with image support the sprite in its place, cropped to
/// its monitor in the small drawing's place.
pub fn drawing() -> vtium_render::Drawing {
    let drawing = vtium_render::Drawing::text(FULL, SMALL);
    #[cfg(feature = "images")]
    let drawing = match samples() {
        Some(samples) => {
            let sprite = vtium_render::Sprite::new(SPRITE_SIZE.0, SPRITE_SIZE.1, &PALETTE, samples);
            drawing.with_sprite(sprite, monitor(sprite))
        }
        None => drawing,
    };
    drawing
}

/// `sprite` cropped to its monitor, [`SPRITE_MONITOR_AT`] and
/// [`SPRITE_MONITOR_SIZE`].
#[cfg(feature = "images")]
pub const fn monitor(sprite: vtium_render::Sprite) -> vtium_render::Sprite {
    sprite.crop(
        SPRITE_MONITOR_AT.0,
        SPRITE_MONITOR_AT.1,
        SPRITE_MONITOR_SIZE.0,
        SPRITE_MONITOR_SIZE.1,
    )
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

    // ADR-1401 D7: the crop the small drawing's place draws is the
    // sprite's monitor, its bezel and screen and nothing past them. The
    // crop's outer ring is the bezel, white, apart from its four corners,
    // which are the black background outside the rounded bezel; so a crop
    // a pixel wider, narrower, taller or shorter on any side has a ring
    // that differs. The ring just outside the crop is black, apart from
    // where the arms join the bezel, and every pixel inside the crop
    // matches a color of the palette. The ring just outside the crop is
    // black, apart from where the arms and the stand join the bezel.
    #[cfg(feature = "images")]
    #[test]
    fn the_small_places_crop_is_the_sprites_monitor() {
        let (black, white) = (Rgb::new(0, 0, 0), Rgb::new(255, 255, 255));
        let sprite = vtium_render::Sprite::new(80, 80, &PALETTE, samples().expect("the samples"));
        let crop = monitor(sprite);
        assert_eq!(crop.size(), (50, 44));
        assert!(crop.is_whole());
        let (w, h) = crop.size();
        for y in 0..h {
            for x in 0..w {
                let ring = x == 0 || y == 0 || x == w - 1 || y == h - 1;
                let corner = (x == 0 || x == w - 1) && (y == 0 || y == h - 1);
                if ring {
                    let want = if corner { black } else { white };
                    assert_eq!(crop.get(x, y), Some(want), "the crop's ring at {x},{y}");
                }
                assert_eq!(
                    crop.get(x, y),
                    sprite.get(x + 15, y + 6),
                    "the sprite's pixel at {},{}",
                    x + 15,
                    y + 6
                );
            }
        }
        // just outside the crop: the row above it is black; the row below
        // it is black but for the stand's two posts, columns 35, 36, 43
        // and 44; and the columns to either side are black but for the
        // arms, rows 30 and 31
        for x in 14..=65 {
            assert_eq!(sprite.get(x, 5), Some(black), "above, {x}");
            let post = [35, 36, 43, 44].contains(&x);
            assert_eq!(
                sprite.get(x, 50),
                Some(if post { white } else { black }),
                "below, {x}"
            );
        }
        for y in 5..=50 {
            let arm = y == 30 || y == 31;
            let want = Some(if arm { white } else { black });
            assert_eq!(sprite.get(14, y), want, "left, {y}");
            assert_eq!(sprite.get(65, y), want, "right, {y}");
        }
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
            text.with_sprite(sprite, sprite.crop(15, 6, 50, 44))
        };
        #[cfg(not(feature = "images"))]
        let want = text;
        assert_eq!(report.drawing, Some(want));
    }
}
