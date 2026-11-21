//! The Flow tab driven headlessly: its document of paths, its layers, the
//! mouse, and the draft it edits through the real loop and the desk.
//! `tui.rs` says how the tests drive the TUI, and `tui_common/` holds the
//! helpers.

mod tui_common;
use tui_common::*;

mod flow_view {
    use super::*;
    use vtium_config::tui::flow::Item;
    use vtium_config::tui::input::{Button, Key, Mouse};
    use vtium_config::tui::model::{App, Effect, Event, MOUSE_OFF, MOUSE_ON, View};
    use vtium_config::tui::view;
    use vtium_config::tui::words;

    #[test]
    fn mouse_reporting_is_on_in_the_flow_view_alone() {
        let mut i = inputs();
        i.view = View::Flow;
        let (_, first) = App::new(i, 80, 25);
        assert_eq!(first[0], Effect::Write(model::probe()));
        assert!(writes(&first, MOUSE_ON), "{first:?}");
        let (_, first) = App::new(inputs(), 80, 25);
        assert!(!writes(&first, MOUSE_ON), "{first:?}");
        // leaving the TUI from the flow turns it off
        let mut a = app(80, 25);
        let out = press(&mut a, Key::Char('q'));
        assert!(
            out.iter()
                .any(|e| matches!(e, Effect::Write(b) if b.ends_with(MOUSE_OFF))),
            "{out:?}"
        );
        assert_eq!(out.last(), Some(&Effect::Exit(0)));
    }

    /// Types `bytes` into `app` through the TUI's own key decoder, as the
    /// loop does, and returns what the app asked the loop to do.
    fn type_in(app: &mut App, bytes: &[u8]) -> Vec<Effect> {
        let mut decoder = vtium_config::tui::input::Decoder::keys();
        decoder
            .feed(bytes)
            .into_iter()
            .flat_map(|input| app.event(Event::Input(input)))
            .collect()
    }

    const TAB: &[u8] = b"\t";
    const RIGHT: &[u8] = b"\x1b[C";

    // Tab and Shift-Tab step the views wherever the selection is, and the
    // arrows move inside a view (#355)
    #[test]
    fn tab_and_shift_tab_step_the_views_from_every_box_of_the_flow() {
        for boxes in 0..6 {
            for (key, next) in [(TAB, View::Settings), (BACKTAB, View::Looks)] {
                let mut a = app(80, 25);
                for _ in 0..boxes {
                    type_in(&mut a, RIGHT);
                }
                let at = a.flow.state().at.clone();
                assert!(at.is_some());
                let out = type_in(&mut a, key);
                assert_eq!(a.nav.view, next, "{key:?} from box {boxes}");
                assert!(writes(&out, MOUSE_OFF), "{out:?}");
                // and back: the flow keeps the box that was highlighted
                let back = if key == TAB { BACKTAB } else { TAB };
                let out = type_in(&mut a, back);
                assert_eq!(
                    (a.nav.view, &a.flow.state().at),
                    (View::Flow, &at),
                    "box {boxes}"
                );
                assert!(writes(&out, MOUSE_ON), "{out:?}");
            }
        }
    }

    #[test]
    fn the_arrows_and_the_keys_that_move_in_a_column_never_leave_the_flow() {
        let mut a = app(80, 25);
        let keys: [&[u8]; 8] = [
            b"\x1b[D", b"\x1b[C", b"\x1b[A", b"\x1b[B", b"\x1b[5~", b"\x1b[6~", b"\x1b[H",
            b"\x1b[F",
        ];
        for key in keys {
            for _ in 0..8 {
                type_in(&mut a, key);
                assert_eq!(a.nav.view, View::Flow, "{key:?}");
            }
        }
    }

    /// The flow with a draft, so a card offers fields to change.
    fn drafted(cols: usize, rows: usize) -> App {
        let mut a = app(cols, rows);
        a.shared.draft = Some(vtium_config::tui::desk::Opened {
            name: "mine".into(),
            text: inputs().conf.expect("the fixture's vtium.conf"),
            ..Default::default()
        });
        a
    }

    #[test]
    fn tab_and_shift_tab_leave_nothing_open_behind() {
        // a box's detail, then the file view: Tab and Shift-Tab do nothing
        // while each is open (ADR-1403 D7, The keys)
        let mut a = drafted(80, 25);
        type_in(&mut a, b"\r");
        let layer = a.flow.state().layer.clone();
        assert!(layer.is_some(), "{}", text(&a));
        for key in [TAB, BACKTAB] {
            type_in(&mut a, key);
            assert_eq!((a.nav.view, &a.flow.state().layer), (View::Flow, &layer));
        }
        type_in(&mut a, b"\x1b");
        assert!(a.flow.state().layer.is_none());
        type_in(&mut a, b"v");
        assert!(a.flow.pager().is_some());
        for key in [TAB, BACKTAB] {
            type_in(&mut a, key);
            assert_eq!((a.nav.view, a.flow.pager().is_some()), (View::Flow, true));
        }
    }

    #[test]
    fn a_click_on_a_view_s_name_leaves_nothing_open_behind() {
        let mut a = drafted(80, 25);
        let looks = cell(&a, "Looks");
        type_in(&mut a, b"v");
        click(&mut a, looks);
        assert_eq!((a.nav.view, a.flow.pager().is_some()), (View::Flow, true));
        let mut a = drafted(80, 25);
        type_in(&mut a, b"\r");
        assert!(a.flow.state().layer.is_some());
        click(&mut a, looks);
        assert_eq!(
            (a.nav.view, a.flow.state().layer.is_some()),
            (View::Flow, true)
        );
        // with nothing open, the click switches
        let mut a = drafted(80, 25);
        click(&mut a, looks);
        assert_eq!(a.nav.view, View::Looks);
    }

    /// The cell where `needle` starts on the screen of `app`.
    fn cell(app: &App, needle: &str) -> (usize, usize) {
        text(app)
            .lines()
            .enumerate()
            .find_map(|(y, l)| {
                l.find(needle)
                    .map(|b| (l.get(..b).unwrap_or_default().chars().count(), y))
            })
            .unwrap_or_else(|| panic!("{needle} on the screen:\n{}", text(app)))
    }

    fn click(app: &mut App, (col, row): (usize, usize)) -> Vec<Effect> {
        press(
            app,
            Key::Mouse(Mouse {
                button: Button::Press(0),
                col,
                row,
            }),
        )
    }

    #[test]
    fn a_click_reaches_the_tui_through_its_decoder() {
        // SGR mouse, mode 1006: a press on DP-8's box, row 5 column 39
        // counted from 1, then Enter opens it
        let mut c = console().keys(&[b"\x1b[<0;39;5M", b"\x1b[<0;39;5m", b"\r"]);
        let mut i = inputs();
        i.view = View::Flow;
        drive(&mut c, i).expect("the loop");
        let written = String::from_utf8_lossy(&c.written).into_owned();
        assert!(written.contains("\x1b[?1000h\x1b[?1006h"), "turned on");
        let screen = c.seen.clone().expect("the screen");
        // the second press on the highlighted box opens its detail
        assert!(screen.contains("DISPLAY DP-8"), "{screen}");
    }

