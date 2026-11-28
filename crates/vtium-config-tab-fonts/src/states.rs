//! Where every font the tab lists stands, worked out once for each state
//! of the document and of the fonts found, and kept until either changes.
//!
//! The tab lists three kinds of font: the files the scan found, the fonts
//! built into vtium, and the fonts a font set of the configuration names
//! that the scan did not find, such as a family no file is installed for.
//! Each carries one of vtium-config-core's five states
//! ([`vtium_config_core::fontuse::State`]), or the built-in fonts' own
//! `● always active`, so the list, the facts and the detail mark a font in
//! the same words as every other screen.
//!
//! The states of the fonts the configuration uses come from
//! [`Used::of`], the one walk that also gives Activate missing its list,
//! and the other files are judged against the same batch by the same
//! rules. Building the batch view reads the batch file and the lists, and
//! the walk reads a font set's layers, so [`Known`] is built again only
//! when the document or the fonts found change, never on a key that only
//! moves, filters or searches. [`builds`] counts the builds, which the
//! tests check stays put while the person types.
//!
//! References:
//! - ADR-1205 D4, "Fonts a configuration uses but has not activated": the
//!   five states, how each is found, and that a walk reads no font
//!   folder.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   every place a font appears marks it in the same words.
//! - ADR-1206 D10: nothing scans the font folders on a keypress.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use vtium_config_core::batch::{self, Change, Changes, Folders, Start, View};
use vtium_config_core::fontuse::{self, Counts, Machine, Off, State, Used};
use vtium_config_core::found::Origin;
use vtium_config_core::found::fonts::FontFile;
use vtium_config_core::ui::Shared;

use crate::words::Group;

/// One font of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// The file numbered so among the files the scan found.
    File(usize),
    /// A font built into vtium, by its name.
    BuiltIn(&'static str),
    /// The font numbered so among the fonts the configuration uses, which
    /// the scan did not find.
    Named(usize),
}

/// What the tab knows of each font, for one state of the document and of
/// the fonts found.
#[derive(Clone, Debug, Default)]
pub struct Known {
    /// The state it was built for, `None` before the first build.
    key: Option<u64>,
    /// The fonts the configuration uses, with their states and users.
    pub used: Used,
    /// Every font of the list, in its groups' order.
    pub items: Vec<Item>,
    /// Each item's group.
    pub groups: Vec<Group>,
    /// Each item's state; `None` for a built-in font.
    pub states: Vec<Option<State>>,
    /// Each item's family and file name, which the search matches.
    pub fields: Vec<[String; 2]>,
    /// Where each file the configuration uses sits in `used`.
    pub used_at: BTreeMap<PathBuf, usize>,
    /// How many fonts the configuration uses in each state but active.
    pub counts: Counts,
    /// What the batch view says of each path a draft does not change:
    /// whether the installed batch holds it.
    pub installed: BTreeMap<PathBuf, bool>,
}

thread_local! {
    static BUILDS: Cell<usize> = const { Cell::new(0) };
}

/// How many times this thread has built what the tab knows: read the
/// batch, walked the configuration and judged every font.
pub fn builds() -> usize {
    BUILDS.with(Cell::get)
}

impl Known {
    /// Whether this was built for the state `shared` is in now.
    pub fn holds(&self, shared: &Shared) -> bool {
        self.key == Some(key(shared))
    }

    /// Everything the tab knows for `shared` as it is now.
    pub fn of(shared: &Shared) -> Known {
        BUILDS.with(|b| b.set(b.get().saturating_add(1)));
        let fonts = &shared.found.fonts;
        let view = batch_view(shared);
        let production = shared.found.places.as_ref();
        let euid = production.map_or(0, |p| p.euid);
        let judge = move |path: &Path| batch::trusted(path, euid);
        let fixture_judge = |_: &Path| Ok(());
        let layers = production.and_then(|_| layers_folder(shared));
        let kbd = if production.is_some() {
            fontuse::kbd_dirs()
        } else {
            Vec::new()
        };
        let machine = Machine {
            fonts,
            view: &view,
            layers: layers.as_deref(),
            kbd_dirs: &kbd,
            judge: if production.is_some() {
                &judge
            } else {
                &fixture_judge
            },
        };
        let document = shared.document();
        let used = Used::of(document, &machine);
        let changes = &document.content().active;
        let used_at: BTreeMap<PathBuf, usize> = used
            .fonts
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.path.clone().map(|p| (p, i)))
            .collect();

