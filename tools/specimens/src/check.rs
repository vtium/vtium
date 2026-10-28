//! Writing the specimens, and judging what is committed against them.

use std::collections::BTreeMap;
use std::path::Path;

use crate::docs::{self, Content};
use crate::png;

/// Writes every file `docs::files` draws under `dir`, and removes each
/// file there it does not draw but `provisional.tsv`.
pub fn write(dir: &Path) -> Result<Vec<String>, String> {
    let files = docs::files(&docs::provisional(dir)?)?;
    for stray in strays(dir, &files)? {
        std::fs::remove_file(dir.join(&stray)).map_err(|e| format!("{stray}: {e}"))?;
    }
    for (name, content) in &files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let bytes = match content {
            Content::Png(img) => png::encode(img),
            Content::Text(t) => t.clone().into_bytes(),
        };
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(files.into_keys().collect())
}

/// Each fault of `dir` against what `docs::files` draws: a file missing,
/// differing, unreadable, or there and not drawn. Empty when every file
/// matches.
pub fn check(dir: &Path) -> Result<Vec<String>, String> {
    let files = docs::files(&docs::provisional(dir)?)?;
    let mut faults: Vec<String> = strays(dir, &files)?
        .into_iter()
        .map(|s| format!("{s}: not drawn by the profile table"))
        .collect();
    for (name, want) in &files {
        let bytes = match std::fs::read(dir.join(name)) {
            Ok(b) => b,
            Err(e) => {
                faults.push(format!("{name}: {e}"));
                continue;
            }
        };
        match want {
            Content::Text(t) if t.as_bytes() != bytes => {
                faults.push(format!(
                    "{name}: differs from what the profile table writes"
                ));
            }
            Content::Png(img) => match png::decode(&bytes) {
                Err(e) => faults.push(format!("{name}: {e}")),
                Ok(got) if (got.width, got.height) != (img.width, img.height) => {
                    faults.push(format!(
                        "{name}: {}x{}, drawn {}x{}",
                        got.width, got.height, img.width, img.height
                    ))
                }
                Ok(got) => {
                    let differ = got
                        .rgb
                        .chunks(3)
                        .zip(img.rgb.chunks(3))
                        .filter(|(a, b)| a != b)
                        .count();
                    if differ > 0 {
                        faults.push(format!("{name}: {differ} pixels differ from the drawing"));
                    }
                }
            },
            Content::Text(_) => {}
        }
    }
    Ok(faults)
}

/// The files under `dir` that `files` does not hold, but `provisional.tsv`.
fn strays(dir: &Path, files: &BTreeMap<String, Content>) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    walk(dir, dir, &mut found)?;
    Ok(found
        .into_iter()
        .filter(|f| f != "provisional.tsv" && !files.contains_key(f))
        .collect())
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().into_owned());
        }
    }
    Ok(())
}
