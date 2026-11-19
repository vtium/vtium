//! The TUI's contract as the TUI reads it: each node of the interface tree
//! with the key line and the help it shows there. The interface tree and
//! the key table are two TSV files beside ADR-1403. A generator, the test
//! `generated_help_and_page_match_the_contract` below, reads them and
//! writes `contract_text.rs`, which the TUI compiles in. The same test
//! writes the site's page "vtium-config's screens and keys",
//! `docs/reference/vtium-config-keys.md`. The test fails when either file
//! differs from what the generator writes, and
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --lib generated_help`
//! writes both again.
//!
//! A node's key line lists each of its supported rows that has a label,
//! the keys that share a label as one item. The key line is one row: where
//! the items do not fit on a row at 80 columns, it shows them in pages,
//! and each page ends with the item of `.`, `. more 1/3`, which shows the
//! next page. The first page holds the items of the rows that the key
//! table's `page` column marks with `1`, and the later pages hold the
//! other items in the key line's order, as many as fit on each. So the
//! pages are the same at every width. The help lists the same rows, each
//! with what it does. A node whose status is still planned gets no
//! generated key line or help: the TUI keeps its hand-written words there
//! until the work that builds the node moves it to supported. So the help
//! names only what the code does, and the scenario test checks that the
//! code does it.
//!
//! References:
//! - ADR-1403 D7, "The contract": the tree and the key table are the
//!   TUI's contract, and the Help tab, the key lines and the site's page
//!   are generated from them.
//! - ADR-1403 D7, "The same keys in every view": the key line is one row
//!   that shows its keys in pages.

/// A node of the interface tree, with what the TUI shows there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// The node's id, dotted by its parent, such as `flow.card`.
    pub id: &'static str,
    /// What the node is, in plain words.
    pub what: &'static str,
    /// The code has the node now.
    pub supported: bool,
    /// The pages of the node's key line: the keys that act in the node
    /// and their labels, each item two spaces from the next. A key line
    /// that fits on one row has one page; empty for a planned node.
    pub key_pages: &'static [&'static str],
    /// Each key that acts in the node, as the key line names it, and what
    /// it does; empty for a planned node.
    pub help: &'static [(&'static str, &'static str)],
    /// The id of each row of the key table that applies in the node, in
    /// the order the tree's `keys` column names them, and whether the row
    /// is supported. The navigation module reads these to decide what the
    /// keys that act the same in every tab do here, planned nodes
    /// included.
    pub rows: &'static [(&'static str, bool)],
}

pub use super::contract_text::NODES;

/// The node with the id `id`.
pub fn find(id: &str) -> Option<&'static Node> {
    NODES.iter().find(|n| n.id == id)
}

/// The columns a page of a key line may fill: an 80-column screen, the
/// smallest the TUI draws, less a column of margin on each side.
pub const PAGE_COLUMNS: usize = 78;

/// The pages of the generated key line of the node `id`, where the node
/// is supported.
pub fn key_pages(id: &str) -> Option<&'static [&'static str]> {
    find(id).filter(|n| n.supported).map(|n| n.key_pages)
}

#[cfg(test)]
mod generate {
    //! The generator: reads the key table and the tree, and writes
    //! `contract_text.rs` and the site's page.

    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::path::PathBuf;

    const KEYS: &str = "docs/architecture/platform/ADR-1403-keys.tsv";
    const TREE: &str = "docs/architecture/platform/ADR-1403-tree.tsv";
    const TEXT: &str = "crates/vtium-config/src/tui/contract_text.rs";
    const PAGE: &str = "docs/reference/vtium-config-keys.md";
    const ISSUES: &str = "https://github.com/vtium/vtium/issues/";

    type Row = BTreeMap<String, String>;

    /// A TSV's rows, each as its columns by name.
    fn read(text: &str) -> Vec<Row> {
        let mut lines = text.lines();
        let head: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
        lines
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                head.iter()
                    .zip(l.split('\t').chain(std::iter::repeat("")))
                    .map(|(h, v)| ((*h).to_owned(), v.to_owned()))
                    .collect()
            })
            .collect()
    }

    fn col<'a>(row: &'a Row, name: &str) -> &'a str {
        row.get(name).map_or("", String::as_str)
    }

    /// The arrow keys, in the order a key line writes them together.
    const ARROWS: [(&str, &str); 4] = [("Up", "↑"), ("Down", "↓"), ("Left", "←"), ("Right", "→")];

    /// The keys a row names, as the key table writes them: `Up, Down`
    /// names two.
    fn names(key: &str) -> Vec<&str> {
        key.split(", ").filter(|k| !k.is_empty()).collect()
    }

    /// How a key line writes one key: an arrow as its arrow, and a name
    /// for a set of keys as a plural.
    fn shown(key: &str) -> &str {
        match key {
            "any letter" => "letters",
            "any other key" => "other keys",
            _ => ARROWS
                .iter()
                .find(|(name, _)| *name == key)
                .map_or(key, |(_, arrow)| arrow),
        }
    }

    /// Writes keys together as a key line shows them: each key's name a
    /// space from the next, and the arrows last, side by side in the
    /// order ↑↓←→, as in `j k ↑↓`.
    fn joined(keys: &[&str]) -> String {
        let mut words: Vec<String> = Vec::new();
        for key in keys {
            let word = shown(key).to_owned();
            if !ARROWS.iter().any(|(name, _)| name == key) && !words.contains(&word) {
                words.push(word);
            }
        }
        let arrows: String = ARROWS
            .iter()
            .filter(|(name, _)| keys.contains(name))
            .map(|(_, arrow)| *arrow)
            .collect();
        if !arrows.is_empty() {
            words.push(arrows);
        }
        words.join(" ")
    }

    /// Where a key goes on a key line: the keys that move first, then
    /// Enter and Space, then the other keys, then Esc, Tab, Ctrl-C, and
    /// `q` last, the key that leaves.
    fn place(key: &str) -> u8 {
        match key {
            "Up" | "Down" | "Left" | "Right" | "Home" | "End" | "PgUp" | "PgDn" | "j" | "k"
            | "h" | "l" | "g" | "G" | "Ctrl-D" | "Ctrl-U" => 0,
            "Enter" | "Space" | "any letter" | "Backspace" => 1,
            "Esc" => 3,
            "Tab" | "Shift-Tab" => 4,
            "Ctrl-C" => 5,
            "q" => 6,
            _ => 2,
        }
    }

    /// One item of a key line: the keys that share a label, the label, and
    /// whether a paged key line shows it on its first page.
    struct Item<'a> {
        place: (u8, usize),
        keys: Vec<&'a str>,
        label: &'a str,
        first: bool,
    }

    impl Item<'_> {
        fn text(&self) -> String {
            format!("{} {}", joined(&self.keys), self.label)
        }
    }

    /// The columns of items two spaces apart.
    fn width(items: &[String]) -> usize {
        items
            .iter()
            .map(|i| i.chars().count())
            .sum::<usize>()
            .saturating_add(items.len().saturating_sub(1).saturating_mul(2))
    }

    /// A node's help: each key as a key line writes it, and what it does.
    type Help = Vec<(String, String)>;

    /// A node's key line in pages, and its help, from its supported rows
    /// with a label. A key line that fits on one row of
    /// [`super::PAGE_COLUMNS`] is one page. Otherwise the first page holds
    /// the items marked for it, the later pages the other items in order,
    /// as many as fit beside the more item, and every page ends with the
    /// more item of the row for `.`. The help lists `.` only where it acts.
    fn lines(id: &str, rows: &[&Row]) -> Result<(Vec<String>, Help), String> {
        let mut items: Vec<Item<'_>> = Vec::new();
        let mut help = Vec::new();
        let mut more = None;
        for (i, row) in rows.iter().enumerate() {
            let label = col(row, "label");
            if col(row, "status") != "supported" || label == "-" {
                continue;
            }
            let keys = names(col(row, "key"));
            let place = (keys.iter().map(|k| place(k)).min().unwrap_or(2), i);
            let entry = (place, joined(&keys), col(row, "means").to_owned());
            if keys == ["."] {
                more = Some((label, entry));
                continue;
            }
            help.push(entry);
            let first = col(row, "page") == "1";
            match items.iter_mut().find(|item| item.label == label) {
                Some(item) => {
                    item.place = item.place.min(place);
                    item.keys.extend(keys);
                    item.first |= first;
                }
                None => items.push(Item {
                    place,
                    keys,
                    label,
                    first,
                }),
            }
        }
        items.sort_by_key(|item| item.place);
        let all: Vec<String> = items.iter().map(Item::text).collect();
        let pages = match more {
            Some((label, entry)) if width(&all) > super::PAGE_COLUMNS => {
                help.push(entry);
                pages(id, &items, label)?
            }
            // with no row for `.`, the key line has no pages, so it must
            // fit on one row
            _ if width(&all) > super::PAGE_COLUMNS => {
                return Err(format!(
                    "the key line of {id} is wider than {} columns and has no row for `.` to show it in pages: {}",
                    super::PAGE_COLUMNS,
                    all.join("  ")
                ));
            }
            _ => vec![all.join("  ")],
        };
        help.sort_by_key(|(place, _, _)| *place);
        let help = help
            .into_iter()
            .map(|(_, keys, means)| (keys, means))
            .collect();
        Ok((pages, help))
    }

    /// The pages of a key line too wide for one row, each ending with the
    /// more item `. more 1/3`, which the row of `.` labels `label`.
    fn pages(id: &str, items: &[Item<'_>], label: &str) -> Result<Vec<String>, String> {
        // the widest more item a key line of up to nine pages shows
        let more = format!(". {label} 9/9").chars().count().saturating_add(2);
        let mut pages: Vec<Vec<String>> = Vec::new();
        let first: Vec<String> = items.iter().filter(|i| i.first).map(Item::text).collect();
        // with no item marked for it, the first page fills like the others
        let marked = !first.is_empty();
        if marked {
            pages.push(first);
        }
        let mut page: Vec<String> = Vec::new();
        for item in items.iter().filter(|i| !(marked && i.first)) {
            let mut wider = page.clone();
            wider.push(item.text());
            if !page.is_empty() && width(&wider).saturating_add(more) > super::PAGE_COLUMNS {
                pages.push(std::mem::take(&mut page));
                page.push(item.text());
            } else {
                page = wider;
            }
        }
        if !page.is_empty() {
            pages.push(page);
        }
        let count = pages.len();
        if count > 9 {
            return Err(format!(
                "the key line of {id} needs {count} pages, and nine is the most"
            ));
        }
        pages
            .into_iter()
            .enumerate()
            .map(|(i, mut page)| {
                page.push(format!(". {label} {}/{count}", i.saturating_add(1)));
                let line = page.join("  ");
                if line.chars().count() > super::PAGE_COLUMNS {
                    Err(format!(
                        "page {} of the key line of {id} is wider than {} columns: {line}",
                        i.saturating_add(1),
                        super::PAGE_COLUMNS
                    ))
                } else {
                    Ok(line)
                }
            })
            .collect()
    }

    /// The two tables, with each node's rows in the order its `keys`
    /// column names them.
    struct Contract {
        tree: Vec<Row>,
        rows: BTreeMap<String, Row>,
    }

    impl Contract {
        fn read(keys: &str, tree: &str) -> Result<Self, String> {
            let mut rows = BTreeMap::new();
            for row in read(keys) {
                let id = col(&row, "id").to_owned();
                if rows.insert(id.clone(), row).is_some() {
                    return Err(format!("the key table has two rows {id}"));
                }
            }
            Ok(Self {
                tree: read(tree),
                rows,
            })
        }

        fn rows_of(&self, node: &Row) -> Result<Vec<&Row>, String> {
            col(node, "keys")
                .split(", ")
                .filter(|k| !k.is_empty() && *k != "-")
                .map(|k| {
                    self.rows.get(k).ok_or_else(|| {
                        format!(
                            "the tree's node {} names {k}, which the key table lacks",
                            col(node, "id")
                        )
                    })
                })
                .collect()
        }
    }

    /// A Rust string literal of `s`.
    fn lit(s: &str) -> String {
        format!("{s:?}")
    }

    /// The text of `contract_text.rs`.
    fn text(c: &Contract) -> Result<String, String> {
        let mut out = String::from(
            "//! Generated from ADR-1403's interface tree and key table by the test\n\
             //! `generated_help_and_page_match_the_contract` in `contract.rs`.\n\
             //! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --lib generated_help`\n\
             //! writes it again. Do not edit it by hand.\n\n\
             use super::contract::Node;\n\n\
             /// Every node of the interface tree, in the tree's order.\n\
             #[rustfmt::skip]\n\
             pub const NODES: &[Node] = &[\n",
        );
        for node in &c.tree {
            let supported = col(node, "status") == "supported";
            let rows = c.rows_of(node)?;
            let (pages, help) = if supported {
                lines(col(node, "id"), &rows)?
            } else {
                (Vec::new(), Vec::new())
            };
            let _ = writeln!(out, "    Node {{");
            let _ = writeln!(out, "        id: {},", lit(col(node, "id")));
            let _ = writeln!(out, "        what: {},", lit(col(node, "node")));
            let _ = writeln!(out, "        supported: {supported},");
            if pages.is_empty() {
                let _ = writeln!(out, "        key_pages: &[],");
            } else {
                let _ = writeln!(out, "        key_pages: &[");
                for page in pages {
                    let _ = writeln!(out, "            {},", lit(&page));
                }
                let _ = writeln!(out, "        ],");
            }
            if help.is_empty() {
                let _ = writeln!(out, "        help: &[],");
            } else {
                let _ = writeln!(out, "        help: &[");
                for (keys, means) in help {
                    let _ = writeln!(out, "            ({}, {}),", lit(&keys), lit(&means));
                }
                let _ = writeln!(out, "        ],");
            }
            if rows.is_empty() {
                let _ = writeln!(out, "        rows: &[],");
            } else {
                let _ = writeln!(out, "        rows: &[");
                for row in rows {
                    let supported = col(row, "status") == "supported";
                    let _ = writeln!(out, "            ({}, {supported}),", lit(col(row, "id")));
                }
                let _ = writeln!(out, "        ],");
            }
            let _ = writeln!(out, "    }},");
        }
        out.push_str("];\n");
        Ok(out)
    }

    /// A Mermaid node id for a tree id, which may hold dots and hyphens.
    fn mermaid_id(id: &str) -> String {
        id.replace(['.', '-'], "_")
    }

    /// Text for a cell of a Markdown table: a `|` would end the cell.
    fn cell(s: &str) -> String {
        s.replace('|', "\\|")
    }

    /// Links each issue that `issues` names, such as `#379, #412`.
    fn issue_links(issues: &str) -> String {
        issues
            .split(", ")
            .filter_map(|i| i.strip_prefix('#'))
            .map(|n| format!("[#{n}]({ISSUES}{n})"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// A sentence from a phrase of the tree: its first letter capitalized
    /// and a full stop at its end.
    fn sentence(s: &str) -> String {
        let mut chars = s.chars();
        let first = chars.next().map(|c| c.to_uppercase().collect::<String>());
        stop(&(first.unwrap_or_default() + chars.as_str()))
    }

    /// A phrase of the tree with a full stop at its end, where it has none.
    fn stop(s: &str) -> String {
        let mut out = s.to_owned();
        if !out.ends_with(['.', '?', '!', '"']) {
            out.push('.');
        }
        out
    }

    /// The site's page "vtium-config's screens and keys".
    fn page(c: &Contract) -> Result<String, String> {
        let mut out = String::from(
            "<!-- Generated from ADR-1403's interface tree and key table by the test generated_help_and_page_match_the_contract in crates/vtium-config/src/tui/contract.rs. VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --lib generated_help rewrites it. -->\n\n\
             # vtium-config's screens and keys\n\n\
             vtium-config's TUI is its full-screen view of the configuration, which `vtium config tui` opens. \
             This page lists every screen of the TUI and what each key does there. \
             The TUI's Help tab and its key lines come from the same two tables as this page, so they say the same. \
             [Configure vtium](../configure.md#the-tui) explains what the TUI is for.\n\n\
             The diagram shows how the screens open from one another. \
             A dashed box is a screen that is not built as described yet. \
             Its section links the issue that tracks it, and says what vtium-config has today.\n\n\
             ```mermaid\nflowchart LR\n",
        );
        let mut planned = Vec::new();
        for node in &c.tree {
            let id = col(node, "id");
            let _ = writeln!(out, "    {}[\"{id}\"]", mermaid_id(id));
            if col(node, "status") != "supported" {
                planned.push(mermaid_id(id));
            }
        }
        for node in &c.tree {
            let parent = col(node, "parent");
            if parent != "-" && !parent.is_empty() {
                let _ = writeln!(
                    out,
                    "    {} --> {}",
                    mermaid_id(parent),
                    mermaid_id(col(node, "id"))
                );
            }
        }
        if !planned.is_empty() {
            out.push_str("    classDef planned stroke-dasharray: 5 5\n");
            let _ = writeln!(out, "    class {} planned", planned.join(","));
        }
        out.push_str("```\n\n");
        out.push_str(
            "Each section below names a screen by its place in the tree, says how you open and leave it, and lists its keys. \
             The column \"Shown as\" gives the word the key line shows beside the key, and a dash marks a key that the key line leaves out.\n",
        );
        for node in &c.tree {
            let id = col(node, "id");
            let _ = write!(out, "\n## `{id}`\n\n{}\n", sentence(col(node, "node")));
            // a planned screen may exist today in another shape, which the
            // `now` column says, after a colon and so in lower case
            if col(node, "status") != "supported" {
                let _ = write!(
                    out,
                    "\nThis screen is not built as described yet. See {}.",
                    issue_links(col(node, "issue"))
                );
                let now = col(node, "now");
                if !now.is_empty() {
                    let _ = write!(out, " Today: {}", stop(now));
                }
                out.push('\n');
            }
            let reach = col(node, "reach");
            if reach != "-" && !reach.is_empty() {
                let _ = write!(out, "\n**Open it:** {}\n", sentence(reach));
            }
            let leave = col(node, "leave");
            if leave != "-" && !leave.is_empty() {
                let _ = write!(out, "\n**Leave it:** {}\n", sentence(leave));
            }
            let rows = c.rows_of(node)?;
            if rows.is_empty() {
                continue;
            }
            out.push_str("\n| Key | Shown as | What it does |\n|---|---|---|\n");
            for row in rows {
                let mut does = cell(col(row, "means"));
                if col(row, "status") != "supported" {
                    let _ = write!(
                        does,
                        " Not built yet: see {}.",
                        issue_links(col(row, "issue"))
                    );
                }
                let _ = writeln!(
                    out,
                    "| {} | {} | {does} |",
                    cell(col(row, "key")),
                    cell(col(row, "label")),
                );
            }
        }
        Ok(out)
    }

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn file(path: &str) -> String {
        std::fs::read_to_string(root().join(path)).unwrap_or_default()
    }

    /// The generated files match the tables, or, with
    /// `VTIUM_CONFIG_BLESS` set, are written from them.
    #[test]
    fn generated_help_and_page_match_the_contract() -> Result<(), String> {
        let contract = Contract::read(&file(KEYS), &file(TREE))?;
        let bless = std::env::var_os("VTIUM_CONFIG_BLESS").is_some();
        let mut stale = Vec::new();
        for (path, want) in [(TEXT, text(&contract)), (PAGE, page(&contract))] {
            let want = want?;
            if file(path) == want {
                continue;
            }
            if bless {
                std::fs::write(root().join(path), &want)
                    .map_err(|e| format!("cannot write {path}: {e}"))?;
            } else {
                stale.push(path);
            }
        }
        if stale.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{} differ from what ADR-1403's tree and key table give; rerun with VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --lib generated_help to write them again",
                stale.join(" and ")
            ))
        }
    }

    /// The rules of a key line, on a node of a few rows: the keys that
    /// move come first, keys that share a label share an item, a planned
    /// row and a row labelled `-` are left out, and `q` comes last.
    #[test]
    fn a_key_line_groups_keys_by_label_and_ends_with_q() -> Result<(), String> {
        let keys = "id\tkey\tstate\tmeans\tstatus\tnote\tdecided\tissue\tlabel\n\
                    q.x\tq\tview\tQuits.\tsupported\t\t\t\tquit\n\
                    esc.x\tEsc\tview\tGoes back.\tsupported\t\t\t\tback\n\
                    c.x\tCtrl-C\tview\tCancels.\tsupported\t\t\t\tback\n\
                    down.x\tDown\tview\tMoves down.\tsupported\t\t\t\tmove\n\
                    j.x\tj\tview\tMoves down.\tsupported\t\t\t\tmove\n\
                    up.x\tUp\tview\tMoves up.\tsupported\t\t\t\tmove\n\
                    p.x\tp\tview\tPreviews.\tplanned\t\t\t#1\tpreview\n\
                    t.x\tTab\tview\tDoes nothing.\tsupported\t\t\t\t-\n\
                    y.x\ty\tview\tSays yes.\tsupported\t\t\t\tyes\n";
        let tree = "id\tparent\tnode\treach\tleave\tkeys\tstatus\tissue\tnow\n\
                    x\t-\ta node\t-\t-\tq.x, esc.x, c.x, down.x, j.x, up.x, p.x, t.x, y.x\tsupported\t\t\n";
        let c = Contract::read(keys, tree)?;
        let node = c.tree.first().ok_or("no node")?;
        let (pages, help) = lines("x", &c.rows_of(node)?)?;
        assert_eq!(pages, ["j ↑↓ move  y yes  Esc Ctrl-C back  q quit"]);
        assert_eq!(help.len(), 7);
        assert_eq!(
            help.first(),
            Some(&("↓".to_owned(), "Moves down.".to_owned()))
        );
        assert_eq!(help.last(), Some(&("q".to_owned(), "Quits.".to_owned())));
        Ok(())
    }

    /// A key line too wide for one row shows its keys in pages: the
    /// first page holds the keys whose `page` column is 1, the later
    /// pages the other keys in order, as many as fit, and each page ends
    /// with the more item. Where the keys fit on one row, the key line
    /// has one page, with no more item, and the help leaves `.` out.
    #[test]
    fn a_key_line_too_wide_for_a_row_shows_pages() -> Result<(), String> {
        let head = "id\tkey\tstate\tmeans\tstatus\tnote\tdecided\tissue\tlabel\tpage\n";
        let mut keys = String::from(head);
        keys.push_str("q.x\tq\tview\tQuits.\tsupported\t\t\t\tquit\t1\n");
        keys.push_str("up.x\tUp\tview\tMoves.\tsupported\t\t\t\tmove\t1\n");
        keys.push_str(".x\t.\tview\tShows the next page.\tsupported\t\t\t\tmore\t\n");
        let mut ids = vec!["q.x", "up.x", ".x"];
        let letters = ["a", "b", "c", "d", "e", "f"];
        for l in letters {
            keys.push_str(&format!(
                "{l}.x\t{l}\tview\tDoes {l}.\tsupported\t\t\t\tdoes the letter {l}\t\n"
            ));
        }
        let ids_wide: Vec<String> = letters.iter().map(|l| format!("{l}.x")).collect();
        ids.extend(ids_wide.iter().map(String::as_str));
        let tree = format!(
            "id\tparent\tnode\treach\tleave\tkeys\tstatus\tissue\tnow\n\
             wide\t-\ta node\t-\t-\t{}\tsupported\t\t\n\
             narrow\t-\ta node\t-\t-\tq.x, up.x, .x, a.x\tsupported\t\t\n",
            ids.join(", ")
        );
        let c = Contract::read(&keys, &tree)?;
        let wide = c.tree.first().ok_or("no node")?;
        let (pages, help) = lines("wide", &c.rows_of(wide)?)?;
        assert_eq!(
            pages,
            [
                "↑ move  q quit  . more 1/3",
                "a does the letter a  b does the letter b  c does the letter c  . more 2/3",
                "d does the letter d  e does the letter e  f does the letter f  . more 3/3",
            ]
        );
        assert!(help.iter().any(|(k, _)| k == "."));
        let narrow = c.tree.get(1).ok_or("no node")?;
        let (pages, help) = lines("narrow", &c.rows_of(narrow)?)?;
        assert_eq!(pages, ["↑ move  a does the letter a  q quit"]);
        assert!(!help.iter().any(|(k, _)| k == "."));
        Ok(())
    }

    /// A first page wider than a row is an error, not a key line that
    /// runs off the screen.
    #[test]
    fn a_first_page_wider_than_a_row_is_refused() -> Result<(), String> {
        let mut keys =
            String::from("id\tkey\tstate\tmeans\tstatus\tnote\tdecided\tissue\tlabel\tpage\n");
        keys.push_str(".x\t.\tview\tShows the next page.\tsupported\t\t\t\tmore\t\n");
        let mut ids = vec![".x".to_owned()];
        for l in ["a", "b", "c", "d"] {
            keys.push_str(&format!(
                "{l}.x\t{l}\tview\tDoes {l}.\tsupported\t\t\t\tdoes the letter {l} at length\t1\n"
            ));
            ids.push(format!("{l}.x"));
        }
        let tree = format!(
            "id\tparent\tnode\treach\tleave\tkeys\tstatus\tissue\tnow\n\
             x\t-\ta node\t-\t-\t{}\tsupported\t\t\n",
            ids.join(", ")
        );
        let c = Contract::read(&keys, &tree)?;
        assert!(text(&c).is_err_and(|e| e.contains("page 1 of the key line of x")));
        Ok(())
    }

    /// A key line wider than a row is an error where the node has no row
    /// for `.`, since such a key line cannot show its keys in pages.
    #[test]
    fn a_key_line_wider_than_a_row_with_no_more_key_is_refused() -> Result<(), String> {
        let mut keys =
            String::from("id\tkey\tstate\tmeans\tstatus\tnote\tdecided\tissue\tlabel\tpage\n");
        let mut ids = Vec::new();
        for l in ["a", "b", "c", "d"] {
            keys.push_str(&format!(
                "{l}.x\t{l}\tview\tDoes {l}.\tsupported\t\t\t\tdoes the letter {l} at length\t\n"
            ));
            ids.push(format!("{l}.x"));
        }
        let tree = format!(
            "id\tparent\tnode\treach\tleave\tkeys\tstatus\tissue\tnow\n\
             x\t-\ta node\t-\t-\t{}\tsupported\t\t\n",
            ids.join(", ")
        );
        let c = Contract::read(&keys, &tree)?;
        assert!(text(&c).is_err_and(|e| e.contains("the key line of x is wider than 78 columns")));
        Ok(())
    }

    /// A node that names a row the key table lacks is an error, not a
    /// key line that silently drops the key.
    #[test]
    fn a_node_naming_a_missing_row_is_refused() -> Result<(), String> {
        let keys = "id\tkey\tstate\tmeans\tstatus\tnote\tdecided\tissue\tlabel\n";
        let tree = "id\tparent\tnode\treach\tleave\tkeys\tstatus\tissue\tnow\n\
                    x\t-\ta node\t-\t-\tq.x\tsupported\t\t\n";
        let c = Contract::read(keys, tree)?;
        assert!(text(&c).is_err_and(|e| e.contains("q.x")));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every node the TUI can name is a node of the tree, so a renamed
    /// node in the tree fails here and not as a missing key line.
    #[test]
    fn every_node_the_tui_names_is_in_the_tree() {
        for id in [
            "any.question.quit",
            "any.question.leave",
            "any.question.confirm",
            "any.trial",
            "any.too-small",
            "flow",
            "flow.card",
            "flow.card.typing",
            "flow.new-card",
            "flow.info",
            "flow.detail",
            "flow.detail.typing",
            "flow.detail.choices",
            "flow.maker",
            "flow.maker.picker",
            "flow.maker.picker.typing",
            "flow.others",
            "flow.file",
            "flow.restore",
            "settings",
            "settings.section",
            "settings.section.typing",
            "settings.section.list",
            "looks",
            "looks.new-frit",
            "looks.new-frit.typing",
            "fonts",
            "help",
        ] {
            assert!(find(id).is_some(), "the tree has no node {id}");
        }
    }
}
