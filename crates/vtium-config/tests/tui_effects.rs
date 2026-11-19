//! The Effects tab driven by its events: its list in groups, the filter
//! line and its list of groups, an item's detail, and Space through the
//! controls the resource tabs share. `tui.rs` says how the tests drive the
//! TUI, and `tui_common/` holds the helpers.

mod tui_common;
use tui_common::*;
use vtium_config::batch::{Change, State};
use vtium_config::tui::desk::Op;
use vtium_config::tui::input::Key;
use vtium_config::tui::model::{App, Effect, Event, View};
use vtium_config::tui::nav;

/// The app on the Effects tab at 80×25.
fn effects() -> App {
    let mut a = app(80, 25);
    press(&mut a, Key::Char('2'));
    assert_eq!(a.nav.view, View::Effects);
    a
}

/// Moves the highlight to the item `name`, from the filter line down.
fn to(a: &mut App, name: &str) {
    press(a, Key::Home);
    for _ in 0..20 {
        if highlighted(a, name) {
            return;
        }
        press(a, Key::Down);
    }
    panic!("never reached {name}:\n{}", text(a));
}

/// Whether the highlighted item is `name`.
fn highlighted(a: &App, name: &str) -> bool {
    a.effects
        .highlighted(&a.shared)
        .is_some_and(|item| match item {
            vtium_config::tui::tabs::effects::Effect::Vtlook(i) => a
                .shared
                .inputs
                .effects
                .vtlooks
                .get(i)
                .is_some_and(|v| v.name == name || (name == "No filter" && i == 0)),
            other => vtium_config::tui::tabs::effects::Effects::resource(&a.shared, other)
                .is_some_and(|r| r.name() == name),
        })
}

#[test]
fn the_list_shows_no_filter_then_the_presets_then_the_administrator_s_vtlooks() {
    let a = effects();
    let screen = text(&a);
    let at = |s: &str| screen.find(s).unwrap_or_else(|| panic!("{s}:\n{screen}"));
    assert!(at(" Show: All effects") < at(" vtlooks"), "{screen}");
    assert!(at("No filter") < at("amber"), "{screen}");
    assert!(at("amber") < at("sepia"), "{screen}");
    assert!(at(" vtlooks") < at(" Color maps"), "{screen}");
    // the preset green is hidden by the administrator's green, which does
    // not read
    assert!(
        screen.contains("preset, hidden by the administrator's vtlook of this name"),
        "{screen}"
    );
    assert!(
        screen.contains("administrator's, does not read"),
        "{screen}"
    );
    assert!(screen.contains("sepia.cube"), "{screen}");
    assert!(screen.contains("active, trusted"), "{screen}");
    assert!(
        screen.contains("not active, fails the trust rule"),
        "{screen}"
    );
    assert!(screen.contains("not usable"), "{screen}");
    assert!(screen.contains("No shaders are installed."), "{screen}");
    assert_eq!(nav::node(&a), "effects");
}

#[test]
fn the_filter_line_opens_the_groups_and_shows_one() {
    let mut a = effects();
    press(&mut a, Key::Enter);
    assert_eq!(nav::node(&a), "effects.filter");
    // the action keys do nothing while the list of groups is open
    assert!(press(&mut a, Key::Char('s')).is_empty());
    press(&mut a, Key::Down);
    press(&mut a, Key::Down);
    press(&mut a, Key::Enter);
    assert_eq!(nav::node(&a), "effects");
    let screen = text(&a);
    assert!(screen.contains(" Show: Color maps"), "{screen}");
    assert!(!screen.contains("amber"), "{screen}");
    assert!(screen.contains("warm.cube"), "{screen}");
    // Esc closes the list of groups and keeps the group shown
    press(&mut a, Key::Enter);
    press(&mut a, Key::Escape);
    assert!(text(&a).contains(" Show: Color maps"));
}

