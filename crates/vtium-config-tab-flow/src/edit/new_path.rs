//! NEW PATH's own keys: `a` adds the path to the configuration, `e` takes
//! a path out of the configuration into NEW PATH to revise it, and Esc on
//! a path being revised takes it out of the configuration.
//!
//! Revise changes no document. NEW PATH holds the path, and CONFIGURATION
//! leaves it out while the draft still has it, so `a` on a path that came
//! back unchanged writes nothing, and the draft's text stays byte for
//! byte as it was. Esc then writes the configuration without the path, as
//! one change.
//!
//! References:
//! - ADR-1403 D7, "Composing and adding a path": what `a` checks and
//!   refuses.
//! - ADR-1403 D7, "Editing an added path, and Revise": `e`, Esc on a path
//!   being revised, and Revise coming back unchanged.

use vtium_config_core::ui::{Command, Request, Shared};

use super::plan::{self, Adding, Revising};
use super::words;
use crate::page::Model;
use crate::page::layout::{Obj, Slot};
use crate::paths::{self, Path};
use crate::{FlowTab, Level, say};

impl FlowTab {
    /// `a`: adds NEW PATH to the configuration, as one change, and
    /// highlights the path's terminal there. A path the configuration
    /// holds already changes nothing, and a path that cannot be added
    /// stays in NEW PATH with the reason in one line.
    pub(crate) fn add(&mut self, shared: &Shared, model: &Model) -> Vec<Command> {
        let (flow, doc) = (&model.flow, &model.doc);
        let mut new = self.state.new.clone();
        if new.name.is_none() && !new.active && !new.none {
            new.name = Some(paths::new_name(flow, doc));
        }
        let name = new.name.clone().unwrap_or_default();
        let (edits, said, want) = match plan::check_add(flow, doc, &new) {
            Err(why) => return say(why),
            Ok(Adding::Same(there)) => (plan::plan(flow, doc), words::same_path(&there), None),
            Ok(Adding::SameActive) => (plan::plan(flow, doc), words::SAME_ACTIVE.to_owned(), None),
            Ok(Adding::SameNone) => (plan::plan(flow, doc), words::SAME_NONE.to_owned(), None),
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
                (plan::plan(flow, &plan::with(doc, new)), said, Some(want))
            }
        };
        let out = match super::changed(shared, words::adding(&name), edits, said) {
            Ok(out) => out,
            Err(refused) => return refused,
        };
        self.state.new = Path::default();
        self.state.revising = None;
        if let Some(w) = want {
            self.state.at = Some(w);
        }
        out
    }

    /// Esc on NEW PATH while it holds a path being revised: the path
    /// leaves the configuration, as one change, and NEW PATH is empty.
    pub(crate) fn drop_revised(&mut self, shared: &Shared, model: &Model) -> Vec<Command> {
        let edits = plan::plan(&model.flow, &model.doc);
        let out = match super::changed(
            shared,
            words::REMOVING.to_owned(),
            edits,
            words::REVISED_DROPPED.to_owned(),
        ) {
            Ok(out) => out,
            Err(refused) => return refused,
        };
        self.state.new = Path::default();
        self.state.revising = None;
        out
    }

    /// `e` on `obj`, a part of a path in CONFIGURATION, of the roaming
    /// inputs or of the path to Nowhere: the path goes into NEW PATH with
    /// its parts picked, CONFIGURATION leaves it out, the level open
    /// closes and NEW PATH's terminal is highlighted. While NEW PATH holds
    /// other parts, `e` says so and changes nothing.
    pub(crate) fn revise(&mut self, model: &Model, obj: &Obj) -> Vec<Command> {
        let (what, said) = match obj {
            Obj::Doc(name, _) => (Revising::Path(name.clone()), words::path_revised(name)),
            Obj::Roam(_) => (Revising::Active, words::ACTIVE_REVISED.to_owned()),
            Obj::Nothing(_) => (Revising::Nothing, words::NONE_REVISED.to_owned()),
            _ => return say(words::REVISE_WHERE),
        };
        if !self.state.new.is_empty() {
            return say(words::EDITOR_BUSY);
        }
        let Some((_, path)) = plan::revise(&model.full, &what) else {
            return say(words::REVISE_WHERE);
        };
        self.state.new = path;
        self.state.revising = Some(what);
        self.state.at = Some(Obj::New(Slot::Terminal));
        let mut out = Vec::new();
        if self.level != Level::Page {
            self.level = Level::Page;
            out.push(Command::Request(Request::Close));
        }
        out.extend(say(said));
        out
    }
}