    #[test]
    fn without_a_draft_the_action_keys_say_so_and_change_nothing() {
        let mut a = app(80, 25);
        for key in ['d', 'v', 'r', 'o', 'n', 'x', 'i'] {
            let out = press(&mut a, Key::Char(key));
            assert_eq!(a.shared.message.as_deref(), Some(words::NO_DESK), "{key}");
            assert_eq!(a.nav.view, View::Flow);
            assert!(out.is_empty(), "{key}: {out:?}");
        }
    }

    /// Every SGR parameter `bytes` sets.
    fn sgr(bytes: &[u8]) -> Vec<Vec<u16>> {
        let text = String::from_utf8_lossy(bytes);
        text.split("\x1b[")
            .skip(1)
            .filter_map(|s| {
                let end = s.find(|c: char| ('@'..='~').contains(&c))?;
                (s.get(end..=end) == Some("m")).then(|| {
                    s.get(..end)
                        .unwrap_or_default()
                        .split(';')
                        .filter_map(|p| p.parse().ok())
                        .collect()
                })
            })
            .collect()
    }

    #[test]
    fn the_views_draw_in_the_sixteen_colors_with_no_blue() {
        let mut a = app(100, 30);
        let mut screens = vec![view::draw(&a)];
        press(&mut a, Key::Enter);
        screens.push(view::draw(&a));
        press(&mut a, Key::Escape);
        for _ in 1..View::TABS.len() {
            press(&mut a, Key::BackTab);
            screens.push(view::draw(&a));
        }
        for bytes in &screens {
            for params in sgr(bytes) {
                for p in &params {
                    assert!(![34, 44, 94, 104].contains(p), "blue in {params:?}");
                }
                // 256-color and direct color are not used either
                assert!(!params.contains(&38) && !params.contains(&48), "{params:?}");
            }
        }
        // each kind's title in its color: displays cyan, terminals yellow,
        // inputs green (ADR-1403 D7, Colors, borders and lines)
        let flow = sgr(&screens[0]);
        for color in [36, 33, 32] {
            assert!(
                flow.iter().any(|p| p.last() == Some(&color)),
                "{color}: {flow:?}"
            );
        }
    }

    #[test]
    fn without_color_the_view_draws_no_color_and_keeps_its_words() {
        let mut i = inputs();
        i.view = View::Flow;
        i.color = false;
        let (a, _) = App::new(i, 80, 25);
        let bytes = view::draw(&a);
        for params in sgr(&bytes) {
            assert!(
                params
                    .iter()
                    .all(|p| !(30..=49).contains(p) && !(90..=107).contains(p)),
                "{params:?}"
            );
        }
        let screen = text(&a);
        // the boxes keep their borders and their words
        for want in ["CONFIGURATION", "DP-8", "┌", "┘"] {
            assert!(screen.contains(want), "{want}: {screen}");
        }
    }

    #[test]
    fn a_rule_to_a_head_that_does_not_exist_belongs_to_no_path_and_says_so() {
        let mut i = inputs();
        i.view = View::Flow;
        i.flow.rules[0].target = "tv".into();
        let (a, _) = App::new(i, 100, 30);
        let screen = text(&a);
        assert!(
            screen.contains("1 rule belongs to no path. v shows it in the file."),
            "{screen}"
        );
    }

    #[test]
    fn below_80x25_the_flow_view_draws_the_way_out() {
        for (cols, rows) in [(79, 25), (80, 24)] {
            let mut a = app(cols, rows);
            let screen = text(&a);
            assert!(screen.starts_with("This terminal is too small"), "{screen}");
            // the flow's keys do nothing there; q still leaves
            press(&mut a, Key::Right);
            assert_eq!(a.flow.at(), Item::Display(0));
            let out = press(&mut a, Key::Char('q'));
            assert_eq!(out.last(), Some(&Effect::Exit(0)));
        }
    }
}

/// The Flow view on a draft (ADR-1403 D7, D8), driven through the real
/// loop with keys: a configuration directory standing in for
/// `/etc/vtium`, the store under a directory of the tests' own home, a
/// stand-in `vtium` whose `check-config` refuses each line that holds
/// `BAD`, and devices the test gives the desk instead of the machine's.
/// Each test reads the draft's text after its keys.
mod draft_flow {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use vtium_config::flowmodel::Present;
    use vtium_config::tui::desk;
    use vtium_config::tui::port::drive_with;
    use vtium_config::tui::words;
    use vtium_settings::{Class, Identity};

    use vtium_config::tui::fixture::{CHECKER, DRAFT as INSTALLED};

    const RIGHT: &[u8] = b"\x1b[C";
    const LEFT: &[u8] = b"\x1b[D";
    const ESC: &[u8] = b"\x1b";

    /// A fixture under the tests' own home.
    /// The screen's words with each run of spaces and line ends as one
    /// space, so a sentence the message line wraps reads whole.
    fn flat(screen: &str) -> String {
        screen.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// A change on the Flow view: the first path's terminal, desk, renamed
    /// deskx on its card.
    const CHANGE: [&[u8]; 7] = [b"\x1b[H", RIGHT, b"\r", b"\r", b"x", b"\r", ESC];

    struct Bench {
        root: PathBuf,
    }

    fn bench(name: &str) -> Bench {
        let home = home::vars()
            .into_iter()
            .find(|(k, _)| *k == "HOME")
            .map(|(_, v)| v)
            .expect("the tests' home");
        let root = home.join("tui-draft").join(name);
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("a fresh fixture");
        }
        for dir in ["etc/fonts", "home", "sys"] {
            std::fs::create_dir_all(root.join(dir)).expect("the fixture");
        }
        std::fs::write(root.join("etc/vtium.conf"), INSTALLED).expect("vtium.conf");
        let checker = root.join("vtium");
        script::executable(&checker, CHECKER);
        Bench { root }
    }

    /// The keyboard at `pci-0:3.3`, which rule 2 names.
    fn keyboard() -> Identity {
        let mut k = Identity::input().with_class(Class::Keyboard);
        k.name = Some("kbd".into());
        k.path = Some("pci-0:3.3".into());
        k
    }

    /// A second keyboard no rule names alone.
    fn other_keyboard() -> Identity {
        let mut k = Identity::input().with_class(Class::Keyboard);
        k.name = Some("spare".into());
        k.path = Some("pci-0:9.9".into());
        k
    }

    /// A monitor whose EDID reads `maker`, `product` and `serial`, on the
    /// connector `connector`.
    fn monitor(maker: &str, product: u16, serial: &str, connector: &str) -> Identity {
        let mut d = Identity::display();
        d.edid = Some(vtium_settings::Edid {
            maker: maker.into(),
            product,
            serial: serial.into(),
        });
        d.connector = Some(connector.into());
        d
    }

    /// An input of `class` named `name`, on the interface `port` of the
    /// USB controller that docs/heads.md's files name.
    fn plugged(class: Class, name: &str, port: &str) -> Identity {
        let mut d = Identity::input().with_class(class);
        d.name = Some(name.into());
        d.path = Some(format!("pci-0000:15:00.0-usb-0:{port}"));
        d
    }

