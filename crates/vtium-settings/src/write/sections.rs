//! The section writer of vtium.conf (ADR-1403 D7, D8): a head's or a
//! look's key set or unset, a section added, and a rule added, replaced,
//! moved or removed, each keeping every byte of the file it does not need.
//!
//! Every function here edits only a file whose sections and policy vtium
//! reads whole, by [`parse_sections`], finds the lines to change by the
//! line numbers that reading gives, and reads its own result the same way
//! before it hands it back, so it never hands back a file whose sections
//! vtium would read only in part. vtium-config's commands and its views
//! write the sections through these functions alone; the line editor,
//! [`edit_conf`](crate::edit_conf), writes the top level.

use crate::config::{Config, Rule};
use crate::error::Rejected;
use crate::identity::Kind;
use crate::keys::{self, Level};
use crate::matching::Match;
use crate::parse::{
    CONF_HEAD_KEYS, Parsed, RESERVED, VTLOOK, content, content_range, good_name, key_value,
    parse_sections, rule_line, rules_heading,
};
use crate::route::names_one_device;
use crate::split::{VERSION_KEY, heading, split};

/// A section of vtium.conf, by its kind and its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Of<'a> {
    /// `[head NAME]`.
    Head(&'a str),
    /// `[look NAME]`.
    Look(&'a str),
}

/// Why a section writer changed nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionEdit {
    /// The sections or the policy hold errors, so vtium reads them only in
    /// part or not at all; the writer edits only sections vtium reads
    /// whole.
    Unreadable(Rejected),
    /// No section of this kind and name.
    NoSection(String),
    /// A section of this kind and name is there already.
    Exists(String),
    /// A name vtium would refuse for a section of this kind.
    BadName(String),
    /// A key a section of this kind does not take.
    BadKey(String),
    /// A value vtium would read back as another value, or not at all.
    BadValue { key: String, value: String },
    /// No rule of this number, counting the rules from 1.
    NoRule(usize),
    /// Text that is not one rule vtium reads.
    BadRule(Rejected),
    /// The edit would leave the sections with an error, such as a value
    /// out of bounds or a rule naming a head no section declares, so the
    /// writer hands nothing back.
    Refused(Rejected),
}

/// The file's lines, each with its ending, and the ending a new line takes:
/// the first line's.
struct Lines {
    lines: Vec<String>,
    newline: &'static str,
}

impl Lines {
    fn of(text: &str) -> Lines {
        let lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
        let newline = match lines.first() {
            Some(l) if l.ends_with("\r\n") => "\r\n",
            _ => "\n",
        };
        Lines { lines, newline }
    }

    /// The line numbered `n` from 1, without its ending.
    fn body(&self, n: usize) -> Option<&str> {
        let line = self.lines.get(n.checked_sub(1)?)?;
        Some(line.trim_end_matches(['\n', '\r']))
    }

    /// Inserts `text` as a whole line before the line numbered `n`, or at
    /// the end past the last line, giving the line before it an ending
    /// where it has none.
    fn insert(&mut self, n: usize, text: &str) {
        let at = n.saturating_sub(1).min(self.lines.len());
        if let Some(before) = at.checked_sub(1).and_then(|i| self.lines.get_mut(i))
            && !before.ends_with('\n')
        {
            before.push_str(self.newline);
        }
        self.lines.insert(at, format!("{text}{}", self.newline));
    }

    /// Replaces the bytes `range` of the line numbered `n`.
    fn replace(&mut self, n: usize, range: std::ops::Range<usize>, with: &str) {
        if let Some(line) = n.checked_sub(1).and_then(|i| self.lines.get_mut(i))
            && line.get(range.clone()).is_some()
        {
            line.replace_range(range, with);
        }
    }

    /// The number of the line past the last one.
    fn end(&self) -> usize {
        self.lines.len().saturating_add(1)
    }

    /// The number of the first heading line whose content `wanted` takes.
    fn heading(&self, wanted: impl Fn(&str) -> bool) -> Option<usize> {
        (1..self.end()).find(|&n| {
            self.body(n)
                .is_some_and(|b| heading(b.as_bytes()) && wanted(&content(b)))
        })
    }

