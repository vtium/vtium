//! vtium-config-macro reads and writes vtium-config's macros. A macro is a
//! short UTF-8 text file with one step on each line: a key to press, text
//! to type, a label to click, a check of the screen or of the TUI's state,
//! a pause, a screenshot, or a request for a live run. Comments and blank
//! lines are kept as lines of the macro too.
//!
//! ```
//! use vtium_config_macro::{Macro, Step};
//!
//! let text = "# Open the fonts tab.\nAlt+N\nExpect At fonts\n";
//! let m: Macro = text.parse().unwrap();
//! assert_eq!(m.steps().len(), 3);
//! assert!(matches!(m.steps()[1], Step::Key(_)));
//! // The canonical text parses back into the same macro.
//! assert_eq!(m.to_string().parse::<Macro>().unwrap(), m);
//! ```
//!
//! The crate knows nothing of the TUI. Its keys are names with modifiers,
//! and the player in vtium-config turns them into the TUI's own key type,
//! refusing a key the TUI cannot read. The crate already takes a key as a
//! terminal sends it: Ctrl+I is Tab, and a chord that no terminal can
//! send as a key, such as Alt+[, does not parse. A macro may come over ssh
//! from another machine, so the parser treats its input as untrusted: it
//! never panics, and it holds every bound in [`bounds`].
//!
//! References:
//! - ADR-1208 D1: a macro as a short text file of steps, and its limits.
//! - ADR-1208 D2: the grammar and the two example macros.
//! - ADR-1208 D3: a key as a terminal sends it.
//! - ADR-1208 D6: the key path that names a failure.
//! - ADR-1208 D7: the format's crate, with no dependency beyond std.

#![forbid(unsafe_code)]
// A macro is untrusted input, so nothing in it may panic the parser.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]

mod chord;
mod parse;
mod path;
mod write;

use std::time::Duration;

pub use chord::Unpressable;
pub use parse::{Error, Parser, parse, parse_bytes};
pub use path::{KeyPath, failure_name, failure_report};

/// The bounds on a macro, each one in this one place.
pub mod bounds {
    use std::time::Duration;

    /// The most bytes a macro file holds, counting a newline after each
    /// line. A file is read whole before its first step plays, so this
    /// bounds the memory it can take. A stream, as `--macro -` reads, is
    /// bounded by [`STEPS_MAX`] and [`LINE_MAX`] instead, since its parser
    /// keeps one line at a time.
    ///
    /// References:
    /// - ADR-1208 D1: a file of at most 1 MiB, which does not bound a
    ///   stream.
    pub const FILE_MAX: usize = 1024 * 1024;

    /// The most bytes one line holds, without its newline. It keeps the
    /// one-line error about a line short, and it stops a file of one huge
    /// line before the parser reads its words.
    ///
    /// References:
    /// - ADR-1208 D1: a line of at most 4,096 bytes.
    pub const LINE_MAX: usize = 4096;

    /// The most lines a macro holds, comments and blank lines included,
    /// whether it comes from a file or a stream. A macro of this many keys
    /// at their highest count presses a million keys, which bounds how
    /// long a test or a remote run can take.
    ///
    /// References:
    /// - ADR-1208 D1: at most 10,000 steps, which bound `--macro -`.
    pub const STEPS_MAX: usize = 10_000;

    /// The highest count after a key, as in `Down 3`. A typo such as
    /// `Down 30000` then fails to parse rather than pressing a key thirty
    /// thousand times.
    ///
    /// References:
    /// - ADR-1208 D1: a count from 1 to 100.
    pub const COUNT_MAX: u8 = 100;

    /// The most characters `Type` types in one step. Each character is a
    /// key, pressed 100 milliseconds apart in visible play, so this keeps
    /// one step under two minutes, and the step's line under
    /// [`LINE_MAX`] for any characters.
    pub const TYPE_MAX: usize = 1000;

    /// The most characters in a label that `Click` and `Click Mouse` look
    /// for, and in a text that `Expect` looks for. A label is a few words
    /// on one row, and a text that `Expect` checks spans a few rows at
    /// most, so a longer one is a mistake. It also bounds the player's
    /// search of the screen for the label.
    pub const LABEL_MAX: usize = 256;

    /// The longest `Sleep`. A typo such as `Sleep 600s` then fails to
    /// parse rather than stalling a run for ten minutes.
    ///
    /// References:
    /// - ADR-1208 D1: a pause of at most 60 seconds.
    pub const SLEEP_MAX: Duration = Duration::from_secs(60);

