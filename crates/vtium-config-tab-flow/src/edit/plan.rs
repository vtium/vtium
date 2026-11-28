//! What the Flow tab writes: the configuration as paths turned back into
//! vtium.conf's heads, rules and labels. [`plan`] compares the paths the
//! tab wants with the draft's flow and gives the edits that make the draft
//! say them, so a configuration that changed nothing writes nothing, and a
//! path that Revise took out and `a` put back leaves the draft's text byte
//! for byte as it was.
//!
//! The rest of the module changes the paths themselves: a path added to
//! the configuration ([`with`]), a path taken out to revise it
//! ([`revise`]), a label given or taken away ([`set_label`]), and the
//! checks that refuse a path, a terminal's name or a login command.
//!
//! References:
//! - ADR-1403 D8, "What the Flow view writes": a path's head, its rules,
//!   the order of the rules by rank, the `[labels]` section, and Revise
//!   and add back writing nothing new.
//! - ADR-1403 D7, "Composing and adding a path": what `a` refuses.
//! - ADR-1205 D1: a path names a look the draft holds, so the Flow tab
//!   writes the head's `look =` line and makes or removes no look.

use vtium_config_core::document::ConfEdit;
use vtium_config_core::flow::{Flow, Kind, Rule, Special, Target};

use crate::paths::{self, Doc, Path, Piece, expand};
use crate::words;

/// The terminal settings a path writes as its head's keys: how many
/// consoles, the login, what happens when its screen is unplugged,
/// whether Enter on its own keyboard switches to its VT, whether roaming
/// inputs stay out, and the VTs it runs on.
pub const SETTINGS: [&str; 6] = [
    "consoles",
    "login",
    "when-display-lost",
    "switch-on-enter",
    "isolated",
    "vt",
];

/// The classes in the order the shipped default lists their rules, which
/// the rules a path writes keep.
const CLASS_ORDER: [&str; 7] = [
    "class:keyboard",
    "class:mouse",
    "class:touchpad",
    "class:touchscreen",
    "class:trackball",
    "class:pointingstick",
    "class:key",
];

/// The value `path` writes for the setting `key`: `isolated = yes` for a
/// path with No input, else the setting as the path has it.
fn wanted(path: &Path, key: &str) -> String {
    if key == "isolated" && path.no_input {
        return "yes".to_owned();
    }
    path.setting(key)
}

/// Sets the setting `key` of `path` to `value`, or leaves it at its
/// default when `value` is empty.
pub fn set(path: &mut Path, key: &str, value: &str) {
    path.settings.retain(|(k, _)| k != key);
    if !value.is_empty() {
        path.settings.push((key.to_owned(), value.to_owned()));
    }
}

/// Puts `new` in the place of the part `old` in `path`, keeping its
/// place. Where `new` is `old`, the path stays as it is; where the path
/// holds `new` already, `old` goes, so the path holds each part once.
pub fn replace(path: &mut Path, old: &Piece, new: Piece) {
    let swap = |list: &mut Vec<String>, o: &str, n: String| {
        if o == n {
            // the part's own choice, picked again
        } else if list.contains(&n) {
            list.retain(|x| x != o);
        } else if let Some(x) = list.iter_mut().find(|x| *x == o) {
            *x = n;
        } else {
            list.push(n);
        }
    };
    match (old, new) {
        (Piece::Display(o), Piece::Display(n)) => swap(&mut path.displays, o, n),
        (Piece::Input(o), Piece::Input(n)) => swap(&mut path.inputs, o, n),
        _ => {}
    }
}

/// Gives the part of `kind` that matches by `sel` the label `label` in
/// `doc`, or takes its label away when `label` is empty.
pub fn set_label(doc: &mut Doc, kind: Kind, sel: &str, label: &str) {
    match doc
        .labels
        .iter_mut()
        .find(|(k, s, _)| *k == kind && s == sel)
    {
        Some(entry) if !label.is_empty() => label.clone_into(&mut entry.2),
        Some(_) => doc.labels.retain(|(k, s, _)| !(*k == kind && s == sel)),
        None if !label.is_empty() => doc.labels.push((kind, sel.to_owned(), label.to_owned())),
        None => {}
    }
}

