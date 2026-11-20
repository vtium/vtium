//! What the Effects tab finds: the vtlooks, the color maps and the
//! shaders. A vtlook is a file of filters that change the colors or the
//! look of the picture before the display shows it. [`find`] reads them
//! from their folders when the TUI starts, outside the state machine, and
//! the state machine reads the result from the inputs. Each color map and
//! shader is a [`Resource`], so the Effects tab shows its marks and takes
//! Space on it with the controls the resource tabs share.
//!
//! References:
//! - ADR-1205 D3: where the Effects tab finds color maps and shaders, and
//!   that one in the person's own folder is listed as not usable.
//! - ADR-1304 D4, its note of 2026-11-19: No filter first, then the
//!   presets, then the administrator's vtlooks, which win on a name.
//! - ADR-1304 D15: a shader is a `NAME.vtshader` file in the package's
//!   or the administrator's shader folder.

use std::path::{Path, PathBuf};

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
/// `note` says why the marks may be wrong, when vtium-config could not
/// read the draft's active resources.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub vtlooks: Vec<Vtlook>,
    pub maps: Vec<Resource>,
    pub shaders: Vec<Resource>,
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

    /// Gives each color map and shader its state from `states`, as
    /// [`super::resources::set_states`] does.
    pub fn set_states(&mut self, states: &[(PathBuf, State)]) {
        super::resources::set_states(&mut self.maps, states);
        super::resources::set_states(&mut self.shaders, states);
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
        note: None,
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
