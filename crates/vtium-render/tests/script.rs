//! SGR 73 and 74, superscript and subscript, drawn as the glyph scaled to
//! two thirds and raised to the top or lowered to the bottom of its cell;
//! SGR 75 draws the normal glyph.

mod common;

use common::{Screen, psf, rgb, style, unifont};
use vtium_font::{Face, Font, Script, Variant, Width as FontWidth};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Script as Position, Style};

const FG: &str = "e1e1e1";
const BG: &str = "213141";
const BG_PX: u32 = 0x21_31_41;

fn draw_with<F: Face>(font: Font<F>, s: &Screen) -> Canvas {
    let cell = font.cell();
    let (w, h) = (s.cols as u32 * cell.width, s.rows as u32 * cell.height);
    let mut r = Renderer::new(font, w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

fn draw(s: &Screen) -> Canvas {
    draw_with(unifont(), s)
}

fn scripted(script: Position) -> Style {
    let mut st = style(rgb(FG), rgb(BG));
    st.attrs.set_script(script);
    st
}

/// The rows of cell `col` of row 0 that hold ink, `cw` pixels wide and `ch`
/// high.
fn ink_rows(c: &Canvas, col: u32, cw: u32, ch: u32) -> Vec<u32> {
    (0..ch)
        .filter(|&y| (col * cw..(col + 1) * cw).any(|x| c.pixel(x, y) != Some(BG_PX)))
        .collect()
}

// A superscript glyph is the normal glyph at two thirds, its top on the
// cell's top; a subscript glyph's bottom on the cell's bottom; SGR 75's
// glyph the normal one.
// covers: sgr.script
#[test]
fn superscript_and_subscript_draw_the_glyph_small_at_the_top_and_bottom() {
    // a glyph that fills its 8 by 16 cell shows where the small one sits
    let mut s = Screen::new(4, 1, rgb(FG), rgb(BG));
    s.put(0, 0, '\u{2588}', false, scripted(Position::Baseline));
    s.put(1, 0, '\u{2588}', false, scripted(Position::Super));
    s.put(2, 0, '\u{2588}', false, scripted(Position::Sub));
    let c = draw(&s);
    assert_eq!(ink_rows(&c, 0, 8, 16), (0..16).collect::<Vec<_>>());
    assert_eq!(ink_rows(&c, 1, 8, 16), (0..11).collect::<Vec<_>>());
    assert_eq!(ink_rows(&c, 2, 8, 16), (5..16).collect::<Vec<_>>());
    // the small block's columns: 5 of 8, from the second
    for col in [1u32, 2] {
        let cols: Vec<u32> = (0..8)
            .filter(|&x| (0..16).any(|y| c.pixel(col * 8 + x, y) != Some(BG_PX)))
            .collect();
        assert_eq!(cols, [1, 2, 3, 4, 5], "cell {col}");
    }
}

// The pixels of a superscript and a subscript letter are the font's own
// scaled variant, drawn into the cell as vtium-font gives it, in a bitmap
// font and in a wide cell.
// covers: sgr.script
#[test]
fn each_variant_draws_vtium_fonts_scaled_glyph() {
    for (script, position) in [
        (Script::Super, Position::Super),
        (Script::Sub, Position::Sub),
    ] {
        // a bitmap font of 8 by 16 cells, a narrow letter
        let font = psf(8, 16);
        let want = font
            .drawn_variant(
                &['g'],
                FontWidth::Narrow,
                Variant {
                    script,
                    ..Variant::default()
                },
            )
            .unwrap()
            .glyph;
        let mut s = Screen::new(1, 1, rgb("ffffff"), rgb("000000"));
        s.put(0, 0, 'g', false, {
            let mut st = style(rgb("ffffff"), rgb("000000"));
            st.attrs.set_script(position);
            st
        });
        let c = draw_with(font, &s);
        for y in 0..16 {
            for x in 0..8 {
                let a = u32::from(want.pixel(x, y).unwrap()[0]);
                let px = c.pixel(x, y).unwrap();
                assert_eq!(px & 0xff, a, "{script:?} at {x},{y}");
            }
        }
        // a wide character across its two cells, in unifont
        let mut s = Screen::new(2, 1, rgb("ffffff"), rgb("000000"));
        let mut st = style(rgb("ffffff"), rgb("000000"));
        st.attrs.set_script(position);
        s.put(0, 0, '\u{4e2d}', true, st);
        let c = draw(&s);
        let want = unifont()
            .drawn_variant(
                &['\u{4e2d}'],
                FontWidth::Wide,
                Variant {
                    script,
                    ..Variant::default()
                },
            )
            .unwrap()
            .glyph;
        for y in 0..16 {
            for x in 0..16 {
                let a = u32::from(want.pixel(x, y).unwrap()[0]);
                assert_eq!(
                    c.pixel(x, y).unwrap() & 0xff,
                    a,
                    "wide {script:?} at {x},{y}"
                );
            }
        }
    }
}
