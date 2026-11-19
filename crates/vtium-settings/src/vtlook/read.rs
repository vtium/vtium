//! The vtlook reader: a vtlook's file read under the ownership check, its
//! lines read by vtium.conf's syntax, and each value checked against the
//! filters' declarations and the display requirements.
//!
//! A vtlook starts with `version = 1`. Then come its sections: at most one
//! `[requires]`, and one `[LAYER FILTER]` for each filter, whose lines set
//! the filter's parameters. A parameter the section leaves out takes its
//! declared default. The reader reads every line whatever comes before
//! it, so a check names every error the file holds. Any error at all
//! makes the whole vtlook fail, and the head draws the built-in vtlook.
//!
//! References:
//! - ADR-1304 D4: the file's place, its ownership rule, its syntax and its bounds.
//! - ADR-1304 D6: the reading and checking stages, and that any error fails the whole vtlook.
//! - ADR-1304 D7: the `[requires]` section.

use std::path::{Path, PathBuf};

use vtium_render::filter::{self, Declaration, Fixed, Given, Kind, Layer};

use super::report::{Failure, Part, Report, Stage, keep_first, quoted};
use super::requires::{Requires, list};
use super::{DEFAULT, LOOKS, SUFFIX, named};
use crate::bounds::{ERRORS_MAX, VTLOOK_FILE_MAX, VTLOOK_SECTIONS_MAX};
use crate::conf::comment;
use crate::error::Problem;
use crate::file::{Trust, read_limited};
use crate::parse::good_name;
use crate::split::BOM;

/// A parameter's value, of its kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Number(Fixed),
    Integer(i64),
    Color([u8; 3]),
    /// The word, one of the parameter's choices.
    Choice(&'static str),
    Switch(bool),
    /// A file's name in the looks folder.
    File(String),
}

/// One filter a vtlook chooses, with every parameter's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    pub declaration: &'static Declaration,
    /// The line of its section heading.
    pub line: usize,
    /// Each parameter in its declaration's order, with the value the file
    /// sets or the declared default.
    pub values: Vec<(&'static str, Value)>,
}

/// A vtlook that reads cleanly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vtlook {
    pub name: String,
    pub file: PathBuf,
    pub requires: Requires,
    /// The spatial filters, in the order of their sections.
    pub spatial: Vec<Filter>,
    /// The color filters, in the order of their sections.
    pub color: Vec<Filter>,
}

impl Vtlook {
    /// Whether it holds a filter. One that holds none changes no pixel,
    /// and vtium draws it as it draws the built-in vtlook.
    pub fn holds_filters(&self) -> bool {
        !self.spatial.is_empty() || !self.color.is_empty()
    }
}

/// Reads the vtlook `name` from the folder `dir` under the ownership check
/// `trust`, as vtium reads it at start. `Err` holds the first errors in
/// the file's order and counts the rest. The built-in vtlook's name is an
/// error here, since no file may take it.
pub fn read(name: &str, dir: &Path, trust: &Trust) -> Result<Vtlook, Failure> {
    let file = dir.join(format!("{name}{SUFFIX}"));
    let fail = |file: &Path, message: String| {
        Err(Failure::one(Report {
            vtlook: quoted(name),
            file: file.to_path_buf(),
            line: None,
            part: None,
            stage: Stage::Reading,
            message,
        }))
    };
    if name == DEFAULT {
        return fail(
            &file,
            format!(
                "vtium never reads a file named {DEFAULT}{SUFFIX}, because {DEFAULT} is the name of the built-in vtlook. Give the file another name."
            ),
        );
    }
    if !named(name) {
        // the name is no file's, so the report names the folder
        return fail(
            dir,
            format!(
                "The name \"{}\" is not a vtlook's name. A vtlook's name is 1 to 64 of the characters A-Z a-z 0-9 . _ + -, with no path.",
                quoted(name)
            ),
        );
    }
    match read_limited(&file, trust, VTLOOK_FILE_MAX) {
        Ok(Some(bytes)) => read_text(name, &file, &bytes),
        Ok(None) => fail(
            &file,
            format!("There is no file {name}{SUFFIX} in {}.", dir.display()),
        ),
        Err(rejected) => {
            let message = match rejected.errors.first().map(|e| &e.problem) {
                Some(Problem::Untrusted(why)) => {
                    format!("vtium does not read the file, because {}", why.reason())
                }
                Some(Problem::TooLarge) => format!(
                    "The file is larger than {VTLOOK_FILE_MAX} bytes, the most a vtlook may hold."
                ),
                Some(Problem::NotAFile) => "The vtlook is not a regular file.".to_owned(),
                Some(Problem::Unreadable(kind)) => format!("vtium cannot read the file: {kind}."),
                _ => "vtium cannot read the file.".to_owned(),
            };
            fail(&file, message)
        }
    }
}

