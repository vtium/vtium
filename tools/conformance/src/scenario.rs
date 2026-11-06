//! Tier 1's scenarios (ADR-1504): short text files under `scenarios/`
//! that each check one behaviour or one capability through the replay
//! interface.
//!
//! A scenario is one command a line. The conformance tool turns it into a
//! replay input (the program's bytes, a test command for each action of
//! the user or the host, and the cuts) and plays that input through the
//! replay code the corpus uses. A behaviour scenario has a cut after each
//! command and is judged by its transcript, its golden, which
//! [`transcript`] prints from the blocks; a capability scenario has a cut
//! at each `check` and is judged by its assertions against
//! `capabilities.list`, with the rules [`capability`] codes. The README's
//! "Scenarios" section gives the language.

use std::collections::{BTreeMap, BTreeSet};

pub mod capability;
mod commands;
pub mod keys;
pub mod play;
pub mod run;
pub mod strings;
pub mod transcript;

pub use strings::{quoted, unquote};

use crate::command::{END, INTRO};
use crate::probes::{Probe, Step};

/// The areas a behaviour scenario's name starts with (ADR-1504 D5).
pub const AREAS: [&str; 11] = [
    "key", "keypad", "mouse", "paste", "focus", "select", "view", "palette", "reset", "reply",
    "bidi",
];

/// The interface commands that interface 1.3 adds for scenarios, with the
/// feature each needs; until vtium-replay declares them, a scenario that
/// uses one is reported as not run.
pub const LATER_COMMANDS: [(&str, &str); 5] = [
    ("select", "selection"),
    ("copy", "selection"),
    ("prompt", "marks"),
    ("backspace", "backspace"),
    ("state", "state"),
];

/// Which sort of scenario a file is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sort {
    /// Judged by its transcript, and by its assertions when it has any.
    Behaviour,
    /// Judged by its assertions against the capability of this name.
    Capability(String),
}

/// A view or the state a `dump` prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dump {
    Screen,
    Frame,
    Display,
    State,
}

impl Dump {
    pub fn name(self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::Frame => "frame",
            Self::Display => "display",
            Self::State => "state",
        }
    }
}

/// A command the transcript echoes: what it adds to the stream, and for a
/// `dump`, what it prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// The line 1 counts from.
    pub line: usize,
    /// The command as written, its comment removed and its spaces
    /// collapsed outside strings.
    pub echo: String,
    /// The program's bytes, for `send`.
    pub send: Vec<u8>,
    /// The test commands, each as the interface writes its text.
    pub tests: Vec<String>,
    /// The views and state a `dump` prints, in the order written.
    pub dump: Vec<Dump>,
}

impl Command {
    fn new(line: usize, echo: String) -> Self {
        Self {
            line,
            echo,
            send: Vec::new(),
            tests: Vec::new(),
            dump: Vec::new(),
        }
    }

    /// The bytes the command adds to the replay input.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = self.send.clone();
        for text in &self.tests {
            out.extend_from_slice(INTRO);
            out.extend_from_slice(text.as_bytes());
            out.extend_from_slice(END);
        }
        out
    }
}

/// One line of a scenario after its header.
#[derive(Clone, Debug)]
pub(crate) enum Item {
    Command(Command),
    Check,
    /// An assertion, in the form the probes used.
    Assert(Step),
    /// The example after a `want-re` or `reply-re`.
    Example(Vec<u8>),
}

/// A scenario file, read and checked for form.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: String,
    /// The file it was read from, for messages.
    pub file: String,
    pub sort: Sort,
    /// What `# spec:` names.
    pub spec: String,
    /// The ids of the ADR-1000 rows its header's covers line names.
    pub row_ids: Vec<String>,
    /// The comments of the header, for `--play`.
    pub header: Vec<String>,
    pub cols: usize,
    pub rows: usize,
    /// The view the assertions judge.
    pub view: Dump,
    pub needs: BTreeSet<String>,
    pub fixtures: Vec<String>,
    pub placeholder: Option<String>,
    pub(crate) items: Vec<(Item, usize)>,
}

/// A scenario's replay input: the bytes and the cuts, with the cut each
/// command's effects end at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compiled {
    pub data: Vec<u8>,
    pub cuts: Vec<usize>,
    /// For each command the transcript echoes, the index in `cuts` of the
    /// cut after it: a behaviour scenario's only.
    pub command_cuts: Vec<usize>,
}

