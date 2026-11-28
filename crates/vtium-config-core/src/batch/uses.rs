//! The looks that use a resource, and the warnings before it is
//! deactivated.

use std::path::{Path, PathBuf};

use super::kind;

/// Where [`uses`] finds what the looks name: the font sets' folder, the
/// folders of the color schemes, and the color maps and shaders each
/// vtlook names.
pub struct Places<'a> {
    pub frits: Option<&'a Path>,
    pub palettes: &'a [&'a Path],
    /// The color maps and shaders that the vtlook of this name names, as
    /// vtium reads the vtlook, or none when it cannot be read.
    pub vtlook: &'a dyn Fn(&str) -> Vec<PathBuf>,
}

/// How a look uses a resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Through {
    /// The look's font set, of this name, names the font by its path.
    FontSet(String),
    /// The look names the color scheme.
    Palette,
    /// The look's vtlook, of this name, names the color map or shader.
    Vtlook(String),
    /// The look's `pointer-theme` names the pointer theme, or for the top
    /// level, vtium.conf's top-level `pointer-theme`, which every look that
    /// names no theme of its own draws.
    PointerTheme,
}

/// A look that uses a resource, and how. `look` is `None` for the top
/// level of vtium.conf, which a head draws with when no section has the
/// name of its look.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    pub look: Option<String>,
    pub through: Through,
}

/// One look of vtium.conf's text, or its top level, with the font set,
/// the color scheme and the vtlook it draws with.
struct Drawn {
    /// `None` for the top level.
    look: Option<String>,
    frit: Option<String>,
    palette: Option<String>,
    vtlook: Option<String>,
    /// The look's own `pointer-theme`, `None` where it takes the top
    /// level's.
    pointer: Option<String>,
}

/// What each look of vtium.conf's text `conf` draws with. A look takes
/// the top level's font set, color scheme and vtlook where it names none
/// of its own. The top level is listed too when a head draws with it,
/// because no section has the name of the head's look, or because the
/// text has no heads.
///
/// References:
/// - ADR-1304 D5: a head's vtlook comes from its look, else the top level, else the built-in one.
fn looks_of(conf: &str) -> Vec<Drawn> {
    use vtium_settings::keys::{FONT_NAME, PALETTE, POINTER_THEME};
    let top = Drawn {
        look: None,
        frit: vtium_settings::setting(conf, FONT_NAME.name),
        palette: vtium_settings::setting(conf, PALETTE.name),
        vtlook: vtium_settings::setting(conf, "vtlook"),
        pointer: None,
    };
    let Some(reading) = vtium_settings::sections(conf.as_bytes()) else {
        return vec![top];
    };
    let config = reading.parsed.config;
    let mut out = Vec::new();
    // a head draws its look's section, or the top level when no section
    // has its look's name; an unset `look` names the look `default`
    let bare = config
        .heads
        .iter()
        .any(|h| !config.looks.iter().any(|l| l.name == h.look));
    if config.heads.is_empty() || bare {
        out.push(Drawn {
            look: None,
            frit: top.frit.clone(),
            palette: top.palette.clone(),
            vtlook: top.vtlook.clone(),
            pointer: None,
        });
    }
    for look in config.looks {
        let value = |key: &str| {
            look.settings
                .iter()
                .rev()
                .find(|s| s.key.name == key)
                .map(|s| s.value.clone())
        };
        out.push(Drawn {
            frit: value(FONT_NAME.name).or_else(|| top.frit.clone()),
            palette: value(PALETTE.name).or_else(|| top.palette.clone()),
            vtlook: look.vtlook.clone().or_else(|| top.vtlook.clone()),
            pointer: value(POINTER_THEME.name),
            look: Some(look.name),
        });
    }
    out
}

/// The looks of vtium.conf's text `conf` that use the resource at `path`,
/// the top level first, and how each uses it: a font that its font set
/// names by its path, its color scheme, or a color map or shader that its
/// vtlook names; and the top level when its `pointer-theme` names the
/// pointer theme at `path`. A font that a set names by its family is
/// found only by the font helper, so no look is named for it here. The
/// command line's warning and the TUI's question both come from this
/// list.
pub fn uses(path: &Path, conf: &str, places: &Places<'_>) -> Vec<Use> {
    let mut out = Vec::new();
    // paths compare by their components, so a last slash, a doubled
    // slash or a `.` in the key's value changes nothing, as in the batch
    let theme = vtium_settings::setting(conf, vtium_settings::keys::POINTER_THEME.name);
    if theme.is_some_and(|t| Path::new(&t) == path) {
        out.push(Use {
            look: None,
            through: Through::PointerTheme,
        });
    }
    for drawn in looks_of(conf) {
        let mut through = None;
        if let (Some(frit), Some(frits)) = (&drawn.frit, places.frits)
            && frit_names(frits, frit, path)
        {
            through = Some(Through::FontSet(frit.clone()));
        }
        if let Some(palette) = &drawn.palette
            && palette_file(places.palettes, palette).as_deref() == Some(path)
        {
            through = Some(Through::Palette);
        }
        if let Some(vtlook) = &drawn.vtlook
            && vtlook != vtium_settings::vtlook::DEFAULT
            && (places.vtlook)(vtlook).iter().any(|f| f == path)
        {
            through = Some(Through::Vtlook(vtlook.clone()));
        }
        if drawn
            .pointer
            .as_deref()
            .is_some_and(|t| Path::new(t) == path)
        {
            through = Some(Through::PointerTheme);
        }
        if let Some(through) = through {
            out.push(Use {
                look: drawn.look,
                through,
            });
        }
    }
    out
}

