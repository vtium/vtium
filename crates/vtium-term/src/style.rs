//! What a cell looks like: its colors and attributes, kept unresolved.
//!
//! A color stays a reference (the default, a palette index or RGB) until
//! a frame resolves it through the palette at draw time, so a palette
//! change recolors text already on the screen and in the scrollback.

/// An RGB color, 8 bits a channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// A color as a program set it: resolved only when a frame is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    /// The default foreground or background, or for an underline, the
    /// color the configuration gives it (the foreground unless set).
    #[default]
    Default,
    /// An entry of the 256-color palette.
    Indexed(u8),
    /// A direct color.
    Rgb(Rgb),
}

/// The underline styles of SGR 4 and its sub-parameters, and SGR 21.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UnderlineStyle {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// Superscript and subscript, SGR 73 and 74, reset by 75.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Script {
    #[default]
    Baseline,
    Super,
    Sub,
}

/// A cell's attributes in 16 bits: ten flags, the underline style in
/// three bits and the script in two, with one bit spare.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attrs(u16);

impl Attrs {
    /// SGR 1.
    pub const BOLD: Self = Self(1 << 0);
    /// SGR 2.
    pub const DIM: Self = Self(1 << 1);
    /// SGR 3.
    pub const ITALIC: Self = Self(1 << 2);
    /// SGR 5.
    pub const BLINK: Self = Self(1 << 3);
    /// SGR 6.
    pub const RAPID_BLINK: Self = Self(1 << 4);
    /// SGR 7.
    pub const INVERSE: Self = Self(1 << 5);
    /// SGR 8: drawn as the background, its text kept for selection.
    pub const CONCEAL: Self = Self(1 << 6);
    /// SGR 9.
    pub const STRIKE: Self = Self(1 << 7);
    /// SGR 53.
    pub const OVERLINE: Self = Self(1 << 8);
    /// DECSCA: selective erase (DECSED, DECSEL) leaves the cell alone.
    pub const PROTECTED: Self = Self(1 << 9);

    const FLAGS: u16 = (1 << 10) - 1;
    const UNDERLINE_SHIFT: u16 = 10;
    const UNDERLINE_MASK: u16 = 0b111 << Self::UNDERLINE_SHIFT;
    const SCRIPT_SHIFT: u16 = 13;
    const SCRIPT_MASK: u16 = 0b11 << Self::SCRIPT_SHIFT;

    pub const fn empty() -> Self {
        Self(0)
    }

    /// Whether every flag of `other` is set. The underline style and the
    /// script are not flags and are compared with their own accessors.
    pub const fn contains(self, other: Self) -> bool {
        let flags = other.0 & Self::FLAGS;
        self.0 & flags == flags
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0 & Self::FLAGS;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !(other.0 & Self::FLAGS);
    }

    pub const fn underline(self) -> UnderlineStyle {
        match (self.0 & Self::UNDERLINE_MASK) >> Self::UNDERLINE_SHIFT {
            0 => UnderlineStyle::None,
            1 => UnderlineStyle::Single,
            2 => UnderlineStyle::Double,
            3 => UnderlineStyle::Curly,
            4 => UnderlineStyle::Dotted,
            _ => UnderlineStyle::Dashed,
        }
    }

    pub fn set_underline(&mut self, style: UnderlineStyle) {
        let bits: u16 = match style {
            UnderlineStyle::None => 0,
            UnderlineStyle::Single => 1,
            UnderlineStyle::Double => 2,
            UnderlineStyle::Curly => 3,
            UnderlineStyle::Dotted => 4,
            UnderlineStyle::Dashed => 5,
        };
        self.0 = (self.0 & !Self::UNDERLINE_MASK) | (bits << Self::UNDERLINE_SHIFT);
    }

    pub const fn script(self) -> Script {
        match (self.0 & Self::SCRIPT_MASK) >> Self::SCRIPT_SHIFT {
            0 => Script::Baseline,
            1 => Script::Super,
            _ => Script::Sub,
        }
    }

    pub fn set_script(&mut self, script: Script) {
        let bits: u16 = match script {
            Script::Baseline => 0,
            Script::Super => 1,
            Script::Sub => 2,
        };
        self.0 = (self.0 & !Self::SCRIPT_MASK) | (bits << Self::SCRIPT_SHIFT);
    }
}

/// Everything SGR and DECSCA set on a cell. A line keeps the styles its
/// cells use in its own table, and each cell names one by index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    /// SGR 58 and 59. `Color::Default` follows the foreground, or the
    /// configured underline color.
    pub underline_color: Color,
    pub attrs: Attrs,
}

impl Style {
    /// The style of a cell no program has styled.
    pub const DEFAULT: Self = Self {
        fg: Color::Default,
        bg: Color::Default,
        underline_color: Color::Default,
        attrs: Attrs::empty(),
    };
}

const _: () = assert!(size_of::<Color>() == 4);
const _: () = assert!(size_of::<Style>() == 14);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_underline_and_script_are_independent() {
        let mut a = Attrs::empty();
        a.insert(Attrs::BOLD);
        a.insert(Attrs::OVERLINE);
        a.insert(Attrs::CONCEAL);
        a.set_underline(UnderlineStyle::Curly);
        a.set_script(Script::Sub);
        assert!(a.contains(Attrs::BOLD));
        assert!(a.contains(Attrs::OVERLINE));
        assert!(a.contains(Attrs::CONCEAL));
        assert!(!a.contains(Attrs::ITALIC));
        assert_eq!(a.underline(), UnderlineStyle::Curly);
        assert_eq!(a.script(), Script::Sub);

        a.set_underline(UnderlineStyle::None);
        a.remove(Attrs::BOLD);
        assert_eq!(a.underline(), UnderlineStyle::None);
        assert_eq!(a.script(), Script::Sub);
        assert!(!a.contains(Attrs::BOLD));
        assert!(a.contains(Attrs::OVERLINE));
    }

    #[test]
    fn every_underline_style_and_script_round_trips() {
        for u in [
            UnderlineStyle::None,
            UnderlineStyle::Single,
            UnderlineStyle::Double,
            UnderlineStyle::Curly,
            UnderlineStyle::Dotted,
            UnderlineStyle::Dashed,
        ] {
            for s in [Script::Baseline, Script::Super, Script::Sub] {
                let mut a = Attrs::empty();
                a.insert(Attrs::PROTECTED);
                a.set_underline(u);
                a.set_script(s);
                assert_eq!((a.underline(), a.script()), (u, s));
                assert!(a.contains(Attrs::PROTECTED));
            }
        }
    }

    #[test]
    fn the_default_style_is_all_defaults() {
        assert_eq!(Style::DEFAULT, Style::default());
    }
}
