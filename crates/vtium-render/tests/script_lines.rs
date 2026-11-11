//! The lines of a superscript or subscript cell (ADR-1202 D8): each
//! underline style, the strike-through and the overline lie on the rows of
//! the script's own set of lines, which follows the small glyph, while the
//! background, reverse video, the selection and the cursors cover the
//! whole cell as in a normal cell. The CPU and GPU paths draw the same
//! pixels.

mod common;

use common::{Screen, rgb, style};
use vtium_font::{Font, Unifont};
use vtium_render::{Canvas, Config, Depth, DrawState, Renderer};
use vtium_term::{Age, Attrs, CursorShape, Script as Position, Style, UnderlineStyle};

const FG: &str = "e0e0e0";
const BG: &str = "203040";

/// A line a cell can draw: an underline style, or the lines over the
/// glyph.
#[derive(Clone, Copy, Debug)]
enum Line {
    Under(UnderlineStyle),
    Strike,
    Overline,
    StrikeOverline,
}

const LINES: [Line; 8] = [
    Line::Under(UnderlineStyle::Single),
    Line::Under(UnderlineStyle::Double),
    Line::Under(UnderlineStyle::Curly),
    Line::Under(UnderlineStyle::Dotted),
    Line::Under(UnderlineStyle::Dashed),
    Line::Strike,
    Line::Overline,
    Line::StrikeOverline,
];

fn styled(script: Position, line: Line) -> Style {
    let mut st = style(rgb(FG), rgb(BG));
    st.attrs.set_script(script);
    match line {
        Line::Under(u) => st.attrs.set_underline(u),
        Line::Strike => st.attrs.insert(Attrs::STRIKE),
        Line::Overline => st.attrs.insert(Attrs::OVERLINE),
        Line::StrikeOverline => {
            st.attrs.insert(Attrs::STRIKE);
            st.attrs.insert(Attrs::OVERLINE);
        }
    }
    st
}

fn unifont_at(height: u32) -> Font<Unifont> {
    Font::new(Unifont::new(height))
}

