//! The rebuilt TUI with stand-ins around it, so the walk drives it with no
//! terminal, no pty and no files. [`Sim`] holds vtium-config-tui's [`App`]
//! with the six tab crates' tabs, a stand-in for the terminal it runs in,
//! and a stand-in for the machine the loop reads and saves through. Each
//! key goes to [`App::event`], and every effect the key causes is answered
//! at once, the way the loop in vtium-config-tui's `port` module answers
//! it. So the state after [`Sim::press`] is the state the TUI rests in
//! once the key's work is done.
//!
//! The App holds its tabs as trait objects and its document over a desk
//! kept in memory, so it cannot be cloned. A walk reaches a state again by
//! starting a new TUI and pressing the keys that reach it.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use ratatui_core::backend::TestBackend;
use ratatui_core::buffer::Buffer;
use ratatui_core::terminal::Terminal;
use vtium_config_core::apply::Placement;
use vtium_config_core::document::{Content, Installed};
use vtium_config_core::draft::Plan;
use vtium_config_core::fixture;
use vtium_config_core::flowmodel::Present;
use vtium_config_core::ui::{Effect, Op, Shared};
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Event, Size};
use vtium_config_tui::editor::Edited;
use vtium_config_tui::live::machine::Ready;
use vtium_config_tui::save::{Came, Saved};
use vtium_config_widgets::keys::{Button, Key, Mouse, Reply};

/// The kinds of terminal the TUI tells apart (ADR-1201 D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Term {
    /// A vtium console that switches fonts: OSC 50 works.
    Vtium,
    /// A vtium console with `allow-font-ops` off, which leaves OSC 50's
    /// query unanswered.
    OpsOff,
    /// Any other terminal, which answers DA1 alone.
    Other,
}

impl Term {
    pub const ALL: [Term; 3] = [Term::Vtium, Term::OpsOff, Term::Other];

    pub fn name(self) -> &'static str {
        match self {
            Term::Vtium => "vtium",
            Term::OpsOff => "ops-off",
            Term::Other => "other",
        }
    }
}

/// The machine the TUI runs on, from vtium-config-core's fixtures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Machine {
    /// The screen map's machine: two displays, a keyboard and a power
    /// button, and the configuration workstation with one change not
    /// saved.
    Map,
    /// The large machine: ten cards of four displays, sixty inputs,
    /// twelve configurations and thirty versions.
    Large,
}

impl Machine {
    pub const BOTH: [Machine; 2] = [Machine::Map, Machine::Large];

    pub fn name(self) -> &'static str {
        match self {
            Machine::Map => "map",
            Machine::Large => "large",
        }
    }

    /// The shared state at start, with what `vtium check-config` finds in
    /// the open configuration, as the loop records it after its first
    /// check.
    fn shared(self) -> Shared {
        let mut shared = match self {
            Machine::Map => fixture::padnoir::shared(),
            Machine::Large => fixture::large::shared(),
        };
        match self {
            Machine::Map => fixture::padnoir::checked(&mut shared),
            Machine::Large => fixture::large::checked(&mut shared),
        }
        shared
    }

    fn present(self) -> Present {
        match self {
            Machine::Map => fixture::padnoir::present(),
            Machine::Large => fixture::large::present(),
        }
    }
}

/// The font the stand-in console draws when the TUI starts.
pub const START_FONT: &str = "unifont";

/// The six tabs, in the top row's order, each from its own crate, as the
/// binary builds them for the shell (ADR-1206 D9).
pub fn tabs(color: bool) -> Vec<BoxTab> {
    vec![
        Box::new(vtium_config_tab_fonts::Fonts::new(color)),
        Box::new(vtium_config_tab_effects::Effects::new(color)),
        Box::new(vtium_config_tab_looks::Looks::new(color)),
        Box::new(vtium_config_tab_flow::FlowTab::new(color)),
        Box::new(vtium_config_tab_settings::Settings::new(color)),
        Box::new(vtium_config_tab_help::HelpTab::new(color)),
    ]
}

