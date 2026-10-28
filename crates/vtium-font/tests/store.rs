//! The store's check (ADR-1200 D3, D10): the directory, the manifest and
//! each set opened without following a link, owned by the expected owner
//! and writable by no one else, the set's SHA-256 matched to its row, and
//! each chunk to its own hash as it is read. The expected owner is the
//! invoking user here, as the store takes it for a parameter; the console
//! passes root.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use vtium_font::set::{
    Manifest, ManifestRow, PixelFormat, SetBuilder, SetError, SetFace, Store, StoreError,
    StrikeBuilder,
};
use vtium_font::{Cell, Face, Sha256, Style, Width};

/// Where the tests' stores go: Cargo's temporary directory, unless one of
/// its parents is writable by others, as `/tmp` is when the target
/// directory is under it, which the store's check refuses; then the
/// workspace's own `target/tmp`.
fn base() -> PathBuf {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let kept = |p: &Path| {
        p.ancestors()
            .all(|a| fs::metadata(a).is_ok_and(|m| m.mode() & 0o022 == 0))
    };
    if kept(&tmp) {
        return tmp;
    }
    let workspace = fs::canonicalize(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .expect("the workspace")
        .join("target/tmp");
    fs::create_dir_all(&workspace).expect("create target/tmp");
    workspace
}

/// A fresh store directory for `test`, mode 0755, whose parents the
/// invoking user or root owns.
fn store_dir(test: &str) -> PathBuf {
    let dir = base().join("vtium-font-store").join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the store");
    for d in [dir.as_path(), dir.parent().expect("a parent")] {
        fs::set_permissions(d, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    dir
}

fn uid(dir: &Path) -> u32 {
    fs::metadata(dir).expect("stat").uid()
}

/// A set of `n` R8 glyphs, 64 to a 16 KiB chunk.
fn set_bytes(name: &str, n: u32) -> Vec<u8> {
    let cell = Cell {
        width: 16,
        height: 16,
    };
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(false);
    for c in (0x4e00..0x4e00 + n).filter_map(char::from_u32) {
        let g = s.glyph(Width::Narrow, vec![(u32::from(c) % 251) as u8; 256]);
        s.map(c, g);
    }
    SetBuilder::new(name).strike(s).write().expect("writes")
}

fn row(name: &str, bytes: &[u8]) -> ManifestRow {
    ManifestRow {
        name: name.to_owned(),
        sha256: Sha256::digest(bytes),
        build_key: Sha256::digest(name.as_bytes()),
        version: (1, 0),
        cells: vec![Cell {
            width: 16,
            height: 16,
        }],
        source: format!("file:/fonts/{name}.psf"),
    }
}

/// Writes `file` with `bytes` at mode 0644.
fn write(file: &Path, bytes: &[u8]) {
    fs::write(file, bytes).expect("write");
    fs::set_permissions(file, fs::Permissions::from_mode(0o644)).expect("chmod");
}

/// A store holding one set, `name`, of `n` glyphs; its directory and the
/// set's path.
fn store_with(test: &str, name: &str, n: u32) -> (PathBuf, PathBuf) {
    let dir = store_dir(test);
    let bytes = set_bytes(name, n);
    let row = row(name, &bytes);
    let path = dir.join(row.file_name());
    write(&path, &bytes);
    let manifest = Manifest::new(vec![row]).expect("a manifest");
    write(&dir.join("manifest"), manifest.to_tsv().as_bytes());
    (dir, path)
}

// A set in a well-kept store loads, draws, and holds only the chunks drawn
#[test]
fn a_set_loads_from_the_store() {
    let (dir, _) = store_with("loads", "cjk", 200);
    let store = Store::open(&dir, uid(&dir)).expect("the store opens");
    let set = store.load("cjk").expect("the set loads");
    assert_eq!(set.name(), "cjk");
    let face = SetFace::new(set.clone(), 16).expect("a face");
    let g = face.draw(&['\u{4e01}'], Style::default()).expect("draws");
    assert_eq!(g.pixels()[0], (0x4e01 % 251) as u8);
    assert_eq!(set.loaded_chunks(), 1);
    assert!(matches!(store.load("other"), Err(StoreError::NoSuchSet(_))));
}

// The whole file's SHA-256 must be its row's (ADR-1200 D3, D10)
#[test]
fn a_whole_file_mismatch_is_refused() {
    let (dir, path) = store_with("file-hash", "cjk", 100);
    let mut bytes = fs::read(&path).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    write(&path, &bytes);
    let store = Store::open(&dir, uid(&dir)).expect("opens");
    let err = store.load("cjk").expect_err("refused");
    assert!(
        matches!(
            err,
            StoreError::Set {
                error: SetError::FileHash { .. },
                ..
            }
        ),
        "{err}"
    );
}

// A file changed after the check is caught chunk by chunk (ADR-1200 D3)
#[test]
fn a_chunk_changed_after_the_check_is_refused() {
    let (dir, path) = store_with("chunk-hash", "cjk", 200);
    let store = Store::open(&dir, uid(&dir)).expect("opens");
    let set = store.load("cjk").expect("loads");
    // the same file, its last chunk's last byte changed in place
    let mut bytes = fs::read(&path).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, &bytes))
        .expect("rewrite in place");
    let face = SetFace::new(set.clone(), 16).expect("a face");
    assert!(face.draw(&['\u{4e00}'], Style::default()).is_some());
    assert_eq!(face.draw(&['\u{4ec7}'], Style::default()), None);
    assert!(matches!(set.fault(), Some(SetError::ChunkHash(3))));
}

// The store, its manifest and its sets must be the expected owner's
#[test]
fn a_wrong_owner_is_refused() {
    let (dir, _) = store_with("owner", "cjk", 1);
    let other = uid(&dir) + 1;
    let err = Store::open(&dir, other).expect_err("refused");
    assert!(
        matches!(&err, StoreError::Owner { path, .. } if path == &dir),
        "{err}"
    );
}

// Nothing in the store may be writable by group or others
#[test]
fn a_writable_mode_is_refused() {
    for (what, mode) in [
        ("store", 0o775),
        ("store", 0o757),
        ("manifest", 0o664),
        ("set", 0o666),
    ] {
        let (dir, set) = store_with(&format!("mode-{what}-{mode:o}"), "cjk", 1);
        let target = match what {
            "store" => dir.clone(),
            "manifest" => dir.join("manifest"),
            _ => set,
        };
        fs::set_permissions(&target, fs::Permissions::from_mode(mode)).expect("chmod");
        let err = Store::open(&dir, uid(&dir)).and_then(|s| s.load("cjk").map(drop));
        assert!(
            matches!(&err, Err(StoreError::Writable { path, .. }) if path == &target),
            "{what} {mode:o}: {err:?}"
        );
    }
}

// A parent of the store writable by others is refused
#[test]
fn a_writable_parent_is_refused() {
    let dir = store_dir("parent").join("inner");
    fs::create_dir(&dir).expect("mkdir");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod");
    let parent = dir.parent().expect("a parent").to_owned();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).expect("chmod");
    let err = Store::open(&dir, uid(&dir)).expect_err("refused");
    assert!(
        matches!(&err, StoreError::Writable { path, .. } if path == &parent),
        "{err}"
    );
}

