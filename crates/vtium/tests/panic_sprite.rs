//! The panic screen's sprite against the maintainer's PNG (ADR-1401 D7).
//! vtium's root process decodes no compressed image, so the sprite is
//! compiled in as palette indices, `art/panic-penguin-80x80-oO-indices.txt`,
//! and a palette of six colors. This test is the one place that reads the
//! PNG: it decodes the PNG, names each pixel by its palette entry, and
//! checks that the indices vtium carries are those, pixel for pixel. When
//! they differ, it writes the indices the PNG gives into cargo's scratch
//! directory for tests and names the file, so that a new PNG is carried
//! over by copying that file into `art/`.

use std::path::{Path, PathBuf};

use vtium::notice::penguin::{PALETTE, SPRITE, SPRITE_SIZE};

fn art(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("art").join(name)
}

/// The PNG's pixels as the sprite's indices: row by row, each pixel the
/// digit of its palette entry, each row ended by a newline.
fn indices_of_the_png() -> String {
    let bytes = std::fs::read(art("panic-penguin-80x80-oO.png")).expect("the PNG");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("a PNG");
    let mut rgb = vec![0; reader.output_buffer_size().expect("a frame's size")];
    let info = reader.next_frame(&mut rgb).expect("a frame");
    assert_eq!((info.width, info.height), SPRITE_SIZE);
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Rgb, png::BitDepth::Eight)
    );
    let width = usize::try_from(info.width).expect("a width");
    let mut out = String::new();
    for (y, row) in rgb.chunks_exact(width * 3).enumerate() {
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            let entry = PALETTE
                .iter()
                .position(|c| [c.r, c.g, c.b] == *p)
                .unwrap_or_else(|| panic!("pixel {x},{y}, {p:?}, is no color of the palette"));
            out.push(char::from(b'0' + u8::try_from(entry).expect("a digit")));
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_panic_sprite_is_the_maintainers_png_pixel_for_pixel() {
    let want = indices_of_the_png();
    if SPRITE != want.as_bytes() {
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("panic-penguin-80x80-oO-indices.txt");
        std::fs::write(&out, &want).expect("the indices written");
        panic!(
            "the sprite's indices are not the PNG's pixels. The indices the PNG gives are in {}: copy that file to crates/vtium/art/panic-penguin-80x80-oO-indices.txt",
            out.display()
        );
    }
    // every color of the palette is in the picture
    for digit in b'0'..b'6' {
        assert!(SPRITE.contains(&digit), "palette entry {}", digit - b'0');
    }
}
