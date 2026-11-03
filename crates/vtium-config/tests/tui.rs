//! The TUI driven headlessly through its real loop (ADR-1201 D8): scripted
//! keys go in as bytes, the console's own terminal, vtium-term, answers on
//! the other side as a vtium console does, and each test reads the state's
//! effects in the bytes the TUI wrote and in the screen the terminal holds.
//!
//! The screens at 80x25 and 100x30 are vtium's own goldens in
//! `tests/screens/`, provisional until the maintainer reviews the layout:
//! `tests/screens/provisional.tsv` lists them, and a provisional golden
//! that drifts is reported and judges nothing.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use vtium_config::draft::Plan;
use vtium_config::kitty;
use vtium_config::specimen::Picture;
use vtium_config::tui::model::{self, DpiFrom, Font, FontState, Inputs};
use vtium_config::tui::port::{Port, Wake, drive};
use vtium_term::Glyph;
use vtium_term::Terminal;

/// What answers the TUI.
enum Answerer {
    /// The console's terminal, as vtium answers: XTVERSION, the OSC 50
    /// query and switches under `allow-font-ops`, DA1.
    Vtium(Box<Terminal>),
    /// A terminal that answers DA1 alone.
    Da1Only { cols: usize, rows: usize },
}

/// A console standing in for the terminal the TUI runs in.
struct Console {
    answerer: Answerer,
    /// The fonts the console takes, each with the grid it gives.
    fonts: BTreeMap<&'static str, (usize, usize)>,
    /// Keys still to type, one read each.
    script: VecDeque<Vec<u8>>,
    pending: VecDeque<Wake>,
    armed: bool,
    /// Every byte the TUI wrote.
    written: Vec<u8>,
    /// Each OSC 50 switch the console took ("set NAME") or refused
    /// ("refuse NAME") as the TUI wrote it, and each reply ("reply NAME")
    /// as the TUI read it, in order.
    log: Vec<String>,
    saves: Vec<Plan>,
    save_result: Result<usize, String>,
    panic_bytes: Vec<u8>,
    /// The screen when the script ran out, after which the console sends
    /// SIGTERM.
    seen: Option<String>,
    /// The console answers kitty's graphics query, as vtium built with
    /// `images` does.
    graphics: bool,
    /// Each specimen the TUI asked for: the font, the pixel height and the
    /// most pixels it may take.
    pictures: Vec<(String, u32, (u32, u32))>,
    /// The terminal's cell in pixels, as the terminal reports it.
    cell_px: Option<(u32, u32)>,
}

impl Console {
    fn vtium(cols: usize, rows: usize, ops: bool, start: &str) -> Console {
        let mut t = Terminal::new(cols, rows, 0);
        t.set_version("vtium(test)").expect("printable");
        t.set_font_ops(ops);
        assert!(t.set_font_name(start));
        Console::with(Answerer::Vtium(Box::new(t)))
    }

    fn da1_only(cols: usize, rows: usize) -> Console {
        Console::with(Answerer::Da1Only { cols, rows })
    }

    fn with(answerer: Answerer) -> Console {
        Console {
            answerer,
            fonts: BTreeMap::new(),
            script: VecDeque::new(),
            pending: VecDeque::new(),
            armed: false,
            written: Vec::new(),
            log: Vec::new(),
            saves: Vec::new(),
            save_result: Ok(1),
            panic_bytes: Vec::new(),
            seen: None,
            graphics: false,
            pictures: Vec::new(),
            cell_px: Some((8, 16)),
        }
    }

    fn takes(mut self, name: &'static str, grid: (usize, usize)) -> Console {
        self.fonts.insert(name, grid);
        self
    }

    fn keys(mut self, chunks: &[&[u8]]) -> Console {
        self.script.extend(chunks.iter().map(|c| c.to_vec()));
        self
    }

    /// The terminal's screen as text, one line a row.
    fn screen(&self) -> String {
        let Answerer::Vtium(t) = &self.answerer else {
            return String::new();
        };
        let s = t.screen();
        (0..s.rows())
            .map(|r| {
                let line = s.line(r).expect("a row");
                let text: String = line
                    .cells()
                    .iter()
                    .map(|&c| match line.glyph(c) {
                        Glyph::Char(ch) => ch.to_string(),
                        Glyph::Cluster(s) => s.to_owned(),
                        Glyph::Empty => " ".to_owned(),
                    })
                    .collect();
                format!("{}\n", text.trim_end())
            })
            .collect()
    }

