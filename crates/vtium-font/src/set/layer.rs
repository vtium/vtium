//! Layers: shared parts that a font set (a frit) adds by name.
//!
//! A layer is a file `NAME.vtlayer` in the `layers/` folder of the frits'
//! directory. Its first key, `role=`, says what the layer brings: a
//! script, symbols, emoji or the fallback for every other character. Its
//! other lines are `source=` lines and the keys that describe the source
//! above them. A frit adds a layer with a `layer=NAME` line, and whoever
//! hands the frit to the glyph helper puts the layer's sources in place of
//! that line, so the helper still reads one list of sources.
//!
//! This module reads a layer's structure and splices layers into a frit's
//! text. It parses no font and no value of a source's keys: the glyph
//! helper's frit reader checks those, after the splice, as it checks a
//! frit's own lines. The console and vtium-config both splice through
//! [`expand`], so a set holds the same sources wherever it is read.
//!
//! References:
//! - ADR-1204 D5: layers, their folder, `role=`, and the splice in place.
//! - ADR-1204 D6: the roles, and one symbols, emoji and fallback layer at
//!   most in a set.

use std::fmt;

/// The folder of the frits' directory that holds the layers.
pub const DIR: &str = "layers";

/// The ending of a layer's file name.
pub const SUFFIX: &str = ".vtlayer";

/// What a layer brings to a set.
///
/// References:
/// - ADR-1204 D6: the four roles and the order they come in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Role {
    /// A writing system that the text font lacks.
    Script,
    /// The icons that prompts and file listers draw.
    Symbols,
    /// Color emoji.
    Emoji,
    /// Every character that the sources before it lack.
    Fallback,
}

impl Role {
    /// The roles in the order a set's layers come in.
    pub const ORDER: [Role; 4] = [Role::Script, Role::Symbols, Role::Emoji, Role::Fallback];

    /// The role `role=`'s value names, spelled exactly so.
    pub fn parse(value: &str) -> Option<Role> {
        Role::ORDER.into_iter().find(|r| r.name() == value)
    }

    /// The role's name, as `role=` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Role::Script => "script",
            Role::Symbols => "symbols",
            Role::Emoji => "emoji",
            Role::Fallback => "fallback",
        }
    }

    /// Whether a set holds one layer of this role at most. A set may hold
    /// several script layers, one for each writing system it adds.
    pub fn single(self) -> bool {
        self != Role::Script
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The keys that describe the `source=` line above them, which a layer
/// holds besides its sources.
pub const SOURCE_KEYS: [&str; 5] = ["cover", "fit", "render", "style", "strike"];

/// The keys that describe a frit's own `source=` line and that a layer's
/// source does not take: a console font's Unicode table, a CPI font's
/// height and a bitmap font's smaller size.
pub const FRIT_ONLY_SOURCE_KEYS: [&str; 3] = ["unimap", "cp-size", "script-source"];

/// The keys of the set as a whole, which apply to every source, a
/// layer's included, and which a layer therefore cannot hold.
pub const SET_KEYS: [&str; 10] = [
    "name",
    "cell",
    "size",
    "line-spacing",
    "antialias",
    "hinting",
    "license",
    "compress",
    "glyph-source",
    "layer",
];

/// Why a layer's text is refused: its line, from 1, and the reason.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LayerError {
    pub line: usize,
    pub why: String,
}

impl fmt::Display for LayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.why)
        } else {
            write!(f, "line {}: {}", self.line, self.why)
        }
    }
}

/// A layer whose structure reads: its role, and its text with the
/// `role=` line left blank, so that every other line keeps its number
/// for a reader that checks the values.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Layer {
    pub role: Role,
    pub body: String,
}