/// `s` drawn whole in unifont `height` pixels high, with `config`.
fn draw(s: &Screen, height: u32, config: Config, cursor_on: bool) -> Canvas {
    let font = unifont_at(height);
    let cell = font.cell();
    let (w, h) = (s.cols as u32 * cell.width, s.rows as u32 * cell.height);
    let mut r = Renderer::new(font, w, h, config);
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on,
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

/// The rows of cell `col` of row 0, `cw` by `ch`, that hold any pixel but
/// the background.
fn lit_rows(c: &Canvas, col: u32, (cw, ch): (u32, u32)) -> Vec<u32> {
    let bg = rgb(BG);
    let bg = u32::from(bg.r) << 16 | u32::from(bg.g) << 8 | u32::from(bg.b);
    (0..ch)
        .filter(|&y| (0..cw).any(|x| c.pixel(col * cw + x, y) != Some(bg)))
        .collect()
}

/// One blank cell of each line in `script` across row 0, so only the
/// lines draw.
fn line_screen(script: Position) -> Screen {
    let mut s = Screen::new(LINES.len(), 1, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    for (col, line) in LINES.into_iter().enumerate() {
        s.text(col, 0, " ", styled(script, line));
    }
    s
}

fn range(from: u32, to: u32) -> Vec<u32> {
    (from..=to).collect()
}

// D8 steps 4 and 5 and its table, in unifont at 16 rows: each line's lit
// rows are the rows of its script's set, the normal cell's as before; a
// superscript's double and curl lie on rows 10 to 12 under the raised
// glyph and a subscript's on rows 13 to 15, as a normal cell's
// covers: sgr.script
#[test]
fn each_line_lies_on_its_scripts_rows() {
    let want = |script: Position| -> [Vec<u32>; 8] {
        match script {
            Position::Baseline => [
                vec![15],
                vec![13, 15],
                range(13, 15),
                vec![15],
                vec![15],
                vec![8],
                vec![0],
                vec![0, 8],
            ],
            Position::Super => [
                vec![10],
                vec![10, 12],
                range(10, 12),
                vec![10],
                vec![10],
                vec![5],
                vec![0],
                vec![0, 5],
            ],
            Position::Sub => [
                vec![15],
                vec![13, 15],
                range(13, 15),
                vec![15],
                vec![15],
                vec![10],
                vec![5],
                vec![5, 10],
            ],
        }
    };
    for script in [Position::Baseline, Position::Super, Position::Sub] {
        let c = draw(&line_screen(script), 16, Config::default(), false);
        for (col, rows) in want(script).iter().enumerate() {
            assert_eq!(
                &lit_rows(&c, col as u32, (8, 16)),
                rows,
                "{script:?} {:?}",
                LINES[col]
            );
        }
    }
}

// D8's table at 2x and 3x: unifont's script lines come from its smaller
// multiple, one row thick at 2x and two at 3x
// covers: sgr.script
#[test]
fn at_2x_and_3x_the_lines_follow_the_smaller_multiple() {
    for (height, sup, sub) in [
        (32, [15, 8, 0], [31, 24, 16]),
        (48, [30, 16, 0], [46, 32, 16]),
    ] {
        let thick = if height == 32 { 1 } else { 2 };
        let cell = (height / 2, height);
        for (script, rows) in [(Position::Super, sup), (Position::Sub, sub)] {
            let c = draw(&line_screen(script), height, Config::default(), false);
            let band = |top: u32| range(top, top + thick - 1);
            assert_eq!(lit_rows(&c, 0, cell), band(rows[0]), "{script:?} under");
            assert_eq!(lit_rows(&c, 5, cell), band(rows[1]), "{script:?} strike");
            assert_eq!(lit_rows(&c, 6, cell), band(rows[2]), "{script:?} over");
        }
    }
}

// D8 step 6: two underlined cells side by side, one normal and one a
// superscript, each draw the underline on their own rows, so it steps
// covers: sgr.script
#[test]
fn an_underline_steps_between_a_normal_and_a_superscript_cell() {
    let mut s = Screen::new(3, 1, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    let under = Line::Under(UnderlineStyle::Single);
    s.text(0, 0, " ", styled(Position::Baseline, under));
    s.text(1, 0, " ", styled(Position::Super, under));
    s.text(2, 0, " ", styled(Position::Super, under));
    let c = draw(&s, 16, Config::default(), false);
    assert_eq!(lit_rows(&c, 0, (8, 16)), [15]);
    assert_eq!(lit_rows(&c, 1, (8, 16)), [10]);
    // a run of superscripts shares one unbroken line
    for x in 8..24 {
        assert_ne!(c.pixel(x, 10), c.pixel(0, 0), "column {x}");
    }
}

// D8 step 5: on a 1-bit display a script cell's curl is the wave of whole
// pixels deco.rs draws in any cell, joined from column to column, in the
// script's rows
// covers: sgr.script
#[test]
fn a_1_bit_curl_in_a_script_cell_is_joined_whole_pixels() {
    let config = Config {
        depth: Depth::One,
        ..Config::default()
    };
    for (script, rows) in [(Position::Super, 10..=12), (Position::Sub, 13..=15)] {
        let c = draw(&line_screen(script), 16, config, false);
        let bg = c.pixel(0, 0);
        // the curl is the third cell
        let ink: Vec<Vec<u32>> = (16..24)
            .map(|x| (0..16).filter(|&y| c.pixel(x, y) != bg).collect())
            .collect();
        for (x, col) in ink.iter().enumerate() {
            assert!(!col.is_empty(), "{script:?} column {x} has no ink");
            assert!(col.iter().all(|y| rows.contains(y)), "{script:?} {col:?}");
            assert_eq!(col.len() as u32, col[col.len() - 1] - col[0] + 1);
        }
        for pair in ink.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let (a0, a1, b0, b1) = (a[0], a[a.len() - 1], b[0], b[b.len() - 1]);
            assert!(b0 <= a1 + 1 && a0 <= b1 + 1, "{script:?}: {a:?} then {b:?}");
        }
    }
}

/// A screen of `text` in `st` across row 0, with the cursor at column 1
/// in `shape`.
fn whole_cell_screen(st: Style, shape: Option<CursorShape>) -> Screen {
    let mut s = Screen::new(3, 1, rgb(FG), rgb(BG));
    s.text(0, 0, "   ", st);
    s.cursor.visible = shape.is_some();
    s.cursor.col = 1;
    if let Some(shape) = shape {
        s.cursor.shape = shape;
    }
    s
}

// D8 step 1: the background, reverse video, the selection and the block,
// bar and underline cursors cover a script cell as they cover a normal
// one; the underline cursor keeps a normal cell's rows
// covers: sgr.script
#[test]
fn what_covers_the_whole_cell_draws_in_a_script_cell_as_in_a_normal_one() {
    let shapes = [
        None,
        Some(CursorShape::Block),
        Some(CursorShape::Bar),
        Some(CursorShape::Underline),
    ];
    for inverse in [false, true] {
        for selected in [false, true] {
            for shape in shapes {
                let mut normal = style(rgb(FG), rgb(BG));
                if inverse {
                    normal.attrs.insert(Attrs::INVERSE);
                }
                let draw_in = |script: Position| {
                    let mut st = normal;
                    st.attrs.set_script(script);
                    let s = whole_cell_screen(st, shape);
                    let font = unifont_at(16);
                    let mut r = Renderer::new(font, 24, 16, Config::default());
                    let mut c = Canvas::new(24, 16, 0);
                    let frame = s.frame(Age::NEVER);
                    let frame = if selected {
                        frame.with_selection(std::iter::once(1..2).collect())
                    } else {
                        frame
                    };
                    let state = DrawState {
                        cursor_on: true,
                        background: s.palette.bg(),
                        ..DrawState::default()
                    };
                    r.draw(&frame, &mut c, 0, &state);
                    c
                };
                let base = draw_in(Position::Baseline);
                for script in [Position::Super, Position::Sub] {
                    assert!(
                        draw_in(script) == base,
                        "{script:?} inverse {inverse} selected {selected} {shape:?}"
                    );
                }
            }
        }
    }
}

// D8 step 1: the underline cursor on an underlined script cell hides the
// cell's own underline, as on any cell, and draws nothing of its own
// covers: sgr.script
#[test]
fn the_underline_cursor_hides_a_script_cells_own_underline() {
    let st = styled(Position::Super, Line::Under(UnderlineStyle::Single));
    let s = whole_cell_screen(st, Some(CursorShape::Underline));
    let c = draw(&s, 16, Config::default(), true);
    assert_eq!(lit_rows(&c, 0, (8, 16)), [10]);
    assert_eq!(lit_rows(&c, 1, (8, 16)), Vec::<u32>::new());
}

#[cfg(feature = "gpu")]
mod gpu {
    use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};

    use super::*;

    /// `s` drawn whole on the GPU path's stand-in, in unifont `height`
    /// pixels high: the upright frame's pixels.
    fn draw_gpu(s: &Screen, height: u32) -> Vec<u32> {
        let font = unifont_at(height);
        let cell = font.cell();
        let size = Size::new(s.cols as u32 * cell.width, s.rows as u32 * cell.height);
        let mut gl = StandIn::new();
        let mut pipeline =
            Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
        let buffer = gl.import_buffer(&size).unwrap();
        let mut r = Renderer::new(font, size.width, size.height, Config::default());
        let st = DrawState {
            background: s.palette.bg(),
            ..DrawState::default()
        };
        r.draw_gpu(
            &mut gl,
            &mut pipeline,
            &s.frame(Age::NEVER),
            &buffer,
            0,
            &st,
            &[],
        )
        .unwrap();
        gl.frame_names()
            .iter()
            .rev()
            .find_map(|&n| gl.frame_pixels(n).filter(|(f, _)| *f == size))
            .unwrap()
            .1
            .to_vec()
    }

    // D8: the GPU path picks each line's sprite by the cell's script as
    // the CPU path does, and draws the same pixels, in every script and
    // every line, at 1x and 2x
    // covers: sgr.script
    #[test]
    fn the_gpu_path_draws_a_script_cells_lines_as_the_cpu_path_does() {
        for height in [16, 32] {
            let mut s = Screen::new(LINES.len(), 3, rgb(FG), rgb(BG));
            s.cursor.visible = false;
            for (row, script) in [Position::Baseline, Position::Super, Position::Sub]
                .into_iter()
                .enumerate()
            {
                for (col, line) in LINES.into_iter().enumerate() {
                    s.text(col, row, "x", styled(script, line));
                }
            }
            let cpu = draw(&s, height, Config::default(), false);
            let gpu = draw_gpu(&s, height);
            let w = cpu.width();
            let differ = (0..cpu.height())
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| cpu.pixel(x, y) != Some(gpu[(y * w + x) as usize]))
                .count();
            assert_eq!(differ, 0, "at {height}");
            // not vacuous: each script's row of cells draws differently
            let row = |r: u32| -> Vec<Option<u32>> {
                let ch = height;
                (0..ch)
                    .flat_map(|y| (0..w).map(move |x| (x, r * ch + y)))
                    .map(|(x, y)| cpu.pixel(x, y))
                    .collect()
            };
            assert_ne!(row(0), row(1));
            assert_ne!(row(1), row(2));
        }
    }
}
