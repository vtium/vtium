//! The fonts a configuration uses, and where each stands: active, not
//! active, turned off on purpose, failing the trust rule, or missing.
//!
//! A font draws on the console only while the batch holds it, so a look
//! whose font set names a font that is installed and trusted but not
//! active draws without it. [`Used::of`] lists, for the open document,
//! every font file that a font set the configuration uses would draw:
//! the top-level `font-name`, each look's, and the layers of their font
//! sets. Each file carries its [`State`] and the looks that use it, so a
//! screen marks the gap before the console draws without the font, and
//! [`activate()`] activates the fonts that are not active as one change.
//!
//! The walk reads with the code vtium and the font helper read with: the
//! font helper's reader of a font set and its layers, its lookup of a kbd
//! console font, and its comparison of family names; the batch as
//! vtium-settings reads it, with the packages' lists ([`crate::batch::View`]).
//! A font that a set names by its family is every font file the scan
//! found with a face of that family, as the font helper's description
//! names it, because the font helper may pick any face of the family:
//! regular, bold or italic. Before the font helper has described a file,
//! a file whose name is the family's, or the family's and `Regular`,
//! stands for it. The walk reads no font folder: the document holds its
//! font sets, the scan holds the fonts, and only a set's layers and a kbd
//! name are looked up on disk, so a screen walks again after each change
//! and never on a keypress.
//!
//! References:
//! - ADR-1205 D4, "Fonts a configuration uses but has not activated": the function, what it walks, the five states, and Activate missing.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated": where the states show and the marks they show in.
//! - ADR-1204 D5: layers, read from the `layers/` folder of the font sets' directory and put in place.

pub mod activate;
pub mod states;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::batch::View;
use crate::document::Document;
use crate::found::Fonts;
use crate::frit::{Frit, Origin};

pub use activate::{Activated, Scope, activate};
pub use states::{Counts, NOT_ALL_ACTIVE, NOT_INSTALLED, Off, State};

/// What the walk reads besides the document.
pub struct Machine<'a> {
    /// The font files the scan found, with their trust marks and what the
    /// font helper said of each.
    pub fonts: &'a Fonts,
    /// The batch the system holds, with the packages' and root's lists.
    pub view: &'a View,
    /// The layers' folder of the font sets' directory, `layers/` beside
    /// the font sets, or `None` where there is no font sets' directory.
    pub layers: Option<&'a Path>,
    /// kbd's data folders, where a `kbd:` source is looked up.
    pub kbd_dirs: &'a [PathBuf],
    /// The trust rule for a file the scan did not find, such as a path a
    /// font set names outside the font folders: `Err` names the file and
    /// the check it failed.
    pub judge: &'a dyn Fn(&Path) -> Result<(), String>,
}

/// kbd's data folders, in the order the font helper searches them.
pub fn kbd_dirs() -> Vec<PathBuf> {
    crate::sources::KBD_DIRS.iter().map(PathBuf::from).collect()
}

/// Who uses a font: a look, or the top level of vtium.conf for `None`,
/// through a font set, or the font `font-name` names itself, and the
/// set's layer the font comes from, `None` for the set's own sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub look: Option<String>,
    pub font_set: String,
    pub layer: Option<String>,
}

/// One font the configuration uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    /// The font's file, or `None` for a font no file is installed for,
    /// such as a family the scan found no face of.
    pub path: Option<PathBuf>,
    /// How the font set names the font, as in `font:JetBrains Mono` or
    /// `bdf:/usr/share/fonts/misc/ter-u16n.bdf`.
    pub named: String,
    pub state: State,
    /// Every user of the font, in the order the walk met them.
    pub users: Vec<User>,
}

impl Font {
    /// The font's file name, or how the font set names it where no file
    /// is installed, as a list shows the font.
    pub fn name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map_or_else(|| self.named.clone(), |n| n.to_string_lossy().into_owned())
    }
}

/// The fonts the configuration uses, each once, in the order the walk met
/// them: the top level's first, then each look's in the file's order, a
/// set's own sources before its layers'.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Used {
    pub fonts: Vec<Font>,
}

