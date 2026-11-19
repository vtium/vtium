//! The controls the resource tabs share, and what the Effects tab finds.
//!
//! A resource is a file vtium draws from: a font, a color map, a shader, a
//! color scheme or a pointer theme. vtium uses a resource only while the
//! batch lists it, and a resource can be activated only when it passes the
//! trust rule. The tabs that list resources show the same marks for each:
//! whether it is active, whether it passes the trust rule, and whether it
//! is in the person's own folder, where vtium cannot use it. Space
//! activates or deactivates the highlighted resource in the draft through
//! the desk, which keeps the change beside the draft until the install.
//! Before it deactivates a resource that a look uses, the TUI asks, and
//! the question names those looks. [`Resource`] holds the marks,
//! [`space`] turns Space into the tab's commands, and [`Resource::row`]
//! gives the marks as a list's detail, so the Fonts tab and the Looks
//! tab's color schemes call the same code as the Effects tab.
//!
//! The Effects tab lists the vtlooks, the color maps and the shaders.
//! [`find`] reads them from their folders when the TUI starts, outside the
//! state machine, and the state machine reads the result from the inputs.
//!
//! References:
//! - ADR-1205 D1: the two resource tabs share their controls.
//! - ADR-1205 D3: where the Effects tab finds color maps and shaders, and
//!   that one in the person's own folder is listed as not usable.
//! - ADR-1205 D4: the batch, the trust rule, and a draft keeping its
//!   changes to the batch beside it.
//! - ADR-1205 D8: deactivating a resource warns and lists the looks that
//!   use it.
//! - ADR-1304 D4, its note of 2026-11-19: No filter first, then the
//!   presets, then the administrator's vtlooks, which win on a name.

use std::path::{Path, PathBuf};

use crate::batch::{Change as BatchChange, State};

use super::desk::Op;
use super::model::{Effect, Then};
use super::tabs::{Change, Command, Request};
use super::words;

/// One resource a tab lists, with its marks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub path: PathBuf,
    /// What kind of resource it is, in a person's words: `color map`,
    /// `shader`, `font` and so on.
    pub kind: &'static str,
    /// Where it stands in the draft.
    pub state: State,
    /// Whether it passes the trust rule, or the check it fails, file
    /// first.
    pub trust: Result<(), String>,
    /// It is in the person's own folder, so vtium cannot use it.
    pub own: bool,
}

impl Resource {
    /// Whether vtium uses it once the draft is installed.
    pub fn active(&self) -> bool {
        matches!(self.state, State::Always | State::Active | State::Activates)
    }

    /// The file's name, as a list shows it.
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// The resource's marks, as a list's detail: not usable for one in
    /// the person's own folder; otherwise whether it is active and
    /// whether it passes the trust rule.
    pub fn row(&self) -> String {
        if self.own {
            return words::NOT_USABLE.to_owned();
        }
        let trust = match self.trust {
            Ok(()) => words::TRUSTED,
            Err(_) => words::UNTRUSTED,
        };
        format!("{}, {trust}", words::active_mark(self.state))
    }
}

/// Space on `resource`, which the looks named in `users` use: activates
/// it, or deactivates it when it is active, through the desk. Before a
/// deactivation of a resource that a look uses, the TUI asks, naming
/// those looks. A resource in the person's own folder, the built-in font,
/// and one that fails the trust rule and is not active change nothing,
/// and the message line says why.
pub fn space(resource: &Resource, users: &[String]) -> Vec<Command> {
    let path = resource.path.display().to_string();
    let say = |line: String| vec![Command::Change(Change::Say(line))];
    if resource.own {
        return say(words::own_resource(resource.kind, &path));
    }
    if resource.state == State::Always {
        return Vec::new();
    }
    if !resource.active() {
        if let Err(why) = &resource.trust {
            return say(words::untrusted(resource.kind, why));
        }
        return vec![Command::Effect(Effect::Desk(Op::Resource {
            path: resource.path.clone(),
            change: BatchChange::Activate,
        }))];
    }
    let op = Op::Resource {
        path: resource.path.clone(),
        change: BatchChange::Deactivate,
    };
    if users.is_empty() {
        return vec![Command::Effect(Effect::Desk(op))];
    }
    vec![Command::Request(Request::Ask {
        question: words::deactivate_question(resource.kind, &path, users),
        then: Then::Desk(op),
    })]
}

/// Gives each of `resources` its state in the draft from `states`, the
/// resources the desk says are active or changed. A resource the list
/// leaves out is not active.
pub fn set_states(resources: &mut [Resource], states: &[(PathBuf, State)]) {
    for r in resources {
        r.state = states
            .iter()
            .find(|(p, _)| *p == r.path)
            .map_or(State::Inactive, |(_, s)| *s);
    }
}

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
    /// error, when it does not read.
    pub read: Result<Vec<String>, String>,
    /// The color maps and shaders it names.
    pub files: Vec<PathBuf>,
}

