//! `r` and the two questions on the way. `r` on a font set marks it for
//! removal at the next save, and `r` again takes the mark back, each as
//! one change. `r` on a look asks first, naming the terminals that then
//! take the default look, and only the yes deletes it. Space on an active
//! color scheme file or pointer theme that a look uses asks first too,
//! naming those looks, and only the yes deactivates it.
//!
//! A question's yes comes back to the tab as the action the question
//! named, carrying the item's place in the list, which a key press never
//! carries. The question holds the screen until it is answered, so the
//! list stays as it was while it is asked.
//!
//! References:
//! - ADR-1403 D8, "Every font set change is kept at once": `r` keeps a removal, takes back a font set only the document adds, and takes a removal back.
//! - ADR-1205 D4: deactivating what a look uses names those looks first.
//! - ADR-1206 D2: a question is a request, and its yes hands the tab the action it named.

use std::path::{Path, PathBuf};

use vtium_config_core::batch::{self, Change as Batch};
use vtium_config_core::document::{Change, ConfEdit, Edit, FontSetEdit};
use vtium_config_core::found::Resource;
use vtium_config_core::ui::{
    Action, Carries, Change as UiChange, Command, Message, Request, Shared,
};

use super::words;
use crate::card;
use crate::list::{self, Item, Scheme};

/// The row whose action, carrying the look's place, is the yes to the
/// question of deleting it.
pub const DELETE: &str = "r.view.looks";

/// The row whose action, carrying the resource's place, is the yes to the
/// question of deactivating it.
pub const DEACTIVATE: &str = "space.view.resources";

/// The item a question's yes names: the place it carries, where it is
/// one.
pub fn answered(action: &Action) -> Option<usize> {
    match action.carries {
        Carries::Item(at) => Some(at),
        _ => None,
    }
}

/// A change to the document under `name`.
fn change(name: String, edit: Edit) -> Command {
    Command::Change(UiChange::Document(Change::new(name, edit)))
}

/// A line on the message line.
fn say(line: String) -> Command {
    Command::Change(UiChange::Say(Message { line, more: None }))
}

/// `r` on the look `name`, at `at` in the list: the question before it is
/// deleted.
pub fn ask_delete(shared: &Shared, name: &str, at: usize) -> Vec<Command> {
    let terminals = list::users(shared.document().text(), name);
    vec![Command::Request(Request::Ask {
        question: words::delete_question(name, &terminals),
        then: Action::item(DELETE, at),
    })]
}

/// The yes to the question of deleting the look at `at`: the change that
/// deletes it, which moves the terminals that draw it to the default
/// look.
pub fn delete(shared: &Shared, at: usize) -> Vec<Command> {
    match list::items(shared).into_iter().nth(at) {
        Some(Item::Look(name)) => vec![change(
            words::look_deleted(&name),
            Edit::Conf(vec![ConfEdit::LookRm(name)]),
        )],
        _ => Vec::new(),
    }
}

/// `r` on the font set `name`. A set the document removes gets its mark
/// taken back; an installed set is marked for removal; a set only the
/// document adds is taken back out. A font built into vtium, and a name
/// neither installed nor added, cannot be removed, and the message line
/// says why.
pub fn font_set(shared: &Shared, name: &str) -> Vec<Command> {
    let document = shared.document();
    let sets = &document.content().font_sets;
    let edit = |edit| Edit::FontSet {
        frit: name.to_owned(),
        edit,
    };
    if sets.removed.contains(name) {
        return vec![
            change(words::keep_name(name), edit(FontSetEdit::Forget)),
            say(words::removal_taken_back(name)),
        ];
    }
    if vtium_font_helper::frit::RESERVED.contains(&name) {
        return vec![say(words::cannot_remove_built_in(name))];
    }
    if document.installed().frits.contains_key(name) {
        return vec![
            change(words::remove_name(name), edit(FontSetEdit::Remove)),
            say(words::removal_marked(name)),
        ];
    }
    if sets.kept.contains_key(name) {
        return vec![
            change(words::remove_name(name), edit(FontSetEdit::Forget)),
            say(words::new_set_taken_back(name)),
        ];
    }
    vec![say(words::cannot_remove(name))]
}

/// The resource Space acts on for `item`: a color scheme file or a
/// pointer theme.
fn resource<'a>(shared: &'a Shared, item: &Item) -> Option<&'a Resource> {
    match item {
        Item::Scheme(Scheme::File(at)) => shared.found.palettes.files.get(*at).map(|f| &f.resource),
        Item::Pointer(at) => shared.found.pointers.get(*at),
        _ => None,
    }
}

/// The looks that use the resource at `path`, `None` for vtium.conf's
/// top level.
fn users(shared: &Shared, path: &Path) -> Vec<Option<String>> {
    let folders: Vec<&Path> = shared
        .found
        .palettes
        .folders
        .iter()
        .map(PathBuf::as_path)
        .collect();
    let none = |_: &str| Vec::new();
    batch::uses(
        path,
        shared.document().text(),
        &batch::Places {
            frits: None,
            palettes: &folders,
            vtlook: &none,
        },
    )
    .into_iter()
    .map(|u| u.look)
    .collect()
}

/// Space on `item`, at `at` in the list, where it is an active resource
/// vtium can use that a look uses: the question before it is
/// deactivated. `None` for any other item, which Space acts on at once.
pub fn ask_deactivate(shared: &Shared, item: &Item, at: usize) -> Option<Vec<Command>> {
    let r = resource(shared, item)?;
    if r.own || r.trust.is_err() || !card::active(shared, r) {
        return None;
    }
    let looks = users(shared, &r.path);
    if looks.is_empty() {
        return None;
    }
    Some(vec![Command::Request(Request::Ask {
        question: words::deactivate_question(&item.name(shared), &looks),
        then: Action::item(DEACTIVATE, at),
    })])
}

/// The yes to the question of deactivating the resource at `at`: the
/// change that deactivates it, or that takes back the document's own
/// activation of it.
pub fn deactivate(shared: &Shared, at: usize) -> Vec<Command> {
    let Some(item) = list::items(shared).into_iter().nth(at) else {
        return Vec::new();
    };
    let Some(r) = resource(shared, &item) else {
        return Vec::new();
    };
    if !card::active(shared, r) {
        return Vec::new();
    }
    let change_to = match shared.document().content().active.get(&r.path) {
        Some(Batch::Activate) => None,
        _ => Some(Batch::Deactivate),
    };
    vec![change(
        crate::words::deactivated(&item.name(shared)),
        Edit::Active {
            path: r.path.clone(),
            change: change_to,
        },
    )]
}
