//! A parsed head file: its heads, its rules and its guards (ADR-1401 D3,
//! D9).

use crate::identity::Kind;
use crate::matching::Match;
use crate::parse::parse;

/// The head file's commented template. The compiled-in configuration is
/// its parse, so the built-in configuration, the file `vtiumctl template`
/// writes and the files a package installs are the same by construction
/// (ADR-1401 D6).
pub const TEMPLATE: &str = include_str!("../templates/heads.conf");

/// What a head does when it loses its last display (D8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnLoss {
    /// The sessions keep running and come back when a display of the head
    /// returns.
    #[default]
    Nohup,
    /// The sessions are hung up at once.
    Hangup,
    /// A display returning within this many seconds resumes the sessions;
    /// after it they are hung up.
    HangupAfter(u32),
}

/// The font keys, as kmscon.conf carries them; `None` is a key not set.
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
    /// The number of virtual consoles, 1 to [`CONSOLES_MAX`](crate::CONSOLES_MAX).
    pub consoles: u32,
    /// The consoles' command as the file gives it, for vtium to read as
    /// kmscon.conf's `login`; unset, kmscon.conf's.
    pub login: Option<String>,
    pub on_loss: OnLoss,
    /// Whether the rescue key may take the keyboards this head claims.
    pub rescue: bool,
    /// The head's own font keys.
    pub font: Font,
}

impl Head {
    pub(crate) fn new(name: &str, line: usize) -> Head {
        Head {
            name: name.to_owned(),
            line,
            profile: "default".to_owned(),
            consoles: 1,
            login: None,
            on_loss: OnLoss::Nohup,
            rescue: true,
            font: Font::default(),
        }
    }

    /// The head's font: the more specific setting wins (D3). kmscon.conf's
    /// keys, `base`, apply to every head; the head's profile overrides what
    /// it states; the head's own keys override both. Until profiles have
    /// files, the `default` profile states nothing and any other profile
    /// states only its frit, the frit of its name.
    pub fn font(&self, base: &Font) -> Font {
        let profile_name = (self.profile != "default").then(|| self.profile.clone());
        Font {
            name: self
                .font
                .name
                .clone()
                .or(profile_name)
                .or_else(|| base.name.clone()),
            size: self.font.size.or(base.size),
        }
    }
}

/// Where a rule sends what it matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The head at this index of [`Config::heads`].
    Head(usize),
    /// An input follows the focus chord.
    Focus,
    /// An input vtium never opens, but a keyboard watched for the rescue
    /// key alone.
    Ignore,
    /// A display vtium sets off.
    Dark,
}

/// One rule of `[rules]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub line: usize,
    pub kind: Kind,
    pub matcher: Match,
    pub target: Target,
}

/// The `[guards]` switches, each on unless the file turns it off (D9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guards {
    pub anti_lockout: bool,
    pub rescue_key: bool,
    pub never_dark: bool,
}

impl Default for Guards {
    fn default() -> Guards {
        Guards {
            anti_lockout: true,
            rescue_key: true,
            never_dark: true,
        }
    }
}

/// A valid head file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// The heads in the file's order.
    pub heads: Vec<Head>,
    /// The rules in the file's order, which each device walks.
    pub rules: Vec<Rule>,
    pub guards: Guards,
}

impl Config {
    /// The compiled-in configuration: the parse of [`TEMPLATE`], which
    /// vtium runs with no head file, with an invalid one, and under
    /// `vtium.heads=builtin`. `the_compiled_in_configuration_is_the_parse_of_the_template`
    /// proves the template valid; were it not, this is the empty
    /// configuration, whose every display the display guard lights in the
    /// default head and whose every input follows the focus.
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
