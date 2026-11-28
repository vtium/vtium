//! Undo and redo over the one document: each step brings back vtium.conf's
//! text, the font set changes, the active fonts and the base, writes the
//! draft as every change does, and a Save keeps the history.

use std::collections::BTreeMap;
use std::path::PathBuf;

use vtium_config_core::batch;
use vtium_config_core::document::{
    Base, Change, Content, Desk, Document, Documents, Edit, FontSetEdit, InMemory, Installed, words,
};
use vtium_config_core::flowmodel::Present;
use vtium_config_core::ui::Shared;

const INSTALLED: &str = "version = 1\n[look My-Look]\nfont-size = 10\n";
const ELEVEN: &str = "version = 1\n[look My-Look]\nfont-size = 11\n";
const CHANGED_ON_DISK: &str = "version = 1\nosd=minimal\n[look My-Look]\nfont-size = 10\n";
const TERMINUS: &str = "/usr/share/fonts/Terminus.otb";
const MONO: &[u8] = b"source=builtin:unifont\n";

fn installed() -> Installed {
    Installed {
        text: Some(INSTALLED.to_owned()),
        frits: BTreeMap::from([("mono".to_owned(), MONO.to_vec())]),
    }
}

fn open() -> Documents {
    let content = Content {
        text: INSTALLED.to_owned(),
        base: Base::of(Some(INSTALLED)),
        ..Content::default()
    };
    let desk = InMemory::new().with_draft("padnoir", content.clone());
    Documents::new(
        Document::new("padnoir", content, installed()),
        Box::new(desk),
    )
}

/// One change of each kind: the text, a font set added, an installed font
/// set removed, a font activated, and the base the screen for a file
/// changed on disk records.
fn changes() -> Vec<Change> {
    vec![
        Change::new("font-size = 11 in My-Look", Edit::Text(ELEVEN.to_owned())),
        Change::new(
            "add the font set mine",
            Edit::FontSet {
                frit: "mine".to_owned(),
                edit: FontSetEdit::Keep("source=builtin:unifont\n# mine\n".to_owned()),
            },
        ),
        Change::new(
            "remove the font set mono",
            Edit::FontSet {
                frit: "mono".to_owned(),
                edit: FontSetEdit::Remove,
            },
        ),
        Change::new(
            "activate Terminus",
            Edit::Active {
                path: PathBuf::from(TERMINUS),
                change: Some(batch::Change::Activate),
            },
        ),
        Change::new(
            "Update from the file on disk",
            Edit::All(vec![
                Edit::Text(CHANGED_ON_DISK.replace("10", "11")),
                Edit::Base(Base::of(Some(CHANGED_ON_DISK))),
            ]),
        ),
    ]
}

fn drafted(docs: &Documents) -> Content {
    docs.desk().read("padnoir").expect("read").expect("there")
}

#[test]
fn undo_and_redo_bring_back_everything_a_step_keeps() {
    let mut docs = open();
    let mut seen = vec![docs.document().content().clone()];
    for change in changes() {
        docs.change(change).expect("made");
        seen.push(docs.document().content().clone());
    }
    // back to the start, one step at a time, the draft following each
    for (n, before) in seen.iter().enumerate().rev().skip(1) {
        assert!(
            docs.undo().expect("written").is_some(),
            "step {n} is undone"
        );
        assert_eq!(docs.document().content(), before, "after undoing to {n}");
        assert_eq!(&drafted(&docs), before, "the draft after undoing to {n}");
    }
    assert_eq!(docs.undo(), Ok(None), "nothing left to undo");
    // and forward again
    for (n, after) in seen.iter().enumerate().skip(1) {
        assert!(
            docs.redo().expect("written").is_some(),
            "step {n} is redone"
        );
        assert_eq!(docs.document().content(), after, "after redoing {n}");
        assert_eq!(&drafted(&docs), after, "the draft after redoing {n}");
    }
    assert_eq!(docs.redo(), Ok(None), "nothing left to redo");
}

#[test]
fn undo_brings_back_the_base_from_before_an_update() {
    let mut docs = open();
    let start = docs.document().content().base.clone();
    for change in changes() {
        docs.change(change).expect("made");
    }
    assert_eq!(
        docs.document().content().base,
        Base::of(Some(CHANGED_ON_DISK))
    );
    docs.undo().expect("undone");
    assert_eq!(docs.document().content().base, start);
}

#[test]
fn each_step_names_its_change() {
    let mut docs = open();
    for change in changes() {
        docs.change(change).expect("made");
    }
    assert_eq!(
        docs.history().to_undo().map(|s| s.name.as_str()),
        Some("Update from the file on disk")
    );
    assert_eq!(
        docs.undo().expect("undone").as_deref(),
        Some("Update from the file on disk")
    );
    assert_eq!(
        docs.history().to_redo().map(|s| s.name.as_str()),
        Some("Update from the file on disk")
    );
}

