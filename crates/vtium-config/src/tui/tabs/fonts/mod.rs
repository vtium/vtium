//! The Fonts tab, the first tab of the bar. It lists the font files found
//! on this system, grouped by the kind of folder each was found in: the
//! system's folders, vtium's own, kbd's console fonts, the folders that
//! `font-paths` names, and the person's own folders. A filter line above
//! the list shows every font, or only those that pass one filter: bitmap
//! or outline, monospaced or proportional, a well-known group such as Nerd
//! Fonts or color emoji, or a script. Each font shows whether it is
//! active, whether it passes the trust rule, its family once the font
//! helper has described it, and a duplicate mark. A font in the person's
//! own folder is not usable until it is installed as a system font.
//!
//! Space activates or deactivates the highlighted font with the controls
//! the resource tabs share, and asks first when a look uses it. Enter on
//! the filter line opens the list of filters, and Enter on a font opens its
//! detail, which changes nothing. Enter in the detail of a font of the
//! person's own asks whether to install it as a system font, and shows the
//! exact command first. Under the list, the tab previews the highlighted
//! font as its sample characters, or with `p` as the picture of the
//! console drawn in it.
//!
//! References:
//! - ADR-1205 D1: the Fonts tab, and the controls it shares with the
//!   Effects tab.
//! - ADR-1205 D3: the folders, the filters, and a user's font listed as
//!   not usable.
//! - ADR-1205 D5: installing a font of the person's as a system font.
//! - ADR-1205 D8: a font's detail, and the warning before a font a look
//!   uses is deactivated.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.
//! - ADR-1403 D10: the tab's filter line, its list, its detail and its
//!   keys.

mod detail;

use std::path::PathBuf;

use super::super::canvas::{Row, span};
use super::super::color::Style;
use super::super::desk::Op;
use super::super::fonts_found::{Filter, FontFile};
use super::super::input::Key;
use super::super::layout::PICTURE_ROW;
use super::super::listbox::{Act, Body, Item, ListBox, Marks, plain, prose, rows, step, tail};
use super::super::model::{Effect, Shared, Size};
use super::super::resources;
use super::super::shot::{Preview, Source};
use super::super::words;
use super::{Change, Command, Room, Tab};
use crate::discover::Described;
use crate::onscreen::{path as shown, text};

pub use detail::uses;

/// What is open over the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    /// The list of filters, which the filter line opens.
    Filter(ListBox),
    /// A font's detail, by its file, with its first row in sight.
    Detail { path: PathBuf, top: usize },
}

/// The Fonts tab's state: the highlighted row, where row 0 is the filter
/// line and each font follows; the first font the window of the list
/// shows; the filter; what is open over the list; and whether the preview
/// shows the picture of the console in place of the sample characters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fonts {
    list: ListBox,
    top: usize,
    filter: Filter,
    open: Option<Open>,
    picture: bool,
}

/// How many fonts the window of the list shows on a screen of `size`: at
/// most a third of the rows, so the preview keeps room below the list
/// however many fonts the system holds.
fn window(size: Size) -> usize {
    size.rows
        .saturating_sub(20)
        .clamp(3, (size.rows / 3).max(3))
}

impl Fonts {
    /// The highlighted row, 0 on the filter line.
    pub fn at(&self) -> usize {
        self.list.at
    }

    /// The first font the window of the list shows, counted from 0.
    pub fn top(&self) -> usize {
        self.top
    }

    /// Whether the preview shows the picture of the console in place of
    /// the sample characters, which `p` switches.
    pub fn picture(&self) -> bool {
        self.picture
    }

    /// The filter the list shows.
    pub fn filter(&self) -> Filter {
        self.filter
    }

