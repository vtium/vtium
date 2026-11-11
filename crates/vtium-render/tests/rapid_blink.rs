//! SGR 6, rapid blink, drawn at its own phase: `DrawState::rapid_hidden`,
//! which the binary's rapid timer sets apart from SGR 5's `blink_hidden`.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{BlinkStyle, Canvas, Config, DrawState, Rect, Renderer, TextBlink};
use vtium_term::{Age, Attrs, Style};

const FG: &str = "e1e1e1";
const BG: &str = "213141";
const BG_PX: u32 = 0x21_31_41;

const FULL: TextBlink = TextBlink {
    style: BlinkStyle::Full,
    decorations: false,
};

fn draw(s: &Screen, blink_hidden: bool, rapid_hidden: bool) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let config = Config {
        blink: FULL,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), w, h, config);
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        blink_hidden,
        rapid_hidden,
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

/// background.
fn inked(c: &Canvas, col: u32) -> bool {
    (0..16).any(|y| (col * 8..col * 8 + 8).any(|x| c.pixel(x, y) != Some(BG_PX)))
}

fn with(attrs: Attrs) -> Style {
    let mut st = style(rgb(FG), rgb(BG));
    st.attrs.insert(attrs);
    st
}

// Each blink hides its own cells in its own off phase: SGR 5's at the text
// timer's, SGR 6's at the rapid timer's, and a cell with both at the rapid
// timer's.
// covers: sgr.rapid-blink
#[test]
fn rapid_blink_hides_in_its_own_phase() {
    let mut s = Screen::new(3, 1, rgb(FG), rgb(BG));
    s.put(0, 0, 'a', false, with(Attrs::BLINK));
    s.put(1, 0, 'b', false, with(Attrs::RAPID_BLINK));
    let mut both = with(Attrs::BLINK);
    both.attrs.insert(Attrs::RAPID_BLINK);
    s.put(2, 0, 'c', false, both);
    let ink = |c: &Canvas| [inked(c, 0), inked(c, 1), inked(c, 2)];
    assert_eq!(ink(&draw(&s, false, false)), [true, true, true]);
    assert_eq!(ink(&draw(&s, true, false)), [false, true, true]);
    assert_eq!(ink(&draw(&s, false, true)), [true, false, false]);
    assert_eq!(ink(&draw(&s, true, true)), [false, false, false]);
}

// A change of the rapid blink's phase alone redraws the rapidly blinking
// cell and nothing else.
// covers: sgr.rapid-blink
#[test]
fn a_rapid_phase_change_redraws_its_cells() {
    let mut s = Screen::new(4, 1, rgb(FG), rgb(BG));
    s.put(0, 0, 'a', false, with(Attrs::BLINK));
    s.put(2, 0, 'b', false, with(Attrs::RAPID_BLINK));
    let mut r = Renderer::new(
        unifont(),
        32,
        16,
        Config {
            blink: FULL,
            ..Config::default()
        },
    );
    let mut c = Canvas::new(32, 16, 0);
    let mut st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    let since = s.damage.checkpoint();
    st.rapid_hidden = true;
    let damage = r.draw(&s.frame(since), &mut c, 1, &st).damage.to_vec();
    let cell = Rect {
        x: 16,
        y: 0,
        width: 8,
        height: 16,
    };
    assert_eq!(damage, [cell]);
}
