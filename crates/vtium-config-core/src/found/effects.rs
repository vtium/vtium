//! The effects found: the vtlooks, the color maps and the shaders. A
//! vtlook is a file of frame filters, which change the colors or the look
//! of the picture before the display shows it. A color map is a `.cube`
//! table that a vtlook's `cube` filter reads, and a shader is a spatial
//! filter's file.
//!
//! Each vtlook lists its filters in the order of its sections, each with
//! its settings and whether this build of vtium draws it. The color
//! filters are built. Every spatial filter is planned, since vtium draws
//! no spatial filter yet: [`Built::Planned`] names the slice of the frame
//! filters' plan that brings it. A vtlook that names a spatial filter
//! does not read, and vtium draws the built-in vtlook in its place, so its
//! error stays beside the list.
//!
//! References:
//! - ADR-1205 D3: where the Effects tab finds color maps and shaders, and that one in the person's own folder is listed as not usable.
//! - ADR-1304 D4, its note of 2026-11-19: No filter first, then the presets, then the administrator's vtlooks, which win on a name.
//! - ADR-1304 D8: the first filters, color and spatial.
//! - ADR-1304 D13: the slices that build the spatial layer (4), curvature (5) and Ghostty's dialect (11).
//! - ADR-1304 D15, its note of 2026-11-24: a shader is `NAME.vtshader`, or a Ghostty shader's `NAME.glsl`, in the package's or the administrator's shader folder.

use std::path::{Path, PathBuf};

use vtium_render::filter::Layer;
use vtium_settings::vtlook::{self, DEFAULT, SUFFIX};

use crate::batch::{Places as UsePlaces, Use};

use super::{Kind, Origin, Places, Resource, counted, files, stem};

/// Where a vtlook comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    /// The built-in vtlook, No filter, which has no file.
    BuiltIn,
    /// A preset the package ships.
    Preset,
    /// The administrator's folder.
    Administrator,
}

/// Whether this build of vtium draws a filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Built {
    /// vtium draws it.
    Built,
    /// vtium does not draw it yet; this slice of the frame filters' plan
    /// brings it.
    Planned { slice: u8 },
    /// No filter of this name is built or planned, so the vtlook does not
    /// read.
    Unknown,
}

/// The spatial filters the frame filters' plan names, each with the slice
/// that brings it. A spatial filter it does not name, such as a shader of
/// the administrator's own, waits for the spatial layer itself.
///
/// References:
/// - ADR-1304 D8: scanlines, sharpen and mask are the spatial layer's first shader files, and curvature follows in a slice of its own.
/// - ADR-1304 D13: slice 4 builds the spatial layer, slice 5 curvature, and slice 11 Ghostty's dialect, crt's.
pub const PLANNED: [(&str, u8); 5] = [
    ("scanlines", 4),
    ("sharpen", 4),
    ("mask", 4),
    ("curvature", 5),
    ("crt", 11),
];

/// The slice that brings the spatial layer, and with it every spatial
/// filter [`PLANNED`] does not name.
pub const SPATIAL_SLICE: u8 = 4;

/// One filter of a vtlook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    pub layer: Layer,
    pub name: String,
    /// Each setting as a parameter's name and its value. For a vtlook that
    /// reads, every parameter, with the declared default where the file
    /// sets none; otherwise the lines the file holds.
    pub settings: Vec<(String, String)>,
    pub built: Built,
}

/// A vtlook found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vtlook {
    pub name: String,
    /// The file, `None` for the built-in vtlook.
    pub file: Option<PathBuf>,
    pub from: From,
    /// A preset that an administrator's vtlook of the same name hides, so
    /// vtium reads the administrator's file in its place.
    pub hidden: bool,
    /// Its filters, in the order of its sections.
    pub filters: Vec<Filter>,
    /// Whether vtium reads it, or its first error, file and line first, as
    /// `vtium check-config` prints it.
    pub reads: Result<(), String>,
    /// The color maps it names.
    pub files: Vec<PathBuf>,
}

/// A shader's language.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dialect {
    /// vtium's own format, a `.vtshader` file.
    Vtium,
    /// Ghostty's, a `.glsl` file, which vtium translates.
    Ghostty,
}

/// A shader found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shader {
    pub resource: Resource,
    pub dialect: Dialect,
}

/// What the Effects tab lists: the vtlooks, No filter first, then the
/// presets, then the administrator's; the color maps; and the shaders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub vtlooks: Vec<Vtlook>,
    pub maps: Vec<Resource>,
    pub shaders: Vec<Shader>,
}

impl Effects {
    /// The vtlook vtium reads for `name`: the administrator's over a
    /// preset of the same name, and No filter for `default`.
    pub fn vtlook(&self, name: &str) -> Option<&Vtlook> {
        self.vtlooks.iter().find(|v| v.name == name && !v.hidden)
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
            &UsePlaces {
                frits: None,
                palettes: &[],
                vtlook: &vtlook,
            },
        )
    }
}

