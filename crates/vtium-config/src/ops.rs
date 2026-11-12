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
    Edit, Of, SectionEdit, add_rule, add_section, edit_conf, move_rule, remove_rule,
    remove_section, rename_section, rule_lines, set_key, set_rule, split,
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
    let no_rule = || refused(&SectionEdit::NoRule(number));
    let line = number
        .checked_sub(1)
        .and_then(|i| rule_lines(text).get(i).copied())
        .ok_or_else(no_rule)?;
    text.lines()
        .nth(line.saturating_sub(1))
        .and_then(RuleText::read)
        .ok_or_else(|| {
            Fail::Usage(format!(
                "rule {number} is not written as KIND MATCH -> TARGET, so only rule set {number} 'KIND MATCH -> TARGET' or rule rm {number} can change it"
            ))
        })
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
        To::Last => rule_lines(text).len().saturating_add(1),
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

/// The keys a look takes (ADR-1403 D5), each with the option that names
/// it to `vtium config look add` and `look set`, in the order the TUI's
/// look card lists them. Those two commands and the look card read this
/// one table, and a test holds it to the key table's keys of a look.
pub const LOOK_KEYS: [(&str, &str); 8] = [
    ("font-name", "--frit"),
    ("font-size", "--size"),
    ("cell", "--cell"),
    ("font-engine", "--engine"),
    ("font-hinting", "--hinting"),
    ("font-line-spacing", "--line-spacing"),
    ("display-depth", "--display-depth"),
    ("display-dpi", "--display-dpi"),
];

/// The option of [`LOOK_KEYS`] that names the look key `key`.
pub fn look_option(key: &str) -> Option<&'static str> {
    LOOK_KEYS.iter().find(|(k, _)| *k == key).map(|(_, o)| *o)
}

/// Adds `[look NAME]` with each of `keys` that has a value, in the order
/// of [`LOOK_KEYS`], and then its `vtlook` when `keys` gives one; a key
/// left out is inherited from the top level.
pub fn look_add(text: &str, name: &str, keys: &[(&str, Option<&str>)]) -> Result<String, Fail> {
    let added = add_section(text, Of::Look(name)).map_err(|e| refused(&e))?;
    // the section writer puts each new key first in its section, so the
    // keys go in last to first
    let keys: Vec<(&str, Option<&str>)> = LOOK_KEYS
        .iter()
        .map(|(k, _)| *k)
        .chain(["vtlook"])
        .rev()
        .filter_map(|k| keys.iter().find(|(given, _)| *given == k).copied())
        .filter(|(_, v)| v.is_some())
        .collect();
    section_keys(&added, Of::Look(name), &keys)
}