/// Where a click lands, found on the screen at the moment of the click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// The name of the tab at this place in the top row.
    Tab(usize),
    /// The File menu's name in the top row.
    File,
    /// The top row's count of changes not saved.
    Facts,
    /// The first button of the action bar.
    Button,
    /// A row of the viewer, the fourth.
    Row,
    /// The right edge of the screen's middle row, outside any menu.
    Outside,
}

/// One thing the person does: a key, a click, the wheel, or the terminal
/// resized.
#[derive(Clone, Copy, Debug)]
pub enum Press {
    Key(Key),
    Click(Spot),
    Wheel(bool),
    Resize(u16, u16),
}

/// What one key did, beyond the state it left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Did {
    /// What the key caused, the answers' effects included, each as a short
    /// word: `change` for a change to the configuration, `install` for
    /// Save's privileged step, `check`, `prepare`, `switch`, `exit 0`.
    pub effects: Vec<String>,
    /// The TUI exited, with this status.
    pub exit: Option<u8>,
}

/// The stand-in machine: what is installed, and how often Save and the
/// install of a system font ran.
#[derive(Debug)]
pub struct Bench {
    pub installed: Installed,
    pub present: Present,
    pub saves: usize,
    pub fonts: usize,
}

/// The TUI with its stand-ins.
pub struct Sim {
    pub app: App,
    pub term: Term,
    pub bench: Bench,
    /// The font the stand-in console draws.
    pub console_font: String,
    /// Mouse reporting as the terminal has it: on from the start, as the
    /// port turns it on, and off once the TUI leaves.
    pub mouse: bool,
    /// The TUI exited, with this status.
    pub exited: Option<u8>,
}

/// The most effects one key may cause before the walk calls it a loop.
const CASCADE_MAX: usize = 256;

impl Sim {
    /// The TUI at its start in `term`, at `cols` by `rows`, on `machine`,
    /// drawn in color.
    pub fn start(term: Term, machine: Machine, cols: u16, rows: u16) -> Sim {
        Sim::start_in(term, machine, cols, rows, true)
    }

    /// [`Sim::start`], drawn in color where `color` holds and without it,
    /// as under `NO_COLOR`, where it does not.
    pub fn start_in(term: Term, machine: Machine, cols: u16, rows: u16, color: bool) -> Sim {
        let shared = machine.shared();
        let installed = shared.document().installed().clone();
        let size = Size::grid(cols, rows);
        let app = App::new(tabs(color), shared, size, color);
        let mut sim = Sim {
            app,
            term,
            bench: Bench {
                installed,
                present: machine.present(),
                saves: 0,
                fonts: 0,
            },
            console_font: START_FONT.to_owned(),
            mouse: true,
            exited: None,
        };
        // the loop hands the App the screen's size before anything else,
        // and the stand-in terminal answers the questions at start
        let mut did = Did::default();
        sim.event(Event::Resize(size), &mut did);
        sim
    }

    /// The TUI started as `start` says, after `keys`, by the alphabet's
    /// names.
    pub fn reach(start: (Term, Machine, u16, u16), keys: &[&str]) -> Sim {
        let (term, machine, cols, rows) = start;
        let mut sim = Sim::start(term, machine, cols, rows);
        for k in keys {
            sim.press(super::explore::press_of(k));
        }
        sim
    }