/// The configuration `doc` with `new` added. Each input of `new` that
/// roams by [`Doc::would_roam`], and every input of a path to the active
/// terminal, joins the roaming inputs and leaves the path that had it;
/// the rest stay with `new`. A path to Nowhere adds its displays to the
/// displays turned off and its inputs to the inputs set aside.
pub fn with(doc: &Doc, mut new: Path) -> Doc {
    let mut out = doc.clone();
    if new.none {
        for d in new.displays {
            if !out.off.contains(&d) {
                out.off.push(d);
            }
        }
        for i in new.inputs {
            if !out.ignored.contains(&i) {
                out.ignored.push(i);
            }
        }
        return out;
    }
    let first = (out.empty && out.paths.is_empty()) || new.active;
    let mut own = Vec::new();
    for i in std::mem::take(&mut new.inputs) {
        if out.roaming.contains(&i) {
            continue;
        }
        let mut held = false;
        for p in &mut out.paths {
            // a path on another VT keeps the input
            if new.shared_vt(p).is_none() {
                continue;
            }
            if let Some(at) = p.inputs.iter().position(|x| *x == i) {
                p.inputs.remove(at);
                held = true;
            }
        }
        if held || first {
            out.roaming.push(i);
        } else if !own.contains(&i) {
            own.push(i);
        }
    }
    new.inputs = own;
    if !new.active {
        out.paths.push(new);
    }
    out
}

/// What NEW PATH holds for revising, which CONFIGURATION leaves out while
/// the draft still has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Revising {
    /// The path of the terminal of this name.
    Path(String),
    /// The path of the active terminal: the roaming inputs.
    Active,
    /// The path to Nowhere.
    Nothing,
}

/// The configuration `doc` without the path `what`, and that path, with
/// its parts picked and its terminal's name kept; `None` where `doc` has
/// no such path. The roaming inputs stay where they are when a
/// terminal's path is taken out.
pub fn revise(doc: &Doc, what: &Revising) -> Option<(Doc, Path)> {
    let mut rest = doc.clone();
    let taken = match what {
        Revising::Path(name) => {
            let at = rest
                .paths
                .iter()
                .position(|p| p.name.as_deref() == Some(name.as_str()))?;
            rest.paths.remove(at)
        }
        Revising::Active if !rest.roaming.is_empty() => Path {
            inputs: std::mem::take(&mut rest.roaming),
            active: true,
            ..Path::default()
        },
        Revising::Nothing if !(rest.off.is_empty() && rest.ignored.is_empty()) => Path {
            displays: std::mem::take(&mut rest.off),
            inputs: std::mem::take(&mut rest.ignored),
            none: true,
            ..Path::default()
        },
        Revising::Active | Revising::Nothing => return None,
    };
    Some((rest, taken))
}

/// The configuration as CONFIGURATION shows it: `doc` without what NEW
/// PATH is revising.
pub fn shown(doc: &Doc, revising: Option<&Revising>) -> Doc {
    match revising.and_then(|r| revise(doc, r)) {
        Some((rest, _)) => rest,
        None => doc.clone(),
    }
}

/// What adding the new path does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Adding {
    /// It is a new path.
    New,
    /// The configuration holds this path already, in the terminal named:
    /// nothing changes.
    Same(String),
    /// Every input of the path to the active terminal roams already.
    SameActive,
    /// Every display of the path to Nowhere is off already, and every
    /// input set aside.
    SameNone,
}

