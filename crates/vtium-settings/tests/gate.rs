//! One gate for every kind of resource: a font, a color scheme, a color
//! map, a pointer theme by its folder, and the package's SVG pointer file.
//! Each kind passes the same cases through [`Gate`], and no code of the
//! workspace opens a resource past the gate.
//!
//! References:
//! - ADR-1205 D4, its notes of 2026-11-19: one gate for every kind of resource, a pointer theme activated by its folder, and the list of a fresh install when the batch file is missing.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use vtium_settings::Trust;
use vtium_settings::batch::{Batch, Entry, FRESH_FONTS, Places};
use vtium_settings::cullet::{Check, Gate, Rule};
use vtium_settings::mounts::{Mounts, Place, Unsteady};

/// A fresh fixture folder of its own for one test, mode 0755, by its
/// canonical path.
fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vtium-settings-gate")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the fixture folder");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("mode 0755");
    fs::canonicalize(&dir).expect("the fixture's canonical path")
}

fn mkdir(path: &Path, mode: u32) {
    fs::create_dir_all(path).expect("the folder");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("its mode");
}

fn write(path: &Path, mode: u32) {
    fs::write(path, b"x").expect("the file");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("its mode");
}

/// One kind of resource: what the batch lists, and the file vtium opens.
struct Kind {
    name: &'static str,
    /// What the batch lists: the file, or a pointer theme's folder.
    listed: PathBuf,
    /// The file vtium opens: `listed`, or a file inside the theme.
    opened: PathBuf,
    /// Whether the list of a fresh install holds it.
    fresh: bool,
}

impl Kind {
    /// Opens the kind's file through `gate`, the way the console reads
    /// that kind, and gives the check it failed.
    fn through(&self, gate: &Gate) -> Result<(), Check> {
        let opened = if self.listed == self.opened {
            gate.open(&self.opened)
        } else {
            gate.open_in_theme(&self.listed, &self.opened)
        };
        opened.map(drop).map_err(|r| r.check)
    }
}

/// Each kind laid out under `root`, in the folders a package and an
/// administrator keep them in: fonts, palettes, looks and pointers.
fn kinds(root: &Path) -> Vec<Kind> {
    let (fonts, palettes, looks, icons) = (
        root.join("fonts"),
        root.join("palettes"),
        root.join("looks"),
        root.join("icons/Plain/cursors"),
    );
    for d in [&fonts, &palettes, &looks, &icons] {
        mkdir(d, 0o755);
    }
    mkdir(&root.join("icons"), 0o755);
    mkdir(&root.join("icons/Plain"), 0o755);
    let font = fonts.join(FRESH_FONTS[0]);
    let scheme = palettes.join("kiln.palette");
    let cube = looks.join("warm.cube");
    let theme = root.join("icons/Plain");
    let picture = icons.join("default");
    let svg = root.join("pointers.svg");
    for file in [&font, &scheme, &cube, &picture, &svg] {
        write(file, 0o644);
    }
    let file = |name, path: &PathBuf, fresh| Kind {
        name,
        listed: path.clone(),
        opened: path.clone(),
        fresh,
    };
    vec![
        file("font", &font, true),
        file("color scheme", &scheme, true),
        file("color map", &cube, false),
        Kind {
            name: "pointer theme",
            listed: theme,
            opened: picture,
            fresh: false,
        },
        file("pointers.svg", &svg, true),
    ]
}

/// The places of a fresh install under `root`, with no batch file.
fn places(root: &Path) -> Places {
    Places {
        file: root.join("batch.conf"),
        vconsole_conf: PathBuf::from("/dev/null"),
        cmdline: PathBuf::from("/dev/null"),
        fonts: root.join("fonts"),
        palettes: root.join("palettes"),
        pointers: root.join("pointers.svg"),
        console_font_folders: Vec::new(),
    }
}

/// The running system's mount table, with the mount that holds `file`
/// named an NFS mount, which does not stay mounted.
fn nfs_for(file: &Path) -> Mounts {
    let real = Mounts::system().expect("the running system's mount table");
    let handle = fs::File::open(file).expect("the file");
    let place = Place::of(&handle, file).expect("where it sits");
    let mount = real.find(&place).expect("its mount").clone();
    let table = fs::read_to_string("/proc/self/mountinfo").expect("the mount table");
    let nfs: String = table
        .lines()
        .map(|l| match l.split_once(" - ") {
            Some((head, _)) if l.split(' ').next() == Some(&mount.id.to_string()) => {
                format!("{head} - nfs4 server:/export rw\n")
            }
            _ => format!("{l}\n"),
        })
        .collect();
    Mounts::parse(&nfs, Path::new("/sys"))
}

