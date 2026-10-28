//! The runner's list `expected.tsv`: the cases where vtium's output is
//! expected to differ from the golden.

use std::collections::BTreeMap;
use std::path::Path;

/// The non-comment lines of a TSV, numbered from 1, split at tabs.
fn rows(text: &str) -> impl Iterator<Item = (usize, Vec<&str>)> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|(i, l)| (i.saturating_add(1), l.split('\t').collect()))
}

/// One row of `expected.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub line: usize,
    /// `screen`, `frame`, or `*` for both.
    pub view: String,
    pub case: String,
    pub pending: bool,
    /// The file that lists the departure, or the issue that owes the case.
    pub source: String,
    /// The departure's entry in that file, or what vtium lacks.
    pub entry: String,
}

/// What the runner expects of a listed case in one view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expectation {
    /// vtium prints this, which differs from the golden.
    Departure(String),
    /// vtium lacks what the case needs: its output, or its exit as
    /// [`snapshot`](crate::runner::snapshot) writes it, differs from the
    /// golden and stays as this snapshot until the case is taken up.
    Pending(String),
}

/// A listed case in one view: the expectation and the rows behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expected {
    pub kind: Expectation,
    pub rows: Vec<Row>,
}

/// Reads `expected.tsv`: `VIEW CASE KIND SOURCE ENTRY` a line, KIND
/// `departure` or `pending`. A departure's SOURCE is a file the repository
/// holds, or `INTERFACE.md` for the pinned interface, and ENTRY must appear
/// in it word for word; a pending case's SOURCE is the issue that owes it.
pub fn expected_rows(text: &str) -> Result<Vec<Row>, String> {
    let mut out: Vec<Row> = Vec::new();
    for (n, fields) in rows(text) {
        let [view, case, kind, source, entry] = fields[..] else {
            return Err(format!(
                "expected.tsv:{n}: want VIEW CASE KIND SOURCE ENTRY"
            ));
        };
        if !matches!(view, "screen" | "frame" | "*") {
            return Err(format!(
                "expected.tsv:{n}: view {view:?} is not screen, frame or *"
            ));
        }
        let pending = match kind {
            "pending" => true,
            "departure" => false,
            _ => {
                return Err(format!(
                    "expected.tsv:{n}: kind {kind:?} is not departure or pending"
                ));
            }
        };
        let issue = source
            .strip_prefix('#')
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
        if pending != issue {
            return Err(format!(
                "expected.tsv:{n}: a pending row names an issue (#N), a departure the file that lists it"
            ));
        }
        if entry.trim().is_empty() {
            return Err(format!("expected.tsv:{n}: no entry"));
        }
        let row = Row {
            line: n,
            view: view.to_owned(),
            case: case.to_owned(),
            pending,
            source: source.to_owned(),
            entry: entry.to_owned(),
        };
        if out.iter().any(|r| {
            (&r.view, &r.case, &r.source, &r.entry)
                == (&row.view, &row.case, &row.source, &row.entry)
        }) {
            return Err(format!("expected.tsv:{n}: the same row twice"));
        }
        out.push(row);
    }
    Ok(out)
}

/// Checks each departure's entry against its source: `read` returns the
/// text of a SOURCE. In a README the entry must sit under a
/// `## Departures` or a `## Golden differences` heading. In `INTERFACE.md`
/// the entry must be a sentence that lets an implementation list the case
/// as a departure.
pub fn check_sources(
    rows: &[Row],
    read: impl Fn(&str) -> Result<String, String>,
) -> Result<(), String> {
    let mut texts: BTreeMap<&str, String> = BTreeMap::new();
    for row in rows.iter().filter(|r| !r.pending) {
        if !texts.contains_key(row.source.as_str()) {
            texts.insert(&row.source, read(&row.source)?);
        }
        let text = texts.get(row.source.as_str()).map_or("", String::as_str);
        let found = if row.source == "INTERFACE.md" {
            row.entry.contains("may list it as a departure") && text.contains(&row.entry)
        } else {
            departure_lines(text).any(|line| line.contains(&row.entry))
        };
        if !found {
            return Err(format!(
                "expected.tsv:{}: {} has no {:?} under a Departures or a Golden differences heading",
                row.line, row.source, row.entry
            ));
        }
    }
    Ok(())
}

/// The lines of a README under a `## Departures` or a
/// `## Golden differences` heading.
pub fn departure_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut listed = false;
    text.lines().filter(move |line| {
        if line.starts_with('#') {
            listed = matches!(*line, "## Departures" | "## Golden differences");
            return false;
        }
        listed
    })
}

