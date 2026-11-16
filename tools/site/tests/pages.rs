//! The index pages of the crates' interfaces and of the releases, against
//! the tree: every crate with a library has a row that links its
//! reference, and every release's page has a row, newest first.
//!
//! References:
//! - ADR-1508 D1 and D2: the two index pages and their checks.

use std::path::{Path, PathBuf};

use vtium_site::Site;
use vtium_site::api::{crate_index_faults, crate_rows, library_crates};
use vtium_site::release::release_index_faults;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_crate_with_a_library_has_a_row_that_links_its_reference() {
    let text = std::fs::read_to_string(root().join("zensical.toml")).unwrap();
    let site_url = Site::parse(&text).unwrap().site_url.unwrap();
    let index = std::fs::read_to_string(root().join("docs/api/index.md")).unwrap();
    let crates = library_crates(&root().join("crates")).unwrap();
    assert!(!crates.is_empty(), "no crate under crates/ has a library");
    let found = crate_index_faults(&crate_rows(&index), &crates, &site_url);
    assert!(found.is_empty(), "the crates' index:\n{}", found.join("\n"));
}

#[test]
fn every_release_page_has_a_row_and_the_rows_run_newest_first() {
    let found = release_index_faults(&root().join("docs/releases")).unwrap();
    assert!(
        found.is_empty(),
        "the releases' index:\n{}",
        found.join("\n")
    );
}

/// A small releases folder of its own, with a page the index leaves out,
/// a row with no page, a row listed twice, and two rows out of order, so
/// the check is seen to catch each.
#[test]
fn a_page_left_out_a_row_with_no_page_a_row_listed_twice_and_rows_out_of_order_are_faults() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("site-releases");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for page in [
        "0.1.0-alpha.9.md",
        "0.1.0-alpha.10.md",
        "0.1.0-alpha.2.md",
        "0.1.0-alpha.1.md",
    ] {
        std::fs::write(dir.join(page), "# A release\n").unwrap();
    }
    std::fs::write(
        dir.join("index.md"),
        "| Release | Date |\n|---|---|\n\
         | [0.1.0-alpha.9](0.1.0-alpha.9.md) | 2026-11-20 |\n\
         | [0.1.0-alpha.10](0.1.0-alpha.10.md) | 2026-11-21 |\n\
         | [0.1.0-alpha.3](0.1.0-alpha.3.md) | 2026-11-04 |\n\
         | [0.1.0-alpha.2](0.1.0-alpha.2.md) | 2026-11-03 |\n\
         | [0.1.0-alpha.2](0.1.0-alpha.2.md) | 2026-11-03 |\n",
    )
    .unwrap();
    assert_eq!(
        release_index_faults(&dir).unwrap(),
        [
            "docs/releases/0.1.0-alpha.1.md has no row in docs/releases/index.md",
            "docs/releases/index.md has a row for 0.1.0-alpha.3, which has no page",
            "docs/releases/index.md lists 0.1.0-alpha.2 twice",
            "docs/releases/index.md lists 0.1.0-alpha.9 before 0.1.0-alpha.10, but the newest release comes first",
        ]
    );
}