#[test]
fn a_vtlook_s_detail_shows_its_filters_files_and_looks() {
    let mut a = effects();
    to(&mut a, "sepia");
    press(&mut a, Key::Enter);
    assert_eq!(nav::node(&a), "effects.effect");
    let screen = text(&a);
    assert!(screen.contains(" vtlook sepia"), "{screen}");
    assert!(
        screen.contains("File: /etc/vtium/looks/sepia.vtlook"),
        "{screen}"
    );
    assert!(screen.contains("cube: file sepia.cube"), "{screen}");
    assert!(
        screen.contains("/etc/vtium/looks/sepia.cube, active"),
        "{screen}"
    );
    assert!(screen.contains("Used by look proof."), "{screen}");
    // Down scrolls the lines under the title, which stays
    press(&mut a, Key::Down);
    let scrolled = text(&a);
    assert!(scrolled.contains(" vtlook sepia"), "{scrolled}");
    assert!(!scrolled.contains("File: /etc"), "{scrolled}");
    press(&mut a, Key::Escape);
    assert_eq!(nav::node(&a), "effects");
    // No filter's detail says it is built in
    to(&mut a, "No filter");
    press(&mut a, Key::Enter);
    let screen = text(&a);
    assert!(
        screen.contains("File: none, it is built into vtium"),
        "{screen}"
    );
    assert!(screen.contains("It has no filter"), "{screen}");
}

#[test]
fn a_color_map_s_detail_shows_its_marks_and_the_check_it_fails() {
    let mut a = effects();
    to(&mut a, "loose.cube");
    press(&mut a, Key::Enter);
    let screen = text(&a);
    assert!(screen.contains(" color map loose.cube"), "{screen}");
    assert!(screen.contains("Active: not active"), "{screen}");
    assert!(screen.contains("the file belongs to user 1000"), "{screen}");
    assert!(screen.contains("No look uses it."), "{screen}");
}

#[test]
fn space_activates_a_color_map_through_the_desk_and_the_mark_follows() {
    let mut a = effects();
    to(&mut a, "warm.cube");
    let out = press(&mut a, Key::Char(' '));
    let op = Op::Resource {
        path: "/etc/vtium/looks/warm.cube".into(),
        change: Change::Activate,
    };
    assert_eq!(out, [Effect::Desk(op.clone())]);
    // the desk's answer marks the color map and says what changed
    let mut states = active();
    let done = vtium_config::tui::fixture::resource_op(
        &mut states,
        std::path::Path::new("/etc/vtium/looks/warm.cube"),
        Change::Activate,
    );
    a.event(Event::Desked(op, Ok(done)));
    let warm = a
        .shared
        .inputs
        .effects
        .maps
        .iter()
        .find(|m| m.name() == "warm.cube")
        .map(|m| m.state);
    assert_eq!(warm, Some(State::Activates));
    let screen = text(&a);
    assert!(screen.contains("activates on install, trusted"), "{screen}");
    assert!(
        screen.contains("The draft activates this color map."),
        "{screen}"
    );
    // a vtlook takes no Space
    to(&mut a, "amber");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
}

#[test]
fn space_asks_before_it_deactivates_a_color_map_that_a_look_uses() {
    let mut a = effects();
    to(&mut a, "sepia.cube");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert_eq!(nav::node(&a), "any.question.deactivate");
    let screen = text(&a);
    assert!(screen.contains("Look proof uses it"), "{screen}");
    // Esc goes back without acting
    press(&mut a, Key::Escape);
    assert_eq!(nav::node(&a), "effects");
    press(&mut a, Key::Char(' '));
    let out = press(&mut a, Key::Char('y'));
    assert_eq!(
        out,
        [Effect::Desk(Op::Resource {
            path: "/etc/vtium/looks/sepia.cube".into(),
            change: Change::Deactivate,
        })]
    );
}

#[test]
fn space_on_a_color_map_vtium_cannot_use_says_why() {
    let mut a = effects();
    to(&mut a, "loose.cube");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert!(text(&a).contains("fails the trust rule"));
    to(&mut a, "mine.cube");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    let screen = text(&a);
    assert!(screen.contains("in your own folder"), "{screen}");
    // a refusal from the desk goes on the message line
    a.event(Event::Desked(
        Op::Resource {
            path: "/x.cube".into(),
            change: Change::Activate,
        },
        Err("the file is not active".into()),
    ));
    assert!(text(&a).contains("the file is not active"));
}

