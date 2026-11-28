//! The compiled tables: the generator that writes `src/contract/text.rs`
//! from ADR-1403's two tables and the overlay under `contract/`, and the
//! answers the `contract` module gives from them.
//!
//! References:
//! - ADR-1206 D3: the tables are compiled into vtium-config-widgets, and
//!   the action bar reads its buttons and their keys from them.
//! - ADR-1403 D11, "The keys and nodes these add" and "How the tables
//!   carry this section": the rows and nodes the overlay holds.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use vtium_config_widgets::contract;

const KEYS: &str = "docs/architecture/platform/ADR-1403-keys.tsv";
const TREE: &str = "docs/architecture/platform/ADR-1403-tree.tsv";
const D11_KEYS: &str = "crates/vtium-config-widgets/contract/d11-keys.tsv";
const D11_TREE: &str = "crates/vtium-config-widgets/contract/d11-tree.tsv";
const D11_GONE: &str = "crates/vtium-config-widgets/contract/d11-gone.tsv";
const D11_SEARCH: &str = "crates/vtium-config-widgets/contract/d11-search.tsv";
const TEXT: &str = "crates/vtium-config-widgets/src/contract/text.rs";

/// A row of a TSV, as its columns by name.
type Cells = BTreeMap<String, String>;

/// The workspace's root, two levels above this crate.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(root().join(path)).map_err(|e| format!("cannot read {path}: {e}"))
}

/// A TSV's rows, each as its columns by name.
fn parse(text: &str) -> Vec<Cells> {
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

fn col<'a>(cells: &'a Cells, name: &str) -> &'a str {
    cells.get(name).map_or("", String::as_str)
}

/// The six files the tables are made from.
struct Sources {
    keys: String,
    tree: String,
    d11_keys: String,
    d11_tree: String,
    d11_gone: String,
    d11_search: String,
}

impl Sources {
    fn read() -> Result<Self, String> {
        Ok(Self {
            keys: read_file(KEYS)?,
            tree: read_file(TREE)?,
            d11_keys: read_file(D11_KEYS)?,
            d11_tree: read_file(D11_TREE)?,
            d11_gone: read_file(D11_GONE)?,
            d11_search: read_file(D11_SEARCH)?,
        })
    }
}

/// Rows in their table's order, where a row of `over` replaces the row
/// with its id and the rest of `over` follow, and the ids in `gone` are
/// taken out.
fn merge(base: Vec<Cells>, over: Vec<Cells>, gone: &BTreeSet<String>) -> Vec<Cells> {
    let mut over: Vec<Option<Cells>> = over.into_iter().map(Some).collect();
    let mut out = Vec::new();
    for row in base {
        let id = col(&row, "id").to_owned();
        let replaced = over
            .iter_mut()
            .find(|o| o.as_ref().is_some_and(|o| col(o, "id") == id))
            .and_then(Option::take);
        out.push(replaced.unwrap_or(row));
    }
    out.extend(over.into_iter().flatten());
    out.retain(|r| !gone.contains(col(r, "id")));
    out
}

/// The merged tables, checked: unique ids, and every row a node names
/// present.
struct Tables {
    rows: Vec<Cells>,
    nodes: Vec<Cells>,
    search: BTreeMap<String, String>,
}

impl Tables {
    fn of(s: &Sources) -> Result<Self, String> {
        let gone = parse(&s.d11_gone);
        let gone_of = |kind: &str| -> BTreeSet<String> {
            gone.iter()
                .filter(|g| col(g, "kind") == kind)
                .map(|g| col(g, "id").to_owned())
                .collect()
        };
        let rows = merge(parse(&s.keys), parse(&s.d11_keys), &gone_of("row"));
        let nodes = merge(parse(&s.tree), parse(&s.d11_tree), &gone_of("node"));
        let mut ids = BTreeSet::new();
        for r in rows.iter().chain(&nodes) {
            if !ids.insert(col(r, "id")) {
                return Err(format!("two rows or nodes have the id {}", col(r, "id")));
            }
        }
        let search = parse(&s.d11_search)
            .iter()
            .map(|r| (col(r, "id").to_owned(), col(r, "search").to_owned()))
            .collect::<BTreeMap<_, _>>();
        for id in search.keys() {
            if !nodes.iter().any(|n| col(n, "id") == id) {
                return Err(format!(
                    "the search column names {id}, which the tree lacks"
                ));
            }
        }
        Ok(Self {
            rows,
            nodes,
            search,
        })
    }

