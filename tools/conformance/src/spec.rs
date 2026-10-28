//! The replay interface's machine-readable blocks: `describe`, the views,
//! features and commands an implementation with every feature offers, and
//! `grammar`, the form of every line a view prints (INTERFACE.md,
//! "Grammar").

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

/// The views a `line` form lists as `*`.
const ALL_VIEWS: &str = "screen,draw2,frame";

/// One form of a line kind: the views it is printed in, and its pattern
/// with the reserved keys empty and with them allowed.
#[derive(Clone, Debug)]
struct Form {
    views: Vec<String>,
    plain: Regex,
    reserved: Regex,
}

/// The interface's `describe` and `grammar` blocks.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The describe block's lines, `interface MAJOR.MINOR` first.
    pub describe: Vec<String>,
    pub major: String,
    pub features: BTreeSet<String>,
    /// Each command and the feature it needs, if any.
    pub commands: BTreeMap<String, Option<String>>,
    lines: BTreeMap<String, Vec<Form>>,
}

/// The lines of the fenced block named `name`, or None.
pub fn fenced<'a>(text: &'a str, name: &str) -> Option<Vec<&'a str>> {
    let mut lines = text.lines();
    lines.find(|l| l.strip_prefix("```") == Some(name))?;
    let mut out = Vec::new();
    for line in lines {
        if line == "```" {
            return Some(out);
        }
        out.push(line);
    }
    None
}

/// `pattern`, matched against a whole text.
pub fn whole(pattern: &str) -> Result<Regex, String> {
    Regex::new(&format!(r"\A(?:{pattern})\z")).map_err(|e| e.to_string())
}

impl Spec {
    /// Reads the blocks from the interface's text.
    pub fn read(text: &str) -> Result<Self, String> {
        let describe = fenced(text, "describe").ok_or("the interface has no describe block")?;
        let major = describe
            .first()
            .and_then(|l| l.strip_prefix("interface "))
            .and_then(|v| v.split('.').next())
            .ok_or("the describe block does not start with interface MAJOR.MINOR")?
            .to_owned();
        let words = |prefix: &str| -> Vec<Vec<String>> {
            describe
                .iter()
                .filter_map(|l| l.strip_prefix(prefix))
                .map(|rest| rest.split_whitespace().map(str::to_owned).collect())
                .collect()
        };
        let features = words("feature ")
            .into_iter()
            .filter_map(|w| w.into_iter().next())
            .collect();
        let commands = words("command ")
            .into_iter()
            .filter_map(|w| {
                let mut w = w.into_iter();
                Some((w.next()?, w.next()))
            })
            .collect();
        let mut spec = Self {
            describe: describe.iter().map(|l| (*l).to_owned()).collect(),
            major,
            features,
            commands,
            lines: BTreeMap::new(),
        };
        spec.read_grammar(&fenced(text, "grammar").ok_or("the interface has no grammar block")?)?;
        Ok(spec)
    }

