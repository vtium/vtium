//! The glyph variant a cell's attributes ask for: bold, italic, and the
//! superscript or subscript of SGR 73 and 74, which SGR 75 ends. vtium-font
//! draws the variant ([`vtium_font::Variant`]), and the glyph cache keeps
//! each variant of a text apart.

use vtium_font::{Script, Style, Variant};
use vtium_term::{Attrs, Script as Position};

/// The variant `attrs` draws its glyph in.
pub(crate) fn of(attrs: Attrs) -> Variant {
    Variant {
        style: Style {
            bold: attrs.contains(Attrs::BOLD),
            italic: attrs.contains(Attrs::ITALIC),
        },
        script: match attrs.script() {
            Position::Baseline => Script::Baseline,
            Position::Super => Script::Super,
            Position::Sub => Script::Sub,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // covers: sgr.script
    #[test]
    fn each_script_attribute_picks_its_variant() {
        let mut a = Attrs::BOLD;
        assert_eq!(of(a).script, Script::Baseline);
        a.set_script(Position::Super);
        assert_eq!(of(a).script, Script::Super);
        assert!(of(a).style.bold && !of(a).style.italic);
        a.set_script(Position::Sub);
        assert_eq!(of(a).script, Script::Sub);
        a.set_script(Position::Baseline);
        assert_eq!(of(a), of(Attrs::BOLD));
    }
}
