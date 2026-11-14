//! HarfBuzz's shaping tests judge the helper's shaper (vtium/vtium#144).
//! Each case in `tests/harfbuzz/cases.tsv` is a text in a syllabic script,
//! a font, and the glyphs and positions that HarfBuzz's hb-shape gives for
//! the text. The test cuts the text into the clusters a vtium terminal
//! cuts it into, joins the clusters of each syllable as the console joins
//! the cells that hold them (ADR-1203 D1), shapes each cluster or syllable
//! as the helper shapes what the console asks for, and lays them end to
//! end. The result must be
//! hb-shape's glyphs at hb-shape's places. `tests/harfbuzz/differs.tsv`
//! lists the cases where it cannot be, each with the reason.
//! `tests/harfbuzz/README.md` says where the cases come from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use skrifa::MetadataProvider;

use vtium_font::syllable::{Piece, SYLLABLE_SCALARS_MAX, joins};

use super::shape_all;
use crate::shaping::{Ask, kind};

/// One case: its name, its font's file, its text, and hb-shape's output.
struct Case {
    name: String,
    font: String,
    text: Vec<u32>,
    expected: String,
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/harfbuzz")
}

/// The rows of the tab-separated file `name` in `tests/harfbuzz/`, its
/// header left out, each split into `fields` fields.
fn rows(name: &str, fields: usize) -> Vec<Vec<String>> {
    let tsv = std::fs::read_to_string(dir().join(name)).expect("the file");
    tsv.lines()
        .skip(1)
        .map(|line| {
            let f: Vec<String> = line.split('\t').map(str::to_owned).collect();
            assert_eq!(f.len(), fields, "{fields} fields in {name}: {line}");
            f
        })
        .collect()
}

fn cases() -> Vec<Case> {
    rows("cases.tsv", 4)
        .into_iter()
        .map(|f| {
            let text = f[2]
                .split(',')
                .map(|u| {
                    let hex = u.strip_prefix("U+").expect("U+ and hex digits");
                    u32::from_str_radix(hex, 16).expect("hex digits")
                })
                .collect();
            Case {
                name: f[0].clone(),
                font: f[1].clone(),
                text,
                expected: f[3].clone(),
            }
        })
        .collect()
}

/// The clusters a vtium terminal cuts `text` into, which the console asks
/// the helper for one at a time: the text written on a terminal's first
/// row, read back one cell at a time.
fn clusters(text: &[u32]) -> Vec<Vec<u32>> {
    let s: String = text.iter().filter_map(|&c| char::from_u32(c)).collect();
    let mut t = vtium_term::Terminal::new(80, 2, 0);
    let _ = t.input(s.as_bytes());
    let screen = t.screen();
    let line = screen.line(0).expect("a row");
    line.cells()
        .iter()
        .filter_map(|&c| match line.glyph(c) {
            vtium_term::Glyph::Char(ch) => Some(vec![u32::from(ch)]),
            vtium_term::Glyph::Cluster(s) => Some(s.chars().map(u32::from).collect()),
            vtium_term::Glyph::Empty => None,
        })
        .collect()
}

/// Each glyph of a shaped text, as its glyph id and its place from the
/// text's origin in font units, x and y, and the text's whole advance.
type Drawn = (Vec<(u16, i32, i32)>, i32);

/// hb-shape's output, `[name=cluster@x,y+advance|...]`, as a [`Drawn`]:
/// each glyph's place is its offset added to the advances before it. A
/// glyph is named by the font's `post` table, or as `gidN`.
fn expected(out: &str, names: &BTreeMap<String, u16>) -> Drawn {
    let body = out
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .expect("hb-shape's brackets");
    let mut pen = 0;
    let mut glyphs = Vec::new();
    for glyph in body.split('|') {
        let (name, rest) = glyph.split_once('=').expect("a name and a cluster");
        let (at, advance) = rest.split_once('+').expect("an advance");
        let (dx, dy) = match at.split_once('@') {
            Some((_, offsets)) => {
                let (x, y) = offsets.split_once(',').expect("two offsets");
                (x.parse().expect("an x"), y.parse().expect("a y"))
            }
            None => (0, 0),
        };
        let id = names.get(name).copied().unwrap_or_else(|| {
            name.strip_prefix("gid")
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("no glyph named {name}"))
        });
        glyphs.push((id, pen + dx, dy));
        let advance: i32 = advance
            .split(',')
            .next()
            .and_then(|a| a.parse().ok())
            .expect("an advance");
        pen += advance;
    }
    (glyphs, pen)
}

/// The glyph ids of the font in `data`, by their names.
fn names(data: &[u8]) -> BTreeMap<String, u16> {
    let font = skrifa::FontRef::new(data).expect("a font");
    font.glyph_names()
        .iter()
        .filter_map(|(g, n)| Some((n.as_str().to_owned(), u16::try_from(g.to_u32()).ok()?)))
        .collect()
}

