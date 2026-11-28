//! What the Effects tab says on screen. Each fact the old tab gave in a
//! paragraph is one line here, and the rest sits on the detail screen or
//! behind F1. Every key a line names is in brackets.
//!
//! References:
//! - ADR-1205 D2: the words effect, color map, shader, active, Activate
//!   and Deactivate.
//! - crates/AGENTS.md, strings on screen, rules 7 and 8: one line where
//!   a paragraph was, and each key in brackets.

use vtium_config_core::found::effects::{Dialect, From, SPATIAL_SLICE};

/// The tab's name on the top row.
pub const EFFECTS: &str = "Effects";

/// The groups of the list, each as its heading and its choice in the list
/// of groups name it.
pub const VTLOOKS: &str = "vtlooks";
pub const COLOR_MAPS: &str = "Color maps";
pub const SHADERS: &str = "Shaders";

/// The choices of the list of groups, in the order of
/// `vtium_config_core::found::effects::Group::ALL`.
pub const GROUP_CHOICES: [&str; 4] = [
    "All effects",
    "vtlooks (look files)",
    "Color maps",
    "Shaders",
];

/// The groups as the filter line names them, in the order of
/// `vtium_config_core::found::effects::Group::ALL`.
pub const GROUP_SHORT: [&str; 4] = ["All", "vtlooks", "Color maps", "Shaders"];

/// The filter line, which names the groups the list shows.
pub fn show(groups: &str) -> String {
    format!("Show: {groups} ▾")
}

/// What the filter line adds while the `?` search filters the list.
pub fn matching(entry: &str) -> String {
    format!(" · matching {entry}")
}

/// Where the list stands, at the filter line's right, while the list is
/// longer than the rows that show it: `first`–`last` of `of`.
pub fn window(first: usize, last: usize, of: usize) -> String {
    format!("{first}–{last} of {of}")
}

/// A group with nothing in it.
pub const NONE_INSTALLED: &str = "none installed";

/// The mark beside the shaders' heading: the spatial layer they run in is
/// planned.
pub fn shaders_planned() -> String {
    format!("planned (slice {SPATIAL_SLICE})")
}

/// The name the list gives the built-in vtlook.
pub const NO_FILTER: &str = "No filter";

/// Where a vtlook comes from, short for its row and its chain's heading.
pub const fn from_short(from: From) -> &'static str {
    match from {
        From::BuiltIn => "built in",
        From::Preset => "preset",
        From::Administrator => "admin's",
    }
}

/// Where a vtlook comes from, whole, for its detail.
pub const fn from_whole(from: From) -> &'static str {
    match from {
        From::BuiltIn => "built into vtium",
        From::Preset => "preset",
        From::Administrator => "administrator's",
    }
}

/// The marks a vtlook's row adds after where it comes from.
pub const HIDDEN: &str = "hidden";
pub const ERROR: &str = "error";
pub const PLANNED: &str = "planned";

/// A resource's marks in the list and beside it.
pub const ACTIVE: &str = "● active";
pub const NOT_ACTIVE: &str = "○ not active";
pub const ACTIVE_WORD: &str = "active";
pub const NOT_ACTIVE_WORD: &str = "not active";
pub const OWN: &str = "your own";
pub const TRUST: &str = "trust";
pub const TRUSTED: &str = "trusted";
pub const FAILS_TRUST: &str = "fails trust";

/// The chain's ends and the turn between the layers, in the order vtium
/// runs a vtlook.
pub const FRAME: &str = "frame";
pub const TURN: &str = "turn";
pub const SCREEN: &str = "screen";
pub const SPATIAL_NONE: &str = "spatial: none";
pub const ARROW: &str = " → ";

/// A planned filter's name in the chain.
pub fn chain_planned(name: &str) -> String {
    format!("{name} (planned)")
}

/// A filter vtium neither builds nor plans, in the chain.
pub fn chain_unknown(name: &str) -> String {
    format!("{name} (unknown)")
}

/// What a planned filter's line adds: the slice of ADR-1304 that builds
/// it.
pub fn planned_slice(slice: u8) -> String {
    format!("planned, ADR-1304 slice {slice}")
}

/// What an unknown filter's line says.
pub const UNKNOWN: &str = "not a filter vtium knows";

/// The line of a vtlook with no filter.
pub const NO_FILTERS: &str = "no filters: changes no color";

/// The one line a vtlook that does not read gets beside the list; F1 and
/// the detail give the error whole.
pub fn does_not_read(error: &str) -> String {
    format!("does not read, draws No filter: {error}")
}

/// The color maps a vtlook names, in one line.
pub fn maps(names: &[String]) -> String {
    format!("maps       {}", names.join(", "))
}

/// The looks that use an effect, after its name.
pub fn used_by(looks: &[String]) -> String {
    if looks.is_empty() {
        "used by no look".to_owned()
    } else {
        format!("used by {}", looks.join(", "))
    }
}

/// What a resource's line of looks says when no look uses it.
pub const NO_LOOK: &str = "no look";

/// The name of a look, `None` for the top level of vtium.conf.
pub fn look_name(look: Option<&str>) -> String {
    look.map_or_else(|| "the top level".to_owned(), str::to_owned)
}

/// The picture's frame title for an effect.
pub fn on_sample(name: &str) -> String {
    format!("{name}, on the sample screen")
}

/// The picture's frame title on the filter line.
pub const SAMPLE_PLAIN: &str = "the sample screen, with no filter";

