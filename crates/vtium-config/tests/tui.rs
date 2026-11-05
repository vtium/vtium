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
use vtium_config::serve::LeftOut;
use vtium_config::specimen::{Picture, Sources, Summary};
use vtium_config::tui::model::{self, DpiFrom, Font, FontState, Inputs};
use vtium_config::tui::port::{Port, Wake, drive};
use vtium_font::Cell;
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
    /// Each summary the TUI asked for: the font and the pixel height.
    summaries: Vec<(String, u32)>,
    /// Each sources query the TUI made.
    sources_asked: Vec<Vec<String>>,
    /// The helper's answer to a sources query never comes, as a slow one
    /// has not yet.
    hold_sources: bool,
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
            summaries: Vec::new(),
            sources_asked: Vec::new(),
            hold_sources: false,
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

    fn summary(&mut self, font: &str, height: u32) -> Result<Summary, String> {
        self.summaries.push((font.to_owned(), height));
        Ok(Summary {
            cell: Cell {
                width: 8,
                height: 16,
            },
            grid: Some((80, 30)),
            covered: 40,
            of: 51,
        })
    }

    fn sources(&mut self, fonts: Vec<String>) {
        self.sources_asked.push(fonts.clone());
        if self.hold_sources {
            return;
        }
        let found = fonts
            .into_iter()
            .map(|font| {
                let left = match font.as_str() {
                    "proof" => Sources::LeftOut(vec![LeftOut {
                        missing: true,
                        line: 27,
                        spelling: "font:JetBrains Mono".into(),
                        why: "no such family".into(),
                    }]),
                    "tiny5" => Sources::DrawsNothing("no source left".into()),
                    _ => Sources::LeftOut(Vec::new()),
                };
                (font, left)
            })
            .collect();
        self.pending.push_back(Wake::Sourced(found));
    }
}

const DOWN: &[u8] = b"\x1b[B";
const UP: &[u8] = b"\x1b[A";
/// Shift-Tab: from the Fonts view, the Looks view, which holds the sizes.
const BACKTAB: &[u8] = b"\x1b[Z";

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
        frits: PathBuf::from("/etc/vtium/fonts"),
        view: model::View::Fonts,
        display: Some((640, 480)),
        flow: vtium_config::tui::fixture::flow(),
        color: true,
        installed: [
            ("broken", &b"source=builtin:unifont\ncolour=red\n"[..]),
            ("minspec-16", b"source=builtin:unifont\n"),
            ("proof", b"source=font:JetBrains Mono\n"),
            ("tiny5", b"source=bdf:/x/Tiny5Mono.bdf\n"),
            ("latin1", b"source=kbd:lat1-16 # \xe9\n"),
        ]
        .into_iter()
        .map(|(n, b)| (n.to_owned(), b.to_vec()))
        .collect(),
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
    let mut c = Console::vtium(80, 25, false, "unifont").keys(&[b"o", b"i", b"\r", b"q"]);
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

/// A previewed font that leaves a grid too small for any view draws the
/// screen that says so, and keeps the keys to leave and to undo the
/// preview on it however few its rows (ADR-1201 D3).
#[test]
fn a_grid_too_small_draws_the_way_out() {
    let mut c = console().takes("tiny5", (30, 8)).keys(&[UP]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 8);
    assert!(rows[0].starts_with("vtium-config needs a"), "{screen}");
    let joined = rows.iter().map(|r| r.trim()).collect::<Vec<_>>().join(" ");
    assert!(
        joined.ends_with(vtium_config::tui::words::TOO_SMALL_KEYS),
        "{screen}"
    );
    assert!(rows.iter().all(|r| r.chars().count() <= 30), "{screen}");
}

