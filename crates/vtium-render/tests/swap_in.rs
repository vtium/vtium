//! Glyphs that change under a renderer (ADR-1301 D2): the deferred
//! pipeline's generated glyphs replace the built-in font's as they arrive,
//! and [`Renderer::glyphs_changed`] redraws only the cells that show one.
//! The cache drops the glyphs that hold one and keeps every other.

mod common;

use std::cell::Cell as Flag;
use std::rc::Rc;

use common::{Screen, rgb, style};
use vtium_font::{Cell, Face, Font, Format, Glyph, Style, Unifont};
use vtium_render::{Canvas, Config, DrawState, Rect, Renderer};
use vtium_term::{Age, Glyph as Shown, Width};

const FG: &str = "c0c0c0";
const BG: &str = "101010";

/// Unifont, whose `A` turns into a solid block once `swapped` is set, as
/// a generated glyph replaces the built-in's.
struct Swapping {
    unifont: Unifont,
    swapped: Rc<Flag<bool>>,
}

impl Face for Swapping {
    fn cell(&self) -> Cell {
        self.unifont.cell()
    }
    fn size_step(&self) -> u32 {
        self.unifont.size_step()
    }
    fn has_glyph(&self, c: char) -> bool {
        self.unifont.has_glyph(c)
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        if self.swapped.get() && cluster == ['A'] {
            let c = self.cell();
            let pixels = vec![255; (c.width * c.height) as usize];
            return Glyph::from_pixels(c.width, c.height, Format::Mask, pixels);
        }
        self.unifont.draw(cluster, style)
    }
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// The pixels of cell `col` of row 0, on a display exactly the grid.
fn cell_pixels(c: &Canvas, col: u32) -> Vec<u32> {
    (0..16)
        .flat_map(|y| (0..8).map(move |x| (col * 8 + x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect()
}

#[test]
fn a_changed_glyph_redraws_only_its_cells() {
    let swapped = Rc::new(Flag::new(false));
    let font = Font::new(Swapping {
        unifont: Unifont::new(16),
        swapped: Rc::clone(&swapped),
    });
    let mut s = Screen::new(4, 1, rgb(FG), rgb(BG));
    s.text(0, 0, "AB", style(rgb(FG), rgb(BG)));
    let st = state(&s);
    let mut r = Renderer::new(font, 32, 16, Config::default());
    let mut c = Canvas::new(32, 16, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    let since = s.damage.checkpoint();
    let (a, b) = (cell_pixels(&c, 0), cell_pixels(&c, 1));

    // the face changes; without a word to the renderer nothing redraws
    swapped.set(true);
    let damage = r.draw(&s.frame(since), &mut c, 1, &st).damage.to_vec();
    assert!(damage.is_empty(), "{damage:?}");

    r.glyphs_changed(&['A']);
    let damage = r.draw(&s.frame(since), &mut c, 1, &st).damage.to_vec();
    assert_eq!(
        damage,
        vec![Rect {
            x: 0,
            y: 0,
            width: 8,
            height: 16
        }]
    );
    // a solid block in the foreground, where unifont's A left background
    let solid = vec![cell_pixels(&c, 0)[0]; 8 * 16];
    assert_ne!(solid[0], c.pixel(31, 0).unwrap());
    assert_ne!(cell_pixels(&c, 0), a);
    assert_eq!(cell_pixels(&c, 0), solid);
    assert_eq!(cell_pixels(&c, 1), b);

    // once drawn the change is spent: the next frame draws nothing
    let damage = r.draw(&s.frame(since), &mut c, 1, &st).damage.to_vec();
    assert!(damage.is_empty(), "{damage:?}");

    // the cache holds no glyph from before the change: an A written later
    // draws the new one
    s.text(2, 0, "A", style(rgb(FG), rgb(BG)));
    r.draw(&s.frame(since), &mut c, 1, &st);
    assert_eq!(cell_pixels(&c, 2), solid);
}

#[test]
fn a_change_reaches_a_buffer_that_missed_it() {
    // two buffers: the change drawn into one is drawn into the other at
    // its age, so a double-buffered display shows it too
    let swapped = Rc::new(Flag::new(false));
    let font = Font::new(Swapping {
        unifont: Unifont::new(16),
        swapped: Rc::clone(&swapped),
    });
    let mut s = Screen::new(4, 1, rgb(FG), rgb(BG));
    s.text(0, 0, "AB", style(rgb(FG), rgb(BG)));
    let st = state(&s);
    let mut r = Renderer::new(font, 32, 16, Config::default());
    let (mut front, mut back) = (Canvas::new(32, 16, 0), Canvas::new(32, 16, 0));
    r.draw(&s.frame(Age::NEVER), &mut front, 0, &st);
    let since = s.damage.checkpoint();
    r.draw(&s.frame(since), &mut back, 0, &st);
    swapped.set(true);
    r.glyphs_changed(&['A']);
    r.draw(&s.frame(since), &mut front, 2, &st);
    let damage = r.draw(&s.frame(since), &mut back, 2, &st).damage.to_vec();
    assert_eq!(damage.len(), 1, "{damage:?}");
    assert_eq!(cell_pixels(&back, 0), cell_pixels(&front, 0));
}

#[test]
fn an_arrival_keeps_the_glyphs_it_did_not_change() {
    // ADR-1200 D23: an arrival reaches only what shows the arrived
    // characters, so the cache keeps every other glyph it drew
    let swapped = Rc::new(Flag::new(false));
    let font = Font::new(Swapping {
        unifont: Unifont::new(16),
        swapped: Rc::clone(&swapped),
    });
    let mut s = Screen::new(4, 1, rgb(FG), rgb(BG));
    s.text(0, 0, "AB", style(rgb(FG), rgb(BG)));
    let st = state(&s);
    let mut r = Renderer::new(font, 32, 16, Config::default());
    let mut c = Canvas::new(32, 16, 0);
    assert_eq!(r.draw(&s.frame(Age::NEVER), &mut c, 0, &st).stats.misses, 2);
    let since = s.damage.checkpoint();
    swapped.set(true);
    r.glyphs_changed(&['A']);
    // the A drawn again and the A written are one miss; the B a hit
    s.text(2, 0, "BA", style(rgb(FG), rgb(BG)));
    let stats = r.draw(&s.frame(since), &mut c, 1, &st).stats;
    assert_eq!(stats.misses, 1);
    let solid = vec![cell_pixels(&c, 0)[0]; 8 * 16];
    assert_eq!(cell_pixels(&c, 0), solid);
    assert_eq!(cell_pixels(&c, 3), solid);
    assert_eq!(cell_pixels(&c, 2), cell_pixels(&c, 1));
}

#[test]
fn a_full_redraw_after_an_arrival_draws_only_the_arrived_glyph() {
    // an 80x25 screen of printable ASCII, redrawn whole at age 0 after an
    // arrival for 'A'; the frame's time is a machine figure, printed:
    // `cargo test --release -p vtium-render --test swap_in -- --nocapture`
    let mut r = Renderer::new(Font::new(Unifont::new(16)), 640, 400, Config::default());
    let mut s = Screen::new(80, 25, rgb(FG), rgb(BG));
    for row in 0..25 {
        let line: String = (' '..='~').cycle().skip(row * 7).take(80).collect();
        s.text(0, row, &line, style(rgb(FG), rgb(BG)));
    }
    let st = state(&s);
    let mut c = Canvas::new(640, 400, 0);
    let cold = r.draw(&s.frame(Age::NEVER), &mut c, 0, &st).stats;
    assert_eq!(cold.misses, 95);
    let since = s.damage.checkpoint();
    let mut times = Vec::new();
    let mut misses = Vec::new();
    for _ in 0..21 {
        r.glyphs_changed(&['A']);
        let stats = r.draw(&s.frame(since), &mut c, 0, &st).stats;
        assert_eq!((stats.cells, stats.misses), (80 * 25, 1));
        times.push(stats.cpu);
        misses.push(stats.misses);
    }
    times.sort_unstable();
    println!(
        "80x25 full redraw after one arrival: misses {:?}, cpu min {:?} median {:?}",
        misses[0], times[0], times[10]
    );
}

#[test]
fn an_arrival_scans_only_the_rows_that_show_it() {
    // ADR-1200 D23: the cells to redraw are those whose shown text holds an
    // arrived character, and a row without one is not read for them
    let mut r = Renderer::new(Font::new(Unifont::new(16)), 32, 64, Config::default());
    let mut s = Screen::new(4, 4, rgb(FG), rgb(BG));
    let st = style(rgb(FG), rgb(BG));
    for (row, text) in ["BBBB", "BBAB", "BB", "BBB"].into_iter().enumerate() {
        s.text(0, row, text, st);
    }
    let ds = state(&s);
    let mut c = Canvas::new(32, 64, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &ds);
    let since = s.damage.checkpoint();
    r.glyphs_changed(&['A']);
    let stats = r.draw(&s.frame(since), &mut c, 1, &ds).stats;
    assert_eq!((stats.rows, stats.cells), (1, 1));

    // a cluster holding the arrived character puts its row in the scan
    s.lines[3].write(0, Shown::Cluster("A\u{301}"), Width::Narrow, &st, None);
    s.damage.mark_cell(3, 0);
    r.draw(&s.frame(since), &mut c, 1, &ds);
    let since = s.damage.checkpoint();
    r.glyphs_changed(&['A']);
    let stats = r.draw(&s.frame(since), &mut c, 1, &ds).stats;
    assert_eq!((stats.rows, stats.cells), (2, 2));
}