/// The key and value of a line in the frit's line syntax, with `#` to the
/// line's end a comment and blanks around both dropped; `None` for a
/// blank or comment line, and an empty key for a line that is not
/// `key=value`.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let body = line.split('#').next().unwrap_or_default().trim();
    if body.is_empty() {
        return None;
    }
    Some(
        body.split_once('=')
            .map_or(("", body), |(k, v)| (k.trim(), v.trim())),
    )
}

/// Whether `key` is a set's key, `glyph-source-<range>=` included.
fn set_key(key: &str) -> bool {
    SET_KEYS.contains(&key) || key.starts_with("glyph-source-")
}

/// Reads a layer's structure: `role=` first, with one of the four roles;
/// then `source=` lines, each followed by the keys that describe it. A
/// set's key, a `layer=` line and any other key refuse the layer, each
/// naming its line.
pub fn parse(text: &str) -> Result<Layer, LayerError> {
    let err = |line: usize, why: String| LayerError { line, why };
    let mut role = None;
    let mut body = String::with_capacity(text.len());
    let mut sources = 0usize;
    for (piece, n) in text.split_inclusive('\n').zip(1usize..) {
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        let Some((key, value)) = key_value(line) else {
            body.push_str(piece);
            continue;
        };
        if key.is_empty() {
            return Err(err(n, "the line is not key=value".to_owned()));
        }
        if role.is_none() {
            if key != "role" {
                return Err(err(
                    n,
                    format!(
                        "a layer starts with role=, and this line is {key}=. role= takes script, symbols, emoji or fallback"
                    ),
                ));
            }
            role = Some(Role::parse(value).ok_or_else(|| {
                err(
                    n,
                    format!("role={value} is not a role: use script, symbols, emoji or fallback"),
                )
            })?);
            body.push_str(&piece[line.len()..]);
            continue;
        }
        match key {
            "role" => return Err(err(n, "role= appears twice".to_owned())),
            "source" => sources = sources.saturating_add(1),
            k if SOURCE_KEYS.contains(&k) => {
                if sources == 0 {
                    return Err(err(n, format!("{k}= comes before any source= line")));
                }
            }
            "layer" => {
                return Err(err(
                    n,
                    "a layer cannot add another layer: only a font set holds layer= lines"
                        .to_owned(),
                ));
            }
            k if set_key(k) => {
                return Err(err(
                    n,
                    format!(
                        "{k}= is a font set's key, which applies to every source: a layer holds only source= lines and the keys that describe a source"
                    ),
                ));
            }
            k if FRIT_ONLY_SOURCE_KEYS.contains(&k) => {
                return Err(err(
                    n,
                    format!(
                        "a layer's source does not take {k}=. It takes cover=, fit=, render=, style= and strike="
                    ),
                ));
            }
            k => return Err(err(n, format!("unknown key {k}"))),
        }
        body.push_str(piece);
    }
    let role = role.ok_or_else(|| err(0, "the layer has no role= line".to_owned()))?;
    if sources == 0 {
        return Err(err(0, "the layer has no source= line".to_owned()));
    }
    Ok(Layer { role, body })
}

/// Whether `name` can name a layer: 1 to 64 characters of `A-Z a-z 0-9 .
/// _ + -`, not starting with `.`, as [`NAME_RULE`] says. A name holds no
/// `/` and cannot be `.` or `..`, so no name reaches outside the layers'
/// folder.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

/// How the name rule reads in a message: what a layer's name may hold.
pub const NAME_RULE: &str = "A layer's name has 1 to 64 characters: letters, digits, '.', '_', '+' and '-', and it does not start with a dot.";

/// The most sources a font set holds, its layers' sources included.
pub const SET_SOURCES_MAX: usize = 16;

/// The most bytes a font set's text holds, with its layers in place.
pub const SET_BYTES_MAX: usize = 64 * 1024;

/// The most `layer=` lines of a font set that vtium reads. Each layer adds
/// one source at least, so a set reaches [`SET_SOURCES_MAX`] before it
/// runs out of these.
pub const LAYERS_MAX: usize = SET_SOURCES_MAX;

