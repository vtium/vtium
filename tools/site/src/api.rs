//! The index of the crates' interfaces, `docs/api/index.md`, checked
//! against the crates under `crates/`.
//!
//! The index has a table with a row for each crate that has a library.
//! The row's first cell links the crate's reference by its full address on
//! the site, `SITE/api/CRATE/`, where CRATE is the crate's name with `_`
//! for `-`, as rustdoc names the folder. The site's strict build does not
//! see rustdoc's pages, so [`crate_index_faults`] checks those addresses
//! instead.
//!
//! References:
//! - ADR-1508 D1: the crates' interfaces under `/api/`, the index of the
//!   crates, and its check in this crate.

use std::path::Path;

/// One row of the index: the crate it names and the address it links.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub target: String,
}

/// The rows of the index's tables whose first cell is a link.
pub fn crate_rows(index: &str) -> Vec<Row> {
    index
        .lines()
        .filter_map(|line| {
            let cell = line.strip_prefix('|')?.split('|').next()?.trim();
            let (label, rest) = cell.strip_prefix('[')?.split_once("](")?;
            let target = rest.strip_suffix(')')?;
            Some(Row {
                name: label.to_owned(),
                target: target.to_owned(),
            })
        })
        .collect()
}

/// The crates under `crates` that have a library, in sorted order: the
/// crates rustdoc documents.
pub fn library_crates(crates: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(crates).map_err(|e| format!("{}: {e}", crates.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", crates.display()))?;
        if entry.path().join("src/lib.rs").is_file() {
            out.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    out.sort();
    Ok(out)
}

/// Each fault of the index's rows against `crates`, the crates that have a
/// library, one sentence each: a crate with no row, a row that names no
/// such crate, a crate with two rows, and a row whose link is not the
/// crate's reference under `site_url`.
pub fn crate_index_faults(rows: &[Row], crates: &[String], site_url: &str) -> Vec<String> {
    let mut out = Vec::new();
    for name in crates {
        match rows.iter().filter(|r| &r.name == name).count() {
            0 => out.push(format!("docs/api/index.md has no row for the crate {name}")),
            1 => {}
            n => out.push(format!(
                "docs/api/index.md has {n} rows for the crate {name}"
            )),
        }
    }
    for row in rows {
        if !crates.contains(&row.name) {
            out.push(format!(
                "docs/api/index.md has a row for {}, which is no crate under crates/ with a library",
                row.name
            ));
            continue;
        }
        let want = format!("{site_url}api/{}/", row.name.replace('-', "_"));
        if row.target != want {
            out.push(format!(
                "docs/api/index.md links {} to {}, but its reference is at {want}",
                row.name, row.target
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: &str = "https://vtium.github.io/vtium/";

    #[test]
    fn rows_are_table_lines_whose_first_cell_is_a_link() {
        let index = "| Crate | What |\n|---|---|\n| [vtium-term](https://vtium.github.io/vtium/api/vtium_term/) | The screen. |\n| plain | text |\n\n[vtium-parse](x) in prose\n";
        assert_eq!(
            crate_rows(index),
            [Row {
                name: "vtium-term".into(),
                target: "https://vtium.github.io/vtium/api/vtium_term/".into()
            }]
        );
    }

    #[test]
    fn a_missing_crate_a_stray_row_a_twice_listed_crate_and_a_wrong_link_are_faults() {
        let rows = crate_rows(
            "| [vtium-term](https://vtium.github.io/vtium/api/vtium_term/) | |\n\
             | [vtium-term](https://vtium.github.io/vtium/api/vtium_term/) | |\n\
             | [vtium-parse](https://vtium.github.io/vtium/api/vtium-parse/) | |\n\
             | [vtium-gone](https://vtium.github.io/vtium/api/vtium_gone/) | |\n",
        );
        let crates = ["vtium-font", "vtium-parse", "vtium-term"].map(String::from);
        assert_eq!(
            crate_index_faults(&rows, &crates, SITE),
            [
                "docs/api/index.md has no row for the crate vtium-font",
                "docs/api/index.md has 2 rows for the crate vtium-term",
                "docs/api/index.md links vtium-parse to https://vtium.github.io/vtium/api/vtium-parse/, but its reference is at https://vtium.github.io/vtium/api/vtium_parse/",
                "docs/api/index.md has a row for vtium-gone, which is no crate under crates/ with a library",
            ]
        );
    }
}
