//! The Flow tab: the configuration drawn as paths of parts, the file view
//! over the draft, and Restore's list of the installed history. Its state
//! is where the highlight is, what is open over the document, the path
//! being made in NEW PATH, and the item of the flow that the file view
//! marks.
//!
//! References:
//! - ADR-1403 D7: the Flow view, its layers and its keys.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::desk::Kept;
use super::super::flow::{Flow as Wiring, Item};
use super::super::flowview::{self, Obj, Slot};
use super::super::model::Pager;

/// Where the Flow tab stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    /// Where the highlight is on the document and what is open over it.
    pub(crate) state: flowview::State,
    /// The selected item of the flow, which the file view marks and a
    /// rebuild of the flow carries to the item that stands in its place.
    pub(crate) at: Item,
    /// The read-only file view over the draft, when it is open.
    pub(crate) pager: Option<Pager>,
    /// The installed history Restore lists, and the entry highlighted.
    pub(crate) restore: Option<(Vec<Kept>, usize)>,
    /// The item to select once the desk answers the change in flight.
    pub(crate) select_after: Option<Item>,
}

impl Flow {
    /// The tab at its start on `flow`, with its first item selected.
    pub fn new(flow: &Wiring) -> Flow {
        Flow {
            state: flowview::State::default(),
            at: first_item(flow),
            pager: None,
            restore: None,
            select_after: None,
        }
    }

    /// Where the highlight is on the document and what is open over it.
    pub fn state(&self) -> &flowview::State {
        &self.state
    }

    /// The selected item of the flow.
    pub fn at(&self) -> Item {
        self.at
    }

    /// The file view's place in the draft, when the file view is open.
    pub fn pager(&self) -> Option<Pager> {
        self.pager
    }

    /// Restore's list of the installed history and its highlighted entry,
    /// when the list is open.
    pub fn restore(&self) -> Option<&(Vec<Kept>, usize)> {
        self.restore.as_ref()
    }

    /// The node of the interface tree the tab is at: a value being typed,
    /// the file view, Restore's list, a filter's picker, a layer open over
    /// the document, or the document itself.
    pub fn node(&self) -> &'static str {
        let flow = &self.state;
        if flow.typing.is_some() && flow.picker.is_some() {
            return "flow.maker.picker.typing";
        }
        if flow.typing.is_some() {
            return match flow.layer {
                Some(Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal)) => "flow.card.typing",
                _ => "flow.detail.typing",
            };
        }
        if self.pager.is_some() {
            return "flow.file";
        }
        if self.restore.is_some() {
            return "flow.restore";
        }
        if flow.picker.is_some() {
            return "flow.maker.picker";
        }
        match &flow.layer {
            None => "flow",
            Some(Obj::Doc(_, Slot::Terminal)) => "flow.card",
            Some(Obj::New(Slot::Terminal)) => "flow.new-card",
            Some(Obj::Pal(Slot::Template(_)) | Obj::New(Slot::Empty(_))) => "flow.maker",
            Some(Obj::Others) => "flow.others",
            Some(Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_)) => "flow.info",
            // a part's detail: its first line is the label, and the
            // lines below it are the choices
            Some(_) if flow.line == 0 => "flow.detail",
            Some(_) => "flow.detail.choices",
        }
    }
}

/// The item the Flow view selects first: the first display, else the
/// first item of the first column that has one.
pub(crate) fn first_item(flow: &Wiring) -> Item {
    flow.columns()
        .iter()
        .find_map(|c| {
            c.iter()
                .copied()
                .find(|i| *i != Item::Base && !matches!(i, Item::New(_)))
        })
        .unwrap_or(Item::Base)
}
