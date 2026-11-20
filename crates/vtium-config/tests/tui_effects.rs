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

// The test runs at 100×40, where the window of the list holds every
// item of the fixture; at 80×25 it holds five.
#[test]
fn the_list_shows_no_filter_then_the_presets_then_the_administrator_s_vtlooks() {
    let mut a = app(100, 40);
    press(&mut a, Key::Char('2'));
    let screen = text(&a);
    let at = |s: &str| screen.find(s).unwrap_or_else(|| panic!("{s}:\n{screen}"));
    assert!(at(" Show: All effects") < at(" vtlooks"), "{screen}");
    assert!(at("No filter") < at("amber"), "{screen}");
    assert!(at("amber") < at("sepia"), "{screen}");
    assert!(at(" vtlooks") < at(" Color maps"), "{screen}");
    // the preset green is hidden by the administrator's green, which has
    // an error
    assert!(
        screen.contains("preset, hidden by the administrator's vtlook of this name"),
        "{screen}"
    );
    assert!(screen.contains("administrator's, has an error"), "{screen}");
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
        screen.contains("File: none, because it is built into vtium"),
        "{screen}"
    );
    assert!(screen.contains("This vtlook has no filter"), "{screen}");
}

#[test]
fn a_color_map_s_detail_shows_its_marks_and_the_check_it_fails() {
    let mut a = effects();
    to(&mut a, "loose.cube");
    press(&mut a, Key::Enter);
    let screen = text(&a);
    assert!(screen.contains(" color map loose.cube"), "{screen}");
    assert!(screen.contains("State: not active"), "{screen}");
    assert!(
        screen.contains("This color map fails the trust rule"),
        "{screen}"
    );
    assert!(
        joined(&screen).contains("the file belongs to user 1000"),
        "{screen}"
    );
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
    )
    .expect("an activation");
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

/// The state of the color map `name` in the Effects tab's list.
fn mark(a: &App, name: &str) -> Option<State> {
    a.shared
        .inputs
        .effects
        .maps
        .iter()
        .find(|m| m.name() == name)
        .map(|m| m.state)
}

// The marks follow the draft after every desk answer that changes the
// draft or its batch: an install of a draft that activates a color map
// makes it active, and Revert, New, Delete and Restore each bring the
// marks of the draft they leave open.
#[test]
fn the_marks_follow_the_draft_after_install_revert_new_delete_and_restore() {
    use vtium_config::tui::desk::{Done, Opened};
    let warm = std::path::PathBuf::from("/etc/vtium/looks/warm.cube");
    let activated = || {
        let mut a = effects();
        let mut states = active();
        let op = Op::Resource {
            path: warm.clone(),
            change: Change::Activate,
        };
        let done = vtium_config::tui::fixture::resource_op(&mut states, &warm, Change::Activate)
            .expect("an activation");
        a.event(Event::Desked(op, Ok(done)));
        assert_eq!(mark(&a, "warm.cube"), Some(State::Activates));
        a
    };
    let opened = |resources: Vec<(std::path::PathBuf, State)>| Opened {
        name: "mine".into(),
        text: inputs().conf.expect("the fixture's vtium.conf"),
        resources,
        ..Opened::default()
    };
    // the install makes the draft's activation active
    let mut a = activated();
    let mut installed = active();
    installed.push((warm.clone(), State::Active));
    a.event(Event::Installed(Ok(Done::Draft(opened(installed), None))));
    assert_eq!(mark(&a, "warm.cube"), Some(State::Active));
    assert_eq!(mark(&a, "sepia.cube"), Some(State::Active));
    // each op that replaces the draft drops its activation
    for op in [Op::Revert, Op::New, Op::Delete, Op::Restore(1)] {
        let mut a = activated();
        a.event(Event::Desked(
            op.clone(),
            Ok(Done::Draft(opened(active()), None)),
        ));
        assert_eq!(mark(&a, "warm.cube"), Some(State::Inactive), "{op:?}");
        assert_eq!(mark(&a, "sepia.cube"), Some(State::Active), "{op:?}");
    }
}

