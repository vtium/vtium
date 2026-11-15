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
//! FontForge also runs the user's own Python scripts and plugins, and reads
//! the user's preferences, so the build starts it without them, in an
//! environment that holds only what the build sets.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::check;
use crate::finish;
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

/// The curl command that fetches `url` into `part`. It uses HTTPS only,
/// redirects included, and gives up when the server does not answer within
/// 30 seconds or the download takes more than ten minutes, so that a
/// stalled server cannot hang the build.
fn curl(part: &Path, url: &str) -> Command {
    let mut curl = Command::new("curl");
    curl.args([
        "-fsSL",
        "--retry",
        "3",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
    ])
    .args(["--connect-timeout", "30", "--max-time", "600", "-o"])
    .arg(part)
    .arg(url);
    curl
}

/// Fetches `pin` into `cache` with curl unless it is already there, and
/// checks its size and SHA-256. A file that does not match is removed, so
/// the next build fetches it again, and the build stops.
pub fn fetch(cache: &Path, pin: &Pin) -> Result<PathBuf, String> {
    fs::create_dir_all(cache).map_err(|e| io(cache, e))?;
    let path = cache.join(&pin.file);
    if !path.exists() {
        let part = cache.join(format!("{}.part", pin.file));
        println!("Fetching {}", pin.from);
        let status = curl(&part, &pin.from)
            .status()
            .map_err(|e| format!("cannot run curl: {e}"))?;
        if !status.success() {
            let _ = fs::remove_file(&part);
            return Err(format!("curl could not fetch {}: {status}", pin.from));
        }
        fs::rename(&part, &path).map_err(|e| io(&path, e))?;
    }
    let bytes = fs::read(&path).map_err(|e| io(&path, e))?;
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size != pin.bytes {
        let _ = fs::remove_file(&path);
        return Err(format!(
            "{} is {size} bytes, not {} as pins.tsv pins it, so the build refuses it and removed it from {}",
            pin.file,
            pin.bytes,
            cache.display()
        ));
    }
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

/// A command that runs `program` in an environment of its own under `work`.
/// The environment holds only `PATH`, a home folder and XDG folders inside
/// `work/home`, the C locale, and the three variables that keep the patcher
/// reproducible, which the module's comment explains. So FontForge finds
/// none of the user's preferences, scripts or plugins, and Python finds
/// none of the user's modules. Nothing the build runs writes outside
/// `work`.
fn sandboxed(program: &str, work: &Path) -> Command {
    let home = work.join("home");
    let mut command = Command::new(program);
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("LC_ALL", "C.UTF-8")
        .env("SOURCE_DATE_EPOCH", recipe::SOURCE_DATE_EPOCH.to_string())
        .env("GIT_DIR", work.join("no-git-checkout"))
        .env("PYTHONHASHSEED", "0")
        .current_dir(work);
    command
}

/// FontForge's options that keep it from running the user's Python init
/// scripts and plugins. They work only before `--script`.
const FONTFORGE_SKIP: [&str; 2] = ["-skippyfile", "-skippyplug"];

/// FontForge's version, from the line `fontforge VERSION` that
/// `fontforge --version` prints.
fn fontforge_version(work: &Path) -> Result<String, String> {
    let out = sandboxed("fontforge", work)
        .args(FONTFORGE_SKIP)
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

/// What is wrong when the patcher's log for `file` reports adding other
/// glyph sets than the ones NOTICES.txt lists, or `None`.
fn icon_set_problem(file: &str, log: &str) -> Option<String> {
    let sets = icon_sets(log);
    (sets != recipe::ICON_SETS).then(|| {
        format!(
            "the patcher added the glyph sets {} to {file}, where NOTICES.txt lists {}",
            sets.join(", "),
            recipe::ICON_SETS.join(", ")
        )
    })
}

/// The line in which the patcher names its own version and FontForge's.
fn patcher_line(log: &str) -> Option<&str> {
    log.lines()
        .find(|l| l.starts_with("Nerd Fonts Patcher "))
        .map(str::trim)
}

/// The command that runs the patcher on `source`, writing into `into`.
fn patch_command(
    face: &recipe::Face,
    patcher: &Path,
    source: &Path,
    into: &Path,
    work: &Path,
) -> Command {
    let mut command = sandboxed("fontforge", work);
    command
        .args(FONTFORGE_SKIP)
        .arg("--script")
        .arg(patcher)
        .args(recipe::OPTIONS)
        .args(["--name", face.name, "--no-progressbars", "--outputdir"])
        .arg(into)
        .arg(source);
    command
}

/// Patches one face in `work` and finishes it, and writes the patcher's
/// log into `logs`. Returns the log and the finished face's bytes.
fn patch(
    face: &recipe::Face,
    patcher: &Path,
    terminus: &Path,
    work: &Path,
    logs: &Path,
) -> Result<(String, Vec<u8>), String> {
    let into = work.join("out");
    let log_path = logs.join(face.output.replace(".ttf", ".log"));
    let log = fs::File::create(&log_path).map_err(|e| io(&log_path, e))?;
    let err = log.try_clone().map_err(|e| io(&log_path, e))?;
    println!("Patching {} as {}", face.source, face.output);
    let status = patch_command(face, patcher, &terminus.join(face.source), &into, work)
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
    let bytes = fs::read(&built).map_err(|e| io(&built, e))?;
    let finished = finish::face(&bytes).map_err(|why| format!("{}: {why}", built.display()))?;
    Ok((text, finished))
}

/// Checks that each license file in `notices`, which `copies` names with
/// the archive and the path it comes from, still holds the archive's
/// bytes. `unpacked` names the folder each archive was unpacked into.
fn notices_match(
    notices: &Path,
    copies: &[(&str, &str, &str)],
    unpacked: &[(&str, PathBuf)],
) -> Vec<String> {
    let mut problems = Vec::new();
    for &(file, archive, inside) in copies {
        let ours = notices.join(file);
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

/// Copies every file of the folder `from` into `into`.
fn copy_notices(from: &Path, into: &Path) -> Result<(), String> {
    fs::create_dir_all(into).map_err(|e| io(into, e))?;
    let mut entries: Vec<_> = fs::read_dir(from)
        .map_err(|e| io(from, e))?
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

/// The file that marks a folder as one the build writes into. The build
/// writes the marker before anything else.
const MARKER: &str = ".cullet-mono-build";

/// Removes what an earlier build left in `out`, so every build starts
/// clean. It refuses a folder that holds files but no sign of an earlier
/// build, the marker or a `build.tsv`, because it would delete that
/// folder's own `work`, `logs` and `notices` folders.
fn clear(out: &Path) -> Result<(), String> {
    let mut entries = fs::read_dir(out).map_err(|e| io(out, e))?;
    let built_here = out.join(MARKER).is_file() || out.join("build.tsv").is_file();
    if entries.next().is_some() && !built_here {
        return Err(format!(
            "{} holds files that no Cullet Mono build wrote, so the build does not write into it. Name a new or empty folder.",
            out.display()
        ));
    }
    for dir in ["work", "logs", "notices"] {
        let path = out.join(dir);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|e| io(&path, e))?;
        }
    }
    for file in recipe::FACES
        .iter()
        .map(|f| f.output)
        .chain(["build.tsv", MARKER])
    {
        let path = out.join(file);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| io(&path, e))?;
        }
    }
    Ok(())
}

/// Puts the result of a build in place. When the build found no problem,
/// the faces and `build.tsv` go into `out`, the notices from `notices` go
/// into `out/notices`, and the work folder goes away. Otherwise the faces
/// and `build.tsv` stay in `work`, where nobody takes them for the pinned
/// faces, and the problems come back with a last line that says so.
fn deliver(
    out: &Path,
    work: &Path,
    notices: &Path,
    faces: &[(&str, Vec<u8>)],
    record: &str,
    mut problems: Vec<String>,
) -> Result<(), Vec<String>> {
    let into = if problems.is_empty() { out } else { work };
    for (file, bytes) in faces {
        let path = into.join(file);
        fs::write(&path, bytes).map_err(|e| vec![io(&path, e)])?;
    }
    let build_tsv = into.join("build.tsv");
    fs::write(&build_tsv, record).map_err(|e| vec![io(&build_tsv, e)])?;
    if problems.is_empty() {
        copy_notices(notices, &out.join("notices")).map_err(|e| vec![e])?;
        fs::remove_dir_all(work).map_err(|e| vec![io(work, e)])?;
        println!(
            "Built Cullet Mono into {}; both faces are the ones pins.tsv pins.",
            out.display()
        );
        Ok(())
    } else {
        problems.push(format!(
            "The build failed, so it left the faces it wrote in {}, beside its record build.tsv.",
            work.display()
        ));
        Err(problems)
    }
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
    let marker = out.join(MARKER);
    fs::write(
        &marker,
        "cullet-mono build writes this folder, and deletes what it wrote here at the start of each build.\n",
    )
    .map_err(|e| vec![io(&marker, e)])?;
    let work = out.join("work");
    let logs = out.join("logs");
    for dir in [&work, &logs, &work.join("home")] {
        fs::create_dir_all(dir).map_err(|e| vec![io(dir, e)])?;
    }

    let fontforge = fontforge_version(&work).map_err(one)?;
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
    let mut problems = notices_match(&notices_dir(), &recipe::NOTICE_COPIES, &unpacked);
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
    let mut faces = Vec::new();
    for face in recipe::FACES {
        let (log, bytes) = patch(&face, &patcher, &terminus, &work, &logs).map_err(one)?;
        if patcher_version.is_none() {
            patcher_version = patcher_line(&log).map(str::to_owned);
        }
        problems.extend(icon_set_problem(face.output, &log));
        let Some(pin) = pins.face(face.output) else {
            problems.push(format!("pins.tsv pins no face {}", face.output));
            continue;
        };
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
        faces.push((face.output, bytes));
    }
    if let Some(line) = patcher_version {
        record.push_str(&format!("patcher\t{line}\n"));
    }
    deliver(out, &work, &notices_dir(), &faces, &record, problems)
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

    /// A new, empty folder for one test, under the system's temporary
    /// folder.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cullet-mono-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_cached_source_with_other_bytes_is_refused_and_removed() {
        let dir = scratch("fetch");
        let pin = Pin {
            file: "a.zip".into(),
            bytes: 16,
            sha256: hex(&Sha256::digest(b"the pinned bytes")),
            from: "https://example.invalid/a.zip".into(),
        };
        for (bytes, why) in [
            (&b"the pinned bytes and more"[..], "is 25 bytes, not 16"),
            (b"THE PINNED BYTES", "has SHA-256"),
        ] {
            fs::write(dir.join("a.zip"), bytes).unwrap();
            let err = fetch(&dir, &pin).unwrap_err();
            assert!(err.contains(why) && err.contains("refuses"), "{err}");
            assert!(!dir.join("a.zip").exists());
        }
        fs::write(dir.join("a.zip"), b"the pinned bytes").unwrap();
        assert_eq!(fetch(&dir, &pin).unwrap(), dir.join("a.zip"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn curl_fetches_over_https_only_and_gives_up_on_a_stalled_server() {
        let command = curl(Path::new("a.part"), "https://example.invalid/a.zip");
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        for pair in [
            ["--proto", "=https"],
            ["--proto-redir", "=https"],
            ["--connect-timeout", "30"],
            ["--max-time", "600"],
        ] {
            assert!(args.windows(2).any(|w| w == pair), "{pair:?} in {args:?}");
        }
        assert_eq!(args.last().unwrap(), "https://example.invalid/a.zip");
    }

    #[test]
    fn fontforge_runs_without_the_user_s_scripts_and_environment() {
        let work = scratch("sandbox");
        let face = recipe::FACES[0];
        let patch = patch_command(
            &face,
            Path::new("/p/font-patcher"),
            Path::new("/t/a.ttf"),
            Path::new("/o"),
            &work,
        );
        let args: Vec<_> = patch
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[..3], ["-skippyfile", "-skippyplug", "--script"]);
        // the variables the sandbox hands to every program it runs, as
        // `env` prints them
        let out = sandboxed("env", &work).output().unwrap();
        assert!(out.status.success());
        let text = String::from_utf8(out.stdout).unwrap();
        let mut names: Vec<&str> = text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(n, _)| n)
            .collect();
        names.sort_unstable();
        let mut expected = vec![
            "GIT_DIR",
            "HOME",
            "LC_ALL",
            "PYTHONHASHSEED",
            "SOURCE_DATE_EPOCH",
            "XDG_CACHE_HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
        ];
        if std::env::var_os("PATH").is_some() {
            expected.push("PATH");
            expected.sort_unstable();
        }
        assert_eq!(names, expected, "{text}");
        let home = work.join("home");
        for line in text.lines() {
            if let Some(path) = line
                .strip_prefix("HOME=")
                .or_else(|| line.strip_prefix("XDG_CONFIG_HOME="))
            {
                assert!(Path::new(path).starts_with(&home), "{line}");
            }
        }
        fs::remove_dir_all(&work).unwrap();
    }

    #[test]
    fn a_notice_that_differs_from_its_archive_fails() {
        let dir = scratch("notices");
        let (ours, theirs) = (dir.join("ours"), dir.join("theirs"));
        fs::create_dir_all(ours.join("x")).unwrap();
        fs::create_dir_all(theirs.join("src")).unwrap();
        fs::write(ours.join("a-LICENSE.txt"), "the license\n").unwrap();
        fs::write(theirs.join("src/LICENSE"), "the license\n").unwrap();
        let copies = [("a-LICENSE.txt", "a.zip", "src/LICENSE")];
        let unpacked = [("a.zip", theirs.clone())];
        assert_eq!(
            notices_match(&ours, &copies, &unpacked),
            Vec::<String>::new()
        );
        fs::write(ours.join("a-LICENSE.txt"), "the license, edited\n").unwrap();
        assert_eq!(
            notices_match(&ours, &copies, &unpacked),
            ["notices/a-LICENSE.txt differs from src/LICENSE in a.zip"]
        );
        assert_eq!(
            notices_match(&ours, &copies, &[]),
            ["no archive a.zip was unpacked"]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn other_icon_sets_than_the_notices_list_fail() {
        let log = |sets: &[&str]| -> String {
            sets.iter()
                .map(|s| format!("Adding 3 Glyphs from {s} Set\n"))
                .collect()
        };
        assert_eq!(icon_set_problem("F.ttf", &log(&recipe::ICON_SETS)), None);
        let missing = log(&recipe::ICON_SETS[1..]);
        let p = icon_set_problem("F.ttf", &missing).unwrap();
        assert!(
            p.starts_with("the patcher added the glyph sets Heavy Angle Brackets,")
                && p.contains("to F.ttf"),
            "{p}"
        );
        let mut more = recipe::ICON_SETS.to_vec();
        more.push("Font Logos");
        let p = icon_set_problem("F.ttf", &log(&more)).unwrap();
        assert!(p.contains("Codicons, Font Logos to F.ttf"), "{p}");
    }

    #[test]
    fn a_folder_no_build_wrote_is_left_alone() {
        let dir = scratch("clear");
        clear(&dir).unwrap();
        fs::create_dir_all(dir.join("notices")).unwrap();
        fs::write(dir.join("notices/NOTICES.txt"), "keep me").unwrap();
        let err = clear(&dir).unwrap_err();
        assert!(err.contains("no Cullet Mono build wrote"), "{err}");
        assert!(dir.join("notices/NOTICES.txt").is_file());
        fs::write(dir.join(MARKER), "").unwrap();
        fs::write(dir.join(recipe::FACES[0].output), "old face").unwrap();
        clear(&dir).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_build_leaves_its_faces_in_the_work_folder() {
        let dir = scratch("deliver");
        let (out, notices) = (dir.join("out"), dir.join("notices"));
        let work = out.join("work");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&notices).unwrap();
        fs::write(notices.join("NOTICES.txt"), "notices").unwrap();
        let faces = [(recipe::FACES[0].output, b"face".to_vec())];
        let err = deliver(
            &out,
            &work,
            &notices,
            &faces,
            "record",
            vec!["a problem".into()],
        )
        .unwrap_err();
        assert_eq!(err[0], "a problem");
        assert!(err[1].contains("left the faces it wrote in"), "{err:?}");
        assert!(work.join(faces[0].0).is_file() && work.join("build.tsv").is_file());
        assert!(!out.join(faces[0].0).exists() && !out.join("build.tsv").exists());
        assert!(!out.join("notices").exists());
        deliver(&out, &work, &notices, &faces, "record", Vec::new()).unwrap();
        assert_eq!(fs::read(out.join(faces[0].0)).unwrap(), b"face");
        assert!(out.join("build.tsv").is_file() && out.join("notices/NOTICES.txt").is_file());
        assert!(!work.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
