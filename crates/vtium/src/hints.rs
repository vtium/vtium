//! Keyboard link hints: a key labels each OSC 8 link on the screen with
//! letters, and typing a label copies that link's address, so a console
//! with no mouse can still reach a link.
//!
//! This module holds the rules, as functions of what they are given:
//! which links get a label and in what order, the labels themselves, how
//! each key press is read while hints are on, and what the letters typed
//! so far pick. The head keeps a [`Hints`] while hints are on, hands it
//! each key press, and acts on the [`Step`] it returns. The terminal's
//! screen keeps the labelled links beside the hovered link, so a sweep of
//! the link store renames them and never frees one while its label shows,
//! and the frame carries the labels to the renderer.
//!
//! Labels are made from the 26 lowercase letters, the home row first.
//! Every label has the same number of letters, the fewest that give each
//! link its own label, so no label begins another. The last letter typed
//! therefore picks the link, and no Enter is needed.
//!
//! References:
//! - ADR-1002, section 7 ("Keyboard link hints"): the key, the order of
//!   the links, the alphabet, the labels' length, the keys read while
//!   hints are on, and how hints end.

/// The letters labels are made from, in the order labels count through
/// them: the home row, then the row above it, then the row below it.
///
/// References:
/// - ADR-1002, section 7, "How labels are chosen": the alphabet and its
///   order.
pub const ALPHABET: [char; 26] = [
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p',
    'z', 'x', 'c', 'v', 'b', 'n', 'm',
];

/// The text the overlay row shows while hints are on.
pub const PROMPT: &str = "Type a link's label to copy its address. Press Escape to stop.";

/// The text the overlay row shows when the key finds no link to label.
pub const NO_LINKS: &str = "No links are on the screen.";

/// The words the overlay row shows before the address it copied.
pub const COPIED: &str = "Copied: ";

/// How many letters each label has when `count` links need one: the
/// fewest letters that give each link its own label, and at least one.
/// One letter covers 26 links, two cover 676, and so on.
pub fn label_len(count: usize) -> usize {
    let base = ALPHABET.len();
    let mut len = 1usize;
    let mut covered = base;
    while covered < count {
        len = len.saturating_add(1);
        covered = covered.saturating_mul(base);
    }
    len
}

/// The label of the link at `index` in the order links are labelled,
/// `len` letters long. The labels count through the alphabet's order as
/// digits do: with two letters, the first link takes `aa`, the second
/// `as`, and the 27th `sa`.
pub fn label(index: usize, len: usize) -> String {
    let base = ALPHABET.len();
    let mut letters = vec!['a'; len];
    let mut rest = index;
    for slot in letters.iter_mut().rev() {
        *slot = ALPHABET.get(rest % base).copied().unwrap_or('a');
        rest /= base;
    }
    letters.into_iter().collect()
}

/// The labels of `count` links, in the order the links are labelled.
pub fn labels(count: usize) -> Vec<String> {
    let len = label_len(count);
    (0..count).map(|i| label(i, len)).collect()
}

/// The distinct links of a view `cols` columns wide and `rows` rows tall,
/// in the order they first appear: row by row from the top, and in each
/// row from left to right as the row is drawn. `link_at` gives the link
/// of the cell drawn at a column and a row, if the cell has one.
pub fn links_in_view<L: PartialEq + Copy>(
    cols: usize,
    rows: usize,
    link_at: impl Fn(usize, usize) -> Option<L>,
) -> Vec<L> {
    let mut found: Vec<L> = Vec::new();
    for y in 0..rows {
        for x in 0..cols {
            if let Some(link) = link_at(x, y).filter(|l| !found.contains(l)) {
                found.push(link);
            }
        }
    }
    found
}

/// The column a label `len` letters long starts at, for a link that first
/// appears at column `col` of a row `cols` columns wide. A label that
/// would pass the row's last column moves left so that it ends there.
pub fn label_col(col: usize, len: usize, cols: usize) -> usize {
    col.min(cols.saturating_sub(len))
}

