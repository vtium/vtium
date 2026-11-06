//! The Unicode data of right-to-left text (ADR-1004), in a build with the
//! `bidi` feature: each character's Bidi_Class and Joining_Type, the glyph a
//! mirrored character draws, the left-right counterpart of a box-drawing
//! character, which sides of each cell in a run join its neighbours, and a
//! paragraph's direction from its first strong character.
//!
//! The tables come from the Unicode Character Database at the version
//! [`crate::UNICODE_VERSION`] names, generated into `bidi/data.rs` by
//! `gen-unicode`. `levels` is the only module that calls the unicode-bidi
//! crate, and it passes the crate these tables.

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only a test reads the tables' version")
)]
mod data;
pub(crate) mod levels;

/// A paragraph's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    LeftToRight,
    RightToLeft,
}

/// A character's Bidi_Class, by the short names of UAX #9's table of
/// bidirectional character types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    L,
    R,
    Al,
    En,
    Es,
    Et,
    An,
    Cs,
    Nsm,
    Bn,
    B,
    S,
    Ws,
    On,
    Lre,
    Lro,
    Rle,
    Rlo,
    Pdf,
    Lri,
    Rli,
    Fsi,
    Pdi,
}

/// A character's Joining_Type (the Unicode Standard, section 9.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JoiningType {
    /// U: joins neither neighbour, as every character no other value names.
    NonJoining,
    /// D: joins the character before it and the one after it.
    Dual,
    /// R: joins only the character before it in logical order.
    Right,
    /// L: joins only the character after it in logical order.
    Left,
    /// C: makes its neighbours join it, as the tatweel does.
    Causing,
    /// T: a mark, which the letters on either side join across.
    Transparent,
}

/// The run word of the run that holds `c`.
fn run(c: char) -> u32 {
    let cp = u32::from(c);
    let index = data::RUNS
        .partition_point(|&word| word >> data::START_SHIFT <= cp)
        .saturating_sub(1);
    // the first run starts at U+0000, so every character is in one
    data::RUNS.get(index).copied().unwrap_or(0)
}

/// The Bidi_Class of `c`.
pub(crate) fn class(c: char) -> Class {
    match run(c) & data::CLASS {
        data::CLASS_R => Class::R,
        data::CLASS_AL => Class::Al,
        data::CLASS_EN => Class::En,
        data::CLASS_ES => Class::Es,
        data::CLASS_ET => Class::Et,
        data::CLASS_AN => Class::An,
        data::CLASS_CS => Class::Cs,
        data::CLASS_NSM => Class::Nsm,
        data::CLASS_BN => Class::Bn,
        data::CLASS_B => Class::B,
        data::CLASS_S => Class::S,
        data::CLASS_WS => Class::Ws,
        data::CLASS_ON => Class::On,
        data::CLASS_LRE => Class::Lre,
        data::CLASS_LRO => Class::Lro,
        data::CLASS_RLE => Class::Rle,
        data::CLASS_RLO => Class::Rlo,
        data::CLASS_PDF => Class::Pdf,
        data::CLASS_LRI => Class::Lri,
        data::CLASS_RLI => Class::Rli,
        data::CLASS_FSI => Class::Fsi,
        data::CLASS_PDI => Class::Pdi,
        data::CLASS_L => Class::L,
        // the generator writes no other code
        _ => Class::L,
    }
}

/// The Joining_Type of `c`.
pub(crate) fn joining(c: char) -> JoiningType {
    match run(c) & data::JOINING {
        data::JOINING_D => JoiningType::Dual,
        data::JOINING_R => JoiningType::Right,
        data::JOINING_L => JoiningType::Left,
        data::JOINING_C => JoiningType::Causing,
        data::JOINING_T => JoiningType::Transparent,
        data::JOINING_U => JoiningType::NonJoining,
        // the generator writes no other code
        _ => JoiningType::NonJoining,
    }
}

