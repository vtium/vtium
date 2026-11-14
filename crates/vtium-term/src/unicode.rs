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
//!
//! A grapheme cluster takes its first codepoint's width, or two for an
//! emoji-style base with U+FE0F (ADR-1200 D21). Under `cluster-width =
//! double`, the default, two rules of Python wcwidth's specification widen
//! it (ADR-1006 D1). A codepoint of General_Category Mc after a first
//! codepoint of nonzero width gives the cluster two cells. A codepoint of
//! Indic_Syllabic_Category Consonant after one of Virama or
//! Invisible_Stacker, with only U+200D or nonspacing marks between them,
//! adds its width to the cluster's. A virama of category Mc counts as a
//! virama, and no cluster takes more than two cells. Under `single`
//! neither rule applies.

#[cfg(feature = "bidi")]
pub(crate) mod bidi;
mod data;

/// Whether `c` is a right-to-left character by its Bidi_Class: class R,
/// as the Hebrew letters are, or AL, as the Arabic letters are (UAX #9).
/// Arabic digits (class AN), marks and neutrals such as brackets are not.
/// It reads the character alone; where a cell is drawn, and which way its
/// neighbours run, is the frame's (ADR-1004 D8). A build without the bidi
/// feature has no Bidi_Class table and draws every row as written, so it
/// answers false for every character.
pub fn is_rtl_class(c: char) -> bool {
    #[cfg(feature = "bidi")]
    {
        matches!(bidi::class(c), bidi::Class::R | bidi::Class::Al)
    }
    #[cfg(not(feature = "bidi"))]
    {
        let _ = c;
        false
    }
}

/// The version of the Unicode Character Database the tables come from.
pub const UNICODE_VERSION: &str = data::VERSION;

/// The cells a codepoint takes on its own: 0, 1 or 2.
pub(crate) fn char_width(c: char) -> u8 {
    Props::of(c).width()
}

/// A codepoint's property word, as `data.rs` encodes it.
#[derive(Clone, Copy)]
struct Props(u16);

/// The properties of a codepoint outside every run, which the runs starting
/// at U+0000 rule out: those of an unassigned one.
const UNLISTED: Props =
    Props(data::GCB_OTHER | data::INCB_NONE | data::WIDTH_1 | data::MARK_OTHER | data::ISC_OTHER);

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

    fn mark(self) -> Mark {
        match self.0 & data::MARK {
            data::MARK_MN => Mark::Mn,
            data::MARK_MC => Mark::Mc,
            _ => Mark::Other,
        }
    }

    fn isc(self) -> Isc {
        match self.0 & data::ISC {
            data::ISC_VIRAMA => Isc::Virama,
            data::ISC_INVISIBLE_STACKER => Isc::InvisibleStacker,
            data::ISC_CONSONANT => Isc::Consonant,
            _ => Isc::Other,
        }
    }

    /// Indic_Syllabic_Category Virama or Invisible_Stacker: a consonant
    /// after it forms a conjunct (ADR-1006 D1).
    fn virama(self) -> bool {
        matches!(self.isc(), Isc::Virama | Isc::InvisibleStacker)
    }
}

/// The General_Category values a cluster's width reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Other,
    /// Nonspacing_Mark.
    Mn,
    /// Spacing_Mark.
    Mc,
}

/// The Indic_Syllabic_Category values a cluster's width reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Isc {
    Other,
    Virama,
    InvisibleStacker,
    Consonant,
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

/// What vtium-term gives a grapheme cluster with a spacing mark after its
/// first character, or with a virama conjunct: vtium.conf's
/// `cluster-width` (ADR-1006 D8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClusterWidth {
    /// `double`, the default: such a cluster takes two cells, by Python
    /// wcwidth's rules (ADR-1006 D1).
    #[default]
    Double,
    /// `single`: every cluster takes its first character's cells, or two
    /// for an emoji-style base with U+FE0F (ADR-1200 D21).
    Single,
}

/// The cells the grapheme cluster `cluster` takes on a screen whose
/// clusters follow UAX #29 (mode 2027 set, the terminal's default) under
/// `cluster-width = double`, the default: its first codepoint's width, 0,
/// 1 or 2, or two when that codepoint is the base of an emoji-style
/// variation sequence and U+FE0F follows it, widened by ADR-1006's two
/// rules (the module's notes). It is the width the screen stores for the
/// cluster, given here so that the glyph helper places each glyph in the
/// cells the terminal gives it.
pub fn cluster_width(cluster: &[char]) -> u8 {
    cluster_width_with(cluster, ClusterWidth::Double)
}

