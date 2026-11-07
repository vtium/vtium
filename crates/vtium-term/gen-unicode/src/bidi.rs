//! The tables vtium-term's `bidi` feature reads (ADR-1004): each codepoint's
//! Bidi_Class and Joining_Type, the mirroring pairs, the bracket pairs, the
//! box-drawing mirror pairs, and the two conformance files its tests run.

use std::fmt::Write as _;

use crate::{CODEPOINTS, LICENSE, VERSION, commented, fill, records};

/// Bidi_Class values by their code in `data.rs`: the short name and the
/// long name, in the order of UAX #9's table of bidirectional types.
pub const CLASSES: [(&str, &str); 23] = [
    ("L", "Left_To_Right"),
    ("R", "Right_To_Left"),
    ("AL", "Arabic_Letter"),
    ("EN", "European_Number"),
    ("ES", "European_Separator"),
    ("ET", "European_Terminator"),
    ("AN", "Arabic_Number"),
    ("CS", "Common_Separator"),
    ("NSM", "Nonspacing_Mark"),
    ("BN", "Boundary_Neutral"),
    ("B", "Paragraph_Separator"),
    ("S", "Segment_Separator"),
    ("WS", "White_Space"),
    ("ON", "Other_Neutral"),
    ("LRE", "Left_To_Right_Embedding"),
    ("LRO", "Left_To_Right_Override"),
    ("RLE", "Right_To_Left_Embedding"),
    ("RLO", "Right_To_Left_Override"),
    ("PDF", "Pop_Directional_Format"),
    ("LRI", "Left_To_Right_Isolate"),
    ("RLI", "Right_To_Left_Isolate"),
    ("FSI", "First_Strong_Isolate"),
    ("PDI", "Pop_Directional_Isolate"),
];

/// Joining_Type values by their code in `data.rs`: the short name and the
/// long name. Non_Joining is first, so a codepoint no line names has code 0.
pub const JOINING: [(&str, &str); 6] = [
    ("U", "Non_Joining"),
    ("D", "Dual_Joining"),
    ("R", "Right_Joining"),
    ("L", "Left_Joining"),
    ("C", "Join_Causing"),
    ("T", "Transparent"),
];

/// A run word holds the run's first codepoint above this many bits.
const START_SHIFT: u32 = 8;
/// The Joining_Type code sits above the Bidi_Class code.
const JOINING_SHIFT: u32 = 5;
const CLASS_MASK: u32 = 0x1f;
const JOINING_MASK: u32 = 0x7 << JOINING_SHIFT;

/// The box-drawing block, whose left-right pairs mode 2500 draws.
const BOX_FIRST: u32 = 0x2500;
const BOX_LAST: u32 = 0x257F;

/// The value whose short or long name is `name`, as its code.
fn code(table: &[(&str, &str)], name: &str) -> Result<u32, String> {
    table
        .iter()
        .position(|(short, long)| *short == name || *long == name)
        .and_then(|i| u32::try_from(i).ok())
        .ok_or_else(|| format!("unknown property value {name:?}"))
}

/// The `@missing` lines of a UCD file, in the order the file gives them:
/// each one's codepoint range and value. A later line overrides an earlier
/// one over the range they share.
fn missing(text: &str) -> Result<Vec<(usize, usize, String)>, String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("# @missing:") else {
            continue;
        };
        let (range, value) = rest
            .split_once(';')
            .ok_or_else(|| format!("bad @missing line {line:?}"))?;
        let range = range.trim();
        let (first, last) = range.split_once("..").unwrap_or((range, range));
        let parse = |s: &str| {
            usize::from_str_radix(s, 16)
                .ok()
                .filter(|&cp| cp < CODEPOINTS)
                .ok_or_else(|| format!("bad codepoint in {line:?}"))
        };
        out.push((parse(first)?, parse(last)?, value.trim().to_string()));
    }
    Ok(out)
}

