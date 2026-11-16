//! The table of escape sequences that the reference pages under
//! `docs/sequences/` are generated from, and the test that keeps those pages
//! current.
//!
//! ADR-1000's features table says, for each row, whether vtium supports a
//! feature and why, and which tests cover it. It does not say how a sequence
//! is written, what its parameters mean or what vtium does with it. This
//! table adds those facts, one entry for each row that names a sequence, so
//! that a program's author can look a sequence up and read on one page what
//! vtium does with it. Rows that name no sequence, such as a font feature,
//! are listed in [`OTHER`] with a few words on what they are instead.
//!
//! The entries live in the files beside this one, one for each family of
//! ids, next to the code that handles the sequences, so that a change to how
//! vtium handles a sequence and a change to its entry go in one commit. The
//! test `generated_documents_of_the_sequences_match_the_tables` writes every
//! page and the index and compares each with the committed file.
//! `VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents` rewrites
//! them, and `docs/scripts/coverage --update` runs that after it rewrites
//! ADR-1000's tests column, because each page lists its row's tests.
//!
//! References:
//! - ADR-1507 D1: what each page and the index hold, and the two links each
//!   page ends with.
//! - ADR-1507 D2: this table, and why it lives in vtium-term.
//! - ADR-1507 D3: what the test checks.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "the module is a test, which fails by panicking"
)]

mod clipboard;
mod color;
mod ctl;
mod cursor;
mod image;
mod input;
mod platform;
mod query;
mod sgr;
mod text;
mod window;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The repository's root, from the crate's directory.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
/// Where the pages go, under the root.
const PAGES: &str = "docs/sequences";
/// The address of a file on GitHub, for links from a page to the source.
const BLOB: &str = "https://github.com/vtium/vtium/blob/main/";
/// The address of an issue on GitHub, before its number.
const ISSUES: &str = "https://github.com/vtium/vtium/issues/";

/// One row of ADR-1000's features table that names a sequence.
pub(super) struct Entry {
    /// The row's id in ADR-1000's features table.
    pub id: &'static str,
    /// Each sequence of the row, in the notation of xterm's control
    /// sequences document.
    pub forms: &'static [Form],
    /// The parameters that the forms name.
    pub params: &'static [Param],
    /// What vtium does with the sequences: each string is a paragraph.
    pub does: &'static [&'static str],
    /// Where vtium departs from what a specification says, if it does.
    pub departure: Option<Departure>,
    /// The decision record, and its section, that governs the row.
    pub record: Record,
}

/// One sequence's form and a short phrase that names what it does.
pub(super) struct Form {
    /// The form, such as `CSI Ps J`. Its first word says which group of the
    /// index it belongs to: `CSI`, `OSC`, `DCS`, `APC` or `ESC`, and any other
    /// word makes it a control character.
    pub form: &'static str,
    /// What the sequence does, as a short phrase: "erase in display".
    pub name: &'static str,
}

/// A parameter of a form.
pub(super) struct Param {
    /// The parameter's name in the form, such as `Ps`.
    pub name: &'static str,
    /// What the parameter means, as one or more sentences.
    pub meaning: &'static str,
    /// The parameter's default, when it has one, such as "0".
    pub default: Option<&'static str>,
}

/// A difference between what vtium does and what a specification says.
pub(super) struct Departure {
    /// The difference, as a sentence.
    pub says: &'static str,
    /// The README that records it, from the repository's root.
    pub readme: &'static str,
    /// The README's heading that records it.
    pub section: &'static str,
}

/// A decision record and the section of it that governs a row.
pub(super) struct Record {
    /// The record's number, such as 1000.
    pub adr: u32,
    /// The section's heading, or the start of it: "The target set", or "D2"
    /// for the heading "D2: ...".
    pub section: &'static str,
}

/// A row of ADR-1000's features table that names no sequence.
pub(super) struct Other {
    /// The row's id.
    pub id: &'static str,
    /// What the row is instead, as a short phrase: "a font feature".
    pub what: &'static str,
}

/// The rows of the governing record used by most entries.
pub(super) const TARGET_SET: Record = Record {
    adr: 1000,
    section: "The target set",
};
/// The section of ADR-1000 that lists the refusals.
pub(super) const REFUSALS: Record = Record {
    adr: 1000,
    section: "Refusals",
};
/// The section of ADR-1000 that sets the replies' rules.
pub(super) const REPLIES: Record = Record {
    adr: 1000,
    section: "Replies and policy",
};

