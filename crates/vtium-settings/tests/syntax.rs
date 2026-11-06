//! The head file's syntax as ADR-1401 D3 settles it: comments, line
//! endings, the top-level keys, the names and the targets, and the first
//! cut's names, each an error that names the word that replaced it.

use vtium_settings::{PolicyFault, Problem, Target, parse};

/// The first error's text for `text`.
fn first_error(text: &str) -> String {
    match parse(text) {
        Ok(p) => panic!("{text:?} parsed: {p:?}"),
        Err(r) => r.errors[0].to_string(),
    }
}

fn login(text: &str) -> Option<String> {
    parse(&format!("version = 1\n[head a]\n{text}\n"))
        .unwrap_or_else(|e| panic!("{text:?}: {e:?}"))
        .config
        .heads[0]
        .login
        .clone()
}

// covers: platform.heads.rules
#[test]
fn a_hash_starts_a_comment_only_at_the_start_of_a_line_or_after_a_blank() {
    assert_eq!(login("login = cmd --x#1").as_deref(), Some("cmd --x#1"));
    assert_eq!(
        login("login = cmd --x#1 # a note").as_deref(),
        Some("cmd --x#1")
    );
    assert_eq!(login("login = x \\#y").as_deref(), Some("x #y"));
    assert_eq!(login("login = x\\#y").as_deref(), Some("x\\#y"));
    assert_eq!(login("login = a\t# a note").as_deref(), Some("a"));
    let c = parse("version = 1   # the format\n[head a]   # a note\n#[head b]\n")
        .expect("valid")
        .config;
    let names: Vec<_> = c.heads.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["a"]);
    // a value's first character after a blank starts a comment, so the
    // value is empty
    assert!(matches!(
        parse("version = 1\n[head a]\nlogin = #x\n").map_err(|r| r.errors[0].problem.clone()),
        Err(Problem::BadValue { key: "login", .. })
    ));
}

/// The head files under docs/heads/, by name.
fn shipped_examples() -> Vec<(String, String)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/heads");
    let mut files: Vec<(String, String)> = std::fs::read_dir(dir)
        .expect("docs/heads")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "conf"))
        .map(|p| {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let text = std::fs::read_to_string(&p).expect("a head file");
            (name, text)
        })
        .collect();
    files.sort();
    assert!(files.len() >= 7, "{files:?}");
    files
}

// covers: platform.heads.rules
#[test]
fn a_crlf_line_ending_is_a_newline_and_a_lone_carriage_return_an_error() {
    for (name, text) in shipped_examples() {
        let lf = parse(&text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let crlf = parse(&text.replace('\n', "\r\n")).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(lf, crlf, "{name}");
    }
    let r = parse("version = 1\n[head a]\rx\n").expect_err("a lone carriage return");
    assert_eq!(
        (r.errors[0].line, &r.errors[0].problem),
        (Some(2), &Problem::ControlCharacter('\r'))
    );
}

// a byte-order mark before the first line is no part of it
// covers: platform.heads.policy
#[test]
fn a_byte_order_mark_before_the_first_line_is_dropped() {
    let r = vtium_settings::reading(b"\xef\xbb\xbfpolicy = closed\nversion = 1\n[head a]\n");
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.parsed.config.policy, vtium_settings::Policy::Closed);
    assert!(parse("\u{feff}version = 1\n").is_ok());
}

// covers: platform.heads.policy
#[test]
fn version_and_policy_come_before_the_first_section_in_either_order() {
    for text in [
        "version = 1\npolicy = closed\n[head a]\n",
        "policy = closed\nversion = 1\n[head a]\n",
        "# a comment\n\npolicy = closed\n\n# another\nversion = 1\n[head a]\n",
    ] {
        let p = parse(text).unwrap_or_else(|e| panic!("{text:?}: {e:?}"));
        assert_eq!(p.config.heads.len(), 1, "{text:?}");
    }
    assert!(matches!(
        parse("version = 1\npolicy = maybe\n").map_err(|r| r.errors[0].problem.clone()),
        Err(Problem::UnreadablePolicy(PolicyFault::Value(_)))
    ));
}

// covers: platform.heads.rules
#[test]
fn roaming_ignore_and_off_are_the_targets_and_the_reserved_names() {
    let c = parse("version = 1\n[head a]\n[rules]\ninput * -> roaming\ndisplay * -> off\n")
        .expect("valid")
        .config;
    let targets: Vec<_> = c.rules.iter().map(|r| r.target).collect();
    assert_eq!(targets, [Target::Roaming, Target::Off]);
    for name in ["roaming", "ignore", "off"] {
        assert!(
            matches!(
                parse(&format!("version = 1\n[head {name}]\n"))
                    .map_err(|r| r.errors[0].problem.clone()),
                Err(Problem::ReservedName(_))
            ),
            "{name}"
        );
    }
    // the first cut's targets stay reserved: a head of either name is an
    // error naming the word that replaced it, and drops with its rules
    for (name, word) in [("focus", "`roaming`"), ("dark", "`off`")] {
        let shown = first_error(&format!("version = 1\n[head {name}]\n"));
        assert!(shown.contains(word), "{name}: {shown}");
        let r = vtium_settings::reading(
            format!("version = 1\n[head {name}]\n[head a]\n[rules]\ndisplay * -> {name}\ndisplay * -> a\n")
                .as_bytes(),
        );
        let heads: Vec<_> = r
            .parsed
            .config
            .heads
            .iter()
            .map(|h| h.name.as_str())
            .collect();
        assert_eq!(heads, ["a"], "{name}");
        let lines: Vec<_> = r.parsed.config.rules.iter().map(|r| r.line).collect();
        assert_eq!(lines, [6], "{name}");
    }
    assert!(matches!(
        parse("version = 1\n[rules]\ndisplay * -> roaming\n")
            .map_err(|r| r.errors[0].problem.clone()),
        Err(Problem::WrongKind {
            target: "roaming",
            ..
        })
    ));
    assert!(matches!(
        parse("version = 1\n[rules]\ninput * -> off\n").map_err(|r| r.errors[0].problem.clone()),
        Err(Problem::WrongKind { target: "off", .. })
    ));
}

