//! The one set of operations on the draft (ADR-1403 D8): each takes
//! vtium.conf's text and gives back the edited text, through the two
//! writers of vtium-settings, the line editor for the top level and the
//! section writer for the looks, heads and rules. The `vtium config`
//! commands call these, and the TUI's views call the same functions, so a
//! change means one thing wherever it is made.
//!
//! An operation that cannot be made is a usage error, exit 2: a rule
//! number, head or look that does not exist, or a value its key cannot
//! take.

use vtium_settings::{
    Edit, Of, SectionEdit, add_rule, add_section, edit_conf, move_rule, parse_sections,
    remove_rule, set_key, set_rule, split,
};

use crate::outcome::Fail;

/// A section writer's refusal, as the command prints it.
fn refused(e: &SectionEdit) -> Fail {
    Fail::Usage(e.to_string())
}

/// A rule as `rule add` and `rule set` take it: `KIND SELECTOR -> TARGET`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleText {
    pub kind: String,
    pub selector: String,
    pub target: String,
}

impl RuleText {
    /// The rule's line in `[rules]`.
    pub fn line(&self) -> String {
        format!("{} {} -> {}", self.kind, self.selector, self.target)
    }

    /// `line` read as a rule's words, its comment left out; `None` when it
    /// is not `KIND SELECTOR -> TARGET`.
    pub fn read(line: &str) -> Option<RuleText> {
        let words: Vec<&str> = line
            .split_whitespace()
            .take_while(|w| !w.starts_with('#'))
            .collect();
        match words.as_slice() {
            [kind, selector, "->", target] => Some(RuleText {
                kind: (*kind).to_owned(),
                selector: (*selector).to_owned(),
                target: (*target).to_owned(),
            }),
            _ => None,
        }
    }
}

/// The text of rule `number`, counted from 1, in `text`.
pub fn rule_text(text: &str, number: usize) -> Result<RuleText, Fail> {
    let config = parse_sections(text)
        .map_err(|r| refused(&SectionEdit::Unreadable(r)))?
        .map(|p| p.config)
        .unwrap_or_default();
    let rule = number
        .checked_sub(1)
        .and_then(|i| config.rules.get(i))
        .ok_or_else(|| refused(&SectionEdit::NoRule(number)))?;
    text.lines()
        .nth(rule.line.saturating_sub(1))
        .and_then(RuleText::read)
        .ok_or_else(|| refused(&SectionEdit::NoRule(number)))
}

/// Adds `rule` before rule `before`, or with `None` where the placement
/// of ADR-1403 D7 puts it, before the first wider rule of its kind.
pub fn rule_add(text: &str, rule: &str, before: Option<usize>) -> Result<String, Fail> {
    add_rule(text, rule, before).map_err(|e| refused(&e))
}

/// What `rule set` changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleChange {
    /// The whole rule.
    Whole(String),
    /// Its selector, its target, or both, the rest kept.
    Parts {
        selector: Option<String>,
        target: Option<String>,
    },
}

pub fn rule_set(text: &str, number: usize, change: &RuleChange) -> Result<String, Fail> {
    let line = match change {
        RuleChange::Whole(rule) => rule.clone(),
        RuleChange::Parts { selector, target } => {
            let mut rule = rule_text(text, number)?;
            if let Some(s) = selector {
                rule.selector.clone_from(s);
            }
            if let Some(t) = target {
                rule.target.clone_from(t);
            }
            rule.line()
        }
    };
    set_rule(text, number, &line).map_err(|e| refused(&e))
}

/// Where `rule move` puts a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum To {
    Before(usize),
    Last,
}

pub fn rule_move(text: &str, number: usize, to: To) -> Result<String, Fail> {
    let before = match to {
        To::Before(m) => m,
        To::Last => parse_sections(text)
            .map_err(|r| refused(&SectionEdit::Unreadable(r)))?
            .map_or(0, |p| p.config.rules.len())
            .saturating_add(1),
    };
    move_rule(text, number, before).map_err(|e| refused(&e))
}