/// The line that closes a layer's lines in a spliced frit. On its own,
/// outside a layer's lines, it stands for a `layer=` line that added
/// nothing.
pub const END: &str = "layer=";

/// A layer put in place: its name, its role and the frit's `layer=` line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Placed {
    pub name: String,
    pub role: Role,
    pub line: usize,
}

/// Why a `layer=` line added nothing to the set.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Problem {
    /// The value is not a layer's name.
    BadName,
    /// The layers' folder holds no file of that name.
    Missing,
    /// The file could not be read, or failed the ownership check: why,
    /// worded to follow "because" and ending in a full stop.
    Unread(String),
    /// The layer's own structure is wrong, on its line.
    Invalid(LayerError),
    /// An earlier line of the set adds the same layer, on that line.
    Twice(usize),
    /// The layer's role is one a set holds once, and the layer `by`, on
    /// the set's line `at`, already brings it.
    RoleTaken { role: Role, by: String, at: usize },
    /// The layer would give the set this many sources, more than
    /// [`SET_SOURCES_MAX`].
    TooManySources(usize),
    /// The layer would make the set's text larger than [`SET_BYTES_MAX`].
    TooLarge,
    /// The set already has [`LAYERS_MAX`] `layer=` lines above this one,
    /// so vtium does not read the layer.
    TooManyLayers,
}

/// A `layer=` line that added nothing, and why. The set draws without
/// that layer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Note {
    /// The frit's `layer=` line, from 1.
    pub line: usize,
    /// The layer's name, as the line spells it.
    pub layer: String,
    pub problem: Problem,
}

/// A frit with its layers in place.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Expanded {
    /// The spliced text: the frit's text, where each `layer=NAME` line
    /// that adds a layer is followed by every line of the layer, its
    /// `role=` line left blank, and then by an [`END`] line. A `layer=`
    /// line that adds nothing becomes an [`END`] line on its own. So the
    /// glyph helper still reads one list of sources, and [`lines`] tells
    /// from the text alone which file and line each line came from.
    pub text: String,
    /// The layers put in place, in the set's order.
    pub placed: Vec<Placed>,
    /// Each `layer=` line that added nothing.
    pub notes: Vec<Note>,
}

/// What reading a layer by name gives: its text; `Ok(None)` when the
/// layers' folder holds no such file; or why it is not read, worded to
/// follow "because" and ending in a full stop.
pub type Read<'a> = &'a mut dyn FnMut(&str) -> Result<Option<String>, String>;

/// The frit `text` with each layer that the `layer=NAME` lines name put in
/// place, as [`Expanded::text`] describes, so that the sources keep the
/// set's order.
///
/// A `layer=` line adds nothing, with a [`Note`] that says why, when its
/// name is not a layer's, when the layer is missing or cannot be read,
/// when its structure is wrong, when an earlier line adds the same layer,
/// when the set already holds a layer of the same single-use role, and
/// when the layer would take the set past [`SET_SOURCES_MAX`] sources or
/// [`SET_BYTES_MAX`] bytes. The set then draws without that layer: of two
/// layers of one role, the set keeps the one it names first. vtium reads
/// [`LAYERS_MAX`] `layer=` lines at most, and notes each one past them.
pub fn expand(text: &str, read: Read<'_>) -> Expanded {
    let mut out = Expanded::default();
    let mut sources = text
        .split('\n')
        .filter(|line| matches!(key_value(line), Some(("source", _))))
        .count();
    let mut bytes = text.len();
    let mut layer_lines = 0usize;
    for (piece, n) in text.split_inclusive('\n').zip(1usize..) {
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        let layer = match key_value(line) {
            Some(("layer", value)) => value,
            _ => {
                out.text.push_str(piece);
                continue;
            }
        };
        layer_lines = layer_lines.saturating_add(1);
        let placed = if layer_lines > LAYERS_MAX {
            Err(Problem::TooManyLayers)
        } else {
            place(&out.placed, layer, read)
        };
        let region = placed.and_then(|(role, body)| {
            let region = format!("{line}\n{}{END}\n", lines_of(&body));
            let added = body
                .split('\n')
                .filter(|l| matches!(key_value(l), Some(("source", _))))
                .count();
            let grown = bytes
                .saturating_add(region.len())
                .saturating_sub(piece.len());
            if sources.saturating_add(added) > SET_SOURCES_MAX {
                Err(Problem::TooManySources(sources.saturating_add(added)))
            } else if grown > SET_BYTES_MAX {
                Err(Problem::TooLarge)
            } else {
                sources = sources.saturating_add(added);
                bytes = grown;
                Ok((role, region))
            }
        });
        match region {
            Ok((role, region)) => {
                out.placed.push(Placed {
                    name: layer.to_owned(),
                    role,
                    line: n,
                });
                out.text.push_str(&region);
            }
            Err(problem) => {
                out.text.push_str(END);
                out.text.push('\n');
                out.notes.push(Note {
                    line: n,
                    layer: layer.to_owned(),
                    problem,
                });
            }
        }
    }
    out
}

