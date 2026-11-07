//! The render timer's figures on the minspec display, 1366x768 in
//! unifont's 8x16 cells: a cold first frame, a full redraw from a warm
//! cache, a frame with one row of decorated text changed, a keystroke and
//! an idle frame. They are machine figures and are printed, not pinned.
//!
//! Each figure is measured twice (ADR-1306 D4): with unifont, whose ink
//! stays in its cells, and with unifont whose vowels carry an accent two
//! pixels above their row, so that about a third of the letters send ink
//! into the row above.
//!
//! `cargo test --release -p vtium-render --test figures -- --nocapture`

mod common;

use std::time::Duration;

use common::{Screen, rgb, style, unifont};
use vtium_font::{Cell, Face, Font, Glyph, Style, Unifont};
use vtium_render::{Config, DrawState, Mapped, RenderTimer, Renderer};
use vtium_term::{Age, Attrs, UnderlineStyle};

const W: u32 = 1366;
const H: u32 = 768;

fn text(s: &mut Screen, seed: usize) {
    let words = "the quick brown fox jumps over a lazy dog; fn main() { println!(\"hi\"); } ";
    let st = style(rgb("c0c0c0"), rgb("101010"));
    for row in 0..s.rows {
        let line: String = words
            .chars()
            .cycle()
            .skip(row * 7 + seed)
            .take(s.cols)
            .collect();
        s.text(0, row, &line, st);
    }
}

/// Unifont whose vowels carry an accent two pixels above their row, in the
/// glyph's ink box, as an outline font's accented letters do.
struct Accented(Unifont);

impl Face for Accented {
    fn cell(&self) -> Cell {
        self.0.cell()
    }
    fn size_step(&self) -> u32 {
        self.0.size_step()
    }
    fn has_glyph(&self, c: char) -> bool {
        self.0.has_glyph(c)
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        let g = self.0.draw(cluster, style)?;
        if !cluster.first().is_some_and(|c| "aeiou".contains(*c)) {
            return Some(g);
        }
        let w = g.width() as usize;
        let mut boxed = vec![0u8; 2 * w];
        boxed[3..5].fill(255);
        boxed[w + 3..w + 5].fill(255);
        boxed.extend_from_slice(g.pixels());
        g.clone()
            .with_ink((0, -2), (g.width(), g.height() + 2), boxed)
            .or(Some(g))
    }
}

#[test]
fn figures_at_1366x768() {
    println!("unifont, whose ink stays in its cells:");
    figures(unifont());
    println!("unifont with accents above its vowels, whose ink reaches the row above:");
    figures(Font::new(Accented(Unifont::new(16))));
}

fn figures<F: Face>(font: Font<F>) {
    let mut r = Renderer::new(font, W, H, Config::default());
    let (cols, rows) = r.grid_size();
    let mut s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("101010"));
    s.cursor.visible = true;
    text(&mut s, 0);
    let mut buffers = vec![vec![0u8; W as usize * 4 * H as usize]; 2];
    let ds = DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    };
    let mut since = Age::NEVER;
    // the frame each buffer holds, 0 for none, as a display counts them
    let (mut held, mut frames, mut back) = ([0u64; 2], 0u64, 0usize);
    let screen_px = u64::from(W) * u64::from(H);
    let mut draw = |r: &mut Renderer<_>, s: &mut Screen, t: &mut RenderTimer| {
        let age = match held[back] {
            0 => 0,
            h => (frames - h + 1) as u32,
        };
        let mut m = Mapped::new(&mut buffers[back], W, H, W as usize * 4).unwrap();
        let d = r.draw(&s.frame(since), &mut m, age, &ds);
        let st = d.stats;
        let queued = !d.damage.is_empty();
        t.frame(st.cpu, queued, st.damage_px, st.misses);
        if queued {
            t.flip();
            frames += 1;
            held[back] = frames;
            back = 1 - back;
        }
        since = s.damage.checkpoint();
    };
    let report = |name: &str, t: &mut RenderTimer, frames: u32| {
        let rep = t.report(Duration::from_secs(1), screen_px).unwrap();
        println!("{name} ({frames} frames): {}", rep.osd());
        println!("  {rep}");
    };

    let mut t = RenderTimer::new();
    draw(&mut r, &mut s, &mut t);
    report("cold first frame", &mut t, 1);

    let mut t = RenderTimer::new();
    for i in 0..20 {
        text(&mut s, i + 1);
        draw(&mut r, &mut s, &mut t);
    }
    report("full redraw, warm cache", &mut t, 20);

    let mut t = RenderTimer::new();
    let mut ul = style(rgb("ffff00"), rgb("101010"));
    ul.attrs.set_underline(UnderlineStyle::Curly);
    ul.attrs.insert(Attrs::BOLD);
    for i in 0..20 {
        let line: String = "warning: unused variable "
            .chars()
            .cycle()
            .skip(i)
            .take(cols)
            .collect();
        s.text(0, 10, &line, ul);
        draw(&mut r, &mut s, &mut t);
    }
    report("one decorated row", &mut t, 20);

    let mut t = RenderTimer::new();
    for i in 0..20 {
        s.put(
            i % cols,
            rows - 1,
            'x',
            false,
            style(rgb("c0c0c0"), rgb("101010")),
        );
        s.cursor.row = rows - 1;
        s.cursor.col = (i + 1) % cols;
        draw(&mut r, &mut s, &mut t);
    }
    report("a keystroke", &mut t, 20);

    let mut t = RenderTimer::new();
    for _ in 0..20 {
        draw(&mut r, &mut s, &mut t);
    }
    // two buffers: the first idle frame still owes the last keystroke
    let rep = t.report(Duration::from_secs(1), screen_px).unwrap();
    println!("idle (20 frames): {}\n  {rep}", rep.osd());
    assert!(rep.skipped >= 19);
}