    /// The most bytes in a `Screenshot` name and in the place that
    /// `Expect At` names. A screenshot's name becomes a file name in the
    /// debug folder, and the places of the TUI's interface tree are a few
    /// dotted words.
    pub const NAME_MAX: usize = 64;
}

use bounds::{COUNT_MAX, LABEL_MAX, LINE_MAX, NAME_MAX, SLEEP_MAX, TYPE_MAX};

/// A key the macro presses, without its modifiers.
///
/// `Esc` parses as [`Key::Escape`], and the canonical text writes
/// `Escape`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Enter,
    Escape,
    Tab,
    Space,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    /// One character, such as `q` in `q` or `S` in `Ctrl+S`.
    Char(Character),
}

impl Key {
    /// Each named key with its name, in the order the grammar lists them.
    /// `Esc` is the one other spelling the parser takes.
    pub const NAMED: [(&'static str, Key); 25] = [
        ("Enter", Key::Enter),
        ("Escape", Key::Escape),
        ("Tab", Key::Tab),
        ("Space", Key::Space),
        ("Backspace", Key::Backspace),
        ("Up", Key::Up),
        ("Down", Key::Down),
        ("Left", Key::Left),
        ("Right", Key::Right),
        ("PageUp", Key::PageUp),
        ("PageDown", Key::PageDown),
        ("Home", Key::Home),
        ("End", Key::End),
        ("F1", Key::F1),
        ("F2", Key::F2),
        ("F3", Key::F3),
        ("F4", Key::F4),
        ("F5", Key::F5),
        ("F6", Key::F6),
        ("F7", Key::F7),
        ("F8", Key::F8),
        ("F9", Key::F9),
        ("F10", Key::F10),
        ("F11", Key::F11),
        ("F12", Key::F12),
    ];
}

/// One character that a key step can press. It is never a control
/// character or a space of any kind, because a space is [`Key::Space`] and
/// a control character has no place in a line of text. It is never the
/// byte-order mark U+FEFF either, which the parser drops at the start of a
/// file, so a key of it on the first line would vanish from its own
/// canonical text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Character(char);

impl Character {
    /// The character as a key, or `None` for a control character, a space
    /// or the byte-order mark.
    pub fn new(c: char) -> Option<Character> {
        (!c.is_control() && !c.is_whitespace() && c != '\u{feff}').then_some(Character(c))
    }

    /// The character.
    pub fn get(self) -> char {
        self.0
    }
}

/// The modifiers held while a key is pressed. The canonical text writes
/// them in the order Ctrl, Alt, Shift.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    /// No modifier.
    pub const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
    };

    /// Whether no modifier is held.
    pub fn is_empty(self) -> bool {
        self == Mods::NONE
    }
}

/// A key with the set of modifiers held while it is pressed, as in
/// `Alt+Shift+L`.
///
/// The crate keeps a key as the macro wrote it: `Shift+a` and `A` are two
/// different presses here, and the player decides that both type a
/// capital A. A key step is the one exception: it takes a chord that
/// shares its byte with another key as that key (see [`KeyStep::new`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyPress {
    pub mods: Mods,
    pub key: Key,
}

impl KeyPress {
    /// A key with no modifier.
    pub fn plain(key: Key) -> KeyPress {
        KeyPress {
            mods: Mods::NONE,
            key,
        }
    }
}

/// How many times a key step presses its key, from 1 to
/// [`bounds::COUNT_MAX`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Count(u8);

impl Count {
    /// One press.
    pub const ONE: Count = Count(1);

    /// The count, or `None` outside 1 to [`bounds::COUNT_MAX`].
    pub fn new(n: u8) -> Option<Count> {
        (1..=COUNT_MAX).contains(&n).then_some(Count(n))
    }

    /// The count.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// A key step: a key with its modifiers, pressed one or more times.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyStep {
    press: KeyPress,
    count: Count,
}

impl KeyStep {
    /// The step that presses the key as a terminal sends it. Ctrl+I,
    /// Ctrl+M, Ctrl+H and Ctrl+[ send the bytes of Tab, Enter, Backspace
    /// and Escape, so the step presses those keys. Alt and `[`, `]`, `_` or
    /// a capital `P` open a control sequence or a control string, which no
    /// terminal sends as a key, so those are refused. `#` with no modifier
    /// is refused too, because a line that starts with `#` is a comment: a
    /// macro types it with `Type "#"` instead.
    pub fn new(press: KeyPress, count: Count) -> Result<KeyStep, Unpressable> {
        chord::as_sent(press).map(|press| KeyStep { press, count })
    }

