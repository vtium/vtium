//! The child's side of shaping: HarfRust on a font no one has checked. A
//! cluster is shaped once, in font units, then drawn at each cell: each
//! glyph the shaper gives drawn by swash at its position, the glyphs laid
//! over each other into one image, placed in its cells as one glyph is.
//! A panic here ends the child, which fails the source and nothing else
//! (ADR-1200 D14).

use std::collections::BTreeMap;

use harfrust::{BufferFlags, ShapeOptions, ShaperData, UnicodeBuffer};
use swash::FontRef;
use swash::scale::Source;
use swash::scale::image::{Content, Image};
use swash::zeno::Placement;

use super::{Ask, Kind};
use crate::outline::Placed;
use crate::outline::raster::{Cell, Drawer, FOREGROUND, div255, pixel, place_drawn};

/// A cluster as the font shapes it: each glyph, its position from the
/// cluster's origin and whether it is a mark, and the cluster's advance,
/// in font units. A mark is a glyph after the cluster's first that the
/// shaper gives no advance, as HarfRust gives each mark it places on its
/// base.
pub(crate) struct Shaped {
    key: Vec<u32>,
    kind: Kind,
    glyphs: Vec<(u16, i32, i32, bool)>,
    advance: i32,
}

/// Each cluster of `asks` the font in `data`, face `index`, shapes as its
/// kind requires (see [`Kind`]). A font HarfRust cannot read shapes none.
pub(crate) fn shape_all(data: &[u8], index: u32, font: FontRef<'_>, asks: &[Ask]) -> Vec<Shaped> {
    if asks.is_empty() {
        return Vec::new();
    }
    let Ok(face) = harfrust::FontRef::from_index(data, index) else {
        return Vec::new();
    };
    let shaper_data = ShaperData::new(&face);
    let shaper = shaper_data.shaper(&face).build();
    let charmap = font.charmap();
    let mut out = Vec::new();
    for ask in asks {
        let mut buffer = UnicodeBuffer::new();
        for &c in &ask.key {
            let Some(ch) = char::from_u32(c) else {
                continue;
            };
            buffer.add(ch, 0);
        }
        buffer.set_flags(BufferFlags::REMOVE_DEFAULT_IGNORABLES);
        buffer.guess_segment_properties();
        let shaped = shaper.shape(buffer, ShapeOptions::new());
        let mut pen = 0i32;
        let mut glyphs = Vec::new();
        for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            let Ok(g) = u16::try_from(info.glyph_id) else {
                continue;
            };
            let mark = !glyphs.is_empty() && pos.x_advance == 0;
            glyphs.push((g, pen.saturating_add(pos.x_offset), pos.y_offset, mark));
            pen = pen.saturating_add(pos.x_advance);
        }
        let kept = !glyphs.is_empty()
            && glyphs.iter().all(|&(g, ..)| g != 0)
            && match ask.kind {
                // one glyph, a ligature or a form: none of the cluster's own
                // scalar values draws it alone
                Kind::Emoji | Kind::Joining => match glyphs.as_slice() {
                    [(g, ..)] => ask.key.iter().all(|&c| charmap.map(c) != *g),
                    _ => false,
                },
                Kind::Marks => true,
            };
        if kept {
            out.push(Shaped {
                key: ask.key.clone(),
                kind: ask.kind,
                glyphs,
                advance: pen,
            });
        }
    }
    out
}

/// Draws each shaped cluster at `cell`, adding its glyph to `glyphs`, one
/// glyph for clusters drawn alike: each cluster and its glyph, sorted by
/// cluster. A cluster the font draws nothing for is left out.
pub(crate) fn draw_all(
    drawer: &mut Drawer<'_>,
    shaped: &[Shaped],
    cell: &Cell,
    glyphs: &mut Vec<Placed>,
) -> Result<Vec<(Vec<u32>, u32)>, String> {
    let upem = f32::from(drawer.font.metrics(&[]).units_per_em);
    let scale = cell.ppem / upem.max(1.0);
    // the row's top and bottom edges, in pixels up from the baseline
    let row = (
        i32::try_from(cell.baseline).unwrap_or(0),
        i32::try_from(cell.baseline).unwrap_or(0) - i32::try_from(cell.height).unwrap_or(0),
    );
    let mut seen: BTreeMap<Placed, u32> = BTreeMap::new();
    let mut out = Vec::new();
    for s in shaped {
        let wide = match s.kind {
            Kind::Emoji => true,
            Kind::Marks => s.advance as f32 * scale > cell.width as f32 * 1.5,
            // a cell for each letter: lam-alef takes the two its letters do
            Kind::Joining => s.key.iter().filter(|&&c| c != super::ZWJ).count() > 1,
        };
        // ADR-1302 D2: a cluster is placed by its advance; a joining letter,
        // with the marks on it, is stretched as one to its cells, its pen on
        // the cell's left edge; and a ligature of joining letters, such as
        // lam-alef, which shaping places, keeps its pen there unstretched
        let letters = s
            .key
            .iter()
            .filter(|&&c| c != super::ZWJ && super::joins(c))
            .count();
        let advance_px = s.advance as f32 * scale;
        let span = cell.width as f32 * if wide { 2.0 } else { 1.0 };
        let (advance, sx) = match (s.kind, letters) {
            (Kind::Emoji, _) | (_, 0) => (Some(advance_px), 1.0),
            (_, 1) if advance_px > 0.0 => (None, span / advance_px),
            _ => (None, 1.0),
        };
        let Some(placed) = place_drawn(
            &mut |ppem, fit| {
                compose(
                    drawer,
                    &s.glyphs,
                    ppem / upem.max(1.0),
                    ppem,
                    fit,
                    Some(row),
                    sx,
                )
            },
            wide,
            advance,
            cell,
        )
        .filter(|p| p.pixels.len() <= vtium_font::SET_CHUNK_BYTES as usize) else {
            continue;
        };
        let key = placed.clone();
        let g = match seen.get(&key) {
            Some(&g) => g,
            None => {
                let g = u32::try_from(glyphs.len()).map_err(|_| "too many glyphs")?;
                glyphs.push(placed);
                seen.insert(key, g);
                g
            }
        };
        out.push((s.key.clone(), g));
    }
    out.sort();
    Ok(out)
}

