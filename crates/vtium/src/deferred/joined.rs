//! Joined letters in the console's lookup (ADR-1004 D6): the glyph of a
//! letter of a joining script, such as Arabic, in the form its joining
//! sides give it, and the lam-alef ligature.
//!
//! The renderer looks a joined letter up by its key: the letter and its
//! marks, with U+200D, ZERO WIDTH JOINER, before it when it joins the
//! letter read before it and after it when it joins the letter read after
//! it. Lam followed by alef is looked up as one key of both letters, which
//! draws across their two cells (`vtium_render::joined`). The console asks
//! the glyph helper for those keys, and the helper shapes them from an
//! outline font (ADR-1301, the listable clusters).
//!
//! Where the store does not hold a key, as with a bitmap font, which the
//! helper does not shape, the letter draws the Arabic presentation form of
//! its joined form, where the font holds that codepoint: the store, or,
//! while the store holds nothing for the letter, the built-in font, as GNU
//! Unifont holds the presentation forms. Else it draws its own glyph,
//! unjoined. The tables come from the Unicode Character Database through
//! `gen-lookup` ([`super::data::FORMS`], [`super::data::LIGATURES`]).

use vtium_font::{Glyph, Variant, Width};

use super::{DeferredFace, Layers, Sides, Text, data, is_ignorable, is_mark, lock};

/// ZERO WIDTH JOINER.
const ZWJ: char = '\u{200d}';

/// ARABIC LETTER LAM.
const LAM: char = '\u{644}';

/// A joined form's number in [`data::FORMS`] and [`data::LIGATURES`].
const INITIAL: u8 = 1;
const MEDIAL: u8 = 2;
const FINAL: u8 = 3;
const ISOLATED: u8 = 4;

/// A letter and the marks on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Part {
    letter: char,
    marks: Vec<char>,
}

/// A key the renderer draws a joined letter, or the lam-alef ligature, by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Key {
    /// One letter, joined on `sides`.
    Letter { sides: Sides, part: Part },
    /// Lam and an alef as their ligature, lam joining the letter before
    /// it where `before` says.
    Ligature { before: bool, lam: Part, alef: Part },
}

impl Part {
    /// The letter and its marks, in written order.
    fn text(&self) -> Text {
        let mut text = vec![self.letter];
        text.extend_from_slice(&self.marks);
        text
    }
}

/// Each letter of `inner` with the marks after it; `None` where it starts
/// with a mark.
fn parts(inner: &[char]) -> Option<Vec<Part>> {
    let mut out: Vec<Part> = Vec::new();
    for &c in inner {
        if is_mark(c) || is_ignorable(c) {
            out.last_mut()?.marks.push(c);
        } else {
            out.push(Part {
                letter: c,
                marks: Vec::new(),
            });
        }
    }
    Some(out)
}

/// Whether lam and `alef` join into a ligature that Arabic Presentation
/// Forms-B holds.
fn ligates(alef: char) -> bool {
    let alef = u32::from(alef);
    data::LIGATURES.iter().any(|&(_, a, _, _)| a == alef)
}

impl Key {
    /// The key `cluster` is: a letter with U+200D at either end, or lam and
    /// an alef that join, with U+200D before them where lam joins the
    /// letter before it. `None` for any other cluster, which the lookup
    /// draws as ADR-1200 D22 says.
    pub(super) fn of(cluster: &[char]) -> Option<Key> {
        let (before, rest) = match cluster.split_first() {
            Some((&ZWJ, rest)) => (true, rest),
            _ => (false, cluster),
        };
        let (after, inner) = match rest.split_last() {
            Some((&ZWJ, inner)) => (true, inner),
            _ => (false, rest),
        };
        match parts(inner)?.as_slice() {
            [lam, alef] if lam.letter == LAM && ligates(alef.letter) && !after => {
                Some(Key::Ligature {
                    before,
                    lam: lam.clone(),
                    alef: alef.clone(),
                })
            }
            [part] if before || after => Some(Key::Letter {
                sides: Sides { before, after },
                part: part.clone(),
            }),
            _ => None,
        }
    }
}

/// The form a letter joined on `sides` takes.
fn form(sides: Sides) -> u8 {
    match (sides.before, sides.after) {
        (false, true) => INITIAL,
        (true, true) => MEDIAL,
        (true, false) => FINAL,
        (false, false) => ISOLATED,
    }
}