// Space on a vtlook changes nothing, since a vtlook is not a resource the
// batch lists, and says why, since the key line offers Space there. Space
// on the filter line says where Space acts.
#[test]
fn space_on_a_vtlook_or_the_filter_line_changes_nothing_and_says_why() {
    for name in ["No filter", "amber", "sepia"] {
        let mut a = effects();
        to(&mut a, name);
        let draft = a.shared.conf_text().map(str::to_owned);
        assert!(press(&mut a, Key::Char(' ')).is_empty(), "{name}");
        assert_eq!(
            a.shared.message.as_deref(),
            Some(vtium_config::tui::words::SPACE_ON_VTLOOK),
            "{name}"
        );
        assert_eq!(a.shared.conf_text().map(str::to_owned), draft, "{name}");
        assert_eq!(nav::node(&a), "effects", "{name}");
    }
    let mut a = effects();
    press(&mut a, Key::Home);
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert_eq!(
        a.shared.message.as_deref(),
        Some(vtium_config::tui::words::SPACE_ON_EFFECT_FILTER)
    );
}

// An administrator's vtlook that takes a preset's name hides the preset,
// so the looks that name it use the files the administrator's vtlook
// names, not the preset's.
#[test]
fn the_administrator_s_vtlook_decides_which_files_a_look_uses() {
    use vtium_config::tui::effects_found::{From, Vtlook};
    let mut found = inputs().effects;
    let vtlook = |from, hidden, file: &str| Vtlook {
        name: "glow".into(),
        file: None,
        from,
        hidden,
        read: Ok(Vec::new()),
        files: vec![file.into()],
    };
    found
        .vtlooks
        .push(vtlook(From::Preset, true, "/etc/vtium/looks/warm.cube"));
    found.vtlooks.push(vtlook(
        From::Administrator,
        false,
        "/etc/vtium/looks/sepia.cube",
    ));
    let conf = "version = 1\n[look desk]\nvtlook = glow\n";
    let users = |file: &str| found.uses(conf, std::path::Path::new(file));
    assert!(users("/etc/vtium/looks/warm.cube").is_empty());
    assert_eq!(
        users("/etc/vtium/looks/sepia.cube"),
        [vtium_config::batch::Use {
            look: Some("desk".into()),
            through: vtium_config::batch::Through::Vtlook("glow".into()),
        }]
    );
}

#[test]
fn space_asks_before_it_deactivates_a_color_map_that_a_look_uses() {
    let mut a = effects();
    to(&mut a, "sepia.cube");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert_eq!(nav::node(&a), "any.question.deactivate");
    let screen = joined(&text(&a));
    assert!(
        screen.contains("Look proof uses it through the vtlook sepia. From the next login after the install, look proof renders with no filter."),
        "{screen}"
    );
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
    pictured_from(inputs(), ops, cols, rows, keys)
}

/// The screen and the console of [`pictured`], from `inputs`.
fn pictured_from(
    inputs: vtium_config::tui::model::Inputs,
    ops: bool,
    cols: usize,
    rows: usize,
    keys: &[&[u8]],
) -> (String, Console) {
    let grid = (cols, rows);
    let mut c = Console::vtium(cols, rows, ops, "unifont")
        .takes("proof", grid)
        .takes("unifont", grid)
        .keys(keys);
    c.graphics = true;
    drive(&mut c, inputs).expect("the loop");
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
    let because = |place: usize, want: &str| {
        let (screen, c) = pictured(false, 100, 30, &to_item(place));
        assert_eq!(c.shots.last().expect("a picture").filter, None, "{screen}");
        assert!(joined(&screen).contains(want), "{want}\n{screen}");
    };
    because(
        4,
        "The picture shows the sample screen without this vtlook. The vtlook has an error, so vtium renders the built-in vtlook in its place. Line 2: there is no color filter named glow.",
    );
    because(
        7,
        "The picture shows the sample screen without this color map. The color map is not active, so vtium treats it as missing.",
    );
    because(
        8,
        "The picture shows the sample screen without this color map. The color map fails the trust rule, so vtium treats it as missing.",
    );
    because(
        9,
        "The picture shows the sample screen without this color map. The color map is in your own folder, so vtium cannot use it.",
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
        "/etc/vtium/looks/warm.cube:5: color filter 1 of 1, cube: checking: a bad line.".into(),
    ));
    assert!(
        shown.contains("without this vtlook, because vtium cannot read it: /etc/vtium/looks/warm.cube:5: color filter 1 of 1, cube: checking: a bad line. vtium renders the built-in vtlook in its place."),
        "{shown}"
    );
    let shown = answer(Outcome::ColorOnly);
    assert!(
        shown.contains("through the vtlook amber. The picture leaves out the vtlook's spatial filters, which vtium draws only on the GPU."),
        "{shown}"
    );
}

