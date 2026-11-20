//! What the Effects tab finds: the vtlooks, the color maps and the
//! shaders. A vtlook is a file of filters that change the colors or the
//! look of the picture before the display shows it. [`find`] reads them
//! from their folders when the TUI starts, outside the state machine, and
//! the state machine reads the result from the inputs. Each color map and
//! shader is a [`Resource`], so the Effects tab shows its marks and takes
//! Space on it with the controls the resource tabs share.
//!
//! The same module finds the resources the Looks tab lists:
//! [`find_palettes`] reads the color scheme files, colors included, and
//! [`find_pointers`] lists the installed pointer themes. [`Effects`]
//! carries them beside the color maps and shaders, so one answer of the
//! desk sets the marks of every resource tab.
//!
//! References:
//! - ADR-1205 D3: where the Effects tab finds color maps and shaders, and
//!   that one in the person's own folder is listed as not usable.
//! - ADR-1304 D4, its note of 2026-11-19: No filter first, then the
//!   presets, then the administrator's vtlooks, which win on a name.
//! - ADR-1304 D15: a shader is a `NAME.vtshader` file in the package's
//!   or the administrator's shader folder.
//! - ADR-1205 D10: the color scheme folders, the administrator's file
//!   winning on a name, and the strict reader vtium uses.
//! - ADR-1205 D6: the installed pointer themes and their folders.

use std::path::{Path, PathBuf};

use vtium_settings::palette::{Fault, Reason, SUFFIX as PALETTE_SUFFIX};

use crate::batch::{Places, State, Use};

use super::resources::Resource;

/// Where a vtlook the Effects tab lists comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    /// The built-in vtlook, No filter, which has no file.
    BuiltIn,
    /// A preset the package ships.
    Preset,
    /// The administrator's folder.
    Administrator,
}

/// A vtlook the Effects tab lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vtlook {
    pub name: String,
    /// The file, `None` for the built-in vtlook.
    pub file: Option<PathBuf>,
    pub from: From,
    /// A preset that an administrator's vtlook of the same name hides, so
    /// vtium reads the administrator's file in its place.
    pub hidden: bool,
    /// Its filters, each with its settings, one line each; or the first
    /// error, with its file and line, when it cannot be read.
    pub read: Result<Vec<String>, String>,
    /// The color maps and shaders it names.
    pub files: Vec<PathBuf>,
}

/// What the Effects tab lists: the vtlooks, No filter first, then the
/// presets, then the administrator's; the color maps; and the shaders.
/// Beside them, the resources the Looks tab lists: the color scheme files
/// and the folders they were found in, the administrator's first, and the
/// installed pointer themes. `note` says why the marks may be wrong, when
/// vtium-config could not read the draft's active resources.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub vtlooks: Vec<Vtlook>,
    pub maps: Vec<Resource>,
    pub shaders: Vec<Resource>,
    pub palettes: Vec<SchemeFile>,
    pub palette_folders: Vec<PathBuf>,
    pub pointers: Vec<Resource>,
    pub note: Option<String>,
}

impl Effects {
    /// The vtlook vtium reads for `name`: the administrator's over a
    /// preset of the same name, and No filter for `default`.
    pub fn vtlook(&self, name: &str) -> Option<&Vtlook> {
        self.vtlooks.iter().find(|v| v.name == name && !v.hidden)
    }

    /// The color map or shader at `path`.
    pub fn resource(&self, path: &Path) -> Option<&Resource> {
        self.maps
            .iter()
            .chain(&self.shaders)
            .find(|r| r.path == path)
    }

    /// Gives each color map, shader, color scheme file and pointer theme
    /// its state from `states`, as [`super::resources::set_states`] does.
    pub fn set_states(&mut self, states: &[(PathBuf, State)]) {
        super::resources::set_states(&mut self.maps, states);
        super::resources::set_states(&mut self.shaders, states);
        super::resources::set_states(&mut self.pointers, states);
        for f in &mut self.palettes {
            super::resources::set_states(std::slice::from_mut(&mut f.resource), states);
        }
    }

