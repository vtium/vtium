//! The cache changes speed, never pixels (ADR-1300, "Verification"): the
//! same frames drawn with a cache of 0 glyphs, 1, a small prime, the
//! minspec and the performance budgets, and with 1-bit and scale-1 storage
//! each turned off, give the same bytes, including screens that hold more
//! distinct glyphs than the cache.

mod common;

use common::{Screen, reference_screens, rgb, style};
use vtium_font::{Font, Unifont};
use vtium_render::{Budgets, Canvas, Config, DrawState, Profile, Renderer, Storage};
use vtium_term::{Age, Attrs, CursorShape, Rgb, UnderlineStyle};

/// Bold and regular of the same letters, wide and narrow, sprites,
/// decorations and the cursor.
fn busy() -> Screen {
    let mut s = Screen::new(24, 4, rgb("c0c0c0"), rgb("101010"));
    let plain = style(rgb("e0e0e0"), rgb("101010"));
    let mut bold = plain;
    bold.attrs.insert(Attrs::BOLD);
    s.text(0, 0, "Bold b", plain);
    s.text(6, 0, "Bold b", bold);
    s.text(12, 0, "│─┼█▒⠿", plain);
    for (i, c) in "中文字体".chars().enumerate() {
        s.put(i * 2, 1, c, true, if i % 2 == 0 { plain } else { bold });
    }
    let mut ul = plain;
    ul.attrs.set_underline(UnderlineStyle::Curly);
    ul.attrs.insert(Attrs::STRIKE);
    s.text(0, 2, "curly struck", ul);
    s.put(14, 2, '😀', true, ul);
    s.text(0, 3, "the cursor is here", plain);
    s.cursor.row = 3;
    s.cursor.col = 4;
    s.cursor.visible = true;
    s.cursor.shape = CursorShape::Block;
    s
}

/// Distinct glyphs from `first`: narrow letters, then wide ideographs,
/// shifting by `shift` codepoints.
fn flood(shift: u32) -> Screen {
    let (cols, rows) = (40usize, 12usize);
    let mut s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("101010"));
    let mut n = shift;
    for row in 0..rows {
        let mut col = 0;
        while col < cols {
            let mut st = style(Rgb::new(200, (n % 200) as u8, 90), rgb("101010"));
            if n % 3 == 0 {
                st.attrs.insert(Attrs::BOLD);
            }
            if row % 2 == 0 {
                let c = char::from_u32(0x100 + n % 0x180).unwrap();
                s.put(col, row, c, false, st);
                col += 1;
            } else if col + 1 < cols {
                let c = char::from_u32(0x4e00 + n % 0x5000).unwrap();
                s.put(col, row, c, true, st);
                col += 2;
            } else {
                col += 1;
            }
            n += 1;
        }
    }
    s
}

struct Run {
    name: String,
    budgets: Option<Budgets>,
    profile: Profile,
    storage: Storage,
}

fn runs() -> Vec<Run> {
    let mut runs = Vec::new();
    for (name, budgets) in [
        ("0 glyphs", Some(Budgets::glyphs(0))),
        ("1 glyph", Some(Budgets::glyphs(1))),
        ("7 glyphs", Some(Budgets::glyphs(7))),
        ("97 glyphs", Some(Budgets::glyphs(97))),
        ("minspec", None),
    ] {
        runs.push(Run {
            name: name.into(),
            budgets,
            profile: Profile::Minspec,
            storage: Storage::default(),
        });
    }
    runs.push(Run {
        name: "performance".into(),
        budgets: None,
        profile: Profile::Performance,
        storage: Storage::default(),
    });
    for (name, storage) in [
        (
            "8-bit",
            Storage {
                bits: false,
                unscaled: true,
            },
        ),
        (
            "drawn size",
            Storage {
                bits: true,
                unscaled: false,
            },
        ),
    ] {
        runs.push(Run {
            name: name.into(),
            budgets: None,
            profile: Profile::Performance,
            storage,
        });
    }
    runs
}

/// Draws `frames` in turn at each run's budgets and storage, and checks
/// each frame against the same frame drawn with a cache of nothing.
/// Returns the glyphs drawn uncached at 7 glyphs, to show the cache filled.
fn check(height: u32, frames: &[Screen]) -> u32 {
    let (w, h) = (
        frames[0].cols as u32 * 8 * (height / 16) + 2,
        frames[0].rows as u32 * height + 2,
    );
    let mut want = Vec::new();
    let mut uncached_at_7 = 0;
    for run in runs() {
        let config = Config {
            budgets: run.budgets,
            profile: run.profile,
            storage: run.storage,
            ..Config::default()
        };
        let mut r = Renderer::new(Font::new(Unifont::new(height)), w, h, config);
        for (i, s) in frames.iter().enumerate() {
            let mut c = Canvas::new(w, h, 0);
            let st = DrawState {
                cursor_on: true,
                foreground: s.palette.fg(),
                background: s.palette.bg(),
                ..DrawState::default()
            };
            let drawn = r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
            if run.name == "7 glyphs" {
                uncached_at_7 += drawn.stats.uncached;
            }
            if run.name == "0 glyphs" {
                assert_eq!(drawn.stats.uncached, drawn.stats.misses);
                want.push(c);
            } else {
                assert!(
                    c == want[i],
                    "frame {i} at unifont {height} differs with {}",
                    run.name
                );
            }
        }
    }
    uncached_at_7
}

// covers: platform.compositor
#[test]
fn the_reference_screens_draw_the_same_at_every_cache_size() {
    for (name, s) in reference_screens() {
        let frames = [s];
        let _ = check(16, &frames);
        assert!(!name.is_empty());
    }
}

// covers: platform.compositor
#[test]
fn a_busy_screen_draws_the_same_at_every_cache_size_and_scale() {
    for height in [16, 32, 48] {
        let frames = [busy(), flood(0), busy()];
        assert!(check(height, &frames) > 0, "the cache of 7 filled");
    }
}

// A flood of distinct glyphs, more than every budget below the profiles
// holds, frame after frame as the text shifts
// covers: platform.compositor
#[test]
fn a_flood_of_distinct_glyphs_draws_every_glyph() {
    let frames: Vec<Screen> = (0..4).map(|i| flood(i * 131)).collect();
    let distinct = 12 * 40 / 2 + 12 * 20 / 2;
    assert!(distinct > 97);
    assert!(check(16, &frames) > 0);
}
