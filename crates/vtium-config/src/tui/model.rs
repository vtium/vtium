//! The TUI as a state machine apart from its terminal (ADR-1201 D7): its
//! state takes [`Event`]s, a key, a mouse click, a reply, a resize, a
//! signal or the end of a wait, and returns [`Effect`]s, bytes to write, a
//! wait to start, a save to run or the status to leave with. The loop
//! around it ([`super::port`]) does the reading and the writing, and draws
//! [`super::view`]'s screen after each event.
//!
//! The views are Flow, Looks, Fonts, Settings and Help (ADR-1201 D3), and
//! the TUI opens on the one its caller names. The Flow view moves over a
//! [`Flow`], selects, tags, follows lines and opens cards and the route
//! card (ADR-1403 D7); the operations that change the flow come with the
//! draft.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use vtium_settings::keys::{self, Kind, Scope};
use vtium_settings::{Edit, edit_conf, setting, switch as conf_switch};

use super::flow::{COLUMNS, Flow, Item};
use super::input::{Button, Input, Key, Mouse, Reply};
use crate::draft::{FileEdit, Plan, Removal};

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
    /// vtium.conf as read, its keys and its sections, `None` when there is
    /// none.
    pub conf: Option<String>,
    /// How a save is installed, as the save view shows it.
    pub saving: Saving,
    /// The directory drafts go under, as the save view names it.
    pub drafts: PathBuf,
    /// The density size pictures are drawn at, and where it came from.
    pub dpi: (u32, DpiFrom),
    /// The frits' directory, where a new frit is installed.
    pub frits: PathBuf,
    /// The view the TUI opens at: the fonts, or the new-frit view for
    /// `vtium config new`.
    pub view: View,
    /// The first display's size in pixels, which a size's grid is counted
    /// on, where a display is connected.
    pub display: Option<(u32, u32)>,
    /// Each frit installed in the frits' directory, by its name, as its
    /// bytes: what a new frit of that name replaces, and what a removal
    /// removes.
    pub installed: std::collections::BTreeMap<String, Vec<u8>>,
    /// The flow the Flow view draws.
    pub flow: Flow,
    /// The terminal draws color: not with `NO_COLOR`, nor on a terminal
    /// that names none.
    pub color: bool,
}

/// How a save is installed, as the save view shows it: the command a
/// save runs, with `DRAFT` for the draft's directory, from
/// [`crate::apply::save_argv`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Saving {
    /// Through sudo, which asks for the password.
    Sudo(String),
    /// As the user, into directories that are not the system's.
    User(String),
}

/// The name the save view gives the draft's directory in its command.
pub const DRAFT: &str = "DRAFT";

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
    /// The specimen [`Effect::Picture`] asked for, or why there is none.
    Pictured(Result<crate::specimen::Picture, String>),
    /// The summary [`Effect::Summary`] asked for.
    Summarized {
        font: String,
        height: u32,
        summary: Result<crate::specimen::Summary, String>,
    },
    /// The sources each frit [`Effect::Sources`] named lacks.
    Sourced(Vec<(String, SourcesFound)>),
}

/// The sources a frit lacks or refuses, or why it draws nothing here.
pub type SourcesFound = crate::specimen::Sources;

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
    /// Draws the specimen of the font `font` at `height` pixels, then
    /// sends [`Event::Pictured`].
    Picture {
        font: String,
        height: u32,
        max: (u32, u32),
    },
    /// Summarizes the specimen of the font `font` at `height` pixels, for
    /// a terminal that draws no pictures, then sends [`Event::Summarized`].
    Summary { font: String, height: u32 },
    /// Asks the glyph helper which sources each frit named lacks, then
    /// sends [`Event::Sourced`].
    Sources(Vec<String>),
    /// Leaves, with this exit status.
    Exit(u8),
}

/// The views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Flow,
    /// The looks; until looks are built, the sizes the console's font
    /// takes.
    Looks,
    Fonts,
    Settings,
    /// Help and About.
    Help,
    /// `new`'s guided flow: a frit made from fonts, installed like any
    /// change (ADR-1201 D9). It belongs to the Fonts view.
    New,
    /// What Install installs, and the command it runs.
    Save,
}

impl View {
    /// The views Tab steps through, in order.
    pub const TABS: [View; 5] = [
        View::Flow,
        View::Looks,
        View::Fonts,
        View::Settings,
        View::Help,
    ];

    /// The tab the view shows under: its own, or the Fonts view's for the
    /// new-frit view; none for the install view, an action.
    pub const fn tab(self) -> Option<View> {
        match self {
            View::New => Some(View::Fonts),
            View::Save => None,
            v => Some(v),
        }
    }
}

/// Mouse reporting, modes 1000 and 1006, which only the Flow view turns on
/// (ADR-1403 D7).
pub const MOUSE_ON: &[u8] = b"\x1b[?1000h\x1b[?1006h";
pub const MOUSE_OFF: &[u8] = b"\x1b[?1006l\x1b[?1000l";

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

/// The cell assumed, in pixels, where the terminal reports no size in
/// pixels: the console's 8x16 at 640x480.
pub const ASSUMED_CELL: (u32, u32) = (8, 16);

