//! ADR-1000's issue column held to GitHub's issue states.
//!
//! `conformance issues` reads both ADR-1000 tables, asks `gh` once for the
//! state of every issue and once, when a number is not an issue, for every
//! pull request, and fails when a table and GitHub disagree:
//!
//! - a planned, partial or weighed row names no issue, or names only
//!   closed issues, so nothing open carries its work or its decision;
//! - an issue that a supported row names is open, and no planned, partial
//!   or weighed row names it, so the open issue's remaining work has no
//!   row, or the issue's work is done and the issue was never closed;
//! - a number in the issue column is a pull request or does not exist.
//!
//! The rules take the states as a lookup, so the tests need no network.

use std::collections::BTreeMap;

/// What a number in the issue column names on vtium/vtium.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Open,
    Closed,
    PullRequest,
    Missing,
}

/// One ADR-1000 row: its id, its status and the issues its issue column
/// names, in the order the column names them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub status: String,
    pub issues: Vec<u32>,
}

/// The statuses whose row waits on an open issue: one that builds it, builds
/// the rest of it, or decides it.
const OWED: [&str; 3] = ["planned", "partial", "weighed"];

/// Reads one ADR-1000 table: the `id`, `status` and `issue` columns of each
/// row. An issue cell is empty or a list of `#N` joined by `, `.
pub fn rows(name: &str, table: &str) -> Result<Vec<Row>, String> {
    let mut lines = table.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
    let column = |want: &str| {
        header
            .iter()
            .position(|h| *h == want)
            .ok_or_else(|| format!("{name} has no {want} column"))
    };
    let (id, status, issue) = (column("id")?, column("status")?, column("issue")?);
    let mut out = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let field = |i: usize| fields.get(i).copied().unwrap_or_default();
        let row_id = field(id);
        let issues = field(issue)
            .split(", ")
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.strip_prefix('#')
                    .and_then(|n| n.parse::<u32>().ok())
                    .ok_or_else(|| {
                        format!(
                            "{name}: the issue column of row {row_id} holds {s:?}. It holds issues written as #N, joined by a comma and a space."
                        )
                    })
            })
            .collect::<Result<Vec<u32>, String>>()?;
        out.push(Row {
            id: row_id.to_owned(),
            status: field(status).to_owned(),
            issues,
        });
    }
    Ok(out)
}

/// What an open issue does for a row of this status.
fn owed_work(status: &str) -> &'static str {
    match status {
        "partial" => "builds the rest of it",
        "weighed" => "decides it",
        _ => "builds it",
    }
}