/// A family of ids: its entries, and its rows that name no sequence.
type Family = (&'static [Entry], &'static [Other]);

/// The families of ids.
const FAMILIES: &[Family] = &[
    (sgr::ENTRIES, sgr::OTHER),
    (color::ENTRIES, color::OTHER),
    (cursor::ENTRIES, cursor::OTHER),
    (text::ENTRIES, text::OTHER),
    (ctl::ENTRIES, ctl::OTHER),
    (query::ENTRIES, query::OTHER),
    (window::ENTRIES, window::OTHER),
    (input::ENTRIES, input::OTHER),
    (clipboard::ENTRIES, clipboard::OTHER),
    (image::ENTRIES, image::OTHER),
    (platform::ENTRIES, platform::OTHER),
];

/// Every entry, family by family.
fn entries() -> impl Iterator<Item = &'static Entry> {
    FAMILIES.iter().flat_map(|family| family.0)
}

/// Every row that names no sequence, family by family.
fn others() -> impl Iterator<Item = &'static Other> {
    FAMILIES.iter().flat_map(|family| family.1)
}

/// A row of ADR-1000's features table, with the columns the pages show.
struct Row {
    feature: String,
    spec: String,
    status: String,
    reason: String,
    note: String,
    issue: String,
    tests: String,
}

/// ADR-1000's features table, by id.
fn features() -> BTreeMap<String, Row> {
    let path = format!("{ROOT}/docs/architecture/term/ADR-1000-features.tsv");
    let text = std::fs::read_to_string(&path).expect("ADR-1000-features.tsv");
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().expect("a header").split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).expect(name);
    let [id, feature, spec, status, reason, note, issue, tests] = [
        "id", "feature", "spec", "status", "reason", "note", "issue", "tests",
    ]
    .map(col);
    lines
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            let cell = |i: usize| cells.get(i).copied().unwrap_or("").to_owned();
            let row = Row {
                feature: cell(feature),
                spec: cell(spec),
                status: cell(status),
                reason: cell(reason),
                note: cell(note),
                issue: cell(issue),
                tests: cell(tests),
            };
            (cell(id), row)
        })
        .collect()
}

/// The groups of the index, in the order ADR-1507 D1 gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Control,
    Esc,
    Csi,
    Osc,
    Dcs,
    Apc,
}

impl Group {
    fn of(form: &str) -> Group {
        match form.split_whitespace().next() {
            Some("ESC") => Group::Esc,
            Some("CSI") => Group::Csi,
            Some("OSC") => Group::Osc,
            Some("DCS") => Group::Dcs,
            Some("APC") => Group::Apc,
            _ => Group::Control,
        }
    }

    fn heading(self) -> &'static str {
        match self {
            Group::Control => "Control characters",
            Group::Esc => "ESC sequences",
            Group::Csi => "CSI sequences",
            Group::Osc => "OSC strings",
            Group::Dcs => "DCS strings",
            Group::Apc => "APC strings",
        }
    }

    fn intro(self) -> &'static str {
        match self {
            Group::Control => {
                "A control character is a single byte. The C0 controls are the bytes 0x00 to 0x1F, and the C1 controls are 0x80 to 0x9F, which a program can also write as ESC and a second byte."
            }
            Group::Esc => {
                "An ESC sequence is ESC followed by a few bytes, the last of which says what the sequence does."
            }
            Group::Csi => {
                "A CSI sequence is ESC [ followed by numeric parameters and a final byte that says what the sequence does. A private marker such as `?` or `>` can come before the parameters, and intermediate bytes such as `$` or a space can come before the final byte. Either one makes the sequence a different function. The sequences below are sorted by their final byte."
            }
            Group::Osc => {
                "An OSC string is ESC ] followed by text, and ST or BEL ends it. Most strings start with a number that gives the text its meaning. A few start with a word or a letter instead, such as `setBackground` or `l`. The strings below are sorted by their number."
            }
            Group::Dcs => {
                "A DCS string is ESC P followed by parameters, a final byte, the data, and ST to end it."
            }
            Group::Apc => {
                "An APC string is ESC _ followed by data and ST. vtium reads kitty's graphics protocol from it."
            }
        }
    }
}

