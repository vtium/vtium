//! The desk that a document's draft is written through. [`OnDisk`] is the
//! real one: vtium-config's store under the person's XDG directories,
//! where the draft, its font set folder, its changes to the active
//! resources and its base live. [`InMemory`] keeps everything in memory,
//! for the tests, for the TUI's fixtures, and for the scratch copy that a
//! macro plays on.
//!
//! References:
//! - ADR-1403 D8, "vtium-config's store: drafts and the library": the files a draft keeps.
//! - ADR-1208 D5: a macro plays on a scratch copy unless a live run is asked for and allowed.

use std::collections::BTreeMap;
use std::path::Path;

use super::{Base, Content, Document, Installed};
use crate::outcome::Fail;

/// Where a document's draft is written, and where the configurations and
/// the installed versions are listed from.
pub trait Desk: std::fmt::Debug {
    /// Writes `content` as the draft `name`: its vtium.conf, its font set
    /// changes, its changes to the active resources and its base.
    fn write(&mut self, name: &str, content: &Content) -> Result<(), String>;

    /// What the draft `name` holds, `None` when there is no such draft:
    /// for File › Open, and for a test that checks what a change wrote.
    fn read(&self, name: &str) -> Result<Option<Content>, String>;

    /// The configurations kept, by name, in the order of their names.
    fn configurations(&self) -> Result<Vec<String>, String>;

    /// The versions installed before, newest first.
    fn versions(&self) -> Result<Vec<Version>, String>;
}

/// A version installed before, which the installed history keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// The configuration whose save replaced it.
    pub draft: String,
    /// Its number among that configuration's versions, from 1.
    pub number: u32,
    /// When it was replaced, in seconds since 1970.
    pub when: u64,
    /// Its vtium.conf.
    pub text: String,
}

/// A desk that keeps every draft in memory and writes nothing to disk. It
/// counts its writes, so a test sees each one at once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InMemory {
    drafts: BTreeMap<String, Content>,
    versions: Vec<Version>,
    writes: usize,
    /// Why every write fails, for a test of a draft that cannot be
    /// written.
    refuse: Option<String>,
}

impl InMemory {
    pub fn new() -> InMemory {
        InMemory::default()
    }

    /// This desk with the draft `name` holding `content`.
    #[must_use]
    pub fn with_draft(mut self, name: &str, content: Content) -> InMemory {
        self.drafts.insert(name.to_owned(), content);
        self
    }

    /// This desk with `version` in its installed history, which lists the
    /// versions in the order they were added, newest first.
    #[must_use]
    pub fn with_version(mut self, version: Version) -> InMemory {
        self.versions.push(version);
        self
    }

    /// How many writes the desk has taken.
    pub fn writes(&self) -> usize {
        self.writes
    }

    /// Makes every later write fail with `why`, or work again with `None`.
    pub fn refuse(&mut self, why: Option<String>) {
        self.refuse = why;
    }
}

impl Desk for InMemory {
    fn write(&mut self, name: &str, content: &Content) -> Result<(), String> {
        if let Some(why) = &self.refuse {
            return Err(why.clone());
        }
        crate::store::named(name).map_err(|f| f.text().to_owned())?;
        self.drafts.insert(name.to_owned(), content.clone());
        self.writes = self.writes.saturating_add(1);
        Ok(())
    }

    fn read(&self, name: &str) -> Result<Option<Content>, String> {
        Ok(self.drafts.get(name).cloned())
    }

    fn configurations(&self) -> Result<Vec<String>, String> {
        Ok(self.drafts.keys().cloned().collect())
    }

    fn versions(&self) -> Result<Vec<Version>, String> {
        Ok(self.versions.clone())
    }
}

/// The real desk: vtium-config's store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnDisk {
    store: crate::store::Store,
}

impl OnDisk {
    pub fn new(store: crate::store::Store) -> OnDisk {
        OnDisk { store }
    }

    /// Opens the current draft as a document, making one from the
    /// installed file when there is none: its text, its font set folder,
    /// its changes to the active resources, and its base, with the base's
    /// text where the base is the installed file. Also what is installed
    /// now: vtium.conf and the font sets of the font sets' directory.
    pub fn open(session: &crate::drafts::Session<'_>) -> Result<Document, Fail> {
        let (name, text) = session.current()?;
        let store = &session.store;
        let installed = session.installed()?;
        let base = match store.base(&name)? {
            Some(hash) => {
                let same = hash
                    == installed
                        .as_deref()
                        .map(|t| crate::store::hash(t.as_bytes()));
                Base {
                    text: if same { installed.clone() } else { None },
                    hash,
                }
            }
            None => Base::of(installed.as_deref()),
        };
        let content = Content {
            text,
            font_sets: store.frit_changes(&name)?,
            active: store.batch_changes(&name)?,
            base,
        };
        let frits = match &session.paths.frits {
            Ok(dir) => installed_frits(dir),
            Err(_) => BTreeMap::new(),
        };
        Ok(Document::new(
            name,
            content,
            Installed {
                text: installed,
                frits,
            },
        ))
    }
}

