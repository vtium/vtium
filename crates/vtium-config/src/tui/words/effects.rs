//! What the Effects tab says: the tab's name, its filter line and groups,
//! each vtlook's marks, the detail of a vtlook, a color map or a shader,
//! the sentences under the list that say what the keys do, and the line
//! beside the preview, which says what the picture shows and, where it
//! shows the console without the effect, why.
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
pub const EFFECTS_HINT: &str = "Space activates or deactivates a color map or shader. Enter opens an item's detail, or the list of groups.";

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

/// The line beside the preview of an effect in a terminal that shows no
/// pictures.
pub const EFFECT_NO_PICTURES: &str =
    "This terminal does not support images, so vtium-config cannot show effects.";

/// The line beside the preview of the filter line or of No filter.
pub const EFFECT_PLAIN: &str = "The picture shows the sample screen with no filter.";

/// The line beside the preview of a vtlook.
pub fn effect_vtlook(name: &str) -> String {
    format!("The picture shows the sample screen through the vtlook {name}.")
}

/// The line beside the preview of a color map.
pub fn effect_map(name: &str) -> String {
    format!("The picture shows the sample screen through the color map {name}.")
}

/// What the line adds for a vtlook whose spatial filters the picture
/// leaves out.
pub const EFFECT_COLOR_ONLY: &str =
    "It leaves out the vtlook's spatial filters, which only the GPU draws.";

/// The line beside the preview of an effect that vtium cannot use, with
/// why, a phrase that follows "because".
pub fn effect_without(why: &str) -> String {
    format!("The picture shows the sample screen without this effect, because {why}.")
}

/// The line beside the preview of an effect whose colors vtium cannot
/// build, with the first error of the vtlook reader.
pub fn effect_failed(error: &str) -> String {
    let error = error.trim_end_matches('.');
    format!(
        "The picture shows the sample screen without this effect, because vtium cannot build its colors: {error}."
    )
}

/// Why the picture shows no shader.
pub const EFFECT_SHADER: &str =
    "a shader runs only on the GPU, and vtium-config draws this picture on the CPU";

/// Why the picture shows no vtlook that does not read.
pub const EFFECT_VTLOOK_BROKEN: &str =
    "this vtlook does not read, so vtium renders the built-in vtlook in its place";

/// Why vtium cannot use a color map, in the words of its mark.
pub const MAP_OWN: &str = "is in your own folder, where vtium cannot use it";
pub const MAP_INACTIVE: &str = "is not active, so vtium treats it as missing";
pub const MAP_UNTRUSTED: &str = "fails the trust rule, so vtium treats it as missing";
pub const MAP_MISSING: &str = "is not installed";

/// Why the picture shows no highlighted color map.
pub fn map_unusable(why: &str) -> String {
    format!("this color map {why}")
}

/// Why the picture shows no vtlook that names a color map vtium cannot
/// use.
pub fn vtlook_map_unusable(map: &str, why: &str) -> String {
    format!(
        "this vtlook names the color map {map}, which {why}, so vtium renders the built-in vtlook in its place"
    )
}
