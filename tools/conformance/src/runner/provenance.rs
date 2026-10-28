//! `provenance.tsv`: each file tier 1 and the capability probes copied
//! from the C reference, with the repository, commit and path it came from
//! and its SHA-256. A copied file that changes without its row, a row
//! whose file is gone, and a file in a copied directory with no row each
//! fail the check.

use std::path::Path;

use crate::sha256;

/// One copied file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Copy {
    pub line: usize,
    /// The file, from the root of this repository.
    pub path: String,
    pub repo: String,
    pub commit: String,
    /// The path in `repo` at `commit`, with `:FIRST-LAST` for an excerpt.
    pub source: String,
    pub sha256: String,
}

/// Reads `provenance.tsv`: `PATH REPO COMMIT SOURCE SHA256` a line, `#`
/// comments.
pub fn read(text: &str) -> Result<Vec<Copy>, String> {
    let mut out: Vec<Copy> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i.saturating_add(1);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let hex = |s: &str, len: usize| {
            s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        };
        let fields: Vec<&str> = line.split('\t').collect();
        let [path, repo, commit, source, sha] = fields[..] else {
            return Err(format!(
                "provenance.tsv:{n}: want PATH REPO COMMIT SOURCE SHA256"
            ));
        };
        if !hex(commit, 40) || !hex(sha, 64) {
            return Err(format!(
                "provenance.tsv:{n}: want a 40-digit COMMIT and a 64-digit SHA256"
            ));
        }
        if let Some(prev) = out.iter().find(|c| c.path == path) {
            return Err(format!(
                "provenance.tsv:{n}: {path} is already on line {}",
                prev.line
            ));
        }
        out.push(Copy {
            line: n,
            path: path.to_owned(),
            repo: repo.to_owned(),
            commit: commit.to_owned(),
            source: source.to_owned(),
            sha256: sha.to_owned(),
        });
    }
    Ok(out)
}

/// The commit `repo`'s copies came from, when they all came from one.
pub fn commit<'a>(copies: &'a [Copy], repo: &str) -> Result<&'a str, String> {
    let mut commits = copies.iter().filter(|c| c.repo == repo).map(|c| &c.commit);
    let first = commits
        .next()
        .ok_or_else(|| format!("provenance.tsv has no {repo} file"))?;
    match commits.find(|c| *c != first) {
        Some(other) => Err(format!(
            "provenance.tsv names two {repo} commits, {first} and {other}"
        )),
        None => Ok(first),
    }
}

/// Every fault of the copies under `root`, the root of this repository:
/// each row's file hashed against its SHA-256, and each file under the
/// `copied` directories checked for a row.
pub fn check(root: &Path, copies: &[Copy], copied: &[&str]) -> Vec<String> {
    let mut faults = Vec::new();
    for c in copies {
        match std::fs::read(root.join(&c.path)) {
            Ok(data) if sha256::hex(&data) == c.sha256 => {}
            Ok(_) => faults.push(format!(
                "{} differs from {} {}:{} (provenance.tsv:{})",
                c.path, c.repo, c.commit, c.source, c.line
            )),
            Err(e) => faults.push(format!("{}: {e} (provenance.tsv:{})", c.path, c.line)),
        }
    }
    for dir in copied {
        let mut files = Vec::new();
        walk(&root.join(dir), &mut files);
        for file in files {
            let rel = file
                .strip_prefix(root)
                .map_or_else(|_| file.display().to_string(), |p| p.display().to_string());
            if !copies.iter().any(|c| c.path == rel) {
                faults.push(format!("{rel} has no row in provenance.tsv"));
            }
        }
    }
    faults
}

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const COMMIT: &str = "eeaf2931985051585bf50979e0c3f588b4a51ef9";

    fn row(path: &str, sha: &str) -> String {
        format!("{path}\tlibtsm\t{COMMIT}\ttest/conformance/{path}\t{sha}\n")
    }

    fn tree(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("vtium-provenance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(root.join("corpus/golden"));
        let _ = std::fs::write(root.join("corpus/golden/a.txt"), "abc");
        root
    }

    #[test]
    fn rows_need_whole_hex_commits_and_digests_and_one_row_a_file() {
        assert!(read(&format!("# c\n{}", row("corpus/golden/a.txt", ABC))).is_ok());
        assert!(read(&row("a", &ABC[1..])).is_err(), "a short digest");
        assert!(read(&row("a", &ABC.to_uppercase())).is_err(), "upper case");
        assert!(read(&format!("{}{}", row("a", ABC), row("a", ABC))).is_err());
        assert!(read("a\tlibtsm\teeaf293\tx\tabc\n").is_err());
    }

    #[test]
    fn a_copy_that_matches_its_row_passes() {
        let root = tree("match");
        let copies = read(&row("corpus/golden/a.txt", ABC)).unwrap_or_default();
        assert_eq!(check(&root, &copies, &["corpus"]), Vec::<String>::new());
    }

    #[test]
    fn a_changed_copy_a_missing_copy_and_an_unlisted_file_fail() {
        let root = tree("faults");
        let _ = std::fs::write(root.join("corpus/golden/a.txt"), "abd");
        let _ = std::fs::write(root.join("corpus/golden/b.txt"), "new");
        let copies = read(&format!(
            "{}{}",
            row("corpus/golden/a.txt", ABC),
            row("corpus/golden/gone.txt", ABC)
        ))
        .unwrap_or_default();
        let faults = check(&root, &copies, &["corpus"]);
        assert_eq!(faults.len(), 3, "{faults:?}");
        assert!(faults[0].starts_with("corpus/golden/a.txt differs from libtsm"));
        assert!(faults[1].starts_with("corpus/golden/gone.txt: "));
        assert_eq!(
            faults[2],
            "corpus/golden/b.txt has no row in provenance.tsv"
        );
    }

    #[test]
    fn a_repository_has_one_commit() {
        let copies = read(&row("a", ABC)).unwrap_or_default();
        assert_eq!(commit(&copies, "libtsm"), Ok(COMMIT));
        assert!(commit(&copies, "kmscon").is_err());
        let two = read(&format!(
            "{}b\tlibtsm\t{}\tb\t{ABC}\n",
            row("a", ABC),
            "0".repeat(40)
        ))
        .unwrap_or_default();
        assert!(commit(&two, "libtsm").is_err());
    }
}