    /// The number of the line after the section whose heading is line
    /// `start`: the next heading, or the end.
    fn section_end(&self, start: usize) -> usize {
        (start.saturating_add(1)..self.end())
            .find(|&n| self.body(n).is_some_and(|b| heading(b.as_bytes())))
            .unwrap_or_else(|| self.end())
    }

    fn text(&self) -> String {
        self.lines.concat()
    }
}

/// The file's sections read whole: `Ok(None)` for a file with no flow.
fn readable(text: &str) -> Result<Option<Parsed>, SectionEdit> {
    parse_sections(text).map_err(SectionEdit::Unreadable)
}

/// `out` once its sections read whole, else why the writer refuses it.
fn checked(out: String) -> Result<String, SectionEdit> {
    match parse_sections(&out) {
        Ok(_) => Ok(out),
        Err(rejected) => Err(SectionEdit::Refused(rejected)),
    }
}

/// Whether a section of kind `of` takes the key `key` (ADR-1403 D5).
fn takes(of: Of<'_>, key: &str) -> bool {
    let (own, level): (&[&str], Level) = match of {
        Of::Head(_) => (&CONF_HEAD_KEYS, Level::Head),
        Of::Look(_) => (&[VTLOOK], Level::Look),
    };
    own.contains(&key) || keys::at(level).any(|k| k.name == key)
}

/// The line of the section `of`'s heading in `config`.
fn section_line(config: &Config, of: Of<'_>) -> Option<usize> {
    match of {
        Of::Head(name) => config.head(name).map(|h| h.line),
        Of::Look(name) => config.looks.iter().find(|l| l.name == name).map(|l| l.line),
    }
}

/// Sets the key `key` of the section `of` to `value`, or with `None`
/// unsets it, and returns the whole file with only that line changed.
///
/// A key the section sets has its value replaced in place, the blanks and
/// any comment after it kept. A key it does not set gains the line
/// `key = value` directly below the section's heading. Unsetting comments
/// out the key's line, so it is there to set again; a key the section
/// does not set leaves the file as it is.
pub fn set_key(
    text: &str,
    of: Of<'_>,
    key: &str,
    value: Option<&str>,
) -> Result<String, SectionEdit> {
    if !takes(of, key) {
        return Err(SectionEdit::BadKey(key.to_owned()));
    }
    if let Some(value) = value {
        let line = format!("{key} = {value}");
        let reads = !value.contains(['\n', '\r'])
            && key_value(&content(&line)) == Some((key, value))
            && !value.is_empty();
        if !reads {
            return Err(SectionEdit::BadValue {
                key: key.to_owned(),
                value: value.to_owned(),
            });
        }
    }
    let config = readable(text)?.map(|p| p.config).unwrap_or_default();
    let name = match of {
        Of::Head(name) | Of::Look(name) => name,
    };
    let start = section_line(&config, of).ok_or_else(|| SectionEdit::NoSection(name.to_owned()))?;
    let mut lines = Lines::of(text);
    let end = lines.section_end(start);
    let set = (start.saturating_add(1)..end).find(|&n| {
        lines
            .body(n)
            .is_some_and(|b| key_value(&content(b)).is_some_and(|(k, _)| k == key))
    });
    match (set, value) {
        (Some(n), Some(value)) => {
            let body = lines.body(n).unwrap_or_default().to_owned();
            let range = content_range(&body);
            let inner = body.get(range.clone()).unwrap_or_default();
            // the value runs from after `=` and its blanks to the content's end
            let Some(eq) = inner.find('=') else {
                return Err(SectionEdit::BadKey(key.to_owned()));
            };
            let after = inner.get(eq.saturating_add(1)..).unwrap_or_default();
            let lead = after
                .len()
                .saturating_sub(after.trim_start_matches([' ', '\t']).len());
            let from = range
                .start
                .saturating_add(eq)
                .saturating_add(1)
                .saturating_add(lead);
            lines.replace(n, from..range.end.max(from), value);
        }
        (Some(n), None) => lines.replace(n, 0..0, "#"),
        (None, Some(value)) => lines.insert(start.saturating_add(1), &format!("{key} = {value}")),
        (None, None) => return Ok(text.to_owned()),
    }
    checked(lines.text())
}

