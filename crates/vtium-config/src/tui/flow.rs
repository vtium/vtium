//! The flow the Flow view draws (ADR-1403 D7), as plain data: the
//! displays, the display rules, the heads, the looks, the input rules and
//! the input devices, each with its state, and what each item's lines
//! reach.
//!
//! [`Flow`] is the seam where vtium-config's operations on the draft plug
//! in: they read vtium.conf and the devices present, match each device to
//! its first rule, and hand the view a [`Flow`]. The view only draws what
//! the flow says and moves over it; it decides no match itself. A rule's
//! target is the one thing the view resolves, by name, so a rule that
//! sends to a head that does not exist shows as broken however the flow
//! was made.

/// Whether a device or a rule is about displays or input devices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Display,
    Input,
}

/// The configuration the view draws, and the devices present on it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flow {
    /// The top-level keys: the base card.
    pub base: Base,
    /// The displays: those present, and those a rule names that are not.
    pub displays: Vec<Device>,
    /// The input devices, likewise.
    pub inputs: Vec<Device>,
    /// `[rules]`, in the file's order: a rule's number is its place here
    /// counted from 1.
    pub rules: Vec<Rule>,
    pub heads: Vec<Head>,
    /// The `[look NAME]` sections.
    pub looks: Vec<Look>,
    /// The head that is active now, which roaming devices type into.
    pub active: Option<String>,
    /// The policy's value, which decides what happens to a device no rule
    /// matches.
    pub policy: String,
    /// The draft the flow is drawn from, or `None` when there is none.
    pub draft: Option<Draft>,
    /// The flow is drawn from a configuration: `false` while nothing has
    /// read one, so the view says so instead of drawing an empty flow as if
    /// it were the file.
    pub read: bool,
    /// The `[labels]` section's lines in the file's order: each one's kind,
    /// its match as the line writes it, and its label.
    pub labels: Vec<(Kind, String, String)>,
}

/// The top-level keys of vtium.conf, which every head inherits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Base {
    /// The keys the draft sets, each with its value; a switch set on has an
    /// empty value.
    pub set: Vec<(String, String)>,
    /// How many keys of the key table are left at their defaults.
    pub defaults: usize,
}

/// How the draft stands against the installed file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub name: String,
    pub changes: usize,
    pub problems: usize,
}

/// A display or an input device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// Its name: a connector such as `DP-8`, or an input's name.
    pub name: String,
    /// What tells it apart, under its name: an EDID serial, a port, a class.
    pub detail: String,
    /// Every match key it offers, the strongest first, as `vtium config flow
    /// --json` gives them.
    pub keys: Vec<String>,
    /// `false` for a device a rule names that is not plugged in: drawn as a
    /// grey block marked `configured, not found`.
    pub found: bool,
    /// The rule that places it, its first match, by its index in
    /// [`Flow::rules`]; for a device not found, the rule that names it.
    /// `None` when no rule matches it and the policy decides.
    pub rule: Option<usize>,
}

/// One rule of `[rules]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub kind: Kind,
    /// Its line in the file, counted from 1.
    pub line: usize,
    /// What it matches, as written: `edid:LARGE00001`, `class:keyboard`,
    /// `*`.
    pub selector: String,
    /// Where it sends what it matches: a head's name, `off`, `roaming` or
    /// `ignore`.
    pub target: String,
    pub state: RuleState,
}

/// What a rule does with the devices present.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleState {
    /// It is the first match of at least one device present.
    Matches,
    /// It is the first match of no device present.
    MatchesNothing,
    /// An earlier rule, by its index, matches everything it would.
    Shadowed(usize),
    /// It names a key only udev gives, on a machine with no udev daemon.
    NeedsUdev,
    /// The check finds an error in it, in the check's words.
    Error(String),
}

/// A `[head NAME]` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    pub name: String,
    pub consoles: u32,
    /// The look it names, `default` when it names none.
    pub look: String,
    /// Its other keys as the file sets them: `login`, `when-display-lost`,
    /// `isolated`.
    pub keys: Vec<(String, String)>,
}

