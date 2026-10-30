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
    /// or lam and alef as their ligature. Kept only when the font shapes
    /// it to one glyph that none of its scalar values draws alone, placed
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

/// How the cluster `key` is judged and placed: an emoji sequence when
/// Unicode lists it as one or it holds U+200D, U+FE0F, a regional
/// indicator, an emoji modifier, a tag or the keycap mark; else a letter
/// and its marks.
pub fn kind(key: &[u32]) -> Kind {
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
