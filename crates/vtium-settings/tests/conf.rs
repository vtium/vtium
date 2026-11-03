//! vtium.conf edited line by line (ADR-1201 D5): only the lines a change
//! needs move, and every other byte stays.

use std::path::Path;

use vtium_settings::{ConfEdit, Edit, edit_conf, setting, switch};

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
    for bad in ["a#b", "two\nlines", " lead", "trail ", "", "nul\0"] {
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