/// The screen the loop draws in a terminal of `cols` by `rows` that shows
/// pictures, `ops` saying whether it is a vtium console that switches its
/// font, after `keys`; and the console, which keeps each picture asked
/// for.
fn pictured(ops: bool, cols: usize, rows: usize, keys: &[&[u8]]) -> (String, Console) {
    let grid = (cols, rows);
    let mut c = Console::vtium(cols, rows, ops, "unifont")
        .takes("proof", grid)
        .takes("unifont", grid)
        .keys(keys);
    c.graphics = true;
    drive(&mut c, inputs()).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    (screen, c)
}

/// The screen's text as one line, so a sentence wrapped across rows reads
/// whole.
fn joined(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The keys from the Flow view to the Effects tab's item at `place`, 1
/// for No filter.
fn to_item(place: usize) -> Vec<&'static [u8]> {
    let mut keys: Vec<&'static [u8]> = vec![TO_EFFECTS];
    keys.extend(std::iter::repeat_n(DOWN, place));
    keys
}

// The preview of a preset asks for the sample screen drawn through the
// preset's file, with the draft's active color maps as the only ones it
// may read, and the line beside it names the vtlook. No filter asks for
// the plain sample screen, and its line says so. In a vtium console that
// switches its font, the Effects tab still previews as a picture, since
// the console's own text cannot show an effect.
#[test]
fn a_preset_is_previewed_through_its_file_and_no_filter_plain() {
    use vtium_config::specimen::{Filter, Filtered};
    for ops in [false, true] {
        let (screen, c) = pictured(ops, 100, 30, &to_item(2));
        let shot = c.shots.last().expect("a picture");
        assert_eq!(
            shot.filter,
            Some(Filtered {
                filter: Filter::Vtlook {
                    name: "amber".to_owned(),
                    file: "/usr/share/vtium/looks/amber.vtlook".into(),
                },
                active: vec!["/etc/vtium/looks/sepia.cube".into()],
            }),
            "ops {ops}"
        );
        let first = vtium_config::tui::view::text(&shot.screen[0]);
        assert!(first.starts_with("user@desk:~/notes$ ls -l"), "{first}");
        assert!(
            joined(&screen)
                .contains("The picture shows the sample screen through the vtlook amber."),
            "{screen}"
        );
        // with VTIUM_CONFIG_EFFECT_PPM=FILE, the picture as the glyph
        // helper draws it and the preset in this checkout colors it, for
        // the maintainer's review
        if let Some(out) = std::env::var_os("VTIUM_CONFIG_EFFECT_PPM") {
            draw_amber(shot, &out);
        }
        let (screen, c) = pictured(ops, 100, 30, &to_item(1));
        assert_eq!(c.shots.last().expect("a picture").filter, None);
        assert!(
            joined(&screen).contains("The picture shows the sample screen with no filter."),
            "{screen}"
        );
    }
}

/// Writes `shot` as the glyph helper draws it, through the amber preset
/// in this checkout, to the PPM file `out`.
fn draw_amber(shot: &vtium_config::specimen::Shot, out: &std::ffi::OsStr) {
    use vtium_config::specimen::{Filter, Filtered, Outcome};
    let helper = vtium_config::specimen::Helper::at(env!("CARGO_BIN_EXE_vtium-config"));
    let frit = vtium_config::tui::frit_text(Path::new("/nonexistent"), &shot.font)
        .expect("a built-in font's frit");
    let mut picture =
        vtium_config::specimen::screen(&helper, &frit, shot, "C").expect("the picture");
    let looks = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../vtium/install/looks")
        .canonicalize()
        .expect("the presets");
    let euid = rustix::process::geteuid().as_raw();
    let outcome = vtium_config::specimen::through(
        &mut picture,
        &Filtered {
            filter: Filter::Vtlook {
                name: "amber".to_owned(),
                file: looks.join("amber.vtlook"),
            },
            active: Vec::new(),
        },
        &looks,
        &vtium_settings::Trust::running_as(euid),
        vtium_settings::cullet::Rule::running_as(euid),
    );
    assert_eq!(outcome, Outcome::Applied);
    let mut ppm = format!("P6\n{} {}\n255\n", picture.width, picture.height).into_bytes();
    for px in picture.rgba.chunks(4) {
        ppm.extend_from_slice(px.get(..3).unwrap_or_default());
    }
    std::fs::write(out, ppm).expect("the picture's file");
}

