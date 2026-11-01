//! One source for the default configuration (ADR-1401 D6, D11): the
//! compiled-in configuration is the parse of the template, and the
//! template states the shipped rules.

use vtium_config::{
    Class, Config, Kind, Match, Policy, TEMPLATE, Target, Token, WhenDisplayLost, parse,
};

#[test]
fn the_compiled_in_configuration_is_the_parse_of_the_template() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/templates/heads.conf");
    let file = std::fs::read_to_string(path).expect("the template is readable");
    assert_eq!(
        file, TEMPLATE,
        "the compiled-in text is the template file's"
    );
    vtium_config::text(file.as_bytes()).expect("the template passes the file checks");
    let parsed = parse(&file).expect("the template is a valid head file");
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert!(parsed.notes.is_empty(), "{:?}", parsed.notes);
    assert_eq!(Config::builtin(), parsed.config);

    let config = Config::builtin();
    let [head] = config.heads.as_slice() else {
        panic!("the template declares one head: {:?}", config.heads);
    };
    assert_eq!(head.name, "console");
    assert_eq!(head.profile, "default");
    assert_eq!(head.consoles, 1);
    assert_eq!(head.login, None);
    assert_eq!(head.when_display_lost, WhenDisplayLost::Keep);
    assert_eq!(head.isolated, None);
    assert_eq!(config.policy, Policy::Open);
    assert!(!config.isolated(0));
    assert!(!config.survivors);

    let class = |c| Match::All(vec![Token::Class(c)]);
    let want = [
        (Kind::Display, Match::Any, Target::Head(0)),
        (Kind::Input, class(Class::Keyboard), Target::Roaming),
        (Kind::Input, class(Class::Mouse), Target::Roaming),
        (Kind::Input, class(Class::Touchpad), Target::Roaming),
        (Kind::Input, class(Class::Touchscreen), Target::Roaming),
        (Kind::Input, class(Class::Trackball), Target::Roaming),
        (Kind::Input, class(Class::Pointingstick), Target::Roaming),
        (Kind::Input, class(Class::Key), Target::Roaming),
        (Kind::Input, Match::Any, Target::Ignore),
    ];
    let got: Vec<_> = config
        .rules
        .iter()
        .map(|r| (r.kind, r.matcher.clone(), r.target))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn the_template_writes_the_policy_and_every_head_key_as_a_commented_line() {
    // every setting is a commented line, so a file written from the
    // template changes nothing until edited (D6)
    for key in [
        "policy",
        "profile",
        "consoles",
        "login",
        "when-display-lost",
        "isolated",
    ] {
        let commented = format!("\n# {key} = ");
        assert!(
            TEMPLATE.contains(&commented),
            "the template has no `# {key} = ` line"
        );
        let live = format!("\n{key} =");
        assert!(!TEMPLATE.contains(&live), "the template sets `{key}` live");
    }
}

/// `config` with every line number 0, so two files that state the same
/// configuration compare equal.
fn unlined(mut config: Config) -> Config {
    for head in &mut config.heads {
        head.line = 0;
    }
    for rule in &mut config.rules {
        rule.line = 0;
    }
    config
}

// ADR-1401 D6: docs/heads/default.conf writes the compiled-in
// configuration out, and docs/heads/reference.conf runs it as it stands
#[test]
fn the_user_guides_default_and_reference_files_state_the_compiled_in_configuration() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/heads");
    for name in ["default.conf", "reference.conf"] {
        let text = std::fs::read_to_string(format!("{dir}/{name}")).expect("the file");
        let parsed = parse(&text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(unlined(parsed.config), unlined(Config::builtin()), "{name}");
    }
}