/// The presentation form that draws `letter` joined on `sides`, if Unicode
/// has one.
fn letter_form(letter: char, sides: Sides) -> Option<char> {
    let key = (u32::from(letter), form(sides));
    let i = data::FORMS
        .binary_search_by(|&(l, f, _)| (l, f).cmp(&key))
        .ok()?;
    data::FORMS.get(i).and_then(|&(_, _, p)| char::from_u32(p))
}

/// The presentation form of the lam-alef ligature of `alef`, final where
/// lam joins the letter before it, else isolated.
fn ligature_form(alef: char, before: bool) -> Option<char> {
    let form = if before { FINAL } else { ISOLATED };
    let alef = u32::from(alef);
    data::LIGATURES
        .iter()
        .find(|&&(_, a, f, _)| a == alef && f == form)
        .and_then(|&(_, _, _, p)| char::from_u32(p))
}

/// The presentation form a joined letter's or a lam-alef ligature's key
/// draws where the store holds no glyph for the key: the console asks the
/// helper for it beside the key, so a bitmap font that holds it draws it.
/// `None` for a key with no presentation form, and for any other cluster.
pub fn presentation(cluster: &[char]) -> Option<char> {
    match Key::of(cluster)? {
        Key::Letter { sides, part } => letter_form(part.letter, sides),
        Key::Ligature { before, alef, .. } => ligature_form(alef.letter, before),
    }
}

/// The letters `c` presents when it is an Arabic presentation form the
/// lookup draws, so the cells that show those letters redraw when its
/// glyph arrives; empty for any other character.
pub fn presented(c: char) -> Vec<char> {
    let c = u32::from(c);
    let one = data::FORMS
        .iter()
        .filter(|&&(_, _, p)| p == c)
        .map(|&(l, _, _)| [l, 0]);
    let two = data::LIGATURES
        .iter()
        .filter(|&&(_, _, _, p)| p == c)
        .map(|&(l, a, _, _)| [l, a]);
    one.chain(two)
        .flatten()
        .filter(|&l| l != 0)
        .filter_map(char::from_u32)
        .collect()
}

impl DeferredFace {
    /// The glyph `key` draws when the store does not hold the key itself.
    pub(super) fn joined(&self, key: &Key, variant: Variant) -> Option<Glyph> {
        match key {
            Key::Letter { sides, part } => self.unshaped(*sides, &part.text(), variant),
            Key::Ligature { before, lam, alef } => self.ligature((*before, lam, alef), variant),
        }
    }

    /// `letter`'s glyph joined on `sides`, before its marks are laid on it:
    /// the store's glyph for the letter's key, else its presentation form,
    /// else its own glyph, unjoined.
    pub(super) fn form(&self, sides: Sides, letter: char, variant: Variant) -> Option<Glyph> {
        let key: Text = sides.key(vec![letter]);
        let held = lock(&self.generated).draw(self.at(), &key, variant);
        held.or_else(|| self.presented_as(letter_form(letter, sides)?, letter, variant))
            .or_else(|| self.single(letter, variant))
    }

    /// The glyph of `form`, a presentation form that draws `letter`, from
    /// the same place `letter` would draw from: the store, where it holds
    /// `form`; else the built-in font, while the store holds nothing for
    /// `letter`, so a letter the frit draws never takes a joined form from
    /// another font.
    fn presented_as(&self, form: char, letter: char, variant: Variant) -> Option<Glyph> {
        let store = lock(&self.generated);
        if let Some(g) = store.draw(self.at(), &[form], variant) {
            return Some(g);
        }
        if store.held_any(self.at(), &[letter]) || !self.builtin.has_glyph(form) {
            return None;
        }
        drop(store);
        self.built_in_variant(form, variant)
    }

