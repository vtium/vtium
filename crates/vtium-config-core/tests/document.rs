//! The one open document: every change passes one function, which writes
//! the draft at once, and the count not saved is the count Save's review
//! shows. Also the two fixtures the TUI's crates test their screens on.

use std::collections::BTreeMap;
use std::path::PathBuf;

use vtium_config_core::batch;
use vtium_config_core::document::{
    Base, Change, ConfEdit, Content, Document, Documents, Done, Edit, FontSetEdit, InMemory,
    Installed,
};

/// The installed vtium.conf of the book's machine: the look My-Look at
/// 10 pt.
const INSTALLED: &str = "font-size=12\nversion = 1\n[look My-Look]\npalette = vga\nfont-size = 10\nfont-name = cullet-mono\n";

/// A document open on [`INSTALLED`], with no change, over a desk in
/// memory that holds its draft, and the installed font set `mono`.
fn open() -> Documents {
    let content = Content {
        text: INSTALLED.to_owned(),
        base: Base::of(Some(INSTALLED)),
        ..Content::default()
    };
    let desk = InMemory::new().with_draft("padnoir", content.clone());
    let installed = Installed {
        text: Some(INSTALLED.to_owned()),
        frits: BTreeMap::from([("mono".to_owned(), b"source=builtin:unifont\n".to_vec())]),
    };
    Documents::new(Document::new("padnoir", content, installed), Box::new(desk))
}

/// The change of the book's chapter 2: My-Look's size from 10 pt to 11.
fn eleven() -> Change {
    Change::new(
        "font-size = 11 in My-Look",
        Edit::Conf(vec![ConfEdit::LookKey {
            name: "My-Look".to_owned(),
            key: "font-size",
            value: Some("11".to_owned()),
        }]),
    )
}

/// What the stand-in desk holds as the draft.
fn drafted(docs: &Documents) -> Content {
    docs.desk()
        .read("padnoir")
        .expect("the stand-in desk reads")
        .expect("the draft is there")
}

#[test]
fn a_change_writes_the_draft_at_once() {
    let mut docs = open();
    assert_eq!(docs.change(eleven()), Ok(Done::Made));
    assert!(docs.document().text().contains("font-size = 11"));
    assert_eq!(
        &drafted(&docs),
        docs.document().content(),
        "the stand-in desk holds the change as soon as it is made"
    );
    assert_eq!(docs.history().made(), 1, "the change is one step");
}

#[test]
fn every_kind_of_change_reaches_the_draft() {
    let mut docs = open();
    let edits = [
        Edit::FontSet {
            frit: "mine".to_owned(),
            edit: FontSetEdit::Keep("source=builtin:unifont\n".to_owned()),
        },
        Edit::FontSet {
            frit: "mono".to_owned(),
            edit: FontSetEdit::Remove,
        },
        Edit::Active {
            path: PathBuf::from("/usr/share/fonts/Terminus.otb"),
            change: Some(batch::Change::Activate),
        },
        Edit::Text(format!("{INSTALLED}mouse\n")),
        Edit::Base(Base::of(None)),
    ];
    for (n, edit) in edits.into_iter().enumerate() {
        assert_eq!(
            docs.change(Change::new(format!("step {n}"), edit)),
            Ok(Done::Made)
        );
        assert_eq!(&drafted(&docs), docs.document().content(), "after step {n}");
    }
}

#[test]
fn a_change_that_changes_nothing_writes_nothing() {
    let mut docs = open();
    let same = Change::new(
        "font-size = 10 in My-Look",
        Edit::Text(INSTALLED.to_owned()),
    );
    assert_eq!(docs.change(same), Ok(Done::Same));
    assert_eq!(
        docs.history().made(),
        0,
        "no step for a change that changes nothing"
    );
    assert_eq!(docs.document().count(), 0);
}

#[test]
fn a_change_the_desk_cannot_write_leaves_the_document_as_it_was() {
    let mut desk = InMemory::new();
    desk.refuse(Some("the disk is full".to_owned()));
    let content = Content {
        text: INSTALLED.to_owned(),
        ..Content::default()
    };
    let installed = Installed {
        text: Some(INSTALLED.to_owned()),
        ..Installed::default()
    };
    let mut docs = Documents::new(
        Document::new("padnoir", content.clone(), installed),
        Box::new(desk),
    );
    assert_eq!(docs.change(eleven()), Err("the disk is full".to_owned()));
    assert_eq!(docs.document().content(), &content);
    assert_eq!(docs.history().made(), 0);
}

