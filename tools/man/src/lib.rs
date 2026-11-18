//! vtium's man pages, generated in roff from code and from the templates in
//! `templates/`. Each page takes its facts from the code that defines them:
//! vtium.conf(5) its keys from the key table, vtium(8) its subcommands from
//! the console's usage table, and vtium-config(1) its commands, options and
//! exit statuses from vtium-config's own lists. The templates hold the
//! prose that no table holds, such as the units and the files.
//!
//! [`pages`] writes every page; `make man` saves them in `man/`, and
//! `make man-check` fails when a committed page differs.

pub mod roff;

mod clipboard;
mod conf;
mod config;
mod console;
mod gallery;

/// One page: its file name, such as `vtium.conf.5`, and its roff.
pub struct Page {
    pub file: &'static str,
    pub text: String,
}

/// vtium's release notes index: a table with a row for each release, its
/// version and its date. The pages carry the date of the version they
/// describe.
const RELEASES: &str = include_str!("../../../docs/releases/index.md");

/// The version every page names in its footer.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The date of `version` in the release notes index `index`, as
/// `YYYY-MM-DD`. A version the index does not list yet, as between the
/// version's bump and its release notes, takes the date of the newest
/// release.
fn release_date(index: &str, version: &str) -> String {
    let rows: Vec<(String, String)> = index
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let (name, date) = (cells.get(1)?, cells.get(2)?);
            let valid = date.len() == 10 && date.bytes().filter(|&b| b == b'-').count() == 2;
            let name = name.strip_prefix('[')?.split(']').next()?;
            valid.then(|| (name.to_owned(), (*date).to_owned()))
        })
        .collect();
    rows.iter()
        .find(|(v, _)| v == version)
        .or_else(|| rows.iter().max_by(|a, b| a.1.cmp(&b.1)))
        .map(|(_, date)| date.clone())
        .unwrap_or_default()
}

/// What each page's title line carries: its date and its source.
pub(crate) struct Stamp {
    pub date: String,
    pub source: String,
}

fn stamp() -> Stamp {
    Stamp {
        date: release_date(RELEASES, VERSION),
        source: format!("vtium {VERSION}"),
    }
}

/// Every page, in the order `man/` lists them.
pub fn pages() -> Vec<Page> {
    let stamp = stamp();
    vec![
        Page {
            file: "vtium.8",
            text: console::page(&stamp),
        },
        Page {
            file: "vtium-config.1",
            text: config::page(&stamp),
        },
        Page {
            file: "vtium.conf.5",
            text: conf::page(&stamp),
        },
        Page {
            file: "vtium-clipboard.1",
            text: clipboard::page(&stamp),
        },
        Page {
            file: "vtium-gallery.1",
            text: gallery::page(&stamp),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_date_comes_from_its_row_or_else_the_newest_row() {
        let index = "| Version | Date | What | Link |\n|---|---|---|---|\n\
                     | [0.2.0](0.2.0.md) | 2026-12-01 | b | c |\n\
                     | [0.1.0](0.1.0.md) | 2026-11-01 | b | c |\n";
        assert_eq!(release_date(index, "0.1.0"), "2026-11-01");
        assert_eq!(release_date(index, "0.3.0"), "2026-12-01");
    }

    #[test]
    fn this_version_has_a_date() {
        assert_eq!(stamp().date.len(), 10);
    }
}
