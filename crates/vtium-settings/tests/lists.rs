//! The lists of active resources that packages install beside the batch
//! file, root's folder that turns one off, and the batch file's `-` lines
//! that turn one path off.
//!
//! No test runs as root, so each trusts its own user's files with
//! [`Trust::owner`], below a fixture folder of its own. The fixture lays
//! out a machine: `etc/batch.conf` and `etc/batch.d` for root, and
//! `share/vtium/fonts` and `share/vtium/batch.d` for the package.
//!
//! References:
//! - ADR-1205 D4, as amended on 2026-11-27: the packages' lists, their trust rule, a listed file that is not installed, turning a list off from /etc, and the `-` line.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use vtium_settings::Trust;
use vtium_settings::Untrusted;
use vtium_settings::batch::{
    Batch, Entry, Folder, LISTS_MAX, LineFault, ListFile, ListFinding, ListState, MERGED_BATCH_MAX,
    Origin, Places, Read, Unread, off_line,
};
use vtium_settings::cullet::{Check, Gate, Rule};
use vtium_settings::mounts::Mounts;

/// A machine laid out under a fixture folder of its own.
struct Machine {
    root: PathBuf,
    places: Places,
    trust: Trust,
}

impl Machine {
    fn new(name: &str) -> Machine {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("vtium-settings-lists")
            .join(name);
        let _ = fs::remove_dir_all(&root);
        mkdir(&root);
        let root = fs::canonicalize(&root).expect("the fixture's canonical path");
        for folder in ["etc/batch.d", "share/vtium/fonts", "share/vtium/batch.d"] {
            mkdir(&root.join(folder));
        }
        let uid = fs::metadata(&root).expect("the fixture").uid();
        let places = Places {
            file: root.join("etc/batch.conf"),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            fonts: root.join("share/vtium/fonts"),
            palettes: root.join("share/vtium/palettes"),
            pointers: root.join("share/vtium/pointers.svg"),
            console_font_folders: Vec::new(),
        };
        Machine {
            trust: Trust::owner(uid).below(&root),
            root,
            places,
        }
    }

    fn package(&self, name: &str) -> PathBuf {
        self.root.join("share/vtium/batch.d").join(name)
    }

    fn admin(&self, name: &str) -> PathBuf {
        self.root.join("etc/batch.d").join(name)
    }

    fn batch_conf(&self, text: &str) {
        write(&self.places.file, text.as_bytes(), 0o644);
    }

    fn read(&self) -> Read {
        Batch::system(&self.places, &self.trust)
    }
}

fn mkdir(path: &Path) {
    fs::create_dir_all(path).expect("the folder");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("its mode");
}

fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).expect("the file");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("its mode");
}

fn paths(read: &Read) -> Vec<&Path> {
    read.batch
        .entries()
        .iter()
        .map(|Entry::Path(p)| p.as_path())
        .collect()
}

fn origin_list(batch: &Batch, path: &str) -> Option<String> {
    match batch.origin(Path::new(path)) {
        Some(Origin::List(list)) => Some(list.name.clone()),
        _ => None,
    }
}

#[test]
fn a_package_list_adds_to_batch_conf_and_names_itself_as_the_origin() {
    let m = Machine::new("adds");
    m.batch_conf("/usr/share/fonts/own.ttf\n");
    let list = m.package("vtium-fonts.conf");
    write(
        &list,
        b"# the fonts vtium-fonts activates\n/usr/share/fonts/vtium/Tiny5Mono.bdf\n\n/usr/share/fonts/own.ttf\n",
        0o644,
    );
    let read = m.read();
    assert!(read.findings.is_empty(), "{:?}", read.findings);
    assert!(read.list_findings.is_empty(), "{:?}", read.list_findings);
    assert_eq!(
        paths(&read),
        [
            Path::new("/usr/share/fonts/own.ttf"),
            Path::new("/usr/share/fonts/vtium/Tiny5Mono.bdf")
        ]
    );
    assert!(
        read.batch
            .holds(Path::new("/usr/share/fonts/vtium/Tiny5Mono.bdf"))
    );
    assert_eq!(
        origin_list(&read.batch, "/usr/share/fonts/vtium/Tiny5Mono.bdf").as_deref(),
        Some("vtium-fonts")
    );
    // batch.conf holds root's own choices, so it is the origin of a path
    // both name, and the list still names the path
    assert_eq!(
        read.batch.origin(Path::new("/usr/share/fonts/own.ttf")),
        Some(Origin::BatchFile)
    );
    let by = read
        .batch
        .listed_by(Path::new("/usr/share/fonts/own.ttf"))
        .expect("the list names it");
    assert_eq!(
        (by.name.as_str(), by.folder),
        ("vtium-fonts", Folder::Package)
    );
    assert_eq!(by.file, list);
    assert_eq!(
        read.list_files,
        [ListFile {
            path: list.clone(),
            folder: Folder::Package,
            state: ListState::Read,
            set_by: list,
        }]
    );
}