    /// The machine of docs/heads/kiosk-admin.conf: the kiosk's monitor and
    /// keyboard, the operator's monitor and keyboard, and the spare port
    /// empty.
    fn kiosk_machine() -> Vec<Identity> {
        vec![
            monitor("AAA", 1, "KIOSK00001", "DP-1"),
            monitor("BBB", 2, "ADMIN00001", "HDMI-A-1"),
            plugged(Class::Keyboard, "kiosk keyboard", "3.1:1.0"),
            plugged(Class::Keyboard, "operator keyboard", "3.2:1.0"),
        ]
    }

    /// The machine of docs/heads/stations.conf: three monitors, and on
    /// each station's port a hub with its keyboard and mouse.
    fn stations_machine() -> Vec<Identity> {
        let mut devices = Vec::new();
        for (n, connector) in [(1, "DP-1"), (2, "DP-2"), (3, "HDMI-A-1")] {
            devices.push(monitor("AAA", 1, &format!("MON000000{n}"), connector));
            devices.push(plugged(
                Class::Keyboard,
                "keyboard",
                &format!("3.{n}.1:1.0"),
            ));
            devices.push(plugged(Class::Mouse, "mouse", &format!("3.{n}.2:1.0")));
        }
        devices
    }

    fn present(devices: Vec<Identity>, udev: bool) -> Present {
        Present { devices, udev }
    }

    impl Bench {
        fn store(&self) -> vtium_config::store::Store {
            let root = self.root.clone();
            vtium_config::store::Store::locate(
                1000,
                move |name: &str| match name {
                    "HOME" => Some(root.join("home").into()),
                    "XDG_CONFIG_HOME" => Some(root.join("home/config").into()),
                    "XDG_STATE_HOME" => Some(root.join("home/state").into()),
                    _ => None,
                },
                |_| None,
            )
            .expect("the store")
        }

        fn paths(&self) -> vtium_config::tui::Paths {
            vtium_config::tui::Paths {
                configdir: self.root.join("etc"),
                frits: Ok(self.root.join("etc/fonts")),
                drafts: self.root.join("home/bundles"),
                sudo: "sudo".into(),
                vtium: self.root.join("vtium").into(),
                helper: vtium_config::specimen::Helper {
                    program: "/bin/false".into(),
                    options: Vec::new(),
                },
                sys: self.root.join("sys"),
            }
        }

        /// The desk, whose devices are each of `seen` in turn, the last
        /// for every read after. Its draft is `bench`, made from the
        /// installed file the first time, so no screen shows the
        /// machine's hostname.
        fn desk(&self, seen: Vec<Present>) -> desk::Store {
            let paths = self.paths();
            let store = self.store();
            if store.settings().expect("the settings").draft.is_none() {
                vtium_config::drafts::Session {
                    store: store.clone(),
                    paths: &paths,
                }
                .make("bench", vtium_config::drafts::From::Installed)
                .expect("the draft bench");
            }
            let queue = Rc::new(RefCell::new(VecDeque::from(seen)));
            let last = Rc::new(RefCell::new(Present::default()));
            let devices: desk::Devices = Box::new(move || {
                let next = queue.borrow_mut().pop_front();
                if let Some(next) = next {
                    *last.borrow_mut() = next;
                }
                last.borrow().clone()
            });
            desk::Store::new(self.store(), paths, devices)
        }

        /// The current draft's text.
        fn draft(&self) -> String {
            let store = self.store();
            let name = store
                .settings()
                .expect("the settings")
                .draft
                .expect("a draft");
            store
                .read_draft(&name)
                .expect("the draft")
                .expect("the draft's file")
        }

        fn installed(&self) -> String {
            std::fs::read_to_string(self.root.join("etc/vtium.conf")).expect("vtium.conf")
        }

        /// The width at which a message that names one of the bench's
        /// files keeps the words after the path on screen. The message
        /// line wraps to three rows, and it breaks a path that is wider
        /// than a row across rows. The bench lives under the test
        /// target's temporary directory, so its paths are as long as the
        /// checkout's and the target directory's names make them. At this
        /// width a row holds the bench's longest path, its draft's, whole,
        /// with room for the `:LINE:` that the check adds after it, so
        /// the words after the path fit on the rows that are left.
        fn cols(&self) -> usize {
            let path = self.store().draft_path("bench").expect("the draft's path");
            path.to_string_lossy()
                .chars()
                .count()
                .saturating_add(8)
                .max(100)
        }
    }

    /// What a run on the bench left: the console and the app's last screen.
    struct Ran {
        console: Console,
    }

    impl Ran {
        fn screen(&self) -> &str {
            self.console.seen.as_deref().unwrap_or_default()
        }

        /// The screen's words with one space between each, so a message
        /// reads whole wherever its rows wrap: a message that names the
        /// bench's path wraps where the path's length puts it.
        fn said(&self) -> String {
            self.screen()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        }

        fn status(&self) -> String {
            self.screen().lines().nth(1).unwrap_or_default().to_owned()
        }
    }

    /// Runs the TUI on `b`'s desk at 100x30 with the devices `seen`,
    /// typing `keys`.
    fn run_on(b: &Bench, seen: Vec<Present>, hotplug: bool, keys: &[&[u8]]) -> Ran {
        run_at(b, (100, 30), seen, hotplug, keys)
    }

    fn run_at(
        b: &Bench,
        (cols, rows): (usize, usize),
        seen: Vec<Present>,
        hotplug: bool,
        keys: &[&[u8]],
    ) -> Ran {
        let mut console = Console::vtium(cols, rows, true, "unifont")
            .takes("proof", (cols, rows))
            .takes("unifont", (cols, rows))
            .keys(keys);
        console.installs = true;
        let mut desk = b.desk(seen);
        let mut i = inputs();
        i.view = model::View::Flow;
        i.hotplug = hotplug;
        i.flow = vtium_config::tui::flow::Flow::default();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        Ran { console }
    }

    fn run(b: &Bench, keys: &[&[u8]]) -> Ran {
        run_on(b, vec![present(vec![keyboard()], true)], true, keys)
    }

    /// Runs the TUI on `b`'s desk, 30 rows high and [`Bench::cols`] wide,
    /// in the Looks tab of a vtium console of the head `head`, which draws
    /// unifont and takes tiny5 and proof, typing `keys`. The tab lists the
    /// look big, then the font sets broken, minspec-16, proof, tiny5 and
    /// unifont.
    fn run_looks(b: &Bench, head: Option<&str>, keys: &[&[u8]]) -> Ran {
        let grid = (b.cols(), 30);
        let mut console = Console::vtium(grid.0, grid.1, true, "unifont")
            .takes("tiny5", grid)
            .takes("unifont", grid)
            .takes("proof", grid)
            .keys(keys);
        let mut desk = b.desk(vec![present(vec![keyboard()], true)]);
        let mut i = inputs();
        i.view = model::View::Looks;
        i.head = head.map(str::to_owned);
        i.flow = vtium_config::tui::flow::Flow::default();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        Ran { console }
    }

    /// The keys that try tiny5 on the console and keep it: from the look
    /// big, down past the font sets broken, minspec-16 and proof.
    const KEEP_TINY5: [&[u8]; 6] = [DOWN, DOWN, DOWN, DOWN, b"t", b"k"];