/// Each codepoint's value of the property a derived UCD file lists: its
/// `@missing` lines first, then its data lines.
fn property(text: &str, table: &[(&str, &str)]) -> Result<Vec<u32>, String> {
    let defaults = missing(text)?;
    if defaults.is_empty() {
        return Err("no @missing line".to_string());
    }
    let mut values = vec![u32::MAX; CODEPOINTS];
    for (first, last, value) in defaults {
        fill(&mut values, first, last, code(table, &value)?);
    }
    for r in records(text) {
        let (first, last, f) = r?;
        fill(&mut values, first, last, code(table, f.first().copied().unwrap_or(""))?);
    }
    if values.contains(&u32::MAX) {
        return Err("a codepoint has no value".to_string());
    }
    Ok(values)
}

/// The runs of codepoints with equal Bidi_Class and Joining_Type, each as
/// one word: its first codepoint, its Joining_Type code and its Bidi_Class
/// code.
pub fn runs(classes_txt: &str, joining_txt: &str) -> Result<Vec<u32>, String> {
    let classes = property(classes_txt, &CLASSES)?;
    let joining = property(joining_txt, &JOINING)?;
    let mut runs: Vec<u32> = Vec::new();
    let mut last = None;
    for (cp, (&class, &join)) in (0u32..).zip(classes.iter().zip(&joining)) {
        let value = join << JOINING_SHIFT | class;
        if last != Some(value) {
            runs.push(cp << START_SHIFT | value);
            last = Some(value);
        }
    }
    Ok(runs)
}

/// A codepoint field as a char.
fn char_of(field: &str, line: &str) -> Result<char, String> {
    u32::from_str_radix(field.trim(), 16)
        .ok()
        .and_then(char::from_u32)
        .ok_or_else(|| format!("bad codepoint in {line:?}"))
}

/// BidiMirroring.txt's pairs, each character with its Bidi_Mirroring_Glyph,
/// in increasing order of the first.
pub fn mirrors(text: &str) -> Result<Vec<(char, char)>, String> {
    let mut pairs = Vec::new();
    for line in text.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        let (from, to) = data
            .split_once(';')
            .ok_or_else(|| format!("bad line {line:?}"))?;
        pairs.push((char_of(from, line)?, char_of(to, line)?));
    }
    pairs.sort_unstable();
    if pairs.windows(2).any(|w| matches!(w, [a, b] if a.0 == b.0)) {
        return Err("BidiMirroring.txt names a character twice".to_string());
    }
    Ok(pairs)
}

/// Each bracket of BidiBrackets.txt with the opening bracket of its pair
/// after canonical decomposition, and whether it opens, in increasing
/// order. Rule BD16 matches brackets by their canonical equivalents, so
/// U+2329 and U+3008 both give U+3008.
pub fn brackets(text: &str, unicode_data: &str) -> Result<Vec<(char, char, bool)>, String> {
    // a character's singleton canonical decomposition, from UnicodeData.txt
    let mut singleton = std::collections::BTreeMap::new();
    for line in unicode_data.lines() {
        let fields: Vec<&str> = line.split(';').collect();
        let [cp, _, _, _, _, decomposition, ..] = fields.as_slice() else {
            return Err(format!("bad UnicodeData.txt line {line:?}"));
        };
        // a surrogate, which is no char, has no decomposition
        if !decomposition.is_empty() && !decomposition.contains(['<', ' ']) {
            singleton.insert(char_of(cp, line)?, char_of(decomposition, line)?);
        }
    }
    let canonical = |c: char| singleton.get(&c).copied().unwrap_or(c);
    let mut out = Vec::new();
    for line in text.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        let fields: Vec<&str> = data.split(';').map(str::trim).collect();
        let [bracket, pair, kind] = fields.as_slice() else {
            return Err(format!("bad line {line:?}"));
        };
        let (bracket, pair) = (char_of(bracket, line)?, char_of(pair, line)?);
        let (opening, opens) = match *kind {
            "o" => (canonical(bracket), true),
            "c" => (canonical(pair), false),
            _ => return Err(format!("bad bracket type in {line:?}")),
        };
        out.push((bracket, opening, opens));
    }
    out.sort_unstable();
    Ok(out)
}