/// Whether `new` may be added to the configuration `doc`, or why not, in
/// one sentence. A path whose displays, VTs and look a path of `doc` has,
/// and whose inputs that path or the roaming inputs all have, is held
/// already, and adding it changes nothing. A display that a path holds on
/// a VT the new path also runs on is refused, as is a name another path
/// has.
pub fn check_add(flow: &Flow, doc: &Doc, new: &Path) -> Result<Adding, String> {
    if !new.complete(doc) {
        return Err(new.status(doc).to_owned());
    }
    if new.active {
        return Ok(if new.inputs.iter().all(|i| doc.roaming.contains(i)) {
            Adding::SameActive
        } else {
            Adding::New
        });
    }
    let name = |p: &Path| p.name.clone().unwrap_or_default();
    // a display is in one path for each VT: only a path whose terminal
    // shares a VT with the new one's clashes
    let taken = |d: &str| {
        let (title, _) = paths::title(flow, Kind::Display, d);
        let clash = doc
            .paths
            .iter()
            .filter(|p| p.displays.iter().any(|x| x == d))
            .find_map(|p| new.shared_vt(p).map(|vt| (p, vt)));
        if let Some((p, vt)) = clash {
            return Some(words::display_taken(&title, &name(p), vt).0);
        }
        (!new.none && doc.off.iter().any(|x| x == d)).then(|| super::words::display_off(&title))
    };
    if new.none {
        if let Some(why) = new.displays.iter().find_map(|d| taken(d)) {
            return Err(why);
        }
        let held = new.displays.iter().all(|d| doc.off.contains(d))
            && new.inputs.iter().all(|i| doc.ignored.contains(i));
        return Ok(if held { Adding::SameNone } else { Adding::New });
    }
    if let Some(n) = &new.name
        && !valid_name(n)
    {
        return Err(super::words::name_invalid(n));
    }
    let set = |v: &[String]| {
        let mut v = v.to_vec();
        v.sort();
        v.dedup();
        v
    };
    let held = doc.paths.iter().find(|p| {
        set(&p.displays) == set(&new.displays)
            && p.vts() == new.vts()
            && p.no_input == new.no_input
            && new
                .inputs
                .iter()
                .all(|i| p.inputs.contains(i) || doc.roaming.contains(i))
    });
    if let Some(held) = held {
        return if held.look == new.look {
            Ok(Adding::Same(name(held)))
        } else {
            Err(super::words::path_other_look(&name(held)))
        };
    }
    if let Some(n) = &new.name
        && doc.path(n).is_some()
    {
        return Err(super::words::name_taken(n));
    }
    if let Some(why) = new.displays.iter().find_map(|d| taken(d)) {
        return Err(why);
    }
    Ok(Adding::New)
}

/// Whether `name` may name a terminal: 1 to 64 of `A-Z a-z 0-9 . _ + -`,
/// and none of the words a rule sends to that are not terminals.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
        && !Special::ALL.iter().any(|s| s.name() == name)
}