    /// The lam-alef ligature, across two cells: its presentation form
    /// with the marks of both letters laid on it, where the font holds the
    /// form; else lam in its joined form in the right cell and alef in its
    /// final form in the left, each with its marks.
    fn ligature(
        &self,
        (before, lam, alef): (bool, &Part, &Part),
        variant: Variant,
    ) -> Option<Glyph> {
        if let Some(form) = ligature_form(alef.letter, before)
            && self.presented_as(form, lam.letter, variant).is_some()
        {
            let mut text = vec![form];
            text.extend(lam.marks.iter().chain(&alef.marks));
            return self.unshaped(Sides::default(), &text, variant);
        }
        let right = self.unshaped(
            Sides {
                before,
                after: true,
            },
            &lam.text(),
            variant,
        )?;
        let left = self.unshaped(
            Sides {
                before: true,
                after: false,
            },
            &alef.text(),
            variant,
        )?;
        let cell = self.cell;
        let mut layers = Layers::new(cell.width.checked_mul(2)?, cell.height, Width::Wide)?;
        layers.lay(&left, 0);
        layers.lay(&right, i64::from(cell.width));
        layers.glyph()
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::super::{Generated, Shared};
    use super::*;
    use std::sync::Arc;
    use vtium_font::set::{PixelFormat, Set, SetBuilder, StrikeBuilder};
    use vtium_font::spacing::SpacedCell;
    use vtium_font::{Cell, Face, Font, Style, Unifont, Variant};

    const CELL: Cell = Cell {
        width: 8,
        height: 16,
    };
    const SHADDA: char = '\u{651}';

    /// A store at [`CELL`] holding each of `glyphs`, a text and the shade
    /// its glyph is drawn in, every pixel of it.
    fn store(glyphs: &[(&[char], u8)]) -> Shared {
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        let n = (CELL.width * CELL.height) as usize;
        let mut runs = Vec::new();
        for &(text, shade) in glyphs {
            let g = s.glyph(Width::Narrow, vec![shade; n]);
            match text {
                [c] => {
                    s.map(*c, g);
                    runs.push((u32::from(*c), 1));
                }
                _ => s.cluster(text, g),
            }
        }
        let set =
            Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap());
        let generated = Shared::default();
        {
            let mut g: std::sync::MutexGuard<'_, Generated> = lock(&generated);
            let fresh = g
                .fresh((SpacedCell::plain(CELL), Variant::default()), set, &runs, 1)
                .unwrap();
            g.add(fresh);
        }
        generated
    }

    fn face(generated: &Shared) -> DeferredFace {
        DeferredFace::new(Box::new(Unifont::new(16)), CELL, 16, Arc::clone(generated))
    }

    /// `c`'s glyph in the built-in Unifont.
    fn unifont(c: char) -> Glyph {
        Unifont::new(16).draw(&[c], Style::default()).unwrap()
    }

    /// Whether every pixel of `g` is `shade`.
    fn all(g: &Glyph, shade: u8) -> bool {
        g.pixels().iter().all(|&p| p == shade)
    }

    fn draw(f: &DeferredFace, text: &[char]) -> Glyph {
        f.draw(text, Style::default()).unwrap()
    }

    const BEH: char = '\u{628}';
    const ALEF: char = '\u{627}';
    const FATHA: char = '\u{64e}';

    // a letter keyed with U+200D on the sides it joins, and lam with alef,
    // are keys; a letter alone, an emoji sequence and lam with a letter
    // that makes no ligature are not
    #[test]
    fn a_key_names_a_joined_letter_or_the_lam_alef_ligature() {
        let letter = |before, after, marks: &[char]| Key::Letter {
            sides: Sides { before, after },
            part: Part {
                letter: BEH,
                marks: marks.to_vec(),
            },
        };
        assert_eq!(Key::of(&[BEH, ZWJ]), Some(letter(false, true, &[])));
        assert_eq!(Key::of(&[ZWJ, BEH, ZWJ]), Some(letter(true, true, &[])));
        assert_eq!(
            Key::of(&[ZWJ, BEH, FATHA]),
            Some(letter(true, false, &[FATHA]))
        );
        assert!(matches!(
            Key::of(&[ZWJ, LAM, ALEF]),
            Some(Key::Ligature { before: true, .. })
        ));
        assert!(matches!(
            Key::of(&[LAM, FATHA, ALEF]),
            Some(Key::Ligature { before: false, .. })
        ));
        assert_eq!(Key::of(&[BEH, FATHA]), None);
        assert_eq!(Key::of(&['\u{1f468}', ZWJ, '\u{1f469}']), None);
        assert_eq!(Key::of(&[LAM, BEH]), None);
    }