    /// Does `press` and answers everything it causes.
    pub fn press(&mut self, press: Press) -> Did {
        let mut did = Did::default();
        if self.exited.is_some() {
            return did;
        }
        let before = self.app.shared().document().content().clone();
        let event = match press {
            Press::Key(key) => Some(Event::Key(key)),
            Press::Click(spot) => self.spot(spot).map(|(col, row)| {
                Event::Mouse(Mouse {
                    button: Button::Press(0),
                    col,
                    row,
                })
            }),
            Press::Wheel(up) => {
                let size = self.app.size();
                Some(Event::Mouse(Mouse {
                    button: if up {
                        Button::WheelUp
                    } else {
                        Button::WheelDown
                    },
                    col: usize::from(size.cols / 2),
                    row: usize::from(size.rows / 2),
                }))
            }
            Press::Resize(cols, rows) => Some(Event::Resize(Size::grid(cols, rows))),
        };
        if let Some(event) = event {
            self.event(event, &mut did);
        }
        // the wait for the console's reply ends once the TUI is leaving
        if self.exited.is_none() && self.app.leaving().is_some() {
            self.event(Event::Timeout, &mut did);
        }
        if *self.app.shared().document().content() != before {
            did.effects.insert(0, "change".to_owned());
        }
        did
    }

    /// The cell a click on `spot` lands on, read from the screen; `None`
    /// where the screen shows no such place, so the click is not made.
    fn spot(&self, spot: Spot) -> Option<(usize, usize)> {
        let rows = rows(&self.draw());
        let size = self.app.size();
        let top = rows.first()?;
        let find = |row: &str, needle: &str| row.find(needle).map(|at| row[..at].chars().count());
        match spot {
            Spot::Tab(i) => {
                let name = self.app.tabs().get(i)?.name();
                Some((find(top, name)?, 0))
            }
            Spot::File => Some((find(top, "File")?, 0)),
            Spot::Facts => Some((find(top, "not saved")?, 0)),
            Spot::Button => {
                let bar = rows.last()?;
                Some((
                    find(bar, "[")?.saturating_add(1),
                    rows.len().saturating_sub(1),
                ))
            }
            Spot::Row => Some((4, 4)),
            Spot::Outside => Some((
                usize::from(size.cols.saturating_sub(1)),
                usize::from(size.rows / 2),
            )),
        }
    }

    fn event(&mut self, event: Event, did: &mut Did) {
        let out = self.app.event(event);
        self.answer(out, did);
    }

    /// Answers `effects`, and the effects the answers cause, in order, as
    /// the port's loop does.
    fn answer(&mut self, effects: Vec<Effect>, did: &mut Did) {
        let mut queue: VecDeque<Effect> = effects.into();
        let mut n = 0usize;
        while let Some(effect) = queue.pop_front() {
            n = n.saturating_add(1);
            assert!(n < CASCADE_MAX, "one key caused {n} effects: {did:?}");
            if self.exited.is_some() {
                break;
            }
            let event = match effect {
                Effect::Write(bytes) => {
                    for reply in self.terminal(&bytes, did) {
                        queue.extend(self.app.event(Event::Reply(reply)));
                    }
                    continue;
                }
                Effect::Exit(status) => {
                    did.effects.push(format!("exit {status}"));
                    did.exit = Some(status);
                    self.exited = Some(status);
                    // the port turns mouse reporting off as it leaves
                    self.mouse = false;
                    continue;
                }
                Effect::Desk(Op::Devices) => {
                    did.effects.push("devices".into());
                    Event::Devices(Some(self.bench.present.clone()))
                }
                Effect::Desk(Op::SystemFont(path)) => {
                    did.effects.push("system font".into());
                    Event::FontReady(Ok(ready(&path)))
                }
                Effect::InstallFont => {
                    did.effects.push("install font".into());
                    self.bench.fonts = self.bench.fonts.saturating_add(1);
                    Event::FontInstalled(Ok(()))
                }
                Effect::Desk(Op::Prepare) => {
                    did.effects.push("prepare".into());
                    Event::Save(Came::Read(Ok(self.bench.installed.clone())))
                }
                Effect::Install(plan) => {
                    did.effects.push("install".into());
                    self.bench.saves = self.bench.saves.saturating_add(1);
                    let installed = installed_after(&self.bench.installed, &plan);
                    self.bench.installed = installed.clone();
                    Event::Save(Came::Saved(Ok(Saved {
                        installed,
                        said: "vtium-config apply: installed.".into(),
                    })))
                }
                // the stand-in editor saves the copy with one line more,
                // which the check passes
                Effect::Editor { text, .. } => {
                    did.effects.push("editor".into());
                    Event::Edited(Ok(Edited {
                        text: format!("{text}# edited as text\n"),
                        problems: Vec::new(),
                        count: 0,
                    }))
                }
                Effect::Desk(Op::Check) => {
                    did.effects.push("check".into());
                    continue;
                }
                Effect::Desk(Op::Versions) => {
                    did.effects.push("history".into());
                    continue;
                }
                // the previews and the font helper's answers are effects
                // the loop does not perform yet, as the port's loop does
                // not; a wait ends only when the walk says so
                Effect::Wait(_)
                | Effect::Specimen { .. }
                | Effect::Cell { .. }
                | Effect::Picture(_)
                | Effect::ScreenPicture(_)
                | Effect::Summary { .. }
                | Effect::Sources(_)
                | Effect::Describe(_) => continue,
            };
            queue.extend(self.app.event(event));
        }
    }