/// The C0 and C1 controls by name, for sorting the controls by their code.
const CONTROLS: &[(&str, u32)] = &[
    ("NUL", 0x00),
    ("ENQ", 0x05),
    ("BEL", 0x07),
    ("BS", 0x08),
    ("HT", 0x09),
    ("LF", 0x0A),
    ("VT", 0x0B),
    ("FF", 0x0C),
    ("CR", 0x0D),
    ("SO", 0x0E),
    ("LS1", 0x0E),
    ("SI", 0x0F),
    ("LS0", 0x0F),
    ("CAN", 0x18),
    ("SUB", 0x1A),
    ("ESC", 0x1B),
    ("DEL", 0x7F),
    ("IND", 0x84),
    ("NEL", 0x85),
    ("HTS", 0x88),
    ("RI", 0x8D),
    ("SS2", 0x8E),
    ("SS3", 0x8F),
    ("DCS", 0x90),
    ("CSI", 0x9B),
    ("ST", 0x9C),
    ("OSC", 0x9D),
    ("PM", 0x9E),
    ("APC", 0x9F),
];

/// How a form sorts within its group.
type SortKey = (u32, u32, String);

/// The key a form sorts by within its group: a control's code, an OSC
/// string's number, or the final byte of the other sequences; then the first
/// number in the form, so that `CSI 2 m` comes before `CSI 10 m`; then the
/// form itself.
fn sort_key(form: &str) -> SortKey {
    let words: Vec<&str> = form.split_whitespace().collect();
    let first = |word: &str| word.chars().next().map_or(u32::MAX, u32::from);
    let key = match Group::of(form) {
        Group::Control => CONTROLS
            .iter()
            .find(|(name, _)| Some(name) == words.first())
            .map_or(u32::MAX, |c| c.1),
        Group::Osc => words
            .get(1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(u32::MAX),
        Group::Esc | Group::Csi => words.last().map_or(u32::MAX, |w| first(w)),
        Group::Dcs | Group::Apc => 0,
    };
    let number = words.iter().find_map(|w| w.parse().ok()).unwrap_or(0);
    (key, number, form.to_owned())
}

/// A form in a code span for a table cell. A form that holds a `|` is
/// written as HTML, with the `|` as `&#124;`, because the site's tables read
/// a `\\|` inside a code span as two characters, while GitHub's split the
/// cell at a bare `|`. A form that holds a backtick, as HPA's does, goes
/// between double backticks.
fn code(form: &str) -> String {
    if form.contains('|') {
        let html = form
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('|', "&#124;");
        format!("<code>{html}</code>")
    } else if form.contains('`') {
        format!("`` {form} ``")
    } else {
        format!("`{form}`")
    }
}

/// A heading's anchor, as the site and GitHub make it: lower case, with
/// every character but a letter, a digit, `_`, `-` or a space dropped, and
/// each space turned into `-`.
fn anchor(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// The headings of a markdown file, as their text without the `#` marks and
/// the backticks.
fn headings(text: &str) -> Vec<String> {
    let mut fenced = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
        } else if !fenced && line.starts_with('#') {
            let heading = line.trim_start_matches('#');
            if heading.starts_with(' ') {
                out.push(heading.trim().replace('`', ""));
            }
        }
    }
    out
}

/// A decision record's file, from `docs/`, and the heading of the section
/// an entry names, or why they cannot be found.
fn find_record(record: &Record) -> Result<(String, String), String> {
    let prefix = format!("ADR-{}-", record.adr);
    let architecture = Path::new(ROOT).join("docs/architecture");
    let mut found = None;
    for folder in std::fs::read_dir(&architecture).map_err(|e| e.to_string())? {
        let folder = folder.map_err(|e| e.to_string())?.path();
        let Ok(files) = std::fs::read_dir(&folder) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) && name.ends_with(".md") {
                let dir = folder
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                found = Some((format!("architecture/{dir}/{name}"), file.path()));
            }
        }
    }
    let (link, path) =
        found.ok_or_else(|| format!("no record ADR-{} under docs/architecture", record.adr))?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let heading = headings(&text)
        .into_iter()
        .find(|h| h == record.section || h.starts_with(&format!("{}:", record.section)))
        .ok_or_else(|| format!("ADR-{} has no section {:?}", record.adr, record.section))?;
    Ok((link, heading))
}

/// How a page names a record's section: `ADR-1004 D2`, or
/// `ADR-1000, "The target set"`.
fn record_label(record: &Record) -> String {
    let numbered = record
        .section
        .strip_prefix('D')
        .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()));
    if numbered {
        format!("ADR-{} {}", record.adr, record.section)
    } else {
        format!("ADR-{}, \"{}\"", record.adr, record.section)
    }
}

