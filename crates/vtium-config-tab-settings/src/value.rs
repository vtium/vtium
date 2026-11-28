//! Changing a key's value: the allowed values a key offers in the list
//! box, the value typed where the key takes one, and the one change that
//! either makes. A change sets the key's top-level line through the edit
//! that `vtium config set` makes, so the shell's one change function
//! records it as one step that Undo takes back.
//!
//! References:
//! - ADR-1403 D11, "The list box": a line with choices opens them under
//!   itself, and keeping a pick is one change.
//! - ADR-1206 D8: the list box takes the line's choices through the
//!   choices trait.
//! - ADR-1005 D10: an empty key binding binds no key, and its default is
//!   its own combinations.

use std::borrow::Cow;

use vtium_config_core::commands::{set_edit, value_text};
use vtium_config_core::document::{self, ConfEdit, Edit};
use vtium_config_core::ui::{Change, Command, Message};
use vtium_config_widgets::tab::Choices;
use vtium_settings::keys::{Default as KeyDefault, Key, Kind};

use crate::words;

/// What keeping a choice of the list box does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Sets the key to this value, or leaves it at its default with
    /// `None`.
    Set(Option<String>),
    /// Lets the person type a name that the list does not hold.
    Other,
}

/// One choice of the list box: the word it writes, `None` for the choice
/// that leaves the key unset, and whether it is the key's default.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Choice {
    word: Option<&'static str>,
    default: bool,
}

/// The allowed values of one key, which the list box under its line shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Values {
    key: &'static Key,
    choices: Vec<Choice>,
    /// Whether the last choice is Other…, for a name the list does not
    /// hold.
    other: bool,
    current: Option<usize>,
}

impl Values {
    /// The values `key` takes from a fixed set, with the value `text`,
    /// vtium.conf, gives it now: `on` and `off` for a switch, a choice's
    /// words, and a name's words and Other…. A key whose default is no
    /// value offers `unset` first, which takes the key's line out. `None`
    /// for a key whose value is typed.
    pub fn of(key: &'static Key, text: &str) -> Option<Values> {
        let fixed: &[&'static str] = match key.kind {
            Kind::Switch => &["on", "off"],
            Kind::Choice(words) | Kind::Name(words) => words,
            _ => return None,
        };
        let (now, set) = value_text(key, Some(text));
        let default = value_text(key, None).0;
        let unset = key.default == KeyDefault::Unset;
        let mut choices: Vec<Choice> = Vec::new();
        if unset {
            choices.push(Choice {
                word: None,
                default: true,
            });
        }
        choices.extend(fixed.iter().map(|w| Choice {
            word: Some(*w),
            default: !unset && *w == default,
        }));
        let current = if !set && unset {
            Some(0)
        } else {
            choices.iter().position(|c| c.word == Some(now.as_str()))
        };
        Some(Values {
            key,
            choices,
            other: matches!(key.kind, Kind::Name(_)),
            current,
        })
    }
}

impl Values {
    /// The key whose values these are.
    pub fn key(&self) -> &'static Key {
        self.key
    }
}

impl Choices<Pick> for Values {
    fn count(&self) -> usize {
        self.choices.len().saturating_add(usize::from(self.other))
    }

    fn text(&self, choice: usize) -> Cow<'_, str> {
        match self.choices.get(choice) {
            Some(Choice { word, default }) => {
                let word = word.unwrap_or(words::UNSET);
                if *default {
                    Cow::Owned(words::default_choice(word))
                } else {
                    Cow::Borrowed(word)
                }
            }
            None => Cow::Borrowed(words::OTHER),
        }
    }

    fn current(&self) -> Option<usize> {
        self.current
    }

    fn keep(&self, choice: usize) -> Pick {
        match self.choices.get(choice) {
            Some(c) => Pick::Set(c.word.map(str::to_owned)),
            None => Pick::Other,
        }
    }
}

/// What typing starts from for `key`, whose value `text` gives it now: an
/// empty entry for a key left at its default, so Enter alone keeps the
/// default, and the value itself otherwise. A key binding starts from its
/// combinations, its default's included, so Enter alone never empties it.
pub fn typing_starts(key: &'static Key, text: &str) -> String {
    let (now, set) = value_text(key, Some(text));
    if set || key.kind == Kind::KeyBinding {
        now
    } else {
        String::new()
    }
}

/// The value that `typed` gives `key`: `None` for its default. An empty
/// entry leaves a key at its default, except for a key binding, where it
/// binds no key; the word `default`, or the default's own combinations,
/// leave a key binding at its default.
pub fn typed_value(key: &'static Key, typed: &str) -> Option<String> {
    let typed = typed.trim();
    if key.kind == Kind::KeyBinding {
        let default = value_text(key, None).0;
        (typed != words::BINDING_DEFAULT && typed != default).then(|| typed.to_owned())
    } else {
        (!typed.is_empty()).then(|| typed.to_owned())
    }
}

/// Why vtium.conf cannot hold `value` for `key`, as `vtium config set`
/// says it, or `None` where it can.
pub fn refusal(key: &'static Key, value: Option<&str>) -> Option<String> {
    let value = value?;
    set_edit(key, value).err()
}

/// The one change that sets `key` to `value` in the open configuration,
/// or leaves it at its default with `None`.
pub fn set(key: &'static Key, value: Option<String>) -> Command {
    let name = words::change_name(key.name, value.as_deref());
    Command::Change(Change::Document(document::Change::new(
        name,
        Edit::Conf(vec![ConfEdit::Top {
            key: key.name,
            value,
        }]),
    )))
}

/// The message that says why a typed value was refused, which keeps the
/// entry open for the person to fix it.
pub fn say(line: String) -> Command {
    Command::Change(Change::Say(Message { line, more: None }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_settings::keys;

    #[test]
    fn a_switch_offers_on_and_off_with_its_default_marked() -> Result<(), String> {
        let v = Values::of(&keys::ALLOW_FONT_OPS, "").ok_or("a switch has values")?;
        let texts: Vec<String> = (0..v.count()).map(|c| v.text(c).into_owned()).collect();
        assert_eq!(texts, ["on (default)", "off"]);
        assert_eq!(v.current(), Some(0));
        assert_eq!(v.keep(1), Pick::Set(Some("off".to_owned())));
        Ok(())
    }

    #[test]
    fn a_choice_left_unset_offers_unset_first() -> Result<(), String> {
        let v = Values::of(&keys::GLYPH_SOURCE, "glyph-source = auto\n").ok_or("a choice")?;
        let texts: Vec<String> = (0..v.count()).map(|c| v.text(c).into_owned()).collect();
        assert_eq!(texts, ["unset (default)", "font", "sprites", "auto"]);
        assert_eq!(v.current(), Some(3));
        assert_eq!(v.keep(0), Pick::Set(None));
        Ok(())
    }

    #[test]
    fn a_typed_key_has_no_list() {
        assert_eq!(Values::of(&keys::FONT_NAME, ""), None);
    }

    #[test]
    fn an_empty_entry_is_the_default_except_for_a_key_binding() -> Result<(), String> {
        assert_eq!(typed_value(&keys::FONT_NAME, "  "), None);
        assert_eq!(
            typed_value(&keys::FONT_NAME, " proof "),
            Some("proof".to_owned())
        );
        let binding = keys::KEYS
            .iter()
            .find(|k| k.kind == Kind::KeyBinding)
            .ok_or("a key binding")?;
        assert_eq!(typed_value(binding, ""), Some(String::new()));
        assert_eq!(typed_value(binding, "default"), None);
        Ok(())
    }
}