    /// The stand-in terminal reads `bytes`: it follows mouse reporting,
    /// and answers the TUI's queries in the order they come, as every
    /// terminal of the VT100 line does. XTVERSION gets the terminal's name
    /// from a vtium console. A vtium console that switches fonts takes
    /// each OSC 50 switch and answers each OSC 50 query with the font it
    /// draws. DA1 gets its reply in every terminal.
    fn terminal(&mut self, bytes: &[u8], did: &mut Did) -> Vec<Reply> {
        let text = String::from_utf8_lossy(bytes);
        if text.contains("\x1b[?1000l") {
            self.mouse = false;
            did.effects.push("mouse off".into());
        }
        if text.contains("\x1b[?1000h") {
            self.mouse = true;
            did.effects.push("mouse on".into());
        }
        let mut replies = Vec::new();
        let mut rest = text.as_ref();
        while let Some(at) = rest.find('\x1b') {
            rest = &rest[at..];
            if let Some(after) = rest.strip_prefix("\x1b[>q") {
                if self.term != Term::Other {
                    replies.push(Reply::Version("vtium(0.1.0)".into()));
                }
                rest = after;
            } else if let Some(after) = rest.strip_prefix("\x1b[c") {
                replies.push(Reply::Attributes);
                rest = after;
            } else if let Some(osc) = rest.strip_prefix("\x1b]50;") {
                let Some((name, after)) = osc.split_once("\x1b\\") else {
                    break;
                };
                rest = after;
                if name == "?" {
                    if self.term == Term::Vtium {
                        replies.push(Reply::Font(self.console_font.clone()));
                    }
                    continue;
                }
                did.effects.push("switch".into());
                if self.term == Term::Vtium {
                    self.console_font = name.to_owned();
                }
            } else if let Some(apc) = rest.strip_prefix("\x1b_") {
                // a kitty graphics query, which no stand-in answers
                rest = apc.split_once("\x1b\\").map_or("", |(_, after)| after);
            } else {
                rest = &rest[1..];
            }
        }
        replies
    }

    /// The screen as ratatui's buffer.
    pub fn draw(&self) -> Buffer {
        draw(&self.app)
    }

    /// The screen as text, one line per row.
    pub fn screen(&self) -> Vec<String> {
        rows(&self.draw())
    }

    /// What the desk keeps as the open configuration's draft.
    pub fn draft(&self) -> Option<Content> {
        let shared = self.app.shared();
        let name = shared.document().name();
        shared.documents.desk().read(name).ok().flatten()
    }
}

/// The font file `source` readied as a system font, as the machine's
/// check finds it: a new file in the system's fonts folder.
fn ready(source: &Path) -> Ready {
    let file = source.file_name().map(PathBuf::from).unwrap_or_default();
    let target = PathBuf::from("/usr/local/share/fonts").join(file);
    Ready {
        source: source.to_owned(),
        command: format!(
            "sudo vtium-config apply {} {}",
            source.display(),
            target.display()
        ),
        target,
        placement: Placement::New,
    }
}

