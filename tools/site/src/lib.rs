//! The documentation site's menu, read from `zensical.toml`, checked against
//! the pages under `docs/` (ADR-1502 D3).
//!
//! Zensical's strict build fails on a link to a missing page or anchor, but
//! it builds a page that the menu leaves out without a warning, and it says
//! nothing of a menu entry that names a missing file. [`faults`] finds both:
//! a markdown page under `docs/` that `exclude_docs` keeps and that is
//! neither in `nav` nor matched by `not_in_nav`, and a `nav` entry that names
//! no page.
//!
//! `exclude_docs` and `not_in_nav` hold patterns written as a `.gitignore`'s
//! lines are, which is how Zensical reads them. [`Pattern`] reads the part of
//! that syntax the site uses: `*`, `?` and `**`, a leading `/` or a `/` inside
//! the pattern that anchors it at `docs/`, and a trailing `/` that matches
//! only a directory. It refuses a negation (`!`), a character class (`[`) and
//! an escape (`\`), so a pattern it cannot read fails the test instead of
//! being read wrong.

use std::collections::BTreeSet;
use std::path::Path;

/// One line of `exclude_docs` or `not_in_nav`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// The pattern's path segments, with `**` for any number of segments.
    segments: Vec<String>,
    /// A trailing `/`: the pattern matches a directory, and so every file
    /// under it, and never a file of that name.
    dir_only: bool,
}

impl Pattern {
    /// Reads one pattern line. A blank line or a `#` comment gives `None`.
    pub fn parse(line: &str) -> Result<Option<Pattern>, String> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return Ok(None);
        }
        if line.starts_with('!') || line.contains('[') || line.contains('\\') {
            return Err(format!(
                "the pattern `{line}` uses a negation, a character class or an escape, which this check does not read"
            ));
        }
        let dir_only = line.ends_with('/');
        let body = line.trim_end_matches('/');
        // A `/` at the start or inside the pattern anchors it at docs/;
        // without one, it matches at any depth.
        let anchored = body.contains('/');
        let body = body.trim_start_matches('/');
        if body.is_empty() {
            return Err(format!("the pattern `{line}` names nothing"));
        }
        let mut segments: Vec<String> = Vec::new();
        if !anchored {
            segments.push("**".to_owned());
        }
        segments.extend(body.split('/').map(str::to_owned));
        Ok(Some(Pattern { segments, dir_only }))
    }

    /// Whether the pattern matches `path`, a file's path under `docs/` with
    /// `/` between its segments. As in a `.gitignore`, a pattern that matches
    /// a directory matches every file under it.
    pub fn matches(&self, path: &str) -> bool {
        let parts: Vec<&str> = path.split('/').collect();
        (1..=parts.len()).any(|k| {
            let is_dir = k < parts.len();
            (is_dir || !self.dir_only) && segments_match(&self.segments, &parts[..k])
        })
    }
}

/// Every pattern of a multiline `exclude_docs` or `not_in_nav` value.
pub fn patterns(text: &str) -> Result<Vec<Pattern>, String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(p) = Pattern::parse(line)? {
            out.push(p);
        }
    }
    Ok(out)
}

fn segments_match(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first == "**" => {
            (0..=path.len()).any(|skip| segments_match(rest, &path[skip..]))
        }
        Some((first, rest)) => match path.split_first() {
            Some((seg, path_rest)) => {
                wildcard(first.as_bytes(), seg.as_bytes()) && segments_match(rest, path_rest)
            }
            None => false,
        },
    }
}

/// `*` matches any run of bytes within one segment, `?` any one byte.
fn wildcard(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|skip| wildcard(rest, &text[skip..])),
        Some((b'?', rest)) => !text.is_empty() && wildcard(rest, &text[1..]),
        Some((c, rest)) => text.first() == Some(c) && wildcard(rest, &text[1..]),
    }
}

/// What the check reads from `zensical.toml`.
#[derive(Debug)]
pub struct Site {
    /// Every page `nav` names, as written, outside addresses left out.
    pub nav: Vec<String>,
    pub exclude_docs: Vec<Pattern>,
    pub not_in_nav: Vec<Pattern>,
}

impl Site {
    /// Reads the text of a `zensical.toml`, whose settings sit under
    /// `[project]`.
    pub fn parse(text: &str) -> Result<Site, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("zensical.toml: {e}"))?;
        let project = table
            .get("project")
            .and_then(toml::Value::as_table)
            .ok_or("zensical.toml has no [project] table")?;
        let mut nav = Vec::new();
        collect_nav(
            project
                .get("nav")
                .ok_or("zensical.toml's [project] has no nav")?,
            &mut nav,
        )?;
        let multiline = |key: &str| -> Result<Vec<Pattern>, String> {
            match project.get(key) {
                None => Ok(Vec::new()),
                Some(toml::Value::String(s)) => patterns(s).map_err(|e| format!("{key}: {e}")),
                Some(_) => Err(format!("zensical.toml's {key} is not a string")),
            }
        };
        Ok(Site {
            nav,
            exclude_docs: multiline("exclude_docs")?,
            not_in_nav: multiline("not_in_nav")?,
        })
    }
}

