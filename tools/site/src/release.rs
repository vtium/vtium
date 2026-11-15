//! A release's notes, written once as a page under `docs/releases/`, and
//! the text the GitHub release shows, generated from that page.
//!
//! A page of the site links another page by a relative path, so that the
//! site's strict build checks the link and its anchor. GitHub shows the
//! notes outside the site, where a relative path leads nowhere. [`for_github`]
//! therefore turns each relative link to a file under `docs/` into the
//! file's full address on the site, and keeps every other byte as it is:
//! a link that is already a full address, a link inside code, and all the
//! text around the links.
//!
//! [`release_index_faults`] checks the index of the releases: every page
//! under `docs/releases/` has a row there, every row names a page, and the
//! rows run newest first.
//!
//! References:
//! - ADR-1508 D2: a page for each release, and an index newest first.
//! - ADR-1508 D3: the page is the one text, and `make release-notes`
//!   generates the GitHub release's body from it.

use std::cmp::Ordering;
use std::path::Path;

/// Rewrites the markdown of the page at `page`, a path under `docs/` such
/// as `releases/0.1.0-alpha.8.md`, for GitHub: each relative link becomes
/// its target's full address under `site_url`, which ends in `/`.
///
/// A link to a markdown page becomes the page's address as the site serves
/// it: `concepts/security.md` becomes `SITE/concepts/security/`, and an
/// `index.md` or `README.md` becomes its folder's address. A link to any
/// other file, such as a picture, becomes `SITE/` followed by its path. An
/// anchor is kept. A link that names a scheme, such as `https:` or
/// `mailto:`, is kept as it is, and so is a link inside a code span or a
/// fenced code block. A link that leaves `docs/` is an error, since the
/// site has no address for it.
pub fn for_github(text: &str, page: &str, site_url: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<String> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str()) {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let mark = &trimmed[..3];
            let run = trimmed.len() - trimmed.trim_start_matches(&mark[..1]).len();
            fence = Some(mark[..1].repeat(run));
            out.push_str(line);
            continue;
        }
        if let Some(def) = definition(line) {
            let (head, target, tail) = def;
            out.push_str(head);
            out.push_str(&address(target, page, site_url)?);
            out.push_str(tail);
            continue;
        }
        out.push_str(&inline_links(line, page, site_url)?);
    }
    Ok(out)
}

/// A link definition, `[label]: target`, split around its target.
fn definition(line: &str) -> Option<(&str, &str, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 || !line[indent..].starts_with('[') {
        return None;
    }
    let close = line.find("]:")?;
    let after = close + 2;
    let start = after + (line[after..].len() - line[after..].trim_start().len());
    let rest = &line[start..];
    let len = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    if len == 0 {
        return None;
    }
    Some((&line[..start], &rest[..len], &rest[len..]))
}

/// The line with each inline link's target outside code spans rewritten.
/// A code span runs from a run of backticks to the next run of the same
/// length, as CommonMark reads it.
fn inline_links(line: &str, page: &str, site_url: &str) -> Result<String, String> {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while !rest.is_empty() {
        let tick = rest.find('`').unwrap_or(rest.len());
        out.push_str(&prose_links(&rest[..tick], page, site_url)?);
        rest = &rest[tick..];
        if rest.is_empty() {
            break;
        }
        let run = rest.len() - rest.trim_start_matches('`').len();
        let marks = &rest[..run];
        match rest[run..].find(marks) {
            Some(end) => {
                let span = run + end + run;
                out.push_str(&rest[..span]);
                rest = &rest[span..];
            }
            None => {
                // An unmatched run of backticks is plain text.
                out.push_str(marks);
                rest = &rest[run..];
            }
        }
    }
    Ok(out)
}

/// The text with each `](target)` rewritten.
fn prose_links(text: &str, page: &str, site_url: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        let start = at + 2;
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(')') else {
            break;
        };
        // A target may carry a title after a space: keep the title.
        let inner = &rest[..end];
        let len = inner
            .find(|c: char| c.is_whitespace())
            .unwrap_or(inner.len());
        out.push_str(&address(&inner[..len], page, site_url)?);
        out.push_str(&inner[len..]);
        rest = &rest[end..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The full address of `target`, a link written on `page`, or `target`
/// itself when it already names a scheme.
fn address(target: &str, page: &str, site_url: &str) -> Result<String, String> {
    let has_scheme = target.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    });
    if has_scheme || target.is_empty() {
        return Ok(target.to_owned());
    }
    let (path, anchor) = match target.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (target, None),
    };
    let file = if path.is_empty() {
        page.to_owned()
    } else if let Some(absolute) = path.strip_prefix('/') {
        absolute.to_owned()
    } else {
        resolve(page, path).ok_or_else(|| format!("{page}: the link `{target}` leaves docs/"))?
    };
    let served = served_path(&file);
    let mut out = format!("{site_url}{served}");
    if let Some(a) = anchor {
        out.push('#');
        out.push_str(a);
    }
    Ok(out)
}

