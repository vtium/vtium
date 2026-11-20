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

use super::super::super::desk::{FontSetEdit, Op};
use super::super::super::listbox::Item;
use super::super::super::model::{Effect, Font, FontState, Shared};
use super::super::super::words;
use super::super::{Change, Command};
use crate::frit::{Frit, Source};
use crate::serve::LeftOut;
use crate::specimen::Sources;
use std::path::Path;
use vtium_font::Source as GlyphSource;
use vtium_font::Style;
use vtium_font::sprite::Range;

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
    if shared.frits().removed.contains(&f.name) {
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

/// `r` on the font set named `name`: the draft's folder keeps the
/// removal, and the install removes the font set; or, when the draft
/// already removes it, the draft no longer does. The tab refuses what
/// `remove-frit` refuses, by vtium.conf, its top level and its looks as
/// the draft has them, and a font set the draft replaces with a new one.
/// The install lists the font set's file among what it changes, and
/// `apply` removes it, as it removes the frit of `remove-frit`'s draft.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": the removal waits in the draft's folder, as an empty `FRIT.removed`.
pub fn remove(shared: &Shared, name: &str) -> Vec<Command> {
    let say = |line: String| Command::Change(Change::Say(line));
    let Some(draft) = &shared.draft else {
        return vec![say(words::NO_DESK.to_owned())];
    };
    let keep = |edit, line| {
        vec![
            Command::Effect(Effect::Desk(Op::FontSet {
                frit: name.to_owned(),
                edit,
                text: draft.text.clone(),
            })),
            say(line),
        ]
    };
    if shared.frits().removed.contains(name) {
        return keep(FontSetEdit::Forget, words::remove_dropped(name));
    }
    let refused = crate::commands::removal_refusal(name, shared.conf_text())
        .or_else(|| {
            shared
                .frits()
                .kept
                .contains_key(name)
                .then(|| words::remove_replaced(name))
        })
        .or_else(|| {
            (!shared.inputs.installed.contains_key(name)).then(|| words::remove_unread(name))
        });
    if let Some(why) = refused {
        return vec![say(words::remove_refused(name, why))];
    }
    keep(FontSetEdit::Remove, words::remove_pending(name))
}

/// The text of the font set `name` as the install would leave it: a new
/// font set that waits for the install, else the installed file.
fn text<'s>(shared: &'s Shared, name: &str) -> Option<&'s str> {
    shared
        .frit_text(name)
        .and_then(|b| std::str::from_utf8(b).ok())
}

/// The font set `name` as the frit parser reads it, where its text reads.
fn parsed(shared: &Shared, name: &str) -> Option<Frit> {
    let text = text(shared, name)?;
    Frit::parse(text.as_bytes(), Path::new(name), name).ok()
}

/// The role a source of a font set fills: the text font, one of its
/// styled faces, a further font tried after it, or a font of a layer.
fn role(frit: &Frit, source: &Source) -> &'static str {
    if source.layer.is_some() {
        return words::ROLE_LAYER_FONT;
    }
    match (source.style.bold, source.style.italic) {
        (true, true) => words::ROLE_BOLD_ITALIC,
        (true, false) => words::ROLE_BOLD,
        (false, true) => words::ROLE_ITALIC,
        (false, false) => {
            let first = frit
                .sources
                .iter()
                .find(|s| s.layer.is_none() && s.style == Style::default());
            if first.is_some_and(|f| f.line == source.line) {
                words::ROLE_TEXT
            } else {
                words::ROLE_OTHER
            }
        }
    }
}

/// The font `spelling` names, in the words a person uses for it: as the
/// source of `frit` with that spelling names it, else as the frit parser
/// reads the spelling on a `source=` line of its own, which names a font
/// from a layer. A spelling the parser does not read is shown as it is.
fn font_of(frit: Option<&Frit>, spelling: &str) -> String {
    let own = frit
        .and_then(|f| f.sources.iter().find(|s| s.spelling == spelling))
        .map(|s| s.origin.clone());
    own.or_else(|| {
        Frit::parse(
            format!("source={spelling}\n").as_bytes(),
            Path::new("font"),
            "font",
        )
        .ok()
        .and_then(|f| f.sources.into_iter().next())
        .map(|s| s.origin)
    })
    .map_or_else(|| spelling.to_owned(), |o| words::font_named(&o))
}