/// A `[look NAME]` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Look {
    pub name: String,
    /// The frit it names, empty when it names none.
    pub frit: String,
    /// Its size in points, 0 for automatic.
    pub points: u32,
    /// Its other keys as the file sets them, such as `cell`, each with its
    /// value.
    pub keys: Vec<(String, String)>,
    /// The fonts its frit draws with, each with whether this machine has it.
    pub fonts: Vec<(String, bool)>,
    /// What the check finds wrong with it, in the check's words, such as a
    /// frit that does not exist.
    pub problem: Option<String>,
}

/// The targets that are not heads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Special {
    Roaming,
    Ignore,
    Off,
}

impl Special {
    pub const ALL: [Special; 3] = [Special::Roaming, Special::Ignore, Special::Off];

    /// The word a rule names it by.
    pub const fn name(self) -> &'static str {
        match self {
            Special::Roaming => "roaming",
            Special::Ignore => "ignore",
            Special::Off => "off",
        }
    }

    /// The kind of rule that may send to it.
    pub const fn kind(self) -> Kind {
        match self {
            Special::Off => Kind::Display,
            Special::Roaming | Special::Ignore => Kind::Input,
        }
    }
}

/// The `new…` items at the end of four columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum New {
    DisplayRule,
    Head,
    Look,
    InputRule,
}

/// One item the view draws and the user can select.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Item {
    /// The base card, the top-level keys.
    Base,
    Display(usize),
    Input(usize),
    Rule(usize),
    Head(usize),
    /// The look block beside a head, by the head's index.
    HeadLook(usize),
    /// A look no head uses, by its index in [`Flow::looks`].
    Look(usize),
    Special(Special),
    /// The block that takes the devices no rule matches.
    Policy,
    New(New),
}

/// Where a rule sends what it matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Head(usize),
    Special(Special),
    /// A name that is no head and no target its kind of rule may take.
    Missing,
}

/// The number of columns the view draws.
pub const COLUMNS: usize = 6;

/// The selector keys that name one device; the others name a kind.
const ONE_DEVICE: [&str; 7] = [
    "edid",
    "connector",
    "connector-path",
    "serial",
    "path",
    "bluetooth",
    "name",
];

/// Whether `selector` names a kind of device, by `*`, `class`, `id`,
/// `card` or `udev`, rather than one device.
pub fn general(selector: &str) -> bool {
    let key = selector.split(':').next().unwrap_or_default();
    !ONE_DEVICE.contains(&key)
}

impl Flow {
    /// The rules of `kind`, each with its index.
    pub fn rules_of(&self, kind: Kind) -> impl Iterator<Item = (usize, &Rule)> {
        self.rules
            .iter()
            .enumerate()
            .filter(move |(_, r)| r.kind == kind)
    }

    /// The devices of `kind`.
    pub fn devices(&self, kind: Kind) -> &[Device] {
        match kind {
            Kind::Display => &self.displays,
            Kind::Input => &self.inputs,
        }
    }

    /// Where the rule `rule` sends what it matches.
    pub fn target(&self, rule: &Rule) -> Target {
        if let Some(h) = self.heads.iter().position(|h| h.name == rule.target) {
            return Target::Head(h);
        }
        Special::ALL
            .into_iter()
            .find(|s| s.name() == rule.target && s.kind() == rule.kind)
            .map_or(Target::Missing, Target::Special)
    }

    /// The look the head `head` names, by its index, when the file has it.
    pub fn look_of(&self, head: usize) -> Option<usize> {
        let name = &self.heads.get(head)?.look;
        self.looks.iter().position(|l| l.name == *name)
    }

    /// Whether any head names the look `look`.
    pub fn used(&self, look: usize) -> bool {
        self.looks
            .get(look)
            .is_some_and(|l| self.heads.iter().any(|h| h.look == l.name))
    }