    // Unicode's presentation forms of beh, and of lam-alef final and
    // isolated
    #[test]
    fn a_key_has_its_presentation_form() {
        assert_eq!(presentation(&[BEH, ZWJ]), Some('\u{fe91}'));
        assert_eq!(presentation(&[ZWJ, BEH, ZWJ]), Some('\u{fe92}'));
        assert_eq!(presentation(&[ZWJ, BEH]), Some('\u{fe90}'));
        assert_eq!(presentation(&[LAM, ALEF]), Some('\u{fefb}'));
        assert_eq!(presentation(&[ZWJ, LAM, ALEF]), Some('\u{fefc}'));
        // alef joins only the letter before it: no initial form
        assert_eq!(presentation(&[ALEF, ZWJ]), None);
        assert_eq!(presented('\u{fe92}'), vec![BEH]);
        assert_eq!(presented('\u{fefc}'), vec![LAM, ALEF]);
        assert!(presented('a').is_empty());
    }

    // ADR-1004 D6: with nothing in the store, a joined letter draws the
    // built-in font's presentation form of its joined form, as GNU Unifont
    // holds them, and a letter with no such form its own glyph
    // covers: text.bidi
    #[test]
    fn the_built_in_font_draws_a_joined_letter_by_its_presentation_form() {
        let f = face(&Shared::default());
        assert_eq!(draw(&f, &[BEH, ZWJ]), unifont('\u{fe91}'));
        assert_eq!(draw(&f, &[ZWJ, BEH, ZWJ]), unifont('\u{fe92}'));
        assert_eq!(draw(&f, &[ZWJ, BEH]), unifont('\u{fe90}'));
        assert_ne!(unifont('\u{fe91}'), unifont(BEH));
        // Syriac has no presentation forms: the letter's own glyph
        let alaph = '\u{710}';
        assert_eq!(draw(&f, &[ZWJ, alaph]), unifont(alaph));
        // through the font, which asks by the key's first character
        let font = Font::new(face(&Shared::default()));
        let drawn = font
            .glyph(&[ZWJ, BEH], vtium_font::Width::Narrow, Style::default())
            .unwrap();
        assert_eq!(drawn, unifont('\u{fe90}'));
    }

    // ADR-1004 D6: the store's glyph for a joined form comes first, and its
    // presentation form, where the frit holds that, next; a letter the
    // frit draws never takes the built-in font's presentation form
    // covers: text.bidi
    #[test]
    fn a_joined_letter_draws_from_the_store_before_any_presentation_form() {
        let f = face(&store(&[(&[BEH, ZWJ], 7), (&[BEH], 255)]));
        assert!(all(&draw(&f, &[BEH, ZWJ]), 7), "the shaped form");
        assert!(all(&draw(&f, &[ZWJ, BEH]), 255), "the frit's own beh");
        let f = face(&store(&[(&[BEH], 255), (&['\u{fe90}'], 9)]));
        assert!(all(&draw(&f, &[ZWJ, BEH]), 9), "the frit's final form");
    }

    // ADR-1004 D6: lam and alef draw the ligature the store holds; else its
    // presentation form; else lam's joined form in the right cell and
    // alef's final form in the left
    // covers: text.bidi
    #[test]
    fn lam_and_alef_draw_their_ligature_or_their_two_forms() {
        let f = face(&store(&[(&[LAM, ALEF], 5)]));
        assert!(all(&draw(&f, &[LAM, ALEF]), 5));
        let f = face(&Shared::default());
        assert_eq!(draw(&f, &[ZWJ, LAM, ALEF]), unifont('\u{fefc}'));
        let halves = store(&[(&[LAM], 255), (&[LAM, ZWJ], 3), (&[ZWJ, ALEF], 4)]);
        let g = draw(&face(&halves), &[LAM, ALEF]);
        assert_eq!((g.width(), g.cells()), (16, Width::Wide));
        let shade = |x: u32| g.pixel(x, 8).unwrap().first().copied();
        assert_eq!(
            (shade(0), shade(15)),
            (Some(4), Some(3)),
            "alef left, lam right"
        );
    }

    // ADR-1200 D22 step 3 for a joined letter: the store's part of the key,
    // the letter joined with the marks the font has, takes the mark the
    // font lacks over it
    // covers: text.overhang
    #[test]
    fn a_joined_letter_draws_the_part_of_it_the_store_holds() {
        let f = face(&store(&[(&[ZWJ, BEH, SHADDA, ZWJ], 6)]));
        let g = draw(&f, &[ZWJ, BEH, SHADDA, FATHA, ZWJ]);
        // the part's shade, with Unifont's fatha laid over it
        assert!(g.pixels().iter().all(|&p| p == 6 || p == 255));
        assert!(g.pixels().contains(&255), "the fatha over it");
    }
}
