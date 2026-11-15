//! The bytes vtium-clipboard writes for a copy, what vtium-term makes of
//! them, and how the program reads the replies.
//!
//! The expected base64 strings were computed with coreutils' base64(1),
//! not with the program's own encoder.
//!
//! References:
//! - ADR-1007 D4: the inputs the bytes test covers, and the tier 1
//!   scenario that feeds the same bytes to vtium.

use vtium_clipboard::{
    Answers, CLIPBOARD_MAX, QUERIES, Text, answers, kept_len, names_vtium, read_text, sequence,
};
use vtium_term::Terminal;

/// Each text and the base64 the sequence carries for it. All but the empty
/// text are also in tier 1's scenario.
const TEXTS: &[(&[u8], &str)] = &[
    (b"a", "YQ=="),
    (b"ab", "YWI="),
    (b"abc", "YWJj"),
    (
        b"caf\xc3\xa9 \xe4\xb8\xad\xe6\x96\x87\n",
        "Y2Fmw6kg5Lit5paHCg==",
    ),
    (b"\xc3\xa9\xe4\xb8\xad\xf0\x9f\x98\x80", "w6nkuK3wn5iA"),
    (b"h\xffi", "aP9p"),
];

/// The tier 1 scenario that feeds these bytes to vtium.
const SCENARIO: &str =
    include_str!("../../../tools/conformance/scenarios/vtium-clipboard-copy.scenario");

/// `ESC ] 52 ; c ; BASE64 ESC \`, written out.
fn osc52(base64: &str) -> Vec<u8> {
    format!("\x1b]52;c;{base64}\x1b\\").into_bytes()
}

/// The text of `CLIPBOARD_MAX` bytes the tests copy: the letter a, over
/// and over.
fn largest() -> Vec<u8> {
    vec![b'a'; CLIPBOARD_MAX]
}

/// A terminal as vtium's console sets it up, at the replay's size.
fn vtium() -> Terminal {
    Terminal::new(8, 1, 0)
}

// RFC 4648 section 4 and ctlseqs OSC 52: the sequence carries the text in
// padded base64 for the clipboard target c, and ends with ST.
// covers: clipboard.copy-tool
#[test]
fn copy_writes_osc_52_with_padded_base64() {
    assert_eq!(sequence(b""), b"\x1b]52;c;\x1b\\");
    for (text, base64) in TEXTS {
        assert_eq!(sequence(text), osc52(base64), "{text:?}");
    }
    // 524288 is 3 times 174762, and 2 over: groups of aaa, then aa padded
    let mut want = "YWFh".repeat(174_762);
    want.push_str("YWE=");
    assert_eq!(sequence(&largest()), osc52(&want));
    // the questions follow the copy: XTVERSION, then DA1
    assert_eq!(QUERIES, b"\x1b[>0q\x1b[c");
}

// ADR-1007 D2: text up to CLIPBOARD_MAX bytes is read whole, and one byte
// more is refused.
#[test]
fn text_past_the_clipboard_bound_is_refused() {
    assert_eq!(read_text(&b""[..]).unwrap(), Text::Fits(Vec::new()));
    let most = largest();
    assert_eq!(read_text(&most[..]).unwrap(), Text::Fits(most.clone()));
    let mut over = most;
    over.push(b'a');
    assert_eq!(read_text(&over[..]).unwrap(), Text::TooLarge);
    // a source far larger is read only to the bound and one byte more
    let endless = std::io::repeat(b'a');
    assert_eq!(read_text(endless).unwrap(), Text::TooLarge);
}

/// What vtium's clipboard holds after the program's bytes for `text`, or
/// `None` when vtium keeps nothing.
fn vtium_keeps(text: &[u8]) -> Option<Vec<u8>> {
    let mut t = vtium();
    let mut bytes = sequence(text);
    bytes.extend_from_slice(QUERIES);
    t.input(&bytes);
    t.take_clipboard_write()
}

// Unicode 16.0, 3.9, "U+FFFD Substitution of Maximal Subparts": vtium
// puts one U+FFFD, 3 bytes, in place of each maximal subpart that is not
// valid UTF-8, and drops the copy when the text then passes the bound.
// The program counts the same length, and refuses exactly the texts that
// vtium would drop, though each fits the bound as it is.
#[test]
fn text_past_the_bound_once_replaced_is_refused() {
    // a byte that never starts UTF-8, a 3-byte sequence cut off after 2
    // bytes, and a 4-byte sequence cut off after 3 bytes each become one
    // U+FFFD; the valid text around them keeps its length
    assert_eq!(kept_len(b""), 0);
    assert_eq!(kept_len(b"caf\xc3\xa9"), 5);
    assert_eq!(kept_len(b"h\xffi"), 5);
    assert_eq!(kept_len(b"\xe4\xb8x"), 4);
    assert_eq!(kept_len(b"\xf0\x9f\x98"), 3);
    assert_eq!(kept_len(b"\xff\xff"), 6);
    // 174762 bytes of 0xff are 524286 bytes once replaced, and fit; one
    // more is 524289 bytes, and vtium drops it
    let fits = vec![0xff; 174_762];
    let over = vec![0xff; 174_763];
    assert!(matches!(read_text(&fits[..]).unwrap(), Text::Fits(_)));
    assert_eq!(vtium_keeps(&fits).map(|k| k.len()), Some(524_286));
    assert_eq!(
        read_text(&over[..]).unwrap(),
        Text::TooLargeReplaced { kept: 524_289 }
    );
    assert_eq!(vtium_keeps(&over), None);
    // a cut-off sequence at the end of text at the bound: exactly the
    // bound once replaced fits, and one byte more does not
    let mut at = vec![b'a'; CLIPBOARD_MAX - 3];
    at.extend_from_slice(b"\xe4\xb8");
    let mut past = vec![b'a'; CLIPBOARD_MAX - 2];
    past.extend_from_slice(b"\xe4\xb8");
    assert_eq!(read_text(&at[..]).unwrap(), Text::Fits(at.clone()));
    assert_eq!(vtium_keeps(&at).map(|k| k.len()), Some(CLIPBOARD_MAX));
    assert_eq!(
        read_text(&past[..]).unwrap(),
        Text::TooLargeReplaced {
            kept: CLIPBOARD_MAX + 1
        }
    );
    assert_eq!(vtium_keeps(&past), None);
}