    /// The keys that open the card of the look big, from tiny5 among the
    /// font sets, and step its font set line from proof to tiny5, the
    /// next font set that reads, then close the card.
    const BIG_TO_TINY5: [&[u8]; 8] = [UP, UP, UP, UP, b"\r", DOWN, RIGHT, ESC];

    /// A font kept after a trial stays on the console when the person then
    /// chooses that font set on the card of the look the console's head
    /// draws with, and `s` saves the choice into that look. The console
    /// switches to tiny5 once, for the trial, and back to unifont once, when
    /// the TUI leaves; the card and the save switch nothing. vtium.conf's
    /// top-level font-name stays unset.
    ///
    /// This replaces a test of the old Fonts view's `s`, which saved the
    /// font tried into the look of the console's head. The defect it caught,
    /// a font that the person keeps and saves not reaching that look, is
    /// caught here on the path that replaces the old `s`.
    ///
    /// References:
    /// - ADR-1201 D4, the note of 2026-11-16 on saving the font tried: `s`
    ///   saves as in every view, and a choice on a look's card writes that
    ///   look.
    /// - ADR-1201 D4, the live trial: `k` keeps the font until the TUI
    ///   leaves.
    #[test]
    fn a_kept_font_stays_when_the_look_s_card_chooses_it_and_s_saves_it() {
        let b = bench("kept-then-card");
        let ran = run_looks(
            &b,
            Some("desk"),
            &[&KEEP_TINY5[..], &BIG_TO_TINY5[..], &[b"s"]].concat(),
        );
        let draft = b.draft();
        assert!(
            draft.contains("[look big]\nfont-name = tiny5\n"),
            "{draft}\n{}",
            ran.screen()
        );
        assert!(draft.contains("[head desk]\nlook = big\n"), "{draft}");
        assert_eq!(
            vtium_settings::setting(&draft, "font-name"),
            None,
            "{draft}"
        );
        assert_eq!(ran.console.switches(), ["tiny5", "unifont"]);
        // the console still drew tiny5 when the keys ran out, and went
        // back to unifont only as the TUI left
        let (during, leaving) = ran.console.written.split_at(ran.console.seen_at);
        let back = "\x1b]50;unifont\x1b\\";
        assert!(
            !String::from_utf8_lossy(during).contains(back),
            "put back early"
        );
        assert!(
            String::from_utf8_lossy(leaving).contains(back),
            "given back on leaving"
        );
        assert!(ran.status().contains("no problems"), "{}", ran.status());
    }

    /// `s` after a kept trial saves the draft's changes and nothing else:
    /// the font kept is not written anywhere, for a head whose look names
    /// no font, and where vtium-config cannot tell the console's head.
    /// With no other change, `s` says there is nothing to save; with a new
    /// look made first, `s` saves that look alone.
    ///
    /// This replaces two tests of the old Fonts view's `s`. One saved the
    /// font tried as the top-level font-name for a head whose look names
    /// none. The other saved nothing without `VTIUM_HEAD` while a look
    /// names a font. The defect they caught, the font tried written where
    /// the person did not choose it, is caught here: no `s` writes it.
    ///
    /// References:
    /// - ADR-1201 D4, the note of 2026-11-16 on saving the font tried: `s`
    ///   saves as in every view, and the Looks tab writes no top-level key.
    /// - ADR-1403 D7: Save writes the changes not yet saved.
    #[test]
    fn s_after_a_kept_trial_saves_the_draft_s_changes_and_nothing_else() {
        for head in [Some("panel"), None] {
            let b = bench(&format!("kept-then-s-{}", head.unwrap_or("none")));
            let ran = run_looks(&b, head, &[&KEEP_TINY5[..], &[b"s"]].concat());
            assert_eq!(b.draft(), INSTALLED, "{head:?}");
            assert!(
                flat(ran.screen()).contains(&flat(words::NOTHING_TO_SAVE)),
                "{head:?}: {}",
                ran.screen()
            );
            // a new look first, its name kept and its card closed
            let b = bench(&format!("new-then-kept-{}", head.unwrap_or("none")));
            let ran = run_looks(
                &b,
                head,
                &[&[b"n", b"\r", ESC][..], &KEEP_TINY5[..], &[b"s"]].concat(),
            );
            let draft = b.draft();
            assert!(
                draft.contains("[look new-look]\n"),
                "{head:?}: {draft}\n{}",
                ran.screen()
            );
            assert!(!draft.contains("tiny5"), "{head:?}: {draft}");
            assert_eq!(
                draft.replacen("\n[look new-look]\n", "", 1),
                INSTALLED,
                "{head:?}: only the new look"
            );
        }
    }

    #[test]
    fn the_view_opens_on_the_draft_and_its_state() {
        let b = bench("opens");
        let ran = run(&b, &[]);
        assert_eq!(b.draft(), INSTALLED);
        let status = ran.status();
        assert!(status.contains("· no changes · no problems"), "{status}");
        assert!(!status.contains(words::NO_HOTPLUG), "{status}");
        let screen = ran.screen();
        for want in ["desk", "panel", "look big", "kbd"] {
            assert!(screen.contains(want), "{want}: {screen}");
        }
    }