    /// The index of each row a node's `keys` column names, in its order.
    fn indexes(&self, node: &Cells) -> Result<Vec<usize>, String> {
        col(node, "keys")
            .split(", ")
            .filter(|k| !k.is_empty() && *k != "-")
            .map(|k| {
                self.rows
                    .iter()
                    .position(|r| col(r, "id") == k)
                    .ok_or_else(|| {
                        format!(
                            "the node {} names the row {k}, which the key table lacks",
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

/// The text of `src/contract/text.rs`.
fn text(t: &Tables) -> Result<String, String> {
    let mut out = String::from(
        "//! Generated from ADR-1403's interface tree and key table, with the\n\
         //! overlay under `contract/`, by the test `the_compiled_tables_match_the_tsvs`\n\
         //! in `tests/contract.rs`. Do not edit it by hand.\n\n\
         use super::{Node, Row};\n\n\
         /// Every row of the key table, in the table's order.\n\
         #[rustfmt::skip]\n\
         pub(super) static ROWS: &[Row] = &[\n",
    );
    if t.rows.len() > usize::from(u16::MAX) {
        return Err(format!("{} rows do not fit a u16 index", t.rows.len()));
    }
    for r in &t.rows {
        let _ = writeln!(
            out,
            "    Row {{ id: {}, key: {}, bar_key: {}, state: {}, means: {}, label: {}, supported: {} }},",
            lit(col(r, "id")),
            lit(col(r, "key")),
            lit(&contract::bar_form(col(r, "key"))),
            lit(col(r, "state")),
            lit(col(r, "means")),
            lit(col(r, "label")),
            col(r, "status") == "supported",
        );
    }
    out.push_str(
        "];\n\n\
         /// Every node of the interface tree, in the tree's order.\n\
         #[rustfmt::skip]\n\
         pub(super) static NODES: &[Node] = &[\n",
    );
    for n in &t.nodes {
        let id = col(n, "id");
        let search: Vec<String> = t
            .search
            .get(id)
            .map(|s| s.split(", ").filter(|f| !f.is_empty()).map(lit).collect())
            .unwrap_or_default();
        let rows: Vec<String> = t.indexes(n)?.iter().map(usize::to_string).collect();
        let _ = writeln!(
            out,
            "    Node {{ id: {}, parent: {}, what: {}, supported: {}, search: &[{}], rows: &[{}] }},",
            lit(id),
            lit(col(n, "parent")),
            lit(col(n, "node")),
            col(n, "status") == "supported",
            search.join(", "),
            rows.join(", "),
        );
    }
    out.push_str("];\n");
    Ok(out)
}

/// The generator: `text.rs` holds what the six files give, and
/// `VTIUM_CONFIG_BLESS=1` writes it when it does not.
#[test]
fn the_compiled_tables_match_the_tsvs() -> Result<(), String> {
    let want = text(&Tables::of(&Sources::read()?)?)?;
    let have = read_file(TEXT).unwrap_or_default();
    if have == want {
        return Ok(());
    }
    if std::env::var_os("VTIUM_CONFIG_BLESS").is_some() {
        std::fs::write(root().join(TEXT), &want)
            .map_err(|e| format!("cannot write {TEXT}: {e}"))?;
        return Ok(());
    }
    Err(format!(
        "{TEXT} differs from what ADR-1403's tables and the overlay give; rerun with VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-widgets --test contract to write it again"
    ))
}

/// A node that names a row the merged key table lacks is an error, so a
/// row dropped from the overlay cannot vanish from a node silently.
#[test]
fn a_node_naming_a_missing_row_is_refused() -> Result<(), String> {
    let s = Sources {
        keys: "id\tkey\tstate\tmeans\tstatus\tnote\tdecided\tissue\tlabel\tpage\n\
               q.x\tq\tview\tQuits.\tsupported\t\t\t\tQuit\t\n"
            .to_owned(),
        tree: "id\tparent\tnode\treach\tleave\tkeys\tstatus\tissue\tnow\n\
               x\t-\ta node\t-\t-\tq.x, gone.x\tsupported\t\t\n"
            .to_owned(),
        d11_keys: "id\tkey\tstate\tmeans\tstatus\tnote\tdecided\tissue\tlabel\tpage\n\
                   gone.x\tg\tview\tGoes.\tsupported\t\t\t\tGo\t\n"
            .to_owned(),
        d11_tree: "id\n".to_owned(),
        d11_gone: "kind\tid\nrow\tgone.x\n".to_owned(),
        d11_search: "id\tsearch\n".to_owned(),
    };
    let t = Tables::of(&s)?;
    assert!(text(&t).is_err_and(|e| e.contains("gone.x")));
    Ok(())
}

/// The overlay replaces a row and a node by id, keeps the tables' order,
/// adds its own after them, and takes out what `d11-gone.tsv` names.
#[test]
fn the_overlay_replaces_adds_and_takes_out() -> Result<(), String> {
    let head = "id\tkey\tstate\tmeans\tstatus\tnote\tdecided\tissue\tlabel\tpage\n";
    let s = Sources {
        keys: format!(
            "{head}a.x\ta\tview\tOld.\tplanned\t\t\t\tA\t\nb.x\tb\tview\tB.\tsupported\t\t\t\tB\t\n"
        ),
        tree: "id\tparent\tnode\treach\tleave\tkeys\tstatus\tissue\tnow\n\
               x\t-\tx\t-\t-\ta.x, b.x\tsupported\t\t\n\
               y\t-\ty\t-\t-\tb.x\tsupported\t\t\n"
            .to_owned(),
        d11_keys: format!(
            "{head}c.x\tCtrl-c\tview\tC.\tsupported\t\t\t\tC\t\na.x\ta\tview\tNew.\tsupported\t\t\t\tA\t\n"
        ),
        d11_tree: "id\tparent\tnode\treach\tleave\tkeys\tstatus\tissue\tnow\n\
                   x\t-\tx\t-\t-\tc.x, a.x\tsupported\t\t\n"
            .to_owned(),
        d11_gone: "kind\tid\nrow\tb.x\nnode\ty\n".to_owned(),
        d11_search: "id\tsearch\nx\tname, file\n".to_owned(),
    };
    let t = Tables::of(&s)?;
    let ids: Vec<&str> = t.rows.iter().map(|r| col(r, "id")).collect();
    assert_eq!(ids, ["a.x", "c.x"]);
    assert_eq!(t.rows.first().map(|r| col(r, "means")), Some("New."));
    assert_eq!(t.nodes.len(), 1);
    let out = text(&t)?;
    assert!(out.contains(r#"bar_key: "Ctrl+C""#), "{out}");
    assert!(
        out.contains(r#"search: &["name", "file"], rows: &[1, 0]"#),
        "{out}"
    );
    Ok(())
}

// What the compiled tables answer. These read the committed `text.rs`
// through the crate, so they check what the TUI compiles in.

/// The rows of `d11-gone.tsv` and its nodes are not in the tables, and no
/// node names a row that went.
#[test]
fn the_rows_and_nodes_that_go_are_absent() -> Result<(), String> {
    let gone = parse(&read_file(D11_GONE)?);
    assert!(!gone.is_empty());
    for g in &gone {
        let id = col(g, "id");
        match col(g, "kind") {
            "row" => assert!(contract::row(id).is_none(), "the row {id} is still there"),
            "node" => assert!(contract::node(id).is_none(), "the node {id} is still there"),
            kind => return Err(format!("d11-gone.tsv has a kind {kind}")),
        }
    }
    for n in contract::nodes() {
        for r in n.rows() {
            assert!(
                !gone.iter().any(|g| col(g, "id") == r.id),
                "{} names {}",
                n.id,
                r.id
            );
        }
    }
    Ok(())
}

/// Every row and node of the overlay is in the tables as the overlay
/// writes it, and every node of the overlay names its rows in its order.
#[test]
fn every_row_and_node_of_the_overlay_is_compiled_in() -> Result<(), String> {
    for r in parse(&read_file(D11_KEYS)?) {
        let id = col(&r, "id");
        let got = contract::row(id).ok_or(format!("no row {id}"))?;
        assert_eq!(got.means, col(&r, "means"), "{id}");
        assert_eq!(got.label, col(&r, "label"), "{id}");
        assert!(got.supported, "{id}");
    }
    for n in parse(&read_file(D11_TREE)?) {
        let id = col(&n, "id");
        let got = contract::node(id).ok_or(format!("no node {id}"))?;
        let want: Vec<&str> = col(&n, "keys").split(", ").filter(|k| *k != "-").collect();
        let have: Vec<&str> = got.rows().map(|r| r.id).collect();
        assert_eq!(have, want, "{id}");
        assert_eq!(got.what, col(&n, "node"), "{id}");
    }
    Ok(())
}

/// The labelled rows of the nodes D11 adds and rewrites, in their order of
/// priority, as the approved mock-ups draw the bars.
#[test]
fn each_d11_node_gives_its_labelled_rows_in_order() {
    let labels =
        |id: &str| -> Vec<&str> { contract::buttons(id).iter().map(|r| r.label).collect() };
    let keys =
        |id: &str| -> Vec<&str> { contract::buttons(id).iter().map(|r| r.bar_key).collect() };
    let fonts = labels("fonts");
    assert_eq!(
        fonts.get(..7),
        Some(
            &[
                "activate",
                "Activate missing",
                "Search",
                "open",
                "try live",
                "Save",
                "Undo"
            ][..]
        ),
        "{fonts:?}"
    );
    assert_eq!(
        keys("fonts").get(..7),
        Some(&["Space", "A", "?", "Enter", "t", "Ctrl+S", "Ctrl+Z"][..])
    );
    assert_eq!(
        keys("looks").get(..9),
        Some(&["Enter", "n", "r", "?", "t", "p", "w", "Ctrl+S", "Ctrl+Z"][..])
    );
    assert_eq!(
        keys("flow").get(..6),
        Some(&["Space", "Enter", "a", "e", "Esc", "Ctrl+S"][..])
    );
    assert_eq!(
        keys("any.problems").get(..3),
        Some(&["Enter", "F1", "Esc"][..])
    );
    assert_eq!(
        labels("any.menu.file.manage").get(..4),
        Some(&["Open", "Rename…", "Delete…", "Close"][..])
    );
    assert_eq!(
        keys("any.menu.file.manage").get(..4),
        Some(&["Enter", "r", "Del", "Esc"][..])
    );
    // in the search row q is typed, so Ctrl+C is the bar's Quit
    assert_eq!(labels("any.search"), ["Keep", "Clear", "Quit"]);
    assert_eq!(keys("any.search"), ["Enter", "Esc", "Ctrl+C"]);
    assert_eq!(keys("any.save").get(..3), Some(&["Ctrl+S", "w", "Esc"][..]));
    assert_eq!(
        keys("any.history.version").get(..3),
        Some(&["v", "a", "Esc"][..])
    );
    for tab in [
        "fonts",
        "effects",
        "looks",
        "looks-resources",
        "settings",
        "help",
        "flow",
    ] {
        let k = keys(tab);
        for want in ["Ctrl+S", "Ctrl+Z", "Ctrl+Y", "Ctrl+F", "F10", "q", "."] {
            assert!(k.contains(&want), "{tab} has no {want}: {k:?}");
        }
        // F1 does nothing on the Help tab, whose topics are in view
        assert_eq!(k.contains(&"F1"), tab != "help", "{tab}: {k:?}");
        for off in ["Alt+F", "Alt+N", "Tab", "1", "Up"] {
            assert!(!k.contains(&off), "{tab}'s bar has {off}");
        }
    }
    // the labelled rows include the moves, which the bar leaves out
    assert!(
        contract::labelled("help")
            .iter()
            .any(|r| r.id == "up.view.help")
    );
    assert!(
        !contract::buttons("help")
            .iter()
            .any(|r| r.id == "up.view.help")
    );
}

/// The key column's forms become the bar's: a chord with a plus, a capital
/// after the modifier, `Delete` as `Del`, and the first of several keys.
#[test]
fn each_row_gives_its_key_in_the_bar_s_form() -> Result<(), String> {
    for (id, key, bar) in [
        ("ctrl-s.view", "Ctrl-S", "Ctrl+S"),
        ("alt-f.view", "Alt-F", "Alt+F"),
        ("delete.layer.manage", "Delete", "Del"),
        ("ctrl-s.layer.save", "Ctrl-S, Enter", "Ctrl+S"),
        (
            "alt-tab.view",
            "Alt-N, Alt-E, Alt-L, Alt-O, Alt-S, Alt-H",
            "Alt+N",
        ),
        ("v.layer.history-version", "v, Click on View changes", "v"),
        ("shift-a.view.fonts", "A", "A"),
    ] {
        let r = contract::row(id).ok_or(format!("no row {id}"))?;
        assert_eq!((r.key, r.bar_key), (key, bar), "{id}");
        assert_eq!(r.action(), id);
    }
    Ok(())
}

/// The `search` column names fields on the nodes D11's table names, and
/// `-` everywhere else.
#[test]
fn the_search_fields_are_the_d11_table_s_and_none_elsewhere() {
    let want: BTreeMap<&str, &[&str]> = [
        ("fonts", &["family", "file"][..]),
        ("effects", &["name"][..]),
        ("looks", &["name"][..]),
        ("looks-resources", &["name"][..]),
        ("settings", &["key", "description"][..]),
        ("settings.section", &["key", "description"][..]),
        ("help", &["topic"][..]),
        ("flow.card.list", &["choice"][..]),
        ("looks.look.list", &["choice"][..]),
        ("settings.section.list", &["choice"][..]),
        ("looks.new-frit.list", &["choice"][..]),
    ]
    .into_iter()
    .collect();
    for n in contract::nodes() {
        let fields = want.get(n.id).copied().unwrap_or(&[]);
        assert_eq!(n.search, fields, "{}", n.id);
        assert_eq!(contract::search(n.id), fields, "{}", n.id);
    }
    assert!(contract::search("flow").is_empty());
    assert!(contract::search("no such node").is_empty());
}