/// What is installed after the privileged step installs `plan` over
/// `installed`: its vtium.conf, its font sets and its removals.
fn installed_after(installed: &Installed, plan: &Plan) -> Installed {
    let mut after = installed.clone();
    let stem = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().into_owned());
    for file in &plan.files {
        if file.path.file_name().is_some_and(|n| n == "vtium.conf") {
            after.text = Some(file.new.clone());
        } else if file.path.extension().is_some_and(|e| e == "vtfrit")
            && let Some(name) = stem(&file.path)
        {
            after.frits.insert(name, file.new.clone().into_bytes());
        }
    }
    for removal in &plan.removals {
        if let Some(name) = stem(&removal.path) {
            after.frits.remove(&name);
        }
    }
    after
}

/// The screen `app` draws, as ratatui's buffer.
pub fn draw(app: &App) -> Buffer {
    let size = app.size();
    let mut terminal = match Terminal::new(TestBackend::new(size.cols, size.rows)) {
        Ok(t) => t,
        Err(e) => panic!("a test terminal: {e}"),
    };
    if let Err(e) = terminal.draw(|f| vtium_config_tui::view::draw(app, f)) {
        panic!("a frame: {e}");
    }
    terminal.backend().buffer().clone()
}

/// Each row of `buf` as text, one symbol for each cell, so a wide
/// character's second cell adds nothing.
pub fn rows(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.y..a.bottom())
        .map(|y| {
            (a.x..a.right())
                .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()))
                .collect()
        })
        .collect()
}

/// The cells `s` takes, as ratatui measures it.
pub fn width(s: &str) -> usize {
    ratatui_core::text::Span::raw(s).width()
}

/// The highlighted row of the viewer: the first row under the top row,
/// above the action bar, that holds a letter in reverse video, with the
/// first and last column of its reversed cells.
fn highlight(buf: &Buffer) -> Option<(u16, u16, u16)> {
    use ratatui_core::style::Modifier;
    let a = buf.area;
    let reversed = |x: u16, y: u16| {
        buf.cell((x, y))
            .is_some_and(|c| c.modifier.contains(Modifier::REVERSED))
    };
    (a.y.saturating_add(1)..a.bottom().saturating_sub(1)).find_map(|y| {
        let lit = (a.x..a.right()).any(|x| {
            reversed(x, y)
                && buf
                    .cell((x, y))
                    .is_some_and(|c| c.symbol().chars().any(char::is_alphanumeric))
        });
        let first = (a.x..a.right()).find(|&x| reversed(x, y))?;
        let last = (a.x..a.right()).rev().find(|&x| reversed(x, y))?;
        lit.then_some((y, first, last))
    })
}

/// The text of the cells of row `y` from column `from` to `to`.
fn span(buf: &Buffer, y: u16, from: u16, to: u16) -> String {
    (from..=to)
        .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()))
        .collect()
}