/// The names that some specifications have in the spec column, with the
/// name a page shows and the address it links.
const SPECS: &[(&str, &str, &str)] = &[
    (
        "ctlseqs",
        "xterm's control sequences",
        "https://invisible-island.net/xterm/ctlseqs/ctlseqs.html",
    ),
    (
        "ECMA-48",
        "ECMA-48",
        "https://ecma-international.org/publications-and-standards/standards/ecma-48/",
    ),
    (
        "VT510",
        "the VT510 reference manual",
        "https://vt100.net/docs/vt510-rm/contents.html",
    ),
    (
        "console_codes(4)",
        "console_codes(4)",
        "https://man7.org/linux/man-pages/man4/console_codes.4.html",
    ),
    (
        "foot(7)",
        "foot-ctlseqs(7)",
        "https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd",
    ),
    (
        "terminfo(5)",
        "terminfo(5)",
        "https://man7.org/linux/man-pages/man5/terminfo.5.html",
    ),
];

/// One specification of the spec column, linked when [`SPECS`] knows it.
fn spec(text: &str) -> String {
    for (name, shown, url) in SPECS {
        if let Some(rest) = text.strip_prefix(name) {
            return format!("[{shown}]({url}){rest}");
        }
    }
    text.to_owned()
}

/// What a status means, as the page says it. A supported row whose entry
/// records a departure says that vtium follows the specifications apart
/// from it.
fn status_meaning(status: &str, departs: bool) -> &'static str {
    match status {
        "supported" if departs => {
            "vtium implements this as the specifications below define it, apart from the departure above."
        }
        "supported" => "vtium implements this as the specifications below define it.",
        "partial" => "vtium implements part of this. The note says what is missing.",
        "planned" => "vtium does not implement this yet. The issue below tracks the work.",
        "weighed" => "Another issue decides whether vtium implements this.",
        "ignored" => "vtium reads the sequence and does nothing with it.",
        "refused" => "vtium deliberately does not implement this.",
        _ => "",
    }
}

