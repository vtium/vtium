//! The queue of who asked the terminal what, held outside every view.
//!
//! A terminal's reply does not name the query it answers, so the TUI keeps
//! the queries it has sent and not yet seen settled, oldest first, each
//! with the part of the TUI that asked it. Every terminal of the VT100
//! line answers in the order of the queries, so each reply belongs to the
//! oldest query of its kind in the queue.
//!
//! Every batch of queries ends with DA1 (Primary Device Attributes), and
//! [`Queue::send`] is the only way to write one, so no batch can leave it
//! out. Every terminal of the VT100 line answers DA1. So once DA1's reply
//! comes, a query sent before it that has no reply yet will never get one.
//! The queue then tells that query's asker it went unanswered. No timer
//! waits for a reply.
//!
//! The interface the later slices build on:
//!
//! - [`Batch`] gathers what one asker writes: bytes that ask nothing, such
//!   as a font switch, and its [`Query`]s, in the order they go out.
//! - [`Queue::send`] adds the batch's queries and DA1 to the back of the
//!   queue, and returns the bytes to write, DA1 last.
//! - [`Queue::answer`] takes one reply and returns what each asker hears,
//!   in order: [`Answer::Unanswered`] for each query that the reply shows
//!   will not be answered, then [`Answer::Reply`] for the query it answers.
//!   DA1's own reply goes to the batch's asker, which learns from it that
//!   its batch is settled. A reply that answers nothing in the oldest
//!   batch is dropped.
//! - [`Queue::awaiting`] says whether any query is outstanding. While one
//!   is, the reader keeps an ESC that ends a read for the next read, since
//!   the ESC may begin a reply (see [`super::input::Decoder::next`]).
//! - [`Queue::clear`] forgets every query, for a terminal that did not
//!   answer DA1 by the deadline at start.
//!
//! The askers are the parts of the TUI that query the terminal today: the
//! requests at start, the font switch in flight, the switch that gives
//! the start font back as the TUI leaves, and the question, after a
//! resize, of how large a vtium console's text area is in pixels. When the tabs become
//! modules of their own, a tab that queries the terminal becomes an asker
//! too.
//!
//! References:
//! - ADR-1206 D2, "Replies go back to whoever asked" and "Every batch of
//!   queries ends with DA1": the queue, DA1 at the end of every batch, and
//!   no timer.

use std::collections::VecDeque;

use super::input::Reply;
use super::model::QUERY_ID;

/// DA1, Primary Device Attributes, `CSI c`, which ends every batch.
pub const DA1: &[u8] = b"\x1b[c";

/// A query the TUI sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// XTVERSION, `CSI > q`, which `DCS > | TEXT ST` answers.
    Version,
    /// OSC 50's query, `OSC 50 ; ? ST`, which `OSC 50 ; NAME ST` answers
    /// with the font in use.
    Font,
    /// kitty graphics' query, a one-pixel image sent with `a=q`, which
    /// `APC G ... ST` answers.
    Graphics,
    /// XTWINOPS 14, `CSI 14 t`, which `CSI 4 ; HEIGHT ; WIDTH t` answers
    /// with the text area's size in pixels.
    TextArea,
    /// DA1, which `CSI ? ... c` answers. It ends every batch.
    Attributes,
}

impl Query {
    /// The bytes that ask it.
    pub fn bytes(self) -> Vec<u8> {
        match self {
            Query::Version => b"\x1b[>q".to_vec(),
            Query::Font => b"\x1b]50;?\x1b\\".to_vec(),
            Query::Graphics => {
                format!("\x1b_Gi={QUERY_ID},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\").into_bytes()
            }
            Query::TextArea => b"\x1b[14t".to_vec(),
            Query::Attributes => DA1.to_vec(),
        }
    }

    /// Whether `reply` is the kind of reply that answers this query.
    fn answered_by(self, reply: &Reply) -> bool {
        matches!(
            (self, reply),
            (Query::Version, Reply::Version(_))
                | (Query::Font, Reply::Font(_))
                | (Query::Graphics, Reply::Graphics(_))
                | (Query::TextArea, Reply::TextArea(_))
                | (Query::Attributes, Reply::Attributes)
        )
    }
}

/// The part of the TUI that asked a query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asker {
    /// The requests at start, which find out what the terminal is.
    Start,
    /// A font switch sent while the TUI runs: the live trial's, or the one
    /// that puts the start font back after it.
    Switch,
    /// The switch that gives the start font back as the TUI leaves.
    Leave,
    /// The question of a vtium console's text area in pixels after a
    /// resize, which the live trial of a size divides by the look's cell.
    TextArea,
}