/// The sizes the size view offers, in points, after automatic (0): within
/// the bounds the console reads `font-size` and a frit's `size=` in.
pub const SIZES: [u32; 15] = [6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32];

/// The font keys, which the settings view lists first (D1): the font,
/// its size and options, font operations, the density, and the sprite
/// policy's keys.
pub const FONT_KEYS: [&str; 14] = [
    "font-name",
    "font-size",
    "font-frits",
    "font-hinting",
    "font-line-spacing",
    "allow-font-ops",
    "display-dpi",
    "glyph-source",
    "glyph-source-box",
    "glyph-source-block",
    "glyph-source-braille",
    "glyph-source-sextants",
    "glyph-source-octants",
    "glyph-source-legacy",
];

/// The keys the settings view lists: every key vtium.conf takes, from
/// vtium's own key table, the font keys first and the rest in the table's
/// order.
pub fn setting_keys() -> Vec<&'static keys::Key> {
    let font = FONT_KEYS.iter().filter_map(|n| keys::find(n));
    let rest = keys::KEYS
        .iter()
        .filter(|k| k.scope == Scope::Anywhere && !FONT_KEYS.contains(&k.name));
    font.chain(rest).collect()
}

/// One change a save makes to a key of vtium.conf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Set(String),
    Switch(bool),
    /// The key goes back to its default.
    Unset,
}

/// What a save would change, not yet written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    /// vtium.conf's keys, one change a key, in the order they were made.
    pub conf: Vec<(String, Change)>,
    /// New frits, each its name and text.
    pub frits: Vec<(String, String)>,
    /// Installed frits a save removes, by name.
    pub removals: Vec<String>,
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        *self == Pending::default()
    }

    /// The change a save makes to `key`.
    pub fn get(&self, key: &str) -> Option<&Change> {
        self.conf.iter().find(|(k, _)| k == key).map(|(_, c)| c)
    }

    /// Makes `change` the one change to `key`.
    pub fn set(&mut self, key: &str, change: Change) {
        self.conf.retain(|(k, _)| k != key);
        self.conf.push((key.to_owned(), change));
    }

    /// The font a save names in `font-name`.
    pub fn font_name(&self) -> Option<&str> {
        match self.get("font-name") {
            Some(Change::Set(name)) => Some(name),
            _ => None,
        }
    }

    /// The size a save names in `font-size`, 0 for automatic.
    pub fn font_size(&self) -> Option<u32> {
        match self.get("font-size") {
            Some(Change::Set(size)) => size.parse().ok(),
            _ => None,
        }
    }
}

/// The new-frit view's fields, in order: its name, three sources, its
/// cell and its size; the row after them adds the frit to the save.
pub const NEW_FIELDS: usize = 6;

/// The new frit being made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewFrit {
    /// The fields' text: name, source 1 to 3, cell, size.
    pub fields: [String; NEW_FIELDS],
    /// The highlighted row: a field, or [`NEW_FIELDS`] for the add row.
    pub at: usize,
}

impl NewFrit {
    /// The frit's name: the name field, else one made from the first
    /// source.
    pub fn name(&self) -> String {
        let [name, first, ..] = &self.fields;
        if name.trim().is_empty() {
            crate::commands::name_from(first.trim())
        } else {
            name.trim().to_owned()
        }
    }

    /// The frit's text, from its fields.
    pub fn text(&self) -> String {
        let [_, a, b, c, cell, size] = &self.fields;
        let fonts: Vec<String> = [a, b, c]
            .iter()
            .map(|f| f.trim().to_owned())
            .filter(|f| !f.is_empty())
            .collect();
        fn opt(f: &str) -> Option<&str> {
            Some(f.trim()).filter(|t| !t.is_empty())
        }
        crate::commands::frit_from(&self.name(), &fonts, opt(cell), opt(size))
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
    /// The Flow view's selected item.
    pub flow_at: Item,
    /// The tagged items, in the order they were tagged.
    pub tags: Vec<Item>,
    /// The selected item's card is open below the columns.
    pub opened: bool,
    /// The route card is open, with its cursor on this line.
    pub route: Option<usize>,
    /// The last `f`: the item it started from, which of that item's lines
    /// it took, and where it went, so the next `f` steps to the next line.
    followed: Option<(Item, usize, Item)>,
    /// The Help view shows About.
    pub about: bool,
    /// The Help view's first row in sight.
    pub help_at: usize,
    /// The view the install view goes back to.
    pub back: View,
    /// Mouse reporting is on.
    pub mouse: bool,
    /// The value being typed for the highlighted setting, while it is
    /// edited.
    pub editing: Option<String>,
    pub new_frit: NewFrit,
    pub pending: Pending,
    /// A line for the user, under the list.
    pub message: Option<String>,
    /// What the last failed save's command printed, shown in the save
    /// view.
    pub report: Vec<String>,
    /// The TUI is saving: the privileged step runs.
    pub saving: bool,
    /// The TUI is leaving with this status once the font is back.
    pub leaving: Option<u8>,
    /// A specimen picture is on the screen.
    pub picture: bool,
    /// The terminal's cell in pixels, where it reports its size in pixels.
    pub cell_px: Option<(u32, u32)>,
    /// The last summary of a size: the font, the height and what it gave.
    pub summary: Option<(String, u32, Result<crate::specimen::Summary, String>)>,
    /// The sources each frit lacks, as the helper found them.
    pub sources: std::collections::BTreeMap<String, SourcesFound>,
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
        let view = inputs.view;
        let flow_at = first_item(&inputs.flow);
        let mut app = App {
            inputs,
            cols,
            rows,
            view,
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
            flow_at,
            tags: Vec::new(),
            opened: false,
            route: None,
            followed: None,
            about: false,
            help_at: 0,
            back: View::Flow,
            mouse: false,
            editing: None,
            new_frit: NewFrit::default(),
            pending: Pending::default(),
            message: None,
            report: Vec::new(),
            saving: false,
            leaving: None,
            picture: false,
            cell_px: None,
            summary: None,
            sources: std::collections::BTreeMap::new(),
            replies_due: 0,
        };
        let mut first = vec![Effect::Write(probe()), Effect::Wait(WAIT)];
        first.extend(app.mouse_for_view());
        (app, first)
    }

