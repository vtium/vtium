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
/// filter line and its list show. The first heading says what a vtlook
/// is, the first time the tab shows the word.
pub const EFFECTS_GROUPS: [&str; 3] = ["vtlooks (filter files)", "Color maps", "Shaders"];

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

/// The mark of a vtlook that has an error.
pub const VTLOOK_BROKEN: &str = "has an error";

/// What the tab says under the list, while nothing is open over it.
pub const EFFECTS_HINT: &str = "Space activates or deactivates the highlighted color map or shader. Enter opens its detail, or the groups on the filter line.";

/// What the tab says under the list of groups.
pub const EFFECTS_FILTER_HINT: &str = "Press Enter to show the highlighted group.";

/// What the tab says under a detail.
pub const EFFECTS_DETAIL_HINT: &str = "This detail changes nothing. Esc goes back to the list.";

/// What the tab says when a group holds nothing.
pub fn effects_none(group: &str) -> String {
    format!("No {} are installed.", group.to_lowercase())
}

/// What the tab says when vtium-config cannot read the draft's active
/// resources, so every mark may be wrong.
pub fn effects_unread(why: &str) -> String {
    format!(
        "vtium-config cannot read which resources the draft keeps active, so every color map and shader shows as not active. {why}"
    )
}

/// The first line of a vtlook's detail.
pub fn vtlook_title(name: &str) -> String {
    format!("vtlook {name}")
}

/// The line of a detail that names its file.
pub fn detail_file(path: Option<&str>) -> String {
    match path {
        Some(path) => format!("File: {path}"),
        None => "File: none, because it is built into vtium".to_owned(),
    }
}

/// The line of a vtlook's detail that says whether it has an error, with
/// the first error, its file and line first.
pub fn vtlook_reads(error: Option<&str>) -> String {
    match error {
        None => "This vtlook reads without errors.".to_owned(),
        Some(why) => format!(
            "This vtlook has an error, so vtium renders the built-in vtlook in its place. {why}"
        ),
    }
}

/// The line above a vtlook's filters.
pub const VTLOOK_FILTERS: &str = "The vtlook's filters, in the order vtium applies them:";

/// The line of a vtlook with no filter.
pub const VTLOOK_NO_FILTERS: &str = "This vtlook has no filter, so it changes no color.";

/// The line above the color maps and shaders a vtlook names.
pub const VTLOOK_FILES: &str = "The color maps and shaders this vtlook names:";

/// Where a file that a vtlook names stands, for the vtlook's detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Named {
    /// Active, and it passes the trust rule.
    Usable,
    /// Active, but it fails the trust rule.
    Untrusted,
    /// Not active.
    Inactive,
    /// The Effects tab does not list it.
    Missing,
}

/// A color map or shader that a vtlook names, with where it stands. A
/// file vtium cannot use makes the whole vtlook give way to the built-in
/// vtlook.
pub fn vtlook_file(path: &str, named: Named) -> String {
    let instead = "so vtium renders the built-in vtlook in its place";
    match named {
        Named::Usable => format!("{path}, active"),
        Named::Untrusted => format!("{path}, active, but it fails the trust rule, {instead}"),
        Named::Inactive => format!("{path}, not active, {instead}"),
        Named::Missing => format!("{path}, not installed, {instead}"),
    }
}

/// The first line of a color map's or shader's detail.
pub fn resource_title(kind: &str, name: &str) -> String {
    format!("{kind} {name}")
}

/// The line of a detail that says whether the resource is active.
pub fn detail_state(state: &str) -> String {
    format!("State: {state}")
}

/// The line of a detail that says whether the resource of kind `kind`
/// passes the trust rule, and the check it fails.
pub fn detail_trust(kind: &str, why: Option<&str>) -> String {
    match why {
        None => format!("This {kind} passes the trust rule."),
        Some(why) => format!(
            "This {kind} fails the trust rule, so vtium treats it as missing. The rule asks that the file and each folder above it belong to root, and that nobody else can write them. {why}"
        ),
    }
}

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

/// The line where no font can be read, so no preview is drawn.
pub const EFFECT_NO_FONT: &str =
    "vtium-config cannot draw the preview, because none of the fonts can be read.";

/// The line where the terminal has no room for the preview.
pub const EFFECT_NO_ROOM: &str = "The terminal is too small to show the preview.";

/// The line where the preview failed; the message line says why.
pub const EFFECT_NOT_DRAWN: &str = "vtium-config cannot draw the preview of this effect.";

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
    "The picture leaves out the vtlook's spatial filters, which vtium draws only on the GPU.";

/// What the line adds in a vtium console that draws its own vtlook
/// `name`, which the console applies to the picture as well.
pub fn effect_console_vtlook(name: &str) -> String {
    format!("This console draws the picture through its own vtlook {name} as well.")
}

/// The line beside the preview of an effect of kind `what` that vtium
/// cannot use, with `reason`, a sentence of its own.
pub fn effect_without(what: &str, reason: &str) -> String {
    format!("The picture shows the sample screen without this {what}. {reason}")
}

/// The line beside the preview of an effect of kind `what` that vtium
/// cannot read, with the reader's first error, its file and line first.
/// A vtlook that cannot be read gives way to the built-in vtlook.
pub fn effect_failed(what: &str, error: &str) -> String {
    let error = error.trim_end_matches('.');
    let instead = if what == "vtlook" {
        " vtium renders the built-in vtlook in its place."
    } else {
        ""
    };
    format!(
        "The picture shows the sample screen without this {what}, because vtium cannot read it: {error}.{instead}"
    )
}

/// Why the picture shows no shader.
pub const EFFECT_SHADER: &str =
    "A shader runs only on the GPU, and vtium-config draws this picture on the CPU.";

/// Why the picture shows no vtlook that has an error, with the first
/// error.
pub fn effect_vtlook_broken(error: &str) -> String {
    format!("The vtlook has an error, so vtium renders the built-in vtlook in its place. {error}")
}

/// Why vtium cannot use a color map, in the words of its mark.
pub const MAP_OWN: &str = "is in your own folder";
pub const MAP_INACTIVE: &str = "is not active";
pub const MAP_UNTRUSTED: &str = "fails the trust rule";
pub const MAP_MISSING: &str = "is not installed";

/// Why the picture shows no highlighted color map.
pub fn map_unusable(why: &str) -> String {
    if why == MAP_OWN {
        format!("The color map {why}, so vtium cannot use it.")
    } else {
        format!("The color map {why}, so vtium treats it as missing.")
    }
}

/// Why the picture shows no vtlook that names a color map vtium cannot
/// use.
pub fn vtlook_map_unusable(map: &str, why: &str) -> String {
    format!(
        "The vtlook names the color map {map}, which {why}, so vtium renders the built-in vtlook in its place."
    )
}
