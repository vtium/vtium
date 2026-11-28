//! The configuration as the one open document of vtium-config's TUI. A
//! [`Document`] holds the draft's name, its vtium.conf text, its font set
//! changes, its changes to the active resources and its base, the
//! installed file it was made from. Every change to it passes
//! [`Documents::change`], which applies the change, writes the draft
//! through a [`Desk`] and records the change as one step of the undo
//! history ([`undo`]). So the draft on disk always holds what the screen
//! shows, and quitting loses nothing.
//!
//! The count of changes not saved is the number of lines of vtium.conf
//! that differ from the installed file, plus each font set and each active
//! resource the document changes. It is the count Save's review shows
//! ([`Review`]), so it falls to nothing after a save.
//!
//! References:
//! - ADR-1403 D11, "One flow for every change": the configuration is the one open document, every change edits it and writes the draft, and the count not saved is Save's count.
//! - ADR-1403 D8, "The draft and the installed file": the draft, its base, its font set folder and its active resources.
//! - ADR-1206 D3, "The undo history lives in `shared`": every change passes one place, which records it as one step.

pub mod conf;
pub mod desk;
pub mod undo;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use conf::ConfEdit;
pub use desk::{Desk, InMemory, OnDisk, Version};
pub use undo::{History, Step};

use crate::batch::{self, Changes};
use crate::drafts::FritEdit;
use crate::store::FritChanges;

/// The installed vtium.conf a draft was made from or saved as.
///
/// References:
/// - ADR-1403 D8, "When the installed file changed on disk": the draft keeps the base's text beside its hash, so Update can merge a change made on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Base {
    /// The SHA-256 of the installed file, `None` when no file was
    /// installed.
    pub hash: Option<String>,
    /// The installed file's text, where the draft keeps it. A draft made
    /// before vtium-config kept the text has only the hash.
    pub text: Option<String>,
}

impl Base {
    /// The base of a draft made from `installed`, the installed file's
    /// text or `None` when there is none: its hash and its text.
    pub fn of(installed: Option<&str>) -> Base {
        Base {
            hash: installed.map(|t| crate::store::hash(t.as_bytes())),
            text: installed.map(str::to_owned),
        }
    }
}

/// What is installed now, which the document counts its changes against:
/// the installed vtium.conf and the installed font sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installed {
    /// The installed vtium.conf's text, `None` when none is installed.
    pub text: Option<String>,
    /// Each font set installed in the font sets' directory, by its name,
    /// as its bytes.
    pub frits: BTreeMap<String, Vec<u8>>,
}

/// What a document holds at one moment, which each step of the undo
/// history keeps before and after its change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Content {
    /// vtium.conf's text.
    pub text: String,
    /// The font sets the draft adds, replaces and removes, as its folder
    /// `NAME.frits/` keeps them.
    pub font_sets: FritChanges,
    /// The draft's changes to the active resources, the active fonts among
    /// them, as `NAME.batch` keeps them.
    pub active: Changes,
    /// The installed file the draft was made from or saved as.
    pub base: Base,
}

/// A change to one font set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontSetEdit {
    /// The document adds or replaces the font set, with this text.
    Keep(String),
    /// The document removes the installed font set.
    Remove,
    /// The document no longer changes the font set.
    Forget,
}

/// What one change does to the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Makes these edits to vtium.conf, in order, through the operations
    /// that the `vtium config` commands call.
    Conf(Vec<ConfEdit>),
    /// Puts this text in place of vtium.conf's: Edit as text, a version
    /// from History, or Update's merge.
    Text(String),
    /// Changes the font set `frit`.
    FontSet { frit: String, edit: FontSetEdit },
    /// Makes `change` the document's change to the resource at `path`, or
    /// takes its change back with `None`. The tab that asks has already
    /// found that the change changes something: an activation of a
    /// resource that is active is no change.
    Active {
        path: PathBuf,
        change: Option<batch::Change>,
    },
    /// Drops every font set change and every change to the active
    /// resources, as Revert to saved does.
    Drop,
    /// Records this installed file as the base, as Update, Overwrite and
    /// Revert do on the screen for a file changed on disk.
    Base(Base),
    /// Makes these edits in order, as one change.
    All(Vec<Edit>),
}