    /// The fonts the list shows, by their place among the files found, in
    /// the list's order.
    pub fn items(&self, shared: &Shared) -> Vec<usize> {
        let found = &shared.inputs.font_files;
        found
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| self.filter.keeps(found.described(f.path())))
            .map(|(i, _)| i)
            .collect()
    }

    /// The highlighted font, `None` on the filter line.
    pub fn highlighted<'s>(&self, shared: &'s Shared) -> Option<&'s FontFile> {
        let at = self.list.at.checked_sub(1)?;
        let i = *self.items(shared).get(at)?;
        shared.inputs.font_files.files.get(i)
    }

    /// The font whose detail is open, if one is.
    fn opened<'s>(&self, shared: &'s Shared) -> Option<&'s FontFile> {
        match &self.open {
            Some(Open::Detail { path, .. }) => shared.inputs.font_files.file(path),
            _ => None,
        }
    }

    /// Keeps the highlight on the list once fonts have left it, as after
    /// a scan that follows an install.
    pub fn fit(&mut self, shared: &Shared) {
        self.list.at = self.list.at.min(self.items(shared).len());
        self.top = self.top.min(self.list.at.saturating_sub(1));
    }

    /// Moves the window of the list so it shows the highlighted font, a
    /// window of `n` fonts.
    fn follow(&mut self, n: usize) {
        let Some(at) = self.list.at.checked_sub(1) else {
            self.top = 0;
            return;
        };
        if at < self.top {
            self.top = at;
        } else if at >= self.top.saturating_add(n) {
            self.top = at.saturating_add(1).saturating_sub(n);
        }
    }

    /// `file` as an item of the list: its file's name, then its family
    /// once the font helper has described it, and its marks.
    fn item(shared: &Shared, file: &FontFile) -> Item {
        let mut marks = Vec::new();
        match shared.inputs.font_files.described(file.path()) {
            None => marks.push(words::DESCRIBING.to_owned()),
            Some(Described::Font(d)) => {
                if let Some(face) = d.faces.first() {
                    marks.push(words::face_name(&text(&face.family), &text(&face.style)));
                }
            }
            Some(Described::NotAFont(_)) => marks.push(words::NOT_A_FONT.to_owned()),
            Some(Described::Unknown(_)) => marks.push(words::NOT_DESCRIBED.to_owned()),
        }
        marks.push(file.resource.row());
        if file.duplicate.is_some() {
            marks.push(words::DUPLICATE.to_owned());
        }
        Item::new(text(&file.resource.name()), marks.join(", "))
    }

    /// The tab's body while nothing is open: the filter line, the list in
    /// its groups, and the sentences under it.
    fn list_body(&self, shared: &Shared, size: Size) -> Body {
        let cols = size.cols;
        let found = &shared.inputs.font_files;
        let filter = format!(" {}", words::fonts_filter(&words::filter_name(self.filter)));
        let style = if self.list.at == 0 {
            Style::PLAIN.reversed()
        } else {
            Style::PLAIN
        };
        let mut list: Vec<Row> = vec![vec![span(filter, style)]];
        let shown = self.items(shared);
        // the window of the list, which starts each group it shows with
        // the group's heading, the first font's group included
        let n = window(size);
        let top = self.top.min(shown.len().saturating_sub(n));
        let mut place = None;
        let items: Vec<Item> = shown
            .iter()
            .skip(top)
            .take(n)
            .filter_map(|i| found.files.get(*i))
            .map(|f| {
                let mut item = Self::item(shared, f);
                if place != Some(f.place) {
                    item.heading = Some(words::place_heading(f.place).to_owned());
                    place = Some(f.place);
                }
                item
            })
            .collect();
        let at = self
            .list
            .at
            .checked_sub(1)
            .and_then(|at| at.checked_sub(top))
            .unwrap_or(usize::MAX);
        let (rows_shown, keep) = rows(&items, at, Marks::None, cols);
        list.extend(rows_shown);
        let mut sentence = Vec::new();
        if let Some(note) = &found.note {
            sentence.push(note.clone());
        }
        if found.files.is_empty() {
            sentence.push(words::FONTS_EMPTY.to_owned());
        } else if shown.is_empty() {
            sentence.push(words::FONTS_NONE.to_owned());
        }
        let waiting = found.waiting().len();
        if waiting > 0 {
            sentence.push(words::fonts_waiting(waiting));
        }
        sentence.push(words::FONTS_HINT.to_owned());
        let mut under = tail(&sentence.join(" "), cols);
        if let Some(line) = Tab::preview(self, shared, size).preview_line() {
            under.extend(tail(&line, cols).into_iter().skip(1));
        }
        Body {
            list,
            keep: if self.list.at == 0 {
                0
            } else {
                keep.saturating_add(1)
            },
            tail: under,
        }
    }

    /// A key while nothing is open over the list.
    fn list_key(&mut self, shared: &Shared, size: Size, key: Key) -> Vec<Command> {
        let say = |line: String| vec![Command::Change(Change::Say(line))];
        match key {
            Key::Enter => {
                self.open = Some(match self.highlighted(shared) {
                    Some(f) => Open::Detail {
                        path: f.path().to_path_buf(),
                        top: 0,
                    },
                    None => Open::Filter(ListBox::at(
                        Filter::choices()
                            .iter()
                            .position(|f| *f == self.filter)
                            .unwrap_or(0),
                    )),
                });
                Vec::new()
            }
            Key::Char(' ') => match self.highlighted(shared) {
                // the Fonts tab says how a font of the person's becomes
                // usable: installing it from its detail
                Some(f) if f.resource.own => say(words::own_font(&shown(f.path()))),
                Some(f) => resources::space(&f.resource, &uses(shared, f.path())),
                None => Vec::new(),
            },
            Key::Char('p') => {
                self.picture = !self.picture;
                Vec::new()
            }
            Key::Char('t') => say(words::TRY_A_FONT_SET.to_owned()),
            _ => {
                let n = self.items(shared).len().saturating_add(1);
                self.list.at = step(self.list.at, n, key);
                self.follow(window(size));
                Vec::new()
            }
        }
    }

    /// A key in a font's detail: Up and Down scroll it, Esc goes back, and
    /// Enter on a font of the person's own readies its install as a system
    /// font, which the question shows before anything runs.
    fn detail_key(&mut self, shared: &Shared, path: PathBuf, top: usize, key: Key) -> Vec<Command> {
        let Some(file) = shared.inputs.font_files.file(&path) else {
            self.open = None;
            return Vec::new();
        };
        // the title stays, so the lines under it scroll as far as the last
        let last = detail::lines(shared, file).len().saturating_sub(2);
        match key {
            Key::Escape => self.open = None,
            Key::Up => {
                self.open = Some(Open::Detail {
                    path,
                    top: top.saturating_sub(1),
                });
            }
            Key::Down => {
                self.open = Some(Open::Detail {
                    path,
                    top: top.saturating_add(1).min(last),
                });
            }
            Key::Enter if file.resource.own => {
                return vec![Command::Effect(Effect::Desk(Op::SystemFont(path)))];
            }
            Key::Enter => {
                return vec![Command::Change(Change::Say(
                    words::SYSTEM_FONT_NOT_YOURS.to_owned(),
                ))];
            }
            _ => {}
        }
        Vec::new()
    }
}