// A color map that is not active, one that fails the trust rule, one in
// the person's own folder, a vtlook that does not read and a shader each
// leave the picture plain, and the line beside it says why. A vtlook that
// names an active color map is drawn through.
#[test]
fn an_effect_vtium_cannot_use_leaves_the_picture_plain_and_says_why() {
    let because = |place: usize, why: &str| {
        let (screen, c) = pictured(false, 100, 30, &to_item(place));
        assert_eq!(c.shots.last().expect("a picture").filter, None, "{screen}");
        let want =
            format!("The picture shows the sample screen without this effect, because {why}.");
        assert!(joined(&screen).contains(&want), "{want}\n{screen}");
    };
    because(
        4,
        "this vtlook does not read, so vtium renders the built-in vtlook in its place",
    );
    because(
        7,
        "this color map is not active, so vtium treats it as missing",
    );
    because(
        8,
        "this color map fails the trust rule, so vtium treats it as missing",
    );
    because(
        9,
        "this color map is in your own folder, where vtium cannot use it",
    );
    let (_, c) = pictured(false, 100, 30, &to_item(5));
    assert!(
        c.shots.last().expect("a picture").filter.is_some(),
        "sepia names the active sepia.cube"
    );
}

// The line beside a picture drawn through an effect says what became of
// the effect: a vtlook whose colors vtium cannot build leaves the picture
// plain, and the line gives the reader's first error; a vtlook with
// spatial filters shows its colors and says what it leaves out.
#[test]
fn the_line_says_what_became_of_the_effect() {
    use vtium_config::specimen::{Outcome, Picture};
    let answer = |outcome: Outcome| {
        let mut a = app(100, 30);
        press(&mut a, Key::Char('2'));
        to(&mut a, "amber");
        // another terminal, which shows pictures from here on and has
        // asked for none, so the next key asks for the font's cell toward
        // the picture of amber
        a.shared.live = vtium_config::tui::model::Live::NotVtium;
        a.shared.graphics = true;
        a.shared.drawing = false;
        a.shared.asked = None;
        let asked = press(&mut a, Key::Char('z'));
        let Some((font, height)) = asked.iter().find_map(|e| match e {
            Effect::Cell { font, height } => Some((font.clone(), *height)),
            _ => None,
        }) else {
            panic!("no cell asked for: {asked:?}");
        };
        let cell = vtium_font::Cell {
            width: 8,
            height: 16,
        };
        let drawn = a.event(Event::Celled {
            font,
            height,
            cell: Ok((cell, 16)),
        });
        assert!(
            drawn
                .iter()
                .any(|e| matches!(e, Effect::Picture(shot) if shot.filter.is_some())),
            "{drawn:?}"
        );
        a.event(Event::Filtered {
            picture: Picture {
                width: 1,
                height: 1,
                rgba: vec![0, 0, 0, 255],
            },
            outcome,
        });
        joined(&text(&a))
    };
    let shown = answer(Outcome::Failed(
        "color filter 1 of 1, cube: reading: the file is missing.".into(),
    ));
    assert!(
        shown.contains("without this effect, because vtium cannot build its colors: color filter 1 of 1, cube: reading: the file is missing."),
        "{shown}"
    );
    let shown = answer(Outcome::ColorOnly);
    assert!(
        shown.contains("through the vtlook amber. It leaves out the vtlook's spatial filters, which only the GPU draws."),
        "{shown}"
    );
}

// The Effects tab with the preset amber highlighted, in a terminal that
// shows pictures: the list, the sentences under it, and the line beside
// the picture.
#[test]
fn the_effects_tab_previews_a_preset_at_80x25_and_100x30() {
    for (cols, rows) in [(80, 25), (100, 30)] {
        let (screen, c) = pictured(false, cols, rows, &to_item(2));
        assert!(
            joined(&screen).contains("through the vtlook amber."),
            "{screen}"
        );
        let bytes = c.written.get(..c.seen_at).unwrap_or_default().to_vec();
        golden(&format!("effects-amber-{cols}x{rows}"), &screen, &bytes);
    }
}
