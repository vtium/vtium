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
//!
//! XTPUSHSGR and XTPOPSGR (`CSI Pm # {` and `CSI # }`, and their aliases
//! `CSI Pm # p` and `CSI # q`) keep pens on a stack, as xterm's ctlseqs
//! defines them; [`Stack`] holds it.

use vtium_parse::{Params, SUBPARAMS_MAX};

use super::State;
use crate::bounds::SGR_STACK_MAX;
use crate::screen::Screen;
use crate::style::{Attrs, Color, Rgb, Style, UnderlineStyle};

/// Applies SGR's parameters, in order, to the screen's pen.
pub(super) fn apply(screen: &mut Screen, params: &Params) {
    let mut pen = *screen.pen();
    apply_from(&mut pen, params, 0);
    screen.set_pen(pen);
}

/// Applies SGR's parameters from parameter `first`, in order, to `pen`:
/// SGR's own, or DECCARA's after its rectangle.
pub(super) fn apply_from(pen: &mut Style, params: &Params, first: usize) {
    let count = params.count();
    let mut i = first;
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
            0 => reset(pen),
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

/// XTPUSHSGR's stack: the pens pushed, each with the parts its pop
/// restores, at most [`SGR_STACK_MAX`], and how many pushes past the bound
/// wait for their pops, which restore nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Stack {
    pens: Vec<(Style, Parts)>,
    over: usize,
}

/// The parts of the pen a pop restores: every part SGR sets, or those
/// XTPUSHSGR's parameters name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parts {
    All,
    /// A bit for each SGR code `n` of xterm's list, at `n - 1`.
    Named(u32),
}

/// The flags each code of XTPUSHSGR's list names: xterm's, and rapid
/// blink with blink, which SGR 25 clears with it.
const FLAG_PARTS: [(u32, &[Attrs]); 7] = [
    (1, &[Attrs::BOLD]),
    (2, &[Attrs::DIM]),
    (3, &[Attrs::ITALIC]),
    (5, &[Attrs::BLINK, Attrs::RAPID_BLINK]),
    (7, &[Attrs::INVERSE]),
    (8, &[Attrs::CONCEAL]),
    (9, &[Attrs::STRIKE]),
];

impl Parts {
    /// The parts XTPUSHSGR's parameters name: none or one omitted is every
    /// part; else each code of xterm's list, 1 to 31, with 10 and 11, its
    /// older codes, for 30 and 31; a code it does not list names nothing,
    /// as in xterm.
    fn from_params(params: &Params) -> Self {
        if params.count() == 0 || (params.count() == 1 && params.get(0).is_none()) {
            return Self::All;
        }
        let mut bits = 0_u32;
        for i in 0..params.count() {
            let code = match params.get(i).unwrap_or(0) {
                10 => 30,
                11 => 31,
                code => code,
            };
            if let Some(bit) = code.checked_sub(1).filter(|&b| b < 31) {
                bits |= 1 << bit;
            }
        }
        Self::Named(bits)
    }

    fn names(self, code: u32) -> bool {
        match self {
            Self::All => true,
            Self::Named(bits) => code
                .checked_sub(1)
                .and_then(|bit| bits.checked_shr(bit))
                .is_some_and(|b| b & 1 == 1),
        }
    }

    /// `pen` with the parts named taken from `saved`. Every part is SGR's
    /// alone, and DECSCA's protection stays as `pen` has it. 4 and 21 both
    /// name the underline, its style and its colour (SGR 58), which vtium
    /// holds as one; the overline and the script, which xterm's list does
    /// not name, return only with every part.
    fn restore(self, pen: Style, saved: Style) -> Style {
        let mut out = pen;
        let mut take = |flag: Attrs| {
            out.attrs.remove(flag);
            if saved.attrs.contains(flag) {
                out.attrs.insert(flag);
            }
        };
        for (code, flags) in FLAG_PARTS {
            if self.names(code) {
                flags.iter().copied().for_each(&mut take);
            }
        }
        if self == Self::All {
            take(Attrs::OVERLINE);
            out.attrs.set_script(saved.attrs.script());
        }
        if self.names(4) || self.names(21) {
            out.attrs.set_underline(saved.attrs.underline());
            out.underline_color = saved.underline_color;
        }
        if self.names(30) {
            out.fg = saved.fg;
        }
        if self.names(31) {
            out.bg = saved.bg;
        }
        out
    }
}

impl Stack {
    /// Pushes `pen`; past [`SGR_STACK_MAX`] the push is only counted.
    fn push(&mut self, pen: Style, parts: Parts) {
        if self.pens.len() < SGR_STACK_MAX {
            self.pens.push((pen, parts));
        } else {
            self.over = self.over.saturating_add(1);
        }
    }

    /// The pen the newest push restores over `pen`, or `pen` for a push
    /// past the bound or an empty stack.
    fn pop(&mut self, pen: Style) -> Style {
        if self.over > 0 {
            self.over = self.over.saturating_sub(1);
            return pen;
        }
        match self.pens.pop() {
            Some((saved, parts)) => parts.restore(pen, saved),
            None => pen,
        }
    }
}

impl State {
    /// XTPUSHSGR, `CSI Pm # {` or `CSI Pm # p`.
    pub(super) fn push_sgr(&mut self, params: &Params) {
        let pen = *self.screen.pen();
        self.sgr_stack.push(pen, Parts::from_params(params));
    }

    /// XTPOPSGR, `CSI # }` or `CSI # q`.
    pub(super) fn pop_sgr(&mut self) {
        let pen = self.sgr_stack.pop(*self.screen.pen());
        self.screen.set_pen(pen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Script;

    fn styled() -> Style {
        let mut pen = Style::DEFAULT;
        pen.attrs.insert(Attrs::OVERLINE);
        pen.attrs.insert(Attrs::RAPID_BLINK);
        pen.attrs.set_script(Script::Super);
        pen
    }

    // DECSCA's protection is not SGR's: a pop leaves it as the pen has it,
    // as SGR 0 does.
    #[test]
    fn a_pop_leaves_the_protection() {
        let mut stack = Stack::default();
        stack.push(Style::DEFAULT, Parts::All);
        let mut pen = Style::DEFAULT;
        pen.attrs.insert(Attrs::PROTECTED);
        assert!(stack.pop(pen).attrs.contains(Attrs::PROTECTED));
        stack.push(pen, Parts::All);
        assert!(!stack.pop(Style::DEFAULT).attrs.contains(Attrs::PROTECTED));
    }

    // The parts xterm's list does not name, the overline and the script,
    // return with every part only; rapid blink returns with blink, 5.
    #[test]
    fn the_parts_xterm_does_not_list_return_with_every_part() {
        let mut stack = Stack::default();
        stack.push(styled(), Parts::Named(!0));
        let pen = stack.pop(Style::DEFAULT);
        assert!(!pen.attrs.contains(Attrs::OVERLINE));
        assert_eq!(pen.attrs.script(), Script::Baseline);
        assert!(pen.attrs.contains(Attrs::RAPID_BLINK));
        stack.push(styled(), Parts::All);
        assert_eq!(stack.pop(Style::DEFAULT), styled());
    }
}