/// Adds the line `version = 1` at the end of a file whose top level has
/// no `version` line, as a file with no section may have, which it needs
/// once it has a section (ADR-1403 D3).
fn give_flow(lines: &mut Lines, text: &str) {
    let versioned = split(text.as_bytes())
        .top_lines()
        .any(|(_, l)| crate::conf::key_line(l).is_some_and(|k| k.key == VERSION_KEY.as_bytes()));
    if !versioned {
        let end = lines.end();
        lines.insert(end, &format!("{VERSION_KEY} = 1"));
    }
}

/// Adds the empty section `of` and returns the whole file with only its
/// lines added: a look before the first head, else before `[rules]`, else
/// at the end; a head before `[rules]`, else at the end. A file with no
/// flow gains `version = 1` too.
pub fn add_section(text: &str, of: Of<'_>) -> Result<String, SectionEdit> {
    let (name, heading_text) = match of {
        Of::Head(name) => (name, format!("[head {name}]")),
        Of::Look(name) => (name, format!("[look {name}]")),
    };
    let reserved = matches!(of, Of::Head(_))
        && (RESERVED.contains(&name) || ["focus", "dark"].contains(&name));
    if !good_name(name) || reserved {
        return Err(SectionEdit::BadName(name.to_owned()));
    }
    let parsed = readable(text)?;
    let config = parsed.as_ref().map(|p| &p.config);
    if config.is_some_and(|c| section_line(c, of).is_some()) {
        return Err(SectionEdit::Exists(name.to_owned()));
    }
    let mut lines = Lines::of(text);
    let rules = lines.heading(rules_heading);
    // the first head's heading, as the reading found it
    let first_head = config.and_then(|c| c.heads.iter().map(|h| h.line).min());
    let before = match of {
        Of::Look(_) => first_head.or(rules),
        Of::Head(_) => rules,
    };
    // a heading always follows the top level, so a version line added at
    // the file's end comes before the new section only in a file with none
    if before.is_none() {
        give_flow(&mut lines, text);
    }
    match before {
        Some(n) => {
            lines.insert(n, &heading_text);
            lines.insert(n.saturating_add(1), "");
        }
        None => {
            let end = lines.end();
            let blank_before = end > 1
                && lines
                    .body(end.saturating_sub(1))
                    .is_some_and(|b| !b.trim().is_empty());
            if blank_before {
                lines.insert(end, "");
            }
            let end = lines.end();
            lines.insert(end, &heading_text);
        }
    }
    checked(lines.text())
}

/// Where a new rule of `kind` that matches `matcher` goes among `rules`,
/// as the index of the rule it goes before, or `rules.len()` for after
/// the last (ADR-1403 D7), so that an earlier, wider rule does not match
/// its devices first. A rule that names one device, by `edid`,
/// `connector`, `connector-path`, `serial`, `path`, `bluetooth` or `name`,
/// goes before the first general rule of its kind, one that names a kind
/// of device by `class`, `id`, `card`, `udev` or `*`. A general rule goes
/// before the first rule of its kind whose match is `*`. With nothing of
/// the sort to go before, a rule goes after the last rule. The flow
/// view's `new rule…`, connecting tags and `vtium config rule add`
/// without `--before` all place a rule by this.
pub fn placement(rules: &[Rule], kind: Kind, matcher: &Match) -> usize {
    let at = if names_one_device(matcher) {
        rules
            .iter()
            .position(|r| r.kind == kind && !names_one_device(&r.matcher))
    } else {
        rules
            .iter()
            .position(|r| r.kind == kind && r.matcher == Match::Any)
    };
    at.unwrap_or(rules.len())
}

/// The rules of a readable file, and the file's lines.
fn rules_of(text: &str) -> Result<(Vec<Rule>, Lines), SectionEdit> {
    let rules = readable(text)?.map(|p| p.config.rules).unwrap_or_default();
    Ok((rules, Lines::of(text)))
}