/// What a key press means while hints are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintKey {
    /// A letter of the alphabet, as a lowercase character.
    Letter(char),
    /// Backspace takes back the last letter kept.
    Backspace,
    /// A key that ends hints: Escape, the hints key again, or any other key
    /// that is not a letter or Backspace. vtium drops the key.
    End,
    /// A modifier key alone, such as Shift or Ctrl, which changes nothing.
    Modifier,
}

/// The keysym of BackSpace.
const BACKSPACE: u32 = 0xff08;

/// Whether `keysym` is a modifier key: Shift, Control, Caps Lock, Meta,
/// Alt, Super and Hyper (0xffe1 to 0xffee), and the ISO level and group
/// keys, such as AltGr (0xfe01 to 0xfe13).
fn is_modifier(keysym: u32) -> bool {
    (0xffe1..=0xffee).contains(&keysym) || (0xfe01..=0xfe13).contains(&keysym)
}

/// The lowercase letter a keysym names, for the 26 letters of the
/// alphabet in either case.
fn letter(keysym: u32) -> Option<char> {
    let c = char::from_u32(keysym)?;
    c.is_ascii_alphabetic().then(|| c.to_ascii_lowercase())
}

/// How a key press reads while hints are on. `keysyms` are the symbols
/// the key makes, and `command` says whether Ctrl, Alt or Super is held.
/// A letter typed with Ctrl, Alt or Super held is no letter of a label,
/// so it ends hints, as every other key does.
pub fn hint_key(keysyms: &[u32], command: bool) -> HintKey {
    if !keysyms.is_empty() && keysyms.iter().all(|&k| is_modifier(k)) {
        return HintKey::Modifier;
    }
    if command {
        return HintKey::End;
    }
    match keysyms {
        [BACKSPACE] => HintKey::Backspace,
        [k] => letter(*k).map_or(HintKey::End, HintKey::Letter),
        _ => HintKey::End,
    }
}

/// What a key press did while hints are on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step<L> {
    /// The letters kept changed, so the labels shown changed.
    Typed,
    /// The key changed nothing: a letter that continues no label, a
    /// Backspace with no letter kept, or a modifier key alone.
    Ignored,
    /// The letters kept spell the label of `L`: vtium copies its address,
    /// and hints end.
    Picked(L),
    /// Hints end with no copy.
    End,
}

/// Link hints while they are on: the labels fixed when they started, and
/// the letters typed so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hints<L> {
    /// Each labelled link with its label, in the order the links were
    /// labelled.
    labelled: Vec<(L, String)>,
    /// The letters kept, which every label shown begins with.
    typed: String,
}

impl<L: PartialEq + Copy> Hints<L> {
    /// Hints over `links`, given in the order they first appear in the
    /// view. Returns `None` when no link is visible, since vtium then starts
    /// no hints.
    pub fn start(links: &[L]) -> Option<Hints<L>> {
        if links.is_empty() {
            return None;
        }
        let labelled = links.iter().copied().zip(labels(links.len())).collect();
        Some(Hints {
            labelled,
            typed: String::new(),
        })
    }

    /// The labelled links and their labels, in the order they were
    /// labelled.
    pub fn labelled(&self) -> &[(L, String)] {
        &self.labelled
    }

    /// The letters kept so far.
    pub fn typed(&self) -> &str {
        &self.typed
    }

    /// Whether the label of `link` shows: the link has a label, and the
    /// label begins with the letters kept.
    pub fn shows(&self, link: L) -> bool {
        self.labelled
            .iter()
            .any(|(l, text)| *l == link && text.starts_with(&self.typed))
    }

    /// Keeps only the labelled links that `keep` holds, so a link that
    /// left the view loses its label, and renames each link through
    /// `keep`, as a sweep of the link store renumbers the links it keeps.
    pub fn retain(&mut self, keep: impl Fn(L) -> Option<L>) {
        self.labelled = self
            .labelled
            .drain(..)
            .filter_map(|(link, text)| keep(link).map(|l| (l, text)))
            .collect();
    }

