//! The placement of a bitmap font's glyphs in the cells of a text band:
//! scaled, styled, and made small for a superscript or a subscript.

use vtium_font::script::Rect;
use vtium_font::spacing::{Band, InkRows};
use vtium_font::{Cell, Glyph, Origin, REACH_CELLS, Script, Style, Width};

use crate::font::Font;

#[cfg(doc)]
use super::engine::Engine;

fn bit(bits: &[u8], row: usize, x: u32, y: u32) -> bool {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    bits.get(at).is_some_and(|b| b & (0x80 >> (x % 8)) != 0)
}

fn set_bit(bits: &mut [u8], row: usize, x: u32, y: u32) {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    if let Some(b) = bits.get_mut(at) {
        *b |= 0x80 >> (x % 8);
    }
}

/// The cell's row where the top row of a bitmap glyph scaled by `scale`
/// goes, so that the font's baseline `own`,
/// counted in the font's rows, meets the cell's baseline `base` (ADR-1200
/// D17). The first source's glyphs then fill the cell down to its bottom
/// row, and a later source's glyphs stand on the first source's baseline;
/// the ink a later source's glyph then has above or below the cell goes to
/// its ink box ([`place_styled`]).
pub(super) fn glyph_top(scale: u32, own: u32, base: u32) -> i64 {
    i64::from(base) - i64::from(own) * i64::from(scale)
}

/// A bitmap glyph placed in the cells of the text band, as coverage, and
/// its ink box where its ink passes them (ADR-1302 D6, ADR-1306 D1).
type Drawn = (Vec<u8>, Option<InkRows>);

/// A 1-bit glyph `width` cells wide of a font of cell `from`, drawn in the
/// style `synth` asks (ADR-1302 D4), scaled by `scale` into `cell`, the
/// text band's cell, centred across, its top row at row `top` of the cell
/// ([`glyph_top`]), as coverage of 0 or 255. Bold widens each row, a pixel
/// set when it or its left neighbour is; italic then shears the rows about
/// `baseline` at the font's own size, before the scale, by vtium-font's
/// rule ([`vtium_font::italic::shear_shift`]), so the console's own bitmap
/// faces and the helper draw the same glyph. A glyph whose ink passes its
/// cells, a sheared one beside them or a later source's raised onto the
/// first source's baseline above or below them, carries its ink box: its
/// cells and its ink together, cut to its reach, [`REACH_CELLS`] cells past
/// its cells on each side and `reach` rows above and below the text band.
pub(super) fn place_styled(
    glyph: &[u8],
    width: Width,
    from: Cell,
    (cell, top): (Cell, i64),
    scale: u32,
    synth: Style,
    (baseline, reach): (u32, (u32, u32)),
) -> Drawn {
    let cells = width.cells();
    let src_w = from.width * cells;
    let src_row = src_w.div_ceil(8) as usize;
    let lit = |x: u32, y: u32| {
        bit(glyph, src_row, x, y) || (synth.bold && x > 0 && bit(glyph, src_row, x - 1, y))
    };
    let mask: Vec<u8> = (0..from.height)
        .flat_map(|y| (0..src_w).map(move |x| if lit(x, y) { 255 } else { 0 }))
        .collect();
    let sheared = synth
        .italic
        .then(|| vtium_font::italic::shear(&mask, src_w, from.height, baseline))
        .flatten();
    // the drawn glyph at the font's own size: its pixels, their width and
    // the column of the font's cell its first one lies at
    let (pixels, stride, shift) = match &sheared {
        Some(s) => (&s.pixels, s.width as usize, i64::from(s.left)),
        None => (&mask, src_w as usize, 0),
    };
    let dst_w = cell.width * cells;
    let x0 = i64::from((dst_w - src_w * scale) / 2);
    let scale = i64::from(scale);
    // the glyph's pixel under column `x` from the cells' left edge and row
    // `y` from the text band's top
    let at = |x: i64, y: i64| -> bool {
        let (Ok(sy), Ok(sx)) = (
            usize::try_from((y - top).div_euclid(scale)),
            usize::try_from((x - x0).div_euclid(scale) - shift),
        ) else {
            return false;
        };
        sy < from.height as usize
            && sx < stride
            && pixels.get(sy * stride + sx).is_some_and(|&p| p != 0)
    };
    let rect = |(x0, x1): (i64, i64), (y0, y1): (i64, i64)| -> Vec<u8> {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| if at(x, y) { 255 } else { 0 }))
            .collect()
    };
    let (span, height) = (i64::from(dst_w), i64::from(cell.height));
    let own = rect((0, span), (0, height));
    let across = i64::from(cell.width) * i64::from(REACH_CELLS);
    let (columns, rows) = (
        -across..span + across,
        -i64::from(reach.0)..height + i64::from(reach.1),
    );
    let inked_column = |x: i64| rows.clone().any(|y| at(x, y));
    let inked_row = |y: i64| columns.clone().any(|x| at(x, y));
    let ink = (|| {
        let (first, last) = (
            columns.clone().find(|&x| inked_column(x))?,
            columns.clone().rev().find(|&x| inked_column(x))?,
        );
        let (highest, lowest) = (
            rows.clone().find(|&y| inked_row(y))?,
            rows.clone().rev().find(|&y| inked_row(y))?,
        );
        let (left, right) = (first.min(0), (last + 1).max(span));
        let (up, down) = (highest.min(0), (lowest + 1).max(height));
        if (left, right, up, down) == (0, span, 0, height) {
            return None;
        }
        Some(InkRows {
            left: i32::try_from(left).ok()?,
            top: i32::try_from(up).ok()?,
            width: u32::try_from(right - left).ok()?,
            height: u32::try_from(down - up).ok()?,
            pixels: rect((left, right), (up, down)),
        })
    })();
    (own, ink)
}

