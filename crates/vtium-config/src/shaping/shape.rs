//! The child's side of shaping: HarfRust on a font no one has checked. A
//! cluster is shaped once, in font units, then drawn at each cell: each
//! glyph the shaper gives drawn by swash at its position, the glyphs laid
//! over each other into one image, placed in its cells as one glyph is.
//! A panic here ends the child, which fails the source and nothing else
//! (ADR-1200 D14).

use std::collections::BTreeMap;

use harfrust::{
    BufferClusterLevel, BufferFlags, Direction, ShapeOptions, ShaperData, UnicodeBuffer,
};
use swash::FontRef;
use swash::scale::Source;
use swash::scale::image::{Content, Image};
use swash::zeno::Placement;

use super::{Ask, Kind};
use crate::outline::Placed;
use crate::outline::raster::{Cell, Drawer, FOREGROUND, div255, pixel, place_drawn};

/// One glyph of a shaped cluster: its glyph, its position from the
/// cluster's origin in font units, the index in the cluster of the
/// character it comes from, its place among the cluster's glyphs in the
/// order the text is written (HarfRust gives the glyphs in visual order,
/// which is that order reversed for right-to-left text), whether its
/// character is a mark and on which side of its base the mark's combining
/// class puts it ([`super::mark_side`]: 1 above, 2 below, else 0), and
/// whether the shaper placed it, with an advance or an offset.
#[derive(Clone, Copy, Debug)]
pub(crate) struct At {
    g: u16,
    x: i32,
    y: i32,
    from: u32,
    logical: usize,
    mark: Option<u8>,
    placed: bool,
}

/// A cluster as the font shapes it: each glyph and the cluster's advance,
/// in font units. A glyph is a mark by its own character, which its
/// cluster index names, so a mark is found wherever HarfRust puts it:
/// HarfRust gives a right-to-left cluster in visual order, its marks
/// before their letter.
pub(crate) struct Shaped {
    key: Vec<u32>,
    kind: Kind,
    glyphs: Vec<At>,
    advance: i32,
    /// An emoji presentation sequence drawn with its character's own
    /// glyph, kept only when that glyph is a color one.
    own: bool,
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
    // the letter glyphs of each part of a joining cluster shaped on its
    // own, without U+200D, as a joining form is judged
    let mut alone: BTreeMap<Vec<u32>, Vec<u16>> = BTreeMap::new();
    let mut out = Vec::new();
    for ask in asks {
        let (glyphs, advance) = shape(&shaper, &ask.key);
        // a character and U+FE0F, which a color font draws with the
        // character's own glyph (ADR-1200 D21): kept when that glyph is a
        // color one, which only drawing it tells
        let own = ask.kind == Kind::Emoji && presented(&glyphs, &ask.key, &charmap);
        let kept = own
            || !glyphs.is_empty()
                && glyphs.iter().all(|a| a.g != 0)
                && match ask.kind {
                    // one glyph, a ligature or a form, which none of the
                    // cluster's own scalar values draws alone
                    Kind::Emoji => only_own(&glyphs, &ask.key, &charmap),
                    Kind::Joining => {
                        let mut alone = |part: &[u32]| {
                            alone
                                .entry(part.to_vec())
                                .or_insert_with(|| letters(&shape(&shaper, part).0))
                                .clone()
                        };
                        joined(&glyphs, &ask.key, &mut alone)
                    }
                    Kind::Marks => true,
                };
        if kept {
            out.push(Shaped {
                key: ask.key.clone(),
                kind: ask.kind,
                glyphs,
                advance,
                own,
            });
        }
    }
    out
}