    /// Reads `key`, a key press made while hints are on.
    pub fn key(&mut self, key: HintKey) -> Step<L> {
        match key {
            HintKey::Modifier => Step::Ignored,
            HintKey::End => Step::End,
            HintKey::Backspace => {
                if self.typed.pop().is_some() {
                    Step::Typed
                } else {
                    Step::Ignored
                }
            }
            HintKey::Letter(c) => {
                let mut next = self.typed.clone();
                next.push(c);
                let mut continued = self
                    .labelled
                    .iter()
                    .filter(|(_, text)| text.starts_with(&next));
                let Some(&(first, ref text)) = continued.next() else {
                    return Step::Ignored;
                };
                if text.len() == next.len() {
                    return Step::Picked(first);
                }
                self.typed = next;
                Step::Typed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_take_the_fewest_letters_that_tell_every_link_apart() {
        assert_eq!(label_len(0), 1);
        assert_eq!(label_len(1), 1);
        assert_eq!(label_len(26), 1);
        assert_eq!(label_len(27), 2);
        assert_eq!(label_len(676), 2);
        assert_eq!(label_len(677), 3);
        assert_eq!(label_len(usize::MAX), 14, "no overflow at any count");
    }

    #[test]
    fn labels_count_through_the_home_row_first() {
        assert_eq!(labels(3), ["a", "s", "d"]);
        let one = labels(26);
        assert_eq!(one.first().map(String::as_str), Some("a"));
        assert_eq!(one.get(9).map(String::as_str), Some("q"));
        assert_eq!(one.get(19).map(String::as_str), Some("z"));
        assert_eq!(one.last().map(String::as_str), Some("m"));
        let two = labels(28);
        assert_eq!(two.first().map(String::as_str), Some("aa"));
        assert_eq!(two.get(1).map(String::as_str), Some("as"));
        assert_eq!(two.get(25).map(String::as_str), Some("am"));
        assert_eq!(two.get(26).map(String::as_str), Some("sa"));
        assert_eq!(label(675, 2), "mm");
    }

    #[test]
    fn every_label_is_distinct_and_none_begins_another() {
        let all = labels(700);
        for (i, a) in all.iter().enumerate() {
            assert_eq!(a.len(), 3);
            for b in all.iter().skip(i.saturating_add(1)) {
                assert!(!b.starts_with(a.as_str()), "{a} begins {b}");
            }
        }
    }

    #[test]
    fn links_are_ordered_by_where_each_first_appears() {
        // a 4×3 view: link 7 first on row 0 at column 2, link 5 at row 0
        // column 3, and link 7 again on row 2, which adds nothing
        let cells = [
            [None, None, Some(7), Some(5)],
            [Some(9), None, None, None],
            [Some(7), Some(5), None, Some(1)],
        ];
        let at = |x: usize, y: usize| cells.get(y).and_then(|r| r.get(x)).copied().flatten();
        assert_eq!(links_in_view(4, 3, at), [7, 5, 9, 1]);
        assert!(links_in_view(4, 3, |_, _| None::<u8>).is_empty());
    }

    #[test]
    fn a_label_past_the_last_column_moves_left_to_end_there() {
        assert_eq!(label_col(3, 2, 80), 3);
        assert_eq!(label_col(79, 1, 80), 79);
        assert_eq!(label_col(79, 2, 80), 78);
        assert_eq!(label_col(5, 3, 2), 0, "a row narrower than the label");
    }

    #[test]
    fn keys_read_as_letters_backspace_modifiers_or_the_end() {
        assert_eq!(hint_key(&[u32::from('a')], false), HintKey::Letter('a'));
        assert_eq!(hint_key(&[u32::from('S')], false), HintKey::Letter('s'));
        assert_eq!(hint_key(&[BACKSPACE], false), HintKey::Backspace);
        assert_eq!(hint_key(&[0xff1b], false), HintKey::End, "Escape");
        assert_eq!(hint_key(&[0xff0d], false), HintKey::End, "Return");
        assert_eq!(hint_key(&[u32::from('1')], false), HintKey::End);
        assert_eq!(hint_key(&[u32::from('a')], true), HintKey::End, "Ctrl+a");
        assert_eq!(hint_key(&[0xffe1], false), HintKey::Modifier, "Shift");
        assert_eq!(hint_key(&[0xffe3], true), HintKey::Modifier, "Ctrl");
        assert_eq!(hint_key(&[0xfe03], false), HintKey::Modifier, "AltGr");
        assert_eq!(hint_key(&[], false), HintKey::End);
    }

    #[test]
    fn no_link_starts_no_hints() {
        assert_eq!(Hints::<u8>::start(&[]), None);
    }

    #[test]
    fn the_last_letter_of_a_label_picks_its_link() {
        let mut h = Hints::start(&[10u8, 20, 30]).expect("hints");
        assert_eq!(h.key(HintKey::Letter('s')), Step::Picked(20));
        let links: Vec<u8> = (0..30).collect();
        let mut h = Hints::start(&links).expect("hints");
        assert_eq!(h.key(HintKey::Letter('s')), Step::Typed);
        assert!(h.shows(26), "sa, the 27th label, still shows");
        assert!(!h.shows(0), "aa no longer shows");
        assert_eq!(h.key(HintKey::Letter('d')), Step::Picked(28));
    }

    #[test]
    fn a_letter_that_continues_no_label_is_ignored() {
        let mut h = Hints::start(&[1u8, 2]).expect("hints");
        assert_eq!(h.key(HintKey::Letter('m')), Step::Ignored);
        assert_eq!(h.typed(), "");
        let links: Vec<u8> = (0..30).collect();
        let mut h = Hints::start(&links).expect("hints");
        assert_eq!(
            h.key(HintKey::Letter('d')),
            Step::Ignored,
            "no label starts d"
        );
        assert_eq!(h.key(HintKey::Letter('s')), Step::Typed);
        assert_eq!(h.key(HintKey::Letter('m')), Step::Ignored, "no sm among 30");
        assert_eq!(h.typed(), "s");
    }

    #[test]
    fn backspace_takes_back_the_last_letter_kept() {
        let links: Vec<u8> = (0..30).collect();
        let mut h = Hints::start(&links).expect("hints");
        assert_eq!(h.key(HintKey::Backspace), Step::Ignored);
        assert_eq!(h.key(HintKey::Letter('s')), Step::Typed);
        assert_eq!(h.key(HintKey::Backspace), Step::Typed);
        assert_eq!(h.typed(), "");
        assert!(h.shows(0));
        assert_eq!(h.key(HintKey::Letter('a')), Step::Typed);
        assert_eq!(h.key(HintKey::Letter('a')), Step::Picked(0));
    }

    #[test]
    fn any_other_key_ends_hints_and_a_modifier_changes_nothing() {
        let mut h = Hints::start(&[1u8]).expect("hints");
        assert_eq!(h.key(HintKey::Modifier), Step::Ignored);
        assert_eq!(h.key(HintKey::End), Step::End);
    }

    #[test]
    fn a_link_that_leaves_the_view_loses_its_label_for_good() {
        let mut h = Hints::start(&[1u8, 2, 3]).expect("hints");
        // link 2 left the view, and a sweep renamed link 3 to 2
        h.retain(|l| match l {
            1 => Some(1),
            3 => Some(2),
            _ => None,
        });
        assert_eq!(
            h.labelled(),
            [(1, "a".to_owned()), (2, "d".to_owned())],
            "each label keeps its letters, and so its link"
        );
        assert_eq!(h.key(HintKey::Letter('s')), Step::Ignored);
        assert_eq!(h.key(HintKey::Letter('d')), Step::Picked(2));
    }

    #[test]
    fn the_same_screen_always_gets_the_same_labels() {
        let a = Hints::start(&[4u8, 8, 15]);
        let b = Hints::start(&[4u8, 8, 15]);
        assert_eq!(a, b);
    }
}
