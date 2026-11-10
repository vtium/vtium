//! The keys of the Flow view ([`super::flowview`]), as the state
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
use super::flowdraw;
use super::flowview::{self, Obj, Slot, State};
use super::input::Key;
use super::model::{App, Effect, Live, View, step};
use super::parts::{self, Make, Pick, Stage};
use super::paths::{self, Adding, Doc, Path, Picked, Piece};
use super::words;

impl App {
    /// The Flow view's state, laid out again after every event: the highlight
    /// back on its object, the window where it keeps that object at the
    /// same height, and a layer closed when its object is gone.
    pub(crate) fn flow_settle(&mut self) {
        // the parts, enumerated again and merged by their matches, so a
        // device that comes or goes after any event shows at once
        let flow = &self.inputs.flow;
        let now = parts::enumerate(flow, &paths::read(flow));
        parts::merge(&mut self.flow_view.parts, flow, now);
        let layout = flowdraw::layout_of(self);
        self.flow_with(|s| {
            if let Some(o @ Obj::Doc(..)) = &s.layer
                && layout.find(o).is_none()
            {
                s.layer = None;
                s.typing = None;
            }
        });
        let (_, height) = flowdraw::window(self);
        self.flow_with(|s| s.settle(&layout, height));
    }

    /// Whether the editor terminal's name is being typed, so every key
    /// goes to it.
    pub(crate) fn flow_typing(&self) -> bool {
        self.view == View::Flow && self.flow_view.typing.is_some()
    }

    /// A key in the Flow view.
    pub(crate) fn flow_key(&mut self, key: Key) -> Vec<Effect> {
        let state = self.flow_view.clone();
        // the file view saves and installs as the paths do, and stays open
        if self.pager.is_some() {
            match key {
                Key::Char('s') => return self.save(),
                Key::Char('i') => return self.action('i'),
                _ => {}
            }
        }
        if self.confirm.is_some() || self.pager.is_some() || self.restore.is_some() {
            return self.flow_open_key(key);
        }
        match &state.layer {
            Some(obj @ (Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal))) => {
                self.terminal_card_key(&state, obj, key)
            }
            Some(Obj::Pal(Slot::Template(st)) | Obj::New(Slot::Empty(st))) => {
                let from_new = matches!(state.layer, Some(Obj::New(_)));
                self.make_key(&state, *st, from_new, key)
            }
            Some(obj @ (Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_))) => match key {
                Key::Char('e') => self.revise(obj),
                Key::Escape => {
                    self.flow_with(|s| s.layer = None);
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Some(Obj::Others) => {
                if key == Key::Escape {
                    self.flow_with(|s| s.layer = None);
                }
                Vec::new()
            }
            Some(obj) => self.choice_key(&state, obj, key),
            None => self.overview_key(&state, key),
        }
    }

    fn flow_with(&mut self, f: impl FnOnce(&mut State)) {
        f(&mut self.flow_view);
    }

    fn tell(&mut self, what: impl Into<String>) -> Vec<Effect> {
        self.message = Some(what.into());
        Vec::new()
    }

