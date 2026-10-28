//! [`Manifest`]: the store's list of sets and their hashes (ADR-1200 D3),
//! a TSV file:
//!
//! ```text
//! # vtium-vitrify manifest 1
//! name<TAB>sha256<TAB>build-key<TAB>format<TAB>cells<TAB>source
//! ter-v16n<TAB><64 hex digits><TAB><64 hex digits><TAB>1.0<TAB>8x16<TAB>kbd:ter-v16n
//! ```
//!
//! The fields are separated by tabs. One row a set, sorted by name: its name, the set file's SHA-256, its
//! build key, its format version, its strikes' cell sizes, comma-separated
//! and smallest first, and its first source as the frit spelled it.

use std::fmt;

use super::format::valid_name;
use crate::bounds::{CELL_SIDE_MAX, MANIFEST_BYTES_MAX, MANIFEST_ROWS_MAX};
use crate::face::Cell;
use crate::sha256::{hex, parse_hex};

const FIRST_LINE: &str = "# vtium-vitrify manifest 1";
const HEADER: &str = "name\tsha256\tbuild-key\tformat\tcells\tsource";

/// Why a manifest was refused: its line, from 1, and the reason.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ManifestError {
    pub line: usize,
    pub why: &'static str,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "manifest line {}: {}", self.line, self.why)
    }
}

impl std::error::Error for ManifestError {}

/// One set the store holds.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ManifestRow {
    pub name: String,
    /// The set file's SHA-256, which names it `<sha256>.vtglass`.
    pub sha256: [u8; 32],
    pub build_key: [u8; 32],
    pub version: (u16, u16),
    pub cells: Vec<Cell>,
    /// The frit's first source, as it spelled it.
    pub source: String,
}

impl ManifestRow {
    /// The set's file name in the store.
    pub fn file_name(&self) -> String {
        format!("{}.vtglass", hex(&self.sha256))
    }
}

/// The store's manifest: its rows, sorted by name, no name twice.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Manifest {
    rows: Vec<ManifestRow>,
}

impl Manifest {
    /// A manifest of `rows`, sorted; refused when a row would not read
    /// back or two share a name.
    pub fn new(mut rows: Vec<ManifestRow>) -> Result<Manifest, ManifestError> {
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        let manifest = Manifest { rows };
        Manifest::parse(&manifest.to_tsv())?;
        Ok(manifest)
    }

    /// Parses a manifest's text, every line and field checked and bounded.
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let err = |line: usize, why| ManifestError { line, why };
        if text.len() > usize::try_from(MANIFEST_BYTES_MAX).unwrap_or(usize::MAX) {
            return Err(err(0, "larger than MANIFEST_BYTES_MAX"));
        }
        if !text.ends_with('\n') {
            return Err(err(0, "its last line is not ended"));
        }
        let mut lines = text.lines().zip(1usize..);
        if lines.next().map(|(l, _)| l) != Some(FIRST_LINE) {
            return Err(err(1, "not a vtium-vitrify manifest of version 1"));
        }
        if lines.next().map(|(l, _)| l) != Some(HEADER) {
            return Err(err(2, "the header row is not the manifest's"));
        }
        let mut rows: Vec<ManifestRow> = Vec::new();
        for (line, n) in lines {
            if rows.len() >= usize::try_from(MANIFEST_ROWS_MAX).unwrap_or(usize::MAX) {
                return Err(err(n, "more rows than MANIFEST_ROWS_MAX"));
            }
            let row = parse_row(line).map_err(|why| err(n, why))?;
            if rows.last().is_some_and(|p| p.name >= row.name) {
                return Err(err(n, "rows not sorted by name, or a name twice"));
            }
            rows.push(row);
        }
        Ok(Manifest { rows })
    }

    pub fn rows(&self) -> &[ManifestRow] {
        &self.rows
    }

    /// The row of the set named `name`.
    pub fn row(&self, name: &str) -> Option<&ManifestRow> {
        self.rows
            .binary_search_by(|r| r.name.as_str().cmp(name))
            .ok()
            .and_then(|i| self.rows.get(i))
    }

    /// The manifest's text.
    pub fn to_tsv(&self) -> String {
        let mut s = format!("{FIRST_LINE}\n{HEADER}\n");
        for r in &self.rows {
            let cells: Vec<String> = r
                .cells
                .iter()
                .map(|c| format!("{}x{}", c.width, c.height))
                .collect();
            s += &format!(
                "{}\t{}\t{}\t{}.{}\t{}\t{}\n",
                r.name,
                hex(&r.sha256),
                hex(&r.build_key),
                r.version.0,
                r.version.1,
                cells.join(","),
                r.source
            );
        }
        s
    }
}

fn parse_row(line: &str) -> Result<ManifestRow, &'static str> {
    let fields: Vec<&str> = line.split('\t').collect();
    let &[name, sha256, build_key, version, cells, source] = fields.as_slice() else {
        return Err("a row without six fields");
    };
    if !valid_name(name) {
        return Err("a name that is not a set name");
    }
    let sha256 = parse_hex(sha256)
        .filter(|_| sha256.bytes().all(|b| !b.is_ascii_uppercase()))
        .ok_or("a SHA-256 that is not 64 lower-case hex digits")?;
    let build_key = parse_hex(build_key)
        .filter(|_| build_key.bytes().all(|b| !b.is_ascii_uppercase()))
        .ok_or("a build key that is not 64 lower-case hex digits")?;
    let version = version
        .split_once('.')
        .and_then(|(a, b)| Some((number(a)?, number(b)?)))
        .ok_or("a format version that is not MAJOR.MINOR")?;
    let cells = cells
        .split(',')
        .map(|c| {
            let (w, h) = c.split_once('x')?;
            let cell = Cell {
                width: u32::from(number(w)?),
                height: u32::from(number(h)?),
            };
            (1..=CELL_SIDE_MAX)
                .contains(&cell.width)
                .then_some(cell)
                .filter(|c| (1..=CELL_SIDE_MAX).contains(&c.height))
        })
        .collect::<Option<Vec<Cell>>>()
        .ok_or("cell sizes that are not WxH,WxH")?;
    if source.is_empty() || source.contains('\r') {
        return Err("an empty source");
    }
    Ok(ManifestRow {
        name: name.to_owned(),
        sha256,
        build_key,
        version,
        cells,
        source: source.to_owned(),
    })
}

/// A decimal u16 with no sign, no leading zero and no other character.
fn number(s: &str) -> Option<u16> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) || !s.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    s.parse().ok()
}
