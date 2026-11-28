//! The versions of the document: a file changed on disk and Update's
//! merge, Overwrite, Revert to saved, a version from History, and the diff
//! Show changes shows. Each change passes the one change function, so
//! each is one step that Ctrl+Z takes back, the base with it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use vtium_config_core::batch;
use vtium_config_core::document::{
    Base, Change, Content, Desk, Document, Documents, Edit, FontSetEdit, InMemory, Installed,
    OnDisk, Version,
};
use vtium_config_core::drafts::Session;
use vtium_config_core::places::Paths;
use vtium_config_core::store::Store;
use vtium_config_core::versions::base::{Disk, disk, recover, take};
use vtium_config_core::versions::diff::{Hunk, Line, Mark, diff, whole_file, words};
use vtium_config_core::versions::merge::{Side, merge, update};
use vtium_config_core::versions::{history, overwrite::overwrite, revert, words as said};

const BASE: &str = "version = 1\nosd=full\n[look My-Look]\npalette = vga\nfont-size = 10\n";
/// The person's change: the look at 11 pt.
const MINE: &str = "version = 1\nosd=full\n[look My-Look]\npalette = vga\nfont-size = 11\n";
/// A change made on disk that does not touch the person's lines.
const ON_DISK: &str = "version = 1\nosd=minimal\n[look My-Look]\npalette = vga\nfont-size = 10\n";
/// A change made on disk to the same line as the person's.
const SAME_LINE: &str = "version = 1\nosd=minimal\n[look My-Look]\npalette = vga\nfont-size = 12\n";

/// The document `text`, made from `base`, with `installed` installed now,
/// on a desk kept in memory.
fn open(text: &str, base: Option<&str>, installed: Option<&str>) -> Documents {
    let content = Content {
        text: text.to_owned(),
        base: Base::of(base),
        ..Content::default()
    };
    let desk = InMemory::new().with_draft("padnoir", content.clone());
    let document = Document::new(
        "padnoir",
        content,
        Installed {
            text: installed.map(str::to_owned),
            frits: BTreeMap::new(),
        },
    );
    Documents::new(document, Box::new(desk))
}

fn on_disk(docs: &Documents, installed: Option<&str>) -> Disk {
    disk(docs.document().content(), installed)
}

fn drafted(docs: &Documents) -> Content {
    docs.desk().read("padnoir").expect("read").expect("there")
}

#[test]
fn update_brings_in_a_change_that_does_not_touch_the_persons_lines() {
    let mut docs = open(MINE, Some(BASE), Some(ON_DISK));
    let Disk::Changed(changed) = on_disk(&docs, Some(ON_DISK)) else {
        panic!("the file changed on disk");
    };
    let installed_change = changed.installed_change().expect("the base's text is kept");
    assert_eq!(
        installed_change
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.mark != Mark::Same)
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>(),
        ["osd=full", "osd=minimal"]
    );
    let merged = changed.merge().expect("a base to merge from");
    assert!(!merged.touches(), "the two changes touch different lines");
    let change = update(&merged, ON_DISK).expect("nothing to settle");
    assert_eq!(change.name, said::UPDATE);
    docs.change(change).expect("updated");
    let both = "version = 1\nosd=minimal\n[look My-Look]\npalette = vga\nfont-size = 11\n";
    assert_eq!(docs.document().text(), both);
    assert_eq!(docs.document().content().base, Base::of(Some(ON_DISK)));
    assert_eq!(&drafted(&docs), docs.document().content());
    // the review then shows as for a file that did not change
    assert_eq!(on_disk(&docs, Some(ON_DISK)), Disk::Same);
    // one step: Ctrl+Z brings back the text and the old base, and the next
    // Save shows the screen for a file changed on disk again
    assert_eq!(docs.undo().expect("undone").as_deref(), Some(said::UPDATE));
    assert_eq!(docs.document().text(), MINE);
    assert_eq!(docs.document().content().base, Base::of(Some(BASE)));
    assert!(matches!(on_disk(&docs, Some(ON_DISK)), Disk::Changed(_)));
    docs.redo().expect("redone");
    assert_eq!(docs.document().text(), both);
}

