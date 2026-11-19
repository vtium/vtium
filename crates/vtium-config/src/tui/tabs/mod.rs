//! The TUI's tabs, each a module of its own that holds the tab's state and
//! its update. A tab's update takes the tab's state mutably, the model's
//! shared part read-only, and the key that the navigation module hands to
//! the open tab. It returns [`Command`]s, which update performs in one
//! place: effects go to the loop, changes go to the shared part, and
//! requests go to the navigation module. So no tab changes the shared
//! part or another tab's state itself.
//!
//! Each tab answers the shared files through the [`Tab`] trait: its name,
//! its node of the interface tree, its body, its preview, its key line
//! and its keys. Update, the layout, the navigation module and the view
//! ask the open tab through `App::tab`, which is the one place that
//! names each tab's state. So a change to a tab edits the tab's module,
//! and a new tab adds its state to the model and one line to `App::tab`
//! and `App::tab_mut`.
//!
//! References:
//! - ADR-1206 D2: the model's four parts, and the three kinds of command a
//!   tab's update returns.
//! - ADR-1206 D4: a tab's update calls its own state, the shared part
//!   read-only and words.rs, and never another tab's module.
//! - ADR-1206 D6, slice 5: the tabs that ADR-1205 sets are built as
//!   modules of this structure.

pub mod effects;
pub mod flow;
pub mod fonts;
pub mod help;
pub mod looks;
pub mod settings;

use ratatui_core::text::Line;

use super::color::Style;
use super::edit::Change as DraftChange;
use super::flow::Item;
use super::input::Key;
use super::listbox::Body;
use super::model::{Effect, Pending, Shared, Size, TYPED_MAX, Then, View};
use super::shot::Preview;
use super::words;

/// What the shell of the screen leaves a tab, which update reads from the
/// layout before it hands the tab a key: the terminal's size, the rows the
/// key line takes, and the rows of the body between the rule under the tab
/// bar and the message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Room {
    pub size: Size,
    pub keys: usize,
    pub body: usize,
}

/// What a tab answers for the parts of the TUI that every tab shares: the
/// tab bar, the navigation module, the layout, the pictures, the console's
/// own preview, the key line and update. Each method reads the tab's own
/// state, and only [`Tab::update`], [`Tab::type_key`] and
/// [`Tab::show_from`] change it. A method that a tab does not provide
/// answers as a tab with no list, no preview and no typed value does.
///
/// References:
/// - ADR-1206 D3: the navigation module asks the open tab for its node.
/// - ADR-1206 D4: a tab's layout and its update read its own state and
///   the shared part, and the view draws what the layout places.
pub trait Tab {
    /// The tab's name in the tab bar.
    fn name(&self) -> &'static str;

