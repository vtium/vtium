//! The combining marks of every script: the characters Unicode's
//! General_Category calls Mn (nonspacing), Mc (spacing) or Me
//! (enclosing), less the default ignorables, which draw as nothing.
//!
//! The console and the font helper both ask this module whether a
//! character is a combining mark. A combining mark uses a row made taller
//! by line spacing as `room` does when the frit gives its set no `fit=`
//! ([`crate::spacing::Fit::default_for`]), and the console draws a mark
//! over or beside its base when it builds a cluster from its parts. One
//! table for both programs means the helper and the console always agree
//! on how each character fits its row.
//!
//! `data.rs` holds the table as ranges, generated from the Unicode
//! Character Database by vtium's `gen-lookup` with Unicode's licence
//! notice.
//!
//! References:
//! - ADR-1302 D13: combining marks are `room` by default.
//! - ADR-1200 D22: the console draws a cluster from its base and marks.

mod data;

/// Whether `c` is a combining mark: General_Category Mn, Mc or Me, and
/// not a default ignorable.
pub fn is_mark(c: u32) -> bool {
    let i = data::MARKS.partition_point(|&(_, last)| last < c);
    data::MARKS.get(i).is_some_and(|&(first, _)| first <= c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_its_ranges_do_not_touch() {
        for pair in data::MARKS.windows(2) {
            let [(a, b), (c, _)] = pair else { continue };
            assert!(a <= b, "{a:#x}..{b:#x}");
            assert!(b.saturating_add(1) < *c, "{b:#x} then {c:#x}");
        }
    }

    #[test]
    fn nonspacing_spacing_and_enclosing_marks_are_marks() {
        // U+0301 COMBINING ACUTE ACCENT is Mn, U+0903 DEVANAGARI SIGN
        // VISARGA and U+093E DEVANAGARI VOWEL SIGN AA are Mc, and U+20DD
        // COMBINING ENCLOSING CIRCLE is Me
        for c in [0x0301, 0x0903, 0x093e, 0x20dd, 0x064e] {
            assert!(is_mark(c), "{c:#x}");
        }
    }

    #[test]
    fn letters_and_default_ignorables_are_not_marks() {
        // U+034F COMBINING GRAPHEME JOINER and the variation selectors
        // U+FE00 to U+FE0F are Mn but default ignorable, so they draw as
        // nothing and are not marks here
        for c in [
            0x41, 0x0915, 0x0628, 0x034f, 0xfe00, 0xfe0f, 0x200d, 0xe0100,
        ] {
            assert!(!is_mark(c), "{c:#x}");
        }
    }
}
