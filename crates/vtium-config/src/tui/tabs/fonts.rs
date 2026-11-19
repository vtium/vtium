//! The Fonts tab: the list of frits (font set files) and built-in fonts,
//! and the form that makes a new frit from fonts, which opens over the
//! list. The list's keys highlight a font, set it as the console's font,
//! try it live, show its specimen or the picture of the console, and mark
//! a frit for removal. The form's fields take the new frit's name, its
//! sources, its cell and its size.
//!
//! References:
//! - ADR-1201 D3, the Fonts bullet: the list and its keys.
//! - ADR-1201 D9: `new`'s guided flow, a frit made from fonts and
//!   installed like any change.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::color::Style;
use super::super::input::Key;
use super::super::layout::PICTURE_ROW;
use super::super::listbox::{Body, ListBox, columns, step, tail};
use super::super::model::{Font, FontState, Shared, Size, View};
use super::super::shot::{Preview, Source};
use super::super::words;
use super::{Change, Command, Request, Room, Tab};
use crate::specimen::Sources;

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

/// The new-frit form, which opens over the list of fonts: the new frit
/// being made and the first row of the form that its window shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Form {
    frit: NewFrit,
    offset: usize,
}

/// Where the Fonts tab stands: the highlighted font, whether it shows the
/// detailed specimen or the picture of the console, and the new-frit form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fonts {
    /// The highlighted font, counted from 0 in the list of fonts, and the
    /// window of the list on screen.
    list: ListBox,
    /// The tab shows the detailed specimen of the highlighted font, as it
    /// does at start, in place of the picture of the console, which `s`
    /// switches to.
    detail: bool,
    /// The new-frit form.
    form: Form,
}

impl Fonts {
    /// The tab at its start, with the font at `at` highlighted.
    pub fn new(at: usize) -> Fonts {
        Fonts {
            list: ListBox::at(at),
            detail: true,
            form: Form::default(),
        }
    }

    /// The highlighted font, counted from 0 in the list of fonts.
    pub fn at(&self) -> usize {
        self.list.at
    }

    /// The first row the window of the list of fonts shows.
    pub fn offset(&self) -> usize {
        self.list.offset
    }

    /// Keeps the list of fonts in view from its row `row`, which the
    /// layout gives after every event.
    pub fn show_from(&mut self, row: usize) {
        self.list.offset = row;
    }

    /// Whether the tab shows the detailed specimen of the highlighted font
    /// in place of the picture of the console.
    pub fn detail(&self) -> bool {
        self.detail
    }

    /// The new frit being made.
    pub fn form(&self) -> &NewFrit {
        &self.form.frit
    }

    /// The new-frit form, which answers for the form's level as a tab
    /// does.
    pub fn new_frit(&self) -> &Form {
        &self.form
    }

    /// The new-frit form to change.
    pub fn new_frit_mut(&mut self) -> &mut Form {
        &mut self.form
    }