/// `key` as `shaper` shapes it: each glyph, in visual order, and the
/// cluster's advance, in font units.
fn shape(shaper: &harfrust::Shaper<'_>, key: &[u32]) -> (Vec<At>, i32) {
    let mut buffer = UnicodeBuffer::new();
    // each glyph's cluster is the index of its character in the key
    buffer.set_cluster_level(BufferClusterLevel::Characters);
    for (i, &c) in key.iter().enumerate() {
        let Some(ch) = char::from_u32(c) else {
            continue;
        };
        buffer.add(ch, u32::try_from(i).unwrap_or(u32::MAX));
    }
    buffer.set_flags(BufferFlags::REMOVE_DEFAULT_IGNORABLES);
    buffer.guess_segment_properties();
    let backward = matches!(
        buffer.direction(),
        Direction::RightToLeft | Direction::BottomToTop
    );
    let shaped = shaper.shape(buffer, ShapeOptions::new());
    let n = shaped.len();
    let mut pen = 0i32;
    let mut glyphs = Vec::new();
    for (i, (info, pos)) in shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .enumerate()
    {
        let Ok(g) = u16::try_from(info.glyph_id) else {
            continue;
        };
        let mark = usize::try_from(info.cluster)
            .ok()
            .and_then(|i| key.get(i))
            .and_then(|&c| super::mark_side(c));
        glyphs.push(At {
            g,
            x: pen.saturating_add(pos.x_offset),
            y: pos.y_offset,
            from: info.cluster,
            logical: if backward { n - 1 - i } else { i },
            mark,
            placed: pos.x_advance != 0 || pos.x_offset != 0 || pos.y_offset != 0,
        });
        pen = pen.saturating_add(pos.x_advance);
    }
    (glyphs, pen)
}

/// The glyphs of `glyphs` that come from no mark.
fn letters(glyphs: &[At]) -> Vec<u16> {
    glyphs
        .iter()
        .filter(|a| a.mark.is_none())
        .map(|a| a.g)
        .collect()
}

/// Whether `key` is a character and U+FE0F, an emoji presentation
/// sequence, and `glyphs` the character's own glyph alone.
fn presented(glyphs: &[At], key: &[u32], charmap: &swash::Charmap<'_>) -> bool {
    match (key, glyphs) {
        ([c, super::VS16], [a]) => a.g != 0 && charmap.map(*c) == a.g,
        _ => false,
    }
}

/// Whether `glyphs` is one glyph that none of `key`'s scalar values draws
/// alone in `charmap`: a ligature, or a form the font shaped.
fn only_own(glyphs: &[At], key: &[u32], charmap: &swash::Charmap<'_>) -> bool {
    match glyphs {
        [a] => key.iter().all(|&c| charmap.map(c) != a.g),
        _ => false,
    }
}

