//! A font's detail, one line a paragraph: the file and its folder, each
//! face the font helper found in it with what the face covers and the
//! sizes it supplies, whether the font is active, its trust mark and the
//! check it fails, a duplicate mark, and the looks that use it. Every text
//! that came from a file's name or from the helper passes through
//! [`crate::onscreen`], so it shows as text.
//!
//! References:
//! - ADR-1205 D8: a font's view shows what it covers and the sizes it
//!   supplies.
//! - ADR-1403 D10: a detail is read-only, and Enter on a font from the
//!   person's own folders asks to install it as a system font.

use std::path::Path;

use crate::batch::{Places, Through, Use};
use crate::describe::Face;
use crate::discover::Described;
use crate::onscreen::{path as shown, text};

use super::super::super::fonts_found::{FontFile, squashed};
use super::super::super::model::Shared;
use super::super::super::words;

/// The looks of the draft that use the font at `path`: those whose font
/// set names the file by its path, and those whose font set names by its
/// family a face the font helper found in the file. A font set is read as
/// the install would leave it, so a new set that waits for the install
/// counts too.
///
/// References:
/// - ADR-1205 D8: deactivating a font warns and lists the looks that use
///   it.
pub fn uses(shared: &Shared, path: &Path) -> Vec<Use> {
    let conf = shared.conf_text().unwrap_or_default();
    let mut out = crate::batch::uses(
        path,
        conf,
        &Places {
            frits: Some(&shared.inputs.frits),
            palettes: &[],
            vtlook: &|_| Vec::new(),
        },
    );
    let families: Vec<String> = shared
        .inputs
        .font_files
        .faces(path)
        .iter()
        .map(|face| squashed(&face.family))
        .collect();
    if families.is_empty() {
        return out;
    }
    for (look, frit) in drawn_frits(conf) {
        if out.iter().any(|u| u.look == look) || !names_family(shared, &frit, &families) {
            continue;
        }
        out.push(Use {
            look,
            through: Through::FontSet(frit),
        });
    }
    out
}

/// Whether the font set `name`, as the install would leave it, has a
/// `font:` source of one of `families`, each with case and blanks
/// ignored.
fn names_family(shared: &Shared, name: &str, families: &[String]) -> bool {
    let text = shared.frit_text(name);
    let Some(frit) = text.and_then(|t| crate::frit::Frit::parse(t, Path::new(name), name).ok())
    else {
        return false;
    };
    frit.sources.iter().any(|source| {
        matches!(&source.origin, crate::frit::Origin::Font(family)
            if families.contains(&squashed(family)))
    })
}

/// Each look of vtium.conf's text `conf` that draws with a font set, with
/// that set's name, the top level first: the top level when no head names
/// a look with a section of its own, then each look, which takes the top
/// level's `font-name` when it names none. This is the order and the rule
/// [`crate::batch::uses`] counts looks by.
fn drawn_frits(conf: &str) -> Vec<(Option<String>, String)> {
    use vtium_settings::keys::FONT_NAME;
    let top = vtium_settings::setting(conf, FONT_NAME.name);
    let Some(reading) = vtium_settings::sections(conf.as_bytes()) else {
        return top.map(|t| (None, t)).into_iter().collect();
    };
    let config = reading.parsed.config;
    let mut out = Vec::new();
    let bare = config
        .heads
        .iter()
        .any(|h| !config.looks.iter().any(|l| l.name == h.look));
    if (config.heads.is_empty() || bare)
        && let Some(top) = &top
    {
        out.push((None, top.clone()));
    }
    for look in config.looks {
        let frit = look
            .settings
            .iter()
            .rev()
            .find(|s| s.key.name == FONT_NAME.name)
            .map(|s| s.value.clone())
            .or_else(|| top.clone());
        if let Some(frit) = frit {
            out.push((Some(look.name), frit));
        }
    }
    out
}

/// The lines that describe `face`, the sizes it supplies included.
fn face_lines(face: &Face, format: &str) -> Vec<String> {
    let name = words::face_name(&text(&face.family), &text(&face.style));
    let version = face.version.as_deref().map(text);
    let strikes: Vec<String> = face.strikes.iter().map(words::strike_cell).collect();
    let mut out = vec![
        words::font_face(&name, version.as_deref()),
        words::font_kind(format, face.outline, face.monospaced),
        words::font_covers(face.codepoints, &face.scripts),
    ];
    if !face.groups.is_empty() {
        let groups: Vec<&str> = face.groups.iter().map(|g| g.name()).collect();
        out.push(words::font_groups(&groups));
    }
    out.push(if face.outline && !strikes.is_empty() {
        words::font_outline_strikes(&strikes)
    } else {
        words::font_strikes(&strikes)
    });
    out
}

/// The detail of `file`, one line a paragraph, its title first.
pub fn lines(shared: &Shared, file: &FontFile) -> Vec<String> {
    let r = &file.resource;
    let mut out = vec![words::font_title(&text(&r.name()))];
    // the install the question asks about comes first, so it shows while
    // the question is open: both files on a clash, then the exact command
    //
    // References:
    // - ADR-1205 D5, its note of 2026-11-18: both files, each with its
    //   size, date, family and version, and the exact command.
    if let Some(ready) = shared
        .font_install
        .as_ref()
        .filter(|ready| ready.source == shown(&r.path))
    {
        if let Some((installed, yours)) = &ready.facts {
            out.push(words::SYSTEM_FONT_CLASH.to_owned());
            out.push(words::system_font_installed_file(installed));
            out.push(words::system_font_yours(yours));
        }
        out.push(words::system_font_command(&ready.source, &ready.target));
        out.push(format!("  {}", ready.command));
    }
    out.push(words::detail_file(Some(&shown(&r.path))));
    out.push(words::font_folder(file.place));
    match shared.inputs.font_files.described(&r.path) {
        None => out.push(words::FONT_DESCRIBING.to_owned()),
        Some(Described::Font(d)) => {
            for face in &d.faces {
                out.extend(face_lines(face, d.format.name()));
            }
        }
        Some(Described::NotAFont(why) | Described::Unknown(why)) => {
            out.push(words::font_not_described(&text(why)));
        }
    }
    if r.own {
        out.push(words::FONT_OWN.to_owned());
    } else {
        out.push(words::detail_state(words::active_mark(r.state)));
        out.push(words::detail_trust(
            r.kind,
            r.trust.as_ref().err().map(|why| text(why)).as_deref(),
        ));
    }
    if let Some(duplicate) = &file.duplicate {
        out.push(words::font_duplicate(&duplicate.words()));
    }
    let users: Vec<String> = uses(shared, &r.path)
        .iter()
        .map(|u| words::look_name(u.look.as_deref()))
        .collect();
    out.push(words::detail_users(&users));
    out
}
