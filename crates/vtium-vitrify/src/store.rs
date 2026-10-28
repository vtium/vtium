//! Writing the store (ADR-1200 D3): `<sha256>.glass` sets, the manifest and
//! a lock file in one directory. Every write holds the lock; a set goes in
//! as a temporary file synced and renamed, then the manifest the same way,
//! then the directory is synced and the set files no row names are
//! removed. The store is checked with vtium-font's [`Store`] each time it
//! is opened, as the console checks it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use rustix::fs::{FlockOperation, flock};
use vtium_font::set::{Manifest, ManifestRow, Store, StoreError};

/// A store opened to write, its lock held until it drops.
pub struct StoreWriter {
    store: Store,
    _lock: File,
}

/// The store's manifest, or an empty one when the store has none yet.
pub fn manifest(store: &Store) -> Result<Manifest, StoreError> {
    match store.manifest() {
        Err(StoreError::Io { error, .. }) if error.kind() == io::ErrorKind::NotFound => {
            Ok(Manifest::default())
        }
        other => other,
    }
}

/// Writes `bytes` to `path` through a temporary file beside it, synced and
/// renamed over it, then syncs the directory.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        File::open(dir)?.sync_all()
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

impl StoreWriter {
    /// Opens the store at `path` for writing as `uid`, the owner the check
    /// expects: the directory made with mode 0755 when missing, checked,
    /// and its lock taken.
    pub fn open(path: &Path, uid: u32) -> Result<StoreWriter, String> {
        if !path.exists() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o755)
                .create(path)
                .map_err(|e| format!("cannot make the store {}: {e}", path.display()))?;
        }
        let store = Store::open(path, uid).map_err(|e| e.to_string())?;
        let lock_path = path.join("lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o644)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&lock_path)
            .map_err(|e| format!("{}: {e}", lock_path.display()))?;
        flock(&lock, FlockOperation::LockExclusive)
            .map_err(|e| format!("cannot lock {}: {e}", lock_path.display()))?;
        Ok(StoreWriter { store, _lock: lock })
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn path(&self) -> &Path {
        self.store.path()
    }

    pub fn manifest(&self) -> Result<Manifest, String> {
        manifest(&self.store).map_err(|e| e.to_string())
    }

    /// Adds the set `bytes` under `row`, replacing any set of that name.
    pub fn put(&self, row: ManifestRow, bytes: &[u8]) -> Result<(), String> {
        let path = self.path().join(row.file_name());
        write_atomic(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut rows: Vec<ManifestRow> = self
            .manifest()?
            .rows()
            .iter()
            .filter(|r| r.name != row.name)
            .cloned()
            .collect();
        rows.push(row);
        self.commit(rows)
    }

    /// Drops the set named `name`; `false` when the store holds none.
    pub fn remove(&self, name: &str) -> Result<bool, String> {
        let manifest = self.manifest()?;
        if manifest.row(name).is_none() {
            return Ok(false);
        }
        let rows = manifest
            .rows()
            .iter()
            .filter(|r| r.name != name)
            .cloned()
            .collect();
        self.commit(rows)?;
        Ok(true)
    }

    /// Writes the manifest of `rows`, then removes the set files no row
    /// names.
    fn commit(&self, rows: Vec<ManifestRow>) -> Result<(), String> {
        let manifest = Manifest::new(rows).map_err(|e| e.to_string())?;
        let path = self.path().join("manifest");
        write_atomic(&path, manifest.to_tsv().as_bytes())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let keep: Vec<String> = manifest.rows().iter().map(ManifestRow::file_name).collect();
        let entries = fs::read_dir(self.path()).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_set_file(&name) && !keep.contains(&name) {
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

/// Whether `name` is a set file's: 64 lower-case hex digits and `.glass`.
fn is_set_file(name: &str) -> bool {
    name.strip_suffix(".glass").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// The default store, set at build time with `VTIUM_FONT_STORE`.
pub fn default_store() -> PathBuf {
    PathBuf::from(option_env!("VTIUM_FONT_STORE").unwrap_or("/var/lib/vtium/fonts"))
}

/// The default recipes' directory, set at build time with
/// `VTIUM_FONT_RECIPES`.
pub fn default_recipes() -> PathBuf {
    PathBuf::from(option_env!("VTIUM_FONT_RECIPES").unwrap_or("/etc/vtium/fonts"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_set_files_are_named_as_sets() {
        assert!(is_set_file(&format!("{}.glass", "a".repeat(64))));
        for name in [
            "manifest",
            "lock",
            "x.glass",
            &format!("{}.glass", "A".repeat(64)),
        ] {
            assert!(!is_set_file(name), "{name}");
        }
    }
}
