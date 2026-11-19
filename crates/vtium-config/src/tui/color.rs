//! The TUI's colors (ADR-1403 D7): the console's sixteen colors, named by
//! SGR, so the user's palette applies. The palette's blue is hard to read
//! on black, so [`Color`] has no blue and no bright blue, and no text the
//! TUI draws can ask for either. A box's fill in the Flow view is a
//! background behind light text, so [`Normal`] has blue. With `NO_COLOR`
//! set, or on a terminal without color, the TUI draws no color at all ([`enabled`]).

/// A color of the console's palette, by its SGR number. Blue (4) and
/// bright blue (12) are left out on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Magenta,
    Cyan,
    /// The palette's color 7, light grey.
    White,
    /// The palette's color 8, dark grey.
    Grey,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl Color {
    /// The color's number in the palette, 0 to 15.
    pub const fn index(self) -> u8 {
        match self {
            Color::Black => 0,
            Color::Red => 1,
            Color::Green => 2,
            Color::Yellow => 3,
            Color::Magenta => 5,
            Color::Cyan => 6,
            Color::White => 7,
            Color::Grey => 8,
            Color::BrightRed => 9,
            Color::BrightGreen => 10,
            Color::BrightYellow => 11,
            Color::BrightMagenta => 13,
            Color::BrightCyan => 14,
            Color::BrightWhite => 15,
        }
    }

    /// The SGR parameter that sets the color as the text's color: 30 to
    /// 37, or 90 to 97 for the bright half.
    pub const fn fg(self) -> u8 {
        let i = self.index();
        if i < 8 { 30 + i } else { 90 + (i - 8) }
    }

    /// The SGR parameter that sets the color as the background: 40 to 47,
    /// or 100 to 107 for the bright half.
    pub const fn bg(self) -> u8 {
        let i = self.index();
        if i < 8 { 40 + i } else { 100 + (i - 8) }
    }
}

/// One of the eight normal colors, as the background of the inside of a
/// box. The Linux console has no bright background colors, so a fill is
/// always one of these. Blue is among them, because light text on blue
/// reads well, though blue text on black does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Normal {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl Normal {
    /// The SGR parameter that sets the color as the background, 40 to 47.
    pub const fn bg(self) -> u8 {
        match self {
            Normal::Black => 40,
            Normal::Red => 41,
            Normal::Green => 42,
            Normal::Yellow => 43,
            Normal::Blue => 44,
            Normal::Magenta => 45,
            Normal::Cyan => 46,
            Normal::White => 47,
        }
    }
}

/// The background of the inside of a box: a normal color, and a 24-bit
/// color drawn in its place where the terminal takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Back {
    pub basic: Normal,
    pub rgb: Option<[u8; 3]>,
}

/// How a run of text is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    /// A 24-bit color drawn in `fg`'s place, where the terminal takes it.
    pub rgb: Option<[u8; 3]>,
    pub bg: Option<Color>,
    /// The fill of a box's inside, drawn in `bg`'s place.
    pub back: Option<Back>,
    /// Reverse video, the TUI's selection mark.
    pub reverse: bool,
    pub bold: bool,
    pub underline: bool,
    pub italic: bool,
    /// Superscript or subscript, SGR 73 or 74, which only the font
    /// preview in a vtium console asks for (ADR-1201 D4).
    pub script: vtium_font::Script,
}

impl Style {
    /// Plain text: the terminal's own colors.
    pub const PLAIN: Style = Style {
        fg: None,
        rgb: None,
        bg: None,
        back: None,
        reverse: false,
        bold: false,
        underline: false,
        italic: false,
        script: vtium_font::Script::Baseline,
    };

    /// Text in `color`.
    pub const fn text(color: Color) -> Style {
        Style {
            fg: Some(color),
            ..Style::PLAIN
        }
    }

    /// A block of color: black text on `color`, as every block of the
    /// flow is drawn.
    pub const fn block(color: Color) -> Style {
        Style {
            fg: Some(Color::Black),
            bg: Some(color),
            ..Style::PLAIN
        }
    }

    /// The style in reverse video.
    pub const fn reversed(self) -> Style {
        Style {
            reverse: !self.reverse,
            ..self
        }
    }

