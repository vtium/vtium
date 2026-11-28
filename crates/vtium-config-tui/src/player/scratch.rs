//! The scratch copy a macro plays on unless its run is live. The scratch
//! copy is a copy of the open document and of the drafts, in a temporary
//! folder that belongs to the run and goes when the run ends. The
//! document's changes go to the drafts there, through vtium-config's own
//! store, so nothing of the person's changes.
//!
//! [`Scratch`] is the machine the loop acts on during such a run. It reads
//! the devices of the real machine, and runs the person's editor for Edit
//! as text as the real machine does. Save names the command it would run
//! and the files that command would change, and counts the document as
//! saved, so `Expect Saved` holds after a Save. It changes nothing under
//! `/etc`. Install as a system font names what it would do and acts not
//! at all.
//!
//! References:
//! - ADR-1208 D5, "A macro plays on a scratch copy by default": the copy
//!   of the document and the drafts in a temporary folder, Save and the
//!   Actions saying what they would do, the document marked saved.

use std::path::{Path, PathBuf};

use vtium_config_core::document::{Content, Desk, Documents, Installed, OnDisk, Version};
use vtium_config_core::draft::Plan;
use vtium_config_core::flowmodel::Present;
use vtium_config_core::store::Store;
use vtium_config_core::ui::Shared;

use super::words;
use crate::live::machine::{Machine, Ready};
use crate::save::Saved;
use crate::save::privileged::TARGET;

/// The temporary folder of a scratch copy, which goes when it is dropped,
/// and the store of the drafts in it.
#[derive(Debug)]
pub struct Folder {
    dir: PathBuf,
    store: Store,
}

impl Folder {
    /// The folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The store of the drafts in the folder.
    pub fn store(&self) -> &Store {
        &self.store
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The desk of a scratch copy: the drafts in the scratch folder's store,
/// and the installed history as the real desk listed it when the copy was
/// made.
#[derive(Debug)]
struct ScratchDesk {
    drafts: OnDisk,
    versions: Vec<Version>,
}

impl Desk for ScratchDesk {
    fn write(&mut self, name: &str, content: &Content) -> Result<(), String> {
        self.drafts.write(name, content)
    }

    fn read(&self, name: &str) -> Result<Option<Content>, String> {
        self.drafts.read(name)
    }

    fn configurations(&self) -> Result<Vec<String>, String> {
        self.drafts.configurations()
    }

    fn versions(&self) -> Result<Vec<Version>, String> {
        Ok(self.versions.clone())
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.drafts.rename(from, to)
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        self.drafts.remove(name)
    }
}

/// Puts a scratch copy in place of `shared`'s document and drafts: each
/// draft the desk keeps, and the open document, are written to a new
/// folder under `parent`, and the document's changes go there from now
/// on. The folder goes when the returned [`Folder`] is dropped.
pub fn copy(shared: &mut Shared, parent: &Path) -> Result<Folder, String> {
    let dir = fresh(parent)?;
    let store = Store {
        config: dir.join("config"),
        state: dir.join("state"),
    };
    let folder = Folder {
        dir,
        store: store.clone(),
    };
    for sub in [&store.config, &store.state] {
        std::fs::create_dir_all(sub).map_err(|e| words::cannot_copy(sub, &e))?;
    }
    let real = shared.documents.desk();
    let mut desk = ScratchDesk {
        drafts: OnDisk::new(store),
        versions: real.versions().unwrap_or_default(),
    };
    for name in real.configurations()? {
        if let Some(content) = real.read(&name)? {
            desk.write(&name, &content)?;
        }
    }
    let document = shared.document().clone();
    desk.write(document.name(), document.content())?;
    shared.documents = Documents::new(document, Box::new(desk));
    Ok(folder)
}

/// A folder of this run's own under `parent`, `vtium-config-macro-PID-N`,
/// readable by its owner alone.
fn fresh(parent: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    for n in 0u32..1000 {
        let dir = parent.join(format!("vtium-config-macro-{}-{n}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(words::cannot_copy(&dir, &e)),
        }
    }
    Err(words::cannot_copy(
        parent,
        &std::io::Error::other("every name is taken"),
    ))
}

/// The machine a scratch copy's run acts on: `real` for what only reads,
/// and nothing for what would change the machine.
pub struct Scratch<'a> {
    real: &'a mut dyn Machine,
    /// The drafts of the scratch copy, which Save reads.
    drafts: OnDisk,
    /// What the scratch copy counts as installed: what was installed when
    /// the run began, then what each Save would have installed.
    installed: Installed,
    /// The words of the last command Save would have run, which a test
    /// reads.
    shown: Vec<String>,
}

impl<'a> Scratch<'a> {
    /// The scratch machine over `real`, for the scratch copy in `folder`,
    /// with `installed`, what the open document counts against when the
    /// run begins.
    pub fn new(real: &'a mut dyn Machine, folder: &Folder, installed: Installed) -> Scratch<'a> {
        Scratch {
            real,
            drafts: OnDisk::new(folder.store.clone()),
            installed,
            shown: Vec::new(),
        }
    }

    /// What each Save said it would do, in order.
    pub fn shown(&self) -> &[String] {
        &self.shown
    }
}

impl Machine for Scratch<'_> {
    fn devices(&mut self) -> Option<Present> {
        self.real.devices()
    }

    fn ready_font(&mut self, source: &Path) -> Result<Ready, String> {
        Err(words::scratch_font(source))
    }

    fn install_font(&mut self) -> Result<(), String> {
        Err(words::SCRATCH_INSTALL.to_owned())
    }

    fn installed(&mut self) -> Result<Installed, String> {
        Ok(self.installed.clone())
    }

    /// Names what saving `name` would run and change, and counts the
    /// draft as installed, without running anything.
    fn save(&mut self, name: &str, _plan: &Plan) -> Result<Saved, String> {
        let content = self
            .drafts
            .read(name)?
            .ok_or_else(|| words::scratch_no_draft(name))?;
        let after = would_install(&self.installed, &content);
        let text = said(name, &self.installed, &after);
        self.shown.push(text.clone());
        self.installed = after.clone();
        Ok(Saved {
            installed: after,
            said: text,
        })
    }

    fn edit(&mut self, name: &str, text: &str) -> Result<crate::editor::Edited, String> {
        self.real.edit(name, text)
    }
}

/// What saving `content` on a scratch copy would install: its vtium.conf,
/// and the installed font sets with its own font set changes made.
pub fn would_install(installed: &Installed, content: &Content) -> Installed {
    let mut frits = installed.frits.clone();
    for (name, text) in &content.font_sets.kept {
        frits.insert(name.clone(), text.clone().into_bytes());
    }
    for name in &content.font_sets.removed {
        frits.remove(name);
    }
    Installed {
        text: Some(content.text.clone()),
        frits,
    }
}

/// What a Save on a scratch copy says: the command it would run and the
/// files that command would change.
pub fn said(name: &str, before: &Installed, after: &Installed) -> String {
    let mut files = vec![TARGET.to_owned()];
    for (frit, bytes) in &after.frits {
        if before.frits.get(frit) != Some(bytes) {
            files.push(format!("the font set {frit}"));
        }
    }
    for frit in before.frits.keys() {
        if !after.frits.contains_key(frit) {
            files.push(format!("the font set {frit}, removed"));
        }
    }
    words::scratch_saved(&words::save_command(name), &files)
}