/// The app on the Effects tab at 100×30 with `name` highlighted, in a
/// terminal that shows pictures, its picture drawn with `outcome`.
fn answered(name: &str, outcome: vtium_config::specimen::Outcome) -> App {
    use vtium_config::specimen::Picture;
    let mut a = app(100, 30);
    press(&mut a, Key::Char('2'));
    to(&mut a, name);
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
    a.event(Event::Celled {
        font,
        height,
        cell: Ok((cell, 16)),
    });
    a.event(Event::Filtered {
        picture: Picture {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
        },
        outcome,
    });
    a
}

// The line gives an effect's outcome only beside the preview it answers:
// once the highlight moves on, the next item's line says nothing of the
// last one's outcome while its own picture is on its way, and a picture
// that comes for an item no longer highlighted leaves no outcome behind.
#[test]
fn the_line_never_gives_another_item_s_outcome() {
    use vtium_config::specimen::{Outcome, Picture};
    let mut a = answered("sepia", Outcome::ColorOnly);
    assert!(
        joined(&text(&a)).contains("through the vtlook sepia. The picture leaves out"),
        "{}",
        text(&a)
    );
    to(&mut a, "amber");
    let shown = joined(&text(&a));
    assert!(
        shown.contains("The picture shows the sample screen through the vtlook amber."),
        "{shown}"
    );
    assert!(!shown.contains("leaves out"), "{shown}");
    // a picture of sepia that comes while amber is highlighted keeps no
    // outcome for amber
    a.event(Event::Filtered {
        picture: Picture {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
        },
        outcome: Outcome::Failed("/etc/vtium/looks/sepia.cube:1: a bad line".into()),
    });
    let shown = joined(&text(&a));
    assert!(!shown.contains("cannot read it"), "{shown}");
}

// In a vtium console that draws a vtlook of its own, the line adds that
// the console draws the picture through that vtlook as well; elsewhere,
// and in a console that draws No filter, it adds nothing.
#[test]
fn the_line_names_the_console_s_own_vtlook() {
    use vtium_config::specimen::Outcome;
    let own = "This console draws the picture through its own vtlook green as well.";
    let mut a = answered("amber", Outcome::Applied);
    assert!(!joined(&text(&a)).contains("its own vtlook"));
    let conf = a
        .shared
        .inputs
        .conf
        .clone()
        .expect("the installed vtium.conf");
    for (live, conf, said) in [
        (
            vtium_config::tui::model::Live::On,
            format!("vtlook = green\n{conf}"),
            true,
        ),
        (
            vtium_config::tui::model::Live::OpsOff,
            format!("vtlook = green\n{conf}"),
            true,
        ),
        (
            vtium_config::tui::model::Live::On,
            format!("vtlook = default\n{conf}"),
            false,
        ),
        (
            vtium_config::tui::model::Live::NotVtium,
            format!("vtlook = green\n{conf}"),
            false,
        ),
    ] {
        let named = format!("{live:?}");
        a.shared.live = live;
        a.shared.inputs.conf = Some(conf);
        let shown = joined(&text(&a));
        assert_eq!(shown.contains(own), said, "{named}: {shown}");
    }
}