    /// Turns mouse reporting on when the view is Flow and off when it is
    /// not, where it is not so already.
    fn mouse_for_view(&mut self) -> Vec<Effect> {
        let want = self.view == View::Flow;
        if want == self.mouse {
            return Vec::new();
        }
        self.mouse = want;
        vec![Effect::Write(
            if want { MOUSE_ON } else { MOUSE_OFF }.to_vec(),
        )]
    }

    /// Goes to `view`, turning the mouse on or off for it, and asks for
    /// what the view shows: a specimen, a font switch.
    fn set_view(&mut self, view: View) -> Vec<Effect> {
        self.view = view;
        self.editing = None;
        let mut out = self.mouse_for_view();
        out.extend(self.ask_picture());
        out.extend(self.follow());
        out
    }

    /// Tab's step to the next view, or Shift-Tab's to the one before.
    fn step_view(&mut self, forward: bool) -> Vec<Effect> {
        let tab = self.view.tab().unwrap_or(self.back);
        let at = View::TABS.iter().position(|v| *v == tab).unwrap_or(0);
        let n = View::TABS.len();
        let next = if forward {
            at.saturating_add(1) % n
        } else {
            at.saturating_add(n).saturating_sub(1) % n
        };
        let view = View::TABS.get(next).copied().unwrap_or(View::Flow);
        if view == View::Flow {
            self.flow_at = self.entry(forward);
        }
        self.set_view(view)
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
        match self.pending.get("allow-font-ops") {
            Some(Change::Switch(on)) => *on,
            Some(Change::Unset) => true,
            _ => self
                .inputs
                .conf
                .as_deref()
                .and_then(|c| conf_switch(c, "allow-font-ops"))
                .unwrap_or(true),
        }
    }

    /// The value a save leaves `key` at, as the settings view shows it,
    /// and whether it is the key's default.
    pub fn value(&self, key: &keys::Key) -> (String, bool) {
        let conf = self.inputs.conf.as_deref();
        match self.pending.get(key.name) {
            Some(Change::Set(v)) => (v.clone(), false),
            Some(Change::Switch(on)) => (if *on { "on" } else { "off" }.to_owned(), false),
            Some(Change::Unset) => crate::commands::value_text(key, None),
            None => {
                let (v, set) = crate::commands::value_text(key, conf);
                (v, !set)
            }
        }
    }

