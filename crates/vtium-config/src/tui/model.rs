//! The TUI as a state machine apart from its terminal: its state takes
//! [`Event`]s, a key, a mouse click, a reply, a resize, a signal or the
//! end of a wait, and returns [`Effect`]s, bytes to write, a wait to
//! start, a save to run or the status to leave with. The loop around it
//! ([`super::port`]) does the reading and the writing, and draws
//! [`super::view`]'s screen after each event.
//!
//! The tabs are Fonts, Effects, Looks, Flow, Settings and Help, in the
//! order of [`View::TABS`], and the TUI opens on the one its caller names.
//! The Flow view draws the configuration as paths of parts
//! ([`super::flowview`]), and its keys are in the Flow tab's module,
//! [`super::tabs::flow`].
//!
//! References:
//! - ADR-1201 D7: the TUI is a state machine apart from its terminal.
//! - ADR-1205 D1: the six tabs and their order.
//! - ADR-1403 D7: the Flow view draws the configuration as paths.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use vtium_settings::keys;
use vtium_settings::{setting, switch as conf_switch};

use super::desk::{Done, Op, Opened};
use super::edit::Change as DraftChange;
use super::flow::Flow;
use super::input::{Button, Input, Key, Mouse, Reply};
use super::replies::{Answer, Asker, Batch, Query, Queue};
use crate::draft::Plan;

/// How long the TUI waits for the terminal's answers at start, and for
/// the console's font to come back when it leaves: `set`'s wait.
pub const WAIT: Duration = crate::switch::WAIT;

/// The image id of the kitty graphics query, a one-pixel image the
/// terminal checks and does not keep.
pub const QUERY_ID: u32 = 31;

/// The requests the TUI writes at start: XTVERSION, the OSC 50 query and
/// kitty's graphics query, in that order. [`Queue::send`] adds DA1, the
/// fourth. The answers tell the TUI whether the terminal is a vtium
/// console and which font it draws.
///
/// References:
/// - ADR-1201 D4, "Finding out what the terminal is": the four requests
///   at start and what their answers decide.
fn start_batch() -> Batch {
    Batch::new(Asker::Start)
        .ask(Query::Version)
        .ask(Query::Font)
        .ask(Query::Graphics)
}

/// The bytes of the four requests at start, DA1 last.
pub fn probe() -> Vec<u8> {
    Queue::default().send(start_batch())
}

/// The bytes of one font switch: `OSC 50 ; NAME ST`, the query
/// `OSC 50 ; ? ST`, and DA1, which ends every batch.
pub fn switch(name: &str) -> Vec<u8> {
    Queue::default().send(switch_batch(Asker::Switch, &[], name, &[]))
}

/// The batch that writes `before`, switches the console to `name`, writes
/// `after` and asks which font it draws then.
fn switch_batch(asker: Asker, before: &[u8], name: &str, after: &[u8]) -> Batch {
    Batch::new(asker)
        .write(before)
        .write(format!("\x1b]50;{name}\x1b\\").as_bytes())
        .write(after)
        .ask(Query::Font)
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
    /// The density of each connected display as the console finds it:
    /// `display-dpi` where vtium.conf sets it, else the display's own,
    /// else 96. A look's card converts its sizes at the lowest of them.
    pub densities: Vec<u32>,
    /// The font vconsole names, by `vconsole.font=` on the kernel command
    /// line or `FONT=` in vconsole.conf, which the console draws where
    /// vtium.conf and the look name no font set.
    pub vconsole_font: Option<String>,
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
    /// The head of the console the TUI runs in, from `VTIUM_HEAD`, which
    /// vtium sets in each console's session: the head whose font the
    /// preview names and a trial puts back.
    ///
    /// References:
    /// - ADR-1401 D5: vtium sets `VTIUM_HEAD` in each console's session.
    pub head: Option<String>,
    /// The locale the specimen's line follows, as [`super::locale`] reads
    /// it, for the preview a vtium console draws as text.
    pub locale: String,
    /// The vtlooks, color maps and shaders the Effects tab lists, with each
    /// resource's marks, which an activation through the desk updates.
    pub effects: super::effects_found::Effects,
    /// The font files the Fonts tab lists, with each file's marks and what
    /// the font helper has said of it so far.
    pub font_files: super::fonts_found::Fonts,
    /// The layers in the frits' `layers/` folder that the layer reader
    /// takes, each by its name with its role, in the order of their names:
    /// what the new-frit form offers for the scripts, symbols, emoji and
    /// fallback roles.
    pub layers: Vec<(String, vtium_font::set::layer::Role)>,
}

/// One thing that happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Input(Input),
    /// The terminal's grid changed, or its cell in pixels did: the size as
    /// the loop reads it from the terminal after the change.
    Resize(Size),
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
    /// A picture drawn through the effect the Effects tab previews, and
    /// what became of the effect.
    Filtered {
        picture: crate::specimen::Picture,
        outcome: crate::specimen::Outcome,
    },
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
    /// What the font helper said of some of the files
    /// [`Effect::Describe`] named, as the answers came.
    Described(Vec<(PathBuf, crate::discover::Described)>),
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
    /// A picture of the TUI's own screen at another size: the loop draws
    /// the view at `shot.grid` into `shot.screen`, with no picture on it,
    /// then asks for the picture as [`Effect::Picture`] does. The loop
    /// draws the screen because update never draws.
    ///
    /// References:
    /// - ADR-1206 D4, "Drawing takes the size as an argument": the view
    ///   draws the picture of the console at the picture's size, and
    ///   nothing overwrites the model's size to draw.
    ScreenPicture(crate::specimen::Shot),
    /// Summarizes the specimen of the font `font` at `height` pixels, for
    /// a terminal that draws no pictures, then sends [`Event::Summarized`].
    Summary { font: String, height: u32 },
    /// Asks the glyph helper which sources each frit named lacks, then
    /// sends [`Event::Sourced`].
    Sources(Vec<String>),
    /// Asks the font helper what each of these font files is, then sends
    /// [`Event::Described`] as the answers come, while the loop goes on
    /// reading keys.
    Describe(Vec<PathBuf>),
    /// Hands the op to the desk, then sends [`Event::Desked`].
    Desk(Op),
    /// Runs the privileged step that installs the font file the desk
    /// prepared as a system font, then sends [`Event::Installed`].
    ///
    /// References:
    /// - ADR-1205 D5: the install step for a system font runs through the
    ///   same privileged step as an install, on its own.
    InstallFont,
    /// Leaves, with this exit status.
    Exit(u8),
}

/// The views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Flow,
    /// The Looks tab: the looks, their font sets and their color schemes.
    Looks,
    Fonts,
    Settings,
    /// Help and About.
    Help,
    /// `new`'s guided flow: a font set (a frit) made from fonts, installed
    /// like any change. It belongs to the Looks tab.
    ///
    /// References:
    /// - ADR-1201 D9: `new`'s guided flow is the Looks tab's form for a new
    ///   font set, and it ends in an install.
    New,
    /// The Effects tab: the vtlooks, color maps and shaders.
    Effects,
}

