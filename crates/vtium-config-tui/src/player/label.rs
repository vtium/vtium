//! The label finder: where a label is on the screen, and the keys a person
//! presses to reach it. `Click "LABEL"` and `Click Mouse "LABEL"` both
//! find their label here, and the use-case runner calls the same code.
//!
//! The finder looks in this order: the items of an open
//! menu, the buttons of the action bar, the menus and tabs of the top row,
//! and then the lines and boxes of the viewer. A menu, a tab, a button or
//! a menu item matches when its label equals LABEL. An item's trailing
//! `…` is no part of its name, so `Save` names `Save…`. A line or a box
//! of the viewer matches when it begins with LABEL as whole words, after
//! the marks before its first word, such as `▸` or `●`. A word ends at a
//! space or a stop such as `,`, `:` or `…`, so `cullet-mono` does not
//! begin `cullet-mono-bold`. The first place
//! that holds a match decides, and two matches there are an error that
//! names both.
//!
//! The finder reads the places from the App's own layout, the one the
//! view draws and update reads clicks from, and reads the viewer's lines
//! from the screen the view draws for the App's state. It never reads a
//! terminal.
//!
//! References:
//! - ADR-1208 D3, "A click acts through the keys a person would press":
//!   the order of the places, equal labels and lines that begin with the
//!   label, two matches, and the keys that reach each place.
//! - ADR-1206 D4: the one layout function and its places.

use ratatui_core::backend::TestBackend;
use ratatui_core::layout::Rect;
use ratatui_core::style::Modifier;
use ratatui_core::terminal::Terminal;
use vtium_config_macro::{Character, Key as MacroKey, KeyPress, Mods};
use vtium_config_widgets::menu::Pick;

use super::encode::press_of_bar;
use crate::app::App;
use crate::view;

/// One cell of the screen as the view draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    symbol: String,
    reversed: bool,
}

/// The screen as the view draws it for the App's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    cells: Vec<Vec<Cell>>,
    viewer: Rect,
}

impl Screen {
    /// The screen `app` draws now.
    pub fn of(app: &App) -> Screen {
        let size = app.size();
        let viewer = if size.too_small() {
            Rect::default()
        } else {
            view::layout(app).frame.regions().viewer
        };
        // a test backend cannot fail
        let Ok(mut terminal) = Terminal::new(TestBackend::new(size.cols, size.rows));
        let _ = terminal.draw(|f| view::draw(app, f));
        let buf = terminal.backend().buffer();
        let a = buf.area;
        let cells = (a.y..a.bottom())
            .map(|y| {
                (a.x..a.right())
                    .map(|x| {
                        buf.cell((x, y)).map_or(
                            Cell {
                                symbol: " ".to_owned(),
                                reversed: false,
                            },
                            |c| Cell {
                                symbol: c.symbol().to_owned(),
                                reversed: c.modifier.contains(Modifier::REVERSED),
                            },
                        )
                    })
                    .collect()
            })
            .collect();
        Screen { cells, viewer }
    }

    /// Each row as text, one symbol for each cell, so a wide character's
    /// second cell adds nothing.
    pub fn rows(&self) -> Vec<String> {
        self.cells
            .iter()
            .map(|row| row.iter().map(|c| c.symbol.as_str()).collect())
            .collect()
    }

