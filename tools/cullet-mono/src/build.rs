//! `cullet-mono build`: fetches the pinned sources, patches each face, and
//! writes the faces, their notices and a record of the build.
//!
//! The build is reproducible: two builds from the same sources with the same
//! FontForge give the same bytes. FontForge writes the time of the build
//! into each face's `head` table unless `SOURCE_DATE_EPOCH` names a date,
//! and the patcher appends the output of `git describe` to its version when
//! it runs inside a git checkout, which the build's output folder usually
//! is. So the build sets `SOURCE_DATE_EPOCH`, points `GIT_DIR` at a path
//! that does not exist, and fixes Python's string hashing with
//! `PYTHONHASHSEED`, so that no set the patcher walks can change order.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::check;
use crate::pins::{Pin, Pins};
use crate::recipe;
use crate::sha256::{Sha256, hex};

/// The cache the pinned sources are fetched into:
/// `$XDG_CACHE_HOME/vtium/cullet-mono`.
pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("vtium/cullet-mono")
}

/// The folder in the tree whose files the build copies beside the faces.
fn notices_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("notices")
}

fn io(path: &Path, e: std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Fetches `pin` into `cache` with curl unless it is already there, and
/// checks its SHA-256. A file that does not match is removed, so
/// the next build fetches it again, and the build stops.
pub fn fetch(cache: &Path, pin: &Pin) -> Result<PathBuf, String> {
    fs::create_dir_all(cache).map_err(|e| io(cache, e))?;
    let path = cache.join(&pin.file);
    if !path.exists() {
        let part = cache.join(format!("{}.part", pin.file));
        println!("Fetching {}", pin.from);
        let status = Command::new("curl")
            .args(["-fsSL", "--retry", "3", "-o"])
            .arg(&part)
            .arg(&pin.from)
            .status()
            .map_err(|e| format!("cannot run curl: {e}"))?;
        if !status.success() {
            let _ = fs::remove_file(&part);
            return Err(format!("curl could not fetch {}: {status}", pin.from));
        }
        fs::rename(&part, &path).map_err(|e| io(&path, e))?;
    }
    let bytes = fs::read(&path).map_err(|e| io(&path, e))?;
    let sha256 = hex(&Sha256::digest(&bytes));
    if sha256 != pin.sha256 {
        let _ = fs::remove_file(&path);
        return Err(format!(
            "{} has SHA-256 {sha256}, not {} as pins.tsv pins it, so the build refuses it and removed it from {}",
            pin.file,
            pin.sha256,
            cache.display()
        ));
    }
    Ok(path)
}

/// Unpacks the zip archive `zip` into `into` with Python's zipfile module,
/// which keeps every file inside `into`.
fn unzip(zip: &Path, into: &Path) -> Result<(), String> {
    let status = Command::new("python3")
        .args(["-I", "-m", "zipfile", "-e"])
        .arg(zip)
        .arg(into)
        .status()
        .map_err(|e| format!("cannot run python3: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "python3 could not unpack {}: {status}",
            zip.display()
        ))
    }
}

/// FontForge's version, from the line `fontforge VERSION` that
/// `fontforge --version` prints.
fn fontforge_version() -> Result<String, String> {
    let out = Command::new("fontforge")
        .arg("--version")
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run fontforge, which the build needs: {e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("fontforge ").map(|v| v.trim().to_owned()))
        .ok_or_else(|| "fontforge --version printed no line \"fontforge VERSION\"".to_owned())
}

/// The glyph sets that a patcher log reports adding, in the order of the
/// log, each named once.
pub fn icon_sets(log: &str) -> Vec<String> {
    let mut sets: Vec<String> = Vec::new();
    for line in log.lines() {
        let Some(rest) = line.trim().strip_prefix("Adding ") else {
            continue;
        };
        let Some((_, name)) = rest.split_once(" Glyphs from ") else {
            continue;
        };
        let name = name.strip_suffix(" Set").unwrap_or(name).to_owned();
        if !sets.contains(&name) {
            sets.push(name);
        }
    }
    sets
}

/// The line in which the patcher names its own version and FontForge's.
fn patcher_line(log: &str) -> Option<&str> {
    log.lines()
        .find(|l| l.starts_with("Nerd Fonts Patcher "))
        .map(str::trim)
}

