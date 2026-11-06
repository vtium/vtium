//! The bidirectional algorithm as vtium-term calls it: Unicode's own
//! conformance files at the version the tables pin, the guards on the
//! unicode-bidi crate's call path, and rule L1 a row at a time.

use std::path::Path;

use super::{resolve, visual_order};
use crate::unicode::bidi::Direction;

/// A file beside this one.
fn read(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/unicode/bidi")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The head of a generated conformance file names the crate's Unicode
/// version.
fn assert_version(name: &str, text: &str) {
    let version = format!("Unicode {}.", crate::UNICODE_VERSION);
    assert!(
        text.lines().take(2).any(|l| l.contains(&version)),
        "{name} is not from {version}"
    );
}

/// Data lines: those that are neither blank nor comments.
fn data_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// A character of the Bidi_Class `name`, to stand for it in a BidiTest.txt
/// case.
fn of_class(name: &str) -> char {
    match name {
        "L" => 'a',
        "R" => '\u{05D0}',
        "AL" => '\u{0627}',
        "EN" => '1',
        "ES" => '+',
        "ET" => '$',
        "AN" => '\u{0660}',
        "CS" => ',',
        "NSM" => '\u{0300}',
        "BN" => '\u{00AD}',
        "B" => '\u{2029}',
        "S" => '\t',
        "WS" => ' ',
        "ON" => '!',
        "LRE" => '\u{202A}',
        "LRO" => '\u{202D}',
        "RLE" => '\u{202B}',
        "RLO" => '\u{202E}',
        "PDF" => '\u{202C}',
        "LRI" => '\u{2066}',
        "RLI" => '\u{2067}',
        "FSI" => '\u{2068}',
        "PDI" => '\u{2069}',
        _ => panic!("no Bidi_Class {name}"),
    }
}

/// A list of levels, `None` for each `x`.
fn levels_of(field: &str) -> Vec<Option<u8>> {
    field
        .split_whitespace()
        .map(|t| (t != "x").then(|| t.parse().unwrap_or_else(|e| panic!("{t}: {e}"))))
        .collect()
}

/// A list of indices.
fn order_of(field: &str) -> Vec<usize> {
    field
        .split_whitespace()
        .map(|t| t.parse().unwrap_or_else(|e| panic!("{t}: {e}")))
        .collect()
}

/// What a case expects.
struct Want<'a> {
    /// The paragraph's level, when the case gives it.
    level: Option<u8>,
    /// Each character's level, `None` for one rule X9 removes.
    levels: &'a [Option<u8>],
    /// The visual order of the characters that have a level.
    order: &'a [usize],
}

/// Runs one case through `resolve` and `visual_order` as one row, and says
/// what differs from what it expects.
fn check(text: &[char], direction: Direction, detect: bool, want: &Want) -> Result<(), String> {
    let got = resolve(text, direction, detect, &[]);
    if let Some(level) = want.level
        && got.level != level
    {
        return Err(format!("paragraph level {}, wanted {level}", got.level));
    }
    if got.levels.len() != want.levels.len() {
        return Err(format!(
            "{} levels for {} characters",
            got.levels.len(),
            want.levels.len()
        ));
    }
    let wrong = got
        .levels
        .iter()
        .zip(want.levels)
        .any(|(got, want)| want.is_some_and(|w| w != *got));
    if wrong {
        return Err(format!("levels {:?}, wanted {:?}", got.levels, want.levels));
    }
    // the characters rule X9 removes take no place in the order
    let kept: Vec<usize> = (0..want.levels.len())
        .filter(|&i| want.levels[i].is_some())
        .collect();
    let kept_levels: Vec<u8> = kept.iter().map(|&i| got.levels[i]).collect();
    let order: Vec<usize> = visual_order(&kept_levels)
        .iter()
        .map(|&v| kept[v])
        .collect();
    if order != want.order {
        return Err(format!("order {order:?}, wanted {:?}", want.order));
    }
    Ok(())
}