/// What box drawing comes from, as the font set's `glyph-source-box=`
/// line, else its `glyph-source=` line, chooses.
fn box_drawing(frit: &Frit) -> &'static str {
    let line = |range: Option<Range>| {
        frit.glyph_source
            .iter()
            .rev()
            .find(|(r, _)| *r == range)
            .map(|(_, s)| *s)
    };
    match line(Some(Range::BoxDrawing)).or_else(|| line(None)) {
        Some(GlyphSource::Sprites) => words::ROLE_BOX_OWN,
        Some(GlyphSource::Font) => words::ROLE_BOX_FONT,
        Some(GlyphSource::Auto) => words::ROLE_BOX_AUTO,
        None => words::ROLE_BOX_DEFAULT,
    }
}

/// The fonts the font set `name` draws with, by role, one line each: the
/// text font and its bold and italic faces first, then box drawing, then
/// the fonts vtium tries after the text font, and last the layers that
/// add fonts for scripts, symbols, emoji and the fallback. A font the font
/// helper found missing on this machine, or cannot use, is marked so.
///
/// References:
/// - ADR-1205 D6: the roles, and box drawing apart from the fonts' order.
pub fn roles(shared: &Shared, name: &str) -> Vec<String> {
    let built_in = shared
        .inputs
        .fonts
        .iter()
        .any(|f| f.name == name && f.state == FontState::BuiltIn);
    if built_in {
        return vec![
            words::role_line(words::ROLE_TEXT, &words::built_in_font(name)),
            words::role_line(words::ROLE_BOX, words::ROLE_BOX_DEFAULT),
        ];
    }
    let Some(frit) = parsed(shared, name) else {
        return vec![words::ROLES_UNREAD.to_owned()];
    };
    let left: Vec<&LeftOut> = match shared.sources.get(name) {
        Some(Sources::LeftOut(left)) => left.iter().collect(),
        _ => Vec::new(),
    };
    let shown = |source: &Source| {
        let font = words::font_named(&source.origin);
        match left.iter().find(|l| l.spelling == source.spelling) {
            Some(l) if l.missing => words::font_missing(font),
            Some(_) => words::font_refused(font),
            None => font,
        }
    };
    let mut out = Vec::new();
    for heading in [
        words::ROLE_TEXT,
        words::ROLE_BOLD,
        words::ROLE_ITALIC,
        words::ROLE_BOLD_ITALIC,
    ] {
        let fonts: Vec<String> = frit
            .sources
            .iter()
            .filter(|s| role(&frit, s) == heading)
            .map(shown)
            .collect();
        match (heading, fonts.is_empty()) {
            (_, false) => out.push(words::role_line(heading, &fonts.join(", "))),
            (h, true) if h == words::ROLE_TEXT => {
                out.push(words::role_line(words::ROLE_TEXT, words::ROLE_NONE));
            }
            _ => {}
        }
        if heading == words::ROLE_BOLD_ITALIC {
            out.push(words::role_line(words::ROLE_BOX, box_drawing(&frit)));
        }
    }
    let more: Vec<String> = frit
        .sources
        .iter()
        .filter(|s| role(&frit, s) == words::ROLE_OTHER)
        .map(shown)
        .collect();
    if !more.is_empty() {
        out.push(words::role_line(words::ROLE_OTHER, &more.join(", ")));
    }
    let layers: Vec<String> = frit.layers.iter().map(|(_, n)| n.clone()).collect();
    if !layers.is_empty() {
        out.push(words::role_line(
            words::ROLE_LAYERS,
            &words::role_layers(&layers),
        ));
    }
    out
}

/// What the font set `name` still misses on this machine, as the font
/// helper found it, one line each: nothing, each font it lacks or
/// cannot use, by the role the font fills, or why nothing is known yet.
///
/// References:
/// - ADR-1205 D8: what a look still misses, by role.
pub fn missing(shared: &Shared, name: &str) -> Vec<String> {
    let frit = parsed(shared, name);
    let role_of = |spelling: &str| {
        frit.as_ref()
            .and_then(|f| {
                f.sources
                    .iter()
                    .find(|s| s.spelling == spelling)
                    .map(|s| role(f, s))
            })
            // a font the frit's own lines do not name comes from a layer
            .unwrap_or(words::ROLE_LAYER_FONT)
    };
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
            .map(|l| {
                let (role, font) = (role_of(&l.spelling), font_of(frit.as_ref(), &l.spelling));
                if l.missing {
                    words::misses(role, font, &l.why)
                } else {
                    words::misses_refused(role, font, &l.why)
                }
            })
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