/// The groups the Effects tab's filter line shows, in its order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Group {
    #[default]
    All,
    Vtlooks,
    ColorMaps,
    Shaders,
}

impl Group {
    /// Every group, in the order of the list of filters.
    pub const ALL: [Group; 4] = [Group::All, Group::Vtlooks, Group::ColorMaps, Group::Shaders];
}

/// The folders the effects are found in.
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// The suffix of a shader's file name in vtium's own format.
pub const SHADER_SUFFIX: &str = ".vtshader";

/// The suffix of a shader's file name in Ghostty's dialect.
pub const GHOSTTY_SUFFIX: &str = ".glsl";

/// The suffix of a color map's file name.
pub const MAP_SUFFIX: &str = ".cube";

impl Folders {
    /// The folders vtium reads, and the person's own under `config`, the
    /// person's configuration folder, such as `~/.config`.
    pub fn system(config: Option<PathBuf>) -> Folders {
        let own = config.map(|c| c.join("vtium"));
        Folders {
            presets: PathBuf::from(vtlook::PACKAGE_LOOKS),
            looks: PathBuf::from(vtlook::LOOKS),
            shaders: vec![PathBuf::from(PACKAGE_SHADERS), PathBuf::from(SHADERS)],
            own_looks: own.as_ref().map(|o| o.join("looks")),
            own_shaders: own.map(|o| o.join("shaders")),
        }
    }
}

/// The effects at `places`: each vtlook read under the trust rule of
/// `places.euid`, and each color map and shader with its trust mark and
/// its activation's origin from `origin`. A packaged shader that an
/// administrator's shader of the same name hides is marked so.
pub(super) fn scan(places: &Places, origin: &dyn Fn(&Path) -> Origin) -> Effects {
    counted(|s| &mut s.effects);
    let folders = &places.effects;
    let trust = vtium_settings::Trust::running_as(places.euid);
    let mut vtlooks = vec![Vtlook {
        name: DEFAULT.to_owned(),
        file: None,
        from: From::BuiltIn,
        hidden: false,
        filters: Vec::new(),
        reads: Ok(()),
        files: Vec::new(),
    }];
    let admin: Vec<String> = files(&folders.looks, SUFFIX)
        .iter()
        .map(|p| stem(p, SUFFIX))
        .collect();
    for (dir, from) in [
        (&folders.presets, From::Preset),
        (&folders.looks, From::Administrator),
    ] {
        for file in files(dir, SUFFIX) {
            let name = stem(&file, SUFFIX);
            if name == DEFAULT {
                continue;
            }
            let mut found = read_vtlook(&name, dir, &folders.looks, &trust);
            found.hidden = from == From::Preset && admin.contains(&name);
            found.from = from;
            vtlooks.push(found);
        }
    }
    let resource = |path: PathBuf, kind: Kind, own: bool| Resource {
        trust: if own {
            Ok(())
        } else {
            crate::batch::trusted(&path, places.euid)
        },
        origin: if own { Origin::Nothing } else { origin(&path) },
        path,
        kind,
        own,
        hidden: false,
    };
    let mut maps: Vec<Resource> = files(&folders.looks, MAP_SUFFIX)
        .into_iter()
        .map(|p| resource(p, Kind::ColorMap, false))
        .collect();
    if let Some(own) = &folders.own_looks {
        maps.extend(
            files(own, MAP_SUFFIX)
                .into_iter()
                .map(|p| resource(p, Kind::ColorMap, true)),
        );
    }
    let shader_files = |dir: &Path| {
        let mut out: Vec<(PathBuf, Dialect)> = files(dir, SHADER_SUFFIX)
            .into_iter()
            .map(|p| (p, Dialect::Vtium))
            .chain(
                files(dir, GHOSTTY_SUFFIX)
                    .into_iter()
                    .map(|p| (p, Dialect::Ghostty)),
            )
            .collect();
        out.sort();
        out
    };
    let mut shaders: Vec<Shader> = folders
        .shaders
        .iter()
        .flat_map(|d| shader_files(d))
        .map(|(p, dialect)| Shader {
            resource: resource(p, Kind::Shader, false),
            dialect,
        })
        .collect();
    if let Some(own) = &folders.own_shaders {
        shaders.extend(shader_files(own).into_iter().map(|(p, dialect)| Shader {
            resource: resource(p, Kind::Shader, true),
            dialect,
        }));
    }
    mark_hidden_shaders(&mut shaders, folders);
    Effects {
        vtlooks,
        maps,
        shaders,
    }
}

/// The name a shader file gives its filter: its file's name without the
/// suffix of its dialect.
fn shader_name(s: &Shader) -> String {
    stem(
        &s.resource.path,
        match s.dialect {
            Dialect::Vtium => SHADER_SUFFIX,
            Dialect::Ghostty => GHOSTTY_SUFFIX,
        },
    )
}

