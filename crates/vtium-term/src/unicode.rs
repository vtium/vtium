//! Unicode properties: a codepoint's cell width, and the boundaries of
//! extended grapheme clusters (UAX #29).
//!
//! The properties come from the Unicode Character Database at
//! [`UNICODE_VERSION`], generated into `unicode/data.rs` by `gen-unicode`,
//! whose README says how to rerun it.
//!
//! # Width
//!
//! A codepoint takes no cell when its General_Category is Mn, Me, Cf or Cc,
//! unless it is U+00AD SOFT HYPHEN or its Grapheme_Cluster_Break is Prepend
//! (a prepended concatenation mark such as U+0600 starts a cluster, so it
//! shows), and when it is a Hangul medial vowel or final consonant
//! (Grapheme_Cluster_Break V or T). It takes two when its East_Asian_Width
//! is W or F, or it has Emoji_Presentation; an unassigned codepoint in a
//! range EastAsianWidth.txt's header names is W. Any other takes one.

mod data;

/// The version of the Unicode Character Database the tables come from.
pub const UNICODE_VERSION: &str = data::VERSION;

/// The cells a codepoint takes on its own: 0, 1 or 2.
pub(crate) fn char_width(c: char) -> u8 {
    Props::of(c).width()
}

/// Whether `c` is the base of an emoji-style variation sequence, which VS16
/// (U+FE0F) widens to two cells.
pub(crate) fn is_vs16_base(c: char) -> bool {
    Props::of(c).vs16_base()
}

/// A codepoint's property word, as `data.rs` encodes it.
#[derive(Clone, Copy)]
struct Props(u16);

/// The properties of a codepoint outside every run, which the runs starting
/// at U+0000 rule out: those of an unassigned one.
const UNLISTED: Props = Props(data::GCB_OTHER | data::INCB_NONE | data::WIDTH_1);

impl Props {
    fn of(c: char) -> Self {
        let cp = u32::from(c);
        let run = data::RUN_STARTS
            .partition_point(|&start| start <= cp)
            .saturating_sub(1);
        data::RUN_PROPS.get(run).map_or(UNLISTED, |&p| Self(p))
    }

    fn gcb(self) -> Gcb {
        match self.0 & data::GCB {
            data::GCB_CR => Gcb::Cr,
            data::GCB_LF => Gcb::Lf,
            data::GCB_CONTROL => Gcb::Control,
            data::GCB_EXTEND => Gcb::Extend,
            data::GCB_ZWJ => Gcb::Zwj,
            data::GCB_REGIONAL_INDICATOR => Gcb::RegionalIndicator,
            data::GCB_PREPEND => Gcb::Prepend,
            data::GCB_SPACING_MARK => Gcb::SpacingMark,
            data::GCB_L => Gcb::L,
            data::GCB_V => Gcb::V,
            data::GCB_T => Gcb::T,
            data::GCB_LV => Gcb::Lv,
            data::GCB_LVT => Gcb::Lvt,
            _ => Gcb::Other,
        }
    }

    fn pict(self) -> bool {
        self.0 & data::PICT != 0
    }

    fn incb(self) -> InCb {
        match self.0 & data::INCB {
            data::INCB_LINKER => InCb::Linker,
            data::INCB_CONSONANT => InCb::Consonant,
            data::INCB_EXTEND => InCb::Extend,
            _ => InCb::None,
        }
    }

    fn width(self) -> u8 {
        match self.0 & data::WIDTH {
            data::WIDTH_0 => 0,
            data::WIDTH_2 => 2,
            _ => 1,
        }
    }

    fn vs16_base(self) -> bool {
        self.0 & data::VS16_BASE != 0
    }
}

/// Grapheme_Cluster_Break.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Gcb {
    #[default]
    Other,
    Cr,
    Lf,
    Control,
    Extend,
    Zwj,
    RegionalIndicator,
    Prepend,
    SpacingMark,
    L,
    V,
    T,
    Lv,
    Lvt,
}

/// Indic_Conjunct_Break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InCb {
    None,
    Linker,
    Consonant,
    Extend,
}

/// How far a cluster's last codepoints go into GB11's
/// `\p{Extended_Pictographic} Extend* ZWJ`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Pict {
    #[default]
    Outside,
    /// `\p{Extended_Pictographic} Extend*`.
    Extending,
    /// `\p{Extended_Pictographic} Extend* ZWJ`.
    Joined,
}

/// The UAX #29 extended-grapheme-cluster state after the codepoints of one
/// cluster.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Segmenter {
    /// The last codepoint's Grapheme_Cluster_Break.
    last: Gcb,
    pict: Pict,
    /// The cluster ends in an odd number of Regional_Indicators (GB12,
    /// GB13).
    ri_odd: bool,
    /// The cluster ends in `\p{InCB=Linker} [\p{InCB=Extend}
    /// \p{InCB=Linker}]*` (GB9c).
    linker: bool,
}

impl Segmenter {
    /// The state after a cluster's first codepoint.
    pub(crate) fn start(c: char) -> Self {
        let mut s = Self::default();
        s.advance(Props::of(c));
        s
    }

