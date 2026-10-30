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

/// A cluster as the font shapes it: each glyph and its position from the
/// cluster's origin, and the cluster's advance, in font units.
pub(crate) struct Shaped {
    key: Vec<u32>,
    kind: Kind,
    glyphs: Vec<(u16, i32, i32)>,
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
            glyphs.push((g, pen.saturating_add(pos.x_offset), pos.y_offset));
            pen = pen.saturating_add(pos.x_advance);
        }
        let kept = !glyphs.is_empty()
            && glyphs.iter().all(|&(g, _, _)| g != 0)
            && match ask.kind {
                // one glyph, a ligature or a form: none of the cluster's own
                // scalar values draws it alone
                Kind::Emoji | Kind::Joining => match glyphs.as_slice() {
                    [(g, _, _)] => ask.key.iter().all(|&c| charmap.map(c) != *g),
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
    let mut seen: BTreeMap<(bool, bool, Vec<u8>), u32> = BTreeMap::new();
    let mut out = Vec::new();
    for s in shaped {
        let wide = match s.kind {
            Kind::Emoji => true,
            Kind::Marks => s.advance as f32 * scale > cell.width as f32 * 1.5,
            // a cell for each letter: lam-alef takes the two its letters do
            Kind::Joining => s.key.iter().filter(|&&c| c != super::ZWJ).count() > 1,
        };
        let Some(placed) = place_drawn(
            &mut |ppem, fit| compose(drawer, &s.glyphs, ppem / upem.max(1.0), ppem, fit),
            wide,
            cell,
        )
        .filter(|p| p.pixels.len() <= vtium_font::SET_CHUNK_BYTES as usize) else {
            continue;
        };
        let key = (placed.wide, placed.color, placed.pixels.clone());
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
/// a color cluster takes the text's stand-in color, [`FOREGROUND`].
fn compose(
    drawer: &mut Drawer<'_>,
    glyphs: &[(u16, i32, i32)],
    scale: f32,
    ppem: f32,
    fit: (u32, u32),
) -> Option<Image> {
    let mut parts = Vec::new();
    for &(g, x, y) in glyphs {
        let image = drawer.draw(g, ppem, fit)?;
        let (dx, dy) = (
            (x as f32 * scale).round() as i32,
            (y as f32 * scale).round() as i32,
        );
        parts.push((image, dx, dy));
    }
    if let [(image, 0, 0)] = parts.as_slice() {
        return Some(image.clone());
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
