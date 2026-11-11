//! The section writer of vtium.conf (ADR-1403 D7, D8): a head's or a
//! look's key set or unset, a section added, removed or renamed, and a
//! rule added, replaced, moved or removed, each keeping every byte of the
//! file it does not need.
//!
//! A draft may be unfinished (ADR-1403 D8), so the writer edits a file
//! whatever `vtium check-config` would say of it or of the result: a rule
//! that sends to a head no section declares, a value out of bounds, a key
//! given twice. The check reports those, at an install and in
//! `vtium check-config`. The writer finds what it edits by the file's
//! lines alone, the section headings and the lines of `[rules]`, and
//! refuses only an edit it cannot place: a section or a rule that is not
//! there, a name or a heading given twice, or text it cannot write as one
//! line that reads back as what it was asked to write. vtium-config's
//! commands and its views write the sections through these functions
//! alone; the line editor, [`edit_conf`](crate::edit_conf), writes the top
//! level.

use crate::config::Rule;
use crate::error::Rejected;
use crate::identity::Kind;
use crate::keys::{self, Level};
use crate::matching::Match;
use crate::parse::{
    CONF_HEAD_KEYS, RESERVED, VTLOOK, content, content_range, good_name, key_value, rule_line,
    rules_heading,
};
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
    /// No section of this kind and name.
    NoSection(String),
    /// A section of this kind and name is there already.
    Exists(String),
    /// A name vtium would refuse for a section of this kind.
    BadName(String),
    /// A key a section of this kind does not take, with the kind of
    /// section, so the message can say where the key is set.
    BadKey { key: String, section: Level },
    /// A value vtium would read back as another value, or not at all.
    BadValue { key: String, value: String },
    /// No rule of this number, counting the rules from 1.
    NoRule(usize),
    /// Text that is not one rule vtium reads.
    BadRule(Rejected),
    /// Two sections of this kind and name, so the writer cannot tell which
    /// one it is asked to edit.
    Ambiguous(String),
}

/// The file's lines, each with its ending, the ending a new line takes,
/// the first line's, and the byte-order mark before the first line, which
/// is the file's and no part of the line, as the reader and the split read
/// it, so a heading after it is still a heading.
struct Lines {
    lines: Vec<String>,
    newline: &'static str,
    bom: &'static str,
}

impl Lines {
    fn of(text: &str) -> Lines {
        const BOM: &str = "\u{feff}";
        let (bom, text) = match text.strip_prefix(BOM) {
            Some(rest) => (BOM, rest),
            None => ("", text),
        };
        let lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
        let newline = match lines.first() {
            Some(l) if l.ends_with("\r\n") => "\r\n",
            _ => "\n",
        };
        Lines {
            lines,
            newline,
            bom,
        }
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
        let mut text = self.bom.to_owned();
        text.push_str(&self.lines.concat());
        text
    }
}

/// What a section heading's content names: its kind and its name, or
/// `None` for the heading of `[rules]` or one the writer cannot read.
fn section_of(content: &str) -> Option<(&str, &str)> {
    let inner = content.strip_prefix('[')?.strip_suffix(']')?;
    let mut words = inner.split([' ', '\t']).filter(|w| !w.is_empty());
    match (words.next(), words.next(), words.next()) {
        (Some(kind @ ("head" | "look")), Some(name), None) => Some((kind, name)),
        _ => None,
    }
}

/// The kind word of `of`'s heading, and its name.
fn kind_name<'a>(of: Of<'a>) -> (&'static str, &'a str) {
    match of {
        Of::Head(name) => ("head", name),
        Of::Look(name) => ("look", name),
    }
}

impl Lines {
    /// The number of every heading line, with its content.
    fn headings(&self) -> Vec<(usize, String)> {
        (1..self.end())
            .filter_map(|n| {
                let b = self.body(n)?;
                heading(b.as_bytes()).then(|| (n, content(b)))
            })
            .collect()
    }