/// The glyphs of a shaped cluster drawn at `ppem` and laid over each other
/// at their positions, `scale` pixels to a font unit: one image, color when
/// any glyph is, its placement from the cluster's origin. A mask glyph in
/// a color cluster takes the text's stand-in color, [`FOREGROUND`]. With
/// `row`, its top and bottom edges in pixels up from the baseline, the
/// marks of a mask cluster are fitted to the row first ([`fit_marks`]).
/// The whole cluster is stretched sideways by `sx`, each outline and each
/// offset, so a mark stays over its base.
fn compose(
    drawer: &mut Drawer<'_>,
    glyphs: &[(u16, i32, i32, bool)],
    scale: f32,
    ppem: f32,
    fit: (u32, u32),
    row: Option<(i32, i32)>,
    sx: f32,
) -> Option<Image> {
    let mut parts = Vec::new();
    let mut marks = Vec::new();
    for &(g, x, y, mark) in glyphs {
        let image = drawer.draw_stretched(g, ppem, fit, sx)?;
        let (dx, dy) = (
            (x as f32 * scale * sx).round() as i32,
            (y as f32 * scale).round() as i32,
        );
        parts.push((image, dx, dy));
        marks.push(mark);
    }
    if let [(image, 0, 0)] = parts.as_slice() {
        return Some(image.clone());
    }
    if let Some(row) = row
        && parts.iter().all(|(i, ..)| i.content != Content::Color)
    {
        fit_marks(&mut parts, &marks, row);
    }
    let color = parts.iter().any(|(i, _, _)| i.content == Content::Color);
    let edges = |(i, dx, dy): &(Image, i32, i32)| {
        let left = dx + i.placement.left;
        let top = dy + i.placement.top;
        (
            left,
            top,
            left + i.placement.width as i32,
            top - i.placement.height as i32,
        )
    };
    let (left, top, right, bottom) = parts.iter().map(edges).fold(
        (i32::MAX, i32::MIN, i32::MIN, i32::MAX),
        |(l, t, r, b), (l2, t2, r2, b2)| (l.min(l2), t.max(t2), r.max(r2), b.min(b2)),
    );
    let (w, h) = ((right - left).max(0) as u32, (top - bottom).max(0) as u32);
    let bpp = if color { 4 } else { 1 };
    let mut data = vec![0u8; (w * h) as usize * bpp];
    for part in &parts {
        let (l, t, _, _) = edges(part);
        let image = &part.0;
        let own = image.content == Content::Color;
        for y in 0..image.placement.height {
            for x in 0..image.placement.width {
                let tx = (l - left) as u32 + x;
                let ty = (top - t) as u32 + y;
                let at = (ty * w + tx) as usize * bpp;
                let Some(dst) = data.get_mut(at..at + bpp) else {
                    continue;
                };
                let p = pixel(image, x, y, own);
                if !color {
                    if let Some(d) = dst.first_mut() {
                        *d = (*d).max(p[0]);
                    }
                    continue;
                }
                let src = if own {
                    p
                } else {
                    let a = u32::from(p[0]);
                    FOREGROUND.map(|c| div255(u32::from(c) * a) as u8)
                };
                let alpha = u32::from(src[3]);
                for (d, s) in dst.iter_mut().zip(src) {
                    *d = (u32::from(s) + div255(u32::from(*d) * (255 - alpha))) as u8;
                }
            }
        }
    }
    Some(Image {
        source: Source::Outline,
        content: if color { Content::Color } else { Content::Mask },
        placement: Placement {
            left,
            top,
            width: w,
            height: h,
        },
        data,
    })
}

