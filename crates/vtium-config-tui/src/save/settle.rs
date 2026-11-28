//! Settling the lines both sides changed, after Update found some: one
//! place at a time, the installed file's lines above the person's. `[m]`
//! keeps the person's lines and `[i]` the installed file's, and the next
//! place follows. Once the last is settled, the merge is one change of
//! the document and Save's review shows it. Esc cancels the update and
//! leaves the document as it was.
//!
//! References:
//! - ADR-1403 D8, "When the installed file changed on disk": the lines
//!   both sides changed, listed one by one, `[m]` and `[i]`, Save once all
//!   are settled, Esc cancelling, and the merge as one step.

use vtium_config_core::document::Change;
use vtium_config_core::versions::merge::{self, Merge, Side};
use vtium_config_widgets::roles::Role;

use super::changed::Changed;

/// The places both sides changed, and how many are settled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settle {
    merge: Merge,
    /// The screen Esc goes back to.
    from: Changed,
}

impl Settle {
    /// Settling `merge`, which Update made on the screen `from`.
    pub fn new(merge: Merge, from: Changed) -> Settle {
        Settle { merge, from }
    }

    /// Settles the place on screen with `side`'s lines.
    pub fn settle(&mut self, side: Side) {
        if let Some(n) = self.merge.next() {
            self.merge.settle(n, side);
        }
    }

    /// Update's change, once every place is settled.
    pub fn update(&self) -> Option<Change> {
        merge::update(&self.merge, self.from.installed())
    }

    /// The screen for the file changed on disk again, as Esc leaves it.
    pub fn back(&self) -> Changed {
        self.from.clone()
    }

    /// The screen's lines: how many are settled, then the place on screen
    /// with both sides' lines.
    pub fn lines(&self) -> Vec<(String, Role)> {
        let (n, settled) = self.merge.settled();
        let places = match n {
            1 => "1 place".to_owned(),
            n => format!("{n} places"),
        };
        let mut out = vec![
            (
                format!(
                    " Both sides changed the same lines in {places}: {settled} of {n} settled."
                ),
                Role::Warning,
            ),
            (String::new(), Role::Row),
        ];
        let place = self.merge.next().and_then(|i| self.merge.both().nth(i));
        if let Some(both) = place {
            let side = |out: &mut Vec<(String, Role)>, who: &str, at: usize, text: &str, role| {
                if text.is_empty() {
                    out.push((format!(" {who:<9} {at:>5}  (no lines)"), role));
                }
                for (i, line) in text.lines().enumerate() {
                    out.push((
                        format!(" {who:<9} {:>5}  {line}", at.saturating_add(i)),
                        role,
                    ));
                }
            };
            side(
                &mut out,
                "installed",
                both.installed_line,
                &both.installed,
                Role::DiffRemoved,
            );
            side(
                &mut out,
                "mine",
                both.mine_line,
                &both.mine,
                Role::DiffAdded,
            );
        }
        out.push((String::new(), Role::Row));
        out.push((
            " [m] keeps mine and [i] takes the installed lines, one place at a time.".to_owned(),
            Role::Dim,
        ));
        out.push((
            " Save appears once every place is settled. [Esc] cancels the update.".to_owned(),
            Role::Dim,
        ));
        out
    }
}
