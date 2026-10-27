//! The bytes queued for the child: what it has not read yet, in order, with
//! the kind of each run so that replies are counted and pointer motion is
//! coalesced.

use std::collections::VecDeque;

use crate::bounds::MOTION_COALESCE_AT;

/// What a run of queued bytes is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Keys, pastes, button reports: what the user sent. Queued whole.
    User,
    /// Pointer motion reports, coalesced from [`MOTION_COALESCE_AT`].
    Motion,
    /// Answers to the child's queries, counted against the reply bounds.
    Reply,
}

/// A run of queued bytes of one kind.
#[derive(Debug)]
struct Run {
    kind: Kind,
    len: usize,
    /// Its first bytes have reached the child, so it cannot be replaced.
    started: bool,
}

/// The bytes the child has not read yet.
#[derive(Debug, Default)]
pub(crate) struct WriteQueue {
    bytes: VecDeque<u8>,
    runs: VecDeque<Run>,
    replies: usize,
}

impl WriteQueue {
    /// Bytes queued.
    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Reply bytes queued.
    pub(crate) fn replies(&self) -> usize {
        self.replies
    }

    /// Queues `data` as one run of `kind`, behind everything queued.
    ///
    /// From [`MOTION_COALESCE_AT`] bytes, motion replaces a motion run at
    /// the end of the queue that has not started to reach the child. A
    /// button report, a key or a reply after it ends that run, so their
    /// order is kept and each is queued whole.
    pub(crate) fn push(&mut self, kind: Kind, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if kind == Kind::Motion && self.bytes.len() >= MOTION_COALESCE_AT {
            if let Some(last) = self.runs.back_mut() {
                if last.kind == Kind::Motion && !last.started {
                    let keep = self.bytes.len().saturating_sub(last.len);
                    self.bytes.truncate(keep);
                    self.bytes.extend(data);
                    last.len = data.len();
                    return;
                }
            }
        }
        self.bytes.extend(data);
        if kind == Kind::Reply {
            self.replies = self.replies.saturating_add(data.len());
        }
        match self.runs.back_mut() {
            Some(last) if last.kind == kind && !last.started => {
                last.len = last.len.saturating_add(data.len());
            }
            _ => self.runs.push_back(Run {
                kind,
                len: data.len(),
                started: false,
            }),
        }
    }

    /// The first queued bytes, as one slice: empty only when the queue is.
    pub(crate) fn front(&self) -> &[u8] {
        let (a, b) = self.bytes.as_slices();
        if a.is_empty() { b } else { a }
    }

    /// The child read `n` bytes from the front.
    pub(crate) fn consume(&mut self, n: usize) {
        let mut n = n.min(self.bytes.len());
        self.bytes.drain(..n);
        while n > 0 {
            let Some(run) = self.runs.front_mut() else {
                break;
            };
            let k = n.min(run.len);
            run.len = run.len.saturating_sub(k);
            n = n.saturating_sub(k);
            if run.kind == Kind::Reply {
                self.replies = self.replies.saturating_sub(k);
            }
            if run.len == 0 {
                self.runs.pop_front();
            } else {
                run.started = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything queued, front to back.
    fn contents(q: &WriteQueue) -> Vec<u8> {
        q.bytes.iter().copied().collect()
    }

    #[test]
    fn runs_keep_their_order_and_replies_are_counted() {
        let mut q = WriteQueue::default();
        q.push(Kind::User, b"ab");
        q.push(Kind::Reply, b"RR");
        q.push(Kind::User, b"c");
        q.push(Kind::Reply, b"S");
        assert_eq!(contents(&q), b"abRRcS");
        assert_eq!(q.replies(), 3);
        q.consume(3);
        assert_eq!(q.replies(), 2);
        q.consume(2);
        assert_eq!(q.replies(), 1);
        assert_eq!(q.front(), b"S");
        q.consume(9);
        assert!(q.is_empty());
        assert_eq!(q.replies(), 0);
    }

    #[test]
    fn motion_below_the_bound_is_queued_whole() {
        let mut q = WriteQueue::default();
        q.push(Kind::Motion, b"m1");
        q.push(Kind::Motion, b"m2");
        assert_eq!(contents(&q), b"m1m2");
    }

    /// The fill that takes the queue to the coalescing bound.
    fn filled() -> WriteQueue {
        let mut q = WriteQueue::default();
        q.push(Kind::User, &[b'k'; MOTION_COALESCE_AT]);
        q
    }

    #[test]
    fn motion_from_the_bound_keeps_the_latest_position() {
        let mut q = filled();
        q.push(Kind::Motion, b"<1;1M");
        q.push(Kind::Motion, b"<2;2M");
        q.push(Kind::Motion, b"<33;33M");
        assert_eq!(q.len(), MOTION_COALESCE_AT + 7);
        assert!(contents(&q).ends_with(b"k<33;33M"));
    }

    #[test]
    fn buttons_between_motion_are_kept_in_order() {
        let mut q = filled();
        q.push(Kind::Motion, b"m1");
        q.push(Kind::Motion, b"m2");
        q.push(Kind::User, b"B1");
        q.push(Kind::Motion, b"m3");
        q.push(Kind::Motion, b"m4");
        q.push(Kind::User, b"B2");
        assert!(contents(&q).ends_with(b"km2B1m4B2"));
    }

    #[test]
    fn replies_end_a_motion_run() {
        let mut q = filled();
        q.push(Kind::Motion, b"m1");
        q.push(Kind::Reply, b"R");
        q.push(Kind::Motion, b"m2");
        q.push(Kind::Motion, b"m3");
        assert!(contents(&q).ends_with(b"km1Rm3"));
        assert_eq!(q.replies(), 1);
    }

    #[test]
    fn a_motion_run_that_started_is_not_replaced() {
        let mut q = WriteQueue::default();
        q.push(Kind::Motion, &[b'm'; MOTION_COALESCE_AT + 2]);
        q.consume(1);
        q.push(Kind::Motion, b"xy");
        q.push(Kind::Motion, b"zw");
        // The started run keeps every byte; the new run holds the latest.
        assert_eq!(q.len(), MOTION_COALESCE_AT + 1 + 2);
        assert!(contents(&q).ends_with(b"mzw"));
    }

    #[test]
    fn a_flood_of_motion_stays_bounded() {
        let mut q = filled();
        for i in 0..100_000u32 {
            q.push(
                Kind::Motion,
                format!("<35;{};{}M", i % 300, i % 100).as_bytes(),
            );
            if i % 1000 == 0 {
                q.push(Kind::User, b"<0;1;1M");
            }
        }
        // the fill, 100 button reports and one motion report after each
        assert!(q.len() <= MOTION_COALESCE_AT + 100 * (7 + 14) + 14);
    }
}