// No symbolic link is followed: not the store, the manifest or a set
#[test]
fn a_symlink_is_refused() {
    let (dir, _) = store_with("symlink", "cjk", 1);
    let link = dir.with_file_name("symlink-link");
    let _ = fs::remove_file(&link);
    symlink(&dir, &link).expect("symlink");
    let err = Store::open(&link, uid(&dir)).expect_err("refused");
    assert!(
        matches!(&err, StoreError::Symlink(p) if p == &link),
        "{err}"
    );

    for name in ["manifest", "set"] {
        let (dir, set) = store_with(&format!("symlink-{name}"), "cjk", 1);
        let target = if name == "manifest" {
            dir.join("manifest")
        } else {
            set
        };
        let real = dir.join("real");
        fs::rename(&target, &real).expect("rename");
        symlink(&real, &target).expect("symlink");
        let store = Store::open(&dir, uid(&dir)).expect("opens");
        let err = store.load("cjk").expect_err("refused");
        assert!(
            matches!(&err, StoreError::Symlink(p) if p == &target),
            "{name}: {err}"
        );
    }
}

// A set that is not a regular file is refused, a FIFO without blocking
#[test]
fn a_set_that_is_not_a_file_is_refused() {
    let (dir, set) = store_with("not-regular", "cjk", 1);
    fs::remove_file(&set).expect("rm");
    fs::create_dir(&set).expect("mkdir");
    let store = Store::open(&dir, uid(&dir)).expect("opens");
    let err = store.load("cjk").expect_err("refused");
    assert!(
        matches!(&err, StoreError::NotRegular(p) if p == &set),
        "{err}"
    );
    fs::remove_dir(&set).expect("rmdir");
    let status = std::process::Command::new("mkfifo")
        .arg(&set)
        .status()
        .expect("mkfifo");
    assert!(status.success());
    let err = store.load("cjk").expect_err("refused");
    assert!(
        matches!(&err, StoreError::NotRegular(p) if p == &set),
        "{err}"
    );
}