/// `body` with each of its lines ending in a newline, the last included.
fn lines_of(body: &str) -> String {
    let mut out = body.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The role and body of the layer `name`, which `read` gives, when it can
/// join a set that already holds the layers `placed`; else why it adds
/// nothing.
fn place(placed: &[Placed], name: &str, read: Read<'_>) -> Result<(Role, String), Problem> {
    if !valid_name(name) {
        return Err(Problem::BadName);
    }
    if let Some(first) = placed.iter().find(|p| p.name == name) {
        return Err(Problem::Twice(first.line));
    }
    let layer = match read(name) {
        Ok(Some(text)) => parse(&text).map_err(Problem::Invalid)?,
        Ok(None) => return Err(Problem::Missing),
        Err(why) => return Err(Problem::Unread(why)),
    };
    if layer.role.single()
        && let Some(by) = placed.iter().find(|p| p.role == layer.role)
    {
        return Err(Problem::RoleTaken {
            role: layer.role,
            by: by.name.clone(),
            at: by.line,
        });
    }
    Ok((layer.role, layer.body))
}

/// Where a line of a spliced frit came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Line<'a> {
    /// The frit's own line, from 1. A layer's opening `layer=NAME` line
    /// and its closing [`END`] line are both the frit's `layer=` line.
    Set(usize),
    /// Line `line`, from 1, of the layer `name`, which the frit's line
    /// `at` adds.
    Layer {
        name: &'a str,
        line: usize,
        at: usize,
    },
}

