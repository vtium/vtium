//! A font of your own: a file in the person's own font folders, which
//! vtium does not take until it is installed as a system font. The
//! install is the shell's action, under Actions › Install as a system
//! font… and on Enter in the font's detail. The tab answers which font it
//! would act on, and hands the desk the file, which checks it with the
//! font helper and readies the exact command the question shows before
//! anything runs.
//!
//! References:
//! - ADR-1205 D5: installing a font of the person's as a system font, the
//!   check first, and the exact command shown before it runs.
//! - ADR-1403 D10: Enter on a font from the person's own folders asks to
//!   install it as a system font.

use std::path::PathBuf;

use vtium_config_core::found::fonts::FontFile;
use vtium_config_core::ui::{Change, Command, Effect, Message, Op};

use crate::words;

/// Whether `f` is a font of your own.
pub fn is_own(f: &FontFile) -> bool {
    f.resource.own
}

/// The font that the install as a system font acts on, where `f` is one
/// of your own, or why there is none.
pub fn target(f: Option<&FontFile>) -> Result<PathBuf, String> {
    match f {
        Some(f) if is_own(f) => Ok(f.path().to_path_buf()),
        Some(_) => Err(words::SYSTEM_FONT_NOT_YOURS.to_owned()),
        None => Err(words::NO_OWN_FONT.to_owned()),
    }
}

/// The commands that start the install of `path` as a system font: the
/// message line says that the font helper checks the font, which can take
/// seconds, and the desk checks it and readies the question.
pub fn install(path: PathBuf) -> Vec<Command> {
    vec![
        Command::Change(Change::Say(Message {
            line: words::CHECKING_FONT.to_owned(),
            more: None,
        })),
        Command::Effect(Effect::Desk(Op::SystemFont(path))),
    ]
}