/// Panics with the first failures, and how many there were, when there are
/// any.
fn report(name: &str, cases: usize, failures: &[String]) {
    assert!(cases > 0, "no cases in {name}");
    if !failures.is_empty() {
        panic!(
            "{} of {cases} cases of {name} fail; the first:\n{}",
            failures.len(),
            failures
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Every case of BidiTest.txt at the crate's Unicode version, in each
/// paragraph direction it lists: the level of each character it gives a
/// level, and their visual order.
#[test]
fn every_case_of_bidi_test_passes() {
    let text = read("bidi_test.txt");
    assert_version("bidi_test.txt", &text);
    let (mut levels, mut order) = (Vec::new(), Vec::new());
    let (mut cases, mut failures) = (0, Vec::new());
    for line in data_lines(&text) {
        if let Some(rest) = line.strip_prefix("@Levels:") {
            levels = levels_of(rest);
            continue;
        }
        if let Some(rest) = line.strip_prefix("@Reorder:") {
            order = order_of(rest);
            continue;
        }
        let (classes, bits) = line.split_once(';').unwrap_or_else(|| panic!("{line}"));
        let chars: Vec<char> = classes.split_whitespace().map(of_class).collect();
        let bits = u8::from_str_radix(bits.trim(), 16).unwrap_or_else(|e| panic!("{line}: {e}"));
        let want = Want {
            level: None,
            levels: &levels,
            order: &order,
        };
        for (bit, direction, detect, name) in [
            (1, Direction::LeftToRight, true, "detected"),
            (2, Direction::LeftToRight, false, "left to right"),
            (4, Direction::RightToLeft, false, "right to left"),
        ] {
            if bits & bit != 0 {
                cases += 1;
                if let Err(e) = check(&chars, direction, detect, &want) {
                    failures.push(format!("{line} ({name}): {e}"));
                }
            }
        }
    }
    report("bidi_test.txt", cases, &failures);
    assert_eq!(cases, 770_241, "the cases BidiTest.txt 18.0.0 holds");
}

/// Every case of BidiCharacterTest.txt at the crate's Unicode version: the
/// paragraph's level, the level of each character it gives a level, and
/// their visual order.
#[test]
fn every_case_of_bidi_character_test_passes() {
    let text = read("bidi_character_test.txt");
    assert_version("bidi_character_test.txt", &text);
    let (mut cases, mut failures) = (0, Vec::new());
    for line in data_lines(&text) {
        let fields: Vec<&str> = line.split(';').collect();
        let [cps, direction, level, levels, order] = fields.as_slice() else {
            panic!("{line}");
        };
        let chars: Vec<char> = cps
            .split_whitespace()
            .map(|cp| {
                u32::from_str_radix(cp, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or_else(|| panic!("{line}"))
            })
            .collect();
        let (direction, detect) = match *direction {
            "0" => (Direction::LeftToRight, false),
            "1" => (Direction::RightToLeft, false),
            "2" => (Direction::LeftToRight, true),
            _ => panic!("{line}"),
        };
        let levels = levels_of(levels);
        let order = order_of(order);
        let want = Want {
            level: Some(level.parse().unwrap_or_else(|e| panic!("{line}: {e}"))),
            levels: &levels,
            order: &order,
        };
        cases += 1;
        if let Err(e) = check(&chars, direction, detect, &want) {
            failures.push(format!("{line}: {e}"));
        }
    }
    report("bidi_character_test.txt", cases, &failures);
    assert_eq!(
        cases, 91_707,
        "the cases BidiCharacterTest.txt 18.0.0 holds"
    );
}

/// The input attached to the unicode-bidi crate's report of a level
/// overflow, https://github.com/servo/unicode-bidi/issues/145, on which the
/// crate's `visual_runs` in 0.3.18 panics over the line of characters 2427
/// to 2436. It passes through vtium-term's call path, laid out as that
/// line and as rows of 80 and 132 cells, in both directions and with
/// detection.
#[test]
fn the_level_overflow_input_resolves_without_a_panic() {
    let text: Vec<char> = read("level_overflow.txt").chars().collect();
    assert_eq!(text.len(), 2877);
    for rows in [vec![2427, 9], vec![80; 36], vec![132; 21], vec![]] {
        for (direction, detect) in [
            (Direction::LeftToRight, false),
            (Direction::RightToLeft, false),
            (Direction::LeftToRight, true),
        ] {
            let got = resolve(&text, direction, detect, &rows);
            assert_eq!(got.levels.len(), text.len());
            let mut order = visual_order(&got.levels);
            order.sort_unstable();
            assert!(order.iter().copied().eq(0..text.len()));
        }
    }
}

/// An empty paragraph and an empty row never reach the crate, whose
/// `visual_runs` panics on an empty line.
#[test]
fn empty_paragraphs_and_rows_resolve_without_a_panic() {
    let none = resolve(&[], Direction::RightToLeft, false, &[0, 0]);
    assert_eq!((none.level, none.levels), (1, vec![]));
    assert_eq!(visual_order(&[]), Vec::<usize>::new());

    let text: Vec<char> = "\u{05D0}\u{05D1} ab".chars().collect();
    let got = resolve(&text, Direction::LeftToRight, false, &[0, 3, 0, 0, 2, 0]);
    assert_eq!(got.levels, [1, 1, 0, 0, 0]);
}

/// Rule L1 runs a row at a time: the spaces at the end of each row take
/// the paragraph's level, while the same spaces inside a row take their
/// neighbours'.
#[test]
fn trailing_spaces_take_the_paragraphs_level_on_each_row() {
    // aleph, bet, two spaces, gimel, dalet, in a left-to-right paragraph
    let text: Vec<char> = "\u{05D0}\u{05D1}  \u{05D2}\u{05D3}".chars().collect();
    let one_row = resolve(&text, Direction::LeftToRight, false, &[]);
    assert_eq!(one_row.levels, [1, 1, 1, 1, 1, 1]);
    let two_rows = resolve(&text, Direction::LeftToRight, false, &[4, 2]);
    assert_eq!(two_rows.levels, [1, 1, 0, 0, 1, 1]);
    assert_eq!(visual_order(&two_rows.levels[..4]), [1, 0, 2, 3]);
}

/// A paragraph with detection takes the direction of its first strong
/// character, and the paragraph's own direction when it has none.
#[test]
fn detection_falls_back_to_the_paragraphs_direction() {
    let digits: Vec<char> = "12 !".chars().collect();
    assert_eq!(resolve(&digits, Direction::RightToLeft, true, &[]).level, 1);
    assert_eq!(resolve(&digits, Direction::LeftToRight, true, &[]).level, 0);
    let hebrew: Vec<char> = "1 \u{05D0}".chars().collect();
    assert_eq!(resolve(&hebrew, Direction::LeftToRight, true, &[]).level, 1);
    assert_eq!(
        resolve(&hebrew, Direction::LeftToRight, false, &[]).level,
        0
    );
}
