//! A parsed head file: its policy, its heads and its rules (ADR-1401 D3,
//! D9).

use crate::identity::Kind;
use crate::matching::Match;
use crate::outcome::Policy;
use crate::parse::parse;

/// The head file's commented template. The compiled-in configuration is
/// its parse, so the built-in configuration, the file `vtiumctl template`
/// writes and the files a package installs are the same by construction
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

/// The font keys, as vtium.conf carries them; `None` is a key not set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Font {
    pub name: Option<String>,
    pub size: Option<u32>,
}

/// One `[head NAME]` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    pub name: String,
    /// The line of its `[head NAME]`, 0 for none.
    pub line: usize,
    /// The profile's name; unset, `default`.
    pub profile: String,
    /// The line of its `profile`, 0 for none.
    pub profile_line: usize,
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
}

impl Head {
    pub(crate) fn new(name: &str, line: usize) -> Head {
        Head {
            name: name.to_owned(),
            line,
            profile: "default".to_owned(),
            profile_line: 0,
            consoles: 1,
            consoles_line: 0,
            login: None,
            when_display_lost: WhenDisplayLost::Keep,
            isolated: None,
        }
    }

    /// The head's font: vtium.conf's keys, `base`, with the head's
    /// profile over them (D3). Until profiles have files, the `default`
    /// profile states nothing and any other profile states only its frit,
    /// the frit of its name.
    pub fn font(&self, base: &Font) -> Font {
        let profile_name = (self.profile != "default").then(|| self.profile.clone());
        Font {
            name: profile_name.or_else(|| base.name.clone()),
            size: base.size,
        }
    }
}

/// Where a rule sends what it matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The head at this index of [`Config::heads`].
    Head(usize),
    /// `roaming`: an input that types into the active head.
    Roaming,
    /// `ignore`: an input vtium never opens, but a keyboard watched for
    /// the rescue key alone.
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
