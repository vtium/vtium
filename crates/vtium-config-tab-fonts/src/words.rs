//! What the Fonts tab says on screen: the filter line and its count, the
//! groups' headings, the columns of a font's row, the labels of its facts
//! and detail, and the messages of Space, Try live and the install of a
//! font of your own.
//!
//! The marks a font carries, `● active` and the rest, are vtium-config-core's
//! ([`vtium_config_core::fontuse::State::mark`]), so every screen shows
//! them in the same words. This module adds only the built-in font's
//! mark and the short forms a narrow column takes.
//!
//! References:
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   the marks, their words and their roles, and the figure's cut of
//!   `not active · fails trust` to `not active · trust`.
//! - crates/AGENTS.md, "Strings on screen", rules 7 and 8: one line where
//!   a paragraph was, and every key in brackets.

use std::fmt::Display;

use vtium_config_core::discover::Place;
use vtium_config_core::fontuse::{State, User};
use vtium_config_core::found::fonts::Filter;
use vtium_config_widgets::roles::Role;
use vtium_font_helper::describe::{Format, Group as FontGroup};

/// The tab's name on the top row.
pub const FONTS: &str = "Fonts";

/// The built-in fonts' mark: they need no batch.
pub const ALWAYS: (&str, Role) = ("● always active", Role::Active);

/// The name of the filter that keeps every font.
pub const ALL_FONTS: &str = "All fonts";

/// The filter's name, as the filter line and the list of filters show it.
pub fn filter_name(filter: Filter) -> String {
    match filter {
        Filter::All => ALL_FONTS.to_owned(),
        Filter::Bitmap => "Bitmap fonts".to_owned(),
        Filter::Outline => "Outline fonts".to_owned(),
        Filter::Monospaced => "Monospaced fonts".to_owned(),
        Filter::Proportional => "Proportional fonts".to_owned(),
        Filter::Group(g) => match g {
            FontGroup::NerdFonts => "Nerd Fonts",
            FontGroup::Powerline => "Powerline fonts",
            FontGroup::ColorEmoji => "Color emoji fonts",
            FontGroup::BoxDrawing => "Box-drawing fonts",
        }
        .to_owned(),
        Filter::Script(s) => format!("Fonts for {}", s.replace('_', " ")),
    }
}

/// The filter line's left part: which filter the list shows.
pub fn show(filter: Filter) -> String {
    format!(" Show: {} ▾", filter_name(filter))
}

/// "1 font" or "N fonts".
pub fn fonts(n: usize) -> String {
    match n {
        1 => "1 font".to_owned(),
        n => format!("{n} fonts"),
    }
}

/// The filter line's count: how many fonts the list shows and how many of
/// them are active.
pub fn count(shown: usize, active: usize) -> String {
    format!("{} · {active} active", fonts(shown))
}

/// The filter line's count while the search filters the list.
pub fn count_of(shown: usize, of: usize) -> String {
    format!("{shown} of {}", fonts(of))
}

/// What the filter line adds while the search filters the list.
pub fn matching(entry: &str) -> String {
    format!(" · matching {entry}")
}

/// The groups the list shows its fonts in, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    System,
    Vtium,
    Console,
    FontPaths,
    User,
    /// A font a font set names that no file is installed for.
    NotInstalled,
    /// A file a font set names outside the folders the scan reads.
    Elsewhere,
}

impl Group {
    /// The group of the folder a font was found in.
    pub fn of(place: Place) -> Group {
        match place {
            Place::System => Group::System,
            Place::Vtium => Group::Vtium,
            Place::Console => Group::Console,
            Place::FontPaths => Group::FontPaths,
            Place::User => Group::User,
        }
    }

    /// The group's heading in the list.
    pub fn heading(self) -> &'static str {
        match self {
            Group::System => "System fonts",
            Group::Vtium => "vtium's own fonts",
            Group::Console => "Console fonts (kbd)",
            Group::FontPaths => "Folders that font-paths names",
            Group::User => "Your own fonts",
            Group::NotInstalled => "Not installed",
            Group::Elsewhere => "Outside the font folders",
        }
    }

    /// What the heading adds after it, dim: why the group's fonts are
    /// listed although the scan did not find them.
    pub fn aside(self) -> Option<&'static str> {
        match self {
            Group::NotInstalled | Group::Elsewhere => Some("a font set names it"),
            _ => None,
        }
    }
}