#[test]
fn keeping_a_font_and_a_size_shows_each_changed_line_before_saving() {
    let mut c = console().keys(&[
        UP, b"\r", BACKTAB, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, b"\r", b"i",
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
    let mut c = console().keys(&[UP, b"\r", b"i", b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.saves.len(), 1);
    let file = &c.saves[0].files[0];
    assert_eq!(file.new, "# mine\nfont-name=tiny5\n");
    // after saving, leaving does not switch back
    assert_eq!(c.switches(), ["tiny5"]);
}

#[test]
fn a_failed_save_keeps_what_was_chosen_and_says_why() {
    let mut c = console().keys(&[UP, b"\r", b"i", b"\r"]);
    c.save_result = Err("sudo: a password is required".into());
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Nothing was installed: sudo: a password is required"),
        "{screen}"
    );
    assert!(
        screen.contains("+ font-name=tiny5"),
        "still in the save view: {screen}"
    );
}

/// The settings view lists every key vtium.conf takes, from the console's
/// own key table, the font keys first, each once.
#[test]
fn the_settings_view_lists_every_key_vtium_conf_takes() {
    use vtium_settings::keys::{KEYS, Scope};
    let listed: Vec<&str> = model::setting_keys().iter().map(|k| k.name).collect();
    let takes: Vec<&str> = KEYS
        .iter()
        .filter(|k| k.scope == Scope::Anywhere)
        .map(|k| k.name)
        .collect();
    assert_eq!(listed.len(), takes.len(), "{listed:?}");
    for name in &takes {
        assert_eq!(listed.iter().filter(|n| *n == name).count(), 1, "{name}");
    }
    assert_eq!(&listed[..model::FONT_KEYS.len()], &model::FONT_KEYS[..]);
}

#[test]
fn a_setting_shows_its_default_and_what_it_does() {
    let mut c = console().keys(&[b"\t", DOWN]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("font-size"), "{screen}");
    assert!(screen.contains("0 (default)"), "{screen}");
    assert!(screen.contains("The font's size in points"), "{screen}");
}

/// Enter types a value for a key that takes one, a second Enter keeps it,
/// a switch turns, a choice steps, and the save changes each line; typed
/// text takes `q` and `i` as letters.
#[test]
fn a_typed_value_a_switch_and_a_choice_are_saved_like_a_font() {
    let mut c = console().keys(&[
        b"\t", DOWN, DOWN, DOWN, b"\r", DOWN, b"\r", b"10", b"\r", DOWN, b"\r", DOWN, b"\r", b"qi",
        b"\r", b"i", b"\r", b"q",
    ]);
    assert_eq!(run(&mut c), 0);
    let plan = c.saves.first().expect("a save");
    assert_eq!(
        plan.files[0].new,
        "# mine\nfont-name=unifont\nfont-hinting=light\nfont-line-spacing=10\nno-allow-font-ops\ndisplay-dpi=qi\n"
    );
}

#[test]
fn a_new_frit_is_typed_in_its_view_and_saved_into_the_frits() {
    let mut c = console().keys(&[
        b"n",
        b"quiet",
        b"\r",
        b"builtin:unscii-8",
        b"\r",
        b"Some Family",
        b"\r",
        b"\r",
        b"\r",
        b"\r",
        b"\r",
        b"\r",
        b"q",
    ]);
    assert_eq!(run(&mut c), 0);
    let plan = c.saves.first().expect("a save");
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/etc/vtium/fonts/quiet.vtfrit")
    );
    assert_eq!(
        plan.files[0].new,
        "# quiet: made by vtium-config new\nname=quiet\nsource=builtin:unscii-8\nsource=font:Some Family\n"
    );
}

/// A new frit named as an installed one replaces it, the save showing the
/// installed text it replaces, as the TUI gathered it at start; one whose
/// installed file is not UTF-8 text is not replaced, and says why.
#[test]
fn a_new_frit_replaces_the_installed_one_of_its_name() {
    let to_new = [b"n".as_slice()];
    let mut keys: Vec<&[u8]> = to_new.to_vec();
    keys.extend([b"proof".as_slice(), b"\r", b"builtin:unifont"]);
    // five fields to the add row, the add, and the save
    keys.extend([b"\r".as_slice(); 7]);
    keys.push(b"q");
    let mut c = console().keys(&keys);
    run(&mut c);
    let plan = c.saves.first().expect("a save");
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/etc/vtium/fonts/proof.vtfrit")
    );
    assert_eq!(
        plan.files[0].old.as_deref(),
        Some("source=font:JetBrains Mono\n")
    );
    let mut keys: Vec<&[u8]> = to_new.to_vec();
    keys.extend([b"latin1".as_slice(), b"\r", b"builtin:unifont"]);
    keys.extend([b"\r".as_slice(); 6]);
    let mut c = console().keys(&keys);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("the installed latin1.vtfrit is not UTF-8 text"),
        "{screen}"
    );
    assert!(c.saves.is_empty());
}