/// Patches one face in `work`, writes the patcher's log into `logs`, and
/// moves the face into `out`.
fn patch(
    face: &recipe::Face,
    patcher: &Path,
    terminus: &Path,
    work: &Path,
    logs: &Path,
    out: &Path,
) -> Result<String, String> {
    let into = work.join("out");
    let log_path = logs.join(face.output.replace(".ttf", ".log"));
    let log = fs::File::create(&log_path).map_err(|e| io(&log_path, e))?;
    let err = log.try_clone().map_err(|e| io(&log_path, e))?;
    println!("Patching {} as {}", face.source, face.output);
    let status = Command::new("fontforge")
        .arg("--script")
        .arg(patcher)
        .args(recipe::OPTIONS)
        .args(["--name", face.name, "--no-progressbars", "--outputdir"])
        .arg(&into)
        .arg(terminus.join(face.source))
        .current_dir(work)
        .env("SOURCE_DATE_EPOCH", recipe::SOURCE_DATE_EPOCH.to_string())
        .env("GIT_DIR", work.join("no-git-checkout"))
        .env("PYTHONHASHSEED", "0")
        .stdout(log)
        .stderr(err)
        .status()
        .map_err(|e| format!("cannot run fontforge: {e}"))?;
    let text = fs::read_to_string(&log_path).map_err(|e| io(&log_path, e))?;
    if !status.success() {
        return Err(format!(
            "the patcher failed on {} ({status}); its log is {}",
            face.source,
            log_path.display()
        ));
    }
    let built = into.join(face.output);
    if !built.is_file() {
        return Err(format!(
            "the patcher wrote no {} into {}; its log is {}",
            face.output,
            into.display(),
            log_path.display()
        ));
    }
    let dest = out.join(face.output);
    fs::rename(&built, &dest).map_err(|e| io(&dest, e))?;
    Ok(text)
}

/// Checks that each license file in `notices/` that comes from a pinned
/// archive still holds the archive's bytes.
fn notices_match(unpacked: &[(&str, PathBuf)]) -> Vec<String> {
    let mut problems = Vec::new();
    for (file, archive, inside) in recipe::NOTICE_COPIES {
        let ours = notices_dir().join(file);
        let Some((_, root)) = unpacked.iter().find(|(a, _)| *a == archive) else {
            problems.push(format!("no archive {archive} was unpacked"));
            continue;
        };
        let theirs = root.join(inside);
        match (fs::read(&ours), fs::read(&theirs)) {
            (Ok(a), Ok(b)) if a == b => {}
            (Ok(_), Ok(_)) => {
                problems.push(format!("notices/{file} differs from {inside} in {archive}"))
            }
            (Err(e), _) => problems.push(io(&ours, e)),
            (_, Err(e)) => problems.push(io(&theirs, e)),
        }
    }
    problems
}

/// Copies every file of `notices/` into `into`.
fn copy_notices(into: &Path) -> Result<(), String> {
    fs::create_dir_all(into).map_err(|e| io(into, e))?;
    let from = notices_dir();
    let mut entries: Vec<_> = fs::read_dir(&from)
        .map_err(|e| io(&from, e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    for path in entries {
        if let Some(name) = path.file_name() {
            let dest = into.join(name);
            fs::copy(&path, &dest).map_err(|e| io(&dest, e))?;
        }
    }
    Ok(())
}

/// Removes what an earlier build left in `out`, so every build starts
/// clean.
fn clear(out: &Path) -> Result<(), String> {
    for dir in ["work", "logs", "notices"] {
        let path = out.join(dir);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|e| io(&path, e))?;
        }
    }
    for file in recipe::FACES.iter().map(|f| f.output).chain(["build.tsv"]) {
        let path = out.join(file);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| io(&path, e))?;
        }
    }
    Ok(())
}