impl View {
    /// The tabs in the tab bar's order, which Tab steps through and the
    /// digits 1 to 6 name. vtium-config opens on Flow, the fourth.
    ///
    /// References:
    /// - ADR-1205 D1: the six tabs and their order.
    /// - ADR-1403 D10, "The number keys": the digits follow this order.
    pub const TABS: [View; 6] = [
        View::Fonts,
        View::Effects,
        View::Looks,
        View::Flow,
        View::Settings,
        View::Help,
    ];

    /// The tab the view shows under: its own, or the Looks tab's for the
    /// new-frit view.
    pub const fn tab(self) -> View {
        match self {
            View::New => View::Looks,
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

/// The sizes a look's card offers, in points, after automatic (0): within
/// the bounds the console reads `font-size` and a frit's `size=` in.
pub const SIZES: [u32; 15] = [6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32];

/// The font set changes of no draft, which the TUI shows while no desk
/// answers.
static NO_FRITS: crate::store::FritChanges = crate::store::FritChanges {
    kept: std::collections::BTreeMap::new(),
    removed: std::collections::BTreeSet::new(),
    problems: Vec::new(),
};

/// What the answers at start have said so far.
#[derive(Clone, Debug, Default)]
struct Answers {
    version: Option<String>,
    font: Option<String>,
    graphics: bool,
}

/// What every tab reads, and no tab changes itself: what the TUI started
/// from, the draft and its unsaved changes, the live console's state, what
/// the terminal answered, the pictures and the message line. A tab asks for a change to it, and update makes the
/// change.
///
/// References:
/// - ADR-1206 D2: the model's four parts, and what `shared` holds.
#[derive(Clone, Debug)]
pub struct Shared {
    pub inputs: Inputs,
    pub live: Live,
    /// The terminal answered the kitty graphics query before DA1.
    pub graphics: bool,
    answers: Answers,
    /// The font to give back: the start query's answer, or what was saved.
    pub start_font: Option<String>,
    /// The console's `cluster-width` rule, which decides whether a syllable
    /// with a spacing vowel sign takes one cell or two. vtium reads the key
    /// only when it starts, so the TUI reads it from the installed
    /// vtium.conf at its own start and keeps it. An install that changes
    /// the key leaves the console on the old rule until vtium restarts, and
    /// the TUI measures text by that rule until then.
    pub cluster_width: vtium_term::ClusterWidth,
    /// The font the console is to draw, which a switch is sent for once no
    /// other is in flight: the live trial's font, or the font it puts
    /// back (D4).
    pub want: Option<String>,
    /// What the last detailed specimen gave: its font, its height and its
    /// summary.
    pub specimen: Option<(String, u32, crate::specimen::Summary)>,
    /// The picture last asked for, so the same one is not asked again.
    pub asked: Option<super::shot::Asked>,
    /// A preview is being drawn: the next is asked for once it comes, so
    /// one is drawn at a time.
    pub drawing: bool,
    /// What the picture on the screen shows: the line beside it.
    pub shot: Option<super::shot::Caption>,
    /// Why no picture shows for the preview asked for last, if none does.
    pub unpictured: Option<super::shot::Unpictured>,
    /// The font the console last said it draws.
    pub shown_font: Option<String>,
    /// The switch sent whose reply has not come.
    pub in_flight: Option<String>,
    /// Fonts the console refused, each marked in the list.
    pub refused: BTreeSet<String>,
    /// Mouse reporting is on.
    pub mouse: bool,
    /// A line for the user, under the list.
    pub message: Option<String>,
    /// The TUI is leaving with this status once the font is back.
    pub leaving: Option<u8>,
    /// The cells the picture on the screen covers, while one shows.
    pub picture: Option<super::shot::Covered>,
    /// The last summary of a size: the font, the height and what it gave.
    pub summary: Option<(String, u32, Result<crate::specimen::Summary, String>)>,
    /// The sources each frit lacks, as the helper found them.
    pub sources: std::collections::BTreeMap<String, SourcesFound>,
    /// The queries sent and not yet settled, each with who asked it.
    replies: Queue,
    /// The draft the Flow view shows, as the desk last read or wrote it.
    pub draft: Option<Opened>,
    /// The devices present, which the flow places on the draft.
    pub present: crate::flowmodel::Present,
    /// The privileged step is installing the draft.
    pub installing: bool,
    /// The install of a font of the person's as a system font that the
    /// desk readied and the question asks about: the font's detail shows
    /// both files' facts and the exact command while the question is open.
    pub font_install: Option<super::desk::FontReady>,
    /// The install that waits for the replies still due, which
    /// [`App::event`] hands the loop once no query is outstanding.
    install_waits: Option<Plan>,
    /// What came and went at the last hotplug, which the status line shows
    /// until the next change.
    pub came: Option<String>,
    /// The notice that the live trial is off has been shown.
    live_said: bool,
    /// The changes made since the draft was last read or saved, which
    /// [`Shared::draft`]'s text holds and the store does not hold until
    /// Save writes them.
    ///
    /// References:
    /// - ADR-1403 D7, "Changes wait for Save": the changes stay unsaved
    ///   until Save.
    pub unsaved: usize,
    /// The terminal's grid when the TUI started, columns by rows, which
    /// the TUI gives back when it leaves after the Looks tab changed it.
    pub start_grid: (usize, usize),
    /// The Looks tab has sent the console a grid request, so leaving
    /// gives the console [`Shared::start_grid`] back.
    pub grid_moved: bool,
    /// The colors the console showed, as vtium-config reckons them from
    /// the installed vtium.conf, when vtium-config saved them with
    /// XTPUSHCOLORS before a look's trial set its own; `None` while no
    /// colors are saved. A trial put back and leaving restore them with
    /// XTPOPCOLORS, so colors a program set before vtium-config started
    /// come back as they were.
    pub colors_saved: Option<vtium_term::Palette>,
    /// The colors that `k` kept from a look's trial, which the console
    /// shows until vtium-config leaves. A later look's trial compares its
    /// colors with these, and putting that trial back sends these again.
    pub kept_colors: Option<vtium_term::Palette>,
    /// The colors that leaving sets after XTPOPCOLORS: colors that `k`
    /// kept and that an install then made the colors of the console's
    /// terminal, so the console goes on showing them. `None` leaves the
    /// console with the colors XTPOPCOLORS restores.
    ///
    /// References:
    /// - ADR-1201 D4, "The live trial, on a key": what `k` keeps stays on
    ///   the console after vtium-config leaves once Install made it the
    ///   configured choice.
    pub start_colors: Option<vtium_term::Palette>,
    /// The grid that `k` kept from a trial of a size, with the font set
    /// and the size in points that give it. An install that gives the
    /// console's terminal that font set and size makes the grid the one
    /// leaving gives back.
    pub kept_grid: Option<super::shot::KeptGrid>,
    /// `u` asked for [`Shared::start_grid`] while another font switch was
    /// in flight, so the grid request waits until the start font's switch
    /// goes: a font switch moves the console's grid too, and would undo a
    /// grid request sent before it.
    pub grid_after_font: bool,
    /// The size whose cell the font helper is measuring for a trial of the
    /// size on the console.
    pub size_asked: Option<super::shot::SizeAsked>,
    /// The installed vtium.conf's `allow-window-ops` when vtium-config
    /// started, which [`Shared::window_ops`] reports.
    running_window_ops: bool,
}

/// The terminal's grid, and its cell in pixels where it reports its size
/// in pixels. It changes only through [`Event::Resize`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: usize,
    pub rows: usize,
    /// The terminal's cell in pixels, where it reports its size in pixels.
    pub cell_px: Option<(u32, u32)>,
}

impl Shared {
    /// The installed vtium.conf's value of `key`.
    pub fn configured(&self, key: &str) -> Option<String> {
        self.inputs.conf.as_deref().and_then(|c| setting(c, key))
    }

    /// Measures text by the console's `cluster-width` rule until the guard
    /// drops. Update and the view both measure inside this guard, so the
    /// rows update counts for a list, a page or a click are the rows the
    /// view draws.
    pub fn measure(&self) -> super::canvas::MeasureBy {
        super::canvas::measure_by(self.cluster_width)
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

    /// Whether the console lets programs resize its grid: the
    /// `allow-window-ops` of the vtium.conf that was installed when
    /// vtium-config started, which the running console read when it made
    /// its head. An install during this run changes the installed file
    /// and not the running console, which reads the key again only when
    /// vtium starts again. It is off by default.
    pub fn window_ops(&self) -> bool {
        self.running_window_ops
    }

    /// Whether the draft turns `allow-window-ops` on, so the Looks tab's
    /// question has nothing left to ask.
    pub fn window_ops_drafted(&self) -> bool {
        self.conf_text()
            .and_then(|c| conf_switch(c, "allow-window-ops"))
            .unwrap_or(false)
    }

    /// Whether the TUI runs in a vtium console, with `allow-font-ops` on
    /// or off.
    pub fn in_vtium(&self) -> bool {
        matches!(self.live, Live::On | Live::OpsOff)
    }

    /// The value the draft gives `key`, as the settings view shows it,
    /// and whether it is the key's default.
    pub fn value(&self, key: &keys::Key) -> (String, bool) {
        let (v, set) = crate::commands::value_text(key, self.conf_text());
        (v, !set)
    }

    /// The draft's text with `changes` made on it, in order, or the line
    /// that says why they cannot be made: there is no draft, or one of
    /// the changes does not apply.
    pub fn apply(&self, changes: &[DraftChange]) -> Result<String, String> {
        let Some(draft) = &self.draft else {
            return Err(super::words::NO_DESK.to_owned());
        };
        super::edit::apply_all(&draft.text, changes).map_err(|fail| fail.text().to_owned())
    }

    /// The font sets the draft's folder adds, replaces and removes, which
    /// wait there for the install; none while no desk has answered.
    ///
    /// References:
    /// - ADR-1403 D8, "A draft's font set changes": the TUI keeps no font set change in memory.
    pub fn frits(&self) -> &crate::store::FritChanges {
        self.draft.as_ref().map_or(&NO_FRITS, |d| &d.frits)
    }

    /// The text of the font set `name` as the install would leave it: the
    /// draft's own where its folder adds or replaces it, else the
    /// installed file's.
    pub fn frit_text(&self, name: &str) -> Option<&[u8]> {
        self.frits()
            .kept
            .get(name)
            .map(String::as_bytes)
            .or_else(|| self.inputs.installed.get(name).map(Vec::as_slice))
    }
}

/// The TUI's state.
#[derive(Clone, Debug)]
pub struct App {
    /// Where the person is: the open tab, the question over it, the live
    /// trial and the key line's page.
    pub nav: super::nav::Nav,
    /// What every tab reads: the draft, the paths, and what the terminal
    /// answered.
    pub shared: Shared,
    /// The terminal's grid and its cell in pixels.
    pub size: Size,
    /// The Fonts tab's state.
    pub fonts: super::tabs::fonts::Fonts,
    /// The Effects tab's state.
    pub effects: super::tabs::effects::Effects,
    /// The Looks tab's state.
    pub looks: super::tabs::looks::Looks,
    /// The Settings tab's state.
    pub settings: super::tabs::settings::Settings,
    /// The Help tab's state.
    pub help: super::tabs::help::Help,
    /// The Flow tab's state.
    pub flow: super::tabs::flow::Flow,
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
    /// Hands the desk each of these, in order.
    Desks(Vec<Op>),
    Install(Plan),
    /// Leaves with this status: `y` saves the unsaved changes first, `n`
    /// leaves without them.
    Quit(u8),
    /// Installs the font file the desk prepared as a system font.
    InstallFont,
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
    /// The TUI at its start in a terminal of `cols` by `rows` that reports
    /// no size in pixels, and what it does first, as [`App::start`] says.
    pub fn new(inputs: Inputs, cols: usize, rows: usize) -> (App, Vec<Effect>) {
        App::start(
            inputs,
            Size {
                cols,
                rows,
                cell_px: None,
            },
        )
    }

    /// The TUI at its start in a terminal of `size`, and what it does
    /// first: the four requests, and the wait for their answers.
    pub fn start(inputs: Inputs, size: Size) -> (App, Vec<Effect>) {
        let view = inputs.view;
        let flow = super::tabs::flow::Flow::new(&inputs.flow);
        let cluster_width = match inputs
            .conf
            .as_deref()
            .and_then(|c| setting(c, "cluster-width"))
            .as_deref()
        {
            Some("single") => vtium_term::ClusterWidth::Single,
            _ => vtium_term::ClusterWidth::Double,
        };
        let _rule = super::canvas::measure_by(cluster_width);
        let running_window_ops = inputs
            .conf
            .as_deref()
            .and_then(|c| conf_switch(c, "allow-window-ops"))
            .unwrap_or(false);
        let mut app = App {
            nav: super::nav::Nav {
                view,
                trial: None,
                confirm: None,
                key_page: 0,
            },
            shared: Shared {
                inputs,
                live: Live::Asking,
                graphics: false,
                answers: Answers::default(),
                start_font: None,
                cluster_width,
                want: None,
                specimen: None,
                asked: None,
                drawing: false,
                shot: None,
                unpictured: None,
                shown_font: None,
                in_flight: None,
                refused: BTreeSet::new(),
                mouse: false,
                message: None,
                leaving: None,
                picture: None,
                summary: None,
                sources: std::collections::BTreeMap::new(),
                replies: Queue::default(),
                draft: None,
                present: crate::flowmodel::Present::default(),
                installing: false,
                font_install: None,
                install_waits: None,
                came: None,
                live_said: false,
                unsaved: 0,
                start_grid: (size.cols, size.rows),
                grid_moved: false,
                grid_after_font: false,
                colors_saved: None,
                kept_colors: None,
                start_colors: None,
                kept_grid: None,
                size_asked: None,
                running_window_ops,
            },
            size,
            fonts: super::tabs::fonts::Fonts::default(),
            effects: super::tabs::effects::Effects::default(),
            looks: super::tabs::looks::Looks::default(),
            settings: super::tabs::settings::Settings::default(),
            flow,
            help: super::tabs::help::Help::default(),
        };
        // the Flow view's highlight on its first box from the first frame,
        // and what reading the color file found on the message line
        app.flow_settle();
        app.shared.message = app
            .shared
            .message
            .take()
            .or_else(|| app.shared.inputs.colors_note.clone());
        app.lists_settle();
        // The queries go first, so the terminal answers them while the
        // desk opens the draft, which runs `vtium check-config` and can
        // take seconds on a loaded machine. The wait for the answers
        // starts only after that, so the TUI's own work never uses up
        // the time a terminal has to answer.
        let mut first = vec![Effect::Write(app.shared.replies.send(start_batch()))];
        if app.shared.inputs.desk {
            first.push(Effect::Desk(Op::Open));
        }
        first.push(Effect::Wait(WAIT));
        first.extend(app.mouse_for_view());
        (app, first)
    }

    /// Turns mouse reporting on when the view is Flow and off when it is
    /// not, where it is not so already.
    fn mouse_for_view(&mut self) -> Vec<Effect> {
        let want = self.nav.view == View::Flow;
        if want == self.shared.mouse {
            return Vec::new();
        }
        self.shared.mouse = want;
        vec![Effect::Write(
            if want { MOUSE_ON } else { MOUSE_OFF }.to_vec(),
        )]
    }

    /// Goes to `view`, turning the mouse on or off for it, and asks for
    /// what the view shows: a specimen, a font switch.
    pub(crate) fn set_view(&mut self, view: View) -> Vec<Effect> {
        self.nav.view = view;
        self.settings.left();
        self.say_live_off();
        self.mouse_for_view()
    }

    /// Says once, on the first view other than Flow, that the live trial
    /// is off and why. The Flow view says it only when `t` is pressed
    /// (ADR-1403 D7).
    fn say_live_off(&mut self) {
        if self.nav.view == View::Flow || self.shared.live_said {
            return;
        }
        let notice = match self.shared.live {
            Live::OpsOff => super::words::OPS_OFF,
            Live::NotVtium => super::words::NOT_VTIUM,
            // nothing is off with the trial on, and while the TUI still
            // asks, the notice waits for the answers, which call this again
            Live::On | Live::Asking => return,
        };
        self.shared.live_said = true;
        self.shared.message = Some(notice.to_owned());
    }

    /// Tab's step to the next view, or Shift-Tab's to the one before. The
    /// Flow view keeps the item it had selected.
    fn step_view(&mut self, forward: bool) -> Vec<Effect> {
        let tab = self.nav.view.tab();
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

    /// The font file the Fonts tab highlights.
    pub fn font(&self) -> Option<&super::fonts_found::FontFile> {
        self.fonts.highlighted(&self.shared)
    }

    /// The state of the open tab, as every tab answers the parts of the
    /// TUI they share.
    pub fn tab(&self) -> &dyn super::tabs::Tab {
        self.tab_of(self.nav.view)
    }

    /// The state of the tab that `view` shows. This and `App::tab_mut`
    /// are the one place that names each tab's state, so a new tab adds a
    /// line to each.
    pub fn tab_of(&self, view: View) -> &dyn super::tabs::Tab {
        match view {
            View::Flow => &self.flow,
            View::Fonts => &self.fonts,
            View::New => self.looks.new_frit(),
            View::Effects => &self.effects,
            View::Looks => &self.looks,
            View::Settings => &self.settings,
            View::Help => &self.help,
        }
    }

    /// The open tab's state to change, beside the shared part to read.
    fn tab_mut(&mut self) -> (&mut dyn super::tabs::Tab, &Shared) {
        let tab: &mut dyn super::tabs::Tab = match self.nav.view {
            View::Flow => &mut self.flow,
            View::Fonts => &mut self.fonts,
            View::New => self.looks.new_frit_mut(),
            View::Effects => &mut self.effects,
            View::Looks => &mut self.looks,
            View::Settings => &mut self.settings,
            View::Help => &mut self.help,
        };
        (tab, &self.shared)
    }

    /// The installed vtium.conf's value of `key`.
    pub fn configured(&self, key: &str) -> Option<String> {
        self.shared.configured(key)
    }

    /// The configuration the views show, as [`Shared::conf_text`] says.
    pub fn conf_text(&self) -> Option<&str> {
        self.shared.conf_text()
    }

    /// The draft's value of `key`, else the installed file's.
    pub fn drafted(&self, key: &str) -> Option<String> {
        self.shared.drafted(key)
    }

    /// Whether the draft gives `key` another value than the installed
    /// file.
    pub fn changed(&self, key: &str) -> bool {
        self.shared.changed(key)
    }

    /// Whether the draft leaves `allow-font-ops` on, its default.
    pub fn font_ops(&self) -> bool {
        self.shared.font_ops()
    }

    /// The value the draft gives `key`, as [`Shared::value`] says.
    pub fn value(&self, key: &keys::Key) -> (String, bool) {
        self.shared.value(key)
    }

    /// Lays the Flow tab out again on the shared part and the terminal's
    /// size, as after every event.
    pub(crate) fn flow_settle(&mut self) {
        let keys = super::layout::key_rows(self).len();
        self.flow.settle(&self.shared, self.size, keys);
    }

    /// Keeps the highlighted item of the open tab's list in view, from
    /// the row its layout gives, as after every event. The window's rows
    /// change with the highlight, the terminal's size, the message line
    /// and the sentences under the list, so update settles the window
    /// after every event, not only after a key or a resize. In the Flow
    /// view the list is Restore's list of the installed history.
    ///
    /// References:
    /// - ADR-1206 D4, "A list's selection lives in the tab's state": update
    ///   keeps the selection in view through the tab's layout, and each
    ///   tab has a hook that fits the offset to the rows it has.
    pub fn lists_settle(&mut self) {
        if self.nav.view == View::Flow {
            let (_, first) = super::layout::restore_window(self);
            self.flow.restore_show_from(first);
            return;
        }
        let row = super::layout::list_offset(self);
        self.tab_mut().0.show_from(row);
    }

    /// The first row of the open tab's list that its window shows, as
    /// update last settled it.
    pub fn list_offset(&self) -> usize {
        self.tab().offset()
    }

    /// Takes `event`, and returns what the loop does.
    pub fn event(&mut self, event: Event) -> Vec<Effect> {
        let _rule = self.shared.measure();
        let node = super::nav::node(self);
        let mut out = self.handle(event);
        out.extend(self.ask_picture());
        self.hold_install(&mut out);
        self.flow_settle();
        self.lists_settle();
        // a key line shows its first page in each node the TUI moves to,
        // where the keys a person uses most are
        if super::nav::node(self) != node {
            self.nav.key_page = 0;
        }
        out
    }

    /// Keeps an install back while a query is outstanding, and hands it to
    /// the loop with the event that settles the last one. The privileged
    /// step gives the terminal to sudo with echo on, so a reply that came
    /// then would show on the screen and become part of the password, and
    /// the queue would never see it. The replies' own end, DA1's reply or
    /// the deadline at start, bounds the wait, so the wait needs no timer
    /// of its own. A TUI that is leaving never installs.
    ///
    /// References:
    /// - ADR-1206 D2, "Every batch of queries ends with DA1": no timer waits
    ///   for a reply.
    fn hold_install(&mut self, out: &mut Vec<Effect>) {
        if self.awaiting() {
            if let Some(at) = out.iter().position(|e| matches!(e, Effect::Install(_)))
                && let Effect::Install(plan) = out.remove(at)
            {
                self.shared.install_waits = Some(plan);
            }
        } else if let Some(plan) = self.shared.install_waits.take()
            && self.shared.leaving.is_none()
        {
            out.push(Effect::Install(plan));
        }
    }

    fn handle(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Input(Input::Reply(reply)) => self.reply(reply),
            Event::Input(Input::Key(Key::Mouse(mouse))) => self.click(mouse),
            Event::Input(Input::Key(key)) => self.key(key),
            Event::Resize(size) => {
                self.size = size;
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
            Event::Celled { font, height, cell } => self
                .size_measured(&font, height, &cell)
                .unwrap_or_else(|| self.celled(&font, height, cell)),
            Event::Pictured(result) => self.pictured(result),
            Event::Filtered { picture, outcome } => self.filtered(picture, outcome),
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
                self.shared.drawing = false;
                self.shared.summary = Some((font, height, summary));
                Vec::new()
            }
            Event::Sourced(found) => {
                self.shared.sources.extend(found);
                Vec::new()
            }
            Event::Described(answers) => {
                let highlighted = self.fonts.highlighted_path(&self.shared);
                self.shared.inputs.font_files.describe(answers);
                self.fonts.keep(&self.shared, highlighted.as_deref());
                Vec::new()
            }
            Event::Desked(op, done) => self.desked(&op, done),
            Event::Hotplug => vec![Effect::Desk(Op::Devices)],
        }
    }

    /// The size the Looks tab previews, in points, 0 for automatic.
    pub fn size(&self) -> u32 {
        self.looks.points(&self.shared)
    }

    /// Whether a query the TUI sent is still outstanding, so the reader
    /// keeps an ESC that ends a read for the next read.
    pub fn awaiting(&self) -> bool {
        self.shared.replies.awaiting()
    }

    /// A reply from the terminal, which the queue of askers hands to the
    /// part of the TUI that asked. Once the TUI is leaving and every query
    /// is settled, it leaves.
    fn reply(&mut self, reply: Reply) -> Vec<Effect> {
        let heard = self.shared.replies.answer(reply);
        let settled = !heard.is_empty();
        let mut out = Vec::new();
        for (asker, answer) in heard {
            out.extend(match asker {
                Asker::Start => self.start_answer(answer),
                Asker::Switch => self.switch_answer(answer),
                Asker::Leave => {
                    if let Answer::Reply(Reply::Font(name)) = answer {
                        self.shared.shown_font = Some(name);
                    }
                    Vec::new()
                }
            });
        }
        if let Some(status) = self.shared.leaving
            && settled
            && !self.shared.replies.awaiting()
        {
            out.push(Effect::Exit(status));
        }
        out
    }

    /// What the four requests at start hear back, until DA1's reply
    /// settles what the terminal is.
    fn start_answer(&mut self, answer: Answer) -> Vec<Effect> {
        if self.shared.live != Live::Asking {
            return Vec::new();
        }
        match answer {
            Answer::Reply(Reply::Version(v)) => self.shared.answers.version = Some(v),
            Answer::Reply(Reply::Font(f)) => self.shared.answers.font = Some(f),
            Answer::Reply(Reply::Graphics(ok)) => self.shared.answers.graphics |= ok,
            Answer::Reply(Reply::Attributes) => {
                self.decide();
                return self.started();
            }
            Answer::Unanswered(_) => {}
        }
        Vec::new()
    }

    /// What the font switch in flight hears back: the font the console
    /// draws now, or that the console left the query unanswered. A font
    /// the console did not take is marked refused, and the next switch
    /// goes once no other is in flight.
    fn switch_answer(&mut self, answer: Answer) -> Vec<Effect> {
        let shown = match answer {
            Answer::Reply(Reply::Font(name)) => Some(name),
            Answer::Unanswered(Query::Font) => None,
            _ => return Vec::new(),
        };
        let asked = self.shared.in_flight.take();
        if self.shared.leaving.is_some() {
            self.shared.shown_font = shown.or(self.shared.shown_font.take());
            return Vec::new();
        }
        let mut reset = Vec::new();
        if let Some(asked) = asked
            && shown.as_ref() != Some(&asked)
        {
            self.shared.refused.insert(asked.clone());
            if self.shared.want.as_deref() == Some(asked.as_str()) {
                self.shared.want = shown.clone().or_else(|| self.shared.shown_font.clone());
            }
            if let Some(t) = self.nav.trial.take_if(|t| t.font == asked) {
                // a look's trial that set the console's colors gives them
                // back with the font
                if let Some(tried) = t.look.as_ref().and_then(|l| l.colors.as_ref()) {
                    reset.extend(self.colors_back(tried));
                }
                self.shared.message = Some(if super::shot::is_file(&asked) {
                    super::words::font_file_refused(super::shot::font_shown(&asked))
                } else {
                    super::words::trial_refused(&asked)
                });
            }
        }
        if shown.is_some() {
            self.shared.shown_font = shown;
        }
        let mut out = Vec::new();
        if !reset.is_empty() {
            out.push(Effect::Write(reset));
        }
        out.extend(self.follow());
        out.extend(self.grid_after_font());
        // a look's trial tries its size once the console draws its font
        out.extend(self.look_size());
        out
    }

    /// The start grid that `u` asked for, once nothing but the start
    /// font's switch is in flight. That switch's bytes are already
    /// written, and the console reads its input in order, so the grid
    /// request that follows them lands after the font has changed. A font
    /// wanted since then, other than the start font, replaces what `u`
    /// asked for, and the grid then goes back only when the TUI leaves.
    pub(crate) fn grid_after_font(&mut self) -> Vec<Effect> {
        if !self.shared.grid_after_font {
            return Vec::new();
        }
        let start = self.shared.start_font.as_ref();
        let refused = start.is_some_and(|s| self.shared.refused.contains(s));
        if self.shared.want.as_ref() != start && !refused {
            self.shared.grid_after_font = false;
            return Vec::new();
        }
        if self
            .shared
            .in_flight
            .as_ref()
            .is_some_and(|f| Some(f) != start)
        {
            return Vec::new();
        }
        self.shared.grid_after_font = false;
        if !self.shared.grid_moved {
            return Vec::new();
        }
        self.shared.grid_moved = false;
        vec![Effect::Write(super::shot::grid_request(
            self.shared.start_grid,
        ))]
    }

    /// What follows once the terminal is known: the sources each frit that
    /// reads lacks, found once, a description of each font file the Fonts
    /// tab lists, and the highlighted font's picture where one shows.
    fn started(&mut self) -> Vec<Effect> {
        let frits: Vec<String> = self
            .shared
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
        let waiting = self.shared.inputs.font_files.ask();
        if !waiting.is_empty() {
            out.push(Effect::Describe(waiting));
        }
        out
    }

    /// Settles what the terminal is, from the answers that came.
    fn decide(&mut self) {
        let vtium = self
            .shared
            .answers
            .version
            .as_deref()
            .is_some_and(|v| v.starts_with("vtium("));
        self.shared.graphics = self.shared.answers.graphics;
        self.shared.live = match (vtium, self.shared.answers.font.take()) {
            (true, Some(font)) => {
                self.shared.start_font = Some(font.clone());
                self.shared.shown_font = Some(font.clone());
                Live::On
            }
            (true, None) => Live::OpsOff,
            (false, _) => Live::NotVtium,
        };
        self.say_live_off();
    }

    /// The end of a wait: at start, the deadline for a terminal that does
    /// not answer DA1, so the TUI decides from the answers that came and
    /// forgets the queries, which no reply will settle now.
    fn timeout(&mut self) -> Vec<Effect> {
        if self.shared.live == Live::Asking {
            self.shared.replies.clear();
            self.decide();
            return self.started();
        }
        match self.shared.leaving {
            Some(status) => vec![Effect::Exit(status)],
            None => self.tick(),
        }
    }

    /// Sends the font the console is to draw when no switch is outstanding
    /// and the console draws another (D4): one switch at a time, naming
    /// whatever is wanted when the last reply came.
    pub(crate) fn follow(&mut self) -> Vec<Effect> {
        if self.shared.live != Live::On || self.shared.in_flight.is_some() {
            return Vec::new();
        }
        let Some(name) = self.shared.want.clone() else {
            return Vec::new();
        };
        if self.shared.shown_font.as_deref() == Some(&name) || self.shared.refused.contains(&name) {
            return Vec::new();
        }
        let bytes = self
            .shared
            .replies
            .send(switch_batch(Asker::Switch, &[], &name, &[]));
        self.shared.in_flight = Some(name);
        vec![Effect::Write(bytes)]
    }

    /// Leaves with `status`, giving the console its start font back first
    /// when a switch changed it (D4).
    pub fn leave(&mut self, status: u8) -> Vec<Effect> {
        if self.shared.leaving.is_some() {
            return Vec::new();
        }
        // the colors a look's trial set go back below, whether `k` kept
        // them or the trial still counts down, unless an install made the
        // kept colors the configured ones
        self.nav.trial = None;
        // the font the console ends in once the switch in flight, if any,
        // is answered: the start font is sent only where it is another
        let last = self
            .shared
            .in_flight
            .as_ref()
            .or(self.shared.shown_font.as_ref());
        let back = last != self.shared.start_font.as_ref();
        let mouse = std::mem::take(&mut self.shared.mouse);
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
        bytes.extend(super::shot::colors_on_leaving(&self.shared));
        self.shared.colors_saved = None;
        self.shared.kept_colors = None;
        // the grid the console had at the start goes back after its font,
        // since a font switch can change the grid too
        let grid = if self.shared.grid_moved {
            super::shot::grid_request(self.shared.start_grid)
        } else {
            Vec::new()
        };
        match (&self.shared.live, self.shared.start_font.clone()) {
            (Live::On, Some(start)) if back => {
                let bytes =
                    self.shared
                        .replies
                        .send(switch_batch(Asker::Leave, &bytes, &start, &grid));
                self.shared.leaving = Some(status);
                self.shared.message = Some(super::words::LEAVING.to_owned());
                vec![Effect::Write(bytes), Effect::Wait(WAIT)]
            }
            // a query is still outstanding: the switch in flight, which
            // gives the start font back, so nothing is sent again, or the
            // DA1 after a switch already answered. The TUI waits for the
            // replies, so that none reaches the shell after it leaves.
            (Live::On, Some(_)) if self.shared.replies.awaiting() => {
                bytes.extend_from_slice(&grid);
                self.shared.leaving = Some(status);
                if self.shared.in_flight.is_some() {
                    self.shared.message = Some(super::words::LEAVING.to_owned());
                }
                let mut out = Vec::new();
                if !bytes.is_empty() {
                    out.push(Effect::Write(bytes));
                }
                out.push(Effect::Wait(WAIT));
                out
            }
            _ => {
                bytes.extend_from_slice(&grid);
                self.shared.leaving = Some(status);
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
        self.tab().typing()
    }

    /// A key, as the navigation module routes it: to the question, the
    /// live trial, the screen below the minimum size or the value being
    /// typed, to the keys that act the same in every tab, or to the open
    /// view.
    fn key(&mut self, key: Key) -> Vec<Effect> {
        use super::nav::Route;
        match super::nav::route(self, key) {
            Route::Ignore | Route::Nothing => Vec::new(),
            Route::Question => self.confirm_key(key),
            Route::Trial => self.trial_key(key),
            Route::TooSmall => self.small_key(key),
            Route::SaveAndQuit => self.save_and_leave(130),
            Route::Quit => self.quit(0),
            Route::Type => self.type_key(key),
            Route::NextPage(pages) => {
                self.nav.key_page = self.nav.key_page.saturating_add(1) % pages.max(1);
                Vec::new()
            }
            Route::Step(forward) => self.step_view(forward),
            Route::GoTo(view) => self.set_view(view),
            Route::View => self.view_key(key),
        }
    }

    /// A key that the open view acts on itself.
    fn view_key(&mut self, key: Key) -> Vec<Effect> {
        // the open tab says whether each action key acts there: not while
        // a level is open over it, nor where its own keys use the letter
        let takes = |action: char| self.tab().takes_action(action);
        match key {
            Key::Char('i') if takes('i') && !self.typing() => return self.action('i'),
            Key::Char('s') if takes('s') && !self.typing() => return self.save(),
            _ => {}
        }
        let room = super::layout::room(self);
        let (tab, shared) = self.tab_mut();
        let commands = tab.update(shared, room, key);
        self.perform(commands)
    }

    /// Performs the commands a tab's update returned: the changes to the
    /// shared part first, in order, then the requests to the navigation
    /// module, and returns the effects for the loop.
    ///
    /// References:
    /// - ADR-1206 D2: update performs a tab's commands in one place.
    fn perform(&mut self, commands: Vec<super::tabs::Command>) -> Vec<Effect> {
        use super::tabs::{Change, Command, Request};
        let mut effects = Vec::new();
        let mut requests = Vec::new();
        for command in commands {
            match command {
                Command::Effect(effect) => effects.push(effect),
                Command::Change(Change::Top { key, value }) => {
                    effects.extend(self.top(key, value.as_deref()));
                }
                Command::Change(Change::Say(line)) => self.shared.message = Some(line),
                Command::Change(Change::Try(font)) => effects.extend(self.try_live(font)),
                Command::Change(Change::TryLook(look)) => effects.extend(self.try_look(look)),
                Command::Change(Change::TrySize { font, points }) => {
                    effects.extend(self.try_size(font, points));
                }
                Command::Change(Change::Quiet) => self.shared.message = None,
                Command::Change(Change::Make { changes, select }) => {
                    effects.extend(self.make(changes, select));
                }
                Command::Change(Change::Save) => effects.extend(self.save()),
                Command::Change(Change::Action(key)) => effects.extend(self.action(key)),
                Command::Request(request) => requests.push(request),
            }
        }
        // the requests to the navigation module follow the changes
        for request in requests {
            match request {
                Request::Go(view) => effects.extend(self.set_view(view)),
                Request::Ask { question, then } => self.ask(question, then),
                // font sets are listed in the Looks tab, which highlights
                // the one asked for
                Request::ShowFont(name) => {
                    effects.extend(self.set_view(View::Looks));
                    self.looks.show_font_set(&self.shared, &name);
                }
            }
        }
        effects
    }

    /// A key below the TUI's minimum size, where no view is drawn: `q`
    /// quits, or asks the quit question first while changes are not
    /// saved, and `u` puts back the font and the grid the console had
    /// when the TUI started, where a trial changed them, or says that it
    /// has nothing to put back. Ctrl-C saves and
    /// leaves, as it does everywhere. `u` sends the font switch before
    /// the grid request, as leaving does, because a font switch changes
    /// the console's grid too. While another switch is in flight, the
    /// start font's switch and then the grid request go once the console
    /// answers it.
    ///
    /// References:
    /// - ADR-1201 D3: below the minimum size only `q`, `u` and Ctrl-C act.
    /// - ADR-1201 D4, "Giving the console back": the start font and the
    ///   start grid that leaving gives back, which `u` gives back too.
    /// - ADR-1403 D7: `q` asks before it drops changes that are not saved.
    fn small_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Char('q') => self.quit(0),
            Key::Char('u') => {
                let font_moved = self.shared.in_flight.is_some()
                    || self.shared.shown_font != self.shared.start_font;
                if !font_moved && !self.shared.grid_moved {
                    self.shared.message = Some(super::words::NOTHING_TO_UNDO.to_owned());
                    return Vec::new();
                }
                let mut out = Vec::new();
                if let Some(start) = self.shared.start_font.clone() {
                    self.shared.want = Some(start);
                    out.extend(self.follow());
                }
                // the grid request goes after the start font's switch; while
                // another switch is in flight, the start font's switch can
                // only go once the console answers that one, so the grid
                // request waits for it too
                if self.shared.grid_moved {
                    self.shared.grid_after_font = true;
                    out.extend(self.grid_after_font());
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// A mouse event, which only the Flow view asks for: a click on a
    /// view's name switches to it, as Tab does, while nothing is open that
    /// the switch would leave behind; a click on an item selects it, and a
    /// second click on the selected item opens it; the wheel moves as Up
    /// and Down do.
    fn click(&mut self, mouse: Mouse) -> Vec<Effect> {
        if self.nav.trial.is_some() && self.shared.leaving.is_none() && !self.shared.installing {
            return self.put_back();
        }
        // a question takes only its answer keys, so a click or the wheel
        // leaves it open and changes nothing under it
        if self.nav.confirm.is_some() {
            return Vec::new();
        }
        if self.nav.view != View::Flow
            || self.shared.leaving.is_some()
            || self.shared.installing
            || super::layout::too_small(self.size)
        {
            return Vec::new();
        }
        match mouse.button {
            Button::WheelUp => return self.key(Key::Up),
            Button::WheelDown => return self.key(Key::Down),
            Button::Press(0) => {}
            _ => return Vec::new(),
        }
        if let Some(view) = super::layout::tab_at(self, mouse.col, mouse.row) {
            if view == View::Flow || super::nav::something_open(self) {
                return Vec::new();
            }
            return self.set_view(view);
        }
        if super::flowlayout::shown(self) {
            let keys = super::layout::key_rows(self).len();
            let commands = self
                .flow
                .click(&self.shared, self.size, keys, mouse.col, mouse.row);
            return self.perform(commands);
        }
        Vec::new()
    }

    /// A character or Backspace typed into the field being edited.
    fn type_key(&mut self, key: Key) -> Vec<Effect> {
        self.tab_mut().0.type_key(key);
        Vec::new()
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

    /// After an install of `done`, the font set changes the install made:
    /// the frits it removed leave the list, the frits it wrote join it,
    /// and the installed vtium.conf becomes the one the TUI reads the
    /// console's settings from. The Looks tab holds each font set by its
    /// place in the list, which the install moves, so `spot`, taken before
    /// the installed draft replaced the old one, keeps its highlight and an
    /// open detail by the font set's name.
    pub(crate) fn frits_installed(
        &mut self,
        done: crate::store::FritChanges,
        spot: super::tabs::looks::Spot,
    ) {
        let crate::store::FritChanges { kept, removed, .. } = done;
        self.shared
            .inputs
            .fonts
            .retain(|f| !removed.contains(&f.name));
        for name in &removed {
            self.shared.inputs.installed.remove(name);
            self.shared.sources.remove(name);
        }
        // each new font set joins the list the Looks tab offers at once,
        // as the next start would list it, in place of an installed set of
        // its name
        for (name, text) in kept {
            let state = super::tabs::looks::fontsets::state_of(&name, &text);
            let fonts = &mut self.shared.inputs.fonts;
            fonts.retain(|f| f.name != name);
            fonts.push(Font {
                name: name.clone(),
                state,
            });
            fonts.sort_by(|a, b| a.name.cmp(&b.name));
            self.shared.inputs.installed.insert(name, text.into_bytes());
        }
        self.fonts.keep(&self.shared, None);
        self.looks.restore(&self.shared, spot);
        self.shared.inputs.conf = self.shared.draft.as_ref().map(|d| d.text.clone());
    }
}

/// The most bytes a typed field holds.
pub const TYPED_MAX: usize = 256;

/// `at` moved by `key` in a list of `n`, as the shared list moves.
pub(crate) fn step(at: usize, n: usize, key: Key) -> usize {
    super::listbox::step(at, n, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The TUI on a vtium console after the start, after a live trial of
    /// tiny5 that a key ended before the console answered its switch, and
    /// with the install question asked, as `i` asks it. The trial's switch
    /// is outstanding.
    fn asked_to_install_after_a_trial() -> App {
        let (mut app, _) = App::new(super::super::fixture::inputs(), 80, 25);
        for reply in [
            Reply::Version("vtium(test)".into()),
            Reply::Font("unifont".into()),
            Reply::Attributes,
        ] {
            app.event(Event::Input(Input::Reply(reply)));
        }
        // tiny5 is the fourth font set below the Looks tab's one look
        for key in [
            Key::Char('3'),
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Char('t'),
            Key::Escape,
        ] {
            app.event(Event::Input(Input::Key(key)));
        }
        assert!(app.nav.trial.is_none(), "the key ended the trial");
        assert!(app.awaiting(), "the trial's switch is outstanding");
        app.nav.confirm = Some(Confirm {
            question: "Install?".into(),
            then: Then::Install(Plan::default()),
        });
        app
    }

    fn installs(out: &[Effect]) -> usize {
        out.iter()
            .filter(|e| matches!(e, Effect::Install(_)))
            .count()
    }

    /// The privileged step hands the terminal to sudo, which reads it with
    /// echo on. So an install that `y` starts while replies are due goes
    /// to the loop only with the reply that settles the last query.
    #[test]
    fn an_install_waits_until_no_reply_is_due() {
        let mut app = asked_to_install_after_a_trial();
        let out = app.event(Event::Input(Input::Key(Key::Char('y'))));
        assert_eq!(installs(&out), 0, "{out:?}");
        assert!(app.shared.installing);
        // the console answers the trial's switch, and the TUI sends the
        // switch back to the start font, whose replies are due in turn
        for reply in [Reply::Font("tiny5".into()), Reply::Attributes] {
            let out = app.event(Event::Input(Input::Reply(reply)));
            assert_eq!(installs(&out), 0, "{out:?}");
        }
        assert!(app.awaiting(), "the switch back is outstanding");
        let out = app.event(Event::Input(Input::Reply(Reply::Font("unifont".into()))));
        assert_eq!(installs(&out), 0, "{out:?}");
        let out = app.event(Event::Input(Input::Reply(Reply::Attributes)));
        assert!(!app.awaiting());
        assert_eq!(installs(&out), 1, "{out:?}");
        // with no reply due, `y` installs at once
        app.event(Event::Installed(Err("no desk in this test".into())));
        app.nav.confirm = Some(Confirm {
            question: "Install?".into(),
            then: Then::Install(Plan::default()),
        });
        let out = app.event(Event::Input(Input::Key(Key::Char('y'))));
        assert_eq!(installs(&out), 1, "{out:?}");
    }

    /// The bytes of each write among `out`, run together in order.
    fn written(out: &[Effect]) -> String {
        out.iter()
            .filter_map(|e| match e {
                Effect::Write(b) => Some(String::from_utf8_lossy(b).into_owned()),
                _ => None,
            })
            .collect()
    }

    /// `u` below the minimum size while a font switch is still in flight:
    /// the console has not yet answered the trial's switch to tiny5, so
    /// `u` cannot send the start font's switch yet. The start grid waits
    /// until the start font's switch goes, because the font switch moves
    /// the grid again.
    #[test]
    fn u_with_a_switch_in_flight_sends_the_start_font_before_the_start_grid() {
        let (mut app, _) = App::new(super::super::fixture::inputs(), 80, 25);
        for reply in [
            Reply::Version("vtium(test)".into()),
            Reply::Font("unifont".into()),
            Reply::Attributes,
        ] {
            app.event(Event::Input(Input::Reply(reply)));
        }
        // a size kept from an earlier trial
        app.shared.grid_moved = true;
        // tiny5 is the fourth font set below the Looks tab's one look; t
        // tries it and k keeps it before the console answers the switch
        for key in [
            Key::Char('3'),
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Char('t'),
            Key::Char('k'),
        ] {
            app.event(Event::Input(Input::Key(key)));
        }
        assert_eq!(app.shared.in_flight.as_deref(), Some("tiny5"));
        // the console's resize comes before its reply
        app.event(Event::Resize(Size {
            cols: 30,
            rows: 8,
            cell_px: None,
        }));
        let mut out = String::new();
        out.push_str(&written(
            &app.event(Event::Input(Input::Key(Key::Char('u')))),
        ));
        for reply in [Reply::Font("tiny5".into()), Reply::Attributes] {
            out.push_str(&written(&app.event(Event::Input(Input::Reply(reply)))));
        }
        let font = out.find("\x1b]50;unifont");
        let grid = out.find("\x1b[8;25;80t");
        assert!(font.is_some(), "the start font's switch: {out:?}");
        assert!(grid.is_some(), "the start grid: {out:?}");
        assert!(
            font < grid,
            "u asked for the start grid before the start font: {out:?}"
        );
        // the console answers the start font, and nothing more is sent
        for reply in [Reply::Font("unifont".into()), Reply::Attributes] {
            let more = written(&app.event(Event::Input(Input::Reply(reply))));
            assert!(!more.contains("\x1b[8;"), "{more:?}");
        }
        assert!(!app.shared.grid_moved);
    }

    /// A font tried after `u`, once the terminal is large again, replaces
    /// what `u` asked for: the start grid then waits for the TUI to leave,
    /// and does not land in the middle of the new trial.
    #[test]
    fn a_trial_after_u_drops_u_s_waiting_grid_request() {
        let (mut app, _) = App::new(super::super::fixture::inputs(), 80, 25);
        for reply in [
            Reply::Version("vtium(test)".into()),
            Reply::Font("unifont".into()),
            Reply::Attributes,
        ] {
            app.event(Event::Input(Input::Reply(reply)));
        }
        app.shared.grid_moved = true;
        for key in [
            Key::Char('3'),
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Char('t'),
            Key::Char('k'),
        ] {
            app.event(Event::Input(Input::Key(key)));
        }
        let small = |cols, rows| Size {
            cols,
            rows,
            cell_px: None,
        };
        app.event(Event::Resize(small(30, 8)));
        let mut out = written(&app.event(Event::Input(Input::Key(Key::Char('u')))));
        app.event(Event::Resize(small(80, 25)));
        // unscii-8 is the font set below tiny5 and unifont
        for key in [Key::Down, Key::Down, Key::Char('t')] {
            out.push_str(&written(&app.event(Event::Input(Input::Key(key)))));
        }
        assert_eq!(app.shared.want.as_deref(), Some("unscii-8"));
        for reply in [
            Reply::Font("tiny5".into()),
            Reply::Attributes,
            Reply::Font("unscii-8".into()),
            Reply::Attributes,
        ] {
            out.push_str(&written(&app.event(Event::Input(Input::Reply(reply)))));
        }
        assert!(!out.contains("\x1b[8;25;80t"), "{out:?}");
        assert!(app.shared.grid_moved, "leaving gives the start grid back");
    }

    /// A signal that comes while the install waits makes the TUI leave,
    /// and the install never runs.
    #[test]
    fn an_install_that_waits_is_dropped_when_the_tui_leaves() {
        let mut app = asked_to_install_after_a_trial();
        app.event(Event::Input(Input::Key(Key::Char('y'))));
        let mut out = app.event(Event::Signal(15));
        assert!(app.shared.leaving.is_some());
        // the console answers the trial's switch, then the switch back to
        // the start font
        for reply in [
            Reply::Font("tiny5".into()),
            Reply::Attributes,
            Reply::Font("unifont".into()),
            Reply::Attributes,
        ] {
            out.extend(app.event(Event::Input(Input::Reply(reply))));
        }
        assert!(!app.awaiting());
        assert_eq!(installs(&out), 0, "{out:?}");
        assert!(out.contains(&Effect::Exit(143)), "{out:?}");
    }
}
