//! The string syntax that scenarios, the probe files and the interface's
//! worked examples share, the escaping a transcript writes bytes with, and
//! the splitting of a scenario line into words.

/// The bytes of a `"..."` string, or None when it is not one. Inside the
/// quotes, `\e`, `\a`, `\b`, `\t`, `\n`, `\r`, `\\` and `\"` stand for one
/// byte each, `\xHH` for the byte HH, `\u{HEX}` for the UTF-8 of the code
/// point HEX, and every other character for its UTF-8.
pub fn unquote(text: &str) -> Option<Vec<u8>> {
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = Vec::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return None,
            '\\' => match chars.next()? {
                'e' => out.push(0x1b),
                'a' => out.push(0x07),
                'b' => out.push(0x08),
                't' => out.push(b'\t'),
                'n' => out.push(b'\n'),
                'r' => out.push(b'\r'),
                '\\' => out.push(b'\\'),
                '"' => out.push(b'"'),
                'x' => {
                    let hex: String = [chars.next()?, chars.next()?].iter().collect();
                    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return None;
                    }
                    out.push(u8::from_str_radix(&hex, 16).ok()?);
                }
                'u' => {
                    if chars.next()? != '{' {
                        return None;
                    }
                    let mut hex = String::new();
                    loop {
                        match chars.next()? {
                            '}' => break,
                            h if h.is_ascii_hexdigit() && hex.len() < 6 => hex.push(h),
                            _ => return None,
                        }
                    }
                    let c = char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?;
                    let mut buf = [0; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                _ => return None,
            },
            c => {
                let mut buf = [0; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    Some(out)
}

/// `data` written as a transcript writes it, inside quotes: printable
/// ASCII as itself, `\e`, `\a`, `\b`, `\t`, `\n`, `\r`, `\\` and `\"` for
/// those bytes, and `\xhh` for every other byte. [`unquote`] reads it back.
pub fn quoted(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().saturating_add(2));
    out.push('"');
    for &b in data {
        match b {
            0x1b => out.push_str("\\e"),
            0x07 => out.push_str("\\a"),
            0x08 => out.push_str("\\b"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            0x20..=0x7e => out.push(char::from(b)),
            b => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push('"');
    out
}

/// The words of a scenario line, each as written: a word ends at a space
/// outside a string, a string runs from a `"` to the next `"` that no
/// backslash escapes, and a `#` that starts a word starts a comment that
/// runs to the end of the line.
pub fn words(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
        match chars.peek() {
            None | Some('#') => return Ok(out),
            Some(_) => {}
        }
        let mut word = String::new();
        let mut in_string = false;
        while let Some(&c) = chars.peek() {
            if !in_string && (c == ' ' || c == '\t') {
                break;
            }
            chars.next();
            word.push(c);
            match c {
                '"' => in_string = !in_string,
                '\\' if in_string => {
                    if let Some(next) = chars.next() {
                        word.push(next);
                    }
                }
                _ => {}
            }
        }
        if in_string {
            return Err(format!(
                "The string {word} has no closing quote. A string runs from one \" to the next one that no backslash escapes."
            ));
        }
        out.push(word);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_unquote_with_their_escapes_and_refuse_the_rest() {
        assert_eq!(
            unquote(r#""a\e[\x41\\\"é""#),
            Some(b"a\x1b[A\\\"\xc3\xa9".to_vec())
        );
        assert_eq!(
            unquote(r#""\a\b\t\n\r\u{441}\u{1F600}""#),
            Some(b"\x07\x08\t\n\r\xd1\x81\xf0\x9f\x98\x80".to_vec())
        );
        assert_eq!(unquote(r#""a"b""#), None, "a bare quote");
        assert_eq!(unquote(r#""\q""#), None, "an unknown escape");
        assert_eq!(unquote(r#""\x4""#), None, "one hex digit");
        assert_eq!(unquote(r#""\x4g""#), None, "a digit that is not hex");
        assert_eq!(unquote(r#""\u{110000}""#), None, "past the last code point");
        assert_eq!(unquote(r#""\u{d800}""#), None, "a surrogate");
        assert_eq!(unquote(r#""\u{}""#), None, "no digits");
        assert_eq!(unquote(r#""\u41""#), None, "no braces");
        assert_eq!(unquote(r#""\""#), None, "a trailing backslash");
        assert_eq!(unquote("abc"), None, "no quotes");
    }

    #[test]
    fn a_quoted_string_reads_back_as_its_bytes() {
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(unquote(&quoted(&all)), Some(all));
        assert_eq!(quoted(b"\x1b[?1;2c\x07\xc3"), r#""\e[?1;2c\a\xc3""#);
    }

    #[test]
    fn words_keep_strings_whole_and_stop_at_a_comment() {
        assert_eq!(
            words(r#"send "a b # c" # a comment"#),
            Ok(vec!["send".to_owned(), r#""a b # c""#.to_owned()])
        );
        assert_eq!(
            words(r#"key a  char="\" x"  mods=alt"#),
            Ok(vec![
                "key".to_owned(),
                "a".to_owned(),
                r#"char="\" x""#.to_owned(),
                "mods=alt".to_owned()
            ])
        );
        assert_eq!(
            words("want fg=#010203"),
            Ok(vec!["want".to_owned(), "fg=#010203".to_owned()])
        );
        assert_eq!(words("# all a comment"), Ok(Vec::new()));
        assert!(words(r#"send "open"#).is_err());
    }
}
