//! What one step of a macro does to the TUI: the keys it presses, one at a
//! time with the TUI at rest before each, and the checks it makes.
//!
//! A key step and `Type` become keys at once. A `Click` finds its label
//! ([`super::label`]) and becomes the keys that reach it. A label on a
//! line or a box of the viewer, or a menu item with no letter, is reached
//! by moving the highlight one key at a time, reading the screen again
//! after each, until the highlight is on the label. The highlight that
//! stops moving before it gets there fails the step.
//!
//! References:
//! - ADR-1208 D3: the player waits until the TUI rests before each step,
//!   and a click acts through the keys a person would press: a menu
//!   item's letter, a button's key, a tab's Alt letter, or Up and Down
//!   until the line is highlighted, Left and Right for a box in the Flow
//!   tab. A greyed item is pressed all the same.
//! - ADR-1208 D2: what each step does, and each `Expect`.

use vtium_config_macro::{Character, Expectation, Key as MacroKey, KeyPress, Step};
use vtium_config_widgets::keys::Event as Input;
use vtium_config_widgets::menu::Pick;

use super::label::{self, Found, Place, Screen, begins};
use crate::app::{App, Event};

/// The most keys one `Click` presses to move a highlight, so a list that
/// never reaches its label cannot hold a run forever.
const SEEK_MAX: usize = 500;

/// One piece of a step's work, done while the TUI rests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Work {
    /// Presses a key.
    Key(KeyPress),
    /// Hands the TUI a mouse report, as a click sends it.
    Mouse(Input),
    /// Moves the highlight until it is on a label.
    Seek(Seek),
}

/// A highlight on its way to a label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Seek {
    label: String,
    toward: Toward,
    pressed: usize,
    /// Where the highlight was before the last key, to see that it moved.
    last: Option<String>,
}

/// What the highlight moves through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Toward {
    /// The lines of the viewer, with Up and Down.
    Line,
    /// The boxes of the viewer, with Left and Right.
    Box,
    /// The items of the open menu with no letter, with Up and Down, then
    /// Enter.
    Menu(Pick),
}

/// What a step needs from the TUI when it begins: work to do, or nothing.
pub(super) type Begun = Result<Vec<Work>, String>;

/// The work of a key step, a `Type` or a click, or the outcome of a check,
/// as the TUI rests at the step's start. A `Sleep`, a `Screenshot`, `Set
/// Live`, a comment and a blank line have no work here.
pub(super) fn begin(app: &App, step: &Step, encoder: &mut super::encode::Encoder) -> Begun {
    match step {
        Step::Key(k) => Ok((0..k.count().get()).map(|_| Work::Key(k.press())).collect()),
        Step::Type(text) => text
            .as_str()
            .chars()
            .map(|c| typed(c).map(Work::Key))
            .collect(),
        Step::Click(l) => click(app, l.as_str()),
        Step::ClickMouse(l) => {
            let screen = Screen::of(app);
            let found = label::find(app, &screen, l.as_str())?;
            Ok(encoder
                .click(found.cell.0, found.cell.1)
                .into_iter()
                .map(Work::Mouse)
                .collect())
        }
        Step::Expect(e) => expect(app, e).map(|()| Vec::new()),
        Step::Sleep(_) | Step::Screenshot(_) | Step::SetLive | Step::Comment(_) | Step::Blank => {
            Ok(Vec::new())
        }
    }
}

/// The key that types `c`: Space for a space, and the character itself
/// for the rest.
pub(super) fn typed(c: char) -> Result<KeyPress, String> {
    if c == ' ' {
        return Ok(KeyPress::plain(MacroKey::Space));
    }
    Character::new(c)
        .map(|c| KeyPress::plain(MacroKey::Char(c)))
        .ok_or_else(|| format!("{c:?} cannot be typed: it is no key a terminal sends."))
}

/// The work of `Click "LABEL"`.
fn click(app: &App, label: &str) -> Begun {
    let screen = Screen::of(app);
    let found = label::find(app, &screen, label)?;
    if let Some(keys) = found.keys() {
        return keys.map(|k| k.into_iter().map(Work::Key).collect());
    }
    let toward = match found.place {
        Place::Line { .. } => Toward::Line,
        Place::Box { .. } => Toward::Box,
        Place::MenuItem {
            greyed: Some(why), ..
        } => {
            return Err(format!(
                "{label:?} is greyed out ({why}) and has no letter to press."
            ));
        }
        Place::MenuItem { pick, .. } => Toward::Menu(pick),
        Place::Button { .. } | Place::Menu { .. } | Place::Tab { .. } => return Ok(Vec::new()),
    };
    Ok(vec![Work::Seek(Seek {
        label: label.to_owned(),
        toward,
        pressed: 0,
        last: None,
    })])
}

impl Seek {
    /// The next key that moves the highlight toward the label, or `None`
    /// once the label is reached.
    pub(super) fn next(&mut self, app: &App) -> Result<Option<KeyPress>, String> {
        let screen = Screen::of(app);
        let (here, key) = match self.toward {
            Toward::Line => self.line(&screen)?,
            Toward::Box => self.boxed(&screen)?,
            Toward::Menu(pick) => match self.menu(app, pick)? {
                Some(step) => step,
                None => return Ok(None),
            },
        };
        let Some(key) = key else {
            return Ok(None);
        };
        if self.last.as_ref() == Some(&here) || self.pressed >= SEEK_MAX {
            return Err(format!(
                "the highlight stops before it reaches {:?}.",
                self.label
            ));
        }
        self.last = Some(here);
        self.pressed = self.pressed.saturating_add(1);
        Ok(Some(KeyPress::plain(key)))
    }