/// What an asker hears about one of its queries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The reply to the query. DA1's reply, [`Reply::Attributes`], says
    /// that the asker's batch is settled.
    Reply(Reply),
    /// The query went unanswered: a reply to a later query of its batch
    /// came first, and replies come in order.
    Unanswered(Query),
}

/// What one asker writes in one go: bytes that ask nothing and queries, in
/// the order they go out. [`Queue::send`] ends it with DA1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    asker: Asker,
    bytes: Vec<u8>,
    queries: Vec<Query>,
}

impl Batch {
    /// An empty batch for `asker`.
    pub fn new(asker: Asker) -> Batch {
        Batch {
            asker,
            bytes: Vec::new(),
            queries: Vec::new(),
        }
    }

    /// Adds bytes that ask nothing, such as OSC 50 naming a font.
    #[must_use]
    pub fn write(mut self, bytes: &[u8]) -> Batch {
        self.bytes.extend_from_slice(bytes);
        self
    }

    /// Adds `query`.
    #[must_use]
    pub fn ask(mut self, query: Query) -> Batch {
        self.bytes.extend(query.bytes());
        self.queries.push(query);
        self
    }
}

/// The queries sent and not yet settled, oldest first, each with its
/// asker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Queue {
    asked: VecDeque<(Query, Asker)>,
}

impl Queue {
    /// Adds `batch`'s queries and DA1 to the back of the queue, and
    /// returns the bytes to write: the batch's own, then DA1.
    pub fn send(&mut self, batch: Batch) -> Vec<u8> {
        let Batch {
            asker,
            mut bytes,
            queries,
        } = batch;
        self.asked.extend(queries.into_iter().map(|q| (q, asker)));
        self.asked.push_back((Query::Attributes, asker));
        bytes.extend_from_slice(DA1);
        bytes
    }

    /// Takes `reply` and returns what each asker hears, in order.
    ///
    /// The reply answers the oldest query of its kind in the oldest batch,
    /// the queries up to and including the first DA1. Each query before
    /// that one went unanswered, since replies come in order. A reply that
    /// answers no query of the oldest batch is dropped, and the queue stays
    /// as it was: DA1 is always answered, so no reply may settle a query
    /// behind it.
    pub fn answer(&mut self, reply: Reply) -> Vec<(Asker, Answer)> {
        let batch = self
            .asked
            .iter()
            .position(|(q, _)| *q == Query::Attributes)
            .map_or(self.asked.len(), |end| end.saturating_add(1));
        let Some(at) = self
            .asked
            .iter()
            .take(batch)
            .position(|(q, _)| q.answered_by(&reply))
        else {
            return Vec::new();
        };
        let mut heard: Vec<(Asker, Answer)> = self
            .asked
            .drain(..at)
            .map(|(q, asker)| (asker, Answer::Unanswered(q)))
            .collect();
        if let Some((_, asker)) = self.asked.pop_front() {
            heard.push((asker, Answer::Reply(reply)));
        }
        heard
    }

    /// Whether any query is outstanding.
    pub fn awaiting(&self) -> bool {
        !self.asked.is_empty()
    }

