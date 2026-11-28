//! The model of the TUI: the navigation state, the tabs, the shared state
//! every tab reads, and the size of the screen. The loop turns everything
//! that wakes it into an [`Event`] and hands it to [`App::event`], the one
//! update in [`update`], and changes the model in no other way.
//!
//! References:
//! - ADR-1206 D2: the model's four parts, the message, and one update.
//! - ADR-1206 D9: the shell holds the tabs as trait objects.

pub mod update;

use vtium_config_core::ui::Shared;
use vtium_config_widgets::frame::Mode;
use vtium_config_widgets::keys::{Key, Mouse, Reply};

use crate::BoxTab;
use crate::menus::{self, Line};
use crate::nav::Nav;
use crate::nav::menus::Which;

/// The smallest screen the TUI draws on: 80 columns by 25 rows.
pub const MIN_SIZE: (u16, u16) = (80, 25);

/// The screen's size: its grid, and its character cell in pixels where the
/// terminal reports one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    pub cell_px: Option<(u32, u32)>,
}

impl Size {
    /// A grid of `cols` by `rows` with no size in pixels.
    pub const fn grid(cols: u16, rows: u16) -> Size {
        Size {
            cols,
            rows,
            cell_px: None,
        }
    }

    /// Whether the grid is below the minimum the TUI draws on.
    pub fn too_small(&self) -> bool {
        self.cols < MIN_SIZE.0 || self.rows < MIN_SIZE.1
    }
}

/// One thing that woke the loop, as the model receives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A key.
    Key(Key),
    /// A mouse report: the TUI reads a press of the left button.
    Mouse(Mouse),
    /// A reply to one of the TUI's requests.
    Reply(Reply),
    /// The terminal changed its size.
    Resize(Size),
    /// One of the signals the loop holds, by its number.
    Signal(i32),
    /// A wait the update asked for ended.
    Timeout,
    /// The devices connected now, which the loop read for Read devices
    /// again, or `None` where it cannot read them.
    Devices(Option<vtium_config_core::flowmodel::Present>),
    /// The font file the loop readied as a system font, or why it cannot
    /// be one.
    FontReady(Result<crate::live::machine::Ready, String>),
    /// The install of the readied font ended, or why it failed.
    FontInstalled(Result<(), String>),
    /// What the loop read or did for Save.
    Save(crate::save::Came),
    /// The person's editor exited: the copy's text and what the check
    /// found in it, or why the editor could not run.
    Edited(Result<crate::editor::Edited, String>),
    /// A line for the message row from the macro player: where a macro
    /// stopped, ended or failed, with the failure report behind F1.
    Said(vtium_config_core::ui::Message),
    /// What `vtium check-config` found in `document`, the open document as
    /// it stood when the loop checked it, with the path its lines name the
    /// draft by; or why the check could not run.
    Checked {
        document: vtium_config_core::document::Document,
        found: Result<(vtium_config_core::drafts::Checked, std::path::PathBuf), String>,
    },
    /// What the walk of the fonts in use reads on this machine, which the
    /// loop read at start and again on Read devices again.
    FontReads(vtium_config_core::ui::shared::FontReads),
}

/// The TUI's model.
pub struct App {
    nav: Nav,
    tabs: Vec<BoxTab>,
    shared: Shared,
    size: Size,
    /// Whether the screen is drawn in color, which `NO_COLOR` turns off.
    color: bool,
    /// The exit status once the TUI is leaving.
    leaving: Option<u8>,
    /// The live console's state: the questions at start, the trial, and
    /// the font readied as a system font.
    live: crate::live::State,
    /// What a macro's run makes of the document, which the top row says:
    /// a scratch copy, or LIVE. `None` while no macro plays.
    macro_mode: Option<Mode>,
    /// The `?` search as the open tab was last handed it, and that tab.
    handed: Option<(usize, vtium_config_widgets::search::Search)>,
    /// The document as it stood when the loop last checked it, whether or
    /// not the check could run, so a document is checked once.
    checked: Option<vtium_config_core::document::Document>,
}

impl App {
    /// The TUI on `tabs`, in the top row's order, over `shared`, on a
    /// screen of `size`, on the first tab with nothing open.
    pub fn new(tabs: Vec<BoxTab>, shared: Shared, size: Size, color: bool) -> App {
        App {
            nav: Nav::new(tabs.len()),
            tabs,
            shared,
            size,
            color,
            leaving: None,
            live: crate::live::State::default(),
            macro_mode: None,
            handed: None,
            checked: None,
        }
    }