/// One change to the document: what it does, and its name, which the
/// message line and the File menu show, as `font-size = 11 in My-Look`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub name: String,
    pub edit: Edit,
}

impl Change {
    pub fn new(name: impl Into<String>, edit: Edit) -> Change {
        Change {
            name: name.into(),
            edit,
        }
    }
}

/// What saving the document changes, as Save's review lists it: the lines
/// of vtium.conf that differ from the installed file, then the font sets
/// and the active resources the document changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Review {
    /// The lines of vtium.conf that differ, a line changed in place
    /// counted once.
    pub lines: Vec<crate::draft::Change>,
    /// Each font set the save adds, replaces or removes, in the order of
    /// their names.
    pub font_sets: Vec<(String, FritEdit)>,
    /// Each resource the save activates or deactivates, in the order the
    /// changes were made.
    pub active: Vec<(PathBuf, batch::Change)>,
}

impl Review {
    /// How many changes saving makes: the count not saved.
    pub fn count(&self) -> usize {
        self.lines
            .len()
            .saturating_add(self.font_sets.len())
            .saturating_add(self.active.len())
    }
}

/// The open configuration: the draft's name, what it holds, and what is
/// installed now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    name: String,
    content: Content,
    installed: Installed,
    /// The count not saved, worked out again after each change, so the
    /// top row reads it without comparing the files on every frame.
    count: usize,
}

impl Document {
    /// The draft `name`, holding `content`, against `installed`.
    pub fn new(name: impl Into<String>, content: Content, installed: Installed) -> Document {
        let mut document = Document {
            name: name.into(),
            content,
            installed,
            count: 0,
        };
        document.count = document.review().count();
        document
    }

    /// The draft's name, which the top row shows as the document's.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn content(&self) -> &Content {
        &self.content
    }

    /// vtium.conf's text.
    pub fn text(&self) -> &str {
        &self.content.text
    }

    pub fn installed(&self) -> &Installed {
        &self.installed
    }

    /// The count not saved, which the top row shows beside ●.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Whether the document holds a change that is not saved, which ●
    /// marks.
    pub fn changed(&self) -> bool {
        self.count > 0
    }

    /// What saving the document changes against the installed file.
    ///
    /// References:
    /// - ADR-1403 D8, "A draft's font set changes": a font set with the installed text is no change, and a removal of a font set that is not installed removes nothing.
    pub fn review(&self) -> Review {
        let installed = self.installed.text.as_deref().unwrap_or_default();
        let (font_sets, _) = crate::drafts::frit_changes(
            &self.name,
            &self.content.font_sets,
            Path::new(""),
            |path: &Path| {
                let name = path.file_stem()?.to_str()?;
                self.installed.frits.get(name).cloned()
            },
        );
        Review {
            lines: crate::draft::changes(installed, &self.content.text),
            font_sets: font_sets
                .iter()
                .map(|c| (c.name.clone(), c.edit()))
                .collect(),
            active: self
                .content
                .active
                .iter()
                .map(|(p, c)| (p.to_path_buf(), c))
                .collect(),
        }
    }

    /// Puts `content` in place, and counts it again.
    fn set(&mut self, content: Content) {
        self.content = content;
        self.count = self.review().count();
    }
}

impl Content {
    /// This content with `edit` made, or why the edit cannot be made.
    /// `installed` says which font sets a removal can remove.
    pub fn edited(&self, edit: &Edit, installed: &Installed) -> Result<Content, String> {
        let mut out = self.clone();
        out.edit(edit, installed)?;
        Ok(out)
    }