    /// Forgets every query, for a terminal that did not answer DA1 in time.
    pub fn clear(&mut self) {
        self.asked.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(name: &str) -> Reply {
        Reply::Font(name.into())
    }

    #[test]
    fn every_batch_ends_with_da1() {
        let mut q = Queue::default();
        assert_eq!(q.send(Batch::new(Asker::Switch)), b"\x1b[c");
        assert_eq!(
            q.send(
                Batch::new(Asker::Switch)
                    .write(b"\x1b]50;tiny5\x1b\\")
                    .ask(Query::Font)
            ),
            b"\x1b]50;tiny5\x1b\\\x1b]50;?\x1b\\\x1b[c"
        );
        assert_eq!(
            q.send(
                Batch::new(Asker::Start)
                    .ask(Query::Version)
                    .ask(Query::Font)
                    .ask(Query::Graphics)
            ),
            b"\x1b[>q\x1b]50;?\x1b\\\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c"
        );
    }

    #[test]
    fn the_text_area_s_reply_goes_to_its_asker_and_its_absence_is_heard() {
        let mut q = Queue::default();
        assert_eq!(
            q.send(Batch::new(Asker::TextArea).ask(Query::TextArea)),
            b"\x1b[14t\x1b[c"
        );
        assert_eq!(
            q.answer(Reply::TextArea((7680, 2160))),
            [(
                Asker::TextArea,
                Answer::Reply(Reply::TextArea((7680, 2160)))
            )]
        );
        q.answer(Reply::Attributes);
        q.send(Batch::new(Asker::TextArea).ask(Query::TextArea));
        assert_eq!(
            q.answer(Reply::Attributes),
            [
                (Asker::TextArea, Answer::Unanswered(Query::TextArea)),
                (Asker::TextArea, Answer::Reply(Reply::Attributes))
            ]
        );
    }

    #[test]
    fn each_reply_goes_to_the_asker_of_its_query() {
        let mut q = Queue::default();
        q.send(Batch::new(Asker::Switch).ask(Query::Font));
        q.send(Batch::new(Asker::Leave).ask(Query::Font));
        assert!(q.awaiting());
        assert_eq!(
            q.answer(font("tiny5")),
            [(Asker::Switch, Answer::Reply(font("tiny5")))]
        );
        assert_eq!(
            q.answer(Reply::Attributes),
            [(Asker::Switch, Answer::Reply(Reply::Attributes))]
        );
        assert_eq!(
            q.answer(font("unifont")),
            [(Asker::Leave, Answer::Reply(font("unifont")))]
        );
        assert_eq!(
            q.answer(Reply::Attributes),
            [(Asker::Leave, Answer::Reply(Reply::Attributes))]
        );
        assert!(!q.awaiting());
    }

    #[test]
    fn da1_s_reply_settles_each_query_before_it_as_unanswered() {
        let mut q = Queue::default();
        q.send(
            Batch::new(Asker::Start)
                .ask(Query::Version)
                .ask(Query::Font)
                .ask(Query::Graphics),
        );
        q.send(Batch::new(Asker::Switch).ask(Query::Font));
        // a vtium console with font operations off answers XTVERSION and
        // DA1 alone
        assert_eq!(
            q.answer(Reply::Version("vtium(0.1.0)".into())),
            [(
                Asker::Start,
                Answer::Reply(Reply::Version("vtium(0.1.0)".into()))
            )]
        );
        assert_eq!(
            q.answer(Reply::Attributes),
            [
                (Asker::Start, Answer::Unanswered(Query::Font)),
                (Asker::Start, Answer::Unanswered(Query::Graphics)),
                (Asker::Start, Answer::Reply(Reply::Attributes)),
            ]
        );
        // the second batch stays until its own DA1's reply
        assert!(q.awaiting());
        assert_eq!(
            q.answer(Reply::Attributes),
            [
                (Asker::Switch, Answer::Unanswered(Query::Font)),
                (Asker::Switch, Answer::Reply(Reply::Attributes)),
            ]
        );
        assert!(!q.awaiting());
    }

    #[test]
    fn a_later_reply_settles_the_queries_it_passes() {
        let mut q = Queue::default();
        q.send(
            Batch::new(Asker::Start)
                .ask(Query::Version)
                .ask(Query::Font)
                .ask(Query::Graphics),
        );
        assert_eq!(
            q.answer(Reply::Graphics(true)),
            [
                (Asker::Start, Answer::Unanswered(Query::Version)),
                (Asker::Start, Answer::Unanswered(Query::Font)),
                (Asker::Start, Answer::Reply(Reply::Graphics(true))),
            ]
        );
    }

    #[test]
    fn a_reply_that_answers_nothing_in_the_oldest_batch_is_dropped() {
        let mut q = Queue::default();
        // nothing asked
        assert_eq!(q.answer(font("tiny5")), []);
        q.send(Batch::new(Asker::Start).ask(Query::Version));
        q.send(Batch::new(Asker::Switch).ask(Query::Font));
        // a font reply cannot pass the first batch's DA1, which is always
        // answered
        assert_eq!(q.answer(font("tiny5")), []);
        assert_eq!(
            q.answer(Reply::Attributes),
            [
                (Asker::Start, Answer::Unanswered(Query::Version)),
                (Asker::Start, Answer::Reply(Reply::Attributes)),
            ]
        );
        assert_eq!(
            q.answer(font("tiny5")),
            [(Asker::Switch, Answer::Reply(font("tiny5")))]
        );
        q.clear();
        assert!(!q.awaiting());
    }
}
