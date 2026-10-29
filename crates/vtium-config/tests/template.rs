//! One source for the default configuration (ADR-1401 D6, D11): the
//! compiled-in configuration is the parse of the template, and the
//! template states the shipped rules.

use vtium_config::{
    Class, Config, Font, Guards, Kind, Match, OnLoss, TEMPLATE, Target, Token, parse,
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
    assert_eq!(head.on_loss, OnLoss::Nohup);
    assert!(head.rescue);
    assert_eq!(head.font, Font::default());
    assert_eq!(
        config.guards,
        Guards {
            anti_lockout: true,
            rescue_key: true,
            never_dark: true,
        }
    );

    let class = |c| Match::All(vec![Token::Class(c)]);
    let want = [
        (Kind::Display, Match::Any, Target::Head(0)),
        (Kind::Input, class(Class::Keyboard), Target::Focus),
        (Kind::Input, class(Class::Mouse), Target::Focus),
        (Kind::Input, class(Class::Touchpad), Target::Focus),
        (Kind::Input, class(Class::Touchscreen), Target::Focus),
        (Kind::Input, class(Class::Trackball), Target::Focus),
        (Kind::Input, class(Class::Pointingstick), Target::Focus),
        (Kind::Input, class(Class::Key), Target::Focus),
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
fn the_template_writes_every_head_and_guard_key_as_a_commented_line() {
    // every setting is a commented line, so a file written from the
    // template changes nothing until edited (D6)
    for key in [
        "profile",
        "consoles",
        "login",
        "on-loss",
        "rescue",
        "font-name",
        "font-size",
        "anti-lockout",
        "rescue-key",
        "never-dark",
    ] {
        let commented = format!("\n# {key} = ");
        assert!(
            TEMPLATE.contains(&commented),
            "the template has no `# {key} = ` line"
        );
        let live = format!("\n{key} =");
        assert!(!TEMPLATE.contains(&live), "the template sets `{key}` live");
    }
    assert!(TEMPLATE.contains("\n# [guards]\n"));
}
