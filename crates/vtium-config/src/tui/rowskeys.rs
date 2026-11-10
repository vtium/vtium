//! The keys of the Flow view as paths ([`super::rows`]), as the state
//! machine's. The arrows move the highlight from box to box, and PgUp and
//! PgDn scroll by a window. Space picks a piece into the editor or takes
//! it out, `a` adds the editor's path to the document, `e` revises an
//! added path, which moves it back into the editor, and Esc clears the
//! editor. Enter opens a box's layer: an added terminal's card of the
//! settings that do not change its pieces, a piece's narrowing choices,
//! the editor terminal's name and font set, or a font set's preview in the
//! Fonts view. `v` shows the six-column view in the rows' place.
//!
//! Every change to the draft goes through [`super::paths::plan`] and
//! [`App::make`], so through `src/ops.rs`.

use super::edit::{self, Field, What};
use super::flow::Item;
use super::input::Key;
use super::model::{App, Effect, Live, View, step};
use super::paths::{self, Adding, Path, Picked, Piece};
use super::rows::{self, Obj, Slot, State};
use super::rowsdraw;
use super::words;

impl App {
    /// The rows' state, laid out again after every event: the highlight
    /// back on its object, the window where it keeps that object at the
    /// same height, and a layer closed when its object is gone.
    pub(crate) fn rows_settle(&mut self) {
        if self.rows_flow.is_none() {
            return;
        }
        // a card the route card opened is shown in the six-column view
        if self.opened
            && self
                .rows_flow
                .as_ref()
                .is_some_and(|s| s.layer.is_none() && !s.classic)
        {
            self.rows_with(|s| s.classic = true);
        }
        let layout = rowsdraw::layout_of(self);
        let opened = self.opened;
        let mut gone = false;
        self.rows_with(|s| match &s.layer {
            Some(Obj::Doc(_, Slot::Terminal)) if !opened => s.layer = None,
            Some(o @ Obj::Doc(..)) if layout.find(o).is_none() => {
                gone = true;
                s.layer = None;
            }
            _ => {}
        });
        if gone {
            self.opened = false;
        }
        let (_, height) = rowsdraw::window(self);
        self.rows_with(|s| s.settle(&layout, height));
    }

    /// Whether the editor terminal's name is being typed, so every key
    /// goes to it.
    pub(crate) fn rows_typing(&self) -> bool {
        self.view == View::Flow && self.rows_flow.as_ref().is_some_and(|s| s.typing.is_some())
    }

    /// The fields of an added terminal's card: the settings that do not
    /// change which pieces its path joins, its own keys and its font set's
    /// size and options; `None` when no such card is open.
    pub(crate) fn rows_fields(&self) -> Option<Vec<Field>> {
        let Some(Obj::Doc(name, Slot::Terminal)) =
            self.rows_flow.as_ref().and_then(|s| s.layer.as_ref())
        else {
            return None;
        };
        let flow = &self.inputs.flow;
        let h = flow.heads.iter().position(|x| x.name == *name)?;
        let mut out: Vec<Field> = edit::fields(flow, Item::Head(h), &[], &[])
            .into_iter()
            .filter(|f| matches!(f.what, What::HeadKey(..)))
            .collect();
        if flow.look_of(h).is_some() {
            out.extend(
                edit::fields(flow, Item::HeadLook(h), &[], &[])
                    .into_iter()
                    .filter(|f| matches!(f.what, What::LookKey(_, k) if k != "font-name")),
            );
        }
        Some(out)
    }

