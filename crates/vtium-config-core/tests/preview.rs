//! The previews of the Fonts, Looks and Effects tabs on the screen map's
//! machine and the large one, at 100×30 and at 80×25: the steps of a
//! picture and the cells it takes in the place a tab gives it, the fonts
//! the font helper refuses, the line in place of a picture, the effects a
//! picture is drawn through, and the lines a vtium console draws as its own
//! preview.
#![cfg(feature = "fixture")]

use vtium_config_core::fixture::{large, padnoir};
use vtium_config_core::preview::console::{self, Shown};
use vtium_config_core::preview::{
    Applied, Covered, Facts, Pictures, Place, Preview, Source, Unpictured, readable,
};
use vtium_config_core::specimen::{Filter, Picture, Shot};
use vtium_config_core::ui::{Effect, Live, Shared};
use vtium_config_widgets::canvas;
use vtium_font::Cell;

/// The two sizes every preview is checked at: the size vtium-config is
/// designed for and its minimum.
const SIZES: [(usize, usize); 2] = [(100, 30), (80, 25)];

/// This terminal's character cell in pixels in the tests.
const CELL_PX: (u32, u32) = (10, 20);

/// The display the tests' pictures are counted on.
const DISPLAY: (u32, u32) = (1920, 1080);

/// The facts of a terminal of `grid` on a machine with one 1920×1080
/// display at 96 dpi.
fn facts(grid: (usize, usize)) -> Facts {
    Facts {
        grid,
        cell_px: Some(CELL_PX),
        dpi: 96,
        locale: String::new(),
        color: true,
        display: Some(DISPLAY),
        text_area: None,
        head: None,
    }
}

/// The fixture `shared` in a terminal that is not a vtium console and
/// shows pictures.
fn showing(mut shared: Shared) -> Shared {
    shared.console.live = Live::NotVtium;
    shared.console.graphics = true;
    shared
}

/// A place in the viewer of a terminal of `grid`, as a tab gives it: under
/// a list of eight rows, from the left edge, down to the row above the
/// message line.
fn place((cols, rows): (usize, usize)) -> Place {
    let side = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
    Place::new(1, 10, side(cols - 2), side(rows - 13))
}

/// The one effect of `effects`.
fn only(effects: Vec<Effect>) -> Effect {
    let [one] = <[Effect; 1]>::try_from(effects).expect("one effect");
    one
}

/// A picture of `width` by `height` pixels.
fn picture(width: u32, height: u32) -> Picture {
    Picture {
        width,
        height,
        rgba: vec![0; usize::try_from(width * height * 4).unwrap_or(0)],
    }
}

/// The picture of vtium-config's screen in `font` at the document's 12 pt
/// takes the font's cell, the grid that cell gives on the display, the
/// pixels the place holds, and, once it comes, the cells it covers from
/// the place's top left. These are the cells today's tui/shot.rs takes for
/// the same request and the same place.
#[test]
fn a_picture_of_a_font_set_takes_the_cells_of_today_s_tui() {
    for (shared, font) in [(padnoir::shared(), "proof"), (large::shared(), "set-00")] {
        let shared = showing(shared);
        for grid in SIZES {
            let facts = facts(grid);
            let place = place(grid);
            let preview = Preview::of(
                &shared,
                &facts,
                Source::Font {
                    font: Some(font),
                    detail: false,
                },
            );
            let mut pictures = Pictures::default();
            // 12 pt at 96 dpi is 16 pixels high
            let asked = pictures.ask(&preview, place);
            assert_eq!(
                asked.effects,
                vec![Effect::Cell {
                    font: font.to_owned(),
                    height: 16
                }],
                "{grid:?}"
            );
            assert!(
                pictures.ask(&preview, place).effects.is_empty(),
                "asked once"
            );
            let cell = Cell {
                width: 8,
                height: 16,
            };
            let celled = pictures.celled(&preview, place, font, 16, Ok((cell, 16)));
            let Effect::ScreenPicture(shot) = only(celled.effects) else {
                panic!("a picture of the screen");
            };
            let (pw, ph) = place.area();
            let max = (
                (u32::try_from(pw).unwrap_or(0) * CELL_PX.0).min(1024),
                (u32::try_from(ph).unwrap_or(0) * CELL_PX.1).min(768),
            );
            assert_eq!(
                shot,
                Shot {
                    font: font.to_owned(),
                    height: 16,
                    cell,
                    face_height: 16,
                    display: DISPLAY,
                    grid: (240, 67),
                    screen: Vec::new(),
                    palette: String::new(),
                    color: true,
                    max,
                    filter: None,
                },
                "{grid:?}"
            );
            let line = preview.line(&pictures).expect("a caption").line;
            assert!(
                line.contains("8×16 px") && line.contains("240×67"),
                "{line}"
            );
            let shown = pictures.pictured(&preview, place, Ok(picture(305, 201)));
            let show = shown.show.expect("shown");
            assert_eq!(show.at, (11, 2), "from the place's top left, from 1");
            assert_eq!(
                pictures.shown,
                Some(Covered {
                    row: 10,
                    col: 1,
                    cols: 31,
                    rows: 11
                }),
                "{grid:?}"
            );
        }
    }
}

