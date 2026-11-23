//! The Looks tab's group of font sets: every font set (frit) installed,
//! every built-in font, and each font set the draft adds, each with what
//! the console reads of it. The group marks a font set the draft adds,
//! replaces or removes, and a font set's detail shows its fonts by role,
//! what it still misses, and the looks that use it. A look's card shows
//! the same roles and misses for the look's font set.
//!
//! References:
//! - ADR-1403 D8, "The frits a draft sees": the installed frits plus the
//!   draft's, each marked, and a look card offers them.
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
use crate::drafts::FritEdit;
use crate::frit::{Frit, Source};
use crate::serve::LeftOut;
use crate::specimen::Sources;
use std::path::Path;
use vtium_font::Source as GlyphSource;
use vtium_font::Style;
use vtium_font::sprite::Range;

/// The font sets the draft sees, in the tab's order: every font set
/// installed and every built-in font, then each font set that only the
/// draft's folder adds, by name. The list holds a font set the draft adds
/// apart from the installed ones, after them, so adding or taking one back
/// moves no installed font set in the list. vtium-config reads a font set
/// the draft adds as the next start would, so the list marks one the
/// console does not read.
///
/// References:
/// - ADR-1403 D8, "The frits a draft sees": the installed frits plus each `FRIT.vtfrit` in the folder.
pub fn all(shared: &Shared) -> Vec<Font> {
    let installed = &shared.inputs.fonts;
    let mut out = installed.clone();
    out.extend(
        shared
            .frits()
            .kept
            .iter()
            .filter(|(name, _)| !installed.iter().any(|f| f.name == **name))
            .map(|(name, text)| Font {
                name: name.clone(),
                state: state_of(name, text),
            }),
    );
    out
}

/// The font set at `at` in [`all`].
pub fn font(shared: &Shared, at: usize) -> Option<Font> {
    all(shared).into_iter().nth(at)
}

/// The place of the font set `name` in [`all`].
pub fn place(shared: &Shared, name: &str) -> Option<usize> {
    all(shared).iter().position(|f| f.name == name)
}

/// Whether the font set `name` is one that only the draft adds: its
/// folder holds it, and this machine has no font set of that name.
pub fn only_drafted(shared: &Shared, name: &str) -> bool {
    shared.frits().kept.contains_key(name) && !shared.inputs.fonts.iter().any(|f| f.name == name)
}

/// What the console reads of the font set `name` whose text is `text`,
/// as the frit parser reads it.
pub fn state_of(name: &str, text: &str) -> FontState {
    match Frit::parse(text.as_bytes(), Path::new(name), name) {
        Ok(_) => FontState::Reads,
        Err(e) if e.line == 0 => FontState::Refused(e.why),
        Err(e) => FontState::Refused(format!("line {}: {}", e.line, e.why)),
    }
}

/// Whether the draft replaces the installed font set `name` with text of
/// its own, which the install writes in its place.
pub fn replaced(shared: &Shared, name: &str) -> bool {
    shared.draft.as_ref().is_some_and(|d| {
        d.frit_changes
            .iter()
            .any(|(n, c)| n == name && *c == FritEdit::Replace)
    })
}