/// `r` in the fonts view removes the highlighted frit when the user
/// saves: the font list marks it, the save view lists its file, and the
/// save's plan removes it with the bytes the TUI read at start, as
/// `remove-frit`'s draft does. A second `r` keeps it.
#[test]
fn r_removes_the_highlighted_frit_when_the_user_saves() {
    // unifont is highlighted at start; Up reaches tiny5
    let mut c = console().keys(&[UP, b"r", b"i"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("/etc/vtium/fonts/tiny5.vtfrit"), "{screen}");
    assert!(screen.contains("tiny5.vtfrit.bak"), "{screen}");
    let mut c = console().keys(&[UP, b"r", b"i", b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    let plan = c.saves.first().expect("a save");
    assert!(plan.files.is_empty(), "{plan:?}");
    assert_eq!(
        plan.removals,
        [vtium_config::draft::Removal {
            path: PathBuf::from("/etc/vtium/fonts/tiny5.vtfrit"),
            old: b"source=bdf:/x/Tiny5Mono.bdf\n".to_vec(),
        }]
    );
    // marked in the list, and kept by a second r
    let mut c = console().keys(&[UP, b"r"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("to remove"), "{screen}");
    let mut c = console().keys(&[UP, b"r", b"r", b"i", b"\r", b"q"]);
    run(&mut c);
    assert!(c.saves.is_empty());
}

/// The remove key refuses what `remove-frit` refuses, saying why, and
/// adds nothing to the save: a built-in font, the frit vtium.conf's
/// `font-name` names, as it is or as the save leaves it, and the frit a
/// head's profile names.
#[test]
fn r_refuses_what_remove_frit_refuses() {
    for (keys, says) in [
        // unifont, highlighted at start
        (vec![b"r".as_slice()], "built-in font"),
        // proof is head desk's profile
        (vec![UP, UP, b"r"], "head desk's profile"),
        // minspec-16 kept as font-name, then r
        (vec![UP, UP, UP, b"\r", b"r"], "font-name"),
    ] {
        let mut all = keys.clone();
        all.extend([b"i".as_slice(), b"\r", b"q"]);
        let mut c = console().keys(&keys);
        run(&mut c);
        let screen = c.seen.clone().expect("the screen");
        let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(text.contains("Not removed:"), "{says}: {screen}");
        assert!(text.contains(says), "{says}: {screen}");
        let mut c = console().keys(&all);
        run(&mut c);
        assert!(
            c.saves.iter().all(|p| p.removals.is_empty()),
            "{says}: {:?}",
            c.saves
        );
    }
    // a frit marked for removal and then kept as font-name is not removed:
    // the save says why and saves nothing
    let mut c = console().keys(&[UP, b"r", b"\r", b"i", b"\r", b"q"]);
    run(&mut c);
    assert!(c.saves.is_empty(), "{:?}", c.saves);
}

/// A save that changes font-frits and also writes or removes a frit is
/// refused: the frits would go to the directory font-frits named before
/// the save, so the user saves the new directory first. A save that
/// changes only one of the two is planned.
#[test]
fn a_save_that_moves_the_frits_and_changes_a_frit_is_refused() {
    use vtium_config::tui::model::{App, Change};
    let (mut app, _) = App::new(inputs(), 80, 25);
    app.pending
        .set("font-frits", Change::Set("/srv/frits".to_owned()));
    assert!(app.plan().is_ok());
    app.pending.removals.push("tiny5".to_owned());
    let why = app.plan().expect_err("a removal and a new directory");
    assert!(why.contains("font-frits"), "{why}");
    app.pending.removals.clear();
    app.pending
        .frits
        .push(("made".to_owned(), "source=builtin:unifont\n".to_owned()));
    let why = app.plan().expect_err("a new frit and a new directory");
    assert!(why.contains("font-frits"), "{why}");
    // a save that leaves font-frits alone is no move
    let (mut app, _) = App::new(inputs(), 80, 25);
    app.pending.removals.push("tiny5".to_owned());
    app.pending
        .set("font-name", Change::Set("proof".to_owned()));
    assert!(app.plan().is_ok());
    // nor is one that sets it to the value vtium.conf already names
    let mut named = inputs();
    named.conf = Some("# mine\nfont-name=unifont\nfont-frits=/srv/frits\n".into());
    let (mut app, _) = App::new(named, 80, 25);
    app.pending
        .set("font-frits", Change::Set("/srv/frits".to_owned()));
    app.pending.removals.push("tiny5".to_owned());
    assert!(app.plan().is_ok(), "{:?}", app.plan());
}

#[test]
fn a_new_frit_the_reader_refuses_stays_in_its_view_and_says_why() {
    let mut c = console().keys(&[b"n", b"\r", b"\r", b"\r", b"\r", b"\r", b"\r", b"\r"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("The frit does not read"), "{screen}");
    assert!(screen.contains("Source 1"), "{screen}");
    assert!(c.saves.is_empty());
}

#[test]
fn new_opens_the_tui_at_its_new_frit_view() {
    let mut inputs = inputs();
    inputs.view = model::View::New;
    let mut c = console();
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("Add the frit to the install"), "{screen}");
}

#[test]
fn nothing_kept_means_nothing_to_save() {
    let mut c = console().keys(&[b"i", b"\r", b"q"]);
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

/// The screen the script `keys` reaches at `cols` by `rows`, from the
/// Fonts view.
fn reach(cols: usize, rows: usize, keys: &[&[u8]]) -> String {
    reach_from(model::View::Fonts, cols, rows, keys)
}

/// The screen the script `keys` reaches at `cols` by `rows`, from `view`.
fn reach_from(view: model::View, cols: usize, rows: usize, keys: &[&[u8]]) -> String {
    let grid = (cols, rows);
    let mut c = Console::vtium(cols, rows, true, "unifont")
        .takes("minspec-16", grid)
        .takes("proof", grid)
        .takes("tiny5", grid)
        .takes("unifont", grid)
        .takes("unscii-8", grid)
        .keys(keys);
    let mut inputs = inputs();
    inputs.view = view;
    drive(&mut c, inputs).expect("the loop");
    c.seen.clone().expect("the screen")
}

/// Down from the first display to the keyboard: the Flow view's input
/// devices are its sixth column.
const TO_KEYBOARD: [&[u8]; 6] = [b"\x1b[C", b"\x1b[C", b"\x1b[C", b"\x1b[C", b"\x1b[C", DOWN];

#[test]
fn each_view_at_80x25_and_100x30() {
    use model::View::{Flow, Fonts};
    // f follows the keyboard's line to rule 5, and Enter opens it
    let card: Vec<&[u8]> = [&TO_KEYBOARD[..], &[b"f", b"\r"]].concat();
    let route: Vec<&[u8]> = [&TO_KEYBOARD[..], &[b"?"]].concat();
    let scripts: [(&str, model::View, &[&[u8]]); 9] = [
        ("flow", Flow, &[]),
        ("card", Flow, &card),
        ("route", Flow, &route),
        ("looks", Fonts, &[BACKTAB]),
        ("fonts", Fonts, &[]),
        ("settings", Fonts, &[b"\t"]),
        ("help", Fonts, &[b"\t", b"\t"]),
        ("save", Fonts, &[UP, b"\r", b"i"]),
        ("new", Fonts, &[b"n"]),
    ];
    for (cols, rows) in [(80, 25), (100, 30)] {
        for (name, view, keys) in scripts {
            let screen = reach_from(view, cols, rows, keys);
            let lines: Vec<&str> = screen.lines().collect();
            assert_eq!(lines.len(), rows, "{name} at {cols}x{rows}");
            assert!(lines.iter().all(|l| l.chars().count() <= cols), "{name}");
            assert!(lines[0].contains(" Flow "), "{name}: {screen}");
            // the last row holds the key that leaves, or the route card's
            // that goes back
            let last = lines[rows - 1];
            assert!(
                last.contains("leave") || last.contains("back"),
                "{name}: {screen}"
            );
            golden(&format!("{name}-{cols}x{rows}"), &screen);
        }
    }
}

#[test]
fn a_console_that_never_answers_the_give_back_is_left_after_the_wait() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    let mut started = Vec::new();
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font("unifont".into()),
        Reply::Attributes,
    ] {
        started.extend(app.event(Event::Input(Input::Reply(r))));
    }
    // once the terminal is known, the helper is asked which sources each
    // frit that reads lacks
    let frits = ["minspec-16", "proof", "tiny5"].map(String::from).to_vec();
    assert_eq!(started, [Effect::Sources(frits)]);
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
    let mut c = console().keys(&[UP, b"\r", b"i", b"\r"]);
    c.save_result = Err(
        "the command failed, and the draft stays in /d.\nvtium-config: the console's check refused the draft:\n/x/vtium.conf:2: font-size=\"lots\" is not a whole number, ignored".into(),
    );
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Nothing was installed: the command failed, and the draft stays in /d."),
        "{screen}"
    );
    assert!(screen.contains("The command said:"), "{screen}");
    assert!(
        screen.contains("font-size=\"lots\" is not a whole number"),
        "{screen}"
    );
}

/// The bytes that show the stand-in's picture at the Looks view's place,
/// below its two rows of sentence and the blank row after them.
fn shown() -> Vec<u8> {
    let mut b = b"\x1b[7;26H".to_vec();
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
    let mut c = console().keys(&[BACKTAB, DOWN, DOWN, b"\t", b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    // 6 pt and 7 pt at 96 dpi, unifont highlighted in the font view, in
    // the area beside the list, 55 columns by 14 rows of 8x16 cells: down
    // to the room the keys and the longest message leave
    let area = (55 * 8, 14 * 16);
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

/// Where the console cannot be switched live, the font view shows each
/// highlighted font as a picture, at 12 points where nothing names a size,
/// below the list; where it can, the console itself shows the font.
#[test]
fn a_font_the_console_cannot_switch_to_is_shown_as_a_picture() {
    let mut c = Console::vtium(80, 25, false, "unifont").keys(&[UP, UP, UP, UP, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    let fonts: Vec<(&str, u32)> = c
        .pictures
        .iter()
        .map(|(f, h, _)| (f.as_str(), *h))
        .collect();
    assert_eq!(
        fonts,
        [
            ("unifont", 16),
            ("tiny5", 16),
            ("proof", 16),
            ("minspec-16", 16)
        ]
    );
    // below the six fonts' rows: row 11 to the room above the footer
    let (_, _, (w, h)) = c.pictures[0];
    assert_eq!((w, h), (79 * 8, 10 * 16));
    // the broken frit, which does not read, gets none: each picture is
    // deleted as the next font is highlighted
    assert_eq!(count(&c.written, &kitty::delete(kitty::IMAGE_ID)), 4);
    let mut live = console().keys(&[UP, b"q"]);
    live.graphics = true;
    run(&mut live);
    assert!(live.pictures.is_empty(), "{:?}", live.pictures);
}

/// Where the terminal draws no pictures, a size shows what it gives: its
/// cell, its grid and how much of the specimen the font draws itself.
#[test]
fn without_pictures_a_size_shows_its_cell_grid_and_coverage() {
    let mut c = console().keys(&[BACKTAB, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains(
            "At this size the cell is 8x16 pixels, the display holds 80x30 cells, and the font draws 40 of the specimen's 51 characters itself."
        ),
        "{screen}"
    );
    // one summary a size, at 96 dpi: 6 to 12 pt and the highlighted font
    let heights: Vec<u32> = c.summaries.iter().map(|(_, h)| *h).collect();
    assert_eq!(heights, [8, 9, 11, 12, 13, 15, 16]);
    assert!(c.summaries.iter().all(|(f, _)| f == "unifont"));
    assert!(c.pictures.is_empty());
}

/// The sources query does not hold the keys up: while the helper has not
/// answered, a key switches the console and a signal leaves (ADR-1201 D3).
#[test]
fn keys_reach_the_tui_while_the_helper_finds_the_sources() {
    let mut c = console().keys(&[UP]);
    c.hold_sources = true;
    // the script runs out with the answer still to come, and SIGTERM leaves
    assert_eq!(run(&mut c), 128 + 15);
    assert_eq!(
        c.sources_asked,
        [["minspec-16", "proof", "tiny5"].map(String::from).to_vec()]
    );
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("proof      reads\n"), "{screen}");
}

/// Each frit that reads is noted with the sources the helper says this
/// machine lacks, or that none of its sources is here.
#[test]
fn the_font_list_names_the_sources_a_frit_lacks() {
    let mut c = console().keys(&[]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("proof      reads, 1 source missing"),
        "{screen}"
    );
    assert!(
        screen.contains("tiny5      no source on this machine"),
        "{screen}"
    );
    assert!(screen.contains("minspec-16 reads\n"), "{screen}");
}

#[test]
fn leaving_with_a_picture_deletes_it_before_the_font_comes_back() {
    let mut c = console().keys(&[UP, BACKTAB, DOWN, b"q"]);
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
    let mut c = console().keys(&[BACKTAB, DOWN, UP]);
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
    let mut c = console().keys(&[BACKTAB, DOWN, DOWN]);
    run(&mut c);
    assert!(c.pictures.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("draws no pictures"), "{screen}");
}

#[test]
fn the_picture_never_reaches_the_footer_and_never_passes_the_cap() {
    // 80x25 at 8x16: rows 7 to 20 are the area, above the longest message
    let mut c = console().keys(&[BACKTAB, DOWN]);
    c.graphics = true;
    run(&mut c);
    let (_, _, (w, h)) = c.pictures[0].clone();
    assert!(h <= (25 - 8) * 16 && w <= (80 - 25) * 8, "{w}x{h}");
    // a terminal of very large cells is capped
    let mut c = console().keys(&[BACKTAB, DOWN]);
    c.graphics = true;
    c.cell_px = Some((200, 400));
    run(&mut c);
    let (_, _, max) = c.pictures[0].clone();
    assert_eq!(max, vtium_config::specimen::PICTURE_MAX);
}

#[test]
fn display_dpi_outside_the_console_s_bounds_is_not_used() {
    use vtium_config::tui::density;
    let none = &[];
    assert_eq!(
        density(Some("display-dpi=150\n"), none),
        (150, DpiFrom::Conf)
    );
    assert_eq!(
        density(Some("display-dpi=1000\n"), none),
        (1000, DpiFrom::Conf)
    );
    assert_eq!(
        density(Some("display-dpi=1001\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(
        density(Some("display-dpi=19\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(
        density(Some("display-dpi=lots\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(density(None, none), (96, DpiFrom::Assumed));
    // a display's EDID comes after display-dpi and before the assumption:
    // the first display that states its size
    use vtium_config::display::Display;
    let displays = [
        Display {
            pixels: (1024, 768),
            mm: None,
        },
        Display {
            pixels: (2560, 1440),
            mm: Some((600, 340)),
        },
    ];
    assert_eq!(density(None, &displays), (108, DpiFrom::Edid));
    assert_eq!(
        density(Some("display-dpi=150\n"), &displays),
        (150, DpiFrom::Conf)
    );
}

#[test]
fn after_saving_a_font_leaving_puts_the_console_in_the_saved_font() {
    // keep tiny5, move on to proof, save, leave
    let mut c = console().keys(&[UP, b"\r", UP, b"i", b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.saves[0].files[0].new, "# mine\nfont-name=tiny5\n");
    assert_eq!(c.switches(), ["tiny5", "proof", "tiny5"]);
}

/// Below 80 by 25 cells no view is drawn: the screen names the size the
/// TUI needs and this one's, the commands, which work at any size, and the
/// keys; `u` puts back the font a preview changed, and the views come back
/// with the grid.
#[test]
fn below_the_minimum_the_screen_says_so_and_u_undoes_the_preview() {
    use vtium_config::tui::words;
    for grid in [(79, 30), (80, 24)] {
        let mut c = Console::vtium(80, 25, true, "unifont")
            .takes("unifont", (80, 25))
            .takes("tiny5", grid)
            .keys(&[UP]);
        run(&mut c);
        let screen = c.seen.clone().expect("the screen");
        let joined = screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
        assert!(
            joined.contains(&words::too_small(grid.0, grid.1)),
            "{grid:?}: {screen}"
        );
        assert!(joined.contains("vtium-config fonts"), "{screen}");
        assert!(joined.contains("vtium-config config"), "{screen}");
        assert!(joined.contains(words::TOO_SMALL_KEYS), "{screen}");
        // keys that would move are ignored below the minimum; u gives the
        // console its start font back, and the views come back
        let mut c = Console::vtium(80, 25, true, "unifont")
            .takes("unifont", (80, 25))
            .takes("tiny5", grid)
            .takes("proof", (80, 25))
            .keys(&[UP, UP, b"u"]);
        run(&mut c);
        assert_eq!(c.switches(), ["tiny5", "unifont"], "{grid:?}");
        let screen = c.seen.clone().expect("the screen");
        assert!(screen.contains("Fonts"), "{screen}");
        assert!(!screen.contains("vtium-config needs"), "{screen}");
    }
}

#[test]
fn at_80x25_no_key_or_sentence_is_cut() {
    use vtium_config::tui::words;
    // the keys that reach a view, its key line, and its sentences
    type Case<'a> = (&'a [&'a [u8]], &'a str, &'a [&'a str]);
    let views: [Case; 5] = [
        (&[], words::KEYS_FONTS, &[]),
        (&[BACKTAB], words::KEYS_LOOKS, &[words::NO_PICTURES]),
        (&[b"\t"], words::KEYS_SETTINGS, &[words::SETTINGS_HINT]),
        (&[b"\t", b"\t"], words::KEYS_HELP, &[]),
        (&[UP, b"\r", b"i"], words::KEYS_SAVE, &[words::SAVE_RUNS]),
    ];
    for (keys, key_line, sentences) in views {
        let screen = reach(80, 25, keys);
        let rows: Vec<&str> = screen.lines().collect();
        assert_eq!(rows.len(), 25);
        for item in key_line.split("  ") {
            assert!(
                rows.iter().any(|r| r.contains(item)),
                "{item:?} whole:\n{screen}"
            );
        }
        // a sentence wrapped by words reads whole once its rows are joined
        let joined = rows.iter().map(|r| r.trim()).collect::<Vec<_>>().join(" ");
        for sentence in sentences {
            assert!(joined.contains(sentence), "{sentence:?}:\n{screen}");
        }
        // the views and the actions fit the tab bar
        for name in ["Flow", "Looks", "Fonts", "Settings", "Help"] {
            assert!(rows[0].contains(name), "{name}: {:?}", rows[0]);
        }
        for (_, short) in words::ACTIONS {
            assert!(rows[0].contains(short), "{short}: {:?}", rows[0]);
        }
    }
}

#[test]
fn an_edit_vtium_conf_cannot_take_is_said_in_words() {
    use vtium_config::tui::words::conf_edit;
    use vtium_settings::ConfEdit;
    let key = conf_edit(&ConfEdit::BadKey("Font".into()));
    assert!(key.contains("Font") && !key.contains("BadKey"), "{key}");
    let value = conf_edit(&ConfEdit::BadValue {
        key: "font-name".into(),
        value: "a#b".into(),
    });
    assert!(
        value.contains("font-name") && value.contains("a#b") && !value.contains("BadValue"),
        "{value}"
    );
}

#[test]
fn at_80x25_the_size_list_scrolls_and_its_sentence_shows_whole_or_not_at_all() {
    use vtium_config::tui::words;
    let joined = |screen: &str| screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    let top = reach(80, 25, &[BACKTAB]);
    assert!(joined(&top).contains(words::NO_PICTURES), "{top}");
    assert!(
        !top.contains("32 pt"),
        "the list is longer than its room: {top}"
    );
    // the last size scrolls into sight, and the sentence stays whole
    let bottom = reach(80, 25, &[BACKTAB, b"\x1b[4~"]);
    assert!(bottom.contains("> 32 pt"), "{bottom}");
    let first = words::NO_PICTURES.split(", ").next().unwrap_or("");
    assert_eq!(
        joined(&bottom).contains(first),
        joined(&bottom).contains(words::NO_PICTURES),
        "whole or not at all:\n{bottom}"
    );
}

#[test]
fn a_refused_frit_s_long_reason_wraps_by_words() {
    let reason = "line 2: unknown key colour, which a frit spells color, as vtium.conf spells it in every key";
    let mut inputs = inputs();
    inputs.fonts[0] = font("broken", FontState::Refused(reason.into()));
    let mut c = console();
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    let joined = screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    assert!(
        joined.contains(&format!("does not read: {reason}")),
        "{screen}"
    );
    assert!(screen.lines().all(|l| l.chars().count() <= 80));
    // the reason goes on below its name's row, under its column
    let continued: Vec<&str> = screen
        .lines()
        .filter(|l| l.starts_with("              ") && !l.trim().is_empty())
        .collect();
    assert_eq!(continued.len(), 1, "{screen}");
}

// The Flow view (ADR-1403 D7), on the flow of the record's figure.

mod flow_view {
    use super::*;
    use vtium_config::tui::flow::{Item, New, Special};
    use vtium_config::tui::input::{Button, Input, Key, Mouse};
    use vtium_config::tui::model::{App, Effect, Event, MOUSE_OFF, MOUSE_ON, View};
    use vtium_config::tui::view;
    use vtium_config::tui::words;

    fn app(cols: usize, rows: usize) -> App {
        let mut i = inputs();
        i.view = View::Flow;
        App::new(i, cols, rows).0
    }

    fn press(app: &mut App, key: Key) -> Vec<Effect> {
        app.event(Event::Input(Input::Key(key)))
    }

    fn text(app: &App) -> String {
        view::as_text(&view::screen(app))
    }

    fn writes(effects: &[Effect], bytes: &[u8]) -> bool {
        effects.iter().any(|e| *e == Effect::Write(bytes.to_vec()))
    }

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

    #[test]
    fn the_arrows_move_by_column_to_the_nearest_row_and_within_a_column() {
        let mut a = app(80, 25);
        assert_eq!(a.flow_at, Item::Display(0));
        let mut path = Vec::new();
        for _ in 0..6 {
            press(&mut a, Key::Right);
            path.push(a.flow_at);
        }
        assert_eq!(
            path,
            [
                Item::Rule(0),
                Item::Head(0),
                Item::HeadLook(0),
                Item::Rule(3),
                Item::Input(0),
                // the arrows stop at the last column
                Item::Input(0),
            ]
        );
        assert_eq!(a.view, View::Flow);
        press(&mut a, Key::Down);
        assert_eq!(a.flow_at, Item::Input(1));
        // the keyboard's row is level with rule 6, below rules 4 and 5 that
        // each take two rows
        press(&mut a, Key::Left);
        assert_eq!(a.flow_at, Item::Rule(5));
        press(&mut a, Key::Up);
        assert_eq!(a.flow_at, Item::Rule(4));
        press(&mut a, Key::End);
        assert_eq!(a.flow_at, Item::New(New::InputRule));
        // the heads' column ends in the targets that are not heads
        let mut a = app(80, 25);
        press(&mut a, Key::Right);
        press(&mut a, Key::Right);
        press(&mut a, Key::End);
        assert_eq!(a.flow_at, Item::Policy);
        press(&mut a, Key::Up);
        assert_eq!(a.flow_at, Item::Special(Special::Off));
        // up from the first display is the base card
        let mut a = app(80, 25);
        press(&mut a, Key::Up);
        assert_eq!(a.flow_at, Item::Base);
        press(&mut a, Key::Down);
        assert_eq!(a.flow_at, Item::Display(0));
    }

    #[test]
    fn tab_past_the_last_column_and_shift_tab_before_the_first_go_to_the_next_views() {
        let mut a = app(80, 25);
        for _ in 0..5 {
            press(&mut a, Key::Tab);
        }
        assert_eq!((a.view, a.flow_at), (View::Flow, Item::Input(0)));
        let out = press(&mut a, Key::Tab);
        assert_eq!(a.view, View::Looks);
        assert!(writes(&out, MOUSE_OFF), "{out:?}");
        // and back: Shift-Tab from Looks enters the flow at its last column
        let out = press(&mut a, Key::BackTab);
        assert_eq!((a.view, a.flow_at), (View::Flow, Item::Input(0)));
        assert!(writes(&out, MOUSE_ON), "{out:?}");
        let mut a = app(80, 25);
        press(&mut a, Key::BackTab);
        assert_eq!(a.view, View::Help);
        press(&mut a, Key::Tab);
        assert_eq!((a.view, a.flow_at), (View::Flow, Item::Display(0)));
    }

    #[test]
    fn enter_opens_the_selected_item_s_card_and_esc_closes_it() {
        let mut a = app(80, 25);
        a.flow_at = Item::Rule(4);
        assert!(!text(&a).contains("rule 5, line 23"));
        press(&mut a, Key::Enter);
        assert!(a.opened);
        let screen = text(&a);
        assert!(
            screen.contains("rule 5, line 23   input class:keyboard -> roaming"),
            "{screen}"
        );
        assert!(
            screen.contains("sends to   desk  panel  roaming  ignore"),
            "{screen}"
        );
        // the card follows the selection
        press(&mut a, Key::Down);
        assert!(text(&a).contains("rule 6, line 24"), "{}", text(&a));
        press(&mut a, Key::Escape);
        assert!(!a.opened);
        assert!(!text(&a).contains("rule 6, line 24"));
    }

    #[test]
    fn space_tags_any_item_the_status_line_lists_the_tags_and_esc_clears_them() {
        let mut a = app(100, 30);
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Char(' '));
        a.flow_at = Item::Head(0);
        press(&mut a, Key::Char(' '));
        a.flow_at = Item::HeadLook(0);
        press(&mut a, Key::Char(' '));
        assert_eq!(a.tags, [Item::Input(1), Item::Head(0), Item::HeadLook(0)]);
        let screen = text(&a);
        let status = screen.lines().nth(1).unwrap_or_default();
        assert!(status.contains("tagged: keyboard, desk, big"), "{status}");
        assert!(screen.contains("◆ keyboard"), "{screen}");
        // Space again untags; the base card and the new items take no tag
        press(&mut a, Key::Char(' '));
        assert_eq!(a.tags, [Item::Input(1), Item::Head(0)]);
        a.flow_at = Item::Base;
        press(&mut a, Key::Char(' '));
        a.flow_at = Item::New(New::Head);
        press(&mut a, Key::Char(' '));
        assert_eq!(a.tags.len(), 2);
        press(&mut a, Key::Escape);
        assert!(a.tags.is_empty());
        assert!(!text(&a).contains("tagged:"));
    }

    #[test]
    fn f_follows_a_line_toward_the_heads_and_steps_through_several() {
        let mut a = app(80, 25);
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Char('f'));
        assert_eq!(a.flow_at, Item::Rule(4));
        press(&mut a, Key::Char('f'));
        assert_eq!(a.flow_at, Item::Special(Special::Roaming));
        // desk has two display rules' lines: f steps through them
        a.flow_at = Item::Head(0);
        let mut seen = Vec::new();
        for _ in 0..3 {
            press(&mut a, Key::Char('f'));
            seen.push(a.flow_at);
        }
        assert_eq!(seen, [Item::Rule(0), Item::Rule(2), Item::Rule(0)]);
        a.flow_at = Item::Base;
        press(&mut a, Key::Char('f'));
        assert_eq!(a.flow_at, Item::Base);
        assert_eq!(a.message.as_deref(), Some(words::NOTHING_TO_FOLLOW));
    }

    #[test]
    fn the_route_card_shows_every_route_and_enter_opens_an_item_on_it() {
        let mut a = app(80, 25);
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Char('?'));
        // the cursor starts on the selected item's line in the first route
        assert_eq!(a.route, Some(6));
        let screen = text(&a);
        assert!(
            screen.contains("Route card: keyboard, usb-0:3.3"),
            "{screen}"
        );
        assert!(
            screen.contains("route 2 of 2, when panel is active"),
            "{screen}"
        );
        assert!(
            !screen.contains("DISPLAYS"),
            "the card takes the columns' place"
        );
        // q goes back to the flow and does not leave
        let out = press(&mut a, Key::Char('q'));
        assert!(out.is_empty(), "{out:?}");
        assert_eq!((a.route, a.leaving), (None, None));
        // Up to the input rule, and Enter opens it
        press(&mut a, Key::Char('?'));
        press(&mut a, Key::Up);
        assert_eq!(a.route, Some(5));
        press(&mut a, Key::Enter);
        assert_eq!((a.route, a.flow_at, a.opened), (None, Item::Rule(4), true));
        // a look no head uses is part of no route, and says so
        a.flow_at = Item::Look(2);
        press(&mut a, Key::Char('?'));
        assert!(text(&a).contains(words::LOOK_NOT_USED), "{}", text(&a));
        press(&mut a, Key::Escape);
        assert_eq!(a.route, None);
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
    fn a_click_selects_a_second_click_opens_and_a_view_s_name_switches_to_it() {
        let mut a = app(80, 25);
        // the head's block, right of the arrow that reaches it
        let (col, row) = cell(&a, "► panel");
        let panel = (col + 2, row);
        click(&mut a, panel);
        assert_eq!((a.flow_at, a.opened), (Item::Head(1), false));
        click(&mut a, panel);
        assert!(a.opened);
        assert!(text(&a).contains("head panel · 2 consoles"), "{}", text(&a));
        // the base card's row selects the base card
        click(&mut a, (5, 2));
        assert_eq!(a.flow_at, Item::Base);
        // the wheel moves as Down does
        press(
            &mut a,
            Key::Mouse(Mouse {
                button: Button::WheelDown,
                col: 0,
                row: 0,
            }),
        );
        assert_eq!(a.flow_at, Item::Display(0));
        let looks = cell(&a, "Looks");
        let out = click(&mut a, looks);
        assert_eq!(a.view, View::Looks);
        assert!(writes(&out, MOUSE_OFF), "{out:?}");
        // the other views asked for no mouse, and a click there does nothing
        click(&mut a, (2, 0));
        assert_eq!(a.view, View::Looks);
    }

    #[test]
    fn a_click_reaches_the_tui_through_its_decoder() {
        // SGR mouse, mode 1006: a press on the last cell of desk's block,
        // row 5 column 39 counted from 1, the cell before the look's
        let mut c = console().keys(&[b"\x1b[<0;39;5M", b"\x1b[<0;39;5m", b"\r"]);
        let mut i = inputs();
        i.view = View::Flow;
        drive(&mut c, i).expect("the loop");
        let written = String::from_utf8_lossy(&c.written).into_owned();
        assert!(written.contains("\x1b[?1000h\x1b[?1006h"), "turned on");
        let screen = c.seen.clone().expect("the screen");
        assert!(
            screen.contains("head desk · 1 console · the active head"),
            "{screen}"
        );
    }

    #[test]
    fn the_action_keys_say_what_is_not_built_and_i_opens_the_install_view() {
        let mut a = app(80, 25);
        for (key, what) in [
            ('c', words::CONNECT),
            ('d', words::READ_DEVICES),
            ('v', words::VIEW_DRAFT),
            ('r', words::REVERT),
            ('o', words::RESTORE),
            ('n', words::NEW_CONFIGURATION),
            ('x', words::DELETE_DRAFT),
        ] {
            press(&mut a, Key::Char(key));
            assert_eq!(a.message, Some(words::not_built(what)), "{key}");
            assert_eq!(a.view, View::Flow);
        }
        let out = press(&mut a, Key::Char('i'));
        assert_eq!(a.view, View::Save);
        assert!(writes(&out, MOUSE_OFF));
        let out = press(&mut a, Key::Escape);
        assert_eq!(a.view, View::Flow);
        assert!(writes(&out, MOUSE_ON));
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
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Enter);
        screens.push(view::draw(&a));
        press(&mut a, Key::Char('?'));
        screens.push(view::draw(&a));
        press(&mut a, Key::Escape);
        for _ in 0..4 {
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
        // a display is black on bright cyan, an input device black on
        // bright green, a head black on bright yellow, a look black on
        // bright magenta, the targets that are not heads black on grey
        let flow = sgr(&screens[0]);
        for block in [[30, 106], [30, 102], [30, 105], [30, 100]] {
            assert!(
                flow.iter().any(|p| p.ends_with(&block)),
                "{block:?}: {flow:?}"
            );
        }
        assert!(flow.iter().any(|p| p == &[1, 30, 103]), "a head's name");
        // the rules in their columns' colors: cyan and green
        assert!(flow.iter().any(|p| p == &[36]) && flow.iter().any(|p| p == &[32]));
    }

    #[test]
    fn without_color_the_view_draws_no_color_and_brackets_each_block() {
        let mut i = inputs();
        i.view = View::Flow;
        i.color = false;
        let (mut a, _) = App::new(i, 80, 25);
        let bytes = view::draw(&a);
        for params in sgr(&bytes) {
            assert!(params.iter().all(|p| [0, 1, 7].contains(p)), "{params:?}");
        }
        let screen = text(&a);
        assert!(screen.contains("[DP-8     ]"), "{screen}");
        assert!(screen.contains("[not found ]"), "{screen}");
        assert!(screen.contains("[base"), "{screen}");
        // the selection is still reverse video
        assert!(sgr(&bytes).iter().any(|p| p.contains(&7)));
        // and every state still has its words
        assert!(screen.contains("not found"), "{screen}");
        a.flow_at = Item::Rule(2);
        assert!(text(&a).contains("matches no"), "{}", text(&a));
    }

    #[test]
    fn a_rule_to_a_head_that_does_not_exist_is_red_and_says_so() {
        let mut i = inputs();
        i.view = View::Flow;
        i.flow.rules[0].target = "tv".into();
        let (mut a, _) = App::new(i, 100, 30);
        let screen = text(&a);
        assert!(screen.contains("! 1 edid:"), "{screen}");
        assert!(screen.contains("sends to a head"), "{screen}");
        let bytes = String::from_utf8_lossy(&view::draw(&a)).into_owned();
        assert!(bytes.contains("\x1b[91m! 1 edid:"), "bright red");
        // no line runs from it, and its route card says what is broken
        a.flow_at = Item::Rule(0);
        press(&mut a, Key::Char('?'));
        assert!(
            text(&a).contains(
                "Rule 1 sends edid:AAA:0x0001:LARGE00001 to tv, a head that does not exist."
            ),
            "{}",
            text(&a)
        );
    }

    #[test]
    fn below_80x25_the_flow_view_draws_the_way_out() {
        for (cols, rows) in [(79, 25), (80, 24)] {
            let mut a = app(cols, rows);
            let screen = text(&a);
            assert!(
                screen.starts_with("vtium-config needs a terminal"),
                "{screen}"
            );
            // the flow's keys do nothing there; q still leaves
            press(&mut a, Key::Right);
            assert_eq!(a.flow_at, Item::Display(0));
            let out = press(&mut a, Key::Char('q'));
            assert_eq!(out.last(), Some(&Effect::Exit(0)));
        }
    }

    #[test]
    fn a_flow_not_read_says_so_in_place_of_an_empty_flow() {
        let mut i = inputs();
        i.view = View::Flow;
        i.flow = vtium_config::tui::flow::Flow::default();
        let (a, _) = App::new(i, 80, 25);
        let screen = text(&a);
        let joined = screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
        assert!(joined.contains(words::FLOW_NOT_READ), "{screen}");
        assert!(screen.contains("no draft"), "{screen}");
    }

    #[test]
    fn the_help_view_draws_the_keys_from_the_catalog_and_about_from_its_source() {
        let mut a = app(80, 25);
        a.flow_at = Item::Display(0);
        press(&mut a, Key::BackTab);
        assert_eq!(a.view, View::Help);
        let joined = |a: &App| text(a).lines().map(str::trim).collect::<Vec<_>>().join(" ");
        assert!(
            joined(&a).contains("Flow draws the configuration"),
            "{}",
            text(&a)
        );
        press(&mut a, Key::End);
        let end = joined(&a);
        // a command's line wider than the screen goes on under it
        assert!(
            end.contains("report [--save[=PATH]] [--lines N] [--previous-boot]"),
            "{end}"
        );
        assert!(
            end.contains("gather what a bug report needs into one document, changing nothing"),
            "{end}"
        );
        assert!(text(&a).lines().all(|l| l.chars().count() <= 80));
        press(&mut a, Key::Right);
        assert!(a.about);
        assert!(
            joined(&a).contains("vtium-config sets up the vtium console"),
            "{}",
            text(&a)
        );
    }
}
