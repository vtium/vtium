//! Save's screen when the installed file changed on disk while the person
//! worked: the installed file's own change, its difference from the base,
//! then a line that says whether it touches the person's lines, then the
//! person's change, then the three choices. `[u]` Update merges the change
//! into the document, `[o]` Overwrite saves the document over it, `[r]`
//! Revert asks, then starts again from the file on disk, and Esc changes
//! nothing. Where vtium-config does not have the base's text, Update is
//! greyed out with the reason, and Overwrite and Revert remain.
//!
//! References:
//! - ADR-1403 D8, "When the installed file changed on disk": the screen's
//!   parts, the choices and their keys, and Update greyed out for a draft
//!   whose base's text is lost.

use vtium_config_core::document::{Base, Version};
use vtium_config_core::versions::base::{self, Changed as Disk};
use vtium_config_core::versions::merge::Merge;
use vtium_config_widgets::roles::Role;

/// The screen for a file changed on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    disk: Disk,
}

impl Changed {
    /// The screen for `disk`, the document's change and the change on
    /// disk. A document that has only its base's hash, `base`, finds the
    /// base's text in `versions`, the installed history, where it can.
    pub fn new(mut disk: Disk, base: &Base, versions: &[Version]) -> Changed {
        if disk.base.is_none() {
            disk.base = base::recover(base.clone(), versions).text;
        }
        Changed { disk }
    }

    /// The installed vtium.conf now.
    pub fn installed(&self) -> &str {
        &self.disk.installed
    }

    /// The merge Update makes, or why it cannot.
    pub fn merge(&self) -> Result<Merge, &'static str> {
        self.disk.merge()
    }

    /// Why Update is greyed out, where it is.
    pub fn cannot_merge(&self) -> Option<&'static str> {
        self.merge().err()
    }

    /// The screen's lines.
    pub fn lines(&self) -> Vec<(String, Role)> {
        let mut out = vec![(
            " The file changed on disk since you opened it. Its change:".to_owned(),
            Role::Warning,
        )];
        match self.disk.installed_change() {
            Some(d) => super::hunks(&d, &mut out),
            None => out.push((
                " vtium-config does not have the file yours started from, so it cannot show that change."
                    .to_owned(),
                Role::Dim,
            )),
        }
        let touch = match self.merge() {
            Ok(m) => match m.settled().0 {
                0 => " Your changes do not touch those lines.".to_owned(),
                1 => " Your changes and its change touch the same lines in 1 place.".to_owned(),
                n => format!(" Your changes and its change touch the same lines in {n} places."),
            },
            Err(_) => String::new(),
        };
        if !touch.is_empty() {
            out.push((touch, Role::Row));
        }
        out.push((String::new(), Role::Row));
        out.push((" Your change:".to_owned(), Role::Heading));
        if let Some(d) = self.disk.my_change() {
            super::hunks(&d, &mut out);
        }
        out.push((String::new(), Role::Row));
        match self.cannot_merge() {
            None => out.push((
                "   [u] Update      bring the file's change into yours, then review again"
                    .to_owned(),
                Role::Row,
            )),
            Some(why) => out.push((format!("   Update is greyed out: {why}"), Role::Dim)),
        }
        out.push((
            "   [o] Overwrite   save yours; the version they replace stays in History".to_owned(),
            Role::Row,
        ));
        out.push((
            "   [r] Revert      reload the file from disk and drop your changes".to_owned(),
            Role::Row,
        ));
        out
    }

    /// The document's change and the change on disk, for settling.
    pub fn disk(&self) -> &Disk {
        &self.disk
    }
}
