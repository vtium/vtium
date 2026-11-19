//! The navigation module: the one place that knows where the person is in
//! the TUI, and that decides what happens to a key before any view sees
//! it.
//!
//! Where the person is, is a node of the interface tree, such as `flow`,
//! `flow.card`, `settings.section.typing` or `any.question.quit`.
//! [`node`] works it out from the model, and nothing else in the TUI or
//! its tests derives it again: the key line, the help, the test of whether
//! Tab may change the tab, the mouse and the scenario walk all ask
//! [`node`], [`kind`] or [`something_open`].
//!
//! [`route`] turns a key into a [`Route`]. It reads the rows of the key
//! table that the open node lists, compiled into
//! [`super::contract::Node::rows`], for the keys that act the same in every
//! tab: Tab, Shift-Tab, the digits, `q`, Ctrl-C and `.`. A supported row
//! acts; a planned row leaves the key to the view, which does what the
//! row's `now` column says the TUI does today. A question, the live
//! trial's countdown and the screen below the minimum size take their own
//! keys, and a value being typed takes its characters. Every other key,
//! Esc and Enter included, goes to the open view.
//!
//! The interface the later slices build on:
//!
//! - [`node`] gives the node's id, as the tree writes it, and [`kind`] the
//!   kind of state the key table names for it.
//! - [`something_open`] says whether a layer, a typed value or a question
//!   is open over the tab, which is when Tab, Shift-Tab, the digits and a
//!   click on a tab's name do nothing.
//! - [`route`] answers each key with a [`Route`], and `App::key` performs
//!   it. [`Route::View`] hands the key to the open view.
//! - [`digit_tab`] gives the tab a digit goes to, where the TUI has that
//!   tab.
//!
//! [`Nav`] holds the open tab, the question over everything, the live
//! trial and the key line's page. What is open inside a tab is the tab's
//! own state, so [`node`] asks the open tab for its node.
//!
//! References:
//! - ADR-1206 D3: one navigation module owns where the person is, reads
//!   the compiled tables, and handles the keys that are the same
//!   everywhere.
//! - ADR-1206 D6: this is the first slice, which takes over the precedence
//!   of the keys, the test of what holds the tab, the contract's node and
//!   the scenario walk's abstract state.
//! - ADR-1403 D10, "The number keys": the digits 1 to 6 go straight to a
//!   tab while nothing is open.

use super::contract;
use super::input::Key;
use super::model::{App, Confirm, Then, View};

/// Where the person is in the TUI: the open tab, the question over
/// everything when one is open, the live trial, and the key line's page.
/// What is open inside a tab, its card, its list or its typed value, is
/// the tab's own state, which [`node`] asks each tab for.
///
/// References:
/// - ADR-1206 D3: the navigation state, and the tab's `node` query it
///   reads.
#[derive(Clone, Debug)]
pub struct Nav {
    /// The open tab.
    pub view: View,
    /// The live trial counting down, if one is.
    pub trial: Option<super::shot::Trial>,
    /// The question the view asks before it acts, and what it does on yes.
    pub confirm: Option<Confirm>,
    /// The page of the key line that shows, counted from 0. It goes back
    /// to the first page whenever the TUI moves to another node of the
    /// interface tree.
    pub key_page: usize,
}

/// The kind of state a node is, as the key table names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A tab with nothing open over it.
    View,
    /// Something open over a tab that Esc closes: a card, a detail, a
    /// list, a form, the file view or Restore's list.
    Layer,
    /// A value being typed.
    Typing,
    /// A question that waits for an answer.
    Question,
    /// The live trial's countdown.
    Trial,
    /// The screen below the minimum size.
    TooSmall,
}

/// What happens to a key, as the navigation module decides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The TUI is leaving or installing, and takes no key.
    Ignore,
    /// The key does nothing here.
    Nothing,
    /// The open question takes the key: its answers, or a cancel.
    Question,
    /// The live trial's countdown takes the key.
    Trial,
    /// Below the minimum size, the few keys that act there take the key.
    TooSmall,
    /// Ctrl-C: the changes are saved and the TUI leaves.
    SaveAndQuit,
    /// `q`: the TUI quits, or asks first while changes are not saved.
    Quit,
    /// A character or Backspace goes into the value being typed.
    Type,
    /// `.`: the key line shows its next page, of this many.
    NextPage(usize),
    /// Tab (`true`) or Shift-Tab (`false`): the next tab or the one
    /// before.
    Step(bool),
    /// A digit: straight to this tab.
    GoTo(View),
    /// The open view acts on the key itself.
    View,
}

/// The node of the interface tree that the TUI is in: `flow.card`,
/// `any.question.quit`, `settings`. A question comes first, since it takes
/// the next key, then the live trial, then the screen below the minimum
/// size, then a value being typed, then what is open over the view.
pub fn node(app: &App) -> &'static str {
    if let Some(confirm) = &app.nav.confirm {
        return match confirm.then {
            Then::Quit(_) => "any.question.quit",
            Then::Leave(_) => "any.question.leave",
            _ => "any.question.confirm",
        };
    }
    if app.nav.trial.is_some() {
        return "any.trial";
    }
    if super::layout::too_small(app.size) {
        return "any.too-small";
    }
    match app.nav.view {
        View::Flow => app.flow.node(),
        View::Settings => app.settings.node(),
        View::New => app.fonts.form_node(),
        View::Looks => app.looks.node(),
        View::Fonts => app.fonts.node(),
        View::Help => app.help.node(),
    }
}

