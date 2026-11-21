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
//! - `underlined`: words with descenders, underlined in a color of their
//!   own, each row in the next of the five underline styles. Rows repeat in
//!   blocks of six: a line of normal size, a double-width line, a
//!   double-height pair, and a row of sized runs at scale 2, two rows tall.
//!   Each underline leaves out the pixels near its glyphs' ink, and on the
//!   magnified lines near the ink of the whole magnified character.
//!
//! Each benchmark of text and ligatures runs with `font-ligatures` off and
//! on. `underlined` runs with them off, on a display upright and on one
//! turned a quarter to the right, where every underline runs down a
//! column of the display instead of along a row. The font is
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
use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
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

/// Words with descenders, `cols` cells of them, starting at word `y`.
fn descenders(y: usize, cols: usize) -> String {
    const WORDS: [&str; 8] = [
        "jumpy", "glyphs", "quietly", "typing", "gaps", "a", "query", "pj",
    ];
    let mut s = String::new();
    let mut i = y;
    while s.len() < cols {
        s.push_str(WORDS[i % WORDS.len()]);
        s.push(' ');
        i += 3;
    }
    s.truncate(cols);
    s
}

/// The bytes that write row `y` of `underlined`, and how many rows they
/// fill: 1, or 2 for a double-height pair or a row of sized runs.
fn underlined_row(y: usize) -> (String, usize) {
    // the five underline styles in turn, in a red of their own (SGR 58)
    let sgr = format!("\x1b[4:{};58:2::255:32:64m", y % 5 + 1);
    let at = format!("\x1b[{};1H", y + 1);
    match y % 6 {
        0 => (format!("{at}{sgr}{}\x1b[m", descenders(y, COLS)), 1),
        1 => (
            format!("{at}\x1b#6{sgr}{}\x1b[m", descenders(y, COLS / 2)),
            1,
        ),
        2 => {
            let text = descenders(y, COLS / 2);
            let below = format!("\x1b[{};1H", y + 2);
            (format!("{at}\x1b#3{sgr}{text}{below}\x1b#4{text}\x1b[m"), 2)
        }
        _ => {
            // runs of 10 characters at scale 2, each 20 cells wide
            let mut s = format!("{at}{sgr}");
            for k in 0..COLS / 20 {
                s.push_str(&format!("\x1b]66;s=2;{}\x07", descenders(y + k, 10)));
            }
            s.push_str("\x1b[m");
            (s, 2)
        }
    }
}

/// A terminal whose every row holds `name`'s text.
fn screen(name: &str) -> Terminal {
    let mut t = Terminal::new(COLS, ROWS, 0);
    let mut y = 0;
    while y < ROWS {
        if name == "underlined" {
            // 11 blocks of six rows, then a line of normal size
            let (bytes, rows) = underlined_row(y);
            assert!(y + rows <= ROWS, "row {y} fills {rows} rows");
            t.input(bytes.as_bytes());
            y += rows;
            continue;
        }
        t.input(format!("\x1b[{};1H{}", y + 1, row(name, y)).as_bytes());
        y += 1;
    }
    t.input(b"\x1b[H");
    t
}

/// The milliseconds each of `reps` full redraws of `t` takes, on a
/// display turned by `orientation`, sorted, and how many ligature pieces
/// the renderer asked the font for.
fn time(t: &Terminal, ligatures: bool, orientation: Orientation, reps: usize) -> (Vec<f64>, usize) {
    let config = Config {
        ligatures,
        orientation,
        ..Config::default()
    };
    let (w, h) = (COLS as u32 * CELL_W, ROWS as u32 * CELL_H);
    let turned = matches!(orientation, Orientation::Right | Orientation::Left);
    let (w, h) = if turned { (h, w) } else { (w, h) };
    let pieces = Rc::new(Count::new(0));
    let font = Font::new(Ligatures(Unifont::new(CELL_H), Rc::clone(&pieces)));
    let mut r = Renderer::new(font, w, h, config);
    assert_eq!(r.grid_size(), (COLS, ROWS), "the display holds the grid");
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
    println!("benchmark   variant         reps  ms/frame (median, min-max)");
    for name in ["text", "equals", "arrows", "spaced", "underlined"] {
        if !names.is_empty() && !names.iter().any(|n| n == name) {
            continue;
        }
        let t = screen(name);
        let variants: &[(bool, Orientation, &str)] = if name == "underlined" {
            &[
                (false, Orientation::Normal, "upright"),
                (false, Orientation::Right, "turned right"),
            ]
        } else {
            &[
                (false, Orientation::Normal, "ligatures off"),
                (true, Orientation::Normal, "ligatures on"),
            ]
        };
        for &(ligatures, orientation, label) in variants {
            let (times, pieces) = time(&t, ligatures, orientation, reps);
            let joins = cfg!(feature = "bidi") && ligatures && name == "spaced";
            assert_eq!(pieces > 0, joins, "{name}: ligature pieces drawn");
            let (lo, mid, hi) = (times[0], times[times.len() / 2], times[times.len() - 1]);
            println!("{name:<11} {label:<14}  {reps:4}  {mid:7.3} ({lo:.3}-{hi:.3})");
        }
    }
}
