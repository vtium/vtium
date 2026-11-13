//! The panic screen's sprite (ADR-1401 D7). vtium's root process decodes
//! no compressed image, so the sprite is compiled in as a binary PPM, its
//! RGB samples uncompressed, and the notice draws each pixel as the entry
//! of the six-color palette it matches. This test reads the PPM's header
//! as the PPM format defines it, and checks that the picture is 80x80,
//! that every pixel is exactly one of the palette's colors, and that the
//! palette has no entry the picture does not use.

use vtium::notice::penguin::{PALETTE, PPM, PPM_HEADER, SPRITE_SIZE};

/// The PPM's width, height and largest sample, and its samples: the
/// magic `P6`, then three numbers, each after whitespace, then one
/// whitespace byte. A header comment is refused, since the drawing strips
/// a fixed header.
fn parsed(ppm: &[u8]) -> ((u32, u32, u32), &[u8]) {
    let rest = ppm.strip_prefix(b"P6").expect("the magic P6");
    let mut at = 0;
    let mut numbers = [0u32; 3];
    for n in &mut numbers {
        let spaces = rest[at..]
            .iter()
            .take_while(|b| b.is_ascii_whitespace())
            .count();
        assert!(spaces > 0, "whitespace before each number");
        at += spaces;
        assert_ne!(rest.get(at), Some(&b'#'), "no comment in the header");
        let digits = rest[at..].iter().take_while(|b| b.is_ascii_digit()).count();
        *n = std::str::from_utf8(&rest[at..at + digits])
            .expect("ASCII")
            .parse()
            .expect("a number");
        at += digits;
    }
    assert!(
        rest[at].is_ascii_whitespace(),
        "one whitespace byte after the largest sample"
    );
    ((numbers[0], numbers[1], numbers[2]), &rest[at + 1..])
}

#[test]
fn the_panic_sprite_is_an_80_by_80_ppm_in_the_six_palette_colors() {
    let ((width, height, max), samples) = parsed(PPM);
    assert_eq!((width, height), SPRITE_SIZE);
    assert_eq!(max, 255, "one byte a sample");
    assert_eq!(samples.len(), 80 * 80 * 3, "every pixel and nothing after");
    assert_eq!(
        PPM.len() - samples.len(),
        PPM_HEADER.len(),
        "the header the drawing strips"
    );
    let mut used = [0usize; 6];
    for (i, rgb) in samples.as_chunks::<3>().0.iter().enumerate() {
        let entry = PALETTE
            .iter()
            .position(|c| [c.r, c.g, c.b] == *rgb)
            .unwrap_or_else(|| {
                panic!(
                    "pixel {},{}, {rgb:?}, is no color of the palette",
                    i % 80,
                    i / 80
                )
            });
        used[entry] += 1;
    }
    for (entry, n) in used.iter().enumerate() {
        assert!(
            *n > 0,
            "palette entry {entry}, {:?}, is in no pixel",
            PALETTE[entry]
        );
    }
}