    /// Takes `event`, and returns what the loop does.
    pub fn event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Input(Input::Reply(reply)) => self.reply(reply),
            Event::Input(Input::Key(Key::Mouse(mouse))) => self.click(mouse),
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
            Event::Pictured(result) => self.pictured(result),
            Event::Summarized {
                font,
                height,
                summary,
            } => {
                self.summary = Some((font, height, summary));
                Vec::new()
            }
            Event::Sourced(found) => {
                self.sources.extend(found);
                Vec::new()
            }
        }
    }

    /// The size the size view highlights, in points, 0 for automatic.
    pub fn size(&self) -> u32 {
        match self.size_at {
            0 => 0,
            i => SIZES.get(i.saturating_sub(1)).copied().unwrap_or(0),
        }
    }

    /// The size a font the console cannot switch to is pictured at in the
    /// fonts view: the size a save names, else vtium.conf's `font-size`,
    /// else `preview`'s.
    fn preview_points(&self) -> u32 {
        self.pending
            .font_size()
            .or_else(|| {
                self.configured("font-size")
                    .and_then(|s| s.parse::<u32>().ok())
            })
            .filter(|s| *s > 0)
            .unwrap_or(crate::preview::POINTS)
    }

    /// The specimen for the highlighted size, where the terminal draws
    /// kitty graphics and the size is not automatic, and for the
    /// highlighted font where the console cannot be switched to it live
    /// (D4); otherwise the picture on the screen, if any, is deleted.
    fn ask_picture(&mut self) -> Vec<Effect> {
        let font = self.font().map(|f| f.name.clone());
        let (cols, rows) = super::view::picture_area(self);
        let (cw, ch) = self.cell_px.unwrap_or(ASSUMED_CELL);
        let max = (
            u32::try_from(cols)
                .unwrap_or(0)
                .saturating_mul(cw)
                .min(crate::specimen::PICTURE_MAX.0),
            u32::try_from(rows)
                .unwrap_or(0)
                .saturating_mul(ch)
                .min(crate::specimen::PICTURE_MAX.1),
        );
        let readable = !matches!(self.font().map(|f| &f.state), Some(FontState::Refused(_)));
        let not_live = matches!(self.live, Live::OpsOff | Live::NotVtium);
        let points = match self.view {
            View::Looks => self.size(),
            View::Fonts if not_live && readable => self.preview_points(),
            _ => 0,
        };
        let height = crate::specimen::pixel_height(points, self.inputs.dpi.0);
        match (self.graphics, points, font) {
            (true, 1.., Some(font)) if max.0 > 0 && max.1 > 0 => {
                vec![Effect::Picture { font, height, max }]
            }
            // a terminal that draws no pictures gets a size's summary
            (false, 1.., Some(font))
                if self.view == View::Looks
                    && self
                        .summary
                        .as_ref()
                        .is_none_or(|(f, h, _)| *f != font || *h != height) =>
            {
                vec![Effect::Summary { font, height }]
            }
            _ => self.drop_picture(),
        }
    }

    /// Deletes the picture on the screen, if any.
    fn drop_picture(&mut self) -> Vec<Effect> {
        if std::mem::take(&mut self.picture) {
            vec![Effect::Write(crate::kitty::delete(crate::kitty::IMAGE_ID))]
        } else {
            Vec::new()
        }
    }

    fn pictured(&mut self, result: Result<crate::specimen::Picture, String>) -> Vec<Effect> {
        let mut out = self.drop_picture();
        if !matches!(self.view, View::Looks | View::Fonts) || self.leaving.is_some() {
            return out;
        }
        match result {
            Ok(p) => {
                let (row, col) = super::view::picture_at(self);
                let mut bytes = format!("\x1b[{row};{col}H").into_bytes();
                bytes.extend(crate::kitty::show(
                    crate::kitty::IMAGE_ID,
                    p.width,
                    p.height,
                    &p.rgba,
                ));
                self.picture = true;
                out.push(Effect::Write(bytes));
            }
            Err(why) => self.message = Some(super::words::no_picture(why)),
        }
        out
    }

    fn reply(&mut self, reply: Reply) -> Vec<Effect> {
        if self.live == Live::Asking {
            match reply {
                Reply::Version(v) => self.answers.version = Some(v),
                Reply::Font(f) => self.answers.font = Some(f),
                Reply::Graphics(ok) => self.answers.graphics |= ok,
                Reply::Attributes => {
                    self.decide();
                    return self.started();
                }
            }
            return Vec::new();
        }
        let Reply::Font(name) = reply else {
            return Vec::new();
        };
        if self.leaving.is_some() {
            self.replies_due = self.replies_due.saturating_sub(1);
            self.in_flight = None;
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

    /// What follows once the terminal is known: the sources each frit that
    /// reads lacks, found once, and the highlighted font's picture where
    /// one shows.
    fn started(&mut self) -> Vec<Effect> {
        let frits: Vec<String> = self
            .inputs
            .fonts
            .iter()
            .filter(|f| f.state == FontState::Reads)
            .map(|f| f.name.clone())
            .collect();
        let mut out = Vec::new();
        if !frits.is_empty() {
            out.push(Effect::Sources(frits));
        }
        out.extend(self.ask_picture());
        out
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
            return self.started();
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
        let mouse = std::mem::take(&mut self.mouse);
        let mut bytes = self
            .drop_picture()
            .into_iter()
            .flat_map(|e| match e {
                Effect::Write(b) => b,
                _ => Vec::new(),
            })
            .collect::<Vec<u8>>();
        if mouse {
            bytes.extend_from_slice(MOUSE_OFF);
        }
        match (&self.live, &self.start_font) {
            (Live::On, Some(start)) if changed => {
                bytes.extend(switch(start));
                self.replies_due = usize::from(self.in_flight.is_some()).saturating_add(1);
                self.leaving = Some(status);
                self.message = Some(super::words::LEAVING.to_owned());
                vec![Effect::Write(bytes), Effect::Wait(WAIT)]
            }
            _ => {
                self.leaving = Some(status);
                let mut out = Vec::new();
                if !bytes.is_empty() {
                    out.push(Effect::Write(bytes));
                }
                out.push(Effect::Exit(status));
                out
            }
        }
    }

    /// Whether a key's character is text being typed: in the new-frit view
    /// on a field, and in the settings view while a value is edited. Esc
    /// drops what is typed in the settings view.
    fn typing(&self) -> bool {
        match self.view {
            View::New => self.new_frit.at < NEW_FIELDS,
            View::Settings => self.editing.is_some(),
            _ => false,
        }
    }

    fn key(&mut self, key: Key) -> Vec<Effect> {
        if self.leaving.is_some() || self.saving {
            return Vec::new();
        }
        if super::view::too_small(self) {
            return self.small_key(key);
        }
        if self.typing()
            && let Key::Char(_) | Key::Backspace = key
        {
            return self.type_key(key);
        }
        let route = self.view == View::Flow && self.route.is_some();
        match key {
            Key::Interrupt => return self.leave(130),
            Key::Char('q') if !route => return self.leave(0),
            _ => {}
        }
        if self.view == View::Flow {
            return self.flow_key(key);
        }
        match key {
            Key::Tab | Key::BackTab if self.view != View::Save => {
                return self.step_view(key == Key::Tab);
            }
            Key::Char('i') if self.view != View::Save => {
                self.back = self.view.tab().unwrap_or(View::Flow);
                let mut out = self.drop_picture();
                out.extend(self.set_view(View::Save));
                return out;
            }
            Key::Char('o') if self.live == Live::OpsOff => {
                self.pending.set("allow-font-ops", Change::Switch(true));
                self.message = Some(super::words::OPS_ON_PENDING.to_owned());
                return Vec::new();
            }
            _ => {}
        }
        match self.view {
            View::Flow => Vec::new(),
            View::Fonts => self.fonts_key(key),
            View::Looks => {
                let n = SIZES.len().saturating_add(1);
                let before = self.size_at;
                self.size_at = step(self.size_at, n, key);
                if key == Key::Enter {
                    let size = self.size().to_string();
                    self.pending.set("font-size", Change::Set(size));
                }
                if self.size_at == before {
                    Vec::new()
                } else {
                    self.ask_picture()
                }
            }
            View::Settings => {
                self.settings_key(key);
                Vec::new()
            }
            View::Help => {
                match key {
                    Key::Left | Key::Right => {
                        self.about = !self.about;
                        self.help_at = 0;
                    }
                    Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                        let rows = super::view::help_rows(self).len();
                        self.help_at = step(self.help_at, rows, key);
                    }
                    _ => {}
                }
                Vec::new()
            }
            View::New if key == Key::Escape => self.set_view(View::Fonts),
            View::New => {
                self.new_key(key);
                Vec::new()
            }
            View::Save => match key {
                Key::Backspace | Key::Left | Key::Escape => self.set_view(self.back),
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

    /// A key below the TUI's minimum size, where no view is drawn: `q`
    /// leaves, and `u` puts back the font the console had when the TUI
    /// started, where a preview changed it (ADR-1201 D3).
    fn small_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Interrupt => self.leave(130),
            Key::Char('q') => self.leave(0),
            Key::Char('u') => {
                let Some(start) = self.start_font.clone() else {
                    return Vec::new();
                };
                if let Some(at) = self.inputs.fonts.iter().position(|f| f.name == start) {
                    self.font_at = at;
                }
                if self.live != Live::On
                    || self.in_flight.is_some()
                    || self.shown_font.as_deref() == Some(start.as_str())
                {
                    return Vec::new();
                }
                self.in_flight = Some(start.clone());
                vec![Effect::Write(switch(&start))]
            }
            _ => Vec::new(),
        }
    }

    /// The item the Flow view selects when Tab enters it: the first item
    /// of the first column going forward, of the last going back, past the
    /// base card.
    fn entry(&self, forward: bool) -> Item {
        let columns = self.inputs.flow.columns();
        let mut order: Vec<&Vec<Item>> = columns.iter().collect();
        if !forward {
            order.reverse();
        }
        order
            .into_iter()
            .find_map(|c| c.iter().copied().find(|i| *i != Item::Base))
            .unwrap_or(Item::Base)
    }

    /// A key in the Flow view (ADR-1403 D7).
    fn flow_key(&mut self, key: Key) -> Vec<Effect> {
        if let Some(line) = self.route {
            return self.route_key(key, line);
        }
        self.message = None;
        let flow = &self.inputs.flow;
        let column = flow.column_of(self.flow_at);
        match key {
            Key::Tab | Key::Right => match self.next_column(column, true) {
                Some(item) => self.select(item),
                None if key == Key::Tab => return self.step_view(true),
                None => {}
            },
            Key::BackTab | Key::Left => match self.next_column(column, false) {
                Some(item) => self.select(item),
                None if key == Key::BackTab => return self.step_view(false),
                None => {}
            },
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                let items = flow.columns().get(column).cloned().unwrap_or_default();
                let at = items.iter().position(|i| *i == self.flow_at).unwrap_or(0);
                if let Some(item) = items.get(step(at, items.len(), key)) {
                    self.select(*item);
                }
            }
            Key::Enter => self.opened = true,
            Key::Char(' ') => {
                if !matches!(self.flow_at, Item::Base | Item::Policy | Item::New(_)) {
                    if let Some(at) = self.tags.iter().position(|t| *t == self.flow_at) {
                        self.tags.remove(at);
                    } else {
                        self.tags.push(self.flow_at);
                    }
                }
            }
            Key::Escape => {
                self.tags.clear();
                self.opened = false;
            }
            Key::Char('f') => self.follow_line(),
            Key::Char('?') => {
                let lines = super::flowdraw::route_card(&self.inputs.flow, self.flow_at).2;
                let at = lines
                    .iter()
                    .position(|l| l.item == Some(self.flow_at))
                    .or_else(|| lines.iter().position(|l| l.item.is_some()))
                    .unwrap_or(0);
                self.route = Some(at);
            }
            Key::Char('i') => {
                self.back = View::Flow;
                return self.set_view(View::Save);
            }
            Key::Char(c) => {
                let what = match c {
                    'c' => super::words::CONNECT,
                    'd' => super::words::READ_DEVICES,
                    'v' => super::words::VIEW_DRAFT,
                    'r' => super::words::REVERT,
                    'o' => super::words::RESTORE,
                    'n' => super::words::NEW_CONFIGURATION,
                    'x' => super::words::DELETE_DRAFT,
                    _ => return Vec::new(),
                };
                self.message = Some(super::words::not_built(what));
            }
            _ => {}
        }
        Vec::new()
    }

    /// Selects `item`; a `f` that follows starts again from it.
    fn select(&mut self, item: Item) {
        self.flow_at = item;
    }

    /// The item Tab or the arrows land on in the next column with items,
    /// right or left: the one nearest the selected item's row; `None` past
    /// the last or before the first.
    fn next_column(&self, column: usize, right: bool) -> Option<Item> {
        let flow = &self.inputs.flow;
        let columns = flow.columns();
        let ctx = super::flowdraw::Ctx {
            flow,
            tags: &self.tags,
            color: self.inputs.color,
        };
        let placed = super::flowdraw::stack(&ctx, &super::flowdraw::geometry(self.cols));
        let row_of = |item: Item| placed.iter().find(|p| p.item == item).map_or(0, |p| p.row);
        let here = row_of(self.flow_at);
        let mut c = column;
        loop {
            c = if right {
                c.checked_add(1).filter(|c| *c < COLUMNS)?
            } else {
                c.checked_sub(1)?
            };
            let items: Vec<Item> = columns
                .get(c)
                .map(|items| items.iter().copied().filter(|i| *i != Item::Base).collect())
                .unwrap_or_default();
            if let Some(best) = items
                .iter()
                .min_by_key(|i| row_of(**i).abs_diff(here))
                .copied()
            {
                return Some(best);
            }
            // a column 0 with no display holds only the base card
            if c == 0 && columns.first().is_some_and(|c| c.contains(&Item::Base)) {
                return Some(Item::Base);
            }
        }
    }

    /// `f`: moves to the item the selected one's line reaches, toward the
    /// heads; pressed again where it landed, it steps through the other
    /// lines of the item it started from.
    fn follow_line(&mut self) {
        let flow = &self.inputs.flow;
        if let Some((origin, k, landed)) = self.followed
            && landed == self.flow_at
        {
            let all = flow.follows(origin);
            if all.len() > 1 {
                let next = k.saturating_add(1) % all.len();
                if let Some(item) = all.get(next) {
                    self.followed = Some((origin, next, *item));
                    self.flow_at = *item;
                    return;
                }
            }
        }
        match flow.follows(self.flow_at).first() {
            Some(item) => {
                self.followed = Some((self.flow_at, 0, *item));
                self.flow_at = *item;
            }
            None => self.message = Some(super::words::NOTHING_TO_FOLLOW.to_owned()),
        }
    }

    /// A key while the route card is open: Up and Down move along its
    /// items, Enter opens the item under the cursor, `q` and Esc go back to
    /// the flow.
    fn route_key(&mut self, key: Key, line: usize) -> Vec<Effect> {
        let lines = super::flowdraw::route_card(&self.inputs.flow, self.flow_at).2;
        let items: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.item.is_some())
            .map(|(i, _)| i)
            .collect();
        let at = items.iter().position(|i| *i == line).unwrap_or(0);
        match key {
            Key::Char('q') | Key::Escape => self.route = None,
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                self.route = items
                    .get(step(at, items.len(), key))
                    .copied()
                    .or(Some(line));
            }
            Key::Enter => {
                if let Some(item) = lines.get(line).and_then(|l| l.item) {
                    self.flow_at = item;
                    self.opened = true;
                }
                self.route = None;
            }
            _ => {}
        }
        Vec::new()
    }

    /// A mouse event, which only the Flow view asks for: a click on a
    /// view's name switches to it; a click on an item selects it, and a
    /// second click on the selected item opens it; the wheel moves as Up
    /// and Down do.
    fn click(&mut self, mouse: Mouse) -> Vec<Effect> {
        if self.view != View::Flow
            || self.leaving.is_some()
            || self.saving
            || super::view::too_small(self)
        {
            return Vec::new();
        }
        match mouse.button {
            Button::WheelUp => return self.key(Key::Up),
            Button::WheelDown => return self.key(Key::Down),
            Button::Press(0) => {}
            _ => return Vec::new(),
        }
        if self.route.is_some() {
            return Vec::new();
        }
        if let Some(view) = super::view::tab_at(self, mouse.col, mouse.row) {
            if view == View::Flow {
                return Vec::new();
            }
            return self.set_view(view);
        }
        match super::view::item_at(self, mouse.col, mouse.row) {
            Some(item) if item == self.flow_at => self.opened = true,
            Some(item) => {
                self.message = None;
                self.select(item);
            }
            None => {}
        }
        Vec::new()
    }

    fn fonts_key(&mut self, key: Key) -> Vec<Effect> {
        if key == Key::Char('r') {
            self.remove_key();
            return Vec::new();
        }
        if key == Key::Char('n') {
            return self.set_view(View::New);
        }
        let before = self.font_at;
        let n = self.inputs.fonts.len();
        self.font_at = step(self.font_at, n, key);
        if key == Key::Enter
            && let Some(font) = self.font()
            && !matches!(font.state, FontState::Refused(_))
            && !self.refused.contains(&font.name)
        {
            let name = font.name.clone();
            self.pending.set("font-name", Change::Set(name));
        }
        let mut out = self.follow();
        if matches!(self.live, Live::OpsOff | Live::NotVtium) && self.font_at != before {
            out.extend(self.ask_picture());
        }
        out
    }

    /// `r` in the fonts view: the highlighted frit is removed when the user
    /// saves, or, when a save already removes it, no longer is. The TUI
    /// refuses what `remove-frit` refuses, by vtium.conf, its top level and
    /// its looks, both as it is and as the save leaves it, and a frit the save
    /// replaces with a new one. The save view lists the frit's file among
    /// what the save changes, and `apply` removes it, as it removes the
    /// frit of `remove-frit`'s draft.
    fn remove_key(&mut self) {
        let Some(name) = self.font().map(|f| f.name.clone()) else {
            return;
        };
        if let Some(at) = self.pending.removals.iter().position(|n| *n == name) {
            self.pending.removals.remove(at);
            self.message = Some(super::words::remove_dropped(&name));
            return;
        }
        let refused = crate::commands::removal_refusal(&name, self.inputs.conf.as_deref())
            .or_else(|| {
                self.pending
                    .frits
                    .iter()
                    .any(|(n, _)| *n == name)
                    .then(|| super::words::remove_replaced(&name))
            })
            .or_else(|| {
                (!self.inputs.installed.contains_key(&name))
                    .then(|| super::words::remove_unread(&name))
            });
        if let Some(why) = refused {
            self.message = Some(super::words::remove_refused(why));
            return;
        }
        self.pending.removals.push(name.clone());
        // the save as it would be with the removal: refused when what the
        // save leaves in vtium.conf names the frit
        match self.plan() {
            Ok(_) => self.message = Some(super::words::remove_pending(&name)),
            Err(why) => {
                self.pending.removals.retain(|n| *n != name);
                self.message = Some(super::words::remove_refused(why));
            }
        }
    }

    /// A character or Backspace typed into the field being edited.
    fn type_key(&mut self, key: Key) -> Vec<Effect> {
        let text = match self.view {
            View::New => self.new_frit.fields.get_mut(self.new_frit.at),
            _ => self.editing.as_mut(),
        };
        if let Some(text) = text {
            match key {
                Key::Char(c) if !c.is_control() && text.len() < TYPED_MAX => text.push(c),
                Key::Backspace => {
                    text.pop();
                }
                _ => {}
            }
        }
        Vec::new()
    }

    /// A key in the settings view: Enter turns a switch on or off, steps a
    /// key of a fixed set of words to its next word, and starts typing any
    /// other key's value, which a second Enter keeps; an empty value goes
    /// back to the default. Moving away drops what is typed.
    fn settings_key(&mut self, key: Key) {
        let all = setting_keys();
        if let Some(typed) = self.editing.take() {
            if key == Key::Enter
                && let Some(k) = all.get(self.setting_at)
            {
                self.change(k, typed.trim());
            }
            return;
        }
        self.setting_at = step(self.setting_at, all.len(), key);
        let Some(k) = all.get(self.setting_at).copied() else {
            return;
        };
        if key != Key::Enter {
            return;
        }
        let (now, _) = self.value(k);
        match k.kind {
            Kind::Switch => {
                let on = now != "on";
                self.pending.set(k.name, Change::Switch(on));
            }
            Kind::Choice(words) => {
                let at = words.iter().position(|w| *w == now);
                let next = at.map_or(0, |i| i.saturating_add(1) % words.len().max(1));
                if let Some(w) = words.get(next) {
                    self.pending.set(k.name, Change::Set((*w).to_owned()));
                }
            }
            _ => {
                let (value, default) = self.value(k);
                self.editing = Some(if default { String::new() } else { value });
            }
        }
    }

    /// Sets `key` to the typed `value` as a change the save makes, or back
    /// to its default when `value` is empty; a value vtium would not read
    /// back says why on the message line.
    fn change(&mut self, key: &'static keys::Key, value: &str) {
        if value.is_empty() {
            self.pending.set(key.name, Change::Unset);
            return;
        }
        let change = crate::commands::set_edit(key, value).and_then(|e| {
            edit_conf("", &[e]).map_err(|e| super::words::conf_edit(&e))?;
            Ok(match e {
                Edit::Switch(_, on) => Change::Switch(on),
                _ => Change::Set(value.to_owned()),
            })
        });
        match change {
            Ok(change) => self.pending.set(key.name, change),
            Err(why) => self.message = Some(why),
        }
    }

    /// A key in the new-frit view: Up and Down move between the fields,
    /// Enter moves to the next, and Enter on the last row adds the frit to
    /// the save, once the frit reader takes it, and shows the save.
    fn new_key(&mut self, key: Key) {
        let rows = NEW_FIELDS.saturating_add(1);
        let at = self.new_frit.at;
        self.new_frit.at = match key {
            Key::Enter if at < NEW_FIELDS => at.saturating_add(1),
            _ => step(at, rows, key),
        };
        if key != Key::Enter || at != NEW_FIELDS {
            return;
        }
        let name = self.new_frit.name();
        let text = self.new_frit.text();
        let path = self.inputs.frits.join(format!("{name}.vtfrit"));
        let read =
            if !crate::frit::valid_name(&name) || crate::frit::RESERVED.contains(&name.as_str()) {
                Err(super::words::NEW_BAD_NAME.to_owned())
            } else if self
                .inputs
                .installed
                .get(&name)
                .is_some_and(|b| std::str::from_utf8(b).is_err())
            {
                Err(super::words::new_replaces_binary(&name))
            } else {
                crate::frit::Frit::parse(text.as_bytes(), &path, &name)
                    .map(|_| ())
                    .map_err(|e| e.why)
            };
        match read {
            Ok(()) => {
                self.pending.frits.retain(|(n, _)| *n != name);
                self.pending.removals.retain(|n| *n != name);
                self.pending.frits.push((name.clone(), text));
                self.new_frit = NewFrit::default();
                self.message = Some(super::words::new_added(&name));
                self.back = View::Fonts;
                self.view = View::Save;
            }
            Err(why) => self.message = Some(super::words::new_refused(why)),
        }
    }

    /// What saving now would change: vtium.conf with the pending keys at
    /// its top level, through the line editor (ADR-1403 D8).
    pub fn plan(&self) -> Result<Plan, String> {
        let mut files = Vec::new();
        let p = &self.pending;
        // the frits first, as apply writes them, so the save view lists
        // them in the order they are installed
        for (name, text) in &p.frits {
            let path = self.inputs.frits.join(format!("{name}.vtfrit"));
            let old = self
                .inputs
                .installed
                .get(name)
                .and_then(|b| String::from_utf8(b.clone()).ok());
            files.push(FileEdit {
                path,
                old,
                new: text.clone(),
            });
        }
        let edits: Vec<Edit<'_>> = p
            .conf
            .iter()
            .map(|(key, change)| match change {
                Change::Set(v) => Edit::Set(key, v),
                Change::Switch(on) => Edit::Switch(key, *on),
                Change::Unset => Edit::Unset(key),
            })
            .collect();
        if !edits.is_empty() {
            let old = self.inputs.conf.clone();
            let new = edit_conf(old.as_deref().unwrap_or_default(), &edits)
                .map_err(|e| super::words::conf_edit(&e))?;
            if old.as_deref() != Some(&new) {
                files.push(FileEdit {
                    path: self.inputs.conf_path.clone(),
                    old,
                    new,
                });
            }
        }
        // a frit is removed only when the save leaves nothing naming it
        let after = |path: &PathBuf, now: &Option<String>| {
            files
                .iter()
                .find(|f| f.path == *path)
                .map(|f| f.new.clone())
                .or_else(|| now.clone())
        };
        let conf = after(&self.inputs.conf_path, &self.inputs.conf);
        // the frits' paths come from the directory the TUI started with
        let key = keys::FONT_FRITS.name;
        if !(p.frits.is_empty() && p.removals.is_empty())
            && conf.as_deref().and_then(|c| setting(c, key)) != self.configured(key)
        {
            return Err(crate::apply::FRITS_MOVE_FIRST.to_owned());
        }
        let mut removals = Vec::new();
        for name in &p.removals {
            if let Some(why) = crate::commands::removal_refusal(name, conf.as_deref()) {
                return Err(why);
            }
            let old = self
                .inputs
                .installed
                .get(name)
                .cloned()
                .ok_or_else(|| super::words::remove_unread(name))?;
            removals.push(Removal {
                path: self.inputs.frits.join(format!("{name}.vtfrit")),
                old,
            });
        }
        Ok(Plan { files, removals })
    }

    fn saved(&mut self, result: Result<usize, String>) -> Vec<Effect> {
        self.saving = false;
        match result {
            Ok(files) => {
                if let Ok(plan) = self.plan() {
                    for f in plan.files {
                        if f.path == self.inputs.conf_path {
                            self.inputs.conf = Some(f.new);
                        }
                    }
                }
                // a removed frit leaves the list
                let removed = std::mem::take(&mut self.pending.removals);
                self.inputs.fonts.retain(|f| !removed.contains(&f.name));
                for name in &removed {
                    self.inputs.installed.remove(name);
                    self.sources.remove(name);
                }
                self.font_at = self.font_at.min(self.inputs.fonts.len().saturating_sub(1));
                // the saved font is the console's from here, the font
                // leaving gives back (D4)
                if self.live == Live::On
                    && let Some(name) = self.pending.font_name()
                {
                    self.start_font = Some(name.to_owned());
                }
                self.pending = Pending::default();
                self.report.clear();
                self.view = self.back;
                self.message = Some(super::words::saved(files));
            }
            Err(why) => {
                let mut lines = why.lines().map(str::to_owned);
                self.message = Some(super::words::save_failed(lines.next().unwrap_or_default()));
                self.report = lines.filter(|l| !l.trim().is_empty()).collect();
            }
        }
        self.mouse_for_view()
    }
}

/// The item the Flow view selects first: the first display, else the
/// first item of the first column that has one.
fn first_item(flow: &Flow) -> Item {
    flow.columns()
        .iter()
        .find_map(|c| {
            c.iter()
                .copied()
                .find(|i| *i != Item::Base && !matches!(i, Item::New(_)))
        })
        .unwrap_or(Item::Base)
}

/// The most bytes a typed field holds.
pub const TYPED_MAX: usize = 256;

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