// covers: platform.heads.rules
#[test]
fn the_first_cuts_names_are_errors_that_name_the_new_word() {
    let cases = [
        ("[rules]\ninput * -> focus\n", "`roaming`"),
        ("[rules]\ndisplay * -> dark\n", "`off`"),
        ("[head a]\non-loss = hangup\n", "`when-display-lost`"),
        ("[head a]\nwhen-display-lost = nohup\n", "`keep`"),
        ("[head a]\nrescue = off\n", "`isolated = yes`"),
        ("[guards]\nnever-dark = off\n", "`policy`"),
    ];
    for (lines, word) in cases {
        let shown = first_error(&format!("version = 1\n{lines}"));
        assert!(shown.contains(word), "{lines:?}: {shown}");
    }
    // a head takes its font from its look alone, and the error says so
    for key in ["font-name = t", "font-size = 12"] {
        let shown = first_error(&format!("version = 1\n[head a]\n{key}\n"));
        assert!(shown.contains("`[look NAME]`"), "{key}: {shown}");
    }
    // a misplaced policy line says what it means for the file
    let shown = first_error("version = 1\npolicy = closed\n[head a]\npolicy = closed\n");
    assert!(
        shown.contains(
            "so it ignores this line, and the file's policy comes from the lines above the first section"
        ),
        "{shown}"
    );
    // a top-level key given twice is named as one above the first section
    for key in ["version = 1", "policy = open\npolicy = open"] {
        let shown = first_error(&format!("version = 1\n{key}\n"));
        assert!(shown.contains("above the first section"), "{key}: {shown}");
    }
}

#[test]
fn a_heads_keys_take_their_words() {
    use vtium_settings::WhenDisplayLost;
    let p = parse(
        "version = 1\n[head a]\nisolated = yes\nwhen-display-lost = keep\n[head b]\nisolated = no\nwhen-display-lost = hangup 5m\n[head c]\n",
    )
    .expect("valid");
    let got: Vec<_> = p
        .config
        .heads
        .iter()
        .map(|h| (h.isolated, h.when_display_lost))
        .collect();
    assert_eq!(
        got,
        [
            (Some(true), WhenDisplayLost::Keep),
            (Some(false), WhenDisplayLost::HangupAfter(300)),
            (None, WhenDisplayLost::Keep),
        ]
    );
    assert!(matches!(
        parse("version = 1\n[head a]\nisolated = on\n").map_err(|r| r.errors[0].problem.clone()),
        Err(Problem::BadValue {
            key: "isolated",
            ..
        })
    ));
}

/// The ```ini blocks of docs/heads.md, each with the file the link after
/// it names.
fn guide_blocks() -> Vec<(String, String)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/heads.md");
    let guide = std::fs::read_to_string(path).expect("docs/heads.md");
    let mut blocks = Vec::new();
    let mut lines = guide.lines();
    while let Some(line) = lines.next() {
        if line != "```ini" {
            continue;
        }
        let mut block = String::new();
        for line in lines.by_ref() {
            if line == "```" {
                break;
            }
            block.push_str(line);
            block.push('\n');
        }
        let link = lines
            .by_ref()
            .find_map(|l| {
                let l = l.trim();
                (l.starts_with("[heads/") && l.ends_with(".conf)"))
                    .then(|| {
                        l.trim_start_matches('[')
                            .split(']')
                            .next()
                            .map(str::to_owned)
                    })
                    .flatten()
            })
            .expect("a link to the block's file after it");
        blocks.push((link, block));
    }
    blocks
}

// docs/heads.md says each of its blocks is the file it links, byte for
// byte, and that each file passes the check
// covers: platform.heads.rules
#[test]
fn each_block_of_the_user_guide_equals_its_file_and_parses() {
    let blocks = guide_blocks();
    assert!(blocks.len() >= 6, "{blocks:?}");
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs");
    for (name, block) in &blocks {
        let file = std::fs::read_to_string(format!("{dir}/{name}"))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(block, &file, "docs/heads.md's block for {name}");
    }
    for (name, text) in shipped_examples() {
        let parsed = parse(&text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(parsed.warnings.is_empty(), "{name}: {:?}", parsed.warnings);
    }
}
