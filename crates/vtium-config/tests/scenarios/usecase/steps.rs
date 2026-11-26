//! The steps of a use case, in the person's words, and how the runner
//! follows each one through what vtium-config shows.
//!
//! A step names what the person sees: a tab by its name in the tab bar, an
//! action by its label on the key line, a line of a list or a card by its
//! text. The runner finds that label on the screen and presses the keys a
//! person would press to reach it. A step naming a label that vtium-config
//! does not show fails the case, so a case cannot pass on keys that no
//! longer mean what its words say.
//!
//! The highlighted line is the one vtium-config draws in reverse video,
//! the TUI's selection mark, on any row below the tab bar. The Flow view's
//! document marks its highlighted box with a heavy border instead, so
//! where no row is reversed the runner reads the box drawn in heavy lines.

use vtium_config::tui::canvas::Row;
use vtium_config::tui::input::Key;
use vtium_config::tui::model::View;

use super::Run;

/// One step as the case file words it.
#[derive(Debug, Clone)]
pub enum Step {
    /// "Start on the Looks tab." or "Go to the Looks tab."
    Tab(String),
    /// "Choose New look.": an action of the key line, else a line of the
    /// screen, which the runner highlights and opens with Enter.
    Choose(String),
    /// "Highlight mono.": the runner moves the highlight to the line and
    /// presses nothing more.
    Highlight(String),
    /// "Set Color scheme to vga.": the runner highlights the line, then
    /// steps its value with Right until the line shows the value.
    Set(String, String),
    /// "Erase the name.": Backspace until the text being typed is empty.
    Erase,
    /// "Type big."
    Type(String),
    /// "Press y to save and quit." or "Press y for yes.": the screen must
    /// offer the key with that label. "Press Esc." presses the key alone.
    Press { key: String, label: Option<String> },
    /// "Wait 5 seconds.": ticks of the live trial's countdown.
    Wait(usize),
    /// "Restart vtium."
    Restart,
    /// "Run `vtium-config install`.": the words after `vtium-config`.
    Command(Vec<String>),
}

/// The step a sentence says, without its number and final period.
pub fn parse(sentence: &str) -> Result<Step, String> {
    let s = sentence
        .strip_suffix('.')
        .ok_or("a step ends with a period")?;
    let unquote = |t: &str| {
        t.strip_prefix('"')
            .and_then(|t| t.strip_suffix('"'))
            .unwrap_or(t)
            .to_owned()
    };
    let tab = |t: &str| {
        t.strip_prefix("the ")?
            .strip_suffix(" tab")
            .map(str::to_owned)
    };
    if let Some(t) = s
        .strip_prefix("Start on ")
        .or_else(|| s.strip_prefix("Go to "))
    {
        return tab(t)
            .map(Step::Tab)
            .ok_or_else(|| "a tab reads: the NAME tab".to_owned());
    }
    if let Some(rest) = s.strip_prefix("Set ") {
        let (label, value) = rest.split_once(" to ").ok_or("Set LABEL to VALUE")?;
        return Ok(Step::Set(label.to_owned(), unquote(value)));
    }
    if let Some(rest) = s.strip_prefix("Press ") {
        let (key, label) = match rest.split_once(" to ").or_else(|| rest.split_once(" for ")) {
            Some((k, l)) => (k, Some(l.to_owned())),
            None => (rest, None),
        };
        return Ok(Step::Press {
            key: key.to_owned(),
            label,
        });
    }
    if let Some(rest) = s.strip_prefix("Wait ") {
        let n = rest
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .ok_or("Wait N seconds")?;
        return Ok(Step::Wait(n));
    }
    if let Some(rest) = s.strip_prefix("Run `vtium-config") {
        let words = rest.strip_suffix('`').ok_or("Run `vtium-config WORDS`")?;
        return Ok(Step::Command(
            words.split_whitespace().map(str::to_owned).collect(),
        ));
    }
    Ok(match s {
        "Erase the name" | "Erase the text" => Step::Erase,
        "Restart vtium" => Step::Restart,
        _ => {
            if let Some(l) = s.strip_prefix("Choose ") {
                Step::Choose(unquote(l))
            } else if let Some(l) = s.strip_prefix("Highlight ") {
                Step::Highlight(unquote(l))
            } else if let Some(t) = s.strip_prefix("Type ") {
                Step::Type(unquote(t))
            } else {
                return Err(format!("an unknown step: {sentence}"));
            }
        }
    })
}

/// Whether `text` begins with `label` as whole words, ignoring case and
/// the spaces around and between words.
fn begins(text: &str, label: &str) -> bool {
    begins_before(text, label, &[' ', ',', '.', ':', '…'])
}

