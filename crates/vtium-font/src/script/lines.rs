//! The lines of a superscript or subscript cell (ADR-1202 D8): where its
//! underline, strike-through and overline go, and how thick each is.
//!
//! A script cell's lines are the lines of the script glyph's own size,
//! placed where D1 places the script glyph's own box. A face makes one set
//! for superscript and one for subscript, from its source, as its normal
//! lines come from its source:
//!
//! - **A smaller size (D3).** Where a bitmap source's superscripts come
//!   from a smaller whole multiple of its own strike, the lines are that
//!   strike's own lines at that multiple ([`at_multiple`]), moved down by
//!   the row where D1 places the strike's top ([`placed`]).
//! - **No smaller size (D4).** The normal lines go through D4's own map
//!   ([`mapped`]): each row of a line goes to the small row that holds its
//!   centre, so the line lands where D4 puts the glyph's pixels from the
//!   same row. A line keeps at least one row.
//! - **An outline source (D2).** The glyph helper measures the font's
//!   lines at the script size from the script baseline, and the script
//!   strike reports them.
//!
//! The overline lies on the top row of the script glyph's own box, as
//! thick as the set's underline. Every line lies inside the cell. The
//! console and the glyph helper both call this module, so they never
//! place two different sets of lines (D11).

use super::{Kind, Script, centre, place, size, smaller};
use crate::face::{Decorations, Face, Style};
use crate::spacing::Band;

/// The lines `d` of a strike `height` rows tall, drawn at the whole
/// multiple `scale`. A strike whose lines are [`Decorations::for_height`]
/// of its height, as every bitmap source gives, takes `for_height` of the
/// scaled height, the lines the bitmap faces draw at that size. Any other
/// strike's lines scale as its glyphs do, each row a band `scale` rows
/// tall.
pub fn at_multiple(d: Decorations, height: u32, scale: u32) -> Decorations {
    if d == Decorations::for_height(height) {
        return Decorations::for_height(height.saturating_mul(scale));
    }
    Decorations {
        underline: d.underline.saturating_mul(scale),
        underline_thickness: d.underline_thickness.saturating_mul(scale),
        strike: d.strike.saturating_mul(scale),
        strike_thickness: d.strike_thickness.saturating_mul(scale),
        overline: d.overline.saturating_mul(scale),
    }
}

/// A line `rows` thick from row `top`, cut to a cell `height` rows high:
/// at least a row thick, and moved up to end inside the cell.
fn inside(top: i64, rows: i64, height: u32) -> (u32, u32) {
    let h = i64::from(height.max(1));
    let rows = rows.clamp(1, h);
    let top = top.clamp(0, h.saturating_sub(rows));
    (
        u32::try_from(top).unwrap_or(0),
        u32::try_from(rows).unwrap_or(1),
    )
}

/// The lines a cell `height` rows high draws in `script` when its glyph
/// has no smaller size (D4): the normal lines `normal`, each row sent to
/// the small row that holds its centre as D4 sends the glyph's rows, for a
/// glyph whose own box is `own.1` rows from row `own.0`, made two thirds
/// of its height and placed against the text band, which `band` leaves.
/// The overline lies on the small box's top row. The baseline's lines are
/// `normal` itself.
pub fn mapped(
    normal: Decorations,
    script: Script,
    (top, tall): (u32, u32),
    height: u32,
    band: Band,
) -> Decorations {
    if script == Script::Baseline || tall == 0 {
        return normal;
    }
    let (_, small) = size((1, tall));
    let (_, at) = place(script, (1, small), (1, height), band);
    let at = i64::from(at);
    let row = |r: u32| {
        at.saturating_add(centre(
            i64::from(r).saturating_sub(i64::from(top)),
            tall,
            small,
        ))
    };
    let line = |r: u32, rows: u32| {
        let first = row(r);
        let last = row(r.saturating_add(rows.max(1)).saturating_sub(1)).max(first);
        inside(first, last.saturating_sub(first).saturating_add(1), height)
    };
    let (underline, underline_thickness) = line(normal.underline, normal.underline_thickness);
    let (strike, strike_thickness) = line(normal.strike, normal.strike_thickness);
    let (overline, _) = inside(at, i64::from(underline_thickness), height);
    Decorations {
        underline,
        underline_thickness,
        strike,
        strike_thickness,
        overline,
    }
}

/// The lines a cell `span` columns across and `height` rows high draws in
/// `script` when its glyph is a smaller size drawn whole (D3): `own`, the
/// lines of that size counted from its own top row, moved down by the row
/// where [`place`] puts a glyph `size` pixels across and high against the
/// text band, which `band` leaves. The overline lies on that row. The
/// baseline's lines are `own` itself.
pub fn placed(
    own: Decorations,
    script: Script,
    size: (u32, u32),
    (span, height): (u32, u32),
    band: Band,
) -> Decorations {
    if script == Script::Baseline {
        return own;
    }
    let (_, at) = place(script, size, (span, height), band);
    let at = i64::from(at);
    let line = |r: u32, rows: u32| inside(at.saturating_add(i64::from(r)), i64::from(rows), height);
    let (underline, underline_thickness) = line(own.underline, own.underline_thickness);
    let (strike, strike_thickness) = line(own.strike, own.strike_thickness);
    let (overline, _) = inside(at, i64::from(underline_thickness), height);
    Decorations {
        underline,
        underline_thickness,
        strike,
        strike_thickness,
        overline,
    }
}

/// `face`'s lines in `script` (D8 step 3). A 1-bit bitmap face drawn at a
/// whole multiple that has a smaller multiple in D3's range takes its
/// strike's own lines at that multiple ([`Face::strike_decorations`]),
/// placed whole ([`placed`]). Any other face maps its normal lines by D4
/// ([`mapped`]), its own box the text band. The baseline's lines are the
/// face's normal ones.
pub fn of_face<F: Face + ?Sized>(face: &F, script: Script) -> Decorations {
    let normal = face.decorations();
    if script == Script::Baseline {
        return normal;
    }
    let cell = face.cell();
    let band = face.band();
    let bitmap = face.origin(&[], Style::default()).kind == Kind::Bitmap;
    if let Some((own, multiple)) = face.strike().filter(|_| bitmap)
        && let Some(normal_height) = own.height.checked_mul(multiple)
        && let Some(chosen) = smaller(normal_height, cell.width, &[own])
    {
        let j = chosen.multiple;
        let lines = at_multiple(face.strike_decorations(), own.height, j);
        let size = (own.width.saturating_mul(j), own.height.saturating_mul(j));
        return placed(lines, script, size, (cell.width, cell.height), band);
    }
    let text = (
        band.above.min(cell.height.saturating_sub(1)),
        cell.height.saturating_sub(band.added()).max(1),
    );
    mapped(normal, script, text, cell.height, band)
}