    /// The node of the interface tree the list is at.
    pub fn node(&self) -> &'static str {
        "fonts"
    }

    /// Highlights the font named `name`, where the list has it, and
    /// returns the change that shows the highlighted font in the previews.
    pub fn highlight(&mut self, shared: &Shared, name: &str) -> Command {
        if let Some(at) = shared.inputs.fonts.iter().position(|f| f.name == name) {
            self.list.at = at;
        }
        self.previewed()
    }

    /// Keeps the highlight on the list once fonts have left it, and
    /// returns the change that shows the highlighted font in the previews.
    pub fn fit(&mut self, shared: &Shared) -> Command {
        self.list.at = self
            .list
            .at
            .min(shared.inputs.fonts.len().saturating_sub(1));
        self.previewed()
    }

    /// The change that shows the highlighted font in the previews.
    fn previewed(&self) -> Command {
        Command::Change(Change::Preview(self.list.at))
    }

    /// The highlighted font.
    fn font<'s>(&self, shared: &'s Shared) -> Option<&'s Font> {
        shared.inputs.fonts.get(self.list.at)
    }

    /// The list's body on a screen of `size`: each font with what the
    /// console reads of it, whether the save removes it and whether it is
    /// the console's font, saved or to save; and under the list, the line
    /// that describes the preview of the highlighted font.
    pub fn body(&self, shared: &Shared, size: Size) -> Body {
        let width = shared
            .inputs
            .fonts
            .iter()
            .map(|f| f.name.chars().count())
            .max()
            .unwrap_or(0)
            .clamp(8, 24);
        let configured = shared.configured("font-name");
        let items: Vec<(String, String)> = shared
            .inputs
            .fonts
            .iter()
            .map(|f| {
                let mut note = match (&f.state, shared.sources.get(&f.name)) {
                    (FontState::Reads, Some(Sources::LeftOut(left))) if !left.is_empty() => {
                        format!("{}, {}", words::READS, words::sources_missing(left.len()))
                    }
                    (FontState::Reads, Some(Sources::DrawsNothing(_))) => {
                        words::DRAWS_NOTHING.to_owned()
                    }
                    (FontState::Reads, Some(Sources::Unknown(_))) => {
                        format!("{}, {}", words::READS, words::SOURCES_UNKNOWN)
                    }
                    (FontState::Reads, _) => words::READS.to_owned(),
                    (FontState::BuiltIn, _) => words::BUILT_IN.to_owned(),
                    (FontState::Refused(why), _) => words::does_not_read(why),
                };
                if shared.refused.contains(&f.name) {
                    note = words::KEPT.to_owned();
                }
                if shared.pending.removals.contains(&f.name) {
                    note = format!("{note}, {}", words::TO_REMOVE);
                }
                if shared.changed("font-name")
                    && shared.drafted("font-name").as_deref() == Some(&f.name)
                {
                    note = format!("{note}, {}", words::TO_SAVE);
                } else if configured.as_deref() == Some(&f.name) {
                    note = format!("{note}, {}", words::SAVED);
                }
                (f.name.clone(), note)
            })
            .collect();
        let (list, keep) = columns(&items, self.list.at, width, size.cols);
        Body {
            list,
            keep,
            tail: Tab::preview(self, shared, size)
                .preview_line()
                .map(|line| tail(&line, size.cols))
                .unwrap_or_default(),
        }
    }

    /// A key the navigation module hands to the list. `r` marks the
    /// highlighted frit for removal, `n` opens the new-frit form, `s`
    /// switches between the specimen and the picture of the console, `t`
    /// tries the font live, and Enter sets it as the console's font. In a
    /// vtium console the preview is text and needs no switch, so there `s`
    /// saves the font the console shows, as Save does in the other tabs.
    pub fn update(&mut self, shared: &Shared, key: Key) -> Vec<Command> {
        match key {
            Key::Char('r') => return self.remove(shared),
            Key::Char('n') => return vec![Command::Request(Request::Go(View::New))],
            Key::Char('s') if shared.live == super::super::model::Live::On => {
                return vec![Command::Change(Change::SaveTried)];
            }
            Key::Char('s') => {
                self.detail = !self.detail;
                return Vec::new();
            }
            Key::Char('t') => {
                return self
                    .font(shared)
                    .filter(|f| !matches!(f.state, FontState::Refused(_)))
                    .map(|f| vec![Command::Change(Change::Try(f.name.clone()))])
                    .unwrap_or_default();
            }
            _ => {}
        }
        let before = self.list.at;
        self.list.at = step(self.list.at, shared.inputs.fonts.len(), key);
        let mut out = Vec::new();
        if self.list.at != before {
            out.push(self.previewed());
        }
        match self.font(shared) {
            Some(font)
                if key == Key::Enter
                    && !matches!(font.state, FontState::Refused(_))
                    && !shared.refused.contains(&font.name) =>
            {
                out.push(Command::Change(Change::Top {
                    key: "font-name",
                    value: Some(font.name.clone()),
                }));
                out
            }
            _ => out,
        }
    }

    /// `r`: the highlighted frit is removed when the user saves, or, when
    /// a save already removes it, no longer is. The tab refuses what
    /// `remove-frit` refuses, by vtium.conf, its top level and its looks,
    /// both as it is and as the save leaves it, and a frit the save
    /// replaces with a new one. The save view lists the frit's file among
    /// what the save changes, and `apply` removes it, as it removes the
    /// frit of `remove-frit`'s draft.
    fn remove(&self, shared: &Shared) -> Vec<Command> {
        let Some(name) = self.font(shared).map(|f| f.name.clone()) else {
            return Vec::new();
        };
        let say = |line: String| Command::Change(Change::Say(line));
        let mut pending = shared.pending.clone();
        if let Some(at) = pending.removals.iter().position(|n| *n == name) {
            pending.removals.remove(at);
            return vec![
                Command::Change(Change::Pending(pending)),
                say(words::remove_dropped(&name)),
            ];
        }
        let refused = crate::commands::removal_refusal(&name, shared.conf_text())
            .or_else(|| {
                pending
                    .frits
                    .iter()
                    .any(|(n, _)| *n == name)
                    .then(|| words::remove_replaced(&name))
            })
            .or_else(|| {
                (!shared.inputs.installed.contains_key(&name)).then(|| words::remove_unread(&name))
            });
        if let Some(why) = refused {
            return vec![say(words::remove_refused(&name, why))];
        }
        pending.removals.push(name.clone());
        // the install as it would be with the removal: refused when the
        // draft names the frit
        match shared.frit_plan_of(&pending) {
            Ok(_) => vec![
                Command::Change(Change::Pending(pending)),
                say(words::remove_pending(&name)),
            ],
            Err(why) => vec![say(words::remove_refused(&name, why))],
        }
    }
}