    fn edit(&mut self, edit: &Edit, installed: &Installed) -> Result<(), String> {
        match edit {
            Edit::Conf(edits) => {
                self.text = conf::apply_all(&self.text, edits).map_err(|f| f.text().to_owned())?;
            }
            Edit::Text(text) => self.text.clone_from(text),
            Edit::FontSet { frit, edit } => {
                font_set_name(frit)?;
                let sets = &mut self.font_sets;
                match edit {
                    FontSetEdit::Keep(text) => {
                        sets.removed.remove(frit);
                        sets.kept.insert(frit.clone(), text.clone());
                    }
                    FontSetEdit::Remove => {
                        if !installed.frits.contains_key(frit) && !sets.kept.contains_key(frit) {
                            return Err(words::nothing_to_remove(frit));
                        }
                        sets.kept.remove(frit);
                        sets.removed.insert(frit.clone());
                    }
                    FontSetEdit::Forget => {
                        sets.kept.remove(frit);
                        sets.removed.remove(frit);
                    }
                }
            }
            Edit::Active { path, change } => self.active.set(path.clone(), *change),
            Edit::Drop => {
                self.font_sets.kept.clear();
                self.font_sets.removed.clear();
                self.active = Changes::default();
            }
            Edit::Base(base) => self.base.clone_from(base),
            Edit::All(edits) => {
                for e in edits {
                    self.edit(e, installed)?;
                }
            }
        }
        Ok(())
    }
}

/// `frit` when a font set can have that name and no built-in font has it,
/// as the store refuses any other.
fn font_set_name(frit: &str) -> Result<(), String> {
    if crate::frit::RESERVED.contains(&frit) {
        return Err(words::built_in(frit));
    }
    if !crate::frit::valid_name(frit) {
        return Err(words::bad_name(frit));
    }
    Ok(())
}

/// What a change did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    /// The change made the document what it already was, so it wrote
    /// nothing and is no step of the history.
    Same,
    /// The change is made, written and recorded.
    Made,
}

/// The documents open while vtium-config runs: the open one, its undo
/// history, the histories of the others opened before it, and the desk
/// that every draft is written through.
///
/// References:
/// - ADR-1403 D11, "Undo and redo": each document under File › Open keeps its own history while vtium-config runs, and New and Open switch between documents without a step of their own.
#[derive(Debug)]
pub struct Documents {
    open: Document,
    history: History,
    /// The history of each document opened before, by its name.
    kept: BTreeMap<String, History>,
    desk: Box<dyn Desk>,
}

impl Documents {
    /// `open`, with an empty history, written through `desk`.
    pub fn new(open: Document, desk: Box<dyn Desk>) -> Documents {
        Documents {
            open,
            history: History::default(),
            kept: BTreeMap::new(),
            desk,
        }
    }

