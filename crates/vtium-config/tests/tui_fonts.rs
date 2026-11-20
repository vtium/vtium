//! The Fonts tab driven headlessly through the TUI's real loop: the font
//! files found, grouped by folder with their marks; the descriptions the
//! font helper sends while the TUI reads keys; the filters; Space through
//! the controls the resource tabs share; a font's detail; the install of
//! a font of the person's as a system font; and the preview. The fixture
//! lists five fonts under the filter line: DejaVuSansMono.ttf, active;
//! ter-u16n.otb, not active; NotoColorEmoji.ttf, which fails the trust
//! rule and repeats a package's file; vtium's cullet-mono.otf, active; and
//! Mine-Regular.ttf in the person's own folder. So Down once reaches the
//! first font and Down five times the person's own. `tui.rs` says how the
//! tests drive the TUI, and `tui_common/` holds the helpers.

mod tui_common;
use tui_common::*;

use vtium_config::tui::fixture::{OWN_FONT, OWN_FONT_COMMAND, OWN_FONT_INSTALLED};
use vtium_config::tui::input::{Input, Key};
use vtium_config::tui::model::{App, Event, View};
use vtium_config::tui::tabs::Tab;
use vtium_config::tui::words;

const ESC: &[u8] = b"\x1b";

/// The keys that highlight the `n`th font from the filter line, then
/// `more`.
fn on_font(n: usize, more: &[&'static [u8]]) -> Vec<&'static [u8]> {
    let mut keys: Vec<&'static [u8]> = std::iter::repeat_n(DOWN, n).collect();
    keys.extend_from_slice(more);
    keys
}

/// The screen's text as one line, its words joined by single spaces.
fn flat(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The TUI at 100×30 on the fixture, the font helper's descriptions in,
/// after `keys`.
fn app_after(keys: &[Key]) -> App {
    let (mut app, _) = App::new(inputs(), 100, 30);
    let answers = app
        .shared
        .inputs
        .font_files
        .waiting()
        .into_iter()
        .map(|p| {
            let d = vtium_config::tui::fixture::described(&p);
            (p, d)
        })
        .collect();
    app.event(Event::Described(answers));
    for key in keys {
        app.event(Event::Input(Input::Key(*key)));
    }
    app
}

/// The Fonts tab lists the font files under their folders' headings, each
/// with its family once described, whether it is active, its trust mark,
/// and a duplicate's mark; a font in the person's own folder is not
/// usable. The filter line comes first and shows every font.
///
/// References:
/// - ADR-1205 D3: the folders, and a user's font listed as not usable.
/// - ADR-1403 D10: the filter line, then the fonts found.
#[test]
fn the_fonts_tab_lists_the_fonts_found_by_folder_with_their_marks() {
    let screen = reach(100, 30, &[]);
    let rows: Vec<&str> = screen.lines().skip(3).take(12).map(str::trim_end).collect();
    let rows = rows.join("\n");
    for want in [
        " Show: All fonts",
        " System fonts",
        "   DejaVuSansMono.ttf  DejaVu Sans Mono Regular, active, trusted",
        "   ter-u16n.otb        Terminus Regular, not active, trusted",
        "   NotoColorEmoji.ttf  Noto Color Emoji Regular, not active, fails the trust rule, duplicate",
        " vtium's own fonts",
        "   cullet-mono.otf     Cullet Mono Regular, active, trusted",
        "   Mine-Regular.ttf    Mine Regular, not usable",
    ] {
        assert!(rows.contains(want), "{want:?}:\n{screen}");
    }
    assert!(
        flat(&screen).contains(
            "Your own fonts, which vtium cannot use until you install them as system fonts"
        ),
        "{screen}"
    );
}

/// The font helper describes the files in the background: the TUI asks
/// for every file at start, each font shows "describing…" until its
/// answer comes, the sentence under the list counts the fonts still to
/// come, and keys reach the TUI meanwhile.
///
/// References:
/// - ADR-1205 D3: vtium-config reads a font only through the sandboxed
///   helper.
#[test]
fn each_font_shows_describing_until_the_helper_answers_and_keys_go_on() {
    let mut c = console().keys(&on_font(1, &[b"\r"]));
    c.hold_descriptions = true;
    run(&mut c);
    let paths: Vec<PathBuf> = inputs()
        .font_files
        .files
        .iter()
        .map(|f| f.path().to_path_buf())
        .collect();
    assert_eq!(c.described_asked, [paths]);
    // Enter reached the TUI and opened the first font's detail
    let screen = c.seen.clone().expect("the screen");
    assert!(
        flat(&screen).contains("font DejaVuSansMono.ttf"),
        "{screen}"
    );
    assert!(flat(&screen).contains(words::FONT_DESCRIBING), "{screen}");
    let screen = {
        let mut c = console();
        c.hold_descriptions = true;
        run(&mut c);
        c.seen.clone().expect("the screen")
    };
    assert!(
        screen.contains("DejaVuSansMono.ttf  describing…, active"),
        "{screen}"
    );
    assert!(flat(&screen).contains(&words::fonts_waiting(5)), "{screen}");
}

/// Enter on the filter line opens the filters as the shared list; Enter
/// picks one, and the list then shows the fonts that pass it. A filter
/// other than All fonts leaves out a font not described yet.
///
/// References:
/// - ADR-1403 D10: the filter line opens the shared list.
#[test]
fn a_filter_shows_the_fonts_that_pass_it() {
    use vtium_config::tui::fonts_found::Filter;
    let names = |app: &App| -> Vec<String> {
        app.fonts
            .items(&app.shared)
            .iter()
            .map(|i| app.shared.inputs.font_files.files[*i].resource.name())
            .collect()
    };
    let pick = |filter: Filter| -> Vec<Key> {
        let at = Filter::choices()
            .iter()
            .position(|f| *f == filter)
            .expect("a filter");
        let mut keys = vec![Key::Enter, Key::Home];
        keys.extend(std::iter::repeat_n(Key::Down, at));
        keys.push(Key::Enter);
        keys
    };
    let app = app_after(&[Key::Enter]);
    assert_eq!(app.fonts.node(), "fonts.filter");
    for (filter, want) in [
        (Filter::Bitmap, vec!["ter-u16n.otb"]),
        (
            Filter::Proportional,
            vec!["NotoColorEmoji.ttf", "Mine-Regular.ttf"],
        ),
        (
            Filter::Group(vtium_config::describe::Group::ColorEmoji),
            vec!["NotoColorEmoji.ttf"],
        ),
        (
            Filter::Script("Greek"),
            vec!["DejaVuSansMono.ttf", "cullet-mono.otf"],
        ),
    ] {
        let app = app_after(&pick(filter));
        assert_eq!(app.fonts.node(), "fonts");
        assert_eq!(app.fonts.filter(), filter);
        assert_eq!(names(&app), want, "{filter:?}");
    }
    // a font not described yet passes All fonts alone
    let (mut app, _) = App::new(inputs(), 100, 30);
    for key in pick(Filter::Outline) {
        app.event(Event::Input(Input::Key(key)));
    }
    assert!(names(&app).is_empty());
    let screen = vtium_config::tui::view::screen_at(&app, app.size)
        .iter()
        .map(|r| r.iter().map(|s| s.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(flat(&screen).contains(words::FONTS_NONE), "{screen}");
}

/// The highlight stays on its font while descriptions arrive: under a
/// filter, a font described later can join the list above the highlighted
/// one, and the highlight follows its font rather than its row, so Space
/// acts on the font the person chose.
#[test]
fn the_highlight_stays_on_its_font_as_descriptions_arrive() {
    use vtium_config::tui::fixture::described;
    use vtium_config::tui::fonts_found::Filter;
    let (mut app, _) = App::new(inputs(), 100, 30);
    let path = |name: &str| {
        app.shared
            .inputs
            .font_files
            .files
            .iter()
            .map(|f| f.path().to_path_buf())
            .find(|p| p.ends_with(name))
            .expect("the fixture's font")
    };
    let (dejavu, cullet) = (path("DejaVuSansMono.ttf"), path("cullet-mono.otf"));
    // only cullet-mono.otf is described, so only it is an outline font yet
    app.event(Event::Described(vec![(cullet.clone(), described(&cullet))]));
    let at = Filter::choices()
        .iter()
        .position(|f| *f == Filter::Outline)
        .expect("a filter");
    let mut keys = vec![Key::Enter, Key::Home];
    keys.extend(std::iter::repeat_n(Key::Down, at));
    keys.extend([Key::Enter, Key::Down]);
    for key in keys {
        app.event(Event::Input(Input::Key(key)));
    }
    let highlighted = |app: &App| {
        app.fonts
            .highlighted(&app.shared)
            .map(|f| f.path().to_path_buf())
    };
    assert_eq!(highlighted(&app).as_ref(), Some(&cullet));
    // DejaVu Sans Mono, earlier in the list, is described as an outline
    // font and joins the list above it
    app.event(Event::Described(vec![(dejavu.clone(), described(&dejavu))]));
    assert_eq!(app.fonts.items(&app.shared).len(), 2);
    assert_eq!(highlighted(&app).as_ref(), Some(&cullet));
}

/// Space activates a font that is not active and deactivates one that is,
/// through the desk, and the marks follow the draft.
///
/// References:
/// - ADR-1205 D1, D2: the shared controls, Activate and Deactivate.
#[test]
fn space_activates_or_deactivates_the_highlighted_font() {
    let mut c = console().keys(&on_font(2, &[b" "]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let terminus = PathBuf::from("/usr/share/fonts/misc/ter-u16n.otb");
    assert!(
        d.resources
            .contains(&(terminus, vtium_config::batch::State::Activates)),
        "{:?}",
        d.resources
    );
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("ter-u16n.otb        Terminus Regular, activates on install, trusted"),
        "{screen}"
    );
    let mut c = console().keys(&on_font(1, &[b" "]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let dejavu = PathBuf::from("/usr/share/fonts/TTF/DejaVuSansMono.ttf");
    assert!(
        d.resources
            .contains(&(dejavu, vtium_config::batch::State::Deactivates)),
        "{:?}",
        d.resources
    );
}

/// Space on a font in the person's own folder activates nothing and says
/// how the font becomes usable; on a font that fails the trust rule it
/// says why and activates nothing.
///
/// References:
/// - ADR-1205 D5: activating never installs, and a user's font is first
///   installed as a system font.
#[test]
fn space_on_a_font_of_yours_or_an_untrusted_one_changes_nothing_and_says_why() {
    let mut c = console().keys(&on_font(5, &[b" "]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert_eq!(d.resources, active(), "{:?}", d.resources);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        flat(&screen).contains(&words::own_font(OWN_FONT)),
        "{screen}"
    );
    let mut c = console().keys(&on_font(3, &[b" "]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert_eq!(d.resources, active(), "{:?}", d.resources);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        flat(&screen).contains("the file belongs to user 1000"),
        "{screen}"
    );
}

/// Space on an active font that a look's font set names asks first, and
/// the question names the look; Esc deactivates nothing and y does.
///
/// References:
/// - ADR-1205 D8: deactivating warns and lists the looks that use it.
#[test]
fn deactivating_a_font_a_look_uses_asks_first_and_names_the_look() {
    let frits = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tui-fonts-uses");
    std::fs::create_dir_all(&frits).expect("the frits' folder");
    std::fs::write(
        frits.join("proof.vtfrit"),
        "source=outline:/usr/share/fonts/TTF/DejaVuSansMono.ttf\n",
    )
    .expect("a frit");
    let mut i = inputs();
    i.frits = frits;
    let (mut app, _) = App::new(i, 100, 30);
    for key in [Key::Down, Key::Char(' ')] {
        app.event(Event::Input(Input::Key(key)));
    }
    let question = app.nav.confirm.as_ref().expect("a question");
    assert!(
        question
            .question
            .contains("Look proof uses it through the font set proof"),
        "{}",
        question.question
    );
    assert_eq!(
        vtium_config::tui::nav::node(&app),
        "any.question.deactivate"
    );
}

/// Enter on a font opens its detail: its file and folder, each face with
/// what it covers and its sizes, whether it is active, its trust mark,
/// a duplicate's mark and the looks that use it. Esc goes back to the
/// same font.
///
/// References:
/// - ADR-1205 D8: a font's view shows what it covers and its sizes.
/// - ADR-1403 D10: the detail is read-only.
#[test]
fn enter_opens_a_font_s_detail_and_esc_goes_back() {
    let screen = reach(100, 30, &on_font(3, &[b"\r"]));
    let text = flat(&screen);
    for line in [
        "font NotoColorEmoji.ttf",
        "File: /usr/local/share/fonts/NotoColorEmoji.ttf",
        "Folder: system",
        "Face: Noto Color Emoji Regular, version 2.37",
        "OpenType, outline, proportional",
        "Covers 3000 characters, and no script in full.",
        "Belongs to: color emoji.",
        "Sizes: any size, since it has outlines.",
        "State: not active",
        "This font fails the trust rule",
        "Duplicate: a package installed the same file as /usr/share/fonts/noto/NotoColorEmoji.ttf.",
        "No look uses it.",
    ] {
        assert!(text.contains(line), "{line:?}:\n{screen}");
    }
    let terminus = flat(&reach(100, 30, &on_font(2, &[b"\r"])));
    assert!(
        terminus.contains("OpenType, bitmap, monospaced"),
        "{terminus}"
    );
    assert!(terminus.contains("Sizes: 8×16."), "{terminus}");
    let app = app_after(&[Key::Down, Key::Down, Key::Enter, Key::Escape]);
    assert_eq!((app.fonts.node(), app.fonts.at()), ("fonts", 2));
}

/// Enter in the detail of a font of the person's asks whether to install
/// it as a system font, and the question shows the exact command first.
/// y runs that command through the TUI's sudo runner, and the new system
/// font then joins the list, not active. Esc and every other key run
/// nothing.
///
/// References:
/// - ADR-1205 D5: the exact command, confirmed each time, and activating
///   never installs.
/// - ADR-1403 D10: Enter in a font's detail asks any.question.system-font.
#[test]
fn enter_in_the_detail_of_a_font_of_yours_asks_then_installs_it_as_a_system_font() {
    let ask = on_font(5, &[b"\r", b"\r"]);
    let screen = reach_desk(&ask);
    let text = flat(&screen);
    for line in [
        flat(&words::system_font_command(OWN_FONT, OWN_FONT_INSTALLED)),
        OWN_FONT_COMMAND.to_owned(),
        flat(&words::system_font_question(OWN_FONT_INSTALLED)),
    ] {
        assert!(text.contains(&line), "{line:?}:\n{screen}");
    }
    for answer in [ESC, b"x".as_slice(), b"n"] {
        let mut keys = ask.clone();
        keys.push(answer);
        let mut c = console().keys(&keys);
        let mut d = desk();
        run_desk(&mut c, &mut d);
        assert!(c.saves.is_empty(), "{answer:?}: {:?}", c.saves);
        assert!(d.fonts_installed.is_empty());
    }
    let mut keys = ask.clone();
    keys.push(b"y");
    let mut c = console().keys(&keys);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert_eq!(
        c.saves,
        [["sudo", "vtium-config", "apply", "FONT-DRAFT"]
            .map(std::ffi::OsString::from)
            .to_vec()]
    );
    assert_eq!(d.fonts_installed, [PathBuf::from(OWN_FONT_INSTALLED)]);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        flat(&screen).contains(&flat(&words::system_font_installed(OWN_FONT_INSTALLED))),
        "{screen}"
    );
    // the new system font is listed among the system's, not active
    let mut keys = ask;
    keys.extend([b"y".as_slice(), ESC]);
    let screen = reach_desk(&keys);
    assert!(
        screen.contains("Mine-Regular.ttf    Mine Regular, not active, trusted"),
        "{screen}"
    );
}

/// A different file of the same name is installed already: the detail
/// shows both files' facts and the command while the question asks
/// whether to replace the installed file, and only `y` installs.
///
/// References:
/// - ADR-1205 D5, its note of 2026-11-18: both files on a clash.
#[test]
fn enter_on_a_font_that_replaces_an_installed_file_shows_both_files() {
    use vtium_config::tui::fixture::{OWN_FONT_FACTS, OWN_FONT_INSTALLED_FACTS};
    let ask = on_font(5, &[b"\r", b"\r"]);
    let replacing = || Desk {
        placement: vtium_config::apply::Placement::Replaces,
        ..desk()
    };
    let mut c = console().keys(&ask);
    let mut d = replacing();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let text = flat(&screen);
    for line in [
        words::SYSTEM_FONT_CLASH.to_owned(),
        flat(&words::system_font_installed_file(OWN_FONT_INSTALLED_FACTS)),
        flat(&words::system_font_yours(OWN_FONT_FACTS)),
        OWN_FONT_COMMAND.to_owned(),
        flat(&words::system_font_replace_question(OWN_FONT_INSTALLED)),
    ] {
        assert!(text.contains(&line), "{line:?}:\n{screen}");
    }
    // a detail scrolled down goes back to its first line, so the files
    // and the command show while the question is open
    let scrolled = on_font(5, &[b"\r", DOWN, DOWN, DOWN, DOWN, b"\r"]);
    let mut c = console().keys(&scrolled);
    let mut d = replacing();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    for line in [
        words::SYSTEM_FONT_CLASH.to_owned(),
        flat(&words::system_font_installed_file(OWN_FONT_INSTALLED_FACTS)),
        OWN_FONT_COMMAND.to_owned(),
    ] {
        assert!(flat(&screen).contains(&line), "{line:?}:\n{screen}");
    }
    let mut keys = ask.clone();
    keys.push(b"n");
    let mut c = console().keys(&keys);
    let mut d = replacing();
    run_desk(&mut c, &mut d);
    assert!(c.saves.is_empty(), "{:?}", c.saves);
    let mut keys = ask;
    keys.push(b"y");
    let mut c = console().keys(&keys);
    let mut d = replacing();
    run_desk(&mut c, &mut d);
    assert_eq!(d.fonts_installed, [PathBuf::from(OWN_FONT_INSTALLED)]);
}

/// The same file is installed already: the message line says nothing
/// changed, no question opens, and `y` installs nothing.
///
/// References:
/// - ADR-1205 D5, its note of 2026-11-18: nothing when the files are
///   identical.
#[test]
fn enter_on_a_font_installed_already_says_nothing_changed() {
    let mut c = console().keys(&on_font(5, &[b"\r", b"\r", b"y"]));
    let mut d = Desk {
        placement: vtium_config::apply::Placement::Identical,
        ..desk()
    };
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        flat(&screen).contains(&flat(&words::system_font_identical(OWN_FONT_INSTALLED))),
        "{screen}"
    );
    assert!(
        !flat(&screen).contains(&flat(&words::system_font_question(OWN_FONT_INSTALLED))),
        "{screen}"
    );
    assert!(c.saves.is_empty(), "{:?}", c.saves);
    assert!(d.fonts_installed.is_empty());
}

/// Enter in the detail of a system font installs nothing and says it is a
/// system font already.
#[test]
fn enter_in_the_detail_of_a_system_font_says_it_is_one_already() {
    let mut c = console().keys(&on_font(1, &[b"\r", b"\r"]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert!(c.saves.is_empty());
    let screen = c.seen.clone().expect("the screen");
    assert!(
        flat(&screen).contains(words::SYSTEM_FONT_NOT_YOURS),
        "{screen}"
    );
}

/// The preview shows the highlighted font's sample characters, and `p`
/// switches it to the picture of the console drawn in that font. A vtium
/// console cannot draw a single font file as its own text, so the font is
/// previewed as a picture there too. The filter line previews nothing.
///
/// References:
/// - ADR-1205 D3: vtium-config may preview a user's font, which the
///   helper parses in its sandbox.
/// - ADR-1403 D10: `p` switches the preview.
#[test]
fn the_preview_draws_the_highlighted_font_and_p_switches_it() {
    let mut c = Console::vtium(100, 40, false, "unifont").keys(&on_font(1, &[b"p", b"q"]));
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    let dejavu = "/usr/share/fonts/TTF/DejaVuSansMono.ttf";
    let specimens: Vec<&str> = c.specimens.iter().map(|(f, _, _)| f.as_str()).collect();
    assert_eq!(specimens, [dejavu]);
    let pictures: Vec<&str> = c.pictures.iter().map(|(f, _, _)| f.as_str()).collect();
    assert_eq!(pictures, [dejavu]);
    // in a vtium console the TUI can switch, the font file is still a
    // picture, and no switch goes
    let mut live = Console::vtium(100, 40, true, "unifont")
        .takes("unifont", (100, 40))
        .keys(&on_font(2, &[b"q"]));
    live.graphics = true;
    run(&mut live);
    let specimens: Vec<&str> = live.specimens.iter().map(|(f, _, _)| f.as_str()).collect();
    assert_eq!(
        specimens.last(),
        Some(&"/usr/share/fonts/misc/ter-u16n.otb")
    );
    assert_eq!(live.switches(), Vec::<String>::new());
}

/// The frit that draws a font file alone for the preview names the file
/// by its kind, and a file whose name holds a control character is not
/// previewed, since it would end the source line and start another.
#[test]
fn a_font_file_s_preview_frit_names_the_file_alone() {
    let frits = Path::new("/etc/vtium/fonts");
    for (file, source) in [
        ("/f/a.ttf", "source=outline:/f/a.ttf\n"),
        ("/f/b.bdf.gz", "source=bdf:/f/b.bdf.gz\n"),
        ("/f/c.psf.gz", "source=file:/f/c.psf.gz\n"),
        ("/f/d.hex", "source=hex:/f/d.hex\n"),
    ] {
        let frit = vtium_config::tui::frit_text(frits, file).expect("a frit");
        assert_eq!(frit.text, source);
    }
    let why = vtium_config::tui::frit_text(frits, "/f/x\nsource=kbd:y.ttf").expect_err("refused");
    assert!(why.contains("control character"), "{why}");
}

/// `t` in the Fonts tab tries nothing, since vtium can try a font set on
/// its console but not a single font file, and says where to try one;
/// `n` and `r` do nothing here.
#[test]
fn t_says_where_to_try_a_font_and_n_and_r_do_nothing() {
    let mut c = console().keys(&on_font(1, &[b"t"]));
    run(&mut c);
    assert_eq!(c.switches(), Vec::<String>::new());
    let screen = c.seen.clone().expect("the screen");
    assert!(flat(&screen).contains(words::TRY_A_FONT_SET), "{screen}");
    let before = app_after(&[Key::Down]);
    for key in ['n', 'r'] {
        let after = app_after(&[Key::Down, Key::Char(key)]);
        assert_eq!(after.nav.view, View::Fonts);
        assert_eq!(after.shared.pending, before.shared.pending);
        assert_eq!(after.shared.message, before.shared.message);
    }
}

/// The Fonts tab's key line fits on one row at 80 columns.
#[test]
fn the_fonts_key_line_is_one_row_at_80_columns() {
    let screen = reach(80, 25, &[]);
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 25);
    assert!(
        rows[24].starts_with(&format!(" {}", keys_of("fonts"))),
        "{screen}"
    );
    assert!(rows[23].starts_with("---"), "the rule just above: {screen}");
}

/// The Fonts tab's screens: the list, the list of filters, a font's
/// detail and the question before an install as a system font, at
/// 80×25 and at 100×30, as provisional goldens and `make tui-shots`
/// pictures.
#[test]
fn the_fonts_tab_s_screens() {
    for (name, keys) in [
        ("fonts-list", on_font(1, &[])),
        ("fonts-filter", vec![b"\r".as_slice()]),
        ("fonts-detail", on_font(3, &[b"\r"])),
        ("fonts-system-font", on_font(5, &[b"\r", b"\r"])),
    ] {
        for (cols, rows) in [(80, 25), (100, 30)] {
            let (screen, bytes) = shot(cols, rows, &keys);
            golden(&format!("{name}-{cols}x{rows}"), &screen, &bytes);
        }
    }
}

/// The screen after `keys` at `cols` by `rows`, in a vtium console with
/// `allow-font-ops` off and with the stand-in desk, from the Fonts tab,
/// and the bytes the TUI wrote to draw it.
fn shot(cols: usize, rows: usize, keys: &[&[u8]]) -> (String, Vec<u8>) {
    let mut c = Console::vtium(cols, rows, false, "unifont").keys(keys);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let bytes = c.written.get(..c.seen_at).unwrap_or_default().to_vec();
    (screen, bytes)
}

/// The screen after `keys` with the stand-in desk, from the Fonts tab.
fn reach_desk(keys: &[&[u8]]) -> String {
    let mut c = console().keys(keys);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    c.seen.clone().expect("the screen")
}
