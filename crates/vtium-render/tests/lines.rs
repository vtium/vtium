//! A font whose underline and strike-through lines change after the
//! renderer is made, as an outline font's do once the glyph helper has
//! measured them (vtium/vtium#241): the renderer takes the new lines when
//! it is told the glyphs changed, and draws every cell with them.

mod common;

use std::cell::Cell as Shared;
use std::rc::Rc;

use common::{Screen, rgb, style};
use vtium_font::{Cell, Decorations, Face, Font, Glyph, SpritePolicy, Style, Unifont};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Attrs, UnderlineStyle};

/// Unifont at 16, with lines a test can move.
struct Moving {
    unifont: Unifont,
    lines: Rc<Shared<Decorations>>,
}

impl Face for Moving {
    fn cell(&self) -> Cell {
        self.unifont.cell()
    }
    fn size_step(&self) -> u32 {
        self.unifont.size_step()
    }
    fn has_glyph(&self, c: char) -> bool {
        self.unifont.has_glyph(c)
    }
    fn sprite_policy(&self) -> SpritePolicy {
        self.unifont.sprite_policy()
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.unifont.draw(cluster, style)
    }
    fn decorations(&self) -> Decorations {
        self.lines.get()
    }
}

fn font(lines: &Rc<Shared<Decorations>>) -> Font<Moving> {
    Font::new(Moving {
        unifont: Unifont::new(16),
        lines: Rc::clone(lines),
    })
}

/// The lines the glyph helper measures: thicker, the underline higher.
const MEASURED: Decorations = Decorations {
    underline: 12,
    underline_thickness: 3,
    strike: 7,
    strike_thickness: 2,
};

/// A screen with an underlined and struck word and a line of box drawing,
/// whose sprites the font draws in the underline's thickness.
fn screen() -> Screen {
    let (fg, bg) = (rgb("c0c0c0"), rgb("101010"));
    let mut s = Screen::new(10, 3, fg, bg);
    let mut st = style(fg, bg);
    st.attrs.set_underline(UnderlineStyle::Single);
    st.attrs.insert(Attrs::STRIKE);
    s.text(0, 0, "ab", st);
    s.text(0, 1, "\u{2500}\u{2500}\u{2502}", style(fg, bg));
    s
}

fn state() -> DrawState {
    DrawState {
        background: rgb("101010"),
        ..DrawState::default()
    }
}

#[test]
fn new_lines_from_the_font_redraw_every_cell_with_them() {
    let lines = Rc::new(Shared::new(Decorations::for_height(16)));
    let mut r = Renderer::new(font(&lines), 84, 51, Config::default());
    let mut s = screen();
    let mut canvas = Canvas::new(84, 51, 0);
    r.draw(&s.frame(Age::NEVER), &mut canvas, 0, &state());
    let since = s.damage.checkpoint();
    lines.set(MEASURED);
    r.glyphs_changed(&[]);
    let damage = r
        .draw(&s.frame(since), &mut canvas, 1, &state())
        .damage
        .to_vec();
    assert_eq!(damage.len(), 1);
    assert_eq!((damage[0].width, damage[0].height), (84, 51));
    // the same picture as a renderer made with the measured lines
    let mut fresh = Canvas::new(84, 51, 0);
    Renderer::new(font(&lines), 84, 51, Config::default()).draw(
        &s.frame(Age::NEVER),
        &mut fresh,
        0,
        &state(),
    );
    assert!(canvas == fresh, "the old lines are left on the screen");
}

#[test]
fn unchanged_lines_redraw_nothing() {
    let lines = Rc::new(Shared::new(Decorations::for_height(16)));
    let mut r = Renderer::new(font(&lines), 84, 51, Config::default());
    let mut s = screen();
    let mut canvas = Canvas::new(84, 51, 0);
    r.draw(&s.frame(Age::NEVER), &mut canvas, 0, &state());
    let since = s.damage.checkpoint();
    r.glyphs_changed(&[]);
    let drawn = r.draw(&s.frame(since), &mut canvas, 1, &state());
    assert!(drawn.damage.is_empty());
}

#[cfg(feature = "gpu")]
#[test]
fn new_lines_on_the_gpu_path_make_the_glyphs_and_sprites_anew_and_draw_whole() {
    use vtium_render::gpu::{Call, Config as GpuConfig, Format, Gl, Pipeline, Size, StandIn};
    let display = Size::new(84, 51);
    let mut gl = StandIn::new();
    let mut pipeline = Pipeline::new(&mut gl, display, GpuConfig::default()).unwrap();
    let buffer = gl.import_buffer(&display).unwrap();
    let lines = Rc::new(Shared::new(Decorations::for_height(16)));
    let mut r = Renderer::new(font(&lines), 84, 51, Config::default());
    let mut s = screen();
    r.draw_gpu(
        &mut gl,
        &mut pipeline,
        &s.frame(Age::NEVER),
        &buffer,
        0,
        &state(),
        &[],
    )
    .unwrap();
    let since = s.damage.checkpoint();
    lines.set(MEASURED);
    r.glyphs_changed(&[]);
    gl.clear_calls();
    let damage = r
        .draw_gpu(
            &mut gl,
            &mut pipeline,
            &s.frame(since),
            &buffer,
            1,
            &state(),
            &[],
        )
        .unwrap()
        .damage
        .to_vec();
    assert_eq!(damage, [display.rect()]);
    // the old sprites and pool freed, the sprites' strip made and filled
    // again
    assert!(gl.calls().contains(&Call::FreeTexture));
    assert!(
        gl.calls()
            .contains(&Call::CreateTexture(Format::R8, Size::new(48, 16)))
    );
}