#[test]
fn a_change_that_cannot_be_made_is_refused_with_its_reason() {
    let mut docs = open();
    let remove = |frit: &str| Change {
        name: format!("remove {frit}"),
        edit: Edit::FontSet {
            frit: frit.to_owned(),
            edit: FontSetEdit::Remove,
        },
    };
    let refused = docs
        .change(remove("nowhere"))
        .expect_err("no such font set");
    assert!(refused.contains("nothing to remove"), "{refused}");
    let refused = docs.change(remove("unifont")).expect_err("a built-in font");
    assert!(refused.contains("built-in font"), "{refused}");
    let refused = docs.change(remove("a/b")).expect_err("a bad name");
    assert!(refused.contains("cannot be named"), "{refused}");
    assert_eq!(docs.history().made(), 0);
}

#[test]
fn the_count_not_saved_is_the_count_saves_review_shows() {
    let mut docs = open();
    assert_eq!(docs.document().count(), 0);
    assert!(!docs.document().changed());
    // a line changed in place is one change, as the review's "1 change"
    docs.change(eleven()).expect("the size changes");
    let review = docs.document().review();
    assert_eq!(review.lines.len(), 1);
    assert_eq!(review.count(), 1);
    assert_eq!(docs.document().count(), review.count());
    assert!(docs.document().changed());
    // a font set and an active font each count once
    docs.change(Change::new(
        "add mine",
        Edit::FontSet {
            frit: "mine".to_owned(),
            edit: FontSetEdit::Keep("source=builtin:unifont\n# mine\n".to_owned()),
        },
    ))
    .expect("a font set is added");
    docs.change(Change::new(
        "activate Terminus",
        Edit::Active {
            path: PathBuf::from("/usr/share/fonts/Terminus.otb"),
            change: Some(batch::Change::Activate),
        },
    ))
    .expect("a font is activated");
    let review = docs.document().review();
    assert_eq!(
        (
            review.lines.len(),
            review.font_sets.len(),
            review.active.len()
        ),
        (1, 1, 1)
    );
    assert_eq!(docs.document().count(), 3);
    assert_eq!(docs.document().count(), review.count());
}

#[test]
fn a_font_set_with_the_installed_text_is_no_change() {
    let mut docs = open();
    docs.change(Change::new(
        "keep mono",
        Edit::FontSet {
            frit: "mono".to_owned(),
            edit: FontSetEdit::Keep("source=builtin:unifont\n".to_owned()),
        },
    ))
    .expect("the font set is kept");
    assert_eq!(docs.document().count(), 0, "the review lists no font set");
}

#[test]
fn the_count_against_no_installed_file_counts_every_line() {
    let content = Content {
        text: "version = 1\nmouse\n".to_owned(),
        ..Content::default()
    };
    let document = Document::new("padnoir", content, Installed::default());
    assert_eq!(document.count(), 2);
    assert_eq!(document.count(), document.review().count());
}

#[test]
fn switching_documents_keeps_each_ones_history() {
    let mut docs = open();
    docs.change(eleven()).expect("the size changes");
    let other = Document::new(
        "spare",
        Content {
            text: INSTALLED.to_owned(),
            ..Content::default()
        },
        Installed::default(),
    );
    let padnoir = docs.document().clone();
    docs.switch(other);
    assert_eq!(docs.document().name(), "spare");
    assert_eq!(docs.history().made(), 0, "spare's history starts empty");
    docs.switch(padnoir);
    assert_eq!(docs.history().made(), 1, "padnoir's history comes back");
}

#[cfg(feature = "fixture")]
mod fixtures {
    use vtium_config_core::fixture::{large, padnoir};
    use vtium_settings::Kind;

    #[test]
    fn the_screen_maps_machine_holds_one_change_not_saved() {
        let shared = padnoir::shared();
        let document = shared.document();
        assert_eq!(document.name(), padnoir::NAME);
        assert_eq!(document.count(), 1);
        let desk = shared.documents.desk();
        assert_eq!(
            desk.configurations().expect("listed"),
            ["spare", "workstation"]
        );
        assert_eq!(desk.versions().expect("listed").len(), 1);
        assert_eq!(shared.present.devices.len(), 4);
    }

    #[test]
    fn the_large_machine_has_dozens_of_everything() {
        let shared = large::shared();
        let devices = &shared.present.devices;
        let displays = devices.iter().filter(|d| d.kind == Kind::Display).count();
        assert_eq!(displays, 40);
        assert_eq!(devices.len().saturating_sub(displays), 60);
        let desk = shared.documents.desk();
        assert_eq!(desk.configurations().expect("listed").len(), 12);
        let versions = desk.versions().expect("listed");
        assert_eq!(versions.len(), 30);
        assert!(
            versions.windows(2).all(|w| w[0].when > w[1].when),
            "the versions are newest first"
        );
        assert_eq!(shared.document().count(), 1);
    }

    #[test]
    fn each_fixture_is_the_same_each_time_it_is_built() {
        for build in [padnoir::shared, large::shared] {
            let once = format!("{:#?}", build());
            let twice = format!("{:#?}", build());
            assert_eq!(once, twice);
        }
    }
}