/// The font set at `at` in the list of fonts as an item of the tab's
/// list: its name, then what the console reads of it, whether the draft
/// adds, replaces or removes it, and whether it is vtium.conf's font set,
/// saved or to save.
pub fn item(shared: &Shared, at: usize) -> Item {
    let Some(f) = font(shared, at) else {
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
    } else if only_drafted(shared, &f.name) {
        note = format!("{note}, {}", words::IN_DRAFT);
    } else if replaced(shared, &f.name) {
        note = format!("{note}, {}", words::REPLACED_IN_DRAFT);
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

/// `r` on the font set named `name`, as `remove-frit` does it. On an
/// installed font set, the draft's folder keeps its removal, and the
/// install removes it; the removal takes the place of a new text the draft
/// gives it. The removal is kept even when the draft's vtium.conf names
/// the font set, at its top level or in a look, and the check then lists
/// each line that names it. On a font set that only the draft adds, `r`
/// takes it back out of the folder, and on a removed one it takes the
/// removal back. The tab refuses what `remove-frit` refuses, by the same
/// check: a built-in font, a set of vtium's catalog, and a font set that is
/// neither installed nor in the folder.
///
/// References:
/// - ADR-1403 D8, "Every font set change is kept at once": what `r` keeps, and the removals vtium-config refuses.
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
    // a removal already kept is taken back first, so `r` toggles it even
    // where it would now refuse to keep one
    if shared.frits().removed.contains(name) {
        return keep(FontSetEdit::Forget, words::remove_dropped(name));
    }
    if let Some(why) = crate::commands::removal_refusal(&shared.inputs.frits, name) {
        return vec![say(words::remove_refused(name, why))];
    }
    if shared.inputs.installed.contains_key(name) {
        return keep(FontSetEdit::Remove, words::remove_pending(name));
    }
    if shared.frits().kept.contains_key(name) {
        return keep(FontSetEdit::Forget, words::remove_forgotten(name));
    }
    vec![say(words::remove_refused(name, words::remove_unread(name)))]
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
    let Some(f) = font(shared, at) else {
        return Vec::new();
    };
    let mut out = vec![
        words::font_set_title(&f.name),
        match &f.state {
            FontState::Reads => words::READS_WHOLE.to_owned(),
            FontState::BuiltIn => words::BUILT_IN_WHOLE.to_owned(),
            FontState::Refused(why) => words::does_not_read_whole(why),
        },
    ];
    if only_drafted(shared, &f.name) {
        out.push(words::IN_DRAFT_WHOLE.to_owned());
    }
    out.push(words::ROLES_HEADING.to_owned());
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

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::tui::desk::Opened;
    use crate::tui::fixture;
    use crate::tui::model::App;

    /// The fixture's shared state with a draft open on its vtium.conf,
    /// whose folder holds `kept` and `removed`.
    fn shared(kept: &[&str], removed: &[&str]) -> Shared {
        let inputs = fixture::inputs();
        let text = inputs.conf.clone().unwrap_or_default();
        assert!(text.contains("font-name = proof"), "{text}");
        let (app, _) = App::new(inputs, 80, 25);
        let mut shared = app.shared;
        let mut frits = crate::store::FritChanges::default();
        for name in kept {
            frits
                .kept
                .insert((*name).to_owned(), "source=builtin:unifont\n".to_owned());
        }
        for name in removed {
            frits.removed.insert((*name).to_owned());
        }
        shared.draft = Some(Opened {
            name: "mine".into(),
            text,
            frits,
            ..Default::default()
        });
        shared
    }

    /// The change `r` asks the desk to keep for `name`, and what the
    /// message line says; or, when `r` refuses, `None` and the line.
    fn r(shared: &Shared, name: &str) -> (Option<FontSetEdit>, String) {
        let mut edit = None;
        let mut said = String::new();
        for command in remove(shared, name) {
            match command {
                Command::Effect(Effect::Desk(Op::FontSet { frit, edit: e, .. })) => {
                    assert_eq!(frit, name);
                    edit = Some(e);
                }
                Command::Change(Change::Say(line)) => said = line,
                other => panic!("r on {name}: {other:?}"),
            }
        }
        (edit, said)
    }

    /// `r` keeps the removal of an installed font set even when the
    /// draft's vtium.conf names it, at its top level or in a look: the
    /// check then lists each line that names it.
    ///
    /// References:
    /// - ADR-1403 D8, "Every font set change is kept at once": a removal is written even when the draft's looks name the frit.
    #[test]
    fn r_keeps_the_removal_of_a_font_set_the_draft_names() {
        // look proof names proof
        let (edit, said) = r(&shared(&[], &[]), "proof");
        assert_eq!(edit, Some(FontSetEdit::Remove), "{said}");
        assert_eq!(said, words::remove_pending("proof"));
        // the top level names minspec-16
        let mut named = shared(&[], &[]);
        if let Some(d) = named.draft.as_mut() {
            d.text = d.text.replace("font-name=unifont", "font-name=minspec-16");
        }
        assert_eq!(r(&named, "minspec-16").0, Some(FontSetEdit::Remove));
    }

    /// `r` on an installed font set that the draft replaces removes it, as
    /// `remove-frit` does: the removal takes the place of the new text.
    #[test]
    fn r_removes_an_installed_font_set_the_draft_replaces() {
        let (edit, said) = r(&shared(&["tiny5"], &[]), "tiny5");
        assert_eq!(edit, Some(FontSetEdit::Remove), "{said}");
    }

    /// `r` on a font set that only the draft adds takes it back out of
    /// the draft's folder, and on a removed font set takes the removal
    /// back.
    #[test]
    fn r_takes_back_a_font_set_only_the_draft_adds_and_a_removal() {
        let (edit, said) = r(&shared(&["quiet"], &[]), "quiet");
        assert_eq!(edit, Some(FontSetEdit::Forget), "{said}");
        assert_eq!(said, words::remove_forgotten("quiet"));
        let (edit, said) = r(&shared(&[], &["tiny5"]), "tiny5");
        assert_eq!(edit, Some(FontSetEdit::Forget), "{said}");
        assert_eq!(said, words::remove_dropped("tiny5"));
    }

    /// `r` refuses only what it cannot keep: a built-in font, and a font
    /// set that is neither installed nor in the draft's folder.
    #[test]
    fn r_refuses_a_built_in_font_and_a_font_set_it_cannot_find() {
        let (edit, said) = r(&shared(&[], &[]), "unifont");
        assert_eq!(edit, None);
        assert!(said.contains("built-in font"), "{said}");
        let (edit, said) = r(&shared(&[], &[]), "quiet");
        assert_eq!(edit, None);
        assert_eq!(
            said,
            words::remove_refused("quiet", words::remove_unread("quiet"))
        );
    }

    /// The list marks a font set only the draft adds and one the draft
    /// changes, and leaves unmarked a font set in the folder whose text is
    /// the installed one's, which the install leaves as it is.
    ///
    /// References:
    /// - ADR-1403 D8, "The frits a draft sees": the views mark each frit the draft adds, replaces or removes, and a file with the installed text is no change.
    #[test]
    fn the_list_marks_what_the_draft_adds_and_changes() {
        let mut held = shared(&["quiet", "tiny5", "proof"], &[]);
        if let Some(d) = held.draft.as_mut() {
            d.frit_changes = vec![
                ("quiet".to_owned(), FritEdit::Add),
                ("tiny5".to_owned(), FritEdit::Replace),
            ];
        }
        let note = |name: &str| {
            let at = place(&held, name).unwrap_or_else(|| panic!("{name} is listed"));
            item(&held, at).detail
        };
        assert!(
            note("quiet").ends_with(words::IN_DRAFT),
            "{}",
            note("quiet")
        );
        assert!(
            note("tiny5").ends_with(words::REPLACED_IN_DRAFT),
            "{}",
            note("tiny5")
        );
        assert!(!note("proof").contains("draft"), "{}", note("proof"));
        // quiet comes after the installed font sets and the built-in fonts
        assert_eq!(place(&held, "quiet"), Some(held.inputs.fonts.len()));
    }

    /// A look card's font set line offers a font set only the draft adds,
    /// marked so, and leaves out one the draft removes; a look that names
    /// a removed font set shows it with a note that says so.
    ///
    /// References:
    /// - ADR-1403 D8, "The frits a draft sees": the installed frits plus the folder's, minus each removed one, and a look card offers them.
    #[test]
    fn a_look_card_offers_the_font_sets_the_draft_sees() {
        use super::super::card::{Line, choices};
        let held = shared(&["quiet"], &["proof"]);
        let look = super::super::list::look(&held, "proof").unwrap_or_else(|| panic!("look proof"));
        let offered = choices(&held, &look, Line::FontSet);
        let note = |name: &str| {
            offered
                .iter()
                .find(|c| c.name == name)
                .map(|c| c.note.clone())
                .unwrap_or_else(|| panic!("{name} among {offered:?}"))
        };
        assert_eq!(note("quiet"), words::IN_DRAFT);
        assert_eq!(note("tiny5"), "");
        // proof is the look's own value only, with the note
        assert_eq!(offered.iter().filter(|c| c.name == "proof").count(), 1);
        assert_eq!(note("proof"), words::CHOICE_REMOVED);
    }

    /// `r` keeps no removal of a font set that only a category folder of
    /// vtium's catalog holds, since the install removes only from the
    /// frits' directory itself, and it says why in `remove-frit`'s words.
    /// A font set of the same name in the frits' directory itself is
    /// removed as any other.
    ///
    /// References:
    /// - ADR-1403 D8, "Every font set change is kept at once": `r` and `remove-frit` refuse the same changes.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn r_refuses_a_set_of_the_catalog_as_remove_frit_does() {
        let dir = crate::scratch::Scratch::new("fontsets-catalog");
        std::fs::create_dir_all(dir.join("console")).expect("a category folder");
        std::fs::write(dir.join("console/cat.vtfrit"), "source=builtin:unifont\n")
            .expect("a set of the catalog");
        let mut held = shared(&[], &[]);
        held.inputs.frits = dir.to_path_buf();
        // the TUI lists the catalog's sets among the installed ones
        held.inputs
            .installed
            .insert("cat".to_owned(), b"source=builtin:unifont\n".to_vec());
        let why = crate::commands::removal_refusal(&dir, "cat").expect("the catalog's set");
        let (edit, said) = r(&held, "cat");
        assert_eq!(edit, None, "{said}");
        assert_eq!(said, words::remove_refused("cat", &why));
        std::fs::write(dir.join("cat.vtfrit"), "source=builtin:unifont\n").expect("a set");
        assert_eq!(r(&held, "cat").0, Some(FontSetEdit::Remove));
    }
}
