//! vtium.conf's top level read and edited line by line (ADR-1201 D5,
//! ADR-1403 D8): only the lines a change needs move, every other byte
//! stays, and no reader or edit reaches past the first section heading.

use std::path::Path;

use vtium_settings::{ConfEdit, Edit, edit_conf, key_line, setting, split_line, switch};

fn example() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vtium/install/vtium.conf.example");
    std::fs::read_to_string(path).expect("the shipped example")
}

/// The lines of `a` and `b` that differ, by number.
fn changed(a: &str, b: &str) -> Vec<(usize, String, String)> {
    let a: Vec<&str> = a.split_inclusive('\n').collect();
    let b: Vec<&str> = b.split_inclusive('\n').collect();
    (0..a.len().max(b.len()))
        .filter_map(|i| {
            let (x, y) = (a.get(i).copied(), b.get(i).copied());
            (x != y).then(|| {
                (
                    i + 1,
                    x.unwrap_or("").to_owned(),
                    y.unwrap_or("").to_owned(),
                )
            })
        })
        .collect()
}

#[test]
fn the_shipped_example_with_no_change_is_byte_identical() {
    let text = example();
    assert_eq!(edit_conf(&text, &[]).expect("no edit"), text);
}

#[test]
fn a_new_font_size_in_the_shipped_example_adds_one_line_below_its_comment() {
    let text = example();
    let out = edit_conf(&text, &[Edit::Set("font-size", "12")]).expect("the edit");
    let at = text
        .lines()
        .position(|l| l == "#font-size=0")
        .expect("the example comments font-size");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[at + 1], "font-size=12");
    let mut kept = lines.clone();
    kept.remove(at + 1);
    assert_eq!(kept, text.lines().collect::<Vec<_>>());
    assert!(out.ends_with('\n'));
}

#[test]
fn a_set_key_has_its_value_replaced_in_place_with_its_comment_kept() {
    let text = "# mine\nfont-name = monoid   # the desk's\nxkb-layout=de\n";
    let out = edit_conf(text, &[Edit::Set("font-name", "tiny5")]).expect("the edit");
    assert_eq!(
        out,
        "# mine\nfont-name = tiny5   # the desk's\nxkb-layout=de\n"
    );
    assert_eq!(changed(text, &out).len(), 1);
}

#[test]
fn the_line_vtium_takes_the_value_from_is_the_last() {
    let text = "font-size=10\nfont-size=11\n";
    assert_eq!(
        edit_conf(text, &[Edit::Set("font-size", "12")]).expect("the edit"),
        "font-size=10\nfont-size=12\n"
    );
}

#[test]
fn a_key_with_neither_line_nor_comment_goes_at_the_end() {
    assert_eq!(
        edit_conf("bell\n", &[Edit::Set("font-name", "monoid")]).expect("the edit"),
        "bell\nfont-name=monoid\n"
    );
    assert_eq!(
        edit_conf("bell", &[Edit::Set("font-name", "monoid")]).expect("the edit"),
        "bell\nfont-name=monoid\n",
        "a last line without a newline gains one"
    );
    assert_eq!(
        edit_conf("", &[Edit::Set("font-name", "monoid")]).expect("the edit"),
        "font-name=monoid\n"
    );
}

#[test]
fn crlf_files_keep_crlf() {
    assert_eq!(
        edit_conf(
            "bell\r\nfont-size=1\r\n",
            &[Edit::Set("font-size", "2"), Edit::Set("font-name", "x")]
        )
        .expect("the edit"),
        "bell\r\nfont-size=2\r\nfont-name=x\r\n"
    );
}

#[test]
fn unset_comments_out_every_line_that_sets_the_key() {
    let text = "font-size=10\n# a note\n  font-size = 11 # big\n";
    assert_eq!(
        edit_conf(text, &[Edit::Unset("font-size")]).expect("the edit"),
        "#font-size=10\n# a note\n#  font-size = 11 # big\n"
    );
    assert_eq!(
        edit_conf("bell\n", &[Edit::Unset("font-size")]).expect("nothing to unset"),
        "bell\n"
    );
}

#[test]
fn a_switch_is_written_as_vtium_writes_switches() {
    let text = example();
    let out = edit_conf(&text, &[Edit::Switch("allow-font-ops", false)]).expect("the edit");
    let at = text
        .lines()
        .position(|l| l == "#allow-font-ops")
        .expect("the example comments allow-font-ops");
    assert_eq!(out.lines().nth(at + 1), Some("no-allow-font-ops"));
    let back = edit_conf(&out, &[Edit::Switch("allow-font-ops", true)]).expect("the edit");
    assert_eq!(back.lines().nth(at + 1), Some("allow-font-ops"));
    assert_eq!(changed(&out, &back).len(), 1);
    assert_eq!(
        edit_conf("  no-bell   # quiet\n", &[Edit::Switch("bell", true)]).expect("the edit"),
        "  bell   # quiet\n"
    );
}

