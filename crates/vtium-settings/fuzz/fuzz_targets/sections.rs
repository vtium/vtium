//! A generated vtium.conf through a generated sequence of the section
//! writer's edits (ADR-1403 D7, D8): `set_key`, `add_section`,
//! `remove_section`, `rename_section`, `add_rule`, `set_rule`, `move_rule`
//! and `remove_rule`. After each edit the writer accepts, the target checks
//! three things against the text the edit started from:
//!
//! - the result reads without a structural error that the text before the
//!   edit did not have, such as a heading vtium cannot read, a line in no
//!   section's syntax, or a `version` line inside a section;
//! - the top level's key lines are the same lines in the same order, apart
//!   from the `version = 1` line a file with no flow gains, and no section
//!   moves or disappears except the one the edit names;
//! - every line the edit does not need is kept byte for byte, comments
//!   included, and the lines the edit writes are the ones it was asked to
//!   write.
//!
//! The text is built from lines vtium.conf holds (headings, keys, rules,
//! comments and blank lines) and from any text, with either line ending,
//! with or without a last newline and a byte-order mark.
#![no_main]

use libfuzzer_sys::arbitrary::{Result, Unstructured};
use libfuzzer_sys::fuzz_target;
use vtium_settings::{
    ERRORS_MAX, Of, Problem, Reading, add_rule, add_section, key_line, move_rule, reading,
    remove_rule, remove_section, rename_section, rule_lines, set_key, set_rule, split,
};

/// Lines vtium.conf holds, which the text is built from.
const LINES: &[&str] = &[
    "version = 1",
    "version = 1  # the flow",
    "policy = open",
    "font-size = 12",
    "[head a]",
    "[head b]",
    "  [ head   a ]  # the desk",
    "[head desk]",
    "[look big]",
    "[look a]",
    "[rules]",
    "[ rules ]",
    "[head a b]",
    "[head",
    "[profile p]",
    "[guards]",
    "",
    "  ",
    "# a comment",
    "\t# an indented comment",
    "## a doubled comment",
    "\\# not a comment",
    "look = big",
    "look = big # which look",
    "consoles = 2",
    "isolated = yes",
    "when-display-lost = keep",
    "vtlook = x",
    "font-size = 12 # big",
    "palette = #ff0000",
    "display * -> a",
    "display edid:ABC -> b # the desk",
    "input class:keyboard -> a",
    "input * -> roaming",
    "display connector:HDMI-A-1 -> desk",
    "input name:kbd -> a#b",
];

const NAMES: &[&str] = &["a", "b", "big", "desk", "p", "x y", "", "roaming", "focus"];
const KEYS: &[&str] = &[
    "look",
    "consoles",
    "isolated",
    "switch-on-enter",
    "when-display-lost",
    "vtlook",
    "font-size",
    "font-name",
    "palette",
    "version",
];
const VALUES: &[&str] = &[
    "big", "2", "yes", "keep", "x", "a # b", "#ff0000", "a#b", "",
];
const RULES: &[&str] = &[
    "display * -> a",
    "input class:keyboard -> roaming",
    "display edid:ABC -> b",
    "  input name:kbd -> a ",
    "display connector:HDMI-A-1 -> desk",
    "input * -> big",
];

/// One word from `list`, or any text.
fn pick<'a>(u: &mut Unstructured<'a>, list: &[&'static str]) -> Result<&'a str> {
    let i = u.int_in_range(0..=list.len())?;
    match list.get(i) {
        Some(s) => Ok(s),
        None => u.arbitrary(),
    }
}

fn text(u: &mut Unstructured<'_>) -> Result<String> {
    let mut out = String::new();
    if u.ratio(1, 16)? {
        out.push('\u{feff}');
    }
    let n = u.int_in_range(0..=40)?;
    for i in 0..n {
        out.push_str(pick(u, LINES)?);
        let last = i + 1 == n;
        if !(last && u.ratio(1, 4)?) {
            out.push_str(if u.ratio(1, 6)? { "\r\n" } else { "\n" });
        }
    }
    Ok(out)
}

fn section<'a>(u: &mut Unstructured<'a>) -> Result<Of<'a>> {
    let name = pick(u, NAMES)?;
    Ok(if u.arbitrary()? {
        Of::Head(name)
    } else {
        Of::Look(name)
    })
}