/// The looks of vtium.conf's text `conf` that draw with the vtlook
/// `name`, the top level first, as [`uses`] counts them.
pub fn drawing_vtlook(conf: &str, name: &str) -> Vec<Option<String>> {
    looks_of(conf)
        .into_iter()
        .filter(|d| d.vtlook.as_deref() == Some(name))
        .map(|d| d.look)
        .collect()
}

/// The looks of vtium.conf's text `conf` that draw with the color scheme
/// `name`, the top level first, as [`uses`] counts them. A look that
/// names no scheme takes the top level's, and with neither set it draws
/// `legacy`, the first built-in scheme. This names the users of a scheme
/// built into vtium, which has no file for [`uses`] to match.
///
/// References:
/// - ADR-1205 D10: a look that names no palette takes the top level's, and with neither set a head draws `legacy`.
pub fn drawing_palette(conf: &str, name: &str) -> Vec<Option<String>> {
    let fallback = vtium_term::PALETTE_NAMES
        .first()
        .copied()
        .unwrap_or_default();
    looks_of(conf)
        .into_iter()
        .filter(|d| d.palette.as_deref().unwrap_or(fallback) == name)
        .map(|d| d.look)
        .collect()
}

/// The color maps and shaders that `vtlook` names, each in `looks`, the
/// folder vtium reads them from.
pub fn named_files(vtlook: &vtium_settings::vtlook::Vtlook, looks: &Path) -> Vec<PathBuf> {
    use vtium_settings::vtlook::Value;
    vtlook
        .color
        .iter()
        .chain(&vtlook.spatial)
        .flat_map(|f| f.values.iter())
        .filter_map(|(_, v)| match v {
            Value::File(file) => Some(looks.join(file)),
            _ => None,
        })
        .collect()
}

/// The color maps and shaders that the vtlook `name` names, read as vtium
/// reads it under `trust`: from the administrator's folder, else from the
/// package's presets. None when the vtlook cannot be read.
pub fn vtlook_files(name: &str, trust: &vtium_settings::Trust) -> Vec<PathBuf> {
    use vtium_settings::vtlook::{LOOKS, SUFFIX, folders, read};
    let file = format!("{name}{SUFFIX}");
    let Some(dir) = folders()
        .into_iter()
        .find(|d| d.join(&file).symlink_metadata().is_ok())
    else {
        return Vec::new();
    };
    read(name, dir, trust).map_or_else(|_| Vec::new(), |v| named_files(&v, Path::new(LOOKS)))
}

/// The sentences that warn, before `path` is deactivated, of each look in
/// vtium.conf's text `conf` that uses it, as [`uses`] finds them. A
/// deactivation takes effect at the next login after the install, when
/// vtium reads the batch again.
///
/// References:
/// - ADR-1205 D4, its note of 2026-11-19: vtium reads the batch again at each new login.
/// - ADR-1205 D8: deactivating a resource warns and lists the looks that use it.
pub fn users(path: &Path, conf: &str, places: &Places<'_>) -> Vec<String> {
    let kind = kind(path);
    uses(path, conf, places)
        .into_iter()
        .map(|u| {
            match (&u.through, &u.look) {
                (Through::PointerTheme, None) => {
                    return "vtium.conf's pointer-theme names this pointer theme. From the next login after the install, each look that names no pointer theme of its own shows vtium's own pointer pictures in its place.".to_owned();
                }
                (Through::PointerTheme, Some(name)) => {
                    return format!("Look {name} uses this pointer theme. From the next login after the install, look {name} shows vtium's own pointer pictures in its place.");
                }
                _ => {}
            }
            let (who, after) = match &u.look {
                None => (
                    "The top level of vtium.conf".to_owned(),
                    "each head that uses the top level renders".to_owned(),
                ),
                Some(name) => (format!("Look {name}"), format!("look {name} renders")),
            };
            let (uses, without) = match &u.through {
                Through::FontSet(frit) => (
                    format!("this font through the font set {frit}"),
                    "without it",
                ),
                Through::Palette => ("this color scheme".to_owned(), "without it"),
                Through::Vtlook(vtlook) => (
                    format!("this {kind} through the vtlook {vtlook}"),
                    "with no filter",
                ),
                Through::PointerTheme => (String::new(), ""),
            };
            format!("{who} uses {uses}. From the next login after the install, {after} {without}.")
        })
        .collect()
}

/// The file vtium reads for the color scheme `name`: `NAME.palette` in
/// the first of `folders` that holds one, so the administrator's copy in
/// `/etc/vtium/palettes` shadows the package's. `None` for a scheme built
/// into vtium, which has no file, and for a name no file can take.
fn palette_file(folders: &[&Path], name: &str) -> Option<PathBuf> {
    use vtium_settings::palette::{SUFFIX, built_in, named};
    if built_in(name) || !named(name) {
        return None;
    }
    folders
        .iter()
        .map(|dir| dir.join(format!("{name}{SUFFIX}")))
        .find(|file| file.symlink_metadata().is_ok())
}

/// Whether the font set `name` in the folder `frits` names the font file
/// `path` by its path in one of its sources.
fn frit_names(frits: &Path, name: &str, path: &Path) -> bool {
    use crate::frit::{FRIT_BYTES_MAX, Frit, Origin};
    let Some(file) = crate::switch::frit_file(frits, name) else {
        return false;
    };
    let Ok(bytes) = std::fs::read(&file) else {
        return false;
    };
    if bytes.len() > FRIT_BYTES_MAX {
        return false;
    }
    let Ok(frit) = Frit::parse(&bytes, &file, name) else {
        return false;
    };
    frit.sources.iter().any(|s| match &s.origin {
        Origin::File(p) | Origin::Bdf(p) | Origin::Hex(p) | Origin::Outline(p) => p == path,
        _ => false,
    })
}
