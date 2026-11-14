//! Syllables drawn joined across their clusters (ADR-1203): each cell of a
//! syllable whose clusters join draws its piece of the syllable's glyph,
//! keyed by the syllable's text and the piece's mark, and the syllable
//! draws split, each cluster on its own, where ADR-1203 D4 says. The
//! frames come from vtium-term's own terminal, which cuts the text into
//! clusters by UAX #29.
#![cfg(feature = "bidi")]

use std::cell::{Cell as Flag, RefCell};
use std::rc::Rc;

use vtium_font::syllable::{Piece, piece_of_chars};
use vtium_font::{
    Cell, Face, Font, Format, Glyph as FontGlyph, Style as FontStyle, Unifont, Variant,
    Width as FontWidth,
};
use vtium_render::{Canvas, Config, DrawState, Rect, Renderer};
use vtium_term::{Age, Frame, Rgb, Terminal};

/// `c` as a pixel's red, green and blue.
fn px(c: Rgb) -> u32 {
    u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)
}

const KA: char = '\u{c95}';
const RA: char = '\u{cb0}';
const VIRAMA: char = '\u{ccd}';

/// Unifont, recording each cluster the renderer asks it to draw, and
/// holding each piece of a syllable while `holds` is set. A piece draws
/// its top half inked across its cells, so a picture shows where it lies.
#[derive(Clone)]
struct Pieces {
    font: Unifont,
    drawn: Rc<RefCell<Vec<Vec<char>>>>,
    holds: Rc<Flag<bool>>,
}

impl Pieces {
    fn new() -> Pieces {
        Pieces {
            font: Unifont::new(16),
            drawn: Rc::default(),
            holds: Rc::new(Flag::new(true)),
        }
    }
}

impl Face for Pieces {
    fn cell(&self) -> Cell {
        self.font.cell()
    }

    fn size_step(&self) -> u32 {
        self.font.size_step()
    }

    fn has_glyph(&self, c: char) -> bool {
        self.font.has_glyph(c)
    }

    fn draw(&self, cluster: &[char], style: FontStyle) -> Option<FontGlyph> {
        self.drawn.borrow_mut().push(cluster.to_vec());
        if let Some((_, piece)) = piece_of_chars(cluster) {
            let w = 8 * u32::from(piece.cells);
            let pixels = (0..16).flat_map(|y| (0..w).map(move |_| if y < 8 { 255 } else { 0 }));
            let cells = if piece.cells == 2 {
                FontWidth::Wide
            } else {
                FontWidth::Narrow
            };
            return Some(
                FontGlyph::from_pixels(w, 16, Format::Mask, pixels.collect())?.with_cells(cells),
            );
        }
        self.font.draw(&cluster[..1], style)
    }

    fn holds(&self, cluster: &[char], _variant: Variant) -> bool {
        self.holds.get() && piece_of_chars(cluster).is_some()
    }
}

/// The key of the piece from cell `start` of a syllable of `total` cells,
/// one cell wide.
fn key(text: &[char], start: u8, total: u8) -> Vec<char> {
    let mut k = Vec::new();
    Piece::new(start, 1, total).unwrap().key(text, &mut k);
    k
}

const KA_RA: [char; 3] = [KA, VIRAMA, RA];

fn term(bytes: &str) -> Terminal {
    let mut t = Terminal::new(6, 1, 0);
    t.input(bytes.as_bytes());
    t
}

fn state(t: &Terminal) -> DrawState {
    DrawState {
        cursor_on: true,
        background: t.palette().bg(),
        ..DrawState::default()
    }
}

/// The clusters the renderer asked `face` for when it drew `frame` whole,
/// each once, sorted.
fn asked_in(t: &Terminal, frame: &Frame<'_>, face: Pieces) -> Vec<Vec<char>> {
    let drawn = Rc::clone(&face.drawn);
    let mut r = Renderer::new(Font::new(face), 48, 16, Config::default());
    let mut c = Canvas::new(48, 16, 0);
    r.draw(frame, &mut c, 0, &state(t));
    let mut out = drawn.take();
    out.sort();
    out.dedup();
    out
}

fn asked(t: &Terminal) -> Vec<Vec<char>> {
    asked_in(t, &t.screen().frame(t.palette(), Age::NEVER), Pieces::new())
}

/// Whether the renderer drew `t`'s ka and ra joined: each by its piece, and
/// neither cluster on its own.
fn joined(got: &[Vec<char>]) -> bool {
    let pieces = got.contains(&key(&KA_RA, 0, 2)) && got.contains(&key(&KA_RA, 1, 2));
    let own = got.contains(&vec![KA, VIRAMA]) || got.contains(&vec![RA]);
    assert!(pieces != own, "both or neither: {got:?}");
    pieces
}