/// A rule's number, counting from 1, mostly near the rules there are.
fn number(u: &mut Unstructured<'_>) -> Result<usize> {
    Ok(usize::from(u.int_in_range(0..=12_u8)?))
}

#[derive(Debug)]
enum Op<'a> {
    SetKey(Of<'a>, &'a str, Option<&'a str>),
    AddSection(Of<'a>),
    RemoveSection(Of<'a>),
    RenameSection(Of<'a>, &'a str),
    AddRule(&'a str, Option<usize>),
    SetRule(usize, &'a str),
    MoveRule(usize, usize),
    RemoveRule(usize),
}

fn op<'a>(u: &mut Unstructured<'a>) -> Result<Op<'a>> {
    Ok(match u.int_in_range(0..=8_u8)? {
        0 => Op::SetKey(section(u)?, pick(u, KEYS)?, Some(pick(u, VALUES)?)),
        1 => Op::SetKey(section(u)?, pick(u, KEYS)?, None),
        2 => Op::AddSection(section(u)?),
        3 => Op::RemoveSection(section(u)?),
        4 => Op::RenameSection(section(u)?, pick(u, NAMES)?),
        5 => {
            let before = if u.arbitrary()? {
                Some(number(u)?)
            } else {
                None
            };
            Op::AddRule(pick(u, RULES)?, before)
        }
        6 => Op::SetRule(number(u)?, pick(u, RULES)?),
        7 => Op::MoveRule(number(u)?, number(u)?),
        _ => Op::RemoveRule(number(u)?),
    })
}

fn apply(text: &str, op: &Op<'_>) -> Option<String> {
    match *op {
        Op::SetKey(of, key, value) => set_key(text, of, key, value),
        Op::AddSection(of) => add_section(text, of),
        Op::RemoveSection(of) => remove_section(text, of),
        Op::RenameSection(of, new) => rename_section(text, of, new),
        Op::AddRule(rule, before) => add_rule(text, rule, before),
        Op::SetRule(n, rule) => set_rule(text, n, rule),
        Op::MoveRule(n, before) => move_rule(text, n, before),
        Op::RemoveRule(n) => remove_rule(text, n),
    }
    .ok()
}

const BOM: &str = "\u{feff}";

/// The text's lines, each with its ending, a byte-order mark before the
/// first one apart, as the file's.
fn lines(text: &str) -> Vec<&str> {
    text.strip_prefix(BOM)
        .unwrap_or(text)
        .split_inclusive('\n')
        .collect()
}

/// A line without its ending, as the writer reads it.
fn body(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

/// Whether the line `kept` of the result is the line `was` of the text
/// before the edit: the same bytes, or for the last line of a text with no
/// last newline, the same bytes with the newline the writer gives the line
/// before one it adds.
fn same(was: &str, kept: &str) -> bool {
    was == kept
        || (!was.ends_with('\n')
            && kept
                .strip_prefix(was)
                .is_some_and(|e| e == "\n" || e == "\r\n"))
}

/// Whether `old`'s lines are `new`'s with some lines added, and those
/// added lines, without their endings.
fn added<'a>(old: &[&str], new: &[&'a str]) -> Option<Vec<&'a str>> {
    let mut extra = Vec::new();
    let mut at = 0;
    for line in new {
        if old.get(at).is_some_and(|o| same(o, line)) {
            at += 1;
        } else {
            extra.push(body(line));
        }
    }
    (at == old.len()).then_some(extra)
}

/// Whether a line is a section heading: its first byte past the blanks is
/// `[`.
fn heading(line: &str) -> bool {
    line.trim_start_matches([' ', '\t']).starts_with('[')
}

/// Whether a line holds content by vtium.conf's one comment rule: it is
/// neither blank nor a comment.
fn content(line: &str) -> bool {
    key_line(body(line).as_bytes()).is_some()
}

/// The words between a heading's brackets, the blanks inside them and a
/// comment after them aside; `None` for a line that is no heading or whose
/// brackets do not close.
fn heading_words(line: &str) -> Option<Vec<&str>> {
    let line = body(line).trim_matches([' ', '\t']);
    let line = match line.find(" #").or_else(|| line.find("\t#")) {
        Some(at) => line[..at].trim_end_matches([' ', '\t']),
        None => line,
    };
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.split([' ', '\t']).filter(|w| !w.is_empty()).collect())
}