// The Effects tab with the preset amber highlighted, in a terminal that
// shows pictures: the window of the list, the sentences under it, and the
// line beside the picture. The window keeps the picture's rows at 80×25
// as well.
#[test]
fn the_effects_tab_previews_a_preset_at_80x25_and_100x30() {
    for (cols, rows) in [(80, 25), (100, 30)] {
        let (screen, c) = pictured(false, cols, rows, &to_item(2));
        assert!(
            joined(&screen)
                .contains("The picture shows the sample screen through the vtlook amber."),
            "{screen}"
        );
        assert!(!c.shots.is_empty(), "{screen}");
        let bytes = c.written.get(..c.seen_at).unwrap_or_default().to_vec();
        golden(&format!("effects-amber-{cols}x{rows}"), &screen, &bytes);
    }
}

// The real desk keeps an activation beside the draft, answers with the
// draft's states, carries them in each draft it opens, takes the change
// back on a deactivation, and refuses what the batch refuses: a
// deactivation of a resource that is not active, and an activation of a
// file that fails the trust rule.
#[test]
fn the_real_desk_keeps_a_color_map_s_change_beside_the_draft() {
    use std::os::unix::fs::PermissionsExt;
    use vtium_config::tui::desk::{Desk, Done, Store};
    // a folder that nobody else can write, as the trust rule asks
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("vtium-config-real-desk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["etc/fonts", "etc/looks", "home", "sys"] {
        std::fs::create_dir_all(root.join(dir)).expect("the fixture");
    }
    let root = root.canonicalize().expect("the fixture");
    std::fs::write(
        root.join("etc/vtium.conf"),
        vtium_config::tui::fixture::DRAFT,
    )
    .expect("vtium.conf");
    script::executable(&root.join("vtium"), vtium_config::tui::fixture::CHECKER);
    let cube = root.join("etc/looks/warm.cube");
    std::fs::write(&cube, "LUT_3D_SIZE 2\n").expect("the color map");
    let home = root.clone();
    let store = vtium_config::store::Store::locate(
        1000,
        move |name: &str| match name {
            "HOME" => Some(home.join("home").into()),
            "XDG_CONFIG_HOME" => Some(home.join("home/config").into()),
            "XDG_STATE_HOME" => Some(home.join("home/state").into()),
            _ => None,
        },
        |_| None,
    )
    .expect("the store");
    let paths = vtium_config::tui::Paths {
        configdir: root.join("etc"),
        frits: Ok(root.join("etc/fonts")),
        drafts: root.join("home/bundles"),
        sudo: "sudo".into(),
        vtium: root.join("vtium").into(),
        helper: vtium_config::specimen::Helper {
            program: "/bin/false".into(),
            options: Vec::new(),
        },
        sys: root.join("sys"),
    };
    vtium_config::drafts::Session {
        store: store.clone(),
        paths: &paths,
    }
    .make("bench", vtium_config::drafts::From::Installed)
    .expect("the draft bench");
    let mut desk = Store::new(store.clone(), paths, Box::new(Default::default));
    let op = |change| Op::Resource {
        path: cube.clone(),
        change,
    };
    let held = |states: &[(std::path::PathBuf, State)]| {
        states.iter().find(|(p, _)| *p == cube).map(|(_, s)| *s)
    };
    match desk.run(&op(Change::Activate)) {
        Ok(Done::Resources {
            states,
            said,
            changes,
        }) => {
            assert_eq!(held(&states), Some(State::Activates));
            assert!(
                said.starts_with("The draft activates this color map."),
                "{said}"
            );
            assert_eq!(changes, 1);
        }
        other => panic!("{other:?}"),
    }
    // the change is kept beside the draft, and the next draft the desk
    // opens carries it
    let kept = store.batch_changes("bench").expect("the draft's changes");
    assert_eq!(kept.len(), 1, "{kept:?}");
    match desk.run(&Op::Open) {
        Ok(Done::Draft(opened, _)) => {
            assert_eq!(held(&opened.resources), Some(State::Activates));
        }
        other => panic!("{other:?}"),
    }
    // a deactivation takes the pending activation back
    match desk.run(&op(Change::Deactivate)) {
        Ok(Done::Resources {
            states, changes, ..
        }) => {
            assert_eq!(held(&states), None);
            assert_eq!(changes, 0);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        store
            .batch_changes("bench")
            .expect("the draft's changes")
            .is_empty()
    );
    // the batch refuses a deactivation of a file that is not active
    let refused = desk.run(&op(Change::Deactivate)).expect_err("a refusal");
    assert!(refused.contains("is not active"), "{refused}");
    // and an activation of a file that others can write
    std::fs::set_permissions(&cube, std::fs::Permissions::from_mode(0o666)).expect("the mode");
    let refused = desk.run(&op(Change::Activate)).expect_err("a refusal");
    assert!(refused.contains(&cube.display().to_string()), "{refused}");
    assert!(
        store
            .batch_changes("bench")
            .expect("the draft's changes")
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(&root);
}

// A color map's file name is the administrator's text, so the list and
// the detail show it with a direction override replaced.
#[test]
fn a_color_map_s_name_shows_no_direction_override() {
    let mut a = effects();
    let mut map = a.shared.inputs.effects.maps[0].clone();
    map.path = map.path.with_file_name("evil\u{202e}txt.cube");
    a.shared.inputs.effects.maps.insert(0, map);
    to(&mut a, "evil\u{202e}txt.cube");
    assert!(!text(&a).contains('\u{202e}'), "{}", text(&a));
    press(&mut a, Key::Enter);
    assert_eq!(nav::node(&a), "effects.effect");
    assert!(!text(&a).contains('\u{202e}'), "{}", text(&a));
}

// A long list of effects keeps the preview's rows: the list shows a
// window of the effects around the highlight, and a sentence under the
// list says which effects the window shows, so the person knows that the
// list goes on. With 16 more color maps, in a terminal that shows
// pictures at 80×25 and at 100×30, the picture still comes, and End
// moves the window to the last color map.
#[test]
fn a_long_list_shows_a_window_and_the_preview_keeps_its_rows() {
    const END: &[u8] = b"\x1b[F";
    let mut many = inputs();
    let warm = many.effects.maps[1].clone();
    for n in 1..=16 {
        let mut map = warm.clone();
        map.path = map.path.with_file_name(format!("extra-{n}.cube"));
        many.effects.maps.push(map);
    }
    let vtlooks = many.effects.vtlooks.len();
    let all = vtlooks + many.effects.maps.len() + many.effects.shaders.len();
    for (cols, rows) in [(80, 25), (100, 30)] {
        let (screen, c) = pictured_from(many.clone(), false, cols, rows, &to_item(2));
        assert!(
            !c.shots.is_empty(),
            "no picture at {cols}x{rows}:\n{screen}"
        );
        assert!(
            joined(&screen).contains("The list shows effects 1 to "),
            "{screen}"
        );
        assert!(joined(&screen).contains(&format!(" of {all}.")), "{screen}");
        assert!(!screen.contains("extra-16.cube"), "{screen}");
        let (screen, c) = pictured_from(many.clone(), false, cols, rows, &[TO_EFFECTS, END]);
        assert!(
            !c.shots.is_empty(),
            "no picture at {cols}x{rows}:\n{screen}"
        );
        assert!(screen.contains("extra-16.cube"), "{screen}");
        assert!(
            joined(&screen).contains(&format!(" to {all} of {all}.")),
            "{screen}"
        );
        // the window starts the group it shows with the group's heading,
        // though the group's first color map is out of sight
        assert!(screen.contains("Color maps"), "{screen}");
        assert!(!screen.contains("sepia.cube"), "{screen}");
    }
}