    /// The line of the heading of the section `of`: `None` when there is
    /// none, and an error when two headings name it.
    fn section(&self, of: Of<'_>) -> Result<Option<usize>, SectionEdit> {
        let (kind, name) = kind_name(of);
        let found: Vec<usize> = self
            .headings()
            .into_iter()
            .filter(|(_, c)| section_of(c) == Some((kind, name)))
            .map(|(n, _)| n)
            .collect();
        match found.as_slice() {
            [] => Ok(None),
            [n] => Ok(Some(*n)),
            _ => Err(SectionEdit::Ambiguous(name.to_owned())),
        }
    }

    /// The heading of the section `of`, which must be there.
    fn found(&self, of: Of<'_>) -> Result<usize, SectionEdit> {
        self.section(of)?
            .ok_or_else(|| SectionEdit::NoSection(kind_name(of).1.to_owned()))
    }

    /// The number of each line of every `[rules]` section that holds
    /// content, in order: the rules, numbered from 1 by their place here.
    fn rules(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut inside = false;
        for n in 1..self.end() {
            let Some(b) = self.body(n) else { continue };
            if heading(b.as_bytes()) {
                inside = rules_heading(&content(b));
            } else if inside && !content(b).is_empty() {
                out.push(n);
            }
        }
        out
    }

    /// The line of the last line of the section whose heading is line
    /// `start` that holds content, the heading itself when none does, so
    /// the comments and blanks before the next heading stay with it.
    fn last_content(&self, start: usize) -> usize {
        (start..self.section_end(start))
            .rev()
            .find(|&n| self.body(n).is_some_and(|b| !content(b).is_empty()))
            .unwrap_or(start)
    }

    /// Removes the lines `from` to `to`, both counted.
    fn remove(&mut self, from: usize, to: usize) {
        let (Some(a), Some(b)) = (from.checked_sub(1), to.checked_sub(1)) else {
            return;
        };
        if a <= b && b < self.lines.len() {
            self.lines.drain(a..=b);
        }
    }
}

/// The rule lines of `text`, by their line numbers from 1: each line of a
/// `[rules]` section that holds content, which the commands number from 1
/// in this order whether or not vtium reads every one of them.
pub fn rule_lines(text: &str) -> Vec<usize> {
    Lines::of(text).rules()
}

/// The refusal of the key `key`, which a section of kind `of` does not
/// take.
fn bad_key(of: Of<'_>, key: &str) -> SectionEdit {
    let section = match of {
        Of::Head(_) => Level::Head,
        Of::Look(_) => Level::Look,
    };
    SectionEdit::BadKey {
        key: key.to_owned(),
        section,
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
        return Err(bad_key(of, key));
    }
    if let Some(value) = value {
        let line = format!("{key} = {value}");
        let reads = one_line(value)
            && key_value(&content(&line)) == Some((key, value))
            && !value.is_empty();
        if !reads {
            return Err(SectionEdit::BadValue {
                key: key.to_owned(),
                value: value.to_owned(),
            });
        }
    }
    let mut lines = Lines::of(text);
    let start = lines.found(of)?;
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
                return Err(bad_key(of, key));
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
    Ok(lines.text())
}