/// Whether the joining cluster `key`, shaped to `glyphs`, is a form the
/// font shaped. Its glyphs that come from no mark are its letter, and they
/// must all come from one character, or from one ligature, which HarfRust
/// numbers by its first character: a font may draw a letter as several
/// glyphs, as Noto Naskh Arabic draws beh's joined forms as a dotless body
/// and a separate dot. The letter's glyphs must also differ from what the
/// cluster draws with nothing joined: each of its letters with the marks
/// after it, shaped on its own without U+200D, as `alone` shapes it. So a
/// letter keyed with U+200D on a side it does not join is left out, though
/// HarfRust may compose it with its mark into another glyph, as alef and
/// hamza above become alef with hamza above, and lam-alef, one glyph where
/// lam and alef alone draw two, is kept. The glyphs of the marks on the
/// letter stay beside it.
fn joined(glyphs: &[At], key: &[u32], alone: &mut impl FnMut(&[u32]) -> Vec<u16>) -> bool {
    let mut from = glyphs.iter().filter(|a| a.mark.is_none()).map(|a| a.from);
    let Some(first) = from.next() else {
        return false;
    };
    if !from.all(|f| f == first) {
        return false;
    }
    // each letter and the marks after it, U+200D left out
    let mut parts: Vec<Vec<u32>> = Vec::new();
    for &c in key.iter().filter(|&&c| c != super::ZWJ) {
        match parts.last_mut() {
            Some(part) if super::mark_side(c).is_some() => part.push(c),
            _ => parts.push(vec![c]),
        }
    }
    let mut unjoined: Vec<u16> = parts.iter().flat_map(|part| alone(part)).collect();
    let mut ids = letters(glyphs);
    unjoined.sort_unstable();
    ids.sort_unstable();
    ids != unjoined
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
            // the cells the terminal gives the cluster (ADR-1200 D21)
            Kind::Emoji | Kind::Marks => super::wide(&s.key),
            // a cell for each letter, the marks on it taking none: lam-alef
            // takes the two its letters do
            Kind::Joining => {
                s.key
                    .iter()
                    .filter(|&&c| c != super::ZWJ && super::mark_side(c).is_none())
                    .count()
                    > 1
            }
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
            // a cluster's marks keep the row by the mark rule (ADR-1302 D3)
            false,
            cell,
        ) else {
            continue;
        };
        // a text font's own glyph for the character is the character's, which
        // the console draws for the cluster from its store (ADR-1200 D22,
        // step 3), from the first source in the frit's order that has it
        if s.own && !placed.color {
            continue;
        }
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
/// at their positions ([`draw_parts`]): one image, color when any glyph
/// is, its placement from the cluster's origin. A mask glyph in a color
/// cluster takes the text's stand-in color, [`FOREGROUND`].
fn compose(
    drawer: &mut Drawer<'_>,
    glyphs: &[At],
    scale: f32,
    ppem: f32,
    fit: (u32, u32),
    row: Option<(i32, i32)>,
    sx: f32,
) -> Option<Image> {
    let parts = draw_parts(drawer, glyphs, scale, ppem, fit, row, sx)?;
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

/// Each glyph of a shaped cluster drawn at `ppem`, with its position from
/// the cluster's origin in pixels, `scale` pixels to a font unit. The
/// whole cluster is stretched sideways by `sx`, each outline and each
/// offset, so a mark stays over its base. With `row`, its top and bottom
/// edges in pixels up from the baseline, the marks of a mask cluster of
/// more than one glyph are placed and fitted to the row ([`place_unplaced`],
/// [`fit_marks`]). `None` when a glyph cannot be drawn.
fn draw_parts(
    drawer: &mut Drawer<'_>,
    glyphs: &[At],
    scale: f32,
    ppem: f32,
    fit: (u32, u32),
    row: Option<(i32, i32)>,
    sx: f32,
) -> Option<Vec<(Image, i32, i32)>> {
    let mut parts = Vec::new();
    for &At { g, x, y, .. } in glyphs {
        let image = drawer.draw_stretched(g, ppem, fit, sx)?;
        let (dx, dy) = (
            (x as f32 * scale * sx).round() as i32,
            (y as f32 * scale).round() as i32,
        );
        parts.push((image, dx, dy));
    }
    let marks: Vec<bool> = glyphs.iter().map(|a| a.mark.is_some()).collect();
    if let [(_, 0, 0)] = parts.as_slice() {
        return Some(parts);
    }
    if let Some(row) = row
        && parts.iter().all(|(i, ..)| i.content != Content::Color)
    {
        place_unplaced(&mut parts, glyphs);
        fit_marks(&mut parts, &marks, row);
    }
    Some(parts)
}

/// A glyph's ink as placed in a cluster: its top and bottom edges, in
/// pixels up from the baseline.
fn span((image, _, dy): &(Image, i32, i32)) -> (i32, i32) {
    let top = dy + image.placement.top;
    (top, top - image.placement.height as i32)
}

/// Places each mark above or below its base that the shaper left at the
/// pen, with no advance and no offset: centred over the ink of its base,
/// a row above all the ink since that base for a mark above, or directly
/// under it for a mark below. A mark's base is the
/// letter before it in the order the text is written, as Unicode's
/// combining character sequence and OpenType's mark-to-base attachment
/// take it, so the glyphs are walked in that order ([`At::logical`]):
/// in right-to-left text the base lies right of its marks in the glyphs
/// HarfRust gives. The glyphs one letter is drawn with, such as a body and
/// its dot, are one base. Without this such a mark, a zero-width glyph
/// drawn at the pen, would land past its base, in the next cell, or on the
/// left edge of a right-to-left letter.
fn place_unplaced(parts: &mut [(Image, i32, i32)], glyphs: &[At]) {
    let mut order: Vec<usize> = (0..parts.len().min(glyphs.len())).collect();
    order.sort_by_key(|&i| glyphs.get(i).map(|a| a.logical));
    // the character the base comes from, its ink columns, and the ink's
    // top and bottom edges since it
    let mut base: Option<(u32, i32, i32)> = None;
    let (mut top, mut bottom) = (0, 0);
    for i in order {
        let (Some(a), Some(part)) = (glyphs.get(i), parts.get_mut(i)) else {
            continue;
        };
        let (w, h) = (
            part.0.placement.width as i32,
            part.0.placement.height as i32,
        );
        let unplaced = a.mark.filter(|&side| !a.placed && side != 0);
        if let (Some(side), Some((_, x0, x1))) = (unplaced, base) {
            part.1 = (x0 + x1 - w).div_euclid(2) - part.0.placement.left;
            let new_top = if side == 1 { top + 1 + h } else { bottom };
            part.2 = new_top - part.0.placement.top;
        }
        if w == 0 || h == 0 {
            continue;
        }
        let (t, b) = span(part);
        let left = part.1 + part.0.placement.left;
        match base {
            _ if a.mark.is_some() => (top, bottom) = (top.max(t), bottom.min(b)),
            Some((from, x0, x1)) if from == a.from => {
                base = Some((from, x0.min(left), x1.max(left + w)));
                (top, bottom) = (top.max(t), bottom.min(b));
            }
            _ => {
                base = Some((a.from, left, left + w));
                (top, bottom) = (t, b);
            }
        }
    }
}

/// The mark rule (ADR-1302 D3, the maintainer's decisions of 2026-11-01):
/// the marks of a cluster, `row` its top and bottom edges in pixels up
/// from the baseline, are fitted to the row. Marks over the middle of the
/// base letter's ink form one stack and those under it another. A stack
/// whose ink passes the row's edge moves toward the baseline until it
/// fits, as long as the move lands no more of its ink on the letter's ink
/// than lies there already and leaves the stack inside the row's other
/// edge. When the move fails either test, the whole cluster, letter and
/// marks together, is scaled vertically just enough to fit the row,
/// keeping the font's arrangement. The letter's own ink need not reach
/// the row's edge for that: lam with shadda and fatha in DejaVu Sans Mono
/// has blank rows above the lam, and is scaled because its marks, moved
/// down, would land on the lam's stem. Otherwise base letters never move,
/// and a mark keeps the sideways place the shaper gave it.
fn fit_marks(parts: &mut [(Image, i32, i32)], marks: &[bool], row: (i32, i32)) {
    let (row_top, row_bottom) = row;
    let inked = |p: &(Image, i32, i32)| p.0.placement.height > 0 && p.0.placement.width > 0;
    let bases: Vec<usize> = (0..parts.len())
        .filter(|&i| marks.get(i) == Some(&false) && parts.get(i).is_some_and(inked))
        .collect();
    let base_ink = ink_set(parts, &bases);
    let (base_top, base_bottom) = edges(parts, &bases).unwrap_or((0, 0));
    let middle = (base_top + base_bottom) / 2;
    let mut scale = false;
    for above in [true, false] {
        let stack: Vec<usize> = (0..parts.len())
            .filter(|&i| marks.get(i) == Some(&true) && parts.get(i).is_some_and(inked))
            .filter(|&i| {
                let (t, b) = parts.get(i).map(span).unwrap_or_default();
                (t + b >= 2 * middle) == above
            })
            .collect();
        let Some((top, bottom)) = edges(parts, &stack) else {
            continue;
        };
        let over = if above {
            top - row_top
        } else {
            row_bottom - bottom
        };
        if over <= 0 {
            continue;
        }
        let shift = if above { -over } else { over };
        let fits = if above {
            bottom - over >= row_bottom
        } else {
            top + over <= row_top
        };
        let touches = |shift: i32| {
            stack
                .iter()
                .filter_map(|&i| parts.get(i))
                .map(|p| overlap(p, shift, &base_ink))
                .sum::<usize>()
        };
        if fits && touches(shift) <= touches(0) {
            for &i in &stack {
                if let Some(p) = parts.get_mut(i) {
                    p.2 += shift;
                }
            }
        } else {
            scale = true;
        }
    }
    if !scale {
        return;
    }
    let all: Vec<usize> = (0..parts.len())
        .filter(|&i| parts.get(i).is_some_and(inked))
        .collect();
    let Some((top, bottom)) = edges(parts, &all) else {
        return;
    };
    let (to_top, to_bottom) = (top.min(row_top), bottom.max(row_bottom));
    if top <= bottom || (to_top, to_bottom) == (top, bottom) {
        return;
    }
    let f = (to_top - to_bottom) as f32 / (top - bottom) as f32;
    for &i in &all {
        if let Some(p) = parts.get_mut(i) {
            remap(p, bottom, to_bottom, f);
        }
    }
}

/// The top and bottom edges of `parts`' `which`, in pixels up from the
/// baseline; `None` for none.
fn edges(parts: &[(Image, i32, i32)], which: &[usize]) -> Option<(i32, i32)> {
    which
        .iter()
        .filter_map(|&i| parts.get(i).map(span))
        .reduce(|(t, b), (t2, b2)| (t.max(t2), b.min(b2)))
}

/// Each inked pixel of `parts`' `which`, as its column and its row up from
/// the baseline, the row's top edge.
fn ink_set(parts: &[(Image, i32, i32)], which: &[usize]) -> std::collections::BTreeSet<(i32, i32)> {
    let mut out = std::collections::BTreeSet::new();
    for p in which.iter().filter_map(|&i| parts.get(i)) {
        for_ink(p, 0, |x, y| {
            out.insert((x, y));
        });
    }
    out
}

/// How many of `part`'s inked pixels, moved up by `shift`, land on `ink`.
fn overlap(
    part: &(Image, i32, i32),
    shift: i32,
    ink: &std::collections::BTreeSet<(i32, i32)>,
) -> usize {
    let mut n = 0;
    for_ink(part, shift, |x, y| {
        if ink.contains(&(x, y)) {
            n += 1;
        }
    });
    n
}

/// Calls `f` with the column and the row up from the baseline of each
/// pixel `part` inks, moved up by `shift`.
fn for_ink(part: &(Image, i32, i32), shift: i32, mut f: impl FnMut(i32, i32)) {
    let (image, dx, dy) = part;
    let w = image.placement.width as usize;
    for (row, line) in image.data.chunks(w.max(1)).enumerate() {
        for (col, &p) in line.iter().enumerate() {
            if p > 0 {
                f(
                    dx + image.placement.left + col as i32,
                    dy + shift + image.placement.top - row as i32,
                );
            }
        }
    }
}

/// `part`'s mask scaled vertically by `f`, its row `from` up from the
/// baseline going to row `to`: each row of the result the average of the
/// rows it covers.
fn remap(part: &mut (Image, i32, i32), from: i32, to: i32, f: f32) {
    let (image, _, dy) = part;
    let (w, h) = (
        image.placement.width as usize,
        image.placement.height as i32,
    );
    let top = *dy + image.placement.top;
    let map = |y: i32| to as f32 + (y - from) as f32 * f;
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
        let back = |y: f32| from as f32 + (y - to as f32) / f;
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

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::{Ask, Drawer, Kind, draw_parts, shape_all, span};
    use crate::frit::Hinting;

    /// Each glyph of lam, shadda and fatha (U+0644 U+0651 U+064E) in the
    /// subset of DejaVu Sans Mono in `tests/fixtures/`, as its glyph id
    /// and its ink's top and bottom edges in pixels up from the baseline,
    /// drawn in a 16-pixel cell as `rasterise` draws it: 13.3 pixels to the
    /// em, the baseline 13 rows down, hinted fully. With `row`, the marks
    /// are fitted to it. The test pins the fitting on full hinting, where
    /// its numbers were measured: light hinting, the default, fits each
    /// mark to the rows on its own and leaves the fatha's bottom a row
    /// under the shadda's, at 11 and 12 above the baseline as shaped
    /// (`tests/shaping.rs` drives the stack at the default).
    fn lam_shadda_fatha(row: Option<(i32, i32)>) -> Vec<(u16, (i32, i32))> {
        let data = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/DejaVuSansMono-lam-shadda-fatha.ttf"),
        )
        .expect("the fixture");
        let font = swash::FontRef::from_index(&data, 0).expect("a font");
        let m = font.metrics(&[]);
        // as `ppem` sizes a face to a cell 16 pixels high
        let ppem = 15.5 * f32::from(m.units_per_em) / (m.ascent + m.descent.abs());
        let ask = Ask {
            key: vec![0x644, 0x651, 0x64e],
            kind: Kind::Marks,
        };
        let shaped = shape_all(&data, 0, font, &[ask]);
        let glyphs = &shaped[0].glyphs;
        let scale = ppem / f32::from(m.units_per_em);
        let fit = (u32::MAX, u32::MAX);
        let parts = draw_parts(
            &mut Drawer::new(font, 0, Hinting::Full),
            glyphs,
            scale,
            ppem,
            fit,
            row,
            1.0,
        )
        .expect("drawn");
        glyphs
            .iter()
            .zip(&parts)
            .map(|(a, p)| (a.g, span(p)))
            .collect()
    }

    // ADR-1302 D3 (the maintainer, 2026-11-01): lam with shadda and fatha
    // in DejaVu Sans Mono at 16 pixels is scaled to fit the row, though
    // the lam's own ink stops three rows under the row's top. The shaper
    // puts both marks at the same offset, the fatha's ink inside the
    // bottom of the shadda's, and the pair passes the row's top. Moved
    // down until it fits, the pair would land on the lam's stem, so the
    // whole stack is scaled instead. This pins today's result: the stack
    // fits the row, the lam keeps its bottom edge, and the marks keep
    // their order and stay above the lam
    // covers: text.overhang
    #[test]
    fn lam_shadda_fatha_is_scaled_to_fit_the_row() {
        let row = (13, -3);
        let (lam, shadda, fatha) = (1, 3, 2);
        let edges = |parts: &[(u16, (i32, i32))], g: u16| {
            parts
                .iter()
                .find(|(id, _)| *id == g)
                .map(|&(_, e)| e)
                .expect("the glyph")
        };
        let shaped = lam_shadda_fatha(None);
        let fitted = lam_shadda_fatha(Some(row));
        assert!(edges(&shaped, lam).0 < row.0, "blank rows above the lam");
        assert!(
            edges(&shaped, shadda).0 > row.0,
            "the shadda passes the row's top"
        );
        for (g, (top, bottom)) in &fitted {
            assert!(*top <= row.0 && *bottom >= row.1, "glyph {g} in the row");
        }
        assert_eq!(
            edges(&fitted, lam).1,
            edges(&shaped, lam).1,
            "the lam keeps its bottom edge"
        );
        assert!(
            edges(&fitted, lam).0 < edges(&shaped, lam).0,
            "the lam is scaled, its top lowered"
        );
        for mark in [shadda, fatha] {
            assert!(
                edges(&fitted, mark).1 >= edges(&fitted, lam).0,
                "mark {mark} above the lam"
            );
        }
        for parts in [&shaped, &fitted] {
            assert!(
                edges(parts, shadda).0 > edges(parts, fatha).0,
                "the shadda's top over the fatha's"
            );
            assert_eq!(
                edges(parts, shadda).1,
                edges(parts, fatha).1,
                "the marks' bottom edges together"
            );
        }
        assert_eq!(
            fitted,
            [(fatha, (12, 8)), (shadda, (13, 8)), (lam, (8, -2))],
            "today's result"
        );
    }
}