/// A glyph's ink as placed in a cluster: its top and bottom edges, in
/// pixels up from the baseline.
fn span((image, _, dy): &(Image, i32, i32)) -> (i32, i32) {
    let top = dy + image.placement.top;
    (top, top - image.placement.height as i32)
}

/// The mark rule (ADR-1302 D3, the maintainer, 2026-11-01): the marks of a
/// cluster whose ink passes the row's top or bottom edge, `row` in pixels
/// up from the baseline, move toward the baseline until they fit, and a
/// mark or a stack of marks that still cannot fit is squeezed vertically
/// toward its base letter until it does. Marks over the base's middle are
/// one stack and those under it another; a stack moves as one, no closer
/// to its base letter than the letter's ink, and squeezes about its edge
/// nearest the letter. Base letters never move, and a mark's sideways
/// place stays where the shaper put it.
fn fit_marks(parts: &mut [(Image, i32, i32)], marks: &[bool], row: (i32, i32)) {
    let (row_top, row_bottom) = row;
    let base: Vec<(i32, i32)> = parts
        .iter()
        .zip(marks)
        .filter(|&(p, &m)| !m && p.0.placement.height > 0)
        .map(|(p, _)| span(p))
        .collect();
    let base_top = base.iter().map(|s| s.0).max().unwrap_or(0);
    let base_bottom = base.iter().map(|s| s.1).min().unwrap_or(0);
    let middle = (base_top + base_bottom) / 2;
    for above in [true, false] {
        let stack: Vec<usize> = (0..parts.len())
            .filter(|&i| marks.get(i) == Some(&true))
            .filter(|&i| parts.get(i).is_some_and(|p| p.0.placement.height > 0))
            .filter(|&i| {
                let (t, b) = parts.get(i).map(span).unwrap_or_default();
                (t + b >= 2 * middle) == above
            })
            .collect();
        let edges = |parts: &[(Image, i32, i32)]| {
            stack
                .iter()
                .filter_map(|&i| parts.get(i).map(span))
                .fold((i32::MIN, i32::MAX), |(t, b), (t2, b2)| {
                    (t.max(t2), b.min(b2))
                })
        };
        let (top, bottom) = edges(parts);
        if stack.is_empty() {
            continue;
        }
        // the move toward the baseline, no further than the base's ink
        let shift = if above {
            (top - row_top).min(bottom - base_top).max(0)
        } else {
            (row_bottom - bottom).min(base_bottom - top).max(0)
        };
        for &i in &stack {
            if let Some(p) = parts.get_mut(i) {
                p.2 += if above { -shift } else { shift };
            }
        }
        let (top, bottom) = edges(parts);
        let (anchor, far, limit) = if above {
            (bottom.min(row_top - 1), top, row_top)
        } else {
            (top.max(row_bottom + 1), bottom, row_bottom)
        };
        if (above && far <= limit) || (!above && far >= limit) {
            continue;
        }
        let f = (limit - anchor) as f32 / (far - anchor) as f32;
        for &i in &stack {
            if let Some(p) = parts.get_mut(i) {
                squeeze(p, anchor, f);
            }
        }
    }
}

/// `part`'s mask squeezed vertically by `f` about the row `anchor`, in
/// pixels up from the baseline: each row of the result the average of the
/// rows it covers.
fn squeeze(part: &mut (Image, i32, i32), anchor: i32, f: f32) {
    let (image, _, dy) = part;
    let (w, h) = (
        image.placement.width as usize,
        image.placement.height as i32,
    );
    let top = *dy + image.placement.top;
    let map = |y: i32| anchor as f32 + (y - anchor) as f32 * f;
    let (new_top, new_bottom) = (map(top), map(top - h));
    let (hi, lo) = (
        new_top.max(new_bottom).ceil() as i32,
        new_top.min(new_bottom).floor() as i32,
    );
    let rows = (hi - lo).max(0) as usize;
    let mut data = vec![0u8; rows * w];
    for r in 0..rows {
        // the destination row's span, then the source's, in y up
        let (d_hi, d_lo) = ((hi - r as i32) as f32, (hi - r as i32 - 1) as f32);
        let back = |y: f32| anchor as f32 + (y - anchor as f32) / f;
        let (s_hi, s_lo) = (back(d_hi).max(back(d_lo)), back(d_hi).min(back(d_lo)));
        let span = (s_hi - s_lo).max(f32::EPSILON);
        for x in 0..w {
            let mut sum = 0.0f32;
            for sy in 0..h {
                let (row_hi, row_lo) = ((top - sy) as f32, (top - sy - 1) as f32);
                let overlap = s_hi.min(row_hi) - s_lo.max(row_lo);
                if overlap > 0.0 {
                    let p = image.data.get(sy as usize * w + x).copied().unwrap_or(0);
                    sum += f32::from(p) * overlap;
                }
            }
            if let Some(d) = data.get_mut(r * w + x) {
                *d = (sum / span).round().min(255.0) as u8;
            }
        }
    }
    image.data = data;
    image.placement.top = hi - *dy;
    image.placement.height = rows as u32;
}