/// Each font set in the font sets' directory `dir` and its category
/// folders, by its name, as its bytes, when it is no larger than the
/// largest font set the glyph helper reads. A built-in font's name is no
/// font set.
fn installed_frits(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    use std::io::Read;
    let max = u64::try_from(crate::frit::FRIT_BYTES_MAX).unwrap_or(0);
    let mut out = BTreeMap::new();
    for (name, path) in crate::switch::frit_files(dir) {
        if crate::frit::RESERVED.contains(&name.as_str()) {
            continue;
        }
        let mut bytes = Vec::new();
        let read = std::fs::File::open(&path)
            .and_then(|f| f.take(max.saturating_add(1)).read_to_end(&mut bytes));
        if read.is_ok() && bytes.len() <= crate::frit::FRIT_BYTES_MAX {
            out.insert(name, bytes);
        }
    }
    out
}

impl Desk for OnDisk {
    /// Writes the draft's files that differ from `content`: vtium.conf,
    /// `NAME.batch`, each font set change in `NAME.frits/`, and the base.
    fn write(&mut self, name: &str, content: &Content) -> Result<(), String> {
        let store = &self.store;
        let fail = |f: Fail| f.text().to_owned();
        store.write_draft(name, &content.text).map_err(fail)?;
        store
            .set_batch_changes(name, &content.active)
            .map_err(fail)?;
        let now = store.frit_changes(name).map_err(fail)?;
        let sets = &content.font_sets;
        for frit in now.kept.keys().chain(&now.removed) {
            if !sets.kept.contains_key(frit) && !sets.removed.contains(frit) {
                store.forget_frit(name, frit).map_err(fail)?;
            }
        }
        for (frit, text) in &sets.kept {
            if now.kept.get(frit) != Some(text) {
                store.keep_frit(name, frit, text).map_err(fail)?;
            }
        }
        for frit in &sets.removed {
            if !now.removed.contains(frit) {
                store.keep_removal(name, frit).map_err(fail)?;
            }
        }
        if store.base(name).map_err(fail)? != Some(content.base.hash.clone()) {
            let text = content
                .base
                .text
                .as_deref()
                .filter(|t| Some(crate::store::hash(t.as_bytes())) == content.base.hash);
            match (&content.base.hash, text) {
                (None, _) => store.set_base(name, None).map_err(fail)?,
                (Some(_), Some(text)) => {
                    store.set_base(name, Some(text.as_bytes())).map_err(fail)?
                }
                (Some(_), None) => return Err(words::BASE_WITHOUT_TEXT.to_owned()),
            }
        }
        Ok(())
    }

    /// The draft's files as the store holds them. The store keeps a base's
    /// hash alone, so the base's text is `None`.
    fn read(&self, name: &str) -> Result<Option<Content>, String> {
        let store = &self.store;
        let fail = |f: Fail| f.text().to_owned();
        let Some(text) = store.read_draft(name).map_err(fail)? else {
            return Ok(None);
        };
        Ok(Some(Content {
            text,
            font_sets: store.frit_changes(name).map_err(fail)?,
            active: store.batch_changes(name).map_err(fail)?,
            base: Base {
                hash: store.base(name).map_err(fail)?.flatten(),
                text: None,
            },
        }))
    }

    fn configurations(&self) -> Result<Vec<String>, String> {
        self.store.drafts().map_err(|f| f.text().to_owned())
    }

    fn versions(&self) -> Result<Vec<Version>, String> {
        let mut out = Vec::new();
        for kept in self.store.history().map_err(|f| f.text().to_owned())? {
            let text = crate::places::read_conf(&kept.path)?.unwrap_or_default();
            out.push(Version {
                draft: kept.draft,
                number: kept.number,
                when: kept.when,
                text,
            });
        }
        Ok(out)
    }
}