/// What a bitmap glyph drawn in the cells of a text band `span` by
/// `height` pixels is made from (ADR-1202 D1, D4, D7): a 1-bit source,
/// whose own box is its font's cell `font`, `cells` cells wide, drawn at
/// `scale`, centred across with its top row at `top` as [`place_styled`]
/// places it and cut to the band; sheared about the row under the font's
/// baseline `baseline`; and with `synth` left to the small glyph.
pub(super) fn bitmap_origin(
    (font, cells, scale): (Cell, Width, u32),
    (span, height): (u32, u32),
    (top, baseline): (i64, u32),
    synth: Style,
) -> Origin {
    let w = font.width * cells.cells() * scale;
    let h = i64::from(font.height * scale);
    let row = |y: i64| u32::try_from(y.clamp(0, i64::from(height))).unwrap_or(0);
    let (first, last) = (row(top), row(top + h));
    Origin {
        own: Some(Rect {
            left: span.saturating_sub(w) / 2,
            top: first,
            width: w.min(span),
            height: last.saturating_sub(first),
        }),
        ..Origin::bitmap(row(top + i64::from(baseline * scale)), synth)
    }
}

/// A bitmap glyph's coverage `own`, drawn upright and unstyled in the
/// cells of a text band `size` pixels across and down and `width` cells
/// wide, as `script` draws it from `origin`: made small, styled and placed
/// by vtium-font's script rule ([`Glyph::scripted`], ADR-1202 D1, D4, D7),
/// the one the console's own bitmap fonts take. `None` for coverage that
/// is not the size's.
pub(super) fn script_bitmap(
    own: &[u8],
    size: (u32, u32),
    width: Width,
    (script, origin): (Script, &Origin),
) -> Option<Vec<u8>> {
    let glyph = Glyph::from_pixels(size.0, size.1, vtium_font::Format::Mask, own.to_vec())?
        .with_cells(width);
    Some(
        glyph
            .scripted(script, origin, Band::NONE)?
            .pixels()
            .to_vec(),
    )
}

/// Coverage of 0 or 255, `width` pixels across, as 1-bit rows.
pub(super) fn pack_bits(coverage: &[u8], width: u32) -> Vec<u8> {
    let w = width as usize;
    let row = w.div_ceil(8);
    let mut out = vec![0u8; coverage.len().checked_div(w).unwrap_or(0) * row];
    for (y, line) in coverage.chunks(w.max(1)).enumerate() {
        for (x, &p) in line.iter().enumerate() {
            if p != 0 {
                set_bit(&mut out, row, x as u32, y as u32);
            }
        }
    }
    out
}

/// The row under the baseline `font` is sheared about for italic, from its
/// cell's top: the baseline the font states, as a built-in set's strike
/// and a BDF font's ascent do, else the row under the lowest ink of its
/// `H`, else its cell's bottom edge ([`vtium_font::italic::bitmap_baseline`]).
pub(super) fn bitmap_baseline(font: &Font) -> u32 {
    if let Some(b) = font.baseline {
        return b.min(font.cell.height);
    }
    let h = font
        .map
        .get(&u32::from('H'))
        .and_then(|&g| font.glyphs.get(g as usize))
        .map(|(width, rows)| {
            let w = font.cell.width * width.cells();
            let row = w.div_ceil(8) as usize;
            (0..font.cell.height)
                .map(|y| (0..w).any(|x| bit(rows, row, x, y)))
                .collect::<Vec<bool>>()
        });
    vtium_font::italic::bitmap_baseline(h.as_deref(), font.cell.height)
}

/// A glyph from a smaller size of a bitmap source (ADR-1202 D3), as
/// [`Engine::smaller_glyph`] finds it.
#[derive(Clone, Copy)]
pub(super) struct SmallerGlyph<'a> {
    /// The strike's place: 0 for the source's own, else its smaller
    /// size's place in [`Engine`]'s list, from 1.
    pub(super) strike: usize,
    /// The glyph's number in the strike's font.
    pub(super) glyph: u32,
    /// The whole multiple the strike is drawn at.
    pub(super) multiple: u32,
    pub(super) font: &'a Font,
    pub(super) rows: &'a [u8],
}