/// Whether `name` is lowercase words of a-z and 0-9 joined by hyphens.
fn hyphenated(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|w| {
            !w.is_empty()
                && w.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// The header's tag in a comment line, `# spec`, `# covers` or
/// `# capability`, then a colon and its value, as its name and value.
fn tag(line: &str) -> Option<(&'static str, &str)> {
    let rest = line.trim_start().strip_prefix('#')?.trim_start();
    ["spec", "covers", "capability"]
        .into_iter()
        .find_map(|name| {
            rest.strip_prefix(name)
                .and_then(|r| r.strip_prefix(':'))
                .map(|value| (name, value.trim()))
        })
}

/// Reads one scenario: `name` is its file's name without `.scenario`, and
/// `file` the path messages name it by.
pub fn parse(name: &str, file: &str, text: &str) -> Result<Scenario, String> {
    let mut s = Scenario {
        name: name.to_owned(),
        file: file.to_owned(),
        sort: Sort::Behaviour,
        spec: String::new(),
        row_ids: Vec::new(),
        header: Vec::new(),
        cols: 0,
        rows: 0,
        view: Dump::Screen,
        needs: BTreeSet::new(),
        fixtures: Vec::new(),
        placeholder: None,
        items: Vec::new(),
    };
    let mut in_header = true;
    let mut capability = None;
    let mut sized = false;
    let mut view_set = false;
    let mut acted = false;
    for (i, line) in text.lines().enumerate() {
        let n = i.saturating_add(1);
        let at = format!("{file}:{n}");
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            match (tag(line), in_header) {
                (Some(("spec", v)), true) => {
                    if !s.spec.is_empty() {
                        return Err(format!(
                            "{at}: This is a second `# spec:` line. A scenario names what its expected output follows on one line."
                        ));
                    }
                    v.clone_into(&mut s.spec);
                }
                (Some(("covers", v)), true) => s.row_ids.extend(
                    v.split([',', ' '])
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned),
                ),
                (Some(("capability", v)), true) => capability = Some(v.to_owned()),
                (Some((t, _)), false) => {
                    return Err(format!(
                        "{at}: This `# {t}:` line comes after the first command. The header's lines go before `size`."
                    ));
                }
                _ => {}
            }
            if in_header {
                s.header.push(trimmed.to_owned());
            }
            continue;
        }
        in_header = false;
        let (word, rest) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
        match word {
            "size" if sized => {
                return Err(format!(
                    "{at}: This is a second `size` line. A scenario sets the screen it starts with once, as its first command; `resize` changes the size later."
                ));
            }
            "size" => sized = true,
            "placeholder" => {}
            _ if !sized => {
                return Err(format!(
                    "{at}: The first command is `{word}`. A scenario's first command is `size COLS ROWS`, the screen the replay starts with."
                ));
            }
            _ => {}
        }
        let item = commands::line_item(&mut s, &at, n, word, rest)?;
        let Some(item) = item else { continue };
        if let Item::Command(c) = &item {
            match word {
                "view" if view_set || acted => {
                    return Err(format!(
                        "{at}: This `view` line comes too late. A scenario names its view once, before any command that acts on the terminal."
                    ));
                }
                "view" => view_set = true,
                "needs" | "fixture" | "dump" => {}
                _ => acted = acted || !c.send.is_empty() || !c.tests.is_empty(),
            }
        }
        s.items.push((item, n));
    }
    finish(s, capability)
}

/// The checks every scenario passes once read.
fn finish(mut s: Scenario, capability: Option<String>) -> Result<Scenario, String> {
    let file = s.file.clone();
    if s.spec.is_empty() {
        return Err(format!(
            "{file}: This scenario has no `# spec:` line. Its header names the specification or the vtium decision its expected output follows, as in `# spec: ctlseqs, PC-Style Function Keys`."
        ));
    }
    if let Some(cap) = capability {
        if !s.row_ids.is_empty() {
            return Err(format!(
                "{file}: This capability scenario has a `# covers` line. capabilities.list's ROWS column names a capability's rows, so its scenario names none."
            ));
        }
        if cap != s.name {
            return Err(format!(
                "{file}: This scenario's file is named {}, but it declares the capability {cap}. A capability scenario's file has its capability's name.",
                s.name
            ));
        }
        if let Some((_, n)) = s
            .items
            .iter()
            .find(|(i, _)| matches!(i, Item::Command(c) if !c.dump.is_empty()))
        {
            return Err(format!(
                "{file}:{n}: This capability scenario has a `dump`. A capability scenario is judged by its assertions and prints no transcript."
            ));
        }
        s.sort = Sort::Capability(cap);
    } else {
        let area = s.name.split('-').next().unwrap_or_default();
        if !hyphenated(&s.name) || !AREAS.contains(&area) || !s.name.contains('-') {
            return Err(format!(
                "{file}: The name {} does not start with an area and a hyphen. A behaviour scenario's name is lowercase words joined by hyphens, starting with one of {}, as in key-cursor-decckm.",
                s.name,
                AREAS.join(", ")
            ));
        }
        if s.placeholder.is_some() {
            return Err(format!(
                "{file}: This behaviour scenario has a `placeholder`. Only a capability scenario can stand in for a capability vtium lacks."
            ));
        }
    }
    if s.placeholder.is_some() {
        if !s.items.is_empty() {
            return Err(format!(
                "{file}: This placeholder scenario has commands. A placeholder has nothing to run."
            ));
        }
        return Ok(s);
    }
    if s.cols == 0 {
        return Err(format!(
            "{file}: This scenario has no `size` line. Its first command is `size COLS ROWS`."
        ));
    }
    // each assertion follows a check, with no command between
    let mut checked = false;
    let mut judged = false;
    let mut after: Option<&str> = None;
    for (item, n) in &s.items {
        match item {
            Item::Check => (checked, after) = (true, None),
            Item::Command(c) => (checked, after) = (false, Some(c.echo.as_str())),
            Item::Assert(step) => {
                if !checked {
                    let why = after.map_or_else(
                        || "nothing marks the point it judges".to_owned(),
                        |c| format!("`{c}` comes after the last check"),
                    );
                    return Err(format!(
                        "{file}:{n}: This `{}` has no `check` before it: {why}. Put a `check` line between the commands and the assertions about them.",
                        step.key()
                    ));
                }
                judged |= matches!(
                    step,
                    Step::Want(_) | Step::WantRe(_) | Step::Reply(_) | Step::ReplyRe(..)
                );
            }
            Item::Example(_) => {}
        }
    }
    if matches!(s.sort, Sort::Capability(_)) && !judged {
        return Err(format!(
            "{file}: This capability scenario has no `want`, `want-re`, `reply` or `reply-re`, so nothing vtium prints could fail it."
        ));
    }
    // an example follows a want-re or a reply-re, once
    let mut prev: Option<&Item> = None;
    for (item, n) in &s.items {
        if let Item::Example(_) = item
            && !matches!(
                prev,
                Some(Item::Assert(Step::WantRe(_) | Step::ReplyRe(..)))
            )
        {
            return Err(format!(
                "{file}:{n}: This `example` does not follow a `want-re` or a `reply-re`. An example is a line or reply that the expression just before it must match."
            ));
        }
        prev = Some(item);
    }
    s.row_ids.sort();
    s.row_ids.dedup();
    Ok(s)
}

