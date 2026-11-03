//! The TUI as a state machine apart from its terminal (ADR-1201 D7): its
//! state takes [`Event`]s, a key, a reply, a resize, a signal or the end of
//! a wait, and returns [`Effect`]s, bytes to write, a wait to start, a save
//! to run or the status to leave with. The loop around it
//! ([`super::port`]) does the reading and the writing, and draws
//! [`super::view`]'s screen after each event.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use vtium_settings::{Edit, edit_conf, parse, set_profile, setting, switch as conf_switch};

use super::input::{Input, Key, Reply};
use crate::draft::{FileEdit, Plan};

/// How long the TUI waits for the terminal's answers at start, and for
/// the console's font to come back when it leaves: `set`'s wait.
pub const WAIT: Duration = crate::switch::WAIT;

/// The image id of the kitty graphics query, a one-pixel image the
/// terminal checks and does not keep.
pub const QUERY_ID: u32 = 31;

/// The four requests the TUI writes at start (D4): XTVERSION, the OSC 50
/// query, kitty's graphics query and DA1, in that order.
pub fn probe() -> Vec<u8> {
    format!("\x1b[>q\x1b]50;?\x1b\\\x1b_Gi={QUERY_ID},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c")
        .into_bytes()
}

/// `OSC 50 ; NAME ST` and the query after it, the bytes `set` sends.
pub fn switch(name: &str) -> Vec<u8> {
    format!("\x1b]50;{name}\x1b\\\x1b]50;?\x1b\\").into_bytes()
}

/// A font the TUI lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub name: String,
    pub state: FontState,
}

/// What a font's state is, as the TUI reads it without parsing a font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontState {
    /// A frit that reads.
    Reads,
    /// One of the console's built-in fonts.
    BuiltIn,
    /// A frit that does not read, and why.
    Refused(String),
}

/// Where the density pictures are drawn at came from (ADR-1200 D17's
/// order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpiFrom {
    Conf,
    Edid,
    Assumed,
}

/// What the TUI starts from: the fonts, the configuration's files and
/// where a save goes.
#[derive(Clone, Debug)]
pub struct Inputs {
    pub fonts: Vec<Font>,
    pub conf_path: PathBuf,
    /// vtium.conf as read, `None` when there is none.
    pub conf: Option<String>,
    pub heads_path: PathBuf,
    /// The head file as read, `None` when there is none.
    pub heads: Option<String>,
    /// The tool's name in the command a save runs.
    pub program: String,
    /// The directory drafts go under, as the save view names it.
    pub drafts: PathBuf,
    /// The TUI runs as root, so a save needs no sudo.
    pub root: bool,
    /// The density size pictures are drawn at, and where it came from.
    pub dpi: (u32, DpiFrom),
}

/// One thing that happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Input(Input),
    /// The terminal's grid changed.
    Resize {
        cols: usize,
        rows: usize,
    },
    /// A signal that ends the TUI arrived, by its number.
    Signal(i32),
    /// The wait the last [`Effect::Wait`] started ended.
    Timeout,
    /// The save ended: the files written, or why none was.
    Saved(Result<usize, String>),
}

/// One thing the loop does for the state machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Bytes to write to the terminal.
    Write(Vec<u8>),
    /// Starts a wait, which ends in [`Event::Timeout`] unless another
    /// replaces it.
    Wait(Duration),
    /// Writes the draft and runs the privileged step, then sends
    /// [`Event::Saved`].
    Save(Plan),
    /// Leaves, with this exit status.
    Exit(u8),
}

/// The views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Fonts,
    Sizes,
    Settings,
    Heads,
    Save,
}

impl View {
    /// The views Tab steps through, in order.
    pub const TABS: [View; 4] = [View::Fonts, View::Sizes, View::Settings, View::Heads];
}