    /// The looks of vtium.conf's text `conf` that use the color map or
    /// shader at `path` through their vtlook, by the rule the command
    /// line's warning follows, with each vtlook as this list holds it.
    pub fn uses(&self, conf: &str, path: &Path) -> Vec<Use> {
        let vtlook = |name: &str| {
            self.vtlook(name)
                .map(|v| v.files.clone())
                .unwrap_or_default()
        };
        crate::batch::uses(
            path,
            conf,
            &Places {
                frits: None,
                palettes: &[],
                vtlook: &vtlook,
            },
        )
    }
}

/// A scheme file the group lists: its name, which a look's `palette`
/// names, the file as a resource with its marks, and its colors, or the
/// first error that makes vtium treat the file as missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemeFile {
    pub name: String,
    pub resource: Resource,
    pub colors: Result<vtium_term::Palette, String>,
}

/// What a person calls a scheme file.
pub const PALETTE_KIND: &str = "color scheme";

/// The files whose names end in the scheme suffix in `dir`, sorted by
/// name. A folder is not listed, whatever its name.
fn palette_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| !t.is_dir()))
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(PALETTE_SUFFIX))
        .collect();
    out.sort();
    out
}

/// The colors of the scheme file `file`, named `name`, read as vtium reads
/// them; or the first error, its file and line first. A file that takes a
/// built-in scheme's name is never read. The read stops one byte past the
/// size bound, so a large file never fills memory, and the reader then
/// refuses the file as too large.
pub fn palette_colors(name: &str, file: &Path) -> Result<vtium_term::Palette, String> {
    use std::io::Read;
    let failed = |line: Option<usize>, reason: Reason| {
        let at = match line {
            Some(line) => format!("{}:{line}", file.display()),
            None => file.display().to_string(),
        };
        format!("{at}: {reason}")
    };
    if let Some(built_in) = vtium_term::PALETTE_NAMES
        .iter()
        .chain(std::iter::once(&vtium_settings::palette::CUSTOM))
        .find(|n| **n == name)
    {
        return Err(failed(None, Reason::Shadows(built_in)));
    }
    let max = vtium_settings::PALETTE_FILE_MAX;
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .and_then(|f| {
            f.take(u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1))
                .read_to_end(&mut bytes)
        })
        .map_err(|e| failed(None, Reason::Unreadable(e.kind())))?;
    match vtium_settings::palette::parse(&bytes) {
        Ok((palette, _, _)) => Ok(palette),
        Err((faults, _)) => {
            let Some(Fault { line, reason }) = faults.into_iter().next() else {
                return Err(failed(None, Reason::NotKeyValue));
            };
            Err(failed(line, reason))
        }
    }
}

/// The scheme files in `folders`, the folders in the order vtium looks a
/// name up in, the administrator's first: each with its state from
/// `state`, its trust mark from `trusted`, and its colors. The list holds
/// the package's files first and then the administrator's, as the Effects
/// tab lists presets before the administrator's vtlooks, and a file that
/// an earlier folder's file of the same name hides is marked so.
///
/// References:
/// - ADR-1205 D10: `/etc/vtium/palettes` wins over `/usr/share/vtium/palettes`.
pub fn find_palettes(
    folders: &[PathBuf],
    state: &dyn Fn(&Path) -> State,
    trusted: &dyn Fn(&Path) -> Result<(), String>,
) -> Vec<SchemeFile> {
    let stem = |p: &Path| {
        p.file_name()
            .map(|n| {
                n.to_string_lossy()
                    .trim_end_matches(PALETTE_SUFFIX)
                    .to_owned()
            })
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for (at, dir) in folders.iter().enumerate().rev() {
        for file in palette_files(dir) {
            let name = stem(&file);
            let hidden = folders.get(..at).unwrap_or_default().iter().any(|d| {
                d.join(format!("{name}{PALETTE_SUFFIX}"))
                    .symlink_metadata()
                    .is_ok()
            });
            out.push(SchemeFile {
                colors: palette_colors(&name, &file),
                resource: Resource {
                    state: state(&file),
                    trust: trusted(&file),
                    path: file,
                    kind: PALETTE_KIND,
                    own: false,
                    hidden,
                },
                name,
            });
        }
    }
    out
}

/// What a person calls a pointer theme.
pub const POINTER_KIND: &str = "pointer theme";

/// The system's icon folders, where the desktop's cursor themes are
/// installed.
pub const ICONS: [&str; 2] = ["/usr/share/icons", "/usr/local/share/icons"];

/// The cursor themes in `dir`, by name: each folder in it that holds a
/// `cursors` folder.
fn pointer_themes(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()) && p.join("cursors").is_dir()
        })
        .collect();
    out.sort();
    out
}