    /// The OSC 50 sequences the TUI wrote, but the query at start.
    fn switches(&self) -> Vec<String> {
        let text = String::from_utf8_lossy(&self.written).into_owned();
        text.split("\x1b]50;")
            .skip(1)
            .map(|s| s.split('\x1b').next().unwrap_or("").to_owned())
            .filter(|s| s != "?")
            .collect()
    }
}

impl Port for Console {
    fn size(&mut self) -> (usize, usize) {
        match &self.answerer {
            Answerer::Vtium(t) => (t.screen().cols(), t.screen().rows()),
            Answerer::Da1Only { cols, rows } => (*cols, *rows),
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.written.extend_from_slice(bytes);
        match &mut self.answerer {
            Answerer::Vtium(t) => {
                let mut rest = bytes;
                while !rest.is_empty() {
                    let taken = t.input(rest);
                    if let Some(name) = t.take_font_request() {
                        if let Some(&(cols, rows)) = self.fonts.get(name.as_str()) {
                            assert!(t.set_font_name(&name));
                            self.log.push(format!("set {name}"));
                            if (cols, rows) != (t.screen().cols(), t.screen().rows()) {
                                t.resize(cols, rows);
                                self.pending.push_back(Wake::Resize);
                            }
                        } else {
                            self.log.push(format!("refuse {name}"));
                        }
                    }
                    rest = &rest[taken..];
                }
                let mut replies = t.take_replies();
                if self.graphics && bytes.windows(4).any(|w| w == b"a=q,") {
                    // the graphics query's answer comes before DA1's
                    let at = replies
                        .windows(3)
                        .position(|w| w == b"\x1b[?")
                        .unwrap_or(replies.len());
                    let ok = format!("\x1b_Gi={};OK\x1b\\", model::QUERY_ID);
                    replies.splice(at..at, ok.into_bytes());
                }
                if !replies.is_empty() {
                    self.pending.push_back(Wake::Bytes(replies));
                }
            }
            Answerer::Da1Only { .. } => {
                if bytes.windows(3).any(|w| w == b"\x1b[c") {
                    self.pending.push_back(Wake::Bytes(b"\x1b[?62c".to_vec()));
                }
            }
        }
        Ok(())
    }

    fn arm(&mut self, _after: Duration) {
        self.armed = true;
    }

    fn wait(&mut self) -> io::Result<Vec<Wake>> {
        if !self.pending.is_empty() {
            let wakes: Vec<Wake> = self.pending.drain(..).collect();
            // a reply counts once the TUI reads it
            for wake in &wakes {
                if let Wake::Bytes(bytes) = wake {
                    for name in String::from_utf8_lossy(bytes).split("\x1b]50;").skip(1) {
                        let name = name.split('\x1b').next().unwrap_or("");
                        self.log.push(format!("reply {name}"));
                    }
                }
            }
            return Ok(wakes);
        }
        if let Some(keys) = self.script.pop_front() {
            if keys.first() == Some(&b'!') {
                // a signal, by its number after the mark
                let n = String::from_utf8_lossy(&keys[1..])
                    .parse()
                    .expect("a number");
                return Ok(vec![Wake::Signal(n)]);
            }
            return Ok(vec![Wake::Bytes(keys)]);
        }
        if std::mem::take(&mut self.armed) {
            return Ok(vec![Wake::Timeout]);
        }
        assert!(self.seen.is_none(), "the TUI still waits after SIGTERM");
        self.seen = Some(self.screen());
        Ok(vec![Wake::Signal(15)])
    }

    fn save(&mut self, plan: &Plan) -> Result<usize, String> {
        self.saves.push(plan.clone());
        self.save_result.clone()
    }

    fn on_panic(&mut self, bytes: Vec<u8>) {
        self.panic_bytes = bytes;
    }

    fn cell_px(&mut self) -> Option<(u32, u32)> {
        self.cell_px
    }

    fn picture(&mut self, font: &str, height: u32, max: (u32, u32)) -> Result<Picture, String> {
        self.pictures.push((font.to_owned(), height, max));
        Ok(Picture {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        })
    }
}

const DOWN: &[u8] = b"\x1b[B";
const UP: &[u8] = b"\x1b[A";

fn font(name: &str, state: FontState) -> Font {
    Font {
        name: name.to_owned(),
        state,
    }
}

/// The fonts of a machine with three frits, one that does not read.
fn inputs() -> Inputs {
    Inputs {
        fonts: vec![
            font(
                "broken",
                FontState::Refused("line 2: unknown key colour".into()),
            ),
            font("minspec-16", FontState::Reads),
            font("proof", FontState::Reads),
            font("tiny5", FontState::Reads),
            font("unifont", FontState::BuiltIn),
            font("unscii-8", FontState::BuiltIn),
        ],
        conf_path: PathBuf::from("/etc/vtium/vtium.conf"),
        conf: Some("# mine\nfont-name=unifont\n".into()),
        heads_path: PathBuf::from("/etc/vtium/heads.conf"),
        heads: Some(
            "version = 1\n[head desk]\nprofile = proof\n[head panel]\n[rules]\ndisplay * -> desk\n"
                .into(),
        ),
        saving: model::Saving::Sudo("sudo /usr/bin/vtium-config apply DRAFT".into()),
        drafts: PathBuf::from("/home/me/.local/state/vtium/drafts"),
        dpi: (96, DpiFrom::Assumed),
    }
}

/// The console of most tests: vtium at 80x25 drawing unifont, taking the
/// fonts that read at the same grid.
fn console() -> Console {
    Console::vtium(80, 25, true, "unifont")
        .takes("minspec-16", (80, 25))
        .takes("proof", (80, 25))
        .takes("tiny5", (80, 25))
        .takes("unifont", (80, 25))
        .takes("unscii-8", (80, 25))
}

fn run(console: &mut Console) -> u8 {
    drive(console, inputs()).expect("the loop")
}

#[test]
fn the_four_requests_go_first_in_their_order() {
    let mut c = console().keys(&[b"q"]);
    run(&mut c);
    assert!(
        c.written.starts_with(&model::probe()),
        "{:?}",
        String::from_utf8_lossy(&c.written[..60])
    );
    assert_eq!(
        model::probe(),
        b"\x1b[>q\x1b]50;?\x1b\\\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c"
    );
}

#[test]
fn each_highlighted_font_switches_the_console_one_at_a_time() {
    // unifont is highlighted at start; three Downs in one read, then
    // three more, each answered before the next switch goes
    let mut c = console().keys(&[UP, &[DOWN, DOWN].concat(), DOWN, b"q"]);
    assert_eq!(run(&mut c), 0);
    // between two switches the console always replied
    let mut outstanding = false;
    for entry in &c.log {
        if entry.starts_with("set ") || entry.starts_with("refuse ") {
            assert!(!outstanding, "a second switch before a reply: {:?}", c.log);
            outstanding = true;
        } else if entry.starts_with("reply ") {
            outstanding = false;
        }
    }
    // Up reaches tiny5; of the two Downs in one read, the first sends
    // unifont and the second waits for its reply, then sends unscii-8,
    // what is highlighted then; the last Down moves nothing, and leaving
    // gives unifont back
    assert_eq!(
        c.switches(),
        ["tiny5", "unifont", "unscii-8", "unifont"],
        "{:?}",
        c.log
    );
}

#[test]
fn a_font_the_console_refuses_is_marked_and_the_tui_goes_on() {
    let mut c = Console::vtium(80, 25, true, "unifont")
        .takes("unifont", (80, 25))
        .takes("proof", (80, 25))
        .keys(&[UP, UP, b"q"]);
    assert_eq!(run(&mut c), 0);
    assert!(c.log.contains(&"refuse tiny5".to_owned()), "{:?}", c.log);
    assert!(c.log.contains(&"set proof".to_owned()), "{:?}", c.log);
}

#[test]
fn the_refusal_mark_is_on_the_screen() {
    let mut c = Console::vtium(80, 25, true, "unifont")
        .takes("unifont", (80, 25))
        .keys(&[UP]);
    let mut inputs = inputs();
    inputs.conf = None;
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("tiny5") && screen.contains("kept by the console, see its log"),
        "{screen}"
    );
}

