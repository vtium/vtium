//! What the controls that the resource tabs share say: the marks of a
//! resource, what Space says on one it cannot change, and the question
//! before a resource that a look uses is deactivated.
//!
//! References:
//! - ADR-1205 D2: the words on screen, active, Activate and Deactivate.

use crate::batch::State;

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

/// What Space says on a resource in the person's own folder, `path`.
pub fn own_resource(kind: &str, path: &str) -> String {
    format!(
        "This {kind} is in your own folder, so vtium cannot use it. Install it as a system file with the draft's Install step, then activate it there. {path}"
    )
}

/// What Space says on a resource that fails the trust rule, with the
/// check it fails, `why`.
pub fn untrusted(kind: &str, why: &str) -> String {
    format!("vtium cannot activate this {kind}, because it fails the trust rule. {why}")
}

/// The question before the resource at `path`, of kind `kind`, is
/// deactivated, which names the looks in `looks` that use it.
pub fn deactivate_question(kind: &str, path: &str, looks: &[String]) -> String {
    let (who, uses) = match looks {
        [one] => (one.clone(), "uses it, and renders"),
        [rest @ .., last] => (
            format!("{} and {last}", rest.join(", ")),
            "use it, and render",
        ),
        [] => (String::new(), ""),
    };
    let who = capitalized(&who);
    format!(
        "Deactivate the {kind} {path}? {who} {uses} without it once the change is installed. y deactivates it, Esc goes back."
    )
}

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
/// a resource in the draft: what changed, and that it installs with the
/// draft.
pub fn changed(change: crate::batch::Change, kind: &str) -> String {
    match change {
        crate::batch::Change::Activate => {
            format!(
                "The draft activates this {kind}. The change takes effect once you install the draft."
            )
        }
        crate::batch::Change::Deactivate => {
            format!(
                "The draft deactivates this {kind}. The change takes effect once you install the draft."
            )
        }
    }
}

/// What the message line says when the resource was already active.
pub const ALREADY_ACTIVE: &str = "This resource is already active.";