/// Marks each shader that vtium does not read because another file of its
/// name comes first: in each folder vtium looks for `NAME.vtshader` before
/// `NAME.glsl`, and the administrator's folder, the last of
/// `folders.shaders`, comes before the package's.
fn mark_hidden_shaders(shaders: &mut [Shader], folders: &Folders) {
    let last = folders.shaders.last().map(PathBuf::as_path);
    let rank = |s: &Shader| {
        let admin = s.resource.path.parent() == last;
        (!admin, s.dialect == Dialect::Ghostty)
    };
    let system: Vec<(String, (bool, bool))> = shaders
        .iter()
        .filter(|s| !s.resource.own)
        .map(|s| (shader_name(s), rank(s)))
        .collect();
    for s in shaders.iter_mut().filter(|s| !s.resource.own) {
        let (name, mine) = (shader_name(s), rank(s));
        s.resource.hidden = system.iter().any(|(n, r)| *n == name && *r < mine);
    }
}

/// The vtlook `name` read from `dir` under `trust`: its filters, whether
/// vtium reads it, and the color maps it names in `looks`. The file is
/// read once, through the reader vtium uses, and the filters come from its
/// section headings, so a vtlook that does not read still lists them.
fn read_vtlook(name: &str, dir: &Path, looks: &Path, trust: &vtium_settings::Trust) -> Vtlook {
    let file = dir.join(format!("{name}{SUFFIX}"));
    let mut out = Vtlook {
        name: name.to_owned(),
        file: Some(file.clone()),
        from: From::Administrator,
        hidden: false,
        filters: Vec::new(),
        reads: Ok(()),
        files: Vec::new(),
    };
    let first = |failure: vtlook::Failure| {
        failure
            .reports
            .first()
            .map_or_else(String::new, |r| format!("{}: {r}", r.at()))
    };
    let bytes = match vtium_settings::read_limited(&file, trust, vtium_settings::VTLOOK_FILE_MAX) {
        Ok(Some(bytes)) if vtlook::named(name) => bytes,
        _ => {
            // the reader's own words for a file it cannot open
            out.reads = vtlook::read(name, dir, trust).map(|_| ()).map_err(first);
            return out;
        }
    };
    out.filters = sections(&bytes);
    match vtlook::read_text(name, &file, &bytes) {
        Ok(look) => {
            out.files = crate::batch::named_files(&look, looks);
            let read = look.color.iter().chain(&look.spatial);
            for (filter, read) in out.filters.iter_mut().zip(read) {
                filter.settings = read
                    .values
                    .iter()
                    .map(|(n, v)| ((*n).to_owned(), value(v)))
                    .collect();
            }
        }
        Err(failure) => out.reads = Err(first(failure)),
    }
    out
}

/// The filters a vtlook's text `bytes` names, from its section headings,
/// `[color NAME]` and `[spatial NAME]`, each with the `key = value` lines
/// below it, at most as many as a vtlook may hold.
fn sections(bytes: &[u8]) -> Vec<Filter> {
    let text = String::from_utf8_lossy(bytes);
    let mut out: Vec<Filter> = Vec::new();
    let mut open = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(heading) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let mut words = heading.split_whitespace();
            let layer = match words.next() {
                Some("color") => Some(Layer::Color),
                Some("spatial") => Some(Layer::Spatial),
                _ => None,
            };
            open = false;
            if let (Some(layer), Some(name)) = (layer, words.next())
                && out.len() < vtium_settings::VTLOOK_SECTIONS_MAX
            {
                out.push(Filter {
                    layer,
                    name: name.to_owned(),
                    settings: Vec::new(),
                    built: built(layer, name),
                });
                open = true;
            }
            continue;
        }
        if let (true, Some(filter), Some((key, value))) =
            (open, out.last_mut(), line.split_once('='))
            && filter.settings.len() < vtium_settings::VTLOOK_SECTIONS_MAX
        {
            filter
                .settings
                .push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    out
}

/// Whether this build draws the filter `name` of `layer`.
fn built(layer: Layer, name: &str) -> Built {
    match layer {
        Layer::Color if vtium_render::filter::COLOR.iter().any(|d| d.name == name) => Built::Built,
        Layer::Color => Built::Unknown,
        Layer::Spatial => Built::Planned {
            slice: PLANNED
                .iter()
                .find(|(n, _)| *n == name)
                .map_or(SPATIAL_SLICE, |(_, s)| *s),
        },
    }
}

/// A filter's value as a vtlook writes it.
fn value(v: &vtlook::Value) -> String {
    use vtlook::Value;
    match v {
        Value::Number(n) => n.to_string(),
        Value::Integer(n) => n.to_string(),
        Value::Color([r, g, b]) => format!("{r},{g},{b}"),
        Value::Choice(c) => (*c).to_owned(),
        Value::Switch(on) => if *on { "on" } else { "off" }.to_owned(),
        Value::File(file) => file.clone(),
    }
}