/// The desk's own words.
mod words {
    /// Why the real desk cannot write a base it knows only by its hash.
    pub const BASE_WITHOUT_TEXT: &str = "vtium-config cannot record this draft's base: it knows only the base's hash, and the store records a base from the installed file's text.";
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::document::{Change, Documents, Edit, FontSetEdit};
    use crate::scratch::Scratch;

    const INSTALLED: &str = "version = 1\n[look My-Look]\nfont-size = 10\n";

    /// A machine under `dir`: vtium.conf installed with the font set mono,
    /// and an empty store.
    fn machine(dir: &Path) -> (crate::places::Paths, crate::store::Store) {
        let etc = dir.join("etc");
        let fonts = dir.join("fonts");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::create_dir_all(&fonts).unwrap();
        std::fs::write(etc.join("vtium.conf"), INSTALLED).unwrap();
        std::fs::write(fonts.join("mono.vtfrit"), "source=builtin:unifont\n").unwrap();
        let paths = crate::places::Paths {
            configdir: etc,
            frits: Ok(fonts),
            drafts: dir.join("bundles"),
            sudo: "sudo".into(),
            vtium: "vtium".into(),
            helper: crate::specimen::Helper::at(dir.join("no-helper")),
            sys: dir.join("sys"),
        };
        let store = crate::store::Store {
            config: dir.join("config"),
            state: dir.join("state"),
        };
        (paths, store)
    }

    #[test]
    fn the_real_desk_opens_the_draft_and_writes_each_change_into_the_store() {
        let dir = Scratch::new("document-desk");
        let (paths, store) = machine(&dir);
        let session = crate::drafts::Session {
            store: store.clone(),
            paths: &paths,
        };
        let document = OnDisk::open(&session).unwrap();
        let name = document.name().to_owned();
        assert_eq!(document.text(), INSTALLED);
        assert_eq!(document.content().base, Base::of(Some(INSTALLED)));
        assert!(document.installed().frits.contains_key("mono"));
        let mut docs = Documents::new(document, Box::new(OnDisk::new(store.clone())));
        let terminus = std::path::PathBuf::from("/usr/share/fonts/Terminus.otb");
        for (n, edit) in [
            Edit::Text(format!("{INSTALLED}mouse\n")),
            Edit::FontSet {
                frit: "mine".to_owned(),
                edit: FontSetEdit::Keep("source=builtin:unifont\n".to_owned()),
            },
            Edit::FontSet {
                frit: "mono".to_owned(),
                edit: FontSetEdit::Remove,
            },
            Edit::Active {
                path: terminus.clone(),
                change: Some(crate::batch::Change::Activate),
            },
            Edit::Base(Base::of(None)),
        ]
        .into_iter()
        .enumerate()
        {
            docs.change(Change::new(format!("step {n}"), edit)).unwrap();
            let mut on_disk = docs.desk().read(&name).unwrap().unwrap();
            // the store keeps a base's hash alone
            on_disk
                .base
                .text
                .clone_from(&docs.document().content().base.text);
            assert_eq!(&on_disk, docs.document().content(), "after step {n}");
        }
        assert_eq!(
            store.batch_changes(&name).unwrap().get(&terminus),
            Some(crate::batch::Change::Activate)
        );
        // undo writes the store back step by step: the base, the
        // activation, the removal, the font set added
        for n in (0..4).rev() {
            docs.undo().unwrap();
            let mut on_disk = docs.desk().read(&name).unwrap().unwrap();
            on_disk
                .base
                .text
                .clone_from(&docs.document().content().base.text);
            assert_eq!(&on_disk, docs.document().content(), "undone to step {n}");
        }
        let frits = store.frit_changes(&name).unwrap();
        assert!(
            frits.kept.is_empty() && frits.removed.is_empty(),
            "{frits:?}"
        );
        assert!(store.batch_changes(&name).unwrap().is_empty());
    }

    #[test]
    fn the_real_desk_refuses_a_base_it_knows_only_by_its_hash() {
        let dir = Scratch::new("document-desk-hash");
        let (paths, store) = machine(&dir);
        let session = crate::drafts::Session {
            store: store.clone(),
            paths: &paths,
        };
        let document = OnDisk::open(&session).unwrap();
        let before = document.content().clone();
        let mut docs = Documents::new(document, Box::new(OnDisk::new(store)));
        let hash_only = Base {
            hash: Some(crate::store::hash(b"another file\n")),
            text: None,
        };
        let refused = docs
            .change(Change::new("a base", Edit::Base(hash_only)))
            .unwrap_err();
        assert_eq!(refused, words::BASE_WITHOUT_TEXT);
        assert_eq!(docs.document().content(), &before);
    }
}
