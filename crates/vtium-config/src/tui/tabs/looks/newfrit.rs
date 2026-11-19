//! The form that makes a new font set (a frit) from fonts. It opens over
//! the Looks tab's font sets, on `n`, and its fields take the new font
//! set's name, three fonts, its character cell and its size. The row after
//! the fields adds the font set to the install, once the frit reader takes
//! it, and goes back to the Looks tab.
//!
//! The form opens with a field for each role in the tree's plan. That form
//! needs the list of the active fonts that can fill each role, which comes
//! with the Fonts tab's list of fonts, so this form keeps its three typed
//! fields until then.
//!
//! References:
//! - ADR-1201 D9: `new`'s guided flow, a frit made from fonts and
//!   installed like any change.
//! - ADR-1205 D6: the roles the form's fields are to follow.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::super::input::Key;
use super::super::super::listbox::{Body, columns, step, tail};
use super::super::super::model::{Shared, Size, View};
use super::super::super::words;
use super::super::{Change, Command, Request, Room, Tab};

/// The new-frit form's fields, in order: its name, three sources, its
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

/// The new-frit form, which opens over the Looks tab's font sets: the new
/// frit being made and the first row of the form that its window shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Form {
    frit: NewFrit,
    offset: usize,
}

impl Form {
    /// The new frit being made.
    pub fn frit(&self) -> &NewFrit {
        &self.frit
    }

    /// The node of the interface tree the form is at: a field being
    /// typed, or the row that adds the frit.
    pub fn node(&self) -> &'static str {
        if self.typing() {
            "looks.new-frit.typing"
        } else {
            "looks.new-frit"
        }
    }

    /// Whether the form's highlighted row is a field, whose characters
    /// are typed into it.
    pub fn typing(&self) -> bool {
        self.frit.at < NEW_FIELDS
    }

    /// A character or Backspace typed into the form's highlighted field.
    pub fn type_key(&mut self, key: Key) {
        if let Some(text) = self.frit.fields.get_mut(self.frit.at) {
            super::super::type_into(text, key);
        }
    }

    /// The form's body on a screen of `size`: each field with its
    /// text, the highlighted one with the cursor after it, then the row
    /// that adds the frit, and under them what a frit is made of.
    pub fn body(&self, size: Size) -> Body {
        let mut items: Vec<(String, String)> = words::NEW_FIELD_NAMES
            .iter()
            .zip(&self.frit.fields)
            .enumerate()
            .map(|(i, (name, text))| {
                let cursor = if i == self.frit.at { "_" } else { "" };
                ((*name).to_owned(), format!("{text}{cursor}"))
            })
            .collect();
        items.push((words::NEW_ADD.to_owned(), String::new()));
        let (list, keep) = columns(&items, self.frit.at.min(NEW_FIELDS), 8, size.cols);
        Body {
            list,
            keep,
            tail: tail(words::NEW_HINT, size.cols),
        }
    }

    /// A key the navigation module hands to the new-frit form. Esc goes
    /// back to the Looks tab, and the form keeps what its fields hold. The
    /// form goes back to its first field, so `n` opens it there again. On
    /// a field, Enter moves to the next and every other key does nothing.
    /// On the last row, Up and Down move back onto the fields, and Enter
    /// adds the frit to the save, once the frit reader takes it, and goes
    /// back to the Looks tab.
    pub fn update(&mut self, shared: &Shared, key: Key) -> Vec<Command> {
        if key == Key::Escape {
            self.frit.at = 0;
            self.offset = 0;
            return vec![Command::Request(Request::Go(View::Looks))];
        }
        let rows = NEW_FIELDS.saturating_add(1);
        let at = self.frit.at;
        // while a field is typed, only Enter (keep, next field) and Esc
        // act, as while a Settings value is typed
        if at < NEW_FIELDS && key != Key::Enter {
            return Vec::new();
        }
        self.frit.at = match key {
            Key::Enter if at < NEW_FIELDS => at.saturating_add(1),
            _ => step(at, rows, key),
        };
        if key != Key::Enter || at != NEW_FIELDS {
            return Vec::new();
        }
        let name = self.frit.name();
        let text = self.frit.text();
        let path = shared.inputs.frits.join(format!("{name}.vtfrit"));
        let read =
            if !crate::frit::valid_name(&name) || crate::frit::RESERVED.contains(&name.as_str()) {
                Err(words::NEW_BAD_NAME.to_owned())
            } else if shared
                .inputs
                .installed
                .get(&name)
                .is_some_and(|b| std::str::from_utf8(b).is_err())
            {
                Err(words::new_replaces_binary(&name))
            } else {
                crate::frit::Frit::parse(text.as_bytes(), &path, &name)
                    .map(|_| ())
                    .map_err(|e| e.why)
            };
        match read {
            Ok(()) => {
                let mut pending = shared.pending.clone();
                pending.frits.retain(|(n, _)| *n != name);
                pending.removals.retain(|n| *n != name);
                pending.frits.push((name.clone(), text));
                self.frit = NewFrit::default();
                vec![
                    Command::Change(Change::Pending(pending)),
                    Command::Change(Change::Say(words::new_added(&name))),
                    Command::Request(Request::Go(View::Looks)),
                ]
            }
            Err(why) => vec![Command::Change(Change::Say(words::new_refused(why)))],
        }
    }
}

impl Tab for Form {
    /// The form shows under the Looks tab's name.
    fn name(&self) -> &'static str {
        words::LOOKS
    }

    fn node(&self) -> &'static str {
        Form::node(self)
    }

    fn body(&self, _shared: &Shared, size: Size) -> Body {
        Form::body(self, size)
    }

    fn offset(&self) -> usize {
        self.offset
    }

    fn show_from(&mut self, row: usize) {
        self.offset = row;
    }

    fn planned_keys(&self, _shared: &Shared) -> &'static str {
        words::KEYS_NEW
    }

    fn typing(&self) -> bool {
        Form::typing(self)
    }

    fn type_key(&mut self, key: Key) {
        Form::type_key(self, key);
    }

    /// The form is open over the Looks tab, so no action key acts in it.
    fn takes_action(&self, _action: char) -> bool {
        false
    }

    fn update(&mut self, shared: &Shared, _room: Room, key: Key) -> Vec<Command> {
        Form::update(self, shared, key)
    }
}
