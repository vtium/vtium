//! The key path: every key the player pressed from the start, which names
//! a failure the way the scenario tests name one. A `Click` adds the keys
//! it pressed to reach its label, so the path, played as a macro, reaches
//! the same screen from the same start.

use std::fmt;

use crate::bounds::COUNT_MAX;
use crate::parse::parse_key;
use crate::{Character, Count, Error, Key, KeyPress, KeyStep, Macro, Step, Text};

/// The keys pressed from the start of a run, in order.
///
/// Its text writes the keys in the macro's syntax with one space between
/// each, as in `Alt+N Down Down Space`, and reads back from that text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyPath {
    keys: Vec<KeyPress>,
}

impl KeyPath {
    /// A path with no keys yet.
    pub fn new() -> KeyPath {
        KeyPath::default()
    }

    /// Adds the next key pressed.
    pub fn push(&mut self, key: KeyPress) {
        self.keys.push(key);
    }

    /// The keys, in the order they were pressed.
    pub fn keys(&self) -> &[KeyPress] {
        &self.keys
    }

    /// The path as a macro of key steps, one line for each run of the same
    /// key, with the run's length as its count. `#` with no modifier, which
    /// a line would read as a comment, becomes `Type "#"`.
    ///
    /// A path longer than a macro holds is an error, which names the line
    /// that would pass the bound.
    pub fn to_macro(&self) -> Result<Macro, Error> {
        let hash = KeyPress::plain(Key::Char(Character('#')));
        let mut steps = Vec::new();
        let mut keys = self.keys.iter().peekable();
        while let Some(&key) = keys.next() {
            if key == hash {
                steps.push(Step::Type(Text("#".to_owned())));
                continue;
            }
            let mut n = 1u8;
            while n < COUNT_MAX && keys.next_if_eq(&&key).is_some() {
                n = n.saturating_add(1);
            }
            let count = Count::new(n).unwrap_or(Count::ONE);
            steps.extend(KeyStep::new(key, count).map(Step::Key));
        }
        Macro::new(steps)
    }
}

impl fmt::Display for KeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, key) in self.keys.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{key}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for KeyPath {
    type Err = String;

    /// Reads the keys of a path's text, separated by spaces. Each word is
    /// one key, so a digit is a key of its own and never a count.
    fn from_str(s: &str) -> Result<KeyPath, String> {
        let keys = s
            .split_whitespace()
            .map(|w| parse_key(w).map_err(|_| format!("{w} is not a key in a key path.")))
            .collect::<Result<_, _>>()?;
        Ok(KeyPath { keys })
    }
}

/// The name of a failure, as the scenario tests write one: the scene,
/// such as the kind of terminal and the grid, then the keys from the
/// start, as in `vtium 80×25: Up Up Enter Down r q l`.
pub fn failure_name(scene: &str, path: &KeyPath) -> String {
    if path.keys.is_empty() {
        return format!("{scene}: no keys");
    }
    format!("{scene}: {path}")
}

/// The report of a failed step: the file and the line, the step as the
/// macro writes it, why it failed, and the keys from the start on a line
/// of their own.
///
/// ```
/// use vtium_config_macro::{KeyPath, failure_report, parse};
///
/// let m = parse("Expect \"always active\"").unwrap();
/// let keys: KeyPath = "Alt+N Down Down Space".parse().unwrap();
/// let report = failure_report("refuse-builtin.vtmacro", 6, &m.steps()[0], "the screen does not show it.", &keys);
/// assert_eq!(report, "refuse-builtin.vtmacro:6: Expect \"always active\": the screen does not show it.\nKeys from the start: Alt+N Down Down Space");
/// ```
///
/// References:
/// - ADR-1208 D6: the failure report and its example.
pub fn failure_report(
    file: &str,
    line: usize,
    step: &Step,
    reason: &str,
    path: &KeyPath,
) -> String {
    let keys = if path.keys.is_empty() {
        "none.".to_owned()
    } else {
        path.to_string()
    };
    format!("{file}:{line}: {step}: {reason}\nKeys from the start: {keys}")
}
