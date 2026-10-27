//! What each frame redraws and reports as damage (vtium/vtium#16's gates):
//! a changed cell is drawn into every buffer once and then left alone, a
//! full redraw reaches each buffer once, and a keystroke damages only the
//! cells it changes.

mod common;

use common::{Screen, WriteOnly, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Rect, Renderer, Target};
use vtium_term::{Age, Attrs, CursorShape, Rgb};

const FG: &str = "c0c0c0";
const BG: &str = "101010";

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// Records the rectangles a frame wrote, to check them against its damage.
struct Recorder {
    canvas: WriteOnly,
    writes: Vec<Rect>,
}

impl Target for Recorder {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        self.writes.push(Rect {
            x,
            y,
            width: pixels.len() as u32,
            height: 1,
        });
        self.canvas.write(x, y, pixels);
    }
}

fn inside(w: &Rect, damage: &[Rect]) -> bool {
    damage.iter().any(|d| {
        w.x >= d.x
            && w.y >= d.y
            && w.x + w.width <= d.x + d.width
            && w.y + w.height <= d.y + d.height
    })
}

/// A display drawing a screen frame by frame, keeping its age.
struct Display {
    r: Renderer<vtium_font::Unifont>,
    since: Age,
    buffers: Vec<Recorder>,
    frames: usize,
}

impl Display {
    fn new(buffers: u8, width: u32, height: u32) -> Display {
        let config = Config {
            buffers,
            ..Config::default()
        };
        Display {
            r: Renderer::new(unifont(), width, height, config),
            since: Age::NEVER,
            buffers: (0..buffers)
                .map(|_| Recorder {
                    canvas: WriteOnly::new(width, height),
                    writes: Vec::new(),
                })
                .collect(),
            frames: 0,
        }
    }

    /// Draws the next frame into the next buffer; returns its damage.
    fn draw(&mut self, s: &mut Screen, st: &DrawState) -> Vec<Rect> {
        let n = self.buffers.len();
        let buf = &mut self.buffers[self.frames % n];
        buf.writes.clear();
        let frame = s.frame(self.since);
        let damage = self.r.draw(&frame, buf, st).damage.to_vec();
        drop(frame);
        self.since = s.damage.checkpoint();
        for w in &buf.writes {
            assert!(
                inside(w, &damage),
                "frame {} wrote {w:?} outside its damage {damage:?}",
                self.frames
            );
        }
        self.frames += 1;
        damage
    }

    fn last(&self) -> &Recorder {
        let n = self.buffers.len();
        &self.buffers[(self.frames + n - 1) % n]
    }
}

/// `s` drawn whole by a fresh renderer.
fn fresh(s: &Screen, st: &DrawState, width: u32, height: u32) -> Canvas {
    let mut r = Renderer::new(unifont(), width, height, Config::default());
    let mut c = Canvas::new(width, height, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, st);
    c
}

fn area(damage: &[Rect]) -> u64 {
    damage.iter().map(Rect::area).sum()
}

fn cell(col: u32, row: u32, w: u32, h: u32, cols: u32, rows: u32) -> Rect {
    // the grid is centred on the display
    let (ox, oy) = ((w - cols * 8) / 2, (h - rows * 16) / 2);
    Rect {
        x: ox + col * 8,
        y: oy + row * 16,
        width: 8,
        height: 16,
    }
}

fn prompt() -> Screen {
    let mut s = Screen::new(20, 4, rgb(FG), rgb(BG));
    s.text(0, 0, "$ ls", style(rgb("00ff00"), rgb(BG)));
    s.text(0, 1, "a b c", style(rgb(FG), rgb(BG)));
    s.text(0, 2, "$ ", style(rgb("00ff00"), rgb(BG)));
    s.cursor.row = 2;
    s.cursor.col = 2;
    s.cursor.visible = true;
    s
}

