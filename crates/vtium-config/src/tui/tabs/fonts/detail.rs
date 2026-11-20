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

use crate::batch::{Places, Use};
use crate::describe::Face;
use crate::discover::Described;
use crate::onscreen::{path as shown, text};

use super::super::super::fonts_found::FontFile;
use super::super::super::model::Shared;
use super::super::super::words;

/// The looks of the draft that use the font at `path`: those whose font
/// set names the file by its path.
pub fn uses(shared: &Shared, path: &Path) -> Vec<Use> {
    let conf = shared.conf_text().unwrap_or_default();
    crate::batch::uses(
        path,
        conf,
        &Places {
            frits: Some(&shared.inputs.frits),
            palettes: &[],
            vtlook: &|_| Vec::new(),
        },
    )
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