        let mut rows: Vec<(Group, Item, Option<State>)> = Vec::new();
        for (i, f) in fonts.files().iter().enumerate() {
            let state = match used_at.get(f.path()).and_then(|&u| used.fonts.get(u)) {
                Some(u) => u.state.clone(),
                None => judge_file(f, changes, &view),
            };
            rows.push((Group::of(f.place), Item::File(i), Some(state)));
        }
        for name in &fonts.built_in {
            rows.push((Group::Vtium, Item::BuiltIn(name), None));
        }
        for (u, font) in used.fonts.iter().enumerate() {
            if font
                .path
                .as_deref()
                .is_some_and(|p| fonts.file(p).is_some())
            {
                continue;
            }
            let group = if font.state == State::Missing {
                Group::NotInstalled
            } else {
                Group::Elsewhere
            };
            rows.push((group, Item::Named(u), Some(font.state.clone())));
        }
        // a stable sort, so each group keeps the scan's order
        rows.sort_by_key(|(g, _, _)| *g);

        let mut installed = BTreeMap::new();
        for f in fonts.files() {
            installed.insert(f.path().to_path_buf(), view.batch().holds(f.path()));
        }
        let fields = rows
            .iter()
            .map(|(_, item, _)| fields_of(shared, &used, *item))
            .collect();
        Known {
            key: Some(key(shared)),
            counts: used.counts(),
            items: rows.iter().map(|(_, i, _)| *i).collect(),
            groups: rows.iter().map(|(g, _, _)| *g).collect(),
            states: rows.into_iter().map(|(_, _, s)| s).collect(),
            fields,
            used_at,
            installed,
            used,
        }
    }

    /// The fonts the configuration uses that Activate missing activates.
    pub fn to_activate(&self) -> Vec<PathBuf> {
        self.used.to_activate()
    }

    /// The users of the file at `path`, where the configuration uses it.
    pub fn users_of(&self, path: &Path) -> &[fontuse::User] {
        self.used_at
            .get(path)
            .and_then(|&u| self.used.fonts.get(u))
            .map_or(&[], |f| f.users.as_slice())
    }

    /// The users of `item`.
    pub fn users(&self, shared: &Shared, item: Item) -> &[fontuse::User] {
        match item {
            Item::File(i) => shared
                .found
                .fonts
                .files()
                .get(i)
                .map_or(&[], |f| self.users_of(f.path())),
            Item::Named(u) => self.used.fonts.get(u).map_or(&[], |f| f.users.as_slice()),
            Item::BuiltIn(_) => &[],
        }
    }

    /// The position of `item` in the list.
    pub fn position(&self, item: Item) -> Option<usize> {
        self.items.iter().position(|i| *i == item)
    }

    /// The state of the item at `at` in the list.
    pub fn state(&self, at: usize) -> Option<&State> {
        self.states.get(at).and_then(Option::as_ref)
    }

    /// Whether the item at `at` is active, the built-in fonts included.
    pub fn active(&self, at: usize) -> bool {
        match self.states.get(at) {
            Some(None) => true,
            Some(Some(s)) => *s == State::Active,
            None => false,
        }
    }
}

/// The state of the found file `f`, which no font set of the
/// configuration names, judged as vtium-config-core judges a file that
/// one names: the trust rule first, then the document's change, then the
/// installed batch and what root turned off.
fn judge_file(f: &FontFile, changes: &Changes, view: &View) -> State {
    let path = f.path();
    let own = f.resource.own;
    if let Err(check) = &f.resource.trust {
        return State::FailsTrust {
            check: check.clone(),
            own,
        };
    }
    if own {
        return State::FailsTrust {
            check: format!(
                "{}: this file is in your own folders, where vtium takes no font. Install it as a system font to use it.",
                path.display()
            ),
            own,
        };
    }
    match changes.get(path) {
        Some(Change::Activate) => State::Active,
        Some(Change::Deactivate) => State::TurnedOff(Off::Document),
        None if view.batch().holds(path) => State::Active,
        None => match view.off(path) {
            Some(batch::Off::BatchFile) => State::TurnedOff(Off::BatchFile),
            Some(batch::Off::List(by)) => State::TurnedOff(Off::List(by)),
            None => State::NotActive,
        },
    }
}

