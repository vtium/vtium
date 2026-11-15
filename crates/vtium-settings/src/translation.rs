//! Which wording of a message vtium shows: its translation, or its
//! English.
//!
//! vtium draws its own messages in its built-in font, which holds a short
//! list of characters. A translation that uses a character outside that
//! list would draw part of its words as hollow boxes, which tells the
//! reader nothing. So vtium shows a translated message only when the
//! built-in font holds every character of it, and the English message
//! otherwise. vtium's English uses only characters the font holds, so a
//! message always reads.
//!
//! References:
//! - ADR-1204 D1: a translation beyond the built-in font's list shows in
//!   English.
//! - ADR-1402 D9: the test is the built-in font, the font every head has
//!   from its first frame.

use vtium_font::set::own::holds_all;

/// The wording of a message vtium shows: `translated`, its translation,
/// where the built-in font holds every character of it, and else
/// `english`.
pub fn shown<'a>(english: &'a str, translated: &'a str) -> &'a str {
    if holds_all(translated) {
        translated
    } else {
        english
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGLISH: &str = "No keyboard was found.";

    /// A translation in Latin-1 shows as it is.
    #[test]
    fn a_translation_the_built_in_font_holds_shows() {
        let german = "Keine Tastatur gefunden. Schließen Sie eine an.";
        assert_eq!(shown(ENGLISH, german), german);
    }

    /// A translation with a character outside the built-in font's list,
    /// such as Han or Cyrillic, shows its English instead.
    #[test]
    fn a_translation_beyond_the_built_in_font_shows_in_english() {
        assert_eq!(shown(ENGLISH, "未找到键盘。"), ENGLISH);
        assert_eq!(shown(ENGLISH, "Клавиатура не найдена."), ENGLISH);
        assert_eq!(shown(ENGLISH, "Keine Tastatur „gefunden“."), ENGLISH);
    }
}
