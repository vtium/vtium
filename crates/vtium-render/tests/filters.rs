//! The CPU path's color slot: with a look's color table set, every pixel
//! the renderer writes, the border, the cells and the pointer, is the
//! pixel it writes with no table, looked up in the table. The notices
//! write through the same slot, and a 1-bit display's cut comes after it.
//!
//! References:
//! - ADR-1303 D4: the color slot sits between the pointer's blend and the 1-bit cut.
//! - ADR-1304 D2: each pixel the CPU path writes is looked up in the table, and a new table draws whole.
#![cfg(feature = "filters")]

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::filter::{Fixed, Given, Lookup, Map, Table, color};
use vtium_render::{Canvas, Config, Depth, DrawState, Rect, Renderer, Target};
use vtium_term::Age;

/// The color filter `name` with `values`, as a table.
fn table(steps: &[(&str, &[Given])]) -> Table {
    let maps: Vec<Map> = steps
        .iter()
        .map(|(name, values)| Map::new(color(name).unwrap(), values).unwrap())
        .collect();
    Table::build(&maps).unwrap()
}

fn amber() -> Table {
    table(&[(
        "tint",
        &[Given::Color([255, 176, 0]), Given::Number(Fixed::of(1, 0))],
    )])
}

fn screen() -> Screen {
    let bg = rgb("102030");
    let mut s = Screen::new(10, 3, rgb("c0c0c0"), bg);
    s.text(0, 0, "Hello", style(rgb("ff4040"), bg));
    s.text(2, 1, "world", style(rgb("40ff80"), rgb("202080")));
    s.text(0, 2, "#@%", style(rgb("ffffff"), bg));
    s
}

const W: u32 = 10 * 8 + 6;
const H: u32 = 3 * 16 + 5;

fn state(s: &Screen) -> DrawState {
    DrawState {
        background: s.palette.bg(),
        // the pointer over the second row, so its blend passes the slot
        pointer: Some((20, 20)),
        ..DrawState::default()
    }
}

/// `s` drawn whole with `table`, on a display with a border on each side.
fn drawn(s: &Screen, config: Config, table: Option<&Table>) -> Canvas {
    let mut r = Renderer::new(unifont(), W, H, config);
    r.set_color_table(table).unwrap();
    assert_eq!(r.colors(), table.is_some());
    let mut c = Canvas::new(W, H, 0x00ab_cdef);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(s));
    c
}

// Every pixel of a frame drawn with a table is the pixel drawn without
// one, looked up: the border, the cells, the glyphs' edges and the
// pointer, since nothing the frame writes passes the slot by.
#[test]
fn every_pixel_written_is_looked_up_in_the_table() {
    let s = screen();
    let t = amber();
    let plain = drawn(&s, Config::default(), None);
    let looked = drawn(&s, Config::default(), Some(&t));
    let mut lookup = Lookup::new(&t).unwrap();
    let mut moved = 0;
    for (i, (&p, &l)) in plain.pixels().iter().zip(looked.pixels()).enumerate() {
        assert_eq!(l, lookup.map(p), "pixel {i}: {p:06x}");
        moved += usize::from(l != p);
    }
    // the tint changes almost every pixel, and the canvas's own fill
    // shows nowhere, so the frame wrote each pixel through the slot
    assert!(moved > plain.pixels().len() / 2, "{moved} pixels moved");
    assert!(!looked.pixels().contains(&0x00ab_cdef));
    // the identity table changes no pixel
    let identity = Table::build(&[]).unwrap();
    assert_eq!(drawn(&s, Config::default(), Some(&identity)), plain);
}

// A new table changes every pixel, so the frame after it draws whole into
// a buffer that otherwise lacks nothing.
#[test]
fn a_new_table_draws_the_next_frame_whole() {
    let mut s = screen();
    let mut r = Renderer::new(unifont(), W, H, Config::default());
    let mut c = Canvas::new(W, H, 0);
    let st = state(&s);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    let since = s.damage.checkpoint();
    assert!(r.draw(&s.frame(since), &mut c, 1, &st).damage.is_empty());
    let t = amber();
    r.set_color_table(Some(&t)).unwrap();
    let damage: Vec<Rect> = r.draw(&s.frame(since), &mut c, 1, &st).damage.to_vec();
    let area: u64 = damage.iter().map(Rect::area).sum();
    assert_eq!(area, u64::from(W * H), "{damage:?}");
    assert_eq!(c, drawn(&s, Config::default(), Some(&t)));
    // and back to no table, the frame draws whole again
    r.set_color_table(None).unwrap();
    assert!(!r.draw(&s.frame(since), &mut c, 1, &st).damage.is_empty());
    assert_eq!(c, drawn(&s, Config::default(), None));
}

/// A target that records each run written.
#[derive(Default)]
struct Runs(Vec<(u32, u32, Vec<u32>)>);

impl Target for Runs {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        self.0.push((x, y, pixels.to_vec()));
    }
}

// What vtium's notices write beside the frame passes the same slot: each
// run is looked up, and with no table it passes unchanged.
#[test]
fn a_notice_writes_through_the_same_slot() {
    let mut r = Renderer::new(unifont(), W, H, Config::default());
    let run = [0x00ff_ffff, 0x0000_0000, 0x0012_3456];
    let mut out = Runs::default();
    r.colored(&mut out).write(3, 4, &run);
    assert_eq!(out.0, [(3, 4, run.to_vec())]);
    let t = amber();
    r.set_color_table(Some(&t)).unwrap();
    let mut lookup = Lookup::new(&t).unwrap();
    let mut out = Runs::default();
    r.colored(&mut out).write(3, 4, &run);
    assert_eq!(out.0, [(3, 4, run.map(|p| lookup.map(p)).to_vec())]);
}

// On a 1-bit display the look's filters run before the cut at half: a
// filter that darkens white below the cut leaves no pixel lit.
#[test]
fn a_1_bit_display_cuts_after_the_filters() {
    let s = screen();
    let one = Config {
        depth: Depth::One,
        ..Config::default()
    };
    let plain = drawn(&s, one, None);
    assert!(plain.pixels().contains(&0x00ff_ffff));
    let darker = table(&[(
        "contrast",
        &[Given::Number(Fixed::of(0, 0)), Given::Number(Fixed(-2000))],
    )]);
    let cut = drawn(&s, one, Some(&darker));
    assert!(cut.pixels().iter().all(|&p| p == 0), "a pixel stayed lit");
    // a tint keeps white light enough to stay lit
    let tinted = drawn(&s, one, Some(&amber()));
    assert!(tinted.pixels().iter().all(|&p| p == 0 || p == 0x00ff_ffff));
    assert!(tinted.pixels().contains(&0x00ff_ffff));
}