fn list(issues: &[u32]) -> String {
    issues
        .iter()
        .map(|n| format!("#{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every disagreement between the rows and the issue states, one message
/// each, in the order of the rows and then of the issues.
pub fn check(rows: &[Row], state: impl Fn(u32) -> State) -> Vec<String> {
    let mut out = Vec::new();
    for row in rows {
        for &n in &row.issues {
            match state(n) {
                State::PullRequest => out.push(format!(
                    "ADR-1000 row {} names #{n}, which is a pull request. The issue column names only issues: name the issue that carries the row's work.",
                    row.id
                )),
                State::Missing => out.push(format!(
                    "ADR-1000 row {} names #{n}, which is not an issue on vtium/vtium. Name the issue that carries the row's work.",
                    row.id
                )),
                State::Open | State::Closed => {}
            }
        }
        if !OWED.contains(&row.status.as_str()) {
            continue;
        }
        if row.issues.is_empty() {
            out.push(format!(
                "ADR-1000 row {} is {} and names no issue. Add the open issue that {} to the row's issue column.",
                row.id,
                row.status,
                owed_work(&row.status)
            ));
        } else if row.issues.iter().all(|&n| state(n) == State::Closed) {
            out.push(format!(
                "ADR-1000 row {} is {}, and every issue it names is closed: {}. Name the open issue that {}, or change the row's status in ADR-1000 if its work is done.",
                row.id,
                row.status,
                list(&row.issues),
                owed_work(&row.status)
            ));
        }
    }
    // each open issue a supported row names, with the supported rows that
    // name it, when no row still waiting on work names it
    let mut supported: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.status == "supported") {
        for &n in &row.issues {
            if state(n) == State::Open {
                supported.entry(n).or_default().push(&row.id);
            }
        }
    }
    for (n, ids) in supported {
        let owed = rows
            .iter()
            .any(|r| OWED.contains(&r.status.as_str()) && r.issues.contains(&n));
        if !owed {
            out.push(format!(
                "Issue #{n} is open, and only supported ADR-1000 rows name it: {}. Close the issue if its work is done, or add a planned or partial row that names it for the work it still holds.",
                ids.join(", ")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, status: &str, issues: &[u32]) -> Row {
        Row {
            id: id.to_owned(),
            status: status.to_owned(),
            issues: issues.to_vec(),
        }
    }

    /// #1 to #9 are open issues, #10 to #19 closed ones, #20 a pull request
    /// and the rest missing.
    fn state(n: u32) -> State {
        match n {
            1..=9 => State::Open,
            10..=19 => State::Closed,
            20 => State::PullRequest,
            _ => State::Missing,
        }
    }

    #[test]
    fn rows_in_step_with_their_issues_pass() {
        let rows = [
            row("a.done", "supported", &[10]),
            row("a.part", "supported", &[1, 10]),
            row("a.rest", "planned", &[1]),
            row("b.part", "partial", &[10, 2]),
            row("c.open", "weighed", &[3]),
            row("d.ignored", "ignored", &[]),
            row("e.refused", "refused", &[11]),
            row("f.heads", "supported", &[]),
        ];
        assert_eq!(check(&rows, state), Vec::<String>::new());
    }

    #[test]
    fn a_planned_partial_or_weighed_row_with_no_issue_fails() {
        let rows = [
            row("a.planned", "planned", &[]),
            row("b.partial", "partial", &[]),
            row("c.weighed", "weighed", &[]),
        ];
        assert_eq!(
            check(&rows, state),
            [
                "ADR-1000 row a.planned is planned and names no issue. Add the open issue that builds it to the row's issue column.",
                "ADR-1000 row b.partial is partial and names no issue. Add the open issue that builds the rest of it to the row's issue column.",
                "ADR-1000 row c.weighed is weighed and names no issue. Add the open issue that decides it to the row's issue column.",
            ]
        );
    }

    #[test]
    fn a_row_waiting_on_work_whose_issues_are_all_closed_fails() {
        let rows = [
            row("a.weighed", "weighed", &[10, 12]),
            row("b.planned", "planned", &[11, 4]),
        ];
        assert_eq!(
            check(&rows, state),
            [
                "ADR-1000 row a.weighed is weighed, and every issue it names is closed: #10, #12. Name the open issue that decides it, or change the row's status in ADR-1000 if its work is done."
            ]
        );
    }

    #[test]
    fn an_open_issue_only_supported_rows_name_fails() {
        let rows = [
            row("a.one", "supported", &[5]),
            row("a.two", "supported", &[10, 5]),
            row("b.ignored", "ignored", &[5]),
            row("c.one", "supported", &[6]),
            row("c.rest", "partial", &[6]),
        ];
        assert_eq!(
            check(&rows, state),
            [
                "Issue #5 is open, and only supported ADR-1000 rows name it: a.one, a.two. Close the issue if its work is done, or add a planned or partial row that names it for the work it still holds."
            ]
        );
    }

    #[test]
    fn a_number_that_is_not_an_issue_fails() {
        let rows = [
            row("a.pr", "supported", &[20]),
            row("b.gone", "planned", &[3, 404]),
        ];
        assert_eq!(
            check(&rows, state),
            [
                "ADR-1000 row a.pr names #20, which is a pull request. The issue column names only issues: name the issue that carries the row's work.",
                "ADR-1000 row b.gone names #404, which is not an issue on vtium/vtium. Name the issue that carries the row's work.",
            ]
        );
    }

    #[test]
    fn a_table_reads_its_id_status_and_issue_columns() {
        let table = "id\tstatus\treason\tissue\ttests\n\
                     a.one\tsupported\tsome reason\t#12\tx.rs::t\n\
                     a.two\tplanned\t\t#3, #44\t\n\
                     a.three\tsupported\t\t\t\n";
        assert_eq!(
            rows("t.tsv", table),
            Ok(vec![
                row("a.one", "supported", &[12]),
                row("a.two", "planned", &[3, 44]),
                row("a.three", "supported", &[]),
            ])
        );
        assert_eq!(
            rows("t.tsv", "id\tstatus\tissue\na\tplanned\t12\n"),
            Err("t.tsv: the issue column of row a holds \"12\". It holds issues written as #N, joined by a comma and a space.".to_owned())
        );
        assert_eq!(
            rows("t.tsv", "id\tstatus\n"),
            Err("t.tsv has no issue column".to_owned())
        );
    }

    #[test]
    fn adr_1000s_tables_read() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/architecture/term");
        for name in ["ADR-1000-features.tsv", "ADR-1000-modes.tsv"] {
            let text = std::fs::read_to_string(dir.join(name)).unwrap();
            let read = rows(name, &text).unwrap();
            assert!(read.len() > 100, "{name}: {} rows", read.len());
        }
    }
}
