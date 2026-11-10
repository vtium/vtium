//! The keys of the Flow view as rows ([`super::rows`]), as the state
//! machine's: the arrows move the highlight from box to box, PgUp and PgDn
//! scroll by a window, Enter opens an object's second layer and Esc
//! closes it, and Space starts a path from a device that Space on a
//! terminal ends. `v` shows the six-column view in the rows' place, and
//! Esc there with nothing open comes back.
//!
//! A terminal's layer is the card the six-column view opens for a head,
//! so its keys are the card's ([`App::card_key`]). A device's layer offers
//! choices whose changes [`super::rows::device_lines`] makes with the
//! six-column view's own changes. Every change goes into the draft through
//! [`App::make`].

use super::edit;
use super::flow::Item;
use super::input::Key;
use super::model::{App, Effect, step};
use super::rows::{self, Obj, State};
use super::rowsdraw;
use super::words;

impl App {
    /// The rows' state, laid out again after every event: the highlight
    /// back on its object, the window where it keeps that object at the
    /// same height, and a layer closed when its object is gone.
    pub(crate) fn rows_settle(&mut self) {
        let Some(mut state) = self.rows_flow.take() else {
            return;
        };
        let flow = &self.inputs.flow;
        // a card the route card opened shows as its object's layer, or in
        // the six-column view where the rows have no layer for it
        if self.opened && state.layer.is_none() && !state.classic {
            match rows::obj_of(flow, self.flow_at) {
                Some(Obj::Terminal(name)) => {
                    self.flow_at = flow
                        .heads
                        .iter()
                        .position(|h| h.name == name)
                        .map_or(self.flow_at, Item::Head);
                    state.at = Some(Obj::Terminal(name.clone()));
                    state.layer = Some(Obj::Terminal(name));
                }
                Some(o) => {
                    self.opened = false;
                    state.at = Some(o.clone());
                    state.layer = Some(o);
                }
                None => state.classic = true,
            }
        }
        match &state.layer {
            // the terminal's card closed, or it was renamed: the layer
            // follows the card, which the flow carries across a change
            Some(Obj::Terminal(_)) if !self.opened => state.layer = None,
            Some(Obj::Terminal(_)) => {
                if let Some(o) = rows::obj_of(flow, self.flow_at) {
                    state.at = Some(o.clone());
                    state.layer = Some(o);
                }
            }
            Some(Obj::Others) | None => {}
            Some(o) => {
                if rows::item_of(flow, o).is_none() {
                    state.layer = None;
                }
            }
        }
        let layout = rows::layout(flow, self.cols);
        self.rows_flow = Some(state);
        let (_, height) = rowsdraw::window(self);
        if let Some(state) = self.rows_flow.as_mut() {
            state.settle(&layout, height);
        }
    }

