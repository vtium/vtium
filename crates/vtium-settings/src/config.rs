//! A parsed head file, or the sections of vtium.conf: the policy, the
//! looks, the heads and the rules (ADR-1401 D3, D9; ADR-1403 D1, D5).

use crate::identity::{Identity, Kind};
use crate::keys::{Key, Level};
use crate::matching::Match;
use crate::outcome::Policy;
use crate::parse::{parse, sections};

/// The head file's commented template. The compiled-in configuration is
/// its parse, so the built-in configuration and the example vtium.conf that a
/// package installs are the same by construction
/// (ADR-1401 D6).
pub const TEMPLATE: &str = include_str!("../templates/heads.conf");

/// What a head does when it loses its last display, its
/// `when-display-lost` (D8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WhenDisplayLost {
    /// `keep`: the sessions keep running and come back when a display of
    /// the head returns.
    #[default]
    Keep,
    /// `hangup`: the sessions are hung up at once.
    Hangup,
    /// `hangup DURATION`: a display returning within this many seconds
    /// resumes the sessions; after it they are hung up.
    HangupAfter(u32),
}

/// One key of the key table that a section sets: the key, its value as
/// the file writes it, and its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
    pub key: &'static Key,
    /// The value's text, which the section reader has checked against the
    /// key's values and bounds and which the console reads as it reads the
    /// same key at the top level.
    pub value: String,
    pub line: usize,
}

/// One `[look NAME]` section of vtium.conf (ADR-1403 D1, D5): what a head
/// that names it draws with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Look {
    pub name: String,
    /// The line of its `[look NAME]`.
    pub line: usize,
    /// The keys of the key table it sets, `font-name` naming its frit
    /// among them, in the file's order.
    pub settings: Vec<Setting>,
    /// The vtlook it names with `vtlook =`, ADR-1304's file of frame
    /// filters; unset, the built-in default vtlook.
    pub vtlook: Option<String>,
    /// The line of its `vtlook`, 0 for none.
    pub vtlook_line: usize,
}

/// One `[head NAME]` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    pub name: String,
    /// The line of its `[head NAME]`, 0 for none.
    pub line: usize,
    /// The name of the head's look, its `look =`; unset, `default`.
    pub look: String,
    /// The line of its `look`, 0 for none.
    pub look_line: usize,
    /// The number of consoles, 1 to [`CONSOLES_MAX`](crate::CONSOLES_MAX).
    pub consoles: u32,
    /// The line of its `consoles`, 0 for none.
    pub consoles_line: usize,
    /// The consoles' command as the file gives it, for vtium to read as
    /// vtium.conf's `login`; unset, vtium.conf's.
    pub login: Option<String>,
    pub when_display_lost: WhenDisplayLost,
    /// `isolated` as the file writes it, `None` for unset, which the
    /// policy decides (D5); [`Config::isolated`] reads it.
    pub isolated: Option<bool>,
    /// `switch-on-enter`, unset `no`: on a VT where the head does not run,
    /// Enter on a keyboard tied to it switches to its VT (ADR-1401 D3,
    /// "Enter on a head's own keyboard"); [`Config::switch_on_enter`]
    /// reads it.
    pub switch_on_enter: bool,
    /// The VTs its `vt` names, each once, in the file's order (ADR-1403
    /// D4). Empty when the head has no `vt`, so it runs on every VT.
    pub vts: Vec<u16>,
    /// The keys of the key table a head's section of vtium.conf sets, such
    /// as `login`, in the file's order; the head file's reading leaves it
    /// empty.
    pub settings: Vec<Setting>,
}

impl Head {
    pub(crate) fn new(name: &str, line: usize) -> Head {
        Head {
            name: name.to_owned(),
            line,
            look: "default".to_owned(),
            look_line: 0,
            consoles: 1,
            consoles_line: 0,
            login: None,
            when_display_lost: WhenDisplayLost::Keep,
            isolated: None,
            switch_on_enter: false,
            vts: Vec::new(),
            settings: Vec::new(),
        }
    }

    /// Whether the head runs on VT `vt`: its `vt` names `vt`, or it has no
    /// `vt` (ADR-1401 D2).
    pub fn runs_on(&self, vt: u16) -> bool {
        self.vts.is_empty() || self.names_vt(vt)
    }

    /// Whether the head's `vt` names VT `vt`. A head with no `vt` names
    /// none, though it runs on every VT.
    pub fn names_vt(&self, vt: u16) -> bool {
        self.vts.contains(&vt)
    }
}

/// Where a rule sends what it matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The head at this index of [`Config::heads`].
    Head(usize),
    /// `roaming`: an input that types into the active head.
    Roaming,
    /// `ignore`: an input vtium never opens, but under open a keyboard
    /// watched for the VT keys alone, the rescue key among them.
    Ignore,
    /// `off`: a display vtium sets off.
    Off,
}

/// One rule of `[rules]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub line: usize,
    pub kind: Kind,
    pub matcher: Match,
    pub target: Target,
}