    /// The whole screen read as words with one space between each, so a
    /// text that wraps across rows still reads as one.
    pub fn words(&self) -> String {
        self.rows()
            .iter()
            .flat_map(|r| r.split_whitespace())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Whether the screen shows `text`, read with one space between each
    /// word.
    pub fn shows(&self, text: &str) -> bool {
        self.words().contains(text)
    }

    /// The text of the row `y` from the column `x`, and the column each
    /// character starts at.
    fn row_from(&self, y: u16, x: u16, width: u16) -> Vec<(u16, &str)> {
        let Some(row) = self.cells.get(usize::from(y)) else {
            return Vec::new();
        };
        let end = usize::from(x.saturating_add(width));
        row.iter()
            .enumerate()
            .skip(usize::from(x))
            .take(end.saturating_sub(usize::from(x)))
            .filter(|(_, c)| !c.symbol.is_empty())
            .map(|(i, c)| (u16::try_from(i).unwrap_or(u16::MAX), c.symbol.as_str()))
            .collect()
    }

    /// The viewer's highlighted line: the first row of the viewer with a
    /// cell in reverse video.
    pub fn highlight(&self) -> Option<u16> {
        let v = self.viewer;
        (v.y..v.bottom()).find(|&y| {
            self.cells.get(usize::from(y)).is_some_and(|row| {
                row.iter()
                    .skip(usize::from(v.x))
                    .take(usize::from(v.width))
                    .any(|c| c.reversed && !c.symbol.trim().is_empty())
            })
        })
    }

    /// The viewer's row `y` as text.
    pub fn viewer_line(&self, y: u16) -> String {
        let v = self.viewer;
        self.row_from(y, v.x, v.width)
            .into_iter()
            .map(|(_, s)| s)
            .collect()
    }

    /// The highlighted box of the viewer, in heavy lines, as the Flow tab
    /// draws the box under its cursor: its left column, its top row and
    /// its right column.
    pub fn heavy_box(&self) -> Option<(u16, u16, u16)> {
        self.boxes()
            .into_iter()
            .find(|b| b.heavy)
            .map(|b| (b.area.x, b.area.y, b.area.right().saturating_sub(1)))
    }

    /// The boxes drawn in the viewer: a corner, the top border to the
    /// other corner, and the rows down to the bottom corner.
    fn boxes(&self) -> Vec<Drawn> {
        let v = self.viewer;
        let mut out = Vec::new();
        for y in v.y..v.bottom() {
            for (x, s) in self.row_from(y, v.x, v.width) {
                let heavy = match s {
                    "┌" | "╭" => false,
                    "┏" => true,
                    _ => continue,
                };
                let right = self
                    .row_from(y, x.saturating_add(1), v.right().saturating_sub(x))
                    .into_iter()
                    .find(|(_, s)| matches!(*s, "┐" | "╮" | "┓"))
                    .map(|(rx, _)| rx);
                let bottom = (y.saturating_add(1)..v.bottom()).find(|&by| {
                    self.row_from(by, x, 1)
                        .first()
                        .is_some_and(|(_, s)| matches!(*s, "└" | "╰" | "┗"))
                });
                if let (Some(right), Some(bottom)) = (right, bottom) {
                    out.push(Drawn {
                        area: Rect::new(
                            x,
                            y,
                            right.saturating_sub(x).saturating_add(1),
                            bottom.saturating_sub(y).saturating_add(1),
                        ),
                        heavy,
                    });
                }
            }
        }
        out
    }

    /// The first inner line of a box, and the column its text starts at.
    fn box_label(&self, b: &Drawn) -> (u16, String) {
        let y = b.area.y.saturating_add(1);
        let x = b.area.x.saturating_add(1);
        let line: Vec<(u16, &str)> = self.row_from(y, x, b.area.width.saturating_sub(2));
        let start = line
            .iter()
            .find(|(_, s)| !s.trim().is_empty())
            .map_or(x, |(i, _)| *i);
        (start, line.into_iter().map(|(_, s)| s).collect())
    }
}

/// A box drawn in the viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drawn {
    area: Rect,
    heavy: bool,
}

/// Where a label was found, which says how the player reaches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// An item of the open menu, or of its submenu.
    MenuItem {
        pick: Pick,
        letter: Option<char>,
        greyed: Option<String>,
    },
    /// A button of the action bar, by its key in the bar's form.
    Button { key: String },
    /// The File or the Actions menu's name.
    Menu { letter: char },
    /// A tab's name.
    Tab { letter: char },
    /// A line of the viewer, by its row.
    Line { row: u16 },
    /// A box of the viewer, by its top row.
    Box { row: u16 },
}

/// A label found on the screen: where it is, and its first cell, where
/// `Click Mouse` clicks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub place: Place,
    pub cell: (u16, u16),
}

