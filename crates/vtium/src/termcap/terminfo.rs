//! The terminfo source format (terminfo(5)), read as far as vtium's entry
//! needs, and the table generated from it.

#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test-only: the entry is vtium's own, and a test fails by panicking"
)]

/// One capability: a boolean, a number or a string.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Cap {
    Bool,
    Num(u32),
    Str(Vec<u8>),
}

/// The fields of an entry, split at each comma no backslash escapes.
fn fields(src: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = src
        .lines()
        .filter(|l| !l.starts_with('#'))
        .flat_map(|l| l.chars().chain(['\n']))
        .collect::<Vec<_>>()
        .into_iter();
    while let Some(c) = chars.next() {
        let field = out.last_mut().expect("out starts with a field");
        match c {
            '\\' => {
                field.push(c);
                field.extend(chars.next());
            }
            ',' => out.push(String::new()),
            c => field.push(c),
        }
    }
    out.into_iter()
        .map(|f| f.trim().to_owned())
        .filter(|f| !f.is_empty())
        .collect()
}

/// The entry's primary name and its capabilities in order.
fn parse(src: &str) -> (String, Vec<(String, Cap)>) {
    let mut fields = fields(src).into_iter();
    let names = fields.next().expect("an entry starts with its names");
    let name = names.split('|').next().unwrap_or_default().to_owned();
    let caps = fields
        // `name@` cancels a capability; a string may end in '@'.
        .filter(|f| f.contains('=') || f.contains('#') || !f.ends_with('@'))
        .map(|f| {
            if let Some((n, v)) = f.split_once('=') {
                (n.to_owned(), Cap::Str(decode(v)))
            } else if let Some((n, v)) = f.split_once('#') {
                let num = match v.strip_prefix("0x") {
                    Some(hex) => u32::from_str_radix(hex, 16),
                    None if v.len() > 1 && v.starts_with('0') => u32::from_str_radix(v, 8),
                    None => v.parse(),
                };
                (n.to_owned(), Cap::Num(num.expect("a number")))
            } else {
                (f, Cap::Bool)
            }
        })
        .collect();
    (name, caps)
}

/// A string capability's bytes, as `tigetstr` gives them.
fn decode(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut bytes = text.bytes().peekable();
    while let Some(b) = bytes.next() {
        match b {
            b'\\' => {
                let e = bytes.next().expect("an escape ends the string");
                out.push(match e {
                    b'E' | b'e' => 0x1b,
                    b'n' | b'l' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'f' => 0x0c,
                    b's' => b' ',
                    b'a' => 0x07,
                    b'^' | b'\\' | b',' | b':' => e,
                    b'0'..=b'7' => {
                        let mut n = u32::from(e - b'0');
                        for _ in 0..2 {
                            match bytes.peek() {
                                Some(&d @ b'0'..=b'7') => {
                                    n = n * 8 + u32::from(d - b'0');
                                    bytes.next();
                                }
                                _ => break,
                            }
                        }
                        // NUL cannot be stored, so tic stores it as 0x80.
                        match u8::try_from(n).expect("an octal byte") {
                            0 => 0x80,
                            n => n,
                        }
                    }
                    e => panic!("unknown escape \\{}", char::from(e)),
                });
            }
            b'^' => {
                let c = bytes.next().expect("a control ends the string");
                out.push(if c == b'?' { 0x7f } else { c & 0x1f });
            }
            b => out.push(b),
        }
    }
    out
}

/// The XTGETTCAP table: every capability but `cols` and `lines`, and `TN`,
/// `name` and `Co`, sorted by name.
fn table(src: &str) -> Vec<(String, Option<Vec<u8>>)> {
    let (term, caps) = parse(src);
    let mut out: Vec<(String, Option<Vec<u8>>)> = caps
        .into_iter()
        .filter(|(n, _)| n != "cols" && n != "lines")
        .map(|(n, c)| {
            let value = match c {
                Cap::Bool => None,
                Cap::Num(v) => Some(v.to_string().into_bytes()),
                Cap::Str(v) => Some(v),
            };
            (n, value)
        })
        .collect();
    let colors = out
        .iter()
        .find(|(n, _)| n == "colors")
        .map(|(_, v)| v.clone());
    out.push(("TN".into(), Some(term.clone().into_bytes())));
    out.push(("name".into(), Some(term.into_bytes())));
    if let Some(colors) = colors {
        out.push(("Co".into(), colors));
    }
    out.sort();
    out
}