/// The rule numbered `number`, counting from 1.
fn numbered(rules: &[Rule], number: usize) -> Result<&Rule, SectionEdit> {
    number
        .checked_sub(1)
        .and_then(|i| rules.get(i))
        .ok_or(SectionEdit::NoRule(number))
}

/// `rule` as one rule vtium reads, trimmed, else why not.
fn one_rule(rule: &str) -> Result<(Kind, Match, &str), SectionEdit> {
    if rule.contains(['\n', '\r']) {
        return Err(SectionEdit::BadValue {
            key: "rule".to_owned(),
            value: rule.to_owned(),
        });
    }
    let (kind, matcher) = rule_line(rule).map_err(SectionEdit::BadRule)?;
    Ok((kind, matcher, rule.trim_matches([' ', '\t'])))
}

/// Adds the rule `rule`, one line such as `input class:keyboard ->
/// roaming`, and returns the whole file with only that line added: before
/// the rule numbered `before`, counting the rules from 1, where `before`
/// is past the last rule after it, or with `None` where [`placement`]
/// puts it. A file with no `[rules]` section gains one at its end, and a
/// file with no flow gains `version = 1` too.
pub fn add_rule(text: &str, rule: &str, before: Option<usize>) -> Result<String, SectionEdit> {
    let (kind, matcher, rule) = one_rule(rule)?;
    let (rules, mut lines) = rules_of(text)?;
    let index = match before {
        Some(n) if (1..=rules.len().saturating_add(1)).contains(&n) => n.saturating_sub(1),
        Some(n) => return Err(SectionEdit::NoRule(n)),
        None => placement(&rules, kind, &matcher),
    };
    match (rules.get(index), rules.last()) {
        (Some(next), _) => lines.insert(next.line, rule),
        (None, Some(last)) => lines.insert(last.line.saturating_add(1), rule),
        (None, None) => match lines.heading(rules_heading) {
            Some(n) => lines.insert(n.saturating_add(1), rule),
            None => {
                give_flow(&mut lines, text);
                let end = lines.end();
                lines.insert(end, "[rules]");
                let end = lines.end();
                lines.insert(end, rule);
            }
        },
    }
    checked(lines.text())
}

/// Replaces the rule numbered `number` with `rule`, keeping the line's
/// blanks and any comment after it.
pub fn set_rule(text: &str, number: usize, rule: &str) -> Result<String, SectionEdit> {
    let (_, _, rule) = one_rule(rule)?;
    let (rules, mut lines) = rules_of(text)?;
    let line = numbered(&rules, number)?.line;
    let range = content_range(lines.body(line).unwrap_or_default());
    lines.replace(line, range, rule);
    checked(lines.text())
}

/// Moves the rule numbered `number` to go before the rule numbered
/// `before`, or after the last where `before` is past it, keeping its
/// line whole.
pub fn move_rule(text: &str, number: usize, before: usize) -> Result<String, SectionEdit> {
    let (rules, mut lines) = rules_of(text)?;
    let from = numbered(&rules, number)?.line;
    let to = match before.checked_sub(1).map(|i| rules.get(i)) {
        Some(Some(next)) => next.line,
        Some(None) if before == rules.len().saturating_add(1) => {
            rules.last().map_or(from, |r| r.line.saturating_add(1))
        }
        _ => return Err(SectionEdit::NoRule(before)),
    };
    if to == from || to == from.saturating_add(1) {
        return Ok(text.to_owned());
    }
    let Some(moved) = from.checked_sub(1).map(|i| lines.lines.remove(i)) else {
        return Err(SectionEdit::NoRule(number));
    };
    let to = if to > from { to.saturating_sub(1) } else { to };
    let body = moved.trim_end_matches(['\n', '\r']).to_owned();
    lines.insert(to, &body);
    checked(lines.text())
}

/// Removes the rule numbered `number`, its whole line.
pub fn remove_rule(text: &str, number: usize) -> Result<String, SectionEdit> {
    let (rules, mut lines) = rules_of(text)?;
    let line = numbered(&rules, number)?.line;
    if let Some(i) = line.checked_sub(1)
        && i < lines.lines.len()
    {
        lines.lines.remove(i);
    }
    checked(lines.text())
}