/// What a head file runs: a valid file, or what survives of one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// `closed` where a `policy = closed` line is readable, else `open`;
    /// it sets the guards and what the rules leave unsaid (D3, D9).
    pub policy: Policy,
    /// The looks in the file's order; the head file has none.
    pub looks: Vec<Look>,
    /// The heads in the file's order.
    pub heads: Vec<Head>,
    /// The rules in the file's order, which each device walks.
    pub rules: Vec<Rule>,
    /// Whether this is what survives of a file with errors (D7): under
    /// open the shipped rules' tail runs after its rules and no head is
    /// isolated, and under closed the display guard lights the boot display
    /// for a notice when no head is left.
    pub survivors: bool,
    /// The input rules a device of a partly broken open file walks when no
    /// rule of [`Config::rules`] places it (D7): `None` for the shipped
    /// rules' own, which vtium.conf's `mouse` shorthand may change.
    pub tail: Option<Vec<Rule>>,
    /// The `[labels]` section's lines, in the file's order.
    pub labels: Vec<Label>,
}

/// The most characters a label holds.
pub const LABEL_MAX: usize = 40;

/// One line of vtium.conf's `[labels]` section, `display MATCH = LABEL`
/// or `input MATCH = LABEL`: a name the person gives the displays or the
/// inputs that a match matches. The match is written as a rule's match, so
/// a label belongs to the part whose rule uses that same match. vtium
/// reads a display's label for its identify overlay (ADR-1401 D12), and
/// vtium-config reads and writes every label (ADR-1403 D8).
/// [`Config::label`] finds a device's label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub line: usize,
    pub kind: Kind,
    pub matcher: Match,
    /// The match as the line writes it, its tokens one blank apart.
    pub selector: String,
    /// The label, with each `\#` read as `#`.
    pub text: String,
}

impl Config {
    /// The compiled-in configuration: the parse of [`TEMPLATE`], which
    /// vtium runs with no head file and with one it cannot use.
    /// `the_compiled_in_configuration_is_the_parse_of_the_template` proves
    /// the template valid; were it not, this is the empty configuration,
    /// whose every display the display guard lights in the default head
    /// and whose every input roams.
    pub fn builtin() -> Config {
        parse(TEMPLATE).map(|p| p.config).unwrap_or_default()
    }

    /// The head named `name`.
    pub fn head(&self, name: &str) -> Option<&Head> {
        self.heads.iter().find(|h| h.name == name)
    }

    /// The look `head` draws with: the `[look NAME]` section its `look =`
    /// names, `default` when it names none. `None` is the built-in default
    /// look, which sets nothing: for a head that names `default` with no
    /// `[look default]` section, and for one whose look has no section or
    /// was dropped by an error, which `vtium check-config` warns of (ADR-1403
    /// D5; ADR-1200 D16).
    pub fn look(&self, head: &Head) -> Option<&Look> {
        self.looks.iter().find(|l| l.name == head.look)
    }

    /// The keys a head's sections set over the top level, each key once,
    /// at the most specific level that sets it: the head's own section,
    /// then its look's (ADR-1403 D5). The caller lays them over the top
    /// level, which the command line has already set over the file's own
    /// lines, since a section wins over both.
    pub fn settings<'a>(&'a self, head: &'a Head) -> Vec<(Level, &'a Setting)> {
        let mut out: Vec<(Level, &Setting)> =
            head.settings.iter().map(|s| (Level::Head, s)).collect();
        for s in self
            .look(head)
            .map(|l| l.settings.as_slice())
            .unwrap_or_default()
        {
            if !out.iter().any(|(_, o)| o.key.name == s.key.name) {
                out.push((Level::Look, s));
            }
        }
        out
    }

    /// The label of the device `id`: the first line of `[labels]` whose
    /// kind is the device's and whose match matches it, as the first
    /// matching rule places a device (ADR-1403 D4).
    pub fn label(&self, id: &Identity) -> Option<&Label> {
        self.labels
            .iter()
            .find(|l| l.kind == id.kind && l.matcher.matches(id))
    }

    /// The rules using a key that needs a udev daemon, each with its line
    /// and the key, for `vtium check-config` to note on a system with none
    /// (D4).
    pub fn needs_udev(&self) -> Vec<(usize, &'static str)> {
        self.rules
            .iter()
            .flat_map(|r| {
                r.matcher
                    .tokens()
                    .iter()
                    .filter(|t| t.needs_udev())
                    .map(|t| (r.line, t.key()))
            })
            .collect()
    }
}

/// The value vtium takes for the key `key` on the head named `head`, and
/// the level it comes from (ADR-1403 D5): the head's section of `text`,
/// else its look's, else the top level's last line that sets it, as vtium
/// runs the file: from what survives of the sections, so an error in one
/// section changes only what that error drops. The command line, which
/// wins over the top level and loses to a section, is the caller's. A head
/// the surviving sections do not declare takes the top level's value.
pub fn value_for(text: &str, head: &str, key: &str) -> Option<(String, Level)> {
    let config = sections(text.as_bytes()).map(|r| r.parsed.config);
    let section = config.as_ref().and_then(|c| {
        let h = c.head(head)?;
        c.settings(h)
            .into_iter()
            .find(|(_, s)| s.key.name == key)
            .map(|(level, s)| (s.value.clone(), level))
    });
    section.or_else(|| crate::conf::setting(text, key).map(|v| (v, Level::Top)))
}
