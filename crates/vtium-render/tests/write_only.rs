//! The display buffer is never read (ADR-1300, "Carried forward"):
//! `Target` has no way to read, so the renderer cannot; and what it draws
//! does not depend on what the buffer held. A mapped buffer full of
//! garbage and a canvas of zeros, drawn the same frames, hold the same
//! pixels after every frame.

mod common;

use common::{Screen, WriteOnly, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Mapped, Renderer};
use vtium_term::{Age, Attrs, UnderlineStyle};

#[test]
fn what_is_drawn_does_not_depend_on_what_the_buffer_held() {
    let (w, h) = (10 * 8 + 5, 3 * 16 + 3);
    let stride = (w as usize * 4).div_ceil(64) * 64;
    let mut garbage = vec![0xa5u8; stride * h as usize];
    let mut zeros = Canvas::new(w, h, 0);
    let mut s = Screen::new(10, 3, rgb("c0c0c0"), rgb("101010"));
    let mut a = Renderer::new(unifont(), w, h, Config::default());
    let mut b = Renderer::new(unifont(), w, h, Config::default());
    let mut since = Age::NEVER;
    for step in 0..6 {
        // one buffer: unknown at first, then holding the frame before
        let age = u32::from(step > 0);
        let mut st = style(rgb("ffffff"), rgb("000080"));
        if step % 2 == 1 {
            st.attrs.set_underline(UnderlineStyle::Dotted);
            st.attrs.insert(Attrs::STRIKE);
        }
        s.text(step, step % 3, "x中", st);
        let ds = DrawState {
            foreground: s.palette.fg(),
            background: s.palette.bg(),
            ..DrawState::default()
        };
        {
            let mut m = Mapped::new(&mut garbage, w, h, stride).unwrap();
            a.draw(&s.frame(since), &mut m, age, &ds);
        }
        b.draw(&s.frame(since), &mut zeros, age, &ds);
        since = s.damage.checkpoint();
        for y in 0..h {
            for x in 0..w {
                let at = y as usize * stride + x as usize * 4;
                let px = u32::from_le_bytes(garbage[at..at + 4].try_into().unwrap());
                assert_eq!(px, zeros.pixel(x, y).unwrap(), "step {step} at {x},{y}");
            }
        }
    }
}

// Every write lands inside the display: the target never sees a run past
// its edge.
#[test]
fn writes_stay_inside_the_display() {
    let mut s = Screen::new(30, 4, rgb("c0c0c0"), rgb("101010"));
    s.put(28, 0, '中', true, style(rgb("ffffff"), rgb("000000")));
    for (w, h) in [(30 * 8, 4 * 16), (29 * 8 + 7, 3 * 16 + 15), (5, 5)] {
        let mut t = WriteOnly::new(w, h);
        let mut r = Renderer::new(unifont(), w, h, Config::default());
        let ds = DrawState {
            pointer: Some((10_000, 10_000)),
            ..DrawState::default()
        };
        r.draw(&s.frame(Age::NEVER), &mut t, 0, &ds);
        assert!(t.writes > 0);
    }
}
