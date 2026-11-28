//! Manage configurations…, the last item of File › Open ›: a sheet that
//! lists each configuration with its state against the installed file,
//! the open one marked, and opens, renames and deletes them. Enter opens
//! the highlighted one and keeps the one open. `r` types a new name in
//! place, and Enter refuses a name another configuration has. Delete, or
//! `d`, asks once and names what is lost. The open configuration cannot
//! be deleted, so the TUI always has one open, and the sheet says why.
//! Nothing here touches /etc: the configurations are drafts in the
//! person's own store. Rename and Delete go through the desk the drafts
//! are written through, and the sheet lists the configurations again after
//! each, with what the step did under the list.
//!
//! References:
//! - ADR-1403 D11, "Managing configurations": the sheet, its buttons,
//!   Rename's refusal, Delete's one question and what it names, and the
//!   open configuration that cannot be deleted.
//! - ADR-1403 D8, "vtium's store": the drafts, which `vtium config draft
//!   rename` and `draft rm` rename and delete on the command line.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::document::desk::words as desk;
use vtium_config_core::store;
use vtium_config_core::ui::{Action, Shared};
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::roles::Role;

use super::{Act, bar_row, put, window};

/// The sheet's title.
pub const TITLE: &str = "Manage configurations";

/// The row a yes to Delete's question hands the shell.
pub const DELETE: &str = "delete.layer.manage";

/// One configuration as the sheet lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    pub name: String,
    /// Its count of changes not saved.
    pub count: usize,
}

/// The sheet's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manage {
    kept: Vec<Kept>,
    /// The open configuration's name.
    open: String,
    at: usize,
    /// The new name being typed for the highlighted configuration.
    typing: Option<String>,
    /// The configuration whose delete was asked, until the answer.
    asked: Option<String>,
    /// A refusal or what the last step did, until the next key.
    note: Option<String>,
}

impl Manage {
    /// The sheet over `kept`, each configuration with its count, where
    /// `open` is the one open, which is highlighted first.
    pub fn new(kept: Vec<(String, usize)>, open: &str) -> Manage {
        let kept: Vec<Kept> = kept
            .into_iter()
            .map(|(name, count)| Kept { name, count })
            .collect();
        let at = kept.iter().position(|k| k.name == open).unwrap_or(0);
        Manage {
            kept,
            open: open.to_owned(),
            at,
            typing: None,
            asked: None,
            note: None,
        }
    }

