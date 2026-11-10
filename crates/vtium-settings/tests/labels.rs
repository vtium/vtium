//! The `[labels]` section of vtium.conf: each line gives the displays or
//! inputs a match matches a label of the person's own, as
//! `display MATCH = LABEL` or `input MATCH = LABEL`. The parser checks a
//! label's match as it checks a rule's, and a bad line is named by its
//! number and ignored alone.

use vtium_settings::{Drops, Kind, LABEL_MAX, Problem, Syntax, escape_label, reading};

const GOOD: &str = "\
version = 1
[head desk]
[rules]
display * -> desk
input id:2dc8:5200 -> desk
[labels]
display * = Every screen
input id:2dc8:5200   =   Desk keyboard, the  black one
input class:keyboard = Board \\#2
";

#[test]
fn good_label_lines_are_read_with_their_match_and_their_text() {
    let r = reading(GOOD.as_bytes());
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let labels = &r.parsed.config.labels;
    let got: Vec<(usize, Kind, &str, &str)> = labels
        .iter()
        .map(|l| (l.line, l.kind, l.selector.as_str(), l.text.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (7, Kind::Display, "*", "Every screen"),
            (
                8,
                Kind::Input,
                "id:2dc8:5200",
                "Desk keyboard, the  black one"
            ),
            (9, Kind::Input, "class:keyboard", "Board #2"),
        ]
    );
    // the rules stay as they are
    assert_eq!(r.parsed.config.rules.len(), 2);
}

#[test]
fn a_bad_label_line_is_named_by_its_line_and_ignored_alone() {
    let long = "x".repeat(LABEL_MAX + 1);
    let text = format!(
        "version = 1\n[head desk]\n[rules]\ninput * -> desk\n[labels]\n\
         input colour:red = Red\n\
         input id:2dc8:5200 Desk\n\
         keyboard * = Desk\n\
         input * =\n\
         input * = {long}\n\
         input id:zz = Bad id\n\
         display * = Fine\n"
    );
    let r = reading(text.as_bytes());
    let got: Vec<(Option<usize>, &Problem, Option<Drops>)> = r
        .errors
        .errors
        .iter()
        .map(|e| (e.line, &e.problem, e.drops))
        .collect();
    assert_eq!(got.len(), 6, "{got:?}");
    assert!(matches!(got[0], (Some(6), Problem::UnknownMatchKey(_), _)));
    assert!(matches!(
        got[1],
        (Some(7), Problem::Syntax(Syntax::LabelEquals), _)
    ));
    assert!(matches!(
        got[2],
        (Some(8), Problem::Syntax(Syntax::LabelKind), _)
    ));
    assert!(matches!(
        got[3],
        (Some(9), Problem::Syntax(Syntax::LabelEmpty), _)
    ));
    assert!(matches!(
        got[4],
        (Some(10), Problem::Syntax(Syntax::LabelLong), _)
    ));
    assert!(matches!(got[5], (Some(11), Problem::BadToken(_), _)));
    // each error drops its own line, and nothing else
    for (_, _, drops) in &got {
        assert_eq!(*drops, Some(Drops::Line));
    }
    let config = &r.parsed.config;
    assert_eq!(config.heads.len(), 1);
    assert_eq!(config.rules.len(), 1);
    let labels: Vec<&str> = config.labels.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(labels, ["Fine"]);
    // each message says what is wrong, topic first
    assert_eq!(
        Problem::Syntax(Syntax::LabelEquals).to_string(),
        "This label line has no `=`. A label line reads `display MATCH = LABEL` or `input MATCH = LABEL`. Add `=` between the match and the label."
    );
}

#[test]
fn a_second_labels_section_is_an_error() {
    let text = "version = 1\n[head desk]\n[rules]\ninput * -> desk\n[labels]\n[labels]\n";
    let r = reading(text.as_bytes());
    assert_eq!(r.errors.errors.len(), 1, "{:?}", r.errors);
    assert_eq!(r.errors.errors[0].line, Some(6));
    assert!(matches!(
        r.errors.errors[0].problem,
        Problem::DuplicateSection("labels")
    ));
}

#[test]
fn a_label_written_with_its_hashes_escaped_reads_back_whole() {
    for label in ["Desk #2", "#hash", "a # b", "plain"] {
        let text = format!(
            "version = 1\n[head desk]\n[rules]\ninput * -> desk\n[labels]\ninput * = {}\n",
            escape_label(label)
        );
        let r = reading(text.as_bytes());
        assert!(r.errors.is_empty(), "{label}: {:?}", r.errors);
        assert_eq!(r.parsed.config.labels[0].text, label);
    }
}