/// What the Effects tab lists: the vtlooks, No filter first, then the
/// presets, then the administrator's; the color maps; and the shaders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub vtlooks: Vec<Vtlook>,
    pub maps: Vec<Resource>,
    pub shaders: Vec<Resource>,
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
    /// [`set_states`] does.
    pub fn set_states(&mut self, states: &[(PathBuf, State)]) {
        set_states(&mut self.maps, states);
        set_states(&mut self.shaders, states);
    }
}

/// The folders [`find`] reads.
#[derive(Clone, Debug)]
pub struct Folders {
    /// The package's vtlooks, the presets.
    pub presets: PathBuf,
    /// The administrator's vtlooks, and the color maps beside them.
    pub looks: PathBuf,
    /// The shaders' folders, the package's and the administrator's.
    pub shaders: Vec<PathBuf>,
    /// The person's own folders for color maps and shaders, whose files
    /// are listed as not usable.
    pub own_looks: Option<PathBuf>,
    pub own_shaders: Option<PathBuf>,
}

impl Folders {
    /// The folders vtium reads, and the person's own under `config`, the
    /// person's configuration folder, such as `~/.config`.
    pub fn system(config: Option<PathBuf>) -> Folders {
        use vtium_settings::vtlook::{LOOKS, PACKAGE_LOOKS};
        let own = config.map(|c| c.join("vtium"));
        Folders {
            presets: PathBuf::from(PACKAGE_LOOKS),
            looks: PathBuf::from(LOOKS),
            shaders: vec![
                PathBuf::from("/usr/share/vtium/shaders"),
                PathBuf::from("/etc/vtium/shaders"),
            ],
            own_looks: own.as_ref().map(|o| o.join("looks")),
            own_shaders: own.map(|o| o.join("shaders")),
        }
    }
}

/// The files in `dir` whose names end in `suffix`, sorted by name.
fn files(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
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
/// files it names in `looks`, or its first error.
fn read_vtlook(
    name: &str,
    dir: &Path,
    looks: &Path,
    trust: &vtium_settings::Trust,
) -> (Result<Vec<String>, String>, Vec<PathBuf>) {
    use vtium_settings::vtlook::Value;
    match vtium_settings::vtlook::read(name, dir, trust) {
        Ok(look) => {
            let filters = look.color.iter().chain(&look.spatial);
            let files = filters
                .clone()
                .flat_map(|f| f.values.iter())
                .filter_map(|(_, v)| match v {
                    Value::File(file) => Some(looks.join(file)),
                    _ => None,
                })
                .collect();
            (Ok(filters.map(filter_line).collect()), files)
        }
        Err(failure) => (
            Err(failure
                .reports
                .first()
                .map_or_else(String::new, |r| r.message.clone())),
            Vec::new(),
        ),
    }
}

/// What the Effects tab lists, read from `folders`: each vtlook read under
/// `trust`, and each color map and shader with its state from `state` and
/// its trust mark from `trusted`.
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
    let stem = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().trim_end_matches(SUFFIX).to_owned())
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
    Effects {
        vtlooks,
        maps: listed(
            &[&folders.looks],
            folders.own_looks.as_ref(),
            ".cube",
            "color map",
        ),
        shaders: listed(
            &folders.shaders.iter().collect::<Vec<_>>(),
            folders.own_shaders.as_ref(),
            ".glsl",
            "shader",
        ),
    }
}

/// Each look of vtium.conf's text `conf`, `None` for the top level, with
/// the vtlook it names, if any.
fn vtlooks_named(conf: &str) -> Vec<(Option<String>, Option<String>)> {
    let mut out = vec![(None, vtium_settings::setting(conf, "vtlook"))];
    let Some(reading) = vtium_settings::sections(conf.as_bytes()) else {
        return out;
    };
    for look in reading.parsed.config.looks {
        out.push((Some(look.name), look.vtlook));
    }
    out
}

/// The looks of `conf` that name the vtlook `name`, the top level first,
/// each as the deactivate question names it.
pub fn looks_using_vtlook(conf: &str, name: &str) -> Vec<String> {
    vtlooks_named(conf)
        .into_iter()
        .filter(|(_, v)| v.as_deref() == Some(name))
        .map(|(look, _)| words::look_name(look.as_deref()))
        .collect()
}