/// The kind column of a font's row: `outline`, `bitmap` or `color`, what
/// the font helper found, or that it has not answered yet.
pub fn kind(faces: Option<(bool, bool)>) -> &'static str {
    match faces {
        None => "…",
        Some((_, true)) => "color",
        Some((true, false)) => "outline",
        Some((false, false)) => "bitmap",
    }
}

/// The mark of `state` cut for a column of `cells`: the whole mark where
/// it fits, else its short form, as the Fonts tab's figure cuts `○ not
/// active · fails trust` to `○ not active · trust`.
pub fn short_mark(state: &State, cells: usize) -> Vec<(&'static str, Role)> {
    let whole = state.mark();
    let width: usize = whole.iter().map(|(w, _)| w.chars().count()).sum();
    if width <= cells {
        return whole;
    }
    match state {
        State::TurnedOff(_) => vec![("○ turned off", Role::Inactive)],
        State::FailsTrust { own: false, .. } => vec![
            ("○ not active · ", Role::Inactive),
            ("trust", Role::Warning),
        ],
        _ => whole,
    }
}

/// Who uses a font, as `big`, `the top level`, or `big's nerd-symbols
/// layer`, joined, with the rest counted once more than `most` are named.
pub fn users(users: &[User], most: usize) -> String {
    let mut names: Vec<String> = Vec::new();
    for u in users {
        let who = match &u.look {
            Some(look) => look.clone(),
            None => "the top level".to_owned(),
        };
        let name = match &u.layer {
            Some(layer) => format!("{who}'s {layer} layer"),
            None => who,
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        return "no look".to_owned();
    }
    let more = names.len().saturating_sub(most);
    names.truncate(most.max(1));
    let mut out = names.join(", ");
    if more > 0 {
        out.push_str(&format!(" and {more} more"));
    }
    out
}

/// The name of the look first among `users`, as a font's row names it
/// beside a mark that is not active.
pub fn first_look(users: &[User]) -> Option<String> {
    users.first().map(|u| match &u.look {
        Some(look) => look.clone(),
        None => "top level".to_owned(),
    })
}

/// A face's name: its family, and its style where it states one.
pub fn face_name(family: &str, style: &str) -> String {
    if style.is_empty() {
        family.to_owned()
    } else {
        format!("{family} {style}")
    }
}

/// A font file's format, as its detail names it.
pub fn format(format: Format) -> &'static str {
    match format {
        Format::OpenType => "OpenType",
        Format::Psf => "PSF",
        Format::Bdf => "BDF",
        Format::Hex => "GNU Unifont .hex",
    }
}

/// A face's kind: drawn with outlines or bitmaps, and its spacing.
pub fn drawn(outline: bool, monospaced: bool) -> String {
    let drawn = if outline { "outline" } else { "bitmap" };
    let spacing = if monospaced {
        "monospaced"
    } else {
        "proportional"
    };
    format!("{drawn}, {spacing}")
}