/// Whether the heading `line` names the section `of`: `[KIND NAME]`.
fn names(line: &str, of: Of<'_>) -> bool {
    let (kind, name) = match of {
        Of::Head(n) => ("head", n),
        Of::Look(n) => ("look", n),
    };
    heading_words(line).is_some_and(|w| w == [kind, name])
}

/// The index of the heading of the section that line `i` sits in.
fn section_of(ls: &[&str], i: usize) -> Option<usize> {
    (0..=i).rev().find(|&j| heading(ls[j]))
}

/// The index past the last line of the section whose heading is `start`.
fn section_end(ls: &[&str], start: usize) -> usize {
    (start + 1..ls.len())
        .find(|&j| heading(ls[j]))
        .unwrap_or(ls.len())
}

/// The bodies of the rule lines, in order.
fn rules(text: &str) -> Vec<String> {
    let ls = lines(text);
    rule_lines(text)
        .into_iter()
        .map(|n| body(ls[n - 1]).to_owned())
        .collect()
}

/// Whether `p` is a structural error: one of the file's shape, its
/// headings, its line syntax and the place of its `version` and `policy`
/// lines, and not one of a value, a key or a rule's target, which a draft
/// may still hold.
fn structural(p: &Problem) -> bool {
    matches!(
        p,
        Problem::NotUtf8
            | Problem::LineTooLong
            | Problem::ControlCharacter(_)
            | Problem::MissingVersion
            | Problem::UnknownVersion(_)
            | Problem::Syntax(_)
            | Problem::DuplicateSection(_)
            | Problem::DuplicateHead(_)
            | Problem::DuplicateLook(_)
            | Problem::BadName(_)
            | Problem::BadLookName(_)
            | Problem::ReservedName(_)
            | Problem::PolicyInSection
            | Problem::VersionInSection
            | Problem::NotASection(_)
            | Problem::UnreadablePolicy(_)
    )
}

/// The heading words of the section that the line at `i` sits in, `None`
/// above the first heading.
fn within<'a>(ls: &[&'a str], i: usize) -> Option<Vec<&'a str>> {
    section_of(ls, i).and_then(|h| heading_words(ls[h]))
}

/// The result reads with no structural error the text before had not.
///
/// An error on a line the text before held, with the same bytes and in a
/// section of the same heading, is the line's own: the text before had it
/// too, though a heading vtium dropped, such as a second `[rules]`, kept
/// the reader from reaching it. A rule moved out of such a section shows
/// it. A heading line is never excused, so a heading the edit adds that
/// makes an older one a second is caught.
fn no_new_structure(before: &str, after: &str, op: &Op<'_>) {
    let was = reading(before.as_bytes());
    let now = reading(after.as_bytes());
    let full = |r: &Reading| r.errors.more > 0 || r.errors.errors.len() >= ERRORS_MAX;
    if full(&was) || full(&now) {
        return;
    }
    let old = lines(before);
    let new = lines(after);
    let own = |line: Option<usize>| {
        let Some(i) = line.and_then(|n| n.checked_sub(1)) else {
            return false;
        };
        let Some(l) = new.get(i) else {
            return false;
        };
        !heading(l)
            && (0..old.len()).any(|j| {
                body(old[j]) == body(l) && !heading(old[j]) && within(&old, j) == within(&new, i)
            })
    };
    let mut had: Vec<&Problem> = was
        .errors
        .errors
        .iter()
        .map(|e| &e.problem)
        .filter(|p| structural(p))
        .collect();
    for e in now.errors.errors.iter().filter(|e| structural(&e.problem)) {
        match had.iter().position(|q| **q == e.problem) {
            Some(i) => {
                had.swap_remove(i);
            }
            None if own(e.line) => {}
            None => panic!("{op:?} adds the error {e:?}\nbefore:\n{before:?}\nafter:\n{after:?}"),
        }
    }
}

