//! The TUI as a state machine apart from its terminal (ADR-1201 D7): its
//! state takes [`Event`]s, a key, a mouse click, a reply, a resize, a
//! signal or the end of a wait, and returns [`Effect`]s, bytes to write, a
//! wait to start, a save to run or the status to leave with. The loop
//! around it ([`super::port`]) does the reading and the writing, and draws
//! [`super::view`]'s screen after each event.
//!
//! The views are Flow, Looks, Fonts, Settings and Help (ADR-1201 D3), and
//! the TUI opens on the one its caller names. The Flow view draws the
//! configuration as paths of parts (ADR-1403 D7, [`super::flowview`]), and
//! its keys are in `flowkeys`.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use vtium_settings::keys::{self, Kind, Scope};
use vtium_settings::{setting, switch as conf_switch};

use super::desk::{Done, Kept, Op, Opened};
use super::edit::Change as DraftChange;
use super::flow::{Flow, Item};
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
    /// The loop has a desk, so the Flow view shows the user's draft and
    /// its changes go into it (ADR-1403 D8); without one it shows
    /// [`Inputs::flow`] and changes nothing.
    pub desk: bool,
    /// The loop watches udev's device events, so the view follows hotplug
    /// (ADR-1403 D7).
    pub hotplug: bool,
    /// The Flow view's colors, from `vtium-config.conf`
    /// ([`super::flowcolor`]).
    pub flow_colors: super::flowcolor::Palette,
    /// What the message line says once about that file: where vtium-config
    /// wrote it, or a line of it that it skipped.
    pub colors_note: Option<String>,
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
    /// The privileged step ended: the draft installed, or why not.
    Installed(Result<Done, String>),
    /// The font's cell [`Effect::Cell`] asked for, and the height its
    /// faces draw at, or why there is none.
    Celled {
        font: String,
        height: u32,
        cell: Result<(vtium_font::Cell, u32), String>,
    },
    /// The picture [`Effect::Picture`] asked for, or why there is none.
    Pictured(Result<crate::specimen::Picture, String>),
    /// The detailed specimen [`Effect::Specimen`] asked for, and what it
    /// gives, or why there is none.
    Detailed {
        font: String,
        height: u32,
        result: Result<(crate::specimen::Picture, crate::specimen::Summary), String>,
    },
    /// The summary [`Effect::Summary`] asked for.
    Summarized {
        font: String,
        height: u32,
        summary: Result<crate::specimen::Summary, String>,
    },
    /// The sources each frit [`Effect::Sources`] named lacks.
    Sourced(Vec<(String, SourcesFound)>),
    /// The desk did the op [`Effect::Desk`] asked for, or says why not.
    Desked(Op, Result<Done, String>),
    /// udev reported that a device came or went.
    Hotplug,
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
    /// Runs the privileged step that installs the plan, with the desk,
    /// then sends [`Event::Installed`].
    Install(Plan),
    /// Draws the detailed specimen of the font `font` at `height` pixels,
    /// at most `max` pixels, then sends [`Event::Pictured`].
    Specimen {
        font: String,
        height: u32,
        max: (u32, u32),
        /// The display whose grid the specimen's summary counts.
        display: (u32, u32),
    },
    /// Asks the glyph helper for the font `font`'s cell at `height`
    /// pixels, then sends [`Event::Celled`].
    Cell { font: String, height: u32 },
    /// Draws the picture of the console the shot names (ADR-1201 D4),
    /// then sends [`Event::Pictured`].
    Picture(crate::specimen::Shot),
    /// Summarizes the specimen of the font `font` at `height` pixels, for
    /// a terminal that draws no pictures, then sends [`Event::Summarized`].
    Summary { font: String, height: u32 },
    /// Asks the glyph helper which sources each frit named lacks, then
    /// sends [`Event::Sourced`].
    Sources(Vec<String>),
    /// Hands the op to the desk, then sends [`Event::Desked`].
    Desk(Op),
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
    /// new-frit view.
    pub const fn tab(self) -> View {
        match self {
            View::New => View::Fonts,
            v => v,
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

/// The frits an install writes and removes with the draft. Unlike a
/// key, a frit is a file of its own, which no draft holds, so the TUI
/// keeps it until Install, which writes the frits before vtium.conf
/// (ADR-1403 D8), or until the user leaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    /// New frits, each its name and text.
    pub frits: Vec<(String, String)>,
    /// Installed frits the install removes, by name.
    pub removals: Vec<String>,
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        *self == Pending::default()
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
    /// The font the console is to draw, which a switch is sent for once no
    /// other is in flight: the live trial's font, or the font it puts
    /// back (D4).
    pub want: Option<String>,
    /// The Fonts view shows the detailed specimen of the highlighted font,
    /// as it does at start, in place of the picture of the console, which
    /// `s` switches to (ADR-1201 D4).
    pub detail: bool,
    /// What the last detailed specimen gave: its font, its height and its
    /// summary.
    pub specimen: Option<(String, u32, crate::specimen::Summary)>,
    /// The live trial counting down, if one is.
    pub trial: Option<super::shot::Trial>,
    /// The picture last asked for, so the same one is not asked again.
    pub(crate) asked: Option<super::shot::Asked>,
    /// A preview is being drawn: the next is asked for once it comes, so
    /// one is drawn at a time.
    pub(crate) drawing: bool,
    /// What the picture on the screen shows: the line beside it.
    pub shot: Option<super::shot::Caption>,
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
    /// The Help view shows About.
    pub about: bool,
    /// The Help view's first row in sight.
    pub help_at: usize,
    /// Mouse reporting is on.
    pub mouse: bool,
    /// The value being typed for the highlighted setting, while it is
    /// edited.
    pub editing: Option<String>,
    pub new_frit: NewFrit,
    pub pending: Pending,
    /// A line for the user, under the list.
    pub message: Option<String>,
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
    /// The draft the Flow view shows, as the desk last read or wrote it.
    pub draft: Option<Opened>,
    /// The devices present, which the flow places on the draft.
    pub present: crate::flowmodel::Present,
    /// The question the view asks before it acts, and what it does on yes.
    pub confirm: Option<Confirm>,
    /// The read-only pager over the draft, when it is open.
    pub pager: Option<Pager>,
    /// The installed history Restore lists, and the entry highlighted.
    pub restore: Option<(Vec<Kept>, usize)>,
    /// The privileged step is installing the draft.
    pub installing: bool,
    /// What came and went at the last hotplug, which the status line shows
    /// until the next change.
    pub came: Option<String>,
    /// The item to select once the desk answers the change in flight.
    pub(crate) select_after: Option<Item>,
    /// The Flow view's state: where its highlight is and what is open
    /// over it.
    pub flow_view: super::flowview::State,
    /// The notice that the live trial is off has been shown.
    live_said: bool,
    /// The changes made since the draft was last read or saved, which
    /// [`App::draft`]'s text holds and the store does not yet (ADR-1403 D7,
    /// "Changes wait for Save").
    pub unsaved: usize,
}

/// A question the Flow view asks before it acts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub question: String,
    pub then: Then,
}