#[test]
fn lines_both_sides_changed_wait_for_the_person_to_settle_them() {
    for (side, kept) in [
        (Side::Mine, "font-size = 11"),
        (Side::Installed, "font-size = 12"),
    ] {
        let mut docs = open(MINE, Some(BASE), Some(SAME_LINE));
        let Disk::Changed(changed) = on_disk(&docs, Some(SAME_LINE)) else {
            panic!("the file changed on disk");
        };
        let mut merged = changed.merge().expect("a base to merge from");
        assert!(merged.touches());
        assert_eq!(merged.settled(), (1, 0));
        let place = merged.both().next().expect("one place").clone();
        assert_eq!(
            (place.mine.as_str(), place.installed.as_str()),
            ("font-size = 11\n", "font-size = 12\n")
        );
        assert_eq!((place.mine_line, place.installed_line), (5, 5));
        // nothing to save while a line is not settled
        assert_eq!(merged.text(), None);
        assert!(update(&merged, SAME_LINE).is_none());
        assert_eq!(merged.next(), Some(0));
        assert!(merged.settle(0, side));
        assert!(!merged.settle(1, side), "there is one place only");
        assert_eq!(merged.next(), None);
        docs.change(update(&merged, SAME_LINE).expect("settled"))
            .expect("updated");
        // the line settled as chosen, and the installed file's other
        // change comes in with it
        assert_eq!(
            docs.document().text(),
            format!("version = 1\nosd=minimal\n[look My-Look]\npalette = vga\n{kept}\n"),
            "{side:?}"
        );
        assert_eq!(docs.document().content().base, Base::of(Some(SAME_LINE)));
        docs.undo().expect("undone");
        assert_eq!(docs.document().text(), MINE, "{side:?}");
        assert_eq!(docs.document().content().base, Base::of(Some(BASE)));
    }
}

#[test]
fn a_missing_installed_file_is_no_conflict() {
    let mut docs = open(MINE, Some(BASE), None);
    assert_eq!(on_disk(&docs, None), Disk::Missing);
    // whatever the base says, before Overwrite records the missing file
    // as the base and after Ctrl+Z takes that back
    docs.change(overwrite(None)).expect("recorded");
    assert_eq!(docs.document().content().base, Base::of(None));
    assert_eq!(on_disk(&docs, None), Disk::Missing);
    docs.undo().expect("undone");
    assert_eq!(docs.document().content().base, Base::of(Some(BASE)));
    assert_eq!(on_disk(&docs, None), Disk::Missing);
}

#[test]
fn an_installed_file_equal_to_the_base_has_not_changed() {
    let mut docs = open(MINE, Some(BASE), Some(BASE));
    assert_eq!(on_disk(&docs, Some(BASE)), Disk::Same);
    // an Update the person takes back leaves it so
    docs.change(update(&merge(BASE, MINE, BASE), BASE).expect("settled"))
        .expect("no change");
    assert_eq!(docs.history().made(), 0, "the merge changed nothing");
    assert_eq!(on_disk(&docs, Some(BASE)), Disk::Same);
}

#[test]
fn an_installed_change_no_change_of_the_persons_meets_comes_in_without_a_choice() {
    // the person changed only what lies beside vtium.conf
    let mut docs = open(BASE, Some(BASE), Some(ON_DISK));
    assert_eq!(on_disk(&docs, Some(ON_DISK)), Disk::Taken);
    docs.change(take(ON_DISK)).expect("taken");
    assert_eq!(docs.document().text(), ON_DISK);
    assert_eq!(on_disk(&docs, Some(ON_DISK)), Disk::Same);
    docs.undo().expect("undone");
    assert_eq!(docs.document().text(), BASE);
    // the person's text is already the installed file's
    let docs = open(ON_DISK, Some(BASE), Some(ON_DISK));
    assert_eq!(on_disk(&docs, Some(ON_DISK)), Disk::Taken);
}

