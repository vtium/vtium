//! The pin and the line ranges: reading `pin.tsv` and `extract.tsv`, finding
//! the pinned commit, and extracting each range with its edges checked.
//! Shared by `build.rs`, which extracts and compiles the C side, and by the
//! self-test, which moves each range and expects a refusal.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The libtsm commit the C side is taken from, and where to fetch it.
pub struct Pin {
    pub commit: String,
    pub url: String,
}

/// One line range of the C reference, with the text of its edge lines.
#[derive(Clone)]
pub struct Range {
    pub name: String,
    pub path: String,
    pub first: usize,
    pub last: usize,
    pub first_text: String,
    pub last_text: String,
}

/// The rows of a tab-separated file, comments and blank lines left out.
pub fn rows(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
}

pub fn read_pin(text: &str) -> Result<Pin, String> {
    let field = |key: &str| {
        rows(text)
            .find(|row| row.first() == Some(&key))
            .and_then(|row| row.get(1).map(|v| v.to_string()))
            .ok_or_else(|| format!("pin.tsv has no {key}"))
    };
    let commit = field("commit")?;
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("pin.tsv names no full commit hash".into());
    }
    Ok(Pin {
        commit,
        url: field("url")?,
    })
}

pub fn read_ranges(text: &str) -> Result<Vec<Range>, String> {
    rows(text)
        .map(|row| match row[..] {
            [name, path, first, last, first_text, last_text] => Ok(Range {
                name: name.into(),
                path: path.into(),
                first: first
                    .parse()
                    .map_err(|_| format!("{name}: bad first line"))?,
                last: last.parse().map_err(|_| format!("{name}: bad last line"))?,
                first_text: first_text.into(),
                last_text: last_text.into(),
            }),
            _ => Err(format!("extract.tsv: a row without six fields: {row:?}")),
        })
        .collect()
}

/// Runs git and returns its output.
pub fn git(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

fn has_commit(gitdir: &Path, commit: &str) -> bool {
    let dir = gitdir.to_string_lossy();
    let spec = format!("{commit}^{{commit}}");
    git(&["-C", &dir, "rev-parse", "--verify", "--quiet", &spec])
        .is_ok_and(|out| String::from_utf8_lossy(&out).trim() == commit)
}

/// A git directory holding the pinned commit: `libtsm` if it has it, else a
/// clone of the pin's URL at `clone`. Only reads `libtsm`.
pub fn source(libtsm: &Path, clone: &Path, pin: &Pin) -> Result<PathBuf, String> {
    if libtsm.is_dir() && has_commit(libtsm, &pin.commit) {
        return Ok(libtsm.to_path_buf());
    }
    let dir = clone.to_string_lossy();
    if !clone.is_dir() {
        git(&[
            "clone",
            "--quiet",
            "--bare",
            "--filter=blob:none",
            &pin.url,
            &dir,
        ])?;
    }
    if !has_commit(clone, &pin.commit) {
        git(&[
            "-C",
            &dir,
            "fetch",
            "--quiet",
            "--filter=blob:none",
            &pin.url,
            &pin.commit,
        ])
        .map_err(|e| format!("{} has no commit {}\n  {e}", pin.url, pin.commit))?;
    }
    if !has_commit(clone, &pin.commit) {
        return Err(format!("{} has no commit {}", pin.url, pin.commit));
    }
    Ok(clone.to_path_buf())
}

/// A file at the pinned commit.
pub fn show(gitdir: &Path, commit: &str, path: &str) -> Result<Vec<u8>, String> {
    git(&[
        "-C",
        &gitdir.to_string_lossy(),
        "show",
        &format!("{commit}:{path}"),
    ])
}

/// The brace depth at the end of C text, comments and literals aside.
fn braces(text: &str) -> i64 {
    let mut depth = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut star = false;
                for c in chars.by_ref() {
                    if star && c == '/' {
                        break;
                    }
                    star = c == '*';
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' | '\'' => {
                while let Some(d) = chars.next() {
                    match d {
                        '\\' => {
                            chars.next();
                        }
                        d if d == c => break,
                        _ => {}
                    }
                }
            }
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn collapse(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Writes each range to `dest/<name>.inc`, with a `#line` directive back to
/// its source, refusing a range whose edge lines differ from the text
/// recorded for it or whose braces do not balance.
pub fn extract(gitdir: &Path, commit: &str, ranges: &[Range], dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut files: Vec<(String, String)> = Vec::new();
    for r in ranges {
        if !files.iter().any(|(path, _)| *path == r.path) {
            let text = String::from_utf8(show(gitdir, commit, &r.path)?)
                .map_err(|_| format!("{} is not UTF-8", r.path))?;
            files.push((r.path.clone(), text));
        }
        let text = &files
            .iter()
            .find(|(path, _)| *path == r.path)
            .expect("read above")
            .1;
        let lines: Vec<&str> = text.split('\n').collect();
        if !(1 <= r.first && r.first <= r.last && r.last <= lines.len()) {
            return Err(format!(
                "{}: {} has no lines {}-{}",
                r.name, r.path, r.first, r.last
            ));
        }
        let piece = &lines[r.first - 1..r.last];
        for (line, got, want) in [
            (r.first, piece[0], &r.first_text),
            (r.last, piece[piece.len() - 1], &r.last_text),
        ] {
            if collapse(got) != *want {
                return Err(format!(
                    "{}: {}:{line} reads {:?}, extract.tsv expects {want:?}",
                    r.name,
                    r.path,
                    got.trim()
                ));
            }
        }
        let body = piece.join("\n");
        if braces(&body) != 0 {
            return Err(format!(
                "{}: {}:{}-{} does not balance its braces",
                r.name, r.path, r.first, r.last
            ));
        }
        let inc = dest.join(format!("{}.inc", r.name));
        fs::write(&inc, format!("#line {} \"{}\"\n{body}\n", r.first, r.path))
            .map_err(|e| format!("{}: {e}", inc.display()))?;
    }
    Ok(())
}

/// The C reference's conformance inputs at the pin: (path, bytes).
pub fn corpus(gitdir: &Path, commit: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    let listing = git(&[
        "-C",
        &gitdir.to_string_lossy(),
        "ls-tree",
        "-r",
        "--name-only",
        commit,
        "test/conformance/cases",
        "test/conformance/recorded",
    ])?;
    let mut names: Vec<String> = String::from_utf8_lossy(&listing)
        .lines()
        .filter(|name| name.ends_with(".bin"))
        .map(String::from)
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| Ok((name.clone(), show(gitdir, commit, &name)?)))
        .collect()
}
