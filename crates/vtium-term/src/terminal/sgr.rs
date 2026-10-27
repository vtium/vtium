//! SGR, `CSI Pm m`: the pen printed text takes.
//!
//! The forms are the C reference's. An index below 256 or an RGB colour
//! comes from xterm's `38;5;n` and `38;2;r;g;b`, which always take
//! exactly three values, and from ITU T.416's colon forms `38:5:n` and
//! `38:2:cs:r:g:b`, where the colour space may be empty and the fields T.416
//! puts after b are ignored. `38:2:r:g:b`, which programs send, is told
//! apart from the colour-space form by its count of sub-parameters, as
//! foot does (csi.c, MIT, Copyright (c) 2019 Daniel Eklöf). An index or
//! component above 255 rejects a colon form; a semicolon form keeps a
//! component's low byte and draws an index past 255 black, as the C
//! reference's goldens record.

use vtium_parse::{Params, SUBPARAMS_MAX};

use crate::screen::Screen;
use crate::style::{Attrs, Color, Rgb, Style, UnderlineStyle};

/// Applies SGR's parameters, in order, to the screen's pen.
pub(super) fn apply(screen: &mut Screen, params: &Params) {
    let mut pen = *screen.pen();
    let count = params.count();
    let mut i = 0;
    while i < count {
        // An empty parameter is SGR's default, 0 (ECMA-48 5.4.2 e, 8.3.117).
        let code = params.get(i).unwrap_or(0);
        let subs = params.subparams(i).len();
        // Only 4, 38, 48 and 58 take sub-parameters; any other parameter
        // carrying them is skipped on its own.
        if subs > 0 && !matches!(code, 4 | 38 | 48 | 58) {
            i = i.saturating_add(1);
            continue;
        }
        let a = &mut pen.attrs;
        match code {
            0 => reset(&mut pen),
            1 => a.insert(Attrs::BOLD),
            2 => a.insert(Attrs::DIM),
            3 => a.insert(Attrs::ITALIC),
            4 => {
                if let Some(style) = underline(params, i) {
                    a.set_underline(style);
                }
            }
            5 => a.insert(Attrs::BLINK),
            7 => a.insert(Attrs::INVERSE),
            9 => a.insert(Attrs::STRIKE),
            // ECMA-48's doubly underlined; the Linux console once read it as
            // normal intensity.
            21 => a.set_underline(UnderlineStyle::Double),
            22 => {
                a.remove(Attrs::BOLD);
                a.remove(Attrs::DIM);
            }
            23 => a.remove(Attrs::ITALIC),
            24 => a.set_underline(UnderlineStyle::None),
            25 => {
                a.remove(Attrs::BLINK);
                a.remove(Attrs::RAPID_BLINK);
            }
            27 => a.remove(Attrs::INVERSE),
            29 => a.remove(Attrs::STRIKE),
            30..=37 => pen.fg = indexed(code, 30, 0),
            39 => pen.fg = Color::Default,
            40..=47 => pen.bg = indexed(code, 40, 0),
            49 => pen.bg = Color::Default,
            90..=97 => pen.fg = indexed(code, 90, 8),
            100..=107 => pen.bg = indexed(code, 100, 8),
            38 | 48 | 58 => {
                if let Some((color, used)) = color(params, i) {
                    match code {
                        38 => pen.fg = color,
                        48 => pen.bg = color,
                        _ => pen.underline_color = color,
                    }
                    i = i.saturating_add(used);
                }
            }
            59 => pen.underline_color = Color::Default,
            _ => {}
        }
        i = i.saturating_add(1);
    }
    screen.set_pen(pen);
}

/// SGR 0: everything SGR sets returns to its default; DECSCA's protection
/// is not SGR's and stays.
fn reset(pen: &mut Style) {
    let protected = pen.attrs.contains(Attrs::PROTECTED);
    *pen = Style::DEFAULT;
    if protected {
        pen.attrs.insert(Attrs::PROTECTED);
    }
}

/// A palette colour from a code in a run of eight.
fn indexed(code: u32, first: u32, base: u32) -> Color {
    let index = code.saturating_sub(first).saturating_add(base);
    Color::Indexed(u8::try_from(index).unwrap_or(0))
}

/// SGR 4 with its sub-parameter: 4 and 4:1 single, 4:0 none, 4:2 to 4:5
/// double, curly, dotted and dashed, an empty one none, and any other
/// single, as Ghostty draws it (sgr.zig, MIT). More than one
/// sub-parameter is not this form.
fn underline(params: &Params, i: usize) -> Option<UnderlineStyle> {
    let mut subs = params.subparams(i);
    if subs.len() > 1 {
        return None;
    }
    Some(match subs.next() {
        None => UnderlineStyle::Single,
        Some(None | Some(0)) => UnderlineStyle::None,
        Some(Some(2)) => UnderlineStyle::Double,
        Some(Some(3)) => UnderlineStyle::Curly,
        Some(Some(4)) => UnderlineStyle::Dotted,
        Some(Some(5)) => UnderlineStyle::Dashed,
        Some(Some(_)) => UnderlineStyle::Single,
    })
}

/// The colour SGR 38, 48 or 58 at parameter `i` selects, and how many
/// parameters after `i` it used; `None` when it is none of the forms.
fn color(params: &Params, i: usize) -> Option<(Color, usize)> {
    let mut buf = [0_u32; SUBPARAMS_MAX];
    let mut n = 0;
    for (k, (slot, sub)) in buf.iter_mut().zip(params.subparams(i)).enumerate() {
        *slot = sub.unwrap_or(0);
        n = k.saturating_add(1);
    }
    match buf.get(..n).unwrap_or_default() {
        [5, index] => {
            return Some((Color::Indexed(u8::try_from(*index).ok()?), 0));
        }
        [2, rgb @ ..] if rgb.len() >= 3 => {
            // r, g and b follow the colour space's slot when there is one.
            let rgb = if rgb.len() == 3 { rgb } else { rgb.get(1..)? };
            let [r, g, b] = [rgb.first()?, rgb.get(1)?, rgb.get(2)?].map(|&c| u8::try_from(c).ok());
            return Some((Color::Rgb(Rgb::new(r?, g?, b?)), 0));
        }
        [] => {}
        _ => return None,
    }

    // The semicolon forms: each value a parameter of its own, carrying no
    // sub-parameters. An empty one before a `;` is 0 and the last one
    // missing, as the C reference read them.
    let count = params.count();
    let arg = |k: usize| -> Option<u32> {
        let j = i.checked_add(k)?;
        if j >= count || params.subparams(j).len() > 0 {
            return None;
        }
        match params.get(j) {
            Some(v) => Some(v),
            None if j.checked_add(1)? < count => Some(0),
            None => None,
        }
    };
    match arg(1)? {
        5 => {
            let index = arg(2)?;
            let color = match u8::try_from(index) {
                Ok(index) => Color::Indexed(index),
                Err(_) => Color::Rgb(Rgb::new(0, 0, 0)),
            };
            Some((color, 2))
        }
        2 => {
            let low = |c: u32| c.to_le_bytes()[0];
            let (r, g, b) = (arg(2)?, arg(3)?, arg(4)?);
            Some((Color::Rgb(Rgb::new(low(r), low(g), low(b))), 4))
        }
        _ => None,
    }
}
