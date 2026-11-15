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
/// anchor is kept, and a target written in angle brackets keeps them. A
/// link that names a scheme, such as `https:` or `mailto:`, is kept as it
/// is. So is everything that is not a link: a code span, which may run
/// across the lines of one paragraph, a fenced code block, and a `](` with
/// no `[` to open it. A link that leaves `docs/` is an error, since the
/// site has no address for it.
///
/// It reads the subset of CommonMark that release notes use: inline links
/// and images, link definitions, code spans, fenced code blocks, and
/// backslash escapes. A blank line, a fence, a heading, a list item, a
/// quote or a table row starts a new block, and a code span never runs
/// from one block into the next.
pub fn for_github(text: &str, page: &str, site_url: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<String> = None;
    // The lines of the block read so far, which a code span may cross.
    let mut block = String::new();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str()) {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        let opens_fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        let def = definition(line);
        if opens_fence || def.is_some() || trimmed.trim_end().is_empty() || starts_block(trimmed) {
            out.push_str(&inline_links(&block, page, site_url)?);
            block.clear();
        }
        if opens_fence {
            let mark = &trimmed[..1];
            let run = trimmed.len() - trimmed.trim_start_matches(mark).len();
            fence = Some(mark.repeat(run));
            out.push_str(line);
        } else if let Some((head, target, tail)) = def {
            out.push_str(head);
            out.push_str(&address(target, page, site_url)?);
            out.push_str(tail);
        } else {
            block.push_str(line);
        }
    }
    out.push_str(&inline_links(&block, page, site_url)?);
    Ok(out)
}

/// Whether a line, with its indent removed, starts a block of its own: a
/// heading, a list item, a quote or a table row.
fn starts_block(trimmed: &str) -> bool {
    let digits = trimmed.len()
        - trimmed
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .len();
    let after_digits = &trimmed[digits..];
    trimmed.starts_with('#')
        || trimmed.starts_with('>')
        || trimmed.starts_with('|')
        || ["- ", "* ", "+ "].iter().any(|m| trimmed.starts_with(m))
        || (digits > 0 && (after_digits.starts_with(". ") || after_digits.starts_with(") ")))
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

/// The block with each inline link's target rewritten.
///
/// It reads the block once, from the left. A backslash keeps the character
/// after it as text. A run of backticks opens a code span that runs to the
/// next run of exactly as many backticks, as CommonMark reads it, and an
/// unmatched run is text. Each `[` outside a code span is remembered, and a
/// `]` closes the last one still open; a `]` with none open is text. When
/// the `]` that closes a `[` comes right before `(`, the two are a link,
/// and its target is rewritten.
fn inline_links(block: &str, page: &str, site_url: &str) -> Result<String, String> {
    let bytes = block.as_bytes();
    let mut out = String::with_capacity(block.len());
    let mut open = 0usize;
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'`' => {
                let run = block[i..].len() - block[i..].trim_start_matches('`').len();
                i += code_span(block, i, run).unwrap_or(run);
            }
            b'[' => {
                open += 1;
                i += 1;
            }
            b']' if open > 0 => {
                open -= 1;
                i += 1;
                if bytes.get(i) == Some(&b'(')
                    && let Some((from, to, angle)) = destination(block, i + 1)
                {
                    out.push_str(&block[copied..from]);
                    let full = address(&block[from..to], page, site_url)?;
                    out.push_str(&full);
                    copied = to;
                    i = to + usize::from(angle);
                }
            }
            _ => i += 1,
        }
    }
    out.push_str(&block[copied..]);
    Ok(out)
}

/// The length of the code span that opens with `run` backticks at `at`,
/// through its closing run, or `None` when no run of exactly `run`
/// backticks closes it.
fn code_span(block: &str, at: usize, run: usize) -> Option<usize> {
    let mut j = at + run;
    while let Some(next) = block[j..].find('`') {
        let start = j + next;
        let len = block[start..].len() - block[start..].trim_start_matches('`').len();
        if len == run {
            return Some(start + len - at);
        }
        j = start + len;
    }
    None
}

/// The target of the link whose `(` ends just before `at`, as the byte
/// range of the target itself and whether it sits in angle brackets. A
/// target in angle brackets runs to the `>` on the same line. A bare target
/// runs to the first space, or to the `)` that balances the parentheses
/// inside it. `None` when the link does not close.
fn destination(block: &str, at: usize) -> Option<(usize, usize, bool)> {
    let rest = &block[at..];
    if let Some(inner) = rest.strip_prefix('<') {
        let end = inner.find(['>', '\n'])?;
        return (inner.as_bytes()[end] == b'>').then_some((at + 1, at + 1 + end, true));
    }
    let mut depth = 0usize;
    for (k, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some((at, at + k, false)),
            ')' => depth -= 1,
            c if c.is_whitespace() => return Some((at, at + k, false)),
            _ => {}
        }
    }
    None
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
/// pre-release part's fields in turn, a numeric field by its value and
/// before any field with a letter or a hyphen.
///
/// A version names the file of its page, so [`Version::parse`] takes only
/// what semantic versioning allows: three numbers, and an optional
/// pre-release part after a `-`, made of fields of ASCII letters, digits
/// and hyphens. No field is empty and no number has a leading zero, so
/// each version has one spelling, and two versions compare equal only
/// when their text is the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    text: String,
    numbers: [u64; 3],
    pre: Option<Vec<Field>>,
}

/// One field of a pre-release part.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    // Declared first, so a numeric field orders before any other.
    Number(u64),
    Text(String),
}