/// Adds the line `version = 1` at the end of the top level, before the
/// first section heading or at the file's end, when the top level has no
/// `version` line, as a file with no section may have, which it needs once
/// it has a section (ADR-1403 D3).
fn give_flow(lines: &mut Lines, text: &str) {
    let versioned = split(text.as_bytes())
        .top_lines()
        .any(|(_, l)| crate::conf::key_line(l).is_some_and(|k| k.key == VERSION_KEY.as_bytes()));
    if !versioned {
        let top_end = lines.heading(|_| true).unwrap_or_else(|| lines.end());
        lines.insert(top_end, &format!("{VERSION_KEY} = 1"));
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
    if !named_well(of, name) {
        return Err(SectionEdit::BadName(name.to_owned()));
    }
    let mut lines = Lines::of(text);
    if lines.section(of)?.is_some() {
        return Err(SectionEdit::Exists(name.to_owned()));
    }
    let rules = lines.heading(rules_heading);
    let first_head = lines
        .headings()
        .into_iter()
        .find(|(_, c)| section_of(c).is_some_and(|(k, _)| k == "head"))
        .map(|(n, _)| n);
    let before = match of {
        Of::Look(_) => first_head.or(rules),
        Of::Head(_) => rules,
    };
    // the version line goes before the first heading, so the new section
    // at the file's end comes after it
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
    Ok(lines.text())
}

/// Whether `name` may name a section of kind `of`: the name rule, and for
/// a head none of the targets' names.
fn named_well(of: Of<'_>, name: &str) -> bool {
    let reserved = matches!(of, Of::Head(_))
        && (RESERVED.contains(&name) || ["focus", "dark"].contains(&name));
    good_name(name) && !reserved
}

/// Removes the section `of`, its heading and its lines up to its last
/// line that holds content, and returns the whole file with only those
/// lines gone. The comments before the next heading stay, as the next
/// section's, and the blank line that only separated the section from the
/// next heading goes with it. A
/// head's rules stay where they are, sending to a head the file no longer
/// declares, which the check lists until each is pointed elsewhere or
/// removed (ADR-1403 D7).
pub fn remove_section(text: &str, of: Of<'_>) -> Result<String, SectionEdit> {
    let mut lines = Lines::of(text);
    let start = lines.found(of)?;
    let last = lines.last_content(start);
    lines.remove(start, last);
    // a blank line left at the cut that only separated the section from
    // the next heading, or that doubles the blank line before it, goes
    // with the section
    let blank = |l: &Lines, n: usize| l.body(n).is_some_and(|b| b.trim().is_empty());
    let next = start.saturating_add(1);
    let separates = next >= lines.end() || lines.body(next).is_some_and(|b| heading(b.as_bytes()));
    let doubles = start == 1 || blank(&lines, start.saturating_sub(1));
    if blank(&lines, start) && (separates || doubles) {
        lines.remove(start, start);
    }
    Ok(lines.text())
}

/// Renames the section `of` to `new`, and returns the whole file with only
/// the lines that name it changed: its heading, and for a head each rule
/// that sends to it, for a look each head's `look` line that names it.
/// The rename is one change, so nothing is left naming the old name.
pub fn rename_section(text: &str, of: Of<'_>, new: &str) -> Result<String, SectionEdit> {
    let (kind, old) = kind_name(of);
    if !named_well(of, new) {
        return Err(SectionEdit::BadName(new.to_owned()));
    }
    let mut lines = Lines::of(text);
    let start = lines.found(of)?;
    let renamed = match of {
        Of::Head(_) => Of::Head(new),
        Of::Look(_) => Of::Look(new),
    };
    if old != new && lines.section(renamed)?.is_some() {
        return Err(SectionEdit::Exists(new.to_owned()));
    }
    let range = content_range(lines.body(start).unwrap_or_default());
    lines.replace(start, range, &format!("[{kind} {new}]"));
    match of {
        Of::Head(_) => {
            for n in lines.rules() {
                let body = lines.body(n).unwrap_or_default().to_owned();
                let range = content_range(&body);
                let inner = body.get(range.clone()).unwrap_or_default();
                let Some(arrow) = inner.rfind("->") else {
                    continue;
                };
                let after = inner.get(arrow.saturating_add(2)..).unwrap_or_default();
                if after.trim_matches([' ', '\t']) != old {
                    continue;
                }
                let lead = after
                    .len()
                    .saturating_sub(after.trim_start_matches([' ', '\t']).len());
                let from = range
                    .start
                    .saturating_add(arrow)
                    .saturating_add(2)
                    .saturating_add(lead);
                lines.replace(n, from..range.end, new);
            }
        }
        Of::Look(_) => {
            let heads: Vec<usize> = lines
                .headings()
                .into_iter()
                .filter(|(_, c)| section_of(c).is_some_and(|(k, _)| k == "head"))
                .map(|(n, _)| n)
                .collect();
            for head in heads {
                for n in head.saturating_add(1)..lines.section_end(head) {
                    let body = lines.body(n).unwrap_or_default().to_owned();
                    let names_it = key_value(&content(&body)) == Some(("look", old));
                    if !names_it {
                        continue;
                    }
                    let range = content_range(&body);
                    let inner = body.get(range.clone()).unwrap_or_default();
                    let Some(eq) = inner.find('=') else { continue };
                    let after = inner.get(eq.saturating_add(1)..).unwrap_or_default();
                    let lead = after
                        .len()
                        .saturating_sub(after.trim_start_matches([' ', '\t']).len());
                    let from = range
                        .start
                        .saturating_add(eq)
                        .saturating_add(1)
                        .saturating_add(lead);
                    lines.replace(n, from..range.end, new);
                }
            }
        }
    }
    Ok(lines.text())
}

/// Where a new rule of `kind` that matches `matcher` goes among `rules`,
/// as the index of the rule it goes before, or `rules.len()` for after
/// the last (ADR-1403 D8): just before the first rule of its kind whose
/// rank, [`Match::rank`], is lower, so that a wider rule does not match
/// the new rule's devices first. With no such rule it goes after the last
/// rule. The Flow view's writes and `vtium config rule add` without
/// `--before` both place a rule by this.
pub fn placement(rules: &[Rule], kind: Kind, matcher: &Match) -> usize {
    let read: Vec<Option<(Kind, Match)>> = rules
        .iter()
        .map(|r| Some((r.kind, r.matcher.clone())))
        .collect();
    place(&read, kind, matcher)
}

/// [`placement`] over rules each read alone, `None` for a rule line the
/// writer cannot read, which no new rule goes before by its kind.
fn place(rules: &[Option<(Kind, Match)>], kind: Kind, matcher: &Match) -> usize {
    let rank = matcher.rank();
    rules
        .iter()
        .position(|r| {
            r.as_ref()
                .is_some_and(|(k, m)| *k == kind && m.rank() > rank)
        })
        .unwrap_or(rules.len())
}

/// The rules of `lines`, each by its line and as it reads alone.
fn read_rules(lines: &Lines) -> Vec<(usize, Option<(Kind, Match)>)> {
    lines
        .rules()
        .into_iter()
        .map(|n| (n, rule_line(lines.body(n).unwrap_or_default()).ok()))
        .collect()
}

/// The line of the rule numbered `number`, counting from 1.
fn numbered(lines: &Lines, number: usize) -> Result<usize, SectionEdit> {
    number
        .checked_sub(1)
        .and_then(|i| lines.rules().get(i).copied())
        .ok_or(SectionEdit::NoRule(number))
}

/// Whether `text` can be written into one line that vtium reads: it holds
/// no control character but a tab, since a newline or a carriage return
/// would split the line, and the reader takes any other as an error on its
/// line.
fn one_line(text: &str) -> bool {
    !text.chars().any(|c| c.is_control() && c != '\t')
}

/// `rule` as one rule vtium reads, trimmed, else why not.
fn one_rule(rule: &str) -> Result<(Kind, Match, &str), SectionEdit> {
    if !one_line(rule) {
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
    let mut lines = Lines::of(text);
    let rules = read_rules(&lines);
    let index = match before {
        Some(n) if (1..=rules.len().saturating_add(1)).contains(&n) => n.saturating_sub(1),
        Some(n) => return Err(SectionEdit::NoRule(n)),
        None => {
            let read: Vec<Option<(Kind, Match)>> = rules.iter().map(|(_, r)| r.clone()).collect();
            place(&read, kind, &matcher)
        }
    };
    match (rules.get(index), rules.last()) {
        (Some((next, _)), _) => lines.insert(*next, rule),
        (None, Some((last, _))) => lines.insert(last.saturating_add(1), rule),
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
    Ok(lines.text())
}

/// Replaces the rule numbered `number` with `rule`, keeping the line's
/// blanks and any comment after it.
pub fn set_rule(text: &str, number: usize, rule: &str) -> Result<String, SectionEdit> {
    let (_, _, rule) = one_rule(rule)?;
    let mut lines = Lines::of(text);
    let line = numbered(&lines, number)?;
    let range = content_range(lines.body(line).unwrap_or_default());
    lines.replace(line, range, rule);
    Ok(lines.text())
}

/// Moves the rule numbered `number` to go before the rule numbered
/// `before`, or after the last where `before` is past it, keeping its
/// line whole.
pub fn move_rule(text: &str, number: usize, before: usize) -> Result<String, SectionEdit> {
    let mut lines = Lines::of(text);
    let rules = lines.rules();
    let from = numbered(&lines, number)?;
    let to = match before.checked_sub(1).map(|i| rules.get(i)) {
        Some(Some(next)) => *next,
        Some(None) if before == rules.len().saturating_add(1) => {
            rules.last().map_or(from, |r| r.saturating_add(1))
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
    Ok(lines.text())
}

/// Removes the rule numbered `number`, its whole line.
pub fn remove_rule(text: &str, number: usize) -> Result<String, SectionEdit> {
    let mut lines = Lines::of(text);
    let line = numbered(&lines, number)?;
    lines.remove(line, line);
    Ok(lines.text())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_order_mark_before_the_first_heading_is_no_part_of_it() {
        let text = "\u{feff}[rules]\ndisplay * -> a\n";
        assert_eq!(
            add_rule(text, "input * -> roaming", None),
            Ok("\u{feff}[rules]\ndisplay * -> a\ninput * -> roaming\n".to_owned())
        );
        assert_eq!(
            set_key("\u{feff}[head a]\n", Of::Head("a"), "look", Some("big")),
            Ok("\u{feff}[head a]\nlook = big\n".to_owned())
        );
        assert_eq!(
            rename_section("\u{feff}[head a] # desk\n", Of::Head("a"), "b"),
            Ok("\u{feff}[head b] # desk\n".to_owned())
        );
        assert_eq!(
            remove_section("\u{feff}[look a]\n[head b]\n", Of::Look("a")),
            Ok("\u{feff}[head b]\n".to_owned())
        );
    }

    #[test]
    fn a_control_character_is_refused_since_vtium_cannot_read_its_line() {
        let text = "version = 1\n[look a]\n[rules]\n";
        assert!(matches!(
            set_key(text, Of::Look("a"), "font-size", Some("1\u{0}2")),
            Err(SectionEdit::BadValue { .. })
        ));
        assert!(matches!(
            add_rule(text, "input name:k\u{1b}b -> a", None),
            Err(SectionEdit::BadValue { .. })
        ));
        assert!(matches!(
            set_rule("[rules]\ninput * -> a\n", 1, "input name:\u{7f} -> a"),
            Err(SectionEdit::BadValue { .. })
        ));
        assert!(set_key(text, Of::Look("a"), "font-name", Some("a\tb")).is_ok());
    }

    #[test]
    fn a_version_line_the_writer_adds_goes_to_the_top_level() {
        assert_eq!(
            add_rule("# mine\n[head b]\n", "display * -> b", None),
            Ok("# mine\nversion = 1\n[head b]\n[rules]\ndisplay * -> b\n".to_owned())
        );
        assert_eq!(
            add_section("[look a]\n", Of::Head("h")),
            Ok("version = 1\n[look a]\n\n[head h]\n".to_owned())
        );
        assert_eq!(
            add_section("font-size = 12", Of::Head("h")),
            Ok("font-size = 12\nversion = 1\n\n[head h]\n".to_owned())
        );
    }
}
