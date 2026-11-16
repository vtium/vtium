//! vtium-render's drawing benchmark: the CPU time the renderer takes to
//! draw a whole screen of text into memory, with no display.
//!
//! Each benchmark fills a terminal of 240 by 67 cells, a 1920×1072 screen
//! of 8×16 cells, with one kind of text, takes its frame once, and then
//! times a full redraw of that frame into a canvas in memory, as the
//! renderer draws after a console switch or a resize. The renderer keeps
//! its glyph cache from one redraw to the next, as it does on a console.
//! It repeats the redraw and prints the median, the fastest and the
//! slowest repetition in milliseconds.
//!
//! - `text`: plain ASCII words and punctuation, as a shell or an editor
//!   shows them.
//! - `equals`: every row a run of `=` across the screen. A ligature run
//!   holds 2 to 14 symbols, so the renderer walks this run to its ends
//!   and draws each `=` on its own.
//! - `arrows`: every row `->` repeated with no space, again one long run
//!   of symbols that draws apart.
//! - `spaced`: every row `-> ` repeated, each arrow a ligature run of two
//!   cells.
//!
//! Each benchmark runs with `font-ligatures` off and on. The font is
//! Unifont at 16 pixels, wrapped so that it holds a glyph for each piece of
//! a ligature run, as a programming font with ligatures does. Only a build
//! with the bidi feature finds ligature runs, so without it both lines of
//! a benchmark draw the same cells.
//!
//! Run it with `cargo run --release -p vtium-render --features bidi
//! --example draw_bench -- [NAME ...] [--reps N]`. With no name it runs
//! every benchmark, 30 repetitions each.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::print_stdout,
    reason = "a benchmark program prints its figures and stops on a bad argument"
)]
#![allow(
    clippy::disallowed_methods,
    reason = "a benchmark measures wall time; the renderer it drives reads its own clock"
)]

use std::cell::Cell as Count;
use std::rc::Rc;
use std::time::Instant;

use vtium_font::syllable::piece_of_chars;
use vtium_font::{Cell, Face, Font, Format, Glyph, Style, Unifont, Variant, Width};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Terminal};

const COLS: usize = 240;
const ROWS: usize = 67;
const CELL_W: u32 = 8;
const CELL_H: u32 = 16;

/// Unifont, which also holds a glyph for each piece of a ligature run: the
/// piece's cells with their top half inked. It counts the pieces the
/// renderer asks it to draw, so the benchmark can check that ligatures
/// joined where it expects them to.
struct Ligatures(Unifont, Rc<Count<usize>>);

impl Face for Ligatures {
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
        let Some((_, piece)) = piece_of_chars(cluster) else {
            return self.0.draw(cluster, style);
        };
        self.1.set(self.1.get() + 1);
        let w = CELL_W * u32::from(piece.cells);
        let pixels = (0..CELL_H).flat_map(|y| (0..w).map(move |_| if y < 8 { 255 } else { 0 }));
        let cells = if piece.cells == 2 {
            Width::Wide
        } else {
            Width::Narrow
        };
        Some(Glyph::from_pixels(w, CELL_H, Format::Mask, pixels.collect())?.with_cells(cells))
    }

    fn holds(&self, cluster: &[char], _variant: Variant) -> bool {
        piece_of_chars(cluster).is_some()
    }
}

/// One row of each benchmark's text, `COLS` cells wide.
fn row(name: &str, y: usize) -> String {
    match name {
        "text" => {
            const WORDS: [&str; 8] = [
                "the", "terminal", "draws", "each", "cell,", "of", "plain", "text;",
            ];
            let mut s = String::new();
            let mut i = y;
            while s.len() < COLS {
                s.push_str(WORDS[i % WORDS.len()]);
                s.push(' ');
                i += 3;
            }
            s.truncate(COLS);
            s
        }
        "equals" => "=".repeat(COLS),
        "arrows" => "->".repeat(COLS / 2),
        "spaced" => "-> ".repeat(COLS / 3),
        _ => unreachable!("{name}"),
    }
}

/// A terminal whose every row holds `name`'s text.
fn screen(name: &str) -> Terminal {
    let mut t = Terminal::new(COLS, ROWS, 0);
    for y in 0..ROWS {
        t.input(format!("\x1b[{};1H{}", y + 1, row(name, y)).as_bytes());
    }
    t.input(b"\x1b[H");
    t
}

/// The milliseconds each of `reps` full redraws of `t` takes, sorted, and
/// how many ligature pieces the renderer asked the font for.
fn time(t: &Terminal, ligatures: bool, reps: usize) -> (Vec<f64>, usize) {
    let config = Config {
        ligatures,
        ..Config::default()
    };
    let (w, h) = (COLS as u32 * CELL_W, ROWS as u32 * CELL_H);
    let pieces = Rc::new(Count::new(0));
    let font = Font::new(Ligatures(Unifont::new(CELL_H), Rc::clone(&pieces)));
    let mut r = Renderer::new(font, w, h, config);
    let mut canvas = Canvas::new(w, h, 0);
    let frame = t.screen().frame(t.palette(), Age::NEVER);
    let state = DrawState {
        cursor_on: true,
        background: t.palette().bg(),
        ..DrawState::default()
    };
    // one redraw fills the glyph cache before the timed ones
    r.draw(&frame, &mut canvas, 0, &state);
    let mut times: Vec<f64> = (0..reps)
        .map(|_| {
            let start = Instant::now();
            r.draw(&frame, &mut canvas, 0, &state);
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    times.sort_by(f64::total_cmp);
    (times, pieces.get())
}

fn main() {
    let mut names = Vec::new();
    let mut reps = 30;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--reps" {
            reps = args.next().and_then(|n| n.parse().ok()).expect("--reps N");
        } else {
            names.push(a);
        }
    }
    assert!(reps > 0, "--reps must be at least 1");
    println!(
        "{COLS}×{ROWS} cells of {CELL_W}×{CELL_H}, bidi feature {}",
        if cfg!(feature = "bidi") { "on" } else { "off" }
    );
    println!("benchmark  ligatures  reps  ms/frame (median, min-max)");
    for name in ["text", "equals", "arrows", "spaced"] {
        if !names.is_empty() && !names.iter().any(|n| n == name) {
            continue;
        }
        let t = screen(name);
        for ligatures in [false, true] {
            let (times, pieces) = time(&t, ligatures, reps);
            let joins = cfg!(feature = "bidi") && ligatures && name == "spaced";
            assert_eq!(pieces > 0, joins, "{name}: ligature pieces drawn");
            let (lo, mid, hi) = (times[0], times[times.len() / 2], times[times.len() - 1]);
            println!(
                "{name:<10} {:<9}  {reps:4}  {mid:7.3} ({lo:.3}-{hi:.3})",
                if ligatures { "on" } else { "off" },
            );
        }
    }
}
