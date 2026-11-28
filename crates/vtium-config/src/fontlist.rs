//! The fonts vtium-config lists: the frits installed and the built-in
//! fonts, each with its state, and the layers a font set can use. The
//! commands and the TUI share them.

use std::path::Path;

use vtium_font::set::builtin;

use crate::frit::{Frit, RESERVED};

/// A font the TUI lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub name: String,
    pub state: FontState,
}

/// What a font's state is, as the TUI reads it without parsing a font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontState {
    /// A frit that reads.
    Reads,
    /// One of the console's built-in fonts.
    BuiltIn,
    /// A frit that does not read, and why.
    Refused(String),
}

/// The frits in `dir` and in its category folders, and the built-in fonts,
/// sorted by name, each with its state. A frit is read as text with the
/// layers it names, as the console reads it.
pub fn fonts(dir: &Path) -> Vec<Font> {
    let mut out: Vec<Font> = RESERVED
        .iter()
        .filter(|n| builtin(n).is_some())
        .map(|n| Font {
            name: (*n).to_owned(),
            state: FontState::BuiltIn,
        })
        .collect();
    let layers = dir.join(vtium_font::set::layer::DIR);
    for (name, path) in crate::switch::frit_files(dir) {
        if RESERVED.contains(&name.as_str()) {
            continue;
        }
        let state = match Frit::load_in(&path, &layers) {
            Ok(_) => FontState::Reads,
            Err(e) if e.line == 0 => FontState::Refused(e.why.clone()),
            Err(e) => FontState::Refused(format!("line {}: {}", e.line, e.why)),
        };
        out.push(Font { name, state });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The layers in the `layers/` folder of the frits' directory `dir`, each
/// by its name with the role its `role=` line names, in the order of their
/// names. A layer the layer reader refuses is left out, because a font set
/// cannot use it.
///
/// References:
/// - ADR-1204 D5: the layers' folder, and the roles a layer fills.
pub fn layers(dir: &Path) -> Vec<(String, vtium_font::set::layer::Role)> {
    use vtium_font::set::layer;
    let folder = dir.join(layer::DIR);
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(layer::SUFFIX)
                .map(str::to_owned)
        })
        .filter(|n| layer::valid_name(n))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let text = crate::frit::read_layer(&folder, &name).ok()??;
            let file = crate::frit::layer_file(&folder, &name);
            // the list of layers has no place to show a layer's warnings
            let (role, _) = Frit::parse_layer(text.as_bytes(), &file, &name).ok()?;
            Some((name, role))
        })
        .collect()
}