#[test]
fn a_package_list_adds_to_the_fresh_install_list_where_no_batch_conf_is_read() {
    let m = Machine::new("fresh");
    write(
        &m.package("vtium-fonts.conf"),
        b"/usr/share/fonts/vtium/Unscii-8.hex\n",
        0o644,
    );
    let read = m.read();
    let fresh = m.places.fonts.join("CulletMono-Regular.ttf");
    assert_eq!(read.batch.origin(&fresh), Some(Origin::FreshInstall));
    assert_eq!(
        origin_list(&read.batch, "/usr/share/fonts/vtium/Unscii-8.hex").as_deref(),
        Some("vtium-fonts")
    );

    // a batch.conf vtium refuses leaves the list of a fresh install, and
    // the package's list still adds to it
    write(&m.places.file, b"/usr/share/fonts/own.ttf\n", 0o666);
    let refused = m.read();
    assert_eq!(refused.batch.origin(&fresh), Some(Origin::FreshInstall));
    assert!(!refused.batch.holds(Path::new("/usr/share/fonts/own.ttf")));
    assert!(
        refused
            .batch
            .holds(Path::new("/usr/share/fonts/vtium/Unscii-8.hex"))
    );
}

#[test]
fn only_names_ending_in_conf_are_read_in_the_byte_order_of_the_names() {
    let m = Machine::new("names");
    m.batch_conf("");
    write(
        &m.package("b.conf"),
        b"/fonts/shared.ttf\n/fonts/b.ttf\n",
        0o644,
    );
    write(
        &m.package("a.conf"),
        b"/fonts/shared.ttf\n/fonts/a.ttf\n",
        0o644,
    );
    for passed in ["c.conf.pacnew", "c.conf.pacsave", "c.conf~", ".conf", "c"] {
        write(&m.package(passed), b"/fonts/never.ttf\n", 0o644);
    }
    let read = m.read();
    assert!(read.list_findings.is_empty(), "{:?}", read.list_findings);
    assert_eq!(
        paths(&read),
        [
            Path::new("/fonts/shared.ttf"),
            Path::new("/fonts/a.ttf"),
            Path::new("/fonts/b.ttf")
        ]
    );
    assert_eq!(
        origin_list(&read.batch, "/fonts/shared.ttf").as_deref(),
        Some("a")
    );
    assert!(!read.batch.holds(Path::new("/fonts/never.ttf")));
    let names: Vec<&str> = read.batch.lists().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
}

#[test]
fn roots_file_of_the_same_name_turns_a_package_list_off_or_replaces_it() {
    let m = Machine::new("mask");
    m.batch_conf("");
    let off = m.package("off.conf");
    let swapped = m.package("swapped.conf");
    write(&off, b"/fonts/off.ttf\n", 0o644);
    write(&swapped, b"/fonts/theirs.ttf\n", 0o644);
    let mask = m.admin("off.conf");
    let replacement = m.admin("swapped.conf");
    let own = m.admin("own.conf");
    write(&mask, b"# turned off by root\n\n", 0o644);
    write(&replacement, b"/fonts/ours.ttf\n", 0o644);
    write(&own, b"/fonts/own.ttf\n", 0o644);

    let read = m.read();
    assert!(read.list_findings.is_empty(), "{:?}", read.list_findings);
    assert!(!read.batch.holds(Path::new("/fonts/off.ttf")));
    assert!(!read.batch.holds(Path::new("/fonts/theirs.ttf")));
    assert!(read.batch.holds(Path::new("/fonts/ours.ttf")));
    assert!(read.batch.holds(Path::new("/fonts/own.ttf")));
    let by = read
        .batch
        .listed_by(Path::new("/fonts/ours.ttf"))
        .expect("root's list");
    assert_eq!((by.name.as_str(), by.folder), ("swapped", Folder::Admin));
    let file = |path: &Path, folder, state, set_by: &Path| ListFile {
        path: path.to_path_buf(),
        folder,
        state,
        set_by: set_by.to_path_buf(),
    };
    assert_eq!(
        read.list_files,
        [
            file(&off, Folder::Package, ListState::TurnedOff, &mask),
            file(&mask, Folder::Admin, ListState::Read, &mask),
            file(&own, Folder::Admin, ListState::Read, &own),
            file(&swapped, Folder::Package, ListState::Replaced, &replacement),
            file(&replacement, Folder::Admin, ListState::Read, &replacement),
        ]
    );
}