/// The Looks tab's picture of a size lays the sample screen out at the
/// size's cell on its nominal 1024×768 screen, and its caption still
/// counts the real display's grid.
#[test]
fn a_size_s_picture_lays_the_sample_screen_out_at_its_cell() {
    let shared = showing(padnoir::shared());
    for grid in SIZES {
        let facts = facts(grid);
        let place = place(grid);
        let preview = Preview::of(
            &shared,
            &facts,
            Source::Size {
                font: Some("proof"),
                points: 11,
            },
        );
        let mut pictures = Pictures::default();
        // 11 pt at 96 dpi is 15 pixels high
        let asked = pictures.ask(&preview, place);
        assert_eq!(
            only(asked.effects),
            Effect::Cell {
                font: "proof".into(),
                height: 15
            }
        );
        let cell = Cell {
            width: 7,
            height: 15,
        };
        let celled = pictures.celled(&preview, place, "proof", 15, Ok((cell, 15)));
        let Effect::Picture(shot) = only(celled.effects) else {
            panic!("a picture of the sample screen");
        };
        assert_eq!(shot.display, (1024, 768));
        assert_eq!(shot.grid, (146, 51));
        assert_eq!(shot.screen.len(), 51);
        let text = |row: &canvas::Row| row.iter().map(|s| s.text.as_str()).collect::<String>();
        assert!(shot.screen.iter().all(|r| text(r).chars().count() == 146));
        assert!(text(&shot.screen[0]).starts_with("user@desk:~/notes$ ls -l"));
        let caption = pictures.caption.as_ref().expect("a caption");
        assert_eq!(caption.grid, (274, 72), "the real display's grid");
    }
}

/// A font set that does not read, one that is not text, and a font that
/// is not installed get no picture, and moving onto one deletes the
/// picture on the screen. The built-in fonts and the font sets that read
/// are drawn.
#[test]
fn the_fonts_the_font_helper_refuses_get_no_picture() {
    let shared = showing(padnoir::shared());
    for name in ["broken", "latin1", "missing", "/no/such/file.ttf"] {
        assert!(!readable(&shared, name), "{name}");
    }
    for name in ["proof", "minspec-16", "tiny5", "unifont", "unscii-8"] {
        assert!(readable(&shared, name), "{name}");
    }
    let facts = facts((100, 30));
    let place = place((100, 30));
    let mut pictures = Pictures::default();
    let proof = Preview::of(
        &shared,
        &facts,
        Source::Font {
            font: Some("proof"),
            detail: false,
        },
    );
    pictures.ask(&proof, place);
    let cell = Cell {
        width: 8,
        height: 16,
    };
    pictures.celled(&proof, place, "proof", 16, Ok((cell, 16)));
    pictures.pictured(&proof, place, Ok(picture(80, 32)));
    assert!(pictures.shown.is_some());
    let broken = Preview::of(
        &shared,
        &facts,
        Source::Font {
            font: Some("broken"),
            detail: false,
        },
    );
    assert_eq!(broken.wanted(), None);
    let moved = pictures.ask(&broken, place);
    assert!(moved.effects.is_empty() && moved.delete, "{moved:?}");
    assert_eq!(pictures.shown, None);
}