/// Reads the vtlook `name` from the first of `folders` that holds a file
/// of that name, under the ownership check `trust`, as [`read`] reads it
/// from one folder. vtium passes [`folders`](super::folders), the
/// administrator's folder and then the package's, so an administrator's
/// vtlook replaces a preset of the same name. The first folder that holds
/// the file is the one vtium reads, even when that file fails a check: a
/// broken copy of a preset is an error to fix, and vtium does not quietly
/// draw the preset instead. `Err` names every folder when none holds the
/// file.
///
/// References:
/// - ADR-1304 D4, its note of 2026-11-19: the administrator's vtlook wins over a preset of the same name.
/// - ADR-1304 D15: the rule it follows, a local shader hiding a packaged one until it is removed.
pub fn find(name: &str, folders: &[&Path], trust: &Trust) -> Result<Vtlook, Failure> {
    let first = folders.first().copied().unwrap_or(Path::new(LOOKS));
    if name == DEFAULT || !named(name) {
        // the name is no file's in any folder, and read says why
        return read(name, first, trust);
    }
    let file = format!("{name}{SUFFIX}");
    let held = folders
        .iter()
        .find(|dir| match dir.join(&file).symlink_metadata() {
            Ok(_) => true,
            Err(e) => !matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ),
        });
    if let Some(dir) = held {
        return read(name, dir, trust);
    }
    let names: Vec<String> = folders.iter().map(|d| d.display().to_string()).collect();
    let within = match names.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => first.display().to_string(),
    };
    Err(Failure::one(Report {
        vtlook: quoted(name),
        file: first.join(&file),
        line: None,
        part: None,
        stage: Stage::Reading,
        message: format!("There is no file {file} in {within}."),
    }))
}

/// Reads a vtlook's bytes, `bytes`, as the file `file` of the vtlook
/// `name`, against vtium's own filters.
pub fn read_text(name: &str, file: &Path, bytes: &[u8]) -> Result<Vtlook, Failure> {
    read_text_with(name, file, bytes, filter::COLOR)
}

/// What a section heading names.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Heading {
    Requires,
    Filter(Layer, String),
    /// A heading the reader does not know, whose lines it skips.
    Unknown,
}

/// One section as the file gives it: its heading, its line and its
/// `key = value` lines.
struct Section {
    heading: Heading,
    line: usize,
    lines: Vec<(usize, String, String)>,
}

/// The reader's state while it reads one vtlook.
struct Reader<'a> {
    name: &'a str,
    file: &'a Path,
    reports: Vec<Report>,
    /// The errors dropped, past the first [`ERRORS_MAX`] by line.
    more: usize,
}

impl Reader<'_> {
    fn error(&mut self, line: Option<usize>, part: Option<Part>, stage: Stage, message: String) {
        self.reports.push(Report {
            vtlook: self.name.to_owned(),
            file: self.file.to_path_buf(),
            line,
            part,
            stage,
            message,
        });
        // The reader finds errors out of line order, since it checks the
        // sections after it reads every line, so it keeps twice the bound
        // and cuts back to the first by line. An error cut here lies after
        // ERRORS_MAX that are kept, so it would never be among the first.
        if self.reports.len() >= ERRORS_MAX.saturating_mul(2) {
            self.more = self.more.saturating_add(keep_first(&mut self.reports));
        }
    }
}

/// Reads a vtlook's bytes against the color filters `color`. The tests
/// pass declarations of their own, since vtium's list is empty until the
/// color layer arrives.
pub fn read_text_with(
    name: &str,
    file: &Path,
    bytes: &[u8],
    color: &[&'static Declaration],
) -> Result<Vtlook, Failure> {
    let mut r = Reader {
        name,
        file,
        reports: Vec::new(),
        more: 0,
    };
    let sections = lines(&mut r, bytes);
    let mut look = Vtlook {
        name: name.to_owned(),
        file: file.to_path_buf(),
        requires: Requires::default(),
        spatial: Vec::new(),
        color: Vec::new(),
    };
    check(&mut r, &sections, color, &mut look);
    if r.reports.is_empty() {
        Ok(look)
    } else {
        Err(Failure::of(r.reports, r.more))
    }
}

/// The file's lines, read into its sections, with every reading error.
fn lines(r: &mut Reader, bytes: &[u8]) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut version = false;
    // the line of the top's `version` line, once read
    let mut version_line = None;
    let mut too_many = false;
    let mut n = 0_usize;
    // a byte-order mark is not part of the first line, as in vtium.conf
    let bytes = bytes.strip_prefix(BOM).unwrap_or(bytes);
    for raw in bytes.split(|&b| b == b'\n') {
        n = n.saturating_add(1);
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        let (end, escapes) = comment(raw);
        let kept: Vec<u8> = raw
            .iter()
            .take(end)
            .enumerate()
            .filter(|(i, _)| !escapes.contains(i))
            .map(|(_, &b)| b)
            .collect();
        let Ok(text) = std::str::from_utf8(&kept) else {
            r.error(
                Some(n),
                None,
                Stage::Reading,
                "This line holds bytes that are not UTF-8 text.".to_owned(),
            );
            continue;
        };
        if text.chars().any(|c| c.is_control() && c != '\t') {
            r.error(
                Some(n),
                None,
                Stage::Reading,
                "This line holds a control character, and a vtlook allows none but tab.".to_owned(),
            );
            continue;
        }
        let line = text.trim_matches([' ', '\t']);
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if !version {
                r.error(
                    Some(n),
                    None,
                    Stage::Reading,
                    format!(
                        "found {} before version = 1, needs version = 1 first",
                        quoted(line)
                    ),
                );
                version = true;
            }
            if sections.len() >= VTLOOK_SECTIONS_MAX {
                if !too_many {
                    r.error(
                        Some(n),
                        None,
                        Stage::Reading,
                        format!("The file has more than {VTLOOK_SECTIONS_MAX} sections, the most a vtlook may have."),
                    );
                    too_many = true;
                }
                continue;
            }
            let heading = heading(r, n, line);
            sections.push(Section {
                heading,
                line: n,
                lines: Vec::new(),
            });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            r.error(
                Some(n),
                None,
                Stage::Reading,
                format!(
                    "found {}, needs a section heading or key = value",
                    quoted(line)
                ),
            );
            continue;
        };
        let (key, value) = (
            key.trim_matches([' ', '\t']),
            value.trim_matches([' ', '\t']),
        );
        if too_many {
            continue;
        }
        match sections.last_mut() {
            Some(section) => section.lines.push((n, key.to_owned(), value.to_owned())),
            None if key == "version" => match version_line {
                Some(first) => r.error(
                    Some(n),
                    None,
                    Stage::Reading,
                    format!("version is set twice, needs it once; the first is on line {first}"),
                ),
                None => {
                    version = true;
                    version_line = Some(n);
                    if value != "1" {
                        r.error(
                            Some(n),
                            None,
                            Stage::Reading,
                            format!("version = {}, needs version = 1", quoted(value)),
                        );
                    }
                }
            },
            None => r.error(
                Some(n),
                None,
                Stage::Reading,
                format!(
                    "found {} before the first section, needs version = 1 alone there",
                    quoted(key)
                ),
            ),
        }
    }
    if !version {
        r.error(
            None,
            None,
            Stage::Reading,
            "The file has no version = 1 line, which a vtlook needs before its first section."
                .to_owned(),
        );
    }
    sections
}