// A set larger than SET_BYTES_MAX is refused before it is read
#[test]
fn a_set_past_its_bound_is_refused() {
    let dir = store_dir("too-large");
    let mut row = row("big", b"");
    row.sha256 = [7; 32];
    let path = dir.join(row.file_name());
    let f = fs::File::create(&path).expect("create");
    f.set_len(u64::from(vtium_font::SET_BYTES_MAX) + 1)
        .expect("a sparse file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
    write(
        &dir.join("manifest"),
        Manifest::new(vec![row])
            .expect("a manifest")
            .to_tsv()
            .as_bytes(),
    );
    let store = Store::open(&dir, uid(&dir)).expect("opens");
    let err = store.load("big").expect_err("refused");
    assert!(
        matches!(
            err,
            StoreError::Set {
                error: SetError::Bound("SET_BYTES_MAX"),
                ..
            }
        ),
        "{err}"
    );
}

// The manifest reads back what it wrote, sorted, and refuses each
// malformed line and its bounds
#[test]
fn the_manifest_reads_back_and_refuses_what_is_malformed() {
    let a = row("b-set", b"one");
    let mut b = row("a-set", b"two");
    b.cells.push(Cell {
        width: 32,
        height: 32,
    });
    let m = Manifest::new(vec![a.clone(), b.clone()]).expect("a manifest");
    let text = m.to_tsv();
    assert!(text.starts_with(
        "# vtium-vitrify manifest 1\nname\tsha256\tbuild-key\tformat\tcells\tsource\na-set\t"
    ));
    assert!(text.contains("\t1.0\t16x16,32x32\tfile:/fonts/a-set.psf\n"));
    let back = Manifest::parse(&text).expect("reads back");
    assert_eq!(back.rows(), [b.clone(), a.clone()]);
    assert_eq!(back.row("b-set"), Some(&a));
    assert_eq!(back.row("c-set"), None);

    let bad = |text: &str| Manifest::parse(text).expect_err("refused");
    let rows: Vec<&str> = text.lines().collect();
    let swapped = format!("{}\n{}\n{}\n{}\n", rows[0], rows[1], rows[3], rows[2]);
    assert_eq!(bad(&swapped).line, 4);
    assert_eq!(
        bad(&text.replace("# vtium-vitrify manifest 1", "# vtium-vitrify manifest 2")).line,
        1
    );
    assert_eq!(bad(&text.replace("16x16,32x32", "16x16,0x32")).line, 3);
    assert_eq!(bad(&text.replace("\t1.0\t", "\t1.x\t")).line, 3);
    assert_eq!(
        bad(&text.replacen(&vtium_font::sha256_hex(&b.sha256), "AB", 1)).line,
        3
    );
    assert_eq!(bad(&text.replace("a-set", ".a-set")).line, 3);
    assert_eq!(bad(text.trim_end()).line, 0);

    let many: Vec<ManifestRow> = (0..4097).map(|i| row(&format!("s{i:04}"), b"x")).collect();
    let mut long = String::from(
        "# vtium-vitrify manifest 1\nname\tsha256\tbuild-key\tformat\tcells\tsource\n",
    );
    let mut sorted = many.clone();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for r in &sorted {
        let one = Manifest::new(vec![r.clone()]).expect("a row").to_tsv();
        long += one.lines().nth(2).expect("the row");
        long += "\n";
    }
    assert!(bad(&long).why.contains("MANIFEST_ROWS_MAX"));
    assert!(Manifest::new(many[..4096].to_vec()).is_ok());
    let huge = format!("{text}{}", "#".repeat(1 << 20));
    assert!(bad(&huge).why.contains("MANIFEST_BYTES_MAX"));
}

// The path must be absolute, and a store that is a file is refused
#[test]
fn the_store_must_be_an_absolute_directory() {
    assert!(matches!(
        Store::open("relative/store", 0),
        Err(StoreError::Relative(_))
    ));
    let (dir, _) = store_with("not-dir", "cjk", 1);
    let file = dir.join("manifest");
    assert!(matches!(
        Store::open(&file, uid(&dir)),
        Err(StoreError::NotDirectory(_))
    ));
}