    /// Where the highlighted line is, and Down or Up toward the line that
    /// begins with the label, or no key on it.
    fn line(&self, screen: &Screen) -> Result<(String, Option<MacroKey>), String> {
        let Some(at) = screen.highlight() else {
            return Err(format!(
                "nothing in the viewer is highlighted, so no key moves to {:?}.",
                self.label
            ));
        };
        let text = screen.viewer_line(at);
        if begins(&text, &self.label) {
            return Ok((text, None));
        }
        let target = label::viewer_hits(screen, &self.label)
            .into_iter()
            .find_map(|f| match f.place {
                Place::Line { row } => Some(row),
                _ => None,
            })
            .ok_or_else(|| format!("no line of the viewer begins with {:?}.", self.label))?;
        let key = if target > at {
            MacroKey::Down
        } else {
            MacroKey::Up
        };
        Ok((format!("{at}:{text}"), Some(key)))
    }

    /// Where the heavy box is, and Right or Left toward the box whose
    /// first line begins with the label, or no key on it.
    fn boxed(&self, screen: &Screen) -> Result<(String, Option<MacroKey>), String> {
        let hits: Vec<Found> = label::viewer_hits(screen, &self.label)
            .into_iter()
            .filter(|f| matches!(f.place, Place::Box { .. }))
            .collect();
        let Some(target) = hits.first() else {
            return Err(format!(
                "no box of the viewer begins with {:?}.",
                self.label
            ));
        };
        let Some(at) = screen.heavy_box() else {
            return Err(format!(
                "no box of the viewer is highlighted, so no key moves to {:?}.",
                self.label
            ));
        };
        let Place::Box { row } = target.place else {
            return Ok((String::new(), None));
        };
        if row == at.1 && target.cell.0 > at.0 && target.cell.0 <= at.2 {
            return Ok((String::new(), None));
        }
        let after = (row, target.cell.0) > (at.1, at.0);
        let key = if after {
            MacroKey::Right
        } else {
            MacroKey::Left
        };
        Ok((format!("{}:{}", at.1, at.0), Some(key)))
    }

    /// Where the open menu's highlight is, and Down or Up toward the
    /// item, or Enter on it. `None` once Enter has been pressed.
    fn menu(&self, app: &App, pick: Pick) -> Result<Option<(String, Option<MacroKey>)>, String> {
        let Some(open) = app.nav().menu() else {
            // Enter chose the item, which closed the menu
            return Ok(None);
        };
        let (now, want) = match pick {
            Pick::Item(i) => (open.highlight, i),
            Pick::Sub(i) => (open.sub.as_ref().and_then(|s| s.highlight), i),
        };
        let key = match now {
            Some(h) if h == want => MacroKey::Enter,
            Some(h) if h > want => MacroKey::Up,
            _ => MacroKey::Down,
        };
        if self.last.as_deref() == Some("enter") {
            return Ok(None);
        }
        let here = if key == MacroKey::Enter {
            "enter".to_owned()
        } else {
            format!("{now:?}")
        };
        Ok(Some((here, Some(key))))
    }
}

/// Whether the TUI, at rest, holds what `expectation` says, or why not.
pub(super) fn expect(app: &App, expectation: &Expectation) -> Result<(), String> {
    let count = app.shared().document().count();
    match expectation {
        Expectation::Shows(text) => {
            if Screen::of(app).shows(text.as_str()) {
                Ok(())
            } else {
                Err("the screen does not show it.".to_owned())
            }
        }
        Expectation::ShowsNot(text) => {
            if Screen::of(app).shows(text.as_str()) {
                Err("the screen shows it.".to_owned())
            } else {
                Ok(())
            }
        }
        Expectation::At(node) => {
            let at = app.node();
            if at == node.as_str() {
                Ok(())
            } else {
                Err(format!("the TUI is at {at}."))
            }
        }
        Expectation::Saved if count == 0 => Ok(()),
        Expectation::Saved => Err(super::words::not_saved(count)),
        Expectation::Unsaved if count > 0 => Ok(()),
        Expectation::Unsaved => Err("every change is saved.".to_owned()),
        Expectation::Exited(_) => Err("the TUI has not exited.".to_owned()),
    }
}

/// What a step does once the TUI has left with `status`: an `Expect
/// Exited` checks the status, and every other step that acts fails.
pub(super) fn after_exit(step: &Step, status: u8) -> Result<(), String> {
    match step {
        Step::Expect(Expectation::Exited(n)) if *n == status => Ok(()),
        Step::Expect(Expectation::Exited(_)) => {
            Err(format!("the TUI exited with status {status}."))
        }
        Step::Comment(_) | Step::Blank | Step::Sleep(_) => Ok(()),
        _ => Err(format!("the TUI has exited, with status {status}.")),
    }
}

/// The event a piece of work hands the TUI, for a mouse report.
pub(super) fn mouse(input: Input) -> Option<Event> {
    match input {
        Input::Mouse(m) => Some(Event::Mouse(m)),
        Input::Key(k) => Some(Event::Key(k)),
        Input::Reply(_) => None,
    }
}