/// The expectation for each listed case in each view, keyed by view and
/// case. A departure's output is `expected/<view>/<case>.txt` under `dir`;
/// a case with a pending row is pending whatever else is listed, and needs
/// no file.
pub fn expectations(
    rows: &[Row],
    views: &[&str],
    dir: &Path,
) -> Result<BTreeMap<(String, String), Expected>, String> {
    let mut grouped: BTreeMap<(String, String), Vec<Row>> = BTreeMap::new();
    for view in views {
        for row in rows.iter().filter(|r| r.view == "*" || r.view == *view) {
            grouped
                .entry(((*view).to_owned(), row.case.clone()))
                .or_default()
                .push(row.clone());
        }
    }
    let mut out = BTreeMap::new();
    for ((view, case), rows) in grouped {
        let pending = rows.iter().any(|r| r.pending);
        let path = expected_path(dir, &view, &case, pending);
        let text = std::fs::read_to_string(&path).map_err(|_| {
            format!(
                "{case} is {} in the {view} view with no {}; write it with `conformance expect {view} {case}` and read it",
                if pending { "pending" } else { "a departure" },
                path.display()
            )
        })?;
        let kind = if pending {
            Expectation::Pending(text)
        } else {
            Expectation::Departure(text)
        };
        out.insert((view, case), Expected { kind, rows });
    }
    // an expected output or snapshot no row asks for is stale
    for view in views {
        for pending in [false, true] {
            let sub = if pending {
                dir.join("pending").join(view)
            } else {
                dir.join(view)
            };
            let Ok(files) = std::fs::read_dir(&sub) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();
                let case = name.strip_suffix(".txt").unwrap_or(&name).to_owned();
                let listed = out
                    .get(&((*view).to_owned(), case))
                    .is_some_and(|e| matches!(e.kind, Expectation::Pending(_)) == pending);
                if !listed {
                    return Err(format!(
                        "{} has no {} row in expected.tsv",
                        file.path().display(),
                        if pending { "pending" } else { "departure" }
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// Where a listed case's expected output lives: `<view>/<case>.txt` for a
/// departure, `pending/<view>/<case>.txt` for a pending case's snapshot.
pub fn expected_path(dir: &Path, view: &str, case: &str, pending: bool) -> std::path::PathBuf {
    let dir = if pending {
        dir.join("pending")
    } else {
        dir.to_owned()
    };
    dir.join(view).join(format!("{case}.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_checked_as_they_are_read() {
        let ok = "# comment\n*\tx\tpending\t#48\tsixel\nscreen\ty\tdeparture\tcrates/a/README.md\tAn entry.\n";
        assert_eq!(expected_rows(ok).map(|r| r.len()), Ok(2));
        for bad in [
            "*\tx\tpending\tREADME.md\tsixel\n",
            "*\tx\tdeparture\t#48\tsixel\n",
            "both\tx\tpending\t#48\tsixel\n",
            "*\tx\tlater\t#48\tsixel\n",
            "*\tx\tpending\t#48\n",
            "*\tx\tpending\t#48\t \n",
            "*\tx\tpending\t#48\ta\n*\tx\tpending\t#48\ta\n",
        ] {
            assert!(expected_rows(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_departure_names_an_entry_its_source_lists_as_one() {
        let rows = expected_rows("*\tx\tdeparture\tREADME.md\tDA1 reports\n").unwrap_or_default();
        let readme = |text: &'static str| move |_: &str| Ok(text.to_owned());
        let departures = "# c\n\n## Departures\n\n- **DA1 reports a VT220**\n\n## Bounds\n";
        assert_eq!(check_sources(&rows, readme(departures)), Ok(()));
        assert!(check_sources(&rows, readme("## Departures\n\n- **DA2 reports**\n")).is_err());
        // the entry anywhere else in the file is not a listed departure
        assert!(check_sources(&rows, readme("## Notes\n\n- **DA1 reports a VT220**\n")).is_err());
        assert!(
            check_sources(&rows, readme("## Departures\n\n## Bounds\n\nDA1 reports\n")).is_err()
        );
        let goldens = "## Golden differences\n\n- `x`: DA1 reports a VT220.\n\n## Bounds\n";
        assert_eq!(check_sources(&rows, readme(goldens)), Ok(()));
        // a goldens list under any other heading is not one
        let bullet =
            "## Notes\n\n- **Goldens the departures move**:\n  - `x`: DA1 reports a VT220.\n";
        assert!(check_sources(&rows, readme(bullet)).is_err());
        assert!(check_sources(&rows, readme("## Golden cases\n\nDA1 reports\n")).is_err());
        assert!(check_sources(&rows, readme("### Departures\n\nDA1 reports\n")).is_err());
        let iface =
            expected_rows("*\tx\tdeparture\tINTERFACE.md\tDA1 reports\n").unwrap_or_default();
        assert!(check_sources(&iface, readme("DA1 reports")).is_err());
        let iface = expected_rows("*\tx\tdeparture\tINTERFACE.md\tY may list it as a departure.\n")
            .unwrap_or_default();
        assert_eq!(
            check_sources(&iface, readme("X. Y may list it as a departure.")),
            Ok(())
        );
    }
}
