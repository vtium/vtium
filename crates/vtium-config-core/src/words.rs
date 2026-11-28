//! The words the commands print and the TUI shows too, kept with the
//! modules that print them. The TUI's catalog, `tui::words`, re-exports
//! each of them, so the TUI names them as it names its own words.

/// Why vtium.conf cannot take an edit.
pub fn conf_edit(e: &vtium_settings::ConfEdit) -> String {
    match e {
        vtium_settings::ConfEdit::BadKey(key) => format!(
            "There is no setting named {key:?}. A setting's name uses lowercase letters, digits and -."
        ),
        vtium_settings::ConfEdit::BadValue { key, value } => format!(
            "{key} cannot be set to {value:?}: vtium would read it back as a different value."
        ),
    }
}

/// Install's question: what the install changes, the lines of vtium.conf
/// added and removed, then the `batch` changes to the active resources and
/// the `frits` font sets it writes or removes. A draft that changes no line
/// of vtium.conf names only what else it installs.
pub fn install_question(added: usize, removed: usize, batch: usize, frits: usize) -> String {
    let mut parts = Vec::new();
    if (added, removed) != (0, 0) || (batch, frits) == (0, 0) {
        parts.push(lines(added, removed));
    }
    if batch > 0 {
        parts.push(crate::batch::resource_changes(batch));
    }
    match frits {
        0 => {}
        1 => parts.push("1 font set".to_owned()),
        n => parts.push(format!("{n} font sets")),
    }
    let counted = match parts.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{}, and {last}", rest.join(", ")),
        None => String::new(),
    };
    format!("Install the draft? {counted}.")
}

/// The first half of Install's question for a draft that wires no
/// complete route, and of the refusal without a terminal (ADR-1403 D7,
/// D8).
pub const NO_ROUTE: &str = "This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches.";

/// What `vtium config install` says, with no terminal to ask and no
/// `--force`, of a draft that wires no complete route (ADR-1403 D8).
pub fn no_route_force() -> String {
    format!("{NO_ROUTE} To install it anyway, run vtium config install --force.")
}

/// What `vtium config install` says when the person answers no.
pub const NO_ROUTE_NOT_INSTALLED: &str =
    "Nothing installed: you answered no. The draft keeps your changes.";

/// Install's question for a draft whose only problem is that it wires no
/// complete route (ADR-1403 D7).
pub fn no_route_question(added: usize, removed: usize, batch: usize, frits: usize) -> String {
    let install = install_question(added, removed, batch, frits);
    let counted = install
        .strip_prefix("Install the draft? ")
        .unwrap_or(&install);
    format!(
        "{NO_ROUTE} After the install, nobody at this machine may be able to type into vtium. Install it anyway? {counted}"
    )
}

pub fn installed(name: &str) -> String {
    format!("Installed draft {name}. The console uses it after vtium restarts.")
}

/// `3 lines added, 1 removed`.
fn lines(added: usize, removed: usize) -> String {
    crate::drafts::lines(added, removed)
}

/// The command line's note on a look that no head uses: it changes
/// nothing on screen, and the command that gives it to `head`, the first
/// head vtium runs for the draft. A draft with no head at all gets the
/// commands that make a head with the look and send the displays to it.
pub fn no_head_uses(look: &str, head: Option<&str>) -> String {
    match head {
        Some(head) => format!(
            "No head uses look {look}, so it changes nothing on screen. To use it, run vtium config head set {head} --look {look}."
        ),
        None => format!(
            "No head uses look {look}, so it changes nothing on screen. The draft has no head yet. To use the look, run vtium config head add console --look {look}, then vtium config rule add 'display * -> console'."
        ),
    }
}

pub const NO_HISTORY: &str =
    "Nothing to restore: no install has replaced a vtium.conf yet, so there is no earlier file.";