/// The value `pairs` gives `c`, from a table in increasing order of its
/// first column.
fn pair(pairs: &[(char, char)], c: char) -> Option<char> {
    pairs
        .binary_search_by_key(&c, |&(from, _)| from)
        .ok()
        .and_then(|i| pairs.get(i))
        .map(|&(_, to)| to)
}

/// The glyph `c` draws at an odd level by rule L4: its Bidi_Mirroring_Glyph,
/// or `None` for a character with none, which draws as it is.
pub(crate) fn mirrored(c: char) -> Option<char> {
    pair(&data::MIRRORS, c)
}

/// The box-drawing character (U+2500 to U+257F) whose Unicode name swaps
/// LEFT and RIGHT in the name of `c`, which a right-to-left paragraph draws
/// in place of `c` while mode 2500 is set (ADR-1004 D7); `None` for a
/// character with no such partner, which draws as it is.
pub(crate) fn box_mirrored(c: char) -> Option<char> {
    pair(&data::BOX_MIRRORS, c)
}

/// Whether `c` can move text or change a letter's form when a paragraph is
/// drawn: a character of class R, AL or AN, which the bidirectional
/// algorithm reorders, or one that joins a neighbour (Joining_Type D, R, L
/// or C). A line that has held one is drawn through its paragraph's order
/// (ADR-1004 D8). Every character below U+0590, the start of the Hebrew
/// block, is neither.
pub(crate) fn reorders(c: char) -> bool {
    if c < '\u{0590}' {
        return false;
    }
    matches!(class(c), Class::R | Class::Al | Class::An)
        || matches!(
            joining(c),
            JoiningType::Dual | JoiningType::Right | JoiningType::Left | JoiningType::Causing
        )
}

/// If `c` is a paired bracket, the opening bracket of its pair after
/// canonical decomposition and whether `c` opens, as rule BD16 matches
/// brackets.
fn bracket(c: char) -> Option<(char, bool)> {
    let i = data::BRACKETS
        .binary_search_by_key(&c, |&(b, _, _)| b)
        .ok()?;
    data::BRACKETS
        .get(i)
        .map(|&(_, opening, opens)| (opening, opens))
}

/// The direction of a paragraph's first strong character by rules P2 and
/// P3: the first character of class L, R or AL that no isolate holds, or
/// `None` when it has none. An isolate runs from LRI, RLI or FSI to its
/// matching PDI, or to the paragraph's end when no PDI matches it.
pub(crate) fn first_strong(text: &[char]) -> Option<Direction> {
    let mut isolates: usize = 0;
    for &c in text {
        match class(c) {
            Class::Lri | Class::Rli | Class::Fsi => isolates = isolates.saturating_add(1),
            Class::Pdi => isolates = isolates.saturating_sub(1),
            Class::L if isolates == 0 => return Some(Direction::LeftToRight),
            Class::R | Class::Al if isolates == 0 => return Some(Direction::RightToLeft),
            _ => {}
        }
    }
    None
}

/// Which sides of a cell join its neighbours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Joins {
    /// It joins the character before it in the order the run was given.
    pub(crate) before: bool,
    /// It joins the character after it.
    pub(crate) after: bool,
}

/// Whether a character of type `t` joins the one before it, when that one
/// reaches forward.
fn reaches_back(t: JoiningType) -> bool {
    matches!(
        t,
        JoiningType::Dual | JoiningType::Right | JoiningType::Causing
    )
}

/// Whether a character of type `t` joins the one after it, when that one
/// reaches back.
fn reaches_forward(t: JoiningType) -> bool {
    matches!(
        t,
        JoiningType::Dual | JoiningType::Left | JoiningType::Causing
    )
}