    /// The items of each column, top to bottom, as the view stacks them.
    pub fn columns(&self) -> [Vec<Item>; COLUMNS] {
        let rules = |kind, new| {
            self.rules_of(kind)
                .map(|(i, _)| Item::Rule(i))
                .chain(std::iter::once(Item::New(new)))
                .collect()
        };
        let mut heads: Vec<Item> = (0..self.heads.len()).map(Item::Head).collect();
        heads.push(Item::New(New::Head));
        heads.extend(Special::ALL.map(Item::Special));
        heads.push(Item::Policy);
        let mut looks: Vec<Item> = (0..self.heads.len()).map(Item::HeadLook).collect();
        looks.extend(
            (0..self.looks.len())
                .filter(|l| !self.used(*l))
                .map(Item::Look),
        );
        looks.push(Item::New(New::Look));
        [
            std::iter::once(Item::Base)
                .chain((0..self.displays.len()).map(Item::Display))
                .collect(),
            rules(Kind::Display, New::DisplayRule),
            heads,
            looks,
            rules(Kind::Input, New::InputRule),
            (0..self.inputs.len()).map(Item::Input).collect(),
        ]
    }

    /// The column `item` sits in.
    pub fn column_of(&self, item: Item) -> usize {
        match item {
            Item::Base | Item::Display(_) => 0,
            Item::Rule(r) => match self.rules.get(r).map(|r| r.kind) {
                Some(Kind::Input) => 4,
                _ => 1,
            },
            Item::New(New::DisplayRule) => 1,
            Item::Head(_) | Item::Special(_) | Item::Policy | Item::New(New::Head) => 2,
            Item::HeadLook(_) | Item::Look(_) | Item::New(New::Look) => 3,
            Item::New(New::InputRule) => 4,
            Item::Input(_) => 5,
        }
    }

    /// Whether `item` is in the flow as it stands.
    pub fn has(&self, item: Item) -> bool {
        self.columns().iter().any(|c| c.contains(&item))
    }

    /// The items `item`'s lines reach toward the heads, which `f` steps
    /// through: a device's rule; a rule's target; and from a head, a look
    /// or a target that is not a head, the rules that send to it.
    pub fn follows(&self, item: Item) -> Vec<Item> {
        let device = |d: Option<&Device>| d.and_then(|d| d.rule).map(Item::Rule);
        let sending = |to: &dyn Fn(Target, Kind) -> bool| -> Vec<Item> {
            self.rules
                .iter()
                .enumerate()
                .filter(|(_, r)| to(self.target(r), r.kind))
                .map(|(i, _)| Item::Rule(i))
                .collect()
        };
        match item {
            Item::Display(d) => device(self.displays.get(d)).into_iter().collect(),
            Item::Input(d) => device(self.inputs.get(d)).into_iter().collect(),
            Item::Rule(r) => {
                let Some(rule) = self.rules.get(r) else {
                    return Vec::new();
                };
                match (self.target(rule), rule.kind) {
                    (Target::Head(h), Kind::Display) => vec![Item::Head(h)],
                    (Target::Head(h), Kind::Input) => vec![Item::HeadLook(h)],
                    (Target::Special(s), _) => vec![Item::Special(s)],
                    (Target::Missing, _) => Vec::new(),
                }
            }
            Item::Head(h) => sending(&|t, k| t == Target::Head(h) && k == Kind::Display),
            Item::HeadLook(h) => sending(&|t, k| t == Target::Head(h) && k == Kind::Input),
            Item::Special(s) => sending(&|t, _| t == Target::Special(s)),
            Item::Base | Item::Look(_) | Item::Policy | Item::New(_) => Vec::new(),
        }
    }

    /// The name an item goes by in the status line's list of tags and on
    /// the route card.
    pub fn name(&self, item: Item) -> String {
        match item {
            Item::Base => "base".into(),
            Item::Display(d) => self
                .displays
                .get(d)
                .map(|d| d.name.clone())
                .unwrap_or_default(),
            Item::Input(d) => self
                .inputs
                .get(d)
                .map(|d| d.name.clone())
                .unwrap_or_default(),
            Item::Rule(r) => format!("rule {}", r.saturating_add(1)),
            Item::Head(h) => self
                .heads
                .get(h)
                .map(|h| h.name.clone())
                .unwrap_or_default(),
            Item::HeadLook(h) => self
                .heads
                .get(h)
                .map(|h| h.look.clone())
                .unwrap_or_default(),
            Item::Look(l) => self
                .looks
                .get(l)
                .map(|l| l.name.clone())
                .unwrap_or_default(),
            Item::Special(s) => s.name().into(),
            Item::Policy => format!("policy {}", self.policy),
            Item::New(_) => String::new(),
        }
    }