/// What a section heading names; an unknown one is a checking error.
fn heading(r: &mut Reader, n: usize, line: &str) -> Heading {
    let words: Vec<&str> = line
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .map(|inner| inner.split([' ', '\t']).filter(|w| !w.is_empty()).collect())
        .unwrap_or_default();
    match words.as_slice() {
        ["requires"] => Heading::Requires,
        [layer, name] if Layer::of_word(layer).is_some() => Heading::Filter(
            Layer::of_word(layer).unwrap_or(Layer::Color),
            (*name).to_owned(),
        ),
        _ => {
            r.error(
                Some(n),
                None,
                Stage::Checking,
                format!(
                    "found {}, needs [requires], [color FILTER] or [spatial FILTER]",
                    quoted(line)
                ),
            );
            Heading::Unknown
        }
    }
}

/// Checks the sections against the declarations `color` and the display
/// requirements, filling `look` with what reads cleanly.
fn check(r: &mut Reader, sections: &[Section], color: &[&'static Declaration], look: &mut Vtlook) {
    let mut requires_line = None;
    for s in sections {
        match &s.heading {
            Heading::Requires => match requires_line {
                Some(first) => r.error(
                    Some(s.line),
                    Some(Part::Requires),
                    Stage::Checking,
                    format!(
                        "found a second [requires], needs one at most; the first is on line {first}"
                    ),
                ),
                None => {
                    requires_line = Some(s.line);
                    look.requires = requires(r, s);
                }
            },
            Heading::Filter(layer, name) => {
                let in_layer = |h: &Heading| matches!(h, Heading::Filter(l, _) if l == layer);
                let of = sections.iter().filter(|o| in_layer(&o.heading)).count();
                let place = sections
                    .iter()
                    .take_while(|o| o.line != s.line)
                    .filter(|o| in_layer(&o.heading))
                    .count()
                    .saturating_add(1);
                let part = Part::Filter {
                    layer: *layer,
                    name: quoted(name),
                    place,
                    of,
                };
                let first = sections
                    .iter()
                    .find(
                        |o| matches!(&o.heading, Heading::Filter(l, m) if l == layer && m == name),
                    )
                    .map_or(s.line, |o| o.line);
                if first != s.line {
                    r.error(
                        Some(s.line),
                        Some(part),
                        Stage::Checking,
                        format!("found {} a second time, needs each filter once; the first is on line {first}", quoted(name)),
                    );
                    continue;
                }
                if let Some(f) = filter_section(r, s, *layer, name, part, color) {
                    match layer {
                        Layer::Color => look.color.push(f),
                        Layer::Spatial => look.spatial.push(f),
                    }
                }
            }
            Heading::Unknown => {}
        }
    }
}

/// The `[requires]` section `s`, each key checked once and each pair in
/// order.
fn requires(r: &mut Reader, s: &Section) -> Requires {
    let mut out = Requires::default();
    let mut written: Vec<(&str, &str, usize)> = Vec::new();
    for (n, key, value) in &s.lines {
        if let Some((_, _, first)) = written.iter().find(|(k, _, _)| k == key) {
            let message =
                format!("{key} is set twice, needs it once; the first is on line {first}");
            r.error(Some(*n), Some(Part::Requires), Stage::Checking, message);
            continue;
        }
        match out.set(key, value) {
            Ok(()) => written.push((key, value, *n)),
            Err(message) => r.error(Some(*n), Some(Part::Requires), Stage::Checking, message),
        }
    }
    let values: Vec<(&str, &str)> = written.iter().map(|(k, v, _)| (*k, *v)).collect();
    for (min, message) in out.out_of_order(&values) {
        let line = written
            .iter()
            .find(|(k, _, _)| *k == min)
            .map(|(_, _, n)| *n);
        r.error(line, Some(Part::Requires), Stage::Checking, message);
    }
    out
}

/// One filter's section, `None` when it holds an error.
fn filter_section(
    r: &mut Reader,
    s: &Section,
    layer: Layer,
    name: &str,
    part: Part,
    color: &[&'static Declaration],
) -> Option<Filter> {
    let declaration = match layer {
        Layer::Color => color.iter().copied().find(|d| d.name == name),
        // the spatial filters' shader files arrive with the spatial layer
        Layer::Spatial => None,
    };
    let Some(declaration) = declaration else {
        let names: Vec<&str> = match layer {
            Layer::Color => color.iter().map(|d| d.name).collect(),
            Layer::Spatial => Vec::new(),
        };
        let message = if names.is_empty() {
            format!(
                "found {}, but this build of vtium has no {} filters",
                quoted(name),
                layer.word()
            )
        } else {
            format!("found {}, needs {}", quoted(name), list(&names))
        };
        r.error(Some(s.line), Some(part), Stage::Checking, message);
        return None;
    };
    let before = r.reports.len();
    let mut set: Vec<(&'static str, Value, usize)> = Vec::new();
    for (n, key, value) in &s.lines {
        let Some(param) = declaration.param(key) else {
            let names: Vec<&str> = declaration.params.iter().map(|p| p.name).collect();
            let needs = if names.is_empty() {
                format!("no parameter, since {name} has none")
            } else {
                format!("one of {}", list(&names))
            };
            r.error(
                Some(*n),
                Some(part.clone()),
                Stage::Checking,
                format!("found {}, needs {needs}", quoted(key)),
            );
            continue;
        };
        if let Some((_, _, first)) = set.iter().find(|(k, _, _)| *k == param.name) {
            let message = format!(
                "{} is set twice, needs it once; the first is on line {first}",
                quoted(key)
            );
            r.error(Some(*n), Some(part.clone()), Stage::Checking, message);
            continue;
        }
        match parse_value(param.kind, value) {
            Some(v) => set.push((param.name, v, *n)),
            None => r.error(
                Some(*n),
                Some(part.clone()),
                Stage::Checking,
                format!(
                    "{} = {}, needs {}",
                    quoted(key),
                    quoted(value),
                    needs(param.kind)
                ),
            ),
        }
    }
    let mut values = Vec::new();
    for p in declaration.params {
        let value = set
            .iter()
            .find(|(k, _, _)| *k == p.name)
            .map(|(_, v, _)| v.clone())
            .or_else(|| p.default.map(given));
        match value {
            Some(v) => values.push((p.name, v)),
            None => r.error(
                Some(s.line),
                Some(part.clone()),
                Stage::Checking,
                format!(
                    "{} is unset, needs {}, since it has no default",
                    p.name,
                    needs(p.kind)
                ),
            ),
        }
    }
    (r.reports.len() == before).then_some(Filter {
        declaration,
        line: s.line,
        values,
    })
}

/// A declared default as a value.
fn given(g: Given) -> Value {
    match g {
        Given::Number(n) => Value::Number(n),
        Given::Integer(n) => Value::Integer(n),
        Given::Color(c) => Value::Color(c),
        Given::Choice(w) => Value::Choice(w),
        Given::Switch(b) => Value::Switch(b),
    }
}

/// What a parameter of kind `kind` takes, as an error says it.
pub(super) fn needs(kind: Kind) -> String {
    match kind {
        Kind::Number { min, max } => format!("a number from {min} to {max}"),
        Kind::Integer { min, max } => format!("an integer from {min} to {max}"),
        Kind::Color => "a color r,g,b, each 0 to 255".to_owned(),
        Kind::Choice(words) => format!("one of {}", list(words)),
        Kind::Switch => "yes or no".to_owned(),
        Kind::File(suffixes) => format!(
            "the name of a {} file in {LOOKS}, with no path",
            list(suffixes)
        ),
    }
}

/// `value` read as a value of kind `kind`, `None` when it is not one or is
/// out of range.
fn parse_value(kind: Kind, value: &str) -> Option<Value> {
    match kind {
        Kind::Number { min, max } => number(value)
            .filter(|n| (min..=max).contains(n))
            .map(Value::Number),
        Kind::Integer { min, max } => integer(value)
            .filter(|n| (min..=max).contains(n))
            .map(Value::Integer),
        Kind::Color => {
            let mut channels = value.split(',').map(|c| {
                let c = c.trim_matches([' ', '\t']);
                c.bytes()
                    .all(|b| b.is_ascii_digit())
                    .then(|| c.parse::<u8>().ok())
                    .flatten()
            });
            let rgb = [channels.next()??, channels.next()??, channels.next()??];
            channels.next().is_none().then_some(Value::Color(rgb))
        }
        Kind::Choice(words) => words
            .iter()
            .find(|w| **w == value)
            .map(|w| Value::Choice(w)),
        Kind::Switch => match value {
            "yes" => Some(Value::Switch(true)),
            "no" => Some(Value::Switch(false)),
            _ => None,
        },
        Kind::File(suffixes) => {
            let named = good_name(value)
                && suffixes
                    .iter()
                    .any(|s| value.len() > s.len() && value.ends_with(s));
            named.then(|| Value::File(value.to_owned()))
        }
    }
}

/// A whole number with an optional sign.
fn integer(value: &str) -> Option<i64> {
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// A decimal with an optional sign and at most four digits after the
/// point, in ten-thousandths.
fn number(value: &str) -> Option<Fixed> {
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty()
        || !digits(whole)
        || !digits(fraction)
        || fraction.len() > 4
        || (unsigned.contains('.') && fraction.is_empty())
    {
        return None;
    }
    let whole: i32 = whole.parse().ok()?;
    let padded = format!("{fraction:0<4}");
    let fraction: i32 = padded.parse().ok()?;
    let n = whole.checked_mul(Fixed::ONE)?.checked_add(fraction)?;
    Some(Fixed(if negative { n.checked_neg()? } else { n }))
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use vtium_render::filter::Param;

    const GAMMA: Declaration = Declaration {
        name: "gamma",
        layer: Layer::Color,
        help: "Raises each channel to the power 1 / gamma.",
        params: &[Param {
            name: "gamma",
            kind: Kind::Number {
                min: Fixed::of(0, 5000),
                max: Fixed::of(2, 0),
            },
            default: Some(Given::Number(Fixed::of(1, 0))),
            help: "above 1 lightens the middle tones",
        }],
    };
    const TINT: Declaration = Declaration {
        name: "tint",
        layer: Layer::Color,
        help: "Turns the screen toward one color.",
        params: &[
            Param {
                name: "color",
                kind: Kind::Color,
                default: Some(Given::Color([255, 176, 0])),
                help: "the phosphor's color",
            },
            Param {
                name: "steps",
                kind: Kind::Integer { min: 2, max: 8 },
                default: Some(Given::Integer(3)),
                help: "how many",
            },
            Param {
                name: "kind",
                kind: Kind::Choice(&["warm", "cold"]),
                default: Some(Given::Choice("warm")),
                help: "which",
            },
            Param {
                name: "on",
                kind: Kind::Switch,
                default: Some(Given::Switch(true)),
                help: "whether",
            },
        ],
    };
    const CUBE: Declaration = Declaration {
        name: "cube",
        layer: Layer::Color,
        help: "Maps each color through a .cube table.",
        params: &[Param {
            name: "file",
            kind: Kind::File(&[".cube"]),
            default: None,
            help: "a NAME.cube in the looks folder",
        }],
    };
    const FILTERS: &[&Declaration] = &[&GAMMA, &TINT, &CUBE];

    fn file(name: &str) -> PathBuf {
        PathBuf::from(format!("/etc/vtium/looks/{name}.vtlook"))
    }

    fn errors(name: &str, text: &str) -> Vec<String> {
        match read_text_with(name, &file(name), text.as_bytes(), FILTERS) {
            Ok(_) => Vec::new(),
            Err(failure) => failure
                .reports
                .iter()
                .map(|r| format!("{}: {r}", r.at()))
                .collect(),
        }
    }

    // The smallest vtlook reads cleanly, and a parameter it leaves out
    // takes its default.
    #[test]
    fn a_vtlook_sets_its_filters_parameters_and_the_rest_take_defaults() {
        let text = "# warm.vtlook\nversion = 1\n\n[color tint]\ncolor = 1,2,3\n\n[color gamma]\ngamma = 1.2 # a comment\n";
        let look = read_text_with("warm", &file("warm"), text.as_bytes(), FILTERS);
        let Ok(look) = look else {
            panic!("{look:?}");
        };
        assert_eq!(look.requires, Requires::default());
        let names: Vec<&str> = look.color.iter().map(|f| f.declaration.name).collect();
        assert_eq!(names, ["tint", "gamma"]);
        assert_eq!(
            look.color.first().map(|f| f.values.clone()),
            Some(vec![
                ("color", Value::Color([1, 2, 3])),
                ("steps", Value::Integer(3)),
                ("kind", Value::Choice("warm")),
                ("on", Value::Switch(true)),
            ])
        );
        assert_eq!(
            look.color.get(1).map(|f| f.values.clone()),
            Some(vec![("gamma", Value::Number(Fixed::of(1, 2000)))])
        );
        assert!(look.holds_filters());
        // with no section at all, it holds nothing
        let empty = read_text_with("plain", &file("plain"), b"version = 1\n", FILTERS);
        assert!(empty.is_ok_and(|l| !l.holds_filters()));
    }

    // Each value outside its kind or range is an error that names the
    // filter and its place, the parameter, the value and what it needs.
    #[test]
    fn a_value_out_of_its_kind_or_range_names_what_it_needs() {
        let f = "/etc/vtium/looks/bad.vtlook";
        let text = "version = 1\n[color gamma]\ngamma = 2.5\n[color tint]\ncolor = 255,176\nsteps = 3.0\nkind = hot\non = 1\n[color cube]\n";
        assert_eq!(
            errors("bad", text),
            [
                format!(
                    "{f}:3: color filter 1 of 3, gamma: checking: gamma = 2.5, needs a number from 0.5 to 2"
                ),
                format!(
                    "{f}:5: color filter 2 of 3, tint: checking: color = 255,176, needs a color r,g,b, each 0 to 255"
                ),
                format!(
                    "{f}:6: color filter 2 of 3, tint: checking: steps = 3.0, needs an integer from 2 to 8"
                ),
                format!(
                    "{f}:7: color filter 2 of 3, tint: checking: kind = hot, needs one of warm or cold"
                ),
                format!("{f}:8: color filter 2 of 3, tint: checking: on = 1, needs yes or no"),
                format!(
                    "{f}:9: color filter 3 of 3, cube: checking: file is unset, needs the name of a .cube file in /etc/vtium/looks, with no path, since it has no default"
                ),
            ]
        );
    }

    // One wrong filter fails the whole vtlook: the reader never keeps the
    // filters that read cleanly and drops only the one at fault.
    #[test]
    fn one_wrong_filter_fails_the_whole_vtlook() {
        let text = "version = 1\n[color tint]\n[color gamma]\ngamma = 9\n";
        let read = read_text_with("half", &file("half"), text.as_bytes(), FILTERS);
        assert_eq!(read.map_err(|e| e.reports.len()), Err(1));
    }

    #[test]
    fn a_number_takes_a_sign_and_at_most_four_digits_after_the_point() {
        for (text, n) in [
            ("1", Some(10_000)),
            ("1.15", Some(11_500)),
            ("-0.5", Some(-5000)),
            ("+0.0001", Some(1)),
            ("0.00001", None),
            (".5", None),
            ("1.", None),
            ("1e3", None),
            ("", None),
            ("99999999", None),
        ] {
            assert_eq!(number(text), n.map(Fixed), "{text}");
        }
        assert_eq!(parse_value(Kind::File(&[".cube"]), "a/panel.cube"), None);
        assert_eq!(parse_value(Kind::File(&[".cube"]), ".cube"), None);
        assert_eq!(
            parse_value(Kind::File(&[".cube"]), "panel.cube"),
            Some(Value::File("panel.cube".to_owned()))
        );
    }

    // ADR-1304 D14's second malformed example: a missing version and a
    // filter vtium does not have are two errors, both reported.
    #[test]
    fn a_missing_version_and_an_unknown_filter_are_two_errors() {
        let f = "/etc/vtium/looks/old.vtlook";
        assert_eq!(
            errors(
                "old",
                "# /etc/vtium/looks/old.vtlook\n[color sepia]\namount = 0.5\n"
            ),
            [
                format!(
                    "{f}:2: reading: found [color sepia] before version = 1, needs version = 1 first"
                ),
                format!(
                    "{f}:2: color filter 1 of 1, sepia: checking: found sepia, needs gamma, tint or cube"
                ),
            ]
        );
        // vtium's own list holds the color filters and no spatial filter,
        // so gamma reads, sepia is unknown and scanlines has no list at all
        let own = read_text(
            "old",
            Path::new(f),
            b"version = 1\n[color gamma]\n[color sepia]\n[spatial scanlines]\n",
        );
        let said: Vec<String> = own
            .err()
            .map(|e| e.reports)
            .unwrap_or_default()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            said,
            [
                "color filter 2 of 2, sepia: checking: found sepia, needs gamma, contrast, tint or color-blind",
                "spatial filter 1 of 1, scanlines: checking: found scanlines, but this build of vtium has no spatial filters",
            ]
        );
    }

    // ADR-1304 D14's third malformed example, and the other errors of a
    // `[requires]` section.
    #[test]
    fn a_malformed_requires_section_names_requires() {
        let f = "/etc/vtium/looks/big.vtlook";
        let text = "# /etc/vtium/looks/big.vtlook\nversion = 1\n\n[requires]\nmin-size = 1920x1080\nmax-size = 1280x800\n\n[color gamma]\ngamma = 1.2\n";
        assert_eq!(
            errors("big", text),
            [format!(
                "{f}:5: requires: checking: min-size = 1920x1080 exceeds max-size = 1280x800"
            )]
        );
        let text = "version = 1\n[requires]\nmin-depth = 12\ngpu = maybe\ngpu = yes\n[requires]\n";
        assert_eq!(
            errors("big", text),
            [
                format!("{f}:3: requires: checking: min-depth = 12, needs any, 1, 8, 16 or 24"),
                format!("{f}:4: requires: checking: gpu = maybe, needs any, yes or no"),
                format!(
                    "{f}:6: requires: checking: found a second [requires], needs one at most; the first is on line 2"
                ),
            ]
        );
        // every key at any reads as a vtlook with no section
        let every = "version = 1\n[requires]\nmin-depth = any\nmax-depth = any\nmin-size = any\nmax-size = any\nmin-dpi = any\nmax-dpi = any\nmin-fps = any\ngpu = any\n";
        let read = read_text_with("any", &file("any"), every.as_bytes(), FILTERS);
        assert!(read.is_ok_and(|l| l.requires == Requires::default()));
    }

    // The errors of the file's structure: a line that is neither a heading
    // nor a key, a wrong version, a key above the sections, an unknown
    // heading, a filter or a parameter named twice, and an unknown
    // parameter.
    #[test]
    fn every_line_is_read_whatever_comes_before_it() {
        let f = "/etc/vtium/looks/odd.vtlook";
        let text = "version = 2\nname = odd\n[colour gamma]\ngamma = 1\njust words\n[color gamma]\ngamma = 1\ngamma = 1.1\ngain = 2\n[color gamma]\n";
        assert_eq!(
            errors("odd", text),
            [
                format!("{f}:1: reading: version = 2, needs version = 1"),
                format!(
                    "{f}:2: reading: found name before the first section, needs version = 1 alone there"
                ),
                format!(
                    "{f}:3: checking: found [colour gamma], needs [requires], [color FILTER] or [spatial FILTER]"
                ),
                format!("{f}:5: reading: found just words, needs a section heading or key = value"),
                format!(
                    "{f}:8: color filter 1 of 2, gamma: checking: gamma is set twice, needs it once; the first is on line 7"
                ),
                format!(
                    "{f}:9: color filter 1 of 2, gamma: checking: found gain, needs one of gamma"
                ),
                format!(
                    "{f}:10: color filter 2 of 2, gamma: checking: found gamma a second time, needs each filter once; the first is on line 6"
                ),
            ]
        );
        assert_eq!(
            errors("none", ""),
            [
                "/etc/vtium/looks/none.vtlook: reading: The file has no version = 1 line, which a vtlook needs before its first section."
            ]
        );
    }

    #[test]
    fn a_vtlook_holds_at_most_its_bound_of_sections() {
        let mut text = String::from("version = 1\n");
        for _ in 0..=VTLOOK_SECTIONS_MAX {
            text.push_str("[requires]\n");
        }
        let said = errors("many", &text);
        assert_eq!(said.len(), VTLOOK_SECTIONS_MAX, "{said:?}");
        assert!(
            said.last()
                .is_some_and(|s| s.contains("The file has more than 32 sections")),
            "{said:?}"
        );
    }

    // The file is read under the ownership check, its name by the name
    // rule, and the built-in vtlook's name is no file's.
    #[test]
    fn the_file_is_found_by_its_name_and_read_under_the_ownership_check() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = std::env::temp_dir().join(format!("vtlook-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_default();
        let me = std::fs::metadata(&dir).map_or(0, |m| m.uid());
        let trust = Trust::owner(me).below(&dir);
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
        let path = dir.join("plain.vtlook");
        std::fs::write(&path, "version = 1\n").unwrap_or_default();
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        assert!(read("plain", &dir, &trust).is_ok());
        let first = |r: Result<Vtlook, Failure>| {
            r.err()
                .and_then(|e| e.reports.first().map(|e| (e.stage, e.message.clone())))
                .unwrap_or((Stage::Building, String::new()))
        };
        let (stage, said) = first(read("missing", &dir, &trust));
        assert_eq!(stage, Stage::Reading);
        assert_eq!(
            said,
            format!("There is no file missing.vtlook in {}.", dir.display())
        );
        let (_, said) = first(read("default", &dir, &trust));
        assert_eq!(
            said,
            "vtium never reads a file named default.vtlook, because default is the name of the built-in vtlook. Give the file another name."
        );
        // a name that is no vtlook's names the folder, not a made-up file
        let bad = read("a/b", &dir, &trust).err().unwrap_or(Failure {
            reports: Vec::new(),
            more: 0,
        });
        assert_eq!(
            bad.reports.first().map(Report::at),
            Some(dir.display().to_string())
        );
        let (_, said) = first(read("a/b", &dir, &trust));
        assert!(
            said.starts_with("The name \"a/b\" is not a vtlook's name."),
            "{said}"
        );
        let (_, said) = first(read("\u{202e}", &dir, &trust));
        assert!(said.starts_with("The name \"\\u{202e}\""), "{said}");
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664));
        let (_, said) = first(read("plain", &dir, &trust));
        assert!(
            said.starts_with("vtium does not read the file, because other users can change"),
            "{said}"
        );
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        std::fs::write(&path, vec![b'#'; VTLOOK_FILE_MAX.saturating_add(1)]).unwrap_or_default();
        let (_, said) = first(read("plain", &dir, &trust));
        assert_eq!(
            said,
            "The file is larger than 65536 bytes, the most a vtlook may hold."
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A vtlook in the administrator's folder wins over the package's preset
    // of the same name, even when the administrator's copy is broken, and
    // removing the copy brings the preset back. A name in neither folder
    // names both.
    #[test]
    fn the_administrator_s_vtlook_wins_over_the_package_s_preset() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = std::env::temp_dir().join(format!("vtlook-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (etc, share) = (root.join("etc"), root.join("share"));
        for dir in [&root, &etc, &share] {
            std::fs::create_dir_all(dir).unwrap_or_default();
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
        }
        let me = std::fs::metadata(&root).map_or(0, |m| m.uid());
        let trust = Trust::owner(me).below(&root);
        let write = |path: &Path, text: &str| {
            std::fs::write(path, text).unwrap_or_default();
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644));
        };
        let folders = [etc.as_path(), share.as_path()];
        let filter = |r: Result<Vtlook, Failure>| {
            r.map(|l| {
                l.color
                    .iter()
                    .map(|f| f.declaration.name)
                    .collect::<Vec<_>>()
            })
            .map_err(|e| e.reports.first().map(|r| r.message.clone()))
        };
        write(&share.join("amber.vtlook"), "version = 1\n[color tint]\n");
        assert_eq!(filter(find("amber", &folders, &trust)), Ok(vec!["tint"]));
        write(&etc.join("amber.vtlook"), "version = 1\n[color gamma]\n");
        assert_eq!(filter(find("amber", &folders, &trust)), Ok(vec!["gamma"]));
        // a broken copy is the copy's error, not the preset drawn instead
        write(&etc.join("amber.vtlook"), "version = 1\n[color sepia]\n");
        let broken = find("amber", &folders, &trust).err().unwrap_or(Failure {
            reports: Vec::new(),
            more: 0,
        });
        assert_eq!(
            broken.reports.first().map(|r| r.file.clone()),
            Some(etc.join("amber.vtlook"))
        );
        let _ = std::fs::remove_file(etc.join("amber.vtlook"));
        assert_eq!(filter(find("amber", &folders, &trust)), Ok(vec!["tint"]));
        assert_eq!(
            filter(find("green", &folders, &trust)),
            Err(Some(format!(
                "There is no file green.vtlook in {} or {}.",
                etc.display(),
                share.display()
            )))
        );
        // a folder that does not exist holds nothing
        let gone = root.join("gone");
        assert_eq!(
            filter(find("amber", &[gone.as_path(), share.as_path()], &trust)),
            Ok(vec!["tint"])
        );
        // the built-in vtlook's name is no file's in any folder
        write(&share.join("default.vtlook"), "version = 1\n");
        assert!(
            filter(find("default", &folders, &trust))
                .is_err_and(|e| e.is_some_and(|m| m.starts_with("vtium never reads")))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // A byte-order mark before the first line is not part of it, as in
    // vtium.conf.
    #[test]
    fn a_byte_order_mark_is_not_part_of_the_first_line() {
        let text = "\u{feff}version = 1\n[color gamma]\n";
        assert_eq!(errors("bom", text), Vec::<String>::new());
        // only at the very start: elsewhere it is text the line holds
        let text = "version = 1\n\u{feff}[color gamma]\n";
        assert_eq!(errors("bom", text).len(), 1);
    }

    // A second version line names the first, as a key set twice does.
    #[test]
    fn a_second_version_line_names_the_first() {
        let f = "/etc/vtium/looks/twice.vtlook";
        assert_eq!(
            errors("twice", "version = 1\nversion = 1\n[color gamma]\n"),
            [format!(
                "{f}:2: reading: version is set twice, needs it once; the first is on line 1"
            )]
        );
        // a wrong first version is the only error of the first line
        assert_eq!(
            errors("twice", "version = 2\nversion = 1\n"),
            [
                format!("{f}:1: reading: version = 2, needs version = 1"),
                format!(
                    "{f}:2: reading: version is set twice, needs it once; the first is on line 1"
                ),
            ]
        );
    }

    // However many errors a file holds, a vtlook keeps the first
    // ERRORS_MAX by line and counts the rest, and each quote from the
    // file is cut and escaped, so a check's output and the log line stay
    // small and hold nothing that acts on a screen.
    #[test]
    fn a_vtlook_s_errors_and_quotes_are_bounded() {
        let many = "a\n".repeat(VTLOOK_FILE_MAX / 2);
        let failure = read_text_with("many", &file("many"), many.as_bytes(), FILTERS)
            .err()
            .unwrap_or(Failure {
                reports: Vec::new(),
                more: 0,
            });
        assert_eq!(failure.reports.len(), ERRORS_MAX);
        // every line is an error, and so is the missing version
        assert_eq!(
            failure.reports.len() + failure.more,
            VTLOOK_FILE_MAX / 2 + 1
        );
        assert_eq!(failure.reports.first().and_then(|r| r.line), None);
        assert_eq!(failure.reports.get(1).and_then(|r| r.line), Some(1));
        // a checking error on an early line is kept ahead of later
        // reading errors, though the reader finds it after them
        let late = format!("version = 1\n[color gamma]\ngamma = 9\n{many}");
        let failure = read_text_with("late", &file("late"), late.as_bytes(), FILTERS)
            .err()
            .unwrap_or(Failure {
                reports: Vec::new(),
                more: 0,
            });
        assert_eq!(failure.reports.first().and_then(|r| r.line), Some(3));
        let long = format!("version = 1\n{}\n", "x".repeat(65_000));
        let said = errors("long", &long);
        assert_eq!(
            said,
            [format!(
                "/etc/vtium/looks/long.vtlook:2: reading: found {}..., needs a section heading or key = value",
                "x".repeat(crate::bounds::QUOTE_MAX)
            )]
        );
        let said = errors("rlo", "version = 1\n[color \u{202e}kool]\n");
        assert_eq!(
            said,
            [
                "/etc/vtium/looks/rlo.vtlook:2: color filter 1 of 1, \\u{202e}kool: checking: found \\u{202e}kool, needs gamma, tint or cube"
            ]
        );
        let said = errors("rlo", "version = 1\n[color gamma]\ngamma = \u{202e}1\n");
        assert!(
            said.iter().all(|s| !s.contains('\u{202e}')) && said.len() == 1,
            "{said:?}"
        );
    }
}