/// The heads of `text` that name the look `look` with a `look =` line,
/// as vtium reads what survives of the sections, so a draft with errors
/// elsewhere still answers.
pub fn heads_using(text: &str, look: &str) -> Vec<String> {
    vtium_settings::sections(text.as_bytes())
        .map(|r| {
            r.parsed
                .config
                .heads
                .iter()
                .filter(|h| h.look == look && h.look_line > 0)
                .map(|h| h.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The rules of `text` whose target is `head`, by their numbers, every
/// rule line counted as the writer numbers them.
pub fn rules_to(text: &str, head: &str) -> Vec<usize> {
    rule_lines(text)
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            text.lines()
                .nth(n.saturating_sub(1))
                .and_then(RuleText::read)
                .is_some_and(|r| r.target == head)
        })
        .map(|(i, _)| i.saturating_add(1))
        .collect()
}

/// Deletes `[head NAME]`. The rules that send to it stay, broken, until
/// each is pointed elsewhere or deleted; [`rules_to`] names them.
pub fn head_rm(text: &str, name: &str) -> Result<String, Fail> {
    remove_section(text, Of::Head(name)).map_err(|e| refused(&e))
}

/// Deletes `[look NAME]`. While a head names it, it refuses and names the
/// heads; with `force` it sets those heads to the default look first, by
/// commenting out their `look` lines.
pub fn look_rm(text: &str, name: &str, force: bool) -> Result<String, Fail> {
    let users = heads_using(text, name);
    if !users.is_empty() && !force {
        return Err(Fail::Usage(match users.as_slice() {
            [one] => format!(
                "Head {one} uses look {name}. Switch it to another look first, or pass --force to switch it to the default look."
            ),
            _ => format!(
                "Heads {} use look {name}. Switch them to another look first, or pass --force to switch them to the default look.",
                users.join(", ")
            ),
        }));
    }
    let mut out = text.to_owned();
    for head in &users {
        out = section_keys(&out, Of::Head(head), &[("look", None)])?;
    }
    remove_section(&out, Of::Look(name)).map_err(|e| refused(&e))
}

/// Renames the head or look `of` to `new`, with every rule that sends to
/// the head or every head that names the look, in one change.
pub fn rename(text: &str, of: Of<'_>, new: &str) -> Result<String, Fail> {
    rename_section(text, of, new).map_err(|e| refused(&e))
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

/// `text` with vtium's compiled-in flow written out as its sections, when
/// it has none. A draft with no section, as a copied kmscon.conf is, runs
/// the compiled-in flow, and the Flow view shows that flow (ADR-1403 D3,
/// D7). A change to a section must first write that flow into the file:
/// a section alone, such as `[labels]`, would make the file one with
/// sections, no `version` line and no rules, which turns every display
/// off. A draft with sections is returned as it is.
pub fn with_flow(text: &str) -> String {
    if vtium_settings::sections(text.as_bytes()).is_some() {
        return text.to_owned();
    }
    let mut out = text.trim_end().to_owned();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(vtium_settings::TEMPLATE);
    out
}

/// The comment vtium-config writes above the `[labels]` section, wrapped
/// at 78 columns as the shipped vtium.conf's comments are, in the words of
/// ADR-1403 D4's example.
pub const LABELS_COMMENT: &str = "\
# The person's own names for displays and inputs. vtium shows a
# display's label when vtium-config identifies the displays.
";

/// The longer comment the Flow view's try-out wrote above the section. A
/// draft may still hold it, and it goes with the section as the comment
/// above does.
pub(crate) const LABELS_COMMENT_TRIED: &str = "\
# Labels. A label is a name of your own for a display or an input, such as
# \"desk keyboard\" or \"left screen\". vtium shows it on the on-screen
# display, in place of the name the system gives the device.
#
# Each line reads \"display MATCH = LABEL\" or \"input MATCH = LABEL\". MATCH is
# the same match the device's rule uses in [rules]. A label holds up to 40
# characters, spaces included. vtium-config writes this section when you
# label a part in its Flow view.
";

/// `text` with its `[labels]` section holding `lines`: the section, with
/// the comment above it, goes after every other section, and with no lines
/// it goes. vtium reads the section for its identify overlay (ADR-1401
/// D12).
pub fn labels(text: &str, lines: &[String]) -> String {
    let mut kept: Vec<&str> = text.split_inclusive('\n').collect();
    let is_heading = |l: &str| l.trim_start().starts_with('[');
    if let Some(start) = kept.iter().position(|l| l.trim() == "[labels]") {
        let end = kept
            .iter()
            .enumerate()
            .skip(start.saturating_add(1))
            .find(|(_, l)| is_heading(l))
            .map_or(kept.len(), |(i, _)| i);
        // the comment vtium-config wrote above the section, and the blank
        // line before it, go with it
        let mut from = start;
        for written in [LABELS_COMMENT, LABELS_COMMENT_TRIED] {
            let comment: Vec<&str> = written.split_inclusive('\n').collect();
            if start >= comment.len()
                && kept.get(start.saturating_sub(comment.len())..start) == Some(&comment[..])
            {
                from = start.saturating_sub(comment.len());
            }
        }
        if from > 0
            && kept
                .get(from.saturating_sub(1))
                .is_some_and(|l| l.trim().is_empty())
        {
            from = from.saturating_sub(1);
        }
        kept.drain(from..end);
    }
    let mut out: String = kept.concat();
    if lines.is_empty() {
        return out;
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(LABELS_COMMENT);
    out.push_str("[labels]\n");
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    const DESK: &str = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming  # every keyboard\n";

    // ADR-1403 D5, D8: `look add`, `look set` and the look card offer
    // every key a look takes, `cell` among them, by one option each
    #[test]
    fn the_look_keys_are_the_key_table_s_keys_of_a_look() {
        let mut ours: Vec<&str> = LOOK_KEYS.iter().map(|(k, _)| *k).collect();
        ours.sort_unstable();
        let mut table: Vec<&str> = vtium_settings::keys::at(vtium_settings::keys::Level::Look)
            .map(|k| k.name)
            .collect();
        table.sort_unstable();
        assert_eq!(ours, table);
        let mut options: Vec<&str> = LOOK_KEYS.iter().map(|(_, o)| *o).collect();
        options.sort_unstable();
        options.dedup();
        assert_eq!(options.len(), LOOK_KEYS.len(), "one option a key");
        assert_eq!(look_option("cell"), Some("--cell"));
        assert_eq!(look_option("login"), None);
    }

    #[test]
    fn a_look_is_added_with_its_keys_in_the_table_s_order() {
        let out = look_add(
            DESK,
            "big",
            &[
                ("cell", Some("12x24")),
                ("font-size", None),
                ("font-name", Some("monoid")),
            ],
        )
        .unwrap();
        assert!(
            out.contains("[look big]\nfont-name = monoid\ncell = 12x24\n"),
            "{out}"
        );
        let read = vtium_settings::sections(out.as_bytes()).unwrap();
        assert!(read.errors.is_empty(), "{:?}", read.errors);
        let look = read.parsed.config.looks.first().unwrap();
        let set: Vec<(&str, &str)> = look
            .settings
            .iter()
            .map(|s| (s.key.name, s.value.as_str()))
            .collect();
        assert_eq!(set, [("font-name", "monoid"), ("cell", "12x24")]);
    }

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
        assert_eq!(
            look_rm(text, "big", true).unwrap(),
            "version = 1\n[head desk]\n#look = big\n[rules]\ndisplay * -> desk\n"
        );
    }

    #[test]
    fn a_removed_head_leaves_its_rules_for_the_check_to_list() {
        let out = head_rm(DESK, "panel").unwrap();
        assert_eq!(out, DESK.replace("[head panel]\n", ""));
        let out = head_rm(DESK, "desk").unwrap();
        assert_eq!(rules_to(&out, "desk"), [1]);
        // and the broken rule is still the commands' to change
        assert_eq!(
            rule_set(
                &out,
                1,
                &RuleChange::Parts {
                    selector: None,
                    target: Some("panel".to_owned())
                }
            )
            .unwrap(),
            "version = 1\n[head panel]\n[rules]\ndisplay * -> panel\ninput class:keyboard -> roaming  # every keyboard\n"
        );
    }
}
