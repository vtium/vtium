//! The rectangle operations' parameters (DECERA, DECSERA, DECFRA, DECCRA,
//! DECCARA, DECRARA), DECSACE and DECSCA, read into the screen's calls.
//!
//! Each rectangle's corners are its first four parameters, `Pt ; Pl ; Pb ;
//! Pr`. DECCRA's page numbers name vtium's one page, so they change
//! nothing, as in xterm. DECCARA takes every SGR parameter as SGR applies
//! it to the pen, kitty's extension of DEC's 0, 1, 4, 5, 7 and 22 to 27;
//! DECRARA reverses DEC's 1, 4, 5 and 7, all four for 0, and 8, as xterm
//! does. An omitted parameter after the corners is 0.

use vtium_parse::Params;

use super::{State, sgr};
use crate::screen::{Erase, Extent, Rect};
use crate::style::{Attrs, Style, UnderlineStyle};
use crate::unicode::char_width;

impl State {
    /// The rectangle of parameters 0 to 3, `None` when it is inverted.
    fn rect(&self, params: &Params) -> Option<Rect> {
        let p = |i| Self::param(params, i);
        self.screen.rect(p(0), p(1), p(2), p(3))
    }

    /// `CSI Pt ; Pl ; Pb ; Pr ... $ F`: the rectangle operation of final
    /// byte `F`.
    pub(super) fn rect_op(&mut self, params: &Params, final_byte: u8) {
        // Only DECCARA's attributes take sub-parameters, never a corner.
        if (0..4).any(|i| params.subparams(i).len() > 0) {
            return;
        }
        match final_byte {
            b'x' => return self.fill(params),
            b'v' => {
                if let Some(source) = self.rect(params) {
                    let (top, left) = (Self::param(params, 5), Self::param(params, 6));
                    self.screen.copy_rect(source, top, left);
                }
                return;
            }
            _ => {}
        }
        let Some(rect) = self.rect(params) else {
            return;
        };
        match final_byte {
            b'z' => self.screen.erase_rect(rect, Erase::All),
            b'{' => self.screen.erase_rect(rect, Erase::Unprotected),
            b'r' if params.count() > 4 => self.screen.change_attrs(rect, |style| {
                let mut style = style;
                sgr::apply_from(&mut style, params, 4);
                style
            }),
            b't' => {
                let reverse = Reverse::of(params);
                self.screen.change_attrs(rect, |style| reverse.apply(style));
            }
            _ => {}
        }
    }

    /// DECFRA, `CSI Pc ; Pt ; Pl ; Pb ; Pr $ x`: the character of code
    /// `Pc`, 0 or omitted a space, through the character set in GL or GR
    /// as a printed one goes. DEC STD 070 takes a GL or GR graphic
    /// character and xterm any printable one; vtium takes one a cell wide,
    /// and the sequence with no parameter at all does nothing, as in xterm.
    fn fill(&mut self, params: &Params) {
        if params.count() == 1 && params.get(0).is_none() {
            return;
        }
        let code = params.get(0).filter(|&c| c != 0).unwrap_or(0x20);
        let eight_bit = self.input_mode == vtium_parse::InputMode::EightBit;
        // A copy, so a pending single shift stays for the next character.
        let mut charsets = self.charsets;
        let Some(c) = char::from_u32(code).and_then(|c| charsets.map(c, eight_bit)) else {
            return;
        };
        if c.is_control() || char_width(c) != 1 {
            return;
        }
        let rect = self.screen.rect(
            Self::param(params, 1),
            Self::param(params, 2),
            Self::param(params, 3),
            Self::param(params, 4),
        );
        if let Some(rect) = rect {
            self.screen.fill_rect(rect, c);
        }
    }

    /// DECSACE, `CSI Ps * x`: 2 the rectangle, anything else the stream.
    pub(super) fn set_attribute_extent(&mut self, params: &Params) {
        let extent = match params.get(0) {
            Some(2) => Extent::Rectangle,
            _ => Extent::Stream,
        };
        self.screen.set_attribute_extent(extent);
    }

    /// DECSCA, `CSI Ps " q`: 1 protects what is printed next from DECSED,
    /// DECSEL and DECSERA, 0, 2 and an omitted parameter end it, and any
    /// other value changes nothing.
    pub(super) fn set_protection(&mut self, params: &Params) {
        let mut pen = *self.screen.pen();
        match params.get(0).unwrap_or(0) {
            1 => pen.attrs.insert(Attrs::PROTECTED),
            0 | 2 => pen.attrs.remove(Attrs::PROTECTED),
            _ => return,
        }
        self.screen.set_pen(pen);
    }
}

/// The attributes DECRARA reverses: each named an odd number of times.
#[derive(Clone, Copy, Debug, Default)]
struct Reverse {
    flags: Attrs,
    underline: bool,
}

impl Reverse {
    fn of(params: &Params) -> Self {
        let mut r = Self::default();
        let mut flip = |flag: Attrs| {
            if r.flags.contains(flag) {
                r.flags.remove(flag);
            } else {
                r.flags.insert(flag);
            }
        };
        let mut underline = false;
        for i in 4..params.count() {
            match params.get(i).unwrap_or(0) {
                0 => {
                    flip(Attrs::BOLD);
                    flip(Attrs::BLINK);
                    flip(Attrs::INVERSE);
                    underline = !underline;
                }
                1 => flip(Attrs::BOLD),
                4 => underline = !underline,
                5 => flip(Attrs::BLINK),
                7 => flip(Attrs::INVERSE),
                8 => flip(Attrs::CONCEAL),
                _ => {}
            }
        }
        r.underline = underline;
        r
    }

    fn apply(self, mut style: Style) -> Style {
        for flag in [Attrs::BOLD, Attrs::BLINK, Attrs::INVERSE, Attrs::CONCEAL] {
            if self.flags.contains(flag) {
                if style.attrs.contains(flag) {
                    style.attrs.remove(flag);
                } else {
                    style.attrs.insert(flag);
                }
            }
        }
        if self.underline {
            let underline = match style.attrs.underline() {
                UnderlineStyle::None => UnderlineStyle::Single,
                _ => UnderlineStyle::None,
            };
            style.attrs.set_underline(underline);
        }
        style
    }
}