#[test]
fn leaving_by_q_gives_the_start_font_back() {
    let mut c = console().keys(&[DOWN, DOWN, b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches().last().map(String::as_str), Some("unifont"));
    assert_eq!(c.log.last().map(String::as_str), Some("reply unifont"));
    assert!(
        c.panic_bytes.is_empty(),
        "the font is back, so a panic writes nothing"
    );
}

#[test]
fn leaving_by_ctrl_c_gives_the_start_font_back() {
    let mut c = console().keys(&[UP, b"\x03"]);
    assert_eq!(run(&mut c), 130);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
}

#[test]
fn leaving_by_sigterm_gives_the_start_font_back() {
    let mut c = console().keys(&[UP, b"!15"]);
    assert_eq!(run(&mut c), 143);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
}

#[test]
fn leaving_with_a_switch_in_flight_waits_for_both_replies() {
    // the switch to tiny5 and the leave come in one read
    let mut c = console().keys(&[&[UP, b"q"].concat()]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.log.last().map(String::as_str), Some("reply unifont"));
    assert!(
        c.panic_bytes.is_empty(),
        "the font is back, so nothing is written again"
    );
}

#[test]
fn another_terminal_gets_no_osc_50_but_the_start_query() {
    let mut c = Console::da1_only(80, 25).keys(&[DOWN, DOWN, UP, b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), Vec::<String>::new());
    let text = String::from_utf8_lossy(&c.written);
    assert_eq!(
        text.matches("\x1b]50;").count(),
        1,
        "the query at start only"
    );
}

#[test]
fn with_font_operations_off_the_tui_says_so_and_offers_the_key() {
    let mut c = Console::vtium(80, 25, false, "unifont")
        .takes("tiny5", (80, 25))
        .keys(&[UP, UP]);
    run(&mut c);
    assert_eq!(c.switches(), Vec::<String>::new());
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("live preview: off"), "{screen}");
    assert!(screen.contains("allow-font-ops is off"), "{screen}");
}