impl Used {
    /// Every font file that a font set the open `document` uses would
    /// draw, each with its state against `machine` and the document's own
    /// changes to the active resources.
    pub fn of(document: &Document, machine: &Machine<'_>) -> Used {
        let mut walk = Walk {
            document,
            machine,
            at: BTreeMap::new(),
            used: Used::default(),
        };
        for (look, value) in font_names(document.text()) {
            walk.font_name(look, &value);
        }
        walk.used
    }

    /// How many of the fonts are in each state but active.
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for font in &self.fonts {
            let n = match font.state {
                State::Active => continue,
                State::NotActive => &mut counts.not_active,
                State::TurnedOff(_) => &mut counts.turned_off,
                State::FailsTrust { .. } => &mut counts.fails_trust,
                State::Missing => &mut counts.missing,
            };
            *n = n.saturating_add(1);
        }
        counts
    }

    /// The fonts that `look` uses, the top level's for `None`.
    pub fn of_look(&self, look: Option<&str>) -> Used {
        Used {
            fonts: self
                .fonts
                .iter()
                .filter(|f| f.users.iter().any(|u| u.look.as_deref() == look))
                .cloned()
                .collect(),
        }
    }

    /// The files Activate missing activates: each font that is installed
    /// and trusted but not active.
    pub fn to_activate(&self) -> Vec<PathBuf> {
        self.fonts
            .iter()
            .filter(|f| f.state == State::NotActive)
            .filter_map(|f| f.path.clone())
            .collect()
    }
}

/// Each `font-name` of vtium.conf's text `conf`, with the look it is for:
/// the top level's, for `None`, then each look's in the file's order. A
/// look that names no font takes the top level's.
fn font_names(conf: &str) -> Vec<(Option<String>, String)> {
    use vtium_settings::keys::FONT_NAME;
    let top = vtium_settings::setting(conf, FONT_NAME.name);
    let mut out: Vec<(Option<String>, String)> = top.iter().map(|v| (None, v.clone())).collect();
    let Some(reading) = vtium_settings::sections(conf.as_bytes()) else {
        return out;
    };
    for look in reading.parsed.config.looks {
        let own = look
            .settings
            .iter()
            .rev()
            .find(|s| s.key.name == FONT_NAME.name)
            .map(|s| s.value.clone());
        if let Some(value) = own.or_else(|| top.clone()) {
            out.push((Some(look.name), value));
        }
    }
    out
}