/// A picture that comes after the highlight moved on is not shown, and a
/// cell for a font no longer highlighted asks for nothing.
#[test]
fn a_picture_that_comes_after_the_highlight_moved_is_not_shown() {
    let shared = showing(padnoir::shared());
    let facts = facts((80, 25));
    let place = place((80, 25));
    let source = |font| Source::Font {
        font: Some(font),
        detail: false,
    };
    let proof = Preview::of(&shared, &facts, source("proof"));
    let mut pictures = Pictures::default();
    pictures.ask(&proof, place);
    let tiny = Preview::of(&shared, &facts, source("tiny5"));
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let late = pictures.celled(&tiny, place, "proof", 16, Ok((cell, 16)));
    assert!(late.effects.is_empty());
    assert!(!pictures.drawing, "the next is asked for");
    let shown = pictures.pictured(&tiny, place, Ok(picture(80, 32)));
    assert_eq!(shown.show, None);
    assert_eq!(pictures.shown, None);
}

/// The detailed specimen is asked for in one step, at most the pixels its
/// place holds, and a failed picture says why on the message line.
#[test]
fn the_detailed_specimen_is_one_step_and_a_failure_says_why() {
    let shared = showing(padnoir::shared());
    for grid in SIZES {
        let facts = facts(grid);
        let place = place(grid);
        let preview = Preview::of(
            &shared,
            &facts,
            Source::Font {
                font: Some("proof"),
                detail: true,
            },
        );
        let mut pictures = Pictures::default();
        let asked = pictures.ask(&preview, place);
        assert_eq!(
            only(asked.effects),
            Effect::Specimen {
                font: "proof".into(),
                height: 16,
                max: place.max(Some(CELL_PX)),
                display: DISPLAY,
            }
        );
        let failed = pictures.detailed(&preview, place, "proof".into(), 16, Err("gone".into()));
        assert_eq!(
            failed.say.as_deref(),
            Some("No picture of the console: gone")
        );
        assert_eq!(pictures.unpictured, Some(Unpictured::Failed));
    }
}

/// Where this terminal shows no pictures, a font is described in a line,
/// and an effect, which has no description, asks for nothing.
#[test]
fn a_terminal_with_no_pictures_gets_a_line() {
    let mut shared = showing(padnoir::shared());
    shared.console.graphics = false;
    let facts = facts((80, 25));
    let place = place((80, 25));
    let preview = Preview::of(
        &shared,
        &facts,
        Source::Font {
            font: Some("proof"),
            detail: false,
        },
    );
    let mut pictures = Pictures::default();
    let asked = pictures.ask(&preview, place);
    assert_eq!(
        only(asked.effects),
        Effect::Summary {
            font: "proof".into(),
            height: 16
        }
    );
    pictures.summarized("proof".into(), 16, Err("too small".into()));
    let line = preview.line(&pictures).expect("a line").line;
    assert!(
        line.starts_with("This terminal shows no pictures"),
        "{line}"
    );
    let effect = Preview::of(
        &shared,
        &facts,
        Source::Effect {
            font: Some("proof"),
            effect: Applied::Nothing,
        },
    );
    let mut pictures = Pictures::default();
    assert!(pictures.ask(&effect, place).effects.is_empty());
}

