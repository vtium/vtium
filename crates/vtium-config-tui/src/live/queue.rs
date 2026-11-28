//! The queries the TUI writes to the terminal, and how each reply finds the
//! part of the TUI that asked for it.
//!
//! A terminal's reply does not name the query it answers. So the shared
//! state keeps a queue of askers, oldest first, and every terminal of the
//! VT100 line answers in the order of the queries. [`send`] is the only way
//! to write queries, and it ends every batch with DA1 (Primary Device
//! Attributes), which every such terminal answers. So once DA1's reply
//! comes, a query sent before it that has no reply yet will never get one,
//! and [`answer`] tells its asker so. No timer waits for a reply.
//!
//! References:
//! - ADR-1206 D2, "Replies go back to whoever asked": the queue of askers
//!   in the shared state, and each reply to the asker at its front.
//! - ADR-1206 D2, "Every batch of queries ends with DA1": DA1 last, and its
//!   reply settles each query before it as unanswered.

use vtium_config_core::ui::{Asker, Askers, Query};
use vtium_config_widgets::keys::Reply;

/// DA1, Primary Device Attributes, `CSI c`, which ends every batch.
pub const DA1: &[u8] = b"\x1b[c";

/// The image id of the kitty graphics query: a one-pixel image the
/// terminal checks and does not keep.
pub const QUERY_ID: u32 = 31;

/// The bytes that ask `query`.
pub fn bytes(query: Query) -> Vec<u8> {
    match query {
        // XTVERSION, which `DCS > | TEXT ST` answers
        Query::Version => b"\x1b[>q".to_vec(),
        // OSC 50's query, which `OSC 50 ; NAME ST` answers
        Query::Font => b"\x1b]50;?\x1b\\".to_vec(),
        // kitty graphics' query, which `APC G ... ST` answers
        Query::Graphics => {
            format!("\x1b_Gi={QUERY_ID},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\").into_bytes()
        }
        // XTWINOPS 14, which `CSI 4 ; HEIGHT ; WIDTH t` answers
        Query::TextArea => b"\x1b[14t".to_vec(),
        Query::Attributes => DA1.to_vec(),
    }
}

/// Whether `reply` is the kind of reply that answers `query`.
fn answers(reply: &Reply, query: Query) -> bool {
    matches!(
        (query, reply),
        (Query::Version, Reply::Version(_))
            | (Query::Font, Reply::Font(_))
            | (Query::Graphics, Reply::Graphics(_))
            | (Query::TextArea, Reply::TextArea(_))
            | (Query::Attributes, Reply::Attributes)
    )
}

/// What one asker writes in one go: bytes that ask nothing, such as a font
/// switch, and queries, in the order they go out. [`send`] ends it with
/// DA1.
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
        self.bytes.extend(bytes(query));
        self.queries.push(query);
        self
    }
}

/// Puts `batch`'s queries and DA1 at the back of `askers`, and returns the
/// bytes to write: the batch's own, then DA1.
pub fn send(askers: &mut Askers, batch: Batch) -> Vec<u8> {
    let Batch {
        asker,
        mut bytes,
        queries,
    } = batch;
    for query in queries {
        askers.push(asker, query);
    }
    askers.push(asker, Query::Attributes);
    bytes.extend_from_slice(DA1);
    bytes
}

/// What an asker hears about one of its queries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The reply to the query. DA1's reply, [`Reply::Attributes`], tells
    /// the asker that its batch is settled.
    Reply(Reply),
    /// The query went unanswered: a reply to a later query of its batch
    /// came first, and replies come in order.
    Unanswered(Query),
}

/// Takes `reply` off the front of `askers`, and returns what each asker
/// hears, in order.
///
/// The reply answers the oldest query of its kind in the oldest batch,
/// the queries up to and including the first DA1. Each query before that
/// one went unanswered, since replies come in order. A reply that answers
/// no query of the oldest batch is dropped, and the queue stays as it was:
/// DA1 is always answered, so no reply may settle a query behind it.
pub fn answer(askers: &mut Askers, reply: Reply) -> Vec<(Asker, Answer)> {
    // the queue is read from its front only, so the walk runs on a copy
    // that replaces the queue once the reply has found its query
    let mut rest = askers.clone();
    let mut passed = Vec::new();
    while let Some((asker, query)) = rest.pop() {
        if answers(&reply, query) {
            *askers = rest;
            let mut heard: Vec<(Asker, Answer)> = passed
                .into_iter()
                .map(|(a, q)| (a, Answer::Unanswered(q)))
                .collect();
            heard.push((asker, Answer::Reply(reply)));
            return heard;
        }
        if query == Query::Attributes {
            break;
        }
        passed.push((asker, query));
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(name: &str) -> Reply {
        Reply::Font(name.into())
    }

    #[test]
    fn every_batch_ends_with_da1() {
        let mut q = Askers::default();
        assert_eq!(send(&mut q, Batch::new(Asker::Switch)), b"\x1b[c");
        assert_eq!(
            send(
                &mut q,
                Batch::new(Asker::Start)
                    .ask(Query::Version)
                    .ask(Query::Font)
                    .ask(Query::Graphics)
            ),
            b"\x1b[>q\x1b]50;?\x1b\\\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c"
        );
        assert_eq!(q.len(), 5);
    }

    #[test]
    fn a_later_reply_settles_the_queries_it_passes() {
        let mut q = Askers::default();
        send(
            &mut q,
            Batch::new(Asker::Start)
                .ask(Query::Version)
                .ask(Query::Font)
                .ask(Query::Graphics),
        );
        assert_eq!(
            answer(&mut q, Reply::Graphics(true)),
            [
                (Asker::Start, Answer::Unanswered(Query::Version)),
                (Asker::Start, Answer::Unanswered(Query::Font)),
                (Asker::Start, Answer::Reply(Reply::Graphics(true))),
            ]
        );
    }

    #[test]
    fn a_reply_that_answers_nothing_in_the_oldest_batch_is_dropped() {
        let mut q = Askers::default();
        assert_eq!(answer(&mut q, font("tiny5")), []);
        send(&mut q, Batch::new(Asker::Start).ask(Query::Version));
        send(&mut q, Batch::new(Asker::Switch).ask(Query::Font));
        // a font reply cannot pass the first batch's DA1
        assert_eq!(answer(&mut q, font("tiny5")), []);
        assert_eq!(q.len(), 4, "the queue stays as it was");
        assert_eq!(
            answer(&mut q, Reply::Attributes),
            [
                (Asker::Start, Answer::Unanswered(Query::Version)),
                (Asker::Start, Answer::Reply(Reply::Attributes)),
            ]
        );
        assert_eq!(
            answer(&mut q, font("tiny5")),
            [(Asker::Switch, Answer::Reply(font("tiny5")))]
        );
    }
}