/// `text` as a number of semantic versioning: digits only, no leading
/// zero, and small enough for a `u64`.
fn number(text: &str) -> Option<u64> {
    let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    if !digits || (text.len() > 1 && text.starts_with('0')) {
        return None;
    }
    text.parse().ok()
}

impl Version {
    /// Reads a version, or says why `text` is not one.
    pub fn parse(text: &str) -> Result<Version, String> {
        let refuse = || format!("`{text}` is not a version such as 0.1.0-alpha.8");
        let (core, pre) = match text.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (text, None),
        };
        let numbers: Vec<u64> = core
            .split('.')
            .map(number)
            .collect::<Option<_>>()
            .ok_or_else(refuse)?;
        let numbers: [u64; 3] = numbers.try_into().map_err(|_| refuse())?;
        let pre = match pre {
            None => None,
            Some(p) => Some(
                p.split('.')
                    .map(|f| {
                        let allowed = !f.is_empty()
                            && f.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
                        if !allowed {
                            None
                        } else if f.bytes().all(|b| b.is_ascii_digit()) {
                            number(f).map(Field::Number)
                        } else {
                            Some(Field::Text(f.to_owned()))
                        }
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(refuse)?,
            ),
        };
        Ok(Version {
            text: text.to_owned(),
            numbers,
            pre,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numbers
            .cmp(&other.numbers)
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                // A Vec compares field by field, and a shorter list that is
                // a prefix of the other comes first, as semantic versioning
                // orders pre-release parts.
                (Some(a), Some(b)) => a.cmp(b),
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
/// names no page or no version, a version listed twice, and two rows out of
/// order.
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
    let mut versions: Vec<Version> = Vec::new();
    for row in &rows {
        if !pages.contains(row) {
            out.push(format!(
                "docs/releases/index.md has a row for {row}, which has no page"
            ));
        }
        match Version::parse(row) {
            Ok(v) if versions.contains(&v) => {
                out.push(format!("docs/releases/index.md lists {row} twice"));
            }
            Ok(v) => versions.push(v),
            Err(e) => out.push(format!("docs/releases/index.md: {e}")),
        }
    }
    for pair in versions.windows(2) {
        if pair[0] < pair[1] {
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
    fn an_angle_bracket_target_keeps_its_brackets() {
        assert_eq!(
            github("[x](<../a b.md#c>) and [y](<../d.md> \"D\")\n"),
            "[x](<https://vtium.github.io/vtium/a b/#c>) and [y](<https://vtium.github.io/vtium/d/> \"D\")\n"
        );
    }

    #[test]
    fn a_code_span_that_crosses_a_line_break_keeps_its_bytes() {
        let text = "A `span that\n[x](../a.md) ends` here, then [y](../b.md).\n";
        assert_eq!(
            github(text),
            "A `span that\n[x](../a.md) ends` here, then [y](https://vtium.github.io/vtium/b/).\n"
        );
    }

    #[test]
    fn a_blank_line_ends_an_unclosed_code_span() {
        assert_eq!(
            github("An unclosed `tick\n\n[x](../a.md) and a `tick`\n"),
            "An unclosed `tick\n\n[x](https://vtium.github.io/vtium/a/) and a `tick`\n"
        );
    }

    #[test]
    fn a_bracket_with_no_opening_bracket_is_text() {
        let text = "f(x)](../a.md), \\[x\\](../b.md) and [a \\] b](../c.md)\n";
        assert_eq!(
            github(text),
            "f(x)](../a.md), \\[x\\](../b.md) and [a \\] b](https://vtium.github.io/vtium/c/)\n"
        );
    }

    /// CommonMark reads `a[i](j)` as the letter a followed by a link, so the
    /// link is rewritten.
    #[test]
    fn a_link_right_after_a_word_is_a_link() {
        assert_eq!(
            github("a[i](j.md)"),
            "a[i](https://vtium.github.io/vtium/releases/j/)"
        );
    }

    #[test]
    fn a_target_with_parentheses_is_read_to_its_matching_parenthesis() {
        assert_eq!(
            github("[x](../a.md#b(c)) (see)"),
            "[x](https://vtium.github.io/vtium/a/#b(c)) (see)"
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

    /// A version names a file, so it holds only semantic versioning's
    /// characters, and a number has one spelling, so two versions that
    /// differ in text never compare equal.
    #[test]
    fn a_version_outside_semantic_versioning_is_refused() {
        for text in [
            "0.1.0-",
            "0.1.0-../x",
            "0.1.0-a/b",
            "0.1.0-alpha..1",
            "0.1.0-alpha.01",
            "0.1.0-alpha.+1",
            "0.1.0-alpha.99999999999999999999",
            "01.1.0",
            "0.1.+0",
            "0.1.99999999999999999999",
            "0.1.0+build",
        ] {
            assert!(Version::parse(text).is_err(), "{text} was accepted");
        }
        for text in [
            "0.1.0",
            "0.1.0-alpha.8",
            "0.1.0-rc-1.0",
            "10.20.30-x.7.z.92",
        ] {
            assert!(Version::parse(text).is_ok(), "{text} was refused");
        }
    }

    #[test]
    fn the_index_rows_are_its_table_lines_that_link_a_page() {
        let index = "| Release | Date |\n|---|---|\n| [0.1.0-alpha.2](0.1.0-alpha.2.md) | 2026-11-03 |\n| [0.1.0-alpha.1](0.1.0-alpha.1.md) | 2026-11-02 |\n\nSee [alpha.1](0.1.0-alpha.1.md).\n";
        assert_eq!(index_rows(index), ["0.1.0-alpha.2", "0.1.0-alpha.1"]);
    }
}
