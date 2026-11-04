//! Any bytes as a font file, plain or gzipped: no panic. A font the loader
//! accepts has every glyph drawn at the scale the input's last byte picks,
//! plain, bold, italic and bold italic, and the first 0x300 codepoints and
//! the last drawn through a [`Font`] bold and bold italic, each exactly its
//! cells' size, and each sheared glyph's ink box inside its reach: its
//! cell and one cell past it on each side (ADR-1302 D1, D4, D6).
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_font::{Face, Font, Psf, Style, Width};

fuzz_target!(|data: &[u8]| {
    let Ok(mut psf) = Psf::parse(data) else {
        return;
    };
    let height = u32::from(data.last().copied().unwrap_or(0) % 4) * psf.glyph_size().1;
    psf.set_height(height);
    let style = |bold, italic| Style { bold, italic };
    let styles = [
        style(false, false),
        style(true, false),
        style(false, true),
        style(true, true),
    ];
    let cell = psf.cell();
    for i in 0..psf.glyph_count() {
        for style in styles {
            let g = psf.render(i, style).expect("every glyph draws");
            assert_eq!((g.width(), g.height()), (cell.width, cell.height));
            if let Some(ink) = g.ink() {
                let (w, left) = (i64::from(cell.width), i64::from(ink.left()));
                let right = left + i64::from(ink.width());
                assert!(left <= 0 && left >= -w, "{left}");
                assert!(right >= w && right <= 2 * w, "{right}");
                assert_eq!(ink.pixels().len(), (ink.width() * g.height()) as usize);
            }
        }
    }
    let font = Font::new(psf);
    for c in (0..0x300).filter_map(char::from_u32).chain(['\u{10ffff}']) {
        for width in [Width::Narrow, Width::Wide] {
            for style in [styles[1], styles[3]] {
                let g = font.glyph(&[c], width, style).expect("every character draws");
                assert_eq!(
                    (g.width(), g.height()),
                    (cell.width * width.cells(), cell.height)
                );
            }
        }
    }
});
