//! `cover-tables generate`: fetches each of Unicode's mapping files the
//! named `cover=` sets come from, once, with curl, into
//! `$XDG_CACHE_HOME/vtium/cover-tables`, checks each against the SHA-256
//! `provenance.tsv` pins for it, and writes vtium-vitrify's `src/cover.rs`
//! and `provenance.tsv`. `--repin` takes a file whose bytes moved and pins
//! its new SHA-256. `cover-tables check` writes nothing and fails when the
//! checked-in files differ from what the cached files generate.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use vtium_cover_tables::{Generated, SOURCES, codepoints, module, pinned, provenance, version};
use vtium_font::{Sha256, sha256_hex};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn cache() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("vtium/cover-tables")
}

/// The file at `url`, from the cache, fetched once into it.
fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let dir = cache();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = url.rsplit('/').next().unwrap_or("file");
    let path = dir.join(name);
    if !path.exists() {
        let part = dir.join(format!("{name}.part"));
        let status = Command::new("curl")
            .args(["-fsSL", "--retry", "3", "-o"])
            .arg(&part)
            .arg(url)
            .status()
            .map_err(|e| format!("curl: {e}"))?;
        if !status.success() {
            return Err(format!("curl {url}: {status}"));
        }
        std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}

fn run(write: bool, repin: bool) -> Result<bool, String> {
    let root = root();
    let module_path = root.join("crates/vtium-vitrify/src/cover.rs");
    let provenance_path = root.join("tools/cover-tables/provenance.tsv");
    let pins = std::fs::read_to_string(&provenance_path).unwrap_or_default();
    let mut sets = Vec::new();
    for source in &SOURCES {
        let bytes = fetch(source.url)?;
        let sha256 = sha256_hex(&Sha256::digest(&bytes));
        if let Some(pin) = pinned(&pins, source.name).filter(|&p| p != sha256) {
            if !repin {
                return Err(format!(
                    "{}: SHA-256 {sha256}, not the pinned {pin}; --repin takes it",
                    source.url
                ));
            }
        }
        let text = String::from_utf8_lossy(&bytes);
        let at = |e: String| format!("{}: {e}", source.url);
        sets.push(Generated {
            source,
            version: version(&text).map_err(at)?,
            sha256,
            codepoints: codepoints(&text, source.column).map_err(at)?,
        });
    }
    let files = [
        (module_path, module(&sets)),
        (provenance_path, provenance(&sets)),
    ];
    if write {
        for (path, text) in &files {
            std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("wrote {}", path.display());
        }
        return Ok(true);
    }
    let mut same = true;
    for (path, text) in &files {
        if std::fs::read_to_string(path).ok().as_deref() != Some(text.as_str()) {
            println!("{} differs from what the files generate", path.display());
            same = false;
        }
    }
    Ok(same)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["generate"] => run(true, false),
        ["generate", "--repin"] => run(true, true),
        ["check"] => run(false, false),
        _ => {
            eprintln!("usage: cover-tables generate [--repin] | check");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("cover-tables: {e}");
            ExitCode::FAILURE
        }
    }
}