/// What the TUI found the terminal to be (D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Live {
    /// The answers have not all come.
    Asking,
    /// A vtium console that answered the OSC 50 query: fonts are tried
    /// live, and the query's answer is the font to give back.
    On,
    /// A vtium console with `allow-font-ops` off.
    OpsOff,
    /// Another terminal: the TUI never sends OSC 50.
    NotVtium,
}

/// The sizes the size view offers, in points, after automatic (0): within
/// the bounds the console reads `font-size` and a frit's `size=` in.
pub const SIZES: [u32; 15] = [6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32];

/// The settings view's keys, the font keys of D1.
pub const SETTING_KEYS: [&str; 7] = [
    "font-name",
    "font-size",
    "font-frits",
    "font-hinting",
    "font-line-spacing",
    "allow-font-ops",
    "display-dpi",
];

/// What a save would change, not yet written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub font_name: Option<String>,
    /// `0` for automatic.
    pub font_size: Option<u32>,
    pub allow_font_ops: Option<bool>,
    /// Each head's new profile, by the head's name.
    pub profiles: Vec<(String, String)>,
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        *self == Pending::default()
    }
}

/// What the answers at start have said so far.
#[derive(Clone, Debug, Default)]
struct Answers {
    version: Option<String>,
    font: Option<String>,
    graphics: bool,
}

/// The TUI's state.
#[derive(Debug)]
pub struct App {
    pub inputs: Inputs,
    pub cols: usize,
    pub rows: usize,
    pub view: View,
    pub live: Live,
    /// The terminal answered the kitty graphics query before DA1.
    pub graphics: bool,
    answers: Answers,
    /// The font to give back: the start query's answer, or what was saved.
    pub start_font: Option<String>,
    /// The font the console last said it draws.
    pub shown_font: Option<String>,
    /// The switch sent whose reply has not come.
    pub in_flight: Option<String>,
    /// Fonts the console refused, each marked in the list.
    pub refused: BTreeSet<String>,
    pub font_at: usize,
    pub size_at: usize,
    pub setting_at: usize,
    pub head_at: usize,
    pub pending: Pending,
    /// A line for the user, under the list.
    pub message: Option<String>,
    /// The TUI is saving: the privileged step runs.
    pub saving: bool,
    /// The TUI is leaving with this status once the font is back.
    pub leaving: Option<u8>,
    /// OSC 50 replies still due before the TUI may leave.
    replies_due: usize,
}

impl App {
    /// The TUI at its start in a terminal of `cols` by `rows`, and what it
    /// does first: the four requests, and the wait for their answers.
    pub fn new(inputs: Inputs, cols: usize, rows: usize) -> (App, Vec<Effect>) {
        let configured = inputs.conf.as_deref().and_then(|c| setting(c, "font-name"));
        let font_at = configured
            .as_deref()
            .and_then(|n| inputs.fonts.iter().position(|f| f.name == n))
            .unwrap_or(0);
        let app = App {
            inputs,
            cols,
            rows,
            view: View::Fonts,
            live: Live::Asking,
            graphics: false,
            answers: Answers::default(),
            start_font: None,
            shown_font: None,
            in_flight: None,
            refused: BTreeSet::new(),
            font_at,
            size_at: 0,
            setting_at: 0,
            head_at: 0,
            pending: Pending::default(),
            message: None,
            saving: false,
            leaving: None,
            replies_due: 0,
        };
        (app, vec![Effect::Write(probe()), Effect::Wait(WAIT)])
    }

    /// The highlighted font.
    pub fn font(&self) -> Option<&Font> {
        self.inputs.fonts.get(self.font_at)
    }

    /// The value vtium.conf gives `key` now.
    pub fn configured(&self, key: &str) -> Option<String> {
        self.inputs.conf.as_deref().and_then(|c| setting(c, key))
    }

    /// Whether a save leaves `allow-font-ops` on: the pending change, else
    /// vtium.conf's, else its default, on.
    pub fn font_ops(&self) -> bool {
        self.pending.allow_font_ops.unwrap_or_else(|| {
            self.inputs
                .conf
                .as_deref()
                .and_then(|c| conf_switch(c, "allow-font-ops"))
                .unwrap_or(true)
        })
    }