/// The installed pointer themes: `svg`, vtium's own pictures, where it is
/// installed, then the cursor themes of each of `icons`, each with its
/// state from `state` and its trust mark from `trusted`.
pub fn find_pointers(
    svg: &Path,
    icons: &[PathBuf],
    state: &dyn Fn(&Path) -> State,
    trusted: &dyn Fn(&Path) -> Result<(), String>,
) -> Vec<Resource> {
    let own = svg
        .symlink_metadata()
        .is_ok()
        .then(|| svg.to_path_buf())
        .into_iter();
    own.chain(icons.iter().flat_map(|d| pointer_themes(d)))
        .map(|path| Resource {
            state: state(&path),
            trust: trusted(&path),
            path,
            kind: POINTER_KIND,
            own: false,
            hidden: false,
        })
        .collect()
}

/// The folders [`find`] reads.
#[derive(Clone, Debug)]
pub struct Folders {
    /// The package's vtlooks, the presets.
    pub presets: PathBuf,
    /// The administrator's vtlooks, and the color maps beside them.
    pub looks: PathBuf,
    /// The shaders' folders, the package's and then the administrator's.
    pub shaders: Vec<PathBuf>,
    /// The person's own folders for color maps and shaders, whose files
    /// are listed as not usable.
    pub own_looks: Option<PathBuf>,
    pub own_shaders: Option<PathBuf>,
}

/// The package's shader folder.
pub const PACKAGE_SHADERS: &str = "/usr/share/vtium/shaders";

/// The administrator's shader folder.
pub const SHADERS: &str = "/etc/vtium/shaders";

/// The suffix of a shader's file name.
pub const SHADER_SUFFIX: &str = ".vtshader";

impl Folders {
    /// The folders vtium reads, and the person's own under `config`, the
    /// person's configuration folder, such as `~/.config`.
    pub fn system(config: Option<PathBuf>) -> Folders {
        use vtium_settings::vtlook::{LOOKS, PACKAGE_LOOKS};
        let own = config.map(|c| c.join("vtium"));
        Folders {
            presets: PathBuf::from(PACKAGE_LOOKS),
            looks: PathBuf::from(LOOKS),
            shaders: vec![PathBuf::from(PACKAGE_SHADERS), PathBuf::from(SHADERS)],
            own_looks: own.as_ref().map(|o| o.join("looks")),
            own_shaders: own.map(|o| o.join("shaders")),
        }
    }
}

/// The files in `dir` whose names end in `suffix`, sorted by name. A
/// folder is not listed, whatever its name.
fn files(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| !t.is_dir()))
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(suffix))
        .collect();
    out.sort();
    out
}

/// A filter's settings as one line: its name, then each parameter and its
/// value.
fn filter_line(f: &vtium_settings::vtlook::Filter) -> String {
    use vtium_settings::vtlook::Value;
    let values: Vec<String> = f
        .values
        .iter()
        .map(|(name, value)| {
            let shown = match value {
                Value::Number(n) => n.to_string(),
                Value::Integer(n) => n.to_string(),
                Value::Color([r, g, b]) => format!("{r},{g},{b}"),
                Value::Choice(c) => (*c).to_owned(),
                Value::Switch(on) => if *on { "on" } else { "off" }.to_owned(),
                Value::File(file) => file.clone(),
            };
            format!("{name} {shown}")
        })
        .collect();
    if values.is_empty() {
        f.declaration.name.to_owned()
    } else {
        format!("{}: {}", f.declaration.name, values.join(", "))
    }
}