#[test]
fn a_color_value_s_hash_is_no_comment() {
    let text = "palette-foreground = #ffffff # white\n";
    assert_eq!(
        setting(text, "palette-foreground"),
        Some("#ffffff".to_owned())
    );
    assert_eq!(
        edit_conf(text, &[Edit::Set("palette-foreground", "#000000")]).expect("the edit"),
        "palette-foreground = #000000 # white\n"
    );
}

#[test]
fn setting_reads_the_value_vtium_takes() {
    let text = "font-size=10\n#font-size=3\nfont-size = 11 # x\nbell\nno-bell\n";
    assert_eq!(setting(text, "font-size"), Some("11".to_owned()));
    assert_eq!(setting(text, "font-name"), None);
    assert_eq!(setting(&example(), "font-size"), None);
}

#[test]
fn a_value_vtium_would_read_back_otherwise_is_refused() {
    // a `#` glued inside a value is part of it (ADR-1403 D4), and one
    // after a blank starts a comment
    assert_eq!(
        edit_conf("", &[Edit::Set("font-name", "a#b")]).expect("the edit"),
        "font-name=a#b\n"
    );
    for bad in [
        "a #b",
        "a \\#b",
        "two\nlines",
        " lead",
        "trail ",
        "",
        "nul\0",
    ] {
        match edit_conf("", &[Edit::Set("font-name", bad)]) {
            Err(ConfEdit::BadValue { key, value }) => {
                assert_eq!((key.as_str(), value.as_str()), ("font-name", bad));
            }
            other => panic!("{bad:?}: {other:?}"),
        }
    }
    for bad in ["", "Font", "a=b", "a b", "#x", "no-"] {
        assert!(
            matches!(
                edit_conf("", &[Edit::Set(bad, "1")]),
                Err(ConfEdit::BadKey(_))
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn switch_reads_the_last_line_that_sets_it() {
    assert_eq!(switch("bell\nno-bell # quiet\n", "bell"), Some(false));
    assert_eq!(switch("no-bell\n  bell\n", "bell"), Some(true));
    assert_eq!(switch("#no-bell\n", "bell"), None);
    assert_eq!(switch(&example(), "allow-font-ops"), None);
}

/// A line reads as the console reads it: the key and the value with their
/// blanks dropped, a comment cut, a color's `#` kept, a switch with no
/// value, and a blank or comment line as nothing.
#[test]
fn a_line_splits_into_its_key_and_value_as_the_console_reads_it() {
    let split = |l: &str| {
        split_line(l.as_bytes()).map(|(k, v)| {
            (
                String::from_utf8_lossy(k).into_owned(),
                v.map(|v| String::from_utf8_lossy(v).into_owned()),
            )
        })
    };
    let pair = |k: &str, v: Option<&str>| Some((k.to_owned(), v.map(str::to_owned)));
    assert_eq!(
        split(" font-size = 12 # big\r"),
        pair("font-size", Some("12"))
    );
    assert_eq!(
        split("palette-red=#ff0000 # red"),
        pair("palette-red", Some("#ff0000"))
    );
    assert_eq!(split("no-allow-font-ops"), pair("no-allow-font-ops", None));
    assert_eq!(split("login="), pair("login", Some("")));
    assert_eq!(split("=x"), pair("", Some("x")));
    assert_eq!(split("  # a comment"), None);
    assert_eq!(split(""), None);
}

/// A vtium.conf with sections, whose look sets keys of the top level's
/// names (ADR-1403 D4).
const SECTIONS: &str = "\
# the desk
font-size=12
#xkb-layout=us
version = 1

# the looks
[look big]
font-size = 14
#font-name = monoid
bell
[rules]
input * -> roaming
";

// ADR-1403 D8: every reader of the top level stops at the first heading,
// so a look's `font-size = 14` is never the top level's.
#[test]
fn a_reader_of_the_top_level_never_takes_a_section_s_line() {
    assert_eq!(setting(SECTIONS, "font-size"), Some("12".to_owned()));
    assert_eq!(setting(SECTIONS, "font-name"), None);
    assert_eq!(switch(SECTIONS, "bell"), None);
    assert_eq!(switch("no-bell\n[head x]\nbell\n", "bell"), Some(false));
}

// ADR-1403 D8: a new key goes after the top level's last line that holds
// a key, never at the end of the file, where it would land in `[rules]`.
#[test]
fn a_new_key_in_a_file_with_sections_stays_in_the_top_level() {
    let out = edit_conf(SECTIONS, &[Edit::Set("sb-size", "100")]).expect("the edit");
    assert_eq!(
        changed(SECTIONS, &out).first().map(|c| (c.0, c.2.clone())),
        Some((5, "sb-size=100\n".to_owned()))
    );
    assert_eq!(
        out,
        SECTIONS.replace("version = 1\n", "version = 1\nsb-size=100\n")
    );
    // a commented line counts only at the top level
    let out = edit_conf(SECTIONS, &[Edit::Set("font-name", "tiny5")]).expect("the edit");
    assert_eq!(
        out,
        SECTIONS.replace("version = 1\n", "version = 1\nfont-name=tiny5\n")
    );
    let out = edit_conf(SECTIONS, &[Edit::Set("xkb-layout", "de")]).expect("the edit");
    assert_eq!(
        out,
        SECTIONS.replace("#xkb-layout=us\n", "#xkb-layout=us\nxkb-layout=de\n")
    );
    // with no key line above the heading, the key goes just before it
    assert_eq!(
        edit_conf("# only a comment\n[rules]\n", &[Edit::Set("bell", "x")]).expect("the edit"),
        "# only a comment\nbell=x\n[rules]\n"
    );
    assert_eq!(
        edit_conf("[rules]\n", &[Edit::Switch("bell", true)]).expect("the edit"),
        "bell\n[rules]\n"
    );
}

#[test]
fn a_set_or_unset_key_changes_the_top_level_s_line_alone() {
    let out = edit_conf(SECTIONS, &[Edit::Set("font-size", "16")]).expect("the edit");
    assert_eq!(
        changed(SECTIONS, &out),
        [(2, "font-size=12\n".to_owned(), "font-size=16\n".to_owned())]
    );
    let out = edit_conf(SECTIONS, &[Edit::Unset("font-size")]).expect("the edit");
    assert_eq!(
        changed(SECTIONS, &out),
        [(2, "font-size=12\n".to_owned(), "#font-size=12\n".to_owned())]
    );
    let out = edit_conf(SECTIONS, &[Edit::Switch("bell", false)]).expect("the edit");
    assert_eq!(
        out,
        SECTIONS.replace("version = 1\n", "version = 1\nno-bell\n")
    );
}

// ADR-1403 D4: a `#` glued to a value stays in it, a departure from the
// C reference, which read `font-size=12#big` as 12.
#[test]
fn a_hash_glued_to_a_value_stays_in_the_value() {
    assert_eq!(
        setting("font-size=12#big\n", "font-size"),
        Some("12#big".to_owned())
    );
    assert_eq!(
        setting("font-size=12 #big\n", "font-size"),
        Some("12".to_owned())
    );
    assert_eq!(
        setting("login = a \\#b # c\n", "login"),
        Some("a #b".to_owned())
    );
    let read = key_line(b"font-size=12#big").expect("a key line");
    assert_eq!(read.value.as_deref(), Some(&b"12#big"[..]));
}

// The fuzz target's find: a key added before a first line that opens with
// a byte-order mark takes the mark, so the heading behind it stays one.
#[test]
fn a_key_added_before_the_first_line_keeps_the_byte_order_mark_first() {
    let text = "\u{feff}[rules]\ninput * -> roaming\n";
    let out = edit_conf(text, &[Edit::Set("font-size", "12")]).expect("the edit");
    assert_eq!(out, "\u{feff}font-size=12\n[rules]\ninput * -> roaming\n");
}

// A byte-order mark is no part of the first line's key, for the readers
// and the editor alike, as the split reads it.
#[test]
fn a_byte_order_mark_is_no_part_of_the_first_key() {
    let text = "\u{feff}font-size=3\nbell\n";
    assert_eq!(setting(text, "font-size"), Some("3".to_owned()));
    assert_eq!(switch("\u{feff}no-bell\n", "bell"), Some(false));
    assert_eq!(
        edit_conf(text, &[Edit::Set("font-size", "12")]).expect("the edit"),
        "\u{feff}font-size=12\nbell\n"
    );
}

// The fuzz target's find: an empty value before a comment gains a blank
// with its new value, so the comment's `#` stays a comment.
#[test]
fn a_value_set_before_a_comment_keeps_the_comment_a_comment() {
    let out =
        edit_conf(" font-size=   # big\n", &[Edit::Set("font-size", "12")]).expect("the edit");
    assert_eq!(out, " font-size=   12 # big\n");
    assert_eq!(setting(&out, "font-size"), Some("12".to_owned()));
}