/// What a yes does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Then {
    Change(DraftChange),
    Desk(Op),
    Install(Plan),
    /// Leaves with this status, the pending frits lost; `i` installs
    /// them instead.
    Leave(u8),
    /// Leaves with this status: `y` saves the unsaved changes first, `n`
    /// leaves without them.
    Quit(u8),
}

/// The pager's place in the draft: its first line in sight and how far it
/// is scrolled sideways.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pager {
    pub top: usize,
    pub left: usize,
    /// The line the card selected in the flow sits on, counted from 1.
    pub mark: usize,
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
            want: None,
            detail: true,
            specimen: None,
            trial: None,
            asked: None,
            drawing: false,
            shot: None,
            shown_font: None,
            in_flight: None,
            refused: BTreeSet::new(),
            font_at,
            size_at: 0,
            setting_at: 0,
            flow_at,
            about: false,
            help_at: 0,
            mouse: false,
            editing: None,
            new_frit: NewFrit::default(),
            pending: Pending::default(),
            message: None,
            leaving: None,
            picture: false,
            cell_px: None,
            summary: None,
            sources: std::collections::BTreeMap::new(),
            replies_due: 0,
            draft: None,
            present: crate::flowmodel::Present::default(),
            confirm: None,
            pager: None,
            restore: None,
            installing: false,
            came: None,
            select_after: None,
            flow_view: super::flowview::State::default(),
            live_said: false,
            unsaved: 0,
        };
        // the Flow view's highlight on its first box from the first frame,
        // and what reading the color file found on the message line
        app.flow_settle();
        app.message = app
            .message
            .take()
            .or_else(|| app.inputs.colors_note.clone());
        let mut first = vec![Effect::Write(probe()), Effect::Wait(WAIT)];
        if app.inputs.desk {
            first.push(Effect::Desk(Op::Open));
        }
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
    pub(crate) fn set_view(&mut self, view: View) -> Vec<Effect> {
        self.view = view;
        self.editing = None;
        self.say_live_off();
        self.mouse_for_view()
    }

    /// Says once, on the first view other than Flow, that the live trial
    /// is off and why. The Flow view says it only when `t` is pressed
    /// (ADR-1403 D7).
    fn say_live_off(&mut self) {
        if self.view == View::Flow || self.live_said {
            return;
        }
        let notice = match self.live {
            Live::OpsOff => super::words::OPS_OFF,
            Live::NotVtium => super::words::NOT_VTIUM,
            _ => return,
        };
        self.live_said = true;
        self.message = Some(notice.to_owned());
    }

    /// Tab's step to the next view, or Shift-Tab's to the one before. The
    /// Flow view keeps the item it had selected.
    fn step_view(&mut self, forward: bool) -> Vec<Effect> {
        let tab = self.view.tab();
        let at = View::TABS.iter().position(|v| *v == tab).unwrap_or(0);
        let n = View::TABS.len();
        let next = if forward {
            at.saturating_add(1) % n
        } else {
            at.saturating_add(n).saturating_sub(1) % n
        };
        let view = View::TABS.get(next).copied().unwrap_or(View::Flow);
        self.set_view(view)
    }

    /// Whether something is open in the view that a change of view would
    /// leave behind (#355): a question, and in the Flow view the file
    /// view, Restore's list, or a detail, a part maker or a picker; in
    /// the Settings view a value being typed; and the new-frit view, a
    /// form of the Fonts view's. While one is, Tab, Shift-Tab and a click
    /// on a view's name do not change the view. Esc closes each, and a
    /// question takes any key but its yes as no.
    fn holds_view(&self) -> bool {
        if self.confirm.is_some() {
            return true;
        }
        match self.view {
            View::Flow => {
                self.pager.is_some() || self.restore.is_some() || self.flow_view.layer.is_some()
            }
            View::Settings => self.editing.is_some(),
            View::New => true,
            View::Looks | View::Fonts | View::Help => false,
        }
    }

    /// The highlighted font.
    pub fn font(&self) -> Option<&Font> {
        self.inputs.fonts.get(self.font_at)
    }

    /// The installed vtium.conf's value of `key`.
    pub fn configured(&self, key: &str) -> Option<String> {
        self.inputs.conf.as_deref().and_then(|c| setting(c, key))
    }

    /// The configuration the views show: the draft, else the installed
    /// vtium.conf the TUI read at start.
    pub fn conf_text(&self) -> Option<&str> {
        self.draft
            .as_ref()
            .map(|d| d.text.as_str())
            .or(self.inputs.conf.as_deref())
    }

    /// The draft's value of `key`, else the installed file's.
    pub fn drafted(&self, key: &str) -> Option<String> {
        self.conf_text().and_then(|c| setting(c, key))
    }

    /// Whether the draft gives `key` another value than the installed
    /// file.
    pub fn changed(&self, key: &str) -> bool {
        self.draft.is_some() && self.drafted(key) != self.configured(key)
    }

    /// Whether the draft leaves `allow-font-ops` on, its default.
    pub fn font_ops(&self) -> bool {
        self.conf_text()
            .and_then(|c| conf_switch(c, "allow-font-ops"))
            .unwrap_or(true)
    }

    /// The value the draft gives `key`, as the settings view shows it,
    /// and whether it is the key's default.
    pub fn value(&self, key: &keys::Key) -> (String, bool) {
        let (v, set) = crate::commands::value_text(key, self.conf_text());
        (v, !set)
    }

    /// Takes `event`, and returns what the loop does.
    pub fn event(&mut self, event: Event) -> Vec<Effect> {
        let mut out = self.handle(event);
        out.extend(self.ask_picture());
        self.flow_settle();
        out
    }

    fn handle(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Input(Input::Reply(reply)) => self.reply(reply),
            Event::Input(Input::Key(Key::Mouse(mouse))) => self.click(mouse),
            Event::Input(Input::Key(key)) => self.key(key),
            Event::Resize { cols, rows } => {
                self.cols = cols;
                self.rows = rows;
                Vec::new()
            }
            // a signal saves the unsaved changes before the TUI leaves
            // (ADR-1403 D7, What carries over)
            Event::Signal(n) => {
                let status = u8::try_from(n.saturating_add(128)).unwrap_or(1);
                self.save_and_leave(status)
            }
            Event::Timeout => self.timeout(),
            Event::Installed(result) => self.installed(result),
            Event::Celled { font, height, cell } => self.celled(&font, height, cell),
            Event::Pictured(result) => self.pictured(result),
            Event::Detailed {
                font,
                height,
                result,
            } => self.detailed(font, height, result),
            Event::Summarized {
                font,
                height,
                summary,
            } => {
                self.drawing = false;
                self.summary = Some((font, height, summary));
                Vec::new()
            }
            Event::Sourced(found) => {
                self.sources.extend(found);
                Vec::new()
            }
            Event::Desked(op, done) => self.desked(&op, done),
            Event::Hotplug => vec![Effect::Desk(Op::Devices)],
        }
    }

    /// The size the size view highlights, in points, 0 for automatic.
    pub fn size(&self) -> u32 {
        match self.size_at {
            0 => 0,
            i => SIZES.get(i.saturating_sub(1)).copied().unwrap_or(0),
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
            self.refused.insert(asked.clone());
            if self.want.as_deref() == Some(asked.as_str()) {
                self.want = Some(name.clone());
            }
            if self.trial.as_ref().is_some_and(|t| t.font == asked) {
                self.trial = None;
                self.message = Some(super::words::trial_refused(&asked));
            }
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
            (true, None) => Live::OpsOff,
            (false, _) => Live::NotVtium,
        };
        self.say_live_off();
    }

    fn timeout(&mut self) -> Vec<Effect> {
        if self.live == Live::Asking {
            self.decide();
            return self.started();
        }
        match self.leaving {
            Some(status) => vec![Effect::Exit(status)],
            None => self.tick(),
        }
    }

    /// Sends the font the console is to draw when no switch is outstanding
    /// and the console draws another (D4): one switch at a time, naming
    /// whatever is wanted when the last reply came.
    pub(crate) fn follow(&mut self) -> Vec<Effect> {
        if self.live != Live::On || self.in_flight.is_some() {
            return Vec::new();
        }
        let Some(name) = self.want.clone() else {
            return Vec::new();
        };
        if self.shown_font.as_deref() == Some(&name) || self.refused.contains(&name) {
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
        self.trial = None;
        // the font the console ends in once the switch in flight, if any,
        // is answered: the start font is sent only where it is another
        let last = self.in_flight.as_ref().or(self.shown_font.as_ref());
        let back = last != self.start_font.as_ref();
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
            (Live::On, Some(start)) if back => {
                bytes.extend(switch(start));
                self.replies_due = usize::from(self.in_flight.is_some()).saturating_add(1);
                self.leaving = Some(status);
                self.message = Some(super::words::LEAVING.to_owned());
                vec![Effect::Write(bytes), Effect::Wait(WAIT)]
            }
            // the switch in flight gives the start font back: its reply
            // is waited for, and nothing is sent again
            (Live::On, Some(_)) if self.in_flight.is_some() => {
                self.replies_due = 1;
                self.leaving = Some(status);
                self.message = Some(super::words::LEAVING.to_owned());
                let mut out = Vec::new();
                if !bytes.is_empty() {
                    out.push(Effect::Write(bytes));
                }
                out.push(Effect::Wait(WAIT));
                out
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
        if self.leaving.is_some() || self.installing {
            return Vec::new();
        }
        if self.trial.is_some() {
            return self.trial_key(key);
        }
        if super::view::too_small(self) {
            return self.small_key(key);
        }
        // the name of a new terminal takes every key while it is typed
        if self.flow_typing() {
            return self.flow_key(key);
        }
        if self.typing()
            && let Key::Char(_) | Key::Backspace = key
        {
            return self.type_key(key);
        }
        if self.confirm.is_some() {
            return self.confirm_key(key);
        }
        let modal = self.view == View::Flow && (self.pager.is_some() || self.restore.is_some());
        match key {
            Key::Interrupt => return self.save_and_leave(130),
            Key::Char('q') if !modal => return self.quit(0),
            // Tab and Shift-Tab step the views wherever the selection is,
            // and the arrows move inside a view (#355)
            Key::Tab | Key::BackTab if self.holds_view() => return Vec::new(),
            Key::Tab | Key::BackTab => return self.step_view(key == Key::Tab),
            _ => {}
        }
        if self.view == View::Flow {
            return self.flow_key(key);
        }
        match key {
            Key::Char('i') if !self.typing() => return self.action('i'),
            // Save in the views whose own keys leave `s` free (ADR-1403 D7)
            Key::Char('s')
                if matches!(self.view, View::Looks | View::Settings) && !self.typing() =>
            {
                return self.save();
            }
            Key::Char('o') if self.live == Live::OpsOff => {
                return self.top("allow-font-ops", Some("on"));
            }
            _ => {}
        }
        match self.view {
            View::Flow => Vec::new(),
            View::Fonts => self.fonts_key(key),
            View::Looks => {
                let n = SIZES.len().saturating_add(1);
                self.size_at = step(self.size_at, n, key);
                if key == Key::Enter {
                    let size = self.size().to_string();
                    return self.top("font-size", Some(&size));
                }
                Vec::new()
            }
            View::Settings => self.settings_key(key),
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
                self.want = Some(start);
                self.follow()
            }
            _ => Vec::new(),
        }
    }

    /// A key in the Flow view (ADR-1403 D7) while a question, the file
    /// view or Restore's list is open over it.
    pub(crate) fn flow_open_key(&mut self, key: Key) -> Vec<Effect> {
        if self.confirm.is_some() {
            return self.confirm_key(key);
        }
        if self.pager.is_some() {
            self.pager_key(key);
            return Vec::new();
        }
        if self.restore.is_some() {
            return self.restore_key(key);
        }
        Vec::new()
    }

    /// A mouse event, which only the Flow view asks for: a click on a
    /// view's name switches to it, as Tab does, while nothing is open that
    /// the switch would leave behind; a click on an item selects it, and a
    /// second click on the selected item opens it; the wheel moves as Up
    /// and Down do.
    fn click(&mut self, mouse: Mouse) -> Vec<Effect> {
        if self.trial.is_some() && self.leaving.is_none() && !self.installing {
            return self.put_back();
        }
        if self.view != View::Flow
            || self.leaving.is_some()
            || self.installing
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
        if let Some(view) = super::view::tab_at(self, mouse.col, mouse.row) {
            if view == View::Flow || self.holds_view() {
                return Vec::new();
            }
            return self.set_view(view);
        }
        if super::flowdraw::shown(self) {
            return self.flow_click(mouse.col, mouse.row);
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
        if key == Key::Char('s') {
            self.detail = !self.detail;
            return Vec::new();
        }
        if key == Key::Char('t') {
            let name = self
                .font()
                .filter(|f| !matches!(f.state, FontState::Refused(_)))
                .map(|f| f.name.clone());
            return match name {
                Some(name) => self.try_live(name),
                None => Vec::new(),
            };
        }
        let n = self.inputs.fonts.len();
        self.font_at = step(self.font_at, n, key);
        let mut out = Vec::new();
        if key == Key::Enter
            && let Some(font) = self.font()
            && !matches!(font.state, FontState::Refused(_))
            && !self.refused.contains(&font.name)
        {
            let name = font.name.clone();
            out.extend(self.top("font-name", Some(&name)));
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
        let refused = crate::commands::removal_refusal(&name, self.conf_text())
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
            self.message = Some(super::words::remove_refused(&name, why));
            return;
        }
        self.pending.removals.push(name.clone());
        // the install as it would be with the removal: refused when the
        // draft names the frit
        match self.frit_plan() {
            Ok(_) => self.message = Some(super::words::remove_pending(&name)),
            Err(why) => {
                self.pending.removals.retain(|n| *n != name);
                self.message = Some(super::words::remove_refused(&name, why));
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
    /// other key's value, which a second Enter writes into the draft; an
    /// empty value goes back to the default, but a key binding's binds no
    /// key and the word `default` returns it to its default. Moving away
    /// drops what is typed.
    fn settings_key(&mut self, key: Key) -> Vec<Effect> {
        let all = setting_keys();
        if let Some(typed) = self.editing.take() {
            return match all.get(self.setting_at) {
                // a key binding's empty value binds no key, and the word
                // `default`, or its default's own combinations, leave it
                // at its default (ADR-1005 D10)
                Some(k) if key == Key::Enter && k.kind == Kind::KeyBinding => {
                    let typed = typed.trim().to_owned();
                    let default = crate::commands::value_text(k, None).0;
                    let value = (typed != super::words::BINDING_DEFAULT && typed != default)
                        .then_some(typed);
                    self.top(k.name, value.as_deref())
                }
                Some(k) if key == Key::Enter => {
                    let typed = typed.trim().to_owned();
                    self.top(
                        k.name,
                        Some(&typed).filter(|t| !t.is_empty()).map(String::as_str),
                    )
                }
                _ => Vec::new(),
            };
        }
        self.setting_at = step(self.setting_at, all.len(), key);
        let Some(k) = all.get(self.setting_at).copied() else {
            return Vec::new();
        };
        if key != Key::Enter {
            return Vec::new();
        }
        let (now, default) = self.value(k);
        match k.kind {
            Kind::Switch => self.top(k.name, Some(if now == "on" { "off" } else { "on" })),
            Kind::Choice(words) => {
                let at = words.iter().position(|w| *w == now);
                let next = at.map_or(0, |i| i.saturating_add(1) % words.len().max(1));
                match words.get(next) {
                    Some(w) => self.top(k.name, Some(w)),
                    None => Vec::new(),
                }
            }
            // a key binding's editor starts from its combinations, its
            // default's included, so Enter alone never empties it
            _ => {
                let start = default && k.kind != Kind::KeyBinding;
                self.editing = Some(if start { String::new() } else { now });
                Vec::new()
            }
        }
    }

    /// Sets the top-level key `key` to `value` in the draft, or leaves it
    /// at its default with `None`, through ops.
    fn top(&mut self, key: &str, value: Option<&str>) -> Vec<Effect> {
        let Some(k) = keys::find(key) else {
            return Vec::new();
        };
        self.make(
            vec![DraftChange::Top {
                key: k.name,
                value: value.map(str::to_owned),
            }],
            None,
        )
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
                self.view = View::Fonts;
            }
            Err(why) => self.message = Some(super::words::new_refused(why)),
        }
    }

    /// The frits an install writes and removes with the draft, frits
    /// first, as apply writes them. Refused when the draft names a frit
    /// it removes, or moves the frits' directory while frits change,
    /// since they would go to the directory the TUI started with.
    pub fn frit_plan(&self) -> Result<Plan, String> {
        let p = &self.pending;
        let mut files = Vec::new();
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
        let key = keys::FONT_FRITS.name;
        if !p.is_empty() && self.drafted(key) != self.configured(key) {
            return Err(crate::apply::FRITS_MOVE_FIRST.to_owned());
        }
        let mut removals = Vec::new();
        for name in &p.removals {
            if let Some(why) = crate::commands::removal_refusal(name, self.conf_text()) {
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
        Ok(Plan {
            files,
            removals,
            no_route: false,
        })
    }

    /// After an install: the frits it removed leave the list, nothing is
    /// pending, and on a live console the draft's font is the one leaving
    /// gives back (ADR-1201 D4).
    pub(crate) fn frits_installed(&mut self) {
        let removed = std::mem::take(&mut self.pending.removals);
        self.inputs.fonts.retain(|f| !removed.contains(&f.name));
        for name in &removed {
            self.inputs.installed.remove(name);
            self.sources.remove(name);
        }
        for (name, text) in std::mem::take(&mut self.pending.frits) {
            self.inputs.installed.insert(name, text.into_bytes());
        }
        self.font_at = self.font_at.min(self.inputs.fonts.len().saturating_sub(1));
        if self.live == Live::On
            && let Some(name) = self.drafted("font-name")
        {
            self.start_font = Some(name);
        }
        self.inputs.conf = self.draft.as_ref().map(|d| d.text.clone());
    }
}

/// The item the Flow view selects first: the first display, else the
/// first item of the first column that has one.
pub(crate) fn first_item(flow: &Flow) -> Item {
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
pub(crate) fn step(at: usize, n: usize, key: Key) -> usize {
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