/// The kind of state of the node `id`, read from where the id sits in the
/// tree: a tab is a single word, a typed value ends in `.typing`, and the
/// questions, the trial and the small screen sit under `any`.
pub fn kind(id: &str) -> Kind {
    match id {
        "any.trial" => Kind::Trial,
        "any.too-small" => Kind::TooSmall,
        _ if id.starts_with("any.question.") => Kind::Question,
        _ if id.ends_with(".typing") => Kind::Typing,
        _ if !id.contains('.') => Kind::View,
        _ => Kind::Layer,
    }
}

/// Whether something is open over the tab that a change of tab would leave
/// behind: a question, a layer or a value being typed. While one is, Tab,
/// Shift-Tab, the digits and a click on a tab's name do nothing.
pub fn something_open(app: &App) -> bool {
    kind(node(app)) != Kind::View
}

/// The tab each digit goes to, by the order of the six tabs: `1` Fonts,
/// `2` Effects, `3` Looks, `4` Flow, `5` Settings and `6` Help. The TUI
/// has no Effects tab yet, so `2` names none.
///
/// References:
/// - ADR-1403 D10, "The same key for the same action": the digits and
///   their tabs.
/// - ADR-1205 D1: the six tabs and their order.
const DIGITS: [(char, Option<View>); 6] = [
    ('1', Some(View::Fonts)),
    ('2', None),
    ('3', Some(View::Looks)),
    ('4', Some(View::Flow)),
    ('5', Some(View::Settings)),
    ('6', Some(View::Help)),
];

/// The tab the digit `c` goes to, where the TUI has that tab.
pub fn digit_tab(c: char) -> Option<View> {
    DIGITS.iter().find(|(d, _)| *d == c).and_then(|(_, v)| *v)
}

/// The prefix of the key table's row ids for `key`, for the keys that act
/// the same in every tab: `tab.view`, `q.layer`, `ctrl-c.any`, `3.view`.
fn row_prefix(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::Tab => "tab",
        Key::BackTab => "shift-tab",
        Key::Interrupt => "ctrl-c",
        Key::Char('q') => "q",
        Key::Char('.') => "period",
        Key::Char('1') => "1",
        Key::Char('2') => "2",
        Key::Char('3') => "3",
        Key::Char('4') => "4",
        Key::Char('5') => "5",
        Key::Char('6') => "6",
        _ => return None,
    })
}

/// What a supported row of the key table that acts the same in every tab
/// does: the row's id, read as its `means` column says. `None` for a row
/// whose key the view handles.
fn row_route(here: &str, id: &str, key: Key) -> Option<Route> {
    Some(match id {
        "tab.view" => Route::Step(true),
        "shift-tab.view" => Route::Step(false),
        "tab.layer" | "tab.typing" | "shift-tab.layer" | "shift-tab.typing" => Route::Nothing,
        "q.view" | "q.layer" | "q.layer.file-view" | "q.layer.restore" => Route::Quit,
        "ctrl-c.any" | "ctrl-c.trial" | "ctrl-c.too-small" => Route::SaveAndQuit,
        // where the keys fit on one row, `.` is the view's own key
        "period.view" | "period.layer" => {
            let pages = contract::key_pages(here).map_or(0, <[&str]>::len);
            if pages > 1 {
                Route::NextPage(pages)
            } else {
                return None;
            }
        }
        "1.view" | "2.view" | "3.view" | "4.view" | "5.view" | "6.view" => match key {
            Key::Char(c) => digit_tab(c).map_or(Route::Nothing, Route::GoTo),
            _ => return None,
        },
        _ => return None,
    })
}

/// What happens to `key` in the state `app` is in.
///
/// A question takes every key, then Ctrl-C saves and leaves from every
/// other state, then the live trial and the screen below the minimum size
/// take their keys. The name of a terminal or a part, typed in the Flow
/// view, takes every other key, and a Settings value or a field of the
/// new-frit form takes its characters. Then the supported rows that the
/// node lists decide Tab, Shift-Tab, the digits, `q` and `.`, and the open
/// view acts on every other key.
pub fn route(app: &App, key: Key) -> Route {
    if app.shared.leaving.is_some() || app.shared.installing {
        return Route::Ignore;
    }
    let here = node(app);
    let kind = kind(here);
    let rows = contract::find(here).map_or(&[][..], |n| n.rows);
    let row = |key: Key| {
        let prefix = row_prefix(key)?;
        rows.iter()
            .filter(|(id, supported)| {
                *supported
                    && id
                        .strip_prefix(prefix)
                        .is_some_and(|rest| rest.starts_with('.'))
            })
            .find_map(|(id, _)| row_route(here, id, key))
    };
    match kind {
        Kind::Question => return Route::Question,
        _ if key == Key::Interrupt => return row(key).unwrap_or(Route::View),
        Kind::Trial => return Route::Trial,
        Kind::TooSmall => return Route::TooSmall,
        // a name typed in the Flow view takes every key, Enter and Esc
        // included, which the view reads itself
        Kind::Typing if here.starts_with("flow.") => return Route::View,
        Kind::Typing if matches!(key, Key::Char(_) | Key::Backspace) => return Route::Type,
        _ => {}
    }
    row(key).unwrap_or(Route::View)
}