/// Whether `text` begins with `label`, and what follows the label is
/// nothing or one of `ends`.
fn begins_before(text: &str, label: &str, ends: &[char]) -> bool {
    let flat = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let (text, label) = (flat(text), flat(label));
    !label.is_empty()
        && text.starts_with(&label)
        && text[label.len()..]
            .chars()
            .next()
            .is_none_or(|c| ends.contains(&c))
}

/// The highlighted line: where it is, and its texts.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mark {
    row: usize,
    /// The reversed text, or the heavy box's lines.
    texts: Vec<String>,
    /// Whether the mark is the Flow document's heavy box, which Right and
    /// Left move through, where a list's line moves with Down and Up.
    boxed: bool,
}

impl Mark {
    fn is(&self, label: &str) -> bool {
        self.texts.iter().any(|t| begins(t.trim(), label))
    }
}

/// A key-line item, `KEY label`, split at its first space.
fn item(item: &str) -> Option<(&str, &str)> {
    let (key, label) = item.trim().split_once(' ')?;
    Some((key, label.trim()))
}

impl Run<'_> {
    fn rows(&self) -> Vec<Row> {
        vtium_config::tui::view::screen(&self.app)
    }

    /// The highlighted line, read from the screen.
    fn mark(&self) -> Option<Mark> {
        let rows = self.rows();
        let last = rows.len().saturating_sub(1);
        for (y, row) in rows.iter().enumerate().take(last).skip(1) {
            let reversed: String = row
                .iter()
                .filter(|s| s.style.reverse)
                .map(|s| s.text.as_str())
                .collect();
            if !reversed.trim().is_empty() {
                let whole = vtium_config::tui::view::text(row);
                return Some(Mark {
                    row: y,
                    texts: vec![reversed, whole],
                    boxed: false,
                });
            }
        }
        // the Flow document's box under the cursor, in heavy lines
        let text: Vec<Vec<char>> = rows
            .iter()
            .map(|r| vtium_config::tui::view::text(r).chars().collect())
            .collect();
        for (y, line) in text.iter().enumerate().skip(1) {
            let Some(left) = line.iter().position(|c| *c == '┏') else {
                continue;
            };
            let right = line.iter().skip(left).position(|c| *c == '┓')? + left;
            let mut texts = Vec::new();
            for inside in text.iter().skip(y + 1) {
                if inside.get(left) == Some(&'┗') {
                    break;
                }
                let t: String = inside
                    .get(left + 1..right.min(inside.len()))
                    .unwrap_or_default()
                    .iter()
                    .collect();
                texts.push(t);
            }
            return Some(Mark {
                row: y,
                texts,
                boxed: true,
            });
        }
        None
    }

    /// The key-line items of what is open now: every page of the line,
    /// so an action on a later page counts as offered.
    fn key_items(&self) -> Vec<String> {
        let mut pages: Vec<&str> =
            vtium_config::tui::contract::key_pages(vtium_config::tui::nav::node(&self.app))
                .map(<[&str]>::to_vec)
                .unwrap_or_default();
        pages.push(vtium_config::tui::layout::key_line(&self.app));
        pages
            .iter()
            .flat_map(|p| p.split("  "))
            .filter(|i| !i.trim().is_empty())
            .map(|i| i.trim().to_owned())
            .collect()
    }

    fn key(&mut self, name: &str) -> Result<(), String> {
        let k = super::key(name)?;
        self.press(k);
        Ok(())
    }

    /// Moves the highlight to the line that begins with `label`.
    fn highlight(&mut self, label: &str) -> Result<(), String> {
        let Some(start) = self.mark() else {
            return Err("nothing is highlighted on the screen".to_owned());
        };
        if start.is(label) {
            return Ok(());
        }
        let (forward, back) = if start.boxed {
            (Key::Right, Key::Left)
        } else {
            (Key::Down, Key::Up)
        };
        // toward the label where the screen shows it, else down first
        let screen: Vec<String> = self
            .rows()
            .iter()
            .map(vtium_config::tui::view::text)
            .collect();
        let below = screen
            .iter()
            .enumerate()
            .any(|(y, t)| y > start.row && t.split("   ").any(|part| begins(part.trim(), label)));
        let above = screen
            .iter()
            .enumerate()
            .any(|(y, t)| y < start.row && t.split("   ").any(|part| begins(part.trim(), label)));
        let order = if above && !below {
            [back, forward]
        } else {
            [forward, back]
        };
        // the first way stops at a highlight it has seen; the other way
        // walks back over those, so it stops only where the highlight
        // no longer moves
        let mut seen = vec![start];
        for (way, dir) in order.into_iter().enumerate() {
            let mut last = None;
            for _ in 0..200 {
                self.press(dir);
                let Some(now) = self.mark() else {
                    return Err(format!(
                        "the highlight left the screen looking for {label:?}"
                    ));
                };
                if now.is(label) {
                    return Ok(());
                }
                if (way == 0 && seen.contains(&now)) || last.as_ref() == Some(&now) {
                    break;
                }
                seen.push(now.clone());
                last = Some(now);
            }
        }
        Err(format!(
            "no line on the screen begins with {label:?}:\n{}",
            screen.join("\n")
        ))
    }

    /// Follows `step`, or says why the screen does not offer it.
    pub fn follow(&mut self, step: &Step) -> Result<(), String> {
        match step {
            Step::Tab(name) => {
                let found = View::TABS
                    .iter()
                    .position(|v| self.app.tab_of(*v).name().eq_ignore_ascii_case(name));
                let shown = vtium_config::tui::view::text(&self.rows()[0]);
                let Some(i) = found.filter(|_| {
                    shown
                        .split_whitespace()
                        .any(|w| w.eq_ignore_ascii_case(name))
                }) else {
                    return Err(format!("the tab bar shows no tab {name:?}: {shown}"));
                };
                self.key(&(i + 1).to_string())?;
                if self.app.nav.view.tab() != View::TABS[i] {
                    return Err(format!("the {name} tab did not open"));
                }
                Ok(())
            }
            Step::Choose(label) => {
                let items = self.key_items();
                let exact = items
                    .iter()
                    .filter_map(|i| item(i))
                    .find(|(_, l)| l.eq_ignore_ascii_case(label));
                // a key-line label that the step's words begin with, such
                // as "new" for "New look"
                let prefix = || {
                    items
                        .iter()
                        .filter_map(|i| item(i))
                        .find(|(_, l)| l.len() > 1 && begins(label, l))
                };
                if let Some((key, _)) = exact.or_else(prefix) {
                    let key = key.to_owned();
                    return self.key(&key);
                }
                self.highlight(label)?;
                self.press(Key::Enter);
                Ok(())
            }
            Step::Highlight(label) => self.highlight(label),
            Step::Set(label, value) => {
                self.highlight(label)?;
                // the value under the highlight: the reversed text without
                // the line's label and without a choice's ● or ○ mark
                let flat = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
                // the text after the label, where a text begins with it
                let after = |t: &str| {
                    let t = flat(t);
                    begins(&t, label).then(|| t[label.len().min(t.len())..].trim().to_owned())
                };
                let under = |m: &Mark| {
                    let t = match after(&m.texts[0]) {
                        // only the label is reversed: the value is the
                        // rest of the line
                        Some(rest) if rest.is_empty() => {
                            after(m.texts[1].trim()).unwrap_or_default()
                        }
                        Some(rest) => rest,
                        None => flat(&m.texts[0]),
                    };
                    t.trim_start_matches(['●', '○']).trim().to_owned()
                };
                let mut seen = Vec::new();
                for _ in 0..64 {
                    let Some(now) = self.mark() else { break };
                    // "mono, from vtium.conf" is the value the look takes
                    // from vtium.conf, not one the look sets
                    if begins_before(&under(&now), value, &[' ', '.']) {
                        // a line of choices marks the one it holds with ●,
                        // and Enter sets the one under the highlight
                        if flat(&now.texts[1]).contains(&format!("○ {value}")) {
                            self.press(Key::Enter);
                        }
                        return Ok(());
                    }
                    if seen.contains(&now) {
                        break;
                    }
                    seen.push(now);
                    self.press(Key::Right);
                }
                Err(format!(
                    "Right never steps {label} to {value:?}; the highlight read {:?}",
                    seen.iter()
                        .map(|m: &Mark| m.texts[0].clone())
                        .collect::<Vec<_>>()
                ))
            }
            Step::Erase => {
                for _ in 0..128 {
                    let before = self.screen();
                    self.press(Key::Backspace);
                    if self.screen() == before {
                        return Ok(());
                    }
                }
                Err("Backspace never emptied the text".to_owned())
            }
            Step::Type(text) => {
                for c in text.chars() {
                    self.press(Key::Char(c));
                }
                Ok(())
            }
            Step::Press { key, label } => {
                if let Some(label) = label {
                    let wanted = format!("{key} {label}").to_lowercase();
                    let flat = |s: &str| {
                        s.split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .to_lowercase()
                    };
                    let offered = self.key_items().iter().any(|i| flat(i) == wanted)
                        || flat(&self.screen()).contains(&wanted);
                    if !offered {
                        return Err(format!(
                            "the screen never offers {key} to {label}:\n{}",
                            self.screen()
                        ));
                    }
                }
                self.key(key)
            }
            Step::Wait(n) => {
                (0..*n).for_each(|_| self.tick());
                Ok(())
            }
            Step::Restart => {
                self.console.restart();
                Ok(())
            }
            Step::Command(words) => {
                let said = self.machine.command(words);
                self.said.extend(said.into_bytes());
                Ok(())
            }
        }
    }
}