/// [`cluster_width`] under the rule `rule`, the value of vtium.conf's
/// `cluster-width`.
pub fn cluster_width_with(cluster: &[char], rule: ClusterWidth) -> u8 {
    width_of(cluster.iter().copied(), rule)
}

/// [`cluster_width`] of a cluster's characters as an iterator gives them,
/// the one place the rule is written.
pub(crate) fn width_of(mut chars: impl Iterator<Item = char>, rule: ClusterWidth) -> u8 {
    let Some(first) = chars.next() else {
        return 1;
    };
    let base = Props::of(first);
    let mut width = base.width();
    // the cluster so far ends in a virama or an invisible stacker, perhaps
    // followed by a ZWJ or nonspacing marks
    let mut conjunct = base.virama();
    for c in chars {
        if c == '\u{FE0F}' && base.vs16_base() {
            width = 2;
        }
        if rule == ClusterWidth::Single {
            continue;
        }
        let p = Props::of(c);
        if p.virama() {
            // a spacing virama counts as a virama, not as a spacing mark
            conjunct = true;
        } else if conjunct && p.isc() == Isc::Consonant {
            width = width.saturating_add(p.width());
            conjunct = false;
        } else if p.mark() == Mark::Mc {
            if base.width() > 0 {
                width = 2;
            }
            conjunct = false;
        } else if c != '\u{200D}' && p.mark() != Mark::Mn {
            conjunct = false;
        }
    }
    width.min(2)
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    // cluster_width gives the width the screen stores for each cluster a
    // terminal prints by the Unicode width method: an ideograph, the heart with
    // VS16, with VS15 and alone, a letter and its combining acute, the ZWJ
    // family, Tamil ko and Devanagari ksha (ADR-1006 D1)
    #[test]
    fn a_clusters_width_is_the_one_the_screen_stores() {
        let clusters: [&[char]; 8] = [
            &['\u{4E2D}'],
            &['\u{2764}', '\u{FE0F}'],
            &['\u{2764}', '\u{FE0E}'],
            &['\u{2764}'],
            &['e', '\u{301}'],
            &[
                '\u{1F468}',
                '\u{200D}',
                '\u{1F469}',
                '\u{200D}',
                '\u{1F467}',
            ],
            &['\u{B95}', '\u{BCA}'],
            &['\u{915}', '\u{94D}', '\u{937}'],
        ];
        let widths: Vec<u8> = clusters.iter().map(|k| cluster_width(k)).collect();
        assert_eq!(widths, [2, 2, 1, 1, 1, 2, 2, 2]);
        for cluster in clusters {
            // a terminal prints by the Unicode width method from its start
            let mut term = crate::Terminal::new(10, 1, 0);
            let text: String = cluster.iter().collect();
            term.input(text.as_bytes());
            let stored = match term.screen().line(0).map(|l| l.cell(0).width()) {
                Some(crate::Width::Wide) => 2,
                Some(crate::Width::Narrow) => 1,
                other => panic!("{cluster:?}: {other:?}"),
            };
            assert_eq!(cluster_width(cluster), stored, "{cluster:?}");
        }
        assert_eq!(cluster_width(&[]), 1);
        assert_eq!(cluster_width(&['\u{301}']), 0, "a mark alone");
    }

    /// `text`'s codepoints, for a test's table.
    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    // ADR-1006 D1: under `double`, a cluster with a spacing mark (Mc) after
    // a first character of nonzero width takes two cells, and a virama or
    // invisible stacker followed by a consonant adds the consonant's width,
    // two at most. Python wcwidth 0.9.4's specification gives each of these
    // clusters the width here ("Width of 2", "Virama Conjunct Formation"),
    // and its wcswidth, run on each on 2026-11-13, agrees, apart from Thai
    // sara am. glibc 2.44's wcswidth, run through ctypes in the C.UTF-8
    // locale on the same day, sums its code points' widths, an Mc taking 1:
    // the last column, which agrees with vtium up to two cells.
    #[test]
    fn a_spacing_mark_or_a_virama_conjunct_takes_two_cells() {
        // each line: the cluster, its width under `double`, and glibc's
        // wcswidth of it, which the test does not check
        for (text, double, _glibc) in [
            ("\u{B95}\u{BCA}", 2, Some(2)),        // Tamil ko, vowel sign o (Mc)
            ("\u{B95}\u{BC8}", 2, Some(2)),        // Tamil kai
            ("\u{B95}\u{BC2}", 2, Some(2)),        // Tamil koo
            ("\u{BAE}\u{BBF}", 2, Some(2)),        // Tamil mi
            ("\u{B95}\u{BBE}", 2, Some(2)),        // Tamil kaa, an Mc that is Extend
            ("\u{915}\u{93F}", 2, Some(2)),        // Devanagari ki
            ("\u{915}\u{903}", 2, Some(2)),        // Devanagari ka, visarga (Mc)
            ("\u{915}\u{93C}\u{93F}", 2, Some(2)), // ka, nukta (Mn), i: Mn between
            ("\u{CB0}\u{CC0}", 2, Some(2)),        // Kannada rii
            ("\u{DC3}\u{DD2}\u{D82}", 2, Some(2)), // Sinhala si, anusvara (Mc)
            ("\u{1780}\u{17B6}", 2, Some(2)),      // Khmer kaa
            ("\u{915}\u{94D}\u{937}", 2, Some(2)), // Devanagari ksha, a conjunct
            ("\u{930}\u{94D}\u{92E}", 2, Some(2)), // Devanagari rma
            ("\u{915}\u{94D}\u{200D}\u{937}", 2, Some(2)), // ZWJ after the virama
            ("\u{915}\u{94D}\u{93C}\u{937}", 2, Some(2)), // Mn after the virama
            ("\u{1000}\u{1039}\u{1000}", 2, Some(2)), // Myanmar, an invisible stacker
            ("\u{1780}\u{17D2}\u{1780}", 2, Some(2)), // Khmer coeng
            // at most two, where glibc sums to more
            ("\u{938}\u{94D}\u{924}\u{94D}\u{930}\u{940}", 2, Some(4)), // Devanagari stri
            ("\u{D15}\u{D4D}\u{D37}\u{D4D}\u{D2E}", 2, Some(3)),        // Malayalam kssma
            ("\u{D38}\u{D4D}\u{D24}\u{D4D}\u{D30}\u{D40}", 2, Some(4)), // Malayalam stri
            // a spacing virama counts as a virama, not as a spacing mark
            ("\u{A98F}\u{A9C0}", 1, Some(2)), // Javanese ka, pangkon (Mc)
            ("\u{1B13}\u{1B44}\u{1B13}", 2, Some(3)), // Balinese ka, adeg adeg, ka
            // what takes the width it takes today
            ("\u{915}\u{941}", 1, Some(1)), // Devanagari ku, vowel sign u (Mn)
            ("\u{B95}\u{BCD}", 1, Some(1)), // Tamil k, virama, no consonant
            ("\u{CB8}\u{CCD}", 1, Some(1)), // Kannada s, virama
            ("\u{93F}", 1, Some(1)),        // a spacing mark alone
            ("e\u{301}", 1, Some(1)),       // e, combining acute
            // ADR-1006 D1 takes only the two rules: Thai sara am is Lo, not
            // Mc, so ko kai with it keeps one cell, where Python wcwidth
            // gives two
            ("\u{E01}\u{E33}", 1, Some(2)),
        ] {
            let cluster = chars(text);
            assert_eq!(
                cluster_width_with(&cluster, ClusterWidth::Double),
                double,
                "{cluster:x?} under double"
            );
            assert_eq!(cluster_width(&cluster), double, "{cluster:x?}: the default");
            // ADR-1006 D8: under `single` each takes its first character's
            // width
            let first = cluster.first().map_or(1, |&c| char_width(c));
            assert_eq!(
                cluster_width_with(&cluster, ClusterWidth::Single),
                first,
                "{cluster:x?} under single"
            );
        }
    }

    // ADR-1006 D1: no cluster takes three cells, however its rules add up,
    // and a wide first character with a spacing mark stays two
    #[test]
    fn no_cluster_takes_more_than_two_cells() {
        for text in [
            "\u{915}\u{94D}\u{937}\u{94D}\u{937}\u{94D}\u{937}\u{93F}",
            "\u{4E2D}\u{93F}",
            "\u{2764}\u{FE0F}\u{93F}",
            "\u{1000}\u{1039}\u{1000}\u{1039}\u{1000}\u{102B}",
        ] {
            let cluster = chars(text);
            assert_eq!(cluster_width(&cluster), 2, "{cluster:x?}");
        }
    }

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
            assert_eq!(Props::of(c).vs16_base(), base, "U+{:04X}", u32::from(c));
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