// ADR-1203 D3: ka, virama and ra, two clusters, draw joined: each cell
// asks for its piece of the syllable, keyed by the whole text and the
// piece's place, and the piece is drawn in the cell's own cells
#[test]
fn a_syllable_draws_a_piece_in_each_of_its_cells() {
    let t = term("ಕ್ರ");
    assert!(joined(&asked(&t)));
    let face = Pieces::new();
    let frame = t.screen().frame(t.palette(), Age::NEVER);
    let mut r = Renderer::new(Font::new(face), 48, 16, Config::default());
    let mut c = Canvas::new(48, 16, 0);
    r.draw(&frame, &mut c, 0, &state(&t));
    let fg = t.palette().fg();
    for x in [0, 7, 8, 15] {
        let top = c.pixel(x, 2).unwrap();
        let bottom = c.pixel(x, 12).unwrap();
        assert_eq!(top & 0xff_ffff, px(fg), "column {x}, top");
        assert_ne!(bottom, top, "column {x}, bottom");
    }
}

// ADR-1203 D3: each cell keeps its own background and underline under its
// piece, and a background or an underline does not split the syllable
#[test]
fn each_cell_keeps_its_own_background_and_underline() {
    let t = term("ಕ್\x1b[41;4mರ");
    assert!(joined(&asked(&t)));
    let frame = t.screen().frame(t.palette(), Age::NEVER);
    let mut r = Renderer::new(Font::new(Pieces::new()), 48, 16, Config::default());
    let mut c = Canvas::new(48, 16, 0);
    r.draw(&frame, &mut c, 0, &state(&t));
    let red = px(frame.cell(0, 1).unwrap().bg);
    let bg = px(frame.cell(0, 0).unwrap().bg);
    assert_ne!(red, bg);
    let c = &c;
    let row = move |x0: u32, y: u32| (x0..x0 + 8).map(move |x| c.pixel(x, y).unwrap() & 0xff_ffff);
    assert!(row(0, 10).all(|p| p == bg), "ka's background");
    assert!(row(8, 10).all(|p| p == red), "ra's background");
    // the underline lies in ra's cell alone
    let lined = |x0: u32| (8..16).any(|y| row(x0, y).all(|p| p != bg && p != red));
    assert!(lined(8) && !lined(0), "the underline under ra alone");
}

// ADR-1203 D4: the cursor on either cell, in either phase of its blink,
// draws the syllable split, each cluster on its own
#[test]
fn the_cursor_on_a_syllable_draws_it_split() {
    for col in [1, 2] {
        let t = term(&format!("ಕ್ರ\x1b[1;{col}H"));
        assert!(!joined(&asked(&t)), "cursor at {col}");
    }
    let t = term("ಕ್ರ\x1b[1;1H");
    let frame = t.screen().frame(t.palette(), Age::NEVER);
    let face = Pieces::new();
    let drawn = Rc::clone(&face.drawn);
    let mut r = Renderer::new(Font::new(face), 48, 16, Config::default());
    let mut c = Canvas::new(48, 16, 0);
    let st = DrawState {
        cursor_on: false,
        ..state(&t)
    };
    r.draw(&frame, &mut c, 0, &st);
    assert!(!joined(&drawn.take()), "the cursor's off phase");
}

// ADR-1203 D4: cells that differ in foreground color, in style or in
// script draw split; a cell that differs in background alone stays joined
#[test]
fn a_change_of_color_style_or_script_draws_a_syllable_split() {
    for (what, sgr) in [
        ("color", "31"),
        ("bold", "1"),
        ("italic", "3"),
        ("superscript", "73"),
        ("subscript", "74"),
    ] {
        let t = term(&format!("ಕ್\x1b[{sgr}mರ"));
        assert!(!joined(&asked(&t)), "{what}");
        let t = term(&format!("\x1b[{sgr}mಕ್ರ"));
        assert!(joined(&asked(&t)), "{what} on both");
    }
    assert!(joined(&asked(&term("ಕ್\x1b[44mರ"))), "a background");
}

// ADR-1203 D4: a selection over part of the syllable gives its cells two
// colors and draws it split; one over all of it draws it joined
#[test]
fn a_partial_selection_draws_a_syllable_split() {
    let t = term("ಕ್ರ\x1b[1;6H");
    let frame = |cols| {
        t.screen()
            .frame(t.palette(), Age::NEVER)
            .with_selection(vec![cols])
    };
    assert!(!joined(&asked_in(&t, &frame(0..1), Pieces::new())), "ka");
    assert!(!joined(&asked_in(&t, &frame(1..2), Pieces::new())), "ra");
    assert!(joined(&asked_in(&t, &frame(0..2), Pieces::new())), "both");
}

// ADR-1203 D4: the link overlay over one of its cells draws the rest of
// the syllable split
#[test]
fn the_link_overlay_over_a_cell_draws_a_syllable_split() {
    let t = term("ಕ್ರ\x1b[1;6H");
    let frame = t
        .screen()
        .frame(t.palette(), Age::NEVER)
        .with_hover(None, Some((0, "x")));
    let got = asked_in(&t, &frame, Pieces::new());
    assert!(got.contains(&vec![RA]), "{got:?}");
    assert!(!got.contains(&key(&KA_RA, 1, 2)), "{got:?}");
}

