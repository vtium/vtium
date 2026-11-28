//! The resources found on the machine, as the shared state holds them for
//! the Fonts, Looks and Effects tabs: the font files and the built-in
//! fonts ([`fonts`]), the vtlooks, color maps and shaders ([`effects`]),
//! the color schemes ([`palettes`]) and the pointer themes
//! ([`pointers`]). A resource is a file vtium draws from, and vtium uses
//! it only while the batch, the list of active resources, holds it.
//!
//! [`Found::find`] reads every folder once, when vtium-config starts, and
//! [`Found::again`] reads them again when the person asks to read the
//! devices again or installs a font as a system font. Nothing else reads a
//! folder: a tab filters and searches the lists held here, so a keypress
//! costs no scan, even on a small board with thousands of fonts.
//!
//! Every resource carries the same marks ([`Resource`]): whether it passes
//! the trust rule, whether it sits in the person's own folder, where vtium
//! cannot use it, whether a file of the same name in the administrator's
//! folder hides it, and where the installed batch's activation of it comes
//! from: the batch file, the list of a fresh install, or a list that a
//! package or root keeps in `batch.d`. [`Resource::activation`] adds the
//! open document's draft, which may activate or deactivate it. The state a
//! list shows for a resource, such as active or not active, is worked out
//! from these marks and the document by the code that shows it.
//!
//! References:
//! - ADR-1205 D3: the folders vtium-config finds the fonts and the effects in, and the font helper as the one reader of a font.
//! - ADR-1205 D4, as amended on 2026-11-27: the batch, the trust rule, the packages' lists and the `-` line of the batch file that turns a path off.
//! - ADR-1206 D2: what `shared` holds, which every tab reads.
//! - ADR-1206 D10, "Responsive on a small board": nothing scans the font folders on a keypress.

pub mod effects;
pub mod fonts;
pub mod palettes;
pub mod pointers;

use std::cell::Cell;
use std::path::{Path, PathBuf};

use vtium_settings::batch::{Batch, Folder as ListFolder, Origin as BatchOrigin};

use crate::batch::{Change, Changes};

pub use effects::Effects;
pub use fonts::Fonts;
pub use palettes::Palettes;

/// What kind of resource a file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Font,
    ColorMap,
    Shader,
    ColorScheme,
    PointerTheme,
}

impl Kind {
    /// The kind in the words a person uses, as the batch's commands name
    /// it.
    pub const fn words(self) -> &'static str {
        match self {
            Kind::Font => "font",
            Kind::ColorMap => "color map",
            Kind::Shader => "shader",
            Kind::ColorScheme => "color scheme",
            Kind::PointerTheme => "pointer theme",
        }
    }
}

/// Where the installed batch's activation of a resource comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// No list activates it.
    Nothing,
    /// The batch file, `/etc/vtium/batch.conf`, lists it.
    BatchFile,
    /// The list of a fresh install lists it, which vtium uses while it
    /// reads no batch file.
    FreshInstall,
    /// A list in a `batch.d` folder names it: a package's list, such as
    /// vtium-fonts's, or one of root's own.
    List { name: String, folder: ListFolder },
    /// The batch file turns it off with a `-` line, so it is not active,
    /// although the list `list` names it, where one does.
    TurnedOff { list: Option<String> },
}

impl Origin {
    /// Where `batch`, the installed batch, activates `path` from.
    pub fn of(batch: &Batch, path: &Path) -> Origin {
        if batch.turns_off(path) {
            return Origin::TurnedOff {
                list: batch.listed_by(path).map(|l| l.name.clone()),
            };
        }
        match batch.origin(path) {
            None => Origin::Nothing,
            Some(BatchOrigin::BatchFile) => Origin::BatchFile,
            Some(BatchOrigin::FreshInstall) => Origin::FreshInstall,
            Some(BatchOrigin::List(list)) => Origin::List {
                name: list.name.clone(),
                folder: list.folder,
            },
        }
    }
}

/// Where a resource's activation comes from, once the open document's
/// draft is counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation<'a> {
    /// The built-in font, which is always active and has no file.
    BuiltIn,
    /// The installed batch, as the draft leaves it.
    Installed(&'a Origin),
    /// The draft makes this change to it, which Save makes real.
    Draft(Change),
}

/// One resource found, with the marks every resource carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    /// The file, or a pointer theme's folder, by the path it was found at.
    pub path: PathBuf,
    pub kind: Kind,
    /// Whether it passes the trust rule, or the check it fails, file
    /// first, in the words of `vtium check-config`.
    pub trust: Result<(), String>,
    /// It is in the person's own folder, so vtium cannot use it until it
    /// is installed for the system.
    pub own: bool,
    /// A packaged file that the administrator's file of the same name
    /// hides, so vtium reads the administrator's file in its place.
    pub hidden: bool,
    /// Where the installed batch's activation of it comes from.
    pub origin: Origin,
}