    /// The open document.
    pub fn document(&self) -> &Document {
        &self.open
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn desk(&self) -> &dyn Desk {
        self.desk.as_ref()
    }

    /// Makes `change` on the open document. This is the one place a
    /// change to the document passes: it makes the change on a copy,
    /// writes the copy as the draft, and only then puts the copy in place
    /// and records the change as one step. So a change the desk cannot
    /// write leaves the document as it was, and the draft on disk always
    /// holds what the screen shows. A change that changes nothing writes
    /// nothing and is no step.
    pub fn change(&mut self, change: Change) -> Result<Done, String> {
        let before = self.open.content.clone();
        let after = before.edited(&change.edit, &self.open.installed)?;
        if after == before {
            return Ok(Done::Same);
        }
        self.desk.write(&self.open.name, &after)?;
        self.open.set(after.clone());
        self.history.record(Step {
            name: change.name,
            before,
            after,
        });
        Ok(Done::Made)
    }

    /// Takes the last step back, writing the draft as every change does,
    /// and returns the step's name; `None` with nothing to undo.
    pub fn undo(&mut self) -> Result<Option<String>, String> {
        let Some(step) = self.history.to_undo() else {
            return Ok(None);
        };
        let (name, content) = (step.name.clone(), step.before.clone());
        self.desk.write(&self.open.name, &content)?;
        self.open.set(content);
        self.history.back();
        Ok(Some(name))
    }

    /// Makes again the step the last undo took back, and returns its name;
    /// `None` with nothing to redo.
    pub fn redo(&mut self) -> Result<Option<String>, String> {
        let Some(step) = self.history.to_redo() else {
            return Ok(None);
        };
        let (name, content) = (step.name.clone(), step.after.clone());
        self.desk.write(&self.open.name, &content)?;
        self.open.set(content);
        self.history.forward();
        Ok(Some(name))
    }

    /// Records a save that installed the open document, after which
    /// `installed` is what the system holds. The history stays, and every
    /// step's base becomes the file the save installed, so undoing past
    /// the save makes an ordinary change, which the next save writes
    /// without the screen for a file changed on disk. The font set changes
    /// and the changes to the active resources that each step keeps are
    /// counted again against what the save installed. The privileged step
    /// has already written the draft as it stands after the install, so
    /// this writes nothing.
    ///
    /// References:
    /// - ADR-1403 D11, "Undo and redo": undo goes on past a Save, and what it brings back is then a change not saved.
    /// - ADR-1206 D3, "The undo history lives in `shared`": a Save keeps the history and moves the base that every step keeps.
    pub fn saved(&mut self, installed: Installed) {
        let base = Base::of(installed.text.as_deref());
        let was = self.open.installed.clone();
        let saved = self.open.content.active.clone();
        let rebase = |content: &mut Content| {
            undo::rebase(content, &saved, &was, &installed);
            content.base.clone_from(&base);
        };
        self.history.rebase(rebase);
        let mut content = self.open.content.clone();
        rebase(&mut content);
        self.open.installed = installed;
        self.open.set(content);
    }

    /// Puts `installed`, what is installed now as read again, in place of
    /// what the open document counts its changes against, and counts the
    /// document again. The screen for a file changed on disk reads the
    /// installed file again, so after Update, Overwrite or Revert the count
    /// not saved follows the file that screen showed. A refresh is no step
    /// of the history and writes nothing, since the draft does not change.
    ///
    /// References:
    /// - ADR-1403 D11, "One flow for every change": the count not saved is Save's count, against what is installed now.
    /// - ADR-1403 D8, "When the installed file changed on disk": the review shows the installed file's own change, and Update merges it into the document.
    pub fn refresh(&mut self, installed: Installed) {
        self.open.installed = installed;
        self.open.count = self.open.review().count();
    }

    /// Opens `document` in place of the open one, keeping the open one's
    /// history under its name and taking up the history `document` had,
    /// if it was open before. Switching is no step of either history.
    pub fn switch(&mut self, document: Document) {
        let old = std::mem::replace(&mut self.open, document);
        let history = self.kept.remove(&self.open.name).unwrap_or_default();
        let old_history = std::mem::replace(&mut self.history, history);
        self.kept.insert(old.name, old_history);
    }
}

/// The words of the document's refusals and of its undo and redo, which
/// the message line shows.
pub mod words {
    /// Why a font set cannot take a built-in font's name.
    pub fn built_in(frit: &str) -> String {
        format!("{frit} is a built-in font, not a font set.")
    }

    /// Why a font set cannot have the name `frit`.
    pub fn bad_name(frit: &str) -> String {
        format!(
            "A font set cannot be named {frit:?}. A font set's name uses letters, digits, ., _, + and -."
        )
    }

    /// Why the font set `frit` cannot be removed.
    pub fn nothing_to_remove(frit: &str) -> String {
        format!("No font set named {frit} is installed or added, so there is nothing to remove.")
    }

    /// What Ctrl+Z did.
    pub fn undid(step: &str) -> String {
        format!("Undid {step}. [Ctrl+Y] redoes it.")
    }

    /// What Ctrl+Y did.
    pub fn redid(step: &str) -> String {
        format!("Redid {step}. [Ctrl+Z] undoes it.")
    }

    /// Ctrl+Z with nothing to undo.
    pub const NOTHING_TO_UNDO: &str = "Nothing to undo.";

    /// Ctrl+Y with nothing to redo.
    pub const NOTHING_TO_REDO: &str = "Nothing to redo.";
}
