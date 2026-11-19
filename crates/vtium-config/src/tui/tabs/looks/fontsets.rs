//! The Looks tab's group of font sets: every font set (frit) installed and
//! every built-in font, each with what the console reads of it. The group
//! marks a font set for removal at the next install, and a font set's
//! detail shows its fonts by role, what it still misses, and the looks
//! that use it. A look's card shows the same roles and misses for the
//! look's font set.
//!
//! References:
//! - ADR-1201 D3, the Fonts bullet: the marks of each font set and the
//!   removal at the next install.
//! - ADR-1205 D6: the roles a font set's fonts fill.
//! - ADR-1205 D8: a resource's view shows what works, and within a look
//!   what it still misses, by role.

use super::super::super::listbox::Item;
use super::super::super::model::{Font, FontState, Shared};
use super::super::super::words;
use super::super::{Change, Command};
use crate::specimen::Sources;

/// The font set at `at` in the list of fonts as an item of the tab's
/// list: its name, then what the console reads of it, whether the install
/// removes it, and whether it is vtium.conf's font set, saved or to save.
pub fn item(shared: &Shared, at: usize) -> Item {
    let Some(f) = shared.inputs.fonts.get(at) else {
        return Item::default();
    };
    let mut note = match (&f.state, shared.sources.get(&f.name)) {
        (FontState::Reads, Some(Sources::LeftOut(left))) if !left.is_empty() => {
            format!("{}, {}", words::READS, words::sources_missing(left.len()))
        }
        (FontState::Reads, Some(Sources::DrawsNothing(_))) => words::DRAWS_NOTHING.to_owned(),
        (FontState::Reads, Some(Sources::Unknown(_))) => {
            format!("{}, {}", words::READS, words::SOURCES_UNKNOWN)
        }
        (FontState::Reads, _) => words::READS.to_owned(),
        (FontState::BuiltIn, _) => words::BUILT_IN.to_owned(),
        (FontState::Refused(why), _) => words::does_not_read(why),
    };
    if shared.refused.contains(&f.name) {
        note = words::KEPT.to_owned();
    }
    if shared.pending.removals.contains(&f.name) {
        note = format!("{note}, {}", words::TO_REMOVE);
    }
    if shared.changed("font-name") && shared.drafted("font-name").as_deref() == Some(&f.name) {
        note = format!("{note}, {}", words::TO_SAVE);
    } else if shared.configured("font-name").as_deref() == Some(&f.name) {
        note = format!("{note}, {}", words::SAVED);
    }
    Item::new(f.name.clone(), note)
}

/// Whether the console can draw from the font set `font`: it reads, and
/// the console has not refused it.
pub fn usable(shared: &Shared, font: &Font) -> bool {
    !matches!(font.state, FontState::Refused(_)) && !shared.refused.contains(&font.name)
}

/// `r` on the font set named `name`: the font set is removed when the
/// user installs, or, when the install already removes it, no longer is.
/// The tab refuses what `remove-frit` refuses, by vtium.conf, its top
/// level and its looks, both as it is and as the install leaves it, and a
/// font set the install replaces with a new one. The install lists the
/// font set's file among what it changes, and `apply` removes it, as it
/// removes the frit of `remove-frit`'s draft.
pub fn remove(shared: &Shared, name: &str) -> Vec<Command> {
    let say = |line: String| Command::Change(Change::Say(line));
    let mut pending = shared.pending.clone();
    if let Some(at) = pending.removals.iter().position(|n| n == name) {
        pending.removals.remove(at);
        return vec![
            Command::Change(Change::Pending(pending)),
            say(words::remove_dropped(name)),
        ];
    }
    let refused = crate::commands::removal_refusal(name, shared.conf_text())
        .or_else(|| {
            pending
                .frits
                .iter()
                .any(|(n, _)| n == name)
                .then(|| words::remove_replaced(name))
        })
        .or_else(|| {
            (!shared.inputs.installed.contains_key(name)).then(|| words::remove_unread(name))
        });
    if let Some(why) = refused {
        return vec![say(words::remove_refused(name, why))];
    }
    pending.removals.push(name.to_owned());
    // the install as it would be with the removal: refused when the
    // draft names the font set
    match shared.frit_plan_of(&pending) {
        Ok(_) => vec![
            Command::Change(Change::Pending(pending)),
            say(words::remove_pending(name)),
        ],
        Err(why) => vec![say(words::remove_refused(name, why))],
    }
}

/// The text of the font set `name` as the install would leave it: a new
/// font set that waits for the install, else the installed file.
fn text<'s>(shared: &'s Shared, name: &str) -> Option<&'s str> {
    shared
        .pending
        .frits
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, t)| t.as_str())
        .or_else(|| {
            shared
                .inputs
                .installed
                .get(name)
                .and_then(|b| std::str::from_utf8(b).ok())
        })
}