    /// The heads the head file declares, when it reads.
    pub fn heads(&self) -> Option<Vec<(String, String)>> {
        let text = self.inputs.heads.as_deref()?;
        let parsed = parse(text).ok()?;
        Some(
            parsed
                .config
                .heads
                .into_iter()
                .map(|h| {
                    let profile = self
                        .pending
                        .profiles
                        .iter()
                        .find(|(n, _)| *n == h.name)
                        .map_or(h.profile, |(_, p)| p.clone());
                    (h.name, profile)
                })
                .collect(),
        )
    }

    /// The profiles a head may draw with: `default`, then each frit that
    /// reads.
    pub fn profiles(&self) -> Vec<String> {
        std::iter::once("default".to_owned())
            .chain(
                self.inputs
                    .fonts
                    .iter()
                    .filter(|f| f.state == FontState::Reads)
                    .map(|f| f.name.clone()),
            )
            .collect()
    }

    /// Takes `event`, and returns what the loop does.
    pub fn event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Input(Input::Reply(reply)) => self.reply(reply),
            Event::Input(Input::Key(key)) => self.key(key),
            Event::Resize { cols, rows } => {
                self.cols = cols;
                self.rows = rows;
                Vec::new()
            }
            Event::Signal(n) => {
                let status = u8::try_from(n.saturating_add(128)).unwrap_or(1);
                self.leave(status)
            }
            Event::Timeout => self.timeout(),
            Event::Saved(result) => self.saved(result),
        }
    }

    fn reply(&mut self, reply: Reply) -> Vec<Effect> {
        if self.live == Live::Asking {
            match reply {
                Reply::Version(v) => self.answers.version = Some(v),
                Reply::Font(f) => self.answers.font = Some(f),
                Reply::Graphics(ok) => self.answers.graphics |= ok,
                Reply::Attributes => {
                    self.decide();
                    return Vec::new();
                }
            }
            return Vec::new();
        }
        let Reply::Font(name) = reply else {
            return Vec::new();
        };
        if self.leaving.is_some() {
            self.replies_due = self.replies_due.saturating_sub(1);
            self.shown_font = Some(name);
            return match (self.replies_due, self.leaving) {
                (0, Some(status)) => vec![Effect::Exit(status)],
                _ => Vec::new(),
            };
        }
        if let Some(asked) = self.in_flight.take()
            && asked != name
        {
            self.refused.insert(asked);
        }
        self.shown_font = Some(name);
        self.follow()
    }

    /// Settles what the terminal is, from the answers that came.
    fn decide(&mut self) {
        let vtium = self
            .answers
            .version
            .as_deref()
            .is_some_and(|v| v.starts_with("vtium("));
        self.graphics = self.answers.graphics;
        self.live = match (vtium, self.answers.font.take()) {
            (true, Some(font)) => {
                self.start_font = Some(font.clone());
                self.shown_font = Some(font.clone());
                if let Some(at) = self.inputs.fonts.iter().position(|f| f.name == font) {
                    self.font_at = at;
                }
                Live::On
            }
            (true, None) => {
                self.message = Some(super::words::OPS_OFF.to_owned());
                Live::OpsOff
            }
            (false, _) => {
                self.message = Some(super::words::NOT_VTIUM.to_owned());
                Live::NotVtium
            }
        };
    }

    fn timeout(&mut self) -> Vec<Effect> {
        if self.live == Live::Asking {
            self.decide();
            return Vec::new();
        }
        match self.leaving {
            Some(status) => vec![Effect::Exit(status)],
            None => Vec::new(),
        }
    }

    /// Sends the highlighted font when no switch is outstanding and the
    /// console draws another (D4): one switch at a time, naming whatever
    /// is highlighted when the last reply came.
    fn follow(&mut self) -> Vec<Effect> {
        if self.live != Live::On || self.in_flight.is_some() || self.view != View::Fonts {
            return Vec::new();
        }
        let Some(name) = self.font().map(|f| f.name.clone()) else {
            return Vec::new();
        };
        let readable = !matches!(self.font().map(|f| &f.state), Some(FontState::Refused(_)));
        if self.shown_font.as_deref() == Some(&name) || self.refused.contains(&name) || !readable {
            return Vec::new();
        }
        self.in_flight = Some(name.clone());
        vec![Effect::Write(switch(&name))]
    }

    /// Leaves with `status`, giving the console its start font back first
    /// when a switch changed it (D4).
    pub fn leave(&mut self, status: u8) -> Vec<Effect> {
        if self.leaving.is_some() {
            return Vec::new();
        }
        let changed = self.in_flight.is_some() || self.shown_font != self.start_font;
        match (&self.live, &self.start_font) {
            (Live::On, Some(start)) if changed => {
                let bytes = switch(start);
                self.replies_due = usize::from(self.in_flight.is_some()).saturating_add(1);
                self.leaving = Some(status);
                self.message = Some(super::words::LEAVING.to_owned());
                vec![Effect::Write(bytes), Effect::Wait(WAIT)]
            }
            _ => {
                self.leaving = Some(status);
                vec![Effect::Exit(status)]
            }
        }
    }

    fn key(&mut self, key: Key) -> Vec<Effect> {
        if self.leaving.is_some() || self.saving {
            return Vec::new();
        }
        match key {
            Key::Interrupt => return self.leave(130),
            Key::Char('q') => return self.leave(0),
            Key::Tab | Key::BackTab if self.view != View::Save => {
                let at = View::TABS.iter().position(|v| *v == self.view).unwrap_or(0);
                let n = View::TABS.len();
                let next = if key == Key::Tab {
                    at.saturating_add(1) % n
                } else {
                    at.saturating_add(n).saturating_sub(1) % n
                };
                self.view = View::TABS.get(next).copied().unwrap_or(View::Fonts);
                return self.follow();
            }
            Key::Char('s') if self.view != View::Save => {
                self.view = View::Save;
                return Vec::new();
            }
            Key::Char('o') if self.live == Live::OpsOff => {
                self.pending.allow_font_ops = Some(true);
                self.message = Some(super::words::OPS_ON_PENDING.to_owned());
                return Vec::new();
            }
            _ => {}
        }
        match self.view {
            View::Fonts => self.fonts_key(key),
            View::Sizes => {
                let n = SIZES.len().saturating_add(1);
                self.size_at = step(self.size_at, n, key);
                if key == Key::Enter {
                    let size = match self.size_at {
                        0 => 0,
                        i => SIZES.get(i.saturating_sub(1)).copied().unwrap_or(0),
                    };
                    self.pending.font_size = Some(size);
                }
                Vec::new()
            }
            View::Settings => {
                self.setting_at = step(self.setting_at, SETTING_KEYS.len(), key);
                if key == Key::Enter && SETTING_KEYS.get(self.setting_at) == Some(&"allow-font-ops")
                {
                    let now = self.font_ops();
                    self.pending.allow_font_ops = Some(!now);
                }
                Vec::new()
            }
            View::Heads => {
                let heads = self.heads().unwrap_or_default();
                self.head_at = step(self.head_at, heads.len(), key);
                if let (Key::Left | Key::Right, Some((head, profile))) =
                    (key, heads.get(self.head_at))
                {
                    let all = self.profiles();
                    let at = all.iter().position(|p| p == profile).unwrap_or(0);
                    let n = all.len().max(1);
                    let next = if key == Key::Right {
                        at.saturating_add(1) % n
                    } else {
                        at.saturating_add(n).saturating_sub(1) % n
                    };
                    if let Some(p) = all.get(next) {
                        self.pending.profiles.retain(|(h, _)| h != head);
                        self.pending.profiles.push((head.clone(), p.clone()));
                    }
                }
                Vec::new()
            }
            View::Save => match key {
                Key::Backspace | Key::Left => {
                    self.view = View::Fonts;
                    self.follow()
                }
                Key::Enter => match self.plan() {
                    Ok(plan) if plan.is_empty() => {
                        self.message = Some(super::words::NOTHING_TO_SAVE.to_owned());
                        Vec::new()
                    }
                    Ok(plan) => {
                        self.saving = true;
                        self.message = Some(super::words::SAVING.to_owned());
                        vec![Effect::Save(plan)]
                    }
                    Err(why) => {
                        self.message = Some(super::words::save_failed(why));
                        Vec::new()
                    }
                },
                _ => Vec::new(),
            },
        }
    }

    fn fonts_key(&mut self, key: Key) -> Vec<Effect> {
        let n = self.inputs.fonts.len();
        self.font_at = step(self.font_at, n, key);
        if key == Key::Enter
            && let Some(font) = self.font()
            && !matches!(font.state, FontState::Refused(_))
            && !self.refused.contains(&font.name)
        {
            self.pending.font_name = Some(font.name.clone());
        }
        self.follow()
    }

    /// What saving now would change: vtium.conf with the pending keys, and
    /// the head file with each head's profile, through the one writer.
    pub fn plan(&self) -> Result<Plan, String> {
        let mut files = Vec::new();
        let p = &self.pending;
        let size = p.font_size.map(|s| s.to_string());
        let mut edits = Vec::new();
        if let Some(name) = &p.font_name {
            edits.push(Edit::Set("font-name", name));
        }
        if let Some(size) = &size {
            edits.push(Edit::Set("font-size", size));
        }
        if let Some(on) = p.allow_font_ops {
            edits.push(Edit::Switch("allow-font-ops", on));
        }
        if !edits.is_empty() {
            let old = self.inputs.conf.clone();
            let new = edit_conf(old.as_deref().unwrap_or_default(), &edits)
                .map_err(|e| format!("{e:?}"))?;
            if old.as_deref() != Some(&new) {
                files.push(FileEdit {
                    path: self.inputs.conf_path.clone(),
                    old,
                    new,
                });
            }
        }
        if !p.profiles.is_empty()
            && let Some(old) = &self.inputs.heads
        {
            let mut new = old.clone();
            for (head, profile) in &p.profiles {
                new = set_profile(&new, head, profile).map_err(|e| e.to_string())?;
            }
            if new != *old {
                files.push(FileEdit {
                    path: self.inputs.heads_path.clone(),
                    old: Some(old.clone()),
                    new,
                });
            }
        }
        Ok(Plan { files })
    }

    fn saved(&mut self, result: Result<usize, String>) -> Vec<Effect> {
        self.saving = false;
        match result {
            Ok(files) => {
                if let Ok(plan) = self.plan() {
                    for f in plan.files {
                        if f.path == self.inputs.conf_path {
                            self.inputs.conf = Some(f.new);
                        } else if f.path == self.inputs.heads_path {
                            self.inputs.heads = Some(f.new);
                        }
                    }
                }
                // the console stays in the saved font (D4)
                if self.pending.font_name.is_some() && self.shown_font == self.pending.font_name {
                    self.start_font = self.shown_font.clone();
                }
                self.pending = Pending::default();
                self.view = View::Fonts;
                self.message = Some(super::words::saved(files));
            }
            Err(why) => self.message = Some(super::words::save_failed(why)),
        }
        Vec::new()
    }
}

/// `at` moved by `key` in a list of `n`.
fn step(at: usize, n: usize, key: Key) -> usize {
    let last = n.saturating_sub(1);
    match key {
        Key::Up => at.saturating_sub(1),
        Key::Down => at.saturating_add(1).min(last),
        Key::PageUp => at.saturating_sub(10),
        Key::PageDown => at.saturating_add(10).min(last),
        Key::Home => 0,
        Key::End => last,
        _ => at.min(last),
    }
}
