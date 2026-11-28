//! The shared state: what every tab reads and no tab changes itself. A tab
//! asks for a change with a [`super::Command`], and update makes it here.
//!
//! References:
//! - ADR-1206 D2: the model's four parts, and what `shared` holds: the document and its changes not saved, the live console's state, the queue of askers, the pictures, the pending installs and the message line.
//! - ADR-1206 D3, "The undo history lives in `shared`": one history for each document open while vtium-config runs.

use std::collections::{BTreeSet, VecDeque};

use crate::document::{self, Documents};

/// What every tab reads.
#[derive(Debug)]
pub struct Shared {
    /// The open document, its undo history, the histories of the
    /// documents opened before it, and the desk its draft is written
    /// through.
    pub documents: Documents,
    /// The live console's state.
    pub console: Console,
    /// The queries sent to the terminal and not yet answered, each with
    /// who asked it.
    pub askers: Askers,
    /// The pictures of the console the font helper draws.
    pub pictures: Pictures,
    /// The installs under way or waiting.
    pub installs: Installs,
    /// The line for the person on the viewer's last row.
    pub message: Option<Message>,
    /// The devices present, which the Flow tab places on the document.
    pub present: crate::flowmodel::Present,
    /// The fonts, effects, color schemes and pointer themes found on the
    /// machine, read at start and again on Read devices again.
    pub found: crate::found::Found,
}

impl Shared {
    /// The shared state at start: `documents`, with the devices `present`,
    /// and the terminal not yet asked what it is.
    pub fn new(documents: Documents, present: crate::flowmodel::Present) -> Shared {
        Shared {
            documents,
            console: Console::default(),
            askers: Askers::default(),
            pictures: Pictures::default(),
            installs: Installs::default(),
            message: None,
            present,
            found: crate::found::Found::default(),
        }
    }

    /// The open document.
    pub fn document(&self) -> &document::Document {
        self.documents.document()
    }

    /// Makes `change` on the open document through the one change
    /// function. A refused change leaves the document as it was, and its
    /// reason goes on the message line too.
    pub fn change(&mut self, change: document::Change) -> Result<document::Done, String> {
        let done = self.documents.change(change);
        if let Err(why) = &done {
            self.say(why.clone());
        }
        done
    }

    /// Ctrl+Z: takes the last change back and says what it undid, or that
    /// there is nothing to undo.
    pub fn undo(&mut self) {
        let said = match self.documents.undo() {
            Ok(Some(step)) => document::words::undid(&step),
            Ok(None) => document::words::NOTHING_TO_UNDO.to_owned(),
            Err(why) => why,
        };
        self.say(said);
    }

    /// Ctrl+Y: makes again the change the last undo took back and says
    /// what it redid, or that there is nothing to redo.
    pub fn redo(&mut self) {
        let said = match self.documents.redo() {
            Ok(Some(step)) => document::words::redid(&step),
            Ok(None) => document::words::NOTHING_TO_REDO.to_owned(),
            Err(why) => why,
        };
        self.say(said);
    }

    /// Puts `line` on the message line, with nothing behind it.
    pub fn say(&mut self, line: String) {
        self.message = Some(Message { line, more: None });
    }
}

/// A line on the message line, and the full text behind it, which F1
/// opens, where the line could not say everything.
///
/// References:
/// - ADR-1206 D3, "The navigation module owns F1's help and Find": F1 asks first whether the message line holds a message with more to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub line: String,
    pub more: Option<String>,
}

/// What the TUI found the terminal to be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Live {
    /// The answers at start have not all come.
    #[default]
    Asking,
    /// A vtium console that answered the font query: font sets are tried
    /// live, and the answer is the font to give back.
    On,
    /// A vtium console with `allow-font-ops` off.
    OpsOff,
    /// Another terminal, which the TUI never asks to switch its font.
    NotVtium,
}

/// The live console's state.
///
/// References:
/// - ADR-1201 D4: the live trial, and finding out what the terminal is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Console {
    pub live: Live,
    /// What the terminal said it is, in its answer to XTVERSION.
    pub version: Option<String>,
    /// The terminal answered the kitty graphics query.
    pub graphics: bool,
    /// The font to give back when the TUI leaves: the answer at start, or
    /// what a save made the console's.
    pub start_font: Option<String>,
    /// The font the console last said it draws.
    pub shown_font: Option<String>,
    /// The font the console is to draw, which a switch is sent for once
    /// no other is in flight: the live trial's, or the one it puts back.
    pub want: Option<String>,
    /// The switch sent whose answer has not come.
    pub in_flight: Option<String>,
    /// The font sets the console refused, each marked in its list.
    pub refused: BTreeSet<String>,
}

/// A query the TUI sends the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// XTVERSION, which says what the terminal is.
    Version,
    /// OSC 50's query, which names the font in use.
    Font,
    /// kitty graphics' query, which says whether the terminal shows
    /// pictures.
    Graphics,
    /// XTWINOPS 14, the text area's size in pixels.
    TextArea,
    /// DA1, which ends every batch of queries.
    Attributes,
}

/// The part of the TUI that asked a query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asker {
    /// The requests at start, which find out what the terminal is.
    Start,
    /// A font switch while the TUI runs: the live trial's, or the one that
    /// puts the start font back.
    Switch,
    /// The switch that gives the start font back as the TUI leaves.
    Leave,
    /// The question of a vtium console's text area after a resize.
    TextArea,
    /// A tab, by its node's id.
    Tab(&'static str),
}

/// The queue of who asked the terminal what, oldest first. A terminal's
/// reply does not name the query it answers, and every terminal of the
/// VT100 line answers in the order of the queries, so each reply goes to
/// the asker at the front. The shell's live console module sends the
/// queries and settles them.
///
/// References:
/// - ADR-1206 D2, "Replies go back to whoever asked" and "Every batch of queries ends with DA1".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Askers {
    waiting: VecDeque<(Asker, Query)>,
}

impl Askers {
    /// Puts `asker`'s `query` at the back of the queue.
    pub fn push(&mut self, asker: Asker, query: Query) {
        self.waiting.push_back((asker, query));
    }

    /// The oldest query not yet answered, and who asked it.
    pub fn front(&self) -> Option<(Asker, Query)> {
        self.waiting.front().copied()
    }

    /// Takes the oldest query off the queue.
    pub fn pop(&mut self) -> Option<(Asker, Query)> {
        self.waiting.pop_front()
    }

    /// Whether a query is outstanding, while which the decoder keeps an
    /// ESC that ends a read, since it may begin a reply.
    pub fn awaiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Forgets every query, for a terminal that did not answer DA1 by the
    /// deadline at start.
    pub fn clear(&mut self) {
        self.waiting.clear();
    }

    /// How many queries are outstanding.
    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }
}

/// The pictures of the console: one is drawn at a time, and the one asked
/// for last is not asked for again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pictures {
    /// The picture asked for last.
    pub asked: Option<crate::specimen::Shot>,
    /// A picture is being drawn, so the next is asked for once it comes.
    pub drawing: bool,
    /// The picture on the screen, while one shows.
    pub shown: Option<crate::specimen::Picture>,
    /// Why no picture shows for the one asked for last, if none does.
    pub unpictured: Option<String>,
}

/// The installs under way or waiting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installs {
    /// The privileged step is running.
    pub running: bool,
    /// A save that waits for the terminal's answers still due, which the
    /// loop runs once no query is outstanding.
    pub waiting: Option<crate::draft::Plan>,
}