/// The name a new terminal takes: `console`, the shipped default's, while
/// the configuration has no terminal, else `terminal-N`, the smallest N
/// that no head, no path and none of `made`, the terminals the person
/// made in PARTS, has.
pub fn new_name_besides(flow: &Flow, doc: &Doc, made: &[String]) -> String {
    let taken = |n: &str| {
        flow.heads.iter().any(|h| h.name == n)
            || doc.path(n).is_some()
            || made.iter().any(|m| m == n)
    };
    if flow.heads.is_empty() && doc.paths.is_empty() && !taken("console") {
        return "console".to_owned();
    }
    (1usize..)
        .map(|n| format!("terminal-{n}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| "terminal".to_owned())
}

/// The command a console of a head with no `login` of its own runs: the
/// top level's `login`, or else vtium's default, `/bin/login -p` with
/// `reset-env` on and `/bin/login` with it off.
pub fn login_in_effect(flow: &Flow) -> String {
    let last = |key: &str| flow.base.set.iter().rposition(|(k, _)| k == key);
    if let Some(at) = last("login")
        && let Some((_, command)) = flow.base.set.get(at)
        && !command.is_empty()
    {
        return command.clone();
    }
    // a switch is set by `reset-env` or `no-reset-env`, the last one winning
    let off = match (last("reset-env"), last("no-reset-env")) {
        (Some(on), Some(off)) => off > on,
        (None, Some(_)) => true,
        _ => false,
    };
    if off { "/bin/login" } else { "/bin/login -p" }.to_owned()
}

/// Checks a login command the person typed and gives it back trimmed, or
/// says in a sentence what is wrong with it. Its first word, the program,
/// is an absolute path to a file that exists and that its mode lets run,
/// or a variable, `$NAME`, which vtium expands when the console starts. A
/// `#` is refused, as vtium.conf reads it as the start of a comment.
pub fn check_login(typed: &str) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;
    let command = typed.trim();
    let Some(program) = command.split([' ', '\t']).next().filter(|p| !p.is_empty()) else {
        return Err(super::words::LOGIN_EMPTY.to_owned());
    };
    if command.contains('#') {
        return Err(super::words::LOGIN_HASH.to_owned());
    }
    // vtium replaces a word written `$NAME` with that variable of the
    // console's environment, which vtium-config cannot see, and `$$`
    // writes a literal `$`
    if program.starts_with('$') && !program.starts_with("$$") {
        return Ok(command.to_owned());
    }
    if !program.starts_with('/') {
        return Err(super::words::login_not_absolute(program));
    }
    match std::fs::metadata(program) {
        Err(_) => Err(super::words::login_missing(program)),
        Ok(m) if !m.is_file() || m.permissions().mode() & 0o111 == 0 => {
            Err(super::words::login_not_runnable(program))
        }
        Ok(_) => Ok(command.to_owned()),
    }
}

/// Whether the rule `rule` belongs to the configuration as paths: it
/// sends to a head, roams, turns a display off or sets an input aside.
/// Only a rule whose target the draft does not define fits no path.
fn fits(flow: &Flow, rule: &Rule) -> bool {
    matches!(flow.target(rule), Target::Head(_) | Target::Special(_))
}

/// The match a label line writes for the part of `kind` matched by `sel`:
/// the first rule's match of a part that writes several.
fn label_match(kind: Kind, sel: &str) -> String {
    match kind {
        Kind::Display => sel.to_owned(),
        Kind::Input => expand(sel).first().copied().unwrap_or(sel).to_owned(),
    }
}

/// One line of `[labels]`: `display MATCH = LABEL` or `input MATCH =
/// LABEL`, with each `#` the comment rule would read escaped.
fn label_line(kind: Kind, sel: &str, label: &str) -> String {
    format!(
        "{} {sel} = {}",
        kind_word(kind),
        vtium_settings::escape_label(label)
    )
}

/// The word a rule or a label line starts with for `kind`.
const fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Display => "display",
        Kind::Input => "input",
    }
}

/// The `[labels]` lines `doc` says, given the rules in the order the
/// draft will hold them: a label whose part a rule uses goes where that
/// rule is, and the labels of parts no rule uses come after, in the order
/// the person gave them.
fn labels_plan(doc: &Doc, rules: &[(Line, bool)]) -> Vec<String> {
    let mut placed: Vec<(usize, usize, String)> = doc
        .labels
        .iter()
        .enumerate()
        .map(|(i, (kind, sel, label))| {
            let written = label_match(*kind, sel);
            let at = rules
                .iter()
                .position(|(l, _)| l.kind == *kind && l.selector == written)
                .unwrap_or(usize::MAX);
            (at, i, label_line(*kind, &written, label))
        })
        .collect();
    placed.sort_by_key(|(at, i, _)| (*at, *i));
    placed.into_iter().map(|(_, _, line)| line).collect()
}

/// How specific the match of a rule of `kind` is, from 1 for one device
/// to 7 for the catch-all, as vtium-settings ranks it, so vtium-config and
/// the section writer keep one order. All keyboards and All mice and
/// touchpads rank as the classes they stand for, and a match vtium cannot
/// read ranks with the catch-all.
pub fn rank(kind: Kind, selector: &str) -> u8 {
    let selector = expand(selector).first().copied().unwrap_or(selector);
    let text = format!(
        "version = 1\n[head h]\n[rules]\n{} {selector} -> h\n",
        kind_word(kind)
    );
    vtium_settings::parse_sections(&text)
        .ok()
        .flatten()
        .and_then(|p| p.config.rules.first().map(|r| r.matcher.rank()))
        .unwrap_or(vtium_settings::Match::RANK_ANY)
}

