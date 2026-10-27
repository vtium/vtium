//! Any bytes as a font file, plain or gzipped: no panic. A font the loader
//! accepts has every glyph drawn at the scale the input's last byte picks,
//! plain and bold, and the first 0x300 codepoints and the last drawn
//! through a [`Font`], each exactly its cells' size.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_font::{Face, Font, Psf, Style, Width};

fuzz_target!(|data: &[u8]| {
    let Ok(mut psf) = Psf::parse(data) else {
        return;
    };
    let height = u32::from(data.last().copied().unwrap_or(0) % 4) * psf.glyph_size().1;
    psf.set_height(height);
    let bold = Style {
        bold: true,
        italic: false,
    };
    for i in 0..psf.glyph_count() {
        for style in [Style::default(), bold] {
            let g = psf.render(i, style).expect("every glyph draws");
            assert_eq!((g.width(), g.height()), (psf.cell().width, psf.cell().height));
        }
    }
    let font = Font::new(psf);
    let cell = font.cell();
    for c in (0..0x300).filter_map(char::from_u32).chain(['\u{10ffff}']) {
        for width in [Width::Narrow, Width::Wide] {
            let g = font.glyph(&[c], width, bold).expect("every character draws");
            assert_eq!(
                (g.width(), g.height()),
                (cell.width * width.cells(), cell.height)
            );
        }
    }
});