    fn read_grammar(&mut self, block: &[&str]) -> Result<(), String> {
        // each define with the reserved keys empty, and with them allowed
        let mut defines: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut reserved: BTreeMap<String, String> = BTreeMap::new();
        for line in block {
            let Some((word, rest)) = line.split_once(' ') else {
                continue;
            };
            match word {
                "define" | "reserved" => {
                    let (name, regex) = rest
                        .split_once(' ')
                        .ok_or_else(|| format!("grammar line {line:?}"))?;
                    if word == "reserved" {
                        reserved.insert(name.to_owned(), regex.to_owned());
                    } else {
                        let plain = expand(regex, false, &defines, &reserved)?;
                        let with = expand(regex, true, &defines, &reserved)?;
                        defines.insert(name.to_owned(), (plain, with));
                    }
                }
                "line" => {
                    let mut parts = rest.splitn(3, ' ');
                    let (Some(kind), Some(views), Some(regex)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        return Err(format!("grammar line {line:?}"));
                    };
                    let views = if views == "*" { ALL_VIEWS } else { views };
                    let form = Form {
                        views: views.split(',').map(str::to_owned).collect(),
                        plain: whole(&expand(regex, false, &defines, &reserved)?)?,
                        reserved: whole(&expand(regex, true, &defines, &reserved)?)?,
                    };
                    self.lines.entry(kind.to_owned()).or_default().push(form);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Whether `line` is a line the grammar allows in `view`, with the
    /// reserved keys allowed when `with_reserved`.
    pub fn parses(&self, line: &str, view: &str, with_reserved: bool) -> bool {
        let kind = line.split(' ').next().unwrap_or_default();
        self.lines.get(kind).is_some_and(|forms| {
            forms.iter().any(|f| {
                f.views.iter().any(|v| v == view)
                    && if with_reserved {
                        f.reserved.is_match(line)
                    } else {
                        f.plain.is_match(line)
                    }
            })
        })
    }
}

/// `regex` with each `{NAME}` replaced by `(?:FRAGMENT)`: a reserved
/// fragment empty unless `with_reserved`. A brace that names no upper-case
/// fragment, as in `{6}`, stays.
fn expand(
    regex: &str,
    with_reserved: bool,
    defines: &BTreeMap<String, (String, String)>,
    reserved: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut out = String::with_capacity(regex.len());
    let mut rest = regex;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let name = tail[1..].split('}').next().filter(|n| {
            n.starts_with(|c: char| c.is_ascii_uppercase())
                && n.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && tail[1..].contains('}')
        });
        match name {
            Some(name) => {
                let fragment = match (reserved.get(name), defines.get(name)) {
                    (Some(r), _) if with_reserved => r.as_str(),
                    (Some(_), _) => "",
                    (None, Some((plain, with))) => {
                        if with_reserved {
                            with.as_str()
                        } else {
                            plain.as_str()
                        }
                    }
                    (None, None) => return Err(format!("the grammar has no fragment {name}")),
                };
                out.push_str("(?:");
                out.push_str(fragment);
                out.push(')');
                rest = &tail[name.len().saturating_add(2)..];
            }
            None => {
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "```describe
interface 1.0
view screen
feature sync
command resize
command sync sync
```

```grammar
define N 0|[1-9][0-9]*
reserved RESERVED (?: overline)?
define ATTR (?: bold)?{RESERVED}
line size * size [1-9][0-9]*x[1-9][0-9]*
line attr screen attr {N} {N}-{N}{ATTR}
line reply * reply (?:[0-9a-f]{2})+
```
";

    fn spec() -> Spec {
        Spec::read(SPEC).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn the_describe_block_gives_the_major_features_and_commands() {
        let s = spec();
        assert_eq!(s.major, "1");
        assert_eq!(s.features.iter().collect::<Vec<_>>(), ["sync"]);
        assert_eq!(s.commands.get("resize"), Some(&None));
        assert_eq!(s.commands.get("sync"), Some(&Some("sync".to_owned())));
    }

    #[test]
    fn lines_parse_whole_in_their_views_with_reserved_keys_only_when_allowed() {
        let s = spec();
        assert!(s.parses("size 4x2", "frame", false));
        assert!(s.parses("attr 0 1-2 bold", "screen", false));
        assert!(
            !s.parses("attr 0 1-2 bold", "frame", false),
            "a screen line"
        );
        assert!(
            !s.parses("attr 0 1-2 bold x", "screen", false),
            "matched whole"
        );
        assert!(!s.parses("attr 0 1-2 bold overline", "screen", false));
        assert!(s.parses("attr 0 1-2 bold overline", "screen", true));
        assert!(
            s.parses("reply 1b5b", "screen", false),
            "a count stays a count"
        );
        assert!(!s.parses("reply 1b5", "screen", false));
        assert!(!s.parses("nothing", "screen", false));
    }

    #[test]
    fn a_fragment_the_grammar_lacks_is_refused() {
        let bad = SPEC.replace("{N}-{N}", "{N}-{M}");
        assert!(Spec::read(&bad).is_err());
    }
}
