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

    /// Renames the draft `from` to `to`, with everything kept beside it.
    /// It refuses a name another draft has and a draft that does not
    /// exist. A desk that cannot rename refuses every rename.
    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        let _ = (from, to);
        Err(words::NO_RENAME.to_owned())
    }

    /// Deletes the draft `name`, with everything kept beside it. Nothing
    /// installed changes. It refuses a draft that does not exist. A desk
    /// that cannot delete refuses every delete.
    fn remove(&mut self, name: &str) -> Result<(), String> {
        let _ = name;
        Err(words::NO_REMOVE.to_owned())
    }
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

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        crate::store::named(to).map_err(|f| f.text().to_owned())?;
        if self.drafts.contains_key(to) {
            return Err(words::taken(to));
        }
        let content = self
            .drafts
            .remove(from)
            .ok_or_else(|| words::no_such(from))?;
        self.drafts.insert(to.to_owned(), content);
        Ok(())
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        self.drafts
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| words::no_such(name))
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
    /// its changes to the active resources, and its base with the text the
    /// store keeps of it, so a draft whose installed file changed on disk
    /// opens with the text Update merges from. Also what is installed now:
    /// vtium.conf and the font sets of the font sets' directory.
    ///
    /// References:
    /// - ADR-1403 D8, "When the installed file changed on disk": the draft keeps the base's text beside its hash, so Update can merge a change made on disk.
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
                    text: if same {
                        installed.clone()
                    } else {
                        store.base_text(&name)?
                    },
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

    /// The draft's files as the store holds them, the base's text with
    /// them where the store keeps it.
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
                text: store.base_text(name).map_err(fail)?,
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

    /// Renames the draft with its font set folder, its changes to the
    /// active resources and its base, as `vtium config draft rename` does,
    /// and keeps it current when it was.
    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        let store = &self.store;
        let fail = |f: Fail| f.text().to_owned();
        crate::store::named(to).map_err(fail)?;
        if store.read_draft(to).map_err(fail)?.is_some() {
            return Err(words::taken(to));
        }
        if store.read_draft(from).map_err(fail)?.is_none() {
            return Err(words::no_such(from));
        }
        store.rename_draft(from, to).map_err(fail)
    }

    /// Deletes the draft with everything beside it, as `vtium config draft
    /// rm` does, and leaves no draft current when it was. Nothing under
    /// /etc changes.
    fn remove(&mut self, name: &str) -> Result<(), String> {
        let store = &self.store;
        let fail = |f: Fail| f.text().to_owned();
        if store.read_draft(name).map_err(fail)?.is_none() {
            return Err(words::no_such(name));
        }
        store.remove_draft(name).map_err(fail)?;
        if store.settings().map_err(fail)?.draft.as_deref() == Some(name) {
            store.set_current(None).map_err(fail)?;
        }
        Ok(())
    }
}

/// Renaming and deleting the configurations kept, which File › Open ›'s
/// Manage configurations… does. They live here, beside the desk they go
/// through.
///
/// References:
/// - ADR-1403 D11, "Managing configurations": Rename refuses a name in use, and the open configuration cannot be deleted.
impl super::Documents {
    /// Renames the configuration `from` to `to` on the desk. When `from`
    /// is the open one, the open document takes the new name, and a
    /// configuration opened before keeps its undo history under the new
    /// name. It refuses a name another configuration has, open or kept.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        if from == to {
            return Ok(());
        }
        if to == self.open.name || self.kept.contains_key(to) {
            return Err(words::taken(to));
        }
        let open = from == self.open.name;
        if open && self.desk.read(from)?.is_none() {
            // the open document's draft is not written yet, as after
            // New… from what is saved, so only its name changes
            crate::store::named(to).map_err(|f| f.text().to_owned())?;
            if self.desk.read(to)?.is_some() {
                return Err(words::taken(to));
            }
        } else {
            self.desk.rename(from, to)?;
        }
        if open {
            to.clone_into(&mut self.open.name);
        } else if let Some(history) = self.kept.remove(from) {
            self.kept.insert(to.to_owned(), history);
        }
        Ok(())
    }

    /// Deletes the configuration `name` from the desk, with the undo
    /// history it kept. It refuses the open one, so a configuration is
    /// always open.
    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        if name == self.open.name {
            return Err(words::open_now(name));
        }
        self.desk.remove(name)?;
        self.kept.remove(name);
        Ok(())
    }
}

/// The desk's own words.
pub mod words {
    /// Why a desk that cannot rename a draft refuses.
    pub const NO_RENAME: &str = "This desk cannot rename a configuration.";

    /// Why a desk that cannot delete a draft refuses.
    pub const NO_REMOVE: &str = "This desk cannot delete a configuration.";

    /// Why a configuration that does not exist cannot be renamed or
    /// deleted.
    pub fn no_such(name: &str) -> String {
        format!("There is no configuration named {name}.")
    }

    /// Why a configuration cannot take a name another one has.
    pub fn taken(name: &str) -> String {
        format!("{name} is taken: another configuration has that name.")
    }

    /// Why the open configuration cannot be deleted.
    pub fn open_now(name: &str) -> String {
        format!("{name} is open, so it cannot be deleted; open another first.")
    }

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
            let on_disk = docs.desk().read(&name).unwrap().unwrap();
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
            let on_disk = docs.desk().read(&name).unwrap().unwrap();
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