/// One rule as [`plan`] writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    kind: Kind,
    selector: String,
    target: String,
}

impl Line {
    fn new(kind: Kind, selector: &str, target: &str) -> Line {
        Line {
            kind,
            selector: selector.to_owned(),
            target: target.to_owned(),
        }
    }

    fn of(rule: &Rule) -> Line {
        Line::new(rule.kind, &rule.selector, &rule.target)
    }

    /// Where the rule goes among the rules [`plan`] writes: the display
    /// rules before the input rules, each kind most specific first, and
    /// the classes in the order the shipped default lists them.
    fn order(&self) -> (u8, u8, usize) {
        let class = CLASS_ORDER
            .iter()
            .position(|c| *c == self.selector)
            .unwrap_or(CLASS_ORDER.len());
        let at = match self.kind {
            Kind::Display => 0,
            Kind::Input => 1,
        };
        (at, rank(self.kind, &self.selector), class)
    }

    fn text(&self) -> String {
        format!(
            "{} {} -> {}",
            kind_word(self.kind),
            self.selector,
            self.target
        )
    }
}

/// The edits that make the draft's heads the paths of `doc`: the heads no
/// path has go, and each path's head is added or has its `look =` line
/// and its settings set.
fn heads_plan(flow: &Flow, doc: &Doc) -> Vec<ConfEdit> {
    let mut heads = Vec::new();
    for p in &doc.paths {
        let Some(name) = p.name.clone() else {
            continue;
        };
        match flow.heads.iter().find(|h| h.name == name) {
            None => {
                heads.push(ConfEdit::HeadAdd {
                    name: name.clone(),
                    look: p.look.clone(),
                });
                for key in SETTINGS {
                    let value = wanted(p, key);
                    if !value.is_empty() {
                        heads.push(ConfEdit::HeadKey {
                            name: name.clone(),
                            key,
                            value: Some(value),
                        });
                    }
                }
            }
            Some(h) => {
                let now = (h.look != "default").then(|| h.look.clone());
                if now != p.look {
                    heads.push(ConfEdit::HeadKey {
                        name: name.clone(),
                        key: "look",
                        value: p.look.clone(),
                    });
                }
                // the settings the path changed in place
                for key in SETTINGS {
                    let have = match key {
                        "consoles" if h.consoles != 1 => h.consoles.to_string(),
                        "consoles" => String::new(),
                        _ => h
                            .keys
                            .iter()
                            .find(|(k, _)| k == key)
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default(),
                    };
                    let want = wanted(p, key);
                    if have != want {
                        heads.push(ConfEdit::HeadKey {
                            name: name.clone(),
                            key,
                            value: Some(want).filter(|v| !v.is_empty()),
                        });
                    }
                }
            }
        }
    }
    flow.heads
        .iter()
        .filter(|h| doc.path(&h.name).is_none())
        .map(|h| ConfEdit::HeadRm(h.name.clone()))
        .chain(heads)
        .collect()
}