// aaronsb/kmscon#156: typing one character at a prompt damages the cell it
// wrote and the cell the cursor moved to, on this frame and the next
// buffer's, and nothing after
#[test]
fn a_keystroke_damages_only_the_cells_it_changes() {
    let mut s = prompt();
    let (w, h) = (20 * 8 + 6, 4 * 16 + 4);
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    d.draw(&mut s, &st);
    d.draw(&mut s, &st);
    assert!(
        d.draw(&mut s, &st).is_empty(),
        "an idle frame draws nothing"
    );

    s.put(2, 2, 'x', false, style(rgb(FG), rgb(BG)));
    s.cursor.col = 3;
    let want = vec![Rect {
        width: 16,
        ..cell(2, 2, w, h, 20, 4)
    }];
    assert_eq!(d.draw(&mut s, &st), want);
    assert_eq!(d.draw(&mut s, &st), want, "the other buffer");
    assert!(d.draw(&mut s, &st).is_empty());
    assert!(d.last().writes.is_empty(), "an idle frame writes nothing");
}

// aaronsb/kmscon#81: after a resize, the first two frames draw everything,
// one into each buffer, and the third only what changed
#[test]
fn a_full_redraw_reaches_each_buffer_once() {
    let mut s = prompt();
    let (w, h) = (20 * 8 + 6, 6 * 16 + 4);
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    for _ in 0..3 {
        d.draw(&mut s, &st);
    }
    // the screen grows a row mid-run
    let mut grown = Screen::new(20, 5, rgb(FG), rgb(BG));
    grown.text(0, 0, "$ ls", style(rgb("00ff00"), rgb(BG)));
    grown.cursor = s.cursor;
    let mut s = grown;
    let whole = vec![Rect {
        x: 0,
        y: 0,
        width: w,
        height: h,
    }];
    assert_eq!(d.draw(&mut s, &st), whole);
    assert_eq!(d.draw(&mut s, &st), whole);
    s.put(5, 4, 'z', false, style(rgb(FG), rgb(BG)));
    let third = d.draw(&mut s, &st);
    assert_eq!(third, vec![cell(5, 4, w, h, 20, 5)], "the third frame");
    assert_eq!(area(&d.draw(&mut s, &st)), 8 * 16);
    assert!(d.draw(&mut s, &st).is_empty());
}

#[test]
fn damage_merges_cells_up_to_three_apart() {
    let mut s = Screen::new(20, 2, rgb(FG), rgb(BG));
    let (w, h) = (160, 32);
    let mut d = Display::new(1, w, h);
    let st = DrawState {
        cursor_on: false,
        ..state(&s)
    };
    d.draw(&mut s, &st);
    for col in [0, 3, 7, 11, 12] {
        s.put(col, 1, '#', false, style(rgb(FG), rgb(BG)));
    }
    let r = |col: u32, cells: u32| Rect {
        x: col * 8,
        y: 16,
        width: cells * 8,
        height: 16,
    };
    // 0 and 3 are three apart and share a rectangle; 7 is four from 3 and
    // 11 four from 7, so each starts another, which 12 joins
    assert_eq!(d.draw(&mut s, &st), vec![r(0, 4), r(7, 1), r(11, 2)]);
}