impl Form {
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
            super::type_into(text, key);
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
    /// back to the list, and the form keeps what its fields hold. The form
    /// goes back to its first field, so `n` opens it there again. On a
    /// field, Enter moves to the next and every other key does nothing. On
    /// the last row, Up and Down move back onto the fields, and Enter adds
    /// the frit to the save, once the frit reader takes it, and goes back
    /// to the list.
    pub fn update(&mut self, shared: &Shared, key: Key) -> Vec<Command> {
        if key == Key::Escape {
            self.frit.at = 0;
            self.offset = 0;
            return vec![Command::Request(Request::Go(View::Fonts))];
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
                    Command::Request(Request::Go(View::Fonts)),
                ]
            }
            Err(why) => vec![Command::Change(Change::Say(words::new_refused(why)))],
        }
    }
}

impl Tab for Fonts {
    fn name(&self) -> &'static str {
        words::FONTS
    }

    fn node(&self) -> &'static str {
        Fonts::node(self)
    }

    fn body(&self, shared: &Shared, size: Size) -> Body {
        Fonts::body(self, shared, size)
    }

    fn offset(&self) -> usize {
        Fonts::offset(self)
    }

    fn show_from(&mut self, row: usize) {
        Fonts::show_from(self, row);
    }

    /// Below the list of fonts and the sentences under it, at the left
    /// edge, since the fonts' notes reach across the screen.
    fn picture_at(&self, shared: &Shared, size: Size) -> (usize, usize) {
        let body = Fonts::body(self, shared, size);
        (
            PICTURE_ROW
                .saturating_add(body.list.len())
                .saturating_add(body.tail.len())
                .saturating_add(1),
            2,
        )
    }

    /// The highlighted font at the draft's size: its detailed specimen,
    /// or the picture of the TUI's screen in it.
    fn preview<'a>(&self, shared: &'a Shared, size: Size) -> Preview<'a> {
        Preview::of(
            shared,
            size,
            Source::Font {
                font: self.font(shared),
                detail: self.detail,
            },
        )
    }

    /// The detailed specimen's lines for the locale, then its line of
    /// superscripts and subscripts, each run with its style.
    fn console_lines(&self, shared: &Shared) -> Vec<Vec<(&'static str, Style)>> {
        let mut out: Vec<Vec<(&'static str, Style)>> =
            crate::specimen::detail_lines(&shared.inputs.locale)
                .into_iter()
                .map(|line| vec![(line, Style::PLAIN)])
                .collect();
        out.push(
            crate::specimen::SCRIPT_LINE
                .iter()
                .map(|&(run, script)| {
                    (
                        run,
                        Style {
                            script,
                            ..Style::PLAIN
                        },
                    )
                })
                .collect(),
        );
        out
    }

    fn planned_keys(&self, shared: &Shared) -> &'static str {
        if shared.text_preview() {
            words::KEYS_FONTS_LIVE
        } else {
            words::KEYS_FONTS
        }
    }

    /// `s` is the tab's own key, so Save does not take it.
    fn takes_action(&self, action: char) -> bool {
        action != 's'
    }

    fn update(&mut self, shared: &Shared, _room: Room, key: Key) -> Vec<Command> {
        Fonts::update(self, shared, key)
    }
}

impl Tab for Form {
    /// The form shows under the Fonts tab's name.
    fn name(&self) -> &'static str {
        words::FONTS
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

    /// The form is open over the Fonts tab, so no action key acts in it.
    fn takes_action(&self, _action: char) -> bool {
        false
    }

    fn update(&mut self, shared: &Shared, _room: Room, key: Key) -> Vec<Command> {
        Form::update(self, shared, key)
    }
}