impl Tab for Fonts {
    fn name(&self) -> &'static str {
        words::FONTS
    }

    fn node(&self) -> &'static str {
        match self.open {
            None => "fonts",
            Some(Open::Filter(_)) => "fonts.filter",
            Some(Open::Detail { .. }) => "fonts.font",
        }
    }

    fn body(&self, shared: &Shared, size: Size) -> Body {
        match &self.open {
            None => self.list_body(shared, size),
            Some(Open::Filter(lb)) => {
                let items: Vec<Item> = Filter::choices()
                    .into_iter()
                    .map(|f| Item {
                        on: f == self.filter,
                        ..Item::new(words::filter_name(f), "")
                    })
                    .collect();
                let (list, keep) = rows(&items, lb.at, Marks::Choice, size.cols);
                Body {
                    list,
                    keep,
                    tail: tail(words::FONTS_FILTER_HINT, size.cols),
                }
            }
            Some(Open::Detail { top, .. }) => {
                let Some(file) = self.opened(shared) else {
                    return Body::default();
                };
                // the title stays at the top, so the detail always says
                // what it describes, and the lines under it scroll
                let lines = detail::lines(shared, file);
                let mut list: Vec<Row> = Vec::new();
                for line in lines.iter().take(1).chain(lines.iter().skip(1).skip(*top)) {
                    list.extend(prose(line, size.cols));
                }
                if list.is_empty() {
                    list.push(plain(""));
                }
                let hint = if file.resource.own {
                    words::FONTS_OWN_HINT
                } else {
                    words::FONTS_DETAIL_HINT
                };
                Body {
                    list,
                    keep: 0,
                    tail: tail(hint, size.cols),
                }
            }
        }
    }

    fn offset(&self) -> usize {
        match self.open {
            None => self.list.offset,
            _ => 0,
        }
    }

    fn show_from(&mut self, row: usize) {
        if self.open.is_none() {
            self.list.offset = row;
        }
    }

    /// Under the list and the sentences under it, from the left edge,
    /// since the fonts' marks reach across the screen.
    fn picture_at(&self, shared: &Shared, size: Size) -> (usize, usize) {
        let body = self.list_body(shared, size);
        (
            PICTURE_ROW
                .saturating_add(body.list.len())
                .saturating_add(body.tail.len())
                .saturating_add(1),
            2,
        )
    }

    /// The highlighted font at the draft's size, while the list shows:
    /// its sample characters, or the picture of the TUI's screen drawn in
    /// it. Nothing while the filters or a detail are open, or on the
    /// filter line.
    fn preview<'a>(&self, shared: &'a Shared, size: Size) -> Preview<'a> {
        let font = match (&self.open, self.highlighted(shared)) {
            (None, Some(f)) => &f.specimen,
            _ => return Preview::nothing(shared, size),
        };
        Preview::of(
            shared,
            size,
            Source::Font {
                font: Some(font),
                detail: !self.picture,
            },
        )
    }

    /// The action keys act while nothing is open over the list, and `o`
    /// does nothing, since it opens Restore in the Flow view alone.
    ///
    /// References:
    /// - ADR-1403 D7: the action keys do nothing while a layer is open, and
    ///   the `o.view` row.
    fn takes_action(&self, action: char) -> bool {
        self.open.is_none() && action != 'o'
    }

    fn update(&mut self, shared: &Shared, room: Room, key: Key) -> Vec<Command> {
        match self.open.clone() {
            Some(Open::Filter(mut lb)) => {
                let choices = Filter::choices();
                match lb.key(key, choices.len()) {
                    Act::Picked(i) => {
                        self.filter = choices.get(i).copied().unwrap_or_default();
                        self.list = ListBox::default();
                        self.top = 0;
                        self.open = None;
                    }
                    Act::Closed => self.open = None,
                    _ => self.open = Some(Open::Filter(lb)),
                }
                Vec::new()
            }
            Some(Open::Detail { path, top }) => self.detail_key(shared, path, top, key),
            None => self.list_key(shared, room.size, key),
        }
    }
}