/// The draft's rules as `doc` says them, in order, each marked when it is
/// new. The rules the draft has and `doc` still says stay where they are,
/// with the rules that belong to no path, so a configuration that changed
/// nothing writes nothing. Each new rule goes most specific first: before
/// the first rule of its kind that is wider, else after the last rule of
/// its kind. The first path of a configuration with no rules also brings
/// the shipped default's last rule, which sets aside every input no rule
/// names.
fn rules_plan(flow: &Flow, doc: &Doc) -> Vec<(Line, bool)> {
    let mut want: Vec<Line> = Vec::new();
    let mut push = |l: Line| {
        if !want.contains(&l) {
            want.push(l);
        }
    };
    for p in &doc.paths {
        let Some(name) = &p.name else {
            continue;
        };
        for d in &p.displays {
            push(Line::new(Kind::Display, d, name));
        }
        for i in &p.inputs {
            // an input that two paths on one VT name roams; paths on
            // different VTs each keep it
            let paths = doc
                .paths
                .iter()
                .filter(|q| q.inputs.contains(i) && p.shared_vt(q).is_some())
                .count();
            let target = if paths > 1 { "roaming" } else { name.as_str() };
            for s in expand(i) {
                push(Line::new(Kind::Input, s, target));
            }
        }
    }
    for r in &doc.roaming {
        for s in expand(r) {
            push(Line::new(Kind::Input, s, "roaming"));
        }
    }
    for d in &doc.off {
        push(Line::new(Kind::Display, d, "off"));
    }
    for i in &doc.ignored {
        for s in expand(i) {
            push(Line::new(Kind::Input, s, "ignore"));
        }
    }
    if flow.rules.is_empty() && !doc.paths.is_empty() {
        push(Line::new(Kind::Input, "*", "ignore"));
    }
    want.sort_by_key(Line::order);
    let mut out: Vec<(Line, bool)> = flow
        .rules
        .iter()
        .filter(|r| !fits(flow, r) || want.contains(&Line::of(r)))
        .map(|r| (Line::of(r), false))
        .collect();
    for n in want {
        if out.iter().any(|(l, _)| *l == n) {
            continue;
        }
        let wider = out
            .iter()
            .position(|(l, _)| l.kind == n.kind && l.order() > n.order());
        let after = out
            .iter()
            .rposition(|(l, _)| l.kind == n.kind)
            .map(|i| i.saturating_add(1));
        let at = wider.or(after).unwrap_or(match n.kind {
            Kind::Display => 0,
            Kind::Input => out.len(),
        });
        out.insert(at, (n, true));
    }
    out
}