/// The scripts a face covers, or that it covers none in full.
pub fn covers(scripts: &[&str]) -> String {
    if scripts.is_empty() {
        "no script in full".to_owned()
    } else {
        scripts
            .iter()
            .map(|s| s.replace('_', " "))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The sizes a face supplies: any, for outlines, else its bitmap sizes.
pub fn sizes(outline: bool, strikes: &[String]) -> String {
    match (outline, strikes.is_empty()) {
        (true, true) => "any".to_owned(),
        (true, false) => format!("any, and bitmaps for {}", strikes.join(", ")),
        (false, _) => strikes.join(", "),
    }
}

/// A bitmap size, as `8×16`.
pub fn strike(width: u32, height: u32) -> String {
    format!("{width}×{height}")
}

/// The width of the label column of the facts beside the list.
pub const LABEL: usize = 8;

/// The width of the label column of a font's detail.
pub const DETAIL_LABEL: usize = 12;

/// The facts' labels.
pub const FILE: &str = "file";
pub const FOLDER: &str = "folder";
pub const KIND: &str = "kind";
pub const COVERS: &str = "covers";
pub const SIZES: &str = "sizes";
pub const STATE: &str = "state";
pub const TRUST: &str = "trust";
pub const FROM: &str = "set by";
pub const USED_BY: &str = "used by";
pub const NAMED: &str = "named";
pub const DUPLICATE: &str = "repeats";
pub const TRUSTED: &str = "trusted";

/// The trust line of a font that fails the rule.
pub fn fails(check: &str) -> String {
    format!("fails: {check}")
}

/// What the font helper still owes, in the facts and the detail.
pub const DESCRIBING: &str = "The font helper is still describing this file.";

/// Why the font helper gives no description.
pub fn not_described(why: impl Display) -> String {
    format!("The font helper cannot describe this file: {why}")
}

/// The built-in font's kind.
pub const BUILT_IN_KIND: &str = "bitmap, monospaced, built into vtium";

/// The built-in font's state line in its facts.
pub const BUILT_IN_STATE: &str = "needs no file and no batch";

/// The fonts that fail the trust rule, the same words the facts' trust
/// line gives.
pub const TRUST_RULE: &str =
    "The trust rule: root owns the file and each folder above it, and nobody else can write them.";

/// The hint under the facts of a font that a look uses and that is not
/// active.
pub const ACTIVATE_HINT: &str = "[Space] or [A] activates it";

/// The hint under the facts of any other font that is not active.
pub const SPACE_HINT: &str = "[Space] activates it";

/// The hint under the facts of a font of your own.
pub const OWN_HINT: &str = "[Enter] opens it to install as a system font";

/// The hint under the facts of a font that is not installed.
pub fn missing_hint(named: &str) -> String {
    format!("No font file is installed for {named}.")
}

/// The first item of the list: Activate missing, with its count.
pub fn activate_row(n: usize) -> String {
    vtium_config_core::fontuse::activate::words::item(n)
}

/// The configuration's row where no font waits to be activated but some
/// cannot be: those that fail the trust rule and those that are missing.
pub fn not_all_row(name: &str, fails_trust: usize, missing: usize) -> String {
    let mut parts = Vec::new();
    if fails_trust > 0 {
        parts.push(match fails_trust {
            1 => "1 fails trust".to_owned(),
            n => format!("{n} fail trust"),
        });
    }
    if missing > 0 {
        parts.push(format!("{missing} missing"));
    }
    format!(" · {name}'s fonts: {}", parts.join(", "))
}

/// The facts of the first item: the fonts Activate missing would act on.
pub const ACTIVATES: &str = "Activates:";
pub const STAYS_OFF: &str = "Stays off on purpose:";
pub const CANNOT: &str = "Cannot be activated:";
pub const ACTIVATE_FOOT: &str = "[A] or [Enter] activates them as one change";

/// The facts beside the filter line: how many fonts the scan found,
/// what a filter leaves out, how many the font helper is still
/// describing, and what Enter does there.
pub fn filter_facts(found: usize, waiting: usize) -> Vec<String> {
    let mut out = vec![
        format!("{} found", fonts(found)),
        String::new(),
        FILTER_LEAVES_OUT.to_owned(),
    ];
    if waiting > 0 {
        out.push(String::new());
        out.push(format!("The font helper is describing {}.", fonts(waiting)));
    }
    out.push(String::new());
    out.push("[Enter] lists the filters".to_owned());
    out
}

/// What a filter other than All fonts leaves out, which the facts beside
/// the filter line wrap to their width.
pub const FILTER_LEAVES_OUT: &str =
    "A filter other than All fonts leaves out each font the font helper has not described.";

/// The list's line when the scan found no font.
pub const EMPTY: &str = "vtium-config found no font files in the folders it looks in.";

/// The list's line when no font passes the filter.
pub const NONE_PASS: &str = "No font passes this filter.";

/// The list's line when the search matches no font, and how to go on.
pub fn none_match(entry: &str) -> String {
    format!("No font's family or file name matches {entry}.")
}

/// How to go on after a search matched nothing.
pub const NONE_MATCH_HINT: &str = "[Backspace] edits the pattern; [Esc] clears it.";

/// The picture's title, by what it shows.
pub const PICTURE: &str = "picture of the console";
pub const SAMPLE: &str = "sample characters";

/// `p` where this terminal shows no pictures.
pub const P_NO_IMAGES: &str =
    "This terminal does not support images, so [p] cannot show the picture of the console.";

/// Space on the filter line.
pub const SPACE_ON_FILTER: &str =
    "[Space] activates the highlighted font. Move down to a font first.";

/// Space on the first item.
pub const SPACE_ON_ACTIVATE: &str = "[A] or [Enter] activates these fonts as one change.";

/// Space on a font of your own.
pub fn space_own(name: &str) -> String {
    format!("{name} is in your own folder, so vtium cannot use it. [Enter] opens it to install it.")
}

/// Space on a font that fails the trust rule.
pub fn space_untrusted(name: &str) -> String {
    format!("{name} fails the trust rule, so vtium cannot use it ([F1]).")
}

/// Space on a font that is not installed.
pub fn space_missing(name: &str) -> String {
    format!("{name} is not installed, so it cannot be activated.")
}

/// The change Space makes, as Undo names it.
pub fn activate_step(name: &str) -> String {
    format!("Activate {name}")
}

/// The change Space makes on an active font, as Undo names it.
pub fn deactivate_step(name: &str) -> String {
    format!("Deactivate {name}")
}

/// The question before a font a look uses is deactivated.
pub fn deactivate_question(name: &str, users: &str) -> String {
    format!("Deactivate {name}? {users} uses it and draws without it. [y] Deactivate  [n] Keep")
}

/// `t` on a row that holds no font file.
pub const TRY_NO_FONT: &str = "This line holds no font to try. Move to a font, then press [t].";

/// `t` on a font of your own.
pub fn try_own(name: &str) -> String {
    format!(
        "{name} cannot be tried: the console takes only system fonts, and this one is in your own folder."
    )
}

/// `t` on a path the console would spell another way.
pub fn try_unplain(name: &str) -> String {
    format!("{name} cannot be tried: the console does not take its path ([F1]).")
}

/// What F1 opens behind [`try_unplain`].
pub const TRY_UNPLAIN_MORE: &str =
    "A font's path must be UTF-8 text with no control character, no #, and no space at either end.";

/// `t` on a font outside the console's font folders.
pub fn try_outside(name: &str) -> String {
    format!("{name} cannot be tried: it is in none of the console's font folders ([F1]).")
}

/// What F1 opens behind [`try_outside`].
pub const TRY_OUTSIDE_MORE: &str = "The console looks in the folders that font-paths in vtium.conf names, or in the system font folders when it names none. It also looks in vtium's own font folder and in kbd's console-font folders.";

/// `t` on a font that is not active.
pub fn try_inactive(name: &str) -> String {
    format!("{name} cannot be tried: it is not active. [Space] activates it, then [Ctrl+S] saves.")
}

/// `t` on a font only the document activates.
pub fn try_not_saved(name: &str) -> String {
    format!("{name} cannot be tried yet: only the change not saved activates it. [Ctrl+S] saves.")
}

/// `t` on a font that fails the trust rule.
pub fn try_untrusted(name: &str) -> String {
    format!("{name} cannot be tried: it fails the trust rule ([F1]).")
}

/// `t` on a font that is not installed.
pub fn try_missing(name: &str) -> String {
    format!("{name} cannot be tried: it is not installed.")
}

/// Enter in the detail of a font that is a system font already.
pub const SYSTEM_FONT_NOT_YOURS: &str =
    "This font is a system font already. Only a font in your own folder can be installed as one.";

/// While the font helper checks a font before its install.
pub const CHECKING_FONT: &str = "Checking the font before the install…";

/// Why the tab has no font of your own to install now.
pub const NO_OWN_FONT: &str = "Highlight a font in Your own fonts to install it as a system font.";

/// A font of your own, in its state line.
pub const OWN_STATE: &str = "vtium uses only system fonts. Install it as one, then activate it.";

/// Where a font's folder is, as its detail says after the path.
pub fn place_words(place: Place) -> &'static str {
    match place {
        Place::System => "system folder",
        Place::Vtium => "vtium's folder",
        Place::Console => "kbd's folder",
        Place::FontPaths => "a font-paths folder",
        Place::User => "your folder",
    }
}

/// Find's place for a font of `group`.
pub fn find_place(group: Group) -> String {
    format!("Fonts › {}", group.heading())
}
