/// A look may set `pointer-theme`, so each look picks its own pointer
/// theme. A head takes the theme of the look it names, and a head
/// whose look names none gets no section setting for the key, so it
/// keeps the top level's theme.
#[test]
fn a_look_picks_its_pointer_theme_and_a_look_without_one_keeps_the_top_level_s() {
    let text = "version = 1\npointer-theme = /usr/share/icons/Adwaita\n\
                [look plain]\n\
                [look arrows]\npointer-theme = /usr/share/vtium/pointers.svg\n\
                [head a]\nlook = arrows\n[head b]\nlook = plain\n";
    let r = super::reading(text.as_bytes());
    assert!(r.errors.errors.is_empty(), "{:?}", r.errors.errors);
    let config = super::parse(text).map(|p| p.config).unwrap_or_default();
    let theme = |head: &str| -> Option<(crate::parse::Level, String)> {
        let head = config.head(head)?;
        config
            .settings(head)
            .into_iter()
            .find(|(_, s)| s.key.name == "pointer-theme")
            .map(|(level, s)| (level, s.value.clone()))
    };
    assert_eq!(
        theme("a"),
        Some((
            crate::parse::Level::Look,
            "/usr/share/vtium/pointers.svg".to_owned()
        ))
    );
    assert_eq!(theme("b"), None, "the top level's theme stands");
}

/// A look's `palette` that cannot name a color scheme file is refused
/// with the rule for a name, which says what the value may be, and
/// never with the message for an empty value.
#[test]
fn a_look_s_bad_palette_says_what_a_name_may_be() {
    for value in ["../evil", "a b"] {
        let text = format!("version = 1\n[look x]\npalette = {value}\n");
        let r = super::reading(text.as_bytes());
        let said: Vec<String> = r
            .errors
            .errors
            .iter()
            .map(|e| e.problem.to_string())
            .collect();
        assert_eq!(
            said,
            [format!(
                "`palette` cannot be \"{value}\": a name has 1 to 64 characters, each a letter, a digit or one of `. _ + -`. Change the name."
            )],
            "{value}"
        );
    }
}
