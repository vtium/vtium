//! The site's menu against the pages under docs/ (ADR-1502 D3): every page
//! has a place, and every menu entry names a page.

use std::path::{Path, PathBuf};

use vtium_site::{Site, faults};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_page_under_docs_is_in_the_menu_and_every_entry_is_a_page() {
    let text = std::fs::read_to_string(root().join("zensical.toml")).unwrap();
    let site = Site::parse(&text).unwrap();
    let found = faults(&site, &root().join("docs")).unwrap();
    assert!(found.is_empty(), "the site's menu:\n{}", found.join("\n"));
}

/// A small docs tree of its own, with a page the menu leaves out and an
/// entry that names a missing page, so the check is seen to catch both.
#[test]
fn a_page_left_out_and_an_entry_with_no_page_are_both_faults() {
    let docs = Path::new(env!("CARGO_TARGET_TMPDIR")).join("site-menu-docs");
    let _ = std::fs::remove_dir_all(&docs);
    for page in [
        "index.md",
        "stray.md",
        "architecture/INDEX.md",
        "architecture/term/ADR-1000-x.md",
        "scripts/notes.md",
        "figures/README.md",
    ] {
        let path = docs.join(page);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# A page\n").unwrap();
    }
    let site = Site::parse(
        r#"
[project]
nav = [
  { "Home" = ["index.md"] },
  { "Gone" = "missing.md" },
  { "Hidden" = "scripts/notes.md" },
  { "Decisions" = "architecture/INDEX.md" },
  { "Out" = "https://github.com/vtium/vtium" },
]
exclude_docs = """
/scripts/
**/figures/README.md
"""
not_in_nav = """
/architecture/*/ADR-*.md
"""
"#,
    )
    .unwrap();
    let found = faults(&site, &docs).unwrap();
    assert_eq!(
        found,
        [
            "nav names `missing.md`, which is not a file under docs/",
            "nav names `scripts/notes.md`, which exclude_docs leaves out of the site",
            "docs/stray.md is in neither nav nor not_in_nav: give it a place in zensical.toml's menu, or list it in not_in_nav when a page links it",
        ]
    );
}
