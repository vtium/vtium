//! The document's base, the installed file the draft was made from or
//! saved as, and the installed file as it is now. [`disk`] tells Save's
//! review whether the installed file changed on disk since the base, and
//! what the person can do about it. Where no change of the person's meets
//! the installed file's, [`take`] brings the installed file in as one
//! change, which passes the one change function as one step of the undo
//! history, and the step keeps the base from before it.
//!
//! References:
//! - ADR-1403 D8, "When the installed file changed on disk": what counts as a change on disk, the choices, and the base each choice records.
//! - ADR-1403 D8, "vtium-config's store: drafts and the library": the base's text beside its hash.

use crate::document::{Base, Change, Content, Edit, Version};
use crate::store::hash;

use super::diff::{Diff, diff};
use super::merge::{Merge, merge};
use super::words;

/// How the installed file stands against the document's base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disk {
    /// The installed file is the base.
    Same,
    /// No vtium.conf is installed, so saving overwrites nothing: no
    /// conflict, whatever the base was.
    Missing,
    /// The installed file changed, and no change of the person's meets
    /// that change: the document's vtium.conf is still the base's text, or
    /// already the installed file's. [`take`] brings the installed file in
    /// without a choice.
    Taken,
    /// The installed file changed, and so did the document.
    Changed(Changed),
}

/// An installed file changed on disk while the document changed too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    /// The base's text, `None` when vtium-config has only its hash.
    pub base: Option<String>,
    /// The document's vtium.conf.
    pub mine: String,
    /// The installed vtium.conf now.
    pub installed: String,
}

impl Changed {
    /// The installed file's own change: its difference from the base.
    pub fn installed_change(&self) -> Option<Diff> {
        Some(diff(self.base.as_deref()?, &self.installed))
    }

    /// The person's change: the document's difference from the base.
    pub fn my_change(&self) -> Option<Diff> {
        Some(diff(self.base.as_deref()?, &self.mine))
    }

    /// The installed file's change merged into the document, three ways,
    /// or why Update cannot run, which greys it out.
    pub fn merge(&self) -> Result<Merge, &'static str> {
        let base = self.base.as_deref().ok_or(words::NO_BASE_TEXT)?;
        Ok(merge(base, &self.mine, &self.installed))
    }
}

/// How `installed`, the installed vtium.conf now or `None`, stands against
/// the base of the document that holds `content`.
pub fn disk(content: &Content, installed: Option<&str>) -> Disk {
    let Some(installed) = installed else {
        return Disk::Missing;
    };
    let now = hash(installed.as_bytes());
    if content.base.hash.as_deref() == Some(now.as_str()) {
        return Disk::Same;
    }
    let text = &content.text;
    let unchanged = match &content.base.hash {
        Some(base) => hash(text.as_bytes()) == *base,
        // a draft made when no file was installed starts as the header
        None => text == crate::drafts::HEADER,
    };
    if unchanged || text == installed {
        return Disk::Taken;
    }
    Disk::Changed(Changed {
        base: base_text(&content.base),
        mine: text.clone(),
        installed: installed.to_owned(),
    })
}

/// The base's text when it is the base's: the text the document keeps,
/// or none for a base of no installed file.
fn base_text(base: &Base) -> Option<String> {
    match &base.hash {
        None => Some(String::new()),
        Some(h) => base.text.clone().filter(|t| hash(t.as_bytes()) == *h),
    }
}

/// `base` with its text, when it has only its hash and a version of the
/// installed history has that hash: a draft made before vtium-config kept
/// the base's text finds it there.
pub fn recover(base: Base, versions: &[Version]) -> Base {
    if base.text.is_some() {
        return base;
    }
    let Some(h) = base.hash.as_deref() else {
        return base;
    };
    let text = versions
        .iter()
        .find(|v| hash(v.text.as_bytes()) == h)
        .map(|v| v.text.clone());
    Base { text, ..base }
}

/// The installed file brought in where no change of the person's meets
/// its change ([`Disk::Taken`]): the installed text, and the installed
/// file as the base, in one step.
pub fn take(installed: &str) -> Change {
    Change::new(
        words::TAKEN,
        Edit::All(vec![
            Edit::Text(installed.to_owned()),
            Edit::Base(Base::of(Some(installed))),
        ]),
    )
}
