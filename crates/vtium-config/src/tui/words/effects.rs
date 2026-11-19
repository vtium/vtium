//! What the Effects tab says: the tab's name, its filter line and groups,
//! each vtlook's marks, the detail of a vtlook, a color map or a shader,
//! and the sentences under the list that say what the keys do.
//!
//! References:
//! - ADR-1205 D2: the words on screen, effect, color map, shader, active,
//!   Activate and Deactivate.

/// The Effects tab's name in the tab bar.
pub const EFFECTS: &str = "Effects";

/// The Effects tab's keys, as the Help tab lists them.
pub const KEYS_EFFECTS: &str =
    "Space activate  Enter open  Tab next view  s save  i install  q quit";

/// The groups of the Effects tab's list, in their order, each the name the
/// filter line and its list show.
pub const EFFECTS_GROUPS: [&str; 3] = ["vtlooks", "Color maps", "Shaders"];

/// The filter line's choice that shows every group.
pub const EFFECTS_ALL: &str = "All effects";

/// The filter line, which names the group or groups the list shows.
pub fn effects_filter(shown: &str) -> String {
    format!("Show: {shown}")
}

/// The name the list gives the built-in vtlook.
pub const NO_FILTER: &str = "No filter";

/// The marks of a vtlook in the list.
pub const VTLOOK_BUILT_IN: &str = "built in, changes no color";
pub const VTLOOK_PRESET: &str = "preset";
pub const VTLOOK_ADMIN: &str = "administrator's";

/// The mark of a preset that the administrator's vtlook of the same name
/// hides.
pub const VTLOOK_HIDDEN: &str = "hidden by the administrator's vtlook of this name";

/// The mark of a vtlook that does not read.
pub const VTLOOK_BROKEN: &str = "does not read";

/// What the tab says under the list, while nothing is open over it.
pub const EFFECTS_HINT: &str = "Space activates or deactivates the highlighted color map or shader. Enter opens the highlighted item's detail, or the list of groups on the filter line.";

/// What the tab says under the list of groups.
pub const EFFECTS_FILTER_HINT: &str = "Press Enter to show the highlighted group.";

/// What the tab says under a detail.
pub const EFFECTS_DETAIL_HINT: &str = "This detail changes nothing. Esc goes back to the list.";

/// What the tab says when a group holds nothing.
pub fn effects_none(group: &str) -> String {
    format!("No {} are installed.", group.to_lowercase())
}

/// The first line of a vtlook's detail.
pub fn vtlook_title(name: &str) -> String {
    format!("vtlook {name}")
}

/// The line of a detail that names its file.
pub fn detail_file(path: Option<&str>) -> String {
    match path {
        Some(path) => format!("File: {path}"),
        None => "File: none, it is built into vtium".to_owned(),
    }
}

/// The line of a vtlook's detail that says whether it reads.
pub fn vtlook_reads(error: Option<&str>) -> String {
    match error {
        None => "It reads without errors.".to_owned(),
        Some(why) => {
            format!("It does not read, so vtium renders the built-in vtlook in its place. {why}")
        }
    }
}

/// The line above a vtlook's filters.
pub const VTLOOK_FILTERS: &str = "Its filters, in the order vtium applies them:";

/// The line of a vtlook with no filter.
pub const VTLOOK_NO_FILTERS: &str = "It has no filter, so it changes no color.";

/// The line above the color maps and shaders a vtlook names.
pub const VTLOOK_FILES: &str = "The color maps and shaders it names:";

/// A color map or shader that a vtlook names, with whether it is active.
pub fn vtlook_file(path: &str, active: bool) -> String {
    if active {
        format!("{path}, active")
    } else {
        format!("{path}, not active, so vtium renders the built-in vtlook in its place")
    }
}

/// The first line of a color map's or shader's detail.
pub fn resource_title(kind: &str, name: &str) -> String {
    format!("{kind} {name}")
}

/// The line of a detail that says whether the resource is active.
pub fn detail_state(state: &str) -> String {
    format!("Active: {state}")
}

/// The line of a detail that says whether the resource passes the trust
/// rule, and the check it fails.
pub fn detail_trust(why: Option<&str>) -> String {
    match why {
        None => "It passes the trust rule.".to_owned(),
        Some(why) => format!("It fails the trust rule, so vtium treats it as missing. {why}"),
    }
}

/// The line of a detail of a resource in the person's own folder.
pub const DETAIL_OWN: &str = "It is in your own folder, so vtium cannot use it. Install it as a system file with the draft's Install step, then activate it.";

/// The line of a detail that names the looks that use the item.
pub fn detail_users(looks: &[String]) -> String {
    if looks.is_empty() {
        "No look uses it.".to_owned()
    } else {
        format!("Used by {}.", looks.join(", "))
    }
}
