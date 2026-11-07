//! Right-to-left text as the renderer draws it (ADR-1004 D6, D7, D9): each
//! Arabic letter looked up by its joining key, lam and alef as one
//! ligature across their two cells, a mirrored cluster by its counterpart,
//! and the bar cursor on the right side of a cell that resolves right to
//! left. The frames come from vtium-term's own terminal, which reorders
//! each row and gives each cell its joining sides.
#![cfg(feature = "bidi")]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::rgb;
use vtium_font::{Cell, Face, Font, Glyph as FontGlyph, Style as FontStyle, Unifont};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Terminal};

const ZWJ: char = '\u{200d}';
const BEH: char = '\u{628}';
const YEH: char = '\u{64a}';
const TEH: char = '\u{62a}';
const LAM: char = '\u{644}';
const ALEF: char = '\u{627}';
const FATHA: char = '\u{64e}';

/// Unifont, recording each cluster the renderer asks it to draw.
#[derive(Clone)]
struct Recording {
    font: Unifont,
    drawn: Rc<RefCell<Vec<Vec<char>>>>,
}

impl Face for Recording {
    fn cell(&self) -> Cell {
        self.font.cell()
    }

    fn size_step(&self) -> u32 {
        self.font.size_step()
    }

    fn has_glyph(&self, c: char) -> bool {
        c == ZWJ || self.font.has_glyph(c)
    }

    fn draw(&self, cluster: &[char], style: FontStyle) -> Option<FontGlyph> {
        self.drawn.borrow_mut().push(cluster.to_vec());
        let first = cluster.iter().copied().find(|&c| c != ZWJ)?;
        self.font.draw(&[first], style)
    }
}

/// A terminal `cols` wide and one row high that took `bytes`.
fn term(cols: usize, bytes: &[u8]) -> Terminal {
    let mut t = Terminal::new(cols, 1, 0);
    t.input(bytes);
    t
}

/// The clusters the renderer asked the face for when it drew `t` whole,
/// each once, sorted.
fn asked(t: &Terminal) -> Vec<Vec<char>> {
    let drawn = Rc::new(RefCell::new(Vec::new()));
    let face = Recording {
        font: Unifont::new(16),
        drawn: Rc::clone(&drawn),
    };
    let frame = t.screen().frame(t.palette(), Age::NEVER);
    let (w, h) = (u32::try_from(frame.cols()).unwrap() * 8, 16);
    let mut r = Renderer::new(Font::new(face), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on: true,
        background: rgb("000000"),
        ..DrawState::default()
    };
    r.draw(&frame, &mut c, 0, &st);
    let mut out = drawn.take();
    out.sort();
    out.dedup();
    out
}

/// `t` drawn in Unifont with the cursor shown.
fn draw(t: &Terminal) -> Canvas {
    let frame = t.screen().frame(t.palette(), Age::NEVER);
    let (w, h) = (u32::try_from(frame.cols()).unwrap() * 8, 16);
    let mut r = Renderer::new(Font::new(Unifont::new(16)), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on: true,
        background: t.palette().bg(),
        ..DrawState::default()
    };
    r.draw(&frame, &mut c, 0, &st);
    c
}

// ADR-1004 D6: beh, yeh and teh written in that order join into one word,
// and the renderer asks for beh's initial form, yeh's medial and teh's
// final, each keyed with U+200D on the side it joins
// covers: text.bidi
#[test]
fn each_letter_of_an_arabic_word_is_drawn_by_its_joining_key() {
    let t = term(8, "بيت".as_bytes());
    let got = asked(&t);
    for key in [vec![BEH, ZWJ], vec![ZWJ, YEH, ZWJ], vec![ZWJ, TEH]] {
        assert!(got.contains(&key), "{key:?} among {got:?}");
    }
    assert!(!got.contains(&vec![BEH]), "beh alone, unjoined: {got:?}");
}

// a mark stays inside its letter's key, the joiners outside both
// covers: text.bidi
#[test]
fn a_letter_with_a_mark_is_keyed_with_its_mark() {
    let t = term(8, "بَب".as_bytes());
    let got = asked(&t);
    assert!(got.contains(&vec![BEH, FATHA, ZWJ]), "{got:?}");
    assert!(got.contains(&vec![ZWJ, BEH]), "{got:?}");
}

// ADR-1004 D6: lam then alef draw as one ligature across their two cells,
// keyed by both letters, and neither letter is drawn on its own
// covers: text.bidi
#[test]
fn lam_and_alef_draw_as_one_ligature_across_their_cells() {
    let t = term(8, "لا".as_bytes());
    let got = asked(&t);
    assert!(got.contains(&vec![LAM, ALEF]), "{got:?}");
    assert!(!got.contains(&vec![LAM, ZWJ]), "{got:?}");
    assert!(!got.contains(&vec![ZWJ, ALEF]), "{got:?}");
    // after beh, lam joins the letter before it too
    let t = term(8, "بلا".as_bytes());
    assert!(asked(&t).contains(&vec![ZWJ, LAM, ALEF]));
}

// with the visible cursor on either letter, each draws its own joined form,
// so the cursor marks one letter
// covers: text.bidi
#[test]
fn the_cursor_on_a_ligature_draws_its_letters_apart() {
    for col in [1, 2] {
        let t = term(8, format!("لا\x1b[1;{col}H").as_bytes());
        let got = asked(&t);
        assert!(got.contains(&vec![LAM, ZWJ]), "cursor at {col}: {got:?}");
        assert!(got.contains(&vec![ZWJ, ALEF]), "cursor at {col}: {got:?}");
        assert!(!got.contains(&vec![LAM, ALEF]), "cursor at {col}: {got:?}");
    }
}

// ADR-1004 D7: a mirrored character with a mark on it draws its
// counterpart with the mark; inside Hebrew, `(` draws as `)`
// covers: text.bidi
#[test]
fn a_mirrored_cluster_draws_its_counterpart_with_its_marks() {
    let t = term(8, "ש(\u{301}ש".as_bytes());
    let got = asked(&t);
    assert!(got.contains(&vec![')', '\u{301}']), "{got:?}");
    assert!(!got.contains(&vec!['(', '\u{301}']), "{got:?}");
}

/// The columns of `c`'s cell `col` that hold any pixel of `fg`.
fn inked(c: &Canvas, col: usize, fg: u32) -> Vec<usize> {
    let w = c.width() as usize;
    (0..8)
        .filter(|x| {
            (0..16).any(|y| {
                let px = c.pixels()[y * w + col * 8 + x];
                px & 0xff_ffff == fg & 0xff_ffff
            })
        })
        .collect()
}

// ADR-1004 D9: a bar cursor stands on the right side of a cell whose
// character resolves right to left, and on the left side otherwise
// covers: text.bidi
#[test]
fn a_bar_cursor_stands_on_the_right_of_a_right_to_left_cell() {
    let fg = 0x00ff_00ff;
    // shin is written first, so it is drawn rightmost of the four letters,
    // at column 3; OSC 12 colors the bar, and DECSCUSR 6 makes it a bar
    let t = term(8, "שלום\x1b]12;#ff00ff\x07\x1b[6 q\x1b[1;1H".as_bytes());
    assert_eq!(inked(&draw(&t), 3, fg), vec![7], "the bar at the right");
    let t = term(8, "abcd\x1b]12;#ff00ff\x07\x1b[6 q\x1b[1;1H".as_bytes());
    assert_eq!(inked(&draw(&t), 0, fg), vec![0], "the bar at the left");
}