    /// A key in the Flow view as rows.
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
                if let Some(o) = rows::obj_of(&self.inputs.flow, self.flow_at) {
                    self.rows_with(|s| s.at = Some(o));
                }
                return Vec::new();
            }
            return self.flow_key(key);
        }
        if self.route.is_some() || self.pager.is_some() || self.restore.is_some() {
            return self.flow_key(key);
        }
        match &state.layer {
            Some(Obj::Terminal(_)) => self.card_key(key).unwrap_or_default(),
            Some(Obj::Others) => {
                if key == Key::Escape {
                    self.rows_with(|s| s.layer = None);
                }
                Vec::new()
            }
            Some(obj) => self.device_key(obj, key),
            None => self.overview_key(&state, key),
        }
    }

    fn rows_with(&mut self, f: impl FnOnce(&mut State)) {
        if let Some(s) = self.rows_flow.as_mut() {
            f(s);
        }
    }

    /// A key on the overview.
    fn overview_key(&mut self, state: &State, key: Key) -> Vec<Effect> {
        let layout = rows::layout(&self.inputs.flow, self.cols);
        let (_, height) = rowsdraw::window(self);
        let at = state.at.clone();
        let item = at
            .as_ref()
            .and_then(|o| rows::item_of(&self.inputs.flow, o));
        if !matches!(key, Key::Char(' ')) && state.path.is_none() {
            self.message = None;
        }
        match key {
            Key::Up | Key::Down | Key::Left | Key::Right | Key::Home | Key::End => {
                if let Some(to) = at.as_ref().and_then(|a| rows::step(&layout, a, key)) {
                    self.rows_with(|s| s.go(&layout, to, height));
                }
            }
            Key::PageUp | Key::PageDown => {
                let down = key == Key::PageDown;
                self.rows_with(|s| s.page(&layout, down, height));
            }
            Key::Enter => self.open_layer(at),
            Key::Char(' ') => return self.path_key(state, at, item),
            Key::Escape => {
                self.message = None;
                self.rows_with(|s| s.path = None);
            }
            Key::Char('?') => {
                if let Some(item) = item {
                    self.flow_at = item;
                    self.opened = false;
                    return self.flow_key(key);
                }
            }
            Key::Char('v') => {
                if let Some(item) = item {
                    self.flow_at = item;
                }
                self.opened = false;
                self.message = Some(words::ROWS_RULES_BACK.to_owned());
                self.rows_with(|s| s.classic = true);
            }
            Key::Char('s') => self.message = Some(words::ROWS_SAVE.to_owned()),
            Key::Char(c @ ('i' | 'd' | 'r' | 'o' | 'n' | 'x')) => return self.action(c),
            _ => {}
        }
        Vec::new()
    }

    /// Enter on the overview: opens the highlighted object's second layer.
    fn open_layer(&mut self, at: Option<Obj>) {
        let Some(obj) = at else {
            return;
        };
        let flow = &self.inputs.flow;
        match rows::item_of(flow, &obj) {
            Some(item @ Item::Head(_)) => {
                self.flow_at = item;
                self.open_card();
            }
            Some(item) => {
                let lines = rows::device_lines(flow, item);
                let pick = lines
                    .first()
                    .and_then(|l| l.choices.iter().position(|c| c.on))
                    .unwrap_or(0);
                self.rows_with(|s| {
                    s.line = 0;
                    s.pick = pick;
                });
            }
            None => {}
        }
        self.message = None;
        self.rows_with(|s| s.layer = Some(obj));
    }

    /// Space: starts a path from a screen or a device, or ends the path
    /// started on the terminal highlighted, as the draft's change that
    /// sends the device there.
    fn path_key(&mut self, state: &State, at: Option<Obj>, item: Option<Item>) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        match (&at, item) {
            (Some(Obj::Terminal(_)), Some(head)) => {
                let Some(from) = state.path.as_ref().and_then(|p| rows::item_of(flow, p)) else {
                    self.message = Some(words::ROWS_PATH_TERMINAL.to_owned());
                    return Vec::new();
                };
                self.rows_with(|s| s.path = None);
                match edit::connect(&self.inputs.flow, &[from, head]) {
                    Ok(changes) if changes.is_empty() => {
                        self.message = Some(words::CONNECT_NO_CHANGE.to_owned());
                        Vec::new()
                    }
                    Ok(changes) => self.make(changes, None),
                    Err(why) => {
                        self.message = Some(why);
                        Vec::new()
                    }
                }
            }
            (Some(Obj::Others), _) => {
                self.message = Some(words::ROWS_PATH_OTHERS.to_owned());
                Vec::new()
            }
            (Some(o), Some(_)) => {
                let name = match o {
                    Obj::Screen(n, _) | Obj::Input(n, _) | Obj::Terminal(n) => n.clone(),
                    Obj::Others => String::new(),
                };
                self.message = Some(words::rows_path_from(&name));
                let o = o.clone();
                self.rows_with(|s| s.path = Some(o));
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A key on a device's or a screen's layer: Up and Down move between
    /// its lines, Left and Right between a line's choices, Enter makes the
    /// choice under the cursor, and Esc goes back to the overview.
    fn device_key(&mut self, obj: &Obj, key: Key) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let Some(item) = rows::item_of(flow, obj) else {
            self.rows_with(|s| s.layer = None);
            return Vec::new();
        };
        let lines = rows::device_lines(flow, item);
        let Some(state) = self.rows_flow.clone() else {
            return Vec::new();
        };
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let line = step(state.line, lines.len(), key);
                let pick = lines
                    .get(line)
                    .and_then(|l| l.choices.iter().position(|c| c.on))
                    .unwrap_or(0);
                self.rows_with(|s| {
                    s.line = line;
                    s.pick = pick;
                });
            }
            Key::Left => self.rows_with(|s| s.pick = s.pick.saturating_sub(1)),
            Key::Right => {
                let n = lines.get(state.line).map_or(0, |l| l.choices.len());
                self.rows_with(|s| s.pick = s.pick.saturating_add(1).min(n.saturating_sub(1)));
            }
            Key::Enter => {
                let Some(c) = lines
                    .get(state.line)
                    .and_then(|l| l.choices.get(state.pick))
                else {
                    return Vec::new();
                };
                return match &c.changes {
                    Ok(changes) if c.on || changes.is_empty() => Vec::new(),
                    Ok(changes) => self.make(changes.clone(), None),
                    Err(why) => {
                        self.message = Some(why.clone());
                        Vec::new()
                    }
                };
            }
            Key::Escape => {
                self.message = None;
                self.rows_with(|s| s.layer = None);
            }
            _ => {}
        }
        Vec::new()
    }

    /// A click on the overview: on a box, highlights it, and on the
    /// highlighted box, opens it.
    pub(crate) fn rows_click(&mut self, col: usize, row: usize) -> Vec<Effect> {
        let Some(state) = self.rows_flow.clone() else {
            return Vec::new();
        };
        if state.layer.is_some() {
            return Vec::new();
        }
        let (top, height) = rowsdraw::window(self);
        if row < top || row >= top.saturating_add(height) {
            return Vec::new();
        }
        let y = state.top.saturating_add(row.saturating_sub(top));
        let layout = rows::layout(&self.inputs.flow, self.cols);
        let hit = layout
            .boxes
            .iter()
            .find(|b| col >= b.x && col < b.right() && y >= b.y && y < b.bottom())
            .map(|b| b.obj.clone());
        match hit {
            Some(o) if state.at.as_ref() == Some(&o) => self.open_layer(Some(o)),
            Some(o) => self.rows_with(|s| s.go(&layout, o, height)),
            None => {}
        }
        Vec::new()
    }
}
