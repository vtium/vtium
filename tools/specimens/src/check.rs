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

/// What `check` finds, in two kinds: faults of the specimens, the goldens
/// that judge vtium's drawing, and faults of the simulated renders, which
/// show a panel's conversion and judge nothing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Faults {
    pub goldens: Vec<String>,
    pub simulations: Vec<String>,
}

impl Faults {
    pub fn is_empty(&self) -> bool {
        self.goldens.is_empty() && self.simulations.is_empty()
    }

    fn push(&mut self, name: &str, fault: String) {
        if simulated(name) {
            self.simulations.push(fault);
        } else {
            self.goldens.push(fault);
        }
    }
}

/// Each fault of `dir` against what `docs::files` draws: a file missing,
/// differing, unreadable, or there and not drawn, each in its kind.
pub fn check(dir: &Path) -> Result<Faults, String> {
    let files = docs::files(&docs::provisional(dir)?)?;
    let mut faults = Faults::default();
    for stray in strays(dir, &files)? {
        faults.push(&stray, format!("{stray}: not drawn by the profile table"));
    }
    for (name, want) in &files {
        let bytes = match std::fs::read(dir.join(name)) {
            Ok(b) => b,
            Err(e) => {
                faults.push(name, format!("{name}: {e}"));
                continue;
            }
        };
        let fault = match want {
            Content::Text(t) if t.as_bytes() != bytes => Some(format!(
                "{name}: differs from what the profile table writes"
            )),
            Content::Text(_) => None,
            Content::Png(img) => match png::decode(&bytes) {
                Err(e) => Some(format!("{name}: {e}")),
                Ok(got) if (got.width, got.height) != (img.width, img.height) => Some(format!(
                    "{name}: {}x{}, drawn {}x{}",
                    got.width, got.height, img.width, img.height
                )),
                Ok(got) => {
                    let differ = got
                        .rgb
                        .chunks(3)
                        .zip(img.rgb.chunks(3))
                        .filter(|(a, b)| a != b)
                        .count();
                    (differ > 0).then(|| format!("{name}: {differ} pixels differ from the drawing"))
                }
            },
        };
        if let Some(f) = fault {
            faults.push(name, f);
        }
    }
    faults.goldens.sort();
    faults.simulations.sort();
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

/// Whether `name` is one of the simulated renders' files.
fn simulated(name: &str) -> bool {
    Path::new(name).starts_with(docs::SIMULATED)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        // the entry's own type, so a symbolic link is listed and never
        // followed: a link to a directory above cannot loop the walk
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh copy of the specimens in a directory of this test's own.
    fn drawn(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vtium-specimens-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let here =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specimens/provisional.tsv");
        std::fs::copy(here, dir.join("provisional.tsv")).unwrap();
        write(&dir).unwrap();
        dir
    }

    // A simulated render that is stale, missing or stray is a fault of its
    // own kind, apart from the goldens that judge vtium's drawing.
    #[test]
    fn a_stale_or_stray_simulation_is_its_own_fault() {
        let dir = drawn("sim");
        assert_eq!(check(&dir).unwrap(), Faults::default());
        let sim = dir.join("simulated/320x240-gray4");
        let png = sim.join("screen-1.png");
        let mut img = png::decode(&std::fs::read(&png).unwrap()).unwrap();
        img.rgb[0] ^= 0xff;
        std::fs::write(&png, png::encode(&img)).unwrap();
        std::fs::write(sim.join("extra.png"), b"").unwrap();
        let faults = check(&dir).unwrap();
        assert!(faults.goldens.is_empty(), "{:?}", faults.goldens);
        assert_eq!(
            faults.simulations,
            [
                "simulated/320x240-gray4/extra.png: not drawn by the profile table",
                "simulated/320x240-gray4/screen-1.png: 1 pixels differ from the drawing",
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // A directory symbolic link back up the tree is listed, not walked.
    #[test]
    fn a_symlink_loop_is_not_walked() {
        let dir = drawn("loop");
        std::os::unix::fs::symlink(&dir, dir.join("320x240/loop")).unwrap();
        let faults = check(&dir).unwrap();
        assert_eq!(
            faults.goldens,
            ["320x240/loop: not drawn by the profile table"]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