impl Found {
    /// The keys a person presses to reach the label at once: a menu
    /// item's letter, a button's key, a menu's or a tab's Alt letter.
    /// `None` for a line or a box, or a letterless menu item, which the
    /// player reaches by moving the highlight, and `Err` for a key a macro
    /// has no name for.
    pub fn keys(&self) -> Option<Result<Vec<KeyPress>, String>> {
        let alt = |c: char| {
            Character::new(c).map(|c| KeyPress {
                mods: Mods {
                    alt: true,
                    ..Mods::NONE
                },
                key: MacroKey::Char(c),
            })
        };
        let one = |k: Option<KeyPress>, what: &str| {
            Some(k.map(|k| vec![k]).ok_or_else(|| {
                format!("its key, {what}, has no name in a macro, so no step can press it.")
            }))
        };
        match &self.place {
            Place::MenuItem {
                letter: Some(c), ..
            } => one(
                Character::new(*c).map(|c| KeyPress::plain(MacroKey::Char(c))),
                &c.to_string(),
            ),
            Place::Button { key } => one(press_of_bar(key), key),
            Place::Menu { letter } | Place::Tab { letter } => one(alt(*letter), "an Alt letter"),
            Place::MenuItem { letter: None, .. } | Place::Line { .. } | Place::Box { .. } => None,
        }
    }
}

/// Whether `text` begins with `label` as whole words, after its leading
/// spaces and the marks before its first word, with the spaces in `text`
/// read as one.
pub fn begins(text: &str, label: &str) -> bool {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut rest = flat.as_str();
    loop {
        if let Some(after) = rest.strip_prefix(label)
            && after
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || ",.:;…·".contains(c))
        {
            return true;
        }
        // a mark before the first word, such as ▸, ● or │, then a space
        match rest.split_once(' ') {
            Some((mark, after)) if !mark.chars().any(char::is_alphanumeric) => rest = after,
            _ => return false,
        }
    }
}

/// The column where the text of a line starts after its marks.
fn text_start(line: &[(u16, &str)]) -> Option<u16> {
    let mut seen_mark = false;
    for (x, s) in line {
        if s.trim().is_empty() {
            continue;
        }
        if s.chars().any(char::is_alphanumeric) || seen_mark {
            return Some(*x);
        }
        seen_mark = true;
    }
    None
}

/// A label's name as a menu or a button writes it, without a trailing
/// `…`, which says that more follows and is no part of the name.
fn named(text: &str) -> &str {
    text.trim_end_matches('…')
        .trim_end_matches("...")
        .trim_end()
}

