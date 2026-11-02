//! Shaping (vtium/vtium#144, ADR-1200 D21): the clusters the console asks
//! the helper for, which the engine shapes with HarfRust and draws whole,
//! one glyph fitted to its 1 or 2 cells (ADR-1301 D1).
//!
//! A cluster is text a program wrote to the terminal, so the shaper and
//! the font parser behind it face input anyone with the terminal chooses:
//! they run only in the helper, under its sandbox (ADR-1200 D14, D15), and
//! `fuzz/` drives clusters through them. [`kind`] judges each cluster by
//! what it holds: an emoji sequence must shape to one glyph that is no
//! scalar value's own, and a letter with its marks to glyphs the font
//! holds. The tables in [`data`] come from the Unicode Character Database
//! through `gen-shaping/`.

pub mod data;
pub(crate) mod shape;

/// How a cluster is judged and placed once shaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// An emoji sequence: kept only when the font shapes it to one glyph
    /// that is no scalar value's own, placed two cells wide.
    Emoji = 1,
    /// A letter and its marks: kept when the font holds every glyph it
    /// shapes to, one or two cells wide by its advance.
    Marks = 2,
    /// A joining form: a letter keyed with U+200D on each side it joins,
    /// or lam and alef as their ligature, with any marks on it. Kept only
    /// when the font shapes its letters to one glyph that none of its
    /// scalar values draws alone, and holds the glyph of each mark, placed
    /// a cell wide for each letter.
    Joining = 3,
}

impl Kind {
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Option<Kind> {
        [Kind::Emoji, Kind::Marks, Kind::Joining]
            .into_iter()
            .find(|k| k.code() == code)
    }
}

/// A cluster a source is asked to shape: its scalar values, and how it is
/// judged.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ask {
    pub key: Vec<u32>,
    pub kind: Kind,
}

/// The shaper and the Unicode data the clusters are judged by, for a
/// set's provenance.
pub const PROVENANCE: &str = "harfrust 0.12.0, Unicode 18.0.0";

/// ZERO WIDTH JOINER.
pub(crate) const ZWJ: u32 = 0x200d;
/// VARIATION SELECTOR-16, emoji presentation.
const VS16: u32 = 0xfe0f;

/// A regional indicator, half of a flag.
const REGIONAL: std::ops::RangeInclusive<u32> = 0x1f1e6..=0x1f1ff;
/// The emoji modifiers, the skin tones.
const MODIFIERS: std::ops::RangeInclusive<u32> = 0x1f3fb..=0x1f3ff;
/// The tag characters of an emoji tag sequence.
const TAGS: std::ops::RangeInclusive<u32> = 0xe0020..=0xe007f;
/// COMBINING ENCLOSING KEYCAP.
const KEYCAP: u32 = 0x20e3;

/// How the cluster `key` is judged and placed: a joining form when it
/// holds U+200D and a letter, and every letter joins ([`joins`]), as a
/// letter keyed with the joiner on the side it joins; the marks on a
/// letter, such as a fatha, are not letters here ([`mark_side`]). Else an
/// emoji sequence when Unicode lists it as one or it holds U+200D, U+FE0F,
/// a regional indicator, an emoji modifier, a tag or the keycap mark; else
/// a letter and its marks.
pub fn kind(key: &[u32]) -> Kind {
    let letters = key.iter().filter(|&&c| c != ZWJ && mark_side(c).is_none());
    if key.contains(&ZWJ) && letters.clone().count() > 0 && letters.clone().all(|&c| joins(c)) {
        return Kind::Joining;
    }
    let emoji = key.iter().any(|c| {
        matches!(*c, ZWJ | VS16 | KEYCAP)
            || REGIONAL.contains(c)
            || MODIFIERS.contains(c)
            || TAGS.contains(c)
    });
    if emoji || data::EMOJI.binary_search(&key).is_ok() {
        Kind::Emoji
    } else {
        Kind::Marks
    }
}

/// Whether `c` joins to its neighbours: Joining_Type D, R, L or C in any
/// script (ADR-1302 D2), the characters whose outline glyphs are stretched
/// to their cells. U+200D, which is join-causing, draws nothing.
pub fn joins(c: u32) -> bool {
    let after = data::JOINS.partition_point(|&(a, _)| a <= c);
    after
        .checked_sub(1)
        .and_then(|i| data::JOINS.get(i))
        .is_some_and(|&(_, b)| c <= b)
}

