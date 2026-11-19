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

    /// The color as ratatui names it.
    pub const fn named(self) -> ratatui_core::style::Color {
        use ratatui_core::style::Color as C;
        match self {
            Color::Black => C::Black,
            Color::Red => C::Red,
            Color::Green => C::Green,
            Color::Yellow => C::Yellow,
            Color::Magenta => C::Magenta,
            Color::Cyan => C::Cyan,
            Color::White => C::Gray,
            Color::Grey => C::DarkGray,
            Color::BrightRed => C::LightRed,
            Color::BrightGreen => C::LightGreen,
            Color::BrightYellow => C::LightYellow,
            Color::BrightMagenta => C::LightMagenta,
            Color::BrightCyan => C::LightCyan,
            Color::BrightWhite => C::White,
        }
    }

    /// The color that ratatui's `color` names, if the TUI has it: not
    /// blue, and not an index or a 24-bit color.
    pub const fn of_named(color: ratatui_core::style::Color) -> Option<Color> {
        use ratatui_core::style::Color as C;
        Some(match color {
            C::Black => Color::Black,
            C::Red => Color::Red,
            C::Green => Color::Green,
            C::Yellow => Color::Yellow,
            C::Magenta => Color::Magenta,
            C::Cyan => Color::Cyan,
            C::Gray => Color::White,
            C::DarkGray => Color::Grey,
            C::LightRed => Color::BrightRed,
            C::LightGreen => Color::BrightGreen,
            C::LightYellow => Color::BrightYellow,
            C::LightMagenta => Color::BrightMagenta,
            C::LightCyan => Color::BrightCyan,
            C::White => Color::BrightWhite,
            _ => return None,
        })
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
    const ALL: [Normal; 8] = [
        Normal::Black,
        Normal::Red,
        Normal::Green,
        Normal::Yellow,
        Normal::Blue,
        Normal::Magenta,
        Normal::Cyan,
        Normal::White,
    ];

    /// The color's number in the palette, 0 to 7.
    pub const fn index(self) -> u8 {
        self.bg().saturating_sub(40)
    }

    /// The normal color numbered `index` in the palette, 0 to 7.
    pub fn of_index(index: u8) -> Option<Normal> {
        Normal::ALL.get(usize::from(index)).copied()
    }

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

    /// The style as ratatui's buffer holds it for the terminal, so that
    /// ratatui's cells carry what the TUI sends: the 24-bit color where
    /// one is set, else the named color, and the box fill's color before
    /// the background's. The two script positions ride on the modifiers
    /// that [`super::backend::SUPER`] and [`super::backend::SUB`] name.
    pub fn cell(self) -> ratatui_core::style::Style {
        self.in_cells(Colors::Sent)
    }

    /// The style as ratatui's buffer holds it, its colors read as
    /// `colors` says.
    pub fn in_cells(self, colors: Colors) -> ratatui_core::style::Style {
        use ratatui_core::style::{Color as C, Modifier as M};
        let sent = colors == Colors::Sent;
        let fg = match (self.rgb, self.fg) {
            (Some([r, g, b]), None) => C::Rgb(r, g, b),
            (Some([r, g, b]), Some(_)) if sent => C::Rgb(r, g, b),
            (_, Some(c)) => c.named(),
            (None, None) => C::Reset,
        };
        let bg = match (self.back, self.bg) {
            (
                Some(Back {
                    rgb: Some([r, g, b]),
                    ..
                }),
                _,
            ) if sent => C::Rgb(r, g, b),
            // a box's fill is one of the eight normal colors, so its index
            // keeps it apart from a background, which is a named color
            (Some(Back { basic, .. }), _) => C::Indexed(basic.index()),
            (None, Some(c)) => c.named(),
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

    /// The style of a cell that [`Style::in_cells`] wrote with
    /// [`Colors::Named`], read back. A named color comes back as itself
    /// and a box's fill as its normal color, without the 24-bit colors that
    /// reading left out. A color no style of the TUI names, which only the
    /// About page's banner sets, comes back as the terminal's own color,
    /// except a 24-bit text color.
    pub fn of_cell(cell: ratatui_core::style::Style) -> Style {
        use ratatui_core::style::{Color as C, Modifier as M};
        let m = cell.add_modifier;
        let fg = cell.fg.unwrap_or(C::Reset);
        let bg = cell.bg.unwrap_or(C::Reset);
        Style {
            fg: Color::of_named(fg),
            rgb: match fg {
                C::Rgb(r, g, b) => Some([r, g, b]),
                _ => None,
            },
            bg: Color::of_named(bg),
            back: match bg {
                C::Indexed(n) => Normal::of_index(n).map(|basic| Back { basic, rgb: None }),
                _ => None,
            },
            reverse: m.contains(M::REVERSED),
            bold: m.contains(M::BOLD),
            underline: m.contains(M::UNDERLINED),
            italic: m.contains(M::ITALIC),
            script: if m.contains(super::backend::SUPER) {
                vtium_font::Script::Super
            } else if m.contains(super::backend::SUB) {
                vtium_font::Script::Sub
            } else {
                vtium_font::Script::Baseline
            },
        }
    }
}

/// How the TUI's colors reach ratatui's cells. A style can hold a named
/// color and a 24-bit color side by side, and a cell holds one color for
/// the text and one for the background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colors {
    /// What the terminal is sent: the 24-bit color where the style has
    /// one, else the named color.
    Sent,
    /// The named color wherever the style has one, so the screen read
    /// back from the cells, which the pictures of the console draw from,
    /// keeps the named colors. A style with only a 24-bit color keeps it.
    Named,
}

/// One character of text that styles itself with SGR, and the style its
/// SGR sequences set.
pub type SgrCell = (char, ratatui_core::style::Style);

/// The characters of `text`, each in the style its SGR sequences set, with
/// the sequences left out: the About page's banner, which its own module
/// styles in 24-bit or 256 colors.
pub fn sgr_cells(text: &str) -> Vec<SgrCell> {
    use ratatui_core::style::{Color as C, Modifier as M, Style as S};
    let mut out = Vec::new();
    let mut style = S::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push((c, style));
            continue;
        }
        if chars.peek() != Some(&'[') {
            continue;
        }
        chars.next();
        let mut params = String::new();
        let mut last = ' ';
        for c in chars.by_ref() {
            if ('@'..='~').contains(&c) {
                last = c;
                break;
            }
            params.push(c);
        }
        if last != 'm' {
            continue;
        }
        let ps: Vec<u8> = params.split(';').map(|p| p.parse().unwrap_or(0)).collect();
        let mut ps = ps.into_iter();
        while let Some(p) = ps.next() {
            // the color after 38 or 48: 5 and an index, or 2 and its red,
            // green and blue
            let extended = |ps: &mut std::vec::IntoIter<u8>| match ps.next() {
                Some(5) => C::Indexed(ps.next().unwrap_or(0)),
                Some(2) => C::Rgb(
                    ps.next().unwrap_or(0),
                    ps.next().unwrap_or(0),
                    ps.next().unwrap_or(0),
                ),
                _ => C::Reset,
            };
            style = match p {
                0 => S::new(),
                1 => style.add_modifier(M::BOLD),
                3 => style.add_modifier(M::ITALIC),
                4 => style.add_modifier(M::UNDERLINED),
                7 => style.add_modifier(M::REVERSED),
                30..=37 => style.fg(C::Indexed(p.saturating_sub(30))),
                90..=97 => style.fg(C::Indexed(p.saturating_sub(82))),
                40..=47 => style.bg(C::Indexed(p.saturating_sub(40))),
                100..=107 => style.bg(C::Indexed(p.saturating_sub(92))),
                38 => style.fg(extended(&mut ps)),
                48 => style.bg(extended(&mut ps)),
                39 => style.fg(C::Reset),
                49 => style.bg(C::Reset),
                _ => style,
            };
        }
    }
    out
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
    fn a_style_written_with_named_colors_reads_back_as_itself_without_its_24_bit_colors() {
        let fill = Back {
            basic: Normal::Blue,
            rgb: Some([0x0f, 0x4c, 0x57]),
        };
        let styles = [
            Style::PLAIN,
            Style::block(Color::BrightCyan).reversed(),
            Style {
                rgb: Some([1, 2, 3]),
                bold: true,
                italic: true,
                underline: true,
                ..Style::text(Color::Grey)
            },
            Style {
                back: Some(fill),
                script: vtium_font::Script::Sub,
                ..Style::text(Color::BrightWhite)
            },
            Style {
                script: vtium_font::Script::Super,
                ..Style::PLAIN
            },
        ];
        for s in styles {
            let back = Style::of_cell(s.in_cells(Colors::Named));
            let want = Style {
                rgb: None,
                back: s.back.map(|b| Back { rgb: None, ..b }),
                ..s
            };
            assert_eq!(back, want, "{s:?}");
        }
        for c in ALL {
            assert_eq!(Color::of_named(c.named()), Some(c));
        }
        // a style with only a 24-bit color keeps it either way
        let only = Style {
            rgb: Some([9, 8, 7]),
            ..Style::PLAIN
        };
        assert_eq!(Style::of_cell(only.in_cells(Colors::Named)), only);
    }

    #[test]
    fn the_terminal_is_sent_the_24_bit_colors_where_a_style_has_them() {
        let s = Style {
            rgb: Some([1, 2, 3]),
            back: Some(Back {
                basic: Normal::Blue,
                rgb: Some([4, 5, 6]),
            }),
            ..Style::text(Color::Grey)
        };
        assert_eq!(sent(s, true), "\x1b[38;2;1;2;3;48;2;4;5;6m");
        let named = Style {
            rgb: None,
            back: Some(Back {
                basic: Normal::Blue,
                rgb: None,
            }),
            ..s
        };
        assert_eq!(sent(named, true), "\x1b[90;44m");
    }

    #[test]
    fn a_raw_span_s_colors_reach_ratatui_s_cells() {
        use ratatui_core::style::Color as C;
        let cells = sgr_cells("\x1b[38;2;1;2;3mA\x1b[48;5;200mB\x1b[0mC\x1b[95mD");
        let got: Vec<(char, C, C)> = cells
            .iter()
            .map(|(c, s)| (*c, s.fg.unwrap_or(C::Reset), s.bg.unwrap_or(C::Reset)))
            .collect();
        assert_eq!(
            got,
            [
                ('A', C::Rgb(1, 2, 3), C::Reset),
                ('B', C::Rgb(1, 2, 3), C::Indexed(200)),
                ('C', C::Reset, C::Reset),
                ('D', C::Indexed(13), C::Reset)
            ]
        );
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