/// The looks of `conf` whose vtlook, as `effects` holds it, names the
/// color map or shader at `path`.
pub fn looks_using_file(conf: &str, effects: &Effects, path: &Path) -> Vec<String> {
    vtlooks_named(conf)
        .into_iter()
        .filter(|(_, v)| {
            v.as_deref()
                .and_then(|name| effects.vtlook(name))
                .is_some_and(|v| v.files.iter().any(|f| f == path))
        })
        .map(|(look, _)| words::look_name(look.as_deref()))
        .collect()
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn map(state: State, own: bool) -> Resource {
        Resource {
            path: PathBuf::from("/etc/vtium/looks/sepia.cube"),
            kind: "color map",
            state,
            trust: Ok(()),
            own,
        }
    }

    fn effects() -> Effects {
        Effects {
            vtlooks: vec![Vtlook {
                name: "sepia".into(),
                file: Some("/etc/vtium/looks/sepia.vtlook".into()),
                from: From::Administrator,
                hidden: false,
                read: Ok(vec!["cube: file sepia.cube".into()]),
                files: vec!["/etc/vtium/looks/sepia.cube".into()],
            }],
            maps: vec![map(State::Active, false)],
            shaders: Vec::new(),
        }
    }

    // Space activates a resource that is not active, deactivates one that
    // is, and asks first when a look uses it
    #[test]
    fn space_activates_deactivates_or_asks() {
        let op = |change| Op::Resource {
            path: "/etc/vtium/looks/sepia.cube".into(),
            change,
        };
        assert_eq!(
            space(&map(State::Inactive, false), &[]),
            [Command::Effect(Effect::Desk(op(BatchChange::Activate)))]
        );
        assert_eq!(
            space(&map(State::Active, false), &[]),
            [Command::Effect(Effect::Desk(op(BatchChange::Deactivate)))]
        );
        let asked = space(&map(State::Activates, false), &["look desk".into()]);
        let [Command::Request(Request::Ask { question, then })] = asked.as_slice() else {
            panic!("{asked:?}");
        };
        assert!(
            question.contains("Look desk uses it, and renders"),
            "{question}"
        );
        assert_eq!(*then, Then::Desk(op(BatchChange::Deactivate)));
        // a resource in the person's own folder is not usable
        let own = space(&map(State::Inactive, true), &[]);
        assert!(
            matches!(own.as_slice(), [Command::Change(Change::Say(s))] if s.contains("own folder"))
        );
        // one that fails the trust rule is not activated
        let mut bad = map(State::Inactive, false);
        bad.trust = Err("the file belongs to user 1000".into());
        let refused = space(&bad, &[]);
        assert!(
            matches!(refused.as_slice(), [Command::Change(Change::Say(s))] if s.contains("user 1000"))
        );
    }

    #[test]
    fn the_looks_that_name_a_vtlook_use_the_files_it_names() {
        let conf = "vtlook = sepia\nversion = 1\n[look desk]\nvtlook = sepia\n[look plain]\n";
        let e = effects();
        let sepia = Path::new("/etc/vtium/looks/sepia.cube");
        assert_eq!(
            looks_using_file(conf, &e, sepia),
            ["the top level of vtium.conf", "look desk"]
        );
        assert_eq!(looks_using_vtlook(conf, "sepia").len(), 2);
        assert!(looks_using_file(conf, &e, Path::new("/etc/vtium/looks/warm.cube")).is_empty());
        let mut states = e.clone();
        states.set_states(&[]);
        assert_eq!(states.maps.first().unwrap().state, State::Inactive);
    }

    // the presets come before the administrator's vtlooks, and one that the
    // administrator's folder also holds is hidden
    #[test]
    fn find_lists_no_filter_the_presets_and_the_administrator_s_vtlooks() {
        // a folder that nobody else can write, as the trust rule asks of
        // every folder above a vtlook
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("vtium-config-effects-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (pkg, etc, mine) = (root.join("pkg"), root.join("etc"), root.join("mine"));
        for d in [&pkg, &etc, &mine.join("looks")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let tint = "version = 1\n[color tint]\ncolor = 255,176,0\n";
        std::fs::write(pkg.join("amber.vtlook"), tint).unwrap();
        std::fs::write(pkg.join("green.vtlook"), tint).unwrap();
        std::fs::write(etc.join("green.vtlook"), "version = 1\n[color nope]\n").unwrap();
        std::fs::write(etc.join("sepia.cube"), "LUT_3D_SIZE 2\n").unwrap();
        std::fs::write(mine.join("looks/mine.cube"), "LUT_3D_SIZE 2\n").unwrap();
        let folders = Folders {
            presets: pkg,
            looks: etc.clone(),
            shaders: vec![root.join("none")],
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
        let maps: Vec<(String, bool)> = found.maps.iter().map(|m| (m.name(), m.own)).collect();
        assert_eq!(
            maps,
            [
                ("sepia.cube".to_owned(), false),
                ("mine.cube".to_owned(), true)
            ]
        );
        assert!(found.shaders.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