pub fn rule_rm(text: &str, number: usize) -> Result<String, Fail> {
    remove_rule(text, number).map_err(|e| refused(&e))
}

/// Sets each of `keys`, a key and its value or `None` to unset it, in the
/// section `of`, in order.
pub fn section_keys(text: &str, of: Of<'_>, keys: &[(&str, Option<&str>)]) -> Result<String, Fail> {
    let mut out = text.to_owned();
    for (key, value) in keys {
        out = set_key(&out, of, key, *value).map_err(|e| refused(&e))?;
    }
    Ok(out)
}

/// Adds `[head NAME]`, naming the look `look` where one is given; without
/// one the head takes the default look.
pub fn head_add(text: &str, name: &str, look: Option<&str>) -> Result<String, Fail> {
    let added = add_section(text, Of::Head(name)).map_err(|e| refused(&e))?;
    match look {
        Some(look) => section_keys(&added, Of::Head(name), &[("look", Some(look))]),
        None => Ok(added),
    }
}

/// Adds `[look NAME]` with its `font-name` and `font-size` where given; a
/// key left out is inherited from the top level.
pub fn look_add(
    text: &str,
    name: &str,
    frit: Option<&str>,
    size: Option<&str>,
) -> Result<String, Fail> {
    let added = add_section(text, Of::Look(name)).map_err(|e| refused(&e))?;
    let keys: Vec<(&str, Option<&str>)> = [("font-size", size), ("font-name", frit)]
        .into_iter()
        .filter(|(_, v)| v.is_some())
        .collect();
    section_keys(&added, Of::Look(name), &keys)
}