/// `cullet-mono build --out OUT --cache CACHE`.
pub fn run(out: &Path, cache: &Path) -> Result<(), Vec<String>> {
    let pins = Pins::tree().map_err(|e| vec![e])?;
    let one = |e: String| vec![e];
    // FontForge runs in the work folder, so every path it is given is
    // absolute.
    fs::create_dir_all(out).map_err(|e| vec![io(out, e)])?;
    let out = &fs::canonicalize(out).map_err(|e| vec![io(out, e)])?;
    clear(out).map_err(one)?;
    let work = out.join("work");
    let logs = out.join("logs");
    fs::create_dir_all(&work).map_err(|e| vec![io(&work, e)])?;
    fs::create_dir_all(&logs).map_err(|e| vec![io(&logs, e)])?;

    let fontforge = fontforge_version().map_err(one)?;
    println!("FontForge {fontforge}");

    let mut unpacked = Vec::new();
    for archive in [recipe::TERMINUS_ARCHIVE, recipe::PATCHER_ARCHIVE] {
        let pin = pins
            .source(archive)
            .ok_or_else(|| vec![format!("pins.tsv pins no source {archive}")])?;
        let zip = fetch(cache, pin).map_err(one)?;
        let into = work.join(archive.trim_end_matches(".zip"));
        unzip(&zip, &into).map_err(one)?;
        unpacked.push((archive, into));
    }
    let mut problems = notices_match(&unpacked);
    let [(_, terminus), (_, patcher)] = &unpacked[..] else {
        return Err(vec!["the build unpacked other archives than two".into()]);
    };
    let terminus = terminus.join(recipe::TERMINUS_FOLDER);
    let patcher = patcher.join("font-patcher");

    let mut record = format!(
        "# The Cullet Mono build that wrote this folder: tools/cullet-mono/README.md explains it.\n\
         fontforge\t{fontforge}\n\
         source_date_epoch\t{}\n",
        recipe::SOURCE_DATE_EPOCH
    );
    for pin in &pins.sources {
        record.push_str(&format!(
            "source\t{}\t{}\t{}\n",
            pin.file, pin.sha256, pin.from
        ));
    }
    let mut patcher_version = None;
    for face in recipe::FACES {
        let log = patch(&face, &patcher, &terminus, &work, &logs, out).map_err(one)?;
        if patcher_version.is_none() {
            patcher_version = patcher_line(&log).map(str::to_owned);
        }
        let sets = icon_sets(&log);
        if sets != recipe::ICON_SETS {
            problems.push(format!(
                "the patcher added the glyph sets {} to {}, where NOTICES.txt lists {}",
                sets.join(", "),
                face.output,
                recipe::ICON_SETS.join(", ")
            ));
        }
        let Some(pin) = pins.face(face.output) else {
            problems.push(format!("pins.tsv pins no face {}", face.output));
            continue;
        };
        let path = out.join(face.output);
        let bytes = fs::read(&path).map_err(|e| vec![io(&path, e)])?;
        let (m, p) = check::face(&face, &bytes, pin);
        println!(
            "{}\t{} bytes\t{} characters\tSHA-256 {}",
            face.output, m.bytes, m.characters, m.sha256
        );
        record.push_str(&format!(
            "face\t{}\t{}\t{}\t{}\n",
            face.output, m.bytes, m.characters, m.sha256
        ));
        problems.extend(p);
    }
    if let Some(line) = patcher_version {
        record.push_str(&format!("patcher\t{line}\n"));
    }
    let build_tsv = out.join("build.tsv");
    fs::write(&build_tsv, record).map_err(|e| vec![io(&build_tsv, e)])?;
    copy_notices(&out.join("notices")).map_err(one)?;
    if problems.is_empty() {
        fs::remove_dir_all(&work).map_err(|e| vec![io(&work, e)])?;
        println!(
            "Built Cullet Mono into {}; both faces are the ones pins.tsv pins.",
            out.display()
        );
        Ok(())
    } else {
        Err(problems)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn icon_sets_are_read_from_the_log_once_each() {
        let log = "Nerd Fonts Patcher v3.4.0 (4.20.3) (ff 20251009)\n\
                   Adding 191 Glyphs from Seti-UI + Custom Set\n\
                   Adding 3 Glyphs from Powerline Symbols Set\n\
                   Adding 4 Glyphs from Powerline Symbols Set\n\
                   Done with Patch Sets, generating font...\n";
        assert_eq!(icon_sets(log), ["Seti-UI + Custom", "Powerline Symbols"]);
        assert_eq!(
            patcher_line("x\nNerd Fonts Patcher v3.4.0 (4.20.3) (ff 20251009)\n"),
            Some("Nerd Fonts Patcher v3.4.0 (4.20.3) (ff 20251009)")
        );
    }

    #[test]
    fn a_cached_source_with_other_bytes_is_refused_and_removed() {
        let dir = std::env::temp_dir().join(format!("cullet-mono-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.zip"), b"not the pinned bytes").unwrap();
        let pin = Pin {
            file: "a.zip".into(),
            bytes: 20,
            sha256: hex(&Sha256::digest(b"the pinned bytes")),
            from: "https://example.invalid/a.zip".into(),
        };
        let err = fetch(&dir, &pin).unwrap_err();
        assert!(err.contains("refuses"), "{err}");
        assert!(!dir.join("a.zip").exists());
        fs::write(dir.join("a.zip"), b"the pinned bytes").unwrap();
        assert_eq!(fetch(&dir, &pin).unwrap(), dir.join("a.zip"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