/// The vtlook `name` read from `dir` under `trust`: its filters and the
/// files it names in `looks`, or its first error with its file and line,
/// as `vtium check-config` prints it.
fn read_vtlook(
    name: &str,
    dir: &Path,
    looks: &Path,
    trust: &vtium_settings::Trust,
) -> (Result<Vec<String>, String>, Vec<PathBuf>) {
    match vtium_settings::vtlook::read(name, dir, trust) {
        Ok(look) => {
            let files = crate::batch::named_files(&look, looks);
            let filters = look.color.iter().chain(&look.spatial);
            (Ok(filters.map(filter_line).collect()), files)
        }
        Err(failure) => (
            Err(failure
                .reports
                .first()
                .map_or_else(String::new, |r| format!("{}: {r}", r.at()))),
            Vec::new(),
        ),
    }
}

/// What the Effects tab lists, read from `folders`: each vtlook read under
/// `trust`, and each color map and shader with its state from `state` and
/// its trust mark from `trusted`. A packaged shader that an
/// administrator's shader of the same name hides is marked so.
pub fn find(
    folders: &Folders,
    trust: &vtium_settings::Trust,
    state: &dyn Fn(&Path) -> State,
    trusted: &dyn Fn(&Path) -> Result<(), String>,
) -> Effects {
    use vtium_settings::vtlook::{DEFAULT, SUFFIX};
    let mut vtlooks = vec![Vtlook {
        name: DEFAULT.to_owned(),
        file: None,
        from: From::BuiltIn,
        hidden: false,
        read: Ok(Vec::new()),
        files: Vec::new(),
    }];
    let name_of = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let stem = |p: &Path| {
        name_of(p)
            .map(|n| n.trim_end_matches(SUFFIX).to_owned())
            .unwrap_or_default()
    };
    let admin: Vec<String> = files(&folders.looks, SUFFIX)
        .iter()
        .map(|p| stem(p))
        .collect();
    for (dir, from) in [
        (&folders.presets, From::Preset),
        (&folders.looks, From::Administrator),
    ] {
        for file in files(dir, SUFFIX) {
            let name = stem(&file);
            if name == DEFAULT {
                continue;
            }
            let (read, named) = read_vtlook(&name, dir, &folders.looks, trust);
            vtlooks.push(Vtlook {
                hidden: from == From::Preset && admin.contains(&name),
                name,
                file: Some(file),
                from,
                read,
                files: named,
            });
        }
    }
    let resource = |path: PathBuf, kind: &'static str, own: bool| Resource {
        state: if own { State::Inactive } else { state(&path) },
        trust: if own { Ok(()) } else { trusted(&path) },
        path,
        kind,
        own,
        hidden: false,
    };
    let listed = |dirs: &[&PathBuf], own: Option<&PathBuf>, suffix: &str, kind| {
        let mut out: Vec<Resource> = dirs
            .iter()
            .flat_map(|d| files(d, suffix))
            .map(|p| resource(p, kind, false))
            .collect();
        if let Some(own) = own {
            out.extend(
                files(own, suffix)
                    .into_iter()
                    .map(|p| resource(p, kind, true)),
            );
        }
        out
    };
    let mut shaders = listed(
        &folders.shaders.iter().collect::<Vec<_>>(),
        folders.own_shaders.as_ref(),
        SHADER_SUFFIX,
        "shader",
    );
    // the administrator's shader takes the place of a packaged one of the
    // same name, as the administrator's vtlook does a preset's
    let last = folders.shaders.last();
    let administrator: Vec<String> = shaders
        .iter()
        .filter(|s| !s.own && s.path.parent() == last.map(PathBuf::as_path))
        .filter_map(|s| name_of(&s.path))
        .collect();
    for s in &mut shaders {
        let from_package = s.path.parent() != last.map(PathBuf::as_path);
        s.hidden =
            !s.own && from_package && name_of(&s.path).is_some_and(|n| administrator.contains(&n));
    }
    Effects {
        vtlooks,
        maps: listed(
            &[&folders.looks],
            folders.own_looks.as_ref(),
            ".cube",
            "color map",
        ),
        shaders,
        ..Effects::default()
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    // the presets come before the administrator's vtlooks, and one that the
    // administrator's folder also holds is hidden; a packaged shader that
    // the administrator's folder also holds is hidden too, and a folder
    // named like a color map is not listed
    #[test]
    fn find_lists_no_filter_the_presets_and_the_administrator_s_vtlooks() {
        // a folder that nobody else can write, as the trust rule asks of
        // every folder above a vtlook
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("vtium-config-effects-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (pkg, etc, mine) = (root.join("pkg"), root.join("etc"), root.join("mine"));
        let (pkg_shaders, etc_shaders) = (root.join("pkg-shaders"), root.join("etc-shaders"));
        for d in [
            &pkg,
            &etc,
            &mine.join("looks"),
            &pkg_shaders,
            &etc_shaders,
            &etc.join("folder.cube"),
        ] {
            std::fs::create_dir_all(d).unwrap();
        }
        let tint = "version = 1\n[color tint]\ncolor = 255,176,0\n";
        std::fs::write(pkg.join("amber.vtlook"), tint).unwrap();
        std::fs::write(pkg.join("green.vtlook"), tint).unwrap();
        std::fs::write(etc.join("green.vtlook"), "version = 1\n[color nope]\n").unwrap();
        std::fs::write(etc.join("sepia.cube"), "LUT_3D_SIZE 2\n").unwrap();
        std::fs::write(mine.join("looks/mine.cube"), "LUT_3D_SIZE 2\n").unwrap();
        for dir in [&pkg_shaders, &etc_shaders] {
            std::fs::write(dir.join("crt.vtshader"), "").unwrap();
        }
        std::fs::write(pkg_shaders.join("glow.vtshader"), "").unwrap();
        std::fs::write(pkg_shaders.join("old.glsl"), "").unwrap();
        let folders = Folders {
            presets: pkg,
            looks: etc.clone(),
            shaders: vec![pkg_shaders.clone(), etc_shaders.clone()],
            own_looks: Some(mine.join("looks")),
            own_shaders: None,
        };
        let trust = vtium_settings::Trust::running_as(rustix::process::geteuid().as_raw());
        let found = find(&folders, &trust, &|_| State::Active, &|_| Ok(()));
        let names: Vec<(&str, From, bool, bool)> = found
            .vtlooks
            .iter()
            .map(|v| (v.name.as_str(), v.from, v.hidden, v.read.is_ok()))
            .collect();
        assert_eq!(
            names,
            [
                ("default", From::BuiltIn, false, true),
                ("amber", From::Preset, false, true),
                ("green", From::Preset, true, true),
                ("green", From::Administrator, false, false),
            ]
        );
        assert_eq!(
            found.vtlooks.get(1).unwrap().read,
            Ok(vec!["tint: color 255,176,0, amount 1, floor 0".to_owned()])
        );
        // the error names its file and line
        let broken = found.vtlooks.get(3).unwrap().read.clone().unwrap_err();
        assert!(
            broken.starts_with(&format!("{}:2: ", etc.join("green.vtlook").display())),
            "{broken}"
        );
        let maps: Vec<(String, bool)> = found.maps.iter().map(|m| (m.name(), m.own)).collect();
        assert_eq!(
            maps,
            [
                ("sepia.cube".to_owned(), false),
                ("mine.cube".to_owned(), true)
            ]
        );
        let shaders: Vec<(PathBuf, bool)> = found
            .shaders
            .iter()
            .map(|s| (s.path.clone(), s.hidden))
            .collect();
        assert_eq!(
            shaders,
            [
                (pkg_shaders.join("crt.vtshader"), true),
                (pkg_shaders.join("glow.vtshader"), false),
                (etc_shaders.join("crt.vtshader"), false),
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod palette_tests {
    use super::*;

    // the package's files come first, a packaged file that the
    // administrator's folder also holds is hidden, a file that takes a
    // built-in scheme's name is listed with the error that says so, and a
    // file with an error names its file and line
    #[test]
    fn find_lists_the_package_s_files_then_the_administrator_s() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("vtium-config-palettes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (etc, pkg) = (root.join("etc"), root.join("pkg"));
        for d in [&etc, &pkg, &pkg.join("folder.palette")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let good = "background = #101010\nforeground = #e0e0e0\npalette = 1=#ff0000\n";
        std::fs::write(pkg.join("dusk.palette"), good).unwrap();
        std::fs::write(pkg.join("paper.palette"), good).unwrap();
        std::fs::write(
            etc.join("dusk.palette"),
            "background = #101010\nnot a line\n",
        )
        .unwrap();
        std::fs::write(etc.join("nord.palette"), good).unwrap();
        std::fs::write(etc.join("notes.txt"), good).unwrap();
        let found = find_palettes(
            &[etc.clone(), pkg.clone()],
            &|p| {
                if p.starts_with(&pkg) {
                    State::Active
                } else {
                    State::Inactive
                }
            },
            &|_| Ok(()),
        );
        let listed: Vec<(PathBuf, State, bool, bool)> = found
            .iter()
            .map(|f| {
                (
                    f.resource.path.clone(),
                    f.resource.state,
                    f.resource.hidden,
                    f.colors.is_ok(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                (pkg.join("dusk.palette"), State::Active, true, true),
                (pkg.join("paper.palette"), State::Active, false, true),
                (etc.join("dusk.palette"), State::Inactive, false, false),
                (etc.join("nord.palette"), State::Inactive, false, false),
            ]
        );
        let paper = found.get(1).unwrap().colors.clone().unwrap();
        assert_eq!(
            (paper.bg(), paper.fg(), paper.color(1)),
            (
                vtium_term::Rgb::new(0x10, 0x10, 0x10),
                vtium_term::Rgb::new(0xe0, 0xe0, 0xe0),
                vtium_term::Rgb::new(0xff, 0, 0)
            )
        );
        let broken = found.get(2).unwrap().colors.clone().unwrap_err();
        assert!(
            broken.starts_with(&format!("{}:2: ", etc.join("dusk.palette").display())),
            "{broken}"
        );
        let nord = found.get(3).unwrap().colors.clone().unwrap_err();
        assert!(nord.contains("built-in color scheme nord"), "{nord}");
        let _ = std::fs::remove_dir_all(&root);
    }

    // a file past the size bound is refused without reading all of it
    #[test]
    fn a_scheme_file_past_the_bound_is_an_error() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("vtium-config-palette-big-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("big.palette");
        let line = "background = #101010\n";
        let n = vtium_settings::PALETTE_FILE_MAX / line.len() + 1;
        std::fs::write(&file, line.repeat(n)).unwrap();
        let why = palette_colors("big", &file).unwrap_err();
        assert!(why.contains("larger than"), "{why}");
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod pointer_tests {
    use super::*;

    // vtium's own pictures come first, then each folder that holds a
    // cursors folder, and a folder without one is not a pointer theme
    #[test]
    fn find_lists_vtium_s_pictures_then_the_cursor_themes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("vtium-config-pointers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (share, local) = (root.join("share"), root.join("local"));
        for d in [
            share.join("Adwaita/cursors"),
            share.join("hicolor/48x48"),
            local.join("mine/cursors"),
        ] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(share.join("loose"), "").unwrap();
        let svg = root.join("pointers.svg");
        std::fs::write(&svg, "<svg/>").unwrap();
        let found = find_pointers(
            &svg,
            &[share.clone(), local.clone()],
            &|p| {
                if p == svg {
                    State::Active
                } else {
                    State::Inactive
                }
            },
            &|_| Ok(()),
        );
        let listed: Vec<(PathBuf, State)> =
            found.iter().map(|r| (r.path.clone(), r.state)).collect();
        assert_eq!(
            listed,
            [
                (svg.clone(), State::Active),
                (share.join("Adwaita"), State::Inactive),
                (local.join("mine"), State::Inactive),
            ]
        );
        // without vtium's own pictures installed, the list starts with the
        // cursor themes
        std::fs::remove_file(&svg).unwrap();
        let found = find_pointers(
            &svg,
            std::slice::from_ref(&share),
            &|_| State::Inactive,
            &|_| Ok(()),
        );
        assert_eq!(
            found.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
            [share.join("Adwaita")]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
