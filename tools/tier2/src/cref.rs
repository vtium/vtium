//! The C reference's screens, read from the copy of its
//! `tests/test_pixels.c` in `reference/`, so every screen ported from one is
//! checked to carry its bytes.
//!
//! The reader knows the subset of C that file writes its screens in:
//! string literals, object-like `#define`s whose bodies are strings and
//! other such macros, and the `screens[]` array, whose entries give the
//! name, caption, bytes and second part by position or as `.name =`,
//! `.bytes =` and `.next =`. An entry it cannot expand, such as one using a
//! function-like macro, is left out, and a port of it is refused.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Str(Vec<u8>),
    Ident(String),
    Punct(char),
}

/// A C string literal's bytes, its escapes decoded.
fn unescape(lit: &str) -> Result<Vec<u8>, String> {
    let b = lit.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let e = *b.get(i + 1).ok_or("a string ends in a backslash")?;
        i += 2;
        match e {
            b'x' => {
                let n = b[i..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
                let v = u32::from_str_radix(&lit[i..i + n], 16)
                    .map_err(|_| format!("bad \\x escape in {lit:?}"))?;
                out.push((v & 0xff) as u8);
                i += n;
            }
            b'r' => out.push(b'\r'),
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'0' => out.push(0),
            b'\\' | b'"' | b'\'' => out.push(e),
            _ => return Err(format!("escape \\{} in {lit:?}", e as char)),
        }
    }
    Ok(out)
}

/// The tokens of `src`, comments and preprocessor lines' line ends dropped.
fn tokens(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() || (c == b'\\' && b.get(i + 1) == Some(&b'\n')) {
            i += 1;
        } else if src[i..].starts_with("//") {
            i += src[i..].find('\n').unwrap_or(src.len() - i);
        } else if src[i..].starts_with("/*") {
            i += src[i..].find("*/").ok_or("an unended comment")? + 2;
        } else if c == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            out.push(Tok::Str(unescape(&src[i + 1..j.min(b.len())])?));
            i = j + 1;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let n = b[i..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
                .count();
            out.push(Tok::Ident(src[i..i + n].to_owned()));
            i += n;
        } else {
            let ch = src[i..].chars().next().unwrap_or(' ');
            out.push(Tok::Punct(ch));
            i += ch.len_utf8();
        }
    }
    Ok(out)
}

/// The object-like macros of `src`, by name: each line `#define NAME body`,
/// with its continuations.
fn defines(src: &str) -> Result<BTreeMap<String, Vec<Tok>>, String> {
    let mut out = BTreeMap::new();
    let mut lines = src.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.trim_start().strip_prefix("#define") else {
            continue;
        };
        let mut text = rest.to_owned();
        while text.ends_with('\\') {
            text.pop();
            text.push(' ');
            text.push_str(lines.next().unwrap_or(""));
        }
        let text = text.trim_start();
        let n = text
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
            .count();
        // a function-like macro's name runs straight into its parenthesis
        if n == 0 || text[n..].starts_with('(') {
            continue;
        }
        out.insert(text[..n].to_owned(), tokens(&text[n..])?);
    }
    Ok(out)
}

fn expand(toks: &[Tok], defs: &BTreeMap<String, Vec<Tok>>, depth: u32) -> Result<Vec<u8>, String> {
    if depth > 16 {
        return Err("macros nest too deep".to_owned());
    }
    let mut out = Vec::new();
    for t in toks {
        match t {
            Tok::Str(s) => out.extend_from_slice(s),
            Tok::Ident(name) => {
                let body = defs
                    .get(name)
                    .ok_or_else(|| format!("{name} is not a string macro"))?;
                out.extend(expand(body, defs, depth + 1)?);
            }
            Tok::Punct(c) => return Err(format!("{c:?} in a string expression")),
        }
    }
    Ok(out)
}

/// A C screen's bytes and its second part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CScreen {
    pub bytes: Vec<u8>,
    pub next: Option<Vec<u8>>,
}

/// The screens of `test_pixels.c`'s text that expand to strings, by name.
pub fn screens(src: &str) -> Result<BTreeMap<String, CScreen>, String> {
    let defs = defines(src)?;
    let start = src
        .find("static const struct screen screens[]")
        .ok_or("test_pixels.c has no screens[] array")?;
    let toks = tokens(&src[start..])?;
    let open = toks
        .iter()
        .position(|t| *t == Tok::Punct('{'))
        .ok_or("screens[] has no initializer")?;
    let mut entries: Vec<Vec<Tok>> = Vec::new();
    let mut depth = 0;
    for t in &toks[open + 1..] {
        match t {
            Tok::Punct('{') => {
                depth += 1;
                if depth == 1 {
                    entries.push(Vec::new());
                    continue;
                }
            }
            Tok::Punct('}') if depth == 0 => break,
            Tok::Punct('}') => {
                depth -= 1;
                if depth == 0 {
                    continue;
                }
            }
            _ => {}
        }
        if depth >= 1 {
            if let Some(e) = entries.last_mut() {
                e.push(t.clone());
            }
        }
    }
    let mut out = BTreeMap::new();
    for e in entries {
        let fields: Vec<&[Tok]> = e.split(|t| *t == Tok::Punct(',')).collect();
        let mut named: BTreeMap<String, &[Tok]> = BTreeMap::new();
        let mut pos: Vec<&[Tok]> = Vec::new();
        for f in fields {
            match f {
                [Tok::Punct('.'), Tok::Ident(k), Tok::Punct('='), rest @ ..] => {
                    named.insert(k.clone(), rest);
                }
                [] => {}
                _ => pos.push(f),
            }
        }
        let get = |i: usize, key: &str| named.get(key).copied().or_else(|| pos.get(i).copied());
        let Some(name) = get(0, "name").and_then(|t| expand(t, &defs, 0).ok()) else {
            continue;
        };
        let name = String::from_utf8_lossy(&name).into_owned();
        let Some(Ok(bytes)) = get(2, "bytes").map(|t| expand(t, &defs, 0)) else {
            continue;
        };
        let next = match get(3, "next") {
            None => None,
            Some([Tok::Ident(n)]) if n == "NULL" => None,
            Some(t) => match expand(t, &defs, 0) {
                Ok(b) => Some(b),
                Err(_) => continue,
            },
        };
        out.insert(name, CScreen { bytes, next });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
#define ESC "\x1b"
#define TWO ESC "[2J" /* a comment */ "x"
#define F(a) a
static const struct screen screens[] = {
	{"plain", "caption // not a comment", "a{b}\"c\r\n" ESC "[0m"},
	/* frames */
	{"framed", "c", TWO, ESC "[H"},
	{.name = "named",
	 .desc = "d",
	 .bytes = "n" TWO,
	 .cols = 72},
	{"fn", "d", F("x")},
	{"null", "d", "q", NULL, &conf},
};
"#;

    #[test]
    fn screens_expand_by_position_and_by_name() {
        let s = screens(SRC).unwrap();
        let get = |n: &str| s.get(n).unwrap().clone();
        assert_eq!(get("plain").bytes, b"a{b}\"c\r\n\x1b[0m");
        assert_eq!(get("framed").bytes, b"\x1b[2Jx");
        assert_eq!(get("framed").next.as_deref(), Some(&b"\x1b[H"[..]));
        assert_eq!(get("named").bytes, b"n\x1b[2Jx");
        assert_eq!(get("null").next, None);
        // a function-like macro is not expanded: the entry is left out
        assert!(!s.contains_key("fn"));
    }
}
