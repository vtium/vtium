//! File › New…, as a sheet: the new configuration's name, which starts as
//! the machine's hostname, and where it starts, from what is saved or
//! from vtium's defaults. Up and Down move between the name and the two
//! starting points, a letter typed on the name edits it, Enter makes the
//! configuration and opens it, and Esc cancels. The configuration that
//! was open keeps its changes under File › Open, so New asks nothing.
//! With nothing installed, the first starting point is greyed out, and
//! the second is chosen.
//!
//! References:
//! - ADR-1403 D11, "New…: what is saved, or vtium's defaults": the name,
//!   the two starting points, the one chosen at first, and the keys.
//! - ADR-1403 D8, "vtium's store": a draft's name and the hostname New
//!   offers.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::store;
use vtium_config_widgets::frame::{Bar, Button, Side};
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::roles::Role;

use super::{Act, bar_row, put};

/// The sheet's title.
pub const TITLE: &str = "New configuration";

/// The sheet's node. The interface tree has no node for New… yet, so the
/// sheet reads its keys itself and names its own buttons.
pub const NODE: &str = "any.new";

/// Where a new configuration starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// A copy of /etc/vtium/vtium.conf as installed, with no font set
    /// changes, so it holds no change.
    Saved,
    /// vtium-config's short header and one look, named default, with
    /// every setting at its default.
    Defaults,
}

/// The field the highlight is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Name,
    Start(Start),
}

/// New…'s state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct New {
    name: String,
    field: Field,
    start: Start,
    /// Whether something is installed, which the first starting point
    /// copies.
    saved: bool,
    /// The names the configurations kept already have.
    taken: Vec<String>,
    /// The line that says what the open configuration keeps.
    keeps: String,
    /// Why Enter did not make the configuration, until the next key.
    note: Option<String>,
}

impl New {
    /// The sheet over the open configuration `open`, which holds `count`
    /// changes not saved, offering `hostname` as the name, made free of
    /// the names in `taken`; `saved` says whether something is installed.
    pub fn open(hostname: &str, open: &str, count: usize, taken: Vec<String>, saved: bool) -> New {
        let name = free(hostname, &taken);
        let keeps = match count {
            0 => format!("{open} stays under File › Open."),
            1 => format!("{open} keeps its 1 change, not saved; File › Open brings it back."),
            n => format!("{open} keeps its {n} changes, not saved; File › Open brings them back."),
        };
        let start = if saved { Start::Saved } else { Start::Defaults };
        New {
            name,
            field: Field::Name,
            start,
            saved,
            taken,
            keeps,
            note: None,
        }
    }

    /// The name typed.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The starting point chosen.
    pub fn start(&self) -> Start {
        self.start
    }

    /// Acts on `key`.
    pub fn key(&mut self, key: &Key) -> Act {
        self.note = None;
        if *key == Key::ctrl('c') {
            return Act::Pass;
        }
        if let (Field::Name, Some(c)) = (self.field, key.typed()) {
            self.name.push(c);
            return Act::Stay;
        }
        if !key.mods.is_empty() {
            return Act::Stay;
        }
        match key.code {
            Code::Escape => Act::Close,
            Code::Backspace if self.field == Field::Name => {
                self.name.pop();
                Act::Stay
            }
            Code::Up | Code::Down => {
                self.move_by(key.code == Code::Down);
                Act::Stay
            }
            Code::Enter => self.make(),
            _ => Act::Stay,
        }
    }

    /// The fields in order, the first starting point left out where
    /// nothing is installed.
    fn fields(&self) -> Vec<Field> {
        let mut out = vec![Field::Name];
        if self.saved {
            out.push(Field::Start(Start::Saved));
        }
        out.push(Field::Start(Start::Defaults));
        out
    }