/// The group the highlighted item belongs to, as the screen heads it:
/// the first word of the nearest row above the highlighted row, in its
/// columns, that starts further left, such as `Font` of `Font sets` over
/// a font set. Only the first word counts, so a count beside a heading,
/// or the name of the terminal a card heads, does not tell two states
/// apart. Where no row is highlighted, the column heading over the box
/// drawn in heavy lines, which is how the Flow tab highlights, as
/// `TERMINALS`. Where the list has scrolled its heading off the screen,
/// the next heading below with a `^` before it, as `^Color` for the font
/// sets over Color schemes. A row with no heading over or under it, as a
/// filter line, is named by its own first word after a `=`, as `=Show:`.
/// Empty where the screen highlights nothing.
pub fn group(buf: &Buffer) -> String {
    let first_word = |text: &str| {
        text.split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    let Some((y, from, to)) = highlight(buf) else {
        return heavy_box_heading(buf).unwrap_or_default();
    };
    // the highlighted row's column starts after the nearest rule of
    // panes to its left, so its indent and a heading's are measured alike
    let left = (buf.area.x..from)
        .rev()
        .find(|&x| buf.cell((x, y)).is_some_and(|c| c.symbol() == "│"))
        .map_or(buf.area.x, |x| x.saturating_add(1));
    let indent = |text: &str| text.chars().take_while(|c| *c == ' ').count();
    let own = indent(&span(buf, y, left, to));
    // the nearest row in `rows` that starts further left, until a rule or
    // a box's edge
    let heading = |rows: &mut dyn Iterator<Item = u16>| -> Option<String> {
        for r in rows {
            let text = span(buf, r, left, to);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with(BOX) {
                return None;
            }
            if indent(&text) < own {
                return Some(first_word(trimmed));
            }
        }
        None
    };
    let top = buf.area.y.saturating_add(1);
    let bottom = buf.area.bottom().saturating_sub(1);
    if let Some(h) = heading(&mut (top..y).rev()) {
        return h;
    }
    // a list scrolled past its heading: the group before the next heading
    if let Some(h) = heading(&mut (y.saturating_add(1)..bottom)) {
        return format!("^{h}");
    }
    // a row with no heading over or under it, as a filter line, by its
    // own first word
    format!("={}", first_word(span(buf, y, left, to).trim()))
}

/// Whether `word` is a heading's word in capitals, as `DISPLAYS`.
fn capitals(word: &str) -> bool {
    word.chars().count() > 1 && word.chars().all(|c| c.is_ascii_uppercase() || c == '\'')
}

/// The characters that start a row of a box or a rule, where the search
/// for a heading stops.
const BOX: [char; 8] = ['─', '┌', '└', '│', '┏', '┗', '╔', '╚'];

/// The heading over the box drawn in heavy lines: the nearest row above
/// the box, in its first columns, whose first word is in capitals, such
/// as `DISPLAYS`, skipping the rows of other boxes. Under a rule, the
/// section's own heading at the left of the row under the rule, with the
/// box's first word, as `NEW Terminal` for NEW PATH's terminal box. `None`
/// where the screen draws no such box.
fn heavy_box_heading(buf: &Buffer) -> Option<String> {
    let a = buf.area;
    let (x, y) = (a.y..a.bottom()).find_map(|y| {
        (a.x..a.right())
            .find(|&x| buf.cell((x, y)).is_some_and(|c| c.symbol() == "┏"))
            .map(|x| (x, y))
    })?;
    let to = x.saturating_add(12).min(a.right().saturating_sub(1));
    for above in (a.y.saturating_add(1)..y).rev() {
        let text = span(buf, above, x, to);
        let trimmed = text.trim_start();
        if trimmed.starts_with('─') && trimmed.trim_end().chars().all(|c| c == '─') {
            let under = span(
                buf,
                above.saturating_add(1),
                a.x,
                a.right().saturating_sub(1),
            );
            let section = under.split_whitespace().next().unwrap_or_default();
            if !capitals(section) {
                return None;
            }
            // a plain word that names the box's kind, as `Terminal`, and
            // not a device's name
            let below = y.checked_add(1)?;
            let end = (x.saturating_add(1)..a.right())
                .find(|&e| buf.cell((e, below)).is_some_and(|c| c.symbol() == "┃"))?;
            let title = span(buf, below, x.saturating_add(1), end.saturating_sub(1));
            let first = title.split_whitespace().next().unwrap_or_default();
            let mut letters = first.chars();
            let plain = letters.next().is_some_and(|c| c.is_ascii_uppercase())
                && letters.clone().count() > 2
                && letters.all(|c| c.is_ascii_lowercase());
            return Some(if plain {
                format!("{section} {first}")
            } else {
                section.to_owned()
            });
        }
        let word = trimmed.split_whitespace().next().unwrap_or_default();
        if capitals(word) {
            return Some(word.to_owned());
        }
    }
    None
}