/// The top level keeps its lines that hold content, vtium's own keys and
/// the `version` and `policy` lines, in order, and gains only a
/// `version = 1` line, and only where it had none.
fn top_level_kept(before: &str, after: &str, op: &Op<'_>) {
    let keys = |t: &str| -> Vec<Vec<u8>> {
        split(t.as_bytes())
            .top_lines()
            .filter(|(_, l)| key_line(l).is_some())
            .map(|(_, l)| l.strip_suffix(b"\r").unwrap_or(l).to_vec())
            .collect()
    };
    let was = keys(before);
    let now = keys(after);
    if was == now {
        return;
    }
    let versioned = was
        .iter()
        .any(|l| key_line(l).is_some_and(|k| k.key == b"version"));
    let gains_version = !versioned
        && now.len() == was.len() + 1
        && now.last().map(Vec::as_slice) == Some(b"version = 1".as_slice())
        && now[..was.len()] == was[..];
    assert!(
        gains_version,
        "{op:?} changes the top level\nbefore:\n{before:?}\nafter:\n{after:?}"
    );
}

/// The lines the edit does not need stay byte for byte, and the lines it
/// writes are the ones it was asked to write.
fn lines_kept(before: &str, after: &str, op: &Op<'_>) {
    let old = lines(before);
    let new = lines(after);
    let fail = |why: &str| -> ! {
        panic!("{op:?}: {why}\nbefore:\n{before:?}\nafter:\n{after:?}");
    };
    if before.starts_with(BOM) != after.starts_with(BOM) {
        fail("the edit adds or removes the byte-order mark");
    }
    // the lines that differ where the edit keeps the line count
    let changed = || -> Vec<usize> {
        if old.len() != new.len() {
            fail("the edit changes the number of lines");
        }
        (0..old.len()).filter(|&i| !same(old[i], new[i])).collect()
    };
    match *op {
        Op::SetKey(of, key, value) => {
            if new.len() == old.len() + 1 {
                let Some(extra) = added(&old, &new) else {
                    fail("the edit adds a line and changes another");
                };
                let at = (0..new.len())
                    .find(|&i| old.get(i).is_none_or(|o| !same(o, new[i])))
                    .unwrap_or(new.len());
                let line = format!("{key} = {}", value.unwrap_or_default());
                if value.is_none() || extra != [line.as_str()] {
                    fail("the added line is not the key and its value");
                }
                if at == 0 || !names(new[at - 1], of) {
                    fail("the key's line is not right below its section's heading");
                }
                return;
            }
            let changed = changed();
            let [i] = changed.as_slice() else {
                // unsetting a key the section does not set, or setting the
                // value a line holds already, leaves the text as it was
                let holds = |l: &&str| {
                    key_line(body(l).as_bytes()).is_some_and(|k| {
                        k.key == key.as_bytes() && k.value.as_deref() == value.map(str::as_bytes)
                    })
                };
                if changed.is_empty() && (value.is_none() || old.iter().any(holds)) {
                    return;
                }
                fail("the edit changes no line or more than one");
            };
            let i = *i;
            if !section_of(&old, i).is_some_and(|h| h != i && names(old[h], of)) {
                fail("the changed line is not in the section");
            }
            let was = key_line(body(old[i]).as_bytes());
            if !was.is_some_and(|k| k.key == key.as_bytes()) {
                fail("the changed line did not set the key");
            }
            match value {
                None => {
                    if new[i] != format!("#{}", old[i]) {
                        fail("unsetting does not comment the line out");
                    }
                }
                Some(v) => {
                    let now = key_line(body(new[i]).as_bytes());
                    let reads = now.is_some_and(|k| {
                        k.key == key.as_bytes() && k.value.as_deref() == Some(v.as_bytes())
                    });
                    if !reads {
                        fail("the changed line does not read as the key and its value");
                    }
                }
            }
        }
        Op::AddSection(of) => {
            let Some(extra) = added(&old, &new) else {
                fail("the edit changes or removes a line");
            };
            let headings: Vec<&&str> = extra.iter().filter(|l| heading(l)).collect();
            if headings.len() != 1 || !names(headings[0], of) {
                fail("the edit does not add the one heading");
            }
            let others_fit = extra
                .iter()
                .all(|l| heading(l) || l.is_empty() || *l == "version = 1");
            if !others_fit || extra.len() > 4 {
                fail("the edit adds lines other than the heading, a blank and the version");
            }
        }
        Op::RemoveSection(of) => {
            // the heading goes, with the section's lines up to its last
            // one that holds content, and at most one blank line after it
            let removes = |h: usize| {
                let end = section_end(&old, h);
                let last = (h..end).rev().find(|&i| content(old[i])).unwrap_or(h);
                let blank = old
                    .get(last + 1)
                    .is_some_and(|l| last + 1 < end && body(l).trim().is_empty());
                let mut cuts = vec![last + 1];
                if blank {
                    cuts.push(last + 2);
                }
                cuts.into_iter().any(|cut| {
                    let kept: Vec<&str> = old[..h].iter().chain(&old[cut..]).copied().collect();
                    kept == new
                })
            };
            let found = (0..old.len()).any(|h| heading(old[h]) && names(old[h], of) && removes(h));
            if !found {
                fail("the edit removes other lines than the section's");
            }
        }
        Op::RenameSection(of, new_name) => {
            let renamed = match of {
                Of::Head(_) => Of::Head(new_name),
                Of::Look(_) => Of::Look(new_name),
            };
            let rule_at: Vec<usize> = rule_lines(before).into_iter().map(|n| n - 1).collect();
            let mut headings = 0;
            for i in changed() {
                if heading(old[i]) {
                    headings += 1;
                    if !names(old[i], of) || !names(new[i], renamed) {
                        fail("the edit changes another heading");
                    }
                    continue;
                }
                let refers = match of {
                    Of::Head(_) => rule_at.contains(&i),
                    Of::Look(_) => {
                        section_of(&old, i).is_some_and(|h| {
                            heading_words(old[h]).is_some_and(|w| w.len() == 2 && w[0] == "head")
                        }) && key_line(body(old[i]).as_bytes()).is_some_and(|k| k.key == b"look")
                    }
                };
                if !refers {
                    fail("the edit changes a line that does not name the section");
                }
            }
            if headings > 1 {
                fail("the edit renames more than one heading");
            }
        }
        Op::AddRule(rule, at) => {
            let Some(extra) = added(&old, &new) else {
                fail("the edit changes or removes a line");
            };
            let rule = rule.trim_matches([' ', '\t']);
            let fits = extra
                .iter()
                .all(|l| *l == rule || *l == "[rules]" || *l == "version = 1");
            if extra.iter().filter(|l| **l == rule).count() != 1 || !fits || extra.len() > 3 {
                fail("the edit adds lines other than the rule, its section and the version");
            }
            let mut want = rules(before);
            let index = at.map_or_else(
                || {
                    rules(after)
                        .iter()
                        .zip(want.iter().chain(std::iter::repeat(&String::new())))
                        .position(|(a, b)| a != b)
                        .unwrap_or(want.len())
                },
                |n| n - 1,
            );
            want.insert(index.min(want.len()), rule.to_owned());
            if rules(after) != want {
                fail("the rule is not where it was asked to go");
            }
        }
        Op::SetRule(n, rule) => {
            let at = rule_lines(before)[n - 1] - 1;
            let changed = changed();
            if changed.iter().any(|&i| i != at) {
                fail("the edit changes a line other than the rule's");
            }
            if !body(new[at]).contains(rule.trim_matches([' ', '\t'])) {
                fail("the rule's line does not hold the new rule");
            }
        }
        Op::MoveRule(n, before_n) => {
            let mut want = rules(before);
            let moved = want.remove(n - 1);
            let to = if before_n > n {
                before_n - 2
            } else {
                before_n - 1
            };
            want.insert(to.min(want.len()), moved);
            if rules(after) != want {
                fail("the rules are not in the order asked for");
            }
            // the moved line keeps its bytes, and every other line its own
            let at = rule_lines(before)[n - 1] - 1;
            let mut was: Vec<&str> = old.iter().map(|l| body(l)).collect();
            let line = was.remove(at);
            let now: Vec<&str> = new.iter().map(|l| body(l)).collect();
            let found = (0..now.len()).filter(|&i| now[i] == line).any(|i| {
                let mut rest = now.clone();
                rest.remove(i);
                rest == was
            });
            if !found {
                fail("the edit changes a line other than the moved rule");
            }
        }
        Op::RemoveRule(n) => {
            let at = rule_lines(before)[n - 1] - 1;
            let mut want = old.clone();
            want.remove(at);
            if new != want {
                fail("the edit removes more than the rule's line");
            }
        }
    }
}

fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let Ok(mut current) = text(&mut u) else {
        return;
    };
    for _ in 0..4 {
        let Ok(op) = op(&mut u) else {
            return;
        };
        let Some(after) = apply(&current, &op) else {
            continue;
        };
        no_new_structure(&current, &after, &op);
        top_level_kept(&current, &after, &op);
        lines_kept(&current, &after, &op);
        current = after;
    }
});
