//! `n` among the looks: a new look goes into the document as one change,
//! and its card opens with the name being typed. The look takes the next
//! free name of `new-look`, `new-look-2` and on, and leaves its font set,
//! size, color scheme and pointer theme to vtium.conf. It starts on No
//! filter: a look with no `vtlook` line draws the vtlook vtium.conf's top
//! level sets, so where the top level sets one other than the built-in
//! one, the new look names the built-in one. Enter on the typed name
//! renames the look as a second change, and Esc keeps the name it has.
//!
//! References:
//! - ADR-1304 D4, its note of 2026-11-19: a new look starts on No filter.
//! - ADR-1304 D5: the look's vtlook wins, then the top level's, then the built-in one.
//! - ADR-1206 D2: the look is one change, which the shell's one change function makes.

use vtium_config_core::document::{Change, ConfEdit, Edit};
use vtium_config_core::ui::{Change as UiChange, Command, Message, Shared};

use super::words;
use crate::list;
use crate::{Card, CardOpen, Looks, Open};

/// The next free name for a new look.
pub fn free_name(shared: &Shared) -> String {
    let taken: Vec<String> = list::looks(shared.document().text())
        .into_iter()
        .map(|l| l.name)
        .collect();
    std::iter::once(words::NEW_LOOK_NAME.to_owned())
        .chain((2..).map(|n| format!("{}-{n}", words::NEW_LOOK_NAME)))
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}

/// The vtlook a new look names: the built-in one where vtium.conf's top
/// level sets another, so the look draws No filter, and none elsewhere.
fn vtlook(shared: &Shared) -> Option<String> {
    let default = vtium_settings::vtlook::DEFAULT;
    vtium_settings::setting(shared.document().text(), "vtlook")
        .filter(|v| v != default && !v.is_empty())
        .map(|_| default.to_owned())
}

/// `n` among the looks: the change that makes the look, and the card
/// open on its name being typed; or why the document refuses the look.
pub fn make(tab: &mut Looks, shared: &Shared) -> Vec<Command> {
    let name = free_name(shared);
    let edit = Edit::Conf(vec![ConfEdit::LookAdd {
        name: name.clone(),
        frit: None,
        size: None,
        vtlook: vtlook(shared),
    }]);
    let document = shared.document();
    if let Err(why) = document.content().edited(&edit, document.installed()) {
        return super::say(why);
    }
    tab.open = Open::Card(Card {
        name: name.clone(),
        row: 0,
        open: CardOpen::Typing(name.clone()),
    });
    vec![
        Command::Change(UiChange::Document(Change::new(
            words::look_made(&name),
            edit,
        ))),
        Command::Change(UiChange::Say(Message {
            line: words::type_look_name(&name),
            more: None,
        })),
    ]
}