impl Resource {
    /// The file's name, as a list shows it.
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Where its activation comes from with the open document's draft
    /// changes `changes`: the draft's change where it makes one, else the
    /// installed batch. The lookup costs no scan of the changes.
    pub fn activation<'a>(&'a self, changes: &Changes) -> Activation<'a> {
        match changes.get(&self.path) {
            Some(change) => Activation::Draft(change),
            None => Activation::Installed(&self.origin),
        }
    }
}

/// Where the resources are found, and whose files the trust rule accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// The font folders, in the order the scan reads them.
    pub fonts: crate::discover::Folders,
    /// The vtlooks', color maps' and shaders' folders.
    pub effects: effects::Folders,
    /// The color schemes' folders, in the order vtium looks a name up in,
    /// the administrator's first.
    pub palettes: Vec<PathBuf>,
    /// vtium's own pointer pictures, and the icon folders the desktop's
    /// cursor themes are installed in.
    pub pointers: pointers::Folders,
    /// Where vtium reads the batch from: the batch file, the list
    /// folders, and what builds the list of a fresh install.
    pub batch: vtium_settings::batch::Places,
    /// The user whose files the trust rule accepts besides root's: root
    /// for an install to the system's folders.
    pub euid: u32,
}

impl Places {
    /// This machine's places, with the font folders that `font-paths`
    /// names in vtium.conf's text `conf`, the person's own folders from
    /// the environment, and the trust rule of the user `euid`.
    pub fn system(conf: Option<&str>, euid: u32) -> Places {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|c| !c.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .filter(|c| c.is_absolute());
        Places {
            fonts: crate::discover::Folders::of_user(conf),
            effects: effects::Folders::system(config),
            palettes: vtium_settings::palette::FOLDERS
                .iter()
                .map(PathBuf::from)
                .collect(),
            pointers: pointers::Folders::system(),
            batch: vtium_settings::batch::Places::system(),
            euid,
        }
    }
}

/// How many times this thread has read each kind of resource's folders,
/// which the tests check stays at one while the person types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scans {
    pub fonts: usize,
    pub effects: usize,
    pub palettes: usize,
    pub pointers: usize,
}

thread_local! {
    static SCANS: Cell<Scans> = const {
        Cell::new(Scans { fonts: 0, effects: 0, palettes: 0, pointers: 0 })
    };
}

/// How many times this thread has read each kind's folders so far.
pub fn scans() -> Scans {
    SCANS.with(Cell::get)
}

/// Counts one read of the folders that `kind` picks from [`Scans`].
fn counted(kind: fn(&mut Scans) -> &mut usize) {
    SCANS.with(|s| {
        let mut now = s.get();
        let n = kind(&mut now);
        *n = n.saturating_add(1);
        s.set(now);
    });
}

/// The installed batch at `places`, as vtium reads it: the batch file, or
/// the list of a fresh install, with the lists of the two list folders.
fn installed_batch(places: &Places) -> Batch {
    let trust = vtium_settings::Trust::running_as(places.euid);
    Batch::system(&places.batch, &trust).batch
}

/// Every resource found on the machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub fonts: Fonts,
    pub effects: Effects,
    pub palettes: Palettes,
    pub pointers: Vec<Resource>,
    /// Where the resources were found, which [`Found::again`] reads again.
    pub places: Option<Places>,
}

impl Found {
    /// Reads every resource at `places`, once, with its marks. The font
    /// helper describes the fonts later, as its answers come
    /// ([`Fonts::describe`]).
    pub fn find(places: Places) -> Found {
        let batch = installed_batch(&places);
        let origin = |path: &Path| Origin::of(&batch, path);
        Found {
            fonts: fonts::scan(&places, &origin),
            effects: effects::scan(&places, &origin),
            palettes: palettes::scan(&places, &origin),
            pointers: pointers::scan(&places, &origin),
            places: Some(places),
        }
    }

    /// Reads every resource again at the same places, as Read devices
    /// again and Install as a system font do, keeping what the font helper
    /// said of each font file that is still there. A list found from a
    /// fixture, with no places, stays as it is.
    pub fn again(&mut self) {
        let Some(places) = self.places.take() else {
            return;
        };
        let mut next = Found::find(places);
        next.fonts.keep_descriptions(&mut self.fonts);
        *self = next;
    }

    /// The resource at `path`, of any kind.
    pub fn resource(&self, path: &Path) -> Option<&Resource> {
        self.fonts
            .files()
            .iter()
            .map(|f| &f.resource)
            .chain(&self.effects.maps)
            .chain(self.effects.shaders.iter().map(|s| &s.resource))
            .chain(self.palettes.files.iter().map(|f| &f.resource))
            .chain(&self.pointers)
            .find(|r| r.path == path)
    }
}

/// The files in `dir` whose names end in `suffix`, sorted by path. A
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

/// The file's name without `suffix`, as vtium joins a name and the suffix
/// to find the file.
fn stem(path: &Path, suffix: &str) -> String {
    path.file_name()
        .map(|n| {
            let n = n.to_string_lossy();
            n.strip_suffix(suffix).unwrap_or(&n).to_owned()
        })
        .unwrap_or_default()
}