    /// The heads that can be active: those a display present is sent to.
    pub fn activatable(&self) -> Vec<usize> {
        (0..self.heads.len())
            .filter(|h| {
                self.displays.iter().any(|d| {
                    d.found
                        && d.rule
                            .and_then(|r| self.rules.get(r))
                            .is_some_and(|r| self.target(r) == Target::Head(*h))
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::super::fixture;
    use super::*;

    #[test]
    fn a_rule_s_target_resolves_by_name_and_kind() {
        let f = fixture::flow();
        assert_eq!(f.target(&f.rules[0]), Target::Head(0));
        assert_eq!(f.target(&f.rules[4]), Target::Special(Special::Roaming));
        // a display rule may not send to roaming, nor to a head that is gone
        let mut r = f.rules[0].clone();
        r.target = "roaming".into();
        assert_eq!(f.target(&r), Target::Missing);
        r.target = "tv".into();
        assert_eq!(f.target(&r), Target::Missing);
    }

    #[test]
    fn the_columns_stack_as_the_figure_draws_them() {
        let f = fixture::flow();
        let [displays, drules, heads, looks, irules, inputs] = f.columns();
        assert_eq!(displays, [Item::Base, Item::Display(0), Item::Display(1)]);
        assert_eq!(
            drules,
            [
                Item::Rule(0),
                Item::Rule(1),
                Item::Rule(2),
                Item::New(New::DisplayRule)
            ]
        );
        assert_eq!(
            heads,
            [
                Item::Head(0),
                Item::Head(1),
                Item::New(New::Head),
                Item::Special(Special::Roaming),
                Item::Special(Special::Ignore),
                Item::Special(Special::Off),
                Item::Policy,
            ]
        );
        assert_eq!(
            looks,
            [
                Item::HeadLook(0),
                Item::HeadLook(1),
                Item::Look(2),
                Item::New(New::Look)
            ]
        );
        assert_eq!(
            irules,
            [
                Item::Rule(3),
                Item::Rule(4),
                Item::Rule(5),
                Item::New(New::InputRule)
            ]
        );
        assert_eq!(inputs, [Item::Input(0), Item::Input(1), Item::Input(2)]);
        for (c, items) in f.columns().iter().enumerate() {
            for item in items {
                assert_eq!(f.column_of(*item), c, "{item:?}");
            }
        }
    }

    #[test]
    fn follow_goes_toward_the_heads() {
        let f = fixture::flow();
        assert_eq!(f.follows(Item::Display(0)), [Item::Rule(0)]);
        assert_eq!(f.follows(Item::Rule(0)), [Item::Head(0)]);
        assert_eq!(f.follows(Item::Input(1)), [Item::Rule(4)]);
        assert_eq!(f.follows(Item::Rule(4)), [Item::Special(Special::Roaming)]);
        assert_eq!(f.follows(Item::Rule(3)), [Item::HeadLook(0)]);
        // a head steps through the display rules that send to it
        assert_eq!(f.follows(Item::Head(0)), [Item::Rule(0), Item::Rule(2)]);
        assert_eq!(f.follows(Item::Base), []);
    }

    #[test]
    fn a_selector_names_one_device_or_a_kind() {
        for s in [
            "edid:X",
            "path:pci-0",
            "serial:1",
            "name:Power Button",
            "bluetooth:aa",
        ] {
            assert!(!general(s), "{s}");
        }
        for s in [
            "*",
            "class:keyboard",
            "id:046d:c31c",
            "card:card1",
            "udev:ID_X=1",
        ] {
            assert!(general(s), "{s}");
        }
    }
}
