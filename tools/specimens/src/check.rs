//! Writing the specimens, and judging what is committed against them.

use std::path::Path;

use crate::docs::{self, Content, Drawn};
use crate::draw::Sources;
use crate::png;
use crate::profiles::{PROFILES, Profile};

/// Writes every file `docs::files` draws under `dir`, and removes each
/// file there it does not draw but `provisional.tsv` and a skipped
/// profile's images. Returns the files written and the profiles skipped,
/// each with why.
pub fn write(dir: &Path, sources: &Sources) -> Result<Drawn, String> {
    let drawn = docs::files(&docs::provisional(dir)?, sources)?;
    for stray in strays(dir, &drawn)? {
        std::fs::remove_file(dir.join(&stray)).map_err(|e| format!("{stray}: {e}"))?;
    }
    for (name, content) in &drawn.files {
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
    Ok(drawn)
}

/// What `check` finds. A difference in an accepted profile fails, in
/// `goldens` or, for a simulated render, in `simulations`, kept apart from
/// the goldens that judge vtium's drawing. A difference in a provisional
/// profile is `drift`: reported, failing nothing (the operator,
/// 2026-10-28). A profile whose screens could not be drawn here is
/// `skipped`, with why.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Faults {
    pub goldens: Vec<String>,
    pub simulations: Vec<String>,
    pub drift: Vec<String>,
    pub skipped: Vec<String>,
}

impl Faults {
    /// Whether the check fails.
    pub fn failed(&self) -> bool {
        !self.goldens.is_empty() || !self.simulations.is_empty()
    }

    fn push(&mut self, name: &str, provisional: &[String], fault: String) {
        let provisional = owner(name).is_some_and(|p| provisional.iter().any(|n| n == p.name));
        if provisional {
            self.drift.push(fault);
        } else if simulated(name) {
            self.simulations.push(fault);
        } else {
            self.goldens.push(fault);
        }
    }
}

/// The profile whose directory holds `name`.
fn owner(name: &str) -> Option<&'static Profile> {
    PROFILES
        .iter()
        .find(|p| Path::new(name).starts_with(docs::dir(p)))
}

