//! Glyphs that change under a renderer (ADR-1201 D1): the deferred
//! pipeline's generated glyphs replace the built-in font's as they arrive,
//! and [`Renderer::glyphs_changed`] redraws only the cells that show one,
//! with no glyph the cache held from before.

mod common;

use std::cell::Cell as Flag;
use std::rc::Rc;

use common::{Screen, rgb, style};
use vtium_font::{Cell, Face, Font, Format, Glyph, Style, Unifont};
use vtium_render::{Canvas, Config, DrawState, Rect, Renderer};
use vtium_term::Age;

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