    /// A key in the Flow view as paths.
    pub(crate) fn rows_key(&mut self, key: Key) -> Vec<Effect> {
        let Some(state) = self.rows_flow.clone() else {
            return self.flow_key(key);
        };
        if state.classic {
            let idle = self.tags.is_empty()
                && !self.opened
                && self.route.is_none()
                && self.pager.is_none()
                && self.restore.is_none();
            if key == Key::Escape && idle {
                self.message = None;
                self.rows_with(|s| s.classic = false);
                return Vec::new();
            }
            return self.flow_key(key);
        }
        if self.route.is_some() || self.pager.is_some() || self.restore.is_some() {
            return self.flow_key(key);
        }
        if let Some(typed) = &state.typing {
            return self.name_key(typed.clone(), key);
        }
        match &state.layer {
            Some(Obj::Doc(name, Slot::Terminal)) => {
                if key == Key::Char('e') && self.card.typed.is_none() {
                    self.opened = false;
                    return self.revise(name);
                }
                self.card_key(key).unwrap_or_default()
            }
            Some(Obj::Doc(name, _)) => match key {
                Key::Char('e') => self.revise(name),
                Key::Escape => {
                    self.rows_with(|s| s.layer = None);
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Some(Obj::Others) => {
                if key == Key::Escape {
                    self.rows_with(|s| s.layer = None);
                }
                Vec::new()
            }
            Some(Obj::New(Slot::Terminal)) => self.terminal_key(&state, key),
            Some(obj) => self.choice_key(&state, obj, key),
            None => self.overview_key(&state, key),
        }
    }

    fn rows_with(&mut self, f: impl FnOnce(&mut State)) {
        if let Some(s) = self.rows_flow.as_mut() {
            f(s);
        }
    }

    fn tell(&mut self, what: impl Into<String>) -> Vec<Effect> {
        self.message = Some(what.into());
        Vec::new()
    }

    /// A key on the document.
    fn overview_key(&mut self, state: &State, key: Key) -> Vec<Effect> {
        let layout = rowsdraw::layout_of(self);
        let (_, height) = rowsdraw::window(self);
        let at = state.at.clone();
        if !matches!(key, Key::Char(' ' | 'a' | 'e' | 'p' | 't' | 's')) {
            self.message = None;
        }
        match key {
            Key::Up | Key::Down | Key::Left | Key::Right | Key::Home | Key::End => {
                if let Some(to) = at.as_ref().and_then(|a| rows::step(&layout, a, key)) {
                    self.rows_with(|s| s.go(&layout, to, height));
                }
                Vec::new()
            }
            Key::PageUp | Key::PageDown => {
                let down = key == Key::PageDown;
                self.rows_with(|s| s.page(&layout, down, height));
                Vec::new()
            }
            Key::Enter => self.open(at),
            Key::Char(' ') => self.space(state, at),
            Key::Char('a') => self.add(state),
            Key::Char('e') => match &at {
                Some(Obj::Doc(name, _)) => self.revise(name),
                _ => self.tell(words::REVISE_WHERE),
            },
            Key::Escape if !state.new.is_empty() => {
                self.rows_with(|s| s.new = Path::default());
                self.tell(words::EDITOR_CLEARED)
            }
            Key::Char('p') => self.tell(match self.live {
                Live::On => words::PREVIEW_PATH_LATER,
                _ => words::PREVIEW_PATH_NOT_HERE,
            }),
            Key::Char('t') => self.tell(match self.live {
                Live::On => words::LIVE_ON,
                Live::OpsOff => words::OPS_OFF,
                _ => words::NOT_VTIUM,
            }),
            Key::Char('?') => {
                let item = at
                    .as_ref()
                    .and_then(|o| self.doc_item(o))
                    .unwrap_or(self.flow_at);
                self.flow_at = item;
                self.opened = false;
                self.flow_key(key)
            }
            Key::Char('v') => {
                self.opened = false;
                self.rows_with(|s| s.classic = true);
                self.tell(words::ROWS_RULES_BACK)
            }
            Key::Char('s') => self.tell(words::ROWS_SAVE),
            Key::Char(c @ ('i' | 'd' | 'r' | 'o' | 'n' | 'x')) => self.action(c),
            _ => Vec::new(),
        }
    }

    /// The flow's item an added path's box stands for, for the route card.
    fn doc_item(&self, obj: &Obj) -> Option<Item> {
        let flow = &self.inputs.flow;
        let Obj::Doc(name, slot) = obj else {
            return None;
        };
        let h = flow.heads.iter().position(|x| x.name == *name)?;
        Some(match slot {
            Slot::Display(sel) => flow
                .rules
                .iter()
                .position(|r| r.selector == *sel)
                .map_or(Item::Head(h), Item::Rule),
            Slot::Input(sel) => flow
                .rules
                .iter()
                .position(|r| r.selector == *sel)
                .map_or(Item::HeadLook(h), Item::Rule),
            _ => Item::Head(h),
        })
    }

    /// Enter on the document: opens the highlighted box's layer, or a font
    /// set's preview in the Fonts view.
    fn open(&mut self, at: Option<Obj>) -> Vec<Effect> {
        let Some(obj) = at else {
            return Vec::new();
        };
        self.message = None;
        match &obj {
            Obj::Doc(name, Slot::Terminal) => {
                let Some(h) = self.inputs.flow.heads.iter().position(|x| x.name == *name) else {
                    return Vec::new();
                };
                self.flow_at = Item::Head(h);
                self.rows_with(|s| s.layer = Some(obj.clone()));
                self.open_card();
                Vec::new()
            }
            Obj::Pal(Slot::Font(f)) => {
                let Some(f) = f else {
                    return self.tell(words::DEFAULT_FONT_PREVIEW);
                };
                if let Some(at) = self.inputs.fonts.iter().position(|x| x.name == *f) {
                    self.font_at = at;
                }
                self.set_view(View::Fonts)
            }
            _ => {
                let line = self.choice_line(&obj);
                self.rows_with(|s| {
                    s.layer = Some(obj);
                    s.line = line;
                });
                Vec::new()
            }
        }
    }

    /// The line a layer's cursor starts on: the piece's own choice, or for
    /// the editor's terminal, its name.
    fn choice_line(&self, obj: &Obj) -> usize {
        let now = match obj {
            Obj::New(Slot::Display(s)) | Obj::New(Slot::Input(s)) => Some(s.clone()),
            _ => return 0,
        };
        rowsdraw::choices_for(self, obj)
            .iter()
            .position(|(_, _, v)| *v == now)
            .unwrap_or(0)
    }

    /// Space: picks a palette piece into the editor or takes it out, takes
    /// an editor piece out, and says why anything else cannot be picked.
    fn space(&mut self, state: &State, at: Option<Obj>) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let (doc, _) = paths::read(flow);
        let Some(obj) = at else {
            return Vec::new();
        };
        match &obj {
            Obj::Pal(Slot::Display(sel)) => {
                let held = flow
                    .displays
                    .iter()
                    .find(|d| d.keys.contains(sel))
                    .map(|d| rows::holders(&doc, super::flow::Kind::Display, d))
                    .unwrap_or_default();
                let picked = state.new.has(&Piece::Display(sel.clone()));
                if let (Some(h), false) = (held.first(), picked) {
                    let (title, _) = paths::title(flow, super::flow::Kind::Display, sel);
                    return self.tell(words::display_taken(&title, h));
                }
                self.pick(Piece::Display(sel.clone()))
            }
            Obj::Pal(Slot::Input(sel)) | Obj::New(Slot::Input(sel)) => {
                self.pick(Piece::Input(sel.clone()))
            }
            Obj::New(Slot::Display(sel)) => self.pick(Piece::Display(sel.clone())),
            Obj::Pal(Slot::Font(f)) => self.pick(Piece::Font(f.clone())),
            Obj::Others => self.tell(words::OTHERS_CANNOT),
            Obj::Doc(..) => self.tell(words::IN_DOCUMENT),
            _ => Vec::new(),
        }
    }

    /// Picks `piece` into the editor, or takes it out, and says what that
    /// did and what the path needs next.
    fn pick(&mut self, piece: Piece) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let (doc, _) = paths::read(flow);
        let Some(state) = self.rows_flow.as_mut() else {
            return Vec::new();
        };
        let was_font = state.new.font.clone();
        let done = match &piece {
            // the default font set is the absence of one
            Piece::Font(None) if was_font.is_some() => {
                state.new.font = None;
                Picked::Replaced(was_font.clone())
            }
            Piece::Font(None) => Picked::Added,
            _ => state.new.pick(piece.clone()),
        };
        let new = state.new.clone();
        let (kind, sel) = match &piece {
            Piece::Display(s) => (super::flow::Kind::Display, s.as_str()),
            Piece::Input(s) => (super::flow::Kind::Input, s.as_str()),
            Piece::Font(f) => {
                let now = f
                    .clone()
                    .unwrap_or_else(|| words::DEFAULT_FONT_SET.to_owned());
                return self.tell(match done {
                    Picked::Removed => words::font_default(),
                    Picked::Replaced(Some(old)) => words::font_instead(&now, &old),
                    _ => words::font_picked(&now),
                });
            }
        };
        // a device is named as a reader names it, `the 8BitDo Keyboard`;
        // a wider piece by its title, `any keyboard`
        let title = flow
            .devices(kind)
            .iter()
            .find(|d| d.found && d.keys.iter().any(|k| k == sel))
            .map_or_else(
                || paths::title(flow, kind, sel).0,
                |d| format!("the {}", d.name),
            );
        if done == Picked::Removed {
            return self.tell(words::piece_out(&title));
        }
        let also = flow
            .inputs
            .iter()
            .find(|d| d.found && d.keys.iter().any(|k| k == sel))
            .map(|d| rows::holders(&doc, super::flow::Kind::Input, d))
            .unwrap_or_default();
        let display = kind == super::flow::Kind::Display;
        self.tell(words::picked(&title, display, &new, &also))
    }

