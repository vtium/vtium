//! The fonts found: each font file in the font folders, with the folder
//! it was found in, its marks and what the font helper said of it, and
//! the fonts built into vtium, which have no file.
//!
//! The scan finds the files as `vtium-config fonts --files` finds them
//! ([`crate::discover::scan`]): in the system's folders, vtium's own, kbd's
//! console-font folders, the folders that `font-paths` names, and the
//! person's own folders, opening no font. The font helper then describes
//! the files while the TUI goes on reading keys, and each answer joins the
//! list as it comes ([`Fonts::describe`]). A description gives a font its
//! family, its faces, the scripts and groups it covers and its sizes,
//! which the Fonts tab's filters test and its detail shows.
//!
//! References:
//! - ADR-1205 D3: the folders, a font in a user's folder listed as not usable, and the font helper as the one reader of a font's family, coverage and sizes.
//! - ADR-1205 D5: a font in the administrator's folder that a package also installed carries a duplicate mark.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::describe::{Face, Group, SCRIPTS};
use crate::discover::{Described, Duplicate, Place};

use super::{Kind, Origin, Places, Resource, counted};

/// One font file found, with its marks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFile {
    pub resource: Resource,
    /// The kind of folder it was found under.
    pub place: Place,
    /// The file a package installed that this font repeats.
    pub duplicate: Option<Duplicate>,
    /// What the font helper said of it, once it has answered.
    pub described: Option<Described>,
    /// The font helper has been asked to describe it, answered or not.
    pub asked: bool,
}

impl FontFile {
    /// The file's path.
    pub fn path(&self) -> &Path {
        &self.resource.path
    }

    /// The faces the font helper found in it: none until it answers, and
    /// none for a file it cannot read as a font.
    pub fn faces(&self) -> &[Face] {
        match &self.described {
            Some(Described::Font(d)) => &d.faces,
            _ => &[],
        }
    }

    /// Its family, as its first face names it, once the font helper has
    /// described it.
    pub fn family(&self) -> Option<&str> {
        self.faces().first().map(|f| f.family.as_str())
    }

    /// What the `?` search matches the font on: its family, where the font
    /// helper has named it, and its file's name.
    pub fn fields(&self) -> impl Iterator<Item = &str> {
        let name = self.path().file_name().and_then(|n| n.to_str());
        self.family().into_iter().chain(name)
    }
}

/// The fonts found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fonts {
    /// The font files, in the scan's order: by folder, then by path.
    files: Vec<FontFile>,
    /// The fonts built into vtium, by name, which are always active.
    pub built_in: Vec<&'static str>,
    /// Why the list is empty or its marks may be wrong, such as a mount
    /// table vtium-config cannot read, so it cannot judge the files.
    pub note: Option<String>,
    /// Where each file sits in `files`, so a description finds its file
    /// without a search of the list.
    at: BTreeMap<PathBuf, usize>,
}

impl Fonts {
    /// The list of `files`, with the built-in fonts and nothing described.
    pub fn new(files: Vec<FontFile>) -> Fonts {
        let at = files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.path().to_path_buf(), i))
            .collect();
        Fonts {
            files,
            built_in: built_in(),
            note: None,
            at,
        }
    }

    /// The font files, in the scan's order: by folder, then by path.
    pub fn files(&self) -> &[FontFile] {
        &self.files
    }

    /// The file at `path`, where the scan found it.
    pub fn file(&self, path: &Path) -> Option<&FontFile> {
        self.files.get(*self.at.get(path)?)
    }

    /// Records what the font helper said of each file in `answers`. An
    /// answer for a file the list does not hold is dropped.
    pub fn describe(&mut self, answers: Vec<(PathBuf, Described)>) {
        for (path, described) in answers {
            if let Some(f) = self.at.get(&path).and_then(|i| self.files.get_mut(*i)) {
                f.described = Some(described);
                f.asked = true;
            }
        }
    }

    /// The files the font helper has not been asked to describe yet, in
    /// the list's order, which are marked as asked, so a file is never
    /// asked for twice.
    pub fn ask(&mut self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for f in self.files.iter_mut().filter(|f| !f.asked) {
            f.asked = true;
            out.push(f.path().to_path_buf());
        }
        out
    }

    /// The files the font helper has not answered for yet, in the list's
    /// order.
    pub fn waiting(&self) -> impl Iterator<Item = &FontFile> {
        self.files.iter().filter(|f| f.described.is_none())
    }

    /// Forgets what the font helper said of the file at `path`, and that it
    /// was asked, so the next [`Fonts::ask`] asks for it again. An install
    /// that replaces a file keeps the file's path, so the list would
    /// otherwise show the replaced file's description.
    pub fn forget(&mut self, path: &Path) {
        if let Some(f) = self.at.get(path).and_then(|i| self.files.get_mut(*i)) {
            f.described = None;
            f.asked = false;
        }
    }

    /// Takes over from `old`, the list before a new scan, what the font
    /// helper said of each file that is still here.
    pub(super) fn keep_descriptions(&mut self, old: &mut Fonts) {
        for f in &mut self.files {
            if let Some(was) = old.at.get(f.path()).and_then(|i| old.files.get_mut(*i)) {
                f.described = was.described.take();
                f.asked = was.asked;
            }
        }
    }

    /// The files that `filter` keeps, in the list's order.
    pub fn kept(&self, filter: Filter) -> impl Iterator<Item = &FontFile> {
        self.files.iter().filter(move |f| filter.keeps(f))
    }
}

