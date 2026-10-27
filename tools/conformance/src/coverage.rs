//! The COVERAGE map: each capability of `capabilities.list`, the ADR-1000
//! rows its probe judges, the tests that guard those rows, and the gap.
//! It is generated from the list and ADR-1000's tables, and a test fails
//! when the committed `COVERAGE.md` differs.

use core::fmt::Write;
use std::collections::BTreeMap;

/// A capability: its probe, its status, the issue or build feature, its
/// ADR-1000 rows and its note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capability {
    pub name: String,
    pub status: String,
    pub issue: String,
    pub rows: Vec<String>,
    pub note: String,
}

/// Reads `capabilities.list`: NAME STATUS ISSUE ROWS NOTE a line.
pub fn capabilities(text: &str) -> Result<Vec<Capability>, String> {
    let mut out: Vec<Capability> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let (Some(name), Some(status), Some(issue), Some(rows)) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            return Err(format!(
                "capabilities.list:{}: want NAME STATUS ISSUE ROWS NOTE",
                i.saturating_add(1)
            ));
        };
        let note = words.collect::<Vec<_>>().join(" ");
        if !matches!(status, "works" | "missing") {
            return Err(format!(
                "capabilities.list:{}: status {status:?}",
                i.saturating_add(1)
            ));
        }
        if out.iter().any(|c| c.name == name) {
            return Err(format!(
                "capabilities.list:{}: {name} twice",
                i.saturating_add(1)
            ));
        }
        out.push(Capability {
            name: name.to_owned(),
            status: status.to_owned(),
            issue: issue.to_owned(),
            rows: rows.split(',').map(str::to_owned).collect(),
            note,
        });
    }
    Ok(out)
}

/// Each ADR-1000 row's id and its `tests` column, from both tables.
pub fn adr_tests(tables: &[&str]) -> Result<BTreeMap<String, Vec<String>>, String> {
    let mut out = BTreeMap::new();
    for table in tables {
        let mut lines = table.lines();
        let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
        let tests = header
            .iter()
            .position(|h| *h == "tests")
            .ok_or("an ADR-1000 table has no tests column")?;
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            let (Some(id), Some(t)) = (fields.first(), fields.get(tests)) else {
                continue;
            };
            let t = t
                .split(", ")
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            out.insert((*id).to_owned(), t);
        }
    }
    Ok(out)
}

/// The COVERAGE map's text.
pub fn render(
    caps: &[Capability],
    tests: &BTreeMap<String, Vec<String>>,
) -> Result<String, String> {
    let mut out = String::from(
        "# Coverage\n\n<!-- generated from capabilities.list and ADR-1000's tables; VTIUM_CONFORMANCE_BLESS=1 cargo test -p vtium-conformance coverage rewrites it -->\n\nEach capability the C reference's probes judge, the ADR-1000 rows its probe covers, the tests that guard those rows, and the gap: the issue that owes a missing capability, or a row no test guards yet. The probes themselves run through `conformance probes` (README.md).\n\n| Capability | Status | ADR-1000 rows | Tests | Gap |\n|---|---|---|---|---|\n",
    );
    for c in caps {
        let mut guards: Vec<&str> = Vec::new();
        let mut untested: Vec<&str> = Vec::new();
        for row in &c.rows {
            let t = tests.get(row).ok_or_else(|| {
                format!(
                    "capabilities.list: {} names {row}, which ADR-1000 lacks",
                    c.name
                )
            })?;
            if t.is_empty() {
                untested.push(row);
            }
            for test in t {
                if !guards.contains(&test.as_str()) {
                    guards.push(test);
                }
            }
        }
        let mut gap = Vec::new();
        if c.status == "missing" {
            gap.push(format!("missing, {}", c.issue));
        } else if c.issue.starts_with("-D") {
            gap.push(format!(
                "needs the build feature {}",
                c.issue.trim_start_matches("-D")
            ));
        }
        if !untested.is_empty() {
            gap.push(format!(
                "no test for {}",
                untested
                    .iter()
                    .map(|r| format!("`{r}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let code = |v: &[&str]| {
            v.iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join("<br>")
        };
        let rows: Vec<&str> = c.rows.iter().map(String::as_str).collect();
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} |",
            c.name,
            c.status,
            code(&rows),
            code(&guards),
            gap.join("; ")
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|e| format!("{}: {e}", path.display()))
    }

    #[test]
    fn coverage_map_matches_the_capability_list_and_adr_1000() {
        let tool = Path::new(env!("CARGO_MANIFEST_DIR"));
        let adr = tool.join("../../docs/architecture/term");
        let caps = capabilities(&read(&tool.join("capabilities.list")));
        let tests = adr_tests(&[
            &read(&adr.join("ADR-1000-features.tsv")),
            &read(&adr.join("ADR-1000-modes.tsv")),
        ]);
        let want = caps.and_then(|c| tests.and_then(|t| render(&c, &t)));
        let path = tool.join("COVERAGE.md");
        // an error reads as the difference
        let want = want.unwrap_or_else(|e| e);
        if std::env::var_os("VTIUM_CONFORMANCE_BLESS").is_some() {
            let _ = std::fs::write(&path, &want);
        }
        assert_eq!(
            read(&path),
            want,
            "COVERAGE.md differs from capabilities.list and ADR-1000; VTIUM_CONFORMANCE_BLESS=1 rewrites it"
        );
    }

    #[test]
    fn a_row_adr_1000_lacks_is_refused() {
        let caps = capabilities("p works - no.such-row note\n").unwrap_or_default();
        assert!(render(&caps, &BTreeMap::new()).is_err());
    }
}