#[test]
fn overwrite_records_the_installed_file_as_the_base_in_one_step() {
    let mut docs = open(MINE, Some(BASE), Some(SAME_LINE));
    docs.change(overwrite(Some(SAME_LINE))).expect("recorded");
    assert_eq!(docs.document().text(), MINE, "the person's text stays");
    assert_eq!(docs.document().content().base, Base::of(Some(SAME_LINE)));
    assert_eq!(on_disk(&docs, Some(SAME_LINE)), Disk::Same);
    assert_eq!(
        docs.undo().expect("undone").as_deref(),
        Some(said::OVERWRITE)
    );
    assert!(matches!(on_disk(&docs, Some(SAME_LINE)), Disk::Changed(_)));
}

#[test]
fn a_draft_with_only_its_bases_hash_finds_the_text_in_the_installed_history() {
    let only_hash = Base {
        hash: Base::of(Some(BASE)).hash,
        text: None,
    };
    let content = Content {
        text: MINE.to_owned(),
        base: only_hash.clone(),
        ..Content::default()
    };
    let Disk::Changed(changed) = disk(&content, Some(ON_DISK)) else {
        panic!("the file changed on disk");
    };
    assert_eq!(changed.merge().err(), Some(said::NO_BASE_TEXT));
    assert_eq!(changed.installed_change(), None);
    let version = |text: &str| Version {
        draft: "padnoir".to_owned(),
        number: 1,
        when: 0,
        text: text.to_owned(),
    };
    assert_eq!(
        recover(only_hash.clone(), &[version(ON_DISK), version(BASE)]),
        Base::of(Some(BASE))
    );
    assert_eq!(recover(only_hash.clone(), &[version(ON_DISK)]), only_hash);
}

#[test]
fn revert_to_saved_counts_what_is_lost_and_is_one_step() {
    let mut docs = open(BASE, Some(BASE), Some(BASE));
    let terminus = PathBuf::from("/usr/share/fonts/Terminus.otb");
    for edit in [
        Edit::Text(format!("{MINE}[head console]\nlook = My-Look\n")),
        Edit::FontSet {
            frit: "mine".to_owned(),
            edit: FontSetEdit::Keep("source=builtin:unifont\n".to_owned()),
        },
        Edit::Active {
            path: terminus.clone(),
            change: Some(batch::Change::Activate),
        },
    ] {
        docs.change(vtium_config_core::document::Change::new("a change", edit))
            .expect("made");
    }
    let lost = revert::lost(docs.document());
    assert_eq!(
        lost,
        revert::Lost {
            changes: 5,
            added: 3,
            removed: 1,
            font_sets: 1,
            active: 1,
        }
    );
    assert_eq!(lost.changes, docs.document().count(), "the top row's count");
    assert_eq!(
        said::lost(&lost),
        "5 changes are lost: 3 lines added, 1 removed, 1 font set change, and 1 change to the active fonts."
    );
    let before = docs.document().content().clone();
    docs.change(revert::revert(Some(BASE))).expect("reverted");
    let after = docs.document().content();
    assert_eq!(after.text, BASE);
    assert!(after.font_sets.kept.is_empty() && after.active.is_empty());
    assert_eq!(docs.document().count(), 0);
    assert_eq!(&drafted(&docs), docs.document().content());
    // Ctrl+Z brings every change back at once
    assert_eq!(docs.undo().expect("undone").as_deref(), Some(said::REVERT));
    assert_eq!(docs.document().content(), &before);
    assert_eq!(docs.document().count(), 5);
}

#[test]
fn revert_with_no_installed_file_starts_from_the_header() {
    let mut docs = open(MINE, None, None);
    docs.change(revert::revert(None)).expect("reverted");
    assert_eq!(docs.document().text(), vtium_config_core::drafts::HEADER);
    assert_eq!(docs.document().content().base, Base::of(None));
}