/// The joining sides of each character of `text`, one character a cell, in
/// the order the letters are read: logical order in implicit mode, and the
/// order drawn from right to left in explicit mode (ADR-1004 D6). Two
/// characters join when the first reaches forward (type D, L or C) and the
/// second reaches back (type D, R or C), with only marks (type T) between
/// them; a mark itself joins neither side.
pub(crate) fn joins(text: &[char]) -> Vec<Joins> {
    let mut out = vec![Joins::default(); text.len()];
    // the last character before this one that is not a mark: its index and
    // whether it reaches forward
    let mut last: Option<(usize, bool)> = None;
    for (i, &c) in text.iter().enumerate() {
        let t = joining(c);
        if t == JoiningType::Transparent {
            continue;
        }
        if let Some((before, true)) = last
            && reaches_back(t)
        {
            if let Some(j) = out.get_mut(before) {
                j.after = true;
            }
            if let Some(j) = out.get_mut(i) {
                j.before = true;
            }
        }
        last = Some((i, reaches_forward(t)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_are_at_the_crates_unicode_version() {
        assert_eq!(data::VERSION, crate::UNICODE_VERSION);
    }

    #[test]
    fn classes_come_from_the_tables_and_their_defaults() {
        assert_eq!(class('a'), Class::L);
        assert_eq!(class('\u{05D0}'), Class::R);
        assert_eq!(class('\u{0627}'), Class::Al);
        assert_eq!(class('1'), Class::En);
        assert_eq!(class('\u{0660}'), Class::An);
        assert_eq!(class('('), Class::On);
        assert_eq!(class(' '), Class::Ws);
        assert_eq!(class('\u{2067}'), Class::Rli);
        // unassigned: the @missing ranges of Hebrew, Arabic and the
        // currency symbols, and the default
        assert_eq!(class('\u{05FF}'), Class::R);
        assert_eq!(class('\u{07BF}'), Class::Al);
        assert_eq!(class('\u{20CF}'), Class::Et);
        assert_eq!(class('\u{10FFFF}'), Class::Bn);
        assert_eq!(class('\u{E0080}'), Class::Bn);
        assert_eq!(class('\u{3FFFD}'), Class::L);
    }

    #[test]
    fn joining_types_come_from_the_tables() {
        assert_eq!(joining('\u{0628}'), JoiningType::Dual); // beh
        assert_eq!(joining('\u{0627}'), JoiningType::Right); // alef
        assert_eq!(joining('\u{0640}'), JoiningType::Causing); // tatweel
        assert_eq!(joining('\u{064E}'), JoiningType::Transparent); // fatha
        assert_eq!(joining('a'), JoiningType::NonJoining);
        // the presentation forms, shaped already, join nothing
        assert_eq!(joining('\u{FE91}'), JoiningType::NonJoining);
    }

    #[test]
    fn right_to_left_and_joining_characters_reorder() {
        assert!(reorders('\u{05D0}')); // alef, R
        assert!(reorders('\u{0627}')); // alef, AL
        assert!(reorders('\u{0660}')); // Arabic-Indic zero, AN
        assert!(reorders('\u{1820}')); // a Mongolian letter: L, but it joins
        assert!(reorders('\u{200D}')); // ZWJ joins (type C)
        assert!(!reorders('a'));
        assert!(!reorders('('));
        assert!(!reorders('\u{0400}'));
        assert!(!reorders('\u{064E}')); // a mark, type T, NSM
        // nothing below U+0590 reorders, so the quick path is exact
        assert!((0..0x0590).filter_map(char::from_u32).all(|c| {
            !matches!(class(c), Class::R | Class::Al | Class::An)
                && matches!(
                    joining(c),
                    JoiningType::NonJoining | JoiningType::Transparent
                )
        }));
    }

    #[test]
    fn mirrored_characters_draw_their_counterparts() {
        assert_eq!(mirrored('('), Some(')'));
        assert_eq!(mirrored(')'), Some('('));
        assert_eq!(mirrored('<'), Some('>'));
        assert_eq!(mirrored('\u{226E}'), Some('\u{226F}'));
        assert_eq!(mirrored('a'), None);
        // U+2201 COMPLEMENT is Bidi_Mirrored with no counterpart
        assert_eq!(mirrored('\u{2201}'), None);
        // every pair of BidiMirroring.txt 18.0.0 runs both ways
        for &(a, b) in &data::MIRRORS {
            assert_eq!(mirrored(b), Some(a), "{a:?} and {b:?} mirror each other");
        }
    }

    #[test]
    fn box_drawing_pairs_swap_left_and_right() {
        assert_eq!(box_mirrored('\u{250C}'), Some('\u{2510}'));
        assert_eq!(box_mirrored('\u{2510}'), Some('\u{250C}'));
        assert_eq!(box_mirrored('\u{251C}'), Some('\u{2524}'));
        assert_eq!(box_mirrored('\u{2571}'), Some('\u{2572}'));
        assert_eq!(box_mirrored('\u{257C}'), Some('\u{257E}'));
        // no LEFT or RIGHT in its name
        assert_eq!(box_mirrored('\u{2500}'), None);
        assert_eq!(box_mirrored('\u{253C}'), None);
        for &(a, b) in &data::BOX_MIRRORS {
            assert!(('\u{2500}'..='\u{257F}').contains(&a));
            assert_eq!(box_mirrored(b), Some(a), "{a:?} and {b:?} pair both ways");
        }
    }

    #[test]
    fn brackets_pair_by_their_canonical_equivalents() {
        assert_eq!(bracket('('), Some(('(', true)));
        assert_eq!(bracket(')'), Some(('(', false)));
        assert_eq!(bracket('\u{2329}'), Some(('\u{3008}', true)));
        assert_eq!(bracket('\u{3009}'), Some(('\u{3008}', false)));
        assert_eq!(bracket('a'), None);
    }

    #[test]
    fn the_first_strong_character_outside_an_isolate_sets_the_direction() {
        let s = |t: &str| t.chars().collect::<Vec<_>>();
        assert_eq!(
            first_strong(&s("12 \u{05D0}b")),
            Some(Direction::RightToLeft)
        );
        assert_eq!(first_strong(&s("(\u{0627})")), Some(Direction::RightToLeft));
        assert_eq!(
            first_strong(&s("1 a \u{05D0}")),
            Some(Direction::LeftToRight)
        );
        // an embedding is no isolate, so its contents count
        assert_eq!(first_strong(&s("\u{202B}a")), Some(Direction::LeftToRight));
        // an isolate's contents do not, up to its PDI or the end
        assert_eq!(
            first_strong(&s("\u{2067}a\u{2069}\u{05D0}")),
            Some(Direction::RightToLeft)
        );
        assert_eq!(first_strong(&s("\u{2068}a \u{05D0}")), None);
        assert_eq!(first_strong(&s("12 ()")), None);
        assert_eq!(first_strong(&[]), None);
    }

    fn sides(text: &str) -> Vec<(bool, bool)> {
        let text: Vec<char> = text.chars().collect();
        joins(&text).iter().map(|j| (j.before, j.after)).collect()
    }

    #[test]
    fn arabic_letters_join_by_their_joining_types() {
        // beh beh alef beh: the alef joins only the letter before it
        assert_eq!(
            sides("\u{0628}\u{0628}\u{0627}\u{0628}"),
            [(false, true), (true, true), (true, false), (false, false)]
        );
        // a mark between two letters is skipped and joins nothing itself
        assert_eq!(
            sides("\u{0628}\u{064E}\u{0628}"),
            [(false, true), (false, false), (true, false)]
        );
        // a tatweel joins both sides; a space or a Latin letter breaks a join
        assert_eq!(
            sides("\u{0628}\u{0640}\u{0628} \u{0628}a\u{0628}"),
            [
                (false, true),
                (true, true),
                (true, false),
                (false, false),
                (false, false),
                (false, false),
                (false, false),
            ]
        );
        // a presentation form, shaped by the program, joins nothing
        assert_eq!(sides("\u{0628}\u{FE91}"), [(false, false), (false, false)]);
        assert_eq!(sides(""), []);
    }
}
