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
use crate::draft::{FileEdit, Plan, Removal};

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
    Queue::default().send(switch_batch(Asker::Switch, &[], name))
}

/// The batch that writes `before`, switches the console to `name` and asks
/// which font it draws then.
fn switch_batch(asker: Asker, before: &[u8], name: &str) -> Batch {
    Batch::new(asker)
        .write(before)
        .write(format!("\x1b]50;{name}\x1b\\").as_bytes())
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
    /// `new`'s guided flow: a frit made from fonts, installed like any
    /// change (ADR-1201 D9). It belongs to the Looks tab.
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

/// What the answers at start have said so far.
#[derive(Clone, Debug, Default)]
struct Answers {
    version: Option<String>,
    font: Option<String>,
    graphics: bool,
}

/// What every tab reads, and no tab changes itself: what the TUI started
/// from, the draft and its unsaved changes, the live console's state, what
/// the terminal answered, the pictures, the pending installs and the
/// message line. A tab asks for a change to it, and update makes the
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
    pub pending: Pending,
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
    /// The TUI leaves with this status once the install that `y` at the
    /// leave question started ends well.
    pub quit_after_install: Option<u8>,
    /// The changes made since the draft was last read or saved, which
    /// [`Shared::draft`]'s text holds and the store does not hold until
    /// Save writes them.
    ///
    /// References:
    /// - ADR-1403 D7, "Changes wait for Save": the changes stay unsaved
    ///   until Save.
    pub unsaved: usize,
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

    /// The frits an install writes and removes with the draft, frits
    /// first, as apply writes them. Refused when the draft names a frit
    /// it removes, or moves the frits' directory while frits change,
    /// since they would go to the directory the TUI started with.
    pub fn frit_plan(&self) -> Result<Plan, String> {
        self.frit_plan_of(&self.pending)
    }

    /// The install's frits as [`Shared::frit_plan`] says, with `p` in
    /// place of the frits that wait for the install now.
    pub fn frit_plan_of(&self, p: &Pending) -> Result<Plan, String> {
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
    Install(Plan),
    /// Leaves with this status while new or removed frits wait for an
    /// install: `y` installs them first, and `n` leaves them uninstalled.
    /// Both save the unsaved changes.
    Leave(u8),
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
                pending: Pending::default(),
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
                quit_after_install: None,
                unsaved: 0,
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
        let mut first = vec![
            Effect::Write(app.shared.replies.send(start_batch())),
            Effect::Wait(WAIT),
        ];
        if app.shared.inputs.desk {
            first.push(Effect::Desk(Op::Open));
        }
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
            _ => return,
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
            Event::Celled { font, height, cell } => self.celled(&font, height, cell),
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
        if let Some(asked) = asked
            && shown.as_ref() != Some(&asked)
        {
            self.shared.refused.insert(asked.clone());
            if self.shared.want.as_deref() == Some(asked.as_str()) {
                self.shared.want = shown.clone().or_else(|| self.shared.shown_font.clone());
            }
            if self.nav.trial.as_ref().is_some_and(|t| t.font == asked) {
                self.nav.trial = None;
                self.shared.message = Some(super::words::trial_refused(&asked));
            }
        }
        if shown.is_some() {
            self.shared.shown_font = shown;
        }
        self.follow()
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
        let waiting = self.shared.inputs.font_files.waiting();
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
            .send(switch_batch(Asker::Switch, &[], &name));
        self.shared.in_flight = Some(name);
        vec![Effect::Write(bytes)]
    }

    /// Leaves with `status`, giving the console its start font back first
    /// when a switch changed it (D4).
    pub fn leave(&mut self, status: u8) -> Vec<Effect> {
        if self.shared.leaving.is_some() {
            return Vec::new();
        }
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
        match (&self.shared.live, self.shared.start_font.clone()) {
            (Live::On, Some(start)) if back => {
                let bytes = self
                    .shared
                    .replies
                    .send(switch_batch(Asker::Leave, &bytes, &start));
                self.shared.leaving = Some(status);
                self.shared.message = Some(super::words::LEAVING.to_owned());
                vec![Effect::Write(bytes), Effect::Wait(WAIT)]
            }
            // a query is still outstanding: the switch in flight, which
            // gives the start font back, so nothing is sent again, or the
            // DA1 after a switch already answered. The TUI waits for the
            // replies, so that none reaches the shell after it leaves.
            (Live::On, Some(_)) if self.shared.replies.awaiting() => {
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
            Key::Char('o') if takes('o') && self.shared.live == Live::OpsOff => {
                return self.top("allow-font-ops", Some("on"));
            }
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
                Command::Change(Change::Pending(pending)) => self.shared.pending = pending,
                Command::Change(Change::Try(font)) => effects.extend(self.try_live(font)),
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
                // font sets are listed in the Looks tab
                Request::ShowFont(_) => effects.extend(self.set_view(View::Looks)),
            }
        }
        effects
    }

    /// A key below the TUI's minimum size, where no view is drawn: `q`
    /// quits, or asks the quit question first while changes are not
    /// saved, and `u` puts back the font the console had when the TUI
    /// started, where a preview changed it. Ctrl-C saves and leaves, as
    /// it does everywhere.
    ///
    /// References:
    /// - ADR-1201 D3: below the minimum size only `q`, `u` and Ctrl-C act.
    /// - ADR-1403 D7: `q` asks before it drops changes that are not saved.
    fn small_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Char('q') => self.quit(0),
            Key::Char('u') => {
                let Some(start) = self.shared.start_font.clone() else {
                    return Vec::new();
                };
                self.shared.want = Some(start);
                self.follow()
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

    /// The frits an install writes and removes, as [`Shared::frit_plan`]
    /// says.
    pub fn frit_plan(&self) -> Result<Plan, String> {
        self.shared.frit_plan()
    }

    /// After an install: the frits it removed leave the list, nothing is
    /// pending, and on a live console the font the draft gives the
    /// console's head is the one leaving gives back (ADR-1201 D4).
    pub(crate) fn frits_installed(&mut self) {
        let removed = std::mem::take(&mut self.shared.pending.removals);
        self.shared
            .inputs
            .fonts
            .retain(|f| !removed.contains(&f.name));
        for name in &removed {
            self.shared.inputs.installed.remove(name);
            self.shared.sources.remove(name);
        }
        for (name, text) in std::mem::take(&mut self.shared.pending.frits) {
            self.shared.inputs.installed.insert(name, text.into_bytes());
        }
        self.fonts.keep(&self.shared, None);
        self.shared.inputs.conf = self.shared.draft.as_ref().map(|d| d.text.clone());
        // the font the installed file gives the console's head, from its
        // look where the look names one
        if self.shared.live == Live::On
            && let Some(name) = self.shared.head_font()
        {
            self.shared.start_font = Some(name);
        }
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
