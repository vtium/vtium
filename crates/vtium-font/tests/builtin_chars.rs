//! vtium's own text uses only the characters of its built-in font.
//!
//! The test reads the string and character literals of the source files
//! that hold vtium's messages: every file under `crates/vtium/src` and
//! `crates/vtium-settings/src`, and `crates/vtium-render/src/notice.rs`.
//! It fails on a literal that holds a character the built-in font's list
//! lacks. Test code is left out: a file named `tests.rs`, and the item
//! that follows `#[cfg(test)]` or `#[test]`. Comments are left out too,
//! since vtium never shows them, and so are the few literals in
//! [`NOT_SHOWN`], which vtium compares text with and never draws.
//!
//! References:
//! - ADR-1204 D1: the built-in font holds the characters of vtium's own
//!   messages, and this scan checks them.

use std::path::{Path, PathBuf};

use vtium_font::set::own::holds;

/// A token of Rust source: a literal's text and the line it starts on,
/// a punctuation character, or a word.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Text(String, usize),
    Punct(char),
    Word(String),
}

/// The tokens of Rust source `src`, as far as this scan needs them:
/// string and character literals with their escapes resolved, words, and
/// single punctuation characters. Comments and whitespace are dropped, and
/// a lifetime is a word.
fn tokens(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line) = (0, 1);
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            '/' if at(i + 1) == '/' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if at(i + 1) == '*' => {
                let mut depth = 0;
                while i < chars.len() {
                    if chars[i] == '/' && at(i + 1) == '*' {
                        depth += 1;
                        i += 2;
                    } else if chars[i] == '*' && at(i + 1) == '/' {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        line += usize::from(chars[i] == '\n');
                        i += 1;
                    }
                }
            }
            '"' => {
                let start = line;
                let (text, next) = quoted(&chars, i + 1, '"', &mut line);
                out.push(Token::Text(text, start));
                i = next;
            }
            'r' | 'b' if raw_start(&chars, i).is_some() => {
                let (hashes, open) = raw_start(&chars, i).unwrap_or_default();
                let start = line;
                let mut j = open + 1;
                let mut text = String::new();
                loop {
                    if j >= chars.len() {
                        break;
                    }
                    if chars[j] == '"' && (1..=hashes).all(|k| at(j + k) == '#') {
                        j += 1 + hashes;
                        break;
                    }
                    line += usize::from(chars[j] == '\n');
                    text.push(chars[j]);
                    j += 1;
                }
                out.push(Token::Text(text, start));
                i = j;
            }
            'b' if at(i + 1) == '"' || at(i + 1) == '\'' => {
                // a byte string or byte is ASCII, so it is left out
                let quote = at(i + 1);
                let (_, next) = quoted(&chars, i + 2, quote, &mut line);
                i = next;
            }
            '\'' => {
                let is_char = at(i + 1) == '\\' || at(i + 2) == '\'';
                if is_char {
                    let start = line;
                    let (text, next) = quoted(&chars, i + 1, '\'', &mut line);
                    out.push(Token::Text(text, start));
                    i = next;
                } else {
                    i += 1;
                }
            }
            c if c.is_alphanumeric() || c == '_' => {
                let mut word = String::new();
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    word.push(chars[i]);
                    i += 1;
                }
                out.push(Token::Word(word));
            }
            c if c.is_whitespace() => i += 1,
            c => {
                out.push(Token::Punct(c));
                i += 1;
            }
        }
    }
    out
}

/// Where a raw string starts at `i`, `r"`, `r#"`, `br"` and so on: its
/// count of `#` and the index of its opening quote.
fn raw_start(chars: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    // a word that only ends in r, such as `bar"`, is no raw string
    if i > 0
        && chars
            .get(i - 1)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
    {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    (chars.get(j) == Some(&'"')).then_some((hashes, j))
}

/// The text of a literal that starts at `i` and ends at `quote`, with its
/// escapes resolved, and the index after its end. `line` counts the lines
/// it spans.
fn quoted(chars: &[char], mut i: usize, quote: char, line: &mut usize) -> (String, usize) {
    let mut text = String::new();
    while let Some(&c) = chars.get(i) {
        i += 1;
        match c {
            c if c == quote => break,
            '\\' => {
                let e = chars.get(i).copied().unwrap_or('\0');
                i += 1;
                match e {
                    'n' => text.push('\n'),
                    't' => text.push('\t'),
                    'r' => text.push('\r'),
                    '0' => text.push('\0'),
                    'x' => {
                        let hex: String = chars[i..i + 2].iter().collect();
                        text.push(char::from(u8::from_str_radix(&hex, 16).unwrap_or(0)));
                        i += 2;
                    }
                    'u' => {
                        let end = i + chars[i..].iter().position(|&c| c == '}').unwrap_or(0);
                        let hex: String = chars[i + 1..end].iter().collect();
                        let cp = u32::from_str_radix(&hex, 16).unwrap_or(0);
                        text.push(char::from_u32(cp).unwrap_or('\0'));
                        i = end + 1;
                    }
                    '\n' => {
                        *line += 1;
                        while chars.get(i).is_some_and(|c| c.is_whitespace()) {
                            *line += usize::from(chars[i] == '\n');
                            i += 1;
                        }
                    }
                    e => text.push(e),
                }
            }
            c => {
                *line += usize::from(c == '\n');
                text.push(c);
            }
        }
    }
    (text, i)
}

/// `tokens` less test code: the item after each `#[cfg(test)]` and
/// `#[test]`, up to the `;` or the closing brace that ends it.
fn untested(tokens: Vec<Token>) -> Vec<Token> {
    let punct = |t: Option<&Token>, c: char| t == Some(&Token::Punct(c));
    let word = |t: Option<&Token>, w: &str| matches!(t, Some(Token::Word(x)) if x == w);
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = |k: usize| tokens.get(i + k);
        let cfg_test = punct(t(0), '#')
            && punct(t(1), '[')
            && word(t(2), "cfg")
            && punct(t(3), '(')
            && word(t(4), "test")
            && punct(t(5), ')')
            && punct(t(6), ']');
        let test = punct(t(0), '#') && punct(t(1), '[') && word(t(2), "test") && punct(t(3), ']');
        if !(cfg_test || test) {
            out.push(tokens[i].clone());
            i += 1;
            continue;
        }
        let mut depth = 0;
        while let Some(t) = tokens.get(i) {
            i += 1;
            match t {
                Token::Punct('{') => depth += 1,
                Token::Punct('}') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Token::Punct(';') if depth == 0 => break,
                _ => {}
            }
        }
    }
    out
}