#[test]
fn any_entry_of_the_name_in_roots_folder_turns_the_package_list_off() {
    let m = Machine::new("refused-mask");
    m.batch_conf("");
    let theirs = m.package("vtium-fonts.conf");
    write(&theirs, b"/fonts/a.ttf\n", 0o644);
    // systemd's way to mask a unit: no regular file, so no finding
    let link = m.admin("vtium-fonts.conf");
    symlink("/dev/null", &link).expect("the link");
    let read = m.read();
    assert!(!read.batch.holds(Path::new("/fonts/a.ttf")));
    assert!(read.list_findings.is_empty(), "{:?}", read.list_findings);
    assert_eq!(
        read.list_files,
        [
            ListFile {
                path: theirs.clone(),
                folder: Folder::Package,
                state: ListState::TurnedOff,
                set_by: link.clone(),
            },
            ListFile {
                path: link.clone(),
                folder: Folder::Admin,
                state: ListState::TurnedOff,
                set_by: link.clone(),
            },
        ]
    );
    assert_eq!(read.list_files[0].state.to_string(), "turned off");

    // so does a folder of that name
    fs::remove_file(&link).expect("the link goes");
    mkdir(&link);
    assert!(!m.read().batch.holds(Path::new("/fonts/a.ttf")));
    fs::remove_dir(&link).expect("the folder goes");

    // and a regular file that others can write, which adds nothing and is
    // named
    write(&link, b"/fonts/planted.ttf\n", 0o666);
    let open = m.read();
    assert!(!open.batch.holds(Path::new("/fonts/a.ttf")));
    assert!(!open.batch.holds(Path::new("/fonts/planted.ttf")));
    assert!(matches!(
        &open.list_findings[..],
        [ListFinding::Refused {
            why: Unread::Untrusted(Untrusted::Writable { .. }),
            masks: Some(masked),
            ..
        }] if *masked == theirs
    ));
    let said = open.list_findings[0].to_string();
    assert!(
        said.starts_with(&format!("{}: vtium activates nothing", link.display())),
        "{said}"
    );
    assert!(
        said.ends_with(&format!(
            "Its name still turns off the package's list {}.",
            theirs.display()
        )),
        "{said}"
    );
}

#[test]
fn a_list_is_read_as_batch_conf_is_and_one_that_fails_is_named() {
    let m = Machine::new("untrusted");
    m.batch_conf("/fonts/root.ttf\n");
    let open = m.package("open.conf");
    write(&open, b"/fonts/open.ttf\n", 0o664);
    let large = m.package("large.conf");
    let mut long = b"/fonts/large.ttf\n".to_vec();
    long.resize(vtium_settings::batch::BATCH_MAX + 1, b'\n');
    write(&large, &long, 0o644);
    // a link is followed, and each folder it crosses is checked
    let linked = m.package("linked.conf");
    let target = m.root.join("target.conf");
    write(&target, b"/fonts/linked.ttf\n", 0o644);
    symlink(&target, &linked).expect("the link");
    let shared = m.root.join("shared");
    mkdir(&shared);
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).expect("its mode");
    write(&shared.join("via.conf"), b"/fonts/via.ttf\n", 0o644);
    let via = m.package("via.conf");
    symlink(shared.join("via.conf"), &via).expect("the link");
    let good = m.package("zz.conf");
    write(&good, b"/fonts/good.ttf\n", 0o644);

    let read = m.read();
    assert!(read.findings.is_empty(), "{:?}", read.findings);
    assert_eq!(
        paths(&read),
        [
            Path::new("/fonts/root.ttf"),
            Path::new("/fonts/linked.ttf"),
            Path::new("/fonts/good.ttf")
        ]
    );
    assert_eq!(read.list_findings.len(), 3, "{:?}", read.list_findings);
    assert!(matches!(
        &read.list_findings[0],
        ListFinding::Refused { path, why: Unread::TooLarge, masks: None } if *path == large
    ));
    assert!(matches!(
        &read.list_findings[1],
        ListFinding::Refused { path, why: Unread::Untrusted(Untrusted::Writable { .. }), masks: None } if *path == open
    ));
    assert!(matches!(
        &read.list_findings[2],
        ListFinding::Refused { path, why: Unread::Untrusted(Untrusted::Writable { .. }), masks: None } if *path == via
    ));
    let states: Vec<ListState> = read.list_files.iter().map(|f| f.state).collect();
    assert_eq!(
        states,
        [
            ListState::Refused,
            ListState::Read,
            ListState::Refused,
            ListState::Refused,
            ListState::Read
        ]
    );
}

