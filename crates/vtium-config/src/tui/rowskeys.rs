//! The keys of the Flow view as paths ([`super::rows`]), as the state
//! machine's. The arrows move the highlight from box to box, and PgUp and
//! PgDn scroll by a window. Space picks a piece into the editor or takes
//! it out, `a` adds the editor's path to the document, `e` revises an
//! added path, which moves it back into the editor, and Esc clears the
//! editor. Enter opens a box's layer: an added terminal's card of the
//! settings that do not change its pieces, a piece's narrowing choices,
//! the editor terminal's name and font set, or a font set's preview in the
//! Fonts view. `v` shows the draft's vtium.conf as text, read-only, in the
//! pager, where `s` and `i` save and install as they do on the paths,
//! and Esc there comes back with the same box highlighted.
//!
//! Every change to the draft goes through [`super::paths::plan`] and
//! [`App::make`], so through `src/ops.rs`.

use super::flow::Item;
use super::input::Key;
use super::model::{App, Effect, Live, View, step};
use super::paths::{self, Adding, Doc, Path, Picked, Piece};
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
        let layout = rowsdraw::layout_of(self);
        self.rows_with(|s| {
            if let Some(o @ Obj::Doc(..)) = &s.layer
                && layout.find(o).is_none()
            {
                s.layer = None;
                s.typing = None;
            }
        });
        let (_, height) = rowsdraw::window(self);
        self.rows_with(|s| s.settle(&layout, height));
    }

    /// Whether the editor terminal's name is being typed, so every key
    /// goes to it.
    pub(crate) fn rows_typing(&self) -> bool {
        self.view == View::Flow && self.rows_flow.as_ref().is_some_and(|s| s.typing.is_some())
    }

    /// A key in the Flow view as paths.
    pub(crate) fn rows_key(&mut self, key: Key) -> Vec<Effect> {
        self.rows_with(|s| s.note = None);
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
        // the file view saves and installs as the paths do, and stays open
        if self.pager.is_some() {
            match key {
                Key::Char('s') => return self.tell(words::ROWS_SAVE),
                Key::Char('i') => return self.action('i'),
                _ => {}
            }
        }
        if self.route.is_some() || self.pager.is_some() || self.restore.is_some() {
            return self.flow_key(key);
        }
        match &state.layer {
            Some(obj @ (Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal))) => {
                self.terminal_card_key(&state, obj, key)
            }
            Some(obj @ (Obj::Doc(..) | Obj::Roam(_))) => match key {
                Key::Char('e') => self.revise(obj),
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
                Some(obj @ (Obj::Doc(..) | Obj::Roam(_))) => self.revise(obj),
                _ => self.tell(words::REVISE_WHERE),
            },
            Key::Escape if !state.new.is_empty() => self.clear(state),
            Key::Char('p') => self.tell(match self.live {
                Live::On => words::PREVIEW_PATH_LATER,
                _ => words::PREVIEW_PATH_NOT_HERE,
            }),
            Key::Char('t') => self.tell(match self.live {
                Live::On => words::LIVE_ON,
                Live::OpsOff => words::OPS_OFF,
                _ => words::NOT_VTIUM,
            }),
            // the file, with the highlighted box's line marked
            Key::Char('v') => {
                if let Some(item) = at.as_ref().and_then(|o| self.doc_item(o)) {
                    self.flow_at = item;
                }
                self.opened = false;
                self.action('v')
            }
            Key::Char('s') => self.tell(words::ROWS_SAVE),
            Key::Char(c @ ('i' | 'd' | 'r' | 'o' | 'n' | 'x')) => self.action(c),
            _ => Vec::new(),
        }
    }

    /// The flow's item an added path's box stands for, for the route card.
    fn doc_item(&self, obj: &Obj) -> Option<Item> {
        let flow = &self.inputs.flow;
        if let Obj::Roam(slot) = obj {
            let roams = |sel: &str| {
                flow.rules.iter().position(|r| {
                    r.target == "roaming" && paths::expand(sel).contains(&r.selector.as_str())
                })
            };
            return match slot {
                Slot::Input(sel) => roams(sel),
                _ => flow.rules.iter().position(|r| r.target == "roaming"),
            }
            .map(Item::Rule);
        }
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
                .position(|r| paths::expand(sel).contains(&r.selector.as_str()))
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
        let active = self.rows_flow.as_ref().is_some_and(|s| s.new.active);
        match &obj {
            Obj::Roam(Slot::Terminal) | Obj::Pal(Slot::Terminal) => self.tell(words::ACTIVE_ABOUT),
            Obj::New(Slot::Terminal) if active => self.tell(words::ACTIVE_ABOUT),
            Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal) => {
                let pick = self.card_pick(&obj, 0);
                self.rows_with(|s| {
                    s.layer = Some(obj.clone());
                    s.line = 0;
                    s.pick = pick;
                });
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
        let doc = rowsdraw::shown_doc(self, paths::read(flow));
        let Some(obj) = at else {
            return Vec::new();
        };
        match &obj {
            Obj::Pal(Slot::Display(_)) if state.new.active => self.tell(words::ACTIVE_NO_DISPLAY),
            Obj::Pal(Slot::Terminal) if !state.new.active && !state.new.displays.is_empty() => {
                self.tell(words::ACTIVE_NO_DISPLAY)
            }
            Obj::Pal(Slot::Terminal) => self.pick(Piece::Active),
            Obj::Pal(Slot::Display(sel)) => {
                let held = flow
                    .displays
                    .iter()
                    .find(|d| d.keys.contains(sel))
                    .map(|d| rows::holders(&doc.paths, super::flow::Kind::Display, d))
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
            Obj::Roam(Slot::Input(_)) => self.tell(words::IN_ROAMING),
            Obj::Roam(_) => self.tell(words::IN_DOCUMENT),
            _ => Vec::new(),
        }
    }

    /// Picks `piece` into the editor, or takes it out, and says what that
    /// did, whether the input will roam, and what the path needs next.
    fn pick(&mut self, piece: Piece) -> Vec<Effect> {
        let doc = rowsdraw::shown_doc(self, paths::read(&self.inputs.flow));
        let flow = &self.inputs.flow;
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
            _ => state.new.pick_part(flow, piece.clone()),
        };
        let new = state.new.clone();
        let (kind, sel) = match &piece {
            Piece::Display(s) => (super::flow::Kind::Display, s.as_str()),
            Piece::Input(s) => (super::flow::Kind::Input, s.as_str()),
            Piece::Active => {
                return self.tell(match done {
                    Picked::Removed => words::ACTIVE_UNPICKED,
                    _ => words::ACTIVE_PICKED,
                });
            }
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
        let wide = sel == "*" || sel.starts_with("class:");
        let title = flow
            .devices(kind)
            .iter()
            .find(|d| !wide && d.found && d.keys.iter().any(|k| k == sel))
            .map_or_else(
                || paths::title(flow, kind, sel).0,
                |d| format!("the {}", d.name),
            );
        if done == Picked::Removed {
            return self.tell(words::piece_out(&title));
        }
        let display = kind == super::flow::Kind::Display;
        let roams = !display && (new.active || doc.would_roam(sel));
        let pointers = done == Picked::AddedWithPointers;
        self.tell(words::picked(&title, display, &new, pointers, roams))
    }

    /// `a`: adds the editor's path to the configuration, or says why it
    /// stays. A path being revised is still in the draft, so adding it
    /// back unchanged changes nothing.
    fn add(&mut self, state: &State) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let doc = rowsdraw::shown_doc(self, paths::read(flow));
        let mut new = state.new.clone();
        if new.name.is_none() && !new.active {
            new.name = Some(paths::new_name(flow, &doc));
        }
        let name = new.name.clone().unwrap_or_default();
        let (changes, said, want) = match paths::check_add(flow, &doc, &new) {
            Err(why) => return self.tell(why),
            // what is held already: a path being revised that is held
            // twice leaves the configuration
            Ok(Adding::Same(there)) => (paths::plan(flow, &doc), words::same_path(&there), None),
            Ok(Adding::SameActive) => {
                (paths::plan(flow, &doc), words::SAME_ACTIVE.to_owned(), None)
            }
            Ok(Adding::New) => {
                let want = if new.active {
                    Obj::Roam(Slot::Terminal)
                } else {
                    Obj::Doc(name.clone(), Slot::Terminal)
                };
                let said = if new.active {
                    words::ACTIVE_BACK.to_owned()
                } else {
                    words::path_added(&name)
                };
                (paths::plan(flow, &doc.with(new)), said, Some(want))
            }
        };
        let out = if changes.is_empty() {
            Vec::new()
        } else {
            let out = self.make(changes, None);
            if out.is_empty() {
                return out;
            }
            out
        };
        self.rows_with(|s| {
            s.new = Path::default();
            s.revising = None;
            s.want.clone_from(&want);
            if let Some(w) = want {
                s.at = Some(w);
            }
        });
        self.message = Some(said);
        out
    }

    /// Esc on NEW PATH: clears it. A path being revised leaves the
    /// configuration then, as revising it said it would.
    fn clear(&mut self, state: &State) -> Vec<Effect> {
        let out = match &state.revising {
            Some(_) => {
                let doc = rowsdraw::shown_doc(self, paths::read(&self.inputs.flow));
                let changes = paths::plan(&self.inputs.flow, &doc);
                let out = self.make(changes, None);
                if out.is_empty() {
                    return out;
                }
                self.message = Some(words::REVISED_DROPPED.to_owned());
                out
            }
            None => {
                self.message = Some(words::EDITOR_CLEARED.to_owned());
                Vec::new()
            }
        };
        self.rows_with(|s| {
            s.new = Path::default();
            s.revising = None;
        });
        out
    }

    /// `e`: revises the path of a terminal, or the path of the active
    /// terminal, the roaming inputs: it goes into NEW PATH with its parts
    /// picked, and CONFIGURATION leaves it out. The draft keeps it until
    /// NEW PATH is added, which puts it back as it was when nothing in it
    /// changed, or cleared, which takes it out.
    fn revise(&mut self, obj: &Obj) -> Vec<Effect> {
        let doc: Doc = paths::read(&self.inputs.flow);
        if self.rows_flow.as_ref().is_some_and(|s| !s.new.is_empty()) {
            return self.tell(words::EDITOR_BUSY);
        }
        let (taken, said, revising, at) = match obj {
            Obj::Doc(name, _) => (
                doc.revise(name),
                words::path_revised(name),
                rows::Revising::Path(name.clone()),
                Obj::New(Slot::Terminal),
            ),
            Obj::Roam(_) => (
                doc.revise_active(),
                words::ACTIVE_REVISED.to_owned(),
                rows::Revising::Active,
                Obj::New(Slot::Terminal),
            ),
            _ => return Vec::new(),
        };
        let Some((_, path)) = taken else {
            return Vec::new();
        };
        self.rows_with(|s| {
            s.new = path;
            s.revising = Some(revising);
            s.layer = None;
            s.at = Some(at);
        });
        self.tell(said)
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

    /// The choice a terminal card's line `line` starts on: the one the
    /// path holds now.
    fn card_pick(&self, obj: &Obj, line: usize) -> usize {
        let Some(path) = rowsdraw::card_path(self, obj) else {
            return 0;
        };
        let lines = rowsdraw::card_lines(&path, matches!(obj, Obj::New(_)));
        let Some(l) = lines.get(line) else {
            return 0;
        };
        let value = rowsdraw::card_value(&path, l);
        rowsdraw::card_choices(self, l)
            .iter()
            .position(|(_, v)| *v == value)
            .unwrap_or(0)
    }

    /// A key on a terminal's card, an added path's or the editor's: Up and
    /// Down move between its lines, Left and Right between a line's
    /// choices, Enter sets the choice or starts typing the name or the
    /// login, and Esc goes back. `e` on an added path's card revises it.
    fn terminal_card_key(&mut self, state: &State, obj: &Obj, key: Key) -> Vec<Effect> {
        let Some(path) = rowsdraw::card_path(self, obj) else {
            self.rows_with(|s| s.layer = None);
            return Vec::new();
        };
        let lines = rowsdraw::card_lines(&path, matches!(obj, Obj::New(_)));
        let Some(line) = lines.get(state.line).cloned() else {
            self.rows_with(|s| s.line = 0);
            return Vec::new();
        };
        if let Some(typed) = state.typing.clone() {
            return self.card_type_key(obj, &path, &line, typed, key);
        }
        let choices = rowsdraw::card_choices(self, &line);
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let at = step(state.line, lines.len(), key);
                let pick = self.card_pick(obj, at);
                self.rows_with(|s| {
                    s.line = at;
                    s.pick = pick;
                });
                Vec::new()
            }
            Key::Left => {
                self.rows_with(|s| s.pick = s.pick.saturating_sub(1));
                Vec::new()
            }
            Key::Right => {
                let last = choices.len().saturating_sub(1);
                self.rows_with(|s| s.pick = s.pick.saturating_add(1).min(last));
                Vec::new()
            }
            Key::Enter if choices.is_empty() => {
                let now = rowsdraw::card_value(&path, &line);
                self.rows_with(|s| s.typing = Some(now));
                Vec::new()
            }
            Key::Enter => {
                let value = choices
                    .get(state.pick)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                self.card_apply(obj, &path, &line, value)
            }
            Key::Char('e') => match obj {
                Obj::Doc(..) => self.revise(obj),
                _ => Vec::new(),
            },
            Key::Escape => {
                self.rows_with(|s| s.layer = None);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A key while a card's name or login is typed: characters go into
    /// it, Enter keeps it and Esc drops it.
    fn card_type_key(
        &mut self,
        obj: &Obj,
        path: &Path,
        line: &rowsdraw::CardLine,
        mut typed: String,
        key: Key,
    ) -> Vec<Effect> {
        match key {
            Key::Char(c) if typed.chars().count() < 64 => {
                typed.push(c);
                self.rows_with(|s| s.typing = Some(typed));
                Vec::new()
            }
            Key::Backspace => {
                typed.pop();
                self.rows_with(|s| s.typing = Some(typed));
                Vec::new()
            }
            Key::Enter => self.card_apply(obj, path, line, typed),
            Key::Escape => {
                self.rows_with(|s| s.typing = None);
                self.message = None;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Sets the card's line `line` to `value`: in the editor, on the path
    /// it holds; on an added path, through the draft's changes, which
    /// `paths::plan` makes from the configuration with the path changed.
    /// A name another terminal has, or one a terminal cannot take, is
    /// refused in one sentence.
    fn card_apply(
        &mut self,
        obj: &Obj,
        path: &Path,
        line: &rowsdraw::CardLine,
        value: String,
    ) -> Vec<Effect> {
        use rowsdraw::CardLine;
        let flow = &self.inputs.flow;
        let mut doc = paths::read(flow);
        let own = path.name.clone().unwrap_or_default();
        if *line == CardLine::Name && value != own {
            let taken = doc.path(&value).is_some() || flow.heads.iter().any(|h| h.name == value);
            let why = if !paths::valid_name(&value) {
                Some(words::name_invalid(&value))
            } else if taken {
                Some(words::name_taken(&value))
            } else {
                None
            };
            if let Some(why) = why {
                return self.tell(why);
            }
        }
        let mut changed = path.clone();
        match line {
            CardLine::Name => changed.name = Some(value.clone()),
            CardLine::Font => changed.font = Some(value.clone()).filter(|v| !v.is_empty()),
            CardLine::Size => changed.size = value.parse().ok(),
            CardLine::Setting(key) => changed.set(key, &value),
            CardLine::Part(piece) => {
                changed.pick(piece.clone());
            }
        }
        self.rows_with(|s| s.typing = None);
        self.message = None;
        match obj {
            Obj::New(_) => {
                if matches!(line, CardLine::Part(_)) {
                    self.rows_with(|s| s.line = s.line.saturating_sub(1));
                }
                self.rows_with(|s| s.new = changed);
                Vec::new()
            }
            _ => {
                let Some(at) = doc
                    .paths
                    .iter()
                    .position(|p| p.name.as_deref() == Some(own.as_str()))
                else {
                    return Vec::new();
                };
                if let Some(slot) = doc.paths.get_mut(at) {
                    *slot = changed.clone();
                }
                let changes = paths::plan(&self.inputs.flow, &doc);
                if changes.is_empty() {
                    return Vec::new();
                }
                let out = self.make(changes, None);
                if !out.is_empty() {
                    let name = changed.name.clone().unwrap_or_else(|| own.clone());
                    self.message = Some(words::changed_in_draft(&name));
                }
                if !out.is_empty() && *line == CardLine::Name {
                    let renamed = Obj::Doc(value, Slot::Terminal);
                    self.rows_with(|s| {
                        s.layer = Some(renamed.clone());
                        s.want = Some(renamed);
                    });
                }
                out
            }
        }
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
