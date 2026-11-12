//! Any bytes as a font file, plain or gzipped: no panic. A font the loader
//! accepts has every glyph drawn at the scale the input's last byte picks,
//! plain, bold, italic and bold italic, and the first 0x300 codepoints and
//! the last drawn through a [`Font`] bold and bold italic, each exactly its
//! cells' size, and each sheared glyph's ink box inside its reach: its
//! cell and `REACH_CELLS` cells past it on each side, over its own row's
//! height, since a bitmap glyph's shear moves rows sideways only
//! (ADR-1302 D1, D4, D6). Each of those codepoints' advance is the cells
//! of the glyph the face draws for it, and none where it draws none.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_font::{Face, Font, Psf, REACH_CELLS, Style, Width};

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
                let across = w * i64::from(REACH_CELLS);
                let right = left + i64::from(ink.width());
                assert!(left <= 0 && left >= -across, "{left}");
                assert!(right >= w && right <= w + across, "{right}");
                assert_eq!((ink.top(), ink.height()), (0, g.height()));
                assert_eq!(ink.pixels().len(), (ink.width() * g.height()) as usize);
            }
        }
    }
    let font = Font::new(psf);
    for c in (0..0x300).filter_map(char::from_u32).chain(['\u{10ffff}']) {
        let drawn = font.face().draw(&[c], Style::default()).map(|g| g.cells());
        assert_eq!(font.face().advance(c), drawn);
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