// Every kind passes the same cases through the same gate: listed and
// sound, it opens; outside the batch, it is not active; group-writable,
// reached through a symbolic link, or on a filesystem that does not stay
// mounted, it fails the trust rule; and with no batch file, the list of a
// fresh install holds the fonts, color schemes and pointer file the
// package ships, and neither a color map nor a pointer theme.
#[test]
fn every_kind_of_resource_passes_the_same_gate() {
    let root = fixture("kinds");
    let uid = fs::metadata(&root).expect("the fixture").uid();
    let rule = Rule::new(Trust::owner(uid)).below(&root);
    let system = Mounts::system().expect("the running system's mount table");
    let kinds = kinds(&root);
    let listing = |k: &Kind| Batch::new(vec![Entry::Path(k.listed.clone())]);
    let gate =
        |k: &Kind, mounts: &Mounts| Gate::new(rule.clone(), Some(mounts.clone()), listing(k));
    let fresh = Batch::fresh(&places(&root));
    assert!(!places(&root).file.exists(), "no batch file");
    for k in &kinds {
        assert_eq!(k.through(&gate(k, &system)), Ok(()), "{}: listed", k.name);
        let unlisted = Gate::new(rule.clone(), Some(system.clone()), Batch::default());
        assert_eq!(
            k.through(&unlisted),
            Err(Check::Inactive),
            "{}: outside the batch",
            k.name
        );
        let fresh_gate = Gate::new(rule.clone(), Some(system.clone()), fresh.clone());
        let want = if k.fresh {
            Ok(())
        } else {
            Err(Check::Inactive)
        };
        assert_eq!(k.through(&fresh_gate), want, "{}: no batch file", k.name);
        assert!(
            matches!(
                k.through(&gate(k, &nfs_for(&k.opened))),
                Err(Check::Unsteady(Unsteady::Network { .. }))
            ),
            "{}: an unsteady mount",
            k.name
        );
        // group-writable
        fs::set_permissions(
            &k.listed,
            fs::Permissions::from_mode(if k.listed.is_dir() { 0o775 } else { 0o664 }),
        )
        .expect("group-writable");
        assert!(
            matches!(k.through(&gate(k, &system)), Err(Check::Untrusted(_))),
            "{}: group-writable",
            k.name
        );
        fs::set_permissions(
            &k.listed,
            fs::Permissions::from_mode(if k.listed.is_dir() { 0o755 } else { 0o644 }),
        )
        .expect("its mode again");
        // a symbolic link that the batch lists, to the sound resource
        let link = root.join(format!("link-{}", k.name.replace(' ', "-")));
        symlink(&k.listed, &link).expect("the link");
        let linked = Kind {
            name: k.name,
            opened: link.join(k.opened.strip_prefix(&k.listed).expect("inside")),
            listed: link.clone(),
            fresh: k.fresh,
        };
        let linked = Kind {
            opened: if k.listed == k.opened {
                link.clone()
            } else {
                linked.opened
            },
            ..linked
        };
        assert_eq!(
            linked.through(&gate(&linked, &system)),
            Err(Check::Link { at: link }),
            "{}: a symbolic link",
            k.name
        );
    }
}

/// Every Rust file under `dir`, recursively, leaving out build output.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(list) = fs::read_dir(dir) else {
        return;
    };
    for entry in list.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

// No code opens a resource past the gate. The two functions that open a
// file under the trust rule alone, without the batch, are private to
// cullet.rs, and no other file of the workspace names them. vtium-config
// checks a resource before a person activates it through `cullet::check`,
// which reads nothing.
#[test]
fn gate_guard() {
    let workspace = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .expect("the workspace");
    // this test names the functions it looks for
    let guard = workspace.join("crates/vtium-settings/tests/gate.rs");
    let mut files = Vec::new();
    rust_files(&workspace.join("crates"), &mut files);
    rust_files(&workspace.join("tools"), &mut files);
    let cullet = workspace.join("crates/vtium-settings/src/cullet.rs");
    assert!(
        files.len() > 200,
        "the scan found {} Rust files",
        files.len()
    );
    assert!(files.contains(&cullet), "the scan reaches cullet.rs");
    let text = fs::read_to_string(&cullet).expect("cullet.rs");
    for private in ["\nfn open(", "\nfn open_in_theme("] {
        assert!(
            text.contains(private),
            "cullet.rs declares `{}` private",
            private.trim()
        );
    }
    let mut past = Vec::new();
    for file in files.iter().filter(|f| **f != cullet && **f != guard) {
        let text = fs::read_to_string(file).unwrap_or_default();
        let imports_open = text.lines().any(|l| {
            l.contains("cullet::{")
                && l.split(['{', ',', '}', ' '])
                    .any(|w| w == "open" || w == "open_in_theme")
        });
        if text.contains("cullet::open(") || text.contains("cullet::open_in_theme(") || imports_open
        {
            past.push(file.clone());
        }
    }
    assert_eq!(
        past,
        Vec::<PathBuf>::new(),
        "files that open a resource past the gate"
    );
}