    fn move_by(&mut self, down: bool) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.field).unwrap_or(0);
        let to = if down {
            (at + 1).min(fields.len().saturating_sub(1))
        } else {
            at.saturating_sub(1)
        };
        if let Some(&field) = fields.get(to) {
            self.field = field;
            if let Field::Start(start) = field {
                self.start = start;
            }
        }
    }

    /// Enter: the configuration to make, or why the name cannot be one.
    fn make(&mut self) -> Act {
        if !store::valid_name(&self.name) {
            self.note = Some(words::BAD_NAME.to_owned());
            return Act::Stay;
        }
        if self.taken.contains(&self.name) {
            self.note = Some(words::taken(&self.name));
            return Act::Stay;
        }
        Act::New {
            name: self.name.clone(),
            start: self.start,
        }
    }

    /// The action bar: Make, Field and Cancel, with Ctrl+C's Quit at the
    /// right, since letters type into the name.
    pub fn bar(&self) -> Bar {
        Bar {
            buttons: vec![
                Button::new("Enter", "Make"),
                Button::new("↑↓", "Field"),
                Button::new("Esc", "Cancel"),
                Button {
                    side: Side::Right,
                    ..Button::new("Ctrl+C", "Quit")
                },
            ],
        }
    }

    /// The sheet's lines, each with its field where it shows one.
    fn shown(&self) -> Vec<(String, Option<Field>, Role)> {
        let cursor = if self.field == Field::Name { "_" } else { "" };
        let mark = |s: Start| if self.start == s { "●" } else { "○" };
        let saved = if self.saved {
            format!("{} {}", mark(Start::Saved), words::FROM_SAVED)
        } else {
            format!("○ {} · {}", words::FROM_SAVED, words::NOTHING_SAVED)
        };
        let mut out = vec![
            (
                format!("Name    {}{cursor}", self.name),
                Some(Field::Name),
                Role::Row,
            ),
            (String::new(), None, Role::Row),
            (
                format!("Start   {saved}"),
                Some(Field::Start(Start::Saved)),
                if self.saved { Role::Row } else { Role::Dim },
            ),
            (
                format!("        {} {}", mark(Start::Defaults), words::FROM_DEFAULTS),
                Some(Field::Start(Start::Defaults)),
                Role::Row,
            ),
            (String::new(), None, Role::Row),
            (self.keeps.clone(), None, Role::Dim),
        ];
        if let Some(note) = &self.note {
            out.push((note.clone(), None, Role::Warning));
        }
        out
    }

    /// How many lines the sheet holds.
    pub fn lines(&self) -> usize {
        self.shown().len()
    }

    /// The sheet's box in `viewer`.
    pub fn area(&self, viewer: Rect) -> Rect {
        super::top_box(
            viewer,
            viewer.width.saturating_sub(4).min(WIDTH),
            self.lines(),
        )
    }

    /// Draws the sheet's lines into `area`, the highlighted field a bar.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) {
        let width = usize::from(area.width);
        for (y, (text, field, role)) in self.shown().iter().enumerate() {
            let y = u16::try_from(y).unwrap_or(u16::MAX);
            let lit = *field == Some(self.field);
            let role = if lit {
                bar_row(buf, area, y, Role::RowHighlight, color);
                Role::RowHighlight
            } else {
                *role
            };
            put(buf, area, 0, y, &super::cut(text, width), role, color);
        }
    }
}

/// The widest the sheet grows.
const WIDTH: u16 = 96;

/// `wanted` made into a draft's name that none of `taken` has: the name
/// itself, or the name with `-2`, `-3` and on after it.
fn free(wanted: &str, taken: &[String]) -> String {
    let base = store::name_from(wanted);
    let free = |n: &String| !taken.contains(n);
    std::iter::once(base.clone())
        .chain((2..).map(|i| format!("{base}-{i}")))
        .find(free)
        .unwrap_or(base)
}

/// The sheet's words.
mod words {
    pub const FROM_SAVED: &str = "from what is saved, /etc/vtium/vtium.conf";
    pub const NOTHING_SAVED: &str = "nothing is saved yet";
    pub const FROM_DEFAULTS: &str =
        "from vtium's defaults: every setting at its default and one look, named default";
    pub const BAD_NAME: &str = "A name has 1 to 64 letters, digits, '.', '_', '+' or '-', and cannot start with '.' or '-'.";

    pub fn taken(name: &str) -> String {
        format!("{name} is taken: another configuration has that name.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_offered_is_free_of_the_names_kept() {
        let taken = vec!["north".to_owned(), "north-2".to_owned()];
        assert_eq!(free("north", &taken), "north-3");
        assert_eq!(free("south", &taken), "south");
    }

    #[test]
    fn with_nothing_installed_the_defaults_are_chosen_and_the_highlight_skips_saved() {
        let mut new = New::open("north", "north", 0, Vec::new(), false);
        assert_eq!(new.start(), Start::Defaults);
        new.key(&Key::plain(Code::Down));
        assert_eq!(new.field, Field::Start(Start::Defaults));
        new.key(&Key::plain(Code::Up));
        assert_eq!(new.field, Field::Name);
    }
}