    #[test]
    fn install_saves_first_then_asks_with_its_counts_and_installs() {
        let b = bench("install");
        let asked = run(&b, &[&CHANGE[..], &[b"i"]].concat());
        // Install saves first, then asks
        //
        // References:
        // - ADR-1403 D7: Install saves the draft first, then asks.
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
        assert!(
            flat(asked.screen()).contains("Install the draft? 4 lines added, 3 removed."),
            "{}",
            asked.screen()
        );
        assert!(asked.console.saves.is_empty());
        assert_eq!(b.installed(), INSTALLED);
        let ran = run(&b, &[b"i", b"y"]);
        assert_eq!(ran.console.saves.len(), 1);
        assert_eq!(b.installed(), b.draft());
        assert!(b.installed().contains("display * -> deskx"));
        assert!(ran.status().contains("no changes"), "{}", ran.status());
        let kept = b.store().history().expect("the history");
        assert_eq!(kept.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&kept[0].path).expect("the kept file"),
            INSTALLED
        );
        // and o restores it into the draft, after asking
        let ran = run(&b, &[b"o", b"\r"]);
        assert!(
            ran.screen()
                .contains("Replace the draft with earlier file 1?"),
            "{}",
            ran.screen()
        );
        run(&b, &[b"o", b"\r", b"y"]);
        assert_eq!(b.draft(), INSTALLED);
    }

    // Enter on a terminal shows its subflow, font set, then what its look
    // draws, then its displays
    //
    // References:
    // - ADR-1403 D7, "The terminal's subflow: font set and effects".
    #[test]
    fn a_terminal_s_card_shows_its_subflow() {
        let b = bench("subflow");
        let ran = run_at(
            &b,
            (120, 32),
            vec![present(vec![keyboard()], true)],
            true,
            &[b"\x1b[H", RIGHT, b"\r"],
        );
        let screen = ran.screen();
        assert!(screen.contains(words::SUBFLOW_TITLE), "{screen}");
        assert!(
            screen.contains("Font set proof  →  look big  →  All displays"),
            "{screen}"
        );
        assert!(flat(screen).contains(words::SUBFLOW_REQUIRES), "{screen}");
    }

    // vtium draws a terminal that names no look with the draft's
    // `[look default]` section, so panel's subflow shows that look's font
    // set and not vtium.conf's
    //
    // References:
    // - ADR-1403 D7, "The terminal's subflow: font set and effects".
    #[test]
    fn a_terminal_with_no_look_shows_the_font_set_of_look_default() {
        const DOWN: &[u8] = b"\x1b[B";
        let b = bench("subflow-default");
        let text = INSTALLED.replace(
            "[head desk]\n",
            "[look default]\nfont-name = unifont\n[head desk]\n",
        );
        assert_ne!(text, INSTALLED);
        std::fs::write(b.root.join("etc/vtium.conf"), &text).expect("vtium.conf");
        let ran = run_at(
            &b,
            (120, 32),
            vec![present(vec![keyboard()], true)],
            true,
            &[b"\x1b[H", RIGHT, DOWN, b"\r"],
        );
        let screen = ran.screen();
        assert!(screen.contains("TERMINAL panel"), "{screen}");
        assert!(
            screen.contains("Font set unifont  →  default look  →  no display"),
            "{screen}"
        );
    }

    /// A terminal's card picks one of the draft's looks, and Save writes
    /// only the head's `look =` line. Desk's card steps its look from big
    /// to the default look and back: the first writes desk's head with no
    /// look line and keeps the look big, which panel can still take; the
    /// second puts the installed file back. Panel's card then takes big.
    ///
    /// References:
    /// - ADR-1205 D1: a terminal takes a look from the Looks tab, and the
    ///   Flow view makes no look itself.
    /// - ADR-1403 D8: the head's `look =` line is the one line a path's
    ///   look writes, and a path with no look leaves it out.
    #[test]
    fn a_terminal_s_card_picks_a_look_and_writes_only_its_look_line() {
        const DOWN: &[u8] = b"\x1b[B";
        // desk's card, its Look line, one choice to the left
        let desk = [b"\x1b[H", RIGHT, b"\r", DOWN, LEFT, b"\r", ESC];
        let b = bench("card-look");
        run(&b, &[&desk[..], &[b"s"]].concat());
        let want = INSTALLED.replace("[head desk]\nlook = big\n", "[head desk]\n");
        assert_ne!(want, INSTALLED, "the fixture's desk names big");
        assert_eq!(b.draft(), want);
        // and back to big, which leaves the installed file as it was
        let b = bench("card-look-back");
        let back = [
            b"\x1b[H", RIGHT, b"\r", DOWN, LEFT, b"\r", RIGHT, b"\r", ESC,
        ];
        let ran = run(&b, &[&back[..], &[b"s"]].concat());
        assert_eq!(b.draft(), INSTALLED, "{}", ran.screen());
        // panel's card, one terminal down, takes big
        let panel = [b"\x1b[H", RIGHT, DOWN, b"\r", DOWN, RIGHT, b"\r", ESC];
        let b = bench("card-look-panel");
        let ran = run(&b, &[&panel[..], &[b"s"]].concat());
        assert_eq!(
            b.draft(),
            INSTALLED.replace("[head panel]\n", "[head panel]\nlook = big\n"),
            "{}",
            ran.screen()
        );
    }

    #[test]
    fn a_change_waits_for_save_and_s_saves_it() {
        let b = bench("save");
        let ran = run(&b, &CHANGE);
        assert!(ran.status().contains("1 not saved"), "{}", ran.status());
        // q, then n: the TUI leaves without the change it never saved
        let b = bench("save-n");
        run(&b, &[&CHANGE[..], &[b"q", b"n"]].concat());
        assert_eq!(b.draft(), INSTALLED, "the change waits for Save");
        // the file view shows the change before it is saved
        let ran = run(&b, &[&CHANGE[..], &[b"v"]].concat());
        assert!(ran.screen().contains("[head deskx]"), "{}", ran.screen());
        let b = bench("save-s");
        let ran = run(&b, &[&CHANGE[..], &[b"s"]].concat());
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
        assert!(
            ran.said().contains("Saved 1 change into draft bench."),
            "{}",
            ran.screen()
        );
        assert!(!ran.status().contains("not saved"), "{}", ran.status());
    }

    #[test]
    fn q_with_unsaved_changes_asks_and_ctrl_c_saves_them() {
        let b = bench("quit");
        let ran = run(&b, &[&CHANGE[..], &[b"q"]].concat());
        assert!(
            flat(ran.screen()).contains(
                "1 change is not saved. Save them before you quit? y saves them and quits, n quits without them, and Esc goes back."
            ),
            "{}",
            ran.screen()
        );
        assert!(
            ran.screen().contains(keys_of("any.question.quit")),
            "{}",
            ran.screen()
        );
        // n quits without the change, and y saves it and quits
        let b = bench("quit-n");
        run(&b, &[&CHANGE[..], &[b"q", b"n"]].concat());
        assert_eq!(b.draft(), INSTALLED);
        let b = bench("quit-y");
        run(&b, &[&CHANGE[..], &[b"q", b"y"]].concat());
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
        // Esc goes back, and the TUI stays
        let b = bench("quit-esc");
        let ran = run(&b, &[&CHANGE[..], &[b"q", ESC]].concat());
        assert!(ran.status().contains("1 not saved"), "{}", ran.status());
        // Ctrl-C saves before it leaves, and so does SIGTERM
        let b = bench("quit-ctrl-c");
        run(&b, &[&CHANGE[..], &[b"\x03"]].concat());
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
        let b = bench("quit-sigterm");
        run(&b, &CHANGE);
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
    }

    /// Restore's list and a question stay open on Tab and Shift-Tab: the
    /// list does nothing with them, and the question takes them as no.
    #[test]
    fn tab_leaves_neither_restore_s_list_nor_a_question_open_behind() {
        let b = bench("tab-restore");
        run(&b, &[&CHANGE[..], &[b"i", b"y"]].concat());
        for key in [TAB, BACKTAB] {
            let ran = run(&b, &[b"o", key]);
            assert_eq!(
                foot(ran.screen(), keys_of("flow.restore")),
                keys_of("flow.restore"),
                "{key:?}: {}",
                ran.screen()
            );
        }
        let ran = run(&b, &[b"o", TAB, BACKTAB, b"\r"]);
        assert!(
            ran.said()
                .contains("Replace the draft with earlier file 1?"),
            "{}",
            ran.screen()
        );
        // revert asks, Tab leaves the question open, and Esc answers no
        let b = bench("tab-revert");
        let ran = run(&b, &[&CHANGE[..], &[b"s", b"r", TAB, ESC]].concat());
        assert!(ran.said().contains(words::NOT_DONE), "{}", ran.screen());
        assert!(ran.status().contains("4 changes"), "{}", ran.status());
        // the Flow view's key line shows its first page, which ends in q
        let line = ran.screen().lines().last().unwrap_or_default().trim();
        assert!(line.ends_with("q quit  . more 1/5"), "{}", ran.screen());
    }

    #[test]
    fn install_refuses_a_draft_made_before_the_installed_file_changed() {
        let b = bench("install-stale");
        // the draft made, then root edits the installed file by hand
        run(&b, &[&CHANGE[..], &[b"s"]].concat());
        let edited = INSTALLED.replace("font-size=12", "font-size=14");
        std::fs::write(b.root.join("etc/vtium.conf"), &edited).expect("the hand edit");
        // the refusal names the installed file's path before its words
        let ran = run_at(
            &b,
            (b.cols(), 30),
            vec![present(vec![keyboard()], true)],
            true,
            &[b"i"],
        );
        assert!(ran.said().contains("changed after"), "{}", ran.screen());
        // and no question asked, so a y installs nothing
        let ran = run(&b, &[b"i", b"y"]);
        assert!(ran.console.saves.is_empty());
        assert_eq!(b.installed(), edited);
        assert!(b.draft().contains("display * -> deskx"), "{}", b.draft());
    }

    // A draft whose only problem is a missing complete route installs only
    // after Install's own question, and a no installs nothing
    //
    // References:
    // - ADR-1403 D7: Install asks before it installs a draft with no
    //   complete path.
    #[test]
    fn install_asks_before_it_installs_a_draft_with_no_complete_path() {
        let b = bench("install-no-route");
        let message = vtium_settings::Missing::NoDisplayRule
            .to_string()
            .replace('\'', r"'\''");
        let checker = CHECKER.replace(
            "if [ -n \"$bad\" ]; then",
            &format!(
                "if grep -q NOROUTE \"$file\"; then\n  printf '%s\\n' \"$file: \"'{message}' >&2\n  [ -n \"$bad\" ] || exit 1\nfi\nif [ -n \"$bad\" ]; then"
            ),
        );
        script::executable(&b.root.join("vtium"), &checker);
        run(&b, &[&CHANGE[..], &[b"s"]].concat());
        let unrouted = format!("# NOROUTE\n{}", b.draft());
        let path = b.store().draft_path("bench").expect("the draft's path");
        std::fs::write(path, &unrouted).expect("the draft");
        let asked = run(&b, &[b"i"]);
        assert!(
            flat(asked.screen()).contains(&flat(&words::no_route_question(5, 3, 0))),
            "{}",
            asked.screen()
        );
        let no = run(&b, &[b"i", b"n"]);
        assert!(no.console.saves.is_empty());
        assert_eq!(b.installed(), INSTALLED);
        let ran = run(&b, &[b"i", b"y"]);
        assert_eq!(ran.console.saves.len(), 1);
        assert_eq!(b.installed(), unrouted);
    }

    #[test]
    fn install_refuses_a_draft_the_check_fails() {
        let b = bench("install-refused");
        // a draft the stand-in check refuses: it names BAD
        run(&b, &[&CHANGE[..], &[b"s"]].concat());
        let bad = b.draft().replace("font-size=12", "font-name=BAD");
        let path = b.store().draft_path("bench").expect("the draft's path");
        std::fs::write(path, &bad).expect("the draft");
        let ran = run(&b, &[b"i"]);
        assert!(ran.status().contains("1 problem"), "{}", ran.status());
        assert!(
            ran.screen().contains("the check found problems"),
            "{}",
            ran.screen()
        );
        assert!(ran.console.saves.is_empty());
        assert_eq!(b.installed(), INSTALLED);
    }

    /// Makes `desk`, the installed file, the current draft of `b`, with one
    /// change to the active resources beside it.
    fn desk_with_a_batch_change(b: &Bench) {
        use vtium_config::batch::{Change, Changes};
        let store = b.store();
        store.write_draft("desk", INSTALLED).expect("the draft");
        store.set_current(Some("desk")).expect("the settings");
        let mut changes = Changes::default();
        changes.set(
            "/usr/share/fonts/TTF/Hack-Regular.ttf".into(),
            Some(Change::Activate),
        );
        store
            .set_batch_changes("desk", &changes)
            .expect("the draft's changes");
    }

    /// Writes a connected display to `b`'s sysfs as `card0-NAME`, at
    /// 1920×1080 pixels and an EDID that states its size `cm` in centimeters.
    fn connect(b: &Bench, name: &str, cm: (u8, u8)) {
        let dir = b.root.join("sys/class/drm").join(format!("card0-{name}"));
        std::fs::create_dir_all(&dir).expect("the display");
        std::fs::write(dir.join("status"), "connected\n").expect("its status");
        std::fs::write(dir.join("modes"), "1920x1080\n").expect("its modes");
        let mut edid = vec![0u8; 128];
        edid.splice(0..8, [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
        edid.splice(21..23, [cm.0, cm.1]);
        let sum = edid.iter().fold(0u8, |s, &x| s.wrapping_add(x));
        edid.splice(127..128, [0u8.wrapping_sub(sum)]);
        std::fs::write(dir.join("edid"), edid).expect("its EDID");
    }

    /// A display connected while vtium-config runs counts toward a look's
    /// point sizes: the desk reads the displays' densities again with the
    /// devices present, and the TUI takes them.
    ///
    /// References:
    /// - ADR-1200 D17: the lowest density among the displays.
    #[test]
    fn a_display_connected_while_the_tui_runs_counts_toward_a_look_s_sizes() {
        use vtium_config::tui::desk::Desk as _;
        use vtium_config::tui::model::{App, Event};
        let b = bench("densities");
        let mut desk = b.desk(vec![present(Vec::new(), true)]);
        connect(&b, "DP-1", (52, 29));
        let read = |desk: &mut desk::Store| match desk.run(&desk::Op::Devices) {
            Ok(Done::Present { densities, .. }) => densities,
            other => panic!("{other:?}"),
        };
        let one = read(&mut desk);
        assert_eq!(one.len(), 1, "{one:?}");
        connect(&b, "HDMI-A-1", (30, 17));
        let two = read(&mut desk);
        assert_eq!(two.len(), 2, "{two:?}");
        assert!(
            two.contains(&one[0]) && two.iter().any(|d| *d > one[0]),
            "{two:?}"
        );
        let (mut a, _) = App::new(inputs(), 80, 25);
        a.event(Event::Desked(
            desk::Op::Devices,
            Ok(Done::Present {
                present: Present::default(),
                densities: two.clone(),
            }),
        ));
        assert_eq!(a.shared.inputs.densities, two);
    }

    #[test]
    fn the_status_line_counts_a_draft_s_changes_to_the_active_resources() {
        let b = bench("status-batch");
        desk_with_a_batch_change(&b);
        let status = run(&b, &[]).status();
        assert!(status.contains("· 1 change · no problems"), "{status}");
    }

    #[test]
    fn revert_and_new_drop_the_draft_s_changes_to_the_active_resources() {
        for key in [b"r", b"n"] {
            let b = bench(&format!("reset-batch-{}", key[0] as char));
            desk_with_a_batch_change(&b);
            run(&b, &[key, b"y"]);
            let left = b
                .store()
                .batch_changes("desk")
                .expect("the draft's changes");
            assert!(left.is_empty(), "{}: {left:?}", key[0] as char);
        }
    }

    #[test]
    fn revert_and_new_each_ask_first() {
        let b = bench("revert");
        run(&b, &[&CHANGE[..], &[b"s"]].concat());
        assert_ne!(b.draft(), INSTALLED);
        let ran = run(&b, &[b"r", b"n", ESC]);
        assert!(ran.screen().contains(words::NOT_DONE), "{}", ran.screen());
        assert_ne!(b.draft(), INSTALLED);
        // the question counts an unsaved change among what is lost
        let unsaved = bench("revert-unsaved");
        let ran = run(&unsaved, &[&CHANGE[..], &[b"r"]].concat());
        assert!(
            flat(ran.screen()).contains("This includes 1 change that is not saved."),
            "{}",
            ran.screen()
        );
        run(&b, &[b"r", b"y"]);
        assert_eq!(b.draft(), INSTALLED);
        run(&b, &[b"n", b"y"]);
        assert_eq!(
            b.draft(),
            vtium_config::ops::new_text(vtium_config::drafts::HEADER)
        );
    }

    #[test]
    fn the_file_view_opens_at_the_highlighted_box_s_line() {
        let b = bench("pager");
        // the first path's display, All displays: its rule on line 9
        let ran = run(&b, &[b"\x1b[H", b"v"]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(screen.contains("▸   9 display * -> desk"), "{screen}");
        assert!(screen.contains("Esc back"), "{screen}");
        assert!(screen.contains("q quit  . more 1/3"), "{screen}");
        // the key line's third page names Save and Install
        let ran = run(&b, &[b"\x1b[H", b"v", b".", b"."]);
        let screen = ran.screen();
        assert!(screen.contains("▸   9 display * -> desk"), "{screen}");
        assert!(screen.contains("s save"), "{screen}");
        assert!(screen.contains("i install"), "{screen}");
        assert!(screen.contains(". more 3/3"), "{screen}");
        // the file view shows its first page from any page of the Flow
        // view's, and Esc goes back to the Flow view's first page
        let ran = run(&b, &[b".", b"v"]);
        assert!(ran.screen().contains(words::PAGER_TITLE));
        assert_eq!(last_row(ran.screen()), super::keys_of("flow.file"));
        let ran = run(&b, &[b"v", b".", ESC]);
        assert_eq!(last_row(ran.screen()), super::keys_of("flow"));
        // Esc goes back to the flow
        let ran = run(&b, &[b"v", b"j", b"G", b"\x04", ESC]);
        assert!(!ran.screen().contains(words::PAGER_TITLE));
    }

    /// Runs the Flow view on `b`'s desk at 100x30, typing `keys`.
    fn run_rows(b: &Bench, keys: &[&[u8]]) -> Ran {
        let mut console = Console::vtium(100, 30, true, "unifont")
            .takes("proof", (100, 30))
            .takes("unifont", (100, 30))
            .keys(keys);
        console.installs = true;
        let mut desk = b.desk(vec![present(vec![keyboard()], true)]);
        let mut i = inputs();
        i.view = model::View::Flow;
        i.hotplug = true;
        i.flow = vtium_config::tui::flow::Flow::default();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        Ran { console }
    }

    #[test]
    fn the_file_view_saves_and_installs_and_stays_open() {
        let b = bench("file-view");
        // the key line's third page names Save and Install
        let ran = run_rows(&b, &[b"v", b".", b"."]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(screen.contains("s save"), "{screen}");
        assert!(screen.contains("i install"), "{screen}");
        // s says what Save says, and the file stays open
        let ran = run_rows(&b, &[b"v", b"s"]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(screen.contains(words::NOTHING_TO_SAVE), "{screen}");
        // i runs the install flow: here the draft is what is installed
        let ran = run_rows(&b, &[b"v", b"i"]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(screen.contains(words::NOTHING_TO_INSTALL), "{screen}");
    }

    #[test]
    fn the_pager_reaches_the_draft_s_last_line() {
        let b = bench("pager-end");
        let long = format!(
            "{INSTALLED}{}# the last line\n",
            "# a line of comment\n".repeat(40)
        );
        std::fs::write(b.root.join("etc/vtium.conf"), long).expect("vtium.conf");
        let ran = run(&b, &[b"v", b"G"]);
        let screen = ran.screen();
        assert!(screen.contains("# the last line"), "{screen}");
    }

    #[test]
    fn a_device_that_leaves_becomes_its_placeholder_and_one_that_comes_is_placed() {
        let b = bench("hotplug");
        // the keyboard rule 2 names, and a spare one, then the spare alone
        let seen = vec![
            present(vec![keyboard(), other_keyboard()], true),
            present(vec![other_keyboard()], true),
        ];
        // udev reports the keyboard went: rule 2 names it, so its part
        // stays, grey, and says so
        //
        // References:
        // - ADR-1403 D7, "Parts from the devices present": unplugging a
        //   device never removes it from the configuration.
        // - ADR-1403 D7, "Colors, borders and lines": a part whose device is
        //   not connected has a grey border.
        let ran = run_on(&b, seen, true, &[b"@"]);
        assert!(
            ran.status().contains("disconnected: kbd"),
            "{}",
            ran.status()
        );
        assert!(ran.screen().contains("Not connected"), "{}", ran.screen());
        assert_eq!(b.draft(), INSTALLED);
        // a keyboard that comes is placed by the rules, roaming here
        let b = bench("hotplug-came");
        let seen = vec![
            present(vec![keyboard()], true),
            present(vec![], true),
            present(vec![other_keyboard()], true),
        ];
        let ran = run_on(&b, seen, true, &[b"@", b"@"]);
        assert!(
            ran.status().contains("connected: spare"),
            "{}",
            ran.status()
        );
        assert!(ran.screen().contains("spare"), "{}", ran.screen());
        assert_eq!(b.draft(), INSTALLED);
    }

    #[test]
    fn without_a_udev_daemon_the_status_line_says_hotplug_is_not_followed() {
        let b = bench("no-udev");
        let ran = run_on(&b, vec![present(vec![keyboard()], false)], true, &[]);
        assert!(ran.status().contains(words::NO_HOTPLUG), "{}", ran.status());
        let ran = run_on(&b, vec![present(vec![keyboard()], true)], false, &[]);
        assert!(ran.status().contains(words::NO_HOTPLUG), "{}", ran.status());
    }

    /// An install whose only change is a new frit, with the draft the
    /// installed vtium.conf, installs the frit through the privileged step
    /// and leaves vtium.conf and the installed history as they were.
    #[test]
    fn an_install_of_a_new_frit_alone_installs_the_frit() {
        let b = bench("frit-alone");
        let mut console = Console::vtium(100, 30, true, "unifont")
            .takes("proof", (100, 30))
            .takes("unifont", (100, 30))
            .keys(&[
                // the Looks tab's first font set, where n opens the form
                TO_LOOKS,
                DOWN,
                b"n",
                // the text font typed after Other…, at the end of its list
                b"\r",
                b"\x1b[F",
                b"\r",
                b"builtin:unscii-8",
                b"\r",
                // from box drawing, five rows down to the name
                DOWN,
                DOWN,
                DOWN,
                DOWN,
                DOWN,
                b"\r",
                b"quiet",
                b"\r",
                // the add row
                b"\r",
                b"i",
                b"y",
            ]);
        console.installs = true;
        let mut desk = b.desk(vec![present(vec![keyboard()], true)]);
        let mut i = inputs();
        i.view = model::View::Fonts;
        i.conf = Some(INSTALLED.to_owned());
        i.frits = b.root.join("etc/fonts");
        i.installed = BTreeMap::new();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        let ran = Ran { console };
        assert_eq!(ran.console.saves.len(), 1, "{}", ran.screen());
        assert_eq!(
            std::fs::read_to_string(b.root.join("etc/fonts/quiet.vtfrit"))
                .expect("the installed frit"),
            "# quiet: made by vtium-config new-frit\nname=quiet\nsource=builtin:unscii-8\n"
        );
        assert!(
            ran.screen().contains("Installed draft bench"),
            "{}",
            ran.screen()
        );
        assert_eq!(b.installed(), INSTALLED);
        assert!(b.store().history().expect("the history").is_empty());
    }

    /// The stand-in desk refuses what the store refuses: an install that
    /// no Prepare readied. After a Prepare of a draft that is the installed
    /// file, both run the install.
    #[test]
    fn the_stand_in_desk_installs_only_what_a_prepare_readied_as_the_store_does() {
        use vtium_config::tui::desk::Op;
        let b = bench("stand-in");
        let mut store = b.desk(vec![present(Vec::new(), true)]);
        let mut stand_in = super::desk();
        let desks: [&mut dyn vtium_config::tui::desk::Desk; 2] = [&mut store, &mut stand_in];
        for (n, d) in desks.into_iter().enumerate() {
            let mut ran = 0;
            let mut run = |_: &[std::ffi::OsString]| -> io::Result<(bool, String)> {
                ran += 1;
                Ok((true, String::new()))
            };
            let plan = vtium_config::draft::Plan::default();
            let mut out = Vec::new();
            let mut ask = |_: &mut dyn std::io::Write, _: &str| -> bool { false };
            let why = d
                .install(&plan, &mut run, &mut out, &mut ask)
                .expect_err("no Prepare");
            assert_eq!(why, words::NOT_PREPARED, "desk {n}");
            assert!(matches!(d.run(&Op::Prepare), Ok(Done::Plan(_))), "desk {n}");
            assert!(
                d.install(&plan, &mut run, &mut out, &mut ask).is_ok(),
                "desk {n}"
            );
            let why = d
                .install(&plan, &mut run, &mut out, &mut ask)
                .expect_err("one install a Prepare");
            assert_eq!(why, words::NOT_PREPARED, "desk {n}");
            assert_eq!(ran, 1, "desk {n}");
        }
    }

    /// The Flow view's states on a draft at 80x25 and 120x32 (ADR-1403
    /// D7), each reached by its keys: the shipped default, a configuration
    /// of three paths (desk, panel and the roaming inputs), a new path half
    /// built, the Input maker, the USB port picker, a terminal's card and
    /// a labelled part, and the kiosk with an operator's head and the
    /// stations of docs/heads.md, each drawn from its file there.
    #[test]
    fn the_flow_view_s_states_at_80x25_and_120x32() {
        const HOME: &[u8] = b"\x1b[H";
        // Down from the first box reaches PARTS' inputs at the sixth step
        let to_inputs: Vec<&[u8]> = [&[HOME][..], &[DOWN; 6][..]].concat();
        let maker: Vec<&[u8]> = [&to_inputs[..], &[LEFT, b"\r"][..]].concat();
        let enter: &[u8] = b"\r";
        let picker: Vec<&[u8]> = [&maker[..], &[DOWN; 4][..], &[enter][..]].concat();
        let typed: [&[u8]; 4] = [enter, enter, b"Left desk", enter];
        let label: Vec<&[u8]> = [&to_inputs[..], &typed[..]].concat();
        // the draft with desk on VT 3 and panel on VT 4, each showing
        // every display (ADR-1403 D7)
        let vts = "font-size=12\nversion = 1\n[look big]\nfont-name = proof\n[head desk]\nlook = big\nvt = 3\n[head panel]\nvt = 4\n[rules]\ndisplay * -> desk\ndisplay * -> panel\ninput path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming\n";
        // no sections: the draft runs vtium's built-in default
        let shipped = "font-size=12\n";
        let space: &[u8] = b" ";
        let clash: Vec<&[u8]> = [&[HOME][..], &[DOWN; 4][..], &[space][..]].concat();
        // the terminal desk's card with the cursor on its Login line
        let login: Vec<&[u8]> = [&[HOME, RIGHT, enter][..], &[DOWN; 4][..]].concat();
        // the same card with the cursor on its Switch on Enter line: desk
        // runs on every VT, so the hint says the key has no effect
        let switch: Vec<&[u8]> = [&[HOME, RIGHT, enter][..], &[DOWN; 6][..]].concat();
        // two closed files of docs/heads.md, which docs/concepts/flows.md
        // draws: a kiosk with an operator's head, and three stations
        let kiosk_admin = include_str!("../../../docs/heads/kiosk-admin.conf");
        let stations = include_str!("../../../docs/heads/stations.conf");
        // each golden's name, the vtium.conf it starts from when not the
        // fixture's, and the keys that reach it
        type Script<'a> = (&'a str, Option<&'a str>, &'a [&'a [u8]]);
        let scripts: [Script<'_>; 13] = [
            ("flow-default", Some(shipped), &[]),
            ("flow-paths", None, &[]),
            ("flow-new-path", None, &[HOME, RIGHT, RIGHT, b" "]),
            ("flow-input-maker", None, &maker),
            ("flow-picker", None, &picker),
            ("flow-terminal-card", None, &[HOME, RIGHT, b"\r"]),
            ("flow-login", None, &login),
            ("flow-switch-on-enter", None, &switch),
            ("flow-label", None, &label),
            ("flow-shared-vts", Some(vts), &[]),
            ("flow-vt-clash", Some(vts), &clash),
            ("flow-kiosk-admin", Some(kiosk_admin), &[]),
            ("flow-stations", Some(stations), &[]),
        ];
        for (cols, rows) in [(80, 25), (120, 32)] {
            for (name, conf, keys) in scripts {
                let b = bench(&format!("{name}-{cols}"));
                if let Some(conf) = conf {
                    std::fs::write(b.root.join("etc/vtium.conf"), conf).expect("vtium.conf");
                }
                // the files of docs/heads.md on machines they were made for
                let devices = match name {
                    "flow-kiosk-admin" => kiosk_machine(),
                    "flow-stations" => stations_machine(),
                    _ => vec![keyboard()],
                };
                let ran = run_at(&b, (cols, rows), vec![present(devices, true)], true, keys);
                let lines: Vec<&str> = ran.screen().lines().collect();
                assert_eq!(lines.len(), rows, "{name} at {cols}x{rows}");
                assert!(lines.iter().all(|l| l.chars().count() <= cols), "{name}");
                assert!(lines[1].contains("draft bench"), "{name}: {}", ran.screen());
                let c = &ran.console;
                let bytes = c.written.get(..c.seen_at).unwrap_or_default();
                golden(&format!("{name}-{cols}x{rows}"), ran.screen(), bytes);
            }
        }
    }
}