#[test]
fn turning_font_operations_on_is_a_change_saved_like_any_other() {
    let mut c = Console::vtium(80, 25, false, "unifont").keys(&[b"o", b"s", b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    let plan = c.saves.first().expect("a save");
    assert_eq!(plan.files.len(), 1);
    assert!(
        plan.files[0].new.contains("\nallow-font-ops\n"),
        "{:?}",
        plan.files[0].new
    );
}

#[test]
fn a_font_switch_that_changes_the_grid_redraws_at_the_new_grid() {
    let mut c = console().takes("tiny5", (100, 30)).keys(&[UP]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 30);
    assert!(
        rows[29].contains("Enter keep"),
        "the keys on the last row: {screen}"
    );
    assert!(
        rows[0].len() > 80,
        "the title reaches the new edge: {:?}",
        rows[0]
    );
}

#[test]
fn a_grid_too_small_draws_one_line_with_the_way_out() {
    let mut c = console().takes("tiny5", (30, 8)).keys(&[UP]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert_eq!(screen.lines().next(), Some("tiny5  Enter keep  q back"));
}

#[test]
fn keeping_a_font_and_a_size_shows_each_changed_line_before_saving() {
    let mut c = console().keys(&[
        UP, b"\r", b"\t", DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, b"\r", b"s",
    ]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("/etc/vtium/vtium.conf"), "{screen}");
    assert!(screen.contains("2 - font-name=unifont"), "{screen}");
    assert!(screen.contains("2 + font-name=tiny5"), "{screen}");
    assert!(screen.contains("3 + font-size=12"), "{screen}");
    assert!(
        screen.contains("sudo /usr/bin/vtium-config apply DRAFT"),
        "{screen}"
    );
}

#[test]
fn saving_runs_one_save_and_the_console_keeps_the_saved_font() {
    let mut c = console().keys(&[UP, b"\r", b"s", b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.saves.len(), 1);
    let file = &c.saves[0].files[0];
    assert_eq!(file.new, "# mine\nfont-name=tiny5\n");
    // after saving, leaving does not switch back
    assert_eq!(c.switches(), ["tiny5"]);
}

#[test]
fn a_failed_save_keeps_what_was_chosen_and_says_why() {
    let mut c = console().keys(&[UP, b"\r", b"s", b"\r"]);
    c.save_result = Err("sudo: a password is required".into());
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Nothing was saved: sudo: a password is required"),
        "{screen}"
    );
    assert!(
        screen.contains("+ font-name=tiny5"),
        "still in the save view: {screen}"
    );
}

#[test]
fn a_head_s_profile_is_chosen_and_written_through_the_one_writer() {
    let mut c = console().keys(&[
        b"\t", b"\t", b"\t", DOWN, b"\x1b[C", b"\x1b[C", b"s", b"\r", b"q",
    ]);
    assert_eq!(run(&mut c), 0);
    let file = &c.saves[0].files[0];
    assert_eq!(file.path, Path::new("/etc/vtium/heads.conf"));
    assert_eq!(
        file.new,
        "version = 1\n[head desk]\nprofile = proof\n[head panel]\nprofile = proof\n[rules]\ndisplay * -> desk\n"
    );
}

#[test]
fn nothing_kept_means_nothing_to_save() {
    let mut c = console().keys(&[b"s", b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert!(c.saves.is_empty());
}

/// The provisional goldens' names, from `tests/screens/provisional.tsv`.
fn provisional() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/screens/provisional.tsv");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split('\t').next().map(str::to_owned))
        .collect()
}

/// Compares `screen` with the golden `name`: a settled golden fails on a
/// difference, a provisional one reports it. `VTIUM_CONFIG_BLESS=1`
/// writes the provisional ones.
fn golden(name: &str, screen: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/screens")
        .join(format!("{name}.txt"));
    let is_provisional = provisional().iter().any(|n| n == name);
    if std::env::var_os("VTIUM_CONFIG_BLESS").is_some() {
        assert!(
            is_provisional,
            "{name}: only a provisional golden is written"
        );
        std::fs::write(&path, screen).expect("the golden");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    if want != screen {
        assert!(
            is_provisional,
            "{name} differs from its settled golden:\n{screen}"
        );
        eprintln!("provisional golden {name} drifted; it judges nothing:\n{screen}");
    }
}

/// The screen the script `keys` reaches at `cols` by `rows`.
fn reach(cols: usize, rows: usize, keys: &[&[u8]]) -> String {
    let grid = (cols, rows);
    let mut c = Console::vtium(cols, rows, true, "unifont")
        .takes("minspec-16", grid)
        .takes("proof", grid)
        .takes("tiny5", grid)
        .takes("unifont", grid)
        .takes("unscii-8", grid)
        .keys(keys);
    run(&mut c);
    c.seen.clone().expect("the screen")
}

#[test]
fn each_view_at_80x25_and_100x30() {
    let scripts: [(&str, &[&[u8]]); 5] = [
        ("fonts", &[]),
        ("sizes", &[b"\t"]),
        ("settings", &[b"\t", b"\t"]),
        ("heads", &[b"\t", b"\t", b"\t"]),
        ("save", &[UP, b"\r", b"s"]),
    ];
    for (cols, rows) in [(80, 25), (100, 30)] {
        for (name, keys) in scripts {
            let screen = reach(cols, rows, keys);
            let lines: Vec<&str> = screen.lines().collect();
            assert_eq!(lines.len(), rows, "{name} at {cols}x{rows}");
            assert!(lines.iter().all(|l| l.chars().count() <= cols), "{name}");
            assert!(lines[0].contains("VTium Vitrifier"), "{name}: {screen}");
            assert!(lines[rows - 1].contains("q leave"), "{name}: {screen}");
            golden(&format!("{name}-{cols}x{rows}"), &screen);
        }
    }
}

#[test]
fn a_console_that_never_answers_the_give_back_is_left_after_the_wait() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font("unifont".into()),
        Reply::Attributes,
    ] {
        assert_eq!(app.event(Event::Input(Input::Reply(r))), []);
    }
    let up = app.event(Event::Input(Input::Key(Key::Up)));
    assert_eq!(up, [Effect::Write(model::switch("tiny5"))]);
    assert_eq!(
        app.event(Event::Input(Input::Reply(Reply::Font("tiny5".into())))),
        []
    );
    let leave = app.event(Event::Input(Input::Key(Key::Char('q'))));
    assert_eq!(
        leave,
        [
            Effect::Write(model::switch("unifont")),
            Effect::Wait(model::WAIT)
        ]
    );
    // keys while leaving do nothing; the wait's end leaves
    assert_eq!(app.event(Event::Input(Input::Key(Key::Down))), []);
    assert_eq!(app.event(Event::Timeout), [Effect::Exit(0)]);
}

#[test]
fn a_terminal_that_answers_nothing_is_settled_by_the_wait() {
    use vtium_config::tui::model::{App, Event, Live};
    let (mut app, first) = App::new(inputs(), 80, 25);
    assert_eq!(first[1], model::Effect::Wait(model::WAIT));
    assert_eq!(app.live, Live::Asking);
    app.event(Event::Timeout);
    assert_eq!(app.live, Live::NotVtium);
}

#[test]
fn a_failed_command_s_words_show_in_the_save_view() {
    let mut c = console().keys(&[UP, b"\r", b"s", b"\r"]);
    c.save_result = Err(
        "the command failed, and the draft stays in /d.\nvtium-config: the console's check refused the draft:\n/x/vtium.conf:2: font-size=\"lots\" is not a whole number, ignored".into(),
    );
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Nothing was saved: the command failed, and the draft stays in /d."),
        "{screen}"
    );
    assert!(screen.contains("The command said:"), "{screen}");
    assert!(
        screen.contains("font-size=\"lots\" is not a whole number"),
        "{screen}"
    );
}

/// The bytes that show the stand-in's picture at the size view's place.
fn shown() -> Vec<u8> {
    let mut b = b"\x1b[4;26H".to_vec();
    b.extend(kitty::show(
        kitty::IMAGE_ID,
        2,
        1,
        &[255, 0, 0, 255, 0, 0, 255, 255],
    ));
    b
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

#[test]
fn each_highlighted_size_shows_its_specimen_and_deletes_the_last() {
    let mut c = console().keys(&[b"\t", DOWN, DOWN, b"\t", b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    // 6 pt and 7 pt at 96 dpi, unifont highlighted in the font view, in
    // the area beside the list, 55 columns by 19 rows of 8x16 cells
    let area = (55 * 8, 19 * 16);
    assert_eq!(
        c.pictures,
        [
            ("unifont".to_owned(), 8, area),
            ("unifont".to_owned(), 9, area)
        ]
    );
    assert_eq!(
        kitty::show(kitty::IMAGE_ID, 2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]),
        b"\x1b_Ga=T,f=32,t=d,i=1201,s=2,v=1,C=1,q=2,m=0;/wAA/wAA//8=\x1b\\"
    );
    assert_eq!(count(&c.written, &shown()), 2);
    // deleted when the next size comes and when the view is left
    assert_eq!(count(&c.written, &kitty::delete(kitty::IMAGE_ID)), 2);
    let last_show = c
        .written
        .windows(shown().len())
        .rposition(|w| w == shown())
        .expect("shown");
    let last_delete = c
        .written
        .windows(kitty::delete(kitty::IMAGE_ID).len())
        .rposition(|w| w == kitty::delete(kitty::IMAGE_ID))
        .expect("deleted");
    assert!(last_delete > last_show, "the last picture is deleted");
}

#[test]
fn leaving_with_a_picture_deletes_it_before_the_font_comes_back() {
    let mut c = console().keys(&[UP, b"\t", DOWN, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    let text = String::from_utf8_lossy(&c.written).into_owned();
    let delete = text
        .rfind("\x1b_Ga=d,d=I,i=1201,q=2\x1b\\")
        .expect("deleted");
    let back = text.rfind("\x1b]50;unifont\x1b\\").expect("the font back");
    assert!(delete < back, "deleted, then the font given back");
}

#[test]
fn automatic_has_no_picture() {
    let mut c = console().keys(&[b"\t", DOWN, UP]);
    c.graphics = true;
    run(&mut c);
    assert_eq!(c.pictures.len(), 1, "6 pt only");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Automatic lets vtium pick the size"),
        "{screen}"
    );
}

#[test]
fn without_kitty_graphics_no_picture_is_sent_and_the_view_says_so() {
    let mut c = console().keys(&[b"\t", DOWN, DOWN]);
    run(&mut c);
    assert!(c.pictures.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("draws no pictures"), "{screen}");
}

#[test]
fn the_picture_never_reaches_the_footer_and_never_passes_the_cap() {
    // 80x25 at 8x16: rows 4 to 22 are the area, above the message line
    let mut c = console().keys(&[b"\t", DOWN]);
    c.graphics = true;
    run(&mut c);
    let (_, _, (w, h)) = c.pictures[0].clone();
    assert!(h <= (25 - 6) * 16 && w <= (80 - 25) * 8, "{w}x{h}");
    // a terminal of very large cells is capped
    let mut c = console().keys(&[b"\t", DOWN]);
    c.graphics = true;
    c.cell_px = Some((200, 400));
    run(&mut c);
    let (_, _, max) = c.pictures[0].clone();
    assert_eq!(max, vtium_config::specimen::PICTURE_MAX);
}

#[test]
fn display_dpi_outside_the_console_s_bounds_is_not_used() {
    use vtium_config::tui::density;
    assert_eq!(density(Some("display-dpi=150\n")), (150, DpiFrom::Conf));
    assert_eq!(density(Some("display-dpi=1000\n")), (1000, DpiFrom::Conf));
    assert_eq!(density(Some("display-dpi=1001\n")), (96, DpiFrom::Assumed));
    assert_eq!(density(Some("display-dpi=19\n")), (96, DpiFrom::Assumed));
    assert_eq!(density(Some("display-dpi=lots\n")), (96, DpiFrom::Assumed));
    assert_eq!(density(None), (96, DpiFrom::Assumed));
}