    /// The key and its modifiers.
    pub fn press(self) -> KeyPress {
        self.press
    }

    /// How many times the step presses the key.
    pub fn count(self) -> Count {
        self.count
    }
}

/// The text that `Type` types, one key for each character, exactly as the
/// macro wrote it.
///
/// It holds 1 to [`bounds::TYPE_MAX`] characters, no control character,
/// and not all three kinds of quote, because the text sits between double
/// quotes, single quotes or backticks and has no escapes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Text(String);

impl Text {
    /// The text, or `None` when it breaks one of the rules above.
    pub fn new(s: &str) -> Option<Text> {
        let n = s.chars().count();
        ((1..=TYPE_MAX).contains(&n) && quotable(s)).then(|| Text(s.to_owned()))
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A label that `Click` and `Click Mouse` look for, or a text that
/// `Expect` looks for on the screen.
///
/// The player reads the screen with one space between each word, so a
/// label holds words with one space between each, and no space at its
/// start or end. It holds 1 to [`bounds::LABEL_MAX`] characters, no
/// control character, and not all three kinds of quote.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Label(String);

impl Label {
    /// The label, or `None` when it breaks one of the rules above.
    pub fn new(s: &str) -> Option<Label> {
        let n = s.chars().count();
        ((1..=LABEL_MAX).contains(&n) && quotable(s) && single_spaced(s))
            .then(|| Label(s.to_owned()))
    }

    /// The label.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The quotes a text can sit between, in the order the canonical text
/// prefers them: double quotes, single quotes, then backticks, as in VHS.
pub(crate) const QUOTES: [char; 3] = ['"', '\'', '`'];

/// Whether a text can sit between quotes: no control character, and not
/// all three kinds of quote, since a macro has no escapes.
fn quotable(s: &str) -> bool {
    !s.chars().any(char::is_control) && !QUOTES.iter().all(|&q| s.contains(q))
}

/// Whether a text is words with one space between each, as the player
/// reads the screen.
fn single_spaced(s: &str) -> bool {
    !s.starts_with(' ')
        && !s.ends_with(' ')
        && !s.contains("  ")
        && !s.contains(|c: char| c.is_whitespace() && c != ' ')
}

/// A place in the TUI's interface tree, as `fonts` or `looks.look`: words
/// of lowercase letters, digits, `-` and `_`, joined by dots, at most
/// [`bounds::NAME_MAX`] bytes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node(String);

impl Node {
    /// The place, or `None` when it breaks the rule above.
    pub fn new(s: &str) -> Option<Node> {
        let word = |w: &str| {
            !w.is_empty()
                && w.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        };
        (s.len() <= NAME_MAX && s.split('.').all(word)).then(|| Node(s.to_owned()))
    }

    /// The place.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The name of a screenshot, which debug mode writes to `NAME.txt`.
///
/// It holds 1 to [`bounds::NAME_MAX`] bytes of ASCII letters, digits, `-`,
/// `_` and `.`, and does not start with a dot, so it names a file inside
/// the debug folder and never a hidden file or a path. It is not
/// `failure` or `live`, and does not start with `step-`, because debug
/// mode writes the files of those names itself. A macro uses each name
/// once, which the parser checks.
///
/// References:
/// - ADR-1208 D2: the rule for a Screenshot's name.
/// - ADR-1208 D6: debug mode's own files.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name(String);

impl Name {
    /// The name, or `None` when it breaks one of the rules above.
    pub fn new(s: &str) -> Option<Name> {
        (Name::plain(s) && !Name::reserved(s)).then(|| Name(s.to_owned()))
    }

    /// Whether a name is one plain file name: 1 to [`bounds::NAME_MAX`]
    /// bytes of the characters above, not starting with a dot.
    pub(crate) fn plain(s: &str) -> bool {
        let chars = s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
        (1..=NAME_MAX).contains(&s.len()) && chars && !s.starts_with('.')
    }

    /// Whether debug mode writes a file of this name itself: `failure`,
    /// `live`, or a name that starts with `step-`.
    pub fn reserved(s: &str) -> bool {
        s == "failure" || s == "live" || s.starts_with("step-")
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How long a `Sleep` waits, in whole milliseconds, from 1 ms to
/// [`bounds::SLEEP_MAX`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pause(u32);

impl Pause {
    /// The pause, or `None` for none at all or one longer than
    /// [`bounds::SLEEP_MAX`].
    pub fn from_millis(ms: u32) -> Option<Pause> {
        (ms >= 1 && u128::from(ms) <= SLEEP_MAX.as_millis()).then_some(Pause(ms))
    }

    /// The pause in milliseconds.
    pub fn millis(self) -> u32 {
        self.0
    }

    /// The pause as a duration.
    pub fn duration(self) -> Duration {
        Duration::from_millis(u64::from(self.0))
    }
}

/// What an `Expect` step checks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expectation {
    /// `Expect "TEXT"`: the screen shows the text.
    Shows(Label),
    /// `Expect Not "TEXT"`: the screen does not show the text.
    ShowsNot(Label),
    /// `Expect At NODE`: the TUI is at that place of its interface tree.
    At(Node),
    /// `Expect Saved`: the TUI holds no change that waits to be saved.
    Saved,
    /// `Expect Unsaved`: the TUI holds a change that waits to be saved.
    Unsaved,
    /// `Expect Exited N`: the TUI has exited with status N.
    Exited(u8),
}

/// A comment's text, everything after the `#`, as the macro wrote it. It
/// holds no control character other than a tab, and fits a line of
/// [`bounds::LINE_MAX`] bytes with its `#`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Comment(String);

impl Comment {
    /// The comment, or `None` when it breaks the rule above.
    pub fn new(s: &str) -> Option<Comment> {
        let ok = s.len() < LINE_MAX && !s.chars().any(|c| c.is_control() && c != '\t');
        ok.then(|| Comment(s.to_owned()))
    }

    /// The comment's text, after the `#`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One line of a macro.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    /// A key by name or a character, with any modifiers and a count, as
    /// in `Enter`, `Ctrl+S` or `Down 3`.
    Key(KeyStep),
    /// `Type "TEXT"`: types each character as its own key.
    Type(Text),
    /// `Click "LABEL"`: acts on the label through the keys a person would
    /// press to reach it.
    Click(Label),
    /// `Click Mouse "LABEL"`: clicks the mouse at the label's character
    /// cell, so a macro can test the TUI's mouse handling.
    ClickMouse(Label),
    /// `Expect ...`: checks the screen or the TUI's state.
    Expect(Expectation),
    /// `Sleep 500ms`, `Sleep 2s` or `Sleep 0.5`: waits in visible play.
    Sleep(Pause),
    /// `Screenshot NAME`: writes the screen in debug mode.
    Screenshot(Name),
    /// `Set Live`: asks for a live run. It is the first step, after only
    /// comments and blank lines.
    SetLive,
    /// `# ...`: a comment.
    Comment(Comment),
    /// A blank line.
    Blank,
}

impl Step {
    /// Whether the step acts when the macro plays, which every step does
    /// apart from a comment and a blank line.
    pub fn acts(&self) -> bool {
        !matches!(self, Step::Comment(_) | Step::Blank)
    }
}

/// A macro: its lines in order, each one a [`Step`]. The step at index `i`
/// is on line `i + 1`.
///
/// Every `Macro` writes canonical text that parses back into the same
/// `Macro`, so a macro holds at most [`bounds::STEPS_MAX`] lines whose
/// text fits in [`bounds::FILE_MAX`] bytes, `Set Live` comes only before
/// any step that acts, and no two screenshots share a name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Macro {
    steps: Vec<Step>,
}

impl Macro {
    /// The macro of these lines, or the first error that its text would
    /// meet when parsed.
    pub fn new(steps: Vec<Step>) -> Result<Macro, Error> {
        let mut parser = Parser::file();
        for step in &steps {
            parser.accept(step)?;
        }
        Ok(Macro { steps })
    }

    /// The macro's lines, in order.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The steps that act, with the number of the line each one is on.
    pub fn numbered(&self) -> impl Iterator<Item = (usize, &Step)> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.acts())
            .map(|(i, s)| (i.saturating_add(1), s))
    }

    /// Whether the macro asks for a live run with `Set Live`.
    pub fn is_live(&self) -> bool {
        self.steps.contains(&Step::SetLive)
    }
}

impl std::str::FromStr for Macro {
    type Err = Error;

    fn from_str(s: &str) -> Result<Macro, Error> {
        parse(s)
    }
}