#[test]
fn a_minus_line_in_batch_conf_turns_a_path_off_whichever_list_names_it() {
    let m = Machine::new("minus");
    m.batch_conf("/fonts/own.ttf\n-/fonts/own.ttf\n-/fonts//listed.ttf\n-/fonts/nowhere.ttf\n");
    write(
        &m.package("vtium-fonts.conf"),
        b"/fonts/listed.ttf\n/fonts/kept.ttf\n",
        0o644,
    );
    let read = m.read();
    assert!(read.findings.is_empty(), "{:?}", read.findings);
    assert_eq!(paths(&read), [Path::new("/fonts/kept.ttf")]);
    assert!(!read.batch.holds(Path::new("/fonts/own.ttf")));
    assert!(!read.batch.holds(Path::new("/fonts/listed.ttf")));
    assert!(read.batch.turns_off(Path::new("/fonts/listed.ttf")));
    assert!(!read.batch.turns_off(Path::new("/fonts/kept.ttf")));
    // the list still names the path, so taking the - line out activates it
    let by = read
        .batch
        .listed_by(Path::new("/fonts/listed.ttf"))
        .expect("the list names it");
    assert_eq!(by.name, "vtium-fonts");
    assert!(read.batch.listed_by(Path::new("/fonts/own.ttf")).is_none());
    let off: Vec<&Path> = read.batch.turned_off().collect();
    assert_eq!(
        off,
        [
            Path::new("/fonts/listed.ttf"),
            Path::new("/fonts/nowhere.ttf"),
            Path::new("/fonts/own.ttf")
        ]
    );

    // only batch.conf turns a path off
    write(
        &m.admin("root.conf"),
        b"-/fonts/kept.ttf\n/fonts/also.ttf\n",
        0o644,
    );
    let read = m.read();
    assert!(read.batch.holds(Path::new("/fonts/kept.ttf")));
    assert!(read.batch.holds(Path::new("/fonts/also.ttf")));
    assert_eq!(
        read.list_findings,
        [ListFinding::Line {
            path: m.admin("root.conf"),
            line: 1,
            fault: LineFault::OffInList,
        }]
    );
    assert_eq!(
        read.list_findings[0].to_string(),
        format!(
            "{}:1: This line starts with -, which turns a path off, and only batch.conf can turn a path off. Because of this, vtium ignores the line.",
            m.admin("root.conf").display()
        )
    );
}

#[test]
fn a_minus_line_is_a_line_of_batch_conf_and_not_a_fault() {
    let read = Batch::parse(b"/a\n-/a\n-/b\n-relative\n", Path::new("/batch.conf"));
    assert!(read.batch.entries().is_empty());
    assert!(read.batch.turns_off(Path::new("/b")));
    assert_eq!(
        read.findings,
        [vtium_settings::batch::Finding::Line {
            path: PathBuf::from("/batch.conf"),
            line: 4,
            fault: LineFault::NotAbsolute,
        }]
    );
    assert_eq!(
        off_line(Path::new("/usr/share/fonts//a.ttf")).as_deref(),
        Some("-/usr/share/fonts/a.ttf")
    );
    assert_eq!(off_line(Path::new("fonts/a.ttf")), None);
    assert_eq!(off_line(Path::new("/fonts/../a.ttf")), None);
    // the line vtium-config writes reads back as turning the path off
    let line = off_line(Path::new("/fonts/a.ttf")).expect("a line");
    assert!(
        Batch::parse(line.as_bytes(), Path::new("/b"))
            .batch
            .turns_off(Path::new("/fonts/a.ttf"))
    );
}

#[test]
fn a_listed_file_that_is_not_installed_counts_as_missing_and_is_no_finding() {
    let m = Machine::new("not-installed");
    m.batch_conf("");
    let font = m.root.join("share/fonts/noto-cjk/NotoSansCJK-Regular.ttc");
    write(
        &m.package("vtium-fonts.conf"),
        format!("{}\n", font.display()).as_bytes(),
        0o644,
    );
    let read = m.read();
    assert!(read.list_findings.is_empty(), "{:?}", read.list_findings);
    assert!(read.batch.holds(&font));
    let gate = Gate::new(
        Rule::new(m.trust.clone()).below(&m.root),
        Some(Mounts::system().expect("the mount table")),
        read.batch,
    );
    let refused = gate.open(&font).expect_err("nothing is installed there");
    assert_eq!(refused.check, Check::Missing);
}