/// The `send "..."` lines of the scenario, unquoted: `\e` is ESC and `\\`
/// a backslash, as the scenario language reads them, and the program's
/// bytes hold no other escape.
fn scenario_sends() -> Vec<Vec<u8>> {
    SCENARIO
        .lines()
        .filter_map(|l| l.strip_prefix("send \"")?.strip_suffix('"'))
        .map(|body| {
            let mut out = Vec::new();
            let mut chars = body.chars();
            while let Some(c) = chars.next() {
                match (c, c == '\\') {
                    (_, true) => match chars.next() {
                        Some('e') => out.push(0x1b),
                        Some('\\') => out.push(b'\\'),
                        other => panic!("the scenario holds the escape \\{other:?}"),
                    },
                    (c, false) => out.push(u8::try_from(c).expect("ASCII")),
                }
            }
            out
        })
        .collect()
}

// ADR-1007 D4: tier 1's scenario feeds vtium the exact bytes the program
// writes for each text, the copy and the two questions, in the same order.
#[test]
fn tier_1_feeds_vtium_the_bytes_the_program_writes() {
    let written: Vec<Vec<u8>> = TEXTS
        .iter()
        .map(|(text, _)| {
            let mut bytes = sequence(text);
            bytes.extend_from_slice(QUERIES);
            bytes
        })
        .collect();
    assert_eq!(scenario_sends(), written);
}

// ADR-1007 D4, the bytes in vtium: each copy sets the clipboard to the
// text, U+FFFD in place of each part that is not UTF-8, and the replies
// name vtium and end with DA1. The empty text sets an empty clipboard, and
// the largest text sets all of it.
// covers: clipboard.copy-tool
#[test]
fn vtium_takes_each_copy_and_answers_as_vtium() {
    let mut texts: Vec<(Vec<u8>, Vec<u8>)> = TEXTS
        .iter()
        .map(|(text, _)| {
            (
                text.to_vec(),
                String::from_utf8_lossy(text).into_owned().into_bytes(),
            )
        })
        .collect();
    texts.push((Vec::new(), Vec::new()));
    texts.push((largest(), largest()));
    assert_eq!(texts[5].1, b"h\xef\xbf\xbdi");
    for (text, kept) in texts {
        let mut t = vtium();
        let mut bytes = sequence(&text);
        bytes.extend_from_slice(QUERIES);
        t.input(&bytes);
        assert_eq!(t.take_clipboard_write(), Some(kept), "{} bytes", text.len());
        let replies = t.take_replies();
        let read = answers(&replies);
        assert!(read.da1, "{replies:?}");
        assert!(
            read.version.as_deref().is_some_and(names_vtium),
            "{replies:?}"
        );
    }
}

// ADR-1007 D2: a terminal that answers XTVERSION with another name, as
// tmux does, is not vtium, though its DA1 ends the replies.
#[test]
fn a_terminal_with_another_name_is_not_vtium() {
    let mut t = vtium();
    t.set_version("tmux 3.5a").expect("printable");
    let mut bytes = sequence(b"x");
    bytes.extend_from_slice(QUERIES);
    t.input(&bytes);
    let read = answers(&t.take_replies());
    assert!(read.da1);
    assert_eq!(read.version.as_deref(), Some(&b"tmux 3.5a"[..]));
    assert!(!names_vtium(b"tmux 3.5a"));
}

// The replies as they come: a reply cut partway counts for nothing yet,
// keys typed meanwhile are skipped, and a terminal that answers DA1 alone
// has no name.
#[test]
fn the_replies_are_read_as_far_as_they_have_come() {
    let none = Answers {
        version: None,
        da1: false,
    };
    assert_eq!(answers(b""), none);
    assert_eq!(answers(b"\x1bP>|vtium(0.1"), none);
    assert_eq!(answers(b"\x1b[?62;22"), none);
    assert_eq!(
        answers(b"ls\x1bP>|vtium(0.1)\x1b\\x\x1b[?62;4;22c"),
        Answers {
            version: Some(b"vtium(0.1)".to_vec()),
            da1: true,
        }
    );
    assert_eq!(
        answers(b"\x1b[?1;2c"),
        Answers {
            version: None,
            da1: true,
        }
    );
    // a key's CSI before the reply is not DA1
    assert!(answers(b"\x1b[?1h\x1b[?6c").da1);
    assert!(!answers(b"\x1b[?1h").da1);
    for (text, vtium) in [
        (&b"vtium"[..], true),
        (b"vtium(0.1.0-alpha.8)", true),
        (b"vtium 1.0", true),
        (b"vtium-dev(1)", false),
        (b"xterm(390)", false),
        (b"", false),
    ] {
        assert_eq!(names_vtium(text), vtium, "{text:?}");
    }
}
