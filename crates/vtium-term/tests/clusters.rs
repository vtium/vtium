//! `vtium_term::clusters`, the characters a line of the screen shows for a
//! text: each case and each generated text, printed on a terminal of its
//! own under both rules of `cluster-width`, shows exactly the clusters and
//! widths the function gives.

mod common;

use common::{Rng, TEXT, seeds};
use vtium_term::{CLUSTER_MAX, ClusterWidth, Glyph, Terminal, Width, clusters};

/// The clusters a terminal's first line shows after `text` is printed on
/// it under `rule`, each with its width. The line is wide enough that
/// nothing wraps.
fn printed(text: &str, rule: ClusterWidth) -> Vec<(String, u8)> {
    let cols = text.chars().count().saturating_mul(2).saturating_add(1);
    let mut t = Terminal::new(cols, 1, 0);
    t.set_cluster_width(rule);
    t.input(text.as_bytes());
    let screen = t.screen();
    let line = screen.line(0).expect("the first line");
    line.cells()
        .iter()
        .take(screen.cursor().0)
        .filter(|c| matches!(c.width(), Width::Narrow | Width::Wide))
        .map(|&c| {
            let text = match line.glyph(c) {
                Glyph::Char(ch) => ch.to_string(),
                Glyph::Cluster(s) => s.to_owned(),
                Glyph::Empty => String::new(),
            };
            (text, if c.width() == Width::Wide { 2 } else { 1 })
        })
        .collect()
}

/// What the screen shows of `clusters(text, rule)`: each cluster as far as
/// the codepoints a cell keeps, without the leading cluster of width 0,
/// which the screen drops.
fn expected(text: &str, rule: ClusterWidth) -> Vec<(String, u8)> {
    clusters(text, rule)
        .filter(|&(_, w)| w > 0)
        .map(|(c, w)| (c.chars().take(CLUSTER_MAX).collect(), w))
        .collect()
}

/// Checks that `text` prints as `clusters` says under both rules, and
/// that the clusters joined give `text` back.
fn check(text: &str) {
    for rule in [ClusterWidth::Double, ClusterWidth::Single] {
        assert_eq!(
            expected(text, rule),
            printed(text, rule),
            "{text:?} under {rule:?}"
        );
        let joined: String = clusters(text, rule).map(|(c, _)| c).collect();
        assert_eq!(joined, text, "{text:?} under {rule:?}");
    }
}

#[test]
fn each_cluster_and_width_is_what_the_screen_shows() {
    for text in [
        "",
        "abc",
        "e\u{301}x",
        // a vowel sign after a space, which joins the space, and one on
        // its consonant: two cells under `double`, one under `single`
        "ab \u{BCA}.",
        "ab \u{B95}\u{BCA}.",
        // a virama conjunct
        "\u{915}\u{94D}\u{937}a",
        // ZERO WIDTH SPACE joins the character before it, though UAX #29
        // breaks before it
        "a\u{200B}b",
        "\u{4E2D}\u{200B}\u{301}x",
        // a flag, a lone regional indicator, a ZWJ family and a heart
        // with VS16
        "\u{1F1FA}\u{1F1F8}\u{1F1FA}x",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{2764}\u{FE0F}",
        "\u{1100}\u{1161}\u{11A8}a",
        // a prepended concatenation mark joins what follows it
        "\u{600}1x",
    ] {
        check(text);
    }
}

#[test]
fn marks_at_the_start_make_one_cluster_of_no_width() {
    let text = "\u{301}\u{200B}\u{302}e\u{301}";
    let got: Vec<(&str, u8)> = clusters(text, ClusterWidth::Double).collect();
    assert_eq!(
        got,
        [("\u{301}\u{200B}\u{302}", 0), ("e\u{301}", 1)],
        "the screen drops the marks before the first character"
    );
    check(text);
}

#[test]
fn a_cluster_past_the_codepoints_a_cell_keeps_holds_them_all_and_counts_the_first() {
    // the 33rd codepoint, a spacing mark, would widen the cluster, but the
    // cell keeps only the first 32, so the cluster stays one cell wide
    let mut text = String::from("\u{B95}");
    text.extend(std::iter::repeat_n('\u{301}', CLUSTER_MAX - 1));
    text.push('\u{BCA}');
    text.push('x');
    let got: Vec<(&str, u8)> = clusters(&text, ClusterWidth::Double).collect();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].0.chars().count(), CLUSTER_MAX + 1);
    assert_eq!(got[0].1, 1);
    check(&text);
}

#[test]
fn under_single_a_spacing_mark_adds_no_cell() {
    let text = "\u{B95}\u{BCA}";
    let width = |rule| clusters(text, rule).map(|(_, w)| w).collect::<Vec<_>>();
    assert_eq!(width(ClusterWidth::Double), [2]);
    assert_eq!(width(ClusterWidth::Single), [1]);
}

#[test]
fn generated_text_prints_as_its_clusters_say() {
    let mut extra: Vec<&str> = TEXT.to_vec();
    // spacing marks, viramas and consonants, so the rules of `double`
    // meet each other and the marks in TEXT
    extra.extend(["\u{BCA}", "\u{B95}", "\u{94D}", "\u{937}", "\u{200C}"]);
    for seed in 0..seeds(400) {
        let mut rng = Rng(seed);
        let len = 1 + rng.below(40);
        let text: String = (0..len).map(|_| *rng.pick(&extra)).collect();
        check(&text);
    }
}