/// Every page a `nav` value names: a string is a page or an address, an
/// array a list of entries, and a table a title over a page or a section.
fn collect_nav(value: &toml::Value, out: &mut Vec<String>) -> Result<(), String> {
    match value {
        toml::Value::String(s) => {
            if !(s.starts_with("https://") || s.starts_with("http://")) {
                out.push(s.clone());
            }
            Ok(())
        }
        toml::Value::Array(items) => items.iter().try_for_each(|v| collect_nav(v, out)),
        toml::Value::Table(t) => t.values().try_for_each(|v| collect_nav(v, out)),
        other => Err(format!(
            "zensical.toml's nav holds {other}, which is neither a page, a list nor a titled entry"
        )),
    }
}

/// Every markdown page under `docs`, as paths under it with `/` between
/// their segments, in sorted order.
pub fn pages(docs: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    walk(docs, "", &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|n| format!("{}: a name that is not UTF-8: {n:?}", dir.display()))?;
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let kind = entry.file_type().map_err(|e| format!("{rel}: {e}"))?;
        if kind.is_dir() {
            walk(&entry.path(), &rel, out)?;
        } else if name.ends_with(".md") {
            out.push(rel);
        }
    }
    Ok(())
}

/// Each fault of the menu against the pages under `docs`, one sentence each:
/// a `nav` entry that names no page, or names one `exclude_docs` leaves out,
/// and a page that is neither in `nav` nor matched by `not_in_nav`.
pub fn faults(site: &Site, docs: &Path) -> Result<Vec<String>, String> {
    let excluded = |p: &str| site.exclude_docs.iter().any(|pat| pat.matches(p));
    let mut out = Vec::new();
    for entry in &site.nav {
        if entry
            .split('/')
            .any(|s| s == ".." || s == "." || s.is_empty())
        {
            out.push(format!(
                "nav names `{entry}`, which is not a plain path under docs/"
            ));
        } else if !docs.join(entry).is_file() {
            out.push(format!(
                "nav names `{entry}`, which is not a file under docs/"
            ));
        } else if excluded(entry) {
            out.push(format!(
                "nav names `{entry}`, which exclude_docs leaves out of the site"
            ));
        }
    }
    let in_nav: BTreeSet<&str> = site.nav.iter().map(String::as_str).collect();
    for page in pages(docs)? {
        if excluded(&page) || in_nav.contains(page.as_str()) {
            continue;
        }
        if !site.not_in_nav.iter().any(|pat| pat.matches(&page)) {
            out.push(format!(
                "docs/{page} is in neither nav nor not_in_nav: give it a place in zensical.toml's menu, or list it in not_in_nav when a page links it"
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(s: &str) -> Pattern {
        Pattern::parse(s).unwrap().unwrap()
    }

    #[test]
    fn an_anchored_directory_pattern_matches_only_under_that_directory_at_the_top() {
        let p = pat("/scripts/");
        assert!(p.matches("scripts/adr"));
        assert!(p.matches("scripts/sub/x.md"));
        assert!(!p.matches("scripts"));
        assert!(!p.matches("heads/scripts/x.md"));
    }

    #[test]
    fn a_pattern_with_no_slash_matches_at_any_depth() {
        let p = pat("*.tsv");
        assert!(p.matches("modes.tsv"));
        assert!(p.matches("architecture/term/ADR-1000-modes.tsv"));
        assert!(!p.matches("modes.md"));
        assert!(!p.matches("modes.tsv.md"));
    }

    #[test]
    fn a_slash_inside_a_pattern_anchors_it_at_the_top() {
        let p = pat("figures/README.md");
        assert!(p.matches("figures/README.md"));
        assert!(!p.matches("architecture/term/figures/README.md"));
    }

    #[test]
    fn a_double_star_matches_any_number_of_directories() {
        let p = pat("**/figures/README.md");
        assert!(p.matches("figures/README.md"));
        assert!(p.matches("architecture/term/figures/README.md"));
        assert!(!p.matches("figures/notes.md"));
    }

    #[test]
    fn a_star_stays_within_one_segment() {
        let p = pat("/architecture/*/ADR-*.md");
        assert!(p.matches("architecture/term/ADR-1000-the-modes.md"));
        assert!(!p.matches("architecture/INDEX.md"));
        assert!(!p.matches("architecture/term/old/ADR-1.md"));
        assert!(!p.matches("architecture/term/README.md"));
    }

    #[test]
    fn blank_lines_and_comments_are_no_patterns() {
        assert_eq!(Pattern::parse("   ").unwrap(), None);
        assert_eq!(Pattern::parse("# a note").unwrap(), None);
    }

    #[test]
    fn syntax_this_check_does_not_read_is_refused() {
        for line in ["!keep.md", "page[12].md", "a\\#b.md", "/"] {
            assert!(Pattern::parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn nav_gives_every_page_and_leaves_out_addresses() {
        let site = Site::parse(
            r#"
[project]
nav = [
  { "Home" = ["index.md", { "Showcase" = "showcase/README.md" }] },
  "plain.md",
  { "Out" = "https://github.com/vtium/vtium/blob/main/AGENTS.md" },
]
"#,
        )
        .unwrap();
        assert_eq!(site.nav, ["index.md", "showcase/README.md", "plain.md"]);
    }
}
