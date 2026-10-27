//! The runner's lists: `pin.tsv`, the commits it judges against, and
//! `expected.tsv`, the cases where vtium's output is expected to differ
//! from the golden.

use std::collections::BTreeMap;
use std::path::Path;

/// A repository the runner reads at a pinned commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub name: String,
    pub commit: String,
    pub url: String,
}

/// Reads `pin.tsv`: `NAME COMMIT URL` a line, `#` comments.
pub fn pins(text: &str) -> Result<Vec<Pin>, String> {
    rows(text)
        .map(|(n, fields)| match fields[..] {
            [name, commit, url]
                if commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                Ok(Pin {
                    name: name.to_owned(),
                    commit: commit.to_owned(),
                    url: url.to_owned(),
                })
            }
            _ => Err(format!("pin.tsv:{n}: want NAME, a 40-digit COMMIT and URL")),
        })
        .collect()
}

pub fn pin<'a>(pins: &'a [Pin], name: &str) -> Result<&'a Pin, String> {
    pins.iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("pin.tsv names no {name} commit"))
}

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
    /// vtium lacks what the case needs, and differs from the golden.
    Pending,
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
/// text of a SOURCE.
pub fn check_sources(
    rows: &[Row],
    read: impl Fn(&str) -> Result<String, String>,
) -> Result<(), String> {
    let mut texts: BTreeMap<&str, String> = BTreeMap::new();
    for row in rows.iter().filter(|r| !r.pending) {
        if !texts.contains_key(row.source.as_str()) {
            texts.insert(&row.source, read(&row.source)?);
        }
        let found = texts
            .get(row.source.as_str())
            .is_some_and(|t| t.contains(&row.entry));
        if !found {
            return Err(format!(
                "expected.tsv:{}: {} has no entry {:?}",
                row.line, row.source, row.entry
            ));
        }
    }
    Ok(())
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
    let mut out: BTreeMap<(String, String), Expected> = BTreeMap::new();
    for view in views {
        for row in rows.iter().filter(|r| r.view == "*" || r.view == *view) {
            let key = ((*view).to_owned(), row.case.clone());
            let entry = out.entry(key).or_insert_with(|| Expected {
                kind: Expectation::Pending,
                rows: Vec::new(),
            });
            entry.rows.push(row.clone());
        }
    }
    for ((view, case), e) in &mut out {
        if e.rows.iter().any(|r| r.pending) {
            continue;
        }
        let path = dir.join(view).join(format!("{case}.txt"));
        let text = std::fs::read_to_string(&path).map_err(|_| {
            format!(
                "{case} is a departure in the {view} view with no {}; write it with `conformance expect {view} {case}` and read it",
                path.display()
            )
        })?;
        e.kind = Expectation::Departure(text);
    }
    // an expected output no departure row asks for is stale
    for view in views {
        let Ok(files) = std::fs::read_dir(dir.join(view)) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            let case = name.strip_suffix(".txt").unwrap_or(&name).to_owned();
            let listed = out
                .get(&((*view).to_owned(), case))
                .is_some_and(|e| matches!(e.kind, Expectation::Departure(_)));
            if !listed {
                return Err(format!(
                    "{} has no departure row in expected.tsv",
                    file.path().display()
                ));
            }
        }
    }
    Ok(out)
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
    fn a_departure_names_an_entry_its_source_holds() {
        let rows = expected_rows("*\tx\tdeparture\tREADME.md\tDA1 reports\n").unwrap_or_default();
        assert_eq!(
            check_sources(&rows, |_| Ok("- **DA1 reports a VT220**".to_owned())),
            Ok(())
        );
        assert!(check_sources(&rows, |_| Ok("- **DA2 reports**".to_owned())).is_err());
    }

    #[test]
    fn pins_need_a_whole_commit() {
        let commit = "e".repeat(40);
        assert_eq!(
            pins(&format!(
                "libtsm\t{commit}\thttps://example.org/libtsm.git\n"
            ))
            .map(|p| p.len()),
            Ok(1)
        );
        assert!(pins("libtsm\teeaf293\thttps://example.org/libtsm.git\n").is_err());
    }
}