    /// The node: the sheet, or the new name being typed.
    pub fn node(&self) -> &'static str {
        if self.typing.is_some() {
            "any.menu.file.manage.typing"
        } else {
            "any.menu.file.manage"
        }
    }

    /// The configurations listed.
    pub fn kept(&self) -> &[Kept] {
        &self.kept
    }

    /// The highlighted configuration.
    pub fn highlighted(&self) -> Option<&Kept> {
        self.kept.get(self.at)
    }

    /// The configuration whose delete was asked, which a yes deletes.
    pub fn asked(&self) -> Option<&str> {
        self.asked.as_deref()
    }

    /// Puts `note` under the list, as what a step did or why it could not.
    pub fn say(&mut self, note: String) {
        self.note = Some(note);
    }

    /// Renames the configuration `from` to `to` in `shared`, the open one
    /// too, and lists the configurations again with `to` highlighted. What
    /// it did, or why it could not, goes under the list.
    pub fn rename(&mut self, shared: &mut Shared, from: &str, to: &str) {
        match shared.documents.rename(from, to) {
            Ok(()) => {
                self.reload(shared, to);
                self.note = Some(words::renamed(from, to));
            }
            Err(why) => self.note = Some(why),
        }
    }

    /// A yes to Delete's question: deletes the configuration it named from
    /// `shared`, never the open one, and lists the configurations again.
    /// What it did, or why it could not, goes under the list.
    pub fn delete(&mut self, shared: &mut Shared) {
        let Some(name) = self.asked.take() else {
            return;
        };
        match shared.documents.remove(&name) {
            Ok(()) => {
                let at = self
                    .kept
                    .get(self.at.saturating_add(1))
                    .map(|k| k.name.clone());
                self.reload(shared, at.as_deref().unwrap_or_default());
                self.note = Some(words::deleted(&name));
            }
            Err(why) => self.note = Some(why),
        }
    }

    /// Lists the configurations of `shared` again, with `at` highlighted,
    /// or the last one where no configuration has that name.
    fn reload(&mut self, shared: &Shared, at: &str) {
        let fresh = Manage::new(super::open::states(shared), shared.document().name());
        self.at = fresh
            .kept
            .iter()
            .position(|k| k.name == at)
            .unwrap_or_else(|| fresh.kept.len().saturating_sub(1));
        self.kept = fresh.kept;
        self.open = fresh.open;
    }

    /// Acts on `key`, which chose the row `id` of the sheet's node.
    pub fn key(&mut self, key: &Key, id: &str) -> Act {
        self.note = None;
        if let Some(entry) = &mut self.typing {
            return match id {
                "char.typing" => {
                    if let Some(c) = key.typed() {
                        entry.push(c);
                    }
                    Act::Stay
                }
                "backspace.typing" => {
                    entry.pop();
                    Act::Stay
                }
                "esc.typing" => {
                    self.typing = None;
                    Act::Stay
                }
                "enter.typing" => self.typed_name(),
                _ => Act::Pass,
            };
        }
        let Some(kept) = self.kept.get(self.at) else {
            return match id {
                "esc.layer.manage" => Act::Close,
                _ => Act::Pass,
            };
        };
        match id {
            "updown.layer.manage" => {
                let last = self.kept.len().saturating_sub(1);
                self.at = match key.code {
                    Code::Up => self.at.saturating_sub(1),
                    _ => (self.at + 1).min(last),
                };
                Act::Stay
            }
            "enter.layer.manage" if kept.name == self.open => Act::Close,
            "enter.layer.manage" => Act::Open(kept.name.clone()),
            "r.layer.manage" => {
                self.typing = Some(kept.name.clone());
                Act::Stay
            }
            DELETE | "d.layer.manage" if kept.name == self.open => {
                self.note = Some(desk::open_now(&kept.name));
                Act::Stay
            }
            DELETE | "d.layer.manage" => {
                self.asked = Some(kept.name.clone());
                Act::Ask {
                    question: words::delete(&kept.name, kept.count),
                    then: Action::of(DELETE),
                }
            }
            "esc.layer.manage" => Act::Close,
            "question.layer" => Act::Stay,
            _ => Act::Pass,
        }
    }

    /// Enter on the new name: the rename, or why the name cannot be one.
    fn typed_name(&mut self) -> Act {
        let (Some(to), Some(from)) = (self.typing.clone(), self.highlighted()) else {
            return Act::Stay;
        };
        let from = from.name.clone();
        if to == from {
            self.typing = None;
            return Act::Stay;
        }
        if !store::valid_name(&to) {
            self.note = Some(words::BAD_NAME.to_owned());
            return Act::Stay;
        }
        if self.kept.iter().any(|k| k.name == to) {
            self.note = Some(desk::taken(&to));
            return Act::Stay;
        }
        self.typing = None;
        Act::Rename { from, to }
    }

    /// A click on the sheet's line `line`, of `rows` in view: it
    /// highlights the configuration there.
    pub fn click(&mut self, line: usize, rows: usize) -> Act {
        let list = rows.saturating_sub(FOOT);
        let top = window(self.kept.len(), list, Some(self.at));
        let at = top.saturating_add(line);
        if line < list && at < self.kept.len() && self.typing.is_none() {
            self.at = at;
        }
        Act::Stay
    }

    /// How many lines the sheet holds.
    pub fn lines(&self) -> usize {
        self.kept.len().saturating_add(FOOT)
    }

    /// The sheet's box in `viewer`.
    pub fn area(&self, viewer: Rect) -> Rect {
        super::top_box(
            viewer,
            viewer.width.saturating_sub(4).min(WIDTH),
            self.lines(),
        )
    }

    /// Draws the list into `area`: a configuration a line, its state and
    /// the open one's mark, then where they live and the note.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) {
        let rows = usize::from(area.height);
        let list = rows.saturating_sub(FOOT);
        let top = window(self.kept.len(), list, Some(self.at));
        let width = usize::from(area.width);
        for (y, (i, kept)) in self
            .kept
            .iter()
            .enumerate()
            .skip(top)
            .take(list)
            .enumerate()
        {
            let y = u16::try_from(y).unwrap_or(u16::MAX);
            let lit = i == self.at;
            let role = if lit {
                bar_row(buf, area, y, Role::RowHighlight, color);
                Role::RowHighlight
            } else {
                Role::Row
            };
            let line = match (&self.typing, lit) {
                (Some(entry), true) => format!("{entry}_"),
                _ => {
                    let state = match kept.count {
                        0 => "saved".to_owned(),
                        n => format!("● {n} not saved"),
                    };
                    let open = if kept.name == self.open {
                        "open now"
                    } else {
                        ""
                    };
                    format!("{:<NAME_COLUMNS$} {state:<18} {open}", kept.name)
                }
            };
            put(buf, area, 0, y, &super::cut(&line, width), role, color);
        }
        let foot_y = u16::try_from(list).unwrap_or(u16::MAX);
        put(
            buf,
            area,
            0,
            foot_y.saturating_add(1),
            words::WHERE,
            Role::Dim,
            color,
        );
        if let Some(note) = &self.note {
            let note = super::cut(note, width);
            put(
                buf,
                area,
                0,
                foot_y.saturating_add(2),
                &note,
                Role::Warning,
                color,
            );
        }
    }
}