    /// The documents over a desk in memory: north open with one change,
    /// and the drafts spare and kiosk kept.
    fn three() -> Documents {
        let content = |text: &str| Content {
            text: text.to_owned(),
            base: Base::of(Some(INSTALLED)),
            ..Content::default()
        };
        let desk = InMemory::new()
            .with_draft("north", content(INSTALLED))
            .with_draft("spare", content("# spare\n"))
            .with_draft("kiosk", content("# kiosk\n"));
        let installed = Installed {
            text: Some(INSTALLED.to_owned()),
            frits: BTreeMap::new(),
        };
        let open = Document::new("north", content(INSTALLED), installed);
        let mut docs = Documents::new(open, Box::new(desk));
        docs.change(Change::new(
            "a line",
            Edit::Text(format!("{INSTALLED}mouse\n")),
        ))
        .unwrap();
        docs
    }

    #[test]
    fn a_rename_moves_the_draft_and_its_history_and_refuses_a_name_in_use() {
        let mut docs = three();
        // spare gets a step of its own while it is open, then north again
        let spare = docs.desk().read("spare").unwrap().unwrap();
        let installed = docs.document().installed().clone();
        let north = docs.document().clone();
        docs.switch(Document::new("spare", spare.clone(), installed.clone()));
        docs.change(Change::new("spare's", Edit::Text("# spare 2\n".to_owned())))
            .unwrap();
        let spare = docs.document().content().clone();
        docs.switch(north);

        docs.rename("spare", "travel").unwrap();
        let names = docs.desk().configurations().unwrap();
        assert_eq!(names, ["kiosk", "north", "travel"]);
        assert_eq!(docs.desk().read("travel").unwrap(), Some(spare.clone()));
        docs.switch(Document::new("travel", spare, installed));
        assert_eq!(
            docs.history().to_undo().map(|s| s.name.as_str()),
            Some("spare's")
        );

        // travel is open now, north was opened before, and kiosk is only
        // on the desk
        for (from, to) in [("kiosk", "travel"), ("kiosk", "north"), ("north", "kiosk")] {
            assert_eq!(
                docs.rename(from, to),
                Err(words::taken(to)),
                "{from} to {to}"
            );
        }
        docs.rename("kiosk", "kiosk").unwrap();
        assert_eq!(
            docs.rename("nosuch", "other"),
            Err(words::no_such("nosuch"))
        );
        assert_eq!(docs.desk().configurations().unwrap(), names);
    }

    #[test]
    fn a_rename_of_the_open_configuration_renames_the_document_too() {
        let mut docs = three();
        docs.rename("north", "desk").unwrap();
        assert_eq!(docs.document().name(), "desk");
        assert_eq!(
            docs.desk().configurations().unwrap(),
            ["desk", "kiosk", "spare"]
        );
        // the next change writes the draft under its new name, and the
        // history goes on
        docs.change(Change::new("another", Edit::Text("# desk\n".to_owned())))
            .unwrap();
        assert_eq!(docs.desk().read("desk").unwrap().unwrap().text, "# desk\n");
        assert_eq!(docs.desk().read("north").unwrap(), None);
        assert_eq!(docs.undo().unwrap().as_deref(), Some("another"));
        assert_eq!(docs.undo().unwrap().as_deref(), Some("a line"));
    }

    #[test]
    fn a_delete_removes_a_kept_configuration_and_never_the_open_one() {
        let mut docs = three();
        assert_eq!(docs.remove("north"), Err(words::open_now("north")));
        assert_eq!(docs.document().name(), "north");
        docs.remove("kiosk").unwrap();
        assert_eq!(docs.desk().configurations().unwrap(), ["north", "spare"]);
        assert_eq!(docs.desk().read("kiosk").unwrap(), None);
        assert_eq!(docs.remove("kiosk"), Err(words::no_such("kiosk")));
        assert_eq!(docs.document().count(), 1, "the open one keeps its change");
    }

    #[test]
    fn the_real_desk_renames_and_deletes_in_the_store_and_leaves_etc_alone() {
        let dir = Scratch::new("document-desk-rename");
        let (paths, store) = machine(&dir);
        let session = crate::drafts::Session {
            store: store.clone(),
            paths: &paths,
        };
        let document = OnDisk::open(&session).unwrap();
        let name = document.name().to_owned();
        store.set_current(Some(&name)).unwrap();
        let mut docs = Documents::new(document, Box::new(OnDisk::new(store.clone())));
        docs.change(Change::new(
            "a font set",
            Edit::FontSet {
                frit: "mine".to_owned(),
                edit: FontSetEdit::Keep("source=builtin:unifont\n".to_owned()),
            },
        ))
        .unwrap();
        let spare = Content {
            text: "# spare\n".to_owned(),
            ..Content::default()
        };
        let mut desk = OnDisk::new(store.clone());
        desk.write("spare", &spare).unwrap();

        assert_eq!(docs.rename(&name, "spare"), Err(words::taken("spare")));
        docs.rename(&name, "laptop").unwrap();
        assert_eq!(docs.document().name(), "laptop");
        assert_eq!(store.read_draft(&name).unwrap(), None);
        let moved = docs.desk().read("laptop").unwrap().unwrap();
        assert_eq!(&moved, docs.document().content(), "the font set moved too");
        assert_eq!(store.settings().unwrap().draft.as_deref(), Some("laptop"));

        assert_eq!(docs.remove("laptop"), Err(words::open_now("laptop")));
        store.set_current(Some("spare")).unwrap();
        docs.remove("spare").unwrap();
        assert_eq!(store.read_draft("spare").unwrap(), None);
        assert_eq!(
            store.settings().unwrap().draft,
            None,
            "it is no longer current"
        );
        assert_eq!(docs.remove("spare"), Err(words::no_such("spare")));
        assert_eq!(docs.desk().configurations().unwrap(), ["laptop"]);
        let etc = std::fs::read_to_string(paths.configdir.join("vtium.conf")).unwrap();
        assert_eq!(etc, INSTALLED, "/etc is untouched");
    }
}
