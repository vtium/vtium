//! A part's detail, edited: its label typed and set, and a narrowing
//! choice picked into NEW PATH. Enter on the detail's label line starts
//! typing the label, Enter keeps it as one change, which writes the
//! `[labels]` section, and Esc leaves the label as it was. Enter on a
//! choice picks the part narrowed to it into NEW PATH, or puts it in the
//! place of the NEW PATH part the detail was opened on.
//!
//! References:
//! - ADR-1403 D7, "Labels": what a label holds, where it is written, and
//!   that a label belongs to its part, not to a path.
//! - ADR-1403 D8, "What the Flow view writes": the `[labels]` lines.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::ui::{Action, Change, Command, Shared};
use vtium_config_widgets::roles::Role;

use super::plan;
use super::words;
use super::{Typed, entry, typing};
use crate::page::Model;
use crate::page::draw::Sheet;
use crate::page::layout::{Obj, Slot};
use crate::parts::{self, detail_target};
use crate::paths::{self, Piece};
use crate::{FlowTab, Level, say};

/// The row of the detail's screen that holds the label line.
const LABEL_ROW: u16 = 3;

/// The cell of the label line where the label starts.
const LABEL_X: usize = 13;

impl FlowTab {
    /// Enter on the detail's label line: typing starts from the label the
    /// part has now.
    pub(crate) fn start_label(&mut self, model: &Model, obj: &Obj) -> Vec<Command> {
        let Some((kind, sel)) = detail_target(obj) else {
            return Vec::new();
        };
        let typed = model.full.label(kind, sel).unwrap_or_default().to_owned();
        self.level = Level::Label {
            obj: obj.clone(),
            typed,
        };
        vec![Command::Change(Change::Quiet)]
    }

    /// A key while a part's label is typed: a character goes in, up to the
    /// most a label holds; Enter gives the part the label, or takes its
    /// label away when the entry is blank, as one change; Esc leaves the
    /// label as it was. Either goes back to the detail's label line.
    pub(crate) fn label_key(
        &mut self,
        shared: &Shared,
        model: &Model,
        obj: &Obj,
        mut typed: String,
        action: &Action,
    ) -> Vec<Command> {
        let back = Level::Detail {
            obj: obj.clone(),
            line: 0,
        };
        match typing(&mut typed, action, vtium_settings::LABEL_MAX) {
            Typed::Edited => {
                self.level = Level::Label {
                    obj: obj.clone(),
                    typed,
                };
                Vec::new()
            }
            Typed::Ignored => Vec::new(),
            Typed::Cancel => {
                self.level = back;
                vec![Command::Change(Change::Quiet)]
            }
            Typed::Keep => {
                let label = typed.trim();
                if label.starts_with('#') {
                    return say(words::LABEL_HASH);
                }
                let Some((kind, sel)) = detail_target(obj) else {
                    self.level = back;
                    return Vec::new();
                };
                let mut doc = model.full.clone();
                plan::set_label(&mut doc, kind, sel, label);
                let edits = plan::plan(&model.flow, &doc);
                let said = if label.is_empty() {
                    words::LABEL_REMOVED.to_owned()
                } else {
                    words::label_set(label)
                };
                let part = paths::title(&model.flow, kind, sel).0;
                match super::changed(shared, words::labelling(&part, label), edits, said) {
                    Ok(out) => {
                        self.level = back;
                        out
                    }
                    Err(refused) => refused,
                }
            }
        }
    }

    /// Enter on the detail's choice `at`, counted from 0: the part
    /// narrowed to it goes into NEW PATH, or takes the place of the NEW
    /// PATH part the detail is about, and the detail closes.
    pub(crate) fn choose(&mut self, model: &Model, obj: &Obj, at: usize) -> Vec<Command> {
        let Some((_, _, sel)) = parts::choices(&model.flow, obj).into_iter().nth(at) else {
            return Vec::new();
        };
        let mut out = self.close_level();
        match obj {
            Obj::Pal(slot @ (Slot::Display(_) | Slot::Input(_))) => {
                let piece = narrowed(slot, sel);
                if !self.state.new.has(&piece) {
                    out.extend(self.pick(model, piece));
                }
            }
            Obj::New(slot @ (Slot::Display(old) | Slot::Input(old))) => {
                let old = narrowed(slot, old.clone());
                let piece = narrowed(slot, sel);
                let at = Obj::New(Slot::of(&piece));
                plan::replace(&mut self.state.new, &old, piece);
                self.state.at = Some(at);
            }
            _ => {}
        }
        out
    }
}

/// The part of `slot`'s kind that matches by `sel`.
fn narrowed(slot: &Slot, sel: String) -> Piece {
    match slot {
        Slot::Display(_) => Piece::Display(sel),
        _ => Piece::Input(sel),
    }
}

/// Draws the detail of `obj` with its label being typed: the detail as
/// it is, with the label line showing the entry and what Enter does.
pub fn draw(model: &Model, obj: &Obj, typed: &str, buf: &mut Buffer, area: Rect, color: bool) {
    parts::draw_detail(model, obj, 0, buf, area, color);
    if detail_target(obj).is_none() || area.height <= LABEL_ROW {
        return;
    }
    let row = Rect::new(area.x, area.y.saturating_add(LABEL_ROW), area.width, 1);
    let mut s = Sheet::new(buf, row, color);
    s.highlight(0);
    s.put(3, 0, crate::words::LABEL_FIELD, Role::RowHighlight);
    let width = usize::from(area.width);
    let room = vtium_settings::LABEL_MAX
        .saturating_add(1)
        .min(width.saturating_sub(LABEL_X).saturating_sub(1));
    let x = s.put(LABEL_X, 0, &entry(typed, room), Role::RowHighlight);
    let hint_x = x.saturating_add(4).max(40.min(width / 2));
    s.put(
        hint_x,
        0,
        &vtium_config_widgets::canvas::fit(
            words::LABEL_HINT,
            width.saturating_sub(hint_x).saturating_sub(1),
        ),
        Role::RowHighlight,
    );
}
