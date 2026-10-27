//! The expected differences: vtium-parse's departures, as `departures.tsv`
//! lists them and `src/c_side.rs` applies them to the C side.

use crate::pin::rows;

const TSV: &str = include_str!("../departures.tsv");
const PARSE_README: &str = include_str!("../../../crates/vtium-parse/README.md");

/// A departure of vtium-parse from the C reference, one per row of
/// `departures.tsv` and bullet of the parse README's Departures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Departure {
    StringEnd,
    Intermediates,
    ResetApcSt,
    EmptyParams,
    C1Utf8,
    GrAsGl,
    PastAscii,
    OscBound,
    OscPrefix,
}

impl Departure {
    pub const ALL: [Departure; 9] = [
        Departure::StringEnd,
        Departure::Intermediates,
        Departure::ResetApcSt,
        Departure::EmptyParams,
        Departure::C1Utf8,
        Departure::GrAsGl,
        Departure::PastAscii,
        Departure::OscBound,
        Departure::OscPrefix,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Departure::StringEnd => "string-end",
            Departure::Intermediates => "intermediates",
            Departure::ResetApcSt => "reset-apc-st",
            Departure::EmptyParams => "empty-params",
            Departure::C1Utf8 => "c1-utf8",
            Departure::GrAsGl => "gr-as-gl",
            Departure::PastAscii => "past-ascii",
            Departure::OscBound => "osc-bound",
            Departure::OscPrefix => "osc-prefix",
        }
    }

    pub fn from_id(id: &str) -> Result<Departure, String> {
        Departure::ALL
            .into_iter()
            .find(|d| d.id() == id)
            .ok_or_else(|| format!("no departure {id:?}"))
    }
}

/// The departures applied to the C side.
#[derive(Clone, Copy, Default)]
pub struct Departs(u16);

impl Departs {
    pub fn has(self, d: Departure) -> bool {
        self.0 & 1 << d as u16 != 0
    }

    pub fn with(self, d: Departure) -> Self {
        Departs(self.0 | 1 << d as u16)
    }

    pub fn ids(self) -> Vec<&'static str> {
        Departure::ALL
            .into_iter()
            .filter(|&d| self.has(d))
            .map(Departure::id)
            .collect()
    }
}

/// The bold lead of each bullet under the parse README's Departures.
fn readme_departures() -> Result<Vec<&'static str>, String> {
    let start = PARSE_README
        .find("\n## Departures\n")
        .ok_or("the parse README has no Departures section")?;
    let section = &PARSE_README[start + 1..];
    let section = &section[..section[3..].find("\n## ").map_or(section.len(), |i| i + 3)];
    Ok(section
        .lines()
        .filter_map(|line| line.strip_prefix("- **"))
        .filter_map(|line| line.split_once("**").map(|(lead, _)| lead))
        .collect())
}

/// The departures `departures.tsv` expects, in the README's order, refused
/// unless its rows and the README's list match lead for lead.
pub fn expected() -> Result<Vec<Departure>, String> {
    let table: Vec<(&str, &str)> = rows(TSV)
        .map(|row| match row[..] {
            [id, lead, _] => Ok((id, lead)),
            _ => Err(format!(
                "departures.tsv: a row without three fields: {row:?}"
            )),
        })
        .collect::<Result<_, _>>()?;
    let listed = readme_departures()?;
    let mut problems = String::new();
    for lead in &listed {
        if !table.iter().any(|(_, l)| l == lead) {
            problems += &format!("\n  the README lists, the oracle does not expect: {lead}");
        }
    }
    for (_, lead) in &table {
        if !listed.contains(lead) {
            problems += &format!("\n  the oracle expects, the README does not list: {lead}");
        }
    }
    if !problems.is_empty() {
        return Err(format!(
            "departures.tsv and the parse README disagree:{problems}"
        ));
    }
    listed
        .iter()
        .map(|lead| {
            let (id, _) = table
                .iter()
                .find(|(_, l)| l == lead)
                .expect("checked above");
            Departure::from_id(id)
        })
        .collect()
}
