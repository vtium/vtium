//! The Looks tab's group for the installed pointer themes: vtium's own
//! pictures, the package's `pointers.svg`, and each desktop cursor theme
//! in the system's icon folders, which is a folder that holds a `cursors`
//! folder. Each theme is a resource, so the group shows its active mark
//! and trust mark and takes Space on it with the controls the resource
//! tabs share. A cursor theme is activated by its folder, and every file
//! in the folder is active while the batch lists the folder.
//!
//! A look cannot pick a pointer theme yet, because vtium.conf's
//! `pointer-theme` key is read only at the top level. So every look draws
//! the theme that key names, and a theme's detail says whether the key
//! names it.
//!
//! References:
//! - ADR-1205 D4, its notes of 2026-11-19: a pointer theme is activated by
//!   its folder, vtium's own pictures are active on a fresh install, and
//!   every kind of resource passes the same gate.
//! - ADR-1205 D6: the Looks tab lists the installed pointer themes, from
//!   `/usr/share/vtium/pointers.svg`, `/usr/share/icons` and
//!   `/usr/local/share/icons`.

use std::path::Path;

use super::super::super::listbox::Item;
use super::super::super::model::Shared;
use super::super::super::resources::Resource;
use super::super::super::words;
use crate::batch::{Places, Use};
use crate::onscreen::text;

/// The pointer theme at `at`.
pub fn theme(shared: &Shared, at: usize) -> Option<&Resource> {
    shared.inputs.effects.pointers.get(at)
}

/// The pointer theme at `at` as an item of the list: its name, and its
/// marks.
pub fn item(shared: &Shared, at: usize) -> Item {
    // a folder's name is the administrator's text, which the screen shows
    // with its control and direction characters replaced
    theme(shared, at).map_or_else(Item::default, |r| Item::new(text(&r.name()), r.row()))
}

/// The uses of the pointer theme at `path` in the draft, by the rule the
/// command line's warning follows: the top level of vtium.conf when its
/// `pointer-theme` names the theme, which every look then draws.
pub fn uses(shared: &Shared, path: &Path) -> Vec<Use> {
    let none = |_: &str| Vec::new();
    crate::batch::uses(
        path,
        shared.conf_text().unwrap_or_default(),
        &Places {
            frits: None,
            palettes: &[],
            vtlook: &none,
        },
    )
}

/// The detail of the pointer theme at `at`: its title, its file or
/// folder, its marks, the check it fails, and whether the looks draw it.
pub fn detail(shared: &Shared, at: usize) -> Vec<String> {
    let Some(r) = theme(shared, at) else {
        return Vec::new();
    };
    let shown = crate::onscreen::path(&r.path);
    // vtium reads a pointer file as SVG by its suffix, and any other path
    // as a cursor theme's folder
    let folder = r.path.extension().is_none_or(|e| e != "svg");
    vec![
        words::pointer_title(text(&r.name())),
        if folder {
            words::pointer_folder(&shown)
        } else {
            words::detail_file(Some(&shown))
        },
        words::detail_state(words::active_mark(r.state)),
        words::detail_trust(r.kind, r.trust.as_ref().err().map(String::as_str)),
        words::pointer_users(!uses(shared, &r.path).is_empty()),
    ]
}