/// The fonts built into vtium, by the names a font set gives them.
fn built_in() -> Vec<&'static str> {
    crate::frit::RESERVED
        .iter()
        .copied()
        .filter(|n| vtium_font::set::builtin(n).is_some())
        .collect()
}

/// The font files at `places`, each with its trust mark under the rule of
/// `places.euid`, its duplicate mark, and its activation's origin from
/// `origin`. When vtium-config cannot read the mount table, which the
/// trust rule needs, the list holds no file and its note says why.
pub(super) fn scan(places: &Places, origin: &dyn Fn(&Path) -> Origin) -> Fonts {
    counted(|s| &mut s.fonts);
    let mounts = match vtium_settings::mounts::Mounts::system() {
        Ok(m) => m,
        Err(e) => {
            let mut out = Fonts::new(Vec::new());
            out.note = Some(format!(
                "vtium-config cannot judge the font files, because it cannot read the mount table: {e}."
            ));
            return out;
        }
    };
    let rule = vtium_settings::cullet::Rule::running_as(places.euid);
    let found = crate::discover::scan(&places.fonts, &rule, &mounts, &|_| {
        crate::batch::State::Inactive
    });
    Fonts::new(
        found
            .into_iter()
            .map(|f| FontFile {
                resource: Resource {
                    own: !f.usable(),
                    origin: origin(&f.path),
                    path: f.path,
                    kind: Kind::Font,
                    trust: f.trust,
                    hidden: false,
                },
                place: f.folder.place,
                duplicate: f.duplicate,
                described: None,
                asked: false,
            })
            .collect(),
    )
}

/// The filters of the Fonts tab, in the order the list of filters shows
/// them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    /// Every font, described or not.
    #[default]
    All,
    Bitmap,
    Outline,
    Monospaced,
    Proportional,
    Group(Group),
    /// A script, by its name in Unicode's Scripts.txt.
    Script(&'static str),
}

impl Filter {
    /// Every filter, in the list's order: all fonts, the kinds, the
    /// groups, then each script.
    pub fn choices() -> Vec<Filter> {
        let mut out = vec![
            Filter::All,
            Filter::Bitmap,
            Filter::Outline,
            Filter::Monospaced,
            Filter::Proportional,
        ];
        out.extend(Group::ALL.iter().map(|g| Filter::Group(*g)));
        out.extend(SCRIPTS.iter().map(|s| Filter::Script(s.name)));
        out
    }

    /// Whether the filter keeps `font`. Only All keeps a font the font
    /// helper has not described, or one it cannot read, since nothing is
    /// known of it. A font is kept when any of its faces passes.
    pub fn keeps(self, font: &FontFile) -> bool {
        if self == Filter::All {
            return true;
        }
        font.faces().iter().any(|f| match self {
            Filter::All => true,
            Filter::Bitmap => !f.outline,
            Filter::Outline => f.outline,
            Filter::Monospaced => f.monospaced,
            Filter::Proportional => !f.monospaced,
            Filter::Group(g) => f.groups.contains(&g),
            Filter::Script(s) => f.scripts.contains(&s),
        })
    }
}