impl Scenario {
    /// The commands the transcript echoes, in order.
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.items.iter().filter_map(|(i, _)| match i {
            Item::Command(c) => Some(c),
            _ => None,
        })
    }

    /// The scenario's replay input. A behaviour scenario has a cut after
    /// each command; a capability scenario has a cut at each `check` and
    /// nowhere else, as a probe had.
    pub fn compile(&self) -> Compiled {
        let mut out = Compiled {
            data: Vec::new(),
            cuts: Vec::new(),
            command_cuts: Vec::new(),
        };
        let behaviour = self.sort == Sort::Behaviour;
        for (item, _) in &self.items {
            match item {
                Item::Command(c) => {
                    out.data.extend(c.bytes());
                    if behaviour {
                        out.command_cuts.push(out.cuts.len());
                        out.cuts.push(out.data.len());
                    }
                }
                Item::Check if !behaviour => out.cuts.push(out.data.len()),
                _ => {}
            }
        }
        out
    }

    /// Every test command's name, and the features the scenario needs
    /// beyond them: its `needs` line, its view's and its dumps' features,
    /// and those of the commands interface 1.3 adds.
    pub fn requires(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut names = BTreeSet::new();
        let mut features = self.needs.clone();
        let mut views = vec![self.view];
        for c in self.commands() {
            views.extend(&c.dump);
            for text in &c.tests {
                let name = text.split(';').next().unwrap_or_default();
                names.insert(name.to_owned());
                if let Some((_, f)) = LATER_COMMANDS.iter().find(|(n, _)| *n == name) {
                    features.insert((*f).to_owned());
                }
            }
        }
        for v in views {
            match v {
                Dump::Frame => features.insert("view-frame".to_owned()),
                Dump::Display => features.insert("bidi".to_owned()),
                Dump::Screen | Dump::State => false,
            };
        }
        (names, features)
    }

    /// The views the replay prints for this scenario, each once: the
    /// assertions' view, then each view a dump names.
    pub fn views(&self) -> Vec<Dump> {
        let mut views = vec![self.view];
        for c in self.commands() {
            for d in &c.dump {
                if *d != Dump::State && !views.contains(d) {
                    views.push(*d);
                }
            }
        }
        views
    }

    /// The scenario as the probe runner reads a probe: its input steps,
    /// its checks and its assertions, with each example by the index of
    /// the step before it.
    pub(crate) fn probe(&self) -> Probe {
        let mut steps = Vec::new();
        let mut examples = BTreeMap::new();
        for (item, n) in &self.items {
            match item {
                Item::Command(c) => {
                    if !c.send.is_empty() {
                        steps.push((Step::Send(c.send.clone()), c.line));
                    }
                    steps.extend(c.tests.iter().map(|t| (Step::Cmd(t.clone()), c.line)));
                }
                Item::Check => steps.push((Step::Check, *n)),
                Item::Assert(step) => steps.push((step.clone(), *n)),
                Item::Example(data) => {
                    examples.insert(steps.len().saturating_sub(1), (data.clone(), *n));
                }
            }
        }
        Probe {
            name: self.name.clone(),
            at: format!("{}:1", self.file),
            size: (self.cols != 0).then_some((self.cols, self.rows)),
            view: self.view.name().to_owned(),
            placeholder: self.placeholder.clone(),
            needs: self.needs.clone(),
            fixtures: self.fixtures.clone(),
            steps,
            examples,
        }
    }
}