/// What a source names: a file by its path, a kbd console font by name,
/// or a font by its family.
enum Named<'a> {
    File(&'a Path),
    Kbd(&'a str),
    Family(&'a str),
}

/// The walk's state: the fonts so far, and where each file or missing
/// name sits among them.
struct Walk<'a> {
    document: &'a Document,
    machine: &'a Machine<'a>,
    at: BTreeMap<(Option<PathBuf>, String), usize>,
    used: Used,
}

impl Walk<'_> {
    /// The fonts that `value`, the `font-name` of `look`, names: a
    /// built-in font, which needs no batch; a font file by its absolute
    /// path; a font set of the document; or else a kbd console font.
    fn font_name(&mut self, look: Option<String>, value: &str) {
        if crate::batch::builtin(value) {
            return;
        }
        let user = |layer: Option<String>| User {
            look: look.clone(),
            font_set: value.to_owned(),
            layer,
        };
        if Path::new(value).is_absolute() {
            self.add(Named::File(Path::new(value)), value, user(None));
            return;
        }
        let Some(frit) = self.font_set(value) else {
            self.add(Named::Kbd(value), value, user(None));
            return;
        };
        for source in &frit.sources {
            let mut origins = vec![(&source.origin, source.spelling.as_str())];
            for more in source.script_sources.iter().chain(&source.strikes) {
                origins.push((&more.origin, more.spelling.as_str()));
            }
            for (origin, spelling) in origins {
                let named = match origin {
                    Origin::File(p) | Origin::Bdf(p) | Origin::Hex(p) | Origin::Outline(p) => {
                        Named::File(p)
                    }
                    Origin::Kbd(name) => Named::Kbd(name),
                    Origin::Font(family) => Named::Family(family),
                    // the built-in fonts, vconsole.conf's font and the
                    // kernel's own name no file of the batch
                    Origin::Builtin(_) | Origin::Vconsole | Origin::Console(_) => continue,
                };
                self.add(named, spelling, user(source.layer.clone()));
            }
        }
    }

    /// The font set `name` as the document holds it, with its layers in
    /// place as the console hands it to the font helper: the document's
    /// own text where it adds or changes the set, else the installed set
    /// unless the document removes it. `None` for a name no set has, and
    /// for a set that does not read, which the check reports.
    fn font_set(&self, name: &str) -> Option<Frit> {
        let sets = &self.document.content().font_sets;
        let bytes = match sets.kept.get(name) {
            Some(text) => text.as_bytes(),
            None if sets.removed.contains(name) => return None,
            None => self.document.installed().frits.get(name)?,
        };
        let file = PathBuf::from(format!("{name}.vtfrit"));
        let own = Frit::parse(bytes, &file, name).ok()?;
        let Some(layers) = self.machine.layers.filter(|_| !own.layers.is_empty()) else {
            return Some(own);
        };
        let expanded = vtium_font::set::layer::expand(&own.text, &mut |layer| {
            crate::frit::read_layer(layers, layer)
        });
        Frit::parse_spliced(expanded.text.as_bytes(), &file, name, layers).ok()
    }

    /// Adds the files `named` stands for, or the missing font where it
    /// stands for none, each with `user`.
    fn add(&mut self, named: Named<'_>, spelling: &str, user: User) {
        let files = match named {
            Named::File(path) => vec![path.to_path_buf()],
            Named::Kbd(name) => self.kbd(name).into_iter().collect(),
            Named::Family(family) => self.family(family),
        };
        if files.is_empty() {
            self.put(None, spelling, user);
            return;
        }
        for file in files {
            self.put(Some(file), spelling, user.clone());
        }
    }

    /// Records `user` for the font at `path`, or for the missing font
    /// `spelling` where `path` is `None`, judging a font the walk meets
    /// for the first time.
    fn put(&mut self, path: Option<PathBuf>, spelling: &str, user: User) {
        let key = match &path {
            Some(_) => (path.clone(), String::new()),
            None => (None, spelling.to_owned()),
        };
        if let Some(font) = self.at.get(&key).and_then(|&i| self.used.fonts.get_mut(i)) {
            if !font.users.contains(&user) {
                font.users.push(user);
            }
            return;
        }
        let state = match &path {
            Some(p) => states::judge(p, &self.document.content().active, self.machine),
            None => State::Missing,
        };
        self.at.insert(key, self.used.fonts.len());
        self.used.fonts.push(Font {
            path,
            named: spelling.to_owned(),
            state,
            users: vec![user],
        });
    }

    /// The kbd console font `name`, found as `setfont` finds it, as the
    /// font helper does.
    fn kbd(&self, name: &str) -> Option<PathBuf> {
        use crate::sources::{FONT_SUFFIXES, find};
        find(name, self.machine.kbd_dirs, "consolefonts", &FONT_SUFFIXES)
    }

    /// The font files of `family`: a kbd console font of that name, as the
    /// font helper looks one up first, else every file the scan found
    /// with a face of the family, family names compared as the font
    /// helper compares them, with case, blanks, `-` and `_` ignored. A
    /// file the font helper has not described yet stands for the family
    /// when its name, less its extension, is the family's, or the
    /// family's and `Regular`.
    fn family(&self, family: &str) -> Vec<PathBuf> {
        use crate::outline::family_key;
        if let Some(console) = self.kbd(family) {
            return vec![console];
        }
        let key = family_key(family);
        let regular = format!("{key}regular");
        self.machine
            .fonts
            .files()
            .iter()
            .filter(|f| match &f.described {
                Some(_) => f.faces().iter().any(|face| family_key(&face.family) == key),
                None => f
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(family_key)
                    .is_some_and(|stem| stem == key || stem == regular),
            })
            .map(|f| f.path().to_path_buf())
            .collect()
    }
}