    /// Whether `c` continues the cluster so far (no boundary before it).
    /// The state advances either way: on a boundary it is the state of a new
    /// cluster that starts at `c`.
    pub(crate) fn continues(&mut self, c: char) -> bool {
        use Gcb::{
            Control, Cr, Extend, L, Lf, Lv, Lvt, Prepend, RegionalIndicator, SpacingMark, T, V, Zwj,
        };

        let p = Props::of(c);
        let next = p.gcb();
        let joins = match (self.last, next) {
            (Cr, Lf) => true,                                         // GB3
            (Cr | Lf | Control, _) | (_, Cr | Lf | Control) => false, // GB4, GB5
            (L, L | V | Lv | Lvt) => true,                            // GB6
            (Lv | V, V | T) => true,                                  // GB7
            (Lvt | T, T) => true,                                     // GB8
            (_, Extend | Zwj) => true,                                // GB9
            (_, SpacingMark) => true,                                 // GB9a
            (Prepend, _) => true,                                     // GB9b
            _ if self.linker && p.incb() == InCb::Consonant => true,  // GB9c
            _ if self.pict == Pict::Joined && p.pict() => true,       // GB11
            (RegionalIndicator, RegionalIndicator) => self.ri_odd,    // GB12, GB13
            _ => false,                                               // GB999
        };
        if !joins {
            *self = Self::default();
        }
        self.advance(p);
        joins
    }

    /// Moves the state past a codepoint of the cluster.
    fn advance(&mut self, p: Props) {
        let gcb = p.gcb();
        self.pict = match (self.pict, gcb) {
            _ if p.pict() => Pict::Extending,
            (Pict::Extending, Gcb::Extend) => Pict::Extending,
            (Pict::Extending, Gcb::Zwj) => Pict::Joined,
            _ => Pict::Outside,
        };
        self.ri_odd = gcb == Gcb::RegionalIndicator && !self.ri_odd;
        let incb = p.incb();
        self.linker = incb == InCb::Linker || (self.linker && incb == InCb::Extend);
        self.last = gcb;
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn widths_of_named_codepoints() {
        for (c, width) in [
            ('a', 1),
            ('~', 1),
            ('\u{0301}', 0),  // COMBINING ACUTE ACCENT, Mn
            ('\u{4E2D}', 2),  // CJK ideograph, East_Asian_Width W
            ('\u{1F600}', 2), // GRINNING FACE, Emoji_Presentation
            ('\u{00AD}', 1),  // SOFT HYPHEN, Cf that shows
            ('\u{1160}', 0),  // HANGUL JUNGSEONG FILLER, V
            ('\u{11A8}', 0),  // HANGUL JONGSEONG KIYEOK, T
            ('\u{0600}', 1),  // ARABIC NUMBER SIGN, Prepend Cf
            ('\u{200B}', 0),  // ZERO WIDTH SPACE, Cf
            ('\u{0007}', 0),  // BEL, Cc
            ('\u{009B}', 0),  // CSI, Cc
            ('\u{3400}', 2),  // CJK extension A, East_Asian_Width W
            ('\u{FF21}', 2),  // FULLWIDTH LATIN CAPITAL LETTER A, F
        ] {
            assert_eq!(char_width(c), width, "U+{:04X}", u32::from(c));
        }
    }

    #[test]
    fn vs16_bases() {
        for (c, base) in [
            ('#', true),
            ('0', true),
            ('\u{2764}', true),  // HEAVY BLACK HEART
            ('\u{263A}', true),  // WHITE SMILING FACE
            ('\u{1F44D}', true), // THUMBS UP SIGN
            ('a', false),
            ('\u{4E2D}', false),
            ('\u{FE0F}', false),
        ] {
            assert_eq!(is_vs16_base(c), base, "U+{:04X}", u32::from(c));
        }
    }

    /// One GraphemeBreakTest case through a [`Segmenter`]: `Err` names the
    /// first codepoint whose boundary or state is wrong.
    fn check_case(line: &str) -> Result<(), String> {
        // ÷ or ×, then a codepoint, in turn, and ÷ to end the line
        let mut tokens = line.split_whitespace();
        let mut seg: Option<Segmenter> = None;
        while let (Some(mark), Some(hex)) = (tokens.next(), tokens.next()) {
            let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) else {
                return Err(format!("bad codepoint {hex:?}"));
            };
            let boundary = mark == "÷";
            let ok = match seg.as_mut() {
                None => boundary,
                Some(s) => s.continues(c) != boundary && (!boundary || *s == Segmenter::start(c)),
            };
            if !ok {
                return Err(format!("wrong at U+{hex}"));
            }
            seg.get_or_insert_with(|| Segmenter::start(c));
        }
        Ok(())
    }

    /// Every case of the UCD's GraphemeBreakTest at [`UNICODE_VERSION`],
    /// each boundary and each non-boundary.
    #[test]
    fn grapheme_break_test() {
        let text = include_str!("unicode/grapheme_break_test.txt");
        assert!(
            text.lines()
                .take(2)
                .any(|l| l.contains(&format!("Unicode {UNICODE_VERSION}."))),
            "grapheme_break_test.txt is not from Unicode {UNICODE_VERSION}"
        );
        let cases: Vec<&str> = text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        assert!(!cases.is_empty(), "no cases in grapheme_break_test.txt");
        let failures: Vec<String> = cases
            .iter()
            .filter_map(|line| check_case(line).err().map(|e| format!("{line}: {e}")))
            .collect();
        if !failures.is_empty() {
            panic!(
                "{} of {} GraphemeBreakTest cases fail:\n{}",
                failures.len(),
                cases.len(),
                failures.join("\n")
            );
        }
    }
}