/// The characters of literals that vtium never draws on the console, by
/// the file that holds them, each with the reason.
const NOT_SHOWN: [(&str, char, &str); 6] = [
    (
        "vtium/src/deferred.rs",
        '\u{200d}',
        "the zero width joiner that joins emoji",
    ),
    (
        "vtium/src/deferred/joined.rs",
        '\u{200d}',
        "the zero width joiner",
    ),
    (
        "vtium/src/deferred/joined.rs",
        '\u{644}',
        "lam, which joins with alef",
    ),
    (
        "vtium-settings/src/conf.rs",
        '\u{feff}',
        "a byte-order mark vtium.conf may start with",
    ),
    (
        "vtium-settings/src/write/sections.rs",
        '\u{feff}',
        "the same byte-order mark",
    ),
    // the key table's help, which vtium-config and the installed
    // vtium.conf.example show in a terminal, never the console
    (
        "vtium-settings/src/keys.rs",
        '\u{25a1}',
        "the help of `missing`",
    ),
];

/// The key table's help names a Tamil syllable to show `cluster-width`.
const TAMIL: [char; 2] = ['\u{b95}', '\u{bca}'];

/// Whether `c` in the file `path` is one vtium never draws.
fn not_shown(path: &Path, c: char) -> bool {
    let tamil = path.ends_with("vtium-settings/src/keys.rs") && TAMIL.contains(&c);
    tamil
        || NOT_SHOWN
            .iter()
            .any(|&(f, n, _)| n == c && path.ends_with(f))
}

/// Every `.rs` file under `dir`, but those named `tests.rs`.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && path.file_name().is_some_and(|n| n != "tests.rs")
        {
            out.push(path);
        }
    }
}

/// Each character outside the built-in font's list in the literals of
/// `src`, with its line.
fn outside(src: &str) -> Vec<(usize, char)> {
    untested(tokens(src))
        .into_iter()
        .filter_map(|t| match t {
            Token::Text(text, line) => Some((text, line)),
            _ => None,
        })
        .flat_map(|(text, line)| {
            text.chars()
                // control characters are layout, such as a line break
                .filter(|c| !c.is_control() && !holds(*c))
                .map(move |c| (line, c))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn vtiums_own_text_uses_only_the_built_in_fonts_characters() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    sources(&crates.join("vtium/src"), &mut files);
    sources(&crates.join("vtium-settings/src"), &mut files);
    files.push(crates.join("vtium-render/src/notice.rs"));
    assert!(files.len() > 50, "the scan found {} files", files.len());
    let mut found = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).unwrap();
        for (line, c) in outside(&src)
            .into_iter()
            .filter(|&(_, c)| !not_shown(path, c))
        {
            found.push(format!(
                "{}:{line}: U+{:04X} {c}",
                path.display(),
                u32::from(c)
            ));
        }
    }
    assert!(
        found.is_empty(),
        "vtium's own text holds characters its built-in font lacks:\n{}",
        found.join("\n")
    );
}

/// The scan reads each kind of literal, resolves its escapes, and leaves
/// out comments and test code.
#[test]
fn the_scan_reads_literals_and_leaves_out_comments_and_tests() {
    let src = r##"
        // a comment: 中
        /* a block: 中 /* nested 中 */ */
        fn shown() -> &'static str { "a ⇧ b" }
        const C: char = '\u{21e7}';
        const R: &str = r#"raw ⇩"#;
        const OK: &str = "\u{25cf} · …";
        #[test]
        fn a_test() { let _ = "中"; }
        #[cfg(test)]
        mod tests { fn f() { let _ = "中"; } }
        const LAST: &str = "⌘";
    "##;
    let found: Vec<char> = outside(src).into_iter().map(|(_, c)| c).collect();
    assert_eq!(found, ['⇧', '\u{21e7}', '⇩', '⌘']);
}