#[test]
fn a_version_from_history_is_one_step_and_its_view_reads_only() {
    let mut docs = open(MINE, Some(BASE), Some(BASE));
    let version = Version {
        draft: "padnoir".to_owned(),
        number: 2,
        when: 1_795_770_000,
        text: ON_DISK.to_owned(),
    };
    let before = docs.document().content().clone();
    let shown = history::view(docs.document(), &version);
    let marked: Vec<(Mark, &str)> = shown
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.mark != Mark::Same)
        .map(|l| (l.mark, l.text.as_str()))
        .collect();
    // red lines leave the document, green ones come in with the version
    assert_eq!(
        marked,
        [
            (Mark::Removed, "osd=full"),
            (Mark::Added, "osd=minimal"),
            (Mark::Removed, "font-size = 11"),
            (Mark::Added, "font-size = 10"),
        ]
    );
    assert_eq!(
        docs.document().content(),
        &before,
        "viewing changes nothing"
    );
    let change = history::use_version(&version);
    assert_eq!(change.name, "the version of 2026-11-27 09:00:00 UTC");
    docs.change(change).expect("used");
    assert_eq!(docs.document().text(), ON_DISK);
    assert_eq!(docs.history().made(), 1);
    docs.undo().expect("undone");
    assert_eq!(docs.document().content(), &before);
}

/// ADR-1403 D11's figure of Show changes: three settings added at the top
/// level, the look's size changed from 10 to 11, and a head added.
const FIGURE_OLD: &str = concat!(
    "# This is vtium's configuration file, /etc/vtium/vtium.conf. It holds only\n",
    "# the settings you change, and vtium uses its default for every other setting.\n",
    "#\n",
    "# /etc/vtium/vtium.conf.example describes every setting and shows its default.\n",
    "# vtium --help lists every setting with a one-line description.\n",
    "\n",
    "\n",
    "# My-Look: the laptop's own panel.\n",
    "# It draws at 10 pt.\n",
    "[look My-Look]\n",
    "palette = vga\n",
    "font-size = 10\n",
    "font-name = cullet-mono\n",
    "\n",
);

fn figure_new() -> String {
    FIGURE_OLD
        .replacen(
            "\n\n\n",
            "\n\nfont-size=11\nfont-name=unifont\nosd=full\n\n",
            1,
        )
        .replace("font-size = 10", "font-size = 11")
        + "[head console]\nlook = My-Look\n"
}

fn line(mark: Mark, old: Option<usize>, new: Option<usize>, at: usize, text: &str) -> Line {
    Line {
        mark,
        old,
        new,
        at,
        text: text.to_owned(),
        words: Vec::new(),
    }
}

#[test]
fn the_figures_changes_show_hunk_for_hunk() {
    use Mark::{Added, Removed, Same};
    let shown = diff(FIGURE_OLD, &figure_new());
    let hunk = |section: &str, lines: Vec<Line>| Hunk {
        section: section.to_owned(),
        lines,
    };
    let mut ten = line(Removed, Some(12), None, 15, "font-size = 10");
    ten.words.push(12..14);
    let mut eleven = line(Added, None, Some(15), 15, "font-size = 11");
    eleven.words.push(12..14);
    assert_eq!(
        shown.hunks,
        [
            hunk(
                "settings",
                vec![
                    line(Added, None, Some(7), 7, "font-size=11"),
                    line(Added, None, Some(8), 8, "font-name=unifont"),
                    line(Added, None, Some(9), 9, "osd=full"),
                ]
            ),
            hunk(
                "[look My-Look]",
                vec![
                    line(Same, Some(10), Some(13), 13, "[look My-Look]"),
                    line(Same, Some(11), Some(14), 14, "palette = vga"),
                    ten,
                    eleven,
                    line(Same, Some(13), Some(16), 16, "font-name = cullet-mono"),
                ]
            ),
            hunk(
                "[head console]",
                vec![
                    line(Added, None, Some(18), 18, "[head console]"),
                    line(Added, None, Some(19), 19, "look = My-Look"),
                ]
            ),
        ]
    );
    // the changed word inside the line is the size alone
    fn size(l: &Line) -> Vec<&str> {
        l.words
            .iter()
            .filter_map(|r| l.text.get(r.clone()))
            .collect()
    }
    let look = shown.hunks.get(1).expect("the look's hunk");
    assert_eq!(
        look.lines.iter().map(size).collect::<Vec<_>>(),
        [vec![], vec![], vec!["10"], vec!["11"], vec![]]
    );
}