// ADR-1203 D4: while the font lacks a piece, as before the helper
// answers, the syllable draws split
#[test]
fn a_syllable_the_font_lacks_draws_split() {
    let t = term("ಕ್ರ");
    let face = Pieces::new();
    face.holds.set(false);
    let frame = t.screen().frame(t.palette(), Age::NEVER);
    assert!(!joined(&asked_in(&t, &frame, face)));
}

/// Whether some rectangle of `damage` covers the cell at `col` of row 0.
fn damaged(damage: &[Rect], col: u32) -> bool {
    damage
        .iter()
        .any(|d| d.y == 0 && d.x <= col * 8 && d.x + d.width >= col * 8 + 8)
}

// ADR-1203 D4: a change to one cell of a joined syllable draws all its
// cells again, and so do the cursor coming onto it, a cell leaving it and
// the pieces arriving; a frame that changes none of them draws none
#[test]
fn a_change_to_one_cell_draws_the_whole_syllable_again() {
    let mut t = term("ಕ್ರ  x\x1b[1;6H");
    let face = Pieces::new();
    let holds = Rc::clone(&face.holds);
    let mut r = Renderer::new(Font::new(face), 48, 16, Config::default());
    let mut c = Canvas::new(48, 16, 0);
    let mut since = Age::NEVER;
    let mut step = |t: &mut Terminal, r: &mut Renderer<Pieces>, age: u32| {
        let frame = t.screen().frame(t.palette(), since);
        let damage = r.draw(&frame, &mut c, age, &state(t)).damage.to_vec();
        since = t.screen_mut().checkpoint();
        damage
    };
    step(&mut t, &mut r, 0);
    assert!(step(&mut t, &mut r, 1).is_empty(), "nothing changed");
    // ra becomes ka: ka, virama and ka are still a syllable, and ka with
    // its virama draws its new piece
    t.input("\x1b[1;2Hಕ\x1b[1;6H".as_bytes());
    let d = step(&mut t, &mut r, 1);
    assert!(damaged(&d, 0) && damaged(&d, 1), "{d:?}");
    // the x after it changes alone
    t.input("\x1b[1;5Hy\x1b[1;6H".as_bytes());
    let d = step(&mut t, &mut r, 1);
    assert!(
        !damaged(&d, 0) && !damaged(&d, 1) && damaged(&d, 4),
        "{d:?}"
    );
    // the cursor comes onto the second cell
    t.input("\x1b[1;2H".as_bytes());
    let d = step(&mut t, &mut r, 1);
    assert!(damaged(&d, 0) && damaged(&d, 1), "{d:?}");
    t.input("\x1b[1;6H".as_bytes());
    step(&mut t, &mut r, 1);
    // the second cell leaves the syllable
    t.input("\x1b[1;2Ha\x1b[1;6H".as_bytes());
    let d = step(&mut t, &mut r, 1);
    assert!(damaged(&d, 0), "ka with its virama draws split: {d:?}");
    // back to ka and ra, drawn split while the font lacks the pieces, and
    // joined once they arrive
    holds.set(false);
    t.input("\x1b[1;2Hರ\x1b[1;6H".as_bytes());
    step(&mut t, &mut r, 1);
    holds.set(true);
    r.glyphs_changed(&KA_RA);
    let d = step(&mut t, &mut r, 1);
    assert!(damaged(&d, 0) && damaged(&d, 1), "{d:?}");
}

// ADR-1203 D2, D3: a cluster two cells wide, as ADR-1006 widens a cluster
// with a spacing mark, draws its piece across both its cells, and the
// piece's mark names its two cells among the syllable's three
#[test]
fn a_wide_cluster_draws_its_piece_across_its_two_cells() {
    use vtium_term::{Cursor, Damage, Glyph, Style, Width};
    let t = term("ಕ್");
    let mut l = t.screen().line(0).unwrap().clone();
    l.write(1, Glyph::Char(RA), Width::Wide, &Style::DEFAULT, None);
    l.write(2, Glyph::Empty, Width::WideTail, &Style::DEFAULT, None);
    let damage = Damage::new(6, 1);
    let frame = Frame::new(
        vec![&l],
        &damage,
        t.palette(),
        Age::NEVER,
        Cursor::default(),
        false,
    );
    let face = Pieces::new();
    let drawn = Rc::clone(&face.drawn);
    let mut r = Renderer::new(Font::new(face), 48, 16, Config::default());
    let mut c = Canvas::new(48, 16, 0);
    r.draw(&frame, &mut c, 0, &state(&t));
    let got = drawn.take();
    let mut wide = Vec::new();
    Piece::new(1, 2, 3).unwrap().key(&KA_RA, &mut wide);
    assert!(got.contains(&wide), "{got:?}");
    assert!(got.contains(&key(&KA_RA, 0, 3)), "{got:?}");
    let fg = px(t.palette().fg());
    for x in [8, 15, 16, 23] {
        assert_eq!(c.pixel(x, 2).unwrap() & 0xff_ffff, fg, "column {x}");
    }
}