/// The value of each line of `text` whose key is `key`, in the file's
/// order, with a comment after the value left out.
fn values<'t>(text: &'t str, key: &str) -> Vec<&'t str> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.split('#').next().unwrap_or("").trim())
        .filter(|v| !v.is_empty())
        .collect()
}

/// The fonts the font set `name` draws with, by role, one line each: the
/// text font first, then box drawing, then the font set's other fonts and
/// its layers, in the order vtium tries them. A font the font helper found
/// missing on this machine is marked so.
pub fn roles(shared: &Shared, name: &str) -> Vec<String> {
    let built_in = shared
        .inputs
        .fonts
        .iter()
        .any(|f| f.name == name && f.state == FontState::BuiltIn);
    if built_in {
        return vec![
            words::role_line(words::ROLE_TEXT, &words::built_in_font(name)),
            words::role_line(words::ROLE_BOX, words::ROLE_BOX_OWN),
        ];
    }
    let Some(text) = text(shared, name) else {
        return vec![words::ROLES_UNREAD.to_owned()];
    };
    let left: Vec<String> = match shared.sources.get(name) {
        Some(Sources::LeftOut(left)) => left
            .iter()
            .filter(|l| l.missing)
            .map(|l| l.spelling.clone())
            .collect(),
        _ => Vec::new(),
    };
    let shown = |spelling: &str| {
        if left.iter().any(|l| l == spelling) {
            words::font_missing(spelling)
        } else {
            spelling.to_owned()
        }
    };
    let sources = values(text, "source");
    let mut out = Vec::new();
    match sources.split_first() {
        Some((first, rest)) => {
            out.push(words::role_line(words::ROLE_TEXT, &shown(first)));
            out.push(words::role_line(
                words::ROLE_BOX,
                if values(text, "glyph-source").is_empty() {
                    words::ROLE_BOX_OWN
                } else {
                    words::ROLE_BOX_FONT
                },
            ));
            if !rest.is_empty() {
                let rest: Vec<String> = rest.iter().map(|s| shown(s)).collect();
                out.push(words::role_line(words::ROLE_OTHER, &rest.join(", ")));
            }
        }
        None => out.push(words::role_line(words::ROLE_TEXT, words::ROLE_NONE)),
    }
    let layers = values(text, "layer");
    if !layers.is_empty() {
        out.push(words::role_line(words::ROLE_LAYERS, &layers.join(", ")));
    }
    out
}

/// What the font set `name` still misses on this machine, as the font
/// helper found it, one line each: nothing, each font it lacks or
/// refuses, or why nothing is known yet.
pub fn missing(shared: &Shared, name: &str) -> Vec<String> {
    match shared.sources.get(name) {
        _ if shared
            .inputs
            .fonts
            .iter()
            .any(|f| f.name == name && f.state == FontState::BuiltIn) =>
        {
            vec![words::MISSES_NOTHING.to_owned()]
        }
        Some(Sources::LeftOut(left)) if left.is_empty() => vec![words::MISSES_NOTHING.to_owned()],
        Some(Sources::LeftOut(left)) => left
            .iter()
            .map(|l| words::misses(&l.spelling, &l.why))
            .collect(),
        Some(Sources::DrawsNothing(why)) => vec![words::misses_all(why)],
        Some(Sources::Unknown(why)) => vec![words::misses_unknown(why)],
        None => vec![words::MISSES_NOT_ASKED.to_owned()],
    }
}

/// The looks of the draft that draw the font set `name`, the top level
/// first, each as the questions name it.
pub fn users(shared: &Shared, name: &str) -> Vec<String> {
    let conf = shared.conf_text().unwrap_or_default();
    let mut out = Vec::new();
    if vtium_settings::setting(conf, "font-name").as_deref() == Some(name) {
        out.push(words::look_name(None));
    }
    for look in super::list::looks(shared) {
        if look.frit.as_deref() == Some(name) {
            out.push(words::look_name(Some(&look.name)));
        }
    }
    out
}

/// The detail of the font set at `at` in the list of fonts, one line a
/// paragraph: its name, whether it reads, its fonts by role, what it
/// misses, and the looks that use it.
pub fn detail(shared: &Shared, at: usize) -> Vec<String> {
    let Some(f) = shared.inputs.fonts.get(at) else {
        return Vec::new();
    };
    let mut out = vec![
        words::font_set_title(&f.name),
        match &f.state {
            FontState::Reads => words::READS_WHOLE.to_owned(),
            FontState::BuiltIn => words::BUILT_IN_WHOLE.to_owned(),
            FontState::Refused(why) => words::does_not_read_whole(why),
        },
        words::ROLES_HEADING.to_owned(),
    ];
    out.extend(roles(shared, &f.name).into_iter().map(|l| format!("  {l}")));
    out.push(words::MISSES_HEADING.to_owned());
    out.extend(
        missing(shared, &f.name)
            .into_iter()
            .map(|l| format!("  {l}")),
    );
    out.push(words::detail_users(&users(shared, &f.name)));
    out
}