    /// Says on the top row that a macro plays on a scratch copy, or live.
    ///
    /// References:
    /// - ADR-1208 D5: "scratch copy" or LIVE on the top row.
    pub fn set_macro_mode(&mut self, mode: Option<Mode>) {
        self.macro_mode = mode;
    }

    /// What a macro's run makes of the document, `None` while no macro
    /// plays.
    pub fn macro_mode(&self) -> Option<Mode> {
        self.macro_mode
    }

    /// Where the person is.
    pub fn nav(&self) -> &Nav {
        &self.nav
    }

    /// The tabs, in the top row's order.
    pub fn tabs(&self) -> &[BoxTab] {
        &self.tabs
    }

    /// The state every tab reads.
    pub fn shared(&self) -> &Shared {
        &self.shared
    }

    /// The screen's size.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Whether the screen is drawn in color.
    pub fn color(&self) -> bool {
        self.color
    }

    /// The exit status, once the TUI is leaving.
    pub fn leaving(&self) -> Option<u8> {
        self.leaving
    }

    /// Whether a reply is still due, which decides whether a lone ESC at
    /// the end of a read is the Esc key or the start of a reply.
    pub fn awaiting(&self) -> bool {
        self.shared.askers.awaiting()
    }

    /// The live console's state.
    pub fn live(&self) -> &crate::live::State {
        &self.live
    }

    /// Whether the open document waits for `vtium check-config`: it
    /// changed since the loop last checked it. The loop checks it once the
    /// person pauses, never on each key.
    pub fn check_due(&self) -> bool {
        self.leaving.is_none() && self.checked.as_ref() != Some(self.shared.document())
    }

    /// The facts the previews read: this terminal's grid and cell, whether
    /// it draws color, and what the shared state holds of the machine.
    pub fn facts(&self) -> vtium_config_core::preview::Facts {
        vtium_config_core::preview::Facts::of(
            &self.shared.machine,
            (usize::from(self.size.cols), usize::from(self.size.rows)),
            self.size.cell_px,
            self.color,
            None,
        )
    }

    /// The node of the interface tree the person is on: the live trial's
    /// countdown while one runs, since it takes every key.
    pub fn node(&self) -> &'static str {
        if self.live.trial.is_some() && !self.size.too_small() {
            return "any.trial";
        }
        self.nav.node(&self.tabs, self.size.too_small())
    }

    /// The line the TUI prints on the shell after it leaves, while the
    /// document holds changes that are not saved: how many wait, and that
    /// vtium-config opens them again.
    ///
    /// References:
    /// - ADR-1403 D11, "Quitting, Ctrl-C and the signals": one line on
    ///   leaving says how many changes wait.
    /// - ADR-1208 D5: quitting a scratch copy throws it away without a
    ///   question, so no line follows.
    pub fn farewell(&self) -> Option<String> {
        // a scratch copy goes with the run, so nothing of the person's
        // waits
        if self.macro_mode == Some(Mode::ScratchCopy) {
            return None;
        }
        let document = self.shared.document();
        let n = document.count();
        (n > 0).then(|| {
            let changes = if n == 1 { "change" } else { "changes" };
            format!(
                "vtium-config: {} has {n} {changes}, not saved. vtium config tui opens {} again.",
                document.name(),
                if n == 1 { "it" } else { "them" },
            )
        })
    }
}

/// The lines of the menu `which` over the open tab: File's, Actions' with
/// the tab's own items after them, or `more`, the buttons the action bar
/// has no room for.
pub(crate) fn menu_lines(
    shared: &Shared,
    tab: Option<&BoxTab>,
    node: &'static str,
    which: Which,
    more: &[Line],
) -> Vec<Line> {
    let cx = menus::Context { shared, node };
    match which {
        Which::File => menus::file::lines(&cx),
        Which::Actions => {
            let items = tab.map(|t| t.menu_items(shared)).unwrap_or_default();
            let own = tab.and_then(|t| t.own_font(shared));
            menus::actions::lines(&cx, items, own.is_some())
        }
        Which::More => more.to_vec(),
    }
}