/// A phrase from the table, ending in a full stop. Its first letter keeps
/// its case, since many phrases start with a name such as vtium or kitty.
fn sentence(phrase: &str) -> String {
    let mut out = phrase.to_owned();
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// The tests column's entries, each as a list item that links its file.
fn test_items(tests: &str) -> Vec<String> {
    tests
        .split(", ")
        .filter(|t| !t.is_empty())
        .map(|test| match test.split_once("::") {
            Some((file, name)) => format!("- `{name}` in [{file}]({BLOB}{file})"),
            None => format!("- [{test}]({BLOB}{test})"),
        })
        .collect()
}

/// The issues of the issue column, as links: "issue #12", or "issues #12
/// and #23".
fn issue_links(issue: &str) -> Option<String> {
    let numbers: Vec<&str> = issue
        .split(',')
        .map(|n| n.trim().trim_start_matches('#'))
        .filter(|n| !n.is_empty())
        .collect();
    let links: Vec<String> = numbers
        .iter()
        .map(|n| format!("[#{n}]({ISSUES}{n})"))
        .collect();
    match links.as_slice() {
        [] => None,
        [one] => Some(format!("issue {one}")),
        [rest @ .., last] => Some(format!("issues {} and {last}", rest.join(", "))),
    }
}

/// The line that opens every generated page, which says where the page comes
/// from. It starts with "Generated from", so `docs/scripts/prose` knows the
/// page is generated.
const GENERATED: &str = "<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->\n\n";

/// The page of one entry.
fn page(entry: &Entry, row: &Row) -> Result<String, String> {
    let mut out = String::from(GENERATED);
    let _ = writeln!(out, "# {}\n", row.feature);
    out += "| Sequence | What it does |\n|---|---|\n";
    for form in entry.forms {
        let _ = writeln!(out, "| {} | {} |", code(form.form), form.name);
    }
    out += "\n";
    if !entry.params.is_empty() {
        out += "## Parameters\n\n";
        for param in entry.params {
            let _ = write!(out, "- `{}`: {}", param.name, param.meaning);
            if let Some(default) = param.default {
                let _ = write!(out, " The default is {default}.");
            }
            out += "\n";
        }
        out += "\n";
    }
    out += "## What vtium does\n\n";
    for paragraph in entry.does {
        let _ = writeln!(out, "{paragraph}\n");
    }
    if let Some(departure) = &entry.departure {
        let readme = std::fs::read_to_string(Path::new(ROOT).join(departure.readme))
            .map_err(|e| format!("{}: {e}", departure.readme))?;
        if !headings(&readme).iter().any(|h| h == departure.section) {
            return Err(format!(
                "{} has no section {:?}",
                departure.readme, departure.section
            ));
        }
        let crate_name = departure
            .readme
            .split('/')
            .nth(1)
            .unwrap_or(departure.readme);
        let _ = writeln!(
            out,
            "{} See [{} in {crate_name}'s README]({BLOB}{}#{}).\n",
            departure.says,
            departure.section,
            departure.readme,
            anchor(departure.section)
        );
    }
    out += "## Status\n\n";
    let _ = writeln!(
        out,
        "- **Status:** {}. {}",
        row.status,
        status_meaning(&row.status, entry.departure.is_some())
    );
    if !row.reason.is_empty() {
        let _ = writeln!(out, "- **Why:** {}", sentence(&row.reason));
    }
    if !row.note.is_empty() {
        let _ = writeln!(out, "- **Note:** {}", sentence(&row.note));
    }
    let specs: Vec<String> = row.spec.split("; ").map(spec).collect();
    let _ = writeln!(out, "- **Specifications:** {}", specs.join("; "));
    out += "\n## Tests\n\n";
    let tests = test_items(&row.tests);
    if tests.is_empty() {
        out += "No test covers this row yet.\n";
    } else {
        out += "These tests check what vtium does with the sequences on this page.\n\n";
        for test in tests {
            let _ = writeln!(out, "{test}");
        }
    }
    out += "\n## Where to read more\n\n";
    if let Some(issues) = issue_links(&row.issue) {
        let _ = writeln!(out, "To follow the work on this row, see {issues}.\n");
    }
    let (link, heading) = find_record(&entry.record)?;
    let _ = writeln!(
        out,
        "To read why vtium does what it does here, see [{}](../{link}#{}).",
        record_label(&entry.record),
        anchor(&heading)
    );
    Ok(out)
}

/// The index of every sequence, group by group.
fn index(rows: &BTreeMap<String, Row>) -> String {
    let mut out = String::from(GENERATED);
    out += "# Escape sequences\n\n\
            A program controls vtium by writing escape sequences and control characters among its text. \
            [Escape sequences](../concepts/escape-sequences.md) explains how they are put together, for a reader who has never met them. \
            The modes that a program sets and resets with SM and RM are on a page of their own, [Modes](../modes.md).\n\n\
            Each line below gives a sequence's form, what it does, and whether vtium supports it. \
            Its link leads to the page of the row in ADR-1000's features table that holds the sequence. \
            A row whose sequences fall in two groups appears in each.\n\n\
            The forms use the notation of [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html). \
            `Ps` stands for one number, `Pm` for a list of numbers separated by `;`, and `Pt` for text.\n\n";
    let mut groups: BTreeMap<Group, Vec<(SortKey, String)>> = BTreeMap::new();
    for entry in entries() {
        let status = rows.get(entry.id).map_or("", |r| r.status.as_str());
        for form in entry.forms {
            let line = format!(
                "| {} | {} | {status} | [{}]({}.md) |",
                code(form.form),
                form.name,
                entry.id,
                entry.id
            );
            groups
                .entry(Group::of(form.form))
                .or_default()
                .push((sort_key(form.form), line));
        }
    }
    for (group, mut lines) in groups {
        lines.sort();
        let _ = writeln!(out, "## {}\n\n{}\n", group.heading(), group.intro());
        out += "| Sequence | What it does | Status | Page |\n|---|---|---|---|\n";
        for (_, line) in lines {
            let _ = writeln!(out, "{line}");
        }
        out += "\n";
    }
    out
}

/// Every generated file under `docs/sequences/`, by its name, or the
/// problems that keep the pages from being generated.
fn documents() -> Result<BTreeMap<String, String>, Vec<String>> {
    let rows = features();
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for id in entries().map(|e| e.id).chain(others().map(|o| o.id)) {
        if !rows.contains_key(id) {
            problems.push(format!(
                "{id} is in the table of sequences but not in ADR-1000's features table"
            ));
        }
        if !seen.insert(id) {
            problems.push(format!("{id} is in the table of sequences twice"));
        }
    }
    for id in rows.keys() {
        if !seen.contains(id.as_str()) {
            problems.push(format!(
                "{id}, a row of ADR-1000's features table, has no entry in the table of sequences and is not listed as a row that names no sequence"
            ));
        }
    }
    for other in others().filter(|o| o.what.is_empty()) {
        problems.push(format!(
            "{} needs a few words on what the row is instead of a sequence",
            other.id
        ));
    }
    let mut docs = BTreeMap::new();
    for entry in entries() {
        if entry.forms.is_empty() || entry.does.is_empty() {
            problems.push(format!(
                "{} needs a form and a sentence on what vtium does",
                entry.id
            ));
        }
        let Some(row) = rows.get(entry.id) else {
            continue;
        };
        match page(entry, row) {
            Ok(text) => {
                docs.insert(format!("{}.md", entry.id), text);
            }
            Err(problem) => problems.push(format!("{}: {problem}", entry.id)),
        }
    }
    docs.insert("index.md".to_owned(), index(&rows));
    if problems.is_empty() {
        Ok(docs)
    } else {
        Err(problems)
    }
}

// ADR-1507 D3: every page and the index are generated from the tables, and
// the test fails when a committed file differs, when a row of the features
// table has no entry, when an entry names an id or a record that does not
// exist, and when a file under docs/sequences/ has no entry.
#[test]
fn generated_documents_of_the_sequences_match_the_tables() {
    let docs = match documents() {
        Ok(docs) => docs,
        Err(problems) => panic!("the table of sequences is wrong:\n{}", problems.join("\n")),
    };
    let dir = PathBuf::from(ROOT).join(PAGES);
    let bless = std::env::var_os("VTIUM_TERM_BLESS").is_some();
    if bless {
        std::fs::create_dir_all(&dir).expect("create docs/sequences");
    }
    let mut stale = Vec::new();
    for (name, want) in &docs {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() == Some(want.as_str()) {
            continue;
        }
        if bless {
            std::fs::write(&path, want).expect("rewrite the page");
        } else {
            stale.push(name.clone());
        }
    }
    let mut orphans = Vec::new();
    for file in std::fs::read_dir(&dir).expect("docs/sequences").flatten() {
        let name = file.file_name().to_string_lossy().into_owned();
        if !docs.contains_key(&name) {
            if bless {
                std::fs::remove_file(file.path())
                    .expect("remove the page of a row that left the table");
            } else {
                orphans.push(name);
            }
        }
    }
    assert!(
        stale.is_empty() && orphans.is_empty(),
        "docs/sequences/ differs from the tables. Stale: {stale:?}. With no entry: {orphans:?}. \
         Rerun with VTIUM_TERM_BLESS=1 to rewrite the pages."
    );
}

#[test]
fn the_index_sorts_each_group_by_final_byte_or_number() {
    assert!(sort_key("CSI Ps A") < sort_key("CSI Ps J"));
    assert!(sort_key("CSI Ps SP q") > sort_key("CSI Ps J"));
    assert!(sort_key("OSC 4 ; c ; spec ST") < sort_key("OSC 10 ; spec ST"));
    assert!(sort_key("BEL") < sort_key("CR"));
    assert!(sort_key("CSI 2 m") < sort_key("CSI 10 m"));
    assert_eq!(Group::of("ESC # 8"), Group::Esc);
    assert_eq!(Group::of("BEL"), Group::Control);
}

#[test]
fn an_anchor_is_made_as_the_site_makes_it() {
    assert_eq!(anchor("Replies and policy"), "replies-and-policy");
    assert_eq!(anchor("Sized text (OSC 66)"), "sized-text-osc-66");
    assert_eq!(
        anchor("D2: the table the pages come from"),
        "d2-the-table-the-pages-come-from"
    );
}

#[test]
fn a_form_in_a_table_cell_keeps_its_bars_and_backticks() {
    assert_eq!(code("CSI Ps J"), "`CSI Ps J`");
    assert_eq!(code("CSI Ps `"), "`` CSI Ps ` ``");
    assert_eq!(
        code("DCS Ps ; Ps | Pt ST"),
        "<code>DCS Ps ; Ps &#124; Pt ST</code>"
    );
    assert_eq!(code("CSI > Ps | x"), "<code>CSI &gt; Ps &#124; x</code>");
}

#[test]
fn a_supported_row_with_a_departure_says_so_in_its_status() {
    assert_eq!(
        status_meaning("supported", false),
        "vtium implements this as the specifications below define it."
    );
    assert_eq!(
        status_meaning("supported", true),
        "vtium implements this as the specifications below define it, apart from the departure above."
    );
}