/// The lines under the list: a blank, where they live, and the note.
const FOOT: usize = 3;

/// The widest the sheet grows, and the column of the names.
const WIDTH: u16 = 86;
const NAME_COLUMNS: usize = 14;

/// The sheet's words.
mod words {
    pub const WHERE: &str =
        "They live in ~/.config/vtium/drafts. /etc/vtium/vtium.conf holds what is saved.";
    pub const BAD_NAME: &str = "A name has 1 to 64 letters, digits, '.', '_', '+' or '-', and cannot start with '.' or '-'.";

    pub fn renamed(from: &str, to: &str) -> String {
        format!("Renamed {from} to {to}.")
    }

    pub fn deleted(name: &str) -> String {
        format!("Deleted {name}. /etc/vtium/vtium.conf is untouched.")
    }

    /// Delete's one question, naming what is lost and what stays, in one
    /// line at 80 columns for a short name.
    pub fn delete(name: &str, count: usize) -> String {
        let lost = match count {
            0 => "Its text and font sets are lost".to_owned(),
            1 => "Its 1 change not saved and font sets are lost".to_owned(),
            n => format!("Its {n} changes not saved and font sets are lost"),
        };
        format!("Delete {name}? {lost}; /etc is untouched.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> Manage {
        Manage::new(
            vec![
                ("kiosk-test".to_owned(), 6),
                ("north".to_owned(), 1),
                ("travel".to_owned(), 0),
            ],
            "north",
        )
    }

    #[test]
    fn delete_on_the_open_configuration_asks_nothing_and_says_why() {
        let mut m = sheet();
        assert_eq!(m.highlighted().map(|k| k.name.as_str()), Some("north"));
        assert_eq!(m.key(&Key::char('d'), "d.layer.manage"), Act::Stay);
        assert_eq!(m.asked(), None);
        assert!(
            m.note
                .as_deref()
                .is_some_and(|n| n.contains("open another first"))
        );
    }

    #[test]
    fn rename_refuses_a_name_another_configuration_has() {
        let mut m = sheet();
        m.key(&Key::plain(Code::Down), "updown.layer.manage");
        m.key(&Key::char('r'), "r.layer.manage");
        assert_eq!(m.node(), "any.menu.file.manage.typing");
        for _ in 0.."travel".len() {
            m.key(&Key::plain(Code::Backspace), "backspace.typing");
        }
        for c in "north".chars() {
            m.key(&Key::char(c), "char.typing");
        }
        assert_eq!(m.key(&Key::plain(Code::Enter), "enter.typing"), Act::Stay);
        assert!(m.note.as_deref().is_some_and(|n| n.contains("is taken")));
        assert_eq!(m.node(), "any.menu.file.manage.typing");
    }
}