/// Each box-drawing character's left-right counterpart: the character
/// whose name swaps the words LEFT and RIGHT. Where no name matches the
/// swap word for word, the counterpart is the character whose name gives
/// the swapped name's two halves around AND in the other order, so U+257C
/// BOX DRAWINGS LIGHT LEFT AND HEAVY RIGHT pairs with U+257E BOX DRAWINGS
/// HEAVY LEFT AND LIGHT RIGHT. A character with neither has no pair.
pub fn box_mirrors(unicode_data: &str) -> Result<Vec<(char, char)>, String> {
    let mut names = std::collections::BTreeMap::new();
    for line in unicode_data.lines() {
        let mut fields = line.split(';');
        let (Some(cp), Some(name)) = (fields.next(), fields.next()) else {
            return Err(format!("bad UnicodeData.txt line {line:?}"));
        };
        let cp = u32::from_str_radix(cp, 16).map_err(|e| format!("{line:?}: {e}"))?;
        if (BOX_FIRST..=BOX_LAST).contains(&cp) {
            names.insert(name.to_string(), char_of(&format!("{cp:X}"), line)?);
        }
    }
    let swap = |name: &str| -> String {
        name.split(' ')
            .map(|w| match w {
                "LEFT" => "RIGHT",
                "RIGHT" => "LEFT",
                w => w,
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let halves_swapped = |name: &str| -> Option<String> {
        let rest = name.strip_prefix("BOX DRAWINGS ")?;
        let (a, b) = rest.split_once(" AND ")?;
        Some(format!("BOX DRAWINGS {b} AND {a}"))
    };
    let mut pairs = Vec::new();
    for (name, &c) in &names {
        let swapped = swap(name);
        if swapped == *name {
            continue;
        }
        let partner = names
            .get(&swapped)
            .or_else(|| halves_swapped(&swapped).and_then(|n| names.get(&n)));
        if let Some(&partner) = partner {
            pairs.push((c, partner));
        }
    }
    pairs.sort_unstable();
    for &(a, b) in &pairs {
        if a == b || pairs.binary_search(&(b, a)).is_err() {
            return Err(format!(
                "U+{:04X} pairs with U+{:04X}, which does not pair back",
                u32::from(a),
                u32::from(b)
            ));
        }
    }
    Ok(pairs)
}

/// A char as a Rust literal.
fn lit(c: char) -> String {
    format!("'\\u{{{:04X}}}'", u32::from(c))
}

/// `src/unicode/bidi/data.rs`.
pub fn data_rs(
    runs: &[u32],
    mirrors: &[(char, char)],
    brackets: &[(char, char, bool)],
    box_mirrors: &[(char, char)],
) -> String {
    let mut out = format!(
        "// Generated by crates/vtium-term/gen-unicode from the Unicode Character\n\
         // Database {VERSION}. Do not edit; rerun the generator.\n\
         //\n\
         // The Unicode data it comes from is under this licence:\n\
         //\n"
    );
    out += &commented(LICENSE, "// ");
    out += "\n\
        //! The tables of the `bidi` feature: each codepoint's Bidi_Class and\n\
        //! Joining_Type, as runs of codepoints with equal values, and the\n\
        //! mirroring, bracket and box-drawing pairs.\n\n";
    let _ = writeln!(
        out,
        "/// The Unicode Character Database version of the tables.\n\
         pub(super) const VERSION: &str = \"{VERSION}\";\n\n\
         /// A run word's first codepoint sits above this many bits.\n\
         pub(super) const START_SHIFT: u32 = {START_SHIFT};\n\
         /// Bidi_Class, one of the `CLASS_` values.\n\
         pub(super) const CLASS: u32 = {CLASS_MASK:#04x};\n\
         /// Joining_Type, one of the `JOINING_` values.\n\
         pub(super) const JOINING: u32 = {JOINING_MASK:#04x};"
    );
    for (code, (short, _)) in (0u32..).zip(CLASSES) {
        let _ = writeln!(out, "pub(super) const CLASS_{short}: u32 = {code};");
    }
    for (code, (short, _)) in (0u32..).zip(JOINING) {
        let _ = writeln!(
            out,
            "pub(super) const JOINING_{short}: u32 = {:#04x};",
            code << JOINING_SHIFT
        );
    }

    let words: Vec<String> = runs.iter().map(|w| format!("{w:#010x}")).collect();
    let _ = write!(
        out,
        "\n/// Each run's word: its first codepoint above [`START_SHIFT`], its\n\
         /// Joining_Type and its Bidi_Class, ascending from U+0000.\n\
         pub(super) const RUNS: [u32; {n}] = [\n{body}];\n",
        n = runs.len(),
        body = crate::array_body(&words),
    );
    let pair_lines = |pairs: &[(char, char)]| -> String {
        pairs
            .iter()
            .map(|&(a, b)| format!("    ({}, {}),\n", lit(a), lit(b)))
            .collect()
    };
    let _ = write!(
        out,
        "\n/// Each character of BidiMirroring.txt and its Bidi_Mirroring_Glyph, by\n\
         /// the first.\n\
         pub(super) const MIRRORS: [(char, char); {n}] = [\n{body}];\n",
        n = mirrors.len(),
        body = pair_lines(mirrors),
    );
    let bracket_lines: String = brackets
        .iter()
        .map(|&(b, o, opens)| format!("    ({}, {}, {opens}),\n", lit(b), lit(o)))
        .collect();
    let _ = write!(
        out,
        "\n/// Each bracket of BidiBrackets.txt, the opening bracket of its pair\n\
         /// after canonical decomposition, and whether it opens, by the first.\n\
         pub(super) const BRACKETS: [(char, char, bool); {n}] = [\n{bracket_lines}];\n",
        n = brackets.len(),
    );
    let _ = write!(
        out,
        "\n/// Each box-drawing character with a left-right counterpart, and the\n\
         /// counterpart, by the first.\n\
         pub(super) const BOX_MIRRORS: [(char, char); {n}] = [\n{body}];\n",
        n = box_mirrors.len(),
        body = pair_lines(box_mirrors),
    );
    out
}

/// The head of a conformance file vtium-term's tests read.
fn test_head(source: &str, format: &str) -> String {
    let mut out = format!(
        "# Generated by crates/vtium-term/gen-unicode from {source},\n\
         # Unicode {VERSION}. Do not edit; rerun the generator.\n\
         #\n"
    );
    out += &commented(format, "# ");
    out += "#\n# The Unicode data it comes from is under this licence:\n#\n";
    out += &commented(LICENSE, "# ");
    out
}

/// BidiTest.txt's `@Levels`, `@Reorder` and data lines, each checked and
/// with its comment dropped.
pub fn bidi_test_txt(text: &str) -> Result<(String, usize), String> {
    let mut out = test_head(
        "BidiTest.txt",
        "Unicode's format, without its comments: an @Levels line gives the\n\
         levels of the data lines after it, x for a character rule X9\n\
         removes; an @Reorder line their visual order, left to right; a data\n\
         line the Bidi_Class of each character, then after a semicolon a\n\
         hex bitset of the paragraph directions it runs in: 1 detected by\n\
         rules P2 and P3, falling back to left to right; 2 left to right;\n\
         4 right to left.\n",
    );
    let mut cases = 0;
    let mut levels = 0;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("@Levels:") {
            levels = rest.split_whitespace().count();
        } else if line.starts_with('@') && !line.starts_with("@Reorder:") {
            // the file says to ignore any other @ line
            continue;
        } else if !line.starts_with('@') {
            let (input, bits) = line
                .split_once(';')
                .ok_or_else(|| format!("bad line {line:?}"))?;
            let classes: Vec<&str> = input.split_whitespace().collect();
            for class in &classes {
                code(&CLASSES, class)?;
            }
            let bits = u8::from_str_radix(bits.trim(), 16)
                .ok()
                .filter(|b| (1..=7).contains(b))
                .ok_or_else(|| format!("bad bitset in {line:?}"))?;
            if classes.len() != levels {
                return Err(format!("{line:?} does not match its @Levels line"));
            }
            cases += (0..3).filter(|i| bits & (1 << i) != 0).count();
        }
        out.push('\n');
        out.push_str(&line.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    out.push('\n');
    Ok((out, cases))
}

/// BidiCharacterTest.txt's data lines, each checked.
pub fn bidi_character_test_txt(text: &str) -> Result<(String, usize), String> {
    let mut out = test_head(
        "BidiCharacterTest.txt",
        "Unicode's format, without its comments: on each line, the\n\
         codepoints in hex; the paragraph direction, 0 left to right, 1\n\
         right to left, 2 detected by rules P2 and P3, falling back to left\n\
         to right; the paragraph's resolved level; each character's level, x\n\
         for a character rule X9 removes; and the visual order, left to\n\
         right, of the characters that have a level.\n",
    );
    let mut cases = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(';').collect();
        let [cps, direction, _, levels, _] = fields.as_slice() else {
            return Err(format!("bad line {line:?}"));
        };
        for cp in cps.split_whitespace() {
            char_of(cp, line)?;
        }
        if !matches!(*direction, "0" | "1" | "2") {
            return Err(format!("bad paragraph direction in {line:?}"));
        }
        if cps.split_whitespace().count() != levels.split_whitespace().count() {
            return Err(format!("{line:?} has a level for each codepoint"));
        }
        out.push('\n');
        out.push_str(line);
        cases += 1;
    }
    out.push('\n');
    Ok((out, cases))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNICODE_DATA: &str = "\
2329;LEFT-POINTING ANGLE BRACKET;Ps;0;ON;3008;;;;Y;BRA;;;;
232A;RIGHT-POINTING ANGLE BRACKET;Pe;0;ON;3009;;;;Y;KET;;;;
250C;BOX DRAWINGS LIGHT DOWN AND RIGHT;So;0;ON;;;;;N;;;;;
2510;BOX DRAWINGS LIGHT DOWN AND LEFT;So;0;ON;;;;;N;;;;;
2500;BOX DRAWINGS LIGHT HORIZONTAL;So;0;ON;;;;;N;;;;;
257C;BOX DRAWINGS LIGHT LEFT AND HEAVY RIGHT;So;0;ON;;;;;N;;;;;
257E;BOX DRAWINGS HEAVY LEFT AND LIGHT RIGHT;So;0;ON;;;;;N;;;;;
3008;LEFT ANGLE BRACKET;Ps;0;ON;;;;;Y;;;;;
3009;RIGHT ANGLE BRACKET;Pe;0;ON;;;;;Y;;;;;
";

    #[test]
    fn a_later_missing_line_overrides_an_earlier_one_and_data_lines_override_both() {
        let text = "# @missing: 0000..10FFFF; Left_To_Right\n\
                    # @missing: 0590..05FF; Right_To_Left\n\
                    05D0..05EA ; R\n\
                    0030..0039 ; EN\n";
        let values = property(text, &CLASSES).unwrap();
        let class = |name| code(&CLASSES, name).unwrap();
        assert_eq!(values[0x41], class("L"));
        assert_eq!(values[0x5FF], class("R"));
        assert_eq!(values[0x31], class("EN"));
        assert_eq!(values[0x600], class("L"));
    }

    #[test]
    fn brackets_match_by_their_canonical_equivalents() {
        let text = "2329; 232A; o\n232A; 2329; c\n3008; 3009; o\n3009; 3008; c\n";
        let got = brackets(text, UNICODE_DATA).unwrap();
        assert_eq!(
            got,
            [
                ('\u{2329}', '\u{3008}', true),
                ('\u{232A}', '\u{3008}', false),
                ('\u{3008}', '\u{3008}', true),
                ('\u{3009}', '\u{3008}', false),
            ]
        );
    }

    #[test]
    fn box_drawing_pairs_swap_left_and_right_by_name() {
        let got = box_mirrors(UNICODE_DATA).unwrap();
        assert_eq!(
            got,
            [
                ('\u{250C}', '\u{2510}'),
                ('\u{2510}', '\u{250C}'),
                ('\u{257C}', '\u{257E}'),
                ('\u{257E}', '\u{257C}'),
            ]
        );
    }
}
