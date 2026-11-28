//! The canonical text of a macro: one line for each step, each followed by
//! a newline. The parser reads this text back into the same macro.
//!
//! The canonical line drops the spaces around a step's words, writes one
//! space between them, writes the modifiers in the order Ctrl, Alt,
//! Shift, writes `Escape` for `Esc` and no count of 1, writes a pause in
//! seconds when it is whole seconds, and puts a text in double quotes
//! unless it holds one, then in single quotes unless it holds one, and
//! otherwise in backticks.

use std::fmt;

use crate::{Expectation, Key, KeyPress, KeyStep, Macro, QUOTES, Step};

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Key::Char(c) = self {
            return write!(f, "{}", c.get());
        }
        let name = Key::NAMED
            .iter()
            .find(|(_, k)| k == self)
            .map_or("", |(name, _)| name);
        f.write_str(name)
    }
}

impl fmt::Display for KeyPress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.mods.alt {
            f.write_str("Alt+")?;
        }
        if self.mods.shift {
            f.write_str("Shift+")?;
        }
        write!(f, "{}", self.key)
    }
}

impl fmt::Display for KeyStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.press())?;
        match self.count().get() {
            1 => Ok(()),
            n => write!(f, " {n}"),
        }
    }
}

/// A text between the first kind of quote it does not hold. A text or a
/// label never holds all three kinds.
fn quoted(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    let q = QUOTES.into_iter().find(|&q| !s.contains(q)).unwrap_or('"');
    write!(f, "{q}{s}{q}")
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Key(k) => write!(f, "{k}"),
            Step::Type(t) => {
                f.write_str("Type ")?;
                quoted(f, t.as_str())
            }
            Step::Click(l) => {
                f.write_str("Click ")?;
                quoted(f, l.as_str())
            }
            Step::ClickMouse(l) => {
                f.write_str("Click Mouse ")?;
                quoted(f, l.as_str())
            }
            Step::Expect(e) => match e {
                Expectation::Shows(l) => {
                    f.write_str("Expect ")?;
                    quoted(f, l.as_str())
                }
                Expectation::ShowsNot(l) => {
                    f.write_str("Expect Not ")?;
                    quoted(f, l.as_str())
                }
                Expectation::At(n) => write!(f, "Expect At {}", n.as_str()),
                Expectation::Saved => f.write_str("Expect Saved"),
                Expectation::Unsaved => f.write_str("Expect Unsaved"),
                Expectation::Exited(n) => write!(f, "Expect Exited {n}"),
            },
            Step::Sleep(p) => match p.millis() {
                ms if ms % 1000 == 0 => write!(f, "Sleep {}s", ms / 1000),
                ms => write!(f, "Sleep {ms}ms"),
            },
            Step::Screenshot(n) => write!(f, "Screenshot {}", n.as_str()),
            Step::SetLive => f.write_str("Set Live"),
            Step::Comment(c) => write!(f, "#{}", c.as_str()),
            Step::Blank => Ok(()),
        }
    }
}

impl fmt::Display for Macro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for step in self.steps() {
            writeln!(f, "{step}")?;
        }
        Ok(())
    }
}
