//! What the controls that the resource tabs share say: the marks of a
//! resource, what Space says on one it cannot change, and the question
//! before a resource that a look uses is deactivated. A change to the
//! batch takes effect at the next login after the install, because vtium
//! reads the batch again at each new login.
//!
//! References:
//! - ADR-1205 D2: the words on screen, active, Activate and Deactivate.
//! - ADR-1205 D4, its note of 2026-11-19: vtium reads the batch again at
//!   each new login.

use crate::batch::{State, Through, Use};

/// A resource's active mark, in the words `vtium-config active` prints.
pub fn active_mark(state: State) -> &'static str {
    state.words()
}

/// The mark of a resource that passes the trust rule.
pub const TRUSTED: &str = "trusted";

/// The mark of a resource that fails the trust rule.
pub const UNTRUSTED: &str = "fails the trust rule";

/// The mark of a resource in the person's own folder.
pub const NOT_USABLE: &str = "not usable";

/// The mark of a packaged resource of kind `kind` that the
/// administrator's file of the same name hides.
pub fn resource_hidden(kind: &str) -> String {
    format!("hidden by the administrator's {kind} of this name")
}

/// The system folder a resource of kind `kind` is copied into, so that
/// vtium can use it.
fn system_folder(kind: &str) -> &'static str {
    match kind {
        "shader" => "/etc/vtium/shaders",
        _ => vtium_settings::vtlook::LOOKS,
    }
}

/// What Space says on a resource in the person's own folder, `path`.
pub fn own_resource(kind: &str, path: &str) -> String {
    format!(
        "This {kind} is in your own folder, so vtium cannot use it. Copy it as root into {}, then activate it here. {path}",
        system_folder(kind)
    )
}

/// The line of a detail of a resource of kind `kind` in the person's own
/// folder.
pub fn detail_own(kind: &str) -> String {
    format!(
        "This {kind} is in your own folder, so vtium cannot use it. Copy it as root into {}, then activate it.",
        system_folder(kind)
    )
}

/// What Space says on a resource that fails the trust rule, with the
/// check it fails, `why`.
pub fn untrusted(kind: &str, why: &str) -> String {
    format!(
        "vtium cannot activate this {kind}, because it fails the trust rule: the file and each folder above it must belong to root, and nobody else may write them. {why}"
    )
}

/// The question before the resource at `path`, of kind `kind`, is
/// deactivated. It names the looks in `uses` that use it, and what each
/// renders once the change takes effect: a look whose vtlook names a
/// missing color map or shader renders with no filter, because vtium
/// then draws the built-in vtlook in place of the whole vtlook.
///
/// References:
/// - ADR-1304 D6: any error drops the head to the built-in vtlook.
pub fn deactivate_question(kind: &str, path: &str, uses: &[Use]) -> String {
    let mut said = Vec::new();
    let mut groups: Vec<(&Through, Vec<&Use>)> = Vec::new();
    for u in uses {
        match groups.iter_mut().find(|(t, _)| *t == &u.through) {
            Some((_, group)) => group.push(u),
            None => groups.push((&u.through, vec![u])),
        }
    }
    for (through, group) in groups {
        if *through == Through::PointerTheme {
            said.push(POINTER_THEME_NAMED.to_owned());
            continue;
        }
        let names: Vec<String> = group.iter().map(|u| look_name(u.look.as_deref())).collect();
        let (via, without) = match through {
            Through::FontSet(frit) => (format!(" through the font set {frit}"), "without it"),
            Through::Palette => (String::new(), "without it"),
            Through::Vtlook(vtlook) => (format!(" through the vtlook {vtlook}"), "with no filter"),
            Through::PointerTheme => (String::new(), ""),
        };
        let after = match group.as_slice() {
            [one] => match &one.look {
                Some(name) => format!("look {name} renders {without}"),
                None => format!("each head that uses the top level renders {without}"),
            },
            _ => format!("they render {without}"),
        };
        let (who, verb) = match names.as_slice() {
            [one] => (one.clone(), "uses"),
            [_, _, ..] => (super::looks::and_list(&names), "use"),
            [] => continue,
        };
        said.push(format!(
            "{} {verb} it{via}. From the next login after the install, {after}.",
            capitalized(&who)
        ));
    }
    format!(
        "Deactivate the {kind} {path}? {} y deactivates it, Esc goes back.",
        said.join(" ")
    )
}

/// What the deactivate question says of the pointer theme that
/// vtium.conf's `pointer-theme` names. A look cannot pick a pointer theme
/// of its own, so every head draws that theme, and draws vtium's own
/// pictures once the theme is not active.
const POINTER_THEME_NAMED: &str = "vtium.conf's pointer-theme names it. From the next login after the install, every head shows vtium's own pointer pictures in its place.";

/// `text` with its first letter in upper case, to start a sentence.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// The name a look takes in the deactivate question: `look NAME`, or the
/// top level of vtium.conf for `None`.
pub fn look_name(look: Option<&str>) -> String {
    match look {
        Some(name) => format!("look {name}"),
        None => "the top level of vtium.conf".to_owned(),
    }
}

/// What the message line says once the desk has activated or deactivated
/// a resource of kind `kind` in the draft: what changed, and when the
/// change takes effect.
pub fn changed(change: crate::batch::Change, kind: &str) -> String {
    let does = match change {
        crate::batch::Change::Activate => "activates",
        crate::batch::Change::Deactivate => "deactivates",
    };
    format!(
        "The draft {does} this {kind}. The change takes effect at the next login after you install the draft."
    )
}

/// What the message line says when the resource of kind `kind` was
/// already active.
pub fn already_active(kind: &str) -> String {
    format!("This {kind} is already active.")
}

/// What the message line says when the draft already activates the
/// resource of kind `kind`.
pub fn already_activates(kind: &str) -> String {
    format!(
        "The draft already activates this {kind}. It becomes active when you install the draft."
    )
}