/// `clusters` joined into syllables as the console joins the cells that
/// hold them (ADR-1203 D1): each run of clusters that link, whose text
/// holds at most [`SYLLABLE_SCALARS_MAX`] scalar values, as the key of its
/// first piece, which the helper shapes from the syllable's whole text;
/// each other cluster as itself.
fn syllables(clusters: &[Vec<u32>]) -> Vec<Vec<u32>> {
    let chars = |k: &[u32]| -> Vec<char> { k.iter().filter_map(|&c| char::from_u32(c)).collect() };
    let mut runs: Vec<Vec<&Vec<u32>>> = Vec::new();
    for key in clusters {
        match runs.last_mut() {
            Some(run) if run.last().is_some_and(|l| joins(&chars(l), &chars(key))) => {
                run.push(key);
            }
            _ => runs.push(vec![key]),
        }
    }
    let width = |k: &[u32]| vtium_term::cluster_width(&chars(k)).max(1);
    let mut out = Vec::new();
    for run in runs {
        let text: Vec<u32> = run.iter().flat_map(|k| k.iter().copied()).collect();
        if run.len() < 2 || text.len() > SYLLABLE_SCALARS_MAX {
            out.extend(run.into_iter().cloned());
            continue;
        }
        let total: u8 = run.iter().map(|k| width(k)).sum();
        let piece = Piece::new(0, width(run[0]), total).expect("a piece");
        let mut key = text;
        key.push(u32::from(piece.mark()));
        out.push(key);
    }
    out
}

/// `clusters` shaped by the font in `data` as the helper shapes each
/// cluster or syllable the console asks for ([`syllables`]), and laid end
/// to end; `None` when the helper leaves one of them out, as it leaves out
/// a cluster the font cannot draw whole.
fn drawn(data: &[u8], clusters: &[Vec<u32>]) -> Option<Drawn> {
    let font = swash::FontRef::from_index(data, 0).expect("a font");
    let mut pen = 0;
    let mut glyphs = Vec::new();
    for key in &syllables(clusters) {
        let ask = Ask {
            key: key.clone(),
            kind: kind(key),
        };
        let shaped = shape_all(data, 0, font, &[ask]);
        let s = shaped.iter().find(|s| &s.key == key)?;
        glyphs.extend(s.glyphs.iter().map(|a| (a.g, pen + a.x, a.y)));
        pen += s.advance;
    }
    Some((glyphs, pen))
}

/// A case the helper cannot draw as hb-shape does, by why.
#[derive(Debug, PartialEq, Eq)]
enum Differs {
    /// hb-shape draws the font's .notdef glyph for a character the font
    /// lacks, and the helper leaves the cluster out, so that a later
    /// source or the console draws it.
    LeftOut,
    /// The text is only marks, and the terminal stores no mark that has
    /// no character before it on its row, so the console never asks for
    /// the text.
    NeverAsked,
    /// The terminal cuts the text into several clusters, and the font
    /// shapes or positions its glyphs across them, which the helper,
    /// shaping one cluster at a time, cannot do.
    Cut,
}

fn differs() -> BTreeMap<String, Differs> {
    rows("differs.tsv", 3)
        .into_iter()
        .map(|f| {
            let why = match f[1].as_str() {
                "left-out" => Differs::LeftOut,
                "never-asked" => Differs::NeverAsked,
                "cut" => Differs::Cut,
                other => panic!("no such reason: {other}"),
            };
            (f[0].clone(), why)
        })
        .collect()
}

// Every case that `differs.tsv` does not list draws as hb-shape draws it:
// the same glyphs, at the same places, with the same advance. Each listed
// case differs for the reason it gives: a case left out is one cluster
// that hb-shape draws with .notdef, a case never asked for is no cluster
// at all, and a case cut is several clusters
#[test]
fn harfbuzz_judges_the_clusters_the_console_asks_for() {
    let cases = cases();
    assert_eq!(cases.len(), 180, "the cases the import copied");
    let listed = differs();
    let mut fonts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut single = 0;
    let mut wrong = Vec::new();
    for case in &cases {
        let data = fonts.entry(case.font.clone()).or_insert_with(|| {
            std::fs::read(dir().join("fonts").join(&case.font)).expect("the font")
        });
        let parts = clusters(&case.text);
        single += usize::from(parts.len() == 1);
        let want = expected(&case.expected, &names(data));
        let got = drawn(data, &parts);
        let why = match listed.get(&case.name) {
            None if got.as_ref() == Some(&want) => continue,
            None => "differs from hb-shape",
            Some(Differs::LeftOut)
                if parts.len() == 1 && got.is_none() && want.0.iter().any(|g| g.0 == 0) =>
            {
                continue;
            }
            Some(Differs::LeftOut) => "is listed as left out",
            Some(Differs::NeverAsked) if parts.is_empty() => continue,
            Some(Differs::NeverAsked) => "is listed as never asked for",
            Some(Differs::Cut) if parts.len() > 1 && got.as_ref() != Some(&want) => continue,
            Some(Differs::Cut) => "is listed as cut",
        };
        wrong.push(format!(
            "{} {why}: clusters {:x?}\n  hb-shape {want:?}\n  helper   {got:?}",
            case.name, parts
        ));
    }
    assert_eq!(single, 106, "the cases that are one cluster");
    assert!(
        listed
            .keys()
            .all(|name| cases.iter().any(|c| &c.name == name)),
        "every listed case is a case"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