/// An effect vtium can use is drawn through with the document's active
/// color maps only; one it cannot use leaves the picture plain, and the
/// line says why.
#[test]
fn an_effect_is_drawn_through_with_the_active_color_maps() {
    let shared = showing(padnoir::shared());
    let facts = facts((100, 30));
    let effects = &shared.found.effects;
    let sepia = effects.vtlook("sepia").expect("sepia");
    let preview = |effect| {
        Preview::of(
            &shared,
            &facts,
            Source::Effect {
                font: Some("proof"),
                effect,
            },
        )
    };
    let filtered = preview(Applied::Vtlook(sepia))
        .wanted()
        .and_then(|w| w.filter)
        .expect("drawn through sepia");
    assert!(matches!(&filtered.filter, Filter::Vtlook { name, .. } if name == "sepia"));
    let names: Vec<String> = filtered
        .active
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"sepia.cube".to_owned()), "{names:?}");
    assert!(!names.contains(&"warm.cube".to_owned()), "{names:?}");
    let pictures = Pictures::default();
    let green = effects.vtlook("green").expect("green");
    let broken = preview(Applied::Vtlook(green));
    assert_eq!(broken.wanted().and_then(|w| w.filter), None);
    let said = broken.line(&pictures).expect("a line");
    assert!(
        said.line.contains("leaves out this vtlook"),
        "{}",
        said.line
    );
    assert!(said.more.is_some_and(|m| m.contains("glow")));
    let warm = effects
        .maps
        .iter()
        .find(|r| r.name() == "warm.cube")
        .expect("warm");
    let line = preview(Applied::Map(warm))
        .line(&pictures)
        .expect("a line")
        .line;
    assert!(line.contains("is not active"), "{line}");
}

/// In a vtium console that vtium-config can switch, the console draws the
/// preview as its own lines from the top left of the tab's place, as many
/// as the place holds, each cut at its right edge, and nothing is asked of
/// the font helper. Another terminal gets no lines, and neither does a
/// font file, which the console cannot draw its own text in.
#[test]
fn a_vtium_console_draws_its_own_preview_lines_in_the_place() {
    for (shared, font) in [(padnoir::shared(), "proof"), (large::shared(), "set-00")] {
        let mut shared = showing(shared);
        for grid in SIZES {
            let facts = facts(grid);
            let place = place(grid);
            let source = Source::Size {
                font: Some(font),
                points: 11,
            };
            shared.console.live = Live::NotVtium;
            assert!(
                console::lines(&Preview::of(&shared, &facts, source), Shown::Sample).is_empty()
            );
            shared.console.live = Live::On;
            let preview = Preview::of(&shared, &facts, source);
            let lines = console::lines(&preview, Shown::Sample);
            assert_eq!(lines, console::sample_rows());
            let placed = console::placed(lines, place);
            let held = usize::from(place.height).min(console::sample_rows().len());
            assert_eq!(placed.len(), held, "{grid:?}");
            for (n, p) in placed.iter().enumerate() {
                assert_eq!(usize::from(p.row), usize::from(place.y) + n, "{grid:?}");
                assert_eq!(p.col, place.x);
                let text: String = p.line.iter().map(|s| s.text.as_str()).collect();
                assert!(canvas::cells(&text) <= usize::from(place.width), "{text}");
            }
            let first: String = placed[0].line.iter().map(|s| s.text.as_str()).collect();
            assert!(first.starts_with("user@desk"), "{first}");
            let mut pictures = Pictures::default();
            assert!(pictures.ask(&preview, place).effects.is_empty());
            let file = Preview::of(
                &shared,
                &facts,
                Source::Font {
                    font: Some("/usr/share/fonts/TTF/DejaVuSansMono.ttf"),
                    detail: false,
                },
            );
            assert!(console::lines(&file, Shown::Detail).is_empty());
        }
    }
}

/// The detailed specimen's lines, which the console draws in place of the
/// sample screen with `p`, come one to a row.
#[test]
fn a_vtium_console_draws_the_detailed_specimen_s_lines() {
    let mut shared = padnoir::shared();
    shared.console.live = Live::On;
    let facts = facts((80, 25));
    let preview = Preview::of(
        &shared,
        &facts,
        Source::Font {
            font: Some("proof"),
            detail: true,
        },
    );
    let lines = console::lines(&preview, Shown::Detail);
    let want = vtium_config_core::specimen::detail_lines("");
    assert_eq!(lines.len(), want.len());
    assert_eq!(lines[0][0].text, want[0]);
}
