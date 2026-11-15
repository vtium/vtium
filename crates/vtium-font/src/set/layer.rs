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
                        "a layer starts with role=, and this line is {key}= (role= takes script, symbols, emoji or fallback)"
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

/// Whether `name` can name a layer: a frit's name, 1 to 64 characters of
/// `A-Z a-z 0-9 . _ + -`, not starting with `.`, so that no name reaches
/// outside the layers' folder.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

/// Where a line of a spliced frit came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum From {
    /// The frit's own line, from 1.
    Set(usize),
    /// A layer's line, from 1: the layer is [`Expanded::placed`]'s entry
    /// at `layer`.
    Layer { layer: usize, line: usize },
}

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
    /// The frit's text with each `layer=` line replaced by its layer's
    /// sources, and dropped where the layer adds nothing.
    pub text: String,
    /// Where each line of [`Expanded::text`] came from, in order.
    pub lines: Vec<From>,
    /// The layers put in place, in the set's order.
    pub placed: Vec<Placed>,
    /// Each `layer=` line that added nothing.
    pub notes: Vec<Note>,
}

impl Expanded {
    /// Where line `n` of the spliced text, from 1, came from.
    pub fn from(&self, n: usize) -> Option<From> {
        n.checked_sub(1).and_then(|i| self.lines.get(i)).copied()
    }
}

/// What reading a layer by name gives: its text; `Ok(None)` when the
/// layers' folder holds no such file; or why it is not read, worded to
/// follow "because" and ending in a full stop.
pub type Read<'a> = &'a mut dyn FnMut(&str) -> Result<Option<String>, String>;

/// The frit `text` with each `layer=NAME` line replaced by the sources of
/// the layer that `read` gives for NAME, in place, so that the sources
/// keep the set's order.
///
/// A `layer=` line adds nothing, with a [`Note`] that says why, when its
/// name is not a layer's, when the layer is missing or cannot be read,
/// when its structure is wrong, when an earlier line adds the same layer,
/// and when the set already holds a layer of the same single-use role.
/// The set then draws without that layer: the first of two layers of one
/// role is the one the set keeps, because the set names it first.
pub fn expand(text: &str, read: Read<'_>) -> Expanded {
    let mut out = Expanded::default();
    for (piece, n) in text.split_inclusive('\n').zip(1usize..) {
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        let layer = match key_value(line) {
            Some(("layer", value)) => value,
            _ => {
                out.text.push_str(piece);
                out.lines.push(From::Set(n));
                continue;
            }
        };
        let note = |problem| Note {
            line: n,
            layer: layer.to_owned(),
            problem,
        };
        match place(&out.placed, layer, read) {
            Ok((role, body)) => {
                let index = out.placed.len();
                out.placed.push(Placed {
                    name: layer.to_owned(),
                    role,
                    line: n,
                });
                for (body_line, m) in body.split('\n').zip(1usize..) {
                    if key_value(body_line).is_some() {
                        out.text.push_str(body_line);
                        out.text.push('\n');
                        out.lines.push(From::Layer {
                            layer: index,
                            line: m,
                        });
                    }
                }
            }
            Err(problem) => out.notes.push(note(problem)),
        }
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
            "cell=8x16\nsource=font:Fira Code\nsource=font:Symbols Nerd Font Mono\ncover=U+E000-U+F8FF\nsource=font:DejaVu Sans Mono\nsource=font:Noto Color Emoji\nline-spacing=10\n"
        );
        assert_eq!(
            out.lines,
            vec![
                From::Set(1),
                From::Set(2),
                From::Layer { layer: 0, line: 3 },
                From::Layer { layer: 0, line: 4 },
                From::Set(4),
                From::Layer { layer: 1, line: 2 },
                From::Set(6),
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
            assert_eq!(
                out.lines.len(),
                text.split('\n').count() - usize::from(text.ends_with('\n'))
            );
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
            "source=font:A\nsource=font:Noto Color Emoji\nsource=font:Symbols Nerd Font Mono\ncover=U+E000-U+F8FF\n"
        );
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