/// Each fault of `dir` against what `docs::files` draws: a file missing,
/// differing, unreadable, or there and not drawn, each in its kind.
pub fn check(dir: &Path, sources: &Sources) -> Result<Faults, String> {
    let provisional = docs::provisional(dir)?;
    let drawn = docs::files(&provisional, sources)?;
    let mut faults = Faults::default();
    // a skip never hides an accepted profile's regression: only a
    // provisional profile may go undrawn
    for (p, why) in &drawn.skipped {
        if provisional.iter().any(|n| n == p.name) {
            faults.skipped.push(format!("{}: {why}", p.name));
        } else {
            faults
                .goldens
                .push(format!("{}: accepted, and not drawn here: {why}", p.name));
        }
    }
    for stray in strays(dir, &drawn)? {
        faults.push(
            &stray,
            &provisional,
            format!("{stray}: not drawn by the profile table"),
        );
    }
    for (name, want) in &drawn.files {
        let bytes = match std::fs::read(dir.join(name)) {
            Ok(b) => b,
            Err(e) => {
                faults.push(name, &provisional, format!("{name}: {e}"));
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
            faults.push(name, &provisional, f);
        }
    }
    faults.goldens.sort();
    faults.simulations.sort();
    faults.drift.sort();
    Ok(faults)
}

/// The files under `dir` that `drawn` does not hold, but `provisional.tsv`
/// and the screens and collage of a profile skipped here, which are
/// neither drawn nor judged.
fn strays(dir: &Path, drawn: &Drawn) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    walk(dir, dir, &mut found)?;
    let skipped_image = |f: &str| {
        drawn.skipped.iter().any(|(p, _)| {
            let d = docs::dir(p);
            p.layout().is_ok_and(|l| {
                docs::image_names(l.pages)
                    .iter()
                    .any(|n| f == format!("{d}/{n}"))
            })
        })
    };
    Ok(found
        .into_iter()
        .filter(|f| f != "provisional.tsv" && !drawn.files.contains_key(f) && !skipped_image(f))
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

    /// The sources a test draws with: no desktop set, so the outline
    /// profiles are skipped.
    fn sources() -> Sources {
        Sources {
            desktop: Err("not built in this test".to_owned()),
        }
    }

    /// The outline profiles as provisional, so a test that does not draw
    /// them may skip them.
    const OUTLINE: &str = "1920x1080\tnew\n2560x1440\tnew\n";

    /// A fresh drawing of the specimens in a directory of this test's own,
    /// with `provisional` as its provisional.tsv.
    fn drawn(name: &str, provisional: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vtium-specimens-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("provisional.tsv"), provisional).unwrap();
        write(&dir, &sources()).unwrap();
        dir
    }

    /// Flips one pixel of the PNG at `path`.
    fn plant(path: &Path) {
        let mut img = png::decode(&std::fs::read(path).unwrap()).unwrap();
        img.rgb[0] ^= 0xff;
        std::fs::write(path, png::encode(&img)).unwrap();
    }

    // A simulated render that is stale, missing or stray is a fault of its
    // own kind, apart from the goldens that judge vtium's drawing.
    #[test]
    fn a_stale_or_stray_simulation_is_its_own_fault() {
        let dir = drawn("sim", OUTLINE);
        let clean = check(&dir, &sources()).unwrap();
        assert!(!clean.failed(), "{clean:?}");
        let sim = dir.join("simulated/320x240-gray4");
        plant(&sim.join("screen-1.png"));
        std::fs::write(sim.join("extra.png"), b"").unwrap();
        let faults = check(&dir, &sources()).unwrap();
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

    // A moved pixel fails an accepted profile, and is drift in a
    // provisional one, which fails nothing (the operator, 2026-10-28).
    #[test]
    fn a_provisional_profile_drifts_and_an_accepted_one_fails() {
        let dir = drawn("drift", &format!("{OUTLINE}640x480\tunder review\n"));
        plant(&dir.join("640x480/screen-1.png"));
        plant(&dir.join("800x480/screen-1.png"));
        let faults = check(&dir, &sources()).unwrap();
        assert_eq!(
            faults.drift,
            ["640x480/screen-1.png: 1 pixels differ from the drawing"]
        );
        assert_eq!(
            faults.goldens,
            ["800x480/screen-1.png: 1 pixels differ from the drawing"]
        );
        assert!(faults.failed());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // A provisional profile whose set is not at hand is skipped and says
    // why; its committed screens and collage are neither judged nor
    // strays, any other file there is, and its text is judged still.
    #[test]
    fn a_provisional_profile_not_drawn_here_is_skipped_with_why() {
        let dir = drawn("skip", OUTLINE);
        std::fs::write(dir.join("1920x1080/screen-1.png"), b"anything").unwrap();
        std::fs::write(dir.join("1920x1080/collage.png"), b"anything").unwrap();
        std::fs::write(dir.join("1920x1080/extra.png"), b"anything").unwrap();
        std::fs::write(dir.join("1920x1080/screen-3.png"), b"anything").unwrap();
        let faults = check(&dir, &sources()).unwrap();
        assert_eq!(
            faults.skipped,
            [
                "1920x1080: not built in this test",
                "2560x1440: not built in this test"
            ]
        );
        assert_eq!(
            faults.drift,
            [
                "1920x1080/extra.png: not drawn by the profile table",
                "1920x1080/screen-3.png: not drawn by the profile table",
            ]
        );
        assert!(!faults.failed(), "{faults:?}");
        std::fs::write(dir.join("1920x1080/vtium.conf"), b"").unwrap();
        let faults = check(&dir, &sources()).unwrap();
        assert!(
            faults.drift.contains(
                &"1920x1080/vtium.conf: differs from what the profile table writes".to_owned()
            ),
            "{:?}",
            faults.drift
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // An accepted profile that cannot be drawn here fails with why: a skip
    // never hides an accepted profile's regression.
    #[test]
    fn an_accepted_profile_not_drawn_here_fails() {
        let dir = drawn("skip-accepted", "2560x1440\tnew\n");
        let faults = check(&dir, &sources()).unwrap();
        assert_eq!(faults.skipped, ["2560x1440: not built in this test"]);
        assert_eq!(
            faults.goldens,
            ["1920x1080: accepted, and not drawn here: not built in this test"]
        );
        assert!(faults.failed());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // A directory symbolic link back up the tree is listed, not walked.
    #[test]
    fn a_symlink_loop_is_not_walked() {
        let dir = drawn("loop", OUTLINE);
        std::os::unix::fs::symlink(&dir, dir.join("320x240/loop")).unwrap();
        let faults = check(&dir, &sources()).unwrap();
        assert_eq!(
            faults.goldens,
            ["320x240/loop: not drawn by the profile table"]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