/// A byte string literal holding `bytes`.
fn literal(bytes: &[u8]) -> String {
    let mut out = String::from("b\"");
    for &b in bytes {
        match b {
            b'"' | b'\\' => {
                out.push('\\');
                out.push(char::from(b));
            }
            0x20..=0x7e => out.push(char::from(b)),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push('"');
    out
}

/// `table.rs`.
fn render(table: &[(String, Option<Vec<u8>>)]) -> String {
    let mut out = String::from(
        "//! Generated from install/xterm-vtium.ti by the test `table_matches_the_entry`;\n\
         //! do not edit. `VTIUM_BLESS=1 cargo test -p vtium termcap` rewrites it.\n\
         \n\
         #[rustfmt::skip]\n\
         pub(super) const TABLE: &[(&str, Option<&[u8]>)] = &[\n",
    );
    for (name, value) in table {
        let value = match value {
            Some(v) => format!("Some({})", literal(v)),
            None => "None".to_owned(),
        };
        out.push_str(&format!("    (\"{name}\", {value}),\n"));
    }
    out.push_str("];\n");
    out
}

const ENTRY: &str = include_str!("../../install/xterm-vtium.ti");

#[test]
fn escapes_decode_to_their_bytes() {
    let cases: [(&str, &[u8]); 12] = [
        (r"\E[H\e[2J", b"\x1b[H\x1b[2J"),
        ("^G^I^H^?^[", b"\x07\x09\x08\x7f\x1b"),
        (r"\r\n\l\t\b\f\s", b"\r\n\n\t\x08\x0c "),
        (r"\^\\\,\:", b"^\\,:"),
        (r"\0", b"\x80"),
        (r"\000\177\033", b"\x80\x7f\x1b"),
        (r"\E[?5h$<200/>\E[?5l", b"\x1b[?5h$<200/>\x1b[?5l"),
        ("%p1%d;%p2%dH", b"%p1%d;%p2%dH"),
        (r"\E[?%[;0123456789]c", b"\x1b[?%[;0123456789]c"),
        ("%p1%c\\E[%p2%{1}%-%db", b"%p1%c\x1b[%p2%{1}%-%db"),
        (r"\a", b"\x07"),
        ("plain", b"plain"),
    ];
    for (text, want) in cases {
        assert_eq!(decode(text), want, "{text}");
    }
}

#[test]
fn the_entry_reads() {
    let (name, caps) = parse(ENTRY);
    assert_eq!(name, super::NAME);
    let get = |n: &str| caps.iter().find(|(c, _)| c == n).map(|(_, v)| v.clone());
    assert_eq!(get("am"), Some(Cap::Bool));
    assert_eq!(get("colors"), Some(Cap::Num(256)));
    assert_eq!(get("pairs"), Some(Cap::Num(32767)));
    assert_eq!(get("kbs"), Some(Cap::Str(b"\x7f".to_vec())));
    assert_eq!(get("smcup"), Some(Cap::Str(b"\x1b[?1049h".to_vec())));
    assert_eq!(
        get("acsc"),
        Some(Cap::Str(
            b"``aaffggiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~".to_vec()
        ))
    );
    // OSC 52, as xterm's entry and tmux name it.
    assert_eq!(
        get("Ms"),
        Some(Cap::Str(b"\x1b]52;%p1%s;%p2%s\x07".to_vec()))
    );
    // 182 capabilities, each once, as infocmp -x lists the compiled entry.
    assert_eq!(caps.len(), 182);
    let mut names: Vec<_> = caps.iter().map(|(n, _)| n.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 182);
}

#[test]
fn the_table_adds_xterms_names_and_drops_the_size() {
    let t = table(ENTRY);
    let get = |n: &str| t.iter().find(|(c, _)| c == n).map(|(_, v)| v.clone());
    assert_eq!(get("TN"), Some(Some(b"xterm-vtium".to_vec())));
    assert_eq!(get("name"), Some(Some(b"xterm-vtium".to_vec())));
    assert_eq!(get("Co"), Some(Some(b"256".to_vec())));
    assert_eq!(get("colors"), Some(Some(b"256".to_vec())));
    assert_eq!(get("Tc"), Some(None));
    assert_eq!(get("cols"), None);
    assert_eq!(get("lines"), None);
    assert_eq!(t.len(), 182 - 2 + 3);
    assert!(t.windows(2).all(|w| w[0].0 < w[1].0), "sorted");
}

// covers: platform.terminfo
#[test]
fn table_matches_the_entry() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/termcap/table.rs");
    let want = render(&table(ENTRY));
    let have = std::fs::read_to_string(path).unwrap_or_default();
    if have == want {
        return;
    }
    if std::env::var_os("VTIUM_BLESS").is_some() {
        std::fs::write(path, want).expect("rewrite table.rs");
        return;
    }
    panic!(
        "src/termcap/table.rs differs from install/xterm-vtium.ti; \
         rerun with VTIUM_BLESS=1 to rewrite it"
    );
}

#[test]
fn the_terminal_takes_the_table() {
    let t = super::termcap().expect("vtium-term's bounds hold the table");
    assert_ne!(t, vtium_term::Termcap::default());
}

/// A capability's value as the table holds it.
fn value(cap: &Cap) -> Option<Vec<u8>> {
    match cap {
        Cap::Bool => None,
        Cap::Num(v) => Some(v.to_string().into_bytes()),
        Cap::Str(v) => Some(v.clone()),
    }
}

// The compiled table and the entry hold the same capabilities with the same
// values, read each way: every name the table answers is the entry's, or
// one of xterm's three, and every capability of the entry but the size is
// in the table.
// covers: platform.terminfo
#[test]
fn the_table_and_the_entry_agree() {
    let (term, caps) = parse(ENTRY);
    let entry = |name: &str| caps.iter().find(|(n, _)| n == name).map(|(_, c)| value(c));
    for (name, have) in super::table::TABLE {
        let want = match *name {
            "TN" | "name" => Some(term.clone().into_bytes()),
            "Co" => entry("colors").expect("the entry has colors"),
            n => entry(n).unwrap_or_else(|| {
                panic!("the table answers {n}, which install/xterm-vtium.ti lacks")
            }),
        };
        assert_eq!(have.map(<[u8]>::to_vec), want, "{name}");
    }
    for (name, cap) in &caps {
        let have = super::table::TABLE.iter().find(|(n, _)| n == name);
        if name == "cols" || name == "lines" {
            assert_eq!(have, None, "{name} gives a default size, never asked");
            continue;
        }
        let (_, have) =
            have.unwrap_or_else(|| panic!("install/xterm-vtium.ti has {name}; the table lacks it"));
        assert_eq!(have.map(<[u8]>::to_vec), value(cap), "{name}");
    }
}

// Each mode the entry sets and resets is one vtium-term sets and resets, as
// DECRQM reports it.
// covers: platform.terminfo
#[test]
fn the_entrys_modes_are_vtiums() {
    let (_, caps) = parse(ENTRY);
    let string = |name: &str| match caps.iter().find(|(n, _)| n == name) {
        Some((_, Cap::Str(v))) => v.clone(),
        other => panic!("{name}: {other:?}"),
    };
    // The capability that sets the mode, the one that resets it, and the
    // mode as DECRQM names it.
    let modes = [
        ("smam", "rmam", "?7"),
        ("smir", "rmir", "4"),
        ("cnorm", "civis", "?25"),
        ("smcup", "rmcup", "?1049"),
        ("fe", "fd", "?1004"),
        ("BE", "BD", "?2004"),
    ];
    for (set, reset, mode) in modes {
        let mut t = vtium_term::Terminal::new(80, 24, 0);
        for (cap, state) in [(reset, 2), (set, 1)] {
            t.input(&string(cap));
            t.input(format!("\x1b[{mode}$p").as_bytes());
            assert_eq!(
                String::from_utf8_lossy(&t.take_replies()),
                format!("\x1b[{mode};{state}$y"),
                "{cap}"
            );
        }
    }
}

// A terminal answers XTGETTCAP from the table only while its session's TERM
// is vtium's entry; under kmscon, or any other name, it answers nothing.
// covers: query.xtgettcap
#[test]
fn only_vtiums_own_name_gets_the_table() {
    let ask = |table: Option<vtium_term::Termcap>| {
        let mut t = vtium_term::Terminal::new(80, 24, 0);
        if let Some(table) = table {
            t.set_termcap(table);
        }
        // TN, in hex
        t.input(b"\x1bP+q544E\x1b\\");
        t.take_replies()
    };
    let table = super::for_term(super::NAME).map(|t| t.expect("the table holds"));
    // TN=xterm-vtium
    assert_eq!(ask(table), b"\x1bP1+r544E=787465726D2D767469756D\x1b\\");
    for other in ["kmscon", "xterm-kitty", "xterm", "xterm-vtium ", ""] {
        let table = super::for_term(other).map(|t| t.expect("the table holds"));
        assert!(table.is_none(), "{other:?}");
        assert_eq!(ask(table), b"\x1bP0+r\x1b\\", "{other:?}");
    }
}
