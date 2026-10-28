//! The shades at 1 bit (vtium/vtium#135): ordered 2x2 patterns a quarter,
//! half and three quarters lit, which repeat every 2 pixels so cells of
//! even sides join.

use vtium_font::sprite::shade_one_bit;

/// The shade's rows as `#` lit and `.` unlit.
fn rows(c: char, w: u32, h: u32) -> Vec<String> {
    let g = shade_one_bit(c, w, h).unwrap();
    g.rows()
        .map(|r| {
            r.iter()
                .map(|&p| if p == 0xff { '#' } else { '.' })
                .collect()
        })
        .collect()
}

#[test]
fn each_shade_is_its_ordered_pattern() {
    assert_eq!(rows('\u{2591}', 4, 4), ["#.#.", "....", "#.#.", "...."]);
    assert_eq!(rows('\u{2592}', 4, 4), ["#.#.", ".#.#", "#.#.", ".#.#"]);
    assert_eq!(rows('\u{2593}', 4, 4), ["####", ".#.#", "####", ".#.#"]);
}

#[test]
fn only_the_shades_take_a_pattern() {
    for c in ['\u{2588}', '\u{2590}', '\u{2594}', 'A'] {
        assert!(shade_one_bit(c, 8, 16).is_none(), "{c}");
    }
    assert!(shade_one_bit('\u{2591}', 0, 16).is_none());
}

#[test]
fn every_pixel_is_lit_or_unlit_in_its_share() {
    for (c, share) in [('\u{2591}', 4), ('\u{2592}', 2)] {
        for (w, h) in [(8, 8), (8, 16), (16, 32), (12, 24)] {
            let g = shade_one_bit(c, w, h).unwrap();
            assert!(g.pixels().iter().all(|&p| p == 0 || p == 0xff));
            let lit = g.pixels().iter().filter(|&&p| p == 0xff).count() as u32;
            assert_eq!(lit, w * h / share, "{c} {w}x{h}");
        }
    }
}