/// `path`, relative to the folder of `page`, as a path under `docs/`, or
/// `None` when it climbs out of `docs/`.
fn resolve(page: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// The address under the site's root at which the site serves `file`, a
/// path under `docs/`. The site gives each markdown page a folder of its
/// own, and serves `index.md` and `README.md` as their folder's page.
fn served_path(file: &str) -> String {
    let Some(stem) = file.strip_suffix(".md") else {
        return file.to_owned();
    };
    let (dir, name) = match stem.rsplit_once('/') {
        Some((d, n)) => (Some(d), n),
        None => (None, stem),
    };
    match (dir, name) {
        (None, "index" | "README") => String::new(),
        (Some(d), "index" | "README") => format!("{d}/"),
        _ => format!("{stem}/"),
    }
}

/// A release's version, such as `0.1.0-alpha.8`, compared as semantic
/// versioning orders versions: by the three numbers, then a version with
/// a pre-release part before the same version without one, then the
/// pre-release part's fields in turn, a numeric field by its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version(String);

impl Version {
    /// Reads a version. It must have three numbers and may have a
    /// pre-release part after a `-`.
    pub fn parse(text: &str) -> Result<Version, String> {
        let (core, _) = text.split_once('-').unwrap_or((text, ""));
        let numbers: Vec<&str> = core.split('.').collect();
        if numbers.len() != 3
            || numbers
                .iter()
                .any(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(format!("`{text}` is not a version such as 0.1.0-alpha.8"));
        }
        Ok(Version(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn parts(&self) -> (Vec<u64>, Option<Vec<&str>>) {
        let (core, pre) = match self.0.split_once('-') {
            Some((c, p)) => (c, Some(p.split('.').collect())),
            None => (self.0.as_str(), None),
        };
        let numbers = core
            .split('.')
            .map(|n| n.parse().unwrap_or(u64::MAX))
            .collect();
        (numbers, pre)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let (a_num, a_pre) = self.parts();
        let (b_num, b_pre) = other.parts();
        a_num.cmp(&b_num).then_with(|| match (a_pre, b_pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => {
                for (x, y) in a.iter().zip(&b) {
                    let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                        (Ok(m), Ok(n)) => m.cmp(&n),
                        (Ok(_), Err(_)) => Ordering::Less,
                        (Err(_), Ok(_)) => Ordering::Greater,
                        (Err(_), Err(_)) => x.cmp(y),
                    };
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                a.len().cmp(&b.len())
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The versions the index's table rows link, in the order of the rows. A
/// row is a table line whose first cell is a link to `VERSION.md`.
pub fn index_rows(index: &str) -> Vec<String> {
    index
        .lines()
        .filter_map(|line| {
            let cell = line.strip_prefix('|')?.split('|').next()?.trim();
            let target = cell.split_once("](")?.1.strip_suffix(')')?;
            target.strip_suffix(".md").map(str::to_owned)
        })
        .collect()
}

/// Each fault of the releases' index, `docs/releases/index.md`, against
/// the pages beside it, one sentence each: a page with no row, a row that
/// names no page or no version, and two rows out of order.
pub fn release_index_faults(releases: &Path) -> Result<Vec<String>, String> {
    let index_path = releases.join("index.md");
    let index = std::fs::read_to_string(&index_path)
        .map_err(|e| format!("{}: {e}", index_path.display()))?;
    let rows = index_rows(&index);
    let mut out = Vec::new();
    let mut pages = Vec::new();
    for entry in std::fs::read_dir(releases).map_err(|e| format!("{}: {e}", releases.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", releases.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".md")
            && stem != "index"
        {
            pages.push(stem.to_owned());
        }
    }
    pages.sort();
    for page in &pages {
        if !rows.contains(page) {
            out.push(format!(
                "docs/releases/{page}.md has no row in docs/releases/index.md"
            ));
        }
    }
    let mut versions = Vec::new();
    for row in &rows {
        if !pages.contains(row) {
            out.push(format!(
                "docs/releases/index.md has a row for {row}, which has no page"
            ));
        }
        match Version::parse(row) {
            Ok(v) => versions.push(v),
            Err(e) => out.push(format!("docs/releases/index.md: {e}")),
        }
    }
    for pair in versions.windows(2) {
        if pair[0] <= pair[1] {
            out.push(format!(
                "docs/releases/index.md lists {} before {}, but the newest release comes first",
                pair[0].as_str(),
                pair[1].as_str()
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: &str = "https://vtium.github.io/vtium/";
    const PAGE: &str = "releases/0.1.0-alpha.8.md";

    fn github(text: &str) -> String {
        for_github(text, PAGE, SITE).unwrap()
    }

    #[test]
    fn a_link_with_an_anchor_keeps_its_anchor() {
        assert_eq!(
            github("See [the install](../install.md#what-the-install-does-to-your-vts).\n"),
            "See [the install](https://vtium.github.io/vtium/install/#what-the-install-does-to-your-vts).\n"
        );
    }

    #[test]
    fn a_link_to_an_index_page_becomes_its_folder() {
        assert_eq!(
            github(
                "[Concepts](../concepts/index.md), [home](../index.md), [showcase](../showcase/README.md) and [releases](index.md)"
            ),
            "[Concepts](https://vtium.github.io/vtium/concepts/), [home](https://vtium.github.io/vtium/), [showcase](https://vtium.github.io/vtium/showcase/) and [releases](https://vtium.github.io/vtium/releases/)"
        );
    }

    #[test]
    fn a_full_address_stays_as_it_is() {
        let text = "[the issue](https://github.com/vtium/vtium/issues/370) and [mail](mailto:x@example.org)\n";
        assert_eq!(github(text), text);
    }

    #[test]
    fn a_page_in_a_folder_becomes_its_own_folder() {
        assert_eq!(
            github("[Security](../concepts/security.md \"Who runs as what\")"),
            "[Security](https://vtium.github.io/vtium/concepts/security/ \"Who runs as what\")"
        );
    }

    #[test]
    fn a_picture_keeps_its_file_name() {
        assert_eq!(
            github("![a frame](../figures/frame.svg)"),
            "![a frame](https://vtium.github.io/vtium/figures/frame.svg)"
        );
    }

    #[test]
    fn an_anchor_on_the_same_page_names_the_page() {
        assert_eq!(
            github("[below](#upgrading-from-010-alpha7)"),
            "[below](https://vtium.github.io/vtium/releases/0.1.0-alpha.8/#upgrading-from-010-alpha7)"
        );
    }

    #[test]
    fn a_link_definition_is_rewritten() {
        assert_eq!(
            github("[fonts]: ../fonts.md \"Fonts\"\n"),
            "[fonts]: https://vtium.github.io/vtium/fonts/ \"Fonts\"\n"
        );
    }

    #[test]
    fn code_keeps_its_bytes() {
        let text = "Run `[x](../a.md)` and ``a ` [y](../b.md)`` then [z](../c.md).\n```md\n[w](../d.md)\n```\n[v](../e.md)\n";
        assert_eq!(
            github(text),
            "Run `[x](../a.md)` and ``a ` [y](../b.md)`` then [z](https://vtium.github.io/vtium/c/).\n```md\n[w](../d.md)\n```\n[v](https://vtium.github.io/vtium/e/)\n"
        );
    }

    #[test]
    fn a_link_that_leaves_docs_is_an_error() {
        assert!(for_github("[x](../../README.md)", PAGE, SITE).is_err());
    }

    #[test]
    fn versions_order_as_semantic_versioning_does() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("0.1.0-alpha.10") > v("0.1.0-alpha.9"));
        assert!(v("0.1.0") > v("0.1.0-alpha.9"));
        assert!(v("0.1.0-beta.1") > v("0.1.0-alpha.9"));
        assert!(v("0.2.0-alpha.1") > v("0.1.0"));
        assert!(v("0.1.0-alpha.1.1") > v("0.1.0-alpha.1"));
        assert!(Version::parse("alpha").is_err());
        assert!(Version::parse("0.1").is_err());
    }

    #[test]
    fn the_index_rows_are_its_table_lines_that_link_a_page() {
        let index = "| Release | Date |\n|---|---|\n| [0.1.0-alpha.2](0.1.0-alpha.2.md) | 2026-11-03 |\n| [0.1.0-alpha.1](0.1.0-alpha.1.md) | 2026-11-02 |\n\nSee [alpha.1](0.1.0-alpha.1.md).\n";
        assert_eq!(index_rows(index), ["0.1.0-alpha.2", "0.1.0-alpha.1"]);
    }
}