/// Where `c` goes on its base when it is a mark (ADR-1302 D3): a character
/// of General_Category Mn or Me, or of a combining class other than 0; 1
/// above, 2 below, by its class, else 0. `None` for a character that is no
/// mark.
pub fn mark_side(c: u32) -> Option<u8> {
    let after = data::MARKS.partition_point(|&(a, _, _)| a <= c);
    after
        .checked_sub(1)
        .and_then(|i| data::MARKS.get(i))
        .filter(|&&(_, b, _)| c <= b)
        .map(|&(_, _, side)| side)
}

/// `key` as the engine asks a source for it.
pub fn ask(key: &[u32]) -> Ask {
    Ask {
        key: key.to_vec(),
        kind: kind(key),
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    // ADR-1302 D2 names every joining script: Arabic and its tatweel,
    // Syriac, N'Ko, Mongolian, Mandaic, Adlam and the rest, of Joining_Type
    // D, R, L or C; a letter that does not join and a mark do not
    // a letter keyed with U+200D is a joining form, not an emoji sequence
    // (review finding B4); a ZWJ sequence of emoji, or a letter that does
    // not join beside a joiner, is judged as before
    #[test]
    fn a_letter_with_a_joiner_is_a_joining_form() {
        assert_eq!(kind(&[0x628, ZWJ]), Kind::Joining);
        assert_eq!(kind(&[ZWJ, 0x628, ZWJ]), Kind::Joining);
        assert_eq!(kind(&[ZWJ, 0x7ca]), Kind::Joining, "N'Ko");
        assert_eq!(kind(&[0x1f468, ZWJ, 0x1f469]), Kind::Emoji);
        assert_eq!(kind(&[0x41, ZWJ]), Kind::Emoji);
        assert_eq!(kind(&[0x628, 0x64e]), Kind::Marks);
        // a mark on the letter, of Joining_Type T, leaves it a joining
        // form in either order (review finding 2); a joiner with marks
        // alone has no letter to join
        assert_eq!(kind(&[0x628, 0x64e, ZWJ]), Kind::Joining);
        assert_eq!(kind(&[ZWJ, 0x628, 0x64e]), Kind::Joining);
        assert_eq!(kind(&[ZWJ, 0x628, 0x651, 0x64e, ZWJ]), Kind::Joining);
        assert_ne!(kind(&[ZWJ, 0x64e]), Kind::Joining);
    }

    #[test]
    fn the_letters_of_every_joining_script_join() {
        for (c, what) in [
            (0x628, "Arabic beh, dual-joining"),
            (0x627, "Arabic alef, right-joining"),
            (0x640, "the tatweel, join-causing"),
            (0x710, "Syriac alaph"),
            (0x7ca, "N'Ko a"),
            (0x1820, "Mongolian a"),
            (0x841, "Mandaic ab"),
            (0x1e900, "Adlam capital alif"),
            (0xa840, "Phags-pa ka"),
            (0xa872, "Phags-pa superfixed ra, left-joining"),
        ] {
            assert!(joins(c), "{what}");
        }
        for c in [0x41, 0x5d0, 0x64e, 0x300] {
            assert!(!joins(c), "{c:#x}");
        }
    }

    // PROVENANCE names the HarfRust the manifest pins and the Unicode the
    // tables were generated from
    #[test]
    fn the_provenance_is_what_is_built() {
        let manifest = include_str!("../../Cargo.toml");
        let data = include_str!("data.rs");
        let (shaper, unicode) = PROVENANCE.split_once(", Unicode ").unwrap();
        let version = shaper.strip_prefix("harfrust ").unwrap();
        assert!(manifest.contains(&format!("harfrust = {{ version = \"={version}\"")));
        assert!(data.contains(&format!("Database {unicode}.")));
    }

    #[test]
    fn a_cluster_is_judged_by_what_it_holds() {
        let emoji: [&[u32]; 5] = [
            &[0x1f468, ZWJ, 0x1f469, ZWJ, 0x1f467],
            &[0x1f1fa, 0x1f1f8],
            &[0x31, VS16, KEYCAP],
            &[0x1f44d, 0x1f3fd],
            &[
                0x1f3f4, 0xe0067, 0xe0062, 0xe0073, 0xe0063, 0xe0074, 0xe007f,
            ],
        ];
        for key in emoji {
            assert_eq!(kind(key), Kind::Emoji, "{key:x?}");
        }
        for key in [&[0x65, 0x301][..], &[0x65, 0x302, 0x301], &[0x915, 0x94d]] {
            assert_eq!(kind(key), Kind::Marks, "{key:x?}");
        }
        assert_eq!(ask(&[0x65, 0x301]).kind, Kind::Marks);
        assert!(
            data::EMOJI.windows(2).all(|w| w.first() < w.last()),
            "sorted"
        );
    }
}