/// Finds `label` on `app`'s screen: in an open menu, on the action bar,
/// on the top row, then in the viewer.
pub fn find(app: &App, screen: &Screen, label: &str) -> Result<Found, String> {
    let shown = view::layout(app);
    let places = shown.frame.places();
    // the items of an open menu, its submenu first
    if let Some(menu) = &shown.menu {
        let mut hits = Vec::new();
        for (rect, pick) in &menu.layout.places().items {
            let line = match pick {
                Pick::Sub(i) => menu.sub.get(*i),
                Pick::Item(i) => menu.lines.get(*i),
            };
            if let Some(c) = line.and_then(crate::menus::Line::choice)
                && named(&c.item.label) == label
            {
                hits.push(Found {
                    place: Place::MenuItem {
                        pick: *pick,
                        letter: c.item.letter,
                        greyed: c.item.greyed.clone(),
                    },
                    cell: (rect.x, rect.y),
                });
            }
        }
        hits.sort_by_key(|f| {
            !matches!(
                f.place,
                Place::MenuItem {
                    pick: Pick::Sub(_),
                    ..
                }
            )
        });
        if let Some(found) = one(hits, label, "the open menu")? {
            return Ok(found);
        }
    }
    // the buttons of the action bar, those More holds included
    let mut hits = Vec::new();
    for (i, b) in shown.bar.buttons.iter().enumerate() {
        if named(&b.label) == label {
            let cell = places
                .buttons
                .iter()
                .find(|(j, _)| *j == i)
                .or_else(|| places.buttons.first())
                .map_or((0, 0), |(_, r)| (r.x, r.y));
            hits.push(Found {
                place: Place::Button { key: b.key.clone() },
                cell,
            });
        }
    }
    if label == "More"
        && let Some(r) = places.more
    {
        hits.push(Found {
            place: Place::Button {
                key: ".".to_owned(),
            },
            cell: (r.x, r.y),
        });
    }
    if let Some(found) = one(hits, label, "the action bar")? {
        return Ok(found);
    }
    // the menus and tabs of the top row
    let mut hits = Vec::new();
    for (name, letter, rect) in [
        ("File", 'f', places.menus[0]),
        ("Actions", 'a', places.menus[1]),
    ] {
        if name == label {
            hits.push(Found {
                place: Place::Menu { letter },
                cell: (rect.x, rect.y),
            });
        }
    }
    for (tab, rect) in app.tabs().iter().zip(&places.tabs) {
        if tab.name() == label {
            hits.push(Found {
                place: Place::Tab {
                    letter: tab.letter(),
                },
                cell: (rect.x, rect.y),
            });
        }
    }
    if let Some(found) = one(hits, label, "the top row")? {
        return Ok(found);
    }
    // the lines and boxes of the viewer
    let hits = viewer_hits(screen, label);
    if let Some(found) = one(hits, label, "the viewer")? {
        return Ok(found);
    }
    Err(format!("the screen shows no {label:?} to click."))
}

/// The lines and boxes of the viewer that begin with `label`. A row inside
/// a box counts as the box's, never as a line of its own.
pub fn viewer_hits(screen: &Screen, label: &str) -> Vec<Found> {
    let v = screen.viewer;
    let boxes = screen.boxes();
    let mut hits = Vec::new();
    for b in &boxes {
        let (x, text) = screen.box_label(b);
        if begins(&text, label) {
            hits.push(Found {
                place: Place::Box { row: b.area.y },
                cell: (x, b.area.y.saturating_add(1)),
            });
        }
    }
    let inside = |x: u16, y: u16| {
        boxes
            .iter()
            .any(|b| b.area.contains(ratatui_core::layout::Position::new(x, y)))
    };
    for y in v.y..v.bottom() {
        let line = screen.row_from(y, v.x, v.width);
        let Some(x) = text_start(&line) else {
            continue;
        };
        if inside(x, y) {
            continue;
        }
        let text: String = line.iter().map(|(_, s)| *s).collect();
        if begins(&text, label) {
            hits.push(Found {
                place: Place::Line { row: y },
                cell: (x, y),
            });
        }
    }
    hits
}

/// The one match of a place, `None` for none, and an error that names
/// both for two.
fn one(mut hits: Vec<Found>, label: &str, place: &str) -> Result<Option<Found>, String> {
    match hits.len() {
        0 => Ok(None),
        1 => Ok(hits.pop()),
        _ => {
            let at: Vec<String> = hits
                .iter()
                .take(2)
                .map(|f| format!("row {} column {}", f.cell.1, f.cell.0))
                .collect();
            Err(format!(
                "{place} shows {label:?} twice, at {}.",
                at.join(" and at ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_begins_with_a_label_as_whole_words_after_its_marks() {
        assert!(begins("  ▸ Terminus    ter-u16n.psf.gz", "Terminus"));
        assert!(begins("Font set  terminus", "Font set"));
        assert!(begins("● 11 pt", "11 pt"));
        assert!(begins("│ console │", "console"));
        assert!(!begins("Terminus2", "Terminus"));
        assert!(!begins("Size 11 pt", "11 pt"));
        assert!(!begins("cullet-mono-bold", "cullet-mono"));
        assert!(begins("Save…", "Save"));
    }

    #[test]
    fn a_menu_s_trailing_ellipsis_is_no_part_of_the_name() {
        assert_eq!(named("Save…"), "Save");
        assert_eq!(named("New..."), "New");
        assert_eq!(named("Undo"), "Undo");
    }
}