#[test]
fn an_entry_past_the_count_of_its_folder_activates_nothing_and_is_named() {
    let m = Machine::new("limit");
    m.batch_conf("");
    for n in 0..=LISTS_MAX {
        write(
            &m.package(&format!("l{n:03}.conf")),
            format!("/fonts/{n:03}.ttf\n").as_bytes(),
            0o644,
        );
    }
    // each folder has a count of its own
    write(&m.admin("root.conf"), b"/fonts/root.ttf\n", 0o644);
    let last = LISTS_MAX - 1;
    let read = m.read();
    assert!(
        read.batch
            .holds(Path::new(&format!("/fonts/{last:03}.ttf")))
    );
    assert!(read.batch.holds(Path::new("/fonts/root.ttf")));
    assert!(
        !read
            .batch
            .holds(Path::new(&format!("/fonts/{LISTS_MAX:03}.ttf")))
    );
    let past = m.package(&format!("l{LISTS_MAX:03}.conf"));
    assert_eq!(
        read.list_findings,
        [ListFinding::PastLimit { path: past.clone() }]
    );
    assert_eq!(read.list_files.len(), LISTS_MAX + 2);
    let named = read.list_files.last().expect("the entry past the count");
    assert_eq!((named.state, &named.path), (ListState::Refused, &past));
}

#[test]
fn a_list_that_would_take_the_merged_batch_past_its_bound_is_left_out_with_those_after_it() {
    let m = Machine::new("bound");
    m.batch_conf("/fonts/root.ttf\n");
    // 60 paths of 1,000 bytes a list, so the eighteenth list passes 1 MiB
    let lists = 20;
    for n in 0..lists {
        let mut text = String::new();
        for p in 0..60 {
            let path = format!("/fonts/{n:02}/{p:02}/");
            let pad = "x".repeat(1000 - path.len());
            text.push_str(&path);
            text.push_str(&pad);
            text.push('\n');
        }
        write(&m.package(&format!("l{n:02}.conf")), text.as_bytes(), 0o644);
    }
    let read = m.read();
    let held: usize = read
        .batch
        .entries()
        .iter()
        .map(|Entry::Path(p)| p.as_os_str().len())
        .sum();
    assert!(held <= MERGED_BATCH_MAX, "{held}");
    let fits = (MERGED_BATCH_MAX - "/fonts/root.ttf".len()) / 60_000;
    assert_eq!(fits, 17);
    assert_eq!(read.batch.lists().len(), fits);
    let left_out: Vec<ListFinding> = (fits..lists)
        .map(|n| ListFinding::PastBound {
            path: m.package(&format!("l{n:02}.conf")),
        })
        .collect();
    assert_eq!(read.list_findings, left_out);
    let states: Vec<ListState> = read.list_files.iter().map(|f| f.state).collect();
    assert_eq!(states[fits - 1], ListState::Read);
    assert!(states[fits..].iter().all(|s| *s == ListState::Refused));
    assert!(read.batch.holds(Path::new("/fonts/root.ttf")));
}

#[test]
fn a_list_folder_vtium_cannot_list_is_named_and_a_missing_one_is_not() {
    let m = Machine::new("folder");
    m.batch_conf("/fonts/a.ttf\n");
    fs::remove_dir(m.root.join("etc/batch.d")).expect("the folder goes");
    assert!(m.read().list_findings.is_empty());
    write(&m.root.join("etc/batch.d"), b"", 0o644);
    let read = m.read();
    assert!(read.batch.holds(Path::new("/fonts/a.ttf")));
    assert!(matches!(
        &read.list_findings[..],
        [ListFinding::Folder { path, .. }] if *path == m.root.join("etc/batch.d")
    ));
}

#[test]
fn the_list_folders_sit_beside_the_package_fonts_and_the_batch_file() {
    if option_env!("VTIUM_PACKAGE_FONTS").is_none() {
        assert_eq!(
            Places::system().package_lists(),
            Path::new("/usr/share/vtium/batch.d")
        );
    }
    if option_env!("VTIUM_BATCH").is_none() {
        assert_eq!(
            Places::system().admin_lists(),
            Path::new("/etc/vtium/batch.d")
        );
    }
    let local = Places {
        fonts: PathBuf::from("/usr/local/share/vtium/fonts"),
        ..Places::system()
    };
    assert_eq!(
        local.package_lists(),
        Path::new("/usr/local/share/vtium/batch.d")
    );
}