    /// `a`: adds the editor's path to the document, or says why it stays.
    fn add(&mut self, state: &State) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let (mut doc, _) = paths::read(flow);
        let mut new = state.new.clone();
        if new.name.is_none() {
            new.name = Some(paths::new_name(flow, &doc));
        }
        let name = new.name.clone().unwrap_or_default();
        match paths::check_add(flow, &doc, &new) {
            Err(why) => self.tell(why),
            Ok(Adding::Same(there)) => {
                self.rows_with(|s| s.new = Path::default());
                self.tell(words::same_path(&there))
            }
            Ok(Adding::New) => {
                doc.push(new);
                let changes = paths::plan(flow, &doc);
                let out = self.make(changes, None);
                if !out.is_empty() {
                    self.rows_with(|s| {
                        s.new = Path::default();
                        s.want = Some(Obj::Doc(name.clone(), Slot::Terminal));
                    });
                    self.message = Some(words::path_added(&name));
                }
                out
            }
        }
    }

    /// `e`: revises the path of the terminal `name`: it leaves the
    /// document and goes into the editor with its pieces picked.
    fn revise(&mut self, name: &str) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let (doc, _) = paths::read(flow);
        if self.rows_flow.as_ref().is_some_and(|s| !s.new.is_empty()) {
            return self.tell(words::EDITOR_BUSY);
        }
        let Some((rest, path)) = paths::revise(&doc, name) else {
            return Vec::new();
        };
        let changes = paths::plan(flow, &rest);
        let out = self.make(changes, None);
        if !out.is_empty() {
            self.rows_with(|s| {
                s.new = path;
                s.layer = None;
                s.at = Some(Obj::New(Slot::Terminal));
            });
            self.message = Some(words::path_revised(name));
        }
        out
    }

    /// A key on a piece's choices: Up and Down move, Enter picks the choice
    /// into the editor, or puts it in the editor piece's place, and Esc goes
    /// back.
    fn choice_key(&mut self, state: &State, obj: &Obj, key: Key) -> Vec<Effect> {
        let list = rowsdraw::choices_for(self, obj);
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let line = step(state.line, list.len(), key);
                self.rows_with(|s| s.line = line);
                Vec::new()
            }
            Key::Escape => {
                self.rows_with(|s| s.layer = None);
                Vec::new()
            }
            Key::Enter => {
                let Some((_, _, Some(sel))) = list.get(state.line).cloned() else {
                    return Vec::new();
                };
                self.rows_with(|s| s.layer = None);
                match obj {
                    Obj::Pal(Slot::Display(_)) => {
                        if state.new.has(&Piece::Display(sel.clone())) {
                            return Vec::new();
                        }
                        self.pick(Piece::Display(sel))
                    }
                    Obj::Pal(Slot::Input(_)) => {
                        if state.new.has(&Piece::Input(sel.clone())) {
                            return Vec::new();
                        }
                        self.pick(Piece::Input(sel))
                    }
                    Obj::New(slot @ (Slot::Display(_) | Slot::Input(_))) => {
                        let Some(old) = slot.piece() else {
                            return Vec::new();
                        };
                        let new = match slot {
                            Slot::Display(_) => Piece::Display(sel.clone()),
                            _ => Piece::Input(sel.clone()),
                        };
                        let at = match slot {
                            Slot::Display(_) => Slot::Display(sel),
                            _ => Slot::Input(sel),
                        };
                        self.rows_with(|s| {
                            s.new.replace(&old, new);
                            s.at = Some(Obj::New(at));
                        });
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    /// A key on the editor terminal's layer: line 0 is its name, which
    /// Enter starts typing, and the lines after it its font sets, which
    /// Enter picks.
    fn terminal_key(&mut self, state: &State, key: Key) -> Vec<Effect> {
        let fonts = rowsdraw::choices_for(self, &Obj::New(Slot::Terminal));
        let lines = fonts.len().saturating_add(1);
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let line = step(state.line, lines, key);
                self.rows_with(|s| s.line = line);
                Vec::new()
            }
            Key::Escape => {
                self.rows_with(|s| s.layer = None);
                Vec::new()
            }
            Key::Enter if state.line == 0 => {
                let flow = &self.inputs.flow;
                let (doc, _) = paths::read(flow);
                let name = state
                    .new
                    .name
                    .clone()
                    .unwrap_or_else(|| paths::new_name(flow, &doc));
                self.rows_with(|s| s.typing = Some(name));
                Vec::new()
            }
            Key::Enter => {
                let Some((_, _, f)) = fonts.get(state.line.saturating_sub(1)).cloned() else {
                    return Vec::new();
                };
                self.pick(Piece::Font(f.clone()));
                if f.is_some() {
                    self.rows_with(|s| s.new.font = f);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A key while the editor terminal's name is typed: characters go into
    /// it, Enter keeps it and Esc drops it.
    fn name_key(&mut self, mut typed: String, key: Key) -> Vec<Effect> {
        match key {
            Key::Char(c) if typed.chars().count() < 64 => {
                typed.push(c);
                self.rows_with(|s| s.typing = Some(typed));
            }
            Key::Backspace => {
                typed.pop();
                self.rows_with(|s| s.typing = Some(typed));
            }
            Key::Enter if paths::valid_name(&typed) => {
                self.rows_with(|s| {
                    s.new.name = Some(typed);
                    s.typing = None;
                });
                self.message = None;
            }
            Key::Enter => {
                self.message = Some(words::name_invalid(&typed));
            }
            Key::Escape => {
                self.rows_with(|s| s.typing = None);
                self.message = None;
            }
            _ => {}
        }
        Vec::new()
    }

    /// A click on the document: on a box, highlights it, and on the
    /// highlighted box, opens it.
    pub(crate) fn rows_click(&mut self, col: usize, row: usize) -> Vec<Effect> {
        let Some(state) = self.rows_flow.clone() else {
            return Vec::new();
        };
        if state.layer.is_some() || state.typing.is_some() {
            return Vec::new();
        }
        let (top, height) = rowsdraw::window(self);
        if row < top || row >= top.saturating_add(height) {
            return Vec::new();
        }
        let y = state.top.saturating_add(row.saturating_sub(top));
        let layout = rowsdraw::layout_of(self);
        let hit = layout
            .boxes
            .iter()
            .find(|b| col >= b.x && col < b.right() && y >= b.y && y < b.bottom())
            .map(|b| b.obj.clone());
        match hit {
            Some(o) if state.at.as_ref() == Some(&o) => self.open(Some(o)),
            Some(o) => {
                self.rows_with(|s| s.go(&layout, o, height));
                Vec::new()
            }
            None => Vec::new(),
        }
    }
}