/// Where each line of `spliced`, a text that [`expand`] gave, came from, in
/// order: one entry for each line that `spliced.split('\n')` gives. It is
/// read from the text alone, so the glyph helper, which is handed the
/// spliced text with no file, names a layer's file and line in its
/// messages as vtium-config does.
pub fn lines(spliced: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut own = 0usize;
    // the layer whose lines are being read: its name, the frit's line
    // that adds it, and the layer's last line read
    let mut open: Option<(&str, usize, usize)> = None;
    for line in spliced.split('\n') {
        let kv = key_value(line);
        match (open, kv) {
            (Some(_), Some(("layer", ""))) => {
                open = None;
                out.push(Line::Set(own));
            }
            (Some((name, at, n)), _) => {
                let n = n.saturating_add(1);
                out.push(Line::Layer { name, line: n, at });
                open = Some((name, at, n));
            }
            (None, kv) => {
                own = own.saturating_add(1);
                if let Some(("layer", name)) = kv
                    && !name.is_empty()
                {
                    open = Some((name, own, 0));
                }
                out.push(Line::Set(own));
            }
        }
    }
    out
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const EMOJI: &str = "role=emoji\nsource=font:Noto Color Emoji\n";
    const SYMBOLS: &str =
        "# icons\nrole=symbols\nsource=font:Symbols Nerd Font Mono\ncover=U+E000-U+F8FF\n";

    fn reader(layers: &[(&str, &str)]) -> impl FnMut(&str) -> Result<Option<String>, String> {
        let layers: BTreeMap<String, String> = layers
            .iter()
            .map(|&(n, t)| (n.to_owned(), t.to_owned()))
            .collect();
        move |name| Ok(layers.get(name).cloned())
    }

    #[test]
    fn a_layers_sources_go_in_place_of_its_line() {
        let mut read = reader(&[("nerd-symbols", SYMBOLS), ("noto-emoji", EMOJI)]);
        let set = "cell=8x16\nsource=font:Fira Code\nlayer=nerd-symbols\nsource=font:DejaVu Sans Mono\nlayer=noto-emoji\nline-spacing=10\n";
        let out = expand(set, &mut read);
        assert_eq!(
            out.text,
            "cell=8x16\nsource=font:Fira Code\nlayer=nerd-symbols\n# icons\n\nsource=font:Symbols Nerd Font Mono\ncover=U+E000-U+F8FF\nlayer=\nsource=font:DejaVu Sans Mono\nlayer=noto-emoji\n\nsource=font:Noto Color Emoji\nlayer=\nline-spacing=10\n"
        );
        let symbols = |line| Line::Layer {
            name: "nerd-symbols",
            line,
            at: 3,
        };
        let emoji = |line| Line::Layer {
            name: "noto-emoji",
            line,
            at: 5,
        };
        assert_eq!(
            lines(&out.text),
            vec![
                Line::Set(1),
                Line::Set(2),
                Line::Set(3),
                symbols(1),
                symbols(2),
                symbols(3),
                symbols(4),
                Line::Set(3),
                Line::Set(4),
                Line::Set(5),
                emoji(1),
                emoji(2),
                Line::Set(5),
                Line::Set(6),
                Line::Set(7),
            ]
        );
        assert_eq!(
            out.placed,
            vec![
                Placed {
                    name: "nerd-symbols".to_owned(),
                    role: Role::Symbols,
                    line: 3
                },
                Placed {
                    name: "noto-emoji".to_owned(),
                    role: Role::Emoji,
                    line: 5
                },
            ]
        );
        assert!(out.notes.is_empty());
    }

    #[test]
    fn a_set_with_no_layer_keeps_its_text() {
        let mut read = reader(&[]);
        for text in [
            "source=font:A\n",
            "source=font:A",
            "source=font:A\n\n# end\n",
        ] {
            let out = expand(text, &mut read);
            assert_eq!(out.text, text);
            let own: Vec<Line> = (1..=text.split('\n').count()).map(Line::Set).collect();
            assert_eq!(lines(&out.text), own);
        }
    }

    #[test]
    fn a_layer_that_adds_nothing_is_noted_and_left_out() {
        let mut read = |name: &str| match name {
            "noto-emoji" => Ok(Some(EMOJI.to_owned())),
            "emoji-two" => Ok(Some(EMOJI.to_owned())),
            "nerd-symbols" => Ok(Some(SYMBOLS.to_owned())),
            "theirs" => Err("the file is writable by others.".to_owned()),
            "broken" => Ok(Some("role=emoji\ncover=U+1F600\n".to_owned())),
            _ => Ok(None),
        };
        let set = "source=font:A\nlayer=noto-emoji\nlayer=gone\nlayer=../x\nlayer=noto-emoji\nlayer=emoji-two\nlayer=theirs\nlayer=broken\nlayer=nerd-symbols\n";
        let out = expand(set, &mut read);
        assert_eq!(
            out.text,
            "source=font:A\nlayer=noto-emoji\n\nsource=font:Noto Color Emoji\nlayer=\nlayer=\nlayer=\nlayer=\nlayer=\nlayer=\nlayer=\nlayer=nerd-symbols\n# icons\n\nsource=font:Symbols Nerd Font Mono\ncover=U+E000-U+F8FF\nlayer=\n"
        );
        // each line that adds nothing keeps its place, so the frit's own
        // lines keep their numbers
        let own: Vec<usize> = lines(&out.text)
            .into_iter()
            .filter_map(|l| match l {
                Line::Set(n) => Some(n),
                Line::Layer { .. } => None,
            })
            .collect();
        assert_eq!(own, [1, 2, 2, 3, 4, 5, 6, 7, 8, 9, 9, 10]);
        let problems: Vec<(usize, Problem)> =
            out.notes.into_iter().map(|n| (n.line, n.problem)).collect();
        assert_eq!(
            problems,
            vec![
                (3, Problem::Missing),
                (4, Problem::BadName),
                (5, Problem::Twice(2)),
                (
                    6,
                    Problem::RoleTaken {
                        role: Role::Emoji,
                        by: "noto-emoji".to_owned(),
                        at: 2
                    }
                ),
                (
                    7,
                    Problem::Unread("the file is writable by others.".to_owned())
                ),
                (
                    8,
                    Problem::Invalid(LayerError {
                        line: 2,
                        why: "cover= comes before any source= line".to_owned()
                    })
                ),
            ]
        );
    }

    // A layer that would take the set past its 16 sources or its 64 KiB of
    // text adds nothing, so the set draws without it instead of the helper
    // refusing the whole set. vtium reads 16 layer= lines at most.
    #[test]
    fn a_layer_that_would_overfill_the_set_adds_nothing() {
        let six = "role=script\nsource=font:A\nsource=font:B\nsource=font:C\nsource=font:D\nsource=font:E\nsource=font:F\n";
        let mut read = |_: &str| Ok(Some(six.to_owned()));
        let out = expand("source=font:Text\nlayer=a\nlayer=b\nlayer=c\n", &mut read);
        assert_eq!(out.placed.len(), 2);
        assert_eq!(out.notes.len(), 1);
        assert_eq!(
            (out.notes[0].line, &out.notes[0].problem),
            (4, &Problem::TooManySources(19))
        );
        let big = format!(
            "role=script\nsource=font:A\n# {}\n",
            "x".repeat(SET_BYTES_MAX / 2)
        );
        let mut read = |_: &str| Ok(Some(big.clone()));
        let out = expand("source=font:Text\nlayer=a\nlayer=b\n", &mut read);
        assert_eq!(out.placed.len(), 1);
        assert_eq!(out.notes[0].problem, Problem::TooLarge);
        assert!(out.text.len() <= SET_BYTES_MAX);
        let mut asked = 0usize;
        let mut read = |_: &str| {
            asked += 1;
            Ok(None)
        };
        let many: String = (0..LAYERS_MAX + 2)
            .map(|i| format!("layer=l{i}\n"))
            .collect();
        let out = expand(&many, &mut read);
        assert_eq!(asked, LAYERS_MAX);
        let past: Vec<&Problem> = out.notes[LAYERS_MAX..].iter().map(|n| &n.problem).collect();
        assert_eq!(past, [&Problem::TooManyLayers, &Problem::TooManyLayers]);
    }

    // A layer's name keeps the layer inside the layers' folder: no '/', no
    // leading dot, 1 to 64 characters.
    #[test]
    fn a_layers_name_stays_inside_the_layers_folder() {
        for name in ["a/b", "x/../../y", "", ".hidden", "..", "a b", "a\\b"] {
            assert!(!valid_name(name), "{name:?}");
        }
        assert!(valid_name(&"a".repeat(64)));
        assert!(!valid_name(&"a".repeat(65)));
        for name in ["nerd-symbols", "noto-emoji", "cjk.sc", "a_b+c", "x..y"] {
            assert!(valid_name(name), "{name:?}");
        }
    }

    #[test]
    fn several_script_layers_go_in_one_set() {
        let script = "role=script\nsource=font:Noto Sans Arabic\n";
        let mut read = |_: &str| Ok(Some(script.to_owned()));
        let out = expand("source=font:A\nlayer=arabic\nlayer=hebrew\n", &mut read);
        assert_eq!(out.placed.len(), 2);
        assert!(out.notes.is_empty());
    }

    #[test]
    fn a_layer_starts_with_its_role_and_holds_only_sources() {
        let refused = |text: &str| parse(text).unwrap_err();
        let cases: [(&str, usize, &str); 10] = [
            ("source=font:A\n", 1, "a layer starts with role="),
            ("role=icons\nsource=font:A\n", 1, "role=icons is not a role"),
            ("role=emoji\nrole=emoji\n", 2, "role= appears twice"),
            (
                "role=emoji\nsource=font:A\ncell=8x16\n",
                3,
                "cell= is a font set's key",
            ),
            (
                "role=emoji\nsource=font:A\nsize=12\n",
                3,
                "size= is a font set's key",
            ),
            (
                "role=emoji\nsource=font:A\nline-spacing=10\n",
                3,
                "line-spacing= is a font set's key",
            ),
            (
                "role=emoji\nsource=font:A\nglyph-source-box=font\n",
                3,
                "glyph-source-box= is a font set's key",
            ),
            (
                "role=emoji\nsource=font:A\nlayer=b\n",
                3,
                "a layer cannot add another layer",
            ),
            (
                "role=emoji\nsource=font:A\nversion=2.0\n",
                3,
                "unknown key version",
            ),
            (
                "role=emoji\nfit=natural\n",
                2,
                "fit= comes before any source= line",
            ),
        ];
        for (text, line, why) in cases {
            let e = refused(text);
            assert_eq!(e.line, line, "{text:?}");
            assert!(e.why.starts_with(why), "{text:?}: {}", e.why);
        }
        for key in ["unimap", "cp-size", "script-source"] {
            let e = refused(&format!("role=symbols\nsource=font:A\n{key}=x\n"));
            assert_eq!(
                e.why,
                format!(
                    "a layer's source does not take {key}=. It takes cover=, fit=, render=, style= and strike="
                )
            );
        }
        for key in ["check", "sha256", "url"] {
            let e = refused(&format!("role=symbols\nsource=font:A\n{key}=x\n"));
            assert_eq!(e.why, format!("unknown key {key}"));
        }
        assert_eq!(refused("# nothing\n").why, "the layer has no role= line");
        assert_eq!(
            refused("role=fallback\n").why,
            "the layer has no source= line"
        );
        assert_eq!(
            refused("role=emoji\nsource\n").why,
            "the line is not key=value"
        );
    }

    #[test]
    fn a_layers_body_keeps_its_line_numbers() {
        let layer = parse(SYMBOLS).unwrap();
        assert_eq!(layer.role, Role::Symbols);
        assert_eq!(layer.body.split('\n').count(), SYMBOLS.split('\n').count());
        assert_eq!(
            layer.body.split('\n').nth(2),
            Some("source=font:Symbols Nerd Font Mono")
        );
        assert_eq!(layer.body.split('\n').nth(1), Some(""));
        let every = "role=symbols\nsource=outline:/a.ttf\ncover=U+E000\nfit=natural\nrender=bitmap\nstyle=bold\nstrike=bdf:/a.bdf";
        let layer = parse(every).unwrap();
        assert_eq!(layer.body.split('\n').count(), 7);
    }

    #[test]
    fn roles_read_in_their_order() {
        assert_eq!(
            Role::ORDER.map(Role::name),
            ["script", "symbols", "emoji", "fallback"]
        );
        assert!(Role::ORDER.windows(2).all(|w| w[0] < w[1]));
        assert!(!Role::Script.single());
        assert!(Role::ORDER[1..].iter().all(|r| r.single()));
    }
}