/// The family and file name of `item`, which the `?` search matches.
fn fields_of(shared: &Shared, used: &Used, item: Item) -> [String; 2] {
    match item {
        Item::File(i) => {
            let f = shared.found.fonts.files().get(i);
            let family = f.and_then(FontFile::family).unwrap_or_default();
            let file = f.map(|f| f.resource.name()).unwrap_or_default();
            [family.to_owned(), file]
        }
        Item::BuiltIn(name) => [name.to_owned(), name.to_owned()],
        Item::Named(u) => {
            let font = used.fonts.get(u);
            let named = font.map(|f| f.named.clone()).unwrap_or_default();
            let family = named
                .strip_prefix("font:")
                .map_or_else(|| named.clone(), str::to_owned);
            let file = font
                .and_then(|f| f.path.as_deref())
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            [family, file]
        }
    }
}

/// A digest of what the states depend on: the document's text, its font
/// sets and its changes to the active resources, what is installed, and
/// the fonts found with what the font helper has said of them.
fn key(shared: &Shared) -> u64 {
    let mut h = DefaultHasher::new();
    let document = shared.document();
    let content = document.content();
    document.name().hash(&mut h);
    content.text.hash(&mut h);
    content.font_sets.kept.hash(&mut h);
    content.font_sets.removed.hash(&mut h);
    for (path, change) in content.active.iter() {
        path.hash(&mut h);
        matches!(change, Change::Activate).hash(&mut h);
    }
    let installed = document.installed();
    installed.text.hash(&mut h);
    installed.frits.hash(&mut h);
    let fonts = &shared.found.fonts;
    fonts.files().len().hash(&mut h);
    fonts.waiting().count().hash(&mut h);
    shared.found.places.is_some().hash(&mut h);
    h.finish()
}

/// The batch vtium reads. On this machine it is the batch file, or the
/// list of a fresh install, with the lists of both folders, read as the
/// console reads them, under root's trust. Where the fonts found carry no
/// places, as on a test's machine, it is the batch their origins record:
/// each path a list or the batch file activates, and each path the batch
/// file turns off.
fn batch_view(shared: &Shared) -> View {
    let trust = vtium_settings::Trust::running_as(0);
    match &shared.found.places {
        Some(places) => {
            let start = Start {
                fresh: vtium_settings::batch::Batch::fresh(&places.batch),
                folders: Folders {
                    package: places.batch.package_lists(),
                    admin: places.batch.admin_lists(),
                },
                trust,
            };
            batch::Installed::read(&places.batch.file, 0).view(&start)
        }
        None => {
            let mut text = String::new();
            for f in shared.found.fonts.files() {
                let line = match &f.resource.origin {
                    Origin::BatchFile | Origin::FreshInstall | Origin::List { .. } => {
                        f.path().display().to_string()
                    }
                    Origin::TurnedOff { .. } => format!("-{}", f.path().display()),
                    Origin::Nothing => continue,
                };
                text.push_str(&line);
                text.push('\n');
            }
            let start = Start {
                fresh: vtium_settings::batch::Batch::default(),
                folders: Folders {
                    package: PathBuf::new(),
                    admin: PathBuf::new(),
                },
                trust,
            };
            View::new(Some(text.as_bytes()), &start)
        }
    }
}

/// The layers' folder of the font sets' directory that the document's
/// `font-frits` names, else the build's.
fn layers_folder(shared: &Shared) -> Option<PathBuf> {
    let dir = vtium_settings::setting(shared.document().text(), "font-frits")
        .filter(|d| Path::new(d).is_absolute())
        .unwrap_or_else(|| vtium_settings::keys::FRITS.to_owned());
    Some(PathBuf::from(dir).join(vtium_font::set::layer::DIR))
}