/// The edits that make the draft whose flow is `flow` say the
/// configuration `doc`: the rules `doc` no longer says go, the heads no
/// path has go, each path's head is added or has its look and settings
/// set, each new rule is added most specific first, and the
/// `[labels]` section is written last, after `[rules]`. A draft that says
/// all this already gets no edit.
pub fn plan(flow: &Flow, doc: &Doc) -> Vec<ConfEdit> {
    let mut out = Vec::new();
    let rules = rules_plan(flow, doc);
    let kept: Vec<&Line> = rules.iter().filter(|(_, n)| !n).map(|(l, _)| l).collect();
    // the rules that go, from the last, so the numbers stay right
    for (i, r) in flow.rules.iter().enumerate().rev() {
        if !kept.contains(&&Line::of(r)) {
            out.push(ConfEdit::RuleRm(i.saturating_add(1)));
        }
    }
    out.extend(heads_plan(flow, doc));
    for (i, (l, added)) in rules.iter().enumerate() {
        if *added {
            out.push(ConfEdit::RuleAddBefore {
                line: l.text(),
                before: i.saturating_add(1),
            });
        }
    }
    let labels = labels_plan(doc, &rules);
    let now: Vec<String> = flow
        .labels
        .iter()
        .map(|(kind, sel, label)| label_line(*kind, sel, label))
        .collect();
    if labels != now {
        out.push(ConfEdit::Labels(labels));
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use vtium_config_core::document::conf::apply_all;
    use vtium_config_core::flowmodel::{Present, build};

    use super::*;

    fn read(text: &str) -> (Flow, Doc) {
        let flow = build(text, &Present::default(), None);
        let doc = paths::read(&flow);
        (flow, doc)
    }

    fn apply(text: &str, doc: &Doc) -> String {
        let (flow, _) = read(text);
        apply_all(text, &plan(&flow, doc)).unwrap_or_else(|e| format!("refused: {e:?}"))
    }

    const TWO: &str = "version = 1\n[look big]\nfont-name = proof\n[head desk]\nlook = big\nvt = 3\n[head side]\n[rules]\ndisplay connector:card0/DP-8 -> desk\ndisplay * -> side\ninput id:046d:c31c -> desk\ninput class:keyboard -> roaming\ninput class:key -> roaming\ninput * -> ignore\n[labels]\ndisplay connector:card0/DP-8 = Big screen\n";

    #[test]
    fn a_draft_that_says_its_paths_already_gets_no_edit() {
        let (flow, doc) = read(TWO);
        assert_eq!(plan(&flow, &doc), Vec::new());
    }

    #[test]
    fn each_path_revised_and_added_back_leaves_the_text_as_it_was() {
        let (flow, doc) = read(TWO);
        for what in [
            Revising::Path("desk".into()),
            Revising::Path("side".into()),
            Revising::Active,
            Revising::Nothing,
        ] {
            let (rest, path) = revise(&doc, &what).expect("the path");
            assert_eq!(
                check_add(&flow, &rest, &path).map(|_| ()),
                Ok(()),
                "{what:?}"
            );
            assert_eq!(apply(TWO, &with(&rest, path)), TWO, "{what:?}");
        }
    }

    #[test]
    fn a_path_taken_out_removes_its_head_and_rules_and_keeps_its_labels() {
        let (_, doc) = read(TWO);
        let (rest, _) = revise(&doc, &Revising::Path("desk".into())).expect("desk");
        let text = apply(TWO, &rest);
        assert!(!text.contains("[head desk]"), "{text}");
        assert!(!text.contains("-> desk"), "{text}");
        assert!(
            text.contains("display connector:card0/DP-8 = Big screen"),
            "{text}"
        );
    }

    #[test]
    fn a_new_rule_goes_before_the_first_wider_rule_of_its_kind() {
        let (flow, doc) = read(TWO);
        let new = Path {
            name: Some("work".into()),
            displays: vec!["connector:card1/eDP-1".into()],
            inputs: vec!["path:pci-0000:00:14.0-usb-0:1".into()],
            settings: vec![("vt".into(), "4".into())],
            ..Path::default()
        };
        assert_eq!(check_add(&flow, &doc, &new), Ok(Adding::New));
        let text = apply(TWO, &with(&doc, new));
        let rules: Vec<&str> = text
            .lines()
            .skip_while(|l| *l != "[rules]")
            .skip(1)
            .take_while(|l| !l.starts_with('['))
            .collect();
        assert_eq!(
            rules,
            [
                "display connector:card0/DP-8 -> desk",
                "display connector:card1/eDP-1 -> work",
                "display * -> side",
                "input path:pci-0000:00:14.0-usb-0:1 -> work",
                "input id:046d:c31c -> desk",
                "input class:keyboard -> roaming",
                "input class:key -> roaming",
                "input * -> ignore",
            ]
        );
    }

    #[test]
    fn a_display_held_on_a_shared_vt_is_refused_and_named() {
        let (flow, doc) = read(TWO);
        let new = Path {
            name: Some("work".into()),
            displays: vec!["connector:card0/DP-8".into()],
            inputs: vec!["path:pci-0000:00:14.0-usb-0:2".into()],
            settings: vec![("vt".into(), "3".into())],
            ..Path::default()
        };
        let why = check_add(&flow, &doc, &new).expect_err("a clash");
        assert!(why.contains("VT 3"), "{why}");
        let elsewhere = Path {
            settings: vec![("vt".into(), "5".into())],
            ..new
        };
        assert_eq!(check_add(&flow, &doc, &elsewhere), Ok(Adding::New));
    }

    #[test]
    fn a_name_is_refused_when_it_is_taken_or_no_terminal_can_hold_it() {
        assert!(valid_name("work-2.b_c+d"));
        for bad in ["", "two words", "roaming", "off", "ignore", "a#b"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        let (flow, doc) = read(TWO);
        let new = Path {
            name: Some("side".into()),
            displays: vec!["connector:card1/eDP-1".into()],
            ..Path::default()
        };
        assert!(check_add(&flow, &doc, &new).is_err());
    }

    #[test]
    fn a_login_runs_an_executable_by_its_full_path_or_a_variable() {
        assert_eq!(check_login(" /bin/sh -l "), Ok("/bin/sh -l".to_owned()));
        assert_eq!(check_login("$SHELL"), Ok("$SHELL".to_owned()));
        assert!(check_login("").is_err());
        assert!(check_login("sh").is_err());
        assert!(check_login("/bin/sh # x").is_err());
        assert!(check_login("/no/such/program").is_err());
        assert!(check_login("/etc/hostname").is_err());
    }
}