/// Steps through edits, cursor moves, blink phases and the pointer, drawing
/// each frame into the next of `buffers` buffers poisoned at the start, and
/// checks each against a fresh full draw of the same state.
fn run(buffers: u8, turn: vtium_render::Orientation) {
    let (cols, rows) = (24usize, 6usize);
    let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
    let (w, h) = if turn == vtium_render::Orientation::Normal
        || turn == vtium_render::Orientation::UpsideDown
    {
        (cols as u32 * 8 + 3, rows as u32 * 16 + 5)
    } else {
        (rows as u32 * 16 + 5, cols as u32 * 8 + 3)
    };
    let mut r = Renderer::new(
        unifont(),
        w,
        h,
        Config {
            buffers,
            orientation: turn,
            ..Config::default()
        },
    );
    let mut bufs: Vec<Canvas> = (0..buffers)
        .map(|_| Canvas::new(w, h, 0x00ab_cdef))
        .collect();
    let mut since = Age::NEVER;
    s.cursor.visible = true;
    let mut seed = 0x1234_5678u32;
    let mut rand = move |n: u32| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed % n
    };
    for step in 0..120 {
        for _ in 0..rand(4) {
            let (col, row) = (rand(cols as u32) as usize, rand(rows as u32) as usize);
            let mut st = style(
                Rgb::new(rand(256) as u8, 200, 100),
                Rgb::new(0, rand(256) as u8, 40),
            );
            match rand(6) {
                0 => st.attrs.insert(Attrs::BLINK),
                1 => st.attrs.set_underline(vtium_term::UnderlineStyle::Curly),
                2 => st.attrs.insert(Attrs::STRIKE),
                _ => {}
            }
            let wide = rand(5) == 0 && col + 1 < cols;
            let c = if wide {
                '中'
            } else {
                char::from_u32(0x41 + rand(26)).unwrap()
            };
            s.put(col, row, c, wide, st);
        }
        if rand(3) == 0 {
            s.cursor.col = rand(cols as u32) as usize;
            s.cursor.row = rand(rows as u32) as usize;
            s.cursor.shape = match rand(3) {
                0 => CursorShape::Block,
                1 => CursorShape::Underline,
                _ => CursorShape::Bar,
            };
        }
        let st = DrawState {
            cursor_on: rand(4) != 0,
            blink_hidden: (step / 7) % 2 == 1,
            pointer: (rand(3) == 0).then(|| (rand(cols as u32 * 8), rand(rows as u32 * 16))),
            ..state(&s)
        };
        let buf = &mut bufs[step % usize::from(buffers)];
        r.draw(&s.frame(since), buf, &st);
        since = s.damage.checkpoint();
        let mut whole = Renderer::new(
            unifont(),
            w,
            h,
            Config {
                orientation: turn,
                ..Config::default()
            },
        );
        let mut want = Canvas::new(w, h, 0);
        whole.draw(&s.frame(Age::NEVER), &mut want, &st);
        if *buf != want {
            let i = buf
                .pixels()
                .iter()
                .zip(want.pixels())
                .position(|(a, b)| a != b)
                .unwrap();
            let (x, y) = (i as u32 % w, i as u32 / w);
            panic!(
                "{turn:?}, {buffers} buffers: frame {step} differs from a full draw at {x},{y}: {:06x} for {:06x}; cursor {:?}, state {st:?}",
                buf.pixels()[i],
                want.pixels()[i],
                s.cursor
            );
        }
    }
}

#[test]
fn every_frame_matches_a_full_draw_with_one_buffer() {
    run(1, vtium_render::Orientation::Normal);
}

#[test]
fn every_frame_matches_a_full_draw_with_two_buffers() {
    run(2, vtium_render::Orientation::Normal);
}

#[test]
fn every_frame_matches_a_full_draw_with_three_buffers() {
    run(3, vtium_render::Orientation::Normal);
}

#[test]
fn every_frame_matches_a_full_draw_turned() {
    for turn in [
        vtium_render::Orientation::Right,
        vtium_render::Orientation::UpsideDown,
        vtium_render::Orientation::Left,
    ] {
        run(2, turn);
    }
}

#[test]
fn a_palette_background_change_redraws_the_border() {
    let mut s = prompt();
    let (w, h) = (20 * 8 + 6, 4 * 16 + 4);
    let mut d = Display::new(1, w, h);
    let st = state(&s);
    d.draw(&mut s, &st);
    assert!(d.draw(&mut s, &st).is_empty());
    s.palette.set_bg(rgb("202020"));
    s.damage.mark_all();
    let st = state(&s);
    assert_eq!(area(&d.draw(&mut s, &st)), u64::from(w * h));
    let _ = fresh(&s, &st, w, h);
}