/// The heads of `text` that name the look `look`.
pub fn heads_using(text: &str, look: &str) -> Vec<String> {
    parse_sections(text)
        .ok()
        .flatten()
        .map(|p| {
            p.config
                .heads
                .iter()
                .filter(|h| h.look == look && h.look_line > 0)
                .map(|h| h.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The rules of `text` that send to the head `head`, by their numbers.
pub fn rules_to(text: &str, head: &str) -> Vec<usize> {
    let Some(config) = parse_sections(text).ok().flatten().map(|p| p.config) else {
        return Vec::new();
    };
    let Some(index) = config.heads.iter().position(|h| h.name == head) else {
        return Vec::new();
    };
    config
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.target == vtium_settings::Target::Head(index))
        .map(|(i, _)| i.saturating_add(1))
        .collect()
}

/// Why a section cannot be removed or renamed yet: the section writer of
/// vtium-settings has no function that removes or renames a section, and
/// vtium-config writes no section text of its own (ADR-1201 D6).
const NO_SECTION_WRITER: &str = "vtium-config cannot remove or rename a section yet: vtium-settings's section writer has no function for it. Use vtium config edit to make this change by hand.";

/// Deletes `[head NAME]`. The rules that send to it stay, broken, until
/// each is pointed elsewhere or deleted.
pub fn head_rm(text: &str, name: &str) -> Result<String, Fail> {
    if parse_sections(text)
        .ok()
        .flatten()
        .is_none_or(|p| p.config.head(name).is_none())
    {
        return Err(refused(&SectionEdit::NoSection(name.to_owned())));
    }
    Err(Fail::Usage(NO_SECTION_WRITER.to_owned()))
}

/// Deletes `[look NAME]`. While a head names it, it refuses and names the
/// heads; with `force` it sets those heads to the default look first.
pub fn look_rm(text: &str, name: &str, force: bool) -> Result<String, Fail> {
    let config = parse_sections(text)
        .map_err(|r| refused(&SectionEdit::Unreadable(r)))?
        .map(|p| p.config)
        .unwrap_or_default();
    if !config.looks.iter().any(|l| l.name == name) {
        return Err(refused(&SectionEdit::NoSection(name.to_owned())));
    }
    let users = heads_using(text, name);
    if !users.is_empty() && !force {
        return Err(Fail::Usage(format!(
            "The heads {} use the look {name}. Point them at another look first, or pass --force to set them to the default look.",
            users.join(", ")
        )));
    }
    Err(Fail::Usage(NO_SECTION_WRITER.to_owned()))
}

/// Renames the head or look `of` to `new`, with every rule or head that
/// names it.
pub fn rename(text: &str, of: Of<'_>, new: &str) -> Result<String, Fail> {
    let _ = (text, of, new);
    Err(Fail::Usage(NO_SECTION_WRITER.to_owned()))
}

/// Sets the top-level key `key` to `value` through the line editor, after
/// the checks `set` makes of a value (ADR-1201 D5).
pub fn set_top(text: &str, key: &str, value: &str) -> Result<String, Fail> {
    let key = crate::commands::conf_key(key).map_err(Fail::Usage)?;
    let edit = crate::commands::set_edit(key, value).map_err(Fail::Usage)?;
    edit_conf(text, &[edit]).map_err(|e| Fail::Usage(crate::tui::words::conf_edit(&e)))
}

/// Leaves the top-level key `key` at its default: its line commented out.
pub fn unset_top(text: &str, key: &str) -> Result<String, Fail> {
    let key = crate::commands::conf_key(key).map_err(Fail::Usage)?;
    edit_conf(text, &[Edit::Unset(key.name)])
        .map_err(|e| Fail::Usage(crate::tui::words::conf_edit(&e)))
}

/// The top level of `example`, the shipped vtium.conf: every line above
/// its first section.
fn top_of(example: &str) -> String {
    let parts = split(example.as_bytes());
    parts
        .top_lines()
        .map(|(_, line)| String::from_utf8_lossy(line).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// What `new` empties the draft to: the top level of the shipped example,
/// every key at its default and commented out, then `version = 1` and one
/// look, `[look default]`, with no keys (ADR-1403 D7).
pub fn new_text(example: &str) -> String {
    let mut out = top_of(example).trim_end().to_owned();
    let versioned = split(out.as_bytes())
        .top_lines()
        .any(|(_, l)| vtium_settings::key_line(l).is_some_and(|k| k.key == b"version"));
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    if !versioned {
        out.push_str("version = 1\n\n");
    }
    out.push_str("[look default]\n");
    out
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    const DESK: &str = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming  # every keyboard\n";

    #[test]
    fn a_rule_s_target_changes_and_its_selector_and_comment_stay() {
        let out = rule_set(
            DESK,
            2,
            &RuleChange::Parts {
                selector: None,
                target: Some("panel".to_owned()),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> panel  # every keyboard\n"
        );
    }

    #[test]
    fn a_rule_number_past_the_last_is_a_usage_error() {
        let e = rule_rm(DESK, 9).unwrap_err();
        assert_eq!(e.code(), 2);
        assert!(e.text().contains("no rule 9"), "{}", e.text());
    }

    #[test]
    fn moving_a_rule_last_puts_it_after_every_rule() {
        assert_eq!(
            rule_move(DESK, 1, To::Last).unwrap(),
            "version = 1\n[head desk]\n[head panel]\n[rules]\ninput class:keyboard -> roaming  # every keyboard\ndisplay * -> desk\n"
        );
    }

    #[test]
    fn a_new_configuration_is_the_example_s_top_level_and_one_empty_look() {
        let example = "# a comment\n#font-size=0\n\nversion = 1\n[head console]\n[rules]\ndisplay * -> console\n";
        assert_eq!(
            new_text(example),
            "# a comment\n#font-size=0\n\nversion = 1\n\n[look default]\n"
        );
        assert_eq!(
            new_text("#mouse\n"),
            "#mouse\n\nversion = 1\n\n[look default]\n"
        );
    }

    #[test]
    fn a_look_a_head_names_is_kept_without_force() {
        let text = "version = 1\n[look big]\n[head desk]\nlook = big\n[rules]\ndisplay * -> desk\n";
        let e = look_rm(text, "big", false).unwrap_err();
        assert_eq!(e.code(), 2);
        assert!(e.text().contains("desk"), "{}", e.text());
        assert_eq!(heads_using(text, "big"), ["desk"]);
        assert_eq!(rules_to(text, "desk"), [1]);
    }
}
