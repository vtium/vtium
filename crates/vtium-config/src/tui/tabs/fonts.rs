//! The Fonts tab: the list of frits (font set files) and built-in fonts.
//! The list's keys highlight a font, set it as the console's font, try it
//! live, show its specimen or the picture of the console, mark a frit for
//! removal, and open the form for a new frit, which belongs to the Looks
//! tab. The Looks tab lists the same font sets in its font sets group, and
//! the Fonts tab keeps this list until the work that builds the Fonts tab
//! replaces it with the fonts found on the system.
//!
//! References:
//! - ADR-1201 D3, the Fonts bullet: the list and its keys.
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

/// Where the Fonts tab stands: the highlighted font, and whether it shows
/// the detailed specimen or the picture of the console.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fonts {
    /// The highlighted font, counted from 0 in the list of fonts, and the
    /// window of the list on screen.
    list: ListBox,
    /// The tab shows the detailed specimen of the highlighted font, as it
    /// does at start, in place of the picture of the console, which `s`
    /// switches to.
    detail: bool,
}

impl Fonts {
    /// The tab at its start, with the font at `at` highlighted.
    pub fn new(at: usize) -> Fonts {
        Fonts {
            list: ListBox::at(at),
            detail: true,
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

    /// The node of the interface tree the list is at.
    pub fn node(&self) -> &'static str {
        "fonts"
    }

    /// Highlights the font named `name`, where the list has it. The tab's
    /// preview follows its own highlight.
    pub fn highlight(&mut self, shared: &Shared, name: &str) {
        if let Some(at) = shared.inputs.fonts.iter().position(|f| f.name == name) {
            self.list.at = at;
        }
    }

    /// Keeps the highlight on the list once fonts have left it.
    pub fn fit(&mut self, shared: &Shared) {
        self.list.at = self
            .list
            .at
            .min(shared.inputs.fonts.len().saturating_sub(1));
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
            // the form belongs to the Looks tab, so the message line says
            // where Esc and adding the font set go
            Key::Char('n') => {
                return vec![
                    Command::Request(Request::Go(View::New)),
                    Command::Change(Change::Say(words::NEW_FRIT_IN_LOOKS.to_owned())),
                ];
            }
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
        self.list.at = step(self.list.at, shared.inputs.fonts.len(), key);
        let mut out = Vec::new();
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
