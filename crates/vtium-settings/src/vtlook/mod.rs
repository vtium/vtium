//! vtlooks: the files that choose a head's frame filters and set their
//! parameters.
//!
//! A frame filter changes the finished frame before the display shows it,
//! as a color correction or a CRT effect does. A vtlook (a look file) is
//! `NAME.vtlook` in one of two folders: the administrator's, [`LOOKS`], and
//! the package's, [`PACKAGE_LOOKS`], which holds the presets vtium ships.
//! When both hold a file of the same name, vtium reads the administrator's,
//! so a copy of a preset in [`LOOKS`] replaces the preset, and removing the
//! copy brings the preset back. A `[look NAME]` section of
//! vtium.conf names its vtlook with `vtlook = NAME`, and vtium.conf's
//! top-level `vtlook` sets the vtlook of every look that names none. With
//! neither, a head takes the built-in vtlook, `default`, which holds no
//! filter and changes no pixel. No file can take the name `default`, so
//! the vtlook every failure falls back to can never fail itself.
//!
//! This part is in every build, so a build without frame filters still
//! reads `vtlook` and says that it draws `default`. With the `filters`
//! feature, [`read`] reads a vtlook against the filters' declarations,
//! [`Report`] says where a vtlook fails, [`table`] merges a vtlook's color
//! filters into the one color table both drawing paths read, and
//! [`template`] gives the built-in vtlook's text, which vtium-config writes
//! out to start a new vtlook.
//!
//! References:
//! - ADR-1304 D4: the vtlook file and the built-in default, and its note of 2026-11-19: the presets in the package's folder, and the administrator's folder winning on a name.
//! - ADR-1304 D5: how a head chooses its vtlook through its look section.
//! - ADR-1304 D10: the reader sits behind the `filters` feature, and the key is in every build.

#[cfg(feature = "filters")]
mod read;
#[cfg(feature = "filters")]
mod report;
#[cfg(feature = "filters")]
mod requires;
#[cfg(feature = "filters")]
mod table;
#[cfg(feature = "filters")]
mod template;

#[cfg(feature = "filters")]
pub use read::{Filter, Value, Vtlook, find, read, read_text, read_text_with};
#[cfg(feature = "filters")]
pub use report::{Failure, Part, Report, Stage, log_line};
#[cfg(feature = "filters")]
pub use requires::{Depth, Fps, Gpu, Requires, Size};
#[cfg(feature = "filters")]
pub use table::table;
#[cfg(feature = "filters")]
pub use template::{TEMPLATE, template};

/// The name of the built-in vtlook.
pub const DEFAULT: &str = "default";

/// How vtium-config's listing names the built-in vtlook. vtium's log and
/// `vtium check-config` call it the built-in vtlook.
pub const BUILT_IN: &str = "default (built in)";

/// The administrator's vtlooks folder, which vtium looks in first: set at
/// build time with `VTIUM_LOOKS`, as the font sets' folder is set with
/// `VTIUM_FONT_FRITS`.
pub const LOOKS: &str = match option_env!("VTIUM_LOOKS") {
    Some(dir) => dir,
    None => "/etc/vtium/looks",
};

/// The package's vtlooks folder, which holds the presets vtium ships and
/// which vtium looks in after [`LOOKS`]: set at build time with
/// `VTIUM_PACKAGE_LOOKS`. The install tree puts package data under
/// /usr/share/vtium whatever the prefix is.
pub const PACKAGE_LOOKS: &str = match option_env!("VTIUM_PACKAGE_LOOKS") {
    Some(dir) => dir,
    None => "/usr/share/vtium/looks",
};

/// The folders vtium reads vtlooks from, in the order it looks in them:
/// the administrator's and then the package's.
pub fn folders() -> [&'static std::path::Path; 2] {
    [LOOKS, PACKAGE_LOOKS].map(std::path::Path::new)
}

/// The suffix of a vtlook's file name.
pub const SUFFIX: &str = ".vtlook";

/// The vtlook a configured name chooses: `None` for the built-in one,
/// which an unset `vtlook` and `vtlook = default` both name.
pub fn chosen(vtlook: Option<&str>) -> Option<&str> {
    vtlook.filter(|name| *name != DEFAULT)
}

/// Whether `name` can name a vtlook: 1 to 64 of the characters
/// `A-Z a-z 0-9 . _ + -`, the rule for a head's name, so it never holds
/// a path. A look's `vtlook =` passes it when vtium.conf's sections are
/// read, and `vtium check-config` holds the top level's `vtlook` to it.
pub fn named(name: &str) -> bool {
    crate::parse::good_name(name)
}

/// What vtium logs once for head `head` when its look names the vtlook
/// `vtlook` and this build has no frame filters. The build draws every
/// head with the built-in vtlook by design, so this is a note and not an
/// error of the vtlook.
pub fn no_filters(head: &str, vtlook: &str) -> String {
    format!(
        "This build of vtium has no frame filters, so head {head} draws the built-in vtlook instead of vtlook {vtlook}."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // An unset vtlook and `vtlook = default` both choose the built-in
    // vtlook, and any other name chooses a file.
    #[test]
    fn default_and_unset_both_choose_the_built_in_vtlook() {
        assert_eq!(chosen(None), None);
        assert_eq!(chosen(Some("default")), None);
        assert_eq!(chosen(Some("amber")), Some("amber"));
    }
}