    /// The style as ratatui's buffer holds it, so that ratatui's cells
    /// carry what the TUI sends: the 24-bit color where one is set,
    /// else the named color by its index, and the box fill's color before
    /// the background's. The two script positions ride on the modifiers
    /// that [`super::backend::SUPER`] and [`super::backend::SUB`] name.
    pub fn cell(self) -> ratatui_core::style::Style {
        use ratatui_core::style::{Color as C, Modifier as M};
        let fg = match (self.rgb, self.fg) {
            (Some([r, g, b]), _) => C::Rgb(r, g, b),
            (None, Some(c)) => C::Indexed(c.index()),
            (None, None) => C::Reset,
        };
        let bg = match (self.back, self.bg) {
            (
                Some(Back {
                    rgb: Some([r, g, b]),
                    ..
                }),
                _,
            ) => C::Rgb(r, g, b),
            // a basic fill's SGR code is 40 and its index
            (Some(Back { basic, .. }), _) => C::Indexed(basic.bg().saturating_sub(40)),
            (None, Some(c)) => C::Indexed(c.index()),
            (None, None) => C::Reset,
        };
        let mut m = M::empty();
        for (on, bit) in [
            (self.bold, M::BOLD),
            (self.italic, M::ITALIC),
            (self.underline, M::UNDERLINED),
            (self.reverse, M::REVERSED),
            (
                self.script == vtium_font::Script::Super,
                super::backend::SUPER,
            ),
            (self.script == vtium_font::Script::Sub, super::backend::SUB),
        ] {
            if on {
                m |= bit;
            }
        }
        ratatui_core::style::Style::new()
            .fg(fg)
            .bg(bg)
            .add_modifier(m)
    }
}

/// The colors of the flow's sections (D7).
pub mod palette {
    use super::Color;

    pub const DISPLAYS: Color = Color::BrightCyan;
    pub const DISPLAY_RULES: Color = Color::Cyan;
    pub const HEADS: Color = Color::BrightYellow;
    pub const LOOKS: Color = Color::BrightMagenta;
    pub const INPUT_RULES: Color = Color::Green;
    pub const INPUTS: Color = Color::BrightGreen;
    /// The targets that are not heads, and what is not in play.
    pub const GREY: Color = Color::Grey;
    /// An error.
    pub const ERROR: Color = Color::BrightRed;
    /// The base card, black on light grey.
    pub const BASE: Color = Color::White;
}

/// Whether the TUI draws color, from the environment `env`: not with a
/// non-empty `NO_COLOR`, and not on a terminal that names no color, a
/// `TERM` that is unset or `dumb`.
pub fn enabled(env: &dyn Fn(&str) -> Option<String>) -> bool {
    let set = |name: &str| env(name).filter(|v| !v.is_empty());
    if set("NO_COLOR").is_some() {
        return false;
    }
    set("TERM").is_some_and(|t| t != "dumb")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Color; 14] = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Magenta,
        Color::Cyan,
        Color::White,
        Color::Grey,
        Color::BrightRed,
        Color::BrightGreen,
        Color::BrightYellow,
        Color::BrightMagenta,
        Color::BrightCyan,
        Color::BrightWhite,
    ];

    #[test]
    fn no_color_is_blue_in_either_half() {
        for c in ALL {
            assert!(![4, 12].contains(&c.index()), "{c:?}");
            assert!(![34, 94].contains(&c.fg()), "{c:?}");
            assert!(![44, 104].contains(&c.bg()), "{c:?}");
        }
    }

    #[test]
    fn the_sections_take_the_record_s_colors() {
        use palette::*;
        assert_eq!(
            [
                DISPLAYS,
                DISPLAY_RULES,
                HEADS,
                LOOKS,
                INPUT_RULES,
                INPUTS,
                GREY,
                ERROR
            ]
            .map(Color::index),
            [14, 6, 11, 13, 2, 10, 8, 9]
        );
    }

    /// The SGR sequence the Backend sends for a cell in `style`.
    fn sent(style: Style, color: bool) -> String {
        let mut cell = ratatui_core::buffer::Cell::default();
        cell.set_style(style.cell());
        super::super::backend::sgr(&cell, color)
    }

    #[test]
    fn a_block_is_black_on_its_color_and_reverse_keeps_both() {
        let s = Style::block(Color::BrightCyan);
        assert_eq!(sent(s, true), "\x1b[30;106m");
        assert_eq!(sent(s.reversed(), true), "\x1b[7;30;106m");
        assert_eq!(sent(s.reversed(), false), "\x1b[7m");
        assert_eq!(sent(s, false), "");
    }

    #[test]
    fn superscript_and_subscript_are_sgr_73_and_74_with_or_without_color() {
        let sup = Style {
            script: vtium_font::Script::Super,
            ..Style::PLAIN
        };
        let sub = Style {
            script: vtium_font::Script::Sub,
            ..Style::text(Color::Cyan)
        };
        assert_eq!(sent(sup, false), "\x1b[73m");
        assert_eq!(sent(sub, true), "\x1b[74;36m");
        assert_eq!(sent(Style::PLAIN, true), "");
    }

    #[test]
    fn no_color_and_dumb_terminals_turn_color_off() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| (*v).to_owned())
            }
        };
        assert!(enabled(&env(&[("TERM", "linux")])));
        assert!(!enabled(&env(&[("TERM", "linux"), ("NO_COLOR", "1")])));
        // an empty NO_COLOR is unset, as no-color.org has it
        assert!(enabled(&env(&[("TERM", "xterm"), ("NO_COLOR", "")])));
        assert!(!enabled(&env(&[("TERM", "dumb")])));
        assert!(!enabled(&env(&[])));
    }
}