#[test]
fn a_new_change_after_an_undo_empties_the_redo_history() {
    let mut docs = open();
    let mut list = changes().into_iter();
    docs.change(list.next().expect("one")).expect("made");
    docs.change(list.next().expect("two")).expect("made");
    docs.undo().expect("undone");
    assert!(docs.history().to_redo().is_some());
    docs.change(list.next().expect("three")).expect("made");
    assert!(
        docs.history().to_redo().is_none(),
        "redo has nothing after a new change"
    );
    assert_eq!(docs.history().steps().len(), 2);
}

#[test]
fn undoing_every_step_brings_the_count_to_nothing() {
    let mut docs = open();
    docs.change(changes().remove(0)).expect("made");
    assert_eq!(docs.document().count(), 1);
    docs.undo().expect("undone");
    assert_eq!(docs.document().count(), 0);
    assert!(!docs.document().changed());
}

#[test]
fn a_save_keeps_the_history_and_undo_past_it_is_a_change_not_saved() {
    let mut docs = open();
    for change in changes().into_iter().take(4) {
        docs.change(change).expect("made");
    }
    assert_eq!(docs.document().count(), 4);
    // the save installs the text, mine, the removal of mono and Terminus
    let now = Installed {
        text: Some(ELEVEN.to_owned()),
        frits: BTreeMap::from([(
            "mine".to_owned(),
            b"source=builtin:unifont\n# mine\n".to_vec(),
        )]),
    };
    docs.saved(now);
    assert_eq!(docs.document().count(), 0, "nothing is left to save");
    assert_eq!(docs.history().made(), 4, "the save keeps the history");
    let saved_base = Base::of(Some(ELEVEN));
    for step in docs.history().steps() {
        assert_eq!(step.before.base, saved_base, "{} before", step.name);
        assert_eq!(step.after.base, saved_base, "{} after", step.name);
    }
    // undoing the activation past the save deactivates Terminus, which
    // the save made active
    docs.undo().expect("undone");
    assert_eq!(docs.document().count(), 1);
    let review = docs.document().review();
    assert_eq!(
        review.active,
        [(PathBuf::from(TERMINUS), batch::Change::Deactivate)]
    );
    // undoing the removal of mono brings mono back as a font set to add
    docs.undo().expect("undone");
    let review = docs.document().review();
    assert_eq!(review.font_sets.len(), 1, "{review:?}");
    assert_eq!(
        docs.document()
            .content()
            .font_sets
            .kept
            .get("mono")
            .map(String::as_bytes),
        Some(MONO)
    );
    // undoing everything is the document before the save, against the
    // file the save installed, whose base needs no screen for a file
    // changed on disk
    docs.undo().expect("undone");
    docs.undo().expect("undone");
    let content = docs.document().content();
    assert_eq!(content.text, INSTALLED);
    assert_eq!(content.base, saved_base);
    assert!(content.font_sets.removed.contains("mine"), "{content:?}");
    assert_eq!(docs.document().count(), 4);
    assert_eq!(&drafted(&docs), content);
}

#[test]
fn the_message_line_says_what_undo_and_redo_did() {
    let content = Content {
        text: INSTALLED.to_owned(),
        ..Content::default()
    };
    let desk = InMemory::new().with_draft("padnoir", content.clone());
    let documents = Documents::new(
        Document::new("padnoir", content, installed()),
        Box::new(desk),
    );
    let mut shared = Shared::new(documents, Present::default());
    let said = |shared: &Shared| shared.message.as_ref().map(|m| m.line.clone());
    shared.undo();
    assert_eq!(said(&shared).as_deref(), Some(words::NOTHING_TO_UNDO));
    shared.redo();
    assert_eq!(said(&shared).as_deref(), Some(words::NOTHING_TO_REDO));
    shared.change(changes().remove(0)).expect("made");
    shared.undo();
    assert_eq!(
        said(&shared).as_deref(),
        Some("Undid font-size = 11 in My-Look. [Ctrl+Y] redoes it.")
    );
    shared.redo();
    assert_eq!(
        said(&shared).as_deref(),
        Some("Redid font-size = 11 in My-Look. [Ctrl+Z] undoes it.")
    );
}

#[test]
fn an_undo_the_desk_cannot_write_leaves_the_document_and_the_history() {
    // a desk that refuses after the first write: the change is made, then
    // the undo cannot be written
    #[derive(Debug)]
    struct Once(usize);
    impl Desk for Once {
        fn write(&mut self, _: &str, _: &Content) -> Result<(), String> {
            self.0 += 1;
            if self.0 > 1 {
                Err("the disk is full".to_owned())
            } else {
                Ok(())
            }
        }
        fn read(&self, _: &str) -> Result<Option<Content>, String> {
            Ok(None)
        }
        fn configurations(&self) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }
        fn versions(&self) -> Result<Vec<vtium_config_core::document::Version>, String> {
            Ok(Vec::new())
        }
    }
    let content = Content {
        text: INSTALLED.to_owned(),
        ..Content::default()
    };
    let mut docs = Documents::new(
        Document::new("padnoir", content, installed()),
        Box::new(Once(0)),
    );
    docs.change(changes().remove(0)).expect("made");
    let after = docs.document().content().clone();
    assert_eq!(docs.undo(), Err("the disk is full".to_owned()));
    assert_eq!(docs.document().content(), &after);
    assert_eq!(docs.history().made(), 1, "the step is still there to undo");
}