/// The kinds a detail's title names.
pub const VTLOOK: &str = "vtlook";
pub const COLOR_MAP: &str = "Color map";
pub const SHADER: &str = "Shader";

/// A shader's dialect, beside it and in its detail.
pub fn dialect(dialect: Dialect) -> String {
    match dialect {
        Dialect::Vtium => "vtium's own format".to_owned(),
        Dialect::Ghostty => format!("{}: Ghostty's", planned_slice(11)),
    }
}

/// What a shader's line says about when it runs: on the GPU path, once
/// the spatial layer is built.
pub fn shader_runs() -> String {
    format!("{}, GPU only", planned_slice(SPATIAL_SLICE))
}

/// The labels of the facts beside the list and in a detail.
pub const FILE: &str = "file";
pub const STATE: &str = "state";
pub const USED_BY: &str = "used by";
pub const DIALECT: &str = "dialect";
pub const RUNS: &str = "runs";

/// A detail's labels, capitalized as the book's details write them.
pub const D_FILE: &str = "File";
pub const D_READS: &str = "Reads";
pub const D_STATE: &str = "State";
pub const D_TRUST: &str = "Trust";
pub const D_DIALECT: &str = "Dialect";
pub const D_RUNS: &str = "Runs";
pub const D_REQUIRES: &str = "Requires";
pub const D_DISPLAYS: &str = "Displays";

/// A built-in vtlook's file.
pub const BUILT_IN_FILE: &str = "none: built into vtium";

/// A vtlook that reads.
pub const READS: &str = "without errors";

/// A vtlook that does not read, before its error whole.
pub const DOES_NOT_READ: &str = "does not read: 1 error";

/// What the detail of a vtlook that does not read says it does.
pub const FALLS_BACK: &str =
    "A head that names this vtlook draws the built-in vtlook instead, and its log says where.";

/// What a hidden preset's file line adds, and an administrator's vtlook
/// that hides one.
pub const HIDDEN_BY: &str = "hidden by the administrator's vtlook of this name";
pub const HIDES: &str = "hides the preset of this name";

/// The detail's headings of the two layers.
pub const COLOR_LAYER: &str = "Color layer, in order";
pub const SPATIAL_LAYER: &str = "Spatial layer, in order";
pub const SPATIAL_LAYER_NONE: &str = "Spatial layer   none";
pub const COLOR_LAYER_NONE: &str = "Color layer     none";

/// The heading of the color maps a vtlook names.
pub const MAPS_NAMED: &str = "Color maps it names";

/// Where a color map a vtlook names stands.
pub const MAP_MISSING: &str = "✗ not installed";

/// The requirements and the displays a vtlook applies to, which ADR-1304
/// slice 7 builds.
pub fn requires_planned() -> String {
    planned_slice(7)
}

/// What a preset's detail says about changing it.
pub const CHANGE_PRESET: &str =
    "To change a preset, copy it to /etc/vtium/looks under the same name: vtium reads the copy.";

/// A resource in the person's own folder, in its detail.
pub fn own_detail(kind: &str) -> String {
    format!(
        "in your own folder, so vtium cannot use it: copy it as root into {}, then activate it",
        system_folder(kind)
    )
}

/// Where root installs a resource of `kind`.
fn system_folder(kind: &str) -> &'static str {
    if kind == "shader" {
        "/etc/vtium/shaders"
    } else {
        "/etc/vtium/looks"
    }
}

/// Space on the filter line.
pub const SPACE_ON_FILTER: &str =
    "[Space] activates a color map or shader: move down to one first.";

/// Space on a vtlook, which is no file to activate.
pub const SPACE_ON_VTLOOK: &str =
    "A vtlook needs no activating: a look uses one by naming it on the look's card.";

/// Space on a resource in the person's own folder.
pub fn space_own(kind: &str, path: &str) -> (String, String) {
    (
        format!("This {kind} is in your own folder, so vtium cannot use it. [F1] says how."),
        format!(
            "This {kind} is in your own folder, so vtium cannot use it. Copy it as root into {}, then activate it here. {path}",
            system_folder(kind)
        ),
    )
}

/// Space on a resource that fails the trust rule.
pub fn space_untrusted(kind: &str, why: &str) -> (String, String) {
    (
        format!("vtium cannot activate this {kind}: it fails the trust rule. [F1] says why."),
        format!(
            "vtium cannot activate this {kind}, because it fails the trust rule: the file and each folder above it must belong to root, and nobody else may write them. {why}"
        ),
    )
}

/// The question before a resource that looks use is deactivated: what it
/// names, then what each look draws once the change is saved.
pub fn deactivate_question(kind: &str, name: &str, uses: &[(String, Option<String>)]) -> String {
    let mut said = Vec::new();
    for (look, through) in uses {
        match through {
            Some(vtlook) => said.push(format!("{look} uses it through the vtlook {vtlook}")),
            None => said.push(format!("{look} uses it")),
        }
    }
    format!(
        "Deactivate the {kind} {name}? {}; once saved, those looks draw with no filter from the next login.",
        said.join("; ")
    )
}

/// The name of the change that activates or deactivates a resource, as
/// Undo and the message line name it.
pub fn activate(name: &str) -> String {
    format!("activate {name}")
}
pub fn deactivate(name: &str) -> String {
    format!("deactivate {name}")
}

/// What Find says an item is.
pub fn find_vtlook(from: From) -> String {
    format!("vtlook, {}", from_whole(from))
}
pub const FIND_MAP: &str = "color map";
pub const FIND_SHADER: &str = "shader";