    /// A key on the document.
    fn overview_key(&mut self, state: &State, key: Key) -> Vec<Effect> {
        let layout = flowdraw::layout_of(self);
        let (_, height) = flowdraw::window(self);
        let at = state.at.clone();
        if !matches!(key, Key::Char(' ' | 'a' | 'e' | 'p' | 't' | 's')) {
            self.message = None;
        }
        match key {
            Key::Up | Key::Down | Key::Left | Key::Right | Key::Home | Key::End => {
                if let Some(to) = at.as_ref().and_then(|a| flowview::step(&layout, a, key)) {
                    self.flow_with(|s| s.go(&layout, to, height));
                }
                Vec::new()
            }
            Key::PageUp | Key::PageDown => {
                let down = key == Key::PageDown;
                self.flow_with(|s| s.page(&layout, down, height));
                Vec::new()
            }
            Key::Enter => self.open(at),
            Key::Char(' ') => self.space(state, at),
            Key::Char('a') => self.add(state),
            Key::Char('e') => match &at {
                Some(obj @ (Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_))) => self.revise(obj),
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
                self.action('v')
            }
            Key::Char('s') => self.save(),
            Key::Char(c @ ('i' | 'd' | 'r' | 'o' | 'n' | 'x')) => self.action(c),
            _ => Vec::new(),
        }
    }

    /// The flow's item an added path's box stands for, whose line the file
    /// view opens at.
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
        let active = self.flow_view.new.active;
        let none = self.flow_view.new.none;
        match &obj {
            Obj::Roam(Slot::Terminal) => self.tell(words::ACTIVE_ABOUT),
            Obj::New(Slot::Terminal) if active => self.tell(words::ACTIVE_ABOUT),
            Obj::Pal(Slot::Special(p))
            | Obj::Doc(_, Slot::Special(p))
            | Obj::New(Slot::Special(p)) => {
                self.tell(words::special_about(&parts::special_title(p)))
            }
            Obj::Nothing(Slot::Terminal) => self.tell(words::NO_TERMINAL_ABOUT),
            Obj::New(Slot::Terminal) if none => self.tell(words::NO_TERMINAL_ABOUT),
            Obj::Pal(Slot::Template(_)) | Obj::New(Slot::Empty(_)) => {
                self.flow_with(|s| {
                    s.layer = Some(obj.clone());
                    s.line = 0;
                    s.picker = None;
                });
                Vec::new()
            }
            Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal) => {
                let pick = self.card_pick(&obj, 0);
                self.flow_with(|s| {
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
                self.flow_with(|s| {
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
        // the label is the detail's first line, then the choices
        flowdraw::choices_for(self, obj)
            .iter()
            .position(|(_, _, v)| *v == now)
            .map_or(0, |at| at.saturating_add(1))
    }

    /// Space: picks a palette piece into the editor or takes it out, takes
    /// an editor piece out, and says why anything else cannot be picked.
    fn space(&mut self, state: &State, at: Option<Obj>) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let doc = flowdraw::shown_doc(self, paths::read(flow));
        let Some(obj) = at else {
            return Vec::new();
        };
        match &obj {
            Obj::Pal(Slot::Template(st)) => self.tell(words::template_space(st.name())),
            Obj::New(Slot::Empty(_)) => self.tell(words::EMPTY_SPACE),
            Obj::Pal(Slot::Display(_)) if state.new.active => self.tell(words::ACTIVE_NO_DISPLAY),
            Obj::Pal(Slot::Special(Piece::Active))
                if !state.new.active && !state.new.displays.is_empty() =>
            {
                self.tell(words::ACTIVE_NO_DISPLAY)
            }
            Obj::Pal(Slot::Special(Piece::NoInput)) if state.new.active || state.new.none => {
                self.tell(words::NO_INPUT_NEEDS_TERMINAL)
            }
            Obj::Pal(Slot::Special(p)) | Obj::New(Slot::Special(p)) => self.pick(p.clone()),
            Obj::Pal(Slot::Display(sel)) => {
                // a display is in one path for each VT: picking it is
                // refused only when a path that holds it shares a VT with
                // the new path's terminal (ADR-1403 D7)
                let held = flow
                    .displays
                    .iter()
                    .find(|d| d.keys.contains(sel))
                    .map(|d| flowview::holders(&doc.paths, super::flow::Kind::Display, d))
                    .unwrap_or_default();
                let clash = held.iter().find_map(|h| {
                    let vt = doc.path(h).and_then(|p| state.new.shared_vt(p))?;
                    Some((h, vt))
                });
                let picked = state.new.has(&Piece::Display(sel.clone()));
                if let (Some((h, vt)), false) = (clash, picked) {
                    let (title, _) = paths::title(flow, super::flow::Kind::Display, sel);
                    return self.tell(words::display_taken(&title, h, vt));
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
            Obj::Roam(_) | Obj::Nothing(_) => self.tell(words::IN_DOCUMENT),
            _ => Vec::new(),
        }
    }

    /// Picks `piece` into the editor, or takes it out, and says what that
    /// did, whether the input will roam, and what the path needs next.
    fn pick(&mut self, piece: Piece) -> Vec<Effect> {
        let doc = flowdraw::shown_doc(self, paths::read(&self.inputs.flow));
        let flow = &self.inputs.flow;
        let state = &mut self.flow_view;
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
            Piece::NoInput | Piece::NoTerminal | Piece::Terminal(_) => {
                let title = super::parts::special_title(&piece);
                return self.tell(match done {
                    Picked::Removed => words::piece_out(&title),
                    _ => words::special_picked(&title),
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
        if new.none {
            return self.tell(words::picked_aside(&title, display));
        }
        let roams = !display && (new.active || doc.would_roam(sel, &new));
        let pointers = done == Picked::AddedWithPointers;
        self.tell(words::picked(&title, display, &new, pointers, roams))
    }

    /// `a`: adds the editor's path to the configuration, or says why it
    /// stays. A path being revised is still in the draft, so adding it
    /// back unchanged changes nothing.
    fn add(&mut self, state: &State) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let doc = flowdraw::shown_doc(self, paths::read(flow));
        let mut new = state.new.clone();
        if new.name.is_none() && !new.active && !new.none {
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
            Ok(Adding::SameNone) => (paths::plan(flow, &doc), words::SAME_NONE.to_owned(), None),
            Ok(Adding::New) => {
                let (want, said) = if new.active {
                    (Obj::Roam(Slot::Terminal), words::ACTIVE_BACK.to_owned())
                } else if new.none {
                    (Obj::Nothing(Slot::Terminal), words::NONE_BACK.to_owned())
                } else {
                    (
                        Obj::Doc(name.clone(), Slot::Terminal),
                        words::path_added(&name),
                    )
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
        self.flow_with(|s| {
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
                let doc = flowdraw::shown_doc(self, paths::read(&self.inputs.flow));
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
        self.flow_with(|s| {
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
        if !self.flow_view.new.is_empty() {
            return self.tell(words::EDITOR_BUSY);
        }
        let (taken, said, revising, at) = match obj {
            Obj::Doc(name, _) => (
                doc.revise(name),
                words::path_revised(name),
                flowview::Revising::Path(name.clone()),
                Obj::New(Slot::Terminal),
            ),
            Obj::Roam(_) => (
                doc.revise_active(),
                words::ACTIVE_REVISED.to_owned(),
                flowview::Revising::Active,
                Obj::New(Slot::Terminal),
            ),
            Obj::Nothing(_) => (
                doc.revise_none(),
                words::NONE_REVISED.to_owned(),
                flowview::Revising::Nothing,
                Obj::New(Slot::Terminal),
            ),
            _ => return Vec::new(),
        };
        let Some((_, path)) = taken else {
            return Vec::new();
        };
        self.flow_with(|s| {
            s.new = path;
            s.revising = Some(revising);
            s.layer = None;
            s.at = Some(at);
        });
        self.tell(said)
    }

    /// A key on a part's detail, whose first line is the part's label and
    /// whose other lines are its choices: Up and Down move; Enter on the
    /// label starts typing it, and Enter on a choice picks it into the
    /// editor, or puts it in the editor piece's place; Esc goes back.
    fn choice_key(&mut self, state: &State, obj: &Obj, key: Key) -> Vec<Effect> {
        let list = flowdraw::choices_for(self, obj);
        let Some((kind, sel)) = flowdraw::label_target(obj) else {
            self.flow_with(|s| s.layer = None);
            return Vec::new();
        };
        if let Some(typed) = state.typing.clone() {
            return self.label_key(kind, &sel, typed, key);
        }
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let line = step(state.line, list.len().saturating_add(1), key);
                self.flow_with(|s| s.line = line);
                Vec::new()
            }
            Key::Escape => {
                self.flow_with(|s| s.layer = None);
                Vec::new()
            }
            Key::Enter if state.line == 0 => {
                let doc = paths::read(&self.inputs.flow);
                let now = doc.label(kind, &sel).unwrap_or_default().to_owned();
                self.flow_with(|s| s.typing = Some(now));
                Vec::new()
            }
            Key::Enter => {
                let at = state.line.saturating_sub(1);
                let Some((_, _, Some(sel))) = list.get(at).cloned() else {
                    return Vec::new();
                };
                self.flow_with(|s| s.layer = None);
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
                        self.flow_with(|s| {
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

    /// A key while a part's label is typed: characters go into it, up to
    /// the most a label holds; Enter writes it to the draft, and Esc leaves
    /// the label as it was.
    fn label_key(
        &mut self,
        kind: super::flow::Kind,
        sel: &str,
        mut typed: String,
        key: Key,
    ) -> Vec<Effect> {
        match key {
            Key::Char(c) if typed.chars().count() < vtium_settings::LABEL_MAX => {
                typed.push(c);
                self.flow_with(|s| s.typing = Some(typed));
                Vec::new()
            }
            Key::Backspace => {
                typed.pop();
                self.flow_with(|s| s.typing = Some(typed));
                Vec::new()
            }
            Key::Enter => self.set_label(kind, sel, &typed),
            Key::Escape => {
                self.flow_with(|s| s.typing = None);
                self.message = None;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Gives the part of `kind` that matches by `sel` the label `typed`, or
    /// takes its label away when `typed` is blank, through the draft's
    /// changes, which write the `[labels]` section. A label that starts
    /// with `#` is refused in one sentence, since vtium.conf reads it as a
    /// comment.
    fn set_label(&mut self, kind: super::flow::Kind, sel: &str, typed: &str) -> Vec<Effect> {
        let label = typed.trim();
        if label.starts_with('#') {
            return self.tell(words::LABEL_HASH);
        }
        let flow = &self.inputs.flow;
        let mut doc = paths::read(flow);
        doc.set_label(kind, sel, label);
        let changes = paths::plan(flow, &doc);
        self.flow_with(|s| s.typing = None);
        if changes.is_empty() {
            return Vec::new();
        }
        let out = self.make(changes, None);
        if !out.is_empty() {
            self.message = Some(if label.is_empty() {
                words::LABEL_REMOVED.to_owned()
            } else {
                words::label_set(label)
            });
        }
        out
    }

    /// The choice a terminal card's line `line` starts on: the one the
    /// path holds now.
    fn card_pick(&self, obj: &Obj, line: usize) -> usize {
        let Some(path) = flowdraw::card_path(self, obj) else {
            return 0;
        };
        let lines = flowdraw::card_lines(&path, matches!(obj, Obj::New(_)));
        let Some(l) = lines.get(line) else {
            return 0;
        };
        let value = flowdraw::card_value(&path, l);
        flowdraw::card_choices(self, l)
            .iter()
            .position(|(_, v)| *v == value)
            .unwrap_or(0)
    }

    /// A key on a terminal's card, an added path's or the editor's: Up and
    /// Down move between its lines, Left and Right between a line's
    /// choices, Enter sets the choice or starts typing the name or the
    /// login, and Esc goes back. `e` on an added path's card revises it.
    fn terminal_card_key(&mut self, state: &State, obj: &Obj, key: Key) -> Vec<Effect> {
        let Some(path) = flowdraw::card_path(self, obj) else {
            self.flow_with(|s| s.layer = None);
            return Vec::new();
        };
        let lines = flowdraw::card_lines(&path, matches!(obj, Obj::New(_)));
        let Some(line) = lines.get(state.line).cloned() else {
            self.flow_with(|s| s.line = 0);
            return Vec::new();
        };
        if let Some(typed) = state.typing.clone() {
            return self.card_type_key(obj, &path, &line, typed, key);
        }
        let choices = flowdraw::card_choices(self, &line);
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let at = step(state.line, lines.len(), key);
                let pick = self.card_pick(obj, at);
                self.flow_with(|s| {
                    s.line = at;
                    s.pick = pick;
                });
                Vec::new()
            }
            Key::Left => {
                self.flow_with(|s| s.pick = s.pick.saturating_sub(1));
                Vec::new()
            }
            Key::Right => {
                let last = choices.len().saturating_sub(1);
                self.flow_with(|s| s.pick = s.pick.saturating_add(1).min(last));
                Vec::new()
            }
            Key::Enter if choices.is_empty() => {
                let now = flowdraw::card_value(&path, &line);
                self.flow_with(|s| s.typing = Some(now));
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
                self.flow_with(|s| s.layer = None);
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
        line: &flowdraw::CardLine,
        mut typed: String,
        key: Key,
    ) -> Vec<Effect> {
        match key {
            Key::Char(c) if typed.chars().count() < 64 => {
                typed.push(c);
                self.flow_with(|s| s.typing = Some(typed));
                Vec::new()
            }
            Key::Backspace => {
                typed.pop();
                self.flow_with(|s| s.typing = Some(typed));
                Vec::new()
            }
            Key::Enter => self.card_apply(obj, path, line, typed),
            Key::Escape => {
                self.flow_with(|s| s.typing = None);
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
        line: &flowdraw::CardLine,
        value: String,
    ) -> Vec<Effect> {
        use flowdraw::CardLine;
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
        self.flow_with(|s| s.typing = None);
        self.message = None;
        match obj {
            Obj::New(_) => {
                if matches!(line, CardLine::Part(_)) {
                    self.flow_with(|s| s.line = s.line.saturating_sub(1));
                }
                self.flow_with(|s| s.new = changed);
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
                    self.flow_with(|s| {
                        s.layer = Some(renamed.clone());
                        s.want = Some(renamed);
                    });
                }
                out
            }
        }
    }

    /// A key on a template's layer, the ways to make a part of `stage`,
    /// or on a filter's picker over it: Up and Down move, Enter makes the
    /// part, opens the picker or starts typing a value, and Esc goes back.
    fn make_key(&mut self, state: &State, stage: Stage, from_new: bool, key: Key) -> Vec<Effect> {
        if let Some(typed) = state.typing.clone() {
            return self.typed_key(stage, from_new, typed, key);
        }
        let fonts = flowdraw::font_names(self);
        let length = match &state.picker {
            Some(p) => parts::picker(&self.inputs.flow, p.stage, p.key).len(),
            None => parts::makes(stage, &fonts).len(),
        };
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let line = step(state.line, length, key);
                self.flow_with(|s| s.line = line);
                Vec::new()
            }
            Key::Escape if state.picker.is_some() => {
                let back = parts::makes(stage, &fonts)
                    .iter()
                    .position(|(_, _, m)| {
                        state
                            .picker
                            .as_ref()
                            .is_some_and(|p| *m == Make::Filter(p.key))
                    })
                    .unwrap_or(0);
                self.flow_with(|s| {
                    s.picker = None;
                    s.line = back;
                });
                Vec::new()
            }
            Key::Escape => {
                self.flow_with(|s| s.layer = None);
                Vec::new()
            }
            Key::Enter => match &state.picker {
                Some(p) => match parts::picker(&self.inputs.flow, p.stage, p.key).get(state.line) {
                    Some(Pick::Value { sel, .. }) => {
                        let piece = match stage {
                            Stage::Display => Piece::Display(sel.clone()),
                            _ => Piece::Input(sel.clone()),
                        };
                        let title = parts::title_of(&self.inputs.flow, &piece);
                        self.make_part(piece, title, from_new)
                    }
                    Some(Pick::Typed) => {
                        self.flow_with(|s| s.typing = Some(String::new()));
                        Vec::new()
                    }
                    None => Vec::new(),
                },
                None => match parts::makes(stage, &fonts)
                    .get(state.line)
                    .map(|(_, _, m)| m.clone())
                {
                    Some(Make::Part(piece)) => {
                        let title = parts::title_of(&self.inputs.flow, &piece);
                        self.make_part(piece, title, from_new)
                    }
                    Some(Make::Filter(key)) => {
                        self.flow_with(|s| {
                            s.picker = Some(flowview::Picker {
                                stage,
                                key,
                                from_new,
                            });
                            s.line = 0;
                        });
                        Vec::new()
                    }
                    Some(Make::NewTerminal) => self.new_terminal(from_new),
                    None => Vec::new(),
                },
            },
            _ => Vec::new(),
        }
    }

    /// A key while a value is typed on a filter's picker: characters go
    /// into it, Enter checks it as vtium.conf checks a rule's match and
    /// makes the part, or says what is wrong, and Esc drops it.
    fn typed_key(
        &mut self,
        stage: Stage,
        from_new: bool,
        mut typed: String,
        key: Key,
    ) -> Vec<Effect> {
        match key {
            Key::Char(c) if typed.chars().count() < 200 => {
                typed.push(c);
                self.flow_with(|s| s.typing = Some(typed));
                Vec::new()
            }
            Key::Backspace => {
                typed.pop();
                self.flow_with(|s| s.typing = Some(typed));
                Vec::new()
            }
            Key::Enter => {
                let key = self.flow_view.picker.as_ref().map(|p| p.key);
                // a value typed without a match key takes the picker's key
                let bare = typed.trim();
                let keyed = vtium_settings::Token::KEYS
                    .iter()
                    .any(|k| bare.strip_prefix(k).is_some_and(|r| r.starts_with(':')));
                let typed = match key {
                    Some(k) if !keyed && !bare.is_empty() && bare != "*" => format!("{k}:{bare}"),
                    _ => typed,
                };
                match parts::check_typed(stage, &typed) {
                    Ok(sel) => {
                        let piece = match stage {
                            Stage::Display => Piece::Display(sel.clone()),
                            _ => Piece::Input(sel.clone()),
                        };
                        self.make_part(piece, sel, from_new)
                    }
                    // the parser's sentence names a rule; the reader typed a
                    // value of the picker's kind, so the refusal names that
                    Err(_) => self.tell(words::typed_refused(key.unwrap_or("path"))),
                }
            }
            Key::Escape => {
                self.flow_with(|s| s.typing = None);
                self.message = None;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// "new terminal": makes a terminal part with the next free name. From
    /// NEW PATH it also goes into the new path, and the new path's
    /// terminal card opens, where its name, font set and size are set.
    fn new_terminal(&mut self, from_new: bool) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let doc = flowdraw::shown_doc(self, paths::read(flow));
        let made: Vec<String> = self
            .flow_view
            .parts
            .iter()
            .filter_map(|p| match &p.piece {
                Piece::Terminal(n) => Some(n.clone()),
                _ => None,
            })
            .collect();
        let name = paths::new_name_besides(flow, &doc, &made);
        let out = self.make_part(Piece::Terminal(name.clone()), name, from_new);
        if from_new {
            let obj = Obj::New(Slot::Terminal);
            let pick = self.card_pick(&obj, 0);
            self.flow_with(|s| {
                s.layer = Some(obj);
                s.line = 0;
                s.pick = pick;
            });
        }
        out
    }

    /// Makes the part `piece`, named `title`, and adds it to PARTS, or
    /// finds the part PARTS holds with the same match. From NEW PATH the
    /// part also goes into the new path, and the highlight moves to it
    /// there; else the highlight moves to it in PARTS.
    fn make_part(&mut self, piece: Piece, title: String, from_new: bool) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        let mut name = title.clone();
        let there = {
            let s = &mut self.flow_view;
            let there = s.new.has(&piece);
            let at = parts::make(&mut s.parts, flow, piece.clone(), title);
            if let Some(p) = s.parts.get(at) {
                name.clone_from(&p.title);
            }
            s.layer = None;
            s.picker = None;
            s.typing = None;
            there
        };
        if !from_new {
            let at = Obj::Pal(Slot::of(&piece));
            self.flow_with(|s| {
                s.want = Some(at.clone());
                s.at = Some(at);
            });
            return self.tell(words::part_made(&name));
        }
        let at = match &piece {
            Piece::Display(s) => Obj::New(Slot::Display(s.clone())),
            Piece::Input(s) => Obj::New(Slot::Input(s.clone())),
            Piece::NoInput => Obj::New(Slot::Special(Piece::NoInput)),
            _ => Obj::New(Slot::Terminal),
        };
        let out = if there { Vec::new() } else { self.pick(piece) };
        self.flow_with(|s| {
            s.want = Some(at.clone());
            s.at = Some(at);
        });
        out
    }

    /// A click on the document: on a box, highlights it, and on the
    /// highlighted box, opens it.
    pub(crate) fn flow_click(&mut self, col: usize, row: usize) -> Vec<Effect> {
        let state = self.flow_view.clone();
        if state.layer.is_some() || state.typing.is_some() {
            return Vec::new();
        }
        let (top, height) = flowdraw::window(self);
        if row < top || row >= top.saturating_add(height) {
            return Vec::new();
        }
        let y = state.top.saturating_add(row.saturating_sub(top));
        let layout = flowdraw::layout_of(self);
        let hit = layout
            .boxes
            .iter()
            .find(|b| col >= b.x && col < b.right() && y >= b.y && y < b.bottom())
            .map(|b| b.obj.clone());
        match hit {
            Some(o) if state.at.as_ref() == Some(&o) => self.open(Some(o)),
            Some(o) => {
                self.flow_with(|s| s.go(&layout, o, height));
                Vec::new()
            }
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::super::input::Input;
    use super::super::model::Event;
    use super::super::paths::tests::{PORT_33, flow};
    use super::*;

    /// The Flow view on the test flow, at 120×32.
    fn app() -> App {
        let mut i = super::super::fixture::inputs();
        i.view = View::Flow;
        i.flow = flow();
        let (mut app, _) = App::new(i, 120, 32);
        app.flow_settle();
        app
    }

    fn keys(app: &mut App, keys: &[Key]) {
        for k in keys {
            app.event(Event::Input(Input::Key(*k)));
        }
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            keys(app, &[Key::Char(c)]);
        }
    }

    fn state(app: &App) -> &State {
        &app.flow_view
    }

    fn highlight(app: &mut App, obj: Obj) {
        app.flow_with(|s| s.at = Some(obj));
    }

    #[test]
    fn a_template_refuses_to_be_placed_and_enter_makes_a_part_from_it() {
        let mut app = app();
        let template = Obj::Pal(Slot::Template(Stage::Input));
        highlight(&mut app, template.clone());
        keys(&mut app, &[Key::Char(' ')]);
        assert_eq!(
            app.message.as_deref(),
            Some("The Input template adds inputs. Press Enter to add one.")
        );
        assert!(
            state(&app).new.is_empty(),
            "the template stays out of the new path"
        );
        // Enter opens the ways to make an input part; "by USB port" opens
        // its picker, whose first value is the receiver's port
        keys(&mut app, &[Key::Enter]);
        assert_eq!(state(&app).layer, Some(template.clone()));
        keys(
            &mut app,
            &[Key::Down, Key::Down, Key::Down, Key::Down, Key::Enter],
        );
        let picker = state(&app).picker.clone().expect("the picker");
        assert_eq!(
            (picker.stage, picker.key, picker.from_new),
            (Stage::Input, "path", false)
        );
        keys(&mut app, &[Key::Enter]);
        // the part joins PARTS, highlighted, and stays out of the new path
        let made = Piece::Input(PORT_33.into());
        let part = state(&app)
            .parts
            .iter()
            .find(|p| p.piece == made)
            .expect("the new part");
        assert!(part.made && part.connected);
        assert_eq!(part.title, "inputs on USB 3.3");
        assert_eq!(state(&app).at, Some(Obj::Pal(Slot::Input(PORT_33.into()))));
        assert!(state(&app).new.is_empty() && state(&app).layer.is_none());
    }

    #[test]
    fn a_part_made_from_an_empty_place_in_new_path_is_picked_into_it() {
        let mut app = app();
        highlight(&mut app, Obj::New(Slot::Empty(Stage::Input)));
        keys(&mut app, &[Key::Char(' ')]);
        assert_eq!(app.message.as_deref(), Some(words::EMPTY_SPACE));
        // no input, the first choice
        keys(&mut app, &[Key::Enter, Key::Enter]);
        assert!(state(&app).new.no_input);
        assert!(state(&app).parts.iter().any(|p| p.piece == Piece::NoInput));
        assert_eq!(
            state(&app).at,
            Some(Obj::New(Slot::Special(Piece::NoInput)))
        );
        // a value typed on the picker: refused with the parser's words,
        // then made from the value alone, which takes the picker's key
        highlight(&mut app, Obj::New(Slot::Empty(Stage::Display)));
        keys(
            &mut app,
            &[Key::Enter, Key::Down, Key::Enter, Key::End, Key::Enter],
        );
        assert_eq!(state(&app).typing.as_deref(), Some(""));
        type_text(&mut app, "aaa:1:2");
        keys(&mut app, &[Key::Enter]);
        let why = app.message.clone().unwrap_or_default();
        assert_eq!(
            why,
            "This monitor is not a valid EDID. An EDID looks like AAA:0x0001:LARGE00001."
        );
        assert!(state(&app).typing.is_some(), "the value stays to fix");
        keys(&mut app, &[Key::Escape, Key::Enter]);
        type_text(&mut app, "AAA:0x0001:LARGE00001");
        keys(&mut app, &[Key::Enter]);
        let edid = "edid:AAA:0x0001:LARGE00001".to_owned();
        assert_eq!(state(&app).new.displays, std::slice::from_ref(&edid));
        assert_eq!(state(&app).at, Some(Obj::New(Slot::Display(edid))));
    }
    #[test]
    fn a_part_made_from_a_template_choice_carries_a_label() {
        let mut app = app();
        app.draft = Some(super::super::desk::Opened {
            name: "north".into(),
            text: super::super::paths::tests::TEXT.into(),
            ..Default::default()
        });
        // every keyboard, the Input template's second choice
        highlight(&mut app, Obj::Pal(Slot::Template(Stage::Input)));
        keys(&mut app, &[Key::Enter, Key::Down, Key::Enter]);
        let every = Obj::Pal(Slot::Input(paths::ANY_KEYBOARD.into()));
        assert_eq!(state(&app).at, Some(every.clone()));
        // Enter opens its detail, whose first line is the label
        keys(&mut app, &[Key::Enter]);
        assert_eq!(state(&app).layer, Some(every.clone()));
        assert_eq!(state(&app).line, 0);
        keys(&mut app, &[Key::Enter]);
        assert_eq!(state(&app).typing.as_deref(), Some(""));
        type_text(&mut app, "Any board #1");
        let effects = app.event(Event::Input(Input::Key(Key::Enter)));
        let written = effects
            .iter()
            .find_map(|e| match e {
                Effect::Desk(super::super::desk::Op::Check(t)) => Some(t.clone()),
                _ => None,
            })
            .expect("the draft written");
        assert!(
            written.ends_with("[labels]\ninput class:keyboard = Any board \\#1\n"),
            "{written}"
        );
        assert!(state(&app).typing.is_none());
        // a label that starts with # is refused in one sentence
        keys(&mut app, &[Key::Enter]);
        type_text(&mut app, "#2");
        keys(&mut app, &[Key::Enter]);
        assert_eq!(app.message.as_deref(), Some(words::LABEL_HASH));
        assert!(state(&app).typing.is_some(), "the label stays to fix");
    }
}