#[test]
fn the_whole_file_shows_every_line_with_each_change_marked() {
    let new = figure_new();
    let lines = whole_file(FIGURE_OLD, &new);
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.mark != Mark::Removed)
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>(),
        new.lines().collect::<Vec<_>>()
    );
    assert_eq!(
        lines.iter().filter(|l| l.mark == Mark::Removed).count(),
        1,
        "font-size = 10, where it stood"
    );
    assert!(diff(BASE, BASE).is_empty());
}

#[test]
fn only_the_words_that_changed_are_marked() {
    assert_eq!(
        words(
            "display connector:eDP-1 -> console",
            "display connector:DP-8 -> desk"
        ),
        (vec![18..21, 22..23, 27..34], vec![18..20, 21..22, 26..30])
    );
}

#[test]
fn a_hunk_shows_two_lines_of_context_on_each_side() {
    let old = "[look big]\na = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\ng = 7\n";
    let shown = diff(old, &old.replace("d = 4", "d = 40"));
    assert_eq!(
        shown
            .hunks
            .iter()
            .map(|h| h.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [["b = 2", "c = 3", "d = 4", "d = 40", "e = 5", "f = 6"]]
    );
}

/// A machine under the test's own folder with `installed` as vtium.conf
/// and an empty store, as the real desk reads it.
fn machine(test: &str, installed: &str) -> (Paths, Store) {
    let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("versions-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let etc = root.join("etc");
    std::fs::create_dir_all(&etc).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(etc.join("vtium.conf"), installed).unwrap_or_else(|e| panic!("{e}"));
    let paths = Paths {
        configdir: etc,
        frits: Err("no font sets on this machine".to_owned()),
        drafts: root.join("bundles"),
        sudo: "sudo".into(),
        vtium: "vtium".into(),
        helper: vtium_config_core::specimen::Helper::at(root.join("no-helper")),
        sys: root.join("sys"),
    };
    let store = Store {
        config: root.join("config"),
        state: root.join("state"),
    };
    (paths, store)
}

#[test]
fn a_draft_whose_installed_file_changed_on_disk_opens_with_the_bases_text() {
    let (paths, store) = machine("base-text", BASE);
    let session = Session {
        store: store.clone(),
        paths: &paths,
    };
    let document = OnDisk::open(&session).expect("opened");
    let name = document.name().to_owned();
    let mut docs = Documents::new(document, Box::new(OnDisk::new(store.clone())));
    docs.change(Change::new("font-size = 11", Edit::Text(MINE.to_owned())))
        .expect("changed");
    std::fs::write(session.installed_path(), ON_DISK).unwrap_or_else(|e| panic!("{e}"));
    // vtium-config starts again: the draft's base is the file it was made
    // from, whose text the store keeps
    let document = OnDisk::open(&session).expect("opened again");
    assert_eq!(document.name(), name);
    assert_eq!(document.content().base, Base::of(Some(BASE)));
    let read = OnDisk::new(store)
        .read(&name)
        .expect("read")
        .expect("there");
    assert_eq!(&read, document.content());
    let Disk::Changed(changed) = disk(document.content(), Some(ON_DISK)) else {
        panic!("the file changed on disk");
    };
    let merged = changed.merge().expect("the base's text to merge from");
    let both = "version = 1\nosd=minimal\n[look My-Look]\npalette = vga\nfont-size = 11\n";
    assert_eq!(merged.text().as_deref(), Some(both));
}