    /// The actions the tab bar shows while the tab is open, each in its
    /// long and its short form: Save and Install, unless the tab has
    /// actions of its own.
    fn actions(&self) -> &'static [(&'static str, &'static str)] {
        &words::ACTIONS
    }

    /// The node of the interface tree the tab is at: the tab itself, or
    /// the level open in it.
    fn node(&self) -> &'static str;

    /// The tab's body on a screen of `size`: its list, the row of the list
    /// to keep in view, and the sentences under the list. A tab that draws
    /// its own page, or none, has an empty body.
    fn body(&self, _shared: &Shared, _size: Size) -> Body {
        Body::default()
    }

    /// The first row of the tab's list that its window shows.
    fn offset(&self) -> usize {
        0
    }

    /// Keeps the tab's list in view from its row `row`, which the layout
    /// gives after every event.
    fn show_from(&mut self, _row: usize) {}

    /// Where a picture goes in the tab, row and column from 1.
    fn picture_at(&self, _shared: &Shared, _size: Size) -> (usize, usize) {
        (super::layout::PICTURE_ROW, super::layout::PICTURE_COL)
    }

    /// What the tab previews, and in which font.
    fn preview<'a>(&self, shared: &'a Shared, size: Size) -> Preview<'a> {
        Preview::nothing(shared, size)
    }

    /// The lines a vtium console draws as the tab's preview, in its own
    /// font, where a picture would go elsewhere: none for a tab with no
    /// preview.
    fn console_lines(&self, _shared: &Shared) -> Vec<Vec<(&'static str, Style)>> {
        Vec::new()
    }

    /// The tab's hand-written key line, while its node of the interface
    /// tree is still planned. A supported node's key line comes from the
    /// contract.
    fn planned_keys(&self, _shared: &Shared) -> &'static str {
        ""
    }

    /// The page the tab draws itself in place of a list, with the page's
    /// first row in sight: none for a tab with a list.
    fn page(&self, _shared: &Shared, _size: Size) -> Option<(Vec<Line<'static>>, usize)> {
        None
    }

    /// Whether a key's character is text typed into a value of the tab.
    fn typing(&self) -> bool {
        false
    }

    /// A character or Backspace typed into the tab's value being edited.
    fn type_key(&mut self, _key: Key) {}

    /// Whether the action key `action` acts in the tab as it stands: `i`
    /// installs, `s` saves and `o` turns `allow-font-ops` on. A tab whose
    /// own keys use the letter, or with a level open over it, leaves the
    /// action alone.
    ///
    /// References:
    /// - ADR-1403 D7: the action keys do nothing while a layer is open.
    fn takes_action(&self, _action: char) -> bool {
        true
    }

    /// A key the navigation module hands to the open tab, with what the
    /// shell of the screen leaves the tab. The tab changes its own state
    /// and returns what update does for it.
    fn update(&mut self, shared: &Shared, room: Room, key: Key) -> Vec<Command>;
}

/// What a tab's update asks for, which update performs once the tab's
/// update returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// An effect for the loop to perform, such as a write to the terminal.
    Effect(Effect),
    /// A change to the shared part of the model.
    Change(Change),
    /// A request to the navigation module.
    Request(Request),
}

/// A change to the shared part of the model, which update makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Sets the top-level key `key` to `value` in the draft, or leaves it
    /// at its default with `None`.
    Top {
        key: &'static str,
        value: Option<String>,
    },
    /// Puts this line on the message line.
    Say(String),
    /// Replaces the frits that wait for the install with these.
    Pending(Pending),
    /// Saves the font the console shows, which the live trial put there.
    SaveTried,
    /// Tries this font on the live console.
    Try(String),
    /// Clears the message line.
    Quiet,
    /// The Fonts tab's highlight moved to this place in the list of fonts.
    /// Update hands the place to the Looks tab, which previews the font
    /// that the Fonts tab highlights until looks name fonts of their own.
    Preview(usize),
    /// Makes `changes` on the draft, in order, and hands the result to the
    /// desk to check, then selects `select` in the Flow view once the desk
    /// answers. The tab checks first that the changes can be made, with
    /// [`super::model::Shared::apply`].
    Make {
        changes: Vec<DraftChange>,
        select: Option<Item>,
    },
    /// Writes the unsaved changes into the draft: Save.
    Save,
    /// One of the action keys `i`, `d`, `v`, `r`, `o` and `n`.
    Action(char),
}

/// A request to the navigation module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Goes to this view.
    Go(View),
    /// Asks this question, which does `then` on a yes.
    Ask { question: String, then: Then },
    /// Shows the font set `name` in the Fonts tab, highlighted.
    ShowFont(String),
}

/// Whether a value being typed takes the character `c`. Every tab types
/// through this test, so no value takes a control character, C0 or C1.
/// A value goes into vtium.conf as it is typed, and a C1 control such as
/// U+009B is CSI to a terminal that reads 8-bit controls, so the file
/// would act on the terminal of whoever shows it.
pub fn typable(c: char) -> bool {
    !c.is_control()
}

/// A character or Backspace typed into `text`, the value being edited. A
/// character that [`typable`] refuses does nothing, and `text` holds at
/// most [`TYPED_MAX`] bytes.
pub fn type_into(text: &mut String, key: Key) {
    match key {
        Key::Char(c) if typable(c) && text.len() < TYPED_MAX => text.push(c),
        Key::Backspace => {
            text.pop();
        }
        _ => {}
    }
}
